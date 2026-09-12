use chrono::{Duration, Utc};
use sea_orm::{
    ConnectionTrait,
    sea_query::{Alias, Expr, Query},
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

    let item_id = Uuid::new_v4();
    database
        .execute(
            backend.build(
                Query::insert()
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
                        Alias::new("metadata_revision"),
                        Alias::new("is_present"),
                    ])
                    .values_panic([
                        item_id.into(),
                        "Movie".into(),
                        "Policy Fixture".into(),
                        "policy fixture".into(),
                        "Matched".into(),
                        "Ready".into(),
                        "NotApplicable".into(),
                        "Unknown".into(),
                        0_i64.into(),
                        0_i64.into(),
                        7_i64.into(),
                        true.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("library_catalog_items"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("library_id"),
                        Alias::new("catalog_item_id"),
                    ])
                    .values_panic([
                        Uuid::new_v4().into(),
                        library_id.as_uuid().into(),
                        item_id.into(),
                    ]),
            ),
        )
        .await
        .unwrap();

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
    let revision: i64 = database
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("metadata_revision"))
                    .from(Alias::new("catalog_items"))
                    .and_where(Expr::col(Alias::new("id")).eq(item_id)),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get("", "metadata_revision")
        .unwrap();
    assert_eq!(
        revision, 8,
        "metadata policy changes must invalidate members"
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
    assert_eq!(
        configs[0].provider_object_id(),
        "filesystem-root-id/filesystem-root-id"
    );
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

#[tokio::test]
#[allow(clippy::too_many_lines)] // Keeps membership, presence, and artifact assertions in one scenario.
async fn detaching_a_root_releases_unreachable_memberships_and_retires_orphaned_inventory() {
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
    let root = FilesystemRootDraft::new("/srv/media", "root-object-id", "media").unwrap();
    let created = repository
        .create_with_filesystem_root("Movies", "movies", &policy, &root)
        .await
        .unwrap();

    let file_id = Uuid::new_v4();
    seed_root_object(
        &database,
        created.account_id(),
        created.root_id().as_uuid(),
        file_id,
        "file-one",
        "Movie One.mkv",
    )
    .await;
    let movie_id = seed_presented_item(&database, created.library_id().as_uuid(), file_id).await;
    // A membership without storage reachability (for example an imported item)
    // survives the detach.
    let external_id = Uuid::new_v4();
    seed_catalog_item(&database, external_id).await;
    seed_membership(&database, created.library_id().as_uuid(), external_id).await;

    let backend = database.get_database_backend();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("nfo_choices"))
                    .columns([
                        Alias::new("catalog_item_id"),
                        Alias::new("storage_root_id"),
                        Alias::new("fingerprint"),
                        Alias::new("metadata_revision"),
                        Alias::new("input_sync_revision"),
                        Alias::new("candidates"),
                        Alias::new("conflict_fields"),
                        Alias::new("status"),
                        Alias::new("updated_at"),
                    ])
                    .values_panic([
                        movie_id.into(),
                        created.root_id().as_uuid().into(),
                        "fingerprint".into(),
                        1_i64.into(),
                        1_i64.into(),
                        json!([]).into(),
                        json!([]).into(),
                        "Resolved".into(),
                        Utc::now().into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("direct_metadata_refs"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("library_id"),
                        Alias::new("catalog_item_id"),
                        Alias::new("storage_root_id"),
                        Alias::new("storage_object_id"),
                        Alias::new("resource_kind"),
                        Alias::new("priority"),
                        Alias::new("input_revision"),
                    ])
                    .values_panic([
                        Uuid::new_v4().into(),
                        created.library_id().as_uuid().into(),
                        movie_id.into(),
                        created.root_id().as_uuid().into(),
                        file_id.into(),
                        "Nfo".into(),
                        0_i32.into(),
                        1_i64.into(),
                    ]),
            ),
        )
        .await
        .unwrap();

    let disabled = repository
        .detach_root_by_name("Movies", created.root_id())
        .await
        .unwrap();
    assert_eq!(disabled.len(), 1);
    assert_eq!(disabled[0].account_id(), created.account_id());

    assert_eq!(
        count_rows(&database, "library_storage_roots", "id").await,
        0
    );
    let remaining_membership = database
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("catalog_item_id"))
                    .from(Alias::new("library_catalog_items"))
                    .and_where(
                        Expr::col(Alias::new("library_id")).eq(created.library_id().as_uuid()),
                    ),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        remaining_membership
            .try_get::<Uuid>("", "catalog_item_id")
            .unwrap(),
        external_id,
        "membership of the detached root's item must be released"
    );
    for (table, column, expected) in [
        ("storage_root_objects", "presence_state", "ConfirmedAbsent"),
        ("storage_objects", "presence_state", "ConfirmedAbsent"),
        ("media_locations", "availability_state", "ConfirmedAbsent"),
        ("media_sources", "probe_state", "Stale"),
    ] {
        for state in query_column(&database, table, column).await {
            assert_eq!(state, expected, "{table}.{column}");
        }
    }
    let movie_present: bool = database
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("is_present"))
                    .from(Alias::new("catalog_items"))
                    .and_where(Expr::col(Alias::new("id")).eq(movie_id)),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get("", "is_present")
        .unwrap();
    assert!(!movie_present, "the detached item must tombstone");
    let external_present: bool = database
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("is_present"))
                    .from(Alias::new("catalog_items"))
                    .and_where(Expr::col(Alias::new("id")).eq(external_id)),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get("", "is_present")
        .unwrap();
    assert!(external_present);
    assert_eq!(
        count_rows(&database, "nfo_choices", "catalog_item_id").await,
        0
    );
    assert_eq!(count_rows(&database, "direct_metadata_refs", "id").await, 0);
    let account_status: String = database
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("status"))
                    .from(Alias::new("storage_accounts"))
                    .and_where(Expr::col(Alias::new("id")).eq(created.account_id())),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get("", "status")
        .unwrap();
    assert_eq!(account_status, "Disabled");
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Keeps the two-library fixture and its assertions in one scenario.
async fn detaching_a_shared_root_only_releases_the_detached_library() {
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
    let root = FilesystemRootDraft::new("/srv/media", "root-object-id", "media").unwrap();
    let movies = repository
        .create_with_filesystem_root("Movies", "movies", &policy, &root)
        .await
        .unwrap();
    let archive_id = repository
        .create("Archive", "movies", &policy)
        .await
        .unwrap();
    let bound = repository
        .attach_filesystem_root(archive_id, &root)
        .await
        .unwrap();
    assert_eq!(bound.root_id(), movies.root_id());

    let file_id = Uuid::new_v4();
    seed_root_object(
        &database,
        movies.account_id(),
        movies.root_id().as_uuid(),
        file_id,
        "file-one",
        "Movie One.mkv",
    )
    .await;
    let movie_id = seed_presented_item(&database, movies.library_id().as_uuid(), file_id).await;
    seed_membership(&database, archive_id.as_uuid(), movie_id).await;

    let backend = database.get_database_backend();
    for library in [movies.library_id().as_uuid(), archive_id.as_uuid()] {
        database
            .execute(
                backend.build(
                    Query::insert()
                        .into_table(Alias::new("direct_metadata_refs"))
                        .columns([
                            Alias::new("id"),
                            Alias::new("library_id"),
                            Alias::new("catalog_item_id"),
                            Alias::new("storage_root_id"),
                            Alias::new("storage_object_id"),
                            Alias::new("resource_kind"),
                            Alias::new("priority"),
                            Alias::new("input_revision"),
                        ])
                        .values_panic([
                            Uuid::new_v4().into(),
                            library.into(),
                            movie_id.into(),
                            movies.root_id().as_uuid().into(),
                            file_id.into(),
                            "Nfo".into(),
                            0_i32.into(),
                            1_i64.into(),
                        ]),
                ),
            )
            .await
            .unwrap();
    }
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("nfo_choices"))
                    .columns([
                        Alias::new("catalog_item_id"),
                        Alias::new("storage_root_id"),
                        Alias::new("fingerprint"),
                        Alias::new("metadata_revision"),
                        Alias::new("input_sync_revision"),
                        Alias::new("candidates"),
                        Alias::new("conflict_fields"),
                        Alias::new("status"),
                        Alias::new("updated_at"),
                    ])
                    .values_panic([
                        movie_id.into(),
                        movies.root_id().as_uuid().into(),
                        "fingerprint".into(),
                        1_i64.into(),
                        1_i64.into(),
                        json!([]).into(),
                        json!([]).into(),
                        "Resolved".into(),
                        Utc::now().into(),
                    ]),
            ),
        )
        .await
        .unwrap();

    let disabled = repository
        .detach_root_by_name("Movies", movies.root_id())
        .await
        .unwrap();
    assert!(disabled.is_empty(), "a shared root stays attached");

    let bindings: i64 = database
        .query_one(
            backend.build(
                Query::select()
                    .expr_as(Expr::col(Alias::new("id")).count(), Alias::new("count"))
                    .from(Alias::new("library_storage_roots")),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get("", "count")
        .unwrap();
    assert_eq!(bindings, 1);
    let members = database
        .query_all(
            backend.build(
                Query::select()
                    .column(Alias::new("library_id"))
                    .from(Alias::new("library_catalog_items"))
                    .and_where(Expr::col(Alias::new("catalog_item_id")).eq(movie_id)),
            ),
        )
        .await
        .unwrap();
    assert_eq!(members.len(), 1);
    assert_eq!(
        members[0].try_get::<Uuid>("", "library_id").unwrap(),
        archive_id.as_uuid()
    );
    for (table, column, expected) in [
        ("storage_objects", "presence_state", "Present"),
        ("media_locations", "availability_state", "Available"),
        ("media_sources", "probe_state", "Probed"),
    ] {
        for state in query_column(&database, table, column).await {
            assert_eq!(state, expected, "{table}.{column}");
        }
    }
    // The shared root's relation rows stay present for the other library; NFO
    // choices belong to the root and stay, while direct refs are released per
    // binding.
    assert_eq!(
        count_rows(&database, "nfo_choices", "catalog_item_id").await,
        1
    );
    let remaining_refs = database
        .query_all(
            backend.build(
                Query::select()
                    .column(Alias::new("library_id"))
                    .from(Alias::new("direct_metadata_refs")),
            ),
        )
        .await
        .unwrap();
    assert_eq!(remaining_refs.len(), 1);
    assert_eq!(
        remaining_refs[0].try_get::<Uuid>("", "library_id").unwrap(),
        archive_id.as_uuid()
    );
    let account_status: String = database
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("status"))
                    .from(Alias::new("storage_accounts"))
                    .and_where(Expr::col(Alias::new("id")).eq(movies.account_id())),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get("", "status")
        .unwrap();
    assert_eq!(account_status, "Active");
}

async fn seed_catalog_item(database: &sea_orm::DatabaseConnection, item_id: Uuid) {
    let backend = database.get_database_backend();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("catalog_items"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("item_type"),
                        Alias::new("name"),
                        Alias::new("sort_name"),
                        Alias::new("metadata_state"),
                        Alias::new("classification_state"),
                        Alias::new("structure_state"),
                        Alias::new("source_state"),
                        Alias::new("structure_expansion_revision"),
                        Alias::new("source_index_revision"),
                        Alias::new("metadata_revision"),
                        Alias::new("is_present"),
                    ])
                    .values_panic([
                        item_id.into(),
                        "Movie".into(),
                        "Movie".into(),
                        "movie".into(),
                        "Ready".into(),
                        "Matched".into(),
                        "NotApplicable".into(),
                        "Indexed".into(),
                        0_i64.into(),
                        0_i64.into(),
                        0_i64.into(),
                        true.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
}

async fn seed_membership(database: &sea_orm::DatabaseConnection, library_id: Uuid, item_id: Uuid) {
    let backend = database.get_database_backend();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("library_catalog_items"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("library_id"),
                        Alias::new("catalog_item_id"),
                    ])
                    .values_panic([Uuid::new_v4().into(), library_id.into(), item_id.into()]),
            ),
        )
        .await
        .unwrap();
}

async fn seed_root_object(
    database: &sea_orm::DatabaseConnection,
    account_id: Uuid,
    root_id: Uuid,
    object_id: Uuid,
    provider_object_id: &str,
    name: &str,
) {
    let backend = database.get_database_backend();
    for statement in [
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
                Alias::new("facts_observed_storage_root_id"),
                Alias::new("children_indexed"),
                Alias::new("children_index_revision"),
                Alias::new("identity_quality"),
                Alias::new("presence_state"),
            ])
            .values_panic([
                object_id.into(),
                account_id.into(),
                "local".into(),
                provider_object_id.into(),
                Uuid::new_v4().into_bytes().to_vec().into(),
                name.into(),
                name.to_lowercase().into(),
                "File".into(),
                1_i64.into(),
                root_id.into(),
                false.into(),
                0_i64.into(),
                "ProviderStableId".into(),
                "Present".into(),
            ])
            .to_owned(),
        Query::insert()
            .into_table(Alias::new("storage_root_objects"))
            .columns([
                Alias::new("id"),
                Alias::new("storage_root_id"),
                Alias::new("storage_object_id"),
                Alias::new("observed_sync_revision"),
                Alias::new("children_indexed"),
                Alias::new("children_index_revision"),
                Alias::new("presence_state"),
            ])
            .values_panic([
                Uuid::new_v4().into(),
                root_id.into(),
                object_id.into(),
                1_i64.into(),
                false.into(),
                0_i64.into(),
                "Present".into(),
            ])
            .to_owned(),
    ] {
        database.execute(backend.build(&statement)).await.unwrap();
    }
}

async fn seed_presented_item(
    database: &sea_orm::DatabaseConnection,
    library_id: Uuid,
    object_id: Uuid,
) -> Uuid {
    let item_id = Uuid::new_v4();
    let source_id = Uuid::new_v4();
    seed_catalog_item(database, item_id).await;
    seed_membership(database, library_id, item_id).await;
    let backend = database.get_database_backend();
    for statement in [
        Query::insert()
            .into_table(Alias::new("identity_matches"))
            .columns([
                Alias::new("id"),
                Alias::new("storage_object_id"),
                Alias::new("candidate_catalog_item_id"),
                Alias::new("confidence"),
                Alias::new("state"),
                Alias::new("evidence"),
            ])
            .values_panic([
                Uuid::new_v4().into(),
                object_id.into(),
                item_id.into(),
                1.0.into(),
                "Matched".into(),
                json!({"kind":"fixture"}).into(),
            ])
            .to_owned(),
        Query::insert()
            .into_table(Alias::new("media_sources"))
            .columns([
                Alias::new("id"),
                Alias::new("catalog_item_id"),
                Alias::new("presentation_key"),
                Alias::new("probe_state"),
                Alias::new("probe_revision"),
            ])
            .values_panic([
                source_id.into(),
                item_id.into(),
                Uuid::new_v4().into(),
                "Probed".into(),
                0_i64.into(),
            ])
            .to_owned(),
        Query::insert()
            .into_table(Alias::new("media_locations"))
            .columns([
                Alias::new("id"),
                Alias::new("media_source_id"),
                Alias::new("storage_object_id"),
                Alias::new("availability_state"),
                Alias::new("priority"),
            ])
            .values_panic([
                Uuid::new_v4().into(),
                source_id.into(),
                object_id.into(),
                "Available".into(),
                0_i32.into(),
            ])
            .to_owned(),
    ] {
        database.execute(backend.build(&statement)).await.unwrap();
    }
    item_id
}

async fn count_rows(database: &sea_orm::DatabaseConnection, table: &str, column: &str) -> i64 {
    let backend = database.get_database_backend();
    database
        .query_one(
            backend.build(
                Query::select()
                    .expr_as(Expr::col(Alias::new(column)).count(), Alias::new("count"))
                    .from(Alias::new(table)),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get("", "count")
        .unwrap()
}

async fn query_column(
    database: &sea_orm::DatabaseConnection,
    table: &str,
    column: &str,
) -> Vec<String> {
    let backend = database.get_database_backend();
    database
        .query_all(
            backend.build(
                Query::select()
                    .column(Alias::new(column))
                    .from(Alias::new(table)),
            ),
        )
        .await
        .unwrap()
        .iter()
        .map(|row| row.try_get::<String>("", column).unwrap())
        .collect()
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
