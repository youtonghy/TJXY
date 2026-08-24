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
        WorkRetentionRun::Deleted {
            job_id: submitted.job().id().as_uuid()
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
        WorkRetentionRun::Deferred {
            job_id: sync.job().id().as_uuid()
        }
    );
    assert_eq!(table_count(&database, "work_jobs", "id").await, 2);
    assert_eq!(table_count(&database, "work_results", "id").await, 1);
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
