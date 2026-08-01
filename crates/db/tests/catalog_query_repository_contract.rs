use chrono::Utc;
use sea_orm::{
    ConnectionTrait, DatabaseConnection,
    sea_query::{Alias, Expr, Query},
};
use sea_orm_migration::MigratorTrait;
use tjxy_common::{CatalogItemId, ImageType, UserId, Username};
use tjxy_db::{
    AuthRepository, BrowseParent, CatalogItemType, CatalogPageRequest, CatalogQueryRepository,
};
use tjxy_db::{UserDataPatch, UserDataRepository};
use tjxy_test_support::test_database;
use uuid::Uuid;

async fn database() -> DatabaseConnection {
    let database = test_database().await.unwrap();
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

async fn set_rich_item_fields(
    database: &DatabaseConnection,
    item_id: CatalogItemId,
    index_number: Option<i32>,
) {
    let backend = database.get_database_backend();
    database
        .execute(
            backend.build(
                &Query::update()
                    .table(Alias::new("catalog_items"))
                    .values([
                        (Alias::new("tagline"), "Every second counts".into()),
                        (Alias::new("community_rating"), 8.7_f64.into()),
                        (Alias::new("vote_count"), 7_000_i64.into()),
                        (Alias::new("runtime_ticks"), 36_000_000_000_i64.into()),
                        (Alias::new("release_status"), "Ended".into()),
                        (Alias::new("official_rating"), "TV-MA".into()),
                        (Alias::new("original_language"), "en".into()),
                        (Alias::new("index_number"), index_number.into()),
                    ])
                    .and_where(Expr::col(Alias::new("id")).eq(item_id.as_uuid()))
                    .to_owned(),
            ),
        )
        .await
        .unwrap();
}

#[allow(clippy::too_many_lines)]
async fn seed_rich_detail_associations(database: &DatabaseConnection, item_id: CatalogItemId) {
    let backend = database.get_database_backend();
    let genre = Uuid::new_v4();
    let country = Uuid::new_v4();
    let language = Uuid::new_v4();
    let director = Uuid::new_v4();
    let actor = Uuid::new_v4();
    for statement in [
        Query::insert()
            .into_table(Alias::new("genres"))
            .columns([Alias::new("id"), Alias::new("name")])
            .values_panic([genre.into(), "Drama".into()])
            .to_owned(),
        Query::insert()
            .into_table(Alias::new("item_genres"))
            .columns([
                Alias::new("id"),
                Alias::new("catalog_item_id"),
                Alias::new("genre_id"),
            ])
            .values_panic([
                Uuid::new_v4().into(),
                item_id.as_uuid().into(),
                genre.into(),
            ])
            .to_owned(),
        Query::insert()
            .into_table(Alias::new("countries"))
            .columns([Alias::new("id"), Alias::new("code"), Alias::new("name")])
            .values_panic([country.into(), "US".into(), "United States".into()])
            .to_owned(),
        Query::insert()
            .into_table(Alias::new("item_countries"))
            .columns([
                Alias::new("id"),
                Alias::new("catalog_item_id"),
                Alias::new("country_id"),
                Alias::new("sort_order"),
            ])
            .values_panic([
                Uuid::new_v4().into(),
                item_id.as_uuid().into(),
                country.into(),
                0_i32.into(),
            ])
            .to_owned(),
        Query::insert()
            .into_table(Alias::new("languages"))
            .columns([Alias::new("id"), Alias::new("code"), Alias::new("name")])
            .values_panic([language.into(), "en".into(), "English".into()])
            .to_owned(),
        Query::insert()
            .into_table(Alias::new("item_languages"))
            .columns([
                Alias::new("id"),
                Alias::new("catalog_item_id"),
                Alias::new("language_id"),
                Alias::new("sort_order"),
            ])
            .values_panic([
                Uuid::new_v4().into(),
                item_id.as_uuid().into(),
                language.into(),
                0_i32.into(),
            ])
            .to_owned(),
        Query::insert()
            .into_table(Alias::new("people"))
            .columns([
                Alias::new("id"),
                Alias::new("name"),
                Alias::new("sort_name"),
            ])
            .values_panic([director.into(), "Director First".into(), "director".into()])
            .to_owned(),
        Query::insert()
            .into_table(Alias::new("people"))
            .columns([
                Alias::new("id"),
                Alias::new("name"),
                Alias::new("sort_name"),
            ])
            .values_panic([actor.into(), "Actor Second".into(), "actor".into()])
            .to_owned(),
        Query::insert()
            .into_table(Alias::new("item_people"))
            .columns([
                Alias::new("id"),
                Alias::new("catalog_item_id"),
                Alias::new("person_id"),
                Alias::new("role"),
                Alias::new("sort_order"),
                Alias::new("credit_type"),
            ])
            .values_panic([
                Uuid::new_v4().into(),
                item_id.as_uuid().into(),
                actor.into(),
                "Valery Legasov".into(),
                1_i32.into(),
                "Actor".into(),
            ])
            .to_owned(),
        Query::insert()
            .into_table(Alias::new("item_people"))
            .columns([
                Alias::new("id"),
                Alias::new("catalog_item_id"),
                Alias::new("person_id"),
                Alias::new("role"),
                Alias::new("sort_order"),
                Alias::new("credit_type"),
            ])
            .values_panic([
                Uuid::new_v4().into(),
                item_id.as_uuid().into(),
                director.into(),
                "Director".into(),
                0_i32.into(),
                "Crew".into(),
            ])
            .to_owned(),
    ] {
        database.execute(backend.build(&statement)).await.unwrap();
    }
}

async fn seed_asset(
    database: &DatabaseConnection,
    item_id: CatalogItemId,
    image_type: ImageType,
    priority: i32,
    sha256: &str,
    relative_path: &str,
) {
    let blob_id = Uuid::new_v4();
    let backend = database.get_database_backend();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("asset_blobs"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("sha256"),
                        Alias::new("mime_type"),
                        Alias::new("width"),
                        Alias::new("height"),
                        Alias::new("byte_size"),
                        Alias::new("local_relative_path"),
                    ])
                    .values_panic([
                        blob_id.into(),
                        sha256.into(),
                        "image/jpeg".into(),
                        300.into(),
                        450.into(),
                        4_i64.into(),
                        relative_path.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("item_assets"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("item_id"),
                        Alias::new("asset_blob_id"),
                        Alias::new("image_type"),
                        Alias::new("priority"),
                        Alias::new("source_provider"),
                    ])
                    .values_panic([
                        Uuid::new_v4().into(),
                        item_id.as_uuid().into(),
                        blob_id.into(),
                        image_type.as_str().into(),
                        priority.into(),
                        "fixture".into(),
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
async fn search_hints_filter_visible_items_before_type_filtering_and_paging() {
    let database = database().await;
    let user_id = seed_user(&database).await;
    let library = seed_library(&database, "Movies", "movies", true).await;
    let disabled = seed_library(&database, "Disabled", "movies", false).await;
    let alpha = seed_item(&database, "Alpha", "Movie", None, true, "Matched").await;
    let alpine = seed_item(&database, "Alpine", "Movie", None, true, "Matched").await;
    let audio = seed_item(&database, "Alpha Song", "Audio", None, true, "Matched").await;
    let absent = seed_item(&database, "Alpha Missing", "Movie", None, false, "Matched").await;
    let candidate = seed_item(
        &database,
        "Alpha Candidate",
        "Movie",
        None,
        true,
        "Candidate",
    )
    .await;
    let disabled_item =
        seed_item(&database, "Alpha Disabled", "Movie", None, true, "Matched").await;
    for item in [alpha, alpine, audio, absent, candidate] {
        add_to_library(&database, library, item).await;
    }
    add_to_library(&database, disabled, disabled_item).await;

    let page = CatalogQueryRepository::new(&database)
        .search_hints(
            user_id,
            "Alp",
            CatalogPageRequest::new(1, 1)
                .unwrap()
                .with_item_types(vec![CatalogItemType::Movie]),
        )
        .await
        .unwrap();

    assert_eq!(page.total_record_count(), 2);
    assert_eq!(page.start_index(), 1);
    assert_eq!(page.items().len(), 1);
    assert_eq!(page.items()[0].name(), "Alpine");
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
async fn rich_detail_loads_normalized_facts_and_children_use_numeric_index_order() {
    let database = database().await;
    let user_id = seed_user(&database).await;
    let library = seed_library(&database, "Television", "tvshows", true).await;
    let series = seed_item(&database, "Chernobyl", "Series", None, true, "Matched").await;
    let season_two = seed_item(
        &database,
        "Alpha",
        "Season",
        Some(series.as_uuid()),
        true,
        "Matched",
    )
    .await;
    let season_zero = seed_item(
        &database,
        "Zulu",
        "Season",
        Some(series.as_uuid()),
        true,
        "Matched",
    )
    .await;
    let season_one = seed_item(
        &database,
        "Middle",
        "Season",
        Some(series.as_uuid()),
        true,
        "Matched",
    )
    .await;
    for item in [series, season_two, season_zero, season_one] {
        add_to_library(&database, library, item).await;
    }
    set_rich_item_fields(&database, series, None).await;
    set_rich_item_fields(&database, season_two, Some(2)).await;
    set_rich_item_fields(&database, season_zero, Some(0)).await;
    set_rich_item_fields(&database, season_one, Some(1)).await;
    seed_rich_detail_associations(&database, series).await;

    let repository = CatalogQueryRepository::new(&database);
    let children = repository
        .items(
            user_id,
            BrowseParent::Item(series),
            CatalogPageRequest::new(0, 20).unwrap(),
        )
        .await
        .unwrap();
    let detail = repository
        .item_detail(user_id, series)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        children
            .items()
            .iter()
            .map(tjxy_db::CatalogItemRecord::id)
            .collect::<Vec<_>>(),
        [season_zero, season_one, season_two]
    );
    assert_eq!(detail.item().name(), "Chernobyl");
    assert_eq!(detail.tagline(), Some("Every second counts"));
    assert_eq!(detail.community_rating(), Some(8.7));
    assert_eq!(detail.vote_count(), Some(7_000));
    assert_eq!(detail.runtime_ticks(), Some(36_000_000_000));
    assert_eq!(detail.release_status(), Some("Ended"));
    assert_eq!(detail.official_rating(), Some("TV-MA"));
    assert_eq!(detail.original_language(), Some("en"));
    assert_eq!(detail.genres(), ["Drama"]);
    assert_eq!(detail.countries()[0].code(), "US");
    assert_eq!(detail.languages()[0].name(), "English");
    assert_eq!(detail.credits()[0].person_name(), "Director First");
    assert_eq!(detail.credits()[1].person_name(), "Actor Second");
    assert!(!detail.has_media_sources());
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

#[tokio::test]
async fn latest_items_are_date_ordered_and_library_scoped() {
    let database = database().await;
    let user = seed_user(&database).await;
    let first_library = seed_library(&database, "Movies", "movies", true).await;
    let second_library = seed_library(&database, "Other", "movies", true).await;
    let older = seed_item(&database, "Older", "Movie", None, true, "Matched").await;
    let newer = seed_item(&database, "Newer", "Movie", None, true, "Matched").await;
    let foreign = seed_item(&database, "Foreign", "Movie", None, true, "Matched").await;
    add_to_library(&database, first_library, older).await;
    add_to_library(&database, first_library, newer).await;
    add_to_library(&database, second_library, foreign).await;
    set_date_created(&database, older, Utc::now() - chrono::Duration::days(2)).await;
    set_date_created(&database, newer, Utc::now() - chrono::Duration::days(1)).await;
    set_date_created(&database, foreign, Utc::now()).await;

    let items = CatalogQueryRepository::new(&database)
        .latest_items(user, Some(first_library), &[], 20)
        .await
        .unwrap();

    assert_eq!(
        items
            .iter()
            .map(tjxy_db::CatalogItemRecord::name)
            .collect::<Vec<_>>(),
        ["Newer", "Older"]
    );
}

#[tokio::test]
async fn next_up_returns_the_first_unplayed_episode_per_series() {
    let database = database().await;
    let user = seed_user(&database).await;
    let library = seed_library(&database, "TV", "tvshows", true).await;
    let first_series = seed_item(&database, "First", "Series", None, true, "Matched").await;
    let second_series = seed_item(&database, "Second", "Series", None, true, "Matched").await;
    let first_episode = seed_item(
        &database,
        "S01E01",
        "Episode",
        Some(first_series.as_uuid()),
        true,
        "Matched",
    )
    .await;
    let next_episode = seed_item(
        &database,
        "S01E02",
        "Episode",
        Some(first_series.as_uuid()),
        true,
        "Matched",
    )
    .await;
    let other_episode = seed_item(
        &database,
        "S01E01 other",
        "Episode",
        Some(second_series.as_uuid()),
        true,
        "Matched",
    )
    .await;
    for item in [
        first_series,
        second_series,
        first_episode,
        next_episode,
        other_episode,
    ] {
        add_to_library(&database, library, item).await;
    }
    for (episode, owner) in [
        (first_episode, first_series),
        (next_episode, first_series),
        (other_episode, second_series),
    ] {
        set_structure_owner(&database, episode, owner).await;
    }
    UserDataRepository::new(&database)
        .commit(
            user,
            first_episode,
            UserDataPatch::default().with_played(true),
        )
        .await
        .unwrap();

    let page = CatalogQueryRepository::new(&database)
        .next_up_items(user, None, false, CatalogPageRequest::new(0, 20).unwrap())
        .await
        .unwrap();

    assert_eq!(page.total_record_count(), 2);
    let expected = if first_series.as_uuid() < second_series.as_uuid() {
        [next_episode, other_episode]
    } else {
        [other_episode, next_episode]
    };
    assert_eq!(
        page.items()
            .iter()
            .map(tjxy_db::CatalogItemRecord::id)
            .collect::<Vec<_>>(),
        expected
    );

    UserDataRepository::new(&database)
        .commit(
            user,
            next_episode,
            UserDataPatch::default().with_playback_position_ticks(42_000),
        )
        .await
        .unwrap();
    let repository = CatalogQueryRepository::new(&database);
    let without_resumable = repository
        .next_up_items(user, None, false, CatalogPageRequest::new(0, 20).unwrap())
        .await
        .unwrap();
    let with_resumable = repository
        .next_up_items(user, None, true, CatalogPageRequest::new(0, 20).unwrap())
        .await
        .unwrap();
    assert_eq!(without_resumable.items()[0].id(), other_episode);
    assert_eq!(without_resumable.total_record_count(), 1);
    assert_eq!(with_resumable.total_record_count(), 2);
}

#[tokio::test]
async fn item_pages_batch_primary_tags_and_resolve_authorized_images() {
    let database = database().await;
    let user_id = seed_user(&database).await;
    let library = seed_library(&database, "Movies", "movies", true).await;
    let item = seed_item(&database, "Arrival", "Movie", None, true, "Matched").await;
    add_to_library(&database, library, item).await;
    let sha256 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    seed_asset(
        &database,
        item,
        ImageType::Primary,
        0,
        sha256,
        "01/23/poster.jpg",
    )
    .await;

    let repository = CatalogQueryRepository::new(&database);
    let page = repository
        .items(
            user_id,
            BrowseParent::Library(library),
            CatalogPageRequest::new(0, 20).unwrap(),
        )
        .await
        .unwrap();
    let image = repository
        .image(item, ImageType::Primary, 0)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(page.items()[0].image_tags()["Primary"], sha256);
    assert_eq!(image.sha256(), sha256);
    assert_eq!(image.mime_type(), "image/jpeg");
    assert_eq!(image.byte_size(), 4);
    assert_eq!(image.local_relative_path(), "01/23/poster.jpg");
}

#[tokio::test]
async fn image_resolution_hides_items_without_enabled_visible_membership() {
    let database = database().await;
    let enabled = seed_library(&database, "Movies", "movies", true).await;
    let disabled = seed_library(&database, "Disabled", "movies", false).await;
    let visible = seed_item(&database, "Visible", "Movie", None, true, "Matched").await;
    let absent = seed_item(&database, "Absent", "Movie", None, false, "Matched").await;
    let candidate = seed_item(&database, "Candidate", "Movie", None, true, "Candidate").await;
    let disabled_only = seed_item(&database, "Disabled only", "Movie", None, true, "Matched").await;
    add_to_library(&database, enabled, visible).await;
    add_to_library(&database, enabled, absent).await;
    add_to_library(&database, enabled, candidate).await;
    add_to_library(&database, disabled, disabled_only).await;
    for (index, item) in [visible, absent, candidate, disabled_only]
        .into_iter()
        .enumerate()
    {
        let sha256 = format!("{index:064x}");
        seed_asset(
            &database,
            item,
            ImageType::Primary,
            0,
            &sha256,
            &format!("{index:02x}/poster.jpg"),
        )
        .await;
    }

    let repository = CatalogQueryRepository::new(&database);
    assert!(
        repository
            .image(visible, ImageType::Primary, 0)
            .await
            .unwrap()
            .is_some()
    );
    for item in [absent, candidate, disabled_only] {
        assert!(
            repository
                .image(item, ImageType::Primary, 0)
                .await
                .unwrap()
                .is_none()
        );
    }
}

#[test]
fn page_request_rejects_unbounded_or_overflowing_limits() {
    assert!(CatalogPageRequest::new(0, 0).is_err());
    assert!(CatalogPageRequest::new(0, 201).is_err());
}

async fn set_date_created(
    database: &DatabaseConnection,
    item: CatalogItemId,
    date_created: chrono::DateTime<Utc>,
) {
    let backend = database.get_database_backend();
    database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("catalog_items"))
                    .value(Alias::new("date_created"), date_created)
                    .and_where(sea_orm::sea_query::Expr::col(Alias::new("id")).eq(item.as_uuid())),
            ),
        )
        .await
        .unwrap();
}

async fn set_structure_owner(
    database: &DatabaseConnection,
    episode: CatalogItemId,
    series: CatalogItemId,
) {
    let backend = database.get_database_backend();
    database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("catalog_items"))
                    .value(Alias::new("structure_owner_item_id"), series.as_uuid())
                    .and_where(
                        sea_orm::sea_query::Expr::col(Alias::new("id")).eq(episode.as_uuid()),
                    ),
            ),
        )
        .await
        .unwrap();
}
