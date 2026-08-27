use std::sync::{Arc, Mutex};

use chrono::{Duration, Utc};
use sea_orm::{
    ConnectionTrait, DatabaseConnection,
    sea_query::{Alias, Expr, Query},
};
use sea_orm_migration::MigratorTrait;
use tjxy_common::{CatalogItemId, StorageObjectRecordId};
use tjxy_db::{
    WorkJobClock, WorkJobRepository, WorkJobSpec, WorkRetentionRepository, WorkRetentionRun,
    WorkScope, WorkTaskKind,
};
use tjxy_test_support::test_database;

#[derive(Clone)]
struct ManualClock(Arc<Mutex<chrono::DateTime<Utc>>>);

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
async fn newly_terminal_job_is_scheduled_and_removed_after_retention() {
    let database = database().await;
    let terminal_at = Utc::now() - Duration::days(31);
    let jobs =
        WorkJobRepository::with_clock(&database, ManualClock(Arc::new(Mutex::new(terminal_at))));
    let _submitted = jobs
        .enqueue_or_join(
            &WorkJobSpec::new(
                WorkTaskKind::ProbeMedia,
                WorkScope::CatalogItem(CatalogItemId::new()),
                1,
                100,
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let claimed = jobs
        .claim_next(
            &[WorkTaskKind::ProbeMedia],
            "retention-contract",
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    jobs.fail_terminal(&claimed, "fixture failure")
        .await
        .unwrap();

    assert_eq!(
        table_count(&database, "work_job_retention_queue", "job_id").await,
        1
    );
    assert_eq!(table_count(&database, "work_results", "id").await, 1);

    let run = WorkRetentionRepository::new(&database)
        .run_once(
            "retention-worker",
            Duration::days(30),
            Duration::seconds(30),
        )
        .await
        .unwrap();

    assert_eq!(
        run,
        WorkRetentionRun::Processed {
            deleted: 1,
            compacted: 0,
            deferred: 0,
        }
    );
    assert_eq!(table_count(&database, "work_jobs", "id").await, 0);
    assert_eq!(table_count(&database, "work_results", "id").await, 0);
    assert_eq!(
        table_count(&database, "work_job_retention_queue", "job_id").await,
        0
    );
}

#[tokio::test]
async fn active_dependency_defers_retention() {
    let database = database().await;
    let terminal_at = Utc::now() - Duration::days(31);
    let jobs =
        WorkJobRepository::with_clock(&database, ManualClock(Arc::new(Mutex::new(terminal_at))));
    let sync = jobs
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
    jobs.enqueue_or_join(
        &WorkJobSpec::new(
            WorkTaskKind::ExpandItem,
            WorkScope::CatalogItem(CatalogItemId::new()),
            1,
            100,
        )
        .unwrap()
        .with_pending_required_sync(sync.job().id()),
    )
    .await
    .unwrap();
    let claimed = jobs
        .claim_next(
            &[WorkTaskKind::ScopedStorageSync],
            "dependency-retention-contract",
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    jobs.fail_terminal(&claimed, "fixture failure")
        .await
        .unwrap();

    let run = WorkRetentionRepository::new(&database)
        .run_once(
            "retention-worker",
            Duration::days(30),
            Duration::seconds(30),
        )
        .await
        .unwrap();

    assert_eq!(
        run,
        WorkRetentionRun::Processed {
            deleted: 0,
            compacted: 0,
            deferred: 1,
        }
    );
    assert_eq!(table_count(&database, "work_jobs", "id").await, 2);
    assert_eq!(table_count(&database, "work_results", "id").await, 1);
}

#[tokio::test]
async fn retention_deletes_multiple_terminal_jobs_in_one_batch() {
    let database = database().await;
    let terminal_at = Utc::now() - Duration::days(31);
    let jobs =
        WorkJobRepository::with_clock(&database, ManualClock(Arc::new(Mutex::new(terminal_at))));
    for number in 0..3 {
        let submitted = jobs
            .enqueue_or_join(
                &WorkJobSpec::new(
                    WorkTaskKind::ProbeMedia,
                    WorkScope::CatalogItem(CatalogItemId::new()),
                    number,
                    100,
                )
                .unwrap(),
            )
            .await
            .unwrap();
        let claimed = jobs
            .claim_next(
                &[WorkTaskKind::ProbeMedia],
                "retention-batch-contract",
                Duration::minutes(5),
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(claimed.id(), submitted.job().id());
        jobs.fail_terminal(&claimed, "batch fixture").await.unwrap();
    }

    assert_eq!(
        WorkRetentionRepository::new(&database)
            .run_once(
                "retention-worker",
                Duration::days(30),
                Duration::seconds(30),
            )
            .await
            .unwrap(),
        WorkRetentionRun::Processed {
            deleted: 3,
            compacted: 0,
            deferred: 0,
        }
    );
    assert_eq!(table_count(&database, "work_jobs", "id").await, 0);
    assert_eq!(table_count(&database, "work_results", "id").await, 0);
    assert_eq!(
        table_count(&database, "work_job_retention_queue", "job_id").await,
        0
    );
}

#[tokio::test]
async fn retention_mixes_deleted_and_deferred_jobs_in_one_batch() {
    let database = database().await;
    let terminal_at = Utc::now() - Duration::days(31);
    let jobs =
        WorkJobRepository::with_clock(&database, ManualClock(Arc::new(Mutex::new(terminal_at))));
    let sync = jobs
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
    jobs.enqueue_or_join(
        &WorkJobSpec::new(
            WorkTaskKind::ExpandItem,
            WorkScope::CatalogItem(CatalogItemId::new()),
            1,
            100,
        )
        .unwrap()
        .with_pending_required_sync(sync.job().id()),
    )
    .await
    .unwrap();
    let sync_claim = jobs
        .claim_next(
            &[WorkTaskKind::ScopedStorageSync],
            "retention-mixed-sync",
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    jobs.fail_terminal(&sync_claim, "mixed fixture")
        .await
        .unwrap();
    let probe = jobs
        .enqueue_or_join(
            &WorkJobSpec::new(
                WorkTaskKind::ProbeMedia,
                WorkScope::CatalogItem(CatalogItemId::new()),
                1,
                100,
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let probe_claim = jobs
        .claim_next(
            &[WorkTaskKind::ProbeMedia],
            "retention-mixed-probe",
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(probe_claim.id(), probe.job().id());
    jobs.fail_terminal(&probe_claim, "mixed fixture")
        .await
        .unwrap();

    assert_eq!(
        WorkRetentionRepository::new(&database)
            .run_once(
                "retention-worker",
                Duration::days(30),
                Duration::seconds(30),
            )
            .await
            .unwrap(),
        WorkRetentionRun::Processed {
            deleted: 1,
            compacted: 0,
            deferred: 1,
        }
    );
    assert_eq!(table_count(&database, "work_jobs", "id").await, 2);
    assert_eq!(table_count(&database, "work_results", "id").await, 1);
    assert_eq!(
        table_count(&database, "work_job_retention_queue", "job_id").await,
        1
    );
}

#[tokio::test]
async fn legacy_terminal_job_is_enrolled_then_deleted() {
    let database = database().await;
    let terminal_at = Utc::now() - Duration::days(31);
    let jobs =
        WorkJobRepository::with_clock(&database, ManualClock(Arc::new(Mutex::new(terminal_at))));
    let submitted = jobs
        .enqueue_or_join(
            &WorkJobSpec::new(
                WorkTaskKind::ProbeMedia,
                WorkScope::CatalogItem(CatalogItemId::new()),
                1,
                100,
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let claimed = jobs
        .claim_next(
            &[WorkTaskKind::ProbeMedia],
            "legacy-retention-contract",
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    jobs.fail_terminal(&claimed, "legacy fixture")
        .await
        .unwrap();
    let backend = database.get_database_backend();
    database
        .execute(
            backend.build(
                Query::delete()
                    .from_table(Alias::new("work_job_retention_queue"))
                    .and_where(Expr::col(Alias::new("job_id")).eq(submitted.job().id().as_uuid())),
            ),
        )
        .await
        .unwrap();
    let retention = WorkRetentionRepository::new(&database);

    assert_eq!(
        retention
            .run_once(
                "retention-worker",
                Duration::days(30),
                Duration::seconds(30),
            )
            .await
            .unwrap(),
        WorkRetentionRun::EnrolledLegacy { count: 1 }
    );
    assert_eq!(
        retention
            .run_once(
                "retention-worker",
                Duration::days(30),
                Duration::seconds(30),
            )
            .await
            .unwrap(),
        WorkRetentionRun::Processed {
            deleted: 1,
            compacted: 0,
            deferred: 0,
        }
    );
    assert_eq!(table_count(&database, "work_jobs", "id").await, 0);
    assert_eq!(table_count(&database, "work_results", "id").await, 0);
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Keeps the publication retention lifecycle in one fixture.
async fn published_job_is_compacted_once_without_legacy_reenrollment() {
    let database = database().await;
    let terminal_at = Utc::now() - Duration::days(31);
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
                        "Movie".into(),
                        "Published item".into(),
                        "published item".into(),
                        "Matched".into(),
                        "Ready".into(),
                        "Expanded".into(),
                        "Ready".into(),
                        1_i64.into(),
                        1_i64.into(),
                        true.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    let jobs =
        WorkJobRepository::with_clock(&database, ManualClock(Arc::new(Mutex::new(terminal_at))));
    let submitted = jobs
        .enqueue_or_join(
            &WorkJobSpec::new(
                WorkTaskKind::ProbeMedia,
                WorkScope::CatalogItem(item),
                1,
                100,
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let claimed = jobs
        .claim_next(
            &[WorkTaskKind::ProbeMedia],
            "published-retention-contract",
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    jobs.fail_terminal(&claimed, "published fixture")
        .await
        .unwrap();
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
                        uuid::Uuid::new_v4().into(),
                        submitted.job().id().as_uuid().into(),
                        item.as_uuid().into(),
                        "Sources".into(),
                        1_i64.into(),
                        "Active".into(),
                        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
                        0_i64.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    let retention = WorkRetentionRepository::new(&database);

    assert_eq!(
        retention
            .run_once(
                "retention-worker",
                Duration::days(30),
                Duration::seconds(30),
            )
            .await
            .unwrap(),
        WorkRetentionRun::Processed {
            deleted: 0,
            compacted: 1,
            deferred: 0,
        }
    );
    assert_eq!(table_count(&database, "work_results", "id").await, 0);
    assert_eq!(
        retention
            .run_once(
                "retention-worker",
                Duration::days(30),
                Duration::seconds(30),
            )
            .await
            .unwrap(),
        WorkRetentionRun::Idle
    );
    assert_eq!(table_count(&database, "work_jobs", "id").await, 1);
    assert_eq!(
        table_count(&database, "work_job_retention_queue", "job_id").await,
        0
    );
}

async fn table_count(database: &DatabaseConnection, table: &str, column: &str) -> i64 {
    let backend = database.get_database_backend();
    database
        .query_one(
            backend.build(
                Query::select()
                    .expr_as(Expr::col(Alias::new(column)).count(), Alias::new("count"))
                    .from(Alias::new(table)),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get("", "count")
        .unwrap()
}
