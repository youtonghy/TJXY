use std::collections::BTreeSet;

use sea_orm::{ConnectionTrait, Database, DbBackend, QueryResult, Statement};
use sea_orm_migration::MigratorTrait;

async fn database() -> sea_orm::DatabaseConnection {
    let database = Database::connect("sqlite::memory:").await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    database
}

async fn columns(database: &sea_orm::DatabaseConnection, table: &str) -> BTreeSet<String> {
    database
        .query_all(Statement::from_string(
            DbBackend::Sqlite,
            format!("PRAGMA table_info('{table}')"),
        ))
        .await
        .unwrap()
        .iter()
        .map(|row: &QueryResult| row.try_get("", "name").unwrap())
        .collect()
}

#[tokio::test]
async fn browse_schema_persists_library_sorting_and_session_capabilities() {
    let database = database().await;

    assert!(
        columns(&database, "libraries")
            .await
            .is_superset(&BTreeSet::from([
                "collection_type".to_owned(),
                "sort_key".to_owned(),
                "is_enabled".to_owned(),
            ]))
    );
    assert!(
        columns(&database, "catalog_items")
            .await
            .contains("sort_key")
    );
    assert!(
        columns(&database, "auth_sessions")
            .await
            .is_superset(&BTreeSet::from([
                "playable_media_types".to_owned(),
                "supported_commands".to_owned(),
                "supports_media_control".to_owned(),
                "supports_persistent_identifier".to_owned(),
                "device_profile".to_owned(),
                "app_store_url".to_owned(),
                "icon_url".to_owned(),
            ]))
    );
}

#[tokio::test]
async fn browse_schema_has_membership_and_stable_page_indexes() {
    let database = database().await;
    let rows = database
        .query_all(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT name FROM sqlite_master WHERE type = 'index'".to_owned(),
        ))
        .await
        .unwrap();
    let indexes = rows
        .iter()
        .map(|row| row.try_get::<String>("", "name").unwrap())
        .collect::<BTreeSet<_>>();

    for required in [
        "idx_libraries_browse",
        "idx_catalog_items_parent_browse",
        "idx_catalog_items_type_browse",
        "idx_library_catalog_items_reverse",
        "idx_media_sources_item_probe",
        "idx_media_locations_source_availability",
    ] {
        assert!(indexes.contains(required), "missing index {required}");
    }
}
