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
        "provider_ids",
        "identity_matches",
        "metadata_provenance",
        "people",
        "item_people",
        "genres",
        "item_genres",
        "studios",
        "item_studios",
        "user_data",
        "storage_accounts",
        "storage_credentials",
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
        (
            "user_catalog_state",
            vec!["user_id", "revision", "updated_at"],
        ),
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
                "created_at",
                "started_at",
                "completed_at",
            ],
        ),
        (
            "storage_objects",
            vec!["mime_type", "etag", "remote_modified_at", "last_listed_at"],
        ),
        (
            "storage_sync_cursors",
            vec!["last_success_at", "last_full_sync_at"],
        ),
        ("storage_change_outbox", vec!["created_at", "processed_at"]),
        ("user_data", vec!["last_played_at", "updated_at"]),
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

#[tokio::test]
async fn all_phase_zero_migrations_can_be_rolled_back_on_sqlite() {
    let database = Database::connect("sqlite::memory:").await.unwrap();
    Migrator::up(&database, None).await.unwrap();

    Migrator::down(&database, None).await.unwrap();

    let rows = database
        .query_all(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'seaql_%'"
                .to_owned(),
        ))
        .await
        .unwrap();
    assert!(rows.is_empty());
}

#[tokio::test]
async fn durable_rows_are_not_cascade_deleted_and_active_jobs_are_single_flight() {
    let database = Database::connect("sqlite::memory:").await.unwrap();
    Migrator::up(&database, None).await.unwrap();

    for table in ["storage_change_outbox", "media_locations"] {
        let row = database
            .query_one(Statement::from_string(
                DbBackend::Sqlite,
                format!("SELECT sql FROM sqlite_master WHERE type = 'table' AND name = '{table}'"),
            ))
            .await
            .unwrap()
            .unwrap();
        let ddl = row.try_get::<String>("", "sql").unwrap().to_lowercase();
        assert!(
            !ddl.contains("on delete cascade"),
            "{table} must be reconciled explicitly: {ddl}"
        );
    }

    let row = database
        .query_one(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT sql FROM sqlite_master WHERE type = 'index' AND name = 'uq_work_jobs_active'"
                .to_owned(),
        ))
        .await
        .unwrap()
        .expect("active-job partial unique index must exist");
    let ddl = row.try_get::<String>("", "sql").unwrap().to_lowercase();
    assert!(ddl.contains("unique index"));
    assert!(ddl.contains("where"));
    assert!(ddl.contains("pending"));
    assert!(ddl.contains("running"));

    let outbox_indexes = database
        .query_all(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT name FROM sqlite_master WHERE type = 'index' AND tbl_name = 'storage_change_outbox'"
                .to_owned(),
        ))
        .await
        .unwrap()
        .into_iter()
        .map(|row| row.try_get::<String>("", "name").unwrap())
        .collect::<BTreeSet<_>>();
    assert!(outbox_indexes.contains("idx_outbox_root_claim"));
    assert!(outbox_indexes.contains("idx_outbox_root_revision_state"));
}
