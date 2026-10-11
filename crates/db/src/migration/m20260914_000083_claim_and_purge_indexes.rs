use sea_orm::{ConnectionTrait, DbBackend};
use sea_orm_migration::prelude::{
    Alias, ConditionalStatement, DbErr, DeriveMigrationName, Expr, Index, IndexOrder,
    MigrationTrait, SchemaManager,
};

/// Claims read pending work one task kind at a time in claim order. With this partial index each
/// read is an ordered range that stops after a few rows instead of sorting the whole pending
/// backlog on every claim. `MySQL` has no partial indexes and keeps the legacy claim index.
const PENDING_INDEX: &str = "ix_work_jobs_claim_pending";
/// Queue maintenance and retention test whether a job owns catalog publications. The foreign key
/// alone leaves that lookup without an index, so each check scanned every publication.
const PUBLICATION_JOB_INDEX: &str = "ix_catalog_publications_job";

#[derive(DeriveMigrationName)]
pub struct Migration;

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_index(
                Index::create()
                    .name(PUBLICATION_JOB_INDEX)
                    .table(Alias::new("catalog_publications"))
                    .col(Alias::new("job_id"))
                    .to_owned(),
            )
            .await?;
        if manager.get_connection().get_database_backend() == DbBackend::MySql {
            return Ok(());
        }
        manager
            .create_index(
                Index::create()
                    .name(PENDING_INDEX)
                    .table(Alias::new("work_jobs"))
                    .col(Alias::new("task_kind"))
                    .col((Alias::new("priority"), IndexOrder::Desc))
                    .col(Alias::new("created_at"))
                    .col(Alias::new("id"))
                    .and_where(Expr::col(Alias::new("state")).eq("Pending"))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if manager.get_connection().get_database_backend() != DbBackend::MySql {
            manager
                .drop_index(
                    Index::drop()
                        .name(PENDING_INDEX)
                        .table(Alias::new("work_jobs"))
                        .to_owned(),
                )
                .await?;
        }
        manager
            .drop_index(
                Index::drop()
                    .name(PUBLICATION_JOB_INDEX)
                    .table(Alias::new("catalog_publications"))
                    .to_owned(),
            )
            .await
    }
}
