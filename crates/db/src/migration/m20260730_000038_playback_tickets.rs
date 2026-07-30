use sea_orm::{ConnectionTrait, DbBackend};
use sea_orm_migration::{
    prelude::*,
    schema::{blob, timestamp_with_time_zone, timestamp_with_time_zone_null, uuid},
};

const TOKEN_DIGEST_BYTES: u32 = 32;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let mysql = manager.get_connection().get_database_backend() == DbBackend::MySql;
        manager
            .create_table(
                Table::create()
                    .table(Alias::new("playback_tickets"))
                    .col(uuid(Alias::new("id")).primary_key().take())
                    .col(uuid(Alias::new("auth_session_id")))
                    .col(uuid(Alias::new("user_id")))
                    .col(uuid(Alias::new("item_id")))
                    .col(uuid(Alias::new("media_source_id")))
                    .col(uuid(Alias::new("play_session_id")))
                    .col(if mysql {
                        ColumnDef::new(Alias::new("token_digest"))
                            .var_binary(TOKEN_DIGEST_BYTES)
                            .not_null()
                            .take()
                    } else {
                        blob(Alias::new("token_digest"))
                    })
                    .col(timestamp_with_time_zone(Alias::new("expires_at")))
                    .col(timestamp_with_time_zone_null(Alias::new("revoked_at")))
                    .col(timestamp_with_time_zone(Alias::new("created_at")))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_playback_tickets_session")
                            .from(
                                Alias::new("playback_tickets"),
                                Alias::new("auth_session_id"),
                            )
                            .to(Alias::new("auth_sessions"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_playback_tickets_user")
                            .from(Alias::new("playback_tickets"), Alias::new("user_id"))
                            .to(Alias::new("users"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .to_owned(),
            )
            .await?;
        for index in playback_ticket_indexes() {
            manager.create_index(index).await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(Alias::new("playback_tickets"))
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}

fn playback_ticket_indexes() -> Vec<IndexCreateStatement> {
    vec![
        Index::create()
            .name("uq_playback_tickets_token_digest")
            .table(Alias::new("playback_tickets"))
            .col(Alias::new("token_digest"))
            .unique()
            .to_owned(),
        Index::create()
            .name("ix_playback_tickets_session_state")
            .table(Alias::new("playback_tickets"))
            .col(Alias::new("auth_session_id"))
            .col(Alias::new("revoked_at"))
            .col(Alias::new("expires_at"))
            .to_owned(),
    ]
}
