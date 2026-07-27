use chrono::Duration;
use sea_orm::{
    ConnectionTrait,
    sea_query::{Alias, Expr, Query},
};
use sea_orm_migration::MigratorTrait;
use tjxy_common::{LibraryId, SortKey, StorageRootId};
use tjxy_credentials::{CredentialCipher, CredentialKey};
use tjxy_db::{
    LibraryRepository, StorageBindingDraft, StorageBindingRepository, StorageChangeFeedRepository,
    WorkJobRepository,
};
use tjxy_storage::ChangeCursor;
use tjxy_test_support::test_database;
use uuid::Uuid;

#[tokio::test]
#[allow(clippy::too_many_lines)] // Keeps the encrypted binding transaction and complete readback in one contract.
async fn encrypted_provider_binding_is_created_as_one_complete_runtime_scope() {
    let database = test_database().await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    let library_id = LibraryId::new();
    seed_library(&database, library_id).await;
    let credential_id = Uuid::new_v4();
    let cipher =
        CredentialCipher::new(CredentialKey::new(1, [7_u8; 32]).unwrap(), Vec::new()).unwrap();
    let envelope = cipher
        .seal(credential_id, "onedrive", b"oauth-payload")
        .unwrap();
    let draft = StorageBindingDraft::new(
        "onedrive",
        "Personal Drive",
        "account@example.invalid",
        credential_id,
        library_id,
        envelope,
        "drive-id",
        "root-item-id",
        "OneDrive",
        ChangeCursor::new(
            "https://graph.microsoft.com/v1.0/drives/drive-id/root/delta?token=opaque",
        )
        .unwrap(),
    )
    .unwrap();

    let created = StorageBindingRepository::new(&database)
        .create(&draft)
        .await
        .unwrap();

    assert_eq!(created.credential_id(), credential_id);
    for table in [
        "storage_credentials",
        "storage_accounts",
        "storage_roots",
        "storage_objects",
        "storage_root_objects",
        "library_storage_roots",
        "storage_sync_cursors",
        "work_jobs",
    ] {
        assert_eq!(count(&database, table).await, 1, "missing row in {table}");
    }
    let backend = database.get_database_backend();
    let initial_sync = database
        .query_one(
            backend.build(
                Query::select()
                    .columns([
                        Alias::new("id"),
                        Alias::new("task_kind"),
                        Alias::new("scope_type"),
                        Alias::new("scope_id"),
                        Alias::new("expected_revision"),
                        Alias::new("priority"),
                        Alias::new("state"),
                    ])
                    .from(Alias::new("work_jobs")),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        initial_sync.try_get::<Uuid>("", "id").unwrap(),
        created.initial_sync_job_id().as_uuid()
    );
    assert_eq!(
        initial_sync.try_get::<String>("", "task_kind").unwrap(),
        "ScopedStorageSync"
    );
    assert_eq!(
        initial_sync.try_get::<String>("", "scope_type").unwrap(),
        "StorageObject"
    );
    assert_eq!(
        initial_sync.try_get::<Uuid>("", "scope_id").unwrap(),
        created.root_object_id()
    );
    assert_eq!(
        initial_sync
            .try_get::<i64>("", "expected_revision")
            .unwrap(),
        0
    );
    assert_eq!(initial_sync.try_get::<i32>("", "priority").unwrap(), 50);
    assert_eq!(
        initial_sync.try_get::<String>("", "state").unwrap(),
        "Pending"
    );
    let plaintext = database
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("encrypted_payload"))
                    .from(Alias::new("storage_credentials"))
                    .and_where(Expr::col(Alias::new("id")).eq(credential_id)),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get::<Vec<u8>>("", "encrypted_payload")
        .unwrap();
    assert!(
        !plaintext
            .windows(13)
            .any(|window| window == b"oauth-payload")
    );

    let disabled = LibraryRepository::new(&database)
        .detach_root_by_name("Movies", StorageRootId::from_uuid(created.root_id()))
        .await
        .unwrap();
    assert_eq!(disabled.len(), 1);
    assert_eq!(disabled[0].account_id(), created.account_id());
    assert_eq!(disabled[0].provider_drive_id(), "drive-id");
    assert!(
        WorkJobRepository::new(&database)
            .claim_next_scoped_sync_for_drive(
                created.account_id(),
                "drive-id",
                "disabled-cloud-worker",
                Duration::minutes(5),
            )
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        StorageChangeFeedRepository::new(&database)
            .active_roots(created.account_id(), "drive-id")
            .await
            .unwrap()
            .is_empty()
    );
}

async fn seed_library(database: &sea_orm::DatabaseConnection, library: LibraryId) {
    database
        .execute(
            database.get_database_backend().build(
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
                        Alias::new("is_enabled"),
                    ])
                    .values_panic([
                        library.as_uuid().into(),
                        "Movies".into(),
                        "Lazy".into(),
                        "title_layer".into(),
                        "basic".into(),
                        "on_browse".into(),
                        "on_playback".into(),
                        1.into(),
                        "movies".into(),
                        SortKey::from_text("Movies").into_bytes().into(),
                        true.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
}

async fn count(database: &sea_orm::DatabaseConnection, table: &str) -> i64 {
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
