use sea_orm::{ConnectionTrait, DbBackend};
use sea_orm_migration::{
    prelude::{
        Alias, DbErr, DeriveMigrationName, Expr, Index, MigrationTrait, Query, SchemaManager, Table,
    },
    schema::string_null,
};

const TABLE: &str = "work_jobs";
const INDEX: &str = "uq_work_jobs_active";
const ACTIVE_SLOT: &str = "active_slot";

#[derive(DeriveMigrationName)]
pub struct Migration;

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if manager.get_connection().get_database_backend() != DbBackend::MySql {
            return Ok(());
        }
        drop_active_index(manager).await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new(TABLE))
                    .add_column(string_null(Alias::new(ACTIVE_SLOT)))
                    .to_owned(),
            )
            .await?;
        let active = Query::update()
            .table(Alias::new(TABLE))
            .value(Alias::new(ACTIVE_SLOT), "active")
            .and_where(Expr::col(Alias::new("state")).is_in(["Pending", "Running"]))
            .to_owned();
        let connection = manager.get_connection();
        connection
            .execute(connection.get_database_backend().build(&active))
            .await?;
        create_active_index(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if manager.get_connection().get_database_backend() != DbBackend::MySql {
            return Ok(());
        }
        drop_active_index(manager).await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new(TABLE))
                    .drop_column(Alias::new(ACTIVE_SLOT))
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name(INDEX)
                    .table(Alias::new(TABLE))
                    .col(Alias::new("scope_id"))
                    .col(Alias::new("task_kind"))
                    .col(Alias::new("expected_revision"))
                    .unique()
                    .to_owned(),
            )
            .await
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
    manager
        .create_index(
            Index::create()
                .name(INDEX)
                .table(Alias::new(TABLE))
                .col(Alias::new("scope_id"))
                .col(Alias::new("task_kind"))
                .col(Alias::new("expected_revision"))
                .col(Alias::new(ACTIVE_SLOT))
                .unique()
                .to_owned(),
        )
        .await
}
