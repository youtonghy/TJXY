use chrono::{Duration, Utc};
use sea_orm::{
    ConnectionTrait,
    sea_query::{Alias, Query},
};
use sea_orm_migration::MigratorTrait;
use serde_json::json;
use tjxy_common::{LibraryId, SortKey, Username};
use tjxy_db::{
    AuthRepository, FilesystemRootDraft, LibraryPolicyUpdate, LibraryRepository,
    LibraryRepositoryError, Migrator, WorkJobRepository, WorkTaskKind,
};
use tjxy_test_support::test_database;
use uuid::Uuid;

#[tokio::test]
#[allow(clippy::too_many_lines)] // Keeps policy, multi-root aggregation, and secret exclusion in one fixture.
async fn virtual_folders_aggregate_effective_policy_and_opaque_roots_in_stable_order() {
    let database = test_database().await.unwrap();
    Migrator::up(&database, None).await.unwrap();
    let backend = database.get_database_backend();
    let library_id = LibraryId::new();
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
                        Alias::new("metadata_source_mode"),
                        Alias::new("expansion_policy"),
                        Alias::new("probe_policy"),
                        Alias::new("profile_version"),
                        Alias::new("collection_type"),
                        Alias::new("sort_key"),
                        Alias::new("is_enabled"),
                    ])
                    .values_panic([
                        library_id.as_uuid().into(),
                        "Movies".into(),
                        "Lazy".into(),
                        "title_layer".into(),
                        "basic".into(),
                        "local_only".into(),
                        "on_browse".into(),
                        "on_playback".into(),
                        3.into(),
                        "movies".into(),
                        SortKey::from_text("Movies").into_bytes().into(),
                        true.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    for (provider, name) in [("Filesystem", "Local"), ("GoogleDrive", "Cloud")] {
        let account_id = Uuid::new_v4();
        let root_id = Uuid::new_v4();
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
                            provider.into(),
                            name.into(),
                            format!("{provider}-{name}").into(),
                            format!("credential-{name}").into(),
                            "Ready".into(),
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
                            root_id.into(),
                            account_id.into(),
                            format!("secret-{name}").into(),
                            0.into(),
                            0.into(),
                        ]),
                ),
            )
            .await
            .unwrap();
        database
            .execute(
                backend.build(
                    Query::insert()
                        .into_table(Alias::new("library_storage_roots"))
                        .columns([
                            Alias::new("id"),
                            Alias::new("library_id"),
                            Alias::new("storage_root_id"),
                        ])
                        .values_panic([
                            Uuid::new_v4().into(),
                            library_id.as_uuid().into(),
                            root_id.into(),
                        ]),
                ),
            )
            .await
            .unwrap();
    }

    let folders = LibraryRepository::new(&database)
        .virtual_folders()
        .await
        .unwrap();

    assert_eq!(folders.len(), 1);
    let folder = &folders[0];
    assert_eq!(folder.id(), library_id);
    assert_eq!(folder.scan_profile(), "Lazy");
    assert_eq!(folder.metadata_source_mode(), "local_only");
    assert_eq!(folder.profile_version(), 3);
    assert_eq!(folder.roots().len(), 2);
    assert!(
        folder
            .roots()
            .iter()
            .all(|root| !root.location().contains("secret"))
    );

    let update = LibraryPolicyUpdate::new(
        "Full",
        "all_synced_objects",
        "full",
        "eager",
        "eager",
        false,
    )
    .unwrap();
    assert_eq!(
        LibraryRepository::new(&database)
            .update_policy(library_id, 3, &update)
            .await
            .unwrap(),
        4
    );
    let stale = LibraryRepository::new(&database)
        .update_policy(library_id, 3, &update)
        .await
        .unwrap_err();
    assert!(matches!(stale, LibraryRepositoryError::StaleProfile));
}

#[tokio::test]
async fn virtual_folder_create_and_delete_are_atomic_and_reference_safe() {
    let database = test_database().await.unwrap();
    Migrator::up(&database, None).await.unwrap();
    let repository = LibraryRepository::new(&database);
    let policy = LibraryPolicyUpdate::new(
        "Lazy",
        "title_layer",
        "basic",
        "on_browse",
        "on_playback",
        true,
    )
    .unwrap();

    let library_id = repository
        .create("Movies", "movies", &policy)
        .await
        .unwrap();
    assert_eq!(catalog_generation(&database).await, 1);
    let duplicate = repository
        .create("Movies", "movies", &policy)
        .await
        .unwrap_err();
    assert!(matches!(duplicate, LibraryRepositoryError::NameConflict));
    assert_eq!(catalog_generation(&database).await, 1);
    let folders = repository.virtual_folders().await.unwrap();
    assert_eq!(folders.len(), 1);
    assert_eq!(folders[0].id(), library_id);
    assert_eq!(folders[0].profile_version(), 1);

    let referenced_id = repository
        .create("Imported", "mixed", &policy)
        .await
        .unwrap();
    seed_import_reference(&database, referenced_id).await;
    assert!(matches!(
        repository.delete_by_name("Imported").await.unwrap_err(),
        LibraryRepositoryError::Referenced
    ));
    assert_eq!(catalog_generation(&database).await, 2);

    repository.delete_by_name("Movies").await.unwrap();
    assert_eq!(catalog_generation(&database).await, 3);
    let folders = repository.virtual_folders().await.unwrap();
    assert_eq!(folders.len(), 1);
    assert_eq!(folders[0].id(), referenced_id);
    assert!(matches!(
        repository.delete_by_name("Movies").await.unwrap_err(),
        LibraryRepositoryError::NotFound
    ));
}

#[tokio::test]
async fn direct_mode_requires_local_only_and_a_filesystem_root() {
    let database = test_database().await.unwrap();
    Migrator::up(&database, None).await.unwrap();
    let repository = LibraryRepository::new(&database);
    let automatic_direct = LibraryPolicyUpdate::new(
        "Lazy",
        "title_layer",
        "basic",
        "on_browse",
        "on_playback",
        true,
    )
    .unwrap()
    .with_local_metadata_access_mode("direct")
    .unwrap();
    assert!(matches!(
        repository
            .create("Invalid", "movies", &automatic_direct)
            .await,
        Err(LibraryRepositoryError::InvalidStoredPolicy)
    ));

    let local_direct = LibraryPolicyUpdate::new(
        "Lazy",
        "title_layer",
        "basic",
        "on_browse",
        "on_playback",
        true,
    )
    .unwrap()
    .with_metadata_source_mode("local_only")
    .unwrap()
    .with_local_metadata_access_mode("direct")
    .unwrap();
    assert!(matches!(
        repository.create("No Root", "movies", &local_direct).await,
        Err(LibraryRepositoryError::DirectRequiresFilesystemRoot)
    ));
}

#[tokio::test]
async fn virtual_folder_rename_updates_sorting_once_and_rejects_conflicts() {
    let database = test_database().await.unwrap();
    Migrator::up(&database, None).await.unwrap();
    let repository = LibraryRepository::new(&database);
    let policy = LibraryPolicyUpdate::new(
        "Lazy",
        "title_layer",
        "basic",
        "on_browse",
        "on_playback",
        true,
    )
    .unwrap();
    repository.create("Zulu", "movies", &policy).await.unwrap();
    repository.create("Alpha", "movies", &policy).await.unwrap();

    repository.rename_by_name("Zulu", "Beta").await.unwrap();
    assert_eq!(catalog_generation(&database).await, 3);
    let folders = repository.virtual_folders().await.unwrap();
    assert_eq!(
        folders
            .iter()
            .map(tjxy_db::VirtualFolderRecord::name)
            .collect::<Vec<_>>(),
        ["Alpha", "Beta"]
    );

    assert!(matches!(
        repository
            .rename_by_name("Beta", "Alpha")
            .await
            .unwrap_err(),
        LibraryRepositoryError::NameConflict
    ));
    assert_eq!(catalog_generation(&database).await, 3);
}

#[tokio::test]
async fn filesystem_root_binding_is_restartable_and_last_detach_disables_without_deleting() {
    let database = test_database().await.unwrap();
    Migrator::up(&database, None).await.unwrap();
    let repository = LibraryRepository::new(&database);
    let policy = LibraryPolicyUpdate::new(
        "Lazy",
        "title_layer",
        "basic",
        "on_browse",
        "on_playback",
        true,
    )
    .unwrap();
    let root = FilesystemRootDraft::new(
        "/srv/media",
        "filesystem-root-id/filesystem-root-id",
        "media",
    )
    .unwrap();

    let created = repository
        .create_with_filesystem_root("Movies", "movies", &policy, &root)
        .await
        .unwrap();
    assert_eq!(catalog_generation(&database).await, 1);
    let configs = repository.active_filesystem_roots().await.unwrap();
    assert_eq!(configs.len(), 1);
    assert_eq!(configs[0].account_id(), created.account_id());
    assert_eq!(configs[0].root_path(), "/srv/media");
    let job = WorkJobRepository::new(&database)
        .get(created.initial_sync_job_id())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(job.task_kind(), WorkTaskKind::ScopedStorageSync);

    let disabled = repository
        .detach_root_by_name("Movies", created.root_id())
        .await
        .unwrap();
    assert_eq!(disabled.len(), 1);
    assert_eq!(disabled[0].account_id(), created.account_id());
    assert_eq!(disabled[0].provider_drive_id(), "local");
    assert!(
        WorkJobRepository::new(&database)
            .claim_next_scoped_sync(
                created.account_id(),
                "disabled-account-worker",
                Duration::minutes(5),
            )
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(catalog_generation(&database).await, 2);
    assert!(
        repository
            .active_filesystem_roots()
            .await
            .unwrap()
            .is_empty()
    );
    let backend = database.get_database_backend();
    for table in ["storage_accounts", "storage_roots", "storage_objects"] {
        let count = database
            .query_one(
                backend.build(
                    Query::select()
                        .expr_as(
                            sea_orm::sea_query::Expr::col(Alias::new("id")).count(),
                            Alias::new("count"),
                        )
                        .from(Alias::new(table)),
                ),
            )
            .await
            .unwrap()
            .unwrap()
            .try_get::<i64>("", "count")
            .unwrap();
        assert_eq!(count, 1, "{table} was deleted during detach");
    }

    let rebound = repository
        .create_with_filesystem_root("Archive", "movies", &policy, &root)
        .await
        .unwrap();
    assert_eq!(rebound.account_id(), created.account_id());
    assert_eq!(rebound.root_id(), created.root_id());
    assert_eq!(repository.active_filesystem_roots().await.unwrap().len(), 1);
    assert_eq!(catalog_generation(&database).await, 3);
}

async fn seed_import_reference(database: &sea_orm::DatabaseConnection, library_id: LibraryId) {
    let username = Username::parse("Importer").unwrap();
    let user = AuthRepository::new(database)
        .create_user(&username, "encoded-password", true, false, Utc::now())
        .await
        .unwrap();
    let import_job = Uuid::new_v4();
    let backend = database.get_database_backend();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("import_jobs"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("adapter_kind"),
                        Alias::new("source_instance_id"),
                        Alias::new("state"),
                        Alias::new("dry_run"),
                        Alias::new("checkpoint"),
                        Alias::new("counters"),
                        Alias::new("attempt_count"),
                    ])
                    .values_panic([
                        import_job.into(),
                        "EmbyApi".into(),
                        "reference-contract".into(),
                        "Pending".into(),
                        false.into(),
                        json!({}).into(),
                        json!({}).into(),
                        0_i32.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("import_sources"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("import_job_id"),
                        Alias::new("encrypted_payload"),
                        Alias::new("key_version"),
                        Alias::new("target_library_id"),
                        Alias::new("target_user_id"),
                    ])
                    .values_panic([
                        Uuid::new_v4().into(),
                        import_job.into(),
                        vec![1_u8, 2, 3].into(),
                        1_i32.into(),
                        library_id.as_uuid().into(),
                        user.id().as_uuid().into(),
                    ]),
            ),
        )
        .await
        .unwrap();
}

async fn catalog_generation(database: &sea_orm::DatabaseConnection) -> i64 {
    let backend = database.get_database_backend();
    database
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("generation"))
                    .from(Alias::new("catalog_state")),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get("", "generation")
        .unwrap()
}
