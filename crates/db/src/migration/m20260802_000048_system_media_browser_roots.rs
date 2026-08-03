use sea_orm_migration::{prelude::*, schema::json_null};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("system_settings"))
                    .add_column(json_null(Alias::new("media_browser_roots")))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("system_settings"))
                    .drop_column(Alias::new("media_browser_roots"))
                    .to_owned(),
            )
            .await
    }
}
