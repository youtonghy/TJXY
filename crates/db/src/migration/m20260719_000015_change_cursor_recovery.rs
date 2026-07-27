use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("storage_sync_cursors"))
                    .add_column(ColumnDef::new(Alias::new("recovery_job_id")).uuid().null())
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("ix_storage_sync_cursors_status")
                    .table(Alias::new("storage_sync_cursors"))
                    .col(Alias::new("status"))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("ix_storage_sync_cursors_status")
                    .table(Alias::new("storage_sync_cursors"))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("storage_sync_cursors"))
                    .drop_column(Alias::new("recovery_job_id"))
                    .to_owned(),
            )
            .await
    }
}
