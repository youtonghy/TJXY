use sea_orm_migration::{
    prelude::{Alias, DbErr, DeriveMigrationName, MigrationTrait, SchemaManager, Table},
    schema::big_integer,
};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("catalog_items"))
                    .add_column(
                        big_integer(Alias::new("metadata_resolved_revision"))
                            .not_null()
                            .default(-1),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("catalog_items"))
                    .drop_column(Alias::new("metadata_resolved_revision"))
                    .to_owned(),
            )
            .await
    }
}
