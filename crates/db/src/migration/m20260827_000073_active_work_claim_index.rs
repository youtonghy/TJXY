use sea_orm::{ConnectionTrait, DbBackend};
use sea_orm_migration::prelude::{
    Alias, ConditionalStatement, DbErr, DeriveMigrationName, Expr, Index, MigrationTrait,
    SchemaManager,
};

const TABLE: &str = "work_jobs";
const LEGACY_INDEX: &str = "ix_work_jobs_claim";
const ACTIVE_INDEX: &str = "ix_work_jobs_claim_active";
const ACTIVE_STATES: [&str; 2] = ["Pending", "Running"];

#[derive(DeriveMigrationName)]
pub struct Migration;

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let backend = manager.get_connection().get_database_backend();
        if backend == DbBackend::MySql {
            return Ok(());
        }
        if !manager.has_index(TABLE, ACTIVE_INDEX).await? {
            manager.create_index(active_claim_index()).await?;
        }
        if manager.has_index(TABLE, LEGACY_INDEX).await? {
            manager.drop_index(drop_index(LEGACY_INDEX)).await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let backend = manager.get_connection().get_database_backend();
        if backend == DbBackend::MySql {
            return Ok(());
        }
        if !manager.has_index(TABLE, LEGACY_INDEX).await? {
            manager.create_index(legacy_claim_index()).await?;
        }
        if manager.has_index(TABLE, ACTIVE_INDEX).await? {
            manager.drop_index(drop_index(ACTIVE_INDEX)).await?;
        }
        Ok(())
    }
}

fn active_claim_index() -> sea_orm_migration::prelude::IndexCreateStatement {
    let mut index = Index::create();
    index
        .name(ACTIVE_INDEX)
        .table(Alias::new(TABLE))
        .col(Alias::new("state"))
        .col(Alias::new("task_kind"))
        .col(Alias::new("priority"))
        .col(Alias::new("available_at"))
        .col(Alias::new("lease_expires_at"))
        .col(Alias::new("created_at"))
        .and_where(Expr::col(Alias::new("state")).is_in(ACTIVE_STATES));
    index.clone()
}

fn legacy_claim_index() -> sea_orm_migration::prelude::IndexCreateStatement {
    Index::create()
        .name(LEGACY_INDEX)
        .table(Alias::new(TABLE))
        .col(Alias::new("state"))
        .col(Alias::new("task_kind"))
        .col(Alias::new("priority"))
        .col(Alias::new("available_at"))
        .col(Alias::new("lease_expires_at"))
        .col(Alias::new("created_at"))
        .to_owned()
}

fn drop_index(name: &str) -> sea_orm_migration::prelude::IndexDropStatement {
    Index::drop().name(name).table(Alias::new(TABLE)).to_owned()
}
