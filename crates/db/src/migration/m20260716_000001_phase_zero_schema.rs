use sea_orm_migration::{
    prelude::{
        Alias, ColumnDef, DbErr, DeriveMigrationName, ForeignKey, ForeignKeyAction,
        ForeignKeyCreateStatement, Index, IndexCreateStatement, MigrationTrait, SchemaManager,
        Table, TableCreateStatement,
    },
    schema::{
        big_integer, big_integer_null, boolean, integer, integer_null, json, string,
        string_len_uniq, string_null, string_uniq, text, text_null, timestamp_with_time_zone_null,
        uuid, uuid_null, uuid_uniq,
    },
};

const TABLES: &[&str] = &[
    "catalog_state",
    "users",
    "libraries",
    "catalog_items",
    "user_catalog_state",
    "library_catalog_items",
    "media_sources",
    "media_source_aliases",
    "storage_accounts",
    "storage_roots",
    "storage_objects",
    "media_locations",
    "media_streams",
    "media_stream_index_map",
    "subtitles",
    "user_data",
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
];

#[derive(DeriveMigrationName)]
pub struct Migration;

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for table in tables() {
            manager.create_table(table).await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for table in TABLES.iter().rev() {
            manager
                .drop_table(
                    Table::drop()
                        .table(Alias::new(*table))
                        .if_exists()
                        .to_owned(),
                )
                .await?;
        }
        Ok(())
    }
}

fn tables() -> Vec<TableCreateStatement> {
    vec![
        catalog_state(),
        users(),
        libraries(),
        catalog_items(),
        user_catalog_state(),
        library_catalog_items(),
        media_sources(),
        media_source_aliases(),
        storage_accounts(),
        storage_roots(),
        storage_objects(),
        media_locations(),
        media_streams(),
        media_stream_index_map(),
        subtitles(),
        user_data(),
        storage_sync_cursors(),
        storage_change_outbox(),
        library_storage_roots(),
        asset_blobs(),
        item_assets(),
        work_jobs(),
        work_staging_rows(),
        work_results(),
        import_runs(),
        import_legacy_ids(),
    ]
}

fn id() -> ColumnDef {
    uuid(Alias::new("id")).primary_key().take()
}

fn base(table: &str) -> TableCreateStatement {
    Table::create()
        .table(Alias::new(table))
        .if_not_exists()
        .col(id())
        .to_owned()
}

fn fk(
    name: &str,
    from_table: &str,
    from_column: &str,
    to_table: &str,
) -> ForeignKeyCreateStatement {
    ForeignKey::create()
        .name(name)
        .from(Alias::new(from_table), Alias::new(from_column))
        .to(Alias::new(to_table), Alias::new("id"))
        .on_delete(ForeignKeyAction::Cascade)
        .to_owned()
}

fn unique(name: &str, columns: &[&str]) -> IndexCreateStatement {
    let mut index = Index::create();
    index.name(name).unique();
    for column in columns {
        index.col(Alias::new(*column));
    }
    index.clone()
}

fn catalog_state() -> TableCreateStatement {
    Table::create()
        .table(Alias::new("catalog_state"))
        .if_not_exists()
        .col(integer(Alias::new("id")).primary_key().take())
        .col(big_integer(Alias::new("generation")))
        .to_owned()
}

fn users() -> TableCreateStatement {
    base("users")
        .col(string_uniq(Alias::new("username")))
        .col(string(Alias::new("password_hash")))
        .col(boolean(Alias::new("is_admin")))
        .to_owned()
}

fn libraries() -> TableCreateStatement {
    base("libraries")
        .col(string(Alias::new("name")))
        .col(string(Alias::new("scan_profile")))
        .col(string(Alias::new("object_selection_scope")))
        .col(string(Alias::new("metadata_policy")))
        .col(string(Alias::new("expansion_policy")))
        .col(string(Alias::new("probe_policy")))
        .col(integer(Alias::new("profile_version")))
        .to_owned()
}

fn catalog_items() -> TableCreateStatement {
    base("catalog_items")
        .col(uuid_null(Alias::new("parent_id")))
        .col(string(Alias::new("item_type")))
        .col(string(Alias::new("name")))
        .col(string_null(Alias::new("original_title")))
        .col(string(Alias::new("sort_name")))
        .col(integer_null(Alias::new("production_year")))
        .col(text_null(Alias::new("overview")))
        .col(string(Alias::new("classification_state")))
        .col(string(Alias::new("metadata_state")))
        .col(string(Alias::new("structure_state")))
        .col(string(Alias::new("source_state")))
        .col(big_integer(Alias::new("structure_expansion_revision")))
        .col(big_integer(Alias::new("source_index_revision")))
        .col(uuid_null(Alias::new("active_structure_publication_id")))
        .col(uuid_null(Alias::new("active_source_publication_id")))
        .col(boolean(Alias::new("is_present")))
        .col(text_null(Alias::new("last_error")))
        .foreign_key(&mut fk(
            "fk_catalog_items_parent",
            "catalog_items",
            "parent_id",
            "catalog_items",
        ))
        .to_owned()
}

fn user_catalog_state() -> TableCreateStatement {
    base("user_catalog_state")
        .col(uuid_uniq(Alias::new("user_id")))
        .col(big_integer(Alias::new("revision")))
        .foreign_key(&mut fk(
            "fk_user_catalog_state_user",
            "user_catalog_state",
            "user_id",
            "users",
        ))
        .to_owned()
}

fn library_catalog_items() -> TableCreateStatement {
    base("library_catalog_items")
        .col(uuid(Alias::new("library_id")))
        .col(uuid(Alias::new("catalog_item_id")))
        .index(&mut unique(
            "uq_library_catalog_items",
            &["library_id", "catalog_item_id"],
        ))
        .foreign_key(&mut fk(
            "fk_library_catalog_items_library",
            "library_catalog_items",
            "library_id",
            "libraries",
        ))
        .foreign_key(&mut fk(
            "fk_library_catalog_items_item",
            "library_catalog_items",
            "catalog_item_id",
            "catalog_items",
        ))
        .to_owned()
}

fn media_sources() -> TableCreateStatement {
    base("media_sources")
        .col(uuid(Alias::new("catalog_item_id")))
        .col(uuid(Alias::new("presentation_key")))
        .col(string_null(Alias::new("edition")))
        .col(string_null(Alias::new("container")))
        .col(string(Alias::new("probe_state")))
        .col(big_integer(Alias::new("probe_revision")))
        .col(uuid_null(Alias::new("probe_location_id")))
        .col(string_null(Alias::new("probe_location_revision")))
        .col(string_null(Alias::new("probe_content_identity")))
        .col(text_null(Alias::new("last_probe_error")))
        .index(&mut unique(
            "uq_media_sources_presentation",
            &["catalog_item_id", "presentation_key"],
        ))
        .foreign_key(&mut fk(
            "fk_media_sources_item",
            "media_sources",
            "catalog_item_id",
            "catalog_items",
        ))
        .to_owned()
}

fn media_source_aliases() -> TableCreateStatement {
    base("media_source_aliases")
        .col(string_uniq(Alias::new("alias_key")))
        .col(uuid(Alias::new("media_source_id")))
        .col(string(Alias::new("reason")))
        .foreign_key(&mut fk(
            "fk_media_source_aliases_source",
            "media_source_aliases",
            "media_source_id",
            "media_sources",
        ))
        .to_owned()
}

fn storage_accounts() -> TableCreateStatement {
    base("storage_accounts")
        .col(string(Alias::new("provider")))
        .col(string(Alias::new("display_name")))
        .col(string(Alias::new("account_identity")))
        .col(string(Alias::new("credential_ref")))
        .col(string(Alias::new("status")))
        .index(&mut unique(
            "uq_storage_accounts_identity",
            &["provider", "account_identity"],
        ))
        .to_owned()
}

fn storage_roots() -> TableCreateStatement {
    base("storage_roots")
        .col(uuid(Alias::new("storage_account_id")))
        .col(string(Alias::new("provider_root_id")))
        .col(big_integer(Alias::new("sync_revision")))
        .col(big_integer(Alias::new("reconciled_sync_revision")))
        .foreign_key(&mut fk(
            "fk_storage_roots_account",
            "storage_roots",
            "storage_account_id",
            "storage_accounts",
        ))
        .to_owned()
}

fn storage_objects() -> TableCreateStatement {
    base("storage_objects")
        .col(uuid(Alias::new("storage_account_id")))
        .col(string(Alias::new("provider_drive_id")))
        .col(string(Alias::new("provider_object_id")))
        .col(string_null(Alias::new("provider_parent_id")))
        .col(string(Alias::new("name")))
        .col(string(Alias::new("normalized_name")))
        .col(string(Alias::new("object_type")))
        .col(big_integer_null(Alias::new("size")))
        .col(string_null(Alias::new("checksum")))
        .col(string_null(Alias::new("remote_revision")))
        .col(big_integer(Alias::new("observed_sync_revision")))
        .col(boolean(Alias::new("children_indexed")))
        .col(big_integer(Alias::new("children_index_revision")))
        .col(string(Alias::new("identity_quality")))
        .col(string(Alias::new("presence_state")))
        .col(string_null(Alias::new("availability_reason")))
        .index(&mut unique(
            "uq_storage_objects_provider_identity",
            &[
                "storage_account_id",
                "provider_drive_id",
                "provider_object_id",
            ],
        ))
        .foreign_key(&mut fk(
            "fk_storage_objects_account",
            "storage_objects",
            "storage_account_id",
            "storage_accounts",
        ))
        .to_owned()
}

fn media_locations() -> TableCreateStatement {
    base("media_locations")
        .col(uuid(Alias::new("media_source_id")))
        .col(uuid_uniq(Alias::new("storage_object_id")))
        .col(string_null(Alias::new("content_identity")))
        .col(string_null(Alias::new("content_identity_kind")))
        .col(integer(Alias::new("priority")))
        .col(string(Alias::new("availability_state")))
        .col(text_null(Alias::new("last_error")))
        .foreign_key(&mut fk(
            "fk_media_locations_source",
            "media_locations",
            "media_source_id",
            "media_sources",
        ))
        .foreign_key(&mut fk(
            "fk_media_locations_object",
            "media_locations",
            "storage_object_id",
            "storage_objects",
        ))
        .to_owned()
}

fn media_streams() -> TableCreateStatement {
    base("media_streams")
        .col(uuid(Alias::new("media_source_id")))
        .col(string(Alias::new("stream_type")))
        .col(integer(Alias::new("stream_index")))
        .col(string_null(Alias::new("codec")))
        .col(string_null(Alias::new("language")))
        .foreign_key(&mut fk(
            "fk_media_streams_source",
            "media_streams",
            "media_source_id",
            "media_sources",
        ))
        .to_owned()
}

fn media_stream_index_map() -> TableCreateStatement {
    base("media_stream_index_map")
        .col(uuid(Alias::new("media_source_id")))
        .col(string(Alias::new("stream_identity")))
        .col(integer(Alias::new("delivery_index")))
        .col(integer_null(Alias::new("container_stream_index")))
        .col(string(Alias::new("stream_type")))
        .col(boolean(Alias::new("is_present")))
        .index(&mut unique(
            "uq_stream_delivery_index",
            &["media_source_id", "delivery_index"],
        ))
        .index(&mut unique(
            "uq_stream_identity",
            &["media_source_id", "stream_identity"],
        ))
        .foreign_key(&mut fk(
            "fk_stream_index_source",
            "media_stream_index_map",
            "media_source_id",
            "media_sources",
        ))
        .to_owned()
}

fn subtitles() -> TableCreateStatement {
    base("subtitles")
        .col(uuid(Alias::new("media_source_id")))
        .col(uuid(Alias::new("storage_object_id")))
        .col(string(Alias::new("format")))
        .col(string_null(Alias::new("language")))
        .col(integer_null(Alias::new("delivery_index")))
        .col(boolean(Alias::new("is_default")))
        .col(boolean(Alias::new("is_forced")))
        .foreign_key(&mut fk(
            "fk_subtitles_source",
            "subtitles",
            "media_source_id",
            "media_sources",
        ))
        .foreign_key(&mut fk(
            "fk_subtitles_object",
            "subtitles",
            "storage_object_id",
            "storage_objects",
        ))
        .to_owned()
}

fn user_data() -> TableCreateStatement {
    base("user_data")
        .col(uuid(Alias::new("user_id")))
        .col(uuid(Alias::new("catalog_item_id")))
        .col(big_integer(Alias::new("playback_position_ticks")))
        .col(boolean(Alias::new("is_played")))
        .col(integer(Alias::new("play_count")))
        .col(boolean(Alias::new("is_favorite")))
        .index(&mut unique(
            "uq_user_data_item",
            &["user_id", "catalog_item_id"],
        ))
        .foreign_key(&mut fk(
            "fk_user_data_user",
            "user_data",
            "user_id",
            "users",
        ))
        .foreign_key(&mut fk(
            "fk_user_data_item",
            "user_data",
            "catalog_item_id",
            "catalog_items",
        ))
        .to_owned()
}

fn storage_sync_cursors() -> TableCreateStatement {
    base("storage_sync_cursors")
        .col(uuid(Alias::new("storage_root_id")))
        .col(string(Alias::new("cursor_type")))
        .col(text(Alias::new("cursor_value")))
        .col(string(Alias::new("status")))
        .index(&mut unique(
            "uq_storage_sync_cursor",
            &["storage_root_id", "cursor_type"],
        ))
        .foreign_key(&mut fk(
            "fk_storage_sync_cursor_root",
            "storage_sync_cursors",
            "storage_root_id",
            "storage_roots",
        ))
        .to_owned()
}

fn storage_change_outbox() -> TableCreateStatement {
    base("storage_change_outbox")
        .col(uuid(Alias::new("storage_root_id")))
        .col(big_integer(Alias::new("sync_revision")))
        .col(string(Alias::new("event_type")))
        .col(uuid(Alias::new("storage_object_id")))
        .col(string_null(Alias::new("before_object_revision")))
        .col(string_null(Alias::new("after_object_revision")))
        .col(integer(Alias::new("payload_version")))
        .col(json(Alias::new("payload")))
        .col(string_uniq(Alias::new("dedupe_key")))
        .col(string(Alias::new("state")))
        .col(integer(Alias::new("attempt_count")))
        .col(string_null(Alias::new("lease_owner")))
        .col(timestamp_with_time_zone_null(Alias::new(
            "lease_expires_at",
        )))
        .col(timestamp_with_time_zone_null(Alias::new("available_at")))
        .col(text_null(Alias::new("last_error")))
        .foreign_key(&mut fk(
            "fk_storage_change_outbox_root",
            "storage_change_outbox",
            "storage_root_id",
            "storage_roots",
        ))
        .foreign_key(&mut fk(
            "fk_storage_change_outbox_object",
            "storage_change_outbox",
            "storage_object_id",
            "storage_objects",
        ))
        .to_owned()
}

fn library_storage_roots() -> TableCreateStatement {
    base("library_storage_roots")
        .col(uuid(Alias::new("library_id")))
        .col(uuid(Alias::new("storage_root_id")))
        .index(&mut unique(
            "uq_library_storage_roots",
            &["library_id", "storage_root_id"],
        ))
        .foreign_key(&mut fk(
            "fk_library_storage_roots_library",
            "library_storage_roots",
            "library_id",
            "libraries",
        ))
        .foreign_key(&mut fk(
            "fk_library_storage_roots_root",
            "library_storage_roots",
            "storage_root_id",
            "storage_roots",
        ))
        .to_owned()
}

fn asset_blobs() -> TableCreateStatement {
    base("asset_blobs")
        .col(string_len_uniq(Alias::new("sha256"), 64))
        .col(string(Alias::new("mime_type")))
        .col(integer_null(Alias::new("width")))
        .col(integer_null(Alias::new("height")))
        .col(big_integer(Alias::new("byte_size")))
        .col(string(Alias::new("local_relative_path")))
        .to_owned()
}

fn item_assets() -> TableCreateStatement {
    base("item_assets")
        .col(uuid(Alias::new("item_id")))
        .col(uuid(Alias::new("asset_blob_id")))
        .col(string(Alias::new("image_type")))
        .col(integer(Alias::new("priority")))
        .col(string(Alias::new("source_provider")))
        .col(string_null(Alias::new("source_reference")))
        .index(&mut unique(
            "uq_item_asset_role",
            &["item_id", "image_type", "priority"],
        ))
        .foreign_key(&mut fk(
            "fk_item_assets_item",
            "item_assets",
            "item_id",
            "catalog_items",
        ))
        .foreign_key(&mut fk(
            "fk_item_assets_blob",
            "item_assets",
            "asset_blob_id",
            "asset_blobs",
        ))
        .to_owned()
}

fn work_jobs() -> TableCreateStatement {
    base("work_jobs")
        .col(string(Alias::new("task_kind")))
        .col(string(Alias::new("scope_type")))
        .col(uuid(Alias::new("scope_id")))
        .col(big_integer(Alias::new("expected_revision")))
        .col(uuid_null(Alias::new("required_sync_job_id")))
        .col(big_integer_null(Alias::new("input_sync_revision")))
        .col(string(Alias::new("state")))
        .col(integer(Alias::new("priority")))
        .col(integer(Alias::new("attempt_count")))
        .col(string_null(Alias::new("lease_owner")))
        .col(timestamp_with_time_zone_null(Alias::new(
            "lease_expires_at",
        )))
        .col(text_null(Alias::new("last_error")))
        .index(&mut unique(
            "uq_work_job_revision",
            &["scope_id", "task_kind", "expected_revision", "state"],
        ))
        .to_owned()
}

fn work_staging_rows() -> TableCreateStatement {
    base("work_staging_rows")
        .col(uuid(Alias::new("job_id")))
        .col(uuid(Alias::new("publication_id")))
        .col(string(Alias::new("entity_kind")))
        .col(string(Alias::new("natural_key")))
        .col(json(Alias::new("payload")))
        .col(string(Alias::new("validation_state")))
        .index(&mut unique(
            "uq_work_staging_natural_key",
            &["job_id", "publication_id", "entity_kind", "natural_key"],
        ))
        .foreign_key(&mut fk(
            "fk_work_staging_job",
            "work_staging_rows",
            "job_id",
            "work_jobs",
        ))
        .to_owned()
}

fn work_results() -> TableCreateStatement {
    base("work_results")
        .col(uuid_uniq(Alias::new("job_id")))
        .col(json(Alias::new("counters")))
        .col(json(Alias::new("warnings")))
        .col(text_null(Alias::new("error_summary")))
        .foreign_key(&mut fk(
            "fk_work_results_job",
            "work_results",
            "job_id",
            "work_jobs",
        ))
        .to_owned()
}

fn import_runs() -> TableCreateStatement {
    base("import_runs")
        .col(string(Alias::new("adapter_kind")))
        .col(string(Alias::new("state")))
        .col(boolean(Alias::new("dry_run")))
        .col(json(Alias::new("checkpoint")))
        .col(text_null(Alias::new("last_error")))
        .to_owned()
}

fn import_legacy_ids() -> TableCreateStatement {
    base("import_legacy_ids")
        .col(uuid(Alias::new("import_run_id")))
        .col(string(Alias::new("source_system")))
        .col(string(Alias::new("legacy_id")))
        .col(uuid(Alias::new("catalog_item_id")))
        .index(&mut unique(
            "uq_import_legacy_id",
            &["source_system", "legacy_id"],
        ))
        .foreign_key(&mut fk(
            "fk_import_legacy_ids_run",
            "import_legacy_ids",
            "import_run_id",
            "import_runs",
        ))
        .foreign_key(&mut fk(
            "fk_import_legacy_ids_item",
            "import_legacy_ids",
            "catalog_item_id",
            "catalog_items",
        ))
        .to_owned()
}
