use sea_orm::DbBackend;
use sea_orm_migration::{
    prelude::*,
    schema::{string_len, string_len_null},
};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if manager.get_database_backend() == DbBackend::Sqlite {
            return Ok(());
        }
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("libraries"))
                    .modify_column(string_len(Alias::new("local_metadata_access_mode"), 32))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("work_jobs"))
                    .modify_column(string_len_null(
                        Alias::new("local_metadata_access_mode"),
                        32,
                    ))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if manager.get_database_backend() == DbBackend::Sqlite {
            return Ok(());
        }
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("libraries"))
                    .modify_column(string_len(Alias::new("local_metadata_access_mode"), 16))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("work_jobs"))
                    .modify_column(string_len_null(
                        Alias::new("local_metadata_access_mode"),
                        16,
                    ))
                    .to_owned(),
            )
            .await
    }
}
