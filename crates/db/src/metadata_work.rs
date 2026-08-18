use std::{collections::HashSet, path::Path};

use sea_orm::{
    ConnectionTrait, DatabaseConnection, DbErr, TransactionTrait,
    sea_query::{Alias, Cond, Expr, JoinType, Query},
};
use serde_json::json;
use thiserror::Error;
use tjxy_common::{
    CatalogItemId, ImageType, StorageObjectRecordId, StorageRootId, parse_media_name,
};
use tjxy_metadata::{MetadataItemKind, MetadataLookup, MetadataResolution};
use uuid::Uuid;

use crate::{
    AssetPublication, AssetRepositoryError, ClaimedWorkJob, MetadataPublicationError,
    MetadataRequirement, WorkJobRepository, WorkJobRepositoryError, WorkJobResult, WorkJobSpec,
    WorkJobSubmission, WorkScope, WorkTaskKind,
};

const MAX_NFO_CANDIDATES: u64 = 512;
const MAX_NFO_BYTES: u64 = 2 * 1024 * 1024;
const MAX_IMAGE_CANDIDATES: u64 = 512;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetadataSidecarCandidate {
    record_id: StorageObjectRecordId,
    storage_account_id: Uuid,
    provider: String,
    provider_drive_id: String,
    provider_object_id: String,
    name: String,
    size: u64,
    remote_revision: Option<String>,
    observed_sync_revision: i64,
    facts_observed_storage_root_id: Option<StorageRootId>,
}

impl MetadataSidecarCandidate {
    #[must_use]
    pub const fn record_id(&self) -> StorageObjectRecordId {
        self.record_id
    }

    #[must_use]
    pub const fn storage_account_id(&self) -> Uuid {
        self.storage_account_id
    }

    #[must_use]
    pub fn provider(&self) -> &str {
        &self.provider
    }

    #[must_use]
    pub fn provider_drive_id(&self) -> &str {
        &self.provider_drive_id
    }

    #[must_use]
    pub fn provider_object_id(&self) -> &str {
        &self.provider_object_id
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub const fn size(&self) -> u64 {
        self.size
    }

    #[must_use]
    pub fn remote_revision(&self) -> Option<&str> {
        self.remote_revision.as_deref()
    }

    #[must_use]
    pub const fn observed_sync_revision(&self) -> i64 {
        self.observed_sync_revision
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetadataImageCandidate {
    file: MetadataSidecarCandidate,
    image_type: ImageType,
}

impl MetadataImageCandidate {
    #[must_use]
    pub const fn file(&self) -> &MetadataSidecarCandidate {
        &self.file
    }

    #[must_use]
    pub const fn image_type(&self) -> ImageType {
        self.image_type
    }
}

#[derive(Clone, Debug)]
pub struct MetadataWorkSnapshot {
    lookup: MetadataLookup,
    sidecar: Option<MetadataSidecarCandidate>,
    images: Vec<MetadataImageCandidate>,
    scope: crate::catalog_storage_scope::CatalogStorageScope,
}

impl MetadataWorkSnapshot {
    #[must_use]
    pub fn lookup(&self) -> &MetadataLookup {
        &self.lookup
    }

    #[must_use]
    pub const fn sidecar(&self) -> Option<&MetadataSidecarCandidate> {
        self.sidecar.as_ref()
    }

    #[must_use]
    pub fn images(&self) -> &[MetadataImageCandidate] {
        &self.images
    }

    #[must_use]
    pub const fn storage_root_id(&self) -> StorageRootId {
        self.scope.storage_root_id()
    }
}

pub struct MetadataWorkRepository<'connection> {
    database: &'connection DatabaseConnection,
}

impl<'connection> MetadataWorkRepository<'connection> {
    #[must_use]
    pub const fn new(database: &'connection DatabaseConnection) -> Self {
        Self { database }
    }

    /// Enqueues explicit metadata resolution for one visible matched `CatalogItem`.
    ///
    /// # Errors
    ///
    /// Returns [`MetadataWorkError`] for unavailable/ambiguous scope or SQL failures.
    pub async fn enqueue(
        &self,
        item_id: CatalogItemId,
        priority: i32,
    ) -> Result<WorkJobSubmission, MetadataWorkError> {
        let scope = metadata_storage_scope(self.database, item_id, None).await?;
        let row = self
            .database
            .query_one(
                self.database
                    .get_database_backend()
                    .build(&metadata_schedule_query(item_id)),
            )
            .await?
            .ok_or(MetadataWorkError::StaleOrUnavailable)?;
        let metadata_revision = row.try_get::<i64>("", "metadata_revision")?;
        WorkJobRepository::new(self.database)
            .enqueue_or_join(
                &WorkJobSpec::new(
                    WorkTaskKind::ResolveMetadata,
                    WorkScope::CatalogItem(item_id),
                    metadata_revision,
                    priority,
                )?
                .with_metadata_requirement(MetadataRequirement::Full)?
                .with_input_sync_revision(scope.metadata_input_revision())?,
            )
            .await
            .map_err(Into::into)
    }

    /// Selects a reconciled direct-child NFO exclusively from SQL inventory.
    ///
    /// # Errors
    ///
    /// Returns [`MetadataWorkError`] for invalid/stale claims, ambiguous sidecars, or SQL errors.
    #[allow(clippy::too_many_lines)] // Keeps inventory fencing and sidecar selection in one auditable snapshot.
    pub async fn snapshot(
        &self,
        claimed: &ClaimedWorkJob,
    ) -> Result<MetadataWorkSnapshot, MetadataWorkError> {
        let WorkScope::CatalogItem(item_id) = claimed.job().scope() else {
            return Err(MetadataWorkError::InvalidClaim);
        };
        if claimed.job().task_kind() != WorkTaskKind::ResolveMetadata {
            return Err(MetadataWorkError::InvalidClaim);
        }
        let input_revision = claimed
            .job()
            .input_sync_revision()
            .ok_or(MetadataWorkError::MissingSyncRevision)?;
        let scope = metadata_storage_scope(
            self.database,
            item_id,
            claimed.job().storage_root_affinity(),
        )
        .await?;
        if !scope.accepts_metadata_input(input_revision)
            || !crate::catalog_storage_scope::storage_scope_is_reconciled(
                self.database,
                scope,
                scope.children_indexed(),
            )
            .await?
        {
            return Err(MetadataWorkError::StaleOrUnavailable);
        }
        let item = self
            .database
            .query_one(
                self.database
                    .get_database_backend()
                    .build(&target_query(item_id, claimed.job().expected_revision())),
            )
            .await?
            .ok_or(MetadataWorkError::StaleOrUnavailable)?;
        let kind = parse_kind(&item.try_get::<String>("", "item_type")?)?;
        let stored_name = item.try_get::<String>("", "name")?;
        let stored_year = item.try_get::<Option<i32>>("", "production_year")?;
        let legacy_parts = (stored_year.is_none()
            && matches!(kind, MetadataItemKind::Movie | MetadataItemKind::Series))
        .then(|| parse_media_name(&stored_name).ok())
        .flatten()
        .and_then(|parsed| {
            parsed
                .title()
                .zip(parsed.year())
                .map(|(name, year)| (name.to_owned(), Some(year)))
        });
        let (name, production_year) = legacy_parts.unwrap_or((stored_name, stored_year));
        let lookup = MetadataLookup::new(kind, name, production_year)
            .map_err(|_| MetadataWorkError::InvalidStoredMetadata)?;
        let flat_file_stem = if scope.is_file() {
            storage_object_stem(self.database, scope.storage_object_id()).await?
        } else {
            None
        };
        let rows = self
            .database
            .query_all(
                self.database
                    .get_database_backend()
                    .build(&sibling_file_query(
                        item_id,
                        scope,
                        input_revision,
                        SiblingFileKind::Nfo,
                    )),
            )
            .await?;
        if u64::try_from(rows.len()).unwrap_or(u64::MAX) > MAX_NFO_CANDIDATES {
            return Err(MetadataWorkError::TooManySidecars);
        }
        let mut candidates = rows
            .iter()
            .map(sidecar_from_row)
            .collect::<Result<Vec<_>, _>>()?;
        if let Some(stem) = &flat_file_stem {
            candidates.retain(|candidate| sidecar_stem_matches(&candidate.name, stem));
        }
        let video_names = if kind == MetadataItemKind::Episode {
            crate::source_publication::effective_video_storage_names(
                self.database,
                item_id,
                scope.storage_root_id(),
                scope.storage_object_id(),
            )
            .await?
        } else {
            Vec::new()
        };
        let sidecar = select_sidecar(kind, &mut candidates, &video_names)?;
        let image_rows = self
            .database
            .query_all(
                self.database
                    .get_database_backend()
                    .build(&sibling_file_query(
                        item_id,
                        scope,
                        input_revision,
                        SiblingFileKind::Image,
                    )),
            )
            .await?;
        if u64::try_from(image_rows.len()).unwrap_or(u64::MAX) > MAX_IMAGE_CANDIDATES {
            return Err(MetadataWorkError::TooManyImages);
        }
        let mut image_candidates = image_rows
            .iter()
            .map(sidecar_from_row)
            .collect::<Result<Vec<_>, _>>()?;
        if let Some(stem) = &flat_file_stem {
            image_candidates.retain(|candidate| sidecar_stem_matches(&candidate.name, stem));
        }
        let images = select_images(&image_candidates);
        Ok(MetadataWorkSnapshot {
            lookup,
            sidecar,
            images,
            scope,
        })
    }

    /// Atomically fences the claim/item revision, publishes metadata, and completes work.
    ///
    /// # Errors
    ///
    /// Returns [`MetadataWorkError`] when any publication or fencing step fails.
    pub async fn commit(
        &self,
        claimed: &ClaimedWorkJob,
        snapshot: &MetadataWorkSnapshot,
        resolution: &MetadataResolution,
        asset_publications: &[&AssetPublication],
        used_nfo: bool,
        warnings: Vec<String>,
    ) -> Result<MetadataWorkCommitReport, MetadataWorkError> {
        let WorkScope::CatalogItem(item_id) = claimed.job().scope() else {
            return Err(MetadataWorkError::InvalidClaim);
        };
        let input_revision = claimed
            .job()
            .input_sync_revision()
            .ok_or(MetadataWorkError::MissingSyncRevision)?;
        let transaction = self.database.begin().await?;
        let result = async {
            let requirement = fence_metadata_requirement(&transaction, claimed).await?;
            fence_metadata_revision(&transaction, item_id, claimed.job().expected_revision())
                .await?;
            let scope = metadata_storage_scope(
                &transaction,
                item_id,
                claimed.job().storage_root_affinity(),
            )
            .await?;
            if !scope.has_same_inventory(snapshot.scope)
                || !scope.accepts_metadata_input(input_revision)
                || !crate::catalog_storage_scope::storage_scope_is_reconciled(
                    &transaction,
                    scope,
                    scope.children_indexed(),
                )
                .await?
            {
                return Err(MetadataWorkError::StaleOrUnavailable);
            }
            revalidate_metadata_snapshot(&transaction, item_id, scope, input_revision, snapshot)
                .await?;
            let publication = crate::metadata::publish_in_transaction(
                &transaction,
                item_id,
                resolution,
                requirement,
            )
            .await?;
            advance_metadata_watermark(
                &transaction,
                item_id,
                claimed.job().expected_revision(),
                requirement,
            )
            .await?;
            let mut asset_changed = false;
            for asset in asset_publications {
                let report = crate::asset::publish_in_transaction(
                    &transaction,
                    asset,
                    !publication.changed() && !asset_changed,
                )
                .await?;
                asset_changed |= report.reference_changed();
            }
            WorkJobRepository::new(self.database)
                .complete_in_transaction(
                    &transaction,
                    claimed,
                    WorkJobResult::success(
                        json!({
                            "changed": publication.changed() || asset_changed,
                            "image_changed": asset_changed,
                            "matched": !resolution.provider_ids().is_empty(),
                            "state": resolution.state().as_str(),
                            "used_nfo": used_nfo
                        }),
                        warnings,
                    ),
                )
                .await?;
            Ok(MetadataWorkCommitReport {
                changed: publication.changed() || asset_changed,
                asset_changed,
            })
        }
        .await;
        match result {
            Ok(value) => {
                transaction.commit().await?;
                Ok(value)
            }
            Err(error) => {
                transaction.rollback().await?;
                Err(error)
            }
        }
    }

    /// Publishes only source-object references for Direct mode without importing metadata bytes.
    /// Commits direct metadata references for a claimed metadata job.
    ///
    /// # Errors
    ///
    /// Returns a work error when the claim, snapshot, or referenced objects are stale or invalid.
    pub async fn commit_direct(
        &self,
        claimed: &ClaimedWorkJob,
        snapshot: &MetadataWorkSnapshot,
    ) -> Result<MetadataWorkCommitReport, MetadataWorkError> {
        let WorkScope::CatalogItem(item_id) = claimed.job().scope() else {
            return Err(MetadataWorkError::InvalidClaim);
        };
        let input_revision = claimed
            .job()
            .input_sync_revision()
            .ok_or(MetadataWorkError::MissingSyncRevision)?;
        let transaction = self.database.begin().await?;
        let result = async {
            let requirement = fence_metadata_requirement(&transaction, claimed).await?;
            fence_metadata_revision(&transaction, item_id, claimed.job().expected_revision())
                .await?;
            let scope = metadata_storage_scope(
                &transaction,
                item_id,
                claimed.job().storage_root_affinity(),
            )
            .await?;
            if !scope.has_same_inventory(snapshot.scope)
                || !scope.accepts_metadata_input(input_revision)
            {
                return Err(MetadataWorkError::StaleOrUnavailable);
            }
            revalidate_metadata_snapshot(&transaction, item_id, scope, input_revision, snapshot)
                .await?;
            let library_ids =
                direct_library_ids(&transaction, item_id, scope.storage_root_id()).await?;
            replace_direct_refs(
                &transaction,
                item_id,
                &library_ids,
                snapshot,
                input_revision,
            )
            .await?;
            advance_metadata_watermark(
                &transaction,
                item_id,
                claimed.job().expected_revision(),
                requirement,
            )
            .await?;
            WorkJobRepository::new(self.database)
                .complete_in_transaction(
                    &transaction,
                    claimed,
                    WorkJobResult::success(
                        json!({
                            "changed": true,
                            "direct": true,
                            "references": library_ids.len()
                                * (usize::from(snapshot.sidecar.is_some()) + snapshot.images.len())
                        }),
                        Vec::new(),
                    ),
                )
                .await?;
            crate::advance_catalog_generation(&transaction).await?;
            Ok(MetadataWorkCommitReport {
                changed: true,
                asset_changed: !snapshot.images.is_empty(),
            })
        }
        .await;
        match result {
            Ok(value) => {
                transaction.commit().await?;
                Ok(value)
            }
            Err(error) => {
                transaction.rollback().await?;
                Err(error)
            }
        }
    }
}

async fn direct_library_ids(
    connection: &impl ConnectionTrait,
    item_id: CatalogItemId,
    root_id: StorageRootId,
) -> Result<Vec<Uuid>, DbErr> {
    let membership = Alias::new("direct_membership");
    let library = Alias::new("direct_library");
    let binding = Alias::new("direct_binding");
    let query = Query::select()
        .distinct()
        .expr_as(
            Expr::col((library.clone(), Alias::new("id"))),
            Alias::new("library_id"),
        )
        .from_as(Alias::new("library_catalog_items"), membership.clone())
        .join_as(
            JoinType::InnerJoin,
            Alias::new("libraries"),
            library.clone(),
            Expr::col((library.clone(), Alias::new("id")))
                .equals((membership.clone(), Alias::new("library_id"))),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("library_storage_roots"),
            binding.clone(),
            Expr::col((binding.clone(), Alias::new("library_id")))
                .equals((library.clone(), Alias::new("id"))),
        )
        .and_where(Expr::col((membership, Alias::new("catalog_item_id"))).eq(item_id.as_uuid()))
        .and_where(Expr::col((binding, Alias::new("storage_root_id"))).eq(root_id.as_uuid()))
        .and_where(Expr::col((library.clone(), Alias::new("is_enabled"))).eq(true))
        .and_where(
            Expr::col((library.clone(), Alias::new("metadata_source_mode"))).eq("local_only"),
        )
        .and_where(Expr::col((library, Alias::new("local_metadata_access_mode"))).eq("direct"))
        .to_owned();
    connection
        .query_all(connection.get_database_backend().build(&query))
        .await?
        .into_iter()
        .map(|row| row.try_get("", "library_id"))
        .collect()
}

async fn replace_direct_refs(
    transaction: &sea_orm::DatabaseTransaction,
    item_id: CatalogItemId,
    library_ids: &[Uuid],
    snapshot: &MetadataWorkSnapshot,
    input_revision: i64,
) -> Result<(), DbErr> {
    let backend = transaction.get_database_backend();
    transaction
        .execute(
            backend.build(
                Query::delete()
                    .from_table(Alias::new("direct_metadata_refs"))
                    .and_where(Expr::col(Alias::new("catalog_item_id")).eq(item_id.as_uuid()))
                    .and_where(
                        Expr::col(Alias::new("storage_root_id"))
                            .eq(snapshot.storage_root_id().as_uuid()),
                    ),
            ),
        )
        .await?;
    for library_id in library_ids {
        let mut primary_priority = 0_i32;
        let mut backdrop_priority = 0_i32;
        let image_resources = snapshot.images.iter().map(|image| {
            let (kind, priority) = if image.image_type() == ImageType::Primary {
                let value = primary_priority;
                primary_priority = primary_priority.saturating_add(1);
                ("Primary", value)
            } else {
                let value = backdrop_priority;
                backdrop_priority = backdrop_priority.saturating_add(1);
                ("Backdrop", value)
            };
            (image.file(), kind, priority)
        });
        let resources = snapshot
            .sidecar
            .iter()
            .map(|file| (file, "Nfo", 0_i32))
            .chain(image_resources);
        for (file, kind, priority) in resources {
            transaction
                .execute(
                    backend.build(
                        Query::insert()
                            .into_table(Alias::new("direct_metadata_refs"))
                            .columns([
                                Alias::new("id"),
                                Alias::new("library_id"),
                                Alias::new("catalog_item_id"),
                                Alias::new("storage_root_id"),
                                Alias::new("storage_object_id"),
                                Alias::new("resource_kind"),
                                Alias::new("priority"),
                                Alias::new("input_revision"),
                            ])
                            .values_panic([
                                Uuid::new_v4().into(),
                                (*library_id).into(),
                                item_id.as_uuid().into(),
                                snapshot.storage_root_id().as_uuid().into(),
                                file.record_id().as_uuid().into(),
                                kind.into(),
                                priority.into(),
                                input_revision.into(),
                            ]),
                    ),
                )
                .await?;
        }
    }
    Ok(())
}

async fn storage_object_stem(
    connection: &impl ConnectionTrait,
    object_id: StorageObjectRecordId,
) -> Result<Option<String>, DbErr> {
    let query = Query::select()
        .column(Alias::new("name"))
        .from(Alias::new("storage_objects"))
        .and_where(Expr::col(Alias::new("id")).eq(object_id.as_uuid()))
        .limit(1)
        .to_owned();
    let backend = connection.get_database_backend();
    let Some(row) = connection.query_one(backend.build(&query)).await? else {
        return Ok(None);
    };
    let name: String = row.try_get("", "name")?;
    Ok(Path::new(&name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(str::to_ascii_lowercase))
}

fn sidecar_stem_matches(name: &str, video_stem: &str) -> bool {
    Path::new(name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .is_some_and(|stem| {
            let stem = stem.to_ascii_lowercase();
            stem == video_stem || stem.starts_with(&format!("{video_stem}-"))
        })
}

async fn revalidate_metadata_snapshot(
    transaction: &sea_orm::DatabaseTransaction,
    item_id: CatalogItemId,
    scope: crate::catalog_storage_scope::CatalogStorageScope,
    input_revision: i64,
    snapshot: &MetadataWorkSnapshot,
) -> Result<(), MetadataWorkError> {
    let rows = transaction
        .query_all(
            transaction
                .get_database_backend()
                .build(&sibling_file_query(
                    item_id,
                    scope,
                    input_revision,
                    SiblingFileKind::Nfo,
                )),
        )
        .await?;
    if u64::try_from(rows.len()).unwrap_or(u64::MAX) > MAX_NFO_CANDIDATES {
        return Err(MetadataWorkError::TooManySidecars);
    }
    let mut candidates = rows
        .iter()
        .map(sidecar_from_row)
        .collect::<Result<Vec<_>, _>>()?;
    let video_names = if snapshot.lookup.kind() == MetadataItemKind::Episode {
        crate::source_publication::effective_video_storage_names(
            transaction,
            item_id,
            scope.storage_root_id(),
            scope.storage_object_id(),
        )
        .await?
    } else {
        Vec::new()
    };
    if select_sidecar(snapshot.lookup.kind(), &mut candidates, &video_names)? != snapshot.sidecar {
        return Err(MetadataWorkError::StaleOrUnavailable);
    }
    let image_rows = transaction
        .query_all(
            transaction
                .get_database_backend()
                .build(&sibling_file_query(
                    item_id,
                    scope,
                    input_revision,
                    SiblingFileKind::Image,
                )),
        )
        .await?;
    if u64::try_from(image_rows.len()).unwrap_or(u64::MAX) > MAX_IMAGE_CANDIDATES {
        return Err(MetadataWorkError::TooManyImages);
    }
    let images = select_images(
        &image_rows
            .iter()
            .map(sidecar_from_row)
            .collect::<Result<Vec<_>, _>>()?,
    );
    if images != snapshot.images {
        return Err(MetadataWorkError::StaleOrUnavailable);
    }
    Ok(())
}

async fn fence_metadata_revision(
    transaction: &sea_orm::DatabaseTransaction,
    item_id: CatalogItemId,
    expected_revision: i64,
) -> Result<(), MetadataWorkError> {
    let fence = Query::update()
        .table(Alias::new("catalog_items"))
        .value(
            Alias::new("metadata_revision"),
            Expr::col(Alias::new("metadata_revision")),
        )
        .and_where(Expr::col(Alias::new("id")).eq(item_id.as_uuid()))
        .and_where(Expr::col(Alias::new("metadata_revision")).eq(expected_revision))
        .to_owned();
    let backend = transaction.get_database_backend();
    if transaction
        .execute(backend.build(&fence))
        .await?
        .rows_affected()
        != 1
    {
        return Err(MetadataWorkError::StaleOrUnavailable);
    }
    Ok(())
}

async fn advance_metadata_watermark(
    transaction: &sea_orm::DatabaseTransaction,
    item_id: CatalogItemId,
    expected_revision: i64,
    requirement: MetadataRequirement,
) -> Result<(), MetadataWorkError> {
    let resolved = Query::update()
        .table(Alias::new("catalog_items"))
        .value(Alias::new("metadata_resolved_revision"), expected_revision)
        .value(
            Alias::new("metadata_resolved_requirement"),
            requirement.as_i32(),
        )
        .and_where(Expr::col(Alias::new("id")).eq(item_id.as_uuid()))
        .and_where(Expr::col(Alias::new("metadata_revision")).eq(expected_revision))
        .to_owned();
    let backend = transaction.get_database_backend();
    if transaction
        .execute(backend.build(&resolved))
        .await?
        .rows_affected()
        != 1
    {
        return Err(MetadataWorkError::StaleOrUnavailable);
    }
    Ok(())
}

async fn fence_metadata_requirement(
    transaction: &sea_orm::DatabaseTransaction,
    claimed: &ClaimedWorkJob,
) -> Result<MetadataRequirement, MetadataWorkError> {
    crate::work_job::fence_live_claim(transaction, claimed, chrono::Utc::now()).await?;
    let requirement = claimed
        .job()
        .metadata_requirement()
        .ok_or(MetadataWorkError::InvalidClaim)?;
    let fence = Query::update()
        .table(Alias::new("work_jobs"))
        .value(Alias::new("metadata_requirement"), requirement.as_i32())
        .and_where(Expr::col(Alias::new("id")).eq(claimed.id().as_uuid()))
        .and_where(Expr::col(Alias::new("state")).eq("Running"))
        .and_where(Expr::col(Alias::new("metadata_requirement")).eq(requirement.as_i32()))
        .to_owned();
    let backend = transaction.get_database_backend();
    if transaction
        .execute(backend.build(&fence))
        .await?
        .rows_affected()
        != 1
    {
        return Err(MetadataWorkError::RequirementUpgraded);
    }
    Ok(requirement)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MetadataWorkCommitReport {
    changed: bool,
    asset_changed: bool,
}

impl MetadataWorkCommitReport {
    #[must_use]
    pub const fn changed(self) -> bool {
        self.changed
    }

    #[must_use]
    pub const fn asset_changed(self) -> bool {
        self.asset_changed
    }
}

#[derive(Debug, Error)]
pub enum MetadataWorkError {
    #[error("claimed work is not a metadata resolution job")]
    InvalidClaim,
    #[error("metadata work does not record its synchronized input revision")]
    MissingSyncRevision,
    #[error("metadata work is stale, unauthorized, or unavailable")]
    StaleOrUnavailable,
    #[error("metadata inventory has too many NFO candidates")]
    TooManySidecars,
    #[error("metadata inventory has too many local image candidates")]
    TooManyImages,
    #[error("metadata inventory has ambiguous NFO candidates")]
    AmbiguousSidecars,
    #[error("metadata work resolves to more than one authorized storage scope")]
    AmbiguousStorageScope,
    #[error("metadata NFO sidecar is empty or too large")]
    InvalidSidecarSize,
    #[error("stored metadata work input is invalid")]
    InvalidStoredMetadata,
    #[error("metadata requirement was upgraded while the job was running")]
    RequirementUpgraded,
    #[error("metadata work database operation failed: {0}")]
    Database(#[from] DbErr),
    #[error("metadata publication failed: {0}")]
    Publication(#[from] MetadataPublicationError),
    #[error("metadata image publication failed: {0}")]
    Asset(#[from] AssetRepositoryError),
    #[error("metadata source publication lookup failed: {0}")]
    SourcePublication(#[from] crate::CatalogPublicationError),
    #[error("metadata work lease operation failed: {0}")]
    Work(#[from] WorkJobRepositoryError),
}

async fn metadata_storage_scope(
    database: &impl ConnectionTrait,
    item_id: CatalogItemId,
    storage_root: Option<StorageRootId>,
) -> Result<crate::catalog_storage_scope::CatalogStorageScope, MetadataWorkError> {
    crate::catalog_storage_scope::resolve_catalog_storage_scope(database, item_id, storage_root)
        .await
        .map_err(|error| match error {
            crate::catalog_storage_scope::CatalogStorageScopeError::Ambiguous => {
                MetadataWorkError::AmbiguousStorageScope
            }
            crate::catalog_storage_scope::CatalogStorageScopeError::Database(error) => {
                MetadataWorkError::Database(error)
            }
        })?
        .ok_or(MetadataWorkError::StaleOrUnavailable)
}

fn metadata_schedule_query(item_id: CatalogItemId) -> sea_orm::sea_query::SelectStatement {
    let item = Alias::new("schedule_metadata_item");
    Query::select()
        .expr_as(
            Expr::col((item.clone(), Alias::new("metadata_revision"))),
            Alias::new("metadata_revision"),
        )
        .from_as(Alias::new("catalog_items"), item.clone())
        .and_where(Expr::col((item.clone(), Alias::new("id"))).eq(item_id.as_uuid()))
        .and_where(Expr::col((item.clone(), Alias::new("is_present"))).eq(true))
        .and_where(Expr::col((item, Alias::new("classification_state"))).eq("Matched"))
        .limit(1)
        .to_owned()
}

fn target_query(
    item_id: CatalogItemId,
    expected_revision: i64,
) -> sea_orm::sea_query::SelectStatement {
    let item = Alias::new("metadata_item");
    Query::select()
        .expr_as(
            Expr::col((item.clone(), Alias::new("item_type"))),
            Alias::new("item_type"),
        )
        .expr_as(
            Expr::col((item.clone(), Alias::new("name"))),
            Alias::new("name"),
        )
        .expr_as(
            Expr::col((item.clone(), Alias::new("production_year"))),
            Alias::new("production_year"),
        )
        .from_as(Alias::new("catalog_items"), item.clone())
        .and_where(Expr::col((item.clone(), Alias::new("id"))).eq(item_id.as_uuid()))
        .and_where(Expr::col((item.clone(), Alias::new("metadata_revision"))).eq(expected_revision))
        .and_where(Expr::col((item.clone(), Alias::new("is_present"))).eq(true))
        .and_where(Expr::col((item, Alias::new("classification_state"))).eq("Matched"))
        .limit(1)
        .to_owned()
}

#[allow(clippy::too_many_lines)] // The aliases keep one authorization snapshot legible and auditable.
fn sibling_file_query(
    item_id: CatalogItemId,
    scope: crate::catalog_storage_scope::CatalogStorageScope,
    input_revision: i64,
    file_kind: SiblingFileKind,
) -> sea_orm::sea_query::SelectStatement {
    let item = Alias::new("nfo_item");
    let parent = Alias::new("nfo_parent");
    let child = Alias::new("nfo_child");
    let object = Alias::new("nfo_object");
    let account = Alias::new("nfo_account");
    let root = Alias::new("nfo_root");
    let library_root = Alias::new("nfo_library_root");
    let membership = Alias::new("nfo_membership");
    let library = Alias::new("nfo_library");
    Query::select()
        .distinct()
        .expr_as(
            Expr::col((object.clone(), Alias::new("id"))),
            Alias::new("storage_object_id"),
        )
        .expr_as(
            Expr::col((object.clone(), Alias::new("storage_account_id"))),
            Alias::new("storage_account_id"),
        )
        .expr_as(
            Expr::col((account.clone(), Alias::new("provider"))),
            Alias::new("provider"),
        )
        .expr_as(
            Expr::col((object.clone(), Alias::new("provider_drive_id"))),
            Alias::new("provider_drive_id"),
        )
        .expr_as(
            Expr::col((object.clone(), Alias::new("provider_object_id"))),
            Alias::new("provider_object_id"),
        )
        .expr_as(
            Expr::col((object.clone(), Alias::new("name"))),
            Alias::new("name"),
        )
        .expr_as(
            Expr::col((object.clone(), Alias::new("size"))),
            Alias::new("size"),
        )
        .expr_as(
            Expr::col((object.clone(), Alias::new("remote_revision"))),
            Alias::new("remote_revision"),
        )
        .expr_as(
            Expr::col((object.clone(), Alias::new("observed_sync_revision"))),
            Alias::new("object_observed_sync_revision"),
        )
        .expr_as(
            Expr::col((object.clone(), Alias::new("facts_observed_storage_root_id"))),
            Alias::new("facts_observed_storage_root_id"),
        )
        .from_as(Alias::new("storage_root_objects"), parent.clone())
        .join_as(
            JoinType::InnerJoin,
            Alias::new("catalog_items"),
            item.clone(),
            Expr::col((item.clone(), Alias::new("id"))).eq(item_id.as_uuid()),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("storage_root_objects"),
            child.clone(),
            Cond::all()
                .add(
                    Expr::col((child.clone(), Alias::new("storage_root_id")))
                        .equals((parent.clone(), Alias::new("storage_root_id"))),
                )
                .add(
                    Expr::col((child.clone(), Alias::new("parent_storage_object_id")))
                        .equals((parent.clone(), Alias::new("storage_object_id"))),
                ),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("storage_objects"),
            object.clone(),
            Expr::col((object.clone(), Alias::new("id")))
                .equals((child.clone(), Alias::new("storage_object_id"))),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("storage_accounts"),
            account.clone(),
            Expr::col((account.clone(), Alias::new("id")))
                .equals((object.clone(), Alias::new("storage_account_id"))),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("storage_roots"),
            root.clone(),
            Expr::col((root.clone(), Alias::new("id")))
                .equals((parent.clone(), Alias::new("storage_root_id"))),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("library_storage_roots"),
            library_root.clone(),
            Expr::col((library_root.clone(), Alias::new("storage_root_id")))
                .equals((root.clone(), Alias::new("id"))),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("library_catalog_items"),
            membership.clone(),
            Cond::all()
                .add(
                    Expr::col((membership.clone(), Alias::new("library_id")))
                        .equals((library_root, Alias::new("library_id"))),
                )
                .add(
                    Cond::any()
                        .add(
                            Expr::col((membership.clone(), Alias::new("catalog_item_id")))
                                .equals((item.clone(), Alias::new("id"))),
                        )
                        .add(
                            Expr::col((membership.clone(), Alias::new("catalog_item_id")))
                                .equals((item.clone(), Alias::new("structure_owner_item_id"))),
                        ),
                ),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("libraries"),
            library.clone(),
            Expr::col((library.clone(), Alias::new("id")))
                .equals((membership, Alias::new("library_id"))),
        )
        .and_where(
            Expr::col((parent.clone(), Alias::new("storage_root_id")))
                .eq(scope.storage_root_id().as_uuid()),
        )
        .and_where(
            Expr::col((parent.clone(), Alias::new("storage_object_id")))
                .eq(scope.sidecar_parent_object_id().as_uuid()),
        )
        .and_where(Expr::col((item.clone(), Alias::new("is_present"))).eq(true))
        .and_where(Expr::col((item, Alias::new("classification_state"))).eq("Matched"))
        .and_where(Expr::col((parent.clone(), Alias::new("presence_state"))).eq("Present"))
        .and_where(Expr::col((parent.clone(), Alias::new("children_indexed"))).eq(true))
        .and_where(Expr::col((parent, Alias::new("children_index_revision"))).eq(input_revision))
        .and_where(
            Expr::col((child.clone(), Alias::new("presence_state")))
                .is_in(["Present", "TemporarilyUnavailable"]),
        )
        .and_where(Expr::col((object.clone(), Alias::new("object_type"))).eq("File"))
        .and_where(Expr::col((object.clone(), Alias::new("presence_state"))).eq("Present"))
        .and_where(match file_kind {
            SiblingFileKind::Nfo => {
                Expr::col((object.clone(), Alias::new("normalized_name"))).like("%.nfo")
            }
            SiblingFileKind::Image => Cond::any()
                .add(Expr::col((object.clone(), Alias::new("normalized_name"))).like("%.jpg"))
                .add(Expr::col((object.clone(), Alias::new("normalized_name"))).like("%.jpeg"))
                .add(Expr::col((object.clone(), Alias::new("normalized_name"))).like("%.png"))
                .add(Expr::col((object.clone(), Alias::new("normalized_name"))).like("%.webp"))
                .add(Expr::col((object.clone(), Alias::new("normalized_name"))).like("%.bmp"))
                .add(Expr::col((object, Alias::new("normalized_name"))).like("%.gif"))
                .into(),
        })
        .and_where(Expr::col((account, Alias::new("status"))).eq("Active"))
        .and_where(Expr::col((root, Alias::new("reconciled_sync_revision"))).gte(input_revision))
        .and_where(Expr::col((library.clone(), Alias::new("is_enabled"))).eq(true))
        .limit(match file_kind {
            SiblingFileKind::Nfo => MAX_NFO_CANDIDATES + 1,
            SiblingFileKind::Image => MAX_IMAGE_CANDIDATES + 1,
        })
        .to_owned()
}

#[derive(Clone, Copy)]
enum SiblingFileKind {
    Nfo,
    Image,
}

fn sidecar_from_row(
    row: &sea_orm::QueryResult,
) -> Result<MetadataSidecarCandidate, MetadataWorkError> {
    let size: i64 = row.try_get("", "size")?;
    let size = u64::try_from(size).map_err(|_| MetadataWorkError::InvalidSidecarSize)?;
    Ok(MetadataSidecarCandidate {
        record_id: StorageObjectRecordId::from_uuid(row.try_get("", "storage_object_id")?),
        storage_account_id: row.try_get("", "storage_account_id")?,
        provider: row.try_get("", "provider")?,
        provider_drive_id: row.try_get("", "provider_drive_id")?,
        provider_object_id: row.try_get("", "provider_object_id")?,
        name: row.try_get("", "name")?,
        size,
        remote_revision: row.try_get("", "remote_revision")?,
        observed_sync_revision: row.try_get("", "object_observed_sync_revision")?,
        facts_observed_storage_root_id: row
            .try_get::<Option<Uuid>>("", "facts_observed_storage_root_id")?
            .map(StorageRootId::from_uuid),
    })
}

fn select_images(candidates: &[MetadataSidecarCandidate]) -> Vec<MetadataImageCandidate> {
    let mut selected = Vec::new();
    for image_type in [ImageType::Primary, ImageType::Backdrop] {
        let mut matches = candidates
            .iter()
            .filter_map(|candidate| {
                image_rank(&candidate.name, image_type)
                    .map(|rank| (rank, candidate.name.to_ascii_lowercase(), candidate.clone()))
            })
            .collect::<Vec<_>>();
        matches.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
        if let Some((_, _, file)) = matches.into_iter().next() {
            selected.push(MetadataImageCandidate { file, image_type });
        }
    }
    selected
}

fn image_rank(name: &str, image_type: ImageType) -> Option<u8> {
    let stem = Path::new(name).file_stem()?.to_str()?.to_ascii_lowercase();
    match image_type {
        ImageType::Primary => match stem.as_str() {
            "poster" => Some(0),
            "folder" => Some(1),
            "cover" => Some(2),
            _ if stem.ends_with("-poster") => Some(3),
            _ => None,
        },
        ImageType::Backdrop => match stem.as_str() {
            "fanart" => Some(0),
            "backdrop" => Some(1),
            _ => None,
        },
        _ => None,
    }
}

fn select_sidecar(
    kind: MetadataItemKind,
    candidates: &mut Vec<MetadataSidecarCandidate>,
    video_names: &[String],
) -> Result<Option<MetadataSidecarCandidate>, MetadataWorkError> {
    if kind == MetadataItemKind::Episode {
        let video_stems = video_names
            .iter()
            .filter_map(|name| Path::new(name).file_stem()?.to_str())
            .map(str::to_ascii_lowercase)
            .collect::<HashSet<_>>();
        candidates.retain(|candidate| {
            !["movie.nfo", "tvshow.nfo", "season.nfo"]
                .iter()
                .any(|name| candidate.name.eq_ignore_ascii_case(name))
                && Path::new(&candidate.name)
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .is_some_and(|stem| video_stems.contains(&stem.to_ascii_lowercase()))
        });
    }
    let conventional = match kind {
        MetadataItemKind::Movie => Some("movie.nfo"),
        MetadataItemKind::Series => Some("tvshow.nfo"),
        MetadataItemKind::Season => Some("season.nfo"),
        MetadataItemKind::Audio | MetadataItemKind::Episode => None,
    };
    let selected = if let Some(conventional) = conventional {
        let matches = candidates
            .iter()
            .enumerate()
            .filter(|(_, candidate)| candidate.name.eq_ignore_ascii_case(conventional))
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        match matches.as_slice() {
            [] if candidates.len() <= 1 => candidates.pop(),
            [index] => Some(candidates.swap_remove(*index)),
            _ => return Err(MetadataWorkError::AmbiguousSidecars),
        }
    } else {
        match candidates.len() {
            0 => None,
            1 => candidates.pop(),
            _ => return Err(MetadataWorkError::AmbiguousSidecars),
        }
    };
    if selected
        .as_ref()
        .is_some_and(|candidate| candidate.size == 0 || candidate.size > MAX_NFO_BYTES)
    {
        return Err(MetadataWorkError::InvalidSidecarSize);
    }
    Ok(selected)
}

fn parse_kind(value: &str) -> Result<MetadataItemKind, MetadataWorkError> {
    match value {
        "Audio" => Ok(MetadataItemKind::Audio),
        "Movie" => Ok(MetadataItemKind::Movie),
        "Series" => Ok(MetadataItemKind::Series),
        "Season" => Ok(MetadataItemKind::Season),
        "Episode" => Ok(MetadataItemKind::Episode),
        _ => Err(MetadataWorkError::InvalidStoredMetadata),
    }
}
