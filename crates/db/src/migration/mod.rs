mod m20260716_000001_phase_zero_schema;
mod m20260717_000002_complete_phase_zero_schema;
mod m20260717_000003_outbox_claim_indexes;
mod m20260717_000004_auth_sessions;
mod m20260718_000005_l2_browse;
mod m20260718_000006_work_job_claiming;
mod m20260718_000007_storage_root_objects;
mod m20260718_000008_catalog_publications;
mod m20260718_000009_playback_sessions;
mod m20260718_000010_media_probe;
mod m20260718_000011_import_sources;
mod m20260718_000012_metadata_revision;
mod m20260718_000013_title_discovery;
mod m20260718_000014_catalog_date_created;
mod m20260719_000015_change_cursor_recovery;
mod m20260719_000016_cache_invalidation_outbox;
mod m20260720_000017_storage_relink_candidates;
mod m20260720_000018_filesystem_storage_configs;
mod m20260722_000019_library_root_discovery;
mod m20260722_000020_metadata_resolved_revision;
mod m20260724_000021_natural_key_hashes;
mod m20260725_000022_mysql_work_job_active_slot;
mod m20260725_000023_work_job_scope_type;
mod m20260725_000024_hybrid_candidate_signals;
mod m20260725_000025_metadata_requirements;
mod m20260725_000026_hybrid_admin_candidates;
mod m20260726_000027_structure_projection_scope;
mod m20260726_000028_work_job_storage_root_affinity;
mod m20260726_000029_storage_object_fact_origin;
mod m20260726_000030_storage_work_root_key;
mod m20260726_000031_recover_storage_fact_origins;
mod m20260726_000032_media_stream_compatibility;
mod m20260726_000033_media_source_playback_policy;
mod m20260726_000034_media_collections;
mod m20260726_000035_display_preferences;
mod m20260726_000036_device_options;
mod m20260726_000037_api_keys;
mod m20260730_000038_playback_tickets;
mod m20260730_000039_metadata_provider_settings;
mod m20260730_000040_rich_catalog_metadata;
mod m20260731_000041_provider_id_scope;
mod m20260731_000042_dashboard_indexes;
mod m20260731_000043_user_profile_and_watch_time;
mod m20260801_000044_metadata_source_mode;
mod m20260802_000045_system_settings;
mod m20260802_000046_expand_system_settings;
mod m20260802_000047_ai_assistant;
mod m20260802_000048_system_media_browser_roots;
mod m20260803_000049_ai_model_reasoning_effort;
mod m20260803_000050_ai_usage_analytics;
mod m20260803_000051_ai_daily_quota;
mod m20260803_000052_announcements;
mod m20260804_000053_installation;
mod m20260804_000054_similar_item_indexes;
mod m20260806_000055_ai_message_sequence;
mod m20260811_000056_remove_hybrid_scan_profile;
mod m20260811_000057_normalize_legacy_title_year;

use std::collections::HashSet;

use sea_orm::{DatabaseConnection, DbErr};
use sea_orm_migration::{MigratorTrait, prelude::*};
use thiserror::Error;
use uuid::Uuid;

fn uuid_with_nil_default(backend: sea_orm::DbBackend, name: impl IntoIden) -> ColumnDef {
    let mut column = sea_orm_migration::schema::uuid(name);
    if backend == sea_orm::DbBackend::MySql {
        column.default(Uuid::nil().as_bytes().to_vec());
    } else {
        column.default(Uuid::nil());
    }
    column
}

pub struct Migrator;

#[derive(Debug, Error)]
pub enum SchemaMigrationError {
    #[error("database migration failed: {0}")]
    Database(#[from] DbErr),
    #[error(
        "database schema is newer than this program (latest supported migration: {supported}); unknown applied migrations: {unknown:?}"
    )]
    DatabaseIsNewer {
        supported: String,
        unknown: Vec<String>,
    },
    #[error("database migration history does not match its schema; missing objects: {missing:?}")]
    SchemaDrift { missing: Vec<String> },
}

/// Applies all migrations supported by this build after rejecting a newer database.
///
/// `SeaORM` already handles the forward-upgrade path. The preflight turns its generic
/// "migration file is missing" error into an actionable version-compatibility error.
///
/// # Errors
///
/// Returns [`SchemaMigrationError`] when migration storage is unavailable, the database
/// contains migrations unknown to this build, a migration fails, or critical schema
/// objects are missing after migration.
pub async fn migrate_database(database: &DatabaseConnection) -> Result<(), SchemaMigrationError> {
    let supported: Vec<String> = Migrator::migrations()
        .iter()
        .map(|migration| migration.name().to_owned())
        .collect();
    migration_history(database, &supported).await?;

    Migrator::up(database, None).await?;
    let applied = migration_history(database, &supported).await?;
    let missing_migrations = supported
        .iter()
        .filter(|migration| !applied.contains(migration.as_str()))
        .map(|migration| format!("migration {migration}"))
        .collect::<Vec<_>>();
    if !missing_migrations.is_empty() {
        return Err(SchemaMigrationError::SchemaDrift {
            missing: missing_migrations,
        });
    }
    validate_current_schema(database).await?;
    Ok(())
}

async fn migration_history(
    database: &DatabaseConnection,
    supported: &[String],
) -> Result<HashSet<String>, SchemaMigrationError> {
    let applied: HashSet<String> = Migrator::get_migration_models(database)
        .await?
        .into_iter()
        .map(|migration| migration.version)
        .collect();
    let supported_set: HashSet<&str> = supported.iter().map(String::as_str).collect();
    let mut unknown = applied
        .iter()
        .filter(|version| !supported_set.contains(version.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    unknown.sort();
    if unknown.is_empty() {
        Ok(applied)
    } else {
        Err(SchemaMigrationError::DatabaseIsNewer {
            supported: supported
                .last()
                .cloned()
                .unwrap_or_else(|| "none".to_owned()),
            unknown,
        })
    }
}

async fn validate_current_schema(
    database: &DatabaseConnection,
) -> Result<(), SchemaMigrationError> {
    let manager = SchemaManager::new(database);
    let mut missing = Vec::new();
    let mut missing_tables = HashSet::new();
    for table in [
        "api_keys",
        "playback_tickets",
        "metadata_provider_settings",
        "system_settings",
        "ai_provider_settings",
        "ai_models",
        "ai_conversations",
        "ai_messages",
        "ai_execution_records",
        "ai_daily_usage",
        "announcements",
        "installation_records",
    ] {
        if !manager.has_table(table).await? {
            missing.push(format!("table {table}"));
            missing_tables.insert(table);
        }
    }
    for (table, column) in [
        ("libraries", "metadata_source_mode"),
        ("system_settings", "media_browser_roots"),
        ("ai_models", "reasoning_effort"),
        ("ai_messages", "sequence_number"),
    ] {
        if !missing_tables.contains(table) && !manager.has_column(table, column).await? {
            missing.push(format!("column {table}.{column}"));
        }
    }
    if !missing_tables.contains("ai_messages")
        && !manager
            .has_index("ai_messages", "uq_ai_messages_conversation_sequence")
            .await?
    {
        missing.push("index uq_ai_messages_conversation_sequence".to_owned());
    }
    if missing.is_empty() {
        Ok(())
    } else {
        Err(SchemaMigrationError::SchemaDrift { missing })
    }
}

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260716_000001_phase_zero_schema::Migration),
            Box::new(m20260717_000002_complete_phase_zero_schema::Migration),
            Box::new(m20260717_000003_outbox_claim_indexes::Migration),
            Box::new(m20260717_000004_auth_sessions::Migration),
            Box::new(m20260718_000005_l2_browse::Migration),
            Box::new(m20260718_000006_work_job_claiming::Migration),
            Box::new(m20260718_000007_storage_root_objects::Migration),
            Box::new(m20260718_000008_catalog_publications::Migration),
            Box::new(m20260718_000009_playback_sessions::Migration),
            Box::new(m20260718_000010_media_probe::Migration),
            Box::new(m20260718_000011_import_sources::Migration),
            Box::new(m20260718_000012_metadata_revision::Migration),
            Box::new(m20260718_000013_title_discovery::Migration),
            Box::new(m20260718_000014_catalog_date_created::Migration),
            Box::new(m20260719_000015_change_cursor_recovery::Migration),
            Box::new(m20260719_000016_cache_invalidation_outbox::Migration),
            Box::new(m20260720_000017_storage_relink_candidates::Migration),
            Box::new(m20260720_000018_filesystem_storage_configs::Migration),
            Box::new(m20260722_000019_library_root_discovery::Migration),
            Box::new(m20260722_000020_metadata_resolved_revision::Migration),
            Box::new(m20260724_000021_natural_key_hashes::Migration),
            Box::new(m20260725_000022_mysql_work_job_active_slot::Migration),
            Box::new(m20260725_000023_work_job_scope_type::Migration),
            Box::new(m20260725_000024_hybrid_candidate_signals::Migration),
            Box::new(m20260725_000025_metadata_requirements::Migration),
            Box::new(m20260725_000026_hybrid_admin_candidates::Migration),
            Box::new(m20260726_000027_structure_projection_scope::Migration),
            Box::new(m20260726_000028_work_job_storage_root_affinity::Migration),
            Box::new(m20260726_000029_storage_object_fact_origin::Migration),
            Box::new(m20260726_000030_storage_work_root_key::Migration),
            Box::new(m20260726_000031_recover_storage_fact_origins::Migration),
            Box::new(m20260726_000032_media_stream_compatibility::Migration),
            Box::new(m20260726_000033_media_source_playback_policy::Migration),
            Box::new(m20260726_000034_media_collections::Migration),
            Box::new(m20260726_000035_display_preferences::Migration),
            Box::new(m20260726_000036_device_options::Migration),
            Box::new(m20260726_000037_api_keys::Migration),
            Box::new(m20260730_000038_playback_tickets::Migration),
            Box::new(m20260730_000039_metadata_provider_settings::Migration),
            Box::new(m20260730_000040_rich_catalog_metadata::Migration),
            Box::new(m20260731_000041_provider_id_scope::Migration),
            Box::new(m20260731_000042_dashboard_indexes::Migration),
            Box::new(m20260731_000043_user_profile_and_watch_time::Migration),
            Box::new(m20260801_000044_metadata_source_mode::Migration),
            Box::new(m20260802_000045_system_settings::Migration),
            Box::new(m20260802_000046_expand_system_settings::Migration),
            Box::new(m20260802_000047_ai_assistant::Migration),
            Box::new(m20260802_000048_system_media_browser_roots::Migration),
            Box::new(m20260803_000049_ai_model_reasoning_effort::Migration),
            Box::new(m20260803_000050_ai_usage_analytics::Migration),
            Box::new(m20260803_000051_ai_daily_quota::Migration),
            Box::new(m20260803_000052_announcements::Migration),
            Box::new(m20260804_000053_installation::Migration),
            Box::new(m20260804_000054_similar_item_indexes::Migration),
            Box::new(m20260806_000055_ai_message_sequence::Migration),
            Box::new(m20260811_000056_remove_hybrid_scan_profile::Migration),
            Box::new(m20260811_000057_normalize_legacy_title_year::Migration),
        ]
    }
}
