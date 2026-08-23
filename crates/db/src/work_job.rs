use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Duration, Timelike, Utc};
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbErr, QueryResult, TransactionTrait,
    sea_query::{Alias, Cond, Expr, JoinType, OnConflict, Order, Query, SelectStatement},
};
use serde_json::Value;
use thiserror::Error;
use tjxy_common::{
    CatalogItemId, LibraryId, LibraryRootBindingId, MediaSourceId, StorageObjectRecordId,
    StorageRootId, WorkJobId,
};
use tjxy_domain::{LocalMetadataAccessMode, MetadataSourceMode};
use uuid::Uuid;

const STATE_PENDING: &str = "Pending";
const STATE_RUNNING: &str = "Running";
const STATE_COMPLETED: &str = "Completed";
const STATE_FAILED: &str = "Failed";
const MAX_LEASE_OWNER_CHARS: usize = 128;
const MAX_STAGING_KEY_CHARS: usize = 512;
const MAX_ERROR_CHARS: usize = 4096;
pub const ADMIN_CANCELLED_ERROR: &str = "cancelled by administrator";
const MAX_OBSERVED_JOBS: u64 = 100;
const METADATA_RETRY_COOLDOWN: Duration = Duration::seconds(5);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WorkTaskKind {
    ScopedStorageSync,
    RecoverStorageCursor,
    ValidateStorageRoot,
    DiscoverTitles,
    ExpandItem,
    IndexMediaSources,
    ResolveMetadata,
    ProbeMedia,
    FullMediaScan,
    FullLibraryRootScan,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum MetadataRequirement {
    Basic,
    Full,
}

impl MetadataRequirement {
    #[must_use]
    pub const fn as_i32(self) -> i32 {
        match self {
            Self::Basic => 1,
            Self::Full => 2,
        }
    }

    pub(crate) fn from_database(value: i32) -> Result<Self, WorkJobRepositoryError> {
        match value {
            1 => Ok(Self::Basic),
            2 => Ok(Self::Full),
            _ => Err(WorkJobRepositoryError::InvalidStoredMetadataRequirement),
        }
    }
}

impl WorkTaskKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ScopedStorageSync => "ScopedStorageSync",
            Self::RecoverStorageCursor => "RecoverStorageCursor",
            Self::ValidateStorageRoot => "ValidateStorageRoot",
            Self::DiscoverTitles => "DiscoverTitles",
            Self::ExpandItem => "ExpandItem",
            Self::IndexMediaSources => "IndexMediaSources",
            Self::ResolveMetadata => "ResolveMetadata",
            Self::ProbeMedia => "ProbeMedia",
            Self::FullMediaScan => "FullMediaScan",
            Self::FullLibraryRootScan => "FullLibraryRootScan",
        }
    }

    fn from_database(value: &str) -> Result<Self, WorkJobRepositoryError> {
        match value {
            "ScopedStorageSync" => Ok(Self::ScopedStorageSync),
            "RecoverStorageCursor" => Ok(Self::RecoverStorageCursor),
            "ValidateStorageRoot" => Ok(Self::ValidateStorageRoot),
            "DiscoverTitles" => Ok(Self::DiscoverTitles),
            "ExpandItem" => Ok(Self::ExpandItem),
            "IndexMediaSources" => Ok(Self::IndexMediaSources),
            "ResolveMetadata" => Ok(Self::ResolveMetadata),
            "ProbeMedia" => Ok(Self::ProbeMedia),
            "FullMediaScan" => Ok(Self::FullMediaScan),
            "FullLibraryRootScan" => Ok(Self::FullLibraryRootScan),
            _ => Err(WorkJobRepositoryError::InvalidStoredTaskKind),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkScope {
    Library(LibraryId),
    LibraryRootBinding(LibraryRootBindingId),
    CatalogItem(CatalogItemId),
    MediaSource(MediaSourceId),
    StorageRoot(StorageRootId),
    StorageObject(StorageObjectRecordId),
}

impl WorkScope {
    #[must_use]
    pub const fn scope_type(self) -> &'static str {
        match self {
            Self::Library(_) => "Library",
            Self::LibraryRootBinding(_) => "LibraryRootBinding",
            Self::CatalogItem(_) => "CatalogItem",
            Self::MediaSource(_) => "MediaSource",
            Self::StorageRoot(_) => "StorageRoot",
            Self::StorageObject(_) => "StorageObject",
        }
    }

    #[must_use]
    pub const fn id(self) -> Uuid {
        match self {
            Self::Library(id) => id.as_uuid(),
            Self::LibraryRootBinding(id) => id.as_uuid(),
            Self::CatalogItem(id) => id.as_uuid(),
            Self::MediaSource(id) => id.as_uuid(),
            Self::StorageRoot(id) => id.as_uuid(),
            Self::StorageObject(id) => id.as_uuid(),
        }
    }

    fn from_database(scope_type: &str, id: Uuid) -> Result<Self, WorkJobRepositoryError> {
        match scope_type {
            "Library" => Ok(Self::Library(LibraryId::from_uuid(id))),
            "LibraryRootBinding" => Ok(Self::LibraryRootBinding(LibraryRootBindingId::from_uuid(
                id,
            ))),
            "CatalogItem" => Ok(Self::CatalogItem(CatalogItemId::from_uuid(id))),
            "MediaSource" => Ok(Self::MediaSource(MediaSourceId::from_uuid(id))),
            "StorageRoot" => Ok(Self::StorageRoot(StorageRootId::from_uuid(id))),
            "StorageObject" => Ok(Self::StorageObject(StorageObjectRecordId::from_uuid(id))),
            _ => Err(WorkJobRepositoryError::InvalidStoredScopeType),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkJobSpec {
    task_kind: WorkTaskKind,
    scope: WorkScope,
    expected_revision: i64,
    priority: i32,
    required_sync_job_id: Option<WorkJobId>,
    input_sync_revision: Option<i64>,
    metadata_requirement: Option<MetadataRequirement>,
    metadata_source_mode: Option<MetadataSourceMode>,
    local_metadata_access_mode: Option<LocalMetadataAccessMode>,
    storage_root_affinity: Option<StorageRootId>,
}

impl WorkJobSpec {
    /// Defines one durable natural-key job.
    ///
    /// # Errors
    ///
    /// Returns [`WorkJobRepositoryError::InvalidRevision`] for a negative revision.
    pub fn new(
        task_kind: WorkTaskKind,
        scope: WorkScope,
        expected_revision: i64,
        priority: i32,
    ) -> Result<Self, WorkJobRepositoryError> {
        if expected_revision < 0 {
            return Err(WorkJobRepositoryError::InvalidRevision);
        }
        Ok(Self {
            task_kind,
            scope,
            expected_revision,
            priority,
            required_sync_job_id: None,
            input_sync_revision: None,
            metadata_requirement: (task_kind == WorkTaskKind::ResolveMetadata)
                .then_some(MetadataRequirement::Basic),
            metadata_source_mode: (task_kind == WorkTaskKind::ResolveMetadata)
                .then_some(MetadataSourceMode::AutomaticScrape),
            local_metadata_access_mode: (task_kind == WorkTaskKind::ResolveMetadata)
                .then_some(LocalMetadataAccessMode::Import),
            storage_root_affinity: None,
        })
    }

    /// Sets the completeness required from a metadata resolution job.
    ///
    /// # Errors
    ///
    /// Returns [`WorkJobRepositoryError::InvalidMetadataWork`] for non-metadata tasks.
    pub fn with_metadata_requirement(
        mut self,
        requirement: MetadataRequirement,
    ) -> Result<Self, WorkJobRepositoryError> {
        if self.task_kind != WorkTaskKind::ResolveMetadata {
            return Err(WorkJobRepositoryError::InvalidMetadataWork);
        }
        self.metadata_requirement = Some(requirement);
        Ok(self)
    }

    /// Sets whether a metadata job may use remote providers.
    ///
    /// # Errors
    ///
    /// Returns [`WorkJobRepositoryError::InvalidMetadataWork`] for non-metadata tasks.
    pub fn with_metadata_source_mode(
        mut self,
        mode: MetadataSourceMode,
    ) -> Result<Self, WorkJobRepositoryError> {
        if self.task_kind != WorkTaskKind::ResolveMetadata {
            return Err(WorkJobRepositoryError::InvalidMetadataWork);
        }
        self.metadata_source_mode = Some(mode);
        Ok(self)
    }

    /// Sets whether local metadata is imported or referenced in place.
    ///
    /// # Errors
    ///
    /// Returns [`WorkJobRepositoryError::InvalidMetadataWork`] for non-metadata tasks.
    pub fn with_local_metadata_access_mode(
        mut self,
        mode: LocalMetadataAccessMode,
    ) -> Result<Self, WorkJobRepositoryError> {
        if self.task_kind != WorkTaskKind::ResolveMetadata {
            return Err(WorkJobRepositoryError::InvalidMetadataWork);
        }
        self.local_metadata_access_mode = Some(mode);
        Ok(self)
    }

    #[must_use]
    pub fn with_required_sync(mut self, job_id: WorkJobId, input_sync_revision: i64) -> Self {
        self.required_sync_job_id = Some(job_id);
        self.input_sync_revision = Some(input_sync_revision);
        self
    }

    /// Adds a scoped Storage Sync dependency whose committed revision is not known yet.
    ///
    /// The dependent media job remains pending until the sync job has completed, its
    /// scope is reconciled, and the claim transaction captures the resulting revision.
    #[must_use]
    pub fn with_pending_required_sync(mut self, job_id: WorkJobId) -> Self {
        self.required_sync_job_id = Some(job_id);
        self.input_sync_revision = None;
        self
    }

    /// Captures an already reconciled Storage Sync revision without a new dependency job.
    ///
    /// # Errors
    ///
    /// Returns [`WorkJobRepositoryError::InvalidRevision`] for a negative revision.
    pub fn with_input_sync_revision(
        mut self,
        input_sync_revision: i64,
    ) -> Result<Self, WorkJobRepositoryError> {
        if input_sync_revision < 0 {
            return Err(WorkJobRepositoryError::InvalidRevision);
        }
        self.input_sync_revision = Some(input_sync_revision);
        Ok(self)
    }

    /// Restricts catalog or storage work to one storage root.
    ///
    /// # Errors
    ///
    /// Returns [`WorkJobRepositoryError::InvalidStorageRootAffinity`] when the task/scope pair
    /// cannot be executed within a storage root.
    pub fn with_storage_root_affinity(
        mut self,
        storage_root: StorageRootId,
    ) -> Result<Self, WorkJobRepositoryError> {
        let catalog_work = matches!(self.scope, WorkScope::CatalogItem(_))
            && matches!(
                self.task_kind,
                WorkTaskKind::ResolveMetadata
                    | WorkTaskKind::ExpandItem
                    | WorkTaskKind::IndexMediaSources
            );
        let source_work = matches!(self.scope, WorkScope::MediaSource(_))
            && self.task_kind == WorkTaskKind::ProbeMedia;
        let scoped_storage_work = matches!(self.scope, WorkScope::StorageObject(_))
            && self.task_kind == WorkTaskKind::ScopedStorageSync;
        let root_storage_work = matches!(self.scope, WorkScope::StorageRoot(_))
            && matches!(
                self.task_kind,
                WorkTaskKind::ScopedStorageSync
                    | WorkTaskKind::RecoverStorageCursor
                    | WorkTaskKind::ValidateStorageRoot
            );
        if !catalog_work && !source_work && !scoped_storage_work && !root_storage_work {
            return Err(WorkJobRepositoryError::InvalidStorageRootAffinity);
        }
        self.storage_root_affinity = Some(storage_root);
        Ok(self)
    }

    #[must_use]
    pub const fn task_kind(&self) -> WorkTaskKind {
        self.task_kind
    }

    #[must_use]
    pub const fn scope(&self) -> WorkScope {
        self.scope
    }

    #[must_use]
    pub const fn expected_revision(&self) -> i64 {
        self.expected_revision
    }

    #[must_use]
    pub const fn metadata_requirement(&self) -> Option<MetadataRequirement> {
        self.metadata_requirement
    }

    #[must_use]
    pub const fn metadata_source_mode(&self) -> Option<MetadataSourceMode> {
        self.metadata_source_mode
    }

    #[must_use]
    pub const fn local_metadata_access_mode(&self) -> Option<LocalMetadataAccessMode> {
        self.local_metadata_access_mode
    }

    #[must_use]
    pub const fn storage_root_affinity(&self) -> Option<StorageRootId> {
        self.storage_root_affinity
    }

    fn natural_key_storage_root_id(&self) -> Uuid {
        if matches!(
            self.task_kind,
            WorkTaskKind::ScopedStorageSync
                | WorkTaskKind::RecoverStorageCursor
                | WorkTaskKind::ValidateStorageRoot
        ) {
            self.storage_root_affinity
                .map_or_else(Uuid::nil, StorageRootId::as_uuid)
        } else {
            Uuid::nil()
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkJobState {
    Pending,
    Running,
    Completed,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkJobAdminStatus {
    Pending,
    Retrying,
    Running,
    Completed,
    Cancelled,
    Failed,
}

impl WorkJobState {
    fn from_database(value: &str) -> Result<Self, WorkJobRepositoryError> {
        match value {
            STATE_PENDING => Ok(Self::Pending),
            STATE_RUNNING => Ok(Self::Running),
            STATE_COMPLETED => Ok(Self::Completed),
            STATE_FAILED => Ok(Self::Failed),
            _ => Err(WorkJobRepositoryError::InvalidStoredState),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkJobRecord {
    id: WorkJobId,
    task_kind: WorkTaskKind,
    scope: WorkScope,
    expected_revision: i64,
    required_sync_job_id: Option<WorkJobId>,
    input_sync_revision: Option<i64>,
    state: WorkJobState,
    priority: i32,
    attempt_count: i32,
    metadata_requirement: Option<MetadataRequirement>,
    metadata_source_mode: Option<MetadataSourceMode>,
    local_metadata_access_mode: Option<LocalMetadataAccessMode>,
    storage_root_affinity: Option<StorageRootId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkJobAdminRecord {
    job: WorkJobRecord,
    admin_status: WorkJobAdminStatus,
    created_at: Option<DateTime<Utc>>,
    started_at: Option<DateTime<Utc>>,
    completed_at: Option<DateTime<Utc>>,
    outcome: Option<WorkJobAdminOutcome>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkJobAdminOutcome {
    NoMetadataMatch,
    CompletedWithWarnings,
}

impl WorkJobAdminRecord {
    #[must_use]
    pub const fn job(&self) -> &WorkJobRecord {
        &self.job
    }

    #[must_use]
    pub const fn admin_status(&self) -> WorkJobAdminStatus {
        self.admin_status
    }

    #[must_use]
    pub const fn created_at(&self) -> Option<DateTime<Utc>> {
        self.created_at
    }

    #[must_use]
    pub const fn started_at(&self) -> Option<DateTime<Utc>> {
        self.started_at
    }

    #[must_use]
    pub const fn completed_at(&self) -> Option<DateTime<Utc>> {
        self.completed_at
    }

    #[must_use]
    pub const fn outcome(&self) -> Option<WorkJobAdminOutcome> {
        self.outcome
    }
}

impl WorkJobRecord {
    #[must_use]
    pub const fn id(&self) -> WorkJobId {
        self.id
    }

    #[must_use]
    pub const fn task_kind(&self) -> WorkTaskKind {
        self.task_kind
    }

    #[must_use]
    pub const fn scope(&self) -> WorkScope {
        self.scope
    }

    #[must_use]
    pub const fn expected_revision(&self) -> i64 {
        self.expected_revision
    }

    #[must_use]
    pub const fn required_sync_job_id(&self) -> Option<WorkJobId> {
        self.required_sync_job_id
    }

    #[must_use]
    pub const fn input_sync_revision(&self) -> Option<i64> {
        self.input_sync_revision
    }

    #[must_use]
    pub const fn state(&self) -> WorkJobState {
        self.state
    }

    #[must_use]
    pub const fn priority(&self) -> i32 {
        self.priority
    }

    #[must_use]
    pub const fn attempt_count(&self) -> i32 {
        self.attempt_count
    }

    #[must_use]
    pub const fn metadata_requirement(&self) -> Option<MetadataRequirement> {
        self.metadata_requirement
    }

    #[must_use]
    pub const fn metadata_source_mode(&self) -> Option<MetadataSourceMode> {
        self.metadata_source_mode
    }

    #[must_use]
    pub const fn local_metadata_access_mode(&self) -> Option<LocalMetadataAccessMode> {
        self.local_metadata_access_mode
    }

    #[must_use]
    pub const fn storage_root_affinity(&self) -> Option<StorageRootId> {
        self.storage_root_affinity
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkJobSubmission {
    job: WorkJobRecord,
    created: bool,
}

impl WorkJobSubmission {
    #[must_use]
    pub const fn job(&self) -> &WorkJobRecord {
        &self.job
    }

    #[must_use]
    pub const fn created(&self) -> bool {
        self.created
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FullScanChildSubmission {
    Current,
    Stale,
    Job(WorkJobSubmission),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PublicationFence {
    NeedsWork,
    Current,
    Stale,
}

#[derive(Clone, Debug)]
pub struct ClaimedWorkJob {
    job: WorkJobRecord,
    lease_token: String,
}

impl ClaimedWorkJob {
    #[must_use]
    pub const fn id(&self) -> WorkJobId {
        self.job.id
    }

    #[must_use]
    pub const fn attempt_count(&self) -> i32 {
        self.job.attempt_count
    }

    #[must_use]
    pub const fn job(&self) -> &WorkJobRecord {
        &self.job
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct WorkStagingRow {
    entity_kind: String,
    natural_key: String,
    payload: Value,
    validation_state: String,
}

impl WorkStagingRow {
    /// Defines one idempotent publication staging row.
    ///
    /// # Errors
    ///
    /// Returns [`WorkJobRepositoryError::InvalidStagingRow`] for empty or oversized keys.
    pub fn new(
        entity_kind: impl Into<String>,
        natural_key: impl Into<String>,
        payload: Value,
        validation_state: impl Into<String>,
    ) -> Result<Self, WorkJobRepositoryError> {
        let entity_kind = entity_kind.into();
        let natural_key = natural_key.into();
        let validation_state = validation_state.into();
        if entity_kind.trim().is_empty()
            || natural_key.trim().is_empty()
            || validation_state.trim().is_empty()
            || entity_kind.chars().count() > MAX_STAGING_KEY_CHARS
            || natural_key.chars().count() > MAX_STAGING_KEY_CHARS
            || validation_state.chars().count() > MAX_STAGING_KEY_CHARS
        {
            return Err(WorkJobRepositoryError::InvalidStagingRow);
        }
        Ok(Self {
            entity_kind,
            natural_key,
            payload,
            validation_state,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct WorkJobResult {
    counters: Value,
    warnings: Vec<String>,
    error_summary: Option<String>,
    result_sync_revision: Option<i64>,
}

impl WorkJobResult {
    #[must_use]
    pub fn success(counters: Value, warnings: Vec<String>) -> Self {
        Self {
            counters,
            warnings,
            error_summary: None,
            result_sync_revision: None,
        }
    }

    /// Attaches the committed Storage Sync revision consumed by dependent jobs.
    ///
    /// # Errors
    ///
    /// Returns [`WorkJobRepositoryError::InvalidRevision`] for a negative revision.
    pub fn with_sync_revision(mut self, revision: i64) -> Result<Self, WorkJobRepositoryError> {
        if revision < 0 {
            return Err(WorkJobRepositoryError::InvalidRevision);
        }
        self.result_sync_revision = Some(revision);
        Ok(self)
    }
}

pub trait WorkJobClock: Clone + Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct WorkJobSystemClock;

impl WorkJobClock for WorkJobSystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

pub struct WorkJobRepository<'connection, Clock = WorkJobSystemClock> {
    database: &'connection DatabaseConnection,
    clock: Clock,
}

impl<'connection> WorkJobRepository<'connection, WorkJobSystemClock> {
    #[must_use]
    pub const fn new(database: &'connection DatabaseConnection) -> Self {
        Self {
            database,
            clock: WorkJobSystemClock,
        }
    }
}

impl<'connection, Clock> WorkJobRepository<'connection, Clock>
where
    Clock: WorkJobClock,
{
    fn now(&self) -> DateTime<Utc> {
        mysql_compatible_timestamp(self.database.get_database_backend(), self.clock.now())
    }

    #[must_use]
    pub const fn with_clock(database: &'connection DatabaseConnection, clock: Clock) -> Self {
        Self { database, clock }
    }

    /// Creates a pending job or joins its active natural key.
    ///
    /// A higher-priority join promotes a pending job without replacing it.
    ///
    /// # Errors
    ///
    /// Returns [`WorkJobRepositoryError`] for dependency, invariant, or database failures.
    pub async fn enqueue_or_join(
        &self,
        spec: &WorkJobSpec,
    ) -> Result<WorkJobSubmission, WorkJobRepositoryError> {
        if spec
            .input_sync_revision
            .is_some_and(|revision| revision < 0)
        {
            return Err(WorkJobRepositoryError::InvalidRevision);
        }
        let transaction = self.database.begin().await?;
        let result = enqueue_or_join(&transaction, spec, self.now()).await;
        finish(transaction, result).await
    }

    /// Enqueues or joins one policy-aware media scan for each enabled Library with automatic work.
    ///
    /// The library profile version is captured as the job revision so configuration changes
    /// fence stale scans. The complete batch is created in one transaction.
    ///
    /// # Errors
    ///
    /// Returns [`WorkJobRepositoryError`] for database, row-decoding, or enqueue failures.
    pub async fn enqueue_enabled_library_scans(
        &self,
        priority: i32,
    ) -> Result<Vec<WorkJobSubmission>, WorkJobRepositoryError> {
        let transaction = self.database.begin().await?;
        let result = enqueue_enabled_library_scans(&transaction, priority, self.now()).await;
        finish(transaction, result).await
    }

    /// Reports whether any pending or running job exists for a task type.
    ///
    /// # Errors
    ///
    /// Returns [`WorkJobRepositoryError`] for database failures.
    pub async fn has_active_task(
        &self,
        task_kind: WorkTaskKind,
    ) -> Result<bool, WorkJobRepositoryError> {
        let query = Query::select()
            .expr(Expr::val(1_i32))
            .from(Alias::new("work_jobs"))
            .and_where(Expr::col(Alias::new("task_kind")).eq(task_kind.as_str()))
            .and_where(Expr::col(Alias::new("state")).is_in([STATE_PENDING, STATE_RUNNING]))
            .limit(1)
            .to_owned();
        let backend = self.database.get_database_backend();
        Ok(self
            .database
            .query_one(backend.build(&query))
            .await?
            .is_some())
    }

    /// Returns a bounded, newest-first administrator observation of durable work.
    ///
    /// Persisted error text and lease metadata are deliberately reduced to a safe status enum.
    ///
    /// # Errors
    ///
    /// Returns [`WorkJobRepositoryError`] for an invalid limit, corrupt row, or database failure.
    pub async fn recent_jobs(
        &self,
        limit: u64,
    ) -> Result<Vec<WorkJobAdminRecord>, WorkJobRepositoryError> {
        if !(1..=MAX_OBSERVED_JOBS).contains(&limit) {
            return Err(WorkJobRepositoryError::InvalidObservationLimit);
        }
        let table = Alias::new("work_jobs");
        let mut query = Query::select();
        query
            .from(table.clone())
            .order_by((table.clone(), Alias::new("created_at")), Order::Desc)
            .order_by((table.clone(), Alias::new("id")), Order::Desc)
            .limit(limit);
        select_job_columns(&mut query, &table);
        for column in ["created_at", "started_at", "completed_at", "last_error"] {
            query.expr_as(
                Expr::col((table.clone(), Alias::new(column))),
                Alias::new(column),
            );
        }
        let results = Alias::new("job_results");
        query.join_as(
            JoinType::LeftJoin,
            Alias::new("work_results"),
            results.clone(),
            Expr::col((results.clone(), Alias::new("job_id")))
                .equals((table.clone(), Alias::new("id"))),
        );
        query.expr_as(
            Expr::col((results.clone(), Alias::new("counters"))),
            Alias::new("result_counters"),
        );
        query.expr_as(
            Expr::col((results, Alias::new("warnings"))),
            Alias::new("result_warnings"),
        );
        let backend = self.database.get_database_backend();
        self.database
            .query_all(backend.build(&query))
            .await?
            .iter()
            .map(admin_job_from_row)
            .collect()
    }

    /// Atomically terminates every pending or running job for a task type.
    ///
    /// # Errors
    ///
    /// Returns [`WorkJobRepositoryError`] for invalid error text or persistence failures.
    pub async fn cancel_active_task(
        &self,
        task_kind: WorkTaskKind,
        error: &str,
    ) -> Result<u64, WorkJobRepositoryError> {
        if error.trim().is_empty() || error.chars().count() > MAX_ERROR_CHARS {
            return Err(WorkJobRepositoryError::InvalidErrorSummary);
        }
        let transaction = self.database.begin().await?;
        let result = cancel_active_task(&transaction, task_kind, error, self.now()).await;
        finish(transaction, result).await
    }

    /// Atomically skips already-published lazy work or enqueues/joins its natural key.
    ///
    /// The item revision row is locked before the active publication check, closing
    /// the completion-versus-enqueue race between request and publisher transactions.
    ///
    /// # Errors
    ///
    /// Returns [`WorkJobRepositoryError`] for invalid lazy work or database failures.
    pub async fn enqueue_lazy_or_join(
        &self,
        spec: &WorkJobSpec,
    ) -> Result<Option<WorkJobSubmission>, WorkJobRepositoryError> {
        let transaction = self.database.begin().await?;
        let result = match fence_lazy_item(&transaction, spec).await {
            Ok(PublicationFence::Current | PublicationFence::Stale) => Ok(None),
            Ok(PublicationFence::NeedsWork) => enqueue_or_join(&transaction, spec, self.now())
                .await
                .map(Some),
            Err(error) => Err(error),
        };
        finish(transaction, result).await
    }

    /// Retries incomplete automatic metadata resolution with a short cooldown.
    ///
    /// # Errors
    ///
    /// Returns [`WorkJobRepositoryError`] for invalid metadata work or database failures.
    pub async fn enqueue_metadata_retry_or_join(
        &self,
        spec: &WorkJobSpec,
    ) -> Result<Option<WorkJobSubmission>, WorkJobRepositoryError> {
        if spec.task_kind != WorkTaskKind::ResolveMetadata
            || spec.metadata_source_mode != Some(MetadataSourceMode::AutomaticScrape)
        {
            return Err(WorkJobRepositoryError::InvalidMetadataWork);
        }
        let WorkScope::CatalogItem(item_id) = spec.scope else {
            return Err(WorkJobRepositoryError::InvalidMetadataWork);
        };
        let transaction = self.database.begin().await?;
        let backend = transaction.get_database_backend();
        let incomplete = Query::select()
            .expr(Expr::val(1_i32))
            .from(Alias::new("catalog_items"))
            .and_where(Expr::col(Alias::new("id")).eq(item_id.as_uuid()))
            .and_where(Expr::col(Alias::new("metadata_revision")).eq(spec.expected_revision))
            .cond_where(
                Cond::any()
                    .add(Expr::col(Alias::new("metadata_state")).eq("Partial"))
                    .add(
                        Expr::col(Alias::new("metadata_payload_version"))
                            .lt(crate::metadata::METADATA_PAYLOAD_VERSION),
                    ),
            )
            .limit(1)
            .to_owned();
        if transaction
            .query_one(backend.build(&incomplete))
            .await?
            .is_none()
        {
            return finish(transaction, Ok(None)).await;
        }
        let recent = Query::select()
            .expr(Expr::val(1_i32))
            .from(Alias::new("work_jobs"))
            .and_where(
                Expr::col(Alias::new("task_kind")).eq(WorkTaskKind::ResolveMetadata.as_str()),
            )
            .and_where(Expr::col(Alias::new("scope_type")).eq("CatalogItem"))
            .and_where(Expr::col(Alias::new("scope_id")).eq(item_id.as_uuid()))
            .and_where(Expr::col(Alias::new("expected_revision")).eq(spec.expected_revision))
            .and_where(Expr::col(Alias::new("state")).eq(STATE_COMPLETED))
            .and_where(
                Expr::col(Alias::new("created_at")).gte(self.now() - METADATA_RETRY_COOLDOWN),
            )
            .limit(1)
            .to_owned();
        if transaction
            .query_one(backend.build(&recent))
            .await?
            .is_some()
        {
            return finish(transaction, Ok(None)).await;
        }
        let result = enqueue_or_join(&transaction, spec, self.now())
            .await
            .map(Some);
        finish(transaction, result).await
    }

    /// Claims the highest-priority ready job among the accepted task kinds.
    ///
    /// # Errors
    ///
    /// Returns [`WorkJobRepositoryError`] for invalid lease values or database failures.
    pub async fn claim_next(
        &self,
        accepted_kinds: &[WorkTaskKind],
        lease_owner: &str,
        lease_duration: Duration,
    ) -> Result<Option<ClaimedWorkJob>, WorkJobRepositoryError> {
        validate_lease(accepted_kinds, lease_owner, lease_duration)?;
        let now = self.now();
        let lease_expires_at = now
            .checked_add_signed(lease_duration)
            .ok_or(WorkJobRepositoryError::TimestampOverflow)?;
        let transaction = self.database.begin().await?;
        let result = claim_next(
            &transaction,
            accepted_kinds,
            None,
            lease_owner,
            now,
            lease_expires_at,
        )
        .await;
        finish(transaction, result).await
    }

    /// Claims the highest-priority scoped inventory job owned by one storage account.
    ///
    /// The account predicate participates in the transactional candidate query so workers for
    /// different backends cannot steal and requeue each other's jobs.
    ///
    /// # Errors
    ///
    /// Returns [`WorkJobRepositoryError`] for invalid lease values or database failures.
    pub async fn claim_next_scoped_sync(
        &self,
        account_id: Uuid,
        lease_owner: &str,
        lease_duration: Duration,
    ) -> Result<Option<ClaimedWorkJob>, WorkJobRepositoryError> {
        let accepted_kinds = [
            WorkTaskKind::ScopedStorageSync,
            WorkTaskKind::RecoverStorageCursor,
            WorkTaskKind::ValidateStorageRoot,
        ];
        validate_lease(&accepted_kinds, lease_owner, lease_duration)?;
        let now = self.now();
        let lease_expires_at = now
            .checked_add_signed(lease_duration)
            .ok_or(WorkJobRepositoryError::TimestampOverflow)?;
        let transaction = self.database.begin().await?;
        let result = claim_next(
            &transaction,
            &accepted_kinds,
            Some((account_id, None)),
            lease_owner,
            now,
            lease_expires_at,
        )
        .await;
        finish(transaction, result).await
    }

    /// Claims a scoped inventory job for one account and provider drive.
    ///
    /// # Errors
    ///
    /// Returns [`WorkJobRepositoryError`] for invalid drive/lease values or database failures.
    pub async fn claim_next_scoped_sync_for_drive(
        &self,
        account_id: Uuid,
        provider_drive_id: &str,
        lease_owner: &str,
        lease_duration: Duration,
    ) -> Result<Option<ClaimedWorkJob>, WorkJobRepositoryError> {
        if provider_drive_id.trim().is_empty()
            || provider_drive_id.chars().count() > 2048
            || provider_drive_id.chars().any(char::is_control)
        {
            return Err(WorkJobRepositoryError::InvalidProviderDriveId);
        }
        let accepted_kinds = [
            WorkTaskKind::ScopedStorageSync,
            WorkTaskKind::RecoverStorageCursor,
            WorkTaskKind::ValidateStorageRoot,
        ];
        validate_lease(&accepted_kinds, lease_owner, lease_duration)?;
        let now = self.now();
        let lease_expires_at = now
            .checked_add_signed(lease_duration)
            .ok_or(WorkJobRepositoryError::TimestampOverflow)?;
        let transaction = self.database.begin().await?;
        let result = claim_next(
            &transaction,
            &accepted_kinds,
            Some((account_id, Some(provider_drive_id))),
            lease_owner,
            now,
            lease_expires_at,
        )
        .await;
        finish(transaction, result).await
    }

    /// Extends a live claim using the same fencing token.
    ///
    /// # Errors
    ///
    /// Returns [`WorkJobRepositoryError::LostLease`] for an expired or replaced claim.
    pub async fn renew(
        &self,
        claimed: &ClaimedWorkJob,
        lease_duration: Duration,
    ) -> Result<(), WorkJobRepositoryError> {
        if lease_duration <= Duration::zero() {
            return Err(WorkJobRepositoryError::InvalidLeaseDuration);
        }
        let now = self.now();
        let lease_expires_at = now
            .checked_add_signed(lease_duration)
            .ok_or(WorkJobRepositoryError::TimestampOverflow)?;
        let backend = self.database.get_database_backend();
        let statement = Query::update()
            .table(Alias::new("work_jobs"))
            .value(Alias::new("lease_expires_at"), lease_expires_at)
            .cond_where(lease_condition(claimed, now))
            .to_owned();
        if self
            .database
            .execute(backend.build(&statement))
            .await?
            .rows_affected()
            != 1
        {
            return Err(WorkJobRepositoryError::LostLease);
        }
        Ok(())
    }

    /// Requeues a live claim after a bounded delay.
    ///
    /// # Errors
    ///
    /// Returns [`WorkJobRepositoryError::LostLease`] for an expired or replaced claim.
    pub async fn retry(
        &self,
        claimed: &ClaimedWorkJob,
        backoff: Duration,
        error: &str,
    ) -> Result<(), WorkJobRepositoryError> {
        if backoff < Duration::zero() {
            return Err(WorkJobRepositoryError::InvalidBackoff);
        }
        if error.trim().is_empty() || error.chars().count() > MAX_ERROR_CHARS {
            return Err(WorkJobRepositoryError::InvalidErrorSummary);
        }
        let now = self.now();
        let available_at = now
            .checked_add_signed(backoff)
            .ok_or(WorkJobRepositoryError::TimestampOverflow)?;
        let backend = self.database.get_database_backend();
        let statement = Query::update()
            .table(Alias::new("work_jobs"))
            .value(Alias::new("state"), STATE_PENDING)
            .value(Alias::new("available_at"), available_at)
            .value(Alias::new("lease_owner"), Option::<String>::None)
            .value(
                Alias::new("lease_expires_at"),
                Option::<DateTime<Utc>>::None,
            )
            .value(Alias::new("last_error"), error)
            .cond_where(lease_condition(claimed, now))
            .to_owned();
        if self
            .database
            .execute(backend.build(&statement))
            .await?
            .rows_affected()
            != 1
        {
            return Err(WorkJobRepositoryError::LostLease);
        }
        Ok(())
    }

    /// Permanently fails a live claim and releases its active natural key.
    ///
    /// # Errors
    ///
    /// Returns [`WorkJobRepositoryError::LostLease`] for an expired or replaced claim.
    pub async fn fail_terminal(
        &self,
        claimed: &ClaimedWorkJob,
        error: &str,
    ) -> Result<(), WorkJobRepositoryError> {
        if error.trim().is_empty() || error.chars().count() > MAX_ERROR_CHARS {
            return Err(WorkJobRepositoryError::InvalidErrorSummary);
        }
        let transaction = self.database.begin().await?;
        let result = fail_terminal(&transaction, claimed, error, self.now()).await;
        finish(transaction, result).await
    }

    /// Idempotently writes one staging batch under a live claim.
    ///
    /// # Errors
    ///
    /// Returns [`WorkJobRepositoryError::LostLease`] for an expired or replaced claim.
    pub async fn stage_batch(
        &self,
        claimed: &ClaimedWorkJob,
        publication_id: Uuid,
        rows: &[WorkStagingRow],
    ) -> Result<(), WorkJobRepositoryError> {
        let transaction = self.database.begin().await?;
        let result = stage_batch(&transaction, claimed, publication_id, rows, self.now()).await;
        let result = match result {
            Ok(()) => fence_live_claim(&transaction, claimed, self.now()).await,
            Err(error) => Err(error),
        };
        finish(transaction, result).await
    }

    /// Atomically fences one Full Scan claim, enqueues or joins a child, and records ownership.
    ///
    /// # Errors
    ///
    /// Returns [`WorkJobRepositoryError`] when the parent lease or Library profile is stale, the
    /// child specification is invalid, or persistence fails.
    pub async fn enqueue_full_scan_child(
        &self,
        claimed: &ClaimedWorkJob,
        natural_key: &str,
        spec: &WorkJobSpec,
    ) -> Result<FullScanChildSubmission, WorkJobRepositoryError> {
        let row = WorkStagingRow::new(
            "FullScanChild",
            natural_key,
            Value::Object(serde_json::Map::new()),
            "Required",
        )?;
        let transaction = self.database.begin().await?;
        let result = async {
            fence_full_scan_parent(&transaction, claimed).await?;
            if matches!(
                spec.task_kind(),
                WorkTaskKind::ExpandItem | WorkTaskKind::IndexMediaSources
            ) {
                match fence_lazy_item(&transaction, spec).await? {
                    PublicationFence::Current => {
                        return Ok(FullScanChildSubmission::Current);
                    }
                    PublicationFence::Stale => return Ok(FullScanChildSubmission::Stale),
                    PublicationFence::NeedsWork => {}
                }
            }
            if spec.task_kind() == WorkTaskKind::ResolveMetadata {
                match fence_metadata_item(&transaction, spec).await? {
                    PublicationFence::Current => {
                        return Ok(FullScanChildSubmission::Current);
                    }
                    PublicationFence::Stale => return Ok(FullScanChildSubmission::Stale),
                    PublicationFence::NeedsWork => {}
                }
            }
            let submission = enqueue_or_join(&transaction, spec, self.now()).await?;
            if let (WorkTaskKind::DiscoverTitles, WorkScope::LibraryRootBinding(binding_id)) =
                (spec.task_kind(), spec.scope())
            {
                crate::discover::stage_discovery_binding(
                    &transaction,
                    submission.job().id(),
                    binding_id,
                    claimed.job().expected_revision(),
                )
                .await?;
            }
            let row = WorkStagingRow {
                payload: serde_json::json!({
                    "job_id": submission.job().id().to_string(),
                    "created": submission.created(),
                }),
                ..row
            };
            stage_batch(
                &transaction,
                claimed,
                claimed.id().as_uuid(),
                &[row],
                self.now(),
            )
            .await?;
            Ok(FullScanChildSubmission::Job(submission))
        }
        .await;
        finish(transaction, result).await
    }

    /// Atomically revalidates one Full Scan's Library profile and completes the parent claim.
    ///
    /// # Errors
    ///
    /// Returns [`WorkJobRepositoryError`] for a stale profile, lost lease, or persistence failure.
    pub async fn complete_full_scan(
        &self,
        claimed: &ClaimedWorkJob,
        result: WorkJobResult,
    ) -> Result<(), WorkJobRepositoryError> {
        let transaction = self.database.begin().await?;
        let completion = async {
            fence_full_scan_parent(&transaction, claimed).await?;
            complete_in_transaction(&transaction, claimed, result, self.now()).await
        }
        .await;
        finish(transaction, completion).await
    }

    /// Marks a live claim completed inside the caller's publication transaction.
    ///
    /// # Errors
    ///
    /// Returns [`WorkJobRepositoryError::LostLease`] for an expired or replaced claim.
    pub async fn complete_in_transaction(
        &self,
        transaction: &DatabaseTransaction,
        claimed: &ClaimedWorkJob,
        result: WorkJobResult,
    ) -> Result<(), WorkJobRepositoryError> {
        complete_in_transaction(transaction, claimed, result, self.now()).await
    }

    /// Permanently fails a live claim inside the caller's transaction.
    ///
    /// # Errors
    ///
    /// Returns [`WorkJobRepositoryError::LostLease`] for an expired or replaced claim.
    pub async fn fail_terminal_in_transaction(
        &self,
        transaction: &DatabaseTransaction,
        claimed: &ClaimedWorkJob,
        error: &str,
    ) -> Result<(), WorkJobRepositoryError> {
        if error.trim().is_empty() || error.chars().count() > MAX_ERROR_CHARS {
            return Err(WorkJobRepositoryError::InvalidErrorSummary);
        }
        fail_terminal(transaction, claimed, error, self.now()).await
    }

    /// Reads a durable job for join polling.
    ///
    /// # Errors
    ///
    /// Returns [`WorkJobRepositoryError`] for database or row-decoding failures.
    pub async fn get(
        &self,
        job_id: WorkJobId,
    ) -> Result<Option<WorkJobRecord>, WorkJobRepositoryError> {
        let backend = self.database.get_database_backend();
        self.database
            .query_one(backend.build(&job_by_id(job_id)))
            .await?
            .as_ref()
            .map(job_from_row)
            .transpose()
    }

    /// Returns the committed revision of a completed Scoped Storage Sync job.
    ///
    /// # Errors
    ///
    /// Returns [`WorkJobRepositoryError`] for database or stored-value corruption.
    pub async fn completed_sync_revision(
        &self,
        job_id: WorkJobId,
    ) -> Result<Option<i64>, WorkJobRepositoryError> {
        let job = Alias::new("completed_sync_job");
        let result = Alias::new("completed_sync_result");
        let query = Query::select()
            .expr_as(
                Expr::col((result.clone(), Alias::new("result_sync_revision"))),
                Alias::new("result_sync_revision"),
            )
            .from_as(Alias::new("work_jobs"), job.clone())
            .join_as(
                sea_orm::sea_query::JoinType::InnerJoin,
                Alias::new("work_results"),
                result.clone(),
                Expr::col((result, Alias::new("job_id"))).equals((job.clone(), Alias::new("id"))),
            )
            .and_where(Expr::col((job.clone(), Alias::new("id"))).eq(job_id.as_uuid()))
            .and_where(Expr::col((job.clone(), Alias::new("task_kind"))).eq("ScopedStorageSync"))
            .and_where(Expr::col((job, Alias::new("state"))).eq(STATE_COMPLETED))
            .limit(1)
            .to_owned();
        let backend = self.database.get_database_backend();
        Ok(self
            .database
            .query_one(backend.build(&query))
            .await?
            .map(|row| row.try_get("", "result_sync_revision"))
            .transpose()?)
    }
}

async fn fence_lazy_item(
    transaction: &DatabaseTransaction,
    spec: &WorkJobSpec,
) -> Result<PublicationFence, WorkJobRepositoryError> {
    let WorkScope::CatalogItem(item_id) = spec.scope else {
        return Err(WorkJobRepositoryError::InvalidLazyWork);
    };
    let (revision_column, pointer_column, publication_kind) = match spec.task_kind {
        WorkTaskKind::ExpandItem => (
            "structure_expansion_revision",
            "active_structure_publication_id",
            "Structure",
        ),
        WorkTaskKind::IndexMediaSources => (
            "source_index_revision",
            "active_source_publication_id",
            "Sources",
        ),
        WorkTaskKind::DiscoverTitles
        | WorkTaskKind::ResolveMetadata
        | WorkTaskKind::ScopedStorageSync
        | WorkTaskKind::RecoverStorageCursor
        | WorkTaskKind::ValidateStorageRoot
        | WorkTaskKind::ProbeMedia
        | WorkTaskKind::FullMediaScan
        | WorkTaskKind::FullLibraryRootScan => {
            return Err(WorkJobRepositoryError::InvalidLazyWork);
        }
    };
    let fence = Query::update()
        .table(Alias::new("catalog_items"))
        .value(
            Alias::new(revision_column),
            Expr::col(Alias::new(revision_column)),
        )
        .and_where(Expr::col(Alias::new("id")).eq(item_id.as_uuid()))
        .and_where(Expr::col(Alias::new(revision_column)).eq(spec.expected_revision))
        .to_owned();
    let backend = transaction.get_database_backend();
    if transaction
        .execute(backend.build(&fence))
        .await?
        .rows_affected()
        != 1
    {
        return Ok(PublicationFence::Stale);
    }
    let item = Alias::new("lazy_fenced_item");
    let publication = Alias::new("lazy_current_publication");
    let current = Query::select()
        .expr(Expr::val(1_i32))
        .from_as(Alias::new("catalog_items"), item.clone())
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("catalog_publications"),
            publication.clone(),
            Expr::col((publication.clone(), Alias::new("id")))
                .equals((item.clone(), Alias::new(pointer_column))),
        )
        .and_where(Expr::col((item, Alias::new("id"))).eq(item_id.as_uuid()))
        .and_where(
            Expr::col((publication.clone(), Alias::new("publication_kind"))).eq(publication_kind),
        )
        .and_where(Expr::col((publication.clone(), Alias::new("state"))).eq("Active"))
        .and_where(
            Expr::col((publication, Alias::new("expected_revision"))).eq(spec.expected_revision),
        )
        .limit(1)
        .to_owned();
    Ok(
        if transaction
            .query_one(backend.build(&current))
            .await?
            .is_some()
        {
            PublicationFence::Current
        } else {
            PublicationFence::NeedsWork
        },
    )
}

async fn fence_metadata_item(
    transaction: &DatabaseTransaction,
    spec: &WorkJobSpec,
) -> Result<PublicationFence, WorkJobRepositoryError> {
    let WorkScope::CatalogItem(item_id) = spec.scope else {
        return Err(WorkJobRepositoryError::InvalidMetadataWork);
    };
    let fence = Query::update()
        .table(Alias::new("catalog_items"))
        .value(
            Alias::new("metadata_revision"),
            Expr::col(Alias::new("metadata_revision")),
        )
        .and_where(Expr::col(Alias::new("id")).eq(item_id.as_uuid()))
        .and_where(Expr::col(Alias::new("metadata_revision")).eq(spec.expected_revision))
        .to_owned();
    let backend = transaction.get_database_backend();
    if transaction
        .execute(backend.build(&fence))
        .await?
        .rows_affected()
        != 1
    {
        return Ok(PublicationFence::Stale);
    }
    let requirement = spec
        .metadata_requirement
        .ok_or(WorkJobRepositoryError::InvalidMetadataWork)?;
    let current = Query::select()
        .expr(Expr::val(1_i32))
        .from(Alias::new("catalog_items"))
        .and_where(Expr::col(Alias::new("id")).eq(item_id.as_uuid()))
        .and_where(Expr::col(Alias::new("metadata_resolved_revision")).gte(spec.expected_revision))
        .and_where(Expr::col(Alias::new("metadata_resolved_requirement")).gte(requirement.as_i32()))
        .cond_where(
            Cond::any()
                .add(Expr::col(Alias::new("item_type")).is_not_in(["Movie", "Series"]))
                .add(
                    Expr::col(Alias::new("metadata_payload_version"))
                        .gte(crate::metadata::METADATA_PAYLOAD_VERSION),
                ),
        )
        .limit(1)
        .to_owned();
    Ok(
        if transaction
            .query_one(backend.build(&current))
            .await?
            .is_some()
        {
            PublicationFence::Current
        } else {
            PublicationFence::NeedsWork
        },
    )
}

#[derive(Debug, Error)]
pub enum WorkJobRepositoryError {
    #[error("work revision must not be negative")]
    InvalidRevision,
    #[error("stored metadata requirement is invalid")]
    InvalidStoredMetadataRequirement,
    #[error("stored metadata source mode is invalid")]
    InvalidStoredMetadataSourceMode,
    #[error("lazy enqueue requires an ExpandItem or IndexMediaSources CatalogItem scope")]
    InvalidLazyWork,
    #[error("metadata enqueue requires a ResolveMetadata CatalogItem scope")]
    InvalidMetadataWork,
    #[error("storage-root affinity is incompatible with this work task and scope")]
    InvalidStorageRootAffinity,
    #[error("at least one accepted task kind is required")]
    EmptyAcceptedTaskKinds,
    #[error("lease owner must not be empty")]
    EmptyLeaseOwner,
    #[error("lease owner must not exceed 128 characters")]
    LeaseOwnerTooLong,
    #[error("lease duration must be positive")]
    InvalidLeaseDuration,
    #[error("retry backoff must not be negative")]
    InvalidBackoff,
    #[error("lease or retry timestamp is outside the supported range")]
    TimestampOverflow,
    #[error("staging keys and validation state must be non-empty and bounded")]
    InvalidStagingRow,
    #[error("error summary must be non-empty and bounded")]
    InvalidErrorSummary,
    #[error("required sync job does not exist")]
    MissingDependency,
    #[error("required dependency is not a scoped storage sync job")]
    InvalidDependency,
    #[error(
        "required scoped storage sync is not completed and reconciled at the requested revision"
    )]
    DependencyNotReady,
    #[error("active job has incompatible dependency metadata")]
    IncompatibleActiveJob,
    #[error("full scan child reference is corrupt")]
    InvalidChildReference,
    #[error("full scan parent Library profile changed after the claim was read")]
    StaleParentPolicy,
    #[error("work job disappeared after enqueue")]
    MissingEnqueuedJob,
    #[error("work job lease is expired or no longer owned by this claim")]
    LostLease,
    #[error("stored work task kind is invalid")]
    InvalidStoredTaskKind,
    #[error("stored work scope type is invalid")]
    InvalidStoredScopeType,
    #[error("stored work state is invalid")]
    InvalidStoredState,
    #[error("stored work attempt count is invalid")]
    InvalidAttemptCount,
    #[error("administrator work observation limit must be between 1 and 100")]
    InvalidObservationLimit,
    #[error("database operation failed: {0}")]
    Database(#[from] DbErr),
    #[error("provider drive ID must be non-empty, bounded, and contain no control characters")]
    InvalidProviderDriveId,
    #[error("rollback failed after {original}: {rollback}")]
    RollbackFailed { original: String, rollback: DbErr },
}

async fn cancel_active_task(
    transaction: &DatabaseTransaction,
    task_kind: WorkTaskKind,
    error: &str,
    now: DateTime<Utc>,
) -> Result<u64, WorkJobRepositoryError> {
    let query = Query::select()
        .column(Alias::new("id"))
        .from(Alias::new("work_jobs"))
        .and_where(Expr::col(Alias::new("task_kind")).eq(task_kind.as_str()))
        .and_where(Expr::col(Alias::new("state")).is_in([STATE_PENDING, STATE_RUNNING]))
        .to_owned();
    let backend = transaction.get_database_backend();
    let rows = transaction.query_all(backend.build(&query)).await?;
    let parent_ids = rows
        .iter()
        .map(|row| row.try_get::<Uuid>("", "id"))
        .collect::<Result<Vec<_>, DbErr>>()?;
    let mut children = HashMap::<WorkJobId, bool>::new();
    if !parent_ids.is_empty() {
        let child_rows = Query::select()
            .column(Alias::new("payload"))
            .from(Alias::new("work_staging_rows"))
            .and_where(Expr::col(Alias::new("job_id")).is_in(parent_ids.iter().copied()))
            .and_where(Expr::col(Alias::new("entity_kind")).eq("FullScanChild"))
            .to_owned();
        for row in transaction.query_all(backend.build(&child_rows)).await? {
            let payload: Value = row.try_get("", "payload")?;
            let child_id = payload
                .get("job_id")
                .and_then(Value::as_str)
                .and_then(|value| Uuid::parse_str(value).ok())
                .map(WorkJobId::from_uuid)
                .ok_or(WorkJobRepositoryError::InvalidChildReference)?;
            let created = payload
                .get("created")
                .and_then(Value::as_bool)
                .ok_or(WorkJobRepositoryError::InvalidChildReference)?;
            children
                .entry(child_id)
                .and_modify(|owned| *owned |= created)
                .or_insert(created);
        }
    }
    let staging = Alias::new("active_full_parent_staging");
    let parent = Alias::new("active_full_parent");
    let shared_rows = Query::select()
        .expr_as(
            Expr::col((staging.clone(), Alias::new("job_id"))),
            Alias::new("parent_id"),
        )
        .expr_as(
            Expr::col((staging.clone(), Alias::new("payload"))),
            Alias::new("payload"),
        )
        .from_as(Alias::new("work_staging_rows"), staging.clone())
        .join_as(
            JoinType::InnerJoin,
            Alias::new("work_jobs"),
            parent.clone(),
            Expr::col((parent.clone(), Alias::new("id")))
                .equals((staging.clone(), Alias::new("job_id"))),
        )
        .and_where(Expr::col((staging, Alias::new("entity_kind"))).eq("FullScanChild"))
        .and_where(Expr::col((parent, Alias::new("state"))).is_in([STATE_PENDING, STATE_RUNNING]))
        .to_owned();
    let cancelled_parents = parent_ids.iter().copied().collect::<HashSet<_>>();
    let mut externally_shared = HashSet::new();
    for row in transaction.query_all(backend.build(&shared_rows)).await? {
        let parent_id: Uuid = row.try_get("", "parent_id")?;
        if cancelled_parents.contains(&parent_id) {
            continue;
        }
        let payload: Value = row.try_get("", "payload")?;
        if let Some(child_id) = payload
            .get("job_id")
            .and_then(Value::as_str)
            .and_then(|value| Uuid::parse_str(value).ok())
            .map(WorkJobId::from_uuid)
        {
            externally_shared.insert(child_id);
        }
    }
    let mut cancelled = 0_u64;
    for parent_id in parent_ids {
        if cancel_job(transaction, WorkJobId::from_uuid(parent_id), error, now).await? {
            cancelled += 1;
        }
    }
    for (child_id, created) in children {
        if created && !externally_shared.contains(&child_id) {
            cancel_job(transaction, child_id, error, now).await?;
        }
    }
    Ok(cancelled)
}

async fn cancel_job(
    transaction: &DatabaseTransaction,
    job_id: WorkJobId,
    error: &str,
    now: DateTime<Utc>,
) -> Result<bool, WorkJobRepositoryError> {
    let backend = transaction.get_database_backend();
    let mut update = Query::update();
    update
        .table(Alias::new("work_jobs"))
        .value(Alias::new("state"), STATE_FAILED)
        .value(Alias::new("completed_at"), now)
        .value(Alias::new("lease_owner"), Option::<String>::None)
        .value(
            Alias::new("lease_expires_at"),
            Option::<DateTime<Utc>>::None,
        )
        .value(Alias::new("last_error"), error);
    if backend == sea_orm::DbBackend::MySql {
        update.value(Alias::new("active_slot"), Option::<String>::None);
    }
    let update = update
        .and_where(Expr::col(Alias::new("id")).eq(job_id.as_uuid()))
        .and_where(Expr::col(Alias::new("state")).is_in([STATE_PENDING, STATE_RUNNING]))
        .to_owned();
    if transaction
        .execute(backend.build(&update))
        .await?
        .rows_affected()
        != 1
    {
        return Ok(false);
    }
    insert_result(
        transaction,
        job_id,
        WorkJobResult {
            counters: Value::Object(serde_json::Map::new()),
            warnings: Vec::new(),
            error_summary: Some(error.to_owned()),
            result_sync_revision: None,
        },
    )
    .await?;
    Ok(true)
}

async fn enqueue_enabled_library_scans(
    transaction: &DatabaseTransaction,
    priority: i32,
    now: DateTime<Utc>,
) -> Result<Vec<WorkJobSubmission>, WorkJobRepositoryError> {
    let query = Query::select()
        .columns([Alias::new("id"), Alias::new("profile_version")])
        .from(Alias::new("libraries"))
        .and_where(Expr::col(Alias::new("is_enabled")).eq(true))
        .cond_where(
            Cond::any()
                .add(Expr::col(Alias::new("object_selection_scope")).ne("library_roots"))
                .add(Expr::col(Alias::new("metadata_policy")).ne("none"))
                .add(Expr::col(Alias::new("expansion_policy")).is_in(["eager", "background"]))
                .add(Expr::col(Alias::new("probe_policy")).eq("eager")),
        )
        .order_by(Alias::new("sort_key"), Order::Asc)
        .order_by(Alias::new("id"), Order::Asc)
        .to_owned();
    let backend = transaction.get_database_backend();
    let mut submissions = Vec::new();
    for row in transaction.query_all(backend.build(&query)).await? {
        let library_id = LibraryId::from_uuid(row.try_get("", "id")?);
        let revision = i64::from(row.try_get::<i32>("", "profile_version")?);
        let spec = WorkJobSpec::new(
            WorkTaskKind::FullMediaScan,
            WorkScope::Library(library_id),
            revision,
            priority,
        )?;
        submissions.push(enqueue_or_join(transaction, &spec, now).await?);
    }
    Ok(submissions)
}

#[allow(clippy::too_many_lines)] // Validates and inserts the durable natural key atomically.
async fn enqueue_or_join(
    transaction: &DatabaseTransaction,
    spec: &WorkJobSpec,
    now: DateTime<Utc>,
) -> Result<WorkJobSubmission, WorkJobRepositoryError> {
    let now = mysql_compatible_timestamp(transaction.get_database_backend(), now);
    if let Some(dependency) = spec.required_sync_job_id {
        let backend = transaction.get_database_backend();
        let dependency = transaction
            .query_one(backend.build(&job_by_id(dependency)))
            .await?
            .as_ref()
            .map(job_from_row)
            .transpose()?
            .ok_or(WorkJobRepositoryError::MissingDependency)?;
        if dependency.task_kind != WorkTaskKind::ScopedStorageSync
            || !matches!(dependency.scope, WorkScope::StorageObject(_))
            || spec.storage_root_affinity.is_some()
                && dependency.storage_root_affinity != spec.storage_root_affinity
        {
            return Err(WorkJobRepositoryError::InvalidDependency);
        }
        if let Some(input_revision) = spec.input_sync_revision {
            if transaction
                .query_one(backend.build(&ready_sync_dependency(dependency.id, input_revision)))
                .await?
                .is_none()
            {
                return Err(WorkJobRepositoryError::DependencyNotReady);
            }
        } else if !matches!(
            spec.task_kind,
            WorkTaskKind::ExpandItem | WorkTaskKind::IndexMediaSources
        ) {
            return Err(WorkJobRepositoryError::InvalidDependency);
        }
    }
    let id = WorkJobId::new();
    let backend = transaction.get_database_backend();
    let conflict = if backend == sea_orm::DbBackend::MySql {
        OnConflict::new()
            .update_column(Alias::new("task_kind"))
            .to_owned()
    } else {
        OnConflict::new().do_nothing().to_owned()
    };
    let mut columns = vec![
        Alias::new("id"),
        Alias::new("task_kind"),
        Alias::new("scope_type"),
        Alias::new("scope_id"),
        Alias::new("expected_revision"),
        Alias::new("required_sync_job_id"),
        Alias::new("input_sync_revision"),
        Alias::new("state"),
        Alias::new("priority"),
        Alias::new("attempt_count"),
        Alias::new("available_at"),
        Alias::new("created_at"),
        Alias::new("metadata_requirement"),
        Alias::new("metadata_source_mode"),
        Alias::new("local_metadata_access_mode"),
        Alias::new("storage_root_affinity"),
        Alias::new("natural_key_storage_root_id"),
    ];
    let mut values = vec![
        id.as_uuid().into(),
        spec.task_kind.as_str().into(),
        spec.scope.scope_type().into(),
        spec.scope.id().into(),
        spec.expected_revision.into(),
        spec.required_sync_job_id.map(WorkJobId::as_uuid).into(),
        spec.input_sync_revision.into(),
        STATE_PENDING.into(),
        spec.priority.into(),
        0.into(),
        now.into(),
        now.into(),
        spec.metadata_requirement
            .map(MetadataRequirement::as_i32)
            .into(),
        spec.metadata_source_mode
            .map(MetadataSourceMode::as_str)
            .into(),
        spec.local_metadata_access_mode
            .map(LocalMetadataAccessMode::as_str)
            .into(),
        spec.storage_root_affinity
            .map_or_else(Uuid::nil, StorageRootId::as_uuid)
            .into(),
        spec.natural_key_storage_root_id().into(),
    ];
    if backend == sea_orm::DbBackend::MySql {
        columns.push(Alias::new("active_slot"));
        values.push("active".into());
    }
    let mut statement = Query::insert();
    statement
        .into_table(Alias::new("work_jobs"))
        .columns(columns)
        .values_panic(values)
        .on_conflict(conflict);
    transaction.execute(backend.build(&statement)).await?;
    let mut job = transaction
        .query_one(backend.build(&active_job(spec)))
        .await?
        .as_ref()
        .map(job_from_row)
        .transpose()?
        .ok_or(WorkJobRepositoryError::MissingEnqueuedJob)?;
    let created = job.id == id;
    if job.scope != spec.scope
        || job.required_sync_job_id != spec.required_sync_job_id
        || job.input_sync_revision != spec.input_sync_revision
        || job.storage_root_affinity != spec.storage_root_affinity
    {
        return Err(WorkJobRepositoryError::IncompatibleActiveJob);
    }
    if !created && job.state == WorkJobState::Pending && spec.priority > job.priority {
        let promotion = Query::update()
            .table(Alias::new("work_jobs"))
            .value(Alias::new("priority"), spec.priority)
            .and_where(Expr::col(Alias::new("id")).eq(job.id.as_uuid()))
            .and_where(Expr::col(Alias::new("state")).eq(STATE_PENDING))
            .and_where(Expr::col(Alias::new("priority")).lt(spec.priority))
            .to_owned();
        transaction.execute(backend.build(&promotion)).await?;
        job = transaction
            .query_one(backend.build(&job_by_id(job.id)))
            .await?
            .as_ref()
            .map(job_from_row)
            .transpose()?
            .ok_or(WorkJobRepositoryError::MissingEnqueuedJob)?;
    }
    if let Some(requirement) = spec.metadata_requirement
        && job
            .metadata_requirement
            .is_none_or(|current| current < requirement)
    {
        let upgrade = Query::update()
            .table(Alias::new("work_jobs"))
            .value(Alias::new("metadata_requirement"), requirement.as_i32())
            .and_where(Expr::col(Alias::new("id")).eq(job.id.as_uuid()))
            .and_where(Expr::col(Alias::new("state")).is_in([STATE_PENDING, STATE_RUNNING]))
            .and_where(
                Cond::any()
                    .add(Expr::col(Alias::new("metadata_requirement")).is_null())
                    .add(Expr::col(Alias::new("metadata_requirement")).lt(requirement.as_i32()))
                    .into(),
            )
            .to_owned();
        transaction.execute(backend.build(&upgrade)).await?;
        job = transaction
            .query_one(backend.build(&job_by_id(job.id)))
            .await?
            .as_ref()
            .map(job_from_row)
            .transpose()?
            .ok_or(WorkJobRepositoryError::MissingEnqueuedJob)?;
    }
    if spec.metadata_source_mode == Some(MetadataSourceMode::AutomaticScrape)
        && job.metadata_source_mode != Some(MetadataSourceMode::AutomaticScrape)
    {
        let upgrade = Query::update()
            .table(Alias::new("work_jobs"))
            .value(Alias::new("metadata_source_mode"), "automatic_scrape")
            .and_where(Expr::col(Alias::new("id")).eq(job.id.as_uuid()))
            .and_where(Expr::col(Alias::new("state")).is_in([STATE_PENDING, STATE_RUNNING]))
            .to_owned();
        transaction.execute(backend.build(&upgrade)).await?;
        job = transaction
            .query_one(backend.build(&job_by_id(job.id)))
            .await?
            .as_ref()
            .map(job_from_row)
            .transpose()?
            .ok_or(WorkJobRepositoryError::MissingEnqueuedJob)?;
    }
    if let (Some(spec_mode), Some(job_mode)) = (
        spec.local_metadata_access_mode,
        job.local_metadata_access_mode,
    ) {
        let merged_mode = LocalMetadataAccessMode::from_imports(
            spec_mode.imports_metadata() || job_mode.imports_metadata(),
            spec_mode.imports_images() || job_mode.imports_images(),
        );
        if merged_mode == job_mode {
            return Ok(WorkJobSubmission { job, created });
        }
        let upgrade = Query::update()
            .table(Alias::new("work_jobs"))
            .value(
                Alias::new("local_metadata_access_mode"),
                merged_mode.as_str(),
            )
            .and_where(Expr::col(Alias::new("id")).eq(job.id.as_uuid()))
            .and_where(Expr::col(Alias::new("state")).is_in([STATE_PENDING, STATE_RUNNING]))
            .to_owned();
        transaction.execute(backend.build(&upgrade)).await?;
        job = transaction
            .query_one(backend.build(&job_by_id(job.id)))
            .await?
            .as_ref()
            .map(job_from_row)
            .transpose()?
            .ok_or(WorkJobRepositoryError::MissingEnqueuedJob)?;
    }
    Ok(WorkJobSubmission { job, created })
}

pub(crate) async fn enqueue_in_transaction(
    transaction: &DatabaseTransaction,
    spec: &WorkJobSpec,
    now: DateTime<Utc>,
) -> Result<WorkJobSubmission, WorkJobRepositoryError> {
    enqueue_or_join(transaction, spec, now).await
}

async fn claim_next(
    transaction: &DatabaseTransaction,
    accepted_kinds: &[WorkTaskKind],
    storage_scope: Option<(Uuid, Option<&str>)>,
    lease_owner: &str,
    now: DateTime<Utc>,
    lease_expires_at: DateTime<Utc>,
) -> Result<Option<ClaimedWorkJob>, WorkJobRepositoryError> {
    let backend = transaction.get_database_backend();
    let now = mysql_compatible_timestamp(backend, now);
    let lease_expires_at = mysql_compatible_timestamp(backend, lease_expires_at);
    fail_terminal_dependents(transaction, accepted_kinds, now).await?;
    for _ in 0..8 {
        let Some(row) = transaction
            .query_one(backend.build(&claim_candidate(accepted_kinds, storage_scope, now)))
            .await?
        else {
            return Ok(None);
        };
        let mut job = job_from_row(&row)?;
        if job.required_sync_job_id().is_some() && job.input_sync_revision().is_none() {
            let dependency = job
                .required_sync_job_id()
                .ok_or(WorkJobRepositoryError::MissingDependency)?;
            let Some(input_revision) = transaction
                .query_one(backend.build(&ready_sync_dependency_revision(dependency)))
                .await?
                .map(|row| row.try_get("", "result_sync_revision"))
                .transpose()?
            else {
                continue;
            };
            let hydrate = Query::update()
                .table(Alias::new("work_jobs"))
                .value(Alias::new("input_sync_revision"), input_revision)
                .and_where(Expr::col(Alias::new("id")).eq(job.id.as_uuid()))
                .and_where(Expr::col(Alias::new("input_sync_revision")).is_null())
                .cond_where(claimable_condition(now))
                .to_owned();
            if transaction
                .execute(backend.build(&hydrate))
                .await?
                .rows_affected()
                != 1
            {
                continue;
            }
            job.input_sync_revision = Some(input_revision);
        }
        let lease_token = format!("{lease_owner}:{}", Uuid::new_v4());
        let update = Query::update()
            .table(Alias::new("work_jobs"))
            .value(Alias::new("state"), STATE_RUNNING)
            .value(Alias::new("lease_owner"), &lease_token)
            .value(Alias::new("lease_expires_at"), lease_expires_at)
            .value(Alias::new("available_at"), Option::<DateTime<Utc>>::None)
            .value(
                Alias::new("started_at"),
                Expr::col(Alias::new("started_at")).if_null(now),
            )
            .value(
                Alias::new("attempt_count"),
                Expr::col(Alias::new("attempt_count")).add(1),
            )
            .and_where(Expr::col(Alias::new("id")).eq(job.id.as_uuid()))
            .cond_where(claimable_condition(now))
            .to_owned();
        if transaction
            .execute(backend.build(&update))
            .await?
            .rows_affected()
            == 1
        {
            job.state = WorkJobState::Running;
            job.attempt_count = job
                .attempt_count
                .checked_add(1)
                .ok_or(WorkJobRepositoryError::InvalidAttemptCount)?;
            return Ok(Some(ClaimedWorkJob { job, lease_token }));
        }
    }
    Ok(None)
}

async fn fail_terminal_dependents(
    transaction: &DatabaseTransaction,
    accepted_kinds: &[WorkTaskKind],
    now: DateTime<Utc>,
) -> Result<(), WorkJobRepositoryError> {
    let dependent = Alias::new("failed_dependency_job");
    let failed_dependency = Alias::new("failed_dependency");
    let query = Query::select()
        .column((dependent.clone(), Alias::new("id")))
        .from_as(Alias::new("work_jobs"), dependent.clone())
        .and_where(
            Expr::col((dependent.clone(), Alias::new("task_kind")))
                .is_in(accepted_kinds.iter().map(|kind| kind.as_str())),
        )
        .and_where(Expr::col((dependent.clone(), Alias::new("state"))).eq(STATE_PENDING))
        .and_where(Expr::col((dependent.clone(), Alias::new("required_sync_job_id"))).is_not_null())
        .and_where(Expr::exists(
            Query::select()
                .expr(Expr::val(1_i32))
                .from_as(Alias::new("work_jobs"), failed_dependency.clone())
                .and_where(
                    Expr::col((failed_dependency.clone(), Alias::new("id")))
                        .equals((dependent.clone(), Alias::new("required_sync_job_id"))),
                )
                .and_where(Expr::col((failed_dependency, Alias::new("state"))).eq(STATE_FAILED))
                .limit(1)
                .to_owned(),
        ))
        .limit(100)
        .to_owned();
    let rows = transaction
        .query_all(transaction.get_database_backend().build(&query))
        .await?;
    for row in rows {
        let id = WorkJobId::from_uuid(row.try_get("", "id")?);
        let mut update = Query::update();
        update
            .table(Alias::new("work_jobs"))
            .value(Alias::new("state"), STATE_FAILED)
            .value(Alias::new("completed_at"), now)
            .value(
                Alias::new("last_error"),
                "required scoped storage sync failed",
            )
            .value(Alias::new("lease_owner"), Option::<String>::None)
            .value(
                Alias::new("lease_expires_at"),
                Option::<DateTime<Utc>>::None,
            )
            .and_where(Expr::col(Alias::new("id")).eq(id.as_uuid()))
            .and_where(Expr::col(Alias::new("state")).eq(STATE_PENDING));
        if transaction.get_database_backend() == sea_orm::DbBackend::MySql {
            update.value(Alias::new("active_slot"), Option::<String>::None);
        }
        if transaction
            .execute(transaction.get_database_backend().build(&update))
            .await?
            .rows_affected()
            == 1
        {
            insert_result(
                transaction,
                id,
                WorkJobResult {
                    counters: Value::Object(serde_json::Map::new()),
                    warnings: Vec::new(),
                    error_summary: Some("required scoped storage sync failed".to_owned()),
                    result_sync_revision: None,
                },
            )
            .await?;
        }
    }
    Ok(())
}

fn mysql_compatible_timestamp(
    backend: sea_orm::DbBackend,
    timestamp: DateTime<Utc>,
) -> DateTime<Utc> {
    if backend == sea_orm::DbBackend::MySql {
        timestamp.with_nanosecond(0).unwrap_or(timestamp)
    } else {
        timestamp
    }
}

fn validate_lease(
    accepted_kinds: &[WorkTaskKind],
    lease_owner: &str,
    lease_duration: Duration,
) -> Result<(), WorkJobRepositoryError> {
    if accepted_kinds.is_empty() {
        return Err(WorkJobRepositoryError::EmptyAcceptedTaskKinds);
    }
    if lease_owner.trim().is_empty() {
        return Err(WorkJobRepositoryError::EmptyLeaseOwner);
    }
    if lease_owner.chars().count() > MAX_LEASE_OWNER_CHARS {
        return Err(WorkJobRepositoryError::LeaseOwnerTooLong);
    }
    if lease_duration <= Duration::zero() {
        return Err(WorkJobRepositoryError::InvalidLeaseDuration);
    }
    Ok(())
}

async fn fence_full_scan_parent(
    transaction: &DatabaseTransaction,
    claimed: &ClaimedWorkJob,
) -> Result<(), WorkJobRepositoryError> {
    fence_live_claim(transaction, claimed, Utc::now()).await?;
    let library_id = match (claimed.job().task_kind(), claimed.job().scope()) {
        (WorkTaskKind::FullMediaScan, WorkScope::Library(library_id)) => library_id,
        (WorkTaskKind::FullLibraryRootScan, WorkScope::LibraryRootBinding(binding_id)) => {
            let binding_fence = Query::update()
                .table(Alias::new("library_storage_roots"))
                .value(Alias::new("id"), Expr::col(Alias::new("id")))
                .and_where(Expr::col(Alias::new("id")).eq(binding_id.as_uuid()))
                .to_owned();
            if transaction
                .execute(transaction.get_database_backend().build(&binding_fence))
                .await?
                .rows_affected()
                != 1
            {
                return Err(WorkJobRepositoryError::StaleParentPolicy);
            }
            let binding = Query::select()
                .column(Alias::new("library_id"))
                .from(Alias::new("library_storage_roots"))
                .and_where(Expr::col(Alias::new("id")).eq(binding_id.as_uuid()))
                .limit(1)
                .to_owned();
            let row = transaction
                .query_one(transaction.get_database_backend().build(&binding))
                .await?
                .ok_or(WorkJobRepositoryError::StaleParentPolicy)?;
            LibraryId::from_uuid(row.try_get("", "library_id")?)
        }
        _ => return Err(WorkJobRepositoryError::InvalidChildReference),
    };
    let expected_profile_version = i32::try_from(claimed.job().expected_revision())
        .map_err(|_| WorkJobRepositoryError::StaleParentPolicy)?;
    let fence = Query::update()
        .table(Alias::new("libraries"))
        .value(
            Alias::new("profile_version"),
            Expr::col(Alias::new("profile_version")),
        )
        .and_where(Expr::col(Alias::new("id")).eq(library_id.as_uuid()))
        .and_where(Expr::col(Alias::new("profile_version")).eq(expected_profile_version))
        .and_where(Expr::col(Alias::new("is_enabled")).eq(true))
        .to_owned();
    if transaction
        .execute(transaction.get_database_backend().build(&fence))
        .await?
        .rows_affected()
        != 1
    {
        return Err(WorkJobRepositoryError::StaleParentPolicy);
    }
    Ok(())
}

pub(crate) async fn stage_batch(
    transaction: &DatabaseTransaction,
    claimed: &ClaimedWorkJob,
    publication_id: Uuid,
    rows: &[WorkStagingRow],
    now: DateTime<Utc>,
) -> Result<(), WorkJobRepositoryError> {
    ensure_live_claim(transaction, claimed, now).await?;
    let backend = transaction.get_database_backend();
    for row in rows {
        let statement = Query::insert()
            .into_table(Alias::new("work_staging_rows"))
            .columns([
                Alias::new("id"),
                Alias::new("job_id"),
                Alias::new("publication_id"),
                Alias::new("entity_kind"),
                Alias::new("natural_key"),
                Alias::new("payload"),
                Alias::new("validation_state"),
            ])
            .values_panic([
                Uuid::new_v4().into(),
                claimed.id().as_uuid().into(),
                publication_id.into(),
                row.entity_kind.clone().into(),
                row.natural_key.clone().into(),
                row.payload.clone().into(),
                row.validation_state.clone().into(),
            ])
            .on_conflict(
                OnConflict::columns([
                    Alias::new("job_id"),
                    Alias::new("publication_id"),
                    Alias::new("entity_kind"),
                    Alias::new("natural_key"),
                ])
                .update_columns([Alias::new("payload"), Alias::new("validation_state")])
                .to_owned(),
            )
            .to_owned();
        transaction.execute(backend.build(&statement)).await?;
    }
    Ok(())
}

async fn complete_in_transaction(
    transaction: &DatabaseTransaction,
    claimed: &ClaimedWorkJob,
    result: WorkJobResult,
    now: DateTime<Utc>,
) -> Result<(), WorkJobRepositoryError> {
    let backend = transaction.get_database_backend();
    let mut update = Query::update();
    update
        .table(Alias::new("work_jobs"))
        .value(Alias::new("state"), STATE_COMPLETED)
        .value(Alias::new("completed_at"), now)
        .value(Alias::new("lease_owner"), Option::<String>::None)
        .value(
            Alias::new("lease_expires_at"),
            Option::<DateTime<Utc>>::None,
        )
        .value(Alias::new("last_error"), Option::<String>::None);
    if backend == sea_orm::DbBackend::MySql {
        update.value(Alias::new("active_slot"), Option::<String>::None);
    }
    let update = update.cond_where(lease_condition(claimed, now)).to_owned();
    if transaction
        .execute(backend.build(&update))
        .await?
        .rows_affected()
        != 1
    {
        return Err(WorkJobRepositoryError::LostLease);
    }
    insert_result(transaction, claimed.id(), result).await
}

async fn fail_terminal(
    transaction: &DatabaseTransaction,
    claimed: &ClaimedWorkJob,
    error: &str,
    now: DateTime<Utc>,
) -> Result<(), WorkJobRepositoryError> {
    let backend = transaction.get_database_backend();
    let mut update = Query::update();
    update
        .table(Alias::new("work_jobs"))
        .value(Alias::new("state"), STATE_FAILED)
        .value(Alias::new("completed_at"), now)
        .value(Alias::new("lease_owner"), Option::<String>::None)
        .value(
            Alias::new("lease_expires_at"),
            Option::<DateTime<Utc>>::None,
        )
        .value(Alias::new("last_error"), error);
    if backend == sea_orm::DbBackend::MySql {
        update.value(Alias::new("active_slot"), Option::<String>::None);
    }
    let update = update.cond_where(lease_condition(claimed, now)).to_owned();
    if transaction
        .execute(backend.build(&update))
        .await?
        .rows_affected()
        != 1
    {
        return Err(WorkJobRepositoryError::LostLease);
    }
    insert_result(
        transaction,
        claimed.id(),
        WorkJobResult {
            counters: Value::Object(serde_json::Map::new()),
            warnings: Vec::new(),
            error_summary: Some(error.to_owned()),
            result_sync_revision: None,
        },
    )
    .await
}

async fn insert_result(
    transaction: &DatabaseTransaction,
    job_id: WorkJobId,
    result: WorkJobResult,
) -> Result<(), WorkJobRepositoryError> {
    let backend = transaction.get_database_backend();
    let result_insert = Query::insert()
        .into_table(Alias::new("work_results"))
        .columns([
            Alias::new("id"),
            Alias::new("job_id"),
            Alias::new("counters"),
            Alias::new("warnings"),
            Alias::new("error_summary"),
            Alias::new("result_sync_revision"),
        ])
        .values_panic([
            Uuid::new_v4().into(),
            job_id.as_uuid().into(),
            result.counters.into(),
            serde_json::to_value(result.warnings)
                .map_err(|error| DbErr::Custom(error.to_string()))?
                .into(),
            result.error_summary.into(),
            result.result_sync_revision.into(),
        ])
        .to_owned();
    transaction.execute(backend.build(&result_insert)).await?;
    Ok(())
}

pub(crate) async fn ensure_live_claim(
    transaction: &DatabaseTransaction,
    claimed: &ClaimedWorkJob,
    now: DateTime<Utc>,
) -> Result<(), WorkJobRepositoryError> {
    let statement = Query::select()
        .expr(Expr::val(1_i32))
        .from(Alias::new("work_jobs"))
        .cond_where(lease_condition(claimed, now))
        .limit(1)
        .to_owned();
    let backend = transaction.get_database_backend();
    if transaction
        .query_one(backend.build(&statement))
        .await?
        .is_none()
    {
        return Err(WorkJobRepositoryError::LostLease);
    }
    Ok(())
}

pub(crate) async fn fence_live_claim(
    transaction: &DatabaseTransaction,
    claimed: &ClaimedWorkJob,
    now: DateTime<Utc>,
) -> Result<(), WorkJobRepositoryError> {
    let statement = Query::update()
        .table(Alias::new("work_jobs"))
        .value(
            Alias::new("lease_expires_at"),
            Expr::col(Alias::new("lease_expires_at")),
        )
        .cond_where(lease_condition(claimed, now))
        .to_owned();
    let backend = transaction.get_database_backend();
    if transaction
        .execute(backend.build(&statement))
        .await?
        .rows_affected()
        != 1
    {
        return Err(WorkJobRepositoryError::LostLease);
    }
    Ok(())
}

fn claim_candidate(
    accepted_kinds: &[WorkTaskKind],
    storage_scope: Option<(Uuid, Option<&str>)>,
    now: DateTime<Utc>,
) -> SelectStatement {
    let job = Alias::new("job");
    let completed_dependency = completed_sync_dependency_for(&job);
    let mut query = Query::select();
    query
        .from_as(Alias::new("work_jobs"), job.clone())
        .and_where(
            Expr::col((job.clone(), Alias::new("task_kind")))
                .is_in(accepted_kinds.iter().map(|kind| kind.as_str())),
        )
        .cond_where(claimable_condition_for(&job, now))
        .cond_where(
            Cond::any()
                .add(Expr::col((job.clone(), Alias::new("required_sync_job_id"))).is_null())
                .add(Expr::exists(completed_dependency)),
        )
        .order_by((job.clone(), Alias::new("priority")), Order::Desc)
        .order_by((job.clone(), Alias::new("created_at")), Order::Asc)
        .order_by((job.clone(), Alias::new("id")), Order::Asc)
        .limit(1);
    if let Some((account_id, provider_drive_id)) = storage_scope {
        let account = Alias::new("claim_scope_account");
        query.and_where(Expr::exists(
            Query::select()
                .expr(Expr::val(1_i32))
                .from_as(Alias::new("storage_accounts"), account.clone())
                .and_where(Expr::col((account.clone(), Alias::new("id"))).eq(account_id))
                .and_where(Expr::col((account, Alias::new("status"))).eq("Active"))
                .limit(1)
                .to_owned(),
        ));
        let object = Alias::new("claim_scope_object");
        let mut account_scope = Query::select();
        account_scope
            .expr(Expr::val(1_i32))
            .from_as(Alias::new("storage_objects"), object.clone())
            .and_where(
                Expr::col((object.clone(), Alias::new("id")))
                    .equals((job.clone(), Alias::new("scope_id"))),
            )
            .and_where(
                Expr::col((object.clone(), Alias::new("storage_account_id"))).eq(account_id),
            );
        if let Some(provider_drive_id) = provider_drive_id {
            account_scope.and_where(
                Expr::col((object, Alias::new("provider_drive_id"))).eq(provider_drive_id),
            );
        }
        let root = Alias::new("claim_scope_root");
        let relation = Alias::new("claim_scope_root_relation");
        let root_object = Alias::new("claim_scope_root_object");
        let mut root_scope = Query::select();
        root_scope
            .expr(Expr::val(1_i32))
            .from_as(Alias::new("storage_roots"), root.clone())
            .join_as(
                sea_orm::sea_query::JoinType::InnerJoin,
                Alias::new("storage_root_objects"),
                relation.clone(),
                Expr::col((relation.clone(), Alias::new("storage_root_id")))
                    .equals((root.clone(), Alias::new("id"))),
            )
            .join_as(
                sea_orm::sea_query::JoinType::InnerJoin,
                Alias::new("storage_objects"),
                root_object.clone(),
                Expr::col((root_object.clone(), Alias::new("id")))
                    .equals((relation.clone(), Alias::new("storage_object_id"))),
            )
            .and_where(
                Expr::col((root.clone(), Alias::new("id")))
                    .equals((job.clone(), Alias::new("scope_id"))),
            )
            .and_where(Expr::col((root, Alias::new("storage_account_id"))).eq(account_id))
            .and_where(Expr::col((relation, Alias::new("parent_storage_object_id"))).is_null());
        if let Some(provider_drive_id) = provider_drive_id {
            root_scope.and_where(
                Expr::col((root_object, Alias::new("provider_drive_id"))).eq(provider_drive_id),
            );
        }
        query.and_where(
            Cond::any()
                .add(
                    Cond::all()
                        .add(Expr::col((job.clone(), Alias::new("scope_type"))).eq("StorageObject"))
                        .add(Expr::exists(account_scope)),
                )
                .add(
                    Cond::all()
                        .add(Expr::col((job.clone(), Alias::new("scope_type"))).eq("StorageRoot"))
                        .add(Expr::exists(root_scope)),
                )
                .into(),
        );
    }
    select_job_columns(&mut query, &job);
    query.clone()
}

#[allow(clippy::too_many_lines)] // Completion requires the job, result, page, root, and scope fences in one query.
fn completed_sync_dependency_for(job: &Alias) -> SelectStatement {
    let dependency = Alias::new("dependency");
    let dependency_result = Alias::new("dependency_result");
    let sync_page = Alias::new("sync_page");
    let sync_root = Alias::new("sync_root");
    let scope_state = Alias::new("scope_state");
    Query::select()
        .expr(Expr::val(1_i32))
        .from_as(Alias::new("work_jobs"), dependency.clone())
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("work_results"),
            dependency_result.clone(),
            Expr::col((dependency_result.clone(), Alias::new("job_id")))
                .equals((dependency.clone(), Alias::new("id"))),
        )
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("storage_sync_pages"),
            sync_page.clone(),
            Expr::col((sync_page.clone(), Alias::new("job_id")))
                .equals((dependency.clone(), Alias::new("id"))),
        )
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("storage_roots"),
            sync_root.clone(),
            Expr::col((sync_root.clone(), Alias::new("id")))
                .equals((sync_page.clone(), Alias::new("storage_root_id"))),
        )
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("storage_root_objects"),
            scope_state.clone(),
            Cond::all()
                .add(
                    Expr::col((scope_state.clone(), Alias::new("storage_root_id")))
                        .equals((sync_root.clone(), Alias::new("id"))),
                )
                .add(
                    Expr::col((scope_state.clone(), Alias::new("storage_object_id")))
                        .equals((sync_page.clone(), Alias::new("scope_storage_object_id"))),
                ),
        )
        .and_where(
            Expr::col((dependency.clone(), Alias::new("id")))
                .equals((job.clone(), Alias::new("required_sync_job_id"))),
        )
        .and_where(Expr::col((dependency.clone(), Alias::new("state"))).eq(STATE_COMPLETED))
        .and_where(
            Expr::col((dependency.clone(), Alias::new("task_kind")))
                .eq(WorkTaskKind::ScopedStorageSync.as_str()),
        )
        .and_where(Expr::col((dependency.clone(), Alias::new("scope_type"))).eq("StorageObject"))
        .and_where(
            Expr::col((sync_page.clone(), Alias::new("scope_storage_object_id")))
                .equals((dependency.clone(), Alias::new("scope_id"))),
        )
        .cond_where(
            Cond::any()
                .add(
                    Expr::col((dependency.clone(), Alias::new("storage_root_affinity")))
                        .eq(Uuid::nil()),
                )
                .add(
                    Expr::col((dependency.clone(), Alias::new("storage_root_affinity")))
                        .equals((sync_page.clone(), Alias::new("storage_root_id"))),
                ),
        )
        .cond_where(
            Cond::any()
                .add(Expr::col((job.clone(), Alias::new("storage_root_affinity"))).eq(Uuid::nil()))
                .add(
                    Expr::col((job.clone(), Alias::new("storage_root_affinity")))
                        .equals((sync_page.clone(), Alias::new("storage_root_id"))),
                ),
        )
        .and_where(
            Cond::any()
                .add(
                    Expr::col((
                        dependency_result.clone(),
                        Alias::new("result_sync_revision"),
                    ))
                    .equals((job.clone(), Alias::new("input_sync_revision"))),
                )
                .add(Expr::col((job.clone(), Alias::new("input_sync_revision"))).is_null())
                .into(),
        )
        .and_where(Expr::col((sync_page.clone(), Alias::new("scope_completed"))).eq(true))
        .and_where(Expr::col((sync_page, Alias::new("sync_revision"))).equals((
            dependency_result.clone(),
            Alias::new("result_sync_revision"),
        )))
        .and_where(
            Expr::col((sync_root, Alias::new("reconciled_sync_revision"))).gte(Expr::col((
                dependency_result.clone(),
                Alias::new("result_sync_revision"),
            ))),
        )
        .and_where(Expr::col((scope_state.clone(), Alias::new("children_indexed"))).eq(true))
        .and_where(
            Expr::col((scope_state.clone(), Alias::new("children_index_revision"))).gte(Expr::col(
                (dependency_result, Alias::new("result_sync_revision")),
            )),
        )
        .and_where(Expr::col((scope_state, Alias::new("presence_state"))).eq("Present"))
        .to_owned()
}

fn ready_sync_dependency_revision(job_id: WorkJobId) -> SelectStatement {
    let dependency = Alias::new("ready_dependency");
    let result = Alias::new("ready_dependency_result");
    let page = Alias::new("ready_sync_page");
    let root = Alias::new("ready_sync_root");
    let scope = Alias::new("ready_sync_scope");
    Query::select()
        .expr_as(
            Expr::col((result.clone(), Alias::new("result_sync_revision"))),
            Alias::new("result_sync_revision"),
        )
        .from_as(Alias::new("work_jobs"), dependency.clone())
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("work_results"),
            result.clone(),
            Expr::col((result.clone(), Alias::new("job_id")))
                .equals((dependency.clone(), Alias::new("id"))),
        )
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("storage_sync_pages"),
            page.clone(),
            Expr::col((page.clone(), Alias::new("job_id")))
                .equals((dependency.clone(), Alias::new("id"))),
        )
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("storage_roots"),
            root.clone(),
            Expr::col((root.clone(), Alias::new("id")))
                .equals((page.clone(), Alias::new("storage_root_id"))),
        )
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("storage_root_objects"),
            scope.clone(),
            Cond::all()
                .add(
                    Expr::col((scope.clone(), Alias::new("storage_root_id")))
                        .equals((root.clone(), Alias::new("id"))),
                )
                .add(
                    Expr::col((scope.clone(), Alias::new("storage_object_id")))
                        .equals((page.clone(), Alias::new("scope_storage_object_id"))),
                ),
        )
        .and_where(Expr::col((dependency.clone(), Alias::new("id"))).eq(job_id.as_uuid()))
        .and_where(Expr::col((dependency.clone(), Alias::new("state"))).eq(STATE_COMPLETED))
        .and_where(
            Expr::col((dependency.clone(), Alias::new("task_kind")))
                .eq(WorkTaskKind::ScopedStorageSync.as_str()),
        )
        .and_where(Expr::col((dependency.clone(), Alias::new("scope_type"))).eq("StorageObject"))
        .and_where(
            Expr::col((dependency.clone(), Alias::new("scope_id")))
                .equals((page.clone(), Alias::new("scope_storage_object_id"))),
        )
        .cond_where(
            Cond::any()
                .add(
                    Expr::col((dependency.clone(), Alias::new("storage_root_affinity")))
                        .eq(Uuid::nil()),
                )
                .add(
                    Expr::col((dependency, Alias::new("storage_root_affinity")))
                        .equals((page.clone(), Alias::new("storage_root_id"))),
                ),
        )
        .and_where(Expr::col((page.clone(), Alias::new("scope_completed"))).eq(true))
        .and_where(
            Expr::col((page.clone(), Alias::new("sync_revision")))
                .equals((result.clone(), Alias::new("result_sync_revision"))),
        )
        .and_where(
            Expr::col((root.clone(), Alias::new("reconciled_sync_revision"))).gte(Expr::col((
                result.clone(),
                Alias::new("result_sync_revision"),
            ))),
        )
        .and_where(Expr::col((scope.clone(), Alias::new("children_indexed"))).eq(true))
        .and_where(
            Expr::col((scope.clone(), Alias::new("children_index_revision")))
                .gte(Expr::col((result, Alias::new("result_sync_revision")))),
        )
        .and_where(Expr::col((scope, Alias::new("presence_state"))).eq("Present"))
        .limit(1)
        .to_owned()
}

fn ready_sync_dependency(job_id: WorkJobId, sync_revision: i64) -> SelectStatement {
    let dependency = Alias::new("dependency");
    let result = Alias::new("result");
    let page = Alias::new("page");
    let root = Alias::new("root");
    let scope = Alias::new("scope");
    Query::select()
        .expr(Expr::val(1_i32))
        .from_as(Alias::new("work_jobs"), dependency.clone())
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("work_results"),
            result.clone(),
            Expr::col((result.clone(), Alias::new("job_id")))
                .equals((dependency.clone(), Alias::new("id"))),
        )
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("storage_sync_pages"),
            page.clone(),
            Expr::col((page.clone(), Alias::new("job_id")))
                .equals((dependency.clone(), Alias::new("id"))),
        )
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("storage_roots"),
            root.clone(),
            Expr::col((root.clone(), Alias::new("id")))
                .equals((page.clone(), Alias::new("storage_root_id"))),
        )
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("storage_root_objects"),
            scope.clone(),
            Cond::all()
                .add(
                    Expr::col((scope.clone(), Alias::new("storage_root_id")))
                        .equals((root.clone(), Alias::new("id"))),
                )
                .add(
                    Expr::col((scope.clone(), Alias::new("storage_object_id")))
                        .equals((page.clone(), Alias::new("scope_storage_object_id"))),
                ),
        )
        .and_where(Expr::col((dependency.clone(), Alias::new("id"))).eq(job_id.as_uuid()))
        .and_where(Expr::col((dependency.clone(), Alias::new("state"))).eq(STATE_COMPLETED))
        .and_where(
            Expr::col((dependency.clone(), Alias::new("task_kind")))
                .eq(WorkTaskKind::ScopedStorageSync.as_str()),
        )
        .and_where(Expr::col((dependency.clone(), Alias::new("scope_type"))).eq("StorageObject"))
        .and_where(
            Expr::col((dependency.clone(), Alias::new("scope_id")))
                .equals((page.clone(), Alias::new("scope_storage_object_id"))),
        )
        .cond_where(
            Cond::any()
                .add(
                    Expr::col((dependency.clone(), Alias::new("storage_root_affinity")))
                        .eq(Uuid::nil()),
                )
                .add(
                    Expr::col((dependency, Alias::new("storage_root_affinity")))
                        .equals((page.clone(), Alias::new("storage_root_id"))),
                ),
        )
        .and_where(Expr::col((result, Alias::new("result_sync_revision"))).eq(sync_revision))
        .and_where(Expr::col((page.clone(), Alias::new("sync_revision"))).eq(sync_revision))
        .and_where(Expr::col((page, Alias::new("scope_completed"))).eq(true))
        .and_where(Expr::col((root, Alias::new("reconciled_sync_revision"))).gte(sync_revision))
        .and_where(Expr::col((scope.clone(), Alias::new("children_indexed"))).eq(true))
        .and_where(
            Expr::col((scope.clone(), Alias::new("children_index_revision"))).gte(sync_revision),
        )
        .and_where(Expr::col((scope, Alias::new("presence_state"))).eq("Present"))
        .limit(1)
        .to_owned()
}

fn claimable_condition(now: DateTime<Utc>) -> Cond {
    claimable_condition_for(&Alias::new("work_jobs"), now)
}

fn claimable_condition_for(table: &Alias, now: DateTime<Utc>) -> Cond {
    Cond::any()
        .add(
            Cond::all()
                .add(Expr::col((table.clone(), Alias::new("state"))).eq(STATE_PENDING))
                .add(
                    Cond::any()
                        .add(Expr::col((table.clone(), Alias::new("available_at"))).is_null())
                        .add(Expr::col((table.clone(), Alias::new("available_at"))).lte(now)),
                ),
        )
        .add(
            Cond::all()
                .add(Expr::col((table.clone(), Alias::new("state"))).eq(STATE_RUNNING))
                .add(Expr::col((table.clone(), Alias::new("lease_expires_at"))).lte(now)),
        )
}

fn lease_condition(claimed: &ClaimedWorkJob, now: DateTime<Utc>) -> Cond {
    Cond::all()
        .add(Expr::col(Alias::new("id")).eq(claimed.id().as_uuid()))
        .add(Expr::col(Alias::new("state")).eq(STATE_RUNNING))
        .add(Expr::col(Alias::new("lease_owner")).eq(&claimed.lease_token))
        .add(Expr::col(Alias::new("lease_expires_at")).gt(now))
}

fn active_job(spec: &WorkJobSpec) -> SelectStatement {
    let table = Alias::new("work_jobs");
    let mut query = Query::select();
    query
        .from(table.clone())
        .and_where(Expr::col(Alias::new("scope_id")).eq(spec.scope.id()))
        .and_where(Expr::col(Alias::new("task_kind")).eq(spec.task_kind.as_str()))
        .and_where(Expr::col(Alias::new("expected_revision")).eq(spec.expected_revision))
        .and_where(
            Expr::col(Alias::new("natural_key_storage_root_id"))
                .eq(spec.natural_key_storage_root_id()),
        )
        .and_where(Expr::col(Alias::new("state")).is_in([STATE_PENDING, STATE_RUNNING]))
        .limit(1);
    select_job_columns(&mut query, &table);
    query.clone()
}

fn job_by_id(job_id: WorkJobId) -> SelectStatement {
    let table = Alias::new("work_jobs");
    let mut query = Query::select();
    query
        .from(table.clone())
        .and_where(Expr::col(Alias::new("id")).eq(job_id.as_uuid()))
        .limit(1);
    select_job_columns(&mut query, &table);
    query.clone()
}

fn select_job_columns(query: &mut SelectStatement, table: &Alias) {
    for column in [
        "id",
        "task_kind",
        "scope_type",
        "scope_id",
        "expected_revision",
        "required_sync_job_id",
        "input_sync_revision",
        "state",
        "priority",
        "attempt_count",
        "metadata_requirement",
        "metadata_source_mode",
        "local_metadata_access_mode",
        "storage_root_affinity",
    ] {
        query.expr_as(
            Expr::col((table.clone(), Alias::new(column))),
            Alias::new(column),
        );
    }
}

fn job_from_row(row: &QueryResult) -> Result<WorkJobRecord, WorkJobRepositoryError> {
    let id: Uuid = row.try_get("", "id")?;
    let task_kind: String = row.try_get("", "task_kind")?;
    let scope_type: String = row.try_get("", "scope_type")?;
    let scope_id: Uuid = row.try_get("", "scope_id")?;
    let state: String = row.try_get("", "state")?;
    let required_sync_job_id = row
        .try_get::<Option<Uuid>>("", "required_sync_job_id")?
        .map(WorkJobId::from_uuid);
    let attempt_count: i32 = row.try_get("", "attempt_count")?;
    if attempt_count < 0 {
        return Err(WorkJobRepositoryError::InvalidAttemptCount);
    }
    let metadata_requirement = row
        .try_get::<Option<i32>>("", "metadata_requirement")?
        .map(MetadataRequirement::from_database)
        .transpose()?;
    let metadata_source_mode = row
        .try_get::<Option<String>>("", "metadata_source_mode")?
        .map(|value| {
            value
                .parse()
                .map_err(|_| WorkJobRepositoryError::InvalidStoredMetadataSourceMode)
        })
        .transpose()?;
    let local_metadata_access_mode = row
        .try_get::<Option<String>>("", "local_metadata_access_mode")?
        .map(|value| {
            value
                .parse()
                .map_err(|_| WorkJobRepositoryError::InvalidStoredMetadataSourceMode)
        })
        .transpose()?;
    let storage_root_affinity = row.try_get::<Uuid>("", "storage_root_affinity")?;
    let storage_root_affinity = if storage_root_affinity.is_nil() {
        None
    } else {
        Some(StorageRootId::from_uuid(storage_root_affinity))
    };
    if (task_kind == "ResolveMetadata") != metadata_requirement.is_some() {
        return Err(WorkJobRepositoryError::InvalidStoredMetadataRequirement);
    }
    if (task_kind == "ResolveMetadata") != metadata_source_mode.is_some() {
        return Err(WorkJobRepositoryError::InvalidStoredMetadataSourceMode);
    }
    if (task_kind == "ResolveMetadata") != local_metadata_access_mode.is_some() {
        return Err(WorkJobRepositoryError::InvalidStoredMetadataSourceMode);
    }
    Ok(WorkJobRecord {
        id: WorkJobId::from_uuid(id),
        task_kind: WorkTaskKind::from_database(&task_kind)?,
        scope: WorkScope::from_database(&scope_type, scope_id)?,
        expected_revision: row.try_get("", "expected_revision")?,
        required_sync_job_id,
        input_sync_revision: row.try_get("", "input_sync_revision")?,
        state: WorkJobState::from_database(&state)?,
        priority: row.try_get("", "priority")?,
        attempt_count,
        metadata_requirement,
        metadata_source_mode,
        local_metadata_access_mode,
        storage_root_affinity,
    })
}

fn admin_job_from_row(row: &QueryResult) -> Result<WorkJobAdminRecord, WorkJobRepositoryError> {
    let job = job_from_row(row)?;
    let last_error: Option<String> = row.try_get("", "last_error")?;
    let admin_status = match job.state() {
        WorkJobState::Pending if last_error.is_some() => WorkJobAdminStatus::Retrying,
        WorkJobState::Pending => WorkJobAdminStatus::Pending,
        WorkJobState::Running => WorkJobAdminStatus::Running,
        WorkJobState::Completed => WorkJobAdminStatus::Completed,
        WorkJobState::Failed if last_error.as_deref() == Some(ADMIN_CANCELLED_ERROR) => {
            WorkJobAdminStatus::Cancelled
        }
        WorkJobState::Failed => WorkJobAdminStatus::Failed,
    };
    let counters: Option<Value> = row.try_get("", "result_counters")?;
    let warnings: Option<Value> = row.try_get("", "result_warnings")?;
    let has_warnings = warnings
        .as_ref()
        .and_then(Value::as_array)
        .is_some_and(|warnings| !warnings.is_empty());
    let matched = counters
        .as_ref()
        .and_then(|value| value.get("matched"))
        .and_then(Value::as_bool);
    let partial = counters
        .as_ref()
        .and_then(|value| value.get("state"))
        .and_then(Value::as_str)
        == Some("Partial");
    let outcome = if admin_status == WorkJobAdminStatus::Completed && has_warnings {
        Some(WorkJobAdminOutcome::CompletedWithWarnings)
    } else if admin_status == WorkJobAdminStatus::Completed
        && job.task_kind() == WorkTaskKind::ResolveMetadata
        && job.metadata_source_mode() == Some(MetadataSourceMode::AutomaticScrape)
        && matched != Some(true)
        && partial
    {
        Some(WorkJobAdminOutcome::NoMetadataMatch)
    } else {
        None
    };
    Ok(WorkJobAdminRecord {
        job,
        admin_status,
        created_at: row.try_get("", "created_at")?,
        started_at: row.try_get("", "started_at")?,
        completed_at: row.try_get("", "completed_at")?,
        outcome,
    })
}

async fn finish<T>(
    transaction: DatabaseTransaction,
    result: Result<T, WorkJobRepositoryError>,
) -> Result<T, WorkJobRepositoryError> {
    match result {
        Ok(value) => {
            transaction.commit().await?;
            Ok(value)
        }
        Err(original) => match transaction.rollback().await {
            Ok(()) => Err(original),
            Err(rollback) => Err(WorkJobRepositoryError::RollbackFailed {
                original: original.to_string(),
                rollback,
            }),
        },
    }
}
