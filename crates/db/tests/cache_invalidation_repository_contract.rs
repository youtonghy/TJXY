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
async fn generation_advance_is_atomic_without_legacy_outbox_growth() {
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
}

#[tokio::test]
async fn claim_coalesces_multiple_generations_to_the_latest() {
    let database = database().await;
    let transaction = database.begin().await.unwrap();
    for expected in 1..=50 {
        assert_eq!(
            advance_catalog_generation(&transaction).await.unwrap(),
            expected
        );
    }
    transaction.commit().await.unwrap();
    let repository = CacheInvalidationRepository::new(&database);

    let claimed = repository
        .claim_next("worker", Duration::seconds(5))
        .await
        .unwrap()
        .unwrap();

    assert_eq!(claimed.generation(), 50);
    repository.complete(&claimed).await.unwrap();
    assert!(
        repository
            .claim_next("worker", Duration::seconds(5))
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn generation_created_during_a_claim_becomes_the_next_latest_target() {
    let database = database().await;
    let transaction = database.begin().await.unwrap();
    advance_catalog_generation(&transaction).await.unwrap();
    transaction.commit().await.unwrap();
    let repository = CacheInvalidationRepository::new(&database);
    let first = repository
        .claim_next("worker", Duration::seconds(5))
        .await
        .unwrap()
        .unwrap();

    let transaction = database.begin().await.unwrap();
    advance_catalog_generation(&transaction).await.unwrap();
    advance_catalog_generation(&transaction).await.unwrap();
    transaction.commit().await.unwrap();
    repository.complete(&first).await.unwrap();

    let latest = repository
        .claim_next("worker", Duration::seconds(5))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(latest.generation(), 3);
    repository.complete(&latest).await.unwrap();
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

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn purge_consumed_outbox_removes_only_consumed_generations_in_bounded_windows() {
    let database = database().await;
    let backend = database.get_database_backend();
    let repository = CacheInvalidationRepository::new(&database);

    // Seed the foreign-key chain behind one outbox row shape: catalog item,
    // owning work job, publication, then five change generations.
    let item_id = uuid::Uuid::new_v4();
    let job_id = uuid::Uuid::new_v4();
    let publication_id = uuid::Uuid::new_v4();
    database
        .execute(
            backend.build(
                &Query::insert()
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
                        item_id.into(),
                        "Movie".into(),
                        "Purge Test".into(),
                        "purge test".into(),
                        "Matched".into(),
                        "Ready".into(),
                        "Unexpanded".into(),
                        "Unknown".into(),
                        0_i64.into(),
                        0_i64.into(),
                        true.into(),
                    ])
                    .to_owned(),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            backend.build(
                &Query::insert()
                    .into_table(Alias::new("work_jobs"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("task_kind"),
                        Alias::new("scope_type"),
                        Alias::new("scope_id"),
                        Alias::new("expected_revision"),
                        Alias::new("state"),
                        Alias::new("priority"),
                        Alias::new("attempt_count"),
                    ])
                    .values_panic([
                        job_id.into(),
                        "IndexMediaSources".into(),
                        "CatalogItem".into(),
                        item_id.into(),
                        0_i64.into(),
                        "Completed".into(),
                        100_i32.into(),
                        0_i32.into(),
                    ])
                    .to_owned(),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            backend.build(
                &Query::insert()
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
                        Alias::new("created_at"),
                    ])
                    .values_panic([
                        publication_id.into(),
                        job_id.into(),
                        item_id.into(),
                        "Source".into(),
                        0_i64.into(),
                        "Ready".into(),
                        "0000000000000000000000000000000000000000000000000000000000000000".into(),
                        0_i64.into(),
                        Utc.timestamp_millis_opt(0).unwrap().into(),
                    ])
                    .to_owned(),
            ),
        )
        .await
        .unwrap();
    for generation in 1_i64..=5 {
        database
            .execute(
                backend.build(
                    &Query::insert()
                        .into_table(Alias::new("catalog_change_outbox"))
                        .columns([
                            Alias::new("id"),
                            Alias::new("generation"),
                            Alias::new("event_type"),
                            Alias::new("catalog_item_id"),
                            Alias::new("publication_id"),
                            Alias::new("created_at"),
                        ])
                        .values_panic([
                            uuid::Uuid::new_v4().into(),
                            generation.into(),
                            "SourcesChanged".into(),
                            item_id.into(),
                            publication_id.into(),
                            Utc.timestamp_millis_opt(0).unwrap().into(),
                        ])
                        .to_owned(),
                ),
            )
            .await
            .unwrap();
    }
    database
        .execute(
            backend.build(
                &Query::update()
                    .table(Alias::new("cache_invalidation_state"))
                    .value(Alias::new("processed_generation"), 3_i64)
                    .and_where(Expr::col(Alias::new("id")).eq(1_i32))
                    .to_owned(),
            ),
        )
        .await
        .unwrap();

    // A window of one generation converges one batch at a time.
    assert_eq!(repository.purge_consumed_outbox(1).await.unwrap(), 1);
    assert_eq!(repository.purge_consumed_outbox(1).await.unwrap(), 1);
    assert_eq!(repository.purge_consumed_outbox(1).await.unwrap(), 1);
    assert_eq!(repository.purge_consumed_outbox(1).await.unwrap(), 0);

    let remaining = database
        .query_one(
            backend.build(
                Query::select()
                    .expr_as(
                        Expr::col(Alias::new("generation")).min(),
                        Alias::new("oldest"),
                    )
                    .from(Alias::new("catalog_change_outbox")),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get::<i64>("", "oldest")
        .unwrap();
    assert_eq!(remaining, 4);

    // A wide window removes everything consumed at once.
    assert_eq!(repository.purge_consumed_outbox(10_000).await.unwrap(), 0);

    // Invalid windows are rejected without touching rows.
    assert!(matches!(
        repository.purge_consumed_outbox(0).await,
        Err(CacheInvalidationRepositoryError::InvalidGenerationWindow)
    ));
}

#[tokio::test]
async fn purge_consumed_outbox_is_a_noop_on_an_empty_outbox() {
    let database = database().await;
    let repository = CacheInvalidationRepository::new(&database);

    assert_eq!(repository.purge_consumed_outbox(50_000).await.unwrap(), 0);
}
