use sea_orm_migration::{
    prelude::*,
    schema::{big_integer_null, string_len_null, text_null},
};

const TABLE: &str = "filesystem_storage_configs";

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for column in [
            string_len_null(Alias::new("path_index_state"), 32),
            string_len_null(Alias::new("verified_physical_root_identity"), 256),
            string_len_null(Alias::new("pending_physical_root_identity"), 256),
            big_integer_null(Alias::new("path_index_revision")),
            text_null(Alias::new("path_index_error")),
        ] {
            manager
                .alter_table(
                    Table::alter()
                        .table(Alias::new(TABLE))
                        .add_column(column)
                        .to_owned(),
                )
                .await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for column in [
            "path_index_error",
            "path_index_revision",
            "pending_physical_root_identity",
            "verified_physical_root_identity",
            "path_index_state",
        ] {
            manager
                .alter_table(
                    Table::alter()
                        .table(Alias::new(TABLE))
                        .drop_column(Alias::new(column))
                        .to_owned(),
                )
                .await?;
        }
        Ok(())
    }
}
