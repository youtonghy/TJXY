use sea_orm::{
    ConnectionTrait, DatabaseConnection,
    sea_query::{Alias, Expr, Query},
};
use sea_orm_migration::MigratorTrait;
use tjxy_common::{CatalogItemId, LibraryId, SortKey};
use tjxy_db::{
    HybridCandidateError, HybridCandidateRepository, LibraryPolicyUpdate, LibraryRepository,
};
use tjxy_test_support::test_database;
use uuid::Uuid;

async fn database() -> DatabaseConnection {
    let database = test_database().await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    database
}

async fn seed_library(database: &DatabaseConnection, expansion: &str) -> LibraryId {
    let library = LibraryId::new();
    database
        .execute(
            database.get_database_backend().build(
                Query::insert()
                    .into_table(Alias::new("libraries"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("name"),
                        Alias::new("collection_type"),
                        Alias::new("scan_profile"),
                        Alias::new("object_selection_scope"),
                        Alias::new("metadata_policy"),
                        Alias::new("expansion_policy"),
                        Alias::new("probe_policy"),
                        Alias::new("profile_version"),
                        Alias::new("is_enabled"),
                        Alias::new("sort_key"),
                    ])
                    .values_panic([
                        library.as_uuid().into(),
                        format!("Library {library}").into(),
                        "tvshows".into(),
                        "Hybrid".into(),
                        "title_layer".into(),
                        "basic".into(),
                        expansion.into(),
                        "on_playback".into(),
                        1_i32.into(),
                        true.into(),
                        SortKey::from_text(&library.to_string()).into_bytes().into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    library
}

async fn seed_item(
    database: &DatabaseConnection,
    library: LibraryId,
    item_type: &str,
) -> CatalogItemId {
    let item = CatalogItemId::new();
    database
        .execute(
            database.get_database_backend().build(
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
                        Alias::new("metadata_resolved_revision"),
                        Alias::new("structure_state"),
                        Alias::new("source_state"),
                        Alias::new("structure_expansion_revision"),
                        Alias::new("source_index_revision"),
                        Alias::new("is_present"),
                    ])
                    .values_panic([
                        item.as_uuid().into(),
                        item_type.into(),
                        "Pinned Series".into(),
                        "pinned series".into(),
                        SortKey::from_text("Pinned Series").into_bytes().into(),
                        "Matched".into(),
                        "Ready".into(),
                        0_i64.into(),
                        "NotExpanded".into(),
                        if item_type == "Series" {
                            "NotApplicable"
                        } else {
                            "NotIndexed"
                        }
                        .into(),
                        1_i64.into(),
                        0_i64.into(),
                        true.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            database.get_database_backend().build(
                Query::insert()
                    .into_table(Alias::new("library_catalog_items"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("library_id"),
                        Alias::new("catalog_item_id"),
                    ])
                    .values_panic([
                        Uuid::new_v4().into(),
                        library.as_uuid().into(),
                        item.as_uuid().into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    item
}

#[tokio::test]
async fn administrator_pin_is_library_scoped_idempotent_and_removable() {
    let database = database().await;
    let library = seed_library(&database, "background").await;
    let item = seed_item(&database, library, "Series").await;
    let repository = HybridCandidateRepository::new(&database);

    assert!(repository.pin(library, item).await.unwrap().changed());
    assert!(!repository.pin(library, item).await.unwrap().changed());

    let page = repository.selected(library, 0, 50).await.unwrap();
    assert_eq!(page.total_record_count(), 1);
    assert_eq!(page.start_index(), 0);
    assert_eq!(page.items().len(), 1);
    assert_eq!(page.items()[0].catalog_item_id(), item);
    assert_eq!(page.items()[0].name(), "Pinned Series");
    assert_eq!(page.items()[0].structure_state(), "NotExpanded");

    assert!(repository.unpin(library, item).await.unwrap().changed());
    assert!(!repository.unpin(library, item).await.unwrap().changed());
    assert!(
        repository
            .selected(library, 0, 50)
            .await
            .unwrap()
            .items()
            .is_empty()
    );
}

#[tokio::test]
async fn pin_requires_an_enabled_background_library_and_a_visible_matched_series() {
    let database = database().await;
    let background = seed_library(&database, "background").await;
    let on_browse = seed_library(&database, "on_browse").await;
    let series = seed_item(&database, background, "Series").await;
    let movie = seed_item(&database, background, "Movie").await;
    let absent_series = seed_item(&database, background, "Series").await;
    let repository = HybridCandidateRepository::new(&database);

    assert!(matches!(
        repository.pin(on_browse, series).await,
        Err(HybridCandidateError::LibraryNotBackground)
    ));
    assert!(matches!(
        repository.pin(background, movie).await,
        Err(HybridCandidateError::ItemUnavailable)
    ));
    assert!(matches!(
        repository.pin(background, CatalogItemId::new()).await,
        Err(HybridCandidateError::ItemUnavailable)
    ));
    database
        .execute(
            database.get_database_backend().build(
                Query::update()
                    .table(Alias::new("catalog_items"))
                    .value(Alias::new("is_present"), false)
                    .and_where(Expr::col(Alias::new("id")).eq(absent_series.as_uuid())),
            ),
        )
        .await
        .unwrap();
    assert!(matches!(
        repository.pin(background, absent_series).await,
        Err(HybridCandidateError::ItemUnavailable)
    ));

    database
        .execute(
            database.get_database_backend().build(
                Query::update()
                    .table(Alias::new("libraries"))
                    .value(Alias::new("is_enabled"), false)
                    .and_where(Expr::col(Alias::new("id")).eq(background.as_uuid())),
            ),
        )
        .await
        .unwrap();
    assert!(matches!(
        repository.pin(background, series).await,
        Err(HybridCandidateError::LibraryNotBackground)
    ));
    assert!(matches!(
        repository.selected(LibraryId::new(), 0, 50).await,
        Err(HybridCandidateError::LibraryUnavailable)
    ));
    assert!(matches!(
        repository.selected(background, 0, 0).await,
        Err(HybridCandidateError::InvalidPage)
    ));
}

#[tokio::test]
async fn pin_stays_dormant_across_policy_changes_and_does_not_block_library_deletion() {
    let database = database().await;
    let library = seed_library(&database, "background").await;
    let item = seed_item(&database, library, "Series").await;
    let candidates = HybridCandidateRepository::new(&database);

    candidates.pin(library, item).await.unwrap();
    let lazy = LibraryPolicyUpdate::new(
        "Lazy",
        "title_layer",
        "basic",
        "on_browse",
        "on_playback",
        true,
    )
    .unwrap();
    LibraryRepository::new(&database)
        .update_policy(library, 1, &lazy)
        .await
        .unwrap();

    let dormant = candidates.selected(library, 0, 50).await.unwrap();
    assert_eq!(dormant.items().len(), 1);
    assert!(matches!(
        candidates.pin(library, item).await,
        Err(HybridCandidateError::LibraryNotBackground)
    ));

    LibraryRepository::new(&database)
        .delete_by_name(&format!("Library {library}"))
        .await
        .unwrap();
    assert!(matches!(
        candidates.selected(library, 0, 50).await,
        Err(HybridCandidateError::LibraryUnavailable)
    ));
}
