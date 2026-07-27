use chrono::Utc;
use sea_orm::{
    ConnectionTrait, DatabaseConnection,
    sea_query::{Alias, Expr, Query},
};
use sea_orm_migration::MigratorTrait;
use tjxy_common::{CatalogItemId, SortKey, UserId, Username};
use tjxy_db::{AuthRepository, MediaCollectionRepository, MediaCollectionRepositoryError};
use tjxy_test_support::test_database;
use uuid::Uuid;

async fn database() -> DatabaseConnection {
    let database = test_database().await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    database
}

async fn seed_user(database: &DatabaseConnection, name: &str) -> UserId {
    AuthRepository::new(database)
        .create_user(
            &Username::parse(name).unwrap(),
            "$argon2id$test",
            true,
            false,
            Utc::now(),
        )
        .await
        .unwrap()
        .id()
}

async fn seed_library(database: &DatabaseConnection, enabled: bool) -> Uuid {
    let id = Uuid::new_v4();
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
                        id.into(),
                        "Movies".into(),
                        "Lazy".into(),
                        "title_layer".into(),
                        "basic".into(),
                        "on_browse".into(),
                        "on_playback".into(),
                        1.into(),
                        "movies".into(),
                        SortKey::from_text("Movies").into_bytes().into(),
                        enabled.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    id
}

async fn seed_item(
    database: &DatabaseConnection,
    library: Uuid,
    name: &str,
    is_present: bool,
) -> CatalogItemId {
    let id = CatalogItemId::new();
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
                        Alias::new("sort_key"),
                        Alias::new("classification_state"),
                        Alias::new("metadata_state"),
                        Alias::new("structure_state"),
                        Alias::new("source_state"),
                        Alias::new("structure_expansion_revision"),
                        Alias::new("source_index_revision"),
                        Alias::new("is_present"),
                    ])
                    .values_panic([
                        id.as_uuid().into(),
                        "Movie".into(),
                        name.into(),
                        name.to_lowercase().into(),
                        SortKey::from_text(name).into_bytes().into(),
                        "Matched".into(),
                        "Ready".into(),
                        "NotApplicable".into(),
                        "Indexed".into(),
                        0_i64.into(),
                        0_i64.into(),
                        is_present.into(),
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
                    .values_panic([Uuid::new_v4().into(), library.into(), id.as_uuid().into()]),
            ),
        )
        .await
        .unwrap();
    id
}

#[tokio::test]
async fn playlist_rejects_invisible_writes_retains_duplicate_entries_and_enforces_ownership() {
    let database = database().await;
    let owner = seed_user(&database, "alice").await;
    let other = seed_user(&database, "bob").await;
    let library = seed_library(&database, true).await;
    let visible = seed_item(&database, library, "Arrival", true).await;
    let tombstone = seed_item(&database, library, "Missing", false).await;
    let repository = MediaCollectionRepository::new(&database);

    let playlist = repository
        .create_playlist(owner, "Road trip")
        .await
        .unwrap();
    let rejected = repository
        .append_items(owner, playlist.id(), &[visible, tombstone])
        .await;
    assert!(matches!(
        rejected,
        Err(MediaCollectionRepositoryError::ItemUnavailable(id)) if id == tombstone
    ));
    assert!(
        repository
            .items(owner, playlist.id())
            .await
            .unwrap()
            .is_empty()
    );

    repository
        .append_items(owner, playlist.id(), &[visible, visible])
        .await
        .unwrap();
    let entries = repository.items(owner, playlist.id()).await.unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].item().name(), "Arrival");
    assert_eq!(entries[1].item().name(), "Arrival");
    assert_ne!(entries[0].id(), entries[1].id());
    repository
        .move_item(owner, playlist.id(), entries[1].id(), 0)
        .await
        .unwrap();
    let reordered = repository.items(owner, playlist.id()).await.unwrap();
    assert_eq!(reordered[0].id(), entries[1].id());
    repository
        .delete_item(owner, playlist.id(), entries[1].id())
        .await
        .unwrap();
    let remaining = repository.items(owner, playlist.id()).await.unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].id(), entries[0].id());
    assert!(matches!(
        repository.items(other, playlist.id()).await,
        Err(MediaCollectionRepositoryError::Forbidden)
    ));
}

#[tokio::test]
async fn shared_collection_filters_entries_that_become_invisible_after_publication() {
    let database = database().await;
    let library = seed_library(&database, true).await;
    let item = seed_item(&database, library, "Arrival", true).await;
    let repository = MediaCollectionRepository::new(&database);
    let collection = repository
        .create_shared_collection("Staff picks")
        .await
        .unwrap();
    repository
        .append_shared_items(collection.id(), &[item])
        .await
        .unwrap();
    assert_eq!(
        repository
            .shared_items(collection.id())
            .await
            .unwrap()
            .len(),
        1
    );

    database
        .execute(
            database.get_database_backend().build(
                Query::update()
                    .table(Alias::new("catalog_items"))
                    .value(Alias::new("is_present"), false)
                    .and_where(Expr::col(Alias::new("id")).eq(item.as_uuid())),
            ),
        )
        .await
        .unwrap();

    assert!(
        repository
            .shared_items(collection.id())
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn collection_management_lists_renames_and_deletes_with_the_correct_scope() {
    let database = database().await;
    let alice = seed_user(&database, "alice").await;
    let bob = seed_user(&database, "bob").await;
    let repository = MediaCollectionRepository::new(&database);
    let playlist = repository.create_playlist(alice, "Initial").await.unwrap();
    let shared = repository
        .create_shared_collection("Initial shared")
        .await
        .unwrap();

    assert_eq!(repository.playlists(alice).await.unwrap().len(), 1);
    assert!(repository.playlists(bob).await.unwrap().is_empty());
    let renamed_playlist = repository
        .rename_playlist(alice, playlist.id(), "Renamed")
        .await
        .unwrap();
    assert_eq!(renamed_playlist.name(), "Renamed");
    assert!(matches!(
        repository
            .rename_playlist(bob, playlist.id(), "Denied")
            .await,
        Err(MediaCollectionRepositoryError::Forbidden)
    ));

    assert_eq!(repository.shared_collections().await.unwrap().len(), 1);
    let renamed_shared = repository
        .rename_shared_collection(shared.id(), "Renamed shared")
        .await
        .unwrap();
    assert_eq!(renamed_shared.name(), "Renamed shared");
    repository
        .delete_playlist(alice, playlist.id())
        .await
        .unwrap();
    repository
        .delete_shared_collection(shared.id())
        .await
        .unwrap();
    assert!(repository.playlists(alice).await.unwrap().is_empty());
    assert!(repository.shared_collections().await.unwrap().is_empty());
}
