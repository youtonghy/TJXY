use sea_orm::ConnectionTrait;
use sea_orm_migration::{
    prelude::{
        Alias, DbErr, DeriveMigrationName, Expr, MigrationTrait, Query, SchemaManager, Table,
    },
    schema::integer_null,
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
                    .add_column(integer_null(Alias::new("metadata_requirement")))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("catalog_items"))
                    .add_column(integer_null(Alias::new("metadata_resolved_requirement")))
                    .to_owned(),
            )
            .await?;

        let connection = manager.get_connection();
        let backend = connection.get_database_backend();
        connection
            .execute(
                backend.build(
                    Query::update()
                        .table(Alias::new("work_jobs"))
                        .value(Alias::new("metadata_requirement"), 1_i32)
                        .and_where(Expr::col(Alias::new("task_kind")).eq("ResolveMetadata")),
                ),
            )
            .await?;
        connection
            .execute(
                backend.build(
                    Query::update()
                        .table(Alias::new("catalog_items"))
                        .value(Alias::new("metadata_resolved_requirement"), 1_i32)
                        .and_where(Expr::col(Alias::new("metadata_resolved_revision")).gte(0_i64)),
                ),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("catalog_items"))
                    .drop_column(Alias::new("metadata_resolved_requirement"))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("work_jobs"))
                    .drop_column(Alias::new("metadata_requirement"))
                    .to_owned(),
            )
            .await
    }
}
