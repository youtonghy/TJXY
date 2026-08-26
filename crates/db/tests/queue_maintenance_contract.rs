use chrono::{Duration, Utc};
use sea_orm::{
    ConnectionTrait, DatabaseConnection,
    sea_query::{Alias, Expr, Query},
};
use sea_orm_migration::MigratorTrait;
use tjxy_common::{StorageObjectRecordId, StorageRootId};
use tjxy_db::{QueueMaintenanceRepository, QueueMaintenanceRun};
use tjxy_test_support::test_database;
use uuid::Uuid;

async fn database() -> DatabaseConnection {
    let database = test_database().await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    database
}

async fn seed_storage(database: &DatabaseConnection) -> (StorageRootId, StorageObjectRecordId) {
    let account_id = Uuid::new_v4();
    let root_id = StorageRootId::new();
    let object_id = StorageObjectRecordId::new();
    let backend = database.get_database_backend();
    database
        .execute(
            backend.build(
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
                        account_id.into(),
                        "Filesystem".into(),
                        "maintenance".into(),
                        Uuid::new_v4().to_string().into(),
                        "fixture-ref".into(),
                        "Active".into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("storage_roots"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("storage_account_id"),
                        Alias::new("provider_root_id"),
                        Alias::new("sync_revision"),
                        Alias::new("reconciled_sync_revision"),
                        Alias::new("outbox_degraded_at"),
                        Alias::new("outbox_degraded_revision"),
                        Alias::new("outbox_degraded_reason"),
                    ])
                    .values_panic([
                        root_id.as_uuid().into(),
                        account_id.into(),
                        "root".into(),
                        2_i64.into(),
                        1_i64.into(),
                        Utc::now().into(),
                        2_i64.into(),
                        "InvalidPayload".into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            backend.build(
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
                        object_id.as_uuid().into(),
                        account_id.into(),
                        "local".into(),
                        object_id.to_string().into(),
                        "file.mkv".into(),
                        "file.mkv".into(),
                        "File".into(),
                        2_i64.into(),
                        false.into(),
                        0_i64.into(),
                        "Stable".into(),
                        "Present".into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    (root_id, object_id)
}

async fn seed_outbox(
    database: &DatabaseConnection,
    root_id: StorageRootId,
    object_id: StorageObjectRecordId,
    revision: i64,
    state: &str,
    dead_lettered_at: Option<chrono::DateTime<Utc>>,
) -> Uuid {
    let id = Uuid::new_v4();
    let backend = database.get_database_backend();
    database
        .execute(
            backend.build(
                Query::insert()
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
                        Alias::new("processed_at"),
                        Alias::new("dead_lettered_at"),
                    ])
                    .values_panic([
                        id.into(),
                        root_id.as_uuid().into(),
                        revision.into(),
                        "Upserted".into(),
                        object_id.as_uuid().into(),
                        1_i32.into(),
                        serde_json::json!({"version": 1}).into(),
                        id.to_string().into(),
                        state.into(),
                        10_i32.into(),
                        Utc::now().into(),
                        Utc::now().into(),
                        dead_lettered_at.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    id
}

#[tokio::test]
async fn cleanup_respects_watermark_and_seven_day_dead_letter_retention() {
    let database = database().await;
    let (root_id, object_id) = seed_storage(&database).await;
    let safe_processed = seed_outbox(&database, root_id, object_id, 1, "Processed", None).await;
    let unsafe_processed = seed_outbox(&database, root_id, object_id, 2, "Processed", None).await;
    let expired_dead = seed_outbox(
        &database,
        root_id,
        object_id,
        2,
        "DeadLetter",
        Some(Utc::now() - Duration::days(8)),
    )
    .await;
    let recent_dead = seed_outbox(
        &database,
        root_id,
        object_id,
        2,
        "DeadLetter",
        Some(Utc::now() - Duration::days(6)),
    )
    .await;

    assert_eq!(
        QueueMaintenanceRepository::new(&database)
            .run_once(Duration::days(7))
            .await
            .unwrap(),
        QueueMaintenanceRun::StorageOutbox { deleted: 2 }
    );
    assert!(!row_exists(&database, "storage_change_outbox", safe_processed).await);
    assert!(!row_exists(&database, "storage_change_outbox", expired_dead).await);
    assert!(row_exists(&database, "storage_change_outbox", unsafe_processed).await);
    assert!(row_exists(&database, "storage_change_outbox", recent_dead).await);
    let backend = database.get_database_backend();
    let root = database
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("outbox_degraded_revision"))
                    .from(Alias::new("storage_roots"))
                    .and_where(Expr::col(Alias::new("id")).eq(root_id.as_uuid())),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        root.try_get::<Option<i64>>("", "outbox_degraded_revision")
            .unwrap(),
        Some(2)
    );
}

#[tokio::test]
async fn legacy_cache_cleanup_is_bounded_and_idempotent() {
    let database = database().await;
    let backend = database.get_database_backend();
    let mut insert = Query::insert();
    insert
        .into_table(Alias::new("cache_invalidation_outbox"))
        .columns([
            Alias::new("id"),
            Alias::new("generation"),
            Alias::new("state"),
            Alias::new("attempt_count"),
        ]);
    for generation in 1_i64..=501 {
        insert.values_panic([
            Uuid::new_v4().into(),
            generation.into(),
            "Processed".into(),
            0_i32.into(),
        ]);
    }
    database.execute(backend.build(&insert)).await.unwrap();
    let repository = QueueMaintenanceRepository::new(&database);

    assert_eq!(
        repository.run_once(Duration::days(7)).await.unwrap(),
        QueueMaintenanceRun::LegacyCacheOutbox { deleted: 500 }
    );
    assert_eq!(table_count(&database, "cache_invalidation_outbox").await, 1);
    assert_eq!(
        repository.run_once(Duration::days(7)).await.unwrap(),
        QueueMaintenanceRun::LegacyCacheOutbox { deleted: 1 }
    );
    assert_eq!(
        repository.run_once(Duration::days(7)).await.unwrap(),
        QueueMaintenanceRun::Idle
    );
}

async fn row_exists(database: &DatabaseConnection, table: &str, id: Uuid) -> bool {
    let backend = database.get_database_backend();
    database
        .query_one(
            backend.build(
                Query::select()
                    .expr(Expr::val(1_i32))
                    .from(Alias::new(table))
                    .and_where(Expr::col(Alias::new("id")).eq(id))
                    .limit(1),
            ),
        )
        .await
        .unwrap()
        .is_some()
}

async fn table_count(database: &DatabaseConnection, table: &str) -> i64 {
    let backend = database.get_database_backend();
    database
        .query_one(
            backend.build(
                Query::select()
                    .expr_as(Expr::col(Alias::new("id")).count(), Alias::new("count"))
                    .from(Alias::new(table)),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get("", "count")
        .unwrap()
}
