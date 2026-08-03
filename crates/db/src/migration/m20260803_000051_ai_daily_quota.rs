use sea_orm_migration::{
    prelude::*,
    schema::{big_integer, string_len, timestamp_with_time_zone, uuid},
};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Alias::new("ai_daily_usage"))
                    .col(uuid(Alias::new("id")).primary_key().take())
                    .col(uuid(Alias::new("user_id")))
                    .col(string_len(Alias::new("day_key"), 10))
                    .col(big_integer(Alias::new("request_count")).default(0))
                    .col(timestamp_with_time_zone(Alias::new("created_at")))
                    .col(timestamp_with_time_zone(Alias::new("updated_at")))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_ai_daily_usage_user")
                            .from(Alias::new("ai_daily_usage"), Alias::new("user_id"))
                            .to(Alias::new("users"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("uq_ai_daily_usage_user_day")
                    .table(Alias::new("ai_daily_usage"))
                    .col(Alias::new("user_id"))
                    .col(Alias::new("day_key"))
                    .unique()
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("ix_ai_daily_usage_user")
                    .table(Alias::new("ai_daily_usage"))
                    .col(Alias::new("user_id"))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(Alias::new("ai_daily_usage"))
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}
