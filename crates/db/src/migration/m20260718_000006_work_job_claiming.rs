use sea_orm_migration::{
    prelude::{Alias, DbErr, DeriveMigrationName, Index, MigrationTrait, SchemaManager, Table},
    schema::{big_integer_null, timestamp_with_time_zone_null},
};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("work_jobs"))
                    .add_column(timestamp_with_time_zone_null(Alias::new("available_at")))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("work_results"))
                    .add_column(big_integer_null(Alias::new("result_sync_revision")))
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("ix_work_jobs_claim")
                    .table(Alias::new("work_jobs"))
                    .col(Alias::new("state"))
                    .col(Alias::new("task_kind"))
                    .col(Alias::new("priority"))
                    .col(Alias::new("available_at"))
                    .col(Alias::new("lease_expires_at"))
                    .col(Alias::new("created_at"))
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("ix_work_jobs_claim")
                    .table(Alias::new("work_jobs"))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("work_jobs"))
                    .drop_column(Alias::new("available_at"))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("work_results"))
                    .drop_column(Alias::new("result_sync_revision"))
                    .to_owned(),
            )
            .await
    }
}
