use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

const TABLE: &str = "ai_provider_settings";
const DAILY_TOTAL: &str = "daily_total_token_limit";
const DAILY_USER: &str = "daily_user_token_limit";

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for column in [DAILY_TOTAL, DAILY_USER] {
            if !manager.has_column(TABLE, column).await? {
                manager
                    .alter_table(
                        Table::alter()
                            .table(Alias::new(TABLE))
                            .add_column(
                                ColumnDef::new(Alias::new(column))
                                    .big_integer()
                                    .not_null()
                                    .default(0_i64),
                            )
                            .to_owned(),
                    )
                    .await?;
            }
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for column in [DAILY_USER, DAILY_TOTAL] {
            if manager.has_column(TABLE, column).await? {
                manager
                    .alter_table(
                        Table::alter()
                            .table(Alias::new(TABLE))
                            .drop_column(Alias::new(column))
                            .to_owned(),
                    )
                    .await?;
            }
        }
        Ok(())
    }
}
