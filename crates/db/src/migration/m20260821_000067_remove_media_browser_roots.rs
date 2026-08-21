use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("system_settings"))
                    .drop_column(Alias::new("media_browser_roots"))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("system_settings"))
                    .add_column(
                        ColumnDef::new(Alias::new("media_browser_roots"))
                            .json()
                            .null(),
                    )
                    .to_owned(),
            )
            .await
    }
}
