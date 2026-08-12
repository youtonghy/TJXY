use sea_orm_migration::{
    prelude::*,
    schema::{integer, integer_null, json_null},
};
use tjxy_common::MEDIA_NAME_PARSER_VERSION;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        ensure_schema(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for (table, column) in [
            ("publication_media_sources", "naming_hints"),
            ("media_sources", "naming_hints"),
            ("publication_catalog_items", "index_number"),
            ("catalog_publications", "naming_parser_version"),
            ("catalog_items", "naming_parser_version"),
            ("library_storage_roots", "naming_parser_version"),
        ] {
            if manager.has_column(table, column).await? {
                manager
                    .alter_table(
                        Table::alter()
                            .table(Alias::new(table))
                            .drop_column(Alias::new(column))
                            .to_owned(),
                    )
                    .await?;
            }
        }
        Ok(())
    }
}

pub(crate) async fn ensure_schema(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    for (table, column_name, column) in [
        (
            "library_storage_roots",
            "naming_parser_version",
            parser_version_column(),
        ),
        (
            "catalog_items",
            "naming_parser_version",
            parser_version_column(),
        ),
        (
            "catalog_publications",
            "naming_parser_version",
            parser_version_column(),
        ),
        (
            "publication_catalog_items",
            "index_number",
            integer_null(Alias::new("index_number")),
        ),
        (
            "media_sources",
            "naming_hints",
            json_null(Alias::new("naming_hints")),
        ),
        (
            "publication_media_sources",
            "naming_hints",
            json_null(Alias::new("naming_hints")),
        ),
    ] {
        if !manager.has_column(table, column_name).await? {
            manager
                .alter_table(
                    Table::alter()
                        .table(Alias::new(table))
                        .add_column(column)
                        .to_owned(),
                )
                .await?;
        }
    }
    let stale_shadow_publications = Query::delete()
        .from_table(Alias::new("catalog_publications"))
        .and_where(Expr::col(Alias::new("naming_parser_version")).lt(MEDIA_NAME_PARSER_VERSION))
        .and_where(Expr::col(Alias::new("state")).is_in(["Building", "Ready"]))
        .to_owned();
    manager
        .get_connection()
        .execute(
            manager
                .get_database_backend()
                .build(&stale_shadow_publications),
        )
        .await?;
    Ok(())
}

fn parser_version_column() -> ColumnDef {
    integer(Alias::new("naming_parser_version"))
        .default(0_i32)
        .check(Expr::col(Alias::new("naming_parser_version")).gte(0_i32))
        .take()
}
