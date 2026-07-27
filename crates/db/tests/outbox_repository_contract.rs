use std::sync::{Arc, Mutex};

use chrono::{Duration, TimeZone, Utc};
use sea_orm::{
    ConnectionTrait, DatabaseConnection, TransactionTrait,
    sea_query::{Alias, Expr, Query},
};
use sea_orm_migration::MigratorTrait;
use tjxy_common::{StorageObjectRecordId, StorageRootId};
use tjxy_db::{
    ClaimedOutboxEvent, OutboxClock, OutboxCompletion, OutboxFailureReason, OutboxRepository,
    OutboxRepositoryError,
};
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

impl OutboxClock for ManualClock {
    fn now(&self) -> chrono::DateTime<Utc> {
        *self.0.lock().unwrap()
    }
}

fn repository(
    database: &DatabaseConnection,
    now: chrono::DateTime<Utc>,
) -> (OutboxRepository<'_, ManualClock>, ManualClock) {
    let clock = ManualClock::new(now);
    (OutboxRepository::with_clock(database, clock.clone()), clock)
}

async fn complete_claim(
    repository: &OutboxRepository<'_, ManualClock>,
    database: &DatabaseConnection,
    claimed: &ClaimedOutboxEvent,
) -> Result<OutboxCompletion, OutboxRepositoryError> {
    let transaction = database.begin().await.unwrap();
    let result = repository
        .complete_in_transaction(&transaction, claimed)
        .await;
    match result {
        Ok(completion) => {
            transaction.commit().await.unwrap();
            Ok(completion)
        }
        Err(error) => {
            transaction.rollback().await.unwrap();
            Err(error)
        }
    }
}

async fn database() -> DatabaseConnection {
    let database = test_database().await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    database
}

async fn seed_root(database: &DatabaseConnection, sync_revision: i64) -> StorageRootId {
    let account_id = Uuid::new_v4();
    let root_id = StorageRootId::new();
    let backend = database.get_database_backend();
    database
        .execute(
            backend.build(
                &Query::insert()
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
                        account_id.into(),
                        "Filesystem".into(),
                        "test".into(),
                        Uuid::new_v4().to_string().into(),
                        "test-ref".into(),
                        "Active".into(),
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
                    .into_table(Alias::new("storage_roots"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("storage_account_id"),
                        Alias::new("provider_root_id"),
                        Alias::new("sync_revision"),
                        Alias::new("reconciled_sync_revision"),
                    ])
                    .values_panic([
                        root_id.as_uuid().into(),
                        account_id.into(),
                        "root".into(),
                        sync_revision.into(),
                        0_i64.into(),
                    ])
                    .to_owned(),
            ),
        )
        .await
        .unwrap();
    root_id
}

async fn seed_event(database: &DatabaseConnection, root_id: StorageRootId, revision: i64) -> Uuid {
    let object_id = StorageObjectRecordId::new();
    let event_id = Uuid::new_v4();
    let backend = database.get_database_backend();
    let account_id: Uuid = database
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("storage_account_id"))
                    .from(Alias::new("storage_roots"))
                    .limit(1),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get("", "storage_account_id")
        .unwrap();
    database
        .execute(
            backend.build(
                &Query::insert()
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
                        object_id.as_uuid().into(),
                        account_id.into(),
                        "drive".into(),
                        object_id.to_string().into(),
                        "movie.mkv".into(),
                        "movie.mkv".into(),
                        "File".into(),
                        revision.into(),
                        false.into(),
                        0_i64.into(),
                        "Stable".into(),
                        "Present".into(),
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
                    .into_table(Alias::new("storage_change_outbox"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("storage_root_id"),
                        Alias::new("sync_revision"),
                        Alias::new("event_type"),
                        Alias::new("storage_object_id"),
                        Alias::new("payload_version"),
                        Alias::new("payload"),
                        Alias::new("dedupe_key"),
                        Alias::new("state"),
                        Alias::new("attempt_count"),
                        Alias::new("created_at"),
                    ])
                    .values_panic([
                        event_id.into(),
                        root_id.as_uuid().into(),
                        revision.into(),
                        "Upserted".into(),
                        object_id.as_uuid().into(),
                        1.into(),
                        serde_json::json!({"version": 1}).into(),
                        format!("{root_id}:{revision}:{object_id}:Upserted").into(),
                        "Pending".into(),
                        0.into(),
                        Utc::now().into(),
                    ])
                    .to_owned(),
            ),
        )
        .await
        .unwrap();
    event_id
}

#[tokio::test]
async fn claims_only_the_lowest_unreconciled_revision() {
    let database = database().await;
    let root_id = seed_root(&database, 2).await;
    seed_event(&database, root_id, 2).await;
    let first_id = seed_event(&database, root_id, 1).await;
    let now = Utc.with_ymd_and_hms(2026, 7, 17, 10, 0, 0).unwrap();
    let (repository, _clock) = repository(&database, now);

    let claimed = repository
        .claim_next(root_id, "worker-a", Duration::seconds(30))
        .await
        .unwrap()
        .unwrap();

    assert_eq!(claimed.id(), first_id);
    assert_eq!(claimed.sync_revision(), 1);
    assert!(
        repository
            .claim_next(root_id, "worker-b", Duration::seconds(30))
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn expired_claim_is_fenced_from_completion() {
    let database = database().await;
    let root_id = seed_root(&database, 1).await;
    seed_event(&database, root_id, 1).await;
    let now = Utc.with_ymd_and_hms(2026, 7, 17, 10, 0, 0).unwrap();
    let (repository, clock) = repository(&database, now);
    let stale = repository
        .claim_next(root_id, "claim-one", Duration::seconds(5))
        .await
        .unwrap()
        .unwrap();
    clock.set(now + Duration::seconds(6));
    let current = repository
        .claim_next(root_id, "claim-two", Duration::seconds(5))
        .await
        .unwrap()
        .unwrap();

    let error = complete_claim(&repository, &database, &stale)
        .await
        .unwrap_err();
    assert!(matches!(error, OutboxRepositoryError::LostLease));
    let completion = complete_claim(&repository, &database, &current)
        .await
        .unwrap();
    assert_eq!(completion.reconciled_sync_revision, 1);
}

#[tokio::test]
async fn expired_claim_cannot_complete_before_another_worker_reclaims_it() {
    let database = database().await;
    let root_id = seed_root(&database, 1).await;
    seed_event(&database, root_id, 1).await;
    let now = Utc.with_ymd_and_hms(2026, 7, 17, 10, 0, 0).unwrap();
    let (repository, clock) = repository(&database, now);
    let claimed = repository
        .claim_next(root_id, "worker", Duration::seconds(5))
        .await
        .unwrap()
        .unwrap();

    clock.set(now + Duration::seconds(6));
    let error = complete_claim(&repository, &database, &claimed)
        .await
        .unwrap_err();

    assert!(matches!(error, OutboxRepositoryError::LostLease));
}

#[tokio::test]
async fn watermark_advances_only_after_every_event_in_revision() {
    let database = database().await;
    let root_id = seed_root(&database, 1).await;
    seed_event(&database, root_id, 1).await;
    seed_event(&database, root_id, 1).await;
    let now = Utc.with_ymd_and_hms(2026, 7, 17, 10, 0, 0).unwrap();
    let (repository, _clock) = repository(&database, now);

    let first = repository
        .claim_next(root_id, "first", Duration::seconds(30))
        .await
        .unwrap()
        .unwrap();
    let first_completion = complete_claim(&repository, &database, &first)
        .await
        .unwrap();
    assert_eq!(first_completion.reconciled_sync_revision, 0);

    let second = repository
        .claim_next(root_id, "second", Duration::seconds(30))
        .await
        .unwrap()
        .unwrap();
    let second_completion = complete_claim(&repository, &database, &second)
        .await
        .unwrap();
    assert_eq!(second_completion.reconciled_sync_revision, 1);
}

#[tokio::test]
async fn failure_requeues_with_backoff_and_incremented_attempt() {
    let database = database().await;
    let root_id = seed_root(&database, 1).await;
    seed_event(&database, root_id, 1).await;
    let now = Utc.with_ymd_and_hms(2026, 7, 17, 10, 0, 0).unwrap();
    let (repository, clock) = repository(&database, now);
    let claimed = repository
        .claim_next(root_id, "first", Duration::seconds(30))
        .await
        .unwrap()
        .unwrap();

    repository
        .fail(
            &claimed,
            Duration::seconds(10),
            OutboxFailureReason::TransientProvider,
        )
        .await
        .unwrap();
    assert!(
        repository
            .claim_next(root_id, "early", Duration::seconds(30))
            .await
            .unwrap()
            .is_none()
    );
    clock.set(now + Duration::seconds(10));
    let retried = repository
        .claim_next(root_id, "retry", Duration::seconds(30))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(retried.attempt_count(), 1);
}

#[tokio::test]
async fn empty_revision_is_skipped_before_claiming_the_next_revision() {
    let database = database().await;
    let root_id = seed_root(&database, 2).await;
    seed_event(&database, root_id, 2).await;
    let now = Utc.with_ymd_and_hms(2026, 7, 17, 10, 0, 0).unwrap();
    let (repository, _clock) = repository(&database, now);

    let claimed = repository
        .claim_next(root_id, "worker", Duration::seconds(30))
        .await
        .unwrap()
        .unwrap();

    assert_eq!(claimed.sync_revision(), 2);
    let backend = database.get_database_backend();
    let watermark: i64 = database
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("reconciled_sync_revision"))
                    .from(Alias::new("storage_roots"))
                    .and_where(Expr::col(Alias::new("id")).eq(root_id.as_uuid())),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get("", "reconciled_sync_revision")
        .unwrap();
    assert_eq!(watermark, 1);
}

#[tokio::test]
async fn backlogged_roots_are_ordered_and_bounded() {
    let database = database().await;
    let first = seed_root(&database, 1).await;
    let second = seed_root(&database, 2).await;
    let current = seed_root(&database, 1).await;
    let backend = database.get_database_backend();
    database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("storage_roots"))
                    .value(Alias::new("reconciled_sync_revision"), 1_i64)
                    .and_where(Expr::col(Alias::new("id")).eq(current.as_uuid())),
            ),
        )
        .await
        .unwrap();
    let repository = OutboxRepository::new(&database);

    let roots = repository.backlogged_roots(None, 2).await.unwrap();
    let mut expected = vec![(first.as_uuid(), 1_i64), (second.as_uuid(), 2_i64)];
    expected.sort_unstable_by_key(|(root, _)| *root);

    assert_eq!(roots.len(), 2);
    assert_eq!(
        roots
            .iter()
            .map(|root| (root.root_id().as_uuid(), root.expected_revision()))
            .collect::<Vec<_>>(),
        expected
    );
    assert_eq!(
        repository
            .backlogged_roots(Some(roots[0].root_id()), 2)
            .await
            .unwrap(),
        vec![roots[1]]
    );
    assert_eq!(repository.backlogged_roots(None, 1).await.unwrap().len(), 1);
}

#[tokio::test]
async fn caller_owned_completion_transaction_can_roll_back_atomically() {
    let database = database().await;
    let root_id = seed_root(&database, 1).await;
    let event_id = seed_event(&database, root_id, 1).await;
    let now = Utc.with_ymd_and_hms(2026, 7, 17, 10, 0, 0).unwrap();
    let (repository, _clock) = repository(&database, now);
    let claimed = repository
        .claim_next(root_id, "worker", Duration::seconds(30))
        .await
        .unwrap()
        .unwrap();
    let transaction = database.begin().await.unwrap();

    repository
        .complete_in_transaction(&transaction, &claimed)
        .await
        .unwrap();
    transaction.rollback().await.unwrap();

    let backend = database.get_database_backend();
    let row = database
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("state"))
                    .from(Alias::new("storage_change_outbox"))
                    .and_where(Expr::col(Alias::new("id")).eq(event_id)),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.try_get::<String>("", "state").unwrap(), "Processing");
    let watermark: i64 = database
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("reconciled_sync_revision"))
                    .from(Alias::new("storage_roots"))
                    .and_where(Expr::col(Alias::new("id")).eq(root_id.as_uuid())),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get("", "reconciled_sync_revision")
        .unwrap();
    assert_eq!(watermark, 0);
}
