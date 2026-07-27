use std::sync::{Arc, Mutex};

use chrono::{Duration, TimeZone, Utc};
use sea_orm::{
    ConnectionTrait, DatabaseConnection, TransactionTrait,
    sea_query::{Alias, Expr, Query},
};
use sea_orm_migration::MigratorTrait;
use tjxy_db::{
    CacheInvalidationClock, CacheInvalidationRepository, CacheInvalidationRepositoryError,
    advance_catalog_generation,
};
use tjxy_test_support::test_database;

#[derive(Clone)]
struct ManualClock(Arc<Mutex<chrono::DateTime<Utc>>>);

impl ManualClock {
    fn set(&self, now: chrono::DateTime<Utc>) {
        *self.0.lock().unwrap() = now;
    }
}

impl CacheInvalidationClock for ManualClock {
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
async fn generation_registration_is_atomic_and_unique() {
    let database = database().await;
    let transaction = database.begin().await.unwrap();

    let generation = advance_catalog_generation(&transaction).await.unwrap();
    assert_eq!(generation, 1);
    transaction.rollback().await.unwrap();

    let backend = database.get_database_backend();
    let row = database
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("generation"))
                    .from(Alias::new("catalog_state"))
                    .and_where(Expr::col(Alias::new("id")).eq(1_i32)),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.try_get::<i64>("", "generation").unwrap(), 0);
    let count = database
        .query_one(
            backend.build(
                Query::select()
                    .expr_as(Expr::col(Alias::new("id")).count(), Alias::new("count"))
                    .from(Alias::new("cache_invalidation_outbox")),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(count.try_get::<i64>("", "count").unwrap(), 0);

    let transaction = database.begin().await.unwrap();
    let generation = advance_catalog_generation(&transaction).await.unwrap();
    transaction.commit().await.unwrap();
    assert_eq!(generation, 1);
}

#[tokio::test]
async fn expired_claim_is_fenced_and_failure_requeues_with_backoff() {
    let database = database().await;
    let transaction = database.begin().await.unwrap();
    advance_catalog_generation(&transaction).await.unwrap();
    transaction.commit().await.unwrap();
    let now = Utc.with_ymd_and_hms(2026, 7, 19, 10, 0, 0).unwrap();
    let clock = ManualClock(Arc::new(Mutex::new(now)));
    let repository = CacheInvalidationRepository::with_clock(&database, clock.clone());

    let stale = repository
        .claim_next("worker-a", Duration::seconds(5))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stale.generation(), 1);
    assert_eq!(stale.stale_generation(), 0);
    clock.set(now + Duration::seconds(6));
    let current = repository
        .claim_next("worker-b", Duration::seconds(5))
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        repository.complete(&stale).await.unwrap_err(),
        CacheInvalidationRepositoryError::LostLease
    ));

    repository
        .fail(&current, Duration::seconds(10), "RedisUnavailable")
        .await
        .unwrap();
    assert!(
        repository
            .claim_next("too-early", Duration::seconds(5))
            .await
            .unwrap()
            .is_none()
    );
    clock.set(now + Duration::seconds(16));
    let retried = repository
        .claim_next("worker-c", Duration::seconds(5))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(retried.attempt_count(), 1);
    repository.complete(&retried).await.unwrap();
    assert!(
        repository
            .claim_next("idle", Duration::seconds(5))
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn incomplete_batch_releases_the_claim_without_recording_a_failure() {
    let database = database().await;
    let transaction = database.begin().await.unwrap();
    advance_catalog_generation(&transaction).await.unwrap();
    transaction.commit().await.unwrap();
    let now = Utc.with_ymd_and_hms(2026, 7, 19, 10, 0, 0).unwrap();
    let repository =
        CacheInvalidationRepository::with_clock(&database, ManualClock(Arc::new(Mutex::new(now))));

    let claimed = repository
        .claim_next("worker-a", Duration::seconds(5))
        .await
        .unwrap()
        .unwrap();
    repository.release(&claimed).await.unwrap();

    let resumed = repository
        .claim_next("worker-b", Duration::seconds(5))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(resumed.attempt_count(), 0);
    repository.complete(&resumed).await.unwrap();
}
