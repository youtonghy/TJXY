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
    health_cache: tokio::sync::Mutex<Option<(tokio::time::Instant, tjxy_db::WorkHealth)>>,
    retention: Option<chrono::Duration>,
}

impl TaskService {
    #[must_use]
    pub fn with_history_retention(mut self, retention: Option<std::time::Duration>) -> Self {
        self.retention = retention.and_then(|duration| chrono::Duration::from_std(duration).ok());
        self
    }
    /// Returns a coalesced, five-minute cached queue and storage-space snapshot.
    /// # Errors
    /// Returns database or timeout errors without replacing the previous sample.
    pub async fn work_health(&self) -> Result<tjxy_db::WorkHealth, TaskServiceError> {
        let mut cache = self.health_cache.lock().await;
        if let Some((at, value)) = cache.as_ref() {
            if at.elapsed() < std::time::Duration::from_secs(300) {
                return Ok(value.clone());
            }
        }
        let sampled = tokio::time::timeout(
            std::time::Duration::from_secs(15),
            tjxy_db::sample_work_health(&self.database, self.retention),
        )
        .await
        .map_err(|_| {
            TaskServiceError::Diagnostics(sea_orm::DbErr::Custom(
                "diagnostic sampling timed out".into(),
            ))
        })??;
        *cache = Some((tokio::time::Instant::now(), sampled.clone()));
        Ok(sampled)
    }

    /// Reads one bounded page of actionable NFO choices.
    ///
    /// # Errors
    /// Returns persistence failures.
    pub async fn nfo_choices(
        &self,
        offset: u64,
    ) -> Result<Vec<tjxy_db::NfoChoiceInfo>, TaskServiceError> {
        Ok(MetadataWorkRepository::new(&self.database)
            .nfo_choices(offset)
            .await?)
    }

    /// Saves an administrator's source choice and submits current-revision metadata work.
    ///
    /// # Errors
    /// Returns stale-choice, policy, or persistence failures.
    pub async fn choose_nfo(
        &self,
        item: CatalogItemId,
        root: StorageRootId,
        candidate: uuid::Uuid,
        fingerprint: &str,
    ) -> Result<WorkJobSubmission, TaskServiceError> {
        MetadataWorkRepository::new(&self.database)
            .choose_nfo(item, root, candidate, fingerprint)
            .await
            .map_err(|error| match error {
                sea_orm::DbErr::Custom(_) => TaskServiceError::StaleDiagnostic,
                error => TaskServiceError::Diagnostics(error),
            })
    }

    /// Reads a bounded scan history page independent of child jobs.
    /// # Errors
    /// Returns database failures.
    pub async fn scan_history(
        &self,
        offset: u64,
    ) -> Result<Vec<tjxy_db::ScanHistoryEntry>, TaskServiceError> {
        Ok(FullScanRepository::new(&self.database)
            .history_page(offset)
            .await?)
    }

    /// Reads a scan's bounded issue page and summary, without exposing raw worker errors.
    ///
    /// # Errors
    /// Returns unavailable-task or persistence failures.
    pub async fn scan_report(
        &self,
        job: tjxy_common::WorkJobId,
        offset: u64,
    ) -> Result<tjxy_db::ScanReportPage, TaskServiceError> {
        let record = WorkJobRepository::new(&self.database)
            .get(job)
            .await?
            .ok_or(TaskServiceError::ManualMediaItemUnavailable)?;
        if !matches!(
            record.task_kind(),
            WorkTaskKind::FullMediaScan | WorkTaskKind::FullLibraryRootScan
        ) {
            return Err(TaskServiceError::ManualMediaItemUnavailable);
        }
        Ok(FullScanRepository::new(&self.database)
            .report_page(job, offset)
            .await?)
    }

    /// Retries only failed metadata items in one completed scan-report page.
    /// Pending choices remain explicit and do not create repeated failing jobs.
    ///
    /// # Errors
    /// Returns stale-report, policy, or persistence failures.
    pub async fn retry_scan_issues(
        &self,
        job: tjxy_common::WorkJobId,
        offset: u64,
    ) -> Result<Vec<uuid::Uuid>, TaskServiceError> {
        let jobs = WorkJobRepository::new(&self.database);
        let record = jobs
            .get(job)
            .await?
            .ok_or(TaskServiceError::ManualMediaItemUnavailable)?;
        if matches!(
            record.state(),
            tjxy_db::WorkJobState::Pending | tjxy_db::WorkJobState::Running
        ) {
            return Err(TaskServiceError::StaleDiagnostic);
        }
        let page = self.scan_report(job, offset).await?;
        let mut accepted = Vec::new();
        for issue in page.issues {
            if !issue.needs_selection && issue.task_kind == "ResolveMetadata" {
                accepted.push(
                    self.resolve_metadata(CatalogItemId::from_uuid(issue.item_id))
                        .await?
                        .job()
                        .id()
                        .as_uuid(),
                );
            }
        }
        Ok(accepted)
    }
    #[must_use]
    pub fn new(database: DatabaseConnection) -> Self {
        Self {
            database,
            health_cache: tokio::sync::Mutex::new(None),
            retention: Some(chrono::Duration::days(7)),
        }
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
    #[error("diagnostic state changed; reload before retrying")]
    StaleDiagnostic,
    #[error("task diagnostics are unavailable: {0}")]
    Diagnostics(#[from] sea_orm::DbErr),
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
