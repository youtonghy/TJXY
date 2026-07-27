use sea_orm_migration::{
    prelude::{
        Alias, DbErr, DeriveMigrationName, Expr, Index, MigrationTrait, SchemaManager, Table,
    },
    schema::timestamp_with_time_zone,
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
                        timestamp_with_time_zone(Alias::new("date_created"))
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("ix_catalog_items_latest")
                    .table(Alias::new("catalog_items"))
                    .col(Alias::new("item_type"))
                    .col(Alias::new("is_present"))
                    .col(Alias::new("classification_state"))
                    .col(Alias::new("date_created"))
                    .col(Alias::new("id"))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("ix_catalog_items_latest")
                    .table(Alias::new("catalog_items"))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("catalog_items"))
                    .drop_column(Alias::new("date_created"))
                    .to_owned(),
            )
            .await
    }
}
