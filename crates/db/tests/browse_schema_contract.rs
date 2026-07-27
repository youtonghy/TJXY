use sea_orm_migration::{MigratorTrait, SchemaManager};
use tjxy_test_support::test_database;

async fn database() -> sea_orm::DatabaseConnection {
    let database = test_database().await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    database
}

#[tokio::test]
async fn browse_schema_persists_library_sorting_and_session_capabilities() {
    let database = database().await;
    let schema = SchemaManager::new(&database);

    for column in ["collection_type", "sort_key", "is_enabled"] {
        assert!(schema.has_column("libraries", column).await.unwrap());
    }
    assert!(
        schema
            .has_column("catalog_items", "sort_key")
            .await
            .unwrap()
    );
    for column in [
        "playable_media_types",
        "supported_commands",
        "supports_media_control",
        "supports_persistent_identifier",
        "device_profile",
        "app_store_url",
        "icon_url",
    ] {
        assert!(schema.has_column("auth_sessions", column).await.unwrap());
    }
}

#[tokio::test]
async fn browse_schema_has_membership_and_stable_page_indexes() {
    let database = database().await;
    let schema = SchemaManager::new(&database);

    for (table, index) in [
        ("libraries", "idx_libraries_browse"),
        ("catalog_items", "idx_catalog_items_parent_browse"),
        ("catalog_items", "idx_catalog_items_type_browse"),
        ("library_catalog_items", "idx_library_catalog_items_reverse"),
        ("media_sources", "idx_media_sources_item_probe"),
        ("media_locations", "idx_media_locations_source_availability"),
    ] {
        assert!(
            schema.has_index(table, index).await.unwrap(),
            "missing index {index}"
        );
    }
}
