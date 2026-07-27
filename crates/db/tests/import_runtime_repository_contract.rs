use chrono::Utc;
use sea_orm::{
    ConnectionTrait,
    sea_query::{Alias, Expr, Query},
};
use sea_orm_migration::MigratorTrait;
use tjxy_common::Username;
use tjxy_credentials::{CredentialCipher, CredentialKey};
use tjxy_db::{
    AuthRepository, ImportJobRepository, ImportJobState, ImportRuntimeDraft,
    ImportRuntimeRepository,
};
use tjxy_test_support::test_database;
use uuid::Uuid;

#[tokio::test]
async fn configured_import_is_atomic_encrypted_and_restart_loadable() {
    let database = test_database().await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    let user = AuthRepository::new(&database)
        .create_user(
            &Username::parse("import-target").unwrap(),
            "$argon2id$test",
            false,
            false,
            Utc::now(),
        )
        .await
        .unwrap()
        .id()
        .as_uuid();
    let library = seed_library(&database).await;
    let source_id = Uuid::new_v4();
    let cipher =
        CredentialCipher::new(CredentialKey::new(1, [9_u8; 32]).unwrap(), Vec::new()).unwrap();
    let envelope = cipher
        .seal(source_id, "emby-import", b"api-key-secret")
        .unwrap();
    let draft =
        ImportRuntimeDraft::new(source_id, "legacy-instance", true, envelope, library, user)
            .unwrap();

    let created = ImportRuntimeRepository::new(&database)
        .create_emby(&draft)
        .await
        .unwrap();
    let loaded = ImportRuntimeRepository::new(&database)
        .source_for_job(created.job_id())
        .await
        .unwrap()
        .unwrap();

    assert_eq!(loaded.source_id(), source_id);
    assert_eq!(loaded.target_library_id(), library);
    assert_eq!(loaded.target_user_id(), user);
    assert_eq!(
        cipher
            .open(source_id, "emby-import", loaded.envelope())
            .unwrap()
            .as_slice(),
        b"api-key-secret"
    );
    assert_eq!(
        ImportJobRepository::new(&database)
            .get(created.job_id())
            .await
            .unwrap()
            .unwrap()
            .state(),
        ImportJobState::Pending
    );
    let backend = database.get_database_backend();
    let raw = database
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("encrypted_payload"))
                    .from(Alias::new("import_sources"))
                    .and_where(Expr::col(Alias::new("id")).eq(source_id)),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get::<Vec<u8>>("", "encrypted_payload")
        .unwrap();
    assert!(!raw.windows(14).any(|window| window == b"api-key-secret"));
}

async fn seed_library(database: &sea_orm::DatabaseConnection) -> Uuid {
    let library = Uuid::new_v4();
    let backend = database.get_database_backend();
    database
        .execute(
            backend.build(
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
                        library.into(),
                        "Imported".into(),
                        "Manual".into(),
                        "title_layer".into(),
                        "basic".into(),
                        "manual".into(),
                        "on_playback".into(),
                        1.into(),
                        "mixed".into(),
                        b"imported".to_vec().into(),
                        true.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    library
}
