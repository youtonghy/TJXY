use sea_orm_migration::{
    prelude::{Alias, DbErr, DeriveMigrationName, Index, MigrationTrait, SchemaManager, Table},
    schema::timestamp_with_time_zone_null,
};

const TABLE: &str = "library_catalog_items";
const COLUMN: &str = "hybrid_admin_selected_at";
const INDEX: &str = "ix_library_catalog_items_hybrid_admin";

#[derive(DeriveMigrationName)]
pub struct Migration;

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new(TABLE))
                    .add_column(timestamp_with_time_zone_null(Alias::new(COLUMN)))
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name(INDEX)
                    .table(Alias::new(TABLE))
                    .col(Alias::new("library_id"))
                    .col(Alias::new(COLUMN))
                    .col(Alias::new("catalog_item_id"))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name(INDEX)
                    .table(Alias::new(TABLE))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new(TABLE))
                    .drop_column(Alias::new(COLUMN))
                    .to_owned(),
            )
            .await
    }
}
