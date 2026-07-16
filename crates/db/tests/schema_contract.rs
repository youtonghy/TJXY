use std::collections::BTreeSet;

use sea_orm::{ConnectionTrait, Database, DbBackend, Statement};
use sea_orm_migration::MigratorTrait;
use tjxy_db::Migrator;

#[tokio::test]
async fn phase_zero_schema_contains_catalog_storage_cache_and_job_boundaries() {
    let database = Database::connect("sqlite::memory:").await.unwrap();
    Migrator::up(&database, None).await.unwrap();

    let rows = database
        .query_all(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT name FROM sqlite_master WHERE type = 'table'".to_owned(),
        ))
        .await
        .unwrap();
    let tables = rows
        .into_iter()
        .map(|row| row.try_get::<String>("", "name").unwrap())
        .collect::<BTreeSet<_>>();

    for required in [
        "catalog_state",
        "users",
        "user_catalog_state",
        "libraries",
        "catalog_items",
        "library_catalog_items",
        "media_sources",
        "media_source_aliases",
        "media_locations",
        "media_streams",
        "media_stream_index_map",
        "subtitles",
        "user_data",
        "storage_accounts",
        "storage_roots",
        "storage_objects",
        "storage_sync_cursors",
        "storage_change_outbox",
        "library_storage_roots",
        "asset_blobs",
        "item_assets",
        "work_jobs",
        "work_staging_rows",
        "work_results",
        "import_runs",
        "import_legacy_ids",
    ] {
        assert!(tables.contains(required), "missing table {required}");
    }
}

#[tokio::test]
async fn schema_enforces_stable_external_and_storage_identities() {
    let database = Database::connect("sqlite::memory:").await.unwrap();
    Migrator::up(&database, None).await.unwrap();

    for (table, expected_unique_columns) in [
        ("media_sources", vec!["catalog_item_id", "presentation_key"]),
        (
            "storage_objects",
            vec![
                "storage_account_id",
                "provider_drive_id",
                "provider_object_id",
            ],
        ),
        ("storage_change_outbox", vec!["dedupe_key"]),
        ("asset_blobs", vec!["sha256"]),
    ] {
        let sql =
            format!("SELECT sql FROM sqlite_master WHERE type = 'table' AND name = '{table}'");
        let row = database
            .query_one(Statement::from_string(DbBackend::Sqlite, sql))
            .await
            .unwrap()
            .unwrap();
        let ddl = row.try_get::<String>("", "sql").unwrap().to_lowercase();
        for column in expected_unique_columns {
            assert!(ddl.contains(column), "{table} DDL missing {column}: {ddl}");
        }
        assert!(
            ddl.contains("unique"),
            "{table} lacks a unique constraint: {ddl}"
        );
    }
}

#[tokio::test]
async fn schema_keeps_effective_policy_and_revisions_in_sql() {
    let database = Database::connect("sqlite::memory:").await.unwrap();
    Migrator::up(&database, None).await.unwrap();

    for (table, required_columns) in [
        (
            "libraries",
            vec![
                "scan_profile",
                "object_selection_scope",
                "metadata_policy",
                "expansion_policy",
                "probe_policy",
                "profile_version",
            ],
        ),
        ("user_catalog_state", vec!["user_id", "revision"]),
        (
            "storage_roots",
            vec!["sync_revision", "reconciled_sync_revision"],
        ),
        (
            "work_jobs",
            vec![
                "expected_revision",
                "input_sync_revision",
                "lease_owner",
                "lease_expires_at",
            ],
        ),
    ] {
        let rows = database
            .query_all(Statement::from_string(
                DbBackend::Sqlite,
                format!("PRAGMA table_info('{table}')"),
            ))
            .await
            .unwrap();
        let columns = rows
            .into_iter()
            .map(|row| row.try_get::<String>("", "name").unwrap())
            .collect::<BTreeSet<_>>();
        for required in required_columns {
            assert!(columns.contains(required), "{table} missing {required}");
        }
    }
}
