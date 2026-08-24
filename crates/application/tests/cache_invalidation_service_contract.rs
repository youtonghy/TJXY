use std::sync::Arc;

use async_trait::async_trait;
use sea_orm::{
    ConnectionTrait, DatabaseConnection, TransactionTrait,
    sea_query::{Alias, Expr, Query},
};
use sea_orm_migration::MigratorTrait;
use tjxy_application::{CacheInvalidationRun, CacheInvalidationService};
use tjxy_cache::{
    CacheInvalidationError, CacheInvalidationFailureKind, CacheInvalidationOutcome,
    CacheInvalidator, CacheRuntime,
};
use tjxy_db::advance_catalog_generation;
use tjxy_test_support::test_database;

struct UnavailableCache;

struct IncompleteBatchCache;

#[async_trait]
impl CacheInvalidator for UnavailableCache {
    async fn invalidate_generation(
        &self,
        _generation: i64,
    ) -> Result<CacheInvalidationOutcome, CacheInvalidationError> {
        Err(CacheInvalidationError::Unavailable)
    }
}

#[async_trait]
impl CacheInvalidator for IncompleteBatchCache {
    async fn invalidate_generation(
        &self,
        _generation: i64,
    ) -> Result<CacheInvalidationOutcome, CacheInvalidationError> {
        Ok(CacheInvalidationOutcome::Deleted {
            count: 100,
            remaining: 1,
        })
    }
}

async fn database_with_generation() -> DatabaseConnection {
    let database = test_database().await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    let transaction = database.begin().await.unwrap();
    advance_catalog_generation(&transaction).await.unwrap();
    transaction.commit().await.unwrap();
    database
}

#[tokio::test]
async fn disabled_cache_completes_the_durable_invalidation() {
    let database = database_with_generation().await;
    let service = CacheInvalidationService::new(database.clone(), Arc::new(CacheRuntime::Disabled));

    let run = service.run_once().await.unwrap();

    assert!(matches!(
        run,
        CacheInvalidationRun::Completed {
            generation: 1,
            outcome: CacheInvalidationOutcome::SkippedDisabled,
        }
    ));
    assert_eq!(invalidation_state(&database).await, (1, None, 0));
}

#[tokio::test]
async fn cache_failure_requeues_without_rolling_back_the_catalog_generation() {
    let database = database_with_generation().await;
    let service = CacheInvalidationService::new(database.clone(), Arc::new(UnavailableCache));

    let run = service.run_once().await.unwrap();

    assert!(matches!(
        run,
        CacheInvalidationRun::Deferred {
            generation: 1,
            failure: CacheInvalidationFailureKind::Unavailable,
        }
    ));
    assert_eq!(invalidation_state(&database).await, (0, Some(1), 1));
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
    assert_eq!(row.try_get::<i64>("", "generation").unwrap(), 1);
    let event = database
        .query_one(
            backend.build(
                Query::select()
                    .columns([Alias::new("available_at"), Alias::new("last_error")])
                    .from(Alias::new("cache_invalidation_state")),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert!(
        event
            .try_get::<Option<chrono::DateTime<chrono::Utc>>>("", "available_at")
            .unwrap()
            .is_some()
    );
    assert_eq!(
        event.try_get::<String>("", "last_error").unwrap(),
        "RedisUnavailable"
    );
}

#[tokio::test]
async fn incomplete_batch_is_released_without_failure_backoff() {
    let database = database_with_generation().await;
    let service = CacheInvalidationService::new(database.clone(), Arc::new(IncompleteBatchCache));

    let run = service.run_once().await.unwrap();

    assert!(matches!(
        run,
        CacheInvalidationRun::Progressed {
            generation: 1,
            deleted: 100,
            remaining: 1,
        }
    ));
    assert_eq!(invalidation_state(&database).await, (0, Some(1), 0));
}

async fn invalidation_state(database: &DatabaseConnection) -> (i64, Option<i64>, i32) {
    let backend = database.get_database_backend();
    let row = database
        .query_one(
            backend.build(
                Query::select()
                    .columns([
                        Alias::new("processed_generation"),
                        Alias::new("target_generation"),
                        Alias::new("attempt_count"),
                    ])
                    .from(Alias::new("cache_invalidation_state")),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    (
        row.try_get("", "processed_generation").unwrap(),
        row.try_get("", "target_generation").unwrap(),
        row.try_get("", "attempt_count").unwrap(),
    )
}
