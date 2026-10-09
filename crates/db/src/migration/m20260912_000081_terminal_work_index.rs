use sea_orm_migration::prelude::*;

/// Retention looks for terminal jobs older than its cutoff that were never enrolled. Without an
/// index on the completion time that anti-join read every `work_jobs` row each time the worker
/// went idle (about once a minute).
const TABLE: &str = "work_jobs";
const INDEX: &str = "ix_work_jobs_state_completed";

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_index(
                Index::create()
                    .name(INDEX)
                    .table(Alias::new(TABLE))
                    .col(Alias::new("state"))
                    .col(Alias::new("completed_at"))
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
