use sea_orm::{
    ConnectionTrait, DatabaseConnection,
    sea_query::{Alias, Expr, Query},
};
use sea_orm_migration::MigratorTrait;
use tjxy_application::StorageChangeReconciler;
use tjxy_common::{StorageObjectRecordId, StorageRootId};
use tjxy_test_support::test_database;
use uuid::Uuid;

async fn database() -> DatabaseConnection {
    let database = test_database().await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    database
}

#[allow(clippy::too_many_lines)] // Keeps the complete durable backlog fixture in one setup path.
async fn seed_backlog(database: &DatabaseConnection, valid: bool) -> StorageRootId {
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
                        "filesystem".into(),
                        "Disk".into(),
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
                    ])
                    .values_panic([
                        root_id.as_uuid().into(),
                        account_id.into(),
                        "root".into(),
                        1_i64.into(),
                        0_i64.into(),
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
                        "Root".into(),
                        "root".into(),
                        "Directory".into(),
                        1_i64.into(),
                        true.into(),
                        1_i64.into(),
                        "ProviderStableId".into(),
                        "Present".into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    let kind = "InventoryPageCommitted";
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
                    ])
                    .values_panic([
                        Uuid::new_v4().into(),
                        root_id.as_uuid().into(),
                        1_i64.into(),
                        kind.into(),
                        object_id.as_uuid().into(),
                        1_i32.into(),
                        serde_json::json!({
                            "version": 1,
                            "kind": if valid { kind } else { "Broken" },
                        })
                        .into(),
                        format!("{root_id}:marker").into(),
                        "Pending".into(),
                        0_i32.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    root_id
}

#[tokio::test]
async fn one_reconcile_pass_backs_off_a_poison_root_and_continues_other_roots() {
    let database = database().await;
    let invalid_root = seed_backlog(&database, false).await;
    let valid_root = seed_backlog(&database, true).await;

    let mut reconciler = StorageChangeReconciler::new(database.clone());
    let report = reconciler.run_once().await.unwrap();

    assert_eq!(report.roots_reconciled(), 1);
    assert_eq!(report.failures().len(), 1);
    assert_eq!(report.failures()[0].root_id(), invalid_root);
    let backend = database.get_database_backend();
    let roots = database
        .query_all(
            backend.build(
                Query::select()
                    .columns([Alias::new("id"), Alias::new("reconciled_sync_revision")])
                    .from(Alias::new("storage_roots"))
                    .and_where(
                        Expr::col(Alias::new("id"))
                            .is_in([invalid_root.as_uuid(), valid_root.as_uuid()]),
                    ),
            ),
        )
        .await
        .unwrap();
    for row in roots {
        let id: Uuid = row.try_get("", "id").unwrap();
        let revision: i64 = row.try_get("", "reconciled_sync_revision").unwrap();
        assert_eq!(revision, i64::from(id == valid_root.as_uuid()));
    }
    let failed = database
        .query_one(
            backend.build(
                Query::select()
                    .columns([
                        Alias::new("state"),
                        Alias::new("attempt_count"),
                        Alias::new("available_at"),
                        Alias::new("last_error"),
                    ])
                    .from(Alias::new("storage_change_outbox"))
                    .and_where(Expr::col(Alias::new("storage_root_id")).eq(invalid_root.as_uuid())),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(failed.try_get::<String>("", "state").unwrap(), "Pending");
    assert_eq!(failed.try_get::<i32>("", "attempt_count").unwrap(), 1);
    assert!(
        failed
            .try_get::<Option<chrono::DateTime<chrono::Utc>>>("", "available_at")
            .unwrap()
            .is_some()
    );
    assert_eq!(
        failed.try_get::<String>("", "last_error").unwrap(),
        "InvalidPayload"
    );
}
