use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_index(
                Index::create()
                    .name("ix_playback_sessions_started_item_user")
                    .table(Alias::new("playback_sessions"))
                    .col(Alias::new("started_at"))
                    .col(Alias::new("catalog_item_id"))
                    .col(Alias::new("user_id"))
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("ix_playback_sessions_active_event")
                    .table(Alias::new("playback_sessions"))
                    .col(Alias::new("stopped_at"))
                    .col(Alias::new("last_event_at"))
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("ix_auth_sessions_created_id")
                    .table(Alias::new("auth_sessions"))
                    .col(Alias::new("created_at"))
                    .col(Alias::new("id"))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for (table, index) in [
            ("auth_sessions", "ix_auth_sessions_created_id"),
            ("playback_sessions", "ix_playback_sessions_active_event"),
            (
                "playback_sessions",
                "ix_playback_sessions_started_item_user",
            ),
        ] {
            manager
                .drop_index(
                    Index::drop()
                        .name(index)
                        .table(Alias::new(table))
                        .to_owned(),
                )
                .await?;
        }
        Ok(())
    }
}
