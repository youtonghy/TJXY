use sea_orm::{
    ConnectionTrait, DbBackend, Statement,
    sea_query::{Alias, Expr, Index, Order, Query},
};
use sea_orm_migration::{MigratorTrait, SchemaManager};
use tjxy_common::Username;
use tjxy_db::{AuthRepository, Migrator, SchemaMigrationError, migrate_database};
use tjxy_test_support::test_database;

const AI_MESSAGE_SEQUENCE_MIGRATION_POSITION: u32 = 55;
const HYBRID_REMOVAL_MIGRATION_POSITION: u32 = 56;
const LEGACY_TITLE_YEAR_MIGRATION_POSITION: u32 = 57;
const SITE_THEME_SETTINGS_MIGRATION_POSITION: u32 = 58;
const MEDIA_NAME_PARSER_MIGRATION_POSITION: u32 = 59;

#[tokio::test]
async fn older_database_is_upgraded_by_the_shared_schema_entrypoint() {
    let database = test_database().await.unwrap();
    Migrator::up(&database, Some(AI_MESSAGE_SEQUENCE_MIGRATION_POSITION - 1))
        .await
        .unwrap();

    migrate_database(&database).await.unwrap();

    let schema = SchemaManager::new(&database);
    assert!(
        schema
            .has_column("ai_messages", "sequence_number")
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn work_claim_index_only_covers_active_jobs_where_supported() {
    let database = test_database().await.unwrap();
    Migrator::up(&database, None).await.unwrap();
    let schema = SchemaManager::new(&database);

    if database.get_database_backend() == DbBackend::MySql {
        assert!(
            schema
                .has_index("work_jobs", "ix_work_jobs_claim")
                .await
                .unwrap()
        );
        assert!(
            !schema
                .has_index("work_jobs", "ix_work_jobs_claim_active")
                .await
                .unwrap()
        );
        return;
    }

    assert!(
        schema
            .has_index("work_jobs", "ix_work_jobs_claim_active")
            .await
            .unwrap()
    );
    assert!(
        !schema
            .has_index("work_jobs", "ix_work_jobs_claim")
            .await
            .unwrap()
    );
    let definition = index_definition(&database, "ix_work_jobs_claim_active").await;
    assert!(
        definition.contains("where"),
        "missing active predicate: {definition}"
    );
    assert!(
        definition.contains("pending"),
        "missing pending state: {definition}"
    );
    assert!(
        definition.contains("running"),
        "missing running state: {definition}"
    );
}

#[tokio::test]
async fn site_theme_settings_are_added_by_their_migration() {
    let database = test_database().await.unwrap();
    Migrator::up(&database, Some(SITE_THEME_SETTINGS_MIGRATION_POSITION - 1))
        .await
        .unwrap();
    let schema = SchemaManager::new(&database);
    assert!(!schema.has_table("site_theme_settings").await.unwrap());

    Migrator::up(&database, Some(1)).await.unwrap();

    assert!(schema.has_table("site_theme_settings").await.unwrap());
    for column in [
        "id",
        "active_theme_id",
        "configurations",
        "revision",
        "created_at",
        "updated_at",
    ] {
        assert!(
            schema
                .has_column("site_theme_settings", column)
                .await
                .unwrap()
        );
    }
}

#[tokio::test]
async fn media_name_parser_schema_is_added_by_its_migration() {
    let database = test_database().await.unwrap();
    Migrator::up(&database, Some(MEDIA_NAME_PARSER_MIGRATION_POSITION - 1))
        .await
        .unwrap();
    let schema = SchemaManager::new(&database);
    assert!(
        !schema
            .has_column("catalog_items", "naming_parser_version")
            .await
            .unwrap()
    );

    Migrator::up(&database, Some(1)).await.unwrap();

    for (table, column) in [
        ("library_storage_roots", "naming_parser_version"),
        ("catalog_items", "naming_parser_version"),
        ("catalog_publications", "naming_parser_version"),
        ("publication_catalog_items", "index_number"),
        ("media_sources", "naming_hints"),
        ("publication_media_sources", "naming_hints"),
    ] {
        assert!(schema.has_column(table, column).await.unwrap());
    }
}

#[tokio::test]
async fn media_name_parser_schema_repairs_a_missing_additive_column() {
    let database = test_database().await.unwrap();
    migrate_database(&database).await.unwrap();
    database
        .execute(Statement::from_string(
            DbBackend::Sqlite,
            "ALTER TABLE media_sources DROP COLUMN naming_hints",
        ))
        .await
        .unwrap();
    let schema = SchemaManager::new(&database);
    assert!(
        !schema
            .has_column("media_sources", "naming_hints")
            .await
            .unwrap()
    );

    migrate_database(&database).await.unwrap();

    assert!(
        schema
            .has_column("media_sources", "naming_hints")
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn hybrid_libraries_are_migrated_to_lazy_without_losing_other_policy_values() {
    let database = test_database().await.unwrap();
    Migrator::up(&database, Some(HYBRID_REMOVAL_MIGRATION_POSITION - 1))
        .await
        .unwrap();
    let library = tjxy_common::LibraryId::new();
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
                        library.as_uuid().into(),
                        "Legacy Hybrid".into(),
                        "Hybrid".into(),
                        "title_layer".into(),
                        "full".into(),
                        "background".into(),
                        "on_playback".into(),
                        1_i32.into(),
                        "movies".into(),
                        b"legacy hybrid".to_vec().into(),
                        true.into(),
                    ]),
            ),
        )
        .await
        .unwrap();

    migrate_database(&database).await.unwrap();

    let row = database
        .query_one(
            backend.build(
                Query::select()
                    .columns([
                        Alias::new("scan_profile"),
                        Alias::new("metadata_policy"),
                        Alias::new("expansion_policy"),
                        Alias::new("profile_version"),
                    ])
                    .from(Alias::new("libraries"))
                    .and_where(Expr::col(Alias::new("id")).eq(library.as_uuid())),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.try_get::<String>("", "scan_profile").unwrap(), "Lazy");
    assert_eq!(
        row.try_get::<String>("", "metadata_policy").unwrap(),
        "full"
    );
    assert_eq!(
        row.try_get::<String>("", "expansion_policy").unwrap(),
        "on_browse"
    );
    assert_eq!(row.try_get::<i32>("", "profile_version").unwrap(), 2);
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Keeps legacy and protected rows in one migration transaction contract.
async fn legacy_title_years_are_split_without_overwriting_remote_metadata() {
    let database = test_database().await.unwrap();
    Migrator::up(&database, Some(LEGACY_TITLE_YEAR_MIGRATION_POSITION - 1))
        .await
        .unwrap();
    let backend = database.get_database_backend();
    let legacy = uuid::Uuid::new_v4();
    let protected = uuid::Uuid::new_v4();
    for (id, name) in [
        (legacy, "玩具总动员5(2026)"),
        (protected, "Remote Movie(2024)"),
    ] {
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
                            id.into(),
                            "Movie".into(),
                            name.into(),
                            name.to_lowercase().into(),
                            tjxy_common::SortKey::from_text(name).into_bytes().into(),
                            "Matched".into(),
                            "Partial".into(),
                            "NotApplicable".into(),
                            "NotIndexed".into(),
                            0_i64.into(),
                            0_i64.into(),
                            true.into(),
                        ]),
                ),
            )
            .await
            .unwrap();
    }
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("provider_ids"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("catalog_item_id"),
                        Alias::new("provider"),
                        Alias::new("provider_item_id"),
                    ])
                    .values_panic([
                        uuid::Uuid::new_v4().into(),
                        protected.into(),
                        "tmdb".into(),
                        "123".into(),
                    ]),
            ),
        )
        .await
        .unwrap();

    migrate_database(&database).await.unwrap();
    migrate_database(&database).await.unwrap();

    let legacy = database
        .query_one(
            backend.build(
                &Query::select()
                    .columns([
                        Alias::new("name"),
                        Alias::new("production_year"),
                        Alias::new("metadata_revision"),
                    ])
                    .from(Alias::new("catalog_items"))
                    .and_where(Expr::col(Alias::new("id")).eq(legacy))
                    .to_owned(),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(legacy.try_get::<String>("", "name").unwrap(), "玩具总动员5");
    assert_eq!(
        legacy
            .try_get::<Option<i32>>("", "production_year")
            .unwrap(),
        Some(2026)
    );
    assert_eq!(legacy.try_get::<i64>("", "metadata_revision").unwrap(), 1);

    let protected = database
        .query_one(
            backend.build(
                &Query::select()
                    .columns([Alias::new("name"), Alias::new("production_year")])
                    .from(Alias::new("catalog_items"))
                    .and_where(Expr::col(Alias::new("id")).eq(protected))
                    .to_owned(),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        protected.try_get::<String>("", "name").unwrap(),
        "Remote Movie(2024)"
    );
    assert_eq!(
        protected
            .try_get::<Option<i32>>("", "production_year")
            .unwrap(),
        None
    );
}

#[tokio::test]
async fn newer_database_is_rejected_without_mutating_migration_history() {
    let database = test_database().await.unwrap();
    migrate_database(&database).await.unwrap();
    let backend = database.get_database_backend();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("seaql_migrations"))
                    .columns([Alias::new("version"), Alias::new("applied_at")])
                    .values_panic(["m99999999_999999_future_schema".into(), 0_i64.into()]),
            ),
        )
        .await
        .unwrap();

    let error = migrate_database(&database).await.unwrap_err();
    assert!(matches!(
        error,
        SchemaMigrationError::DatabaseIsNewer { .. }
    ));
    let rows = database
        .query_all(
            backend.build(
                Query::select()
                    .column(Alias::new("version"))
                    .from(Alias::new("seaql_migrations")),
            ),
        )
        .await
        .unwrap();
    assert!(rows.iter().any(|row| {
        row.try_get::<String>("", "version").unwrap() == "m99999999_999999_future_schema"
    }));
}

#[tokio::test]
async fn applied_migration_with_missing_schema_object_is_reported_as_drift() {
    let database = test_database().await.unwrap();
    migrate_database(&database).await.unwrap();
    let manager = SchemaManager::new(&database);
    manager
        .drop_index(
            Index::drop()
                .name("uq_ai_messages_conversation_sequence")
                .table(Alias::new("ai_messages"))
                .to_owned(),
        )
        .await
        .unwrap();

    let error = migrate_database(&database).await.unwrap_err();
    assert!(matches!(error, SchemaMigrationError::SchemaDrift { .. }));
}

#[tokio::test]
async fn missing_site_theme_settings_table_is_reported_as_drift() {
    let database = test_database().await.unwrap();
    migrate_database(&database).await.unwrap();
    let manager = SchemaManager::new(&database);
    manager
        .drop_table(
            sea_orm::sea_query::Table::drop()
                .table(Alias::new("site_theme_settings"))
                .to_owned(),
        )
        .await
        .unwrap();

    let error = migrate_database(&database).await.unwrap_err();
    match error {
        SchemaMigrationError::SchemaDrift { missing } => {
            assert!(
                missing
                    .iter()
                    .any(|item| item == "table site_theme_settings")
            );
        }
        other => panic!("expected schema drift, got {other:?}"),
    }
}

#[test]
fn every_migration_file_is_registered_in_order() {
    let mut files = std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/src/migration"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter_map(|path| {
            let name = path.file_stem()?.to_str()?;
            name.strip_prefix('m')?
                .chars()
                .next()
                .is_some_and(|character| character.is_ascii_digit())
                .then(|| name.to_owned())
        })
        .collect::<Vec<_>>();
    files.sort();
    let registered = Migrator::migrations()
        .into_iter()
        .map(|migration| migration.name().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(registered, files);
}

#[tokio::test]
async fn ai_message_sequence_migration_backfills_existing_conversations() {
    let database = test_database().await.unwrap();
    Migrator::up(&database, Some(AI_MESSAGE_SEQUENCE_MIGRATION_POSITION - 1))
        .await
        .unwrap();
    let backend = database.get_database_backend();
    let auth = AuthRepository::new(&database);
    let user = auth
        .create_user(
            &Username::parse("sequence-migration").unwrap(),
            "test-only",
            false,
            false,
            chrono::Utc::now(),
        )
        .await
        .unwrap();
    let conversation_id = uuid::Uuid::new_v4();
    let timestamp = chrono::Utc::now();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("ai_conversations"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("user_id"),
                        Alias::new("model_id"),
                        Alias::new("title"),
                        Alias::new("created_at"),
                        Alias::new("updated_at"),
                    ])
                    .values_panic([
                        conversation_id.into(),
                        user.id().as_uuid().into(),
                        uuid::Uuid::new_v4().into(),
                        "Existing conversation".into(),
                        timestamp.into(),
                        timestamp.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    let first_id = uuid::Uuid::from_u128(1);
    let second_id = uuid::Uuid::from_u128(2);
    for (id, role) in [(second_id, "assistant"), (first_id, "user")] {
        database
            .execute(
                backend.build(
                    Query::insert()
                        .into_table(Alias::new("ai_messages"))
                        .columns([
                            Alias::new("id"),
                            Alias::new("conversation_id"),
                            Alias::new("role"),
                            Alias::new("content"),
                            Alias::new("metadata_json"),
                            Alias::new("created_at"),
                        ])
                        .values_panic([
                            id.into(),
                            conversation_id.into(),
                            role.into(),
                            role.into(),
                            "{}".into(),
                            timestamp.into(),
                        ]),
                ),
            )
            .await
            .unwrap();
    }

    Migrator::up(&database, Some(1)).await.unwrap();

    let rows = database
        .query_all(
            backend.build(
                Query::select()
                    .columns([Alias::new("id"), Alias::new("sequence_number")])
                    .from(Alias::new("ai_messages"))
                    .order_by(Alias::new("sequence_number"), Order::Asc),
            ),
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].try_get::<uuid::Uuid>("", "id").unwrap(), first_id);
    assert_eq!(rows[0].try_get::<i64>("", "sequence_number").unwrap(), 0);
    assert_eq!(rows[1].try_get::<uuid::Uuid>("", "id").unwrap(), second_id);
    assert_eq!(rows[1].try_get::<i64>("", "sequence_number").unwrap(), 1);
    let schema = SchemaManager::new(&database);
    assert!(
        schema
            .has_index("ai_messages", "uq_ai_messages_conversation_sequence")
            .await
            .unwrap()
    );
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // One contract enumerates the complete foundational schema boundary.
async fn phase_zero_schema_contains_catalog_storage_cache_and_job_boundaries() {
    let database = test_database().await.unwrap();
    Migrator::up(&database, None).await.unwrap();
    let schema = SchemaManager::new(&database);

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
        "storage_relink_candidates",
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
        "metadata_provider_settings",
        "playback_tickets",
        "playback_sessions",
        "storage_accounts",
        "storage_credentials",
        "storage_roots",
        "storage_objects",
        "storage_root_objects",
        "storage_sync_cursors",
        "storage_sync_pages",
        "storage_change_outbox",
        "library_storage_roots",
        "asset_blobs",
        "item_assets",
        "work_jobs",
        "work_staging_rows",
        "work_results",
        "catalog_publications",
        "publication_catalog_items",
        "publication_media_sources",
        "publication_media_locations",
        "publication_subtitles",
        "catalog_change_outbox",
        "cache_invalidation_outbox",
        "import_jobs",
        "import_sources",
        "import_staging_items",
        "legacy_item_mappings",
        "import_conflicts",
        "import_errors",
        "filesystem_storage_configs",
    ] {
        assert!(
            schema.has_table(required).await.unwrap(),
            "missing table {required}"
        );
    }
    assert!(
        schema
            .has_column("library_catalog_items", "hybrid_admin_selected_at")
            .await
            .unwrap()
    );
    assert!(
        schema
            .has_column("ai_models", "reasoning_effort")
            .await
            .unwrap()
    );
    for column in ["daily_total_token_limit", "daily_user_token_limit"] {
        assert!(
            schema
                .has_column("ai_provider_settings", column)
                .await
                .unwrap(),
            "ai_provider_settings missing {column}"
        );
    }
    assert!(schema.has_table("ai_execution_records").await.unwrap());
    for column in [
        "day_key",
        "elapsed_ms",
        "outcome",
        "prompt_tokens",
        "completion_tokens",
        "total_tokens",
    ] {
        assert!(
            schema
                .has_column("ai_execution_records", column)
                .await
                .unwrap(),
            "ai_execution_records missing {column}"
        );
    }
    assert!(schema.has_table("ai_daily_usage").await.unwrap());
    for column in [
        "id",
        "user_id",
        "day_key",
        "request_count",
        "created_at",
        "updated_at",
    ] {
        assert!(
            schema.has_column("ai_daily_usage", column).await.unwrap(),
            "ai_daily_usage missing {column}"
        );
    }
    for index in ["uq_ai_daily_usage_user_day", "ix_ai_daily_usage_user"] {
        assert!(
            schema.has_index("ai_daily_usage", index).await.unwrap(),
            "ai_daily_usage missing {index}"
        );
    }
    for table in ["announcements", "user_announcement_receipts"] {
        assert!(
            schema.has_table(table).await.unwrap(),
            "missing table {table}"
        );
    }
    assert!(schema.has_table("installation_records").await.unwrap());
    for column in [
        "id",
        "title",
        "body_markdown",
        "kind",
        "status",
        "content_version",
        "revision",
        "published_at",
        "created_at",
        "updated_at",
    ] {
        assert!(
            schema.has_column("announcements", column).await.unwrap(),
            "announcements missing {column}"
        );
    }
    for column in [
        "id",
        "announcement_id",
        "user_id",
        "acknowledged_version",
        "acknowledged_at",
    ] {
        assert!(
            schema
                .has_column("user_announcement_receipts", column)
                .await
                .unwrap(),
            "user_announcement_receipts missing {column}"
        );
    }
    assert!(
        schema
            .has_index("announcements", "ix_announcements_status_published")
            .await
            .unwrap()
    );
    for index in [
        "uq_announcement_receipt_pair",
        "ix_announcement_receipts_user",
    ] {
        assert!(
            schema
                .has_index("user_announcement_receipts", index)
                .await
                .unwrap(),
            "user_announcement_receipts missing {index}"
        );
    }
    let daily_usage_definitions = unique_definitions(&database, "ai_daily_usage").await;
    assert!(
        daily_usage_definitions.contains("unique")
            && daily_usage_definitions.contains("user_id")
            && daily_usage_definitions.contains("day_key"),
        "ai_daily_usage must uniquely identify a user-day: {daily_usage_definitions}"
    );
    let user_fk = foreign_keys(&database, "ai_daily_usage")
        .await
        .into_iter()
        .find(|foreign_key| {
            foreign_key.source_column == "user_id"
                && foreign_key.target_table == "users"
                && foreign_key.target_column == "id"
        })
        .unwrap_or_else(|| panic!("missing AI daily usage FK user_id -> users(id)"));
    assert!(
        user_fk.delete_rule.eq_ignore_ascii_case("CASCADE"),
        "ai_daily_usage user FK must use CASCADE, got {}",
        user_fk.delete_rule
    );
}

#[tokio::test]
async fn schema_enforces_stable_external_and_storage_identities() {
    let database = test_database().await.unwrap();
    Migrator::up(&database, None).await.unwrap();
    let schema = SchemaManager::new(&database);

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
        (
            "legacy_item_mappings",
            vec!["source_instance_id", "legacy_item_id"],
        ),
    ] {
        let definitions = unique_definitions(&database, table).await;
        assert!(definitions.contains("unique"), "{table} lacks uniqueness");
        for column in expected_unique_columns {
            assert!(
                schema.has_column(table, column).await.unwrap(),
                "{table} missing {column}"
            );
            assert!(
                definitions.contains(column),
                "{table} unique definitions omit {column}: {definitions}"
            );
        }
    }
}

async fn unique_definitions(database: &sea_orm::DatabaseConnection, table: &str) -> String {
    if database.get_database_backend() == DbBackend::MySql {
        return database
            .query_all(Statement::from_string(
                DbBackend::MySql,
                format!("SHOW CREATE TABLE `{table}`"),
            ))
            .await
            .unwrap()
            .into_iter()
            .map(|row| row.try_get_by_index::<String>(1).unwrap())
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();
    }
    let statement = match database.get_database_backend() {
        DbBackend::Sqlite => Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "SELECT sql FROM sqlite_master \
             WHERE (type = 'table' AND name = ?) OR (type = 'index' AND tbl_name = ?)"
                .to_owned(),
            [table.into(), table.into()],
        ),
        DbBackend::Postgres => Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT indexdef AS sql FROM pg_indexes \
             WHERE schemaname = current_schema() AND tablename = $1"
                .to_owned(),
            [table.into()],
        ),
        DbBackend::MySql => unreachable!(),
    };

    database
        .query_all(statement)
        .await
        .unwrap()
        .into_iter()
        .filter_map(|row| row.try_get::<Option<String>>("", "sql").unwrap())
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

async fn index_definition(database: &sea_orm::DatabaseConnection, index: &str) -> String {
    let statement = match database.get_database_backend() {
        DbBackend::Sqlite => Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "SELECT sql FROM sqlite_master WHERE type = 'index' AND name = ?".to_owned(),
            [index.into()],
        ),
        DbBackend::Postgres => Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT indexdef AS sql FROM pg_indexes \
             WHERE schemaname = current_schema() AND indexname = $1"
                .to_owned(),
            [index.into()],
        ),
        DbBackend::MySql => unreachable!("MySQL does not support partial indexes"),
    };
    database
        .query_one(statement)
        .await
        .unwrap()
        .unwrap()
        .try_get::<String>("", "sql")
        .unwrap()
        .to_lowercase()
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Keeps the cross-table durable revision contract in one matrix.
async fn schema_keeps_effective_policy_and_revisions_in_sql() {
    let database = test_database().await.unwrap();
    Migrator::up(&database, None).await.unwrap();
    let schema = SchemaManager::new(&database);

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
        ("catalog_items", vec!["date_created"]),
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
            "playback_tickets",
            vec![
                "id",
                "auth_session_id",
                "user_id",
                "item_id",
                "media_source_id",
                "play_session_id",
                "token_digest",
                "expires_at",
                "revoked_at",
                "created_at",
            ],
        ),
        (
            "storage_roots",
            vec!["sync_revision", "reconciled_sync_revision"],
        ),
        (
            "storage_sync_cursors",
            vec!["cursor_value", "status", "recovery_job_id"],
        ),
        (
            "work_jobs",
            vec![
                "expected_revision",
                "input_sync_revision",
                "lease_owner",
                "lease_expires_at",
                "available_at",
                "created_at",
                "started_at",
                "completed_at",
            ],
        ),
        (
            "storage_root_objects",
            vec![
                "storage_root_id",
                "storage_object_id",
                "parent_storage_object_id",
                "observed_sync_revision",
                "children_indexed",
                "children_index_revision",
                "presence_state",
            ],
        ),
        ("work_results", vec!["result_sync_revision"]),
        (
            "catalog_publications",
            vec![
                "job_id",
                "owner_catalog_item_id",
                "publication_kind",
                "expected_revision",
                "input_sync_revision",
                "state",
                "manifest_sha256",
                "expected_row_count",
                "activated_generation",
                "sealed_at",
            ],
        ),
        (
            "catalog_items",
            vec![
                "last_expanded_at",
                "structure_owner_item_id",
                "metadata_revision",
                "metadata_resolved_revision",
                "metadata_resolved_requirement",
                "metadata_payload_version",
            ],
        ),
        (
            "work_jobs",
            vec![
                "metadata_requirement",
                "storage_root_affinity",
                "natural_key_storage_root_id",
            ],
        ),
        (
            "publication_catalog_items",
            vec![
                "publication_id",
                "catalog_item_id",
                "parent_catalog_item_id",
                "storage_root_id",
                "scope_storage_object_id",
                "row_sha256",
            ],
        ),
        (
            "publication_media_sources",
            vec![
                "publication_id",
                "media_source_id",
                "catalog_item_id",
                "presentation_key",
                "row_sha256",
            ],
        ),
        (
            "publication_media_locations",
            vec![
                "publication_id",
                "media_location_id",
                "media_source_id",
                "storage_object_id",
                "row_sha256",
            ],
        ),
        (
            "publication_subtitles",
            vec![
                "publication_id",
                "subtitle_id",
                "media_source_id",
                "storage_object_id",
                "delivery_index",
                "row_sha256",
            ],
        ),
        (
            "media_sources",
            vec![
                "video_codec",
                "resolution",
                "bitrate",
                "runtime_ticks",
                "admin_priority",
                "is_default",
                "is_hidden",
            ],
        ),
        (
            "media_streams",
            vec![
                "stream_identity",
                "delivery_index",
                "container_stream_index",
                "width",
                "height",
                "channels",
                "profile",
                "level",
                "is_default",
                "is_forced",
                "is_external",
                "is_text",
            ],
        ),
        (
            "storage_objects",
            vec![
                "mime_type",
                "etag",
                "remote_modified_at",
                "last_listed_at",
                "facts_observed_storage_root_id",
            ],
        ),
        (
            "storage_sync_cursors",
            vec!["last_success_at", "last_full_sync_at"],
        ),
        ("storage_roots", vec!["discovered_sync_revision"]),
        ("library_storage_roots", vec!["discovered_sync_revision"]),
        ("storage_change_outbox", vec!["created_at", "processed_at"]),
        ("user_data", vec!["last_played_at", "updated_at"]),
        (
            "storage_relink_candidates",
            vec![
                "storage_root_id",
                "previous_storage_object_id",
                "replacement_storage_object_id",
                "confidence",
                "evidence",
                "state",
                "created_at",
            ],
        ),
    ] {
        for required in required_columns {
            assert!(
                schema.has_column(table, required).await.unwrap(),
                "{table} missing {required}"
            );
        }
    }
    database
        .query_all(
            database.get_database_backend().build(
                &Query::select()
                    .columns([Alias::new("profile"), Alias::new("level")])
                    .from(Alias::new("media_streams"))
                    .limit(0)
                    .to_owned(),
            ),
        )
        .await
        .expect("media_streams compatibility columns must be queryable");
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // The pre-m30 fixture proves both recoverable and ambiguous upgrade paths.
async fn storage_work_root_key_migration_recovers_or_retires_legacy_jobs() {
    let database = test_database().await.unwrap();
    Migrator::up(&database, Some(29)).await.unwrap();
    let backend = database.get_database_backend();
    let account = uuid::Uuid::new_v4();
    let first_root = uuid::Uuid::new_v4();
    let second_root = uuid::Uuid::new_v4();
    let unique_scope = uuid::Uuid::new_v4();
    let ambiguous_scope = uuid::Uuid::new_v4();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("storage_accounts"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("provider"),
                        Alias::new("display_name"),
                        Alias::new("account_identity"),
                        Alias::new("credential_ref"),
                        Alias::new("status"),
                    ])
                    .values_panic([
                        account.into(),
                        "filesystem".into(),
                        "Fixture".into(),
                        "fixture-account".into(),
                        "fixture".into(),
                        "Active".into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    for (root, provider_root) in [(first_root, "first"), (second_root, "second")] {
        database
            .execute(
                backend.build(
                    Query::insert()
                        .into_table(Alias::new("storage_roots"))
                        .columns([
                            Alias::new("id"),
                            Alias::new("storage_account_id"),
                            Alias::new("provider_root_id"),
                            Alias::new("sync_revision"),
                            Alias::new("reconciled_sync_revision"),
                        ])
                        .values_panic([
                            root.into(),
                            account.into(),
                            provider_root.into(),
                            0_i64.into(),
                            0_i64.into(),
                        ]),
                ),
            )
            .await
            .unwrap();
    }
    for (object, provider_id) in [(unique_scope, "unique"), (ambiguous_scope, "ambiguous")] {
        database
            .execute(
                backend.build(
                    Query::insert()
                        .into_table(Alias::new("storage_objects"))
                        .columns([
                            Alias::new("id"),
                            Alias::new("storage_account_id"),
                            Alias::new("provider_drive_id"),
                            Alias::new("provider_object_id"),
                            Alias::new("name"),
                            Alias::new("normalized_name"),
                            Alias::new("object_type"),
                            Alias::new("observed_sync_revision"),
                            Alias::new("children_indexed"),
                            Alias::new("children_index_revision"),
                            Alias::new("identity_quality"),
                            Alias::new("presence_state"),
                        ])
                        .values_panic([
                            object.into(),
                            account.into(),
                            "drive".into(),
                            provider_id.into(),
                            provider_id.into(),
                            provider_id.into(),
                            "Directory".into(),
                            0_i64.into(),
                            false.into(),
                            0_i64.into(),
                            "ProviderStableId".into(),
                            "Present".into(),
                        ]),
                ),
            )
            .await
            .unwrap();
    }
    for (root, object) in [
        (first_root, unique_scope),
        (first_root, ambiguous_scope),
        (second_root, ambiguous_scope),
    ] {
        database
            .execute(
                backend.build(
                    Query::insert()
                        .into_table(Alias::new("storage_root_objects"))
                        .columns([
                            Alias::new("id"),
                            Alias::new("storage_root_id"),
                            Alias::new("storage_object_id"),
                            Alias::new("observed_sync_revision"),
                            Alias::new("children_indexed"),
                            Alias::new("children_index_revision"),
                            Alias::new("presence_state"),
                        ])
                        .values_panic([
                            uuid::Uuid::new_v4().into(),
                            root.into(),
                            object.into(),
                            0_i64.into(),
                            false.into(),
                            0_i64.into(),
                            "Present".into(),
                        ]),
                ),
            )
            .await
            .unwrap();
    }
    for (id, scope, revision) in [
        (uuid::Uuid::new_v4(), unique_scope, 1_i64),
        (uuid::Uuid::new_v4(), ambiguous_scope, 2_i64),
    ] {
        database
            .execute(
                backend.build(
                    Query::insert()
                        .into_table(Alias::new("work_jobs"))
                        .columns([
                            Alias::new("id"),
                            Alias::new("task_kind"),
                            Alias::new("scope_type"),
                            Alias::new("scope_id"),
                            Alias::new("expected_revision"),
                            Alias::new("state"),
                            Alias::new("priority"),
                            Alias::new("attempt_count"),
                        ])
                        .values_panic([
                            id.into(),
                            "ScopedStorageSync".into(),
                            "StorageObject".into(),
                            scope.into(),
                            revision.into(),
                            "Pending".into(),
                            100_i32.into(),
                            0_i32.into(),
                        ]),
                ),
            )
            .await
            .unwrap();
    }

    Migrator::up(&database, Some(1)).await.unwrap();
    let rows = database
        .query_all(
            backend.build(
                Query::select()
                    .columns([
                        Alias::new("scope_id"),
                        Alias::new("state"),
                        Alias::new("storage_root_affinity"),
                        Alias::new("natural_key_storage_root_id"),
                    ])
                    .from(Alias::new("work_jobs"))
                    .order_by(
                        Alias::new("expected_revision"),
                        sea_orm::sea_query::Order::Asc,
                    ),
            ),
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].try_get::<String>("", "state").unwrap(), "Pending");
    assert_eq!(
        rows[0]
            .try_get::<uuid::Uuid>("", "storage_root_affinity")
            .unwrap(),
        first_root
    );
    assert_eq!(
        rows[0]
            .try_get::<uuid::Uuid>("", "natural_key_storage_root_id")
            .unwrap(),
        first_root
    );
    assert_eq!(rows[1].try_get::<String>("", "state").unwrap(), "Failed");
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // The pre-m31 graph proves deterministic backfill and ambiguous fail-closed recovery.
async fn storage_fact_origin_migration_backfills_unique_and_invalidates_ambiguous_facts() {
    let database = test_database().await.unwrap();
    Migrator::up(&database, Some(30)).await.unwrap();
    let backend = database.get_database_backend();
    let account = uuid::Uuid::new_v4();
    let first_root = uuid::Uuid::new_v4();
    let second_root = uuid::Uuid::new_v4();
    let parent = uuid::Uuid::new_v4();
    let unique = uuid::Uuid::new_v4();
    let ambiguous = uuid::Uuid::new_v4();
    let item = uuid::Uuid::new_v4();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("storage_accounts"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("provider"),
                        Alias::new("display_name"),
                        Alias::new("account_identity"),
                        Alias::new("credential_ref"),
                        Alias::new("status"),
                    ])
                    .values_panic([
                        account.into(),
                        "filesystem".into(),
                        "Fixture".into(),
                        "fact-origin-fixture".into(),
                        "fixture".into(),
                        "Active".into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    for (root, provider_root) in [(first_root, "first"), (second_root, "second")] {
        database
            .execute(
                backend.build(
                    Query::insert()
                        .into_table(Alias::new("storage_roots"))
                        .columns([
                            Alias::new("id"),
                            Alias::new("storage_account_id"),
                            Alias::new("provider_root_id"),
                            Alias::new("sync_revision"),
                            Alias::new("reconciled_sync_revision"),
                        ])
                        .values_panic([
                            root.into(),
                            account.into(),
                            provider_root.into(),
                            1_i64.into(),
                            1_i64.into(),
                        ]),
                ),
            )
            .await
            .unwrap();
    }
    for (object, provider_id, object_type) in [
        (parent, "parent", "Directory"),
        (unique, "unique", "File"),
        (ambiguous, "ambiguous", "File"),
    ] {
        database
            .execute(
                backend.build(
                    Query::insert()
                        .into_table(Alias::new("storage_objects"))
                        .columns([
                            Alias::new("id"),
                            Alias::new("storage_account_id"),
                            Alias::new("provider_drive_id"),
                            Alias::new("provider_object_id"),
                            Alias::new("name"),
                            Alias::new("normalized_name"),
                            Alias::new("object_type"),
                            Alias::new("observed_sync_revision"),
                            Alias::new("children_indexed"),
                            Alias::new("children_index_revision"),
                            Alias::new("identity_quality"),
                            Alias::new("presence_state"),
                        ])
                        .values_panic([
                            object.into(),
                            account.into(),
                            "drive".into(),
                            provider_id.into(),
                            provider_id.into(),
                            provider_id.into(),
                            object_type.into(),
                            1_i64.into(),
                            (object == parent).into(),
                            1_i64.into(),
                            "ProviderStableId".into(),
                            "Present".into(),
                        ]),
                ),
            )
            .await
            .unwrap();
    }
    for (root, object, parent_id) in [
        (first_root, parent, None),
        (first_root, unique, Some(parent)),
        (first_root, ambiguous, Some(parent)),
        (second_root, ambiguous, None),
    ] {
        database
            .execute(
                backend.build(
                    Query::insert()
                        .into_table(Alias::new("storage_root_objects"))
                        .columns([
                            Alias::new("id"),
                            Alias::new("storage_root_id"),
                            Alias::new("storage_object_id"),
                            Alias::new("parent_storage_object_id"),
                            Alias::new("observed_sync_revision"),
                            Alias::new("children_indexed"),
                            Alias::new("children_index_revision"),
                            Alias::new("presence_state"),
                        ])
                        .values_panic([
                            uuid::Uuid::new_v4().into(),
                            root.into(),
                            object.into(),
                            parent_id.into(),
                            1_i64.into(),
                            (object == parent).into(),
                            1_i64.into(),
                            "Present".into(),
                        ]),
                ),
            )
            .await
            .unwrap();
    }
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
                        Alias::new("classification_state"),
                        Alias::new("metadata_state"),
                        Alias::new("structure_state"),
                        Alias::new("source_state"),
                        Alias::new("structure_expansion_revision"),
                        Alias::new("source_index_revision"),
                        Alias::new("is_present"),
                    ])
                    .values_panic([
                        item.into(),
                        "Movie".into(),
                        "Movie".into(),
                        "movie".into(),
                        "Matched".into(),
                        "Ready".into(),
                        "NotApplicable".into(),
                        "Indexed".into(),
                        0_i64.into(),
                        3_i64.into(),
                        true.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("identity_matches"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("storage_object_id"),
                        Alias::new("candidate_catalog_item_id"),
                        Alias::new("confidence"),
                        Alias::new("state"),
                        Alias::new("evidence"),
                    ])
                    .values_panic([
                        uuid::Uuid::new_v4().into(),
                        parent.into(),
                        item.into(),
                        1.0.into(),
                        "Matched".into(),
                        serde_json::json!({}).into(),
                    ]),
            ),
        )
        .await
        .unwrap();

    Migrator::up(&database, Some(1)).await.unwrap();

    let unique_origin = database
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("facts_observed_storage_root_id"))
                    .from(Alias::new("storage_objects"))
                    .and_where(Expr::col(Alias::new("id")).eq(unique)),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        unique_origin
            .try_get::<uuid::Uuid>("", "facts_observed_storage_root_id")
            .unwrap(),
        first_root
    );
    let ambiguous_relations = database
        .query_all(
            backend.build(
                Query::select()
                    .columns([
                        Alias::new("presence_state"),
                        Alias::new("availability_reason"),
                        Alias::new("children_indexed"),
                    ])
                    .from(Alias::new("storage_root_objects"))
                    .and_where(Expr::col(Alias::new("storage_object_id")).eq(ambiguous)),
            ),
        )
        .await
        .unwrap();
    assert_eq!(ambiguous_relations.len(), 2);
    for row in ambiguous_relations {
        assert_eq!(
            row.try_get::<String>("", "presence_state").unwrap(),
            "TemporarilyUnavailable"
        );
        assert_eq!(
            row.try_get::<String>("", "availability_reason").unwrap(),
            "facts-origin-migration-required"
        );
        assert!(!row.try_get::<bool>("", "children_indexed").unwrap());
    }
    let ambiguous_origin = database
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("facts_observed_storage_root_id"))
                    .from(Alias::new("storage_objects"))
                    .and_where(Expr::col(Alias::new("id")).eq(ambiguous)),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert!(
        ambiguous_origin
            .try_get::<Option<uuid::Uuid>>("", "facts_observed_storage_root_id")
            .unwrap()
            .is_none()
    );
    let parent_relation = database
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("children_indexed"))
                    .from(Alias::new("storage_root_objects"))
                    .and_where(Expr::col(Alias::new("storage_root_id")).eq(first_root))
                    .and_where(Expr::col(Alias::new("storage_object_id")).eq(parent)),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert!(
        !parent_relation
            .try_get::<bool>("", "children_indexed")
            .unwrap()
    );
    let item_row = database
        .query_one(
            backend.build(
                Query::select()
                    .columns([
                        Alias::new("source_index_revision"),
                        Alias::new("source_state"),
                    ])
                    .from(Alias::new("catalog_items"))
                    .and_where(Expr::col(Alias::new("id")).eq(item)),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        item_row
            .try_get::<i64>("", "source_index_revision")
            .unwrap(),
        4
    );
    assert_eq!(
        item_row.try_get::<String>("", "source_state").unwrap(),
        "NotIndexed"
    );
}

#[tokio::test]
async fn schema_enforces_probe_delivery_identity_boundaries() {
    let database = test_database().await.unwrap();
    Migrator::up(&database, None).await.unwrap();
    let schema = SchemaManager::new(&database);

    for (table, index, required_columns) in [
        (
            "media_streams",
            "uq_media_streams_identity",
            vec!["media_source_id", "stream_identity"],
        ),
        (
            "subtitles",
            "uq_subtitles_source_object",
            vec!["media_source_id", "storage_object_id"],
        ),
        (
            "catalog_change_outbox",
            "uq_catalog_change_outbox_generation",
            vec!["generation"],
        ),
    ] {
        assert!(schema.has_index(table, index).await.unwrap());
        for column in required_columns {
            assert!(schema.has_column(table, column).await.unwrap());
        }
    }
}

#[tokio::test]
async fn schema_indexes_latest_catalog_order() {
    let database = test_database().await.unwrap();
    Migrator::up(&database, None).await.unwrap();
    let schema = SchemaManager::new(&database);
    assert!(
        schema
            .has_index("catalog_items", "ix_catalog_items_latest")
            .await
            .unwrap()
    );
    for column in [
        "item_type",
        "is_present",
        "classification_state",
        "date_created",
        "id",
    ] {
        assert!(schema.has_column("catalog_items", column).await.unwrap());
    }
}

#[tokio::test]
async fn api_key_schema_is_bounded_binary_and_restrictive() {
    let database = test_database().await.unwrap();
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
    let creator_fk = foreign_keys(&database, "api_keys")
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

#[tokio::test]
async fn all_migrations_can_be_rolled_back() {
    let database = test_database().await.unwrap();
    Migrator::up(&database, None).await.unwrap();

    Migrator::down(&database, None).await.unwrap();
    let schema = SchemaManager::new(&database);
    for table in [
        "users",
        "catalog_items",
        "storage_objects",
        "work_jobs",
        "api_keys",
    ] {
        assert!(
            !schema.has_table(table).await.unwrap(),
            "table {table} remains"
        );
    }
}

#[tokio::test]
async fn provider_identity_scope_rollback_rejects_duplicates_before_changing_the_schema() {
    const PROVIDER_ID_SCOPE_MIGRATION_POSITION: u32 = 41;

    let database = test_database().await.unwrap();
    Migrator::up(&database, Some(PROVIDER_ID_SCOPE_MIGRATION_POSITION))
        .await
        .unwrap();
    let backend = database.get_database_backend();
    let first_item = uuid::Uuid::new_v4();
    let second_item = uuid::Uuid::new_v4();
    for (id, name) in [(first_item, "Movie"), (second_item, "Series")] {
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
                            Alias::new("classification_state"),
                            Alias::new("metadata_state"),
                            Alias::new("structure_state"),
                            Alias::new("source_state"),
                            Alias::new("structure_expansion_revision"),
                            Alias::new("source_index_revision"),
                            Alias::new("is_present"),
                        ])
                        .values_panic([
                            id.into(),
                            name.into(),
                            name.into(),
                            name.to_lowercase().into(),
                            "Matched".into(),
                            "Ready".into(),
                            "NotApplicable".into(),
                            "Indexed".into(),
                            0_i64.into(),
                            0_i64.into(),
                            true.into(),
                        ]),
                ),
            )
            .await
            .unwrap();
        database
            .execute(
                backend.build(
                    Query::insert()
                        .into_table(Alias::new("provider_ids"))
                        .columns([
                            Alias::new("id"),
                            Alias::new("catalog_item_id"),
                            Alias::new("provider"),
                            Alias::new("provider_item_id"),
                        ])
                        .values_panic([
                            uuid::Uuid::new_v4().into(),
                            id.into(),
                            "Tmdb".into(),
                            "42".into(),
                        ]),
                ),
            )
            .await
            .unwrap();
    }

    let error = Migrator::down(&database, Some(1)).await.unwrap_err();

    assert!(
        error.to_string().contains("duplicate provider identities"),
        "{error}"
    );
    let schema = SchemaManager::new(&database);
    assert!(schema.has_table("provider_ids").await.unwrap());
    assert!(
        schema
            .has_index("provider_ids", "uq_provider_ids_item_provider")
            .await
            .unwrap()
    );
    assert!(!schema.has_table("provider_ids_legacy").await.unwrap());
}

#[tokio::test]
async fn metadata_provider_settings_migration_is_reversible() {
    const METADATA_PROVIDER_SETTINGS_MIGRATION_POSITION: usize = 39;

    let database = test_database().await.unwrap();
    Migrator::up(&database, None).await.unwrap();
    let schema = SchemaManager::new(&database);
    assert!(
        schema
            .has_table("metadata_provider_settings")
            .await
            .unwrap()
    );
    for column in [
        "provider",
        "enabled",
        "language",
        "credential_id",
        "encrypted_payload",
        "key_version",
        "revision",
        "created_at",
        "updated_at",
    ] {
        assert!(
            schema
                .has_column("metadata_provider_settings", column)
                .await
                .unwrap(),
            "metadata_provider_settings missing {column}"
        );
    }
    let provider_type = column_type_name(&database, "metadata_provider_settings", "provider").await;
    let language_type = column_type_name(&database, "metadata_provider_settings", "language").await;
    assert!(
        provider_type.to_ascii_lowercase().contains("varchar(64)")
            || provider_type
                .to_ascii_lowercase()
                .contains("character varying(64)"),
        "metadata provider key is not bounded to 64 characters: {provider_type}"
    );
    assert!(
        language_type.to_ascii_lowercase().contains("varchar(32)")
            || language_type
                .to_ascii_lowercase()
                .contains("character varying(32)"),
        "metadata provider language is not bounded to 32 characters: {language_type}"
    );

    let now = chrono::Utc::now();
    let invalid_revision = Query::insert()
        .into_table(Alias::new("metadata_provider_settings"))
        .columns([
            Alias::new("provider"),
            Alias::new("enabled"),
            Alias::new("language"),
            Alias::new("credential_id"),
            Alias::new("encrypted_payload"),
            Alias::new("key_version"),
            Alias::new("revision"),
            Alias::new("created_at"),
            Alias::new("updated_at"),
        ])
        .values_panic([
            "tmdb".into(),
            true.into(),
            "en-US".into(),
            uuid::Uuid::new_v4().into(),
            vec![7_u8; 28].into(),
            1_i32.into(),
            0_i64.into(),
            now.into(),
            now.into(),
        ])
        .to_owned();
    assert!(
        database
            .execute(database.get_database_backend().build(&invalid_revision))
            .await
            .is_err(),
        "metadata provider revision check accepted zero"
    );

    let newer_migrations =
        u32::try_from(Migrator::migrations().len() - METADATA_PROVIDER_SETTINGS_MIGRATION_POSITION)
            .unwrap();
    Migrator::down(&database, Some(newer_migrations))
        .await
        .unwrap();

    assert!(!schema.has_table("metadata_snapshots").await.unwrap());
    assert!(
        schema
            .has_table("metadata_provider_settings")
            .await
            .unwrap()
    );

    Migrator::down(&database, Some(1)).await.unwrap();

    assert!(
        !schema
            .has_table("metadata_provider_settings")
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn durable_rows_are_not_cascade_deleted_and_active_jobs_are_single_flight() {
    let database = test_database().await.unwrap();
    Migrator::up(&database, None).await.unwrap();
    let schema = SchemaManager::new(&database);

    for table in ["storage_change_outbox", "media_locations"] {
        assert!(
            foreign_key_delete_rules(&database, table)
                .await
                .iter()
                .all(|rule| rule != "CASCADE"),
            "{table} must be reconciled explicitly"
        );
    }

    for (table, index) in [
        ("work_jobs", "uq_work_jobs_active"),
        ("storage_change_outbox", "idx_outbox_root_claim"),
        ("storage_change_outbox", "idx_outbox_root_revision_state"),
        ("users", "uq_users_username_key"),
        ("auth_sessions", "uq_auth_sessions_token_digest"),
        ("auth_sessions", "idx_auth_sessions_user_state"),
        ("auth_sessions", "idx_auth_sessions_expiry"),
        ("auth_sessions", "ix_auth_sessions_created_id"),
        ("api_keys", "uq_api_keys_envelope_id"),
        ("api_keys", "uq_api_keys_token_digest"),
        ("api_keys", "ix_api_keys_creator"),
        ("playback_tickets", "uq_playback_tickets_token_digest"),
        ("playback_tickets", "ix_playback_tickets_session_state"),
        (
            "playback_sessions",
            "ix_playback_sessions_started_item_user",
        ),
        ("playback_sessions", "ix_playback_sessions_active_event"),
        ("user_data", "ix_user_data_hybrid_signals"),
        (
            "library_catalog_items",
            "ix_library_catalog_items_hybrid_admin",
        ),
        (
            "storage_objects",
            "ix_storage_objects_facts_observed_root_revision",
        ),
        (
            "storage_root_objects",
            "ix_storage_root_objects_object_root",
        ),
        ("item_genres", "ix_item_genres_genre_item"),
        ("item_people", "ix_item_people_person_item"),
        ("item_languages", "ix_item_languages_language_item"),
        ("item_studios", "ix_item_studios_studio_item"),
        ("item_countries", "ix_item_countries_country_item"),
    ] {
        assert!(
            schema.has_index(table, index).await.unwrap(),
            "missing index {index}"
        );
    }
}

async fn foreign_key_delete_rules(
    database: &sea_orm::DatabaseConnection,
    table: &str,
) -> Vec<String> {
    let (statement, column) = match database.get_database_backend() {
        DbBackend::Sqlite => (
            Statement::from_string(
                DbBackend::Sqlite,
                format!("PRAGMA foreign_key_list('{table}')"),
            ),
            "on_delete",
        ),
        DbBackend::Postgres => (
            Statement::from_sql_and_values(
                DbBackend::Postgres,
                "SELECT rc.delete_rule FROM information_schema.referential_constraints rc \
                 JOIN information_schema.table_constraints tc \
                   ON tc.constraint_catalog = rc.constraint_catalog \
                  AND tc.constraint_schema = rc.constraint_schema \
                  AND tc.constraint_name = rc.constraint_name \
                 WHERE tc.table_schema = current_schema() AND tc.table_name = $1"
                    .to_owned(),
                [table.into()],
            ),
            "delete_rule",
        ),
        DbBackend::MySql => (
            Statement::from_sql_and_values(
                DbBackend::MySql,
                "SELECT rc.delete_rule AS delete_rule FROM information_schema.referential_constraints rc \
                 JOIN information_schema.table_constraints tc \
                   ON tc.constraint_schema = rc.constraint_schema \
                  AND tc.constraint_name = rc.constraint_name \
                 WHERE tc.constraint_schema = DATABASE() AND tc.table_name = ?"
                    .to_owned(),
                [table.into()],
            ),
            "delete_rule",
        ),
    };

    database
        .query_all(statement)
        .await
        .unwrap()
        .into_iter()
        .map(|row| row.try_get::<String>("", column).unwrap())
        .collect()
}

#[derive(Debug)]
struct ApiKeyForeignKey {
    source_column: String,
    target_table: String,
    target_column: String,
    delete_rule: String,
}

async fn foreign_keys(
    database: &sea_orm::DatabaseConnection,
    table: &str,
) -> Vec<ApiKeyForeignKey> {
    let statement = match database.get_database_backend() {
        DbBackend::Sqlite => Statement::from_string(
            DbBackend::Sqlite,
            format!("PRAGMA foreign_key_list('{table}')"),
        ),
        DbBackend::Postgres => Statement::from_sql_and_values(
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
               AND tc.table_name = $1"
                .to_owned(),
            [table.into()],
        ),
        DbBackend::MySql => Statement::from_sql_and_values(
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
               AND kcu.table_name = ? \
               AND kcu.referenced_table_name IS NOT NULL"
                .to_owned(),
            [table.into()],
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
                "SELECT data_type, \
                        CAST(character_maximum_length AS BIGINT) AS character_maximum_length \
                 FROM information_schema.columns \
                 WHERE table_schema = current_schema() AND table_name = $1 AND column_name = $2"
                    .to_owned(),
                [table.into(), column.into()],
            ))
            .await
            .unwrap()
            .map(|row| {
                let data_type = row.try_get::<String>("", "data_type").unwrap();
                row.try_get::<Option<i64>>("", "character_maximum_length")
                    .unwrap()
                    .map_or(data_type.clone(), |length| format!("{data_type}({length})"))
            })
            .unwrap(),
        DbBackend::MySql => {
            let row = database
                .query_one(Statement::from_sql_and_values(
                    DbBackend::MySql,
                    "SELECT data_type AS column_data_type, \
                            character_maximum_length AS column_maximum_length \
                     FROM information_schema.columns \
                     WHERE table_schema = DATABASE() AND table_name = ? AND column_name = ?"
                        .to_owned(),
                    [table.into(), column.into()],
                ))
                .await
                .unwrap()
                .unwrap();
            let data_type = row.try_get::<String>("", "column_data_type").unwrap();
            let maximum_length = row.try_get::<i64>("", "column_maximum_length").unwrap();
            format!("{}({maximum_length})", data_type.to_ascii_uppercase())
        }
    }
}

#[tokio::test]
async fn auth_migration_backfills_portable_username_keys() {
    let database = test_database().await.unwrap();
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
                    .and_where(Expr::col(Alias::new("id")).eq(user_id)),
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

#[tokio::test]
async fn device_migration_backfills_exact_identity_keys() {
    let database = test_database().await.unwrap();
    Migrator::up(&database, Some(35)).await.unwrap();
    let now = chrono::Utc::now();
    let user = AuthRepository::new(&database)
        .create_user(
            &Username::parse("Alice").unwrap(),
            "legacy-hash",
            true,
            true,
            now,
        )
        .await
        .unwrap();
    let session_id = uuid::Uuid::new_v4();
    let backend = database.get_database_backend();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("auth_sessions"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("user_id"),
                        Alias::new("token_digest"),
                        Alias::new("auth_revision"),
                        Alias::new("device_id"),
                        Alias::new("device_name"),
                        Alias::new("client_name"),
                        Alias::new("client_version"),
                        Alias::new("created_at"),
                    ])
                    .values_panic([
                        session_id.into(),
                        user.id().as_uuid().into(),
                        vec![91_u8; 32].into(),
                        user.auth_revision().into(),
                        "Phone".into(),
                        "Legacy Phone".into(),
                        "Legacy Client".into(),
                        "1.0".into(),
                        now.into(),
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
                    .column(Alias::new("device_key"))
                    .from(Alias::new("auth_sessions"))
                    .and_where(Expr::col(Alias::new("id")).eq(session_id)),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    let device_key = row.try_get::<String>("", "device_key").unwrap();
    assert_eq!(device_key.len(), 64);
    assert!(device_key.bytes().all(|byte| byte.is_ascii_hexdigit()));
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Seeds the complete legacy publication graph at migration 26.
async fn structure_scope_migration_invalidates_legacy_active_projections() {
    let database = test_database().await.unwrap();
    Migrator::up(&database, Some(26)).await.unwrap();
    let backend = database.get_database_backend();
    let owner = uuid::Uuid::new_v4();
    let child = uuid::Uuid::new_v4();
    let job = uuid::Uuid::new_v4();
    let publication = uuid::Uuid::new_v4();
    for (id, item_type, name, owner_id) in [
        (owner, "Series", "Legacy Series", None),
        (child, "Season", "Season 01", Some(owner)),
    ] {
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
                            Alias::new("structure_owner_item_id"),
                            Alias::new("is_present"),
                        ])
                        .values_panic([
                            id.into(),
                            item_type.into(),
                            name.into(),
                            name.to_lowercase().into(),
                            name.to_lowercase().into_bytes().into(),
                            "Matched".into(),
                            "Ready".into(),
                            if id == owner {
                                "Expanded"
                            } else {
                                "NotApplicable"
                            }
                            .into(),
                            "Unknown".into(),
                            4_i64.into(),
                            0_i64.into(),
                            owner_id.into(),
                            true.into(),
                        ]),
                ),
            )
            .await
            .unwrap();
    }
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("work_jobs"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("task_kind"),
                        Alias::new("scope_type"),
                        Alias::new("scope_id"),
                        Alias::new("expected_revision"),
                        Alias::new("state"),
                        Alias::new("priority"),
                        Alias::new("attempt_count"),
                    ])
                    .values_panic([
                        job.into(),
                        "ExpandItem".into(),
                        "CatalogItem".into(),
                        owner.into(),
                        4_i64.into(),
                        "Completed".into(),
                        100_i32.into(),
                        1_i32.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("catalog_publications"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("job_id"),
                        Alias::new("owner_catalog_item_id"),
                        Alias::new("publication_kind"),
                        Alias::new("expected_revision"),
                        Alias::new("state"),
                        Alias::new("manifest_sha256"),
                        Alias::new("expected_row_count"),
                    ])
                    .values_panic([
                        publication.into(),
                        job.into(),
                        owner.into(),
                        "Structure".into(),
                        4_i64.into(),
                        "Active".into(),
                        "legacy".into(),
                        1_i64.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("publication_catalog_items"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("publication_id"),
                        Alias::new("catalog_item_id"),
                        Alias::new("parent_catalog_item_id"),
                        Alias::new("item_type"),
                        Alias::new("name"),
                        Alias::new("sort_name"),
                        Alias::new("sort_key"),
                        Alias::new("source_state"),
                        Alias::new("source_index_revision"),
                        Alias::new("row_sha256"),
                    ])
                    .values_panic([
                        uuid::Uuid::new_v4().into(),
                        publication.into(),
                        child.into(),
                        owner.into(),
                        "Season".into(),
                        "Season 01".into(),
                        "season 01".into(),
                        b"season 01".to_vec().into(),
                        "Unknown".into(),
                        0_i64.into(),
                        "legacy-row".into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("catalog_items"))
                    .value(Alias::new("active_structure_publication_id"), publication)
                    .and_where(Expr::col(Alias::new("id")).eq(owner)),
            ),
        )
        .await
        .unwrap();

    Migrator::up(&database, None).await.unwrap();

    let owner_row = database
        .query_one(
            backend.build(
                Query::select()
                    .columns([
                        Alias::new("active_structure_publication_id"),
                        Alias::new("structure_expansion_revision"),
                        Alias::new("structure_state"),
                    ])
                    .from(Alias::new("catalog_items"))
                    .and_where(Expr::col(Alias::new("id")).eq(owner)),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        owner_row
            .try_get::<Option<uuid::Uuid>>("", "active_structure_publication_id")
            .unwrap(),
        None
    );
    assert_eq!(
        owner_row
            .try_get::<i64>("", "structure_expansion_revision")
            .unwrap(),
        5
    );
    assert_eq!(
        owner_row.try_get::<String>("", "structure_state").unwrap(),
        "NotExpanded"
    );
    let state = database
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("state"))
                    .from(Alias::new("catalog_publications"))
                    .and_where(Expr::col(Alias::new("id")).eq(publication)),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get::<String>("", "state")
        .unwrap();
    assert_eq!(state, "Retired");
    let generation = database
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("generation"))
                    .from(Alias::new("catalog_state"))
                    .and_where(Expr::col(Alias::new("id")).eq(1_i32)),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get::<i64>("", "generation")
        .unwrap();
    assert_eq!(generation, 1);
    let invalidations = database
        .query_one(
            backend.build(
                Query::select()
                    .expr_as(Expr::col(Alias::new("id")).count(), Alias::new("count"))
                    .from(Alias::new("cache_invalidation_outbox")),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get::<i64>("", "count")
        .unwrap();
    assert_eq!(invalidations, 0);
}

#[tokio::test]
async fn publication_migration_down_clears_active_pointers_and_derived_states() {
    const PUBLICATION_MIGRATION_POSITION: usize = 8;
    let database = test_database().await.unwrap();
    Migrator::up(&database, None).await.unwrap();
    let item = uuid::Uuid::new_v4();
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
                        Alias::new("classification_state"),
                        Alias::new("metadata_state"),
                        Alias::new("structure_state"),
                        Alias::new("source_state"),
                        Alias::new("structure_expansion_revision"),
                        Alias::new("source_index_revision"),
                        Alias::new("active_structure_publication_id"),
                        Alias::new("active_source_publication_id"),
                        Alias::new("is_present"),
                    ])
                    .values_panic([
                        item.into(),
                        "Series".into(),
                        "Series".into(),
                        "series".into(),
                        "Matched".into(),
                        "Ready".into(),
                        "Expanded".into(),
                        "Indexed".into(),
                        1_i64.into(),
                        1_i64.into(),
                        uuid::Uuid::new_v4().into(),
                        uuid::Uuid::new_v4().into(),
                        true.into(),
                    ]),
            ),
        )
        .await
        .unwrap();

    let steps =
        u32::try_from(Migrator::migrations().len() - PUBLICATION_MIGRATION_POSITION + 1).unwrap();
    Migrator::down(&database, Some(steps)).await.unwrap();

    let row = database
        .query_one(
            backend.build(
                Query::select()
                    .columns([
                        Alias::new("active_structure_publication_id"),
                        Alias::new("active_source_publication_id"),
                        Alias::new("structure_state"),
                        Alias::new("source_state"),
                    ])
                    .from(Alias::new("catalog_items"))
                    .and_where(Expr::col(Alias::new("id")).eq(item)),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert!(
        row.try_get::<Option<uuid::Uuid>>("", "active_structure_publication_id")
            .unwrap()
            .is_none()
    );
    assert!(
        row.try_get::<Option<uuid::Uuid>>("", "active_source_publication_id")
            .unwrap()
            .is_none()
    );
    assert_eq!(
        row.try_get::<String>("", "structure_state").unwrap(),
        "Unexpanded"
    );
    assert_eq!(
        row.try_get::<String>("", "source_state").unwrap(),
        "Unknown"
    );
}
