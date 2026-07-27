use sea_orm_migration::{
    prelude::{
        Alias, DbErr, DeriveMigrationName, ForeignKey, Index, MigrationTrait, SchemaManager, Table,
    },
    schema::{big_integer, timestamp_with_time_zone, timestamp_with_time_zone_null, uuid},
};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Alias::new("playback_sessions"))
                    .col(uuid(Alias::new("id")).primary_key().take())
                    .col(uuid(Alias::new("auth_session_id")))
                    .col(uuid(Alias::new("play_session_id")))
                    .col(uuid(Alias::new("user_id")))
                    .col(uuid(Alias::new("catalog_item_id")))
                    .col(uuid(Alias::new("presentation_key")))
                    .col(big_integer(Alias::new("last_position_ticks")))
                    .col(timestamp_with_time_zone(Alias::new("started_at")))
                    .col(timestamp_with_time_zone(Alias::new("last_event_at")))
                    .col(timestamp_with_time_zone_null(Alias::new("stopped_at")))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_playback_sessions_auth_session")
                            .from(
                                Alias::new("playback_sessions"),
                                Alias::new("auth_session_id"),
                            )
                            .to(Alias::new("auth_sessions"), Alias::new("id")),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_playback_sessions_user")
                            .from(Alias::new("playback_sessions"), Alias::new("user_id"))
                            .to(Alias::new("users"), Alias::new("id")),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_playback_sessions_item")
                            .from(
                                Alias::new("playback_sessions"),
                                Alias::new("catalog_item_id"),
                            )
                            .to(Alias::new("catalog_items"), Alias::new("id")),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("uq_playback_sessions_request_identity")
                    .table(Alias::new("playback_sessions"))
                    .col(Alias::new("auth_session_id"))
                    .col(Alias::new("play_session_id"))
                    .unique()
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("ix_playback_sessions_user_active")
                    .table(Alias::new("playback_sessions"))
                    .col(Alias::new("user_id"))
                    .col(Alias::new("stopped_at"))
                    .col(Alias::new("last_event_at"))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(Alias::new("playback_sessions"))
                    .to_owned(),
            )
            .await
    }
}
