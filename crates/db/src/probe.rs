use std::collections::{HashMap, HashSet};

use chrono::Utc;
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbErr, QueryResult, TransactionTrait,
    sea_query::{Alias, Expr, OnConflict, Order, Query, SelectStatement},
};
use serde_json::json;
use thiserror::Error;
use tjxy_common::{
    CatalogItemId, MediaLocationId, MediaSourceId, PublicationId, StorageObjectRecordId,
    StorageRootId,
};
use uuid::Uuid;

use crate::{
    catalog_publication::{CatalogPublicationError, advance_generation, insert_change_event},
    natural_key,
    source_publication::effective_source_publication_visible,
    work_job::{
        ClaimedWorkJob, WorkJobRepository, WorkJobRepositoryError, WorkJobResult, WorkScope,
        WorkTaskKind, fence_live_claim,
    },
};

const MAX_STREAMS: usize = 256;
const MAX_IDENTITY_CHARS: usize = 2048;
const MAX_METADATA_CHARS: usize = 255;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProbeCandidate {
    source_id: MediaSourceId,
    item_id: CatalogItemId,
    publication_id: PublicationId,
    location_id: MediaLocationId,
    storage_object_id: StorageObjectRecordId,
    storage_root_id: StorageRootId,
    storage_account_id: Uuid,
    provider: String,
    provider_object_id: String,
    size: u64,
    location_revision: String,
    remote_revision: Option<String>,
    locator_kind: String,
}

impl ProbeCandidate {
    #[must_use]
    pub const fn source_id(&self) -> MediaSourceId {
        self.source_id
    }

    #[must_use]
    pub const fn item_id(&self) -> CatalogItemId {
        self.item_id
    }

    #[must_use]
    pub const fn location_id(&self) -> MediaLocationId {
        self.location_id
    }

    #[must_use]
    pub const fn storage_object_id(&self) -> StorageObjectRecordId {
        self.storage_object_id
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
    pub fn provider_object_id(&self) -> &str {
        &self.provider_object_id
    }

    #[must_use]
    pub const fn size(&self) -> u64 {
        self.size
    }

    #[must_use]
    pub fn location_revision(&self) -> &str {
        &self.location_revision
    }

    #[must_use]
    pub fn remote_revision(&self) -> Option<&str> {
        self.remote_revision.as_deref()
    }

    #[must_use]
    pub fn locator_kind(&self) -> &str {
        &self.locator_kind
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProbedStream {
    identity: String,
    stream_type: String,
    container_stream_index: i32,
    codec: Option<String>,
    language: Option<String>,
    width: Option<i32>,
    height: Option<i32>,
    channels: Option<i32>,
    profile: Option<String>,
    level: Option<i32>,
    is_default: bool,
    is_forced: bool,
}

impl ProbedStream {
    /// Creates one validated embedded media stream.
    ///
    /// # Errors
    ///
    /// Returns [`ProbeRepositoryError::InvalidResult`] when identity, type, indexes, or metadata
    /// cannot be persisted safely.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        identity: impl Into<String>,
        stream_type: impl Into<String>,
        container_stream_index: i32,
        codec: Option<String>,
        language: Option<String>,
        width: Option<i32>,
        height: Option<i32>,
        channels: Option<i32>,
        is_default: bool,
        is_forced: bool,
    ) -> Result<Self, ProbeRepositoryError> {
        let stream = Self {
            identity: identity.into(),
            stream_type: stream_type.into(),
            container_stream_index,
            codec,
            language,
            width,
            height,
            channels,
            profile: None,
            level: None,
            is_default,
            is_forced,
        };
        validate_stream(&stream)?;
        Ok(stream)
    }

    #[must_use]
    pub fn stream_type(&self) -> &str {
        &self.stream_type
    }

    #[must_use]
    pub fn codec(&self) -> Option<&str> {
        self.codec.as_deref()
    }

    /// Adds normalized video profile and level metadata.
    ///
    /// # Errors
    ///
    /// Returns [`ProbeRepositoryError::InvalidResult`] for invalid or non-video metadata.
    pub fn with_video_compatibility(
        mut self,
        profile: Option<String>,
        level: Option<i32>,
    ) -> Result<Self, ProbeRepositoryError> {
        self.profile = profile;
        self.level = level;
        validate_stream(&self)?;
        Ok(self)
    }

    #[must_use]
    pub fn profile(&self) -> Option<&str> {
        self.profile.as_deref()
    }

    #[must_use]
    pub const fn level(&self) -> Option<i32> {
        self.level
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProbeResult {
    container: String,
    video_codec: Option<String>,
    resolution: Option<String>,
    bitrate: Option<i64>,
    runtime_ticks: Option<i64>,
    streams: Vec<ProbedStream>,
}

impl ProbeResult {
    /// Creates one validated media Probe result.
    ///
    /// # Errors
    ///
    /// Returns [`ProbeRepositoryError::InvalidResult`] when the container or stream set is
    /// invalid.
    pub fn new(
        container: impl Into<String>,
        streams: Vec<ProbedStream>,
    ) -> Result<Self, ProbeRepositoryError> {
        let result = Self {
            container: container.into(),
            video_codec: None,
            resolution: None,
            bitrate: None,
            runtime_ticks: None,
            streams,
        };
        validate_result(&result)?;
        Ok(result)
    }

    #[must_use]
    pub fn with_video(mut self, codec: Option<String>, resolution: Option<String>) -> Self {
        self.video_codec = codec;
        self.resolution = resolution;
        self
    }

    #[must_use]
    pub const fn with_timing(mut self, bitrate: Option<i64>, runtime_ticks: Option<i64>) -> Self {
        self.bitrate = bitrate;
        self.runtime_ticks = runtime_ticks;
        self
    }

    #[must_use]
    pub fn container(&self) -> &str {
        &self.container
    }

    #[must_use]
    pub fn streams(&self) -> &[ProbedStream] {
        &self.streams
    }
}

pub struct ProbeRepository<'connection> {
    database: &'connection DatabaseConnection,
}

impl<'connection> ProbeRepository<'connection> {
    #[must_use]
    pub const fn new(database: &'connection DatabaseConnection) -> Self {
        Self { database }
    }

    /// Loads one authorized active location snapshot for a live Probe claim.
    ///
    /// # Errors
    ///
    /// Returns [`ProbeRepositoryError`] for invalid work or stored data.
    pub async fn candidate(
        &self,
        claimed: &ClaimedWorkJob,
    ) -> Result<Option<ProbeCandidate>, ProbeRepositoryError> {
        validate_claim(claimed)?;
        load_candidate(self.database, claimed).await
    }

    /// Atomically commits Probe metadata, stable stream indexes, generation, and `WorkJob` result.
    ///
    /// # Errors
    ///
    /// Returns [`ProbeRepositoryError`] and rolls back when lease, revision, active membership,
    /// location snapshot, result validation, or persistence fails.
    pub async fn commit_success(
        &self,
        claimed: &ClaimedWorkJob,
        snapshot: &ProbeCandidate,
        result: &ProbeResult,
    ) -> Result<i64, ProbeRepositoryError> {
        self.commit_success_with_location_revision(
            claimed,
            snapshot,
            result,
            snapshot.location_revision(),
        )
        .await
    }

    /// Commits Probe output while recording the revision of an indirectly resolved media target.
    ///
    /// # Errors
    ///
    /// Returns [`ProbeRepositoryError`] for invalid revisions, stale snapshots, or persistence
    /// failures.
    pub async fn commit_success_with_location_revision(
        &self,
        claimed: &ClaimedWorkJob,
        snapshot: &ProbeCandidate,
        result: &ProbeResult,
        probe_location_revision: &str,
    ) -> Result<i64, ProbeRepositoryError> {
        validate_claim(claimed)?;
        validate_result(result)?;
        if probe_location_revision.trim().is_empty()
            || probe_location_revision.chars().count() > MAX_IDENTITY_CHARS
        {
            return Err(ProbeRepositoryError::InvalidResult);
        }
        let transaction = self.database.begin().await?;
        let outcome = commit_success(
            &transaction,
            self.database,
            claimed,
            snapshot,
            result,
            probe_location_revision,
        )
        .await;
        finish(transaction, outcome).await
    }

    /// Atomically records a deterministic Probe failure and fails its `WorkJob`.
    ///
    /// # Errors
    ///
    /// Returns [`ProbeRepositoryError`] and rolls back on stale lease, revision, or snapshot.
    pub async fn commit_failure(
        &self,
        claimed: &ClaimedWorkJob,
        snapshot: &ProbeCandidate,
        error: &str,
    ) -> Result<i64, ProbeRepositoryError> {
        validate_claim(claimed)?;
        if error.trim().is_empty() || error.chars().count() > 4096 {
            return Err(ProbeRepositoryError::InvalidResult);
        }
        let transaction = self.database.begin().await?;
        let outcome = commit_failure(&transaction, self.database, claimed, snapshot, error).await;
        finish(transaction, outcome).await
    }
}

#[derive(Debug, Error)]
pub enum ProbeRepositoryError {
    #[error("work claim is not a media Probe job")]
    InvalidWork,
    #[error("Probe source revision is stale")]
    StaleRevision,
    #[error("Probe location snapshot changed")]
    StaleSnapshot,
    #[error("Probe result is invalid")]
    InvalidResult,
    #[error("delivery index space is exhausted")]
    IndexExhausted,
    #[error("catalog publication failed: {0}")]
    Publication(#[from] CatalogPublicationError),
    #[error("work job failed: {0}")]
    Work(#[from] WorkJobRepositoryError),
    #[error("database operation failed: {0}")]
    Database(#[from] DbErr),
    #[error("rollback failed after {original}: {rollback}")]
    RollbackFailed { original: String, rollback: DbErr },
}

fn validate_claim(claimed: &ClaimedWorkJob) -> Result<MediaSourceId, ProbeRepositoryError> {
    if claimed.job().task_kind() != WorkTaskKind::ProbeMedia {
        return Err(ProbeRepositoryError::InvalidWork);
    }
    match claimed.job().scope() {
        WorkScope::MediaSource(source_id) => Ok(source_id),
        _ => Err(ProbeRepositoryError::InvalidWork),
    }
}

async fn load_candidate(
    connection: &impl ConnectionTrait,
    claimed: &ClaimedWorkJob,
) -> Result<Option<ProbeCandidate>, ProbeRepositoryError> {
    let source_id = validate_claim(claimed)?;
    let root_affinity = claimed.job().storage_root_affinity();
    let query = candidate_query(source_id, claimed.job().expected_revision(), root_affinity);
    let backend = connection.get_database_backend();
    let mut selected = None;
    for row in connection.query_all(backend.build(&query)).await? {
        if let Some(candidate) = candidate_from_row(&row, source_id, connection).await? {
            if root_affinity.is_none()
                && selected.as_ref().is_some_and(|selected: &ProbeCandidate| {
                    selected.storage_root_id != candidate.storage_root_id
                })
            {
                return Ok(None);
            }
            if selected.is_none() {
                selected = Some(candidate);
            }
        }
    }
    Ok(selected)
}

struct ProbeTables {
    canonical: Alias,
    projection: Alias,
    publication: Alias,
    location: Alias,
    canonical_location: Alias,
    object: Alias,
    account: Alias,
    item: Alias,
    structure_owner: Alias,
    membership: Alias,
    library: Alias,
    root_relation: Alias,
    library_root: Alias,
}

impl ProbeTables {
    fn new() -> Self {
        Self {
            canonical: Alias::new("probe_source"),
            projection: Alias::new("probe_source_projection"),
            publication: Alias::new("probe_publication"),
            location: Alias::new("probe_location_projection"),
            canonical_location: Alias::new("probe_location"),
            object: Alias::new("probe_object"),
            account: Alias::new("probe_account"),
            item: Alias::new("probe_item"),
            structure_owner: Alias::new("probe_structure_owner"),
            membership: Alias::new("probe_membership"),
            library: Alias::new("probe_library"),
            root_relation: Alias::new("probe_root_relation"),
            library_root: Alias::new("probe_library_root"),
        }
    }
}

fn candidate_query(
    source_id: MediaSourceId,
    expected_revision: i64,
    root_affinity: Option<StorageRootId>,
) -> SelectStatement {
    let tables = ProbeTables::new();
    let mut query = Query::select();
    query.distinct();
    select_candidate_columns(&mut query, &tables);
    join_candidate_tables(&mut query, &tables);
    filter_candidate(
        &mut query,
        &tables,
        source_id,
        expected_revision,
        root_affinity,
    );
    query
}

fn select_candidate_columns(query: &mut SelectStatement, tables: &ProbeTables) {
    query
        .expr_as(
            Expr::col((tables.canonical.clone(), Alias::new("catalog_item_id"))),
            Alias::new("catalog_item_id"),
        )
        .expr_as(
            Expr::col((tables.projection.clone(), Alias::new("publication_id"))),
            Alias::new("publication_id"),
        )
        .expr_as(
            Expr::col((tables.location.clone(), Alias::new("media_location_id"))),
            Alias::new("media_location_id"),
        )
        .expr_as(
            Expr::col((tables.object.clone(), Alias::new("id"))),
            Alias::new("storage_object_id"),
        )
        .expr_as(
            Expr::col((tables.account.clone(), Alias::new("id"))),
            Alias::new("storage_account_id"),
        )
        .expr_as(
            Expr::col((tables.account.clone(), Alias::new("provider"))),
            Alias::new("provider"),
        )
        .expr_as(
            Expr::col((tables.object.clone(), Alias::new("provider_object_id"))),
            Alias::new("provider_object_id"),
        )
        .expr_as(
            Expr::col((tables.object.clone(), Alias::new("size"))),
            Alias::new("size"),
        )
        .expr_as(
            Expr::col((tables.object.clone(), Alias::new("remote_revision"))),
            Alias::new("remote_revision"),
        )
        .expr_as(
            Expr::col((tables.object.clone(), Alias::new("observed_sync_revision"))),
            Alias::new("observed_sync_revision"),
        )
        .expr_as(
            Expr::col((tables.canonical.clone(), Alias::new("locator_kind"))),
            Alias::new("locator_kind"),
        )
        .expr_as(
            Expr::col((tables.root_relation.clone(), Alias::new("storage_root_id"))),
            Alias::new("storage_root_id"),
        )
        .expr_as(
            Expr::col((
                tables.publication.clone(),
                Alias::new("activated_generation"),
            )),
            Alias::new("activated_generation"),
        )
        .expr_as(
            Expr::col((tables.location.clone(), Alias::new("priority"))),
            Alias::new("location_priority"),
        );
}

#[allow(clippy::too_many_lines)] // The exact publication, Library, root, and object joins form one candidate graph.
fn join_candidate_tables(query: &mut SelectStatement, tables: &ProbeTables) {
    query
        .from_as(Alias::new("media_sources"), tables.canonical.clone())
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("publication_media_sources"),
            tables.projection.clone(),
            Expr::col((tables.projection.clone(), Alias::new("media_source_id")))
                .equals((tables.canonical.clone(), Alias::new("id"))),
        )
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("catalog_publications"),
            tables.publication.clone(),
            Expr::col((tables.publication.clone(), Alias::new("id")))
                .equals((tables.projection.clone(), Alias::new("publication_id"))),
        )
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("publication_media_locations"),
            tables.location.clone(),
            sea_orm::sea_query::Cond::all()
                .add(
                    Expr::col((tables.location.clone(), Alias::new("publication_id")))
                        .equals((tables.projection.clone(), Alias::new("publication_id"))),
                )
                .add(
                    Expr::col((tables.location.clone(), Alias::new("media_source_id")))
                        .equals((tables.projection.clone(), Alias::new("media_source_id"))),
                ),
        )
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("media_locations"),
            tables.canonical_location.clone(),
            Expr::col((tables.canonical_location.clone(), Alias::new("id")))
                .equals((tables.location.clone(), Alias::new("media_location_id"))),
        )
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("storage_objects"),
            tables.object.clone(),
            Expr::col((tables.object.clone(), Alias::new("id")))
                .equals((tables.location.clone(), Alias::new("storage_object_id"))),
        )
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("storage_accounts"),
            tables.account.clone(),
            Expr::col((tables.account.clone(), Alias::new("id")))
                .equals((tables.object.clone(), Alias::new("storage_account_id"))),
        )
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("catalog_items"),
            tables.item.clone(),
            Expr::col((tables.item.clone(), Alias::new("id")))
                .equals((tables.canonical.clone(), Alias::new("catalog_item_id"))),
        )
        .join_as(
            sea_orm::sea_query::JoinType::LeftJoin,
            Alias::new("catalog_items"),
            tables.structure_owner.clone(),
            Expr::col((tables.structure_owner.clone(), Alias::new("id")))
                .equals((tables.item.clone(), Alias::new("structure_owner_item_id"))),
        )
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("library_catalog_items"),
            tables.membership.clone(),
            sea_orm::sea_query::Cond::any()
                .add(
                    Expr::col((tables.membership.clone(), Alias::new("catalog_item_id")))
                        .equals((tables.item.clone(), Alias::new("id"))),
                )
                .add(
                    Expr::col((tables.membership.clone(), Alias::new("catalog_item_id")))
                        .equals((tables.structure_owner.clone(), Alias::new("id"))),
                ),
        )
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("libraries"),
            tables.library.clone(),
            Expr::col((tables.library.clone(), Alias::new("id")))
                .equals((tables.membership.clone(), Alias::new("library_id"))),
        )
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("storage_root_objects"),
            tables.root_relation.clone(),
            Expr::col((
                tables.root_relation.clone(),
                Alias::new("storage_object_id"),
            ))
            .equals((tables.object.clone(), Alias::new("id"))),
        )
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("library_storage_roots"),
            tables.library_root.clone(),
            sea_orm::sea_query::Cond::all()
                .add(
                    Expr::col((tables.library_root.clone(), Alias::new("storage_root_id")))
                        .equals((tables.root_relation.clone(), Alias::new("storage_root_id"))),
                )
                .add(
                    Expr::col((tables.library_root.clone(), Alias::new("library_id")))
                        .equals((tables.membership.clone(), Alias::new("library_id"))),
                ),
        );
}

fn filter_candidate(
    query: &mut SelectStatement,
    tables: &ProbeTables,
    source_id: MediaSourceId,
    expected_revision: i64,
    root_affinity: Option<StorageRootId>,
) {
    query
        .and_where(Expr::col((tables.canonical.clone(), Alias::new("id"))).eq(source_id.as_uuid()))
        .and_where(
            Expr::col((tables.canonical.clone(), Alias::new("probe_revision")))
                .eq(expected_revision),
        )
        .and_where(Expr::col((tables.publication.clone(), Alias::new("state"))).eq("Active"))
        .and_where(
            Expr::col((
                tables.canonical_location.clone(),
                Alias::new("availability_state"),
            ))
            .eq("Available"),
        )
        .and_where(Expr::col((tables.object.clone(), Alias::new("presence_state"))).eq("Present"))
        .and_where(
            Expr::col((tables.root_relation.clone(), Alias::new("presence_state"))).eq("Present"),
        )
        .and_where(
            Expr::col((tables.account.clone(), Alias::new("status"))).is_in(["Active", "Ready"]),
        )
        .and_where(Expr::col((tables.item.clone(), Alias::new("is_present"))).eq(true))
        .and_where(Expr::col((tables.library.clone(), Alias::new("is_enabled"))).eq(true))
        .order_by(
            (
                tables.publication.clone(),
                Alias::new("activated_generation"),
            ),
            Order::Desc,
        )
        .order_by(
            (tables.location.clone(), Alias::new("priority")),
            Order::Desc,
        )
        .order_by(
            (tables.location.clone(), Alias::new("media_location_id")),
            Order::Asc,
        )
        .order_by(
            (tables.root_relation.clone(), Alias::new("storage_root_id")),
            Order::Asc,
        )
        .limit(2);
    if let Some(root_id) = root_affinity {
        query.and_where(
            Expr::col((tables.root_relation.clone(), Alias::new("storage_root_id")))
                .eq(root_id.as_uuid()),
        );
    }
}

async fn candidate_from_row(
    row: &QueryResult,
    source_id: MediaSourceId,
    connection: &impl ConnectionTrait,
) -> Result<Option<ProbeCandidate>, ProbeRepositoryError> {
    let item_id = CatalogItemId::from_uuid(row.try_get("", "catalog_item_id")?);
    let publication_id = row.try_get::<Uuid>("", "publication_id")?;
    if !effective_publication_exists(connection, item_id, publication_id).await? {
        return Ok(None);
    }
    let size = row
        .try_get::<Option<i64>>("", "size")?
        .and_then(|size| u64::try_from(size).ok())
        .ok_or(ProbeRepositoryError::InvalidResult)?;
    let observed: i64 = row.try_get("", "observed_sync_revision")?;
    let remote_revision: Option<String> = row.try_get("", "remote_revision")?;
    let storage_root_id = StorageRootId::from_uuid(row.try_get("", "storage_root_id")?);
    let storage_object_id = StorageObjectRecordId::from_uuid(row.try_get("", "storage_object_id")?);
    if !crate::storage_path_authorization::storage_path_is_authorized(
        connection,
        storage_root_id,
        storage_object_id,
        crate::storage_path_authorization::StoragePathAvailability::Present,
    )
    .await?
    {
        return Ok(None);
    }
    let location_revision = remote_revision
        .clone()
        .unwrap_or_else(|| format!("sync:{observed}:size:{size}"));
    Ok(Some(ProbeCandidate {
        source_id,
        item_id,
        publication_id: PublicationId::from_uuid(publication_id),
        location_id: MediaLocationId::from_uuid(row.try_get("", "media_location_id")?),
        storage_object_id,
        storage_root_id,
        storage_account_id: row.try_get("", "storage_account_id")?,
        provider: row.try_get("", "provider")?,
        provider_object_id: row.try_get("", "provider_object_id")?,
        size,
        location_revision,
        remote_revision,
        locator_kind: row.try_get("", "locator_kind")?,
    }))
}

async fn effective_publication_exists(
    connection: &impl ConnectionTrait,
    item_id: CatalogItemId,
    publication_id: Uuid,
) -> Result<bool, ProbeRepositoryError> {
    let query = Query::select()
        .expr(Expr::val(1_i32))
        .and_where(Expr::exists(effective_source_publication_visible(
            item_id,
            publication_id,
        )))
        .to_owned();
    let backend = connection.get_database_backend();
    connection
        .query_one(backend.build(&query))
        .await
        .map(|row| row.is_some())
        .map_err(Into::into)
}

async fn commit_success(
    transaction: &DatabaseTransaction,
    database: &DatabaseConnection,
    claimed: &ClaimedWorkJob,
    snapshot: &ProbeCandidate,
    result: &ProbeResult,
    probe_location_revision: &str,
) -> Result<i64, ProbeRepositoryError> {
    fence_live_claim(transaction, claimed, Utc::now()).await?;
    let current = load_candidate(transaction, claimed)
        .await?
        .ok_or(ProbeRepositoryError::StaleSnapshot)?;
    if &current != snapshot {
        return Err(ProbeRepositoryError::StaleSnapshot);
    }
    let assignments =
        assign_stream_indexes(transaction, snapshot.source_id, &result.streams).await?;
    replace_streams(
        transaction,
        snapshot.source_id,
        &result.streams,
        &assignments,
    )
    .await?;
    assign_external_subtitles(transaction, snapshot.source_id, assignments).await?;
    update_source(
        transaction,
        claimed,
        snapshot,
        result,
        probe_location_revision,
    )
    .await?;
    let generation = advance_generation(transaction).await?;
    insert_change_event(
        transaction,
        snapshot.item_id,
        snapshot.publication_id,
        generation,
        "ProbeCompleted",
        Utc::now(),
    )
    .await?;
    WorkJobRepository::new(database)
        .complete_in_transaction(
            transaction,
            claimed,
            WorkJobResult::success(
                json!({"streams": result.streams.len(), "catalog_generation": generation}),
                Vec::new(),
            ),
        )
        .await?;
    Ok(generation)
}

async fn commit_failure(
    transaction: &DatabaseTransaction,
    database: &DatabaseConnection,
    claimed: &ClaimedWorkJob,
    snapshot: &ProbeCandidate,
    error: &str,
) -> Result<i64, ProbeRepositoryError> {
    fence_live_claim(transaction, claimed, Utc::now()).await?;
    let current = load_candidate(transaction, claimed)
        .await?
        .ok_or(ProbeRepositoryError::StaleSnapshot)?;
    if &current != snapshot {
        return Err(ProbeRepositoryError::StaleSnapshot);
    }
    let update = Query::update()
        .table(Alias::new("media_sources"))
        .value(Alias::new("probe_state"), "ProbeFailed")
        .value(Alias::new("last_probe_error"), error)
        .and_where(Expr::col(Alias::new("id")).eq(snapshot.source_id.as_uuid()))
        .and_where(Expr::col(Alias::new("probe_revision")).eq(claimed.job().expected_revision()))
        .to_owned();
    let backend = transaction.get_database_backend();
    if transaction
        .execute(backend.build(&update))
        .await?
        .rows_affected()
        != 1
    {
        return Err(ProbeRepositoryError::StaleRevision);
    }
    let generation = advance_generation(transaction).await?;
    insert_change_event(
        transaction,
        snapshot.item_id,
        snapshot.publication_id,
        generation,
        "ProbeFailed",
        Utc::now(),
    )
    .await?;
    WorkJobRepository::new(database)
        .fail_terminal_in_transaction(transaction, claimed, error)
        .await?;
    Ok(generation)
}

async fn assign_stream_indexes(
    transaction: &DatabaseTransaction,
    source_id: MediaSourceId,
    streams: &[ProbedStream],
) -> Result<HashMap<String, i32>, ProbeRepositoryError> {
    let mut historical = load_index_map(transaction, source_id).await?;
    let mut occupied = historical.values().copied().collect::<HashSet<_>>();
    occupied.extend(load_reserved_subtitle_indexes(transaction, source_id).await?);
    mark_indexes_absent(transaction, source_id).await?;
    let mut ordered = streams.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|stream| stream.container_stream_index);
    let mut assigned = HashMap::new();
    for stream in ordered {
        let identity = format!("embedded:{}", stream.identity);
        let delivery = historical.remove(&identity).map_or_else(
            || allocate_index(&mut occupied, Some(stream.container_stream_index)),
            Ok,
        )?;
        upsert_index(
            transaction,
            source_id,
            &identity,
            delivery,
            Some(stream.container_stream_index),
            &stream.stream_type,
        )
        .await?;
        assigned.insert(stream.identity.clone(), delivery);
    }
    Ok(assigned)
}

async fn assign_external_subtitles(
    transaction: &DatabaseTransaction,
    source_id: MediaSourceId,
    mut assigned: HashMap<String, i32>,
) -> Result<(), ProbeRepositoryError> {
    let mut historical = load_index_map(transaction, source_id).await?;
    let mut occupied = historical.values().copied().collect::<HashSet<_>>();
    occupied.extend(assigned.drain().map(|(_, index)| index));
    let query = Query::select()
        .columns([Alias::new("id"), Alias::new("delivery_index")])
        .from(Alias::new("subtitles"))
        .and_where(Expr::col(Alias::new("media_source_id")).eq(source_id.as_uuid()))
        .order_by(Alias::new("id"), Order::Asc)
        .to_owned();
    let backend = transaction.get_database_backend();
    for row in transaction.query_all(backend.build(&query)).await? {
        let subtitle_id: Uuid = row.try_get("", "id")?;
        let existing_delivery: Option<i32> = row.try_get("", "delivery_index")?;
        let identity = format!("external:{subtitle_id}");
        let delivery = if let Some(delivery) = historical.remove(&identity).or(existing_delivery) {
            occupied.insert(delivery);
            delivery
        } else {
            allocate_index(&mut occupied, None)?
        };
        upsert_index(
            transaction,
            source_id,
            &identity,
            delivery,
            None,
            "Subtitle",
        )
        .await?;
        let update = Query::update()
            .table(Alias::new("subtitles"))
            .value(Alias::new("delivery_index"), delivery)
            .and_where(Expr::col(Alias::new("id")).eq(subtitle_id))
            .and_where(Expr::col(Alias::new("media_source_id")).eq(source_id.as_uuid()))
            .to_owned();
        transaction.execute(backend.build(&update)).await?;
    }
    Ok(())
}

async fn load_reserved_subtitle_indexes(
    transaction: &DatabaseTransaction,
    source_id: MediaSourceId,
) -> Result<Vec<i32>, ProbeRepositoryError> {
    let query = Query::select()
        .column(Alias::new("delivery_index"))
        .from(Alias::new("subtitles"))
        .and_where(Expr::col(Alias::new("media_source_id")).eq(source_id.as_uuid()))
        .and_where(Expr::col(Alias::new("delivery_index")).is_not_null())
        .to_owned();
    let backend = transaction.get_database_backend();
    transaction
        .query_all(backend.build(&query))
        .await?
        .iter()
        .map(|row| row.try_get("", "delivery_index").map_err(Into::into))
        .collect()
}

async fn load_index_map(
    transaction: &DatabaseTransaction,
    source_id: MediaSourceId,
) -> Result<HashMap<String, i32>, ProbeRepositoryError> {
    let query = Query::select()
        .columns([Alias::new("stream_identity"), Alias::new("delivery_index")])
        .from(Alias::new("media_stream_index_map"))
        .and_where(Expr::col(Alias::new("media_source_id")).eq(source_id.as_uuid()))
        .to_owned();
    let backend = transaction.get_database_backend();
    transaction
        .query_all(backend.build(&query))
        .await?
        .iter()
        .map(|row| {
            Ok((
                row.try_get("", "stream_identity")?,
                row.try_get("", "delivery_index")?,
            ))
        })
        .collect()
}

async fn mark_indexes_absent(
    transaction: &DatabaseTransaction,
    source_id: MediaSourceId,
) -> Result<(), ProbeRepositoryError> {
    let update = Query::update()
        .table(Alias::new("media_stream_index_map"))
        .value(Alias::new("is_present"), false)
        .and_where(Expr::col(Alias::new("media_source_id")).eq(source_id.as_uuid()))
        .to_owned();
    let backend = transaction.get_database_backend();
    transaction.execute(backend.build(&update)).await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn upsert_index(
    transaction: &DatabaseTransaction,
    source_id: MediaSourceId,
    identity: &str,
    delivery_index: i32,
    container_stream_index: Option<i32>,
    stream_type: &str,
) -> Result<(), ProbeRepositoryError> {
    let backend = transaction.get_database_backend();
    let mysql = backend == sea_orm::DbBackend::MySql;
    let identity_key = natural_key::hash(&[identity]);
    let conflict_column = if mysql {
        "stream_identity_key"
    } else {
        "stream_identity"
    };
    let mut insert = Query::insert();
    insert
        .into_table(Alias::new("media_stream_index_map"))
        .columns([
            Alias::new("id"),
            Alias::new("media_source_id"),
            Alias::new("stream_identity"),
            Alias::new("delivery_index"),
            Alias::new("container_stream_index"),
            Alias::new("stream_type"),
            Alias::new("is_present"),
        ]);
    if mysql {
        insert.columns([Alias::new("stream_identity_key")]);
    }
    let values = [
        Uuid::new_v4().into(),
        source_id.as_uuid().into(),
        identity.into(),
        delivery_index.into(),
        container_stream_index.into(),
        stream_type.into(),
        true.into(),
    ];
    if mysql {
        insert.values_panic(values.into_iter().chain([identity_key.into()]));
    } else {
        insert.values_panic(values);
    }
    let insert = insert
        .on_conflict(
            OnConflict::columns([Alias::new("media_source_id"), Alias::new(conflict_column)])
                .update_columns([
                    Alias::new("container_stream_index"),
                    Alias::new("stream_type"),
                    Alias::new("is_present"),
                ])
                .to_owned(),
        )
        .to_owned();
    transaction.execute(backend.build(&insert)).await?;
    Ok(())
}

async fn replace_streams(
    transaction: &DatabaseTransaction,
    source_id: MediaSourceId,
    streams: &[ProbedStream],
    assignments: &HashMap<String, i32>,
) -> Result<(), ProbeRepositoryError> {
    let backend = transaction.get_database_backend();
    let delete = Query::delete()
        .from_table(Alias::new("media_streams"))
        .and_where(Expr::col(Alias::new("media_source_id")).eq(source_id.as_uuid()))
        .to_owned();
    transaction.execute(backend.build(&delete)).await?;
    for stream in streams {
        let delivery = *assignments
            .get(&stream.identity)
            .ok_or(ProbeRepositoryError::InvalidResult)?;
        let mut insert = Query::insert();
        insert.into_table(Alias::new("media_streams")).columns([
            Alias::new("id"),
            Alias::new("media_source_id"),
            Alias::new("stream_type"),
            Alias::new("stream_index"),
            Alias::new("stream_identity"),
            Alias::new("delivery_index"),
            Alias::new("container_stream_index"),
            Alias::new("codec"),
            Alias::new("language"),
            Alias::new("width"),
            Alias::new("height"),
            Alias::new("channels"),
            Alias::new("profile"),
            Alias::new("level"),
            Alias::new("is_default"),
            Alias::new("is_forced"),
            Alias::new("is_external"),
            Alias::new("is_text"),
        ]);
        let mysql = backend == sea_orm::DbBackend::MySql;
        if mysql {
            insert.columns([Alias::new("stream_identity_key")]);
        }
        let values = [
            Uuid::new_v4().into(),
            source_id.as_uuid().into(),
            stream.stream_type.as_str().into(),
            delivery.into(),
            stream.identity.as_str().into(),
            delivery.into(),
            stream.container_stream_index.into(),
            stream.codec.as_deref().into(),
            stream.language.as_deref().into(),
            stream.width.into(),
            stream.height.into(),
            stream.channels.into(),
            stream.profile.as_deref().into(),
            stream.level.into(),
            stream.is_default.into(),
            stream.is_forced.into(),
            false.into(),
            (stream.stream_type == "Subtitle").into(),
        ];
        if mysql {
            insert.values_panic(
                values
                    .into_iter()
                    .chain([natural_key::hash(&[stream.identity.as_str()]).into()]),
            );
        } else {
            insert.values_panic(values);
        }
        let insert = insert.clone();
        transaction.execute(backend.build(&insert)).await?;
    }
    Ok(())
}

async fn update_source(
    transaction: &DatabaseTransaction,
    claimed: &ClaimedWorkJob,
    snapshot: &ProbeCandidate,
    result: &ProbeResult,
    probe_location_revision: &str,
) -> Result<(), ProbeRepositoryError> {
    let next_revision = claimed
        .job()
        .expected_revision()
        .checked_add(1)
        .ok_or(ProbeRepositoryError::StaleRevision)?;
    let update = Query::update()
        .table(Alias::new("media_sources"))
        .value(Alias::new("container"), result.container.as_str())
        .value(Alias::new("video_codec"), result.video_codec.as_deref())
        .value(Alias::new("resolution"), result.resolution.as_deref())
        .value(Alias::new("bitrate"), result.bitrate)
        .value(Alias::new("runtime_ticks"), result.runtime_ticks)
        .value(Alias::new("probe_state"), "Probed")
        .value(Alias::new("probe_revision"), next_revision)
        .value(
            Alias::new("probe_location_id"),
            snapshot.location_id.as_uuid(),
        )
        .value(
            Alias::new("probe_location_revision"),
            probe_location_revision,
        )
        .value(Alias::new("last_probe_error"), Option::<String>::None)
        .and_where(Expr::col(Alias::new("id")).eq(snapshot.source_id.as_uuid()))
        .and_where(Expr::col(Alias::new("probe_revision")).eq(claimed.job().expected_revision()))
        .to_owned();
    let backend = transaction.get_database_backend();
    if transaction
        .execute(backend.build(&update))
        .await?
        .rows_affected()
        != 1
    {
        return Err(ProbeRepositoryError::StaleRevision);
    }
    Ok(())
}

fn allocate_index(
    occupied: &mut HashSet<i32>,
    preferred: Option<i32>,
) -> Result<i32, ProbeRepositoryError> {
    if let Some(preferred) = preferred.filter(|index| *index >= 0 && !occupied.contains(index)) {
        occupied.insert(preferred);
        return Ok(preferred);
    }
    let index = (0..=i32::MAX)
        .find(|index| !occupied.contains(index))
        .ok_or(ProbeRepositoryError::IndexExhausted)?;
    occupied.insert(index);
    Ok(index)
}

fn validate_result(result: &ProbeResult) -> Result<(), ProbeRepositoryError> {
    if !valid_text(&result.container, 32)
        || result.streams.len() > MAX_STREAMS
        || !valid_optional_text(result.video_codec.as_deref(), MAX_METADATA_CHARS)
        || !valid_optional_text(result.resolution.as_deref(), MAX_METADATA_CHARS)
        || result.bitrate.is_some_and(|value| value < 0)
        || result.runtime_ticks.is_some_and(|value| value < 0)
    {
        return Err(ProbeRepositoryError::InvalidResult);
    }
    let mut identities = HashSet::new();
    for stream in &result.streams {
        validate_stream(stream)?;
        if !identities.insert(&stream.identity) {
            return Err(ProbeRepositoryError::InvalidResult);
        }
    }
    Ok(())
}

fn validate_stream(stream: &ProbedStream) -> Result<(), ProbeRepositoryError> {
    if !valid_text(&stream.identity, MAX_IDENTITY_CHARS)
        || !matches!(stream.stream_type.as_str(), "Video" | "Audio" | "Subtitle")
        || stream.container_stream_index < 0
        || !valid_optional_text(stream.codec.as_deref(), MAX_METADATA_CHARS)
        || !valid_optional_text(stream.language.as_deref(), 64)
        || !valid_optional_text(stream.profile.as_deref(), 128)
        || [stream.width, stream.height, stream.channels]
            .into_iter()
            .flatten()
            .any(|value| value < 0)
        || stream.level.is_some_and(|level| level < 0)
        || (stream.stream_type != "Video" && (stream.profile.is_some() || stream.level.is_some()))
    {
        return Err(ProbeRepositoryError::InvalidResult);
    }
    Ok(())
}

fn valid_text(value: &str, max: usize) -> bool {
    !value.is_empty() && value.chars().count() <= max && !value.chars().any(char::is_control)
}

fn valid_optional_text(value: Option<&str>, max: usize) -> bool {
    value.is_none_or(|value| valid_text(value, max))
}

async fn finish<T>(
    transaction: DatabaseTransaction,
    result: Result<T, ProbeRepositoryError>,
) -> Result<T, ProbeRepositoryError> {
    match result {
        Ok(value) => {
            transaction.commit().await?;
            Ok(value)
        }
        Err(original) => match transaction.rollback().await {
            Ok(()) => Err(original),
            Err(rollback) => Err(ProbeRepositoryError::RollbackFailed {
                original: original.to_string(),
                rollback,
            }),
        },
    }
}
