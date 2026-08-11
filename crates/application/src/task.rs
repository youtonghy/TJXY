use sea_orm::DatabaseConnection;
use thiserror::Error;
use tjxy_common::{CatalogItemId, LibraryId, StorageRootId, UserId};
use tjxy_db::{
    ADMIN_CANCELLED_ERROR, CatalogItemType, CatalogQueryError, CatalogQueryRepository,
    DiscoverTitlesError, DiscoverTitlesRepository, FullScanRepository, FullScanRepositoryError,
    ManualProbeError, ManualProbeRepository, ManualProbeSubmission, MetadataWorkError,
    MetadataWorkRepository, StorageSyncRepository, StorageSyncRepositoryError, WorkJobAdminRecord,
    WorkJobRepository, WorkJobRepositoryError, WorkJobSpec, WorkJobSubmission, WorkScope,
    WorkTaskKind,
};

const MANUAL_REFRESH_PRIORITY: i32 = 20;
const MANUAL_MEDIA_PRIORITY: i32 = 100;
const SCHEDULED_REFRESH_PRIORITY: i32 = 0;
const MAX_MANUAL_PROBE_SOURCES: usize = 256;

/// Application boundary for durable administrator and scheduled work.
pub struct TaskService {
    database: DatabaseConnection,
}

impl TaskService {
    #[must_use]
    pub const fn new(database: DatabaseConnection) -> Self {
        Self { database }
    }

    /// Enqueues or joins a policy-aware media scan for each enabled Library with automatic work.
    ///
    /// # Errors
    ///
    /// Returns [`TaskServiceError`] when the durable work batch cannot be committed.
    pub async fn refresh_libraries(&self) -> Result<Vec<WorkJobSubmission>, TaskServiceError> {
        WorkJobRepository::new(&self.database)
            .enqueue_enabled_library_scans(MANUAL_REFRESH_PRIORITY)
            .await
            .map_err(Into::into)
    }

    /// Enqueues or joins the lowest-priority policy-aware scan for every enabled Library.
    ///
    /// # Errors
    ///
    /// Returns [`TaskServiceError`] when the durable work batch cannot be committed.
    pub async fn schedule_periodic_library_refresh(
        &self,
    ) -> Result<Vec<WorkJobSubmission>, TaskServiceError> {
        WorkJobRepository::new(&self.database)
            .enqueue_enabled_library_scans(SCHEDULED_REFRESH_PRIORITY)
            .await
            .map_err(Into::into)
    }

    /// Reports whether the full media scan task has pending or running work.
    ///
    /// # Errors
    ///
    /// Returns [`TaskServiceError`] when task state cannot be read.
    pub async fn full_media_scan_active(&self) -> Result<bool, TaskServiceError> {
        WorkJobRepository::new(&self.database)
            .has_active_task(WorkTaskKind::FullMediaScan)
            .await
            .map_err(Into::into)
    }

    /// Cancels all pending and running full media scan jobs.
    ///
    /// # Errors
    ///
    /// Returns [`TaskServiceError`] when the cancellation transaction fails.
    pub async fn cancel_full_media_scan(&self) -> Result<u64, TaskServiceError> {
        WorkJobRepository::new(&self.database)
            .cancel_active_task(WorkTaskKind::FullMediaScan, ADMIN_CANCELLED_ERROR)
            .await
            .map_err(Into::into)
    }

    /// Returns a bounded, newest-first and credential-safe view of durable work.
    ///
    /// # Errors
    ///
    /// Returns [`TaskServiceError`] when the limit or SQL read is invalid.
    pub async fn recent_jobs(
        &self,
        limit: u64,
    ) -> Result<Vec<WorkJobAdminRecord>, TaskServiceError> {
        WorkJobRepository::new(&self.database)
            .recent_jobs(limit)
            .await
            .map_err(Into::into)
    }

    /// Enqueues explicit root-scoped title discovery.
    ///
    /// # Errors
    ///
    /// Returns [`TaskServiceError`] when the root is unavailable or already current.
    pub async fn discover_titles(
        &self,
        root_id: StorageRootId,
    ) -> Result<WorkJobSubmission, TaskServiceError> {
        DiscoverTitlesRepository::new(&self.database)
            .enqueue(root_id, MANUAL_REFRESH_PRIORITY)
            .await
            .map_err(Into::into)
    }

    /// Enqueues explicit recursive validation for one live storage root.
    ///
    /// # Errors
    ///
    /// Returns [`TaskServiceError`] when the root is unavailable or work cannot be enqueued.
    pub async fn validate_storage(
        &self,
        root_id: StorageRootId,
    ) -> Result<WorkJobSubmission, TaskServiceError> {
        StorageSyncRepository::new(&self.database)
            .enqueue_validation(root_id, MANUAL_REFRESH_PRIORITY)
            .await
            .map_err(Into::into)
    }

    /// Enqueues or joins an explicit Full scan for one Library-root binding.
    ///
    /// # Errors
    ///
    /// Returns [`TaskServiceError`] when the binding is unavailable or durable work
    /// cannot be committed.
    pub async fn full_scan_root(
        &self,
        library_id: LibraryId,
        root_id: StorageRootId,
    ) -> Result<WorkJobSubmission, TaskServiceError> {
        FullScanRepository::new(&self.database)
            .enqueue_root_scan(library_id, root_id, MANUAL_REFRESH_PRIORITY)
            .await
            .map_err(Into::into)
    }

    /// Enqueues explicit CatalogItem-scoped metadata resolution.
    ///
    /// # Errors
    ///
    /// Returns [`TaskServiceError`] when the item has no authorized reconciled scope.
    pub async fn resolve_metadata(
        &self,
        item_id: CatalogItemId,
    ) -> Result<WorkJobSubmission, TaskServiceError> {
        MetadataWorkRepository::new(&self.database)
            .enqueue(item_id, MANUAL_REFRESH_PRIORITY)
            .await
            .map_err(Into::into)
    }

    /// Enqueues an explicit re-probe for every available active source of one visible item.
    ///
    /// This command deliberately does not index missing sources. Administrators can
    /// therefore run and retry the Probe stage without broadening its requested scope.
    ///
    /// # Errors
    ///
    /// Returns [`TaskServiceError`] when the item is unavailable, has no probeable
    /// active sources, or the durable batch cannot be committed.
    pub async fn probe_media(
        &self,
        item_id: CatalogItemId,
    ) -> Result<Vec<ManualProbeSubmission>, TaskServiceError> {
        ManualProbeRepository::new(&self.database)
            .enqueue_item(item_id, MANUAL_MEDIA_PRIORITY, MAX_MANUAL_PROBE_SOURCES)
            .await
            .map_err(Into::into)
    }

    /// Enqueues an explicit Series structure expansion, including durable sync-first work.
    ///
    /// # Errors
    ///
    /// Returns [`TaskServiceError`] when the item is unavailable, is not a Series, has no
    /// unambiguous storage scope, or the durable work cannot be enqueued.
    pub async fn expand_item(
        &self,
        principal: UserId,
        item_id: CatalogItemId,
    ) -> Result<WorkJobSubmission, TaskServiceError> {
        self.enqueue_media_stage(principal, item_id, WorkTaskKind::ExpandItem)
            .await
    }

    /// Enqueues an explicit Movie or Episode source re-index, including sync-first work.
    ///
    /// # Errors
    ///
    /// Returns [`TaskServiceError`] when the item is unavailable, has an incompatible type,
    /// has no unambiguous storage scope, or the durable work cannot be enqueued.
    pub async fn index_media_sources(
        &self,
        principal: UserId,
        item_id: CatalogItemId,
    ) -> Result<WorkJobSubmission, TaskServiceError> {
        self.enqueue_media_stage(principal, item_id, WorkTaskKind::IndexMediaSources)
            .await
    }

    async fn enqueue_media_stage(
        &self,
        principal: UserId,
        item_id: CatalogItemId,
        task_kind: WorkTaskKind,
    ) -> Result<WorkJobSubmission, TaskServiceError> {
        let target = CatalogQueryRepository::new(&self.database)
            .lazy_work_target(principal, item_id)
            .await?
            .ok_or(TaskServiceError::ManualMediaItemUnavailable)?;
        let revision = match (task_kind, target.item_type()) {
            (WorkTaskKind::ExpandItem, CatalogItemType::Series) => target.structure_revision(),
            (
                WorkTaskKind::IndexMediaSources,
                CatalogItemType::Movie | CatalogItemType::Episode | CatalogItemType::Audio,
            ) => target.source_revision(),
            _ => return Err(TaskServiceError::InvalidManualMediaItemType),
        };
        let scope = target
            .storage_scope()
            .ok_or(TaskServiceError::ManualMediaItemUnavailable)?;
        let jobs = WorkJobRepository::new(&self.database);
        let direct_audio = task_kind == WorkTaskKind::IndexMediaSources
            && target.item_type() == CatalogItemType::Audio;
        let spec = if scope.is_ready() || (direct_audio && scope.is_ready_for_direct_source()) {
            WorkJobSpec::new(
                task_kind,
                WorkScope::CatalogItem(item_id),
                revision,
                MANUAL_MEDIA_PRIORITY,
            )?
            .with_input_sync_revision(if direct_audio {
                scope.metadata_input_revision()
            } else {
                scope.children_revision()
            })?
        } else {
            let sync = jobs
                .enqueue_or_join(
                    &WorkJobSpec::new(
                        WorkTaskKind::ScopedStorageSync,
                        WorkScope::StorageObject(scope.storage_object_id()),
                        scope.children_revision(),
                        MANUAL_MEDIA_PRIORITY,
                    )?
                    .with_storage_root_affinity(scope.storage_root_id())?,
                )
                .await?;
            WorkJobSpec::new(
                task_kind,
                WorkScope::CatalogItem(item_id),
                revision,
                MANUAL_MEDIA_PRIORITY,
            )?
            .with_pending_required_sync(sync.job().id())
        }
        .with_storage_root_affinity(scope.storage_root_id())?;
        jobs.enqueue_or_join(&spec).await.map_err(Into::into)
    }
}

#[derive(Debug, Error)]
pub enum TaskServiceError {
    #[error("manual media task item is unavailable")]
    ManualMediaItemUnavailable,
    #[error("manual media task is incompatible with the catalog item type")]
    InvalidManualMediaItemType,
    #[error("manual media task catalog query failed: {0}")]
    Catalog(#[from] CatalogQueryError),
    #[error("manual Probe task is unavailable: {0}")]
    Probe(#[from] ManualProbeError),
    #[error("durable task operation failed: {0}")]
    Repository(#[from] WorkJobRepositoryError),
    #[error("title discovery task is unavailable: {0}")]
    Discover(#[from] DiscoverTitlesError),
    #[error("storage validation task is unavailable: {0}")]
    Validation(#[from] StorageSyncRepositoryError),
    #[error("metadata task is unavailable: {0}")]
    Metadata(#[from] MetadataWorkError),
    #[error("root Full scan task is unavailable: {0}")]
    FullScan(#[from] FullScanRepositoryError),
}
