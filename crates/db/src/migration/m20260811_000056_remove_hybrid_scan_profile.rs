use sea_orm::ConnectionTrait;
use sea_orm_migration::prelude::{
    Alias, DbErr, DeriveMigrationName, Expr, MigrationTrait, Query, SchemaManager,
};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let connection = manager.get_connection();
        let backend = connection.get_database_backend();

        connection
            .execute(
                backend.build(
                    Query::update()
                        .table(Alias::new("libraries"))
                        .value(Alias::new("scan_profile"), "Lazy")
                        .value(Alias::new("expansion_policy"), "on_browse")
                        .value(
                            Alias::new("profile_version"),
                            Expr::col(Alias::new("profile_version")).add(1_i32),
                        )
                        .and_where(Expr::col(Alias::new("scan_profile")).eq("Hybrid"))
                        .and_where(Expr::col(Alias::new("expansion_policy")).eq("background")),
                ),
            )
            .await?;

        connection
            .execute(
                backend.build(
                    Query::update()
                        .table(Alias::new("libraries"))
                        .value(Alias::new("scan_profile"), "Lazy")
                        .value(
                            Alias::new("profile_version"),
                            Expr::col(Alias::new("profile_version")).add(1_i32),
                        )
                        .and_where(Expr::col(Alias::new("scan_profile")).eq("Hybrid")),
                ),
            )
            .await?;

        connection
            .execute(
                backend.build(
                    Query::update()
                        .table(Alias::new("libraries"))
                        .value(Alias::new("expansion_policy"), "on_browse")
                        .value(
                            Alias::new("profile_version"),
                            Expr::col(Alias::new("profile_version")).add(1_i32),
                        )
                        .and_where(Expr::col(Alias::new("expansion_policy")).eq("background")),
                ),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        // The previous effective policy cannot be reconstructed after migration.
        Ok(())
    }
}
