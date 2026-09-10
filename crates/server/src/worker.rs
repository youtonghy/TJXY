use std::{
    future::Future,
    pin::pin,
    sync::Arc,
    time::{Duration as StdDuration, Instant},
};

use chrono::Duration;
use sea_orm::{ConnectionTrait, DatabaseConnection};
use tjxy_application::{
    AuthService, CacheInvalidationRun, CacheInvalidationService, CatalogQueryService,
    DiscoverTitlesService, DiscoverTitlesServiceError, FullScanError, FullScanService,
    FullValidateStorageError, FullValidateStorageService, MetadataResolveError,
    MetadataResolveService, ProbeService, ProbeServiceError, ScopedInventoryError,
    ScopedInventoryService, SeriesExpandError, SeriesExpandService, SourceIndexError,
    SourceIndexService, StorageChangeFeedError, StorageChangeFeedService, StorageChangeReconciler,
    TaskService,
};
use tjxy_cache::CacheRuntime;
use tjxy_credentials::{CredentialCipher, CredentialCipherError};
use tjxy_db::{
    CatalogPublicationError, ClaimedImportJob, FullScanRepositoryError, ImportJobRepository,
    ImportRuntimeRepository, ImportRuntimeRepositoryError, ImportStagingRepositoryError,
    MetadataPublicationError, MetadataWorkError, QueueMaintenanceRepository, QueueMaintenanceRun,
    SeriesExpandRepositoryError, SourceIndexRepositoryError, StorageSyncRepositoryError,
    WorkJobRepository, WorkJobRepositoryError, WorkRetentionRepository, WorkRetentionRun,
    WorkTaskKind,
};
use tjxy_import::{EmbyApiCredentials, EmbyApiImporter, EmbyImportError};
use tjxy_storage::{BackendError, StorageBackend};
use tjxy_storage_filesystem::FilesystemBackend;
use tracing::Instrument;
use uuid::Uuid;

use crate::socket::RealtimeEvents;

mod adaptive;
mod lease;
pub(crate) use adaptive::spawn_background_scan_worker;

const LEASE_DURATION: Duration = Duration::minutes(5);
const WORK_LEASE_RECOVERY_INTERVAL: StdDuration = StdDuration::from_secs(5);
const LEASE_RENEW_TIMEOUT: StdDuration = StdDuration::from_secs(5);
const IMPORT_LEASE_DURATION: Duration = Duration::minutes(5);
const FILESYSTEM_EVENT_PRIORITY: i32 = 90;
const FILESYSTEM_EVENT_QUIET_WINDOW: StdDuration = StdDuration::from_millis(500);
const HOME_CACHE_WARM_USER_LIMIT: u64 = 128;
const SESSION_RETENTION: Duration = Duration::days(180);

async fn renew_scan_execution<T, Error>(
    database: &DatabaseConnection,
    claimed: &tjxy_db::ClaimedWorkJob,
    work: impl Future<Output = Result<T, Error>>,
    lost_lease: impl Fn(WorkJobRepositoryError) -> Error,
) -> Result<T, Error>
where
    Error: std::fmt::Display,
{
    let jobs = WorkJobRepository::new(database);
    let execution = execute_logged(claimed, work);
    lease::run(
        execution,
        || async {
            matches!(
                tokio::time::timeout(LEASE_RENEW_TIMEOUT, jobs.renew(claimed, LEASE_DURATION))
                    .await,
                Ok(Ok(()))
            )
        },
        StdDuration::from_secs(60),
        lost_lease(WorkJobRepositoryError::LostLease),
    )
    .await
}

async fn process_scan_job(
    database: &DatabaseConnection,
    services: &adaptive::ScanServices,
    claimed: &tjxy_db::ClaimedWorkJob,
) -> bool {
    let jobs = WorkJobRepository::new(database);
    match claimed.job().task_kind() {
        WorkTaskKind::ResolveMetadata => {
            let outcome = Box::pin(renew_scan_execution(
                database,
                claimed,
                services.metadata.execute(claimed),
                |error| MetadataResolveError::Work(MetadataWorkError::Work(error)),
            ))
            .await;
            let success = outcome.is_ok();
            handle_metadata_outcome(&jobs, claimed, outcome).await;
            success
        }
        WorkTaskKind::ProbeMedia => {
            let outcome = Box::pin(renew_scan_execution(
                database,
                claimed,
                services.probe.execute(claimed),
                |error| ProbeServiceError::Repository(tjxy_db::ProbeRepositoryError::Work(error)),
            ))
            .await;
            let success = outcome.is_ok();
            handle_outcome(&jobs, claimed, outcome).await;
            success
        }
        WorkTaskKind::IndexMediaSources => {
            let outcome = Box::pin(renew_scan_execution(
                database,
                claimed,
                services.sources.execute(claimed),
                |error| SourceIndexError::Publication(CatalogPublicationError::WorkJob(error)),
            ))
            .await;
            let success = outcome.is_ok();
            handle_source_index_outcome(&jobs, claimed, outcome).await;
            success
        }
        WorkTaskKind::ExpandItem => {
            let outcome = Box::pin(renew_scan_execution(
                database,
                claimed,
                services.series.execute(claimed),
                |error| SeriesExpandError::Publication(CatalogPublicationError::WorkJob(error)),
            ))
            .await;
            let success = outcome.is_ok();
            handle_series_expand_outcome(&jobs, claimed, outcome).await;
            success
        }
        _ => unreachable!("background dispatcher only accepts scan item stages"),
    }
}

pub(crate) fn spawn_auth_session_retention_worker(database: DatabaseConnection) {
    tokio::spawn(async move {
        let mut schedule = tokio::time::interval(StdDuration::from_secs(24 * 60 * 60));
        schedule.tick().await;
        loop {
            schedule.tick().await;
            let cutoff = chrono::Utc::now() - SESSION_RETENTION;
            match tjxy_db::AuthRepository::new(&database)
                .prune_old_sessions(cutoff, 500)
                .await
            {
                Ok(deleted) if deleted > 0 => tracing::info!(deleted, "Pruned old auth sessions"),
                Ok(_) => {}
                Err(error) => tracing::error!("Auth session retention failed: {error}"),
            }
        }
    });
}

fn job_span(claimed: &tjxy_db::ClaimedWorkJob) -> tracing::Span {
    let job = claimed.job();
    let scope = job.scope();
    tracing::debug_span!(
        "work_job",
        job_id = %job.id().as_uuid(),
        task_kind = job.task_kind().as_str(),
        scope_type = scope.scope_type(),
        scope_id = %scope.id(),
        expected_revision = job.expected_revision(),
        attempt_count = job.attempt_count(),
    )
}

async fn execute_logged<T, Error, Work>(
    claimed: &tjxy_db::ClaimedWorkJob,
    work: Work,
) -> Result<T, Error>
where
    Error: std::fmt::Display,
    Work: Future<Output = Result<T, Error>>,
{
    async move {
        let started = Instant::now();
        tracing::debug!("work job started");
        let outcome = tjxy_application::with_work_io_priority(claimed.job().priority(), work).await;
        tracing::debug!(
            duration_ms = started.elapsed().as_millis(),
            outcome = if outcome.is_ok() {
                "completed"
            } else {
                "deferred_or_failed"
            },
            "work job finished"
        );
        outcome
    }
    .instrument(job_span(claimed))
    .await
}

pub(crate) fn spawn_media_refresh_scheduler(tasks: Arc<TaskService>, interval: StdDuration) {
    tokio::spawn(async move {
        let mut schedule = tokio::time::interval(interval);
        schedule.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        schedule.tick().await;
        loop {
            schedule.tick().await;
            if let Err(error) = tasks.schedule_periodic_library_refresh().await {
                tracing::error!("Periodic media refresh could not enqueue work: {error}");
            }
        }
    });
}

pub(crate) fn spawn_home_cache_warm_worker(
    auth: Arc<AuthService>,
    catalog: Arc<CatalogQueryService>,
) {
    tokio::spawn(async move {
        let users = match auth.enabled_user_ids(HOME_CACHE_WARM_USER_LIMIT).await {
            Ok(users) => users,
            Err(error) => {
                tracing::error!("Home cache warmup could not select enabled users: {error}");
                return;
            }
        };
        for user in users {
            if let Err(error) = catalog.warm_home(&[user]).await {
                tracing::error!("Home cache warmup failed for user {user}: {error}");
            }
        }
    });
}

pub(crate) fn spawn_filesystem_event_worker(
    database: DatabaseConnection,
    account_id: Uuid,
    backend: Arc<FilesystemBackend>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        run_filesystem_event_worker(database, account_id, backend).await;
    })
}

async fn run_filesystem_event_worker(
    database: DatabaseConnection,
    account_id: Uuid,
    backend: Arc<FilesystemBackend>,
) {
    let mut monitor = loop {
        match backend.watch_events(FILESYSTEM_EVENT_QUIET_WINDOW) {
            Ok(monitor) => break monitor,
            Err(error) => {
                tracing::error!(
                    "Filesystem event monitor startup failed and will be retried: {error}"
                );
                tokio::time::sleep(StdDuration::from_secs(5)).await;
            }
        }
    };
    loop {
        if !filesystem_events_allowed(&database, account_id).await {
            tokio::time::sleep(StdDuration::from_secs(1)).await;
            continue;
        }
        match monitor.next_batch().await {
            Ok(batch) => match backend.inventory_scopes_for(&batch).await {
                Ok(scopes) => {
                    match tjxy_db::StorageSyncRepository::new(&database)
                        .enqueue_event_scopes(
                            account_id,
                            "local",
                            &scopes,
                            FILESYSTEM_EVENT_PRIORITY,
                        )
                        .await
                    {
                        Ok(submissions) => {
                            let created = submissions
                                .iter()
                                .filter(|submission| submission.created())
                                .count();
                            tracing::debug!(
                                resolved_scopes = scopes.len(),
                                created,
                                joined = submissions.len().saturating_sub(created),
                                "Filesystem event scopes enqueued or joined"
                            );
                        }
                        Err(error) => {
                            tracing::error!(
                                "Filesystem event scopes could not be enqueued: {error}"
                            );
                            tokio::time::sleep(StdDuration::from_secs(1)).await;
                        }
                    }
                }
                Err(error) => {
                    tracing::error!("Filesystem event scopes could not be resolved: {error}");
                    tokio::time::sleep(StdDuration::from_secs(1)).await;
                }
            },
            Err(error) => {
                tracing::error!("Filesystem event monitor failed and will be restarted: {error}");
                tokio::time::sleep(StdDuration::from_secs(1)).await;
                loop {
                    match backend.watch_events(FILESYSTEM_EVENT_QUIET_WINDOW) {
                        Ok(replacement) => {
                            monitor = replacement;
                            break;
                        }
                        Err(restart_error) => {
                            tracing::error!(
                                "Filesystem event monitor restart failed: {restart_error}"
                            );
                            tokio::time::sleep(StdDuration::from_secs(5)).await;
                        }
                    }
                }
            }
        }
    }
}

async fn filesystem_events_allowed(database: &DatabaseConnection, account_id: Uuid) -> bool {
    match tjxy_db::FilesystemIndexRepository::new(database)
        .state(account_id)
        .await
    {
        Ok(tjxy_db::FilesystemIndexState::Ready) => true,
        Ok(_) => false,
        Err(error) => {
            tracing::error!(
                storage_account_id = %account_id,
                error = %error,
                "Filesystem path index state could not be read for event processing"
            );
            false
        }
    }
}

pub(crate) fn spawn_cache_invalidation_worker(
    database: DatabaseConnection,
    cache: Arc<CacheRuntime>,
    events: Arc<RealtimeEvents>,
) {
    tokio::spawn(async move {
        let service = CacheInvalidationService::new(database, cache);
        let mut waiter = tjxy_db::WorkQueueWaiter::default();
        let mut last_outbox_purge = std::time::Instant::now();
        loop {
            let delay = match service.run_once().await {
                Ok(CacheInvalidationRun::Completed { generation, .. }) => {
                    waiter.reset();
                    events.publish_library_changed(generation);
                    StdDuration::from_millis(100)
                }
                Ok(CacheInvalidationRun::Progressed { .. }) => {
                    waiter.reset();
                    StdDuration::from_millis(25)
                }
                Ok(CacheInvalidationRun::Idle) => {
                    waiter.wait().await;
                    StdDuration::ZERO
                }
                Ok(CacheInvalidationRun::Deferred {
                    generation,
                    failure,
                }) => {
                    tracing::error!(
                        "Cache invalidation for generation {generation} was deferred: {failure}"
                    );
                    StdDuration::from_millis(250)
                }
                Err(error) => {
                    tracing::error!(
                        "Cache invalidation worker could not persist progress: {error}"
                    );
                    StdDuration::from_secs(1)
                }
            };
            if last_outbox_purge.elapsed() >= OUTBOX_PURGE_INTERVAL {
                last_outbox_purge = std::time::Instant::now();
                match service.purge_consumed_outbox().await {
                    Ok(deleted) if deleted > 0 => {
                        tracing::debug!(deleted, "purged consumed change-outbox rows");
                    }
                    Ok(_) => {}
                    Err(error) => {
                        tracing::warn!("change-outbox purge failed: {error}");
                    }
                }
            }
            tokio::time::sleep(delay).await;
        }
    });
}

/// Cadence of the bounded change-outbox purge driven by the invalidation
/// worker loop.
const OUTBOX_PURGE_INTERVAL: StdDuration = StdDuration::from_secs(30);

pub(crate) fn spawn_storage_change_reconciler(database: DatabaseConnection) {
    tokio::spawn(async move {
        let mut reconciler = StorageChangeReconciler::new(database);
        let mut waiter = tjxy_db::WorkQueueWaiter::default();
        loop {
            let delay = match reconciler.run_once().await {
                Ok(report) => {
                    for failure in report.failures() {
                        tracing::error!(
                            "Storage change reconciliation failed for root {}: {}",
                            failure.root_id(),
                            failure.error()
                        );
                    }
                    if report.events_processed() == 0 {
                        waiter.wait().await;
                        StdDuration::ZERO
                    } else {
                        waiter.reset();
                        StdDuration::from_millis(100)
                    }
                }
                Err(error) => {
                    tracing::error!(
                        "Storage change reconciler could not enumerate backlog: {error}"
                    );
                    StdDuration::from_secs(1)
                }
            };
            tokio::time::sleep(delay).await;
        }
    });
}

pub(crate) fn spawn_queue_maintenance_worker(database: DatabaseConnection) {
    tokio::spawn(async move {
        let repository = QueueMaintenanceRepository::new(&database);
        loop {
            let delay = match repository.run_once(Duration::days(7)).await {
                Ok(QueueMaintenanceRun::StorageOutbox { deleted }) => {
                    tracing::debug!(deleted, "Cleaned storage outbox rows");
                    StdDuration::from_millis(50)
                }
                Ok(QueueMaintenanceRun::LegacyCacheOutbox { deleted }) => {
                    tracing::debug!(deleted, "Cleaned legacy cache outbox rows");
                    StdDuration::from_millis(50)
                }
                Ok(QueueMaintenanceRun::Idle) => StdDuration::from_secs(60),
                Err(error) => {
                    tracing::error!("Internal queue maintenance failed: {error}");
                    StdDuration::from_secs(1)
                }
            };
            tokio::time::sleep(delay).await;
        }
    });
}

pub(crate) fn spawn_work_lease_recovery_worker(database: DatabaseConnection) {
    if database.get_database_backend() == sea_orm::DbBackend::Postgres {
        let notifications_database = database.clone();
        tokio::spawn(async move {
            loop {
                if tjxy_db::listen_for_work(&notifications_database)
                    .await
                    .is_err()
                {
                    tracing::warn!("Work notifications disconnected; workers continue polling");
                }
                tokio::time::sleep(StdDuration::from_secs(30)).await;
            }
        });
    }
    tokio::spawn(async move {
        let repository = WorkJobRepository::new(&database);
        let mut schedule = tokio::time::interval(WORK_LEASE_RECOVERY_INTERVAL);
        schedule.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        schedule.tick().await;
        loop {
            schedule.tick().await;
            match repository.maintain_queue().await {
                Ok(report) if report.reclaimed + report.cancelled > 0 => {
                    tracing::info!(
                        reclaimed = report.reclaimed,
                        cancelled = report.cancelled,
                        "Maintained work queue"
                    );
                }
                Ok(_) => {}
                Err(error) => tracing::error!("Work lease recovery failed: {error}"),
            }
        }
    });
}

#[derive(Clone, Default, serde::Serialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct RetentionObservation {
    last_run_at: Option<chrono::DateTime<chrono::Utc>>,
    deleted: u64,
    compacted: u64,
    purged: u64,
    deferred: u64,
    failed_runs: u64,
}
fn retention_observations() -> &'static std::sync::Mutex<RetentionObservation> {
    static VALUE: std::sync::OnceLock<std::sync::Mutex<RetentionObservation>> =
        std::sync::OnceLock::new();
    VALUE.get_or_init(|| std::sync::Mutex::new(RetentionObservation::default()))
}
pub(crate) fn retention_observation() -> RetentionObservation {
    retention_observations()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
}
fn observe_retention(outcome: &Result<WorkRetentionRun, tjxy_db::WorkRetentionError>) {
    let mut observation = retention_observations()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    observation.last_run_at = Some(chrono::Utc::now());
    match outcome {
        Ok(WorkRetentionRun::Processed {
            deleted,
            compacted,
            purged,
            deferred,
        }) => {
            observation.deleted = observation.deleted.saturating_add(*deleted);
            observation.compacted = observation.compacted.saturating_add(*compacted);
            observation.purged = observation.purged.saturating_add(*purged);
            observation.deferred = observation.deferred.saturating_add(*deferred);
        }
        Err(_) => observation.failed_runs = observation.failed_runs.saturating_add(1),
        _ => {}
    }
}

pub(crate) fn spawn_work_retention_worker(database: DatabaseConnection, retention: StdDuration) {
    tokio::spawn(async move {
        let owner = format!("work-retention-{}", Uuid::new_v4());
        let retention = Duration::from_std(retention).expect("validated work retention duration");
        let repository = WorkRetentionRepository::new(&database);
        let mut burst_started = Instant::now();
        loop {
            if burst_started.elapsed() >= StdDuration::from_secs(2) {
                tokio::time::sleep(StdDuration::from_secs(5)).await;
                burst_started = Instant::now();
            }
            let outcome = repository
                .run_once(&owner, retention, Duration::seconds(30))
                .await;
            observe_retention(&outcome);
            let delay = match outcome {
                Ok(WorkRetentionRun::Processed {
                    deleted,
                    compacted,
                    purged,
                    deferred,
                }) => {
                    tracing::debug!(
                        deleted,
                        compacted,
                        purged,
                        deferred,
                        "Processed work history retention batch"
                    );
                    StdDuration::from_millis(100)
                }
                Ok(WorkRetentionRun::EnrolledLegacy { count }) => {
                    tracing::debug!(count, "Enrolled legacy work history for retention");
                    StdDuration::from_millis(100)
                }
                Ok(WorkRetentionRun::Idle) => StdDuration::from_secs(60),
                Err(error) => {
                    tracing::error!("Work history retention failed: {error}");
                    StdDuration::from_secs(1)
                }
            };
            tokio::time::sleep(delay).await;
        }
    });
}

pub(crate) fn spawn_import_worker(database: DatabaseConnection, cipher: Arc<CredentialCipher>) {
    tokio::spawn(async move {
        let owner = format!("emby-import-worker-{}", Uuid::new_v4());
        let mut idle = tjxy_db::WorkQueueWaiter::default();
        loop {
            let jobs = ImportJobRepository::new(&database);
            match jobs
                .claim_next("EmbyApi", &owner, IMPORT_LEASE_DURATION)
                .await
            {
                Ok(Some(claimed)) => {
                    idle.reset();
                    run_claimed_import(&database, &cipher, &jobs, &claimed).await;
                }
                Ok(None) => idle.wait().await,
                Err(error) => {
                    tracing::error!("Emby import worker could not claim work: {error}");
                    tokio::time::sleep(StdDuration::from_secs(1)).await;
                }
            }
        }
    });
}

async fn run_claimed_import(
    database: &DatabaseConnection,
    cipher: &CredentialCipher,
    jobs: &ImportJobRepository<'_>,
    claimed: &ClaimedImportJob,
) {
    let started = Instant::now();
    let span = tracing::debug_span!(
        "import_job",
        import_job_id = %claimed.id(),
        adapter_kind = claimed.job().adapter_kind(),
        attempt_count = claimed.job().attempt_count(),
    );
    tracing::debug!(parent: &span, "import job started");
    let execution = execute_import(database, cipher, claimed).instrument(span.clone());
    let mut execution = pin!(execution);
    let mut renewal = tokio::time::interval(StdDuration::from_secs(60));
    renewal.tick().await;
    let outcome = loop {
        tokio::select! {
            result = &mut execution => break result,
            _ = renewal.tick() => {
                if let Err(error) = jobs.renew(claimed, IMPORT_LEASE_DURATION).await {
                    break Err(ImportWorkerError::Staging(error));
                }
            }
        }
    };
    let Err(error) = outcome else {
        tracing::debug!(parent: &span, duration_ms = started.elapsed().as_millis(), outcome = "completed", "import job finished");
        return;
    };
    tracing::debug!(parent: &span, duration_ms = started.elapsed().as_millis(), outcome = "failed", error = %error, "import job finished");
    if matches!(
        error,
        ImportWorkerError::Staging(ImportStagingRepositoryError::LostLease)
            | ImportWorkerError::Execution(EmbyImportError::Staging(
                ImportStagingRepositoryError::LostLease
            ))
    ) {
        return;
    }
    let message = truncate_error(&error.to_string());
    let result = if import_error_is_retryable(&error) {
        jobs.retry(claimed, Duration::seconds(5), &message).await
    } else {
        tracing::error!(parent: &span, error = %error, "import job failed terminally");
        jobs.fail_terminal(claimed, &message).await
    };
    if let Err(update_error) = result
        && !matches!(update_error, ImportStagingRepositoryError::LostLease)
    {
        tracing::error!("Emby import worker could not persist failure: {update_error}");
    }
}

async fn execute_import(
    database: &DatabaseConnection,
    cipher: &CredentialCipher,
    claimed: &ClaimedImportJob,
) -> Result<(), ImportWorkerError> {
    let source = ImportRuntimeRepository::new(database)
        .source_for_job(claimed.id())
        .await?
        .ok_or(ImportWorkerError::MissingSource)?;
    let plaintext = cipher.open(source.source_id(), "emby-import", source.envelope())?;
    let credentials = EmbyApiCredentials::from_payload_json(&plaintext)?;
    EmbyApiImporter::new(database.clone(), credentials)?
        .run_claimed(claimed)
        .await?;
    Ok(())
}

fn import_error_is_retryable(error: &ImportWorkerError) -> bool {
    matches!(
        error,
        ImportWorkerError::Runtime(ImportRuntimeRepositoryError::Database(_))
            | ImportWorkerError::Execution(
                EmbyImportError::Transport(_)
                    | EmbyImportError::Staging(
                        ImportStagingRepositoryError::Database(_)
                            | ImportStagingRepositoryError::RollbackFailed { .. },
                    ),
            )
    )
}

#[derive(Debug, thiserror::Error)]
enum ImportWorkerError {
    #[error("import source configuration is missing")]
    MissingSource,
    #[error("import credential authentication failed: {0}")]
    Credential(#[from] CredentialCipherError),
    #[error("import runtime configuration failed: {0}")]
    Runtime(#[from] ImportRuntimeRepositoryError),
    #[error("Emby import execution failed: {0}")]
    Execution(#[from] EmbyImportError),
    #[error("Emby import lease failed: {0}")]
    Staging(ImportStagingRepositoryError),
}

pub(crate) fn spawn_probe_worker(database: DatabaseConnection, service: Arc<ProbeService>) {
    tokio::spawn(async move {
        Box::pin(run_probe_worker(database, service)).await;
    });
}

pub(crate) fn spawn_metadata_worker(
    database: DatabaseConnection,
    service: Arc<MetadataResolveService>,
) {
    tokio::spawn(async move {
        Box::pin(run_metadata_worker(database, service)).await;
    });
}

pub(crate) fn spawn_discover_worker(database: DatabaseConnection) {
    tokio::spawn(async move {
        let service = DiscoverTitlesService::new(database.clone());
        let owner = format!("discover-worker-{}", Uuid::new_v4());
        let mut idle = tjxy_db::WorkQueueWaiter::default();
        'worker: loop {
            let jobs = WorkJobRepository::new(&database);
            match jobs
                .claim_next(&[WorkTaskKind::DiscoverTitles], &owner, LEASE_DURATION)
                .await
            {
                Ok(Some(claimed)) => {
                    idle.reset();
                    let execution = execute_logged(&claimed, service.execute(&claimed));
                    let mut execution = pin!(execution);
                    let mut renewal = tokio::time::interval(StdDuration::from_secs(60));
                    renewal.tick().await;
                    let outcome = loop {
                        tokio::select! {
                            result = &mut execution => break result,
                            _ = renewal.tick() => {
                                if let Err(error) = jobs.renew(&claimed, LEASE_DURATION).await {
                                    if !matches!(error, WorkJobRepositoryError::LostLease) {
                                        tracing::error!("Discover worker could not renew lease: {error}");
                                    }
                                    continue 'worker;
                                }
                            }
                        }
                    };
                    if let Err(error) = outcome {
                        let message = truncate_error(&error.to_string());
                        let result = if discover_error_is_terminal(&error) {
                            log_terminal_failure(
                                &claimed,
                                "title discovery failed terminally",
                                &error,
                            );
                            jobs.fail_terminal(&claimed, &message).await
                        } else if matches!(
                            error,
                            DiscoverTitlesServiceError::Repository(
                                tjxy_db::DiscoverTitlesError::StorageInputPending
                            )
                        ) {
                            jobs.defer(&claimed, Duration::seconds(30), &message).await
                        } else {
                            jobs.retry(
                                &claimed,
                                discover_retry_delay(claimed.attempt_count()),
                                &message,
                            )
                            .await
                        };
                        if let Err(update_error) = result
                            && !matches!(update_error, WorkJobRepositoryError::LostLease)
                        {
                            tracing::error!(
                                "Discover worker could not persist failure: {update_error}"
                            );
                        }
                    }
                }
                Ok(None) => idle.wait().await,
                Err(error) => {
                    tracing::error!("Discover worker could not claim work: {error}");
                    tokio::time::sleep(StdDuration::from_secs(1)).await;
                }
            }
        }
    });
}

fn discover_error_is_terminal(error: &DiscoverTitlesServiceError) -> bool {
    matches!(
        error,
        DiscoverTitlesServiceError::Repository(
            tjxy_db::DiscoverTitlesError::InvalidClaim
                | tjxy_db::DiscoverTitlesError::TitleLimit
                | tjxy_db::DiscoverTitlesError::UnsupportedCollection
                | tjxy_db::DiscoverTitlesError::InvalidTitle
                | tjxy_db::DiscoverTitlesError::IdentityConflict
                | tjxy_db::DiscoverTitlesError::MissingLibraryScope
                | tjxy_db::DiscoverTitlesError::InvalidLibraryScope
                | tjxy_db::DiscoverTitlesError::StaleLibraryPolicy
                | tjxy_db::DiscoverTitlesError::StaleRoot
                | tjxy_db::DiscoverTitlesError::AlreadyCurrent
        )
    )
}

fn discover_retry_delay(attempt_count: i32) -> Duration {
    storage_backoff(attempt_count)
}

pub(crate) fn spawn_storage_worker<Backend>(
    database: DatabaseConnection,
    account_id: Uuid,
    backend: Arc<Backend>,
) -> tokio::task::JoinHandle<()>
where
    Backend: StorageBackend + ?Sized + 'static,
{
    tokio::spawn(async move {
        let scoped = ScopedInventoryService::new(database.clone(), Arc::clone(&backend));
        let validation = FullValidateStorageService::new(database.clone(), backend);
        run_storage_worker(database, account_id, None, scoped, validation).await;
    })
}

pub(crate) fn spawn_storage_worker_for_drive<Backend>(
    database: DatabaseConnection,
    account_id: Uuid,
    provider_drive_id: String,
    backend: Arc<Backend>,
) -> tokio::task::JoinHandle<()>
where
    Backend: StorageBackend + ?Sized + 'static,
{
    tokio::spawn(async move {
        let scoped = ScopedInventoryService::new(database.clone(), Arc::clone(&backend));
        let validation = FullValidateStorageService::new(database.clone(), backend);
        run_storage_worker(
            database,
            account_id,
            Some(provider_drive_id),
            scoped,
            validation,
        )
        .await;
    })
}

pub(crate) fn spawn_storage_change_worker<Backend>(
    database: DatabaseConnection,
    account_id: Uuid,
    provider_drive_id: String,
    backend: Arc<Backend>,
) -> tokio::task::JoinHandle<()>
where
    Backend: StorageBackend + ?Sized + 'static,
{
    tokio::spawn(async move {
        let service = StorageChangeFeedService::new(database, backend);
        loop {
            let delay = match service
                .run_active_roots(account_id, &provider_drive_id)
                .await
            {
                Ok(_) => StdDuration::from_secs(30),
                Err(error) => {
                    tracing::error!("Storage Changes worker could not sync roots: {error}");
                    storage_change_retry_delay(&error)
                }
            };
            tokio::time::sleep(delay).await;
        }
    })
}

fn storage_change_retry_delay(error: &StorageChangeFeedError) -> StdDuration {
    match error {
        StorageChangeFeedError::Backend(BackendError::RateLimited {
            retry_after: Some(delay),
        }) => *delay,
        _ => StdDuration::from_secs(5),
    }
}

pub(crate) fn spawn_source_index_worker(database: DatabaseConnection) {
    tokio::spawn(async move {
        let service = SourceIndexService::new(database.clone());
        Box::pin(run_source_index_worker(database, service)).await;
    });
}

pub(crate) fn spawn_series_expand_worker(database: DatabaseConnection) {
    tokio::spawn(async move {
        let service = SeriesExpandService::new(database.clone());
        Box::pin(run_series_expand_worker(database, service)).await;
    });
}

pub(crate) fn spawn_full_scan_worker(database: DatabaseConnection) {
    tokio::spawn(async move {
        let service = FullScanService::new(database.clone());
        Box::pin(run_full_scan_worker(database, service)).await;
    });
}

async fn run_full_scan_worker(database: DatabaseConnection, service: FullScanService) {
    let owner = format!("full-scan-worker-{}", Uuid::new_v4());
    let mut idle = tjxy_db::WorkQueueWaiter::default();
    loop {
        let jobs = WorkJobRepository::new(&database);
        match jobs
            .claim_next(
                &[
                    WorkTaskKind::FullMediaScan,
                    WorkTaskKind::FullLibraryRootScan,
                ],
                &owner,
                LEASE_DURATION,
            )
            .await
        {
            Ok(Some(claimed)) => {
                idle.reset();
                let outcome = Box::pin(renew_scan_execution(
                    &database,
                    &claimed,
                    service.execute(&claimed),
                    FullScanError::Work,
                ))
                .await;
                handle_full_scan_outcome(&jobs, &claimed, outcome).await;
            }
            Ok(None) => idle.wait().await,
            Err(error) => {
                tracing::error!("Full scan worker could not claim work: {error}");
                tokio::time::sleep(StdDuration::from_secs(1)).await;
            }
        }
    }
}

async fn handle_full_scan_outcome(
    jobs: &WorkJobRepository<'_>,
    claimed: &tjxy_db::ClaimedWorkJob,
    outcome: Result<tjxy_application::FullScanResult, FullScanError>,
) {
    let Err(error) = outcome else { return };
    if matches!(
        error,
        FullScanError::Work(WorkJobRepositoryError::LostLease)
    ) {
        return;
    }
    let message = truncate_error(&error.to_string());
    let result = if full_scan_error_is_terminal(&error) {
        log_terminal_failure(claimed, "full scan failed terminally", &error);
        jobs.fail_terminal(claimed, &message).await
    } else {
        let delay = full_scan_retry_delay(&error, claimed.attempt_count());
        if matches!(error, FullScanError::ChildrenPending { .. }) {
            jobs.defer(claimed, delay, &message).await
        } else {
            jobs.retry(claimed, delay, &message).await
        }
    };
    if let Err(update_error) = result {
        tracing::error!("Full scan worker could not persist failure outcome: {update_error}");
    }
}

fn full_scan_retry_delay(error: &FullScanError, attempt_count: i32) -> Duration {
    if matches!(error, FullScanError::ChildrenPending { .. }) {
        // Completions wake a waiting parent sooner; this is the bounded fallback
        // for a lost hint or a child that finished before the parent deferred.
        return Duration::seconds(2);
    }
    storage_backoff(attempt_count)
}

fn full_scan_error_is_terminal(error: &FullScanError) -> bool {
    matches!(
        error,
        FullScanError::MissingStorageScope(_)
            | FullScanError::MissingValidationDependency(_)
            | FullScanError::MissingInventoryDependency(_)
            | FullScanError::ValidationFailed(_)
            | FullScanError::InventoryFailed(_)
            | FullScanError::MissingChildDependency(_)
            | FullScanError::CorruptChildDependency(_)
            | FullScanError::ChildFailed { .. }
            | FullScanError::ChildCompletedWithoutPublication { .. }
            | FullScanError::Work(
                WorkJobRepositoryError::IncompatibleActiveJob
                    | WorkJobRepositoryError::StaleParentPolicy
                    | WorkJobRepositoryError::InvalidChildReference
                    | WorkJobRepositoryError::InvalidMetadataWork
            )
            | FullScanError::Repository(
                FullScanRepositoryError::InvalidClaim
                    | FullScanRepositoryError::StaleLibrary
                    | FullScanRepositoryError::InvalidStoredPolicy
                    | FullScanRepositoryError::InvalidCandidateLimit
                    | FullScanRepositoryError::CorruptHybridCandidateBatch
                    | FullScanRepositoryError::CorruptRootDependency { .. }
            )
            | FullScanError::Publication(
                CatalogPublicationError::InvalidStructureRow
                    | CatalogPublicationError::InvalidManifest
                    | CatalogPublicationError::InvalidWorkKind
                    | CatalogPublicationError::InvalidPublication
                    | CatalogPublicationError::ManifestMismatch
                    | CatalogPublicationError::InvalidStructure
                    | CatalogPublicationError::InvalidSourceRow
                    | CatalogPublicationError::InvalidSourceManifest
                    | CatalogPublicationError::InvalidSourceGraph
                    | CatalogPublicationError::StableIdentityConflict
                    | CatalogPublicationError::UnauthorizedStorageObject
                    | CatalogPublicationError::StaleExpectedRevision
                    | CatalogPublicationError::MissingCatalogState
            )
    )
}

async fn run_series_expand_worker(database: DatabaseConnection, service: SeriesExpandService) {
    let owner = format!("series-expand-worker-{}", Uuid::new_v4());
    let mut idle = tjxy_db::WorkQueueWaiter::default();
    loop {
        let jobs = WorkJobRepository::new(&database);
        match jobs
            .claim_next_foreground(&[WorkTaskKind::ExpandItem], &owner, LEASE_DURATION)
            .await
        {
            Ok(Some(claimed)) => {
                idle.reset();
                let execution = execute_logged(&claimed, service.execute(&claimed));
                let mut execution = pin!(execution);
                let mut renewal = tokio::time::interval(StdDuration::from_secs(60));
                renewal.tick().await;
                let outcome = loop {
                    tokio::select! {
                        result = &mut execution => break result,
                        _ = renewal.tick() => {
                            if jobs.renew(&claimed, LEASE_DURATION).await.is_err() {
                                break Err(SeriesExpandError::Publication(
                                    CatalogPublicationError::WorkJob(
                                        WorkJobRepositoryError::LostLease,
                                    ),
                                ));
                            }
                        }
                    }
                };
                handle_series_expand_outcome(&jobs, &claimed, outcome).await;
            }
            Ok(None) => idle.wait().await,
            Err(error) => {
                tracing::error!("Series expand worker could not claim work: {error}");
                tokio::time::sleep(StdDuration::from_secs(1)).await;
            }
        }
    }
}

async fn handle_series_expand_outcome(
    jobs: &WorkJobRepository<'_>,
    claimed: &tjxy_db::ClaimedWorkJob,
    outcome: Result<i64, SeriesExpandError>,
) {
    let Err(error) = outcome else { return };
    if matches!(
        error,
        SeriesExpandError::Publication(CatalogPublicationError::WorkJob(
            WorkJobRepositoryError::LostLease
        )) | SeriesExpandError::Work(WorkJobRepositoryError::LostLease)
    ) {
        return;
    }
    let message = truncate_error(&error.to_string());
    let result = if series_expand_error_is_terminal(&error) {
        log_terminal_failure(claimed, "series expansion failed terminally", &error);
        jobs.fail_terminal(claimed, &message).await
    } else {
        let delay = if matches!(error, SeriesExpandError::InventoryPending { .. })
            && claimed.job().priority() >= 100
        {
            Duration::milliseconds(200)
        } else {
            Duration::seconds(5)
        };
        if matches!(error, SeriesExpandError::InventoryPending { .. }) {
            jobs.defer(claimed, delay, &message).await
        } else {
            jobs.retry(claimed, delay, &message).await
        }
    };
    if let Err(update_error) = result {
        tracing::error!("Series expand worker could not persist failure outcome: {update_error}");
    }
}

fn series_expand_error_is_terminal(error: &SeriesExpandError) -> bool {
    matches!(
        error,
        SeriesExpandError::IncompleteTree
            | SeriesExpandError::NoEpisodes
            | SeriesExpandError::InvalidMedia
            | SeriesExpandError::InvalidMediaName(_)
            | SeriesExpandError::InvalidNamingHints
            | SeriesExpandError::Repository(
                SeriesExpandRepositoryError::InvalidClaim
                    | SeriesExpandRepositoryError::MissingSyncRevision
                    | SeriesExpandRepositoryError::AmbiguousScope
                    | SeriesExpandRepositoryError::ObjectLimit
            )
            | SeriesExpandError::Publication(
                CatalogPublicationError::InvalidStructureRow
                    | CatalogPublicationError::InvalidManifest
                    | CatalogPublicationError::InvalidWorkKind
                    | CatalogPublicationError::InvalidPublication
                    | CatalogPublicationError::ManifestMismatch
                    | CatalogPublicationError::InvalidStructure
                    | CatalogPublicationError::InvalidSourceRow
                    | CatalogPublicationError::InvalidSourceManifest
                    | CatalogPublicationError::InvalidSourceGraph
                    | CatalogPublicationError::StableIdentityConflict
                    | CatalogPublicationError::UnauthorizedStorageObject
                    | CatalogPublicationError::StaleExpectedRevision
                    | CatalogPublicationError::MissingCatalogState
            )
    )
}

async fn run_source_index_worker(database: DatabaseConnection, service: SourceIndexService) {
    let owner = format!("source-index-worker-{}", Uuid::new_v4());
    let mut idle = tjxy_db::WorkQueueWaiter::default();
    loop {
        let jobs = WorkJobRepository::new(&database);
        match jobs
            .claim_next_foreground(&[WorkTaskKind::IndexMediaSources], &owner, LEASE_DURATION)
            .await
        {
            Ok(Some(claimed)) => {
                idle.reset();
                let execution = execute_logged(&claimed, service.execute(&claimed));
                let mut execution = pin!(execution);
                let mut renewal = tokio::time::interval(StdDuration::from_secs(60));
                renewal.tick().await;
                let outcome = loop {
                    tokio::select! {
                        result = &mut execution => break result,
                        _ = renewal.tick() => {
                            if jobs.renew(&claimed, LEASE_DURATION).await.is_err() {
                                break Err(SourceIndexError::Publication(
                                    CatalogPublicationError::WorkJob(
                                        WorkJobRepositoryError::LostLease,
                                    ),
                                ));
                            }
                        }
                    }
                };
                handle_source_index_outcome(&jobs, &claimed, outcome).await;
            }
            Ok(None) => idle.wait().await,
            Err(error) => {
                tracing::error!("Source index worker could not claim work: {error}");
                tokio::time::sleep(StdDuration::from_secs(1)).await;
            }
        }
    }
}

async fn handle_source_index_outcome(
    jobs: &WorkJobRepository<'_>,
    claimed: &tjxy_db::ClaimedWorkJob,
    outcome: Result<i64, SourceIndexError>,
) {
    let Err(error) = outcome else { return };
    if matches!(
        error,
        SourceIndexError::Publication(CatalogPublicationError::WorkJob(
            WorkJobRepositoryError::LostLease
        ))
    ) {
        return;
    }
    let message = truncate_error(&error.to_string());
    let result = if source_index_error_is_terminal(&error) {
        log_terminal_failure(claimed, "media source indexing failed terminally", &error);
        jobs.fail_terminal(claimed, &message).await
    } else {
        jobs.retry(claimed, Duration::seconds(5), &message).await
    };
    if let Err(update_error) = result {
        tracing::error!("Source index worker could not persist failure outcome: {update_error}");
    }
}

fn source_index_error_is_terminal(error: &SourceIndexError) -> bool {
    matches!(
        error,
        SourceIndexError::NoMedia
            | SourceIndexError::InvalidMediaName(_)
            | SourceIndexError::InvalidNamingHints
            | SourceIndexError::Repository(
                SourceIndexRepositoryError::InvalidClaim
                    | SourceIndexRepositoryError::MissingSyncRevision
                    | SourceIndexRepositoryError::MissingScope
            )
            | SourceIndexError::Publication(
                CatalogPublicationError::InvalidStructureRow
                    | CatalogPublicationError::InvalidManifest
                    | CatalogPublicationError::InvalidWorkKind
                    | CatalogPublicationError::InvalidPublication
                    | CatalogPublicationError::ManifestMismatch
                    | CatalogPublicationError::InvalidStructure
                    | CatalogPublicationError::InvalidSourceRow
                    | CatalogPublicationError::InvalidSourceManifest
                    | CatalogPublicationError::InvalidSourceGraph
                    | CatalogPublicationError::StableIdentityConflict
                    | CatalogPublicationError::UnauthorizedStorageObject
                    | CatalogPublicationError::StaleExpectedRevision
                    | CatalogPublicationError::MissingCatalogState
            )
    )
}

async fn run_storage_worker<Backend>(
    database: DatabaseConnection,
    account_id: Uuid,
    provider_drive_id: Option<String>,
    scoped: ScopedInventoryService<Backend>,
    validation: FullValidateStorageService<Backend>,
) where
    Backend: StorageBackend + ?Sized,
{
    let owner = format!("storage-worker-{account_id}-{}", Uuid::new_v4());
    let mut idle = tjxy_db::WorkQueueWaiter::default();
    loop {
        let jobs = WorkJobRepository::new(&database);
        let claim = match provider_drive_id.as_deref() {
            Some(drive) => {
                jobs.claim_next_scoped_sync_for_drive(account_id, drive, &owner, LEASE_DURATION)
                    .await
            }
            None => {
                jobs.claim_next_scoped_sync(account_id, &owner, LEASE_DURATION)
                    .await
            }
        };
        match claim {
            Ok(Some(claimed)) => {
                idle.reset();
                let execution = async {
                    if claimed.job().task_kind() == WorkTaskKind::ValidateStorageRoot {
                        let started = Instant::now();
                        validation
                            .run_claimed(&claimed, account_id)
                            .await
                            .map(|report| {
                                tracing::info!(
                                    storage_account_id = %account_id,
                                    directories_scanned = report.directory_count(),
                                    objects_scanned = report.object_count(),
                                    sync_revision = report.sync_revision(),
                                    elapsed_ms = started.elapsed().as_millis(),
                                    "Filesystem root validation completed"
                                );
                            })
                            .map_err(StorageWorkerError::Validation)
                    } else {
                        scoped
                            .run_claimed(&claimed, account_id)
                            .await
                            .map(|_| ())
                            .map_err(StorageWorkerError::Scoped)
                    }
                };
                let outcome =
                    Box::pin(renew_scan_execution(&database, &claimed, execution, |_| {
                        StorageWorkerError::LostLease
                    }))
                    .await;
                handle_storage_outcome(&scoped, &jobs, &claimed, outcome).await;
            }
            Ok(None) => idle.wait().await,
            Err(error) => {
                tracing::error!("Storage worker could not claim work: {error}");
                tokio::time::sleep(StdDuration::from_secs(1)).await;
            }
        }
    }
}

async fn handle_storage_outcome<Backend>(
    service: &ScopedInventoryService<Backend>,
    jobs: &WorkJobRepository<'_>,
    claimed: &tjxy_db::ClaimedWorkJob,
    outcome: Result<(), StorageWorkerError>,
) where
    Backend: StorageBackend + ?Sized,
{
    let Err(error) = outcome else { return };
    match error {
        StorageWorkerError::LostLease
        | StorageWorkerError::Scoped(
            ScopedInventoryError::WorkJob(WorkJobRepositoryError::LostLease)
            | ScopedInventoryError::Persistence(StorageSyncRepositoryError::LostLease),
        )
        | StorageWorkerError::Validation(
            FullValidateStorageError::Work(WorkJobRepositoryError::LostLease)
            | FullValidateStorageError::Persistence(StorageSyncRepositoryError::LostLease),
        ) => {}
        StorageWorkerError::Scoped(error) => {
            let message = if claimed.attempt_count() > tjxy_db::MAX_WORK_JOB_RETRIES {
                truncate_error(&format!(
                    "retry limit exceeded (5 retries); skipped: {error}"
                ))
            } else {
                truncate_error(&error.to_string())
            };
            if storage_error_is_terminal(&error)
                || claimed.attempt_count() > tjxy_db::MAX_WORK_JOB_RETRIES
            {
                log_terminal_failure(claimed, "storage synchronization failed terminally", &error);
                if let Err(update_error) = service.fail_terminal(claimed, &message).await {
                    tracing::error!(
                        "Storage worker could not persist terminal outcome: {update_error}"
                    );
                }
            } else if let Err(update_error) = jobs
                .retry(
                    claimed,
                    storage_retry_delay(&error, claimed.attempt_count()),
                    &message,
                )
                .await
            {
                tracing::error!("Storage worker could not persist failure outcome: {update_error}");
            }
        }
        StorageWorkerError::Validation(error) => {
            let message = truncate_error(&error.to_string());
            let terminal = validation_error_is_terminal(&error);
            let result = if terminal {
                log_terminal_failure(claimed, "storage validation failed terminally", &error);
                jobs.fail_terminal(claimed, &message).await
            } else {
                jobs.retry(
                    claimed,
                    validation_retry_delay(&error, claimed.attempt_count()),
                    &message,
                )
                .await
            };
            if let Err(update_error) = result {
                tracing::error!(
                    "Storage validation worker could not persist outcome: {update_error}"
                );
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
enum StorageWorkerError {
    #[error("storage worker lost its lease")]
    LostLease,
    #[error(transparent)]
    Scoped(ScopedInventoryError),
    #[error(transparent)]
    Validation(FullValidateStorageError),
}

fn storage_retry_delay(error: &ScopedInventoryError, attempt_count: i32) -> Duration {
    if matches!(
        error,
        ScopedInventoryError::Persistence(StorageSyncRepositoryError::RevisionConflict)
    ) {
        return revision_conflict_retry_delay(attempt_count);
    }
    let backoff = storage_backoff(attempt_count);
    let retry_after = match error {
        ScopedInventoryError::Backend(BackendError::RateLimited {
            retry_after: Some(delay),
        }) => Duration::from_std(*delay).unwrap_or_else(|_| Duration::minutes(5)),
        _ => return backoff,
    };
    backoff.max(retry_after)
}

fn validation_retry_delay(error: &FullValidateStorageError, attempt_count: i32) -> Duration {
    if matches!(
        error,
        FullValidateStorageError::Persistence(StorageSyncRepositoryError::RevisionConflict)
    ) {
        return revision_conflict_retry_delay(attempt_count);
    }
    let backoff = storage_backoff(attempt_count);
    let retry_after = match error {
        FullValidateStorageError::Backend(BackendError::RateLimited {
            retry_after: Some(delay),
        }) => Duration::from_std(*delay).unwrap_or_else(|_| Duration::minutes(5)),
        _ => return backoff,
    };
    backoff.max(retry_after)
}

fn revision_conflict_retry_delay(attempt_count: i32) -> Duration {
    if attempt_count <= 3 {
        return Duration::milliseconds(200);
    }
    let exponent = u32::try_from(attempt_count.saturating_sub(4))
        .unwrap_or_default()
        .min(8);
    Duration::seconds((1_i64 << exponent).min(300))
}

fn storage_backoff(attempt_count: i32) -> Duration {
    let exponent = u32::try_from(attempt_count.saturating_sub(1))
        .unwrap_or_default()
        .min(6);
    Duration::seconds((5_i64 * (1_i64 << exponent)).min(300))
}

fn storage_error_is_terminal(error: &ScopedInventoryError) -> bool {
    matches!(
        error,
        ScopedInventoryError::MissingInventoryTarget
            | ScopedInventoryError::WrongStorageAccount
            | ScopedInventoryError::InvalidPagination
            | ScopedInventoryError::ObjectCountOverflow
            | ScopedInventoryError::Backend(
                BackendError::UnsupportedCapability { .. }
                    | BackendError::InvalidValue { .. }
                    | BackendError::NotFound
                    | BackendError::RangeNotSatisfiable { .. }
            )
            | ScopedInventoryError::Persistence(
                StorageSyncRepositoryError::InvalidPage
                    | StorageSyncRepositoryError::InvalidClaimScope
                    | StorageSyncRepositoryError::MissingScope
                    | StorageSyncRepositoryError::ProviderMismatch
                    | StorageSyncRepositoryError::InvalidObjectSize
                    | StorageSyncRepositoryError::InvalidRevision
                    | StorageSyncRepositoryError::PageReplayConflict
                    | StorageSyncRepositoryError::InvalidStoredIdentity
            )
    )
}

fn validation_error_is_terminal(error: &FullValidateStorageError) -> bool {
    matches!(
        error,
        FullValidateStorageError::InvalidClaim
            | FullValidateStorageError::MissingInventoryTarget
            | FullValidateStorageError::WrongStorageAccount
            | FullValidateStorageError::DirectoryLimit
            | FullValidateStorageError::InvalidPagination
            | FullValidateStorageError::CountOverflow
            | FullValidateStorageError::MissingValidationRevision
            | FullValidateStorageError::Backend(
                BackendError::UnsupportedCapability { .. }
                    | BackendError::InvalidValue { .. }
                    | BackendError::NotFound
                    | BackendError::RangeNotSatisfiable { .. }
            )
            | FullValidateStorageError::Persistence(
                StorageSyncRepositoryError::InvalidPage
                    | StorageSyncRepositoryError::InvalidClaimScope
                    | StorageSyncRepositoryError::MissingScope
                    | StorageSyncRepositoryError::ProviderMismatch
                    | StorageSyncRepositoryError::InvalidObjectSize
                    | StorageSyncRepositoryError::InvalidRevision
                    | StorageSyncRepositoryError::PageReplayConflict
                    | StorageSyncRepositoryError::InvalidStoredIdentity
            )
    )
}

async fn run_probe_worker(database: DatabaseConnection, service: Arc<ProbeService>) {
    let owner = format!("probe-worker-{}", Uuid::new_v4());
    let mut idle = tjxy_db::WorkQueueWaiter::default();
    loop {
        let jobs = WorkJobRepository::new(&database);
        match jobs
            .claim_next_foreground(&[WorkTaskKind::ProbeMedia], &owner, LEASE_DURATION)
            .await
        {
            Ok(Some(claimed)) => {
                idle.reset();
                let execution = execute_logged(&claimed, service.execute(&claimed));
                let mut execution = pin!(execution);
                let mut renewal = tokio::time::interval(StdDuration::from_secs(60));
                renewal.tick().await;
                let outcome = loop {
                    tokio::select! {
                        result = &mut execution => break result,
                        _ = renewal.tick() => {
                            if jobs.renew(&claimed, LEASE_DURATION).await.is_err() {
                                break Err(ProbeServiceError::Repository(
                                    tjxy_db::ProbeRepositoryError::Work(
                                        WorkJobRepositoryError::LostLease,
                                    ),
                                ));
                            }
                        }
                    }
                };
                handle_outcome(&jobs, &claimed, outcome).await;
            }
            Ok(None) => idle.wait().await,
            Err(error) => {
                tracing::error!("Probe worker could not claim work: {error}");
                tokio::time::sleep(StdDuration::from_secs(1)).await;
            }
        }
    }
}

async fn run_metadata_worker(database: DatabaseConnection, service: Arc<MetadataResolveService>) {
    let owner = format!("metadata-worker-{}", Uuid::new_v4());
    let mut idle = tjxy_db::WorkQueueWaiter::default();
    loop {
        let jobs = WorkJobRepository::new(&database);
        match jobs
            .claim_next_foreground(&[WorkTaskKind::ResolveMetadata], &owner, LEASE_DURATION)
            .await
        {
            Ok(Some(claimed)) => {
                idle.reset();
                let execution = execute_logged(&claimed, service.execute(&claimed));
                let mut execution = pin!(execution);
                let mut renewal = tokio::time::interval(StdDuration::from_secs(60));
                renewal.tick().await;
                let outcome = loop {
                    tokio::select! {
                        result = &mut execution => break result,
                        _ = renewal.tick() => {
                            if jobs.renew(&claimed, LEASE_DURATION).await.is_err() {
                                break Err(MetadataResolveError::Work(
                                    MetadataWorkError::Work(WorkJobRepositoryError::LostLease),
                                ));
                            }
                        }
                    }
                };
                handle_metadata_outcome(&jobs, &claimed, outcome).await;
            }
            Ok(None) => idle.wait().await,
            Err(error) => {
                tracing::error!("Metadata worker could not claim work: {error}");
                tokio::time::sleep(StdDuration::from_secs(1)).await;
            }
        }
    }
}

async fn handle_metadata_outcome(
    jobs: &WorkJobRepository<'_>,
    claimed: &tjxy_db::ClaimedWorkJob,
    outcome: Result<tjxy_application::MetadataResolveReport, MetadataResolveError>,
) {
    let Err(error) = outcome else { return };
    if matches!(
        error,
        MetadataResolveError::Work(MetadataWorkError::Work(WorkJobRepositoryError::LostLease))
    ) {
        return;
    }
    let message = truncate_error(&error.to_string());
    let result = if metadata_error_is_terminal(&error) {
        log_terminal_failure(claimed, "metadata resolution failed terminally", &error);
        if matches!(
            error,
            MetadataResolveError::NfoSelectionRequired
                | MetadataResolveError::NfoKindMismatch
                | MetadataResolveError::Metadata(_)
                | MetadataResolveError::Work(
                    MetadataWorkError::AmbiguousSidecars
                        | MetadataWorkError::InvalidSidecarSize
                        | MetadataWorkError::TooManySidecars
                )
        ) {
            jobs.fail_item(
                claimed,
                &message,
                matches!(
                    error,
                    MetadataResolveError::NfoSelectionRequired
                        | MetadataResolveError::Work(MetadataWorkError::AmbiguousSidecars)
                ),
            )
            .await
        } else {
            jobs.fail_terminal(claimed, &message).await
        }
    } else if matches!(
        error,
        MetadataResolveError::Storage(BackendError::FilesystemIndexRebuilding)
    ) {
        let jitter = i64::from(claimed.id().as_uuid().as_bytes()[0] % 5);
        jobs.defer(claimed, Duration::seconds(5 + jitter), &message)
            .await
    } else {
        let jitter = i64::from(claimed.id().as_uuid().as_bytes()[0] % 5);
        jobs.retry(
            claimed,
            storage_backoff(claimed.attempt_count()) + Duration::seconds(jitter),
            &message,
        )
        .await
    };
    if let Err(update_error) = result
        && !matches!(update_error, WorkJobRepositoryError::LostLease)
    {
        tracing::error!("Metadata worker could not persist failure outcome: {update_error}");
    }
}

fn metadata_error_is_terminal(error: &MetadataResolveError) -> bool {
    matches!(
        error,
        MetadataResolveError::ObjectChanged
            | MetadataResolveError::NfoSelectionRequired
            | MetadataResolveError::NfoKindMismatch
            | MetadataResolveError::Provider(
                tjxy_metadata::MetadataProviderError::Rejected
                    | tjxy_metadata::MetadataProviderError::InvalidResponse
            )
            | MetadataResolveError::Metadata(_)
            | MetadataResolveError::Storage(
                BackendError::FilesystemIndexFailed
                    | BackendError::UnsupportedCapability { .. }
                    | BackendError::InvalidValue { .. }
                    | BackendError::NotFound
                    | BackendError::RangeNotSatisfiable { .. }
            )
            | MetadataResolveError::Work(
                MetadataWorkError::InvalidClaim
                    | MetadataWorkError::MissingSyncRevision
                    | MetadataWorkError::StaleOrUnavailable
                    | MetadataWorkError::AmbiguousStorageScope
                    | MetadataWorkError::TooManySidecars
                    | MetadataWorkError::AmbiguousSidecars
                    | MetadataWorkError::InvalidSidecarSize
                    | MetadataWorkError::InvalidStoredMetadata
                    | MetadataWorkError::Publication(
                        MetadataPublicationError::ItemNotFound
                            | MetadataPublicationError::ItemKindMismatch
                            | MetadataPublicationError::InvalidResolution
                    )
            )
    )
}

async fn handle_outcome(
    jobs: &WorkJobRepository<'_>,
    claimed: &tjxy_db::ClaimedWorkJob,
    outcome: Result<i64, ProbeServiceError>,
) {
    match outcome {
        Ok(_)
        | Err(ProbeServiceError::Repository(tjxy_db::ProbeRepositoryError::Work(
            WorkJobRepositoryError::LostLease,
        ))) => {}
        Err(ProbeServiceError::InspectionFailed(error)) => {
            tracing::error!(
                job_id = %claimed.id().as_uuid(),
                media_source_id = %claimed.job().scope().id(),
                attempt = claimed.attempt_count(),
                error = %error,
                "media probe inspection failed"
            );
        }
        Err(error) => {
            let message = error.to_string();
            let message = truncate_error(&message);
            tracing::debug!(
                job_id = %claimed.id().as_uuid(),
                media_source_id = %claimed.job().scope().id(),
                attempt = claimed.attempt_count(),
                error = %error,
                "media probe deferred or failed; retry scheduled"
            );
            if let Err(retry_error) = jobs.retry(claimed, Duration::seconds(5), &message).await {
                tracing::error!("Probe worker could not schedule a retry: {retry_error}");
            }
        }
    }
}

fn log_terminal_failure(
    claimed: &tjxy_db::ClaimedWorkJob,
    code: &'static str,
    error: &impl std::fmt::Display,
) {
    tracing::error!(job_id = %claimed.id().as_uuid(), scope_id = %claimed.job().scope().id(),
        scope_type = claimed.job().scope().scope_type(), stage = claimed.job().task_kind().as_str(),
        expected_revision = claimed.job().expected_revision(), error_code = code,
        terminal = true, attempt = claimed.attempt_count(), error = %truncate_error(&error.to_string()),
        "work failed terminally");
}

fn truncate_error(error: &str) -> String {
    crate::log_record::redact_urls(error)
        .chars()
        .take(4096)
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn failed_index_and_deterministic_full_scan_conflicts_are_terminal() {
        assert!(super::metadata_error_is_terminal(
            &super::MetadataResolveError::Storage(super::BackendError::FilesystemIndexFailed)
        ));
        assert!(!super::metadata_error_is_terminal(
            &super::MetadataResolveError::Storage(super::BackendError::FilesystemIndexRebuilding)
        ));
        assert!(super::full_scan_error_is_terminal(
            &super::FullScanError::Work(super::WorkJobRepositoryError::IncompatibleActiveJob)
        ));
    }
    use std::time::Duration as StdDuration;

    use tjxy_application::{FullValidateStorageError, ScopedInventoryError};
    use tjxy_storage::BackendError;

    use super::{
        discover_error_is_terminal, discover_retry_delay, full_scan_error_is_terminal,
        full_scan_retry_delay, series_expand_error_is_terminal, storage_retry_delay,
        validation_error_is_terminal, validation_retry_delay,
    };

    #[test]
    fn pending_series_inventory_is_retried_instead_of_failed() {
        assert!(!series_expand_error_is_terminal(
            &tjxy_application::SeriesExpandError::InventoryPending { scheduled: 1 }
        ));
    }

    #[test]
    fn pending_full_scan_children_are_retried_instead_of_failed() {
        assert!(!full_scan_error_is_terminal(
            &tjxy_application::FullScanError::ChildrenPending { scheduled: 1 }
        ));
    }

    #[test]
    fn stale_discovery_is_terminal() {
        assert!(discover_error_is_terminal(
            &tjxy_application::DiscoverTitlesServiceError::Repository(
                tjxy_db::DiscoverTitlesError::StaleRoot,
            )
        ));
    }

    #[test]
    fn discovery_retries_use_capped_exponential_backoff() {
        assert_eq!(discover_retry_delay(1).num_seconds(), 5);
        assert_eq!(discover_retry_delay(4).num_seconds(), 40);
        assert_eq!(discover_retry_delay(20).num_seconds(), 300);
    }

    #[test]
    fn pending_full_scan_children_keep_a_short_loss_tolerant_poll() {
        let error = tjxy_application::FullScanError::ChildrenPending { scheduled: 1 };

        assert_eq!(full_scan_retry_delay(&error, 1).num_seconds(), 2);
        assert_eq!(full_scan_retry_delay(&error, 4).num_seconds(), 2);
        assert_eq!(full_scan_retry_delay(&error, 20).num_seconds(), 2);
    }

    #[test]
    fn invalid_hybrid_candidate_limit_is_terminal() {
        assert!(full_scan_error_is_terminal(
            &tjxy_application::FullScanError::Repository(
                tjxy_db::FullScanRepositoryError::InvalidCandidateLimit,
            )
        ));
    }

    #[test]
    fn storage_retry_uses_capped_exponential_backoff() {
        let error = ScopedInventoryError::Backend(BackendError::TemporarilyUnavailable {
            message: "temporary".into(),
        });

        assert_eq!(storage_retry_delay(&error, 1).num_seconds(), 5);
        assert_eq!(storage_retry_delay(&error, 4).num_seconds(), 40);
        assert_eq!(storage_retry_delay(&error, 20).num_seconds(), 300);
    }

    #[test]
    fn retry_after_is_not_shortened_by_exponential_backoff() {
        let error = ScopedInventoryError::Backend(BackendError::RateLimited {
            retry_after: Some(StdDuration::from_secs(90)),
        });

        assert_eq!(storage_retry_delay(&error, 2).num_seconds(), 90);
    }

    #[test]
    fn local_root_revision_conflicts_only_retry_quickly_at_first() {
        let error = ScopedInventoryError::Persistence(
            tjxy_db::StorageSyncRepositoryError::RevisionConflict,
        );

        assert_eq!(storage_retry_delay(&error, 3).num_milliseconds(), 200);
        assert_eq!(storage_retry_delay(&error, 4).num_seconds(), 1);
        assert_eq!(storage_retry_delay(&error, 20).num_seconds(), 256);
    }

    #[test]
    fn validation_structure_errors_are_terminal() {
        assert!(validation_error_is_terminal(
            &FullValidateStorageError::DirectoryLimit
        ));
        assert!(validation_error_is_terminal(
            &FullValidateStorageError::Backend(BackendError::NotFound)
        ));
    }

    #[test]
    fn validation_rate_limits_preserve_retry_after() {
        let error = FullValidateStorageError::Backend(BackendError::RateLimited {
            retry_after: Some(StdDuration::from_secs(75)),
        });

        assert!(!validation_error_is_terminal(&error));
        assert_eq!(validation_retry_delay(&error, 2).num_seconds(), 75);
    }
}
