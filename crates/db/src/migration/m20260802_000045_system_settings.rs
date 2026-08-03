use sea_orm_migration::{
    prelude::*,
    schema::{big_integer, integer, string_len, timestamp_with_time_zone},
};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Alias::new("system_settings"))
                    .col(integer(Alias::new("id")).primary_key())
                    .col(string_len(Alias::new("locale"), 16))
                    .col(
                        big_integer(Alias::new("revision"))
                            .check(Expr::col(Alias::new("revision")).gt(0_i64)),
                    )
                    .col(timestamp_with_time_zone(Alias::new("created_at")))
                    .col(timestamp_with_time_zone(Alias::new("updated_at")))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(Alias::new("system_settings"))
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}
