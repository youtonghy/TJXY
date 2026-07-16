use chrono::Utc;
use sea_orm::{
    ConnectionTrait, Database, DatabaseConnection, DbBackend, Statement,
    sea_query::{Alias, Query},
};
use sea_orm_migration::MigratorTrait;
use tjxy_common::{CatalogItemId, UserId, Username};
use tjxy_db::{
    AuthRepository, BrowseParent, CatalogItemType, CatalogPageRequest, CatalogQueryRepository,
};
use tjxy_db::{UserDataPatch, UserDataRepository};
use uuid::Uuid;

async fn database() -> DatabaseConnection {
    let database = Database::connect("sqlite::memory:").await.unwrap();
    database
        .execute(Statement::from_string(
            DbBackend::Sqlite,
            "PRAGMA foreign_keys = ON".to_owned(),
        ))
        .await
        .unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    database
}

async fn seed_user(database: &DatabaseConnection) -> UserId {
    AuthRepository::new(database)
        .create_user(
            &Username::parse("alice").unwrap(),
            "$argon2id$test",
            true,
            false,
            Utc::now(),
        )
        .await
        .unwrap()
        .id()
}

async fn seed_library(
    database: &DatabaseConnection,
    name: &str,
    collection_type: &str,
    enabled: bool,
) -> Uuid {
    let id = Uuid::new_v4();
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
                        id.into(),
                        name.into(),
                        "Lazy".into(),
                        "title_layer".into(),
                        "basic".into(),
                        "on_browse".into(),
                        "on_playback".into(),
                        1.into(),
                        collection_type.into(),
                        tjxy_common::SortKey::from_text(name).into_bytes().into(),
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
    name: &str,
    item_type: &str,
    parent_id: Option<Uuid>,
    is_present: bool,
    classification_state: &str,
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
                        Alias::new("parent_id"),
                        Alias::new("item_type"),
                        Alias::new("name"),
                        Alias::new("sort_name"),
                        Alias::new("sort_key"),
                        Alias::new("production_year"),
                        Alias::new("overview"),
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
                        parent_id.into(),
                        item_type.into(),
                        name.into(),
                        name.to_lowercase().into(),
                        tjxy_common::SortKey::from_text(name).into_bytes().into(),
                        2016.into(),
                        format!("Overview for {name}").into(),
                        classification_state.into(),
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
    id
}

async fn add_to_library(database: &DatabaseConnection, library_id: Uuid, item_id: CatalogItemId) {
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
                    .values_panic([
                        Uuid::new_v4().into(),
                        library_id.into(),
                        item_id.as_uuid().into(),
                    ]),
            ),
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn user_views_are_enabled_and_sorted_by_portable_key() {
    let database = database().await;
    seed_library(&database, "Ｚeta", "folders", true).await;
    seed_library(&database, "Alpha", "movies", true).await;
    seed_library(&database, "Hidden", "folders", false).await;

    let views = CatalogQueryRepository::new(&database)
        .user_views()
        .await
        .unwrap();

    assert_eq!(
        views
            .iter()
            .map(tjxy_db::LibraryViewRecord::name)
            .collect::<Vec<_>>(),
        ["Alpha", "Ｚeta"]
    );
    assert_eq!(views[0].collection_type(), "movies");
}

#[tokio::test]
async fn library_page_filters_membership_tombstones_and_candidates() {
    let database = database().await;
    let user_id = seed_user(&database).await;
    let library = seed_library(&database, "Movies", "movies", true).await;
    let other = seed_library(&database, "Other", "folders", true).await;
    let arrival = seed_item(&database, "Arrival", "Movie", None, true, "Matched").await;
    let blade = seed_item(&database, "Blade Runner", "Movie", None, true, "Matched").await;
    let absent = seed_item(&database, "Gone", "Movie", None, false, "Matched").await;
    let candidate = seed_item(&database, "Maybe", "Movie", None, true, "Candidate").await;
    let foreign = seed_item(&database, "Foreign", "Movie", None, true, "Matched").await;
    for item in [arrival, blade, absent, candidate] {
        add_to_library(&database, library, item).await;
    }
    add_to_library(&database, other, foreign).await;

    let page = CatalogQueryRepository::new(&database)
        .items(
            user_id,
            BrowseParent::Library(library),
            CatalogPageRequest::new(1, 1).unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(page.total_record_count(), 2);
    assert_eq!(page.items().len(), 1);
    assert_eq!(page.items()[0].name(), "Blade Runner");
}

#[tokio::test]
async fn child_page_requires_shared_library_membership_with_parent() {
    let database = database().await;
    let user_id = seed_user(&database).await;
    let first = seed_library(&database, "TV", "tvshows", true).await;
    let second = seed_library(&database, "Other", "folders", true).await;
    let series = seed_item(&database, "Series", "Series", None, true, "Matched").await;
    let visible = seed_item(
        &database,
        "Season 1",
        "Season",
        Some(series.as_uuid()),
        true,
        "Matched",
    )
    .await;
    let leaked = seed_item(
        &database,
        "Leaked",
        "Season",
        Some(series.as_uuid()),
        true,
        "Matched",
    )
    .await;
    add_to_library(&database, first, series).await;
    add_to_library(&database, first, visible).await;
    add_to_library(&database, second, leaked).await;

    let page = CatalogQueryRepository::new(&database)
        .items(
            user_id,
            BrowseParent::Item(series),
            CatalogPageRequest::new(0, 200).unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(page.total_record_count(), 1);
    assert_eq!(page.items()[0].name(), "Season 1");
}

#[tokio::test]
async fn disabled_library_cannot_be_browsed_by_id() {
    let database = database().await;
    let user_id = seed_user(&database).await;
    let library = seed_library(&database, "Disabled", "movies", false).await;
    let item = seed_item(&database, "Hidden", "Movie", None, true, "Matched").await;
    add_to_library(&database, library, item).await;

    let page = CatalogQueryRepository::new(&database)
        .items(
            user_id,
            BrowseParent::Library(library),
            CatalogPageRequest::new(0, 20).unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(page.total_record_count(), 0);
    assert!(page.items().is_empty());
}

#[tokio::test]
async fn children_of_an_invisible_parent_cannot_be_browsed() {
    for (is_present, classification_state) in [(false, "Matched"), (true, "Candidate")] {
        let database = database().await;
        let user_id = seed_user(&database).await;
        let library = seed_library(&database, "TV", "tvshows", true).await;
        let parent = seed_item(
            &database,
            "Invisible series",
            "Series",
            None,
            is_present,
            classification_state,
        )
        .await;
        let child = seed_item(
            &database,
            "Season 1",
            "Season",
            Some(parent.as_uuid()),
            true,
            "Matched",
        )
        .await;
        add_to_library(&database, library, parent).await;
        add_to_library(&database, library, child).await;

        let page = CatalogQueryRepository::new(&database)
            .items(
                user_id,
                BrowseParent::Item(parent),
                CatalogPageRequest::new(0, 20).unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(page.total_record_count(), 0);
        assert!(page.items().is_empty());
    }
}

#[tokio::test]
async fn user_data_is_scoped_to_the_requesting_user_and_defaults_when_absent() {
    let database = database().await;
    let alice = seed_user(&database).await;
    let bob = AuthRepository::new(&database)
        .create_user(
            &Username::parse("bob").unwrap(),
            "$argon2id$test",
            true,
            false,
            Utc::now(),
        )
        .await
        .unwrap()
        .id();
    let library = seed_library(&database, "Movies", "movies", true).await;
    let item = seed_item(&database, "Arrival", "Movie", None, true, "Matched").await;
    add_to_library(&database, library, item).await;
    UserDataRepository::new(&database)
        .commit(
            bob,
            item,
            UserDataPatch::favorite(true)
                .with_playback_position_ticks(42_000)
                .with_played(true)
                .with_play_count_delta(3),
        )
        .await
        .unwrap();

    let repository = CatalogQueryRepository::new(&database);
    let alice_page = repository
        .items(
            alice,
            BrowseParent::Library(library),
            CatalogPageRequest::new(0, 20).unwrap(),
        )
        .await
        .unwrap();
    let bob_page = repository
        .items(
            bob,
            BrowseParent::Library(library),
            CatalogPageRequest::new(0, 20).unwrap(),
        )
        .await
        .unwrap();

    let alice_item = &alice_page.items()[0];
    assert!(!alice_item.is_favorite());
    assert!(!alice_item.is_played());
    assert_eq!(alice_item.play_count(), 0);
    assert_eq!(alice_item.playback_position_ticks(), 0);
    let bob_item = &bob_page.items()[0];
    assert!(bob_item.is_favorite());
    assert!(bob_item.is_played());
    assert_eq!(bob_item.play_count(), 3);
    assert_eq!(bob_item.playback_position_ticks(), 42_000);
}

#[tokio::test]
async fn parent_resolution_only_returns_visible_enabled_catalog_roots() {
    let database = database().await;
    let enabled_library = seed_library(&database, "Movies", "movies", true).await;
    let disabled_library = seed_library(&database, "Disabled", "movies", false).await;
    let visible_item = seed_item(&database, "Visible", "Series", None, true, "Matched").await;
    let hidden_item = seed_item(&database, "Hidden", "Series", None, false, "Matched").await;
    add_to_library(&database, enabled_library, visible_item).await;
    add_to_library(&database, enabled_library, hidden_item).await;
    let repository = CatalogQueryRepository::new(&database);

    assert_eq!(
        repository.resolve_parent(enabled_library).await.unwrap(),
        Some(BrowseParent::Library(enabled_library))
    );
    assert_eq!(
        repository
            .resolve_parent(visible_item.as_uuid())
            .await
            .unwrap(),
        Some(BrowseParent::Item(visible_item))
    );
    assert_eq!(
        repository.resolve_parent(disabled_library).await.unwrap(),
        None
    );
    assert_eq!(
        repository
            .resolve_parent(hidden_item.as_uuid())
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        repository.resolve_parent(Uuid::new_v4()).await.unwrap(),
        None
    );
}

#[tokio::test]
async fn item_type_filter_is_applied_before_count_and_pagination() {
    let database = database().await;
    let user_id = seed_user(&database).await;
    let library = seed_library(&database, "Mixed", "folders", true).await;
    let movie = seed_item(&database, "Arrival", "Movie", None, true, "Matched").await;
    let series = seed_item(&database, "Dark", "Series", None, true, "Matched").await;
    add_to_library(&database, library, movie).await;
    add_to_library(&database, library, series).await;

    let page = CatalogQueryRepository::new(&database)
        .items(
            user_id,
            BrowseParent::Library(library),
            CatalogPageRequest::new(0, 20)
                .unwrap()
                .with_item_types(vec![CatalogItemType::Movie]),
        )
        .await
        .unwrap();

    assert_eq!(page.total_record_count(), 1);
    assert_eq!(page.items()[0].name(), "Arrival");
}

#[test]
fn page_request_rejects_unbounded_or_overflowing_limits() {
    assert!(CatalogPageRequest::new(0, 0).is_err());
    assert!(CatalogPageRequest::new(0, 201).is_err());
}
