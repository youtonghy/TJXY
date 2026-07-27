use sea_orm::ConnectionTrait;
use sea_orm_migration::{
    prelude::{
        Alias, ColumnDef, DbErr, DeriveMigrationName, Index, MigrationTrait, SchemaManager, Table,
    },
    schema::{integer_null, string_len_null},
};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    #[allow(clippy::too_many_lines)] // Keeps the backend-specific schema transition atomic.
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let mysql = manager.get_connection().get_database_backend() == sea_orm::DbBackend::MySql;
        if mysql {
            manager
                .create_index(
                    Index::create()
                        .name("ix_catalog_change_outbox_publication")
                        .table(Alias::new("catalog_change_outbox"))
                        .col(Alias::new("publication_id"))
                        .to_owned(),
                )
                .await?;
        }
        manager
            .drop_index(
                Index::drop()
                    .name("uq_catalog_change_outbox_publication_event")
                    .table(Alias::new("catalog_change_outbox"))
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("uq_catalog_change_outbox_generation")
                    .table(Alias::new("catalog_change_outbox"))
                    .col(Alias::new("generation"))
                    .unique()
                    .to_owned(),
            )
            .await?;
        for column in [
            string_len_null(Alias::new("stream_identity"), 2048),
            integer_null(Alias::new("delivery_index")),
            integer_null(Alias::new("container_stream_index")),
            integer_null(Alias::new("width")),
            integer_null(Alias::new("height")),
            integer_null(Alias::new("channels")),
            nullable_boolean("is_default"),
            nullable_boolean("is_forced"),
            nullable_boolean("is_external"),
            nullable_boolean("is_text"),
        ] {
            manager
                .alter_table(
                    Table::alter()
                        .table(Alias::new("media_streams"))
                        .add_column(column)
                        .to_owned(),
                )
                .await?;
        }
        if mysql {
            manager
                .alter_table(
                    Table::alter()
                        .table(Alias::new("media_streams"))
                        .add_column(string_len_null(Alias::new("stream_identity_key"), 64))
                        .to_owned(),
                )
                .await?;
        }
        let identity_column = if mysql {
            "stream_identity_key"
        } else {
            "stream_identity"
        };
        if mysql {
            manager
                .create_index(
                    Index::create()
                        .name("ix_media_streams_source")
                        .table(Alias::new("media_streams"))
                        .col(Alias::new("media_source_id"))
                        .to_owned(),
                )
                .await?;
        }
        manager
            .create_index(
                Index::create()
                    .name("uq_media_streams_identity")
                    .table(Alias::new("media_streams"))
                    .col(Alias::new("media_source_id"))
                    .col(Alias::new(identity_column))
                    .unique()
                    .to_owned(),
            )
            .await?;
        if mysql {
            manager
                .create_index(
                    Index::create()
                        .name("ix_subtitles_media_source")
                        .table(Alias::new("subtitles"))
                        .col(Alias::new("media_source_id"))
                        .to_owned(),
                )
                .await?;
        }
        manager
            .create_index(
                Index::create()
                    .name("uq_subtitles_source_object")
                    .table(Alias::new("subtitles"))
                    .col(Alias::new("media_source_id"))
                    .col(Alias::new("storage_object_id"))
                    .unique()
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let mysql = manager.get_connection().get_database_backend() == sea_orm::DbBackend::MySql;
        manager
            .drop_index(
                Index::drop()
                    .name("uq_subtitles_source_object")
                    .table(Alias::new("subtitles"))
                    .to_owned(),
            )
            .await?;
        manager
            .drop_index(
                Index::drop()
                    .name("uq_media_streams_identity")
                    .table(Alias::new("media_streams"))
                    .to_owned(),
            )
            .await?;
        if mysql {
            manager
                .alter_table(
                    Table::alter()
                        .table(Alias::new("media_streams"))
                        .drop_column(Alias::new("stream_identity_key"))
                        .to_owned(),
                )
                .await?;
        }
        for column in [
            "stream_identity",
            "delivery_index",
            "container_stream_index",
            "width",
            "height",
            "channels",
            "is_default",
            "is_forced",
            "is_external",
            "is_text",
        ] {
            manager
                .alter_table(
                    Table::alter()
                        .table(Alias::new("media_streams"))
                        .drop_column(Alias::new(column))
                        .to_owned(),
                )
                .await?;
        }
        manager
            .drop_index(
                Index::drop()
                    .name("uq_catalog_change_outbox_generation")
                    .table(Alias::new("catalog_change_outbox"))
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("uq_catalog_change_outbox_publication_event")
                    .table(Alias::new("catalog_change_outbox"))
                    .col(Alias::new("publication_id"))
                    .col(Alias::new("event_type"))
                    .unique()
                    .to_owned(),
            )
            .await?;
        if mysql {
            manager
                .drop_index(
                    Index::drop()
                        .name("ix_catalog_change_outbox_publication")
                        .table(Alias::new("catalog_change_outbox"))
                        .to_owned(),
                )
                .await?;
        }
        Ok(())
    }
}

fn nullable_boolean(name: &'static str) -> ColumnDef {
    let mut column = ColumnDef::new(Alias::new(name));
    column.boolean().null();
    column
}
