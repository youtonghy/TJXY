use sea_orm_migration::{
    prelude::{Alias, DbErr, DeriveMigrationName, MigrationTrait, SchemaManager, Table},
    schema::{integer_null, string_len_null},
};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for column in [
            string_len_null(Alias::new("profile"), 128),
            integer_null(Alias::new("level")),
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
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for column in ["level", "profile"] {
            manager
                .alter_table(
                    Table::alter()
                        .table(Alias::new("media_streams"))
                        .drop_column(Alias::new(column))
                        .to_owned(),
                )
                .await?;
        }
        Ok(())
    }
}
