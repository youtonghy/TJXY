use std::collections::BTreeSet;

use sea_orm::{
    ConnectionTrait, Database, DbBackend, Statement,
    sea_query::{Alias, Query},
};
use sea_orm_migration::{MigratorTrait, SchemaManager};
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
        "auth_state",
        "auth_sessions",
        "api_keys",
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
            "users",
            vec![
                "username_key",
                "has_password",
                "auth_revision",
                "disabled_at",
                "created_at",
                "updated_at",
                "last_login_at",
                "last_activity_at",
            ],
        ),
        ("auth_state", vec!["id", "bootstrap_revision"]),
        (
            "auth_sessions",
            vec![
                "user_id",
                "token_digest",
                "auth_revision",
                "device_id",
                "device_name",
                "client_name",
                "client_version",
                "created_at",
                "expires_at",
                "last_seen_at",
                "revoked_at",
            ],
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
async fn api_key_schema_is_bounded_binary_and_restrictive() {
    let database = api_key_test_database().await;
    Migrator::up(&database, None).await.unwrap();
    let schema = SchemaManager::new(&database);
    for column in [
        "id",
        "envelope_id",
        "creator_user_id",
        "creator_auth_revision",
        "token_digest",
        "encrypted_payload",
        "key_version",
        "app_name",
        "created_at",
        "last_used_at",
    ] {
        assert!(schema.has_column("api_keys", column).await.unwrap());
    }
    for index in [
        "uq_api_keys_envelope_id",
        "uq_api_keys_token_digest",
        "ix_api_keys_creator",
    ] {
        assert!(schema.has_index("api_keys", index).await.unwrap());
    }

    let token_digest_type = column_type_name(&database, "api_keys", "token_digest").await;
    match database.get_database_backend() {
        DbBackend::MySql => assert_eq!(token_digest_type, "VARBINARY(32)"),
        DbBackend::Postgres => assert_eq!(token_digest_type, "bytea"),
        DbBackend::Sqlite => assert!(token_digest_type.to_ascii_uppercase().contains("BLOB")),
    }
    let creator_fk = api_key_foreign_keys(&database)
        .await
        .into_iter()
        .find(|foreign_key| {
            foreign_key.source_column == "creator_user_id"
                && foreign_key.target_table == "users"
                && foreign_key.target_column == "id"
        })
        .unwrap_or_else(|| panic!("missing creator FK creator_user_id -> users(id)"));
    assert!(
        creator_fk.delete_rule.eq_ignore_ascii_case("RESTRICT"),
        "api_keys creator FK must use RESTRICT, got {}",
        creator_fk.delete_rule
    );
}

async fn api_key_test_database() -> sea_orm::DatabaseConnection {
    let database_url =
        std::env::var("TJXY_TEST_DATABASE_URL").unwrap_or_else(|_| "sqlite::memory:".to_owned());
    Database::connect(database_url).await.unwrap()
}

async fn column_type_name(
    database: &sea_orm::DatabaseConnection,
    table: &str,
    column: &str,
) -> String {
    match database.get_database_backend() {
        DbBackend::Sqlite => database
            .query_all(Statement::from_string(
                DbBackend::Sqlite,
                format!("PRAGMA table_info('{table}')"),
            ))
            .await
            .unwrap()
            .into_iter()
            .find(|row| row.try_get::<String>("", "name").unwrap() == column)
            .unwrap()
            .try_get("", "type")
            .unwrap(),
        DbBackend::Postgres => database
            .query_one(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "SELECT data_type FROM information_schema.columns \
                 WHERE table_schema = current_schema() AND table_name = $1 AND column_name = $2"
                    .to_owned(),
                [table.into(), column.into()],
            ))
            .await
            .unwrap()
            .unwrap()
            .try_get("", "data_type")
            .unwrap(),
        DbBackend::MySql => {
            let row = database
                .query_one(Statement::from_sql_and_values(
                    DbBackend::MySql,
                    "SELECT data_type, character_maximum_length \
                     FROM information_schema.columns \
                     WHERE table_schema = DATABASE() AND table_name = ? AND column_name = ?"
                        .to_owned(),
                    [table.into(), column.into()],
                ))
                .await
                .unwrap()
                .unwrap();
            let data_type = row.try_get::<String>("", "data_type").unwrap();
            let maximum_length = row.try_get::<u64>("", "character_maximum_length").unwrap();
            format!("{}({maximum_length})", data_type.to_ascii_uppercase())
        }
    }
}

#[derive(Debug)]
struct ApiKeyForeignKey {
    source_column: String,
    target_table: String,
    target_column: String,
    delete_rule: String,
}

async fn api_key_foreign_keys(database: &sea_orm::DatabaseConnection) -> Vec<ApiKeyForeignKey> {
    let statement = match database.get_database_backend() {
        DbBackend::Sqlite => Statement::from_string(
            DbBackend::Sqlite,
            "PRAGMA foreign_key_list('api_keys')".to_owned(),
        ),
        DbBackend::Postgres => Statement::from_string(
            DbBackend::Postgres,
            "SELECT kcu.column_name AS source_column, \
                    ccu.table_name AS target_table, \
                    ccu.column_name AS target_column, \
                    rc.delete_rule AS delete_rule \
             FROM information_schema.referential_constraints rc \
             JOIN information_schema.table_constraints tc \
               ON tc.constraint_catalog = rc.constraint_catalog \
              AND tc.constraint_schema = rc.constraint_schema \
              AND tc.constraint_name = rc.constraint_name \
             JOIN information_schema.key_column_usage kcu \
               ON kcu.constraint_catalog = tc.constraint_catalog \
              AND kcu.constraint_schema = tc.constraint_schema \
              AND kcu.constraint_name = tc.constraint_name \
             JOIN information_schema.constraint_column_usage ccu \
               ON ccu.constraint_catalog = rc.unique_constraint_catalog \
              AND ccu.constraint_schema = rc.unique_constraint_schema \
              AND ccu.constraint_name = rc.unique_constraint_name \
             WHERE tc.table_schema = current_schema() \
               AND tc.table_name = 'api_keys'"
                .to_owned(),
        ),
        DbBackend::MySql => Statement::from_string(
            DbBackend::MySql,
            "SELECT kcu.column_name AS source_column, \
                    kcu.referenced_table_name AS target_table, \
                    kcu.referenced_column_name AS target_column, \
                    rc.delete_rule AS delete_rule \
             FROM information_schema.key_column_usage kcu \
             JOIN information_schema.referential_constraints rc \
               ON rc.constraint_schema = kcu.constraint_schema \
              AND rc.constraint_name = kcu.constraint_name \
             WHERE kcu.constraint_schema = DATABASE() \
               AND kcu.table_name = 'api_keys' \
               AND kcu.referenced_table_name IS NOT NULL"
                .to_owned(),
        ),
    };

    database
        .query_all(statement)
        .await
        .unwrap()
        .into_iter()
        .map(|row| ApiKeyForeignKey {
            source_column: row
                .try_get("", "from")
                .unwrap_or_else(|_| row.try_get("", "source_column").unwrap()),
            target_table: row
                .try_get("", "table")
                .unwrap_or_else(|_| row.try_get("", "target_table").unwrap()),
            target_column: row
                .try_get("", "to")
                .unwrap_or_else(|_| row.try_get("", "target_column").unwrap()),
            delete_rule: row
                .try_get("", "on_delete")
                .unwrap_or_else(|_| row.try_get("", "delete_rule").unwrap()),
        })
        .collect()
}

#[tokio::test]
async fn all_migrations_can_be_rolled_back() {
    let database = Database::connect("sqlite::memory:").await.unwrap();
    Migrator::up(&database, None).await.unwrap();

    Migrator::down(&database, None).await.unwrap();

    let api_keys = database
        .query_one(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'api_keys'".to_owned(),
        ))
        .await
        .unwrap();
    assert!(api_keys.is_none(), "table api_keys remains");

    let rows = database
        .query_all(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'seaql_%' AND name <> 'sqlite_sequence'"
                .to_owned(),
        ))
        .await
        .unwrap();
    let remaining_tables = rows
        .into_iter()
        .map(|row| row.try_get::<String>("", "name").unwrap())
        .collect::<Vec<_>>();
    assert!(
        remaining_tables.is_empty(),
        "tables remain after rollback: {remaining_tables:?}"
    );
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

    let auth_indexes = database
        .query_all(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT name FROM sqlite_master WHERE type = 'index' AND tbl_name IN ('users', 'auth_sessions')"
                .to_owned(),
        ))
        .await
        .unwrap()
        .into_iter()
        .map(|row| row.try_get::<String>("", "name").unwrap())
        .collect::<BTreeSet<_>>();
    for required in [
        "uq_users_username_key",
        "uq_auth_sessions_token_digest",
        "idx_auth_sessions_user_state",
        "idx_auth_sessions_expiry",
    ] {
        assert!(auth_indexes.contains(required), "missing index {required}");
    }
}

#[tokio::test]
async fn sqlite_auth_migration_backfills_portable_username_keys() {
    let database = Database::connect("sqlite::memory:").await.unwrap();
    Migrator::up(&database, Some(3)).await.unwrap();
    let user_id = uuid::Uuid::new_v4();
    let backend = database.get_database_backend();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("users"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("username"),
                        Alias::new("password_hash"),
                        Alias::new("is_admin"),
                    ])
                    .values_panic([
                        user_id.into(),
                        "Ａlice".into(),
                        "legacy-hash".into(),
                        false.into(),
                    ]),
            ),
        )
        .await
        .unwrap();

    Migrator::up(&database, None).await.unwrap();

    let row = database
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("username_key"))
                    .from(Alias::new("users"))
                    .and_where(sea_orm::sea_query::Expr::col(Alias::new("id")).eq(user_id)),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        row.try_get::<Vec<u8>>("", "username_key").unwrap(),
        b"alice"
    );
}
