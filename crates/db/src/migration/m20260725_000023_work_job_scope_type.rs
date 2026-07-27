use sea_orm::{ConnectionTrait, DbBackend};
use sea_orm_migration::prelude::{
    Alias, ConditionalStatement, DbErr, DeriveMigrationName, Expr, Index, MigrationTrait,
    SchemaManager,
};

const TABLE: &str = "work_jobs";
const INDEX: &str = "uq_work_jobs_active";

#[derive(DeriveMigrationName)]
pub struct Migration;

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        recreate(manager, true).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        recreate(manager, false).await
    }
}

async fn recreate(manager: &SchemaManager<'_>, include_scope_type: bool) -> Result<(), DbErr> {
    manager
        .drop_index(
            Index::drop()
                .name(INDEX)
                .table(Alias::new(TABLE))
                .to_owned(),
        )
        .await?;
    let backend = manager.get_connection().get_database_backend();
    let mut index = Index::create();
    index.name(INDEX).table(Alias::new(TABLE));
    if include_scope_type {
        if backend == DbBackend::MySql {
            index.col((Alias::new("scope_type"), 32));
        } else {
            index.col(Alias::new("scope_type"));
        }
    }
    index.col(Alias::new("scope_id"));
    if backend == DbBackend::MySql {
        index
            .col((Alias::new("task_kind"), 32))
            .col(Alias::new("expected_revision"))
            .col((Alias::new("active_slot"), 16));
    } else {
        index
            .col(Alias::new("task_kind"))
            .col(Alias::new("expected_revision"));
        index.and_where(Expr::col(Alias::new("state")).is_in(["Pending", "Running"]));
    }
    manager.create_index(index.unique().to_owned()).await
}
