use sea_orm_migration::prelude::{
    Alias, DbErr, DeriveMigrationName, Index, MigrationTrait, SchemaManager,
};

const TABLE: &str = "user_data";
const INDEX: &str = "ix_user_data_hybrid_signals";
const ITEM_FK_INDEX: &str = "ix_user_data_catalog_item";

#[derive(DeriveMigrationName)]
pub struct Migration;

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if !manager.has_index(TABLE, ITEM_FK_INDEX).await? {
            manager
                .create_index(
                    Index::create()
                        .name(ITEM_FK_INDEX)
                        .table(Alias::new(TABLE))
                        .col(Alias::new("catalog_item_id"))
                        .to_owned(),
                )
                .await?;
        }
        manager
            .create_index(
                Index::create()
                    .name(INDEX)
                    .table(Alias::new(TABLE))
                    .col(Alias::new("catalog_item_id"))
                    .col(Alias::new("is_played"))
                    .col(Alias::new("playback_position_ticks"))
                    .col(Alias::new("is_favorite"))
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
            .await
    }
}
