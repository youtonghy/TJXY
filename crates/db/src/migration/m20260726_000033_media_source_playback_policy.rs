use sea_orm_migration::{
    prelude::{Alias, DbErr, DeriveMigrationName, Index, MigrationTrait, SchemaManager, Table},
    schema::{boolean, integer},
};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for column in [
            integer(Alias::new("admin_priority")).default(0),
            boolean(Alias::new("is_default")).default(false),
            boolean(Alias::new("is_hidden")).default(false),
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
        manager
            .create_index(
                Index::create()
                    .name("ix_media_sources_playback_policy")
                    .table(Alias::new("media_sources"))
                    .col(Alias::new("catalog_item_id"))
                    .col(Alias::new("is_hidden"))
                    .col(Alias::new("is_default"))
                    .col(Alias::new("admin_priority"))
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("ix_playback_sessions_user_item_started")
                    .table(Alias::new("playback_sessions"))
                    .col(Alias::new("user_id"))
                    .col(Alias::new("catalog_item_id"))
                    .col(Alias::new("started_at"))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("ix_media_sources_playback_policy")
                    .table(Alias::new("media_sources"))
                    .to_owned(),
            )
            .await?;
        manager
            .drop_index(
                Index::drop()
                    .name("ix_playback_sessions_user_item_started")
                    .table(Alias::new("playback_sessions"))
                    .to_owned(),
            )
            .await?;
        for column in ["is_hidden", "is_default", "admin_priority"] {
            manager
                .alter_table(
                    Table::alter()
                        .table(Alias::new("media_sources"))
                        .drop_column(Alias::new(column))
                        .to_owned(),
                )
                .await?;
        }
        Ok(())
    }
}
