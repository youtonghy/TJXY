use sea_orm_migration::{
    prelude::{Alias, DbErr, DeriveMigrationName, MigrationTrait, SchemaManager, Table},
    schema::string_len,
};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("ai_models"))
                    .add_column(
                        string_len(Alias::new("reasoning_effort"), 16)
                            .not_null()
                            .default("off"),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("ai_models"))
                    .drop_column(Alias::new("reasoning_effort"))
                    .to_owned(),
            )
            .await
    }
}
