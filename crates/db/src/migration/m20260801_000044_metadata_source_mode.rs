use sea_orm_migration::{
    prelude::{
        Alias, DbErr, DeriveMigrationName, Expr, MigrationTrait, Query, SchemaManager, Table,
    },
    schema::{string, string_null},
    sea_orm::ConnectionTrait,
};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("libraries"))
                    .add_column(
                        string(Alias::new("metadata_source_mode")).default("automatic_scrape"),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("work_jobs"))
                    .add_column(string_null(Alias::new("metadata_source_mode")))
                    .to_owned(),
            )
            .await?;
        let connection = manager.get_connection();
        connection
            .execute(
                connection.get_database_backend().build(
                    Query::update()
                        .table(Alias::new("work_jobs"))
                        .value(Alias::new("metadata_source_mode"), "automatic_scrape")
                        .and_where(Expr::col(Alias::new("task_kind")).eq("ResolveMetadata")),
                ),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("work_jobs"))
                    .drop_column(Alias::new("metadata_source_mode"))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("libraries"))
                    .drop_column(Alias::new("metadata_source_mode"))
                    .to_owned(),
            )
            .await
    }
}
