use sea_orm_migration::{
    prelude::{Alias, DbErr, DeriveMigrationName, MigrationTrait, SchemaManager, Table},
    schema::{big_integer, text_null},
};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("users"))
                    .add_column(text_null(Alias::new("bio")))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("playback_sessions"))
                    .add_column(big_integer(Alias::new("watched_ticks")).default(0_i64))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("playback_sessions"))
                    .drop_column(Alias::new("watched_ticks"))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("users"))
                    .drop_column(Alias::new("bio"))
                    .to_owned(),
            )
            .await
    }
}
