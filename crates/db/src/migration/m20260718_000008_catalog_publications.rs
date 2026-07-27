use sea_orm::ConnectionTrait;
use sea_orm_migration::{
    prelude::{
        Alias, DbErr, DeriveMigrationName, Expr, ForeignKey, Index, MigrationTrait, Query,
        SchemaManager, Table, TableCreateStatement,
    },
    schema::{
        big_integer, big_integer_null, blob, boolean, integer, integer_null, string, string_len,
        string_null, text_null, timestamp_with_time_zone_null, uuid, uuid_null,
    },
};

const PUBLICATIONS: &str = "catalog_publications";
const ITEMS: &str = "publication_catalog_items";
const SOURCES: &str = "publication_media_sources";
const LOCATIONS: &str = "publication_media_locations";
const SUBTITLES: &str = "publication_subtitles";
const OUTBOX: &str = "catalog_change_outbox";
const MAX_SORT_KEY_BYTES: u32 = 2_000;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("catalog_items"))
                    .add_column(timestamp_with_time_zone_null(Alias::new(
                        "last_expanded_at",
                    )))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("catalog_items"))
                    .add_column(uuid_null(Alias::new("structure_owner_item_id")))
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("ix_catalog_items_structure_owner")
                    .table(Alias::new("catalog_items"))
                    .col(Alias::new("structure_owner_item_id"))
                    .col(Alias::new("id"))
                    .to_owned(),
            )
            .await?;
        for column in [
            string_null(Alias::new("video_codec")),
            string_null(Alias::new("resolution")),
            big_integer_null(Alias::new("bitrate")),
            big_integer_null(Alias::new("runtime_ticks")),
        ] {
            manager
                .alter_table(
                    Table::alter()
                        .table(Alias::new("media_sources"))
                        .add_column(column)
                        .to_owned(),
                )
                .await?;
        }
        manager.create_table(publications()).await?;
        manager.create_index(publication_owner_index()).await?;
        let mysql = manager.get_connection().get_database_backend() == sea_orm::DbBackend::MySql;
        manager.create_table(items(mysql)).await?;
        manager.create_index(item_identity_index()).await?;
        manager.create_index(item_parent_index()).await?;
        manager.create_table(sources()).await?;
        for index in source_indexes() {
            manager.create_index(index).await?;
        }
        manager.create_table(locations()).await?;
        for index in location_indexes() {
            manager.create_index(index).await?;
        }
        manager.create_table(subtitles()).await?;
        for index in subtitle_indexes() {
            manager.create_index(index).await?;
        }
        manager.create_table(outbox()).await?;
        for index in outbox_indexes() {
            manager.create_index(index).await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let connection = manager.get_connection();
        let backend = connection.get_database_backend();
        let reset = Query::update()
            .table(Alias::new("catalog_items"))
            .value(
                Alias::new("active_structure_publication_id"),
                Option::<uuid::Uuid>::None,
            )
            .value(
                Alias::new("active_source_publication_id"),
                Option::<uuid::Uuid>::None,
            )
            .value(
                Alias::new("structure_state"),
                Expr::cust("CASE WHEN structure_state = 'Expanded' THEN 'Unexpanded' ELSE structure_state END"),
            )
            .value(
                Alias::new("source_state"),
                Expr::cust("CASE WHEN source_state = 'Indexed' THEN 'Unknown' ELSE source_state END"),
            )
            .to_owned();
        connection.execute(backend.build(&reset)).await?;
        for table in [OUTBOX, SUBTITLES, LOCATIONS, SOURCES, ITEMS, PUBLICATIONS] {
            manager
                .drop_table(
                    Table::drop()
                        .table(Alias::new(table))
                        .if_exists()
                        .to_owned(),
                )
                .await?;
        }
        for column in ["runtime_ticks", "bitrate", "resolution", "video_codec"] {
            manager
                .alter_table(
                    Table::alter()
                        .table(Alias::new("media_sources"))
                        .drop_column(Alias::new(column))
                        .to_owned(),
                )
                .await?;
        }
        manager
            .drop_index(
                Index::drop()
                    .name("ix_catalog_items_structure_owner")
                    .table(Alias::new("catalog_items"))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("catalog_items"))
                    .drop_column(Alias::new("structure_owner_item_id"))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("catalog_items"))
                    .drop_column(Alias::new("last_expanded_at"))
                    .to_owned(),
            )
            .await
    }
}

fn sources() -> TableCreateStatement {
    Table::create()
        .table(Alias::new(SOURCES))
        .col(uuid(Alias::new("id")).primary_key().take())
        .col(uuid(Alias::new("publication_id")))
        .col(uuid(Alias::new("media_source_id")))
        .col(uuid(Alias::new("catalog_item_id")))
        .col(uuid(Alias::new("presentation_key")))
        .col(string_null(Alias::new("edition")))
        .col(string_null(Alias::new("container")))
        .col(string_len(Alias::new("row_sha256"), 64))
        .foreign_key(
            ForeignKey::create()
                .name("fk_publication_media_sources_publication")
                .from(Alias::new(SOURCES), Alias::new("publication_id"))
                .to(Alias::new(PUBLICATIONS), Alias::new("id")),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_publication_media_sources_item")
                .from(Alias::new(SOURCES), Alias::new("catalog_item_id"))
                .to(Alias::new("catalog_items"), Alias::new("id")),
        )
        .to_owned()
}

fn source_indexes() -> [sea_orm_migration::prelude::IndexCreateStatement; 2] {
    [
        Index::create()
            .name("uq_publication_media_sources_identity")
            .table(Alias::new(SOURCES))
            .col(Alias::new("publication_id"))
            .col(Alias::new("media_source_id"))
            .unique()
            .to_owned(),
        Index::create()
            .name("uq_publication_media_sources_presentation")
            .table(Alias::new(SOURCES))
            .col(Alias::new("publication_id"))
            .col(Alias::new("presentation_key"))
            .unique()
            .to_owned(),
    ]
}

fn locations() -> TableCreateStatement {
    Table::create()
        .table(Alias::new(LOCATIONS))
        .col(uuid(Alias::new("id")).primary_key().take())
        .col(uuid(Alias::new("publication_id")))
        .col(uuid(Alias::new("media_location_id")))
        .col(uuid(Alias::new("media_source_id")))
        .col(uuid(Alias::new("storage_object_id")))
        .col(string_null(Alias::new("content_identity")))
        .col(string_null(Alias::new("content_identity_kind")))
        .col(integer(Alias::new("priority")))
        .col(string_len(Alias::new("row_sha256"), 64))
        .foreign_key(
            ForeignKey::create()
                .name("fk_publication_media_locations_publication")
                .from(Alias::new(LOCATIONS), Alias::new("publication_id"))
                .to(Alias::new(PUBLICATIONS), Alias::new("id")),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_publication_media_locations_object")
                .from(Alias::new(LOCATIONS), Alias::new("storage_object_id"))
                .to(Alias::new("storage_objects"), Alias::new("id")),
        )
        .to_owned()
}

fn location_indexes() -> [sea_orm_migration::prelude::IndexCreateStatement; 2] {
    [
        Index::create()
            .name("uq_publication_media_locations_identity")
            .table(Alias::new(LOCATIONS))
            .col(Alias::new("publication_id"))
            .col(Alias::new("media_location_id"))
            .unique()
            .to_owned(),
        Index::create()
            .name("uq_publication_media_locations_object")
            .table(Alias::new(LOCATIONS))
            .col(Alias::new("publication_id"))
            .col(Alias::new("storage_object_id"))
            .unique()
            .to_owned(),
    ]
}

fn subtitles() -> TableCreateStatement {
    Table::create()
        .table(Alias::new(SUBTITLES))
        .col(uuid(Alias::new("id")).primary_key().take())
        .col(uuid(Alias::new("publication_id")))
        .col(uuid(Alias::new("subtitle_id")))
        .col(uuid(Alias::new("media_source_id")))
        .col(uuid(Alias::new("storage_object_id")))
        .col(string(Alias::new("format")))
        .col(string_null(Alias::new("language")))
        .col(integer_null(Alias::new("delivery_index")))
        .col(boolean(Alias::new("is_default")))
        .col(boolean(Alias::new("is_forced")))
        .col(string_len(Alias::new("row_sha256"), 64))
        .foreign_key(
            ForeignKey::create()
                .name("fk_publication_subtitles_publication")
                .from(Alias::new(SUBTITLES), Alias::new("publication_id"))
                .to(Alias::new(PUBLICATIONS), Alias::new("id")),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_publication_subtitles_object")
                .from(Alias::new(SUBTITLES), Alias::new("storage_object_id"))
                .to(Alias::new("storage_objects"), Alias::new("id")),
        )
        .to_owned()
}

fn subtitle_indexes() -> [sea_orm_migration::prelude::IndexCreateStatement; 2] {
    [
        Index::create()
            .name("uq_publication_subtitles_identity")
            .table(Alias::new(SUBTITLES))
            .col(Alias::new("publication_id"))
            .col(Alias::new("subtitle_id"))
            .unique()
            .to_owned(),
        Index::create()
            .name("uq_publication_subtitles_object")
            .table(Alias::new(SUBTITLES))
            .col(Alias::new("publication_id"))
            .col(Alias::new("media_source_id"))
            .col(Alias::new("storage_object_id"))
            .unique()
            .to_owned(),
    ]
}

fn publications() -> TableCreateStatement {
    Table::create()
        .table(Alias::new(PUBLICATIONS))
        .col(uuid(Alias::new("id")).primary_key().take())
        .col(uuid(Alias::new("job_id")).unique_key().take())
        .col(uuid(Alias::new("owner_catalog_item_id")))
        .col(string(Alias::new("publication_kind")))
        .col(big_integer(Alias::new("expected_revision")))
        .col(big_integer_null(Alias::new("input_sync_revision")))
        .col(string(Alias::new("state")))
        .col(string_len(Alias::new("manifest_sha256"), 64))
        .col(big_integer(Alias::new("expected_row_count")))
        .col(string_null(Alias::new("source_manifest_sha256")))
        .col(big_integer_null(Alias::new("expected_source_row_count")))
        .col(big_integer_null(Alias::new("activated_generation")))
        .col(timestamp_with_time_zone_null(Alias::new("created_at")))
        .col(timestamp_with_time_zone_null(Alias::new("sealed_at")))
        .col(timestamp_with_time_zone_null(Alias::new("published_at")))
        .col(timestamp_with_time_zone_null(Alias::new("retired_at")))
        .foreign_key(
            ForeignKey::create()
                .name("fk_catalog_publications_job")
                .from(Alias::new(PUBLICATIONS), Alias::new("job_id"))
                .to(Alias::new("work_jobs"), Alias::new("id")),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_catalog_publications_owner")
                .from(
                    Alias::new(PUBLICATIONS),
                    Alias::new("owner_catalog_item_id"),
                )
                .to(Alias::new("catalog_items"), Alias::new("id")),
        )
        .to_owned()
}

fn publication_owner_index() -> sea_orm_migration::prelude::IndexCreateStatement {
    Index::create()
        .name("ix_catalog_publications_owner")
        .table(Alias::new(PUBLICATIONS))
        .col(Alias::new("owner_catalog_item_id"))
        .col(Alias::new("publication_kind"))
        .col(Alias::new("state"))
        .to_owned()
}

fn items(mysql: bool) -> TableCreateStatement {
    Table::create()
        .table(Alias::new(ITEMS))
        .col(uuid(Alias::new("id")).primary_key().take())
        .col(uuid(Alias::new("publication_id")))
        .col(uuid(Alias::new("catalog_item_id")))
        .col(uuid(Alias::new("parent_catalog_item_id")))
        .col(string(Alias::new("item_type")))
        .col(string(Alias::new("name")))
        .col(string(Alias::new("sort_name")))
        .col(if mysql {
            sea_orm_migration::prelude::ColumnDef::new(Alias::new("sort_key"))
                .var_binary(MAX_SORT_KEY_BYTES)
                .not_null()
                .take()
        } else {
            blob(Alias::new("sort_key"))
        })
        .col(integer_null(Alias::new("production_year")))
        .col(text_null(Alias::new("overview")))
        .col(string(Alias::new("source_state")))
        .col(big_integer(Alias::new("source_index_revision")))
        .col(string_len(Alias::new("row_sha256"), 64))
        .foreign_key(
            ForeignKey::create()
                .name("fk_publication_catalog_items_publication")
                .from(Alias::new(ITEMS), Alias::new("publication_id"))
                .to(Alias::new(PUBLICATIONS), Alias::new("id")),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_publication_catalog_items_item")
                .from(Alias::new(ITEMS), Alias::new("catalog_item_id"))
                .to(Alias::new("catalog_items"), Alias::new("id")),
        )
        .to_owned()
}

fn item_identity_index() -> sea_orm_migration::prelude::IndexCreateStatement {
    Index::create()
        .name("uq_publication_catalog_items_identity")
        .table(Alias::new(ITEMS))
        .col(Alias::new("publication_id"))
        .col(Alias::new("catalog_item_id"))
        .unique()
        .to_owned()
}

fn item_parent_index() -> sea_orm_migration::prelude::IndexCreateStatement {
    Index::create()
        .name("ix_publication_catalog_items_parent")
        .table(Alias::new(ITEMS))
        .col(Alias::new("publication_id"))
        .col(Alias::new("parent_catalog_item_id"))
        .col(Alias::new("sort_key"))
        .col(Alias::new("catalog_item_id"))
        .to_owned()
}

fn outbox() -> TableCreateStatement {
    Table::create()
        .table(Alias::new(OUTBOX))
        .col(uuid(Alias::new("id")).primary_key().take())
        .col(big_integer(Alias::new("generation")))
        .col(string(Alias::new("event_type")))
        .col(uuid(Alias::new("catalog_item_id")))
        .col(uuid(Alias::new("publication_id")))
        .col(timestamp_with_time_zone_null(Alias::new("created_at")))
        .col(timestamp_with_time_zone_null(Alias::new("processed_at")))
        .foreign_key(
            ForeignKey::create()
                .name("fk_catalog_change_outbox_item")
                .from(Alias::new(OUTBOX), Alias::new("catalog_item_id"))
                .to(Alias::new("catalog_items"), Alias::new("id")),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_catalog_change_outbox_publication")
                .from(Alias::new(OUTBOX), Alias::new("publication_id"))
                .to(Alias::new(PUBLICATIONS), Alias::new("id")),
        )
        .to_owned()
}

fn outbox_indexes() -> [sea_orm_migration::prelude::IndexCreateStatement; 2] {
    [
        Index::create()
            .name("uq_catalog_change_outbox_publication_event")
            .table(Alias::new(OUTBOX))
            .col(Alias::new("publication_id"))
            .col(Alias::new("event_type"))
            .unique()
            .to_owned(),
        Index::create()
            .name("ix_catalog_change_outbox_generation")
            .table(Alias::new(OUTBOX))
            .col(Alias::new("generation"))
            .col(Alias::new("processed_at"))
            .to_owned(),
    ]
}
