use sea_orm::{ConnectionTrait, DbBackend, Statement};
use sea_orm_migration::prelude::{
    Alias, ConditionalStatement, DbErr, DeriveMigrationName, Expr, Index, MigrationTrait,
    SchemaManager,
};

const TABLE: &str = "work_jobs";
const LEGACY_INDEX: &str = "ix_work_jobs_claim";
const ACTIVE_INDEX: &str = "ix_work_jobs_claim_active";
const ACTIVE_STATES: [&str; 2] = ["Pending", "Running"];
const CREATE_ACTIVE_INDEX_CONCURRENTLY: &str = "CREATE INDEX CONCURRENTLY ix_work_jobs_claim_active \
    ON work_jobs (state, task_kind, priority, available_at, lease_expires_at, created_at) \
    WHERE state IN ('Pending', 'Running')";
const DROP_LEGACY_INDEX_CONCURRENTLY: &str = "DROP INDEX CONCURRENTLY ix_work_jobs_claim";
const CREATE_LEGACY_INDEX_CONCURRENTLY: &str = "CREATE INDEX CONCURRENTLY ix_work_jobs_claim \
    ON work_jobs (state, task_kind, priority, available_at, lease_expires_at, created_at)";
const DROP_ACTIVE_INDEX_CONCURRENTLY: &str = "DROP INDEX CONCURRENTLY ix_work_jobs_claim_active";

#[derive(DeriveMigrationName)]
pub struct Migration;

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let backend = manager.get_connection().get_database_backend();
        if backend == DbBackend::MySql {
            return Ok(());
        }
        if backend == DbBackend::Postgres {
            return execute_postgres_concurrently(
                manager,
                &[
                    CREATE_ACTIVE_INDEX_CONCURRENTLY,
                    DROP_LEGACY_INDEX_CONCURRENTLY,
                ],
            )
            .await;
        }
        manager.create_index(active_claim_index()).await?;
        manager.drop_index(drop_index(LEGACY_INDEX)).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let backend = manager.get_connection().get_database_backend();
        if backend == DbBackend::MySql {
            return Ok(());
        }
        if backend == DbBackend::Postgres {
            return execute_postgres_concurrently(
                manager,
                &[
                    CREATE_LEGACY_INDEX_CONCURRENTLY,
                    DROP_ACTIVE_INDEX_CONCURRENTLY,
                ],
            )
            .await;
        }
        manager.create_index(legacy_claim_index()).await?;
        manager.drop_index(drop_index(ACTIVE_INDEX)).await
    }
}

async fn execute_postgres_concurrently(
    manager: &SchemaManager<'_>,
    statements: &[&str],
) -> Result<(), DbErr> {
    let connection = manager.get_connection();
    for statement in statements {
        connection
            .execute(Statement::from_string(
                DbBackend::Postgres,
                (*statement).to_owned(),
            ))
            .await?;
    }
    Ok(())
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
