use std::sync::{Arc, Mutex};

use chrono::{Duration, TimeZone, Utc};
use sea_orm::{
    ConnectionTrait, DatabaseConnection, TransactionTrait,
    sea_query::{Alias, Expr, Query},
};
use sea_orm_migration::MigratorTrait;
use serde_json::json;
use tjxy_common::{CatalogItemId, LibraryId, SortKey, StorageObjectRecordId, StorageRootId};
use tjxy_db::{
    ClaimedWorkJob, FullScanChildSubmission, MetadataRequirement, WorkJobAdminOutcome,
    WorkJobAdminStatus, WorkJobClock, WorkJobRepository, WorkJobRepositoryError, WorkJobResult,
    WorkJobSpec, WorkJobState, WorkScope, WorkStagingRow, WorkTaskKind,
};
use tjxy_domain::MetadataSourceMode;
use tjxy_test_support::test_database;
use uuid::Uuid;

#[derive(Clone)]
struct ManualClock(Arc<Mutex<chrono::DateTime<Utc>>>);

impl ManualClock {
    fn new(now: chrono::DateTime<Utc>) -> Self {
        Self(Arc::new(Mutex::new(now)))
    }

    fn set(&self, now: chrono::DateTime<Utc>) {
        *self.0.lock().unwrap() = now;
    }
}

impl WorkJobClock for ManualClock {
    fn now(&self) -> chrono::DateTime<Utc> {
        *self.0.lock().unwrap()
    }
}

async fn database() -> DatabaseConnection {
    let database = test_database().await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    database
}

#[tokio::test]
async fn storage_root_affinity_is_durable_but_keeps_publication_work_globally_single_flight() {
    let database = database().await;
    let repository = WorkJobRepository::new(&database);
    let item = CatalogItemId::new();
    let first_root = StorageRootId::new();
    let second_root = StorageRootId::new();
    let first = repository
        .enqueue_or_join(
            &WorkJobSpec::new(
                WorkTaskKind::ExpandItem,
                WorkScope::CatalogItem(item),
                3,
                100,
            )
            .unwrap()
            .with_input_sync_revision(7)
            .unwrap()
            .with_storage_root_affinity(first_root)
            .unwrap(),
        )
        .await
        .unwrap();
    let second = repository
        .enqueue_or_join(
            &WorkJobSpec::new(
                WorkTaskKind::ExpandItem,
                WorkScope::CatalogItem(item),
                3,
                100,
            )
            .unwrap()
            .with_input_sync_revision(7)
            .unwrap()
            .with_storage_root_affinity(second_root)
            .unwrap(),
        )
        .await
        .unwrap_err();

    assert_eq!(first.job().storage_root_affinity(), Some(first_root));
    assert!(matches!(
        second,
        WorkJobRepositoryError::IncompatibleActiveJob
    ));
}

#[tokio::test]
async fn scoped_storage_sync_is_single_flight_per_storage_root() {
    let database = database().await;
    let repository = WorkJobRepository::new(&database);
    let scope = StorageObjectRecordId::new();
    let first_root = StorageRootId::new();
    let second_root = StorageRootId::new();
    let first = repository
        .enqueue_or_join(
            &WorkJobSpec::new(
                WorkTaskKind::ScopedStorageSync,
                WorkScope::StorageObject(scope),
                3,
                100,
            )
            .unwrap()
            .with_storage_root_affinity(first_root)
            .unwrap(),
        )
        .await
        .unwrap();
    let second = repository
        .enqueue_or_join(
            &WorkJobSpec::new(
                WorkTaskKind::ScopedStorageSync,
                WorkScope::StorageObject(scope),
                3,
                100,
            )
            .unwrap()
            .with_storage_root_affinity(second_root)
            .unwrap(),
        )
        .await
        .unwrap();

    assert_ne!(first.job().id(), second.job().id());
    assert_eq!(first.job().storage_root_affinity(), Some(first_root));
    assert_eq!(second.job().storage_root_affinity(), Some(second_root));
}

fn repository(
    database: &DatabaseConnection,
    now: chrono::DateTime<Utc>,
) -> (WorkJobRepository<'_, ManualClock>, ManualClock) {
    let clock = ManualClock::new(now);
    (
        WorkJobRepository::with_clock(database, clock.clone()),
        clock,
    )
}

async fn complete(
    repository: &WorkJobRepository<'_, ManualClock>,
    database: &DatabaseConnection,
    claimed: &ClaimedWorkJob,
) {
    complete_with_result(
        repository,
        database,
        claimed,
        WorkJobResult::success(json!({"written": 3}), vec!["fixture warning".to_owned()]),
    )
    .await;
}

async fn complete_with_result(
    repository: &WorkJobRepository<'_, ManualClock>,
    database: &DatabaseConnection,
    claimed: &ClaimedWorkJob,
    result: WorkJobResult,
) {
    let transaction = database.begin().await.unwrap();
    repository
        .complete_in_transaction(&transaction, claimed, result)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
}

#[tokio::test]
async fn enqueue_joins_the_active_natural_key_and_promotes_priority() {
    let database = database().await;
    let now = Utc.with_ymd_and_hms(2026, 7, 18, 9, 0, 0).unwrap();
    let (repository, _) = repository(&database, now);
    let item_id = CatalogItemId::new();
    let background = WorkJobSpec::new(
        WorkTaskKind::ExpandItem,
        WorkScope::CatalogItem(item_id),
        7,
        10,
    )
    .unwrap();
    let interactive = WorkJobSpec::new(
        WorkTaskKind::ExpandItem,
        WorkScope::CatalogItem(item_id),
        7,
        100,
    )
    .unwrap();

    let first = repository.enqueue_or_join(&background).await.unwrap();
    let joined = repository.enqueue_or_join(&interactive).await.unwrap();

    assert!(first.created());
    assert!(!joined.created());
    assert_eq!(first.job().id(), joined.job().id());
    assert_eq!(joined.job().priority(), 100);
    assert_eq!(joined.job().state(), WorkJobState::Pending);
    let backend = database.get_database_backend();
    let count: i64 = database
        .query_one(
            backend.build(
                Query::select()
                    .expr_as(Expr::col(Alias::new("id")).count(), Alias::new("count"))
                    .from(Alias::new("work_jobs")),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get("", "count")
        .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn metadata_join_upgrades_the_active_requirement_without_allowing_a_downgrade() {
    let database = database().await;
    let now = Utc.with_ymd_and_hms(2026, 7, 25, 9, 0, 0).unwrap();
    let (repository, _) = repository(&database, now);
    let item_id = CatalogItemId::new();
    let basic = WorkJobSpec::new(
        WorkTaskKind::ResolveMetadata,
        WorkScope::CatalogItem(item_id),
        7,
        10,
    )
    .unwrap()
    .with_metadata_requirement(MetadataRequirement::Basic)
    .unwrap()
    .with_metadata_source_mode(MetadataSourceMode::LocalOnly)
    .unwrap();
    let full = WorkJobSpec::new(
        WorkTaskKind::ResolveMetadata,
        WorkScope::CatalogItem(item_id),
        7,
        20,
    )
    .unwrap()
    .with_metadata_requirement(MetadataRequirement::Full)
    .unwrap()
    .with_metadata_source_mode(MetadataSourceMode::AutomaticScrape)
    .unwrap();

    let created = repository.enqueue_or_join(&basic).await.unwrap();
    let upgraded = repository.enqueue_or_join(&full).await.unwrap();
    let joined = repository.enqueue_or_join(&basic).await.unwrap();

    assert!(created.created());
    assert!(!upgraded.created());
    assert!(!joined.created());
    assert_eq!(created.job().id(), upgraded.job().id());
    assert_eq!(
        upgraded.job().metadata_source_mode(),
        Some(MetadataSourceMode::AutomaticScrape)
    );
    assert_eq!(
        upgraded.job().metadata_requirement(),
        Some(MetadataRequirement::Full)
    );
    assert_eq!(
        joined.job().metadata_requirement(),
        Some(MetadataRequirement::Full)
    );
    let claimed = repository
        .claim_next(
            &[WorkTaskKind::ResolveMetadata],
            "metadata-requirement-test",
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        claimed.job().metadata_requirement(),
        Some(MetadataRequirement::Full)
    );
    assert_eq!(
        claimed.job().metadata_source_mode(),
        Some(MetadataSourceMode::AutomaticScrape)
    );
}

#[tokio::test]
async fn recent_jobs_are_bounded_ordered_and_never_expose_persisted_error_text() {
    let database = database().await;
    let now = Utc.with_ymd_and_hms(2026, 7, 24, 9, 0, 0).unwrap();
    let (repository, clock) = repository(&database, now);
    let first = repository
        .enqueue_or_join(
            &WorkJobSpec::new(
                WorkTaskKind::FullMediaScan,
                WorkScope::Library(LibraryId::new()),
                1,
                20,
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let claimed = repository
        .claim_next(
            &[WorkTaskKind::FullMediaScan],
            "admin-observation-test",
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    repository
        .fail_terminal(&claimed, "token=secret must not cross the admin boundary")
        .await
        .unwrap();

    clock.set(Utc.with_ymd_and_hms(2026, 7, 24, 9, 1, 0).unwrap());
    let second = repository
        .enqueue_or_join(
            &WorkJobSpec::new(
                WorkTaskKind::ProbeMedia,
                WorkScope::CatalogItem(CatalogItemId::new()),
                2,
                100,
            )
            .unwrap(),
        )
        .await
        .unwrap();

    let recent = repository.recent_jobs(10).await.unwrap();
    assert_eq!(recent.len(), 2);
    assert_eq!(recent[0].job().id(), second.job().id());
    assert_eq!(recent[0].admin_status(), WorkJobAdminStatus::Pending);
    assert_eq!(recent[1].job().id(), first.job().id());
    assert_eq!(recent[1].admin_status(), WorkJobAdminStatus::Failed);
    assert_eq!(recent[1].completed_at(), Some(now));
    assert!(matches!(
        repository.recent_jobs(0).await,
        Err(WorkJobRepositoryError::InvalidObservationLimit)
    ));
    assert!(matches!(
        repository.recent_jobs(101).await,
        Err(WorkJobRepositoryError::InvalidObservationLimit)
    ));
}

#[tokio::test]
async fn recent_jobs_classify_partial_automatic_metadata_without_exposing_result_details() {
    let database = database().await;
    let now = Utc.with_ymd_and_hms(2026, 7, 24, 10, 0, 0).unwrap();
    let (repository, _) = repository(&database, now);
    let submitted = repository
        .enqueue_or_join(
            &WorkJobSpec::new(
                WorkTaskKind::ResolveMetadata,
                WorkScope::CatalogItem(CatalogItemId::new()),
                1,
                100,
            )
            .unwrap()
            .with_metadata_requirement(MetadataRequirement::Basic)
            .unwrap()
            .with_metadata_source_mode(MetadataSourceMode::AutomaticScrape)
            .unwrap(),
        )
        .await
        .unwrap();
    let claimed = repository
        .claim_next(
            &[WorkTaskKind::ResolveMetadata],
            "admin-outcome-test",
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    complete_with_result(
        &repository,
        &database,
        &claimed,
        WorkJobResult::success(
            json!({
                "matched": false,
                "state": "Partial",
                "provider_debug": "must stay private"
            }),
            Vec::new(),
        ),
    )
    .await;

    let recent = repository.recent_jobs(1).await.unwrap();
    assert_eq!(recent[0].job().id(), submitted.job().id());
    assert_eq!(
        recent[0].outcome(),
        Some(WorkJobAdminOutcome::NoMetadataMatch)
    );
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Keeps the effective-policy matrix and durable join assertions together.
async fn library_refresh_enqueues_each_enabled_library_and_joins_retries() {
    let database = database().await;
    let backend = database.get_database_backend();
    let enabled = LibraryId::new();
    let disabled = LibraryId::new();
    let manual_effective_policy = LibraryId::new();
    let manual_named_lazy_policy = LibraryId::new();
    for (id, name, profile, object_selection, metadata, expansion, probe, is_enabled, revision) in [
        (
            enabled,
            "Movies",
            "Lazy",
            "title_layer",
            "basic",
            "on_browse",
            "on_playback",
            true,
            7_i64,
        ),
        (
            disabled,
            "Archive",
            "Lazy",
            "title_layer",
            "basic",
            "on_browse",
            "on_playback",
            false,
            11_i64,
        ),
        (
            manual_effective_policy,
            "Curated",
            "Full",
            "library_roots",
            "none",
            "manual",
            "on_playback",
            true,
            13_i64,
        ),
        (
            manual_named_lazy_policy,
            "Overridden",
            "Manual",
            "title_layer",
            "basic",
            "on_browse",
            "on_playback",
            true,
            17_i64,
        ),
    ] {
        database
            .execute(
                backend.build(
                    Query::insert()
                        .into_table(Alias::new("libraries"))
                        .columns([
                            Alias::new("id"),
                            Alias::new("name"),
                            Alias::new("scan_profile"),
                            Alias::new("object_selection_scope"),
                            Alias::new("metadata_policy"),
                            Alias::new("expansion_policy"),
                            Alias::new("probe_policy"),
                            Alias::new("profile_version"),
                            Alias::new("is_enabled"),
                            Alias::new("sort_key"),
                        ])
                        .values_panic([
                            id.as_uuid().into(),
                            name.into(),
                            profile.into(),
                            object_selection.into(),
                            metadata.into(),
                            expansion.into(),
                            probe.into(),
                            revision.into(),
                            is_enabled.into(),
                            SortKey::from_text(name).into_bytes().into(),
                        ]),
                ),
            )
            .await
            .unwrap();
    }
    let repository = WorkJobRepository::new(&database);

    let first = repository.enqueue_enabled_library_scans(5).await.unwrap();
    let retry = repository.enqueue_enabled_library_scans(25).await.unwrap();
    let background_retry = repository.enqueue_enabled_library_scans(0).await.unwrap();

    assert_eq!(first.len(), 2);
    assert!(first.iter().all(tjxy_db::WorkJobSubmission::created));
    assert!(
        first
            .iter()
            .all(|submission| submission.job().task_kind() == WorkTaskKind::FullMediaScan)
    );
    let find_first = |scope| {
        first
            .iter()
            .find(|submission| submission.job().scope() == scope)
            .expect("expected refresh submission")
    };
    assert_eq!(
        find_first(WorkScope::Library(enabled))
            .job()
            .expected_revision(),
        7
    );
    assert_eq!(
        find_first(WorkScope::Library(manual_named_lazy_policy))
            .job()
            .expected_revision(),
        17
    );
    assert_eq!(retry.len(), 2);
    assert!(retry.iter().all(|submission| !submission.created()));
    for submission in retry {
        let original = find_first(submission.job().scope());
        assert_eq!(original.job().id(), submission.job().id());
        assert_eq!(submission.job().priority(), 25);
    }
    assert_eq!(background_retry.len(), 2);
    for submission in background_retry {
        let original = find_first(submission.job().scope());
        assert!(!submission.created());
        assert_eq!(original.job().id(), submission.job().id());
        assert_eq!(submission.job().priority(), 25);
    }
}

#[tokio::test]
async fn expired_claim_is_fenced_after_another_worker_takes_over() {
    let database = database().await;
    let now = Utc.with_ymd_and_hms(2026, 7, 18, 10, 0, 0).unwrap();
    let (repository, clock) = repository(&database, now);
    let fenced_job = repository
        .enqueue_or_join(
            &WorkJobSpec::new(
                WorkTaskKind::IndexMediaSources,
                WorkScope::CatalogItem(CatalogItemId::new()),
                1,
                80,
            )
            .unwrap(),
        )
        .await
        .unwrap();

    let stale = repository
        .claim_next(
            &[WorkTaskKind::IndexMediaSources],
            "media-worker-a",
            Duration::seconds(5),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stale.id(), fenced_job.job().id());
    clock.set(now + Duration::seconds(6));
    let current = repository
        .claim_next(
            &[WorkTaskKind::IndexMediaSources],
            "media-worker-b",
            Duration::seconds(5),
        )
        .await
        .unwrap()
        .unwrap();

    let transaction = database.begin().await.unwrap();
    let stale_error = repository
        .complete_in_transaction(
            &transaction,
            &stale,
            WorkJobResult::success(json!({}), Vec::new()),
        )
        .await
        .unwrap_err();
    transaction.rollback().await.unwrap();
    assert!(matches!(stale_error, WorkJobRepositoryError::LostLease));
    complete(&repository, &database, &current).await;
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Keeps the pending-sync lifecycle and revision capture in one contract.
async fn media_job_waits_durably_for_pending_sync_and_captures_its_committed_revision() {
    let database = database().await;
    let now = Utc.with_ymd_and_hms(2026, 7, 18, 10, 30, 0).unwrap();
    let (repository, _) = repository(&database, now);
    let scope = StorageObjectRecordId::new();
    let sync = repository
        .enqueue_or_join(
            &WorkJobSpec::new(
                WorkTaskKind::ScopedStorageSync,
                WorkScope::StorageObject(scope),
                4,
                50,
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let media = repository
        .enqueue_or_join(
            &WorkJobSpec::new(
                WorkTaskKind::ExpandItem,
                WorkScope::CatalogItem(CatalogItemId::new()),
                8,
                100,
            )
            .unwrap()
            .with_pending_required_sync(sync.job().id()),
        )
        .await
        .unwrap();

    assert_eq!(media.job().required_sync_job_id(), Some(sync.job().id()));
    assert_eq!(media.job().input_sync_revision(), None);
    assert!(
        repository
            .claim_next(
                &[WorkTaskKind::ExpandItem],
                "early-media-worker",
                Duration::seconds(30),
            )
            .await
            .unwrap()
            .is_none()
    );

    let account = Uuid::new_v4();
    let root = StorageRootId::new();
    let backend = database.get_database_backend();
    for statement in [
        Query::insert()
            .into_table(Alias::new("storage_accounts"))
            .columns([
                Alias::new("id"),
                Alias::new("provider"),
                Alias::new("display_name"),
                Alias::new("account_identity"),
                Alias::new("credential_ref"),
                Alias::new("status"),
            ])
            .values_panic([
                account.into(),
                "filesystem".into(),
                "Fixture".into(),
                Uuid::new_v4().to_string().into(),
                "fixture".into(),
                "Active".into(),
            ])
            .to_owned(),
        Query::insert()
            .into_table(Alias::new("storage_roots"))
            .columns([
                Alias::new("id"),
                Alias::new("storage_account_id"),
                Alias::new("provider_root_id"),
                Alias::new("sync_revision"),
                Alias::new("reconciled_sync_revision"),
            ])
            .values_panic([
                root.as_uuid().into(),
                account.into(),
                "root".into(),
                5_i64.into(),
                5_i64.into(),
            ])
            .to_owned(),
        Query::insert()
            .into_table(Alias::new("storage_objects"))
            .columns([
                Alias::new("id"),
                Alias::new("storage_account_id"),
                Alias::new("provider_drive_id"),
                Alias::new("provider_object_id"),
                Alias::new("name"),
                Alias::new("normalized_name"),
                Alias::new("object_type"),
                Alias::new("observed_sync_revision"),
                Alias::new("children_indexed"),
                Alias::new("children_index_revision"),
                Alias::new("identity_quality"),
                Alias::new("presence_state"),
            ])
            .values_panic([
                scope.as_uuid().into(),
                account.into(),
                "fixture-drive".into(),
                "scope".into(),
                "Scope".into(),
                "scope".into(),
                "Directory".into(),
                5_i64.into(),
                true.into(),
                5_i64.into(),
                "ProviderStable".into(),
                "Present".into(),
            ])
            .to_owned(),
        Query::insert()
            .into_table(Alias::new("storage_root_objects"))
            .columns([
                Alias::new("id"),
                Alias::new("storage_root_id"),
                Alias::new("storage_object_id"),
                Alias::new("observed_sync_revision"),
                Alias::new("children_indexed"),
                Alias::new("children_index_revision"),
                Alias::new("presence_state"),
            ])
            .values_panic([
                Uuid::new_v4().into(),
                root.as_uuid().into(),
                scope.as_uuid().into(),
                5_i64.into(),
                true.into(),
                5_i64.into(),
                "Present".into(),
            ])
            .to_owned(),
        Query::insert()
            .into_table(Alias::new("storage_sync_pages"))
            .columns([
                Alias::new("id"),
                Alias::new("job_id"),
                Alias::new("storage_root_id"),
                Alias::new("scope_storage_object_id"),
                Alias::new("page_identity"),
                Alias::new("payload_sha256"),
                Alias::new("sync_revision"),
                Alias::new("scope_completed"),
            ])
            .values_panic([
                Uuid::new_v4().into(),
                sync.job().id().as_uuid().into(),
                root.as_uuid().into(),
                scope.as_uuid().into(),
                "final".into(),
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
                5_i64.into(),
                true.into(),
            ])
            .to_owned(),
        Query::insert()
            .into_table(Alias::new("work_results"))
            .columns([
                Alias::new("id"),
                Alias::new("job_id"),
                Alias::new("counters"),
                Alias::new("warnings"),
                Alias::new("result_sync_revision"),
            ])
            .values_panic([
                Uuid::new_v4().into(),
                sync.job().id().as_uuid().into(),
                json!({}).into(),
                json!([]).into(),
                5_i64.into(),
            ])
            .to_owned(),
    ] {
        database.execute(backend.build(&statement)).await.unwrap();
    }
    database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("work_jobs"))
                    .value(Alias::new("state"), "Completed")
                    .and_where(Expr::col(Alias::new("id")).eq(sync.job().id().as_uuid())),
            ),
        )
        .await
        .unwrap();

    let claimed = repository
        .claim_next(
            &[WorkTaskKind::ExpandItem],
            "ready-media-worker",
            Duration::seconds(30),
        )
        .await
        .unwrap()
        .unwrap();

    assert_eq!(claimed.id(), media.job().id());
    assert_eq!(claimed.job().input_sync_revision(), Some(5));
}

#[tokio::test]
async fn media_job_rejects_a_dependency_that_is_not_scoped_storage_sync() {
    let database = database().await;
    let now = Utc.with_ymd_and_hms(2026, 7, 18, 10, 40, 0).unwrap();
    let (repository, _) = repository(&database, now);
    let wrong_dependency = repository
        .enqueue_or_join(
            &WorkJobSpec::new(
                WorkTaskKind::ProbeMedia,
                WorkScope::CatalogItem(CatalogItemId::new()),
                1,
                50,
            )
            .unwrap(),
        )
        .await
        .unwrap();

    let error = repository
        .enqueue_or_join(
            &WorkJobSpec::new(
                WorkTaskKind::ExpandItem,
                WorkScope::CatalogItem(CatalogItemId::new()),
                2,
                100,
            )
            .unwrap()
            .with_pending_required_sync(wrong_dependency.job().id()),
        )
        .await
        .unwrap_err();

    assert!(matches!(error, WorkJobRepositoryError::InvalidDependency));
}

#[tokio::test]
async fn media_job_rejects_a_scoped_sync_from_another_storage_root() {
    let database = database().await;
    let repository = WorkJobRepository::new(&database);
    let first_root = StorageRootId::new();
    let second_root = StorageRootId::new();
    let sync = repository
        .enqueue_or_join(
            &WorkJobSpec::new(
                WorkTaskKind::ScopedStorageSync,
                WorkScope::StorageObject(StorageObjectRecordId::new()),
                1,
                50,
            )
            .unwrap()
            .with_storage_root_affinity(first_root)
            .unwrap(),
        )
        .await
        .unwrap();
    let error = repository
        .enqueue_or_join(
            &WorkJobSpec::new(
                WorkTaskKind::ExpandItem,
                WorkScope::CatalogItem(CatalogItemId::new()),
                2,
                100,
            )
            .unwrap()
            .with_pending_required_sync(sync.job().id())
            .with_storage_root_affinity(second_root)
            .unwrap(),
        )
        .await
        .unwrap_err();

    assert!(matches!(error, WorkJobRepositoryError::InvalidDependency));
}

#[tokio::test]
async fn failed_sync_dependency_terminally_fails_the_waiting_media_job() {
    let database = database().await;
    let now = Utc.with_ymd_and_hms(2026, 7, 18, 10, 42, 0).unwrap();
    let (repository, _) = repository(&database, now);
    let sync = repository
        .enqueue_or_join(
            &WorkJobSpec::new(
                WorkTaskKind::ScopedStorageSync,
                WorkScope::StorageObject(StorageObjectRecordId::new()),
                1,
                100,
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let media = repository
        .enqueue_or_join(
            &WorkJobSpec::new(
                WorkTaskKind::IndexMediaSources,
                WorkScope::CatalogItem(CatalogItemId::new()),
                2,
                100,
            )
            .unwrap()
            .with_pending_required_sync(sync.job().id()),
        )
        .await
        .unwrap();
    let backend = database.get_database_backend();
    database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("work_jobs"))
                    .value(Alias::new("state"), "Failed")
                    .value(Alias::new("last_error"), "provider token must stay private")
                    .and_where(Expr::col(Alias::new("id")).eq(sync.job().id().as_uuid())),
            ),
        )
        .await
        .unwrap();

    assert!(
        repository
            .claim_next(
                &[WorkTaskKind::IndexMediaSources],
                "media-worker",
                Duration::seconds(30),
            )
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        repository
            .get(media.job().id())
            .await
            .unwrap()
            .unwrap()
            .state(),
        WorkJobState::Failed
    );
    let result = database
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("error_summary"))
                    .from(Alias::new("work_results"))
                    .and_where(Expr::col(Alias::new("job_id")).eq(media.job().id().as_uuid())),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        result.try_get::<String>("", "error_summary").unwrap(),
        "required scoped storage sync failed"
    );
}

#[tokio::test]
async fn terminal_failure_releases_the_natural_key_for_a_new_job() {
    let database = database().await;
    let now = Utc.with_ymd_and_hms(2026, 7, 18, 10, 45, 0).unwrap();
    let (repository, _) = repository(&database, now);
    let spec = WorkJobSpec::new(
        WorkTaskKind::ExpandItem,
        WorkScope::CatalogItem(CatalogItemId::new()),
        8,
        100,
    )
    .unwrap();
    let first = repository.enqueue_or_join(&spec).await.unwrap();
    let claimed = repository
        .claim_next(&[WorkTaskKind::ExpandItem], "worker", Duration::seconds(30))
        .await
        .unwrap()
        .unwrap();

    repository
        .fail_terminal(&claimed, "invalid staging manifest")
        .await
        .unwrap();
    let replacement = repository.enqueue_or_join(&spec).await.unwrap();

    assert!(replacement.created());
    assert_ne!(first.job().id(), replacement.job().id());
    assert_eq!(
        repository
            .get(first.job().id())
            .await
            .unwrap()
            .unwrap()
            .state(),
        WorkJobState::Failed
    );
}

#[tokio::test]
async fn renew_and_retry_use_fenced_leases_and_bounded_backoff() {
    let database = database().await;
    let now = Utc.with_ymd_and_hms(2026, 7, 18, 11, 0, 0).unwrap();
    let (repository, clock) = repository(&database, now);
    repository
        .enqueue_or_join(
            &WorkJobSpec::new(
                WorkTaskKind::IndexMediaSources,
                WorkScope::CatalogItem(CatalogItemId::new()),
                3,
                100,
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let claimed = repository
        .claim_next(
            &[WorkTaskKind::IndexMediaSources],
            "worker",
            Duration::seconds(5),
        )
        .await
        .unwrap()
        .unwrap();
    clock.set(now + Duration::seconds(4));
    repository
        .renew(&claimed, Duration::seconds(5))
        .await
        .unwrap();
    clock.set(now + Duration::seconds(6));
    assert!(
        repository
            .claim_next(
                &[WorkTaskKind::IndexMediaSources],
                "other",
                Duration::seconds(5),
            )
            .await
            .unwrap()
            .is_none()
    );
    repository
        .retry(&claimed, Duration::seconds(10), "provider unavailable")
        .await
        .unwrap();
    assert!(
        repository
            .claim_next(
                &[WorkTaskKind::IndexMediaSources],
                "early",
                Duration::seconds(5),
            )
            .await
            .unwrap()
            .is_none()
    );
    clock.set(now + Duration::seconds(16));
    let retried = repository
        .claim_next(
            &[WorkTaskKind::IndexMediaSources],
            "retry",
            Duration::seconds(5),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(retried.attempt_count(), 2);
}

#[tokio::test]
async fn staging_is_idempotent_and_completion_follows_the_callers_transaction() {
    let database = database().await;
    let now = Utc.with_ymd_and_hms(2026, 7, 18, 12, 0, 0).unwrap();
    let (repository, _) = repository(&database, now);
    let submitted = repository
        .enqueue_or_join(
            &WorkJobSpec::new(
                WorkTaskKind::ExpandItem,
                WorkScope::CatalogItem(CatalogItemId::new()),
                2,
                100,
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let claimed = repository
        .claim_next(&[WorkTaskKind::ExpandItem], "worker", Duration::seconds(30))
        .await
        .unwrap()
        .unwrap();
    let publication_id = Uuid::new_v4();
    repository
        .stage_batch(
            &claimed,
            publication_id,
            &[WorkStagingRow::new(
                "Episode",
                "season-1/episode-1",
                json!({"name": "old"}),
                "Pending",
            )
            .unwrap()],
        )
        .await
        .unwrap();
    repository
        .stage_batch(
            &claimed,
            publication_id,
            &[WorkStagingRow::new(
                "Episode",
                "season-1/episode-1",
                json!({"name": "new"}),
                "Validated",
            )
            .unwrap()],
        )
        .await
        .unwrap();

    let transaction = database.begin().await.unwrap();
    repository
        .complete_in_transaction(
            &transaction,
            &claimed,
            WorkJobResult::success(json!({"episodes": 1}), Vec::new()),
        )
        .await
        .unwrap();
    transaction.rollback().await.unwrap();
    assert_eq!(
        repository
            .get(submitted.job().id())
            .await
            .unwrap()
            .unwrap()
            .state(),
        WorkJobState::Running
    );

    complete(&repository, &database, &claimed).await;
    let backend = database.get_database_backend();
    let row = database
        .query_one(
            backend.build(
                Query::select()
                    .columns([Alias::new("payload"), Alias::new("validation_state")])
                    .from(Alias::new("work_staging_rows"))
                    .and_where(Expr::col(Alias::new("job_id")).eq(claimed.id().as_uuid())),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        row.try_get::<serde_json::Value>("", "payload").unwrap(),
        json!({"name": "new"})
    );
    assert_eq!(
        row.try_get::<String>("", "validation_state").unwrap(),
        "Validated"
    );
    assert_eq!(
        repository
            .get(submitted.job().id())
            .await
            .unwrap()
            .unwrap()
            .state(),
        WorkJobState::Completed
    );
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Keeps the completed publication race fixture in one transaction narrative.
async fn lazy_enqueue_skips_a_revision_that_is_already_published() {
    let database = database().await;
    let item = CatalogItemId::new();
    let backend = database.get_database_backend();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("catalog_items"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("item_type"),
                        Alias::new("name"),
                        Alias::new("sort_name"),
                        Alias::new("classification_state"),
                        Alias::new("metadata_state"),
                        Alias::new("structure_state"),
                        Alias::new("source_state"),
                        Alias::new("structure_expansion_revision"),
                        Alias::new("source_index_revision"),
                        Alias::new("is_present"),
                    ])
                    .values_panic([
                        item.as_uuid().into(),
                        "Series".into(),
                        "Series".into(),
                        "series".into(),
                        "Matched".into(),
                        "Ready".into(),
                        "Unexpanded".into(),
                        "Unknown".into(),
                        4_i64.into(),
                        0_i64.into(),
                        true.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    let jobs = WorkJobRepository::new(&database);
    let spec = WorkJobSpec::new(
        WorkTaskKind::ExpandItem,
        WorkScope::CatalogItem(item),
        4,
        100,
    )
    .unwrap();
    let old = jobs.enqueue_or_join(&spec).await.unwrap();
    database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("work_jobs"))
                    .value(Alias::new("state"), "Completed")
                    .and_where(Expr::col(Alias::new("id")).eq(old.job().id().as_uuid())),
            ),
        )
        .await
        .unwrap();
    let publication = Uuid::new_v4();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("catalog_publications"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("job_id"),
                        Alias::new("owner_catalog_item_id"),
                        Alias::new("publication_kind"),
                        Alias::new("expected_revision"),
                        Alias::new("state"),
                        Alias::new("manifest_sha256"),
                        Alias::new("expected_row_count"),
                    ])
                    .values_panic([
                        publication.into(),
                        old.job().id().as_uuid().into(),
                        item.as_uuid().into(),
                        "Structure".into(),
                        4_i64.into(),
                        "Active".into(),
                        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
                        0_i64.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("catalog_items"))
                    .value(Alias::new("active_structure_publication_id"), publication)
                    .and_where(Expr::col(Alias::new("id")).eq(item.as_uuid())),
            ),
        )
        .await
        .unwrap();

    assert!(jobs.enqueue_lazy_or_join(&spec).await.unwrap().is_none());
    let count = database
        .query_one(
            backend.build(
                Query::select()
                    .expr_as(Expr::col(Alias::new("id")).count(), Alias::new("job_count"))
                    .from(Alias::new("work_jobs")),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get::<i64>("", "job_count")
        .unwrap();
    assert_eq!(count, 1);

    let library = LibraryId::new();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("libraries"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("name"),
                        Alias::new("scan_profile"),
                        Alias::new("object_selection_scope"),
                        Alias::new("metadata_policy"),
                        Alias::new("expansion_policy"),
                        Alias::new("probe_policy"),
                        Alias::new("profile_version"),
                        Alias::new("collection_type"),
                        Alias::new("sort_key"),
                    ])
                    .values_panic([
                        library.as_uuid().into(),
                        "Series".into(),
                        "Full".into(),
                        "all_synced_objects".into(),
                        "full".into(),
                        "eager".into(),
                        "eager".into(),
                        1_i64.into(),
                        "tvshows".into(),
                        SortKey::from_text("Series").into_bytes().into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    let parent = jobs
        .enqueue_or_join(
            &WorkJobSpec::new(
                WorkTaskKind::FullMediaScan,
                WorkScope::Library(library),
                1,
                50,
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let claimed = jobs
        .claim_next(
            &[WorkTaskKind::FullMediaScan],
            "full-scan-worker",
            Duration::minutes(1),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(claimed.id(), parent.job().id());

    assert_eq!(
        jobs.enqueue_full_scan_child(&claimed, "ExpandItem:published:4", &spec)
            .await
            .unwrap(),
        FullScanChildSubmission::Current
    );
    let count = database
        .query_one(
            backend.build(
                Query::select()
                    .expr_as(Expr::col(Alias::new("id")).count(), Alias::new("job_count"))
                    .from(Alias::new("work_jobs")),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get::<i64>("", "job_count")
        .unwrap();
    assert_eq!(count, 2);

    database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("catalog_items"))
                    .value(Alias::new("structure_expansion_revision"), 5_i64)
                    .and_where(Expr::col(Alias::new("id")).eq(item.as_uuid())),
            ),
        )
        .await
        .unwrap();
    assert_eq!(
        jobs.enqueue_full_scan_child(&claimed, "ExpandItem:stale:4", &spec)
            .await
            .unwrap(),
        FullScanChildSubmission::Stale
    );
}

#[tokio::test]
async fn full_scan_completion_is_fenced_by_the_library_profile_version() {
    let database = database().await;
    let backend = database.get_database_backend();
    let library = LibraryId::new();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("libraries"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("name"),
                        Alias::new("scan_profile"),
                        Alias::new("object_selection_scope"),
                        Alias::new("metadata_policy"),
                        Alias::new("expansion_policy"),
                        Alias::new("probe_policy"),
                        Alias::new("profile_version"),
                        Alias::new("collection_type"),
                        Alias::new("sort_key"),
                    ])
                    .values_panic([
                        library.as_uuid().into(),
                        "Movies".into(),
                        "Full".into(),
                        "all_synced_objects".into(),
                        "full".into(),
                        "eager".into(),
                        "eager".into(),
                        3_i64.into(),
                        "movies".into(),
                        SortKey::from_text("Movies").into_bytes().into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    let jobs = WorkJobRepository::new(&database);
    let submitted = jobs
        .enqueue_or_join(
            &WorkJobSpec::new(
                WorkTaskKind::FullMediaScan,
                WorkScope::Library(library),
                3,
                50,
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let claimed = jobs
        .claim_next(
            &[WorkTaskKind::FullMediaScan],
            "full-scan-worker",
            Duration::minutes(1),
        )
        .await
        .unwrap()
        .unwrap();
    database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("libraries"))
                    .value(Alias::new("profile_version"), 4_i64)
                    .and_where(Expr::col(Alias::new("id")).eq(library.as_uuid())),
            ),
        )
        .await
        .unwrap();

    assert!(matches!(
        jobs.complete_full_scan(
            &claimed,
            WorkJobResult::success(json!({"items": 0}), Vec::new())
        )
        .await,
        Err(WorkJobRepositoryError::StaleParentPolicy)
    ));
    assert_eq!(
        jobs.get(submitted.job().id())
            .await
            .unwrap()
            .unwrap()
            .state(),
        WorkJobState::Running
    );
}
