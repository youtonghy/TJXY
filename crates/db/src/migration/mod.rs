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

use sea_orm_migration::prelude::*;
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
        ]
    }
}
