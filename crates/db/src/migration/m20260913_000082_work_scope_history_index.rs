use sea_orm_migration::prelude::*;

/// Full Scan looks up the latest terminal source-index job for an item before enqueueing another
/// one. The active-job natural key index only covers Pending and Running rows, so without this
/// index that lookup read every `work_jobs` row for each unindexed item on every refresh.
const TABLE: &str = "work_jobs";
const INDEX: &str = "ix_work_jobs_scope_history";

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
                    .col(Alias::new("scope_id"))
                    .col(Alias::new("task_kind"))
                    .col(Alias::new("expected_revision"))
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
