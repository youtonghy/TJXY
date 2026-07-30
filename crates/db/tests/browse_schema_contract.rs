use sea_orm::{
    ConnectionTrait, Value,
    sea_query::{Alias, Expr, Query},
};
use sea_orm_migration::{MigratorTrait, SchemaManager};
use tjxy_test_support::test_database;
use uuid::Uuid;

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

#[tokio::test]
async fn rich_catalog_schema_persists_details_and_ordered_associations() {
    let database = database().await;
    let schema = SchemaManager::new(&database);

    for column in [
        "tagline",
        "community_rating",
        "vote_count",
        "runtime_ticks",
        "premiere_date",
        "end_date",
        "release_status",
        "official_rating",
        "original_language",
        "index_number",
    ] {
        assert!(
            schema.has_column("catalog_items", column).await.unwrap(),
            "catalog_items missing {column}"
        );
    }

    for table in [
        "countries",
        "item_countries",
        "languages",
        "item_languages",
        "person_provider_ids",
        "person_assets",
        "metadata_snapshots",
    ] {
        assert!(
            schema.has_table(table).await.unwrap(),
            "missing table {table}"
        );
    }

    for (table, index) in [
        ("catalog_items", "idx_catalog_items_parent_index"),
        ("item_people", "idx_item_people_order"),
        ("person_provider_ids", "uq_person_provider_ids_identity"),
        ("metadata_snapshots", "uq_metadata_snapshots_identity"),
    ] {
        assert!(
            schema.has_index(table, index).await.unwrap(),
            "missing index {index}"
        );
    }
}

#[tokio::test]
async fn rich_catalog_schema_rejects_invalid_numeric_metadata() {
    let database = database().await;

    for (column, value) in [
        ("community_rating", Value::Double(Some(-0.1))),
        ("community_rating", Value::Double(Some(10.1))),
        ("vote_count", Value::BigInt(Some(-1))),
        ("runtime_ticks", Value::BigInt(Some(-1))),
        ("index_number", Value::Int(Some(-1))),
    ] {
        let backend = database.get_database_backend();
        let mut insert = Query::insert();
        insert
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
                Alias::new(column),
            ])
            .values_panic([
                Uuid::new_v4().into(),
                "Movie".into(),
                "Invalid metadata".into(),
                "invalid metadata".into(),
                b"invalid metadata".to_vec().into(),
                "Matched".into(),
                "Ready".into(),
                "NotApplicable".into(),
                "Missing".into(),
                0_i64.into(),
                0_i64.into(),
                true.into(),
                Expr::value(value),
            ]);

        assert!(
            database.execute(backend.build(&insert)).await.is_err(),
            "{column} accepted an invalid value"
        );
    }
}
