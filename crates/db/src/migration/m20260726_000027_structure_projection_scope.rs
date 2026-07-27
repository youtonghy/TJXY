use sea_orm::{ConnectionTrait, TransactionTrait};
use sea_orm_migration::{
    prelude::{
        Alias, DbErr, DeriveMigrationName, Expr, Index, MigrationTrait, Query, SchemaManager, Table,
    },
    schema::uuid_null,
};

const TABLE: &str = "publication_catalog_items";
const ROOT_COLUMN: &str = "storage_root_id";
const OBJECT_COLUMN: &str = "scope_storage_object_id";
const INDEX: &str = "ix_publication_catalog_items_scope";

#[derive(DeriveMigrationName)]
pub struct Migration;

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new(TABLE))
                    .add_column(uuid_null(Alias::new(ROOT_COLUMN)))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new(TABLE))
                    .add_column(uuid_null(Alias::new(OBJECT_COLUMN)))
                    .to_owned(),
            )
            .await?;
        retire_legacy_structure_projections(manager).await?;
        manager
            .create_index(
                Index::create()
                    .name(INDEX)
                    .table(Alias::new(TABLE))
                    .col(Alias::new(ROOT_COLUMN))
                    .col(Alias::new(OBJECT_COLUMN))
                    .col(Alias::new("publication_id"))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name(INDEX)
                    .table(Alias::new(TABLE))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new(TABLE))
                    .drop_column(Alias::new(OBJECT_COLUMN))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new(TABLE))
                    .drop_column(Alias::new(ROOT_COLUMN))
                    .to_owned(),
            )
            .await
    }
}

async fn retire_legacy_structure_projections(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let publication = Alias::new("catalog_publications");
    let owner = Alias::new("catalog_items");
    let live_structure_owners = Query::select()
        .column((publication.clone(), Alias::new("owner_catalog_item_id")))
        .from(publication.clone())
        .and_where(Expr::col((publication.clone(), Alias::new("publication_kind"))).eq("Structure"))
        .and_where(
            Expr::col((publication.clone(), Alias::new("state")))
                .is_in(["Building", "Ready", "Active"]),
        )
        .to_owned();
    let invalidate_owner = Query::update()
        .table(owner.clone())
        .value(
            Alias::new("active_structure_publication_id"),
            Expr::value(None::<uuid::Uuid>),
        )
        .value(
            Alias::new("structure_expansion_revision"),
            Expr::col(Alias::new("structure_expansion_revision")).add(1),
        )
        .value(Alias::new("structure_state"), "NotExpanded")
        .and_where(Expr::col((owner, Alias::new("id"))).in_subquery(live_structure_owners))
        .to_owned();
    let retire = Query::update()
        .table(publication.clone())
        .value(Alias::new("state"), "Retired")
        .value(Alias::new("retired_at"), Expr::current_timestamp())
        .and_where(Expr::col((publication.clone(), Alias::new("publication_kind"))).eq("Structure"))
        .and_where(
            Expr::col((publication, Alias::new("state"))).is_in(["Building", "Ready", "Active"]),
        )
        .to_owned();
    let connection = manager.get_connection();
    let transaction = connection.begin().await?;
    let backend = transaction.get_database_backend();
    let invalidated = transaction
        .execute(backend.build(&invalidate_owner))
        .await?
        .rows_affected();
    transaction.execute(backend.build(&retire)).await?;
    if invalidated > 0 {
        crate::advance_catalog_generation(&transaction).await?;
    }
    transaction.commit().await?;
    Ok(())
}
