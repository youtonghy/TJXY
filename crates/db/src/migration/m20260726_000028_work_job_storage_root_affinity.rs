use sea_orm::{ConnectionTrait, DbBackend};
use sea_orm_migration::prelude::{
    Alias, ConditionalStatement, DbErr, DeriveMigrationName, Expr, Index, MigrationTrait,
    SchemaManager, Table,
};
const TABLE: &str = "work_jobs";
const COLUMN: &str = "storage_root_affinity";
const INDEX: &str = "uq_work_jobs_active";

#[derive(DeriveMigrationName)]
pub struct Migration;

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_active_index(manager).await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new(TABLE))
                    .add_column(super::uuid_with_nil_default(
                        manager.get_connection().get_database_backend(),
                        Alias::new(COLUMN),
                    ))
                    .to_owned(),
            )
            .await?;
        create_active_index(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_active_index(manager).await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new(TABLE))
                    .drop_column(Alias::new(COLUMN))
                    .to_owned(),
            )
            .await?;
        create_active_index(manager).await
    }
}

async fn drop_active_index(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .drop_index(
            Index::drop()
                .name(INDEX)
                .table(Alias::new(TABLE))
                .to_owned(),
        )
        .await
}

async fn create_active_index(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let backend = manager.get_connection().get_database_backend();
    let mut index = Index::create();
    index.name(INDEX).table(Alias::new(TABLE));
    if backend == DbBackend::MySql {
        index.col((Alias::new("scope_type"), 32));
    } else {
        index.col(Alias::new("scope_type"));
    }
    index.col(Alias::new("scope_id"));
    if backend == DbBackend::MySql {
        index.col((Alias::new("task_kind"), 32));
    } else {
        index.col(Alias::new("task_kind"));
    }
    index.col(Alias::new("expected_revision"));
    if backend == DbBackend::MySql {
        index.col((Alias::new("active_slot"), 16));
    } else {
        index.and_where(Expr::col(Alias::new("state")).is_in(["Pending", "Running"]));
    }
    manager.create_index(index.unique().to_owned()).await
}
