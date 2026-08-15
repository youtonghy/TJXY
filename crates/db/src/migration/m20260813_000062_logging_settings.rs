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
                    .table(Alias::new("logging_settings"))
                    .if_not_exists()
                    .col(integer(Alias::new("id")).primary_key())
                    .col(string_len(Alias::new("mode"), 16).default("Error"))
                    .col(integer(Alias::new("retention_days")).default(30))
                    .col(big_integer(Alias::new("revision")).default(1_i64))
                    .col(timestamp_with_time_zone(Alias::new("created_at")))
                    .col(timestamp_with_time_zone(Alias::new("updated_at")))
                    .check(Expr::col(Alias::new("mode")).is_in(["Error", "Debug"]))
                    .check(Expr::col(Alias::new("retention_days")).between(1, 365))
                    .check(Expr::col(Alias::new("revision")).gt(0_i64))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(Alias::new("logging_settings"))
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}
