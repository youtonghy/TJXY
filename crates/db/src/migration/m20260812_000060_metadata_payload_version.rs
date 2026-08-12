use sea_orm_migration::{prelude::*, schema::integer};

use crate::metadata::METADATA_PAYLOAD_VERSION;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        ensure_schema(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if manager
            .has_column("catalog_items", "metadata_payload_version")
            .await?
        {
            manager
                .alter_table(
                    Table::alter()
                        .table(Alias::new("catalog_items"))
                        .drop_column(Alias::new("metadata_payload_version"))
                        .to_owned(),
                )
                .await?;
        }
        Ok(())
    }
}

pub(crate) async fn ensure_schema(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    if !manager
        .has_column("catalog_items", "metadata_payload_version")
        .await?
    {
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("catalog_items"))
                    .add_column(
                        integer(Alias::new("metadata_payload_version"))
                            .default(0_i32)
                            .check(Expr::col(Alias::new("metadata_payload_version")).gte(0_i32))
                            .take(),
                    )
                    .to_owned(),
            )
            .await?;
    }

    // Older releases could mark search-only TMDB results Ready. Re-open those records so
    // the current detail contract is fulfilled by Full scans or the next Lazy detail access.
    let item = Alias::new("catalog_items");
    let membership = Alias::new("metadata_payload_membership");
    let library = Alias::new("metadata_payload_library");
    let automatic_membership = Query::select()
        .expr(Expr::val(1_i32))
        .from_as(Alias::new("library_catalog_items"), membership.clone())
        .join_as(
            JoinType::InnerJoin,
            Alias::new("libraries"),
            library.clone(),
            Expr::col((library.clone(), Alias::new("id")))
                .equals((membership.clone(), Alias::new("library_id"))),
        )
        .and_where(
            Expr::col((membership, Alias::new("catalog_item_id")))
                .equals((item.clone(), Alias::new("id"))),
        )
        .and_where(Expr::col((library.clone(), Alias::new("is_enabled"))).eq(true))
        .and_where(Expr::col((library, Alias::new("metadata_source_mode"))).eq("automatic_scrape"))
        .to_owned();
    let update = Query::update()
        .table(Alias::new("catalog_items"))
        .value(Alias::new("metadata_state"), "Partial")
        .and_where(Expr::col(Alias::new("item_type")).is_in(["Movie", "Series"]))
        .and_where(Expr::col(Alias::new("metadata_payload_version")).lt(METADATA_PAYLOAD_VERSION))
        .and_where(Expr::col(Alias::new("metadata_state")).eq("Ready"))
        .and_where(Expr::exists(automatic_membership))
        .to_owned();
    manager
        .get_connection()
        .execute(manager.get_database_backend().build(&update))
        .await?;
    Ok(())
}
