use sea_orm_migration::{
    prelude::*,
    schema::{big_integer, string_len, string_len_null, timestamp_with_time_zone_null},
};
#[derive(DeriveMigrationName)]
pub struct Migration;
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Retain the legacy mode column and its CHECK so an older binary can read the row.
        for column in [
            string_len_null(Alias::new("configured_mode"), 16),
            string_len(Alias::new("normal_mode"), 16)
                .default("Info")
                .to_owned(),
            timestamp_with_time_zone_null(Alias::new("debug_expires_at")),
            big_integer(Alias::new("max_file_bytes"))
                .default(33_554_432_i64)
                .to_owned(),
            big_integer(Alias::new("max_directory_bytes"))
                .default(268_435_456_i64)
                .to_owned(),
        ] {
            manager
                .alter_table(
                    Table::alter()
                        .table(Alias::new("logging_settings"))
                        .add_column(column)
                        .to_owned(),
                )
                .await?;
        }
        let connection = manager.get_connection();
        connection
            .execute(
                connection.get_database_backend().build(
                    Query::update()
                        .table(Alias::new("logging_settings"))
                        .value(
                            Alias::new("debug_expires_at"),
                            chrono::Utc::now() + chrono::Duration::minutes(30),
                        )
                        .and_where(Expr::col(Alias::new("mode")).eq("Debug")),
                ),
            )
            .await?;
        Ok(())
    }
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for column in [
            "configured_mode",
            "normal_mode",
            "debug_expires_at",
            "max_file_bytes",
            "max_directory_bytes",
        ] {
            manager
                .alter_table(
                    Table::alter()
                        .table(Alias::new("logging_settings"))
                        .drop_column(Alias::new(column))
                        .to_owned(),
                )
                .await?;
        }
        Ok(())
    }
}
