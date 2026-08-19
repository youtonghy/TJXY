use sea_orm::DbErr;
use sea_orm_migration::prelude::{
    Alias, ColumnDef, DeriveMigrationName, MigrationTrait, SchemaManager, Table,
};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for table in ["media_sources", "publication_media_sources"] {
            manager
                .alter_table(
                    Table::alter()
                        .table(Alias::new(table))
                        .add_column(
                            ColumnDef::new(Alias::new("locator_kind"))
                                .string_len(16)
                                .not_null()
                                .default("storage"),
                        )
                        .to_owned(),
                )
                .await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for table in ["publication_media_sources", "media_sources"] {
            manager
                .alter_table(
                    Table::alter()
                        .table(Alias::new(table))
                        .drop_column(Alias::new("locator_kind"))
                        .to_owned(),
                )
                .await?;
        }
        Ok(())
    }
}
