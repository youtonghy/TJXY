use sea_orm::{
    ConnectionTrait,
    sea_query::{Alias, Expr, Query},
};
use sea_orm_migration::MigratorTrait;
use sha2::{Digest, Sha256};
use tjxy_common::{StorageObjectRecordId, StorageRootId};
use tjxy_db::FilesystemPathRepository;
use tjxy_test_support::test_database;
use uuid::Uuid;

fn identity_key(provider_object_id: &str) -> String {
    let mut digest = Sha256::new();
    for part in ["local", provider_object_id] {
        digest.update((part.len() as u64).to_be_bytes());
        digest.update(part.as_bytes());
    }
    format!("{:x}", digest.finalize())
}

async fn insert_object(
    database: &sea_orm::DatabaseConnection,
    account_id: Uuid,
    object_id: StorageObjectRecordId,
    provider_object_id: &str,
    name: &str,
    object_type: &str,
) {
    let backend = database.get_database_backend();
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
                        Alias::new("identity_key"),
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
                        provider_object_id.into(),
                        identity_key(provider_object_id).into(),
                        name.into(),
                        name.to_lowercase().into(),
                        object_type.into(),
                        7_i64.into(),
                        (object_type == "Directory").into(),
                        7_i64.into(),
                        "StableFileId".into(),
                        "Present".into(),
                    ]),
            ),
        )
        .await
        .unwrap();
}

async fn insert_relation(
    database: &sea_orm::DatabaseConnection,
    root_id: StorageRootId,
    object_id: StorageObjectRecordId,
    parent_id: Option<StorageObjectRecordId>,
) {
    let backend = database.get_database_backend();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("storage_root_objects"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("storage_root_id"),
                        Alias::new("storage_object_id"),
                        Alias::new("parent_storage_object_id"),
                        Alias::new("observed_sync_revision"),
                        Alias::new("children_indexed"),
                        Alias::new("children_index_revision"),
                        Alias::new("presence_state"),
                    ])
                    .values_panic([
                        Uuid::new_v4().into(),
                        root_id.as_uuid().into(),
                        object_id.as_uuid().into(),
                        parent_id.map(StorageObjectRecordId::as_uuid).into(),
                        7_i64.into(),
                        true.into(),
                        7_i64.into(),
                        "Present".into(),
                    ]),
            ),
        )
        .await
        .unwrap();
}

async fn insert_account_and_root(
    database: &sea_orm::DatabaseConnection,
    account_id: Uuid,
    root_id: StorageRootId,
) {
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
                        "Filesystem".into(),
                        Uuid::new_v4().to_string().into(),
                        "filesystem-test".into(),
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
                        "root/root".into(),
                        7_i64.into(),
                        7_i64.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
}

async fn rename_object(
    database: &sea_orm::DatabaseConnection,
    object_id: StorageObjectRecordId,
    name: &str,
) {
    let backend = database.get_database_backend();
    database
        .execute(
            backend.build(
                &Query::update()
                    .table(Alias::new("storage_objects"))
                    .value(Alias::new("name"), name)
                    .value(Alias::new("normalized_name"), name.to_lowercase())
                    .and_where(Expr::col(Alias::new("id")).eq(object_id.as_uuid()))
                    .to_owned(),
            ),
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn normalized_root_relations_resolve_and_follow_directory_renames() {
    let database = test_database().await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    let account_id = Uuid::new_v4();
    let root_id = StorageRootId::new();
    let root_object = StorageObjectRecordId::new();
    let directory = StorageObjectRecordId::new();
    let poster = StorageObjectRecordId::new();
    insert_account_and_root(&database, account_id, root_id).await;
    insert_object(
        &database,
        account_id,
        root_object,
        "root/root",
        "Media",
        "Directory",
    )
    .await;
    insert_object(
        &database,
        account_id,
        directory,
        "root/directory",
        "Movies",
        "Directory",
    )
    .await;
    insert_object(
        &database,
        account_id,
        poster,
        "root/poster",
        "poster.jpg",
        "File",
    )
    .await;
    insert_relation(&database, root_id, root_object, None).await;
    insert_relation(&database, root_id, directory, Some(root_object)).await;
    insert_relation(&database, root_id, poster, Some(directory)).await;

    let repository = FilesystemPathRepository::new(&database);
    let path = repository
        .resolve(account_id, "root/poster")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        path.relative_path(),
        std::path::Path::new("Movies/poster.jpg")
    );

    rename_object(&database, directory, "Films").await;
    let renamed = repository
        .resolve(account_id, "root/poster")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        renamed.relative_path(),
        std::path::Path::new("Films/poster.jpg")
    );
}
