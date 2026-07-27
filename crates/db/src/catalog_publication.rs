use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbErr, QueryResult, TransactionTrait,
    sea_query::{Alias, Expr, OnConflict, Query},
};
use serde_json::json;
use sha2::{Digest, Sha256};
use thiserror::Error;
use tjxy_common::{CatalogItemId, PublicationId, SortKey, StorageObjectRecordId, StorageRootId};
use uuid::Uuid;

use crate::source_publication::{
    SeriesSourcePublication, SourcePublicationManifest, ensure_structure_storage_authorized,
    materialize_structure_sources, seal_structure_sources, series_source_manifest,
};
use crate::work_job::{
    ClaimedWorkJob, WorkJobRepository, WorkJobRepositoryError, WorkJobResult, WorkScope,
    WorkTaskKind, ensure_live_claim, fence_live_claim,
};

pub(crate) const STATE_BUILDING: &str = "Building";
pub(crate) const STATE_READY: &str = "Ready";
pub(crate) const STATE_ACTIVE: &str = "Active";
pub(crate) const STATE_RETIRED: &str = "Retired";
const MAX_NAME_CHARS: usize = 512;
const MAX_OVERVIEW_CHARS: usize = 32_768;
const MAX_ROWS: usize = 100_000;
const MAX_BATCH_ROWS: usize = 5_000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StructurePublicationRow {
    catalog_item_id: CatalogItemId,
    parent_catalog_item_id: CatalogItemId,
    storage_root_id: StorageRootId,
    scope_storage_object_id: StorageObjectRecordId,
    item_type: String,
    name: String,
    sort_name: String,
    production_year: Option<i32>,
    overview: Option<String>,
    row_sha256: String,
}

impl StructurePublicationRow {
    /// Defines one immutable item projection inside a structure publication.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogPublicationError::InvalidStructureRow`] for invalid or
    /// unbounded display fields.
    #[allow(clippy::too_many_arguments)] // Projection identity and required storage scope form one manifest row.
    pub fn new(
        catalog_item_id: CatalogItemId,
        parent_catalog_item_id: CatalogItemId,
        storage_root_id: StorageRootId,
        scope_storage_object_id: StorageObjectRecordId,
        item_type: impl Into<String>,
        name: impl Into<String>,
        sort_name: impl Into<String>,
        production_year: Option<i32>,
        overview: Option<String>,
    ) -> Result<Self, CatalogPublicationError> {
        let item_type = item_type.into();
        let name = name.into();
        let sort_name = sort_name.into();
        if !matches!(item_type.as_str(), "Season" | "Episode" | "Folder")
            || !valid_text(&name, MAX_NAME_CHARS)
            || !valid_text(&sort_name, MAX_NAME_CHARS)
            || overview.as_deref().is_some_and(|value| {
                value.chars().count() > MAX_OVERVIEW_CHARS || value.chars().any(char::is_control)
            })
        {
            return Err(CatalogPublicationError::InvalidStructureRow);
        }
        let mut row = Self {
            catalog_item_id,
            parent_catalog_item_id,
            storage_root_id,
            scope_storage_object_id,
            item_type,
            name,
            sort_name,
            production_year,
            overview,
            row_sha256: String::new(),
        };
        row.row_sha256 = row_hash(&row);
        Ok(row)
    }

    #[must_use]
    pub const fn id(&self) -> CatalogItemId {
        self.catalog_item_id
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StructurePublicationManifest {
    expected_row_count: i64,
    sha256: String,
    source: Option<SourcePublicationManifest>,
}

impl StructurePublicationManifest {
    /// Builds the order-independent manifest expected at the atomic publish boundary.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogPublicationError::InvalidManifest`] for duplicate IDs or
    /// an unbounded publication.
    pub fn from_rows(rows: &[StructurePublicationRow]) -> Result<Self, CatalogPublicationError> {
        if rows.len() > MAX_ROWS {
            return Err(CatalogPublicationError::InvalidManifest);
        }
        let entries = rows
            .iter()
            .map(|row| (row.catalog_item_id, row.row_sha256.as_str()))
            .collect::<Vec<_>>();
        let sha256 = manifest_hash(entries)?;
        let expected_row_count =
            i64::try_from(rows.len()).map_err(|_| CatalogPublicationError::InvalidManifest)?;
        Ok(Self {
            expected_row_count,
            sha256,
            source: None,
        })
    }

    /// Builds a Series expansion manifest covering items and every Episode source graph.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogPublicationError`] for duplicate or unbounded rows.
    pub fn from_series(
        rows: &[StructurePublicationRow],
        sources: &[SeriesSourcePublication],
    ) -> Result<Self, CatalogPublicationError> {
        let mut manifest = Self::from_rows(rows)?;
        manifest.source = Some(series_source_manifest(sources)?);
        Ok(manifest)
    }
}

pub struct CatalogPublicationRepository<'connection> {
    pub(crate) database: &'connection DatabaseConnection,
}

impl<'connection> CatalogPublicationRepository<'connection> {
    #[must_use]
    pub const fn new(database: &'connection DatabaseConnection) -> Self {
        Self { database }
    }

    /// Creates or resumes the structure publication owned by a live `ExpandItem` job.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogPublicationError`] for incompatible work, stale state, or SQL errors.
    pub async fn begin_structure(
        &self,
        claimed: &ClaimedWorkJob,
        manifest: &StructurePublicationManifest,
    ) -> Result<PublicationId, CatalogPublicationError> {
        let owner = structure_owner(claimed)?;
        let transaction = self.database.begin().await?;
        let result = begin_structure(&transaction, claimed, owner, manifest, Utc::now()).await;
        finish(transaction, result).await
    }

    /// Idempotently stages a bounded batch without changing the active pointer.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogPublicationError`] if the lease or publication ownership is invalid.
    pub async fn stage_structure_batch(
        &self,
        claimed: &ClaimedWorkJob,
        publication_id: PublicationId,
        rows: &[StructurePublicationRow],
    ) -> Result<(), CatalogPublicationError> {
        if rows.len() > MAX_BATCH_ROWS {
            return Err(CatalogPublicationError::InvalidStructureRow);
        }
        let transaction = self.database.begin().await?;
        let result =
            stage_structure_batch(&transaction, claimed, publication_id, rows, Utc::now()).await;
        let result = match result {
            Ok(()) => fence_live_claim(&transaction, claimed, Utc::now())
                .await
                .map_err(Into::into),
            Err(error) => Err(error),
        };
        finish(transaction, result).await
    }

    /// Validates and freezes the complete shadow projection before the short publish transaction.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogPublicationError`] when the manifest or rooted topology is incomplete.
    pub async fn seal_structure(
        &self,
        claimed: &ClaimedWorkJob,
        publication_id: PublicationId,
    ) -> Result<(), CatalogPublicationError> {
        let transaction = self.database.begin().await?;
        let result = seal_structure(&transaction, claimed, publication_id, Utc::now()).await;
        let result = match result {
            Ok(()) => fence_live_claim(&transaction, claimed, Utc::now())
                .await
                .map_err(Into::into),
            Err(error) => Err(error),
        };
        finish(transaction, result).await
    }

    /// Atomically activates a complete structure projection and completes its job.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogPublicationError`] without committing any partial state when
    /// the manifest, revision, ownership, or lease fence no longer matches.
    pub async fn publish_structure(
        &self,
        jobs: &WorkJobRepository<'_>,
        claimed: &ClaimedWorkJob,
        publication_id: PublicationId,
    ) -> Result<i64, CatalogPublicationError> {
        let transaction = self.database.begin().await?;
        let result =
            publish_structure(&transaction, jobs, claimed, publication_id, Utc::now()).await;
        finish(transaction, result).await
    }
}

#[derive(Debug, Error)]
pub enum CatalogPublicationError {
    #[error("structure publication rows must have valid bounded fields")]
    InvalidStructureRow,
    #[error("structure publication manifest is invalid or contains duplicate item IDs")]
    InvalidManifest,
    #[error("only a CatalogItem-scoped ExpandItem job can publish structure")]
    InvalidWorkKind,
    #[error("publication does not belong to the claimed job or is not buildable")]
    InvalidPublication,
    #[error("publication manifest does not match the complete staged projection")]
    ManifestMismatch,
    #[error("publication structure is not a rooted acyclic projection")]
    InvalidStructure,
    #[error("source publication rows must have valid bounded fields")]
    InvalidSourceRow,
    #[error("source publication manifest is invalid or contains duplicate identities")]
    InvalidSourceManifest,
    #[error("source publication contains orphaned or incomplete relationships")]
    InvalidSourceGraph,
    #[error("stable media identity conflicts with an existing canonical row")]
    StableIdentityConflict,
    #[error("source publication references storage outside the owner's enabled library roots")]
    UnauthorizedStorageObject,
    #[error("catalog item revision changed before publication")]
    StaleExpectedRevision,
    #[error("publication storage input contains facts not yet reconciled")]
    StorageInputPending,
    #[error("library metadata policy is invalid")]
    InvalidMetadataPolicy,
    #[error("catalog generation row is missing")]
    MissingCatalogState,
    #[error("work job operation failed: {0}")]
    WorkJob(#[from] WorkJobRepositoryError),
    #[error("database operation failed: {0}")]
    Database(#[from] DbErr),
    #[error("rollback failed after {original}: {rollback}")]
    RollbackFailed { original: String, rollback: DbErr },
}

async fn begin_structure(
    transaction: &DatabaseTransaction,
    claimed: &ClaimedWorkJob,
    owner: CatalogItemId,
    manifest: &StructurePublicationManifest,
    now: DateTime<Utc>,
) -> Result<PublicationId, CatalogPublicationError> {
    ensure_live_claim(transaction, claimed, now).await?;
    let publication_id = PublicationId::new();
    let backend = transaction.get_database_backend();
    let conflict = if backend == sea_orm::DbBackend::MySql {
        OnConflict::column(Alias::new("job_id"))
            .update_column(Alias::new("job_id"))
            .to_owned()
    } else {
        OnConflict::column(Alias::new("job_id"))
            .do_nothing()
            .to_owned()
    };
    let insert = Query::insert()
        .into_table(Alias::new("catalog_publications"))
        .columns([
            Alias::new("id"),
            Alias::new("job_id"),
            Alias::new("owner_catalog_item_id"),
            Alias::new("publication_kind"),
            Alias::new("expected_revision"),
            Alias::new("input_sync_revision"),
            Alias::new("state"),
            Alias::new("manifest_sha256"),
            Alias::new("expected_row_count"),
            Alias::new("source_manifest_sha256"),
            Alias::new("expected_source_row_count"),
            Alias::new("created_at"),
        ])
        .values_panic([
            publication_id.as_uuid().into(),
            claimed.id().as_uuid().into(),
            owner.as_uuid().into(),
            "Structure".into(),
            claimed.job().expected_revision().into(),
            claimed.job().input_sync_revision().into(),
            STATE_BUILDING.into(),
            manifest.sha256.clone().into(),
            manifest.expected_row_count.into(),
            manifest
                .source
                .as_ref()
                .map(|source| source.sha256().to_owned())
                .into(),
            manifest
                .source
                .as_ref()
                .map(SourcePublicationManifest::expected_row_count)
                .into(),
            now.into(),
        ])
        .on_conflict(conflict)
        .to_owned();
    transaction.execute(backend.build(&insert)).await?;
    let row = transaction
        .query_one(backend.build(&publication_for_job(claimed.id().as_uuid())))
        .await?
        .ok_or(CatalogPublicationError::InvalidPublication)?;
    validate_publication_row(&row, claimed, owner, manifest)
}

fn validate_publication_row(
    row: &QueryResult,
    claimed: &ClaimedWorkJob,
    owner: CatalogItemId,
    manifest: &StructurePublicationManifest,
) -> Result<PublicationId, CatalogPublicationError> {
    if row.try_get::<Uuid>("", "job_id")? != claimed.id().as_uuid()
        || row.try_get::<Uuid>("", "owner_catalog_item_id")? != owner.as_uuid()
        || row.try_get::<String>("", "publication_kind")? != "Structure"
        || row.try_get::<i64>("", "expected_revision")? != claimed.job().expected_revision()
        || row.try_get::<Option<i64>>("", "input_sync_revision")?
            != claimed.job().input_sync_revision()
        || !matches!(
            row.try_get::<String>("", "state")?.as_str(),
            STATE_BUILDING | STATE_READY
        )
        || row.try_get::<String>("", "manifest_sha256")? != manifest.sha256
        || row.try_get::<i64>("", "expected_row_count")? != manifest.expected_row_count
        || row.try_get::<Option<String>>("", "source_manifest_sha256")?
            != manifest
                .source
                .as_ref()
                .map(|source| source.sha256().to_owned())
        || row.try_get::<Option<i64>>("", "expected_source_row_count")?
            != manifest
                .source
                .as_ref()
                .map(SourcePublicationManifest::expected_row_count)
    {
        return Err(CatalogPublicationError::InvalidPublication);
    }
    Ok(PublicationId::from_uuid(row.try_get("", "id")?))
}

#[allow(clippy::too_many_lines)] // Stages one fenced structure batch in a single transaction.
async fn stage_structure_batch(
    transaction: &DatabaseTransaction,
    claimed: &ClaimedWorkJob,
    publication_id: PublicationId,
    rows: &[StructurePublicationRow],
    now: DateTime<Utc>,
) -> Result<(), CatalogPublicationError> {
    let owner = structure_owner(claimed)?;
    ensure_live_claim(transaction, claimed, now).await?;
    ensure_building_publication(transaction, claimed, publication_id).await?;
    let backend = transaction.get_database_backend();
    for row in rows {
        let identity_conflict = if backend == sea_orm::DbBackend::MySql {
            OnConflict::column(Alias::new("id"))
                .update_column(Alias::new("id"))
                .to_owned()
        } else {
            OnConflict::column(Alias::new("id")).do_nothing().to_owned()
        };
        let identity = Query::insert()
            .into_table(Alias::new("catalog_items"))
            .columns([
                Alias::new("id"),
                Alias::new("item_type"),
                Alias::new("name"),
                Alias::new("sort_name"),
                Alias::new("sort_key"),
                Alias::new("production_year"),
                Alias::new("overview"),
                Alias::new("classification_state"),
                Alias::new("metadata_state"),
                Alias::new("structure_state"),
                Alias::new("source_state"),
                Alias::new("structure_expansion_revision"),
                Alias::new("source_index_revision"),
                Alias::new("structure_owner_item_id"),
                Alias::new("is_present"),
            ])
            .values_panic([
                row.catalog_item_id.as_uuid().into(),
                row.item_type.clone().into(),
                row.name.clone().into(),
                row.sort_name.clone().into(),
                SortKey::from_text(&row.sort_name).into_bytes().into(),
                row.production_year.into(),
                row.overview.clone().into(),
                "Matched".into(),
                "Ready".into(),
                "PublishedProjection".into(),
                "Unknown".into(),
                0.into(),
                0.into(),
                owner.as_uuid().into(),
                true.into(),
            ])
            .on_conflict(identity_conflict)
            .to_owned();
        transaction.execute(backend.build(&identity)).await?;
        ensure_structure_identity(transaction, owner, row).await?;
    }
    for row in rows {
        let projection = Query::insert()
            .into_table(Alias::new("publication_catalog_items"))
            .columns([
                Alias::new("id"),
                Alias::new("publication_id"),
                Alias::new("catalog_item_id"),
                Alias::new("parent_catalog_item_id"),
                Alias::new("storage_root_id"),
                Alias::new("scope_storage_object_id"),
                Alias::new("item_type"),
                Alias::new("name"),
                Alias::new("sort_name"),
                Alias::new("sort_key"),
                Alias::new("production_year"),
                Alias::new("overview"),
                Alias::new("source_state"),
                Alias::new("source_index_revision"),
                Alias::new("row_sha256"),
            ])
            .values_panic([
                Uuid::new_v4().into(),
                publication_id.as_uuid().into(),
                row.catalog_item_id.as_uuid().into(),
                row.parent_catalog_item_id.as_uuid().into(),
                row.storage_root_id.as_uuid().into(),
                row.scope_storage_object_id.as_uuid().into(),
                row.item_type.clone().into(),
                row.name.clone().into(),
                row.sort_name.clone().into(),
                SortKey::from_text(&row.sort_name).into_bytes().into(),
                row.production_year.into(),
                row.overview.clone().into(),
                "Unknown".into(),
                0_i64.into(),
                row.row_sha256.clone().into(),
            ])
            .on_conflict(
                OnConflict::columns([Alias::new("publication_id"), Alias::new("catalog_item_id")])
                    .update_columns([
                        Alias::new("parent_catalog_item_id"),
                        Alias::new("storage_root_id"),
                        Alias::new("scope_storage_object_id"),
                        Alias::new("item_type"),
                        Alias::new("name"),
                        Alias::new("sort_name"),
                        Alias::new("sort_key"),
                        Alias::new("production_year"),
                        Alias::new("overview"),
                        Alias::new("row_sha256"),
                    ])
                    .to_owned(),
            )
            .to_owned();
        transaction.execute(backend.build(&projection)).await?;
    }
    Ok(())
}

async fn ensure_structure_identity(
    transaction: &DatabaseTransaction,
    owner: CatalogItemId,
    expected: &StructurePublicationRow,
) -> Result<(), CatalogPublicationError> {
    let query = Query::select()
        .columns([
            Alias::new("item_type"),
            Alias::new("structure_owner_item_id"),
        ])
        .from(Alias::new("catalog_items"))
        .and_where(Expr::col(Alias::new("id")).eq(expected.catalog_item_id.as_uuid()))
        .to_owned();
    let backend = transaction.get_database_backend();
    let row = transaction
        .query_one(backend.build(&query))
        .await?
        .ok_or(CatalogPublicationError::StableIdentityConflict)?;
    if row.try_get::<String>("", "item_type")? != expected.item_type
        || row.try_get::<Option<Uuid>>("", "structure_owner_item_id")? != Some(owner.as_uuid())
    {
        return Err(CatalogPublicationError::StableIdentityConflict);
    }
    Ok(())
}

async fn publish_structure(
    transaction: &DatabaseTransaction,
    jobs: &WorkJobRepository<'_>,
    claimed: &ClaimedWorkJob,
    publication_id: PublicationId,
    now: DateTime<Utc>,
) -> Result<i64, CatalogPublicationError> {
    let owner = structure_owner(claimed)?;
    ensure_live_claim(transaction, claimed, now).await?;
    let publication = load_publication(transaction, claimed, publication_id, STATE_READY).await?;
    ensure_structure_storage_authorized(transaction, publication_id, owner).await?;
    crate::source_publication::ensure_structure_storage_reconciled(
        transaction,
        publication_id,
        owner,
        claimed,
    )
    .await?;
    let backend = transaction.get_database_backend();
    let owner_row = transaction
        .query_one(backend.build(&owner_publication(owner)))
        .await?
        .ok_or(CatalogPublicationError::StaleExpectedRevision)?;
    let revision: i64 = owner_row.try_get("", "structure_expansion_revision")?;
    if revision != publication.expected_revision {
        return Err(CatalogPublicationError::StaleExpectedRevision);
    }
    materialize_structure_sources(transaction, publication_id).await?;
    let previous: Option<Uuid> = owner_row.try_get("", "active_structure_publication_id")?;
    let switch = Query::update()
        .table(Alias::new("catalog_items"))
        .value(
            Alias::new("active_structure_publication_id"),
            publication_id.as_uuid(),
        )
        .value(Alias::new("structure_state"), "Expanded")
        .value(Alias::new("last_expanded_at"), now)
        .value(Alias::new("last_error"), Option::<String>::None)
        .and_where(Expr::col(Alias::new("id")).eq(owner.as_uuid()))
        .and_where(
            Expr::col(Alias::new("structure_expansion_revision")).eq(publication.expected_revision),
        )
        .to_owned();
    if transaction
        .execute(backend.build(&switch))
        .await?
        .rows_affected()
        != 1
    {
        return Err(CatalogPublicationError::StaleExpectedRevision);
    }
    let generation = advance_generation(transaction).await?;
    activate_publication(transaction, publication_id, previous, generation, now).await?;
    insert_change_event(
        transaction,
        owner,
        publication_id,
        generation,
        "StructurePublished",
        now,
    )
    .await?;
    jobs.complete_in_transaction(
        transaction,
        claimed,
        WorkJobResult::success(
            json!({"published_rows": publication.expected_row_count, "catalog_generation": generation}),
            Vec::new(),
        ),
    )
    .await?;
    Ok(generation)
}

pub(crate) struct StoredPublication {
    pub(crate) expected_revision: i64,
    pub(crate) expected_row_count: i64,
    pub(crate) manifest_sha256: String,
    source_manifest_sha256: Option<String>,
    expected_source_row_count: Option<i64>,
}

struct StoredProjectionRow {
    item_id: CatalogItemId,
    parent_id: CatalogItemId,
    item_type: String,
    row_sha256: String,
}

async fn load_building_publication(
    transaction: &DatabaseTransaction,
    claimed: &ClaimedWorkJob,
    publication_id: PublicationId,
) -> Result<StoredPublication, CatalogPublicationError> {
    load_publication(transaction, claimed, publication_id, STATE_BUILDING).await
}

async fn load_publication(
    transaction: &DatabaseTransaction,
    claimed: &ClaimedWorkJob,
    publication_id: PublicationId,
    expected_state: &str,
) -> Result<StoredPublication, CatalogPublicationError> {
    let backend = transaction.get_database_backend();
    let row = transaction
        .query_one(backend.build(&publication_by_id(publication_id)))
        .await?
        .ok_or(CatalogPublicationError::InvalidPublication)?;
    if row.try_get::<Uuid>("", "job_id")? != claimed.id().as_uuid()
        || row.try_get::<String>("", "publication_kind")? != "Structure"
        || row.try_get::<String>("", "state")? != expected_state
    {
        return Err(CatalogPublicationError::InvalidPublication);
    }
    Ok(StoredPublication {
        expected_revision: row.try_get("", "expected_revision")?,
        expected_row_count: row.try_get("", "expected_row_count")?,
        manifest_sha256: row.try_get("", "manifest_sha256")?,
        source_manifest_sha256: row.try_get("", "source_manifest_sha256")?,
        expected_source_row_count: row.try_get("", "expected_source_row_count")?,
    })
}

async fn seal_structure(
    transaction: &DatabaseTransaction,
    claimed: &ClaimedWorkJob,
    publication_id: PublicationId,
    now: DateTime<Utc>,
) -> Result<(), CatalogPublicationError> {
    let owner = structure_owner(claimed)?;
    ensure_live_claim(transaction, claimed, now).await?;
    let publication = load_building_publication(transaction, claimed, publication_id).await?;
    let projection = load_projection(transaction, publication_id).await?;
    validate_projection(owner, &publication, &projection)?;
    let episodes = projection
        .iter()
        .filter(|row| row.item_type == "Episode")
        .map(|row| row.item_id)
        .collect::<HashSet<_>>();
    seal_structure_sources(
        transaction,
        publication_id,
        owner,
        &episodes,
        publication.source_manifest_sha256.as_deref(),
        publication.expected_source_row_count,
    )
    .await?;
    let update = Query::update()
        .table(Alias::new("catalog_publications"))
        .value(Alias::new("state"), STATE_READY)
        .value(Alias::new("sealed_at"), now)
        .and_where(Expr::col(Alias::new("id")).eq(publication_id.as_uuid()))
        .and_where(Expr::col(Alias::new("state")).eq(STATE_BUILDING))
        .to_owned();
    let backend = transaction.get_database_backend();
    if transaction
        .execute(backend.build(&update))
        .await?
        .rows_affected()
        != 1
    {
        return Err(CatalogPublicationError::InvalidPublication);
    }
    Ok(())
}

async fn load_projection(
    transaction: &DatabaseTransaction,
    publication_id: PublicationId,
) -> Result<Vec<StoredProjectionRow>, CatalogPublicationError> {
    let query = Query::select()
        .columns([
            Alias::new("catalog_item_id"),
            Alias::new("parent_catalog_item_id"),
            Alias::new("item_type"),
            Alias::new("row_sha256"),
        ])
        .from(Alias::new("publication_catalog_items"))
        .and_where(Expr::col(Alias::new("publication_id")).eq(publication_id.as_uuid()))
        .to_owned();
    let backend = transaction.get_database_backend();
    transaction
        .query_all(backend.build(&query))
        .await?
        .iter()
        .map(|row| {
            Ok(StoredProjectionRow {
                item_id: CatalogItemId::from_uuid(row.try_get("", "catalog_item_id")?),
                parent_id: CatalogItemId::from_uuid(row.try_get("", "parent_catalog_item_id")?),
                item_type: row.try_get("", "item_type")?,
                row_sha256: row.try_get("", "row_sha256")?,
            })
        })
        .collect()
}

fn validate_projection(
    owner: CatalogItemId,
    publication: &StoredPublication,
    rows: &[StoredProjectionRow],
) -> Result<(), CatalogPublicationError> {
    if i64::try_from(rows.len()).ok() != Some(publication.expected_row_count)
        || manifest_hash(
            rows.iter()
                .map(|row| (row.item_id, row.row_sha256.as_str())),
        )? != publication.manifest_sha256
    {
        return Err(CatalogPublicationError::ManifestMismatch);
    }
    let parents = rows
        .iter()
        .map(|row| (row.item_id, row.parent_id))
        .collect::<HashMap<_, _>>();
    if parents.contains_key(&owner) {
        return Err(CatalogPublicationError::InvalidStructure);
    }
    let mut resolved = HashSet::from([owner]);
    for row in rows {
        let mut cursor = row.parent_id;
        let mut path = Vec::new();
        let mut visited = HashSet::new();
        while !resolved.contains(&cursor) {
            if !visited.insert(cursor) {
                return Err(CatalogPublicationError::InvalidStructure);
            }
            path.push(cursor);
            cursor = *parents
                .get(&cursor)
                .ok_or(CatalogPublicationError::InvalidStructure)?;
        }
        resolved.extend(path);
    }
    Ok(())
}

async fn ensure_building_publication(
    transaction: &DatabaseTransaction,
    claimed: &ClaimedWorkJob,
    publication_id: PublicationId,
) -> Result<(), CatalogPublicationError> {
    load_building_publication(transaction, claimed, publication_id)
        .await
        .map(|_| ())
}

pub(crate) async fn advance_generation(
    transaction: &DatabaseTransaction,
) -> Result<i64, CatalogPublicationError> {
    crate::advance_catalog_generation(transaction)
        .await
        .map_err(Into::into)
}

pub(crate) async fn activate_publication(
    transaction: &DatabaseTransaction,
    publication_id: PublicationId,
    previous: Option<Uuid>,
    generation: i64,
    now: DateTime<Utc>,
) -> Result<(), CatalogPublicationError> {
    let backend = transaction.get_database_backend();
    if let Some(previous) = previous.filter(|previous| *previous != publication_id.as_uuid()) {
        let retire = Query::update()
            .table(Alias::new("catalog_publications"))
            .value(Alias::new("state"), STATE_RETIRED)
            .value(Alias::new("retired_at"), now)
            .and_where(Expr::col(Alias::new("id")).eq(previous))
            .and_where(Expr::col(Alias::new("state")).eq(STATE_ACTIVE))
            .to_owned();
        transaction.execute(backend.build(&retire)).await?;
    }
    let activate = Query::update()
        .table(Alias::new("catalog_publications"))
        .value(Alias::new("state"), STATE_ACTIVE)
        .value(Alias::new("activated_generation"), generation)
        .value(Alias::new("published_at"), now)
        .and_where(Expr::col(Alias::new("id")).eq(publication_id.as_uuid()))
        .and_where(Expr::col(Alias::new("state")).eq(STATE_READY))
        .to_owned();
    if transaction
        .execute(backend.build(&activate))
        .await?
        .rows_affected()
        != 1
    {
        return Err(CatalogPublicationError::InvalidPublication);
    }
    Ok(())
}

pub(crate) async fn insert_change_event(
    transaction: &DatabaseTransaction,
    owner: CatalogItemId,
    publication_id: PublicationId,
    generation: i64,
    event_type: &str,
    now: DateTime<Utc>,
) -> Result<(), CatalogPublicationError> {
    let insert = Query::insert()
        .into_table(Alias::new("catalog_change_outbox"))
        .columns([
            Alias::new("id"),
            Alias::new("generation"),
            Alias::new("event_type"),
            Alias::new("catalog_item_id"),
            Alias::new("publication_id"),
            Alias::new("created_at"),
        ])
        .values_panic([
            Uuid::new_v4().into(),
            generation.into(),
            event_type.into(),
            owner.as_uuid().into(),
            publication_id.as_uuid().into(),
            now.into(),
        ])
        .to_owned();
    let backend = transaction.get_database_backend();
    transaction.execute(backend.build(&insert)).await?;
    Ok(())
}

fn structure_owner(claimed: &ClaimedWorkJob) -> Result<CatalogItemId, CatalogPublicationError> {
    if claimed.job().task_kind() != WorkTaskKind::ExpandItem {
        return Err(CatalogPublicationError::InvalidWorkKind);
    }
    match claimed.job().scope() {
        WorkScope::CatalogItem(owner) => Ok(owner),
        WorkScope::Library(_)
        | WorkScope::LibraryRootBinding(_)
        | WorkScope::MediaSource(_)
        | WorkScope::StorageRoot(_)
        | WorkScope::StorageObject(_) => Err(CatalogPublicationError::InvalidWorkKind),
    }
}

fn publication_for_job(job_id: Uuid) -> sea_orm::sea_query::SelectStatement {
    Query::select()
        .columns([
            Alias::new("id"),
            Alias::new("job_id"),
            Alias::new("owner_catalog_item_id"),
            Alias::new("publication_kind"),
            Alias::new("expected_revision"),
            Alias::new("input_sync_revision"),
            Alias::new("state"),
            Alias::new("manifest_sha256"),
            Alias::new("expected_row_count"),
            Alias::new("source_manifest_sha256"),
            Alias::new("expected_source_row_count"),
        ])
        .from(Alias::new("catalog_publications"))
        .and_where(Expr::col(Alias::new("job_id")).eq(job_id))
        .to_owned()
}

fn publication_by_id(publication_id: PublicationId) -> sea_orm::sea_query::SelectStatement {
    Query::select()
        .columns([
            Alias::new("job_id"),
            Alias::new("publication_kind"),
            Alias::new("state"),
            Alias::new("expected_revision"),
            Alias::new("expected_row_count"),
            Alias::new("manifest_sha256"),
            Alias::new("source_manifest_sha256"),
            Alias::new("expected_source_row_count"),
        ])
        .from(Alias::new("catalog_publications"))
        .and_where(Expr::col(Alias::new("id")).eq(publication_id.as_uuid()))
        .to_owned()
}

fn owner_publication(owner: CatalogItemId) -> sea_orm::sea_query::SelectStatement {
    Query::select()
        .columns([
            Alias::new("structure_expansion_revision"),
            Alias::new("active_structure_publication_id"),
        ])
        .from(Alias::new("catalog_items"))
        .and_where(Expr::col(Alias::new("id")).eq(owner.as_uuid()))
        .to_owned()
}

fn manifest_hash<'a>(
    entries: impl IntoIterator<Item = (CatalogItemId, &'a str)>,
) -> Result<String, CatalogPublicationError> {
    let mut entries = entries.into_iter().collect::<Vec<_>>();
    entries.sort_unstable_by_key(|(item_id, _)| item_id.as_uuid());
    if entries.windows(2).any(|pair| pair[0].0 == pair[1].0) {
        return Err(CatalogPublicationError::InvalidManifest);
    }
    let mut hasher = Sha256::new();
    for (item_id, row_hash) in entries {
        hasher.update(item_id.as_uuid().as_bytes());
        hasher.update(row_hash.as_bytes());
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn row_hash(row: &StructurePublicationRow) -> String {
    let mut hasher = Sha256::new();
    hasher.update(row.catalog_item_id.as_uuid().as_bytes());
    hasher.update(row.parent_catalog_item_id.as_uuid().as_bytes());
    hasher.update(row.storage_root_id.as_uuid().as_bytes());
    hasher.update(row.scope_storage_object_id.as_uuid().as_bytes());
    hash_text(&mut hasher, &row.item_type);
    hash_text(&mut hasher, &row.name);
    hash_text(&mut hasher, &row.sort_name);
    match row.production_year {
        Some(year) => {
            hasher.update([1]);
            hasher.update(year.to_be_bytes());
        }
        None => hasher.update([0]),
    }
    match &row.overview {
        Some(overview) => {
            hasher.update([1]);
            hash_text(&mut hasher, overview);
        }
        None => hasher.update([0]),
    }
    format!("{:x}", hasher.finalize())
}

fn hash_text(hasher: &mut Sha256, value: &str) {
    hasher.update(value.len().to_be_bytes());
    hasher.update(value.as_bytes());
}

fn valid_text(value: &str, max_chars: usize) -> bool {
    !value.trim().is_empty()
        && value.chars().count() <= max_chars
        && !value.chars().any(char::is_control)
}

pub(crate) async fn finish<T>(
    transaction: DatabaseTransaction,
    result: Result<T, CatalogPublicationError>,
) -> Result<T, CatalogPublicationError> {
    match result {
        Ok(value) => {
            transaction.commit().await?;
            Ok(value)
        }
        Err(error) => match transaction.rollback().await {
            Ok(()) => Err(error),
            Err(rollback) => Err(CatalogPublicationError::RollbackFailed {
                original: error.to_string(),
                rollback,
            }),
        },
    }
}
