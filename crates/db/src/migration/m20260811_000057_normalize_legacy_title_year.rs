use sea_orm::{ConnectionTrait, QueryResult, TransactionTrait};
use sea_orm_migration::prelude::{
    Alias, Cond, DbErr, DeriveMigrationName, Expr, JoinType, MigrationTrait, Query, SchemaManager,
};
use tjxy_common::SortKey;
use uuid::Uuid;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        normalize_legacy_titles(manager).await
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        // Recombining normalized titles would overwrite metadata resolved after this migration.
        Ok(())
    }
}

async fn normalize_legacy_titles(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let transaction = manager.get_connection().begin().await?;
    let backend = transaction.get_database_backend();
    let mut changed = false;
    let item = Alias::new("legacy_title_item");
    let provider = Alias::new("legacy_title_provider");
    let provenance = Alias::new("legacy_title_provenance");
    let candidates = transaction
        .query_all(
            backend.build(
                Query::select()
                    .columns([
                        (item.clone(), Alias::new("id")),
                        (item.clone(), Alias::new("name")),
                    ])
                    .from_as(Alias::new("catalog_items"), item.clone())
                    .join_as(
                        JoinType::LeftJoin,
                        Alias::new("provider_ids"),
                        provider.clone(),
                        Expr::col((provider.clone(), Alias::new("catalog_item_id")))
                            .equals((item.clone(), Alias::new("id"))),
                    )
                    .join_as(
                        JoinType::LeftJoin,
                        Alias::new("metadata_provenance"),
                        provenance.clone(),
                        Cond::all()
                            .add(
                                Expr::col((provenance.clone(), Alias::new("catalog_item_id")))
                                    .equals((item.clone(), Alias::new("id"))),
                            )
                            .add(
                                Expr::col((provenance.clone(), Alias::new("field_name")))
                                    .eq("title"),
                            )
                            .add(
                                Expr::col((provenance.clone(), Alias::new("source_provider")))
                                    .ne("Naming"),
                            ),
                    )
                    .and_where(
                        Expr::col((item.clone(), Alias::new("item_type")))
                            .is_in(["Movie", "Series"]),
                    )
                    .and_where(
                        Expr::col((item.clone(), Alias::new("classification_state"))).eq("Matched"),
                    )
                    .and_where(
                        Expr::col((item.clone(), Alias::new("metadata_state"))).eq("Partial"),
                    )
                    .and_where(Expr::col((item, Alias::new("production_year"))).is_null())
                    .and_where(Expr::col((provider, Alias::new("id"))).is_null())
                    .and_where(Expr::col((provenance, Alias::new("id"))).is_null()),
            ),
        )
        .await?;
    for candidate in candidates {
        if normalize_candidate(&transaction, &candidate).await? {
            changed = true;
        }
    }
    if changed {
        crate::advance_catalog_generation(&transaction).await?;
    }
    transaction.commit().await
}

async fn normalize_candidate(
    transaction: &sea_orm::DatabaseTransaction,
    candidate: &QueryResult,
) -> Result<bool, DbErr> {
    let id: Uuid = candidate.try_get("", "id")?;
    let stored_name: String = candidate.try_get("", "name")?;
    let Some((name, year)) = crate::title_year::split_title_year(&stored_name) else {
        return Ok(false);
    };
    let backend = transaction.get_database_backend();
    let update = Query::update()
        .table(Alias::new("catalog_items"))
        .value(Alias::new("name"), name)
        .value(Alias::new("sort_name"), name.to_lowercase())
        .value(
            Alias::new("sort_key"),
            SortKey::from_text(name).into_bytes(),
        )
        .value(Alias::new("production_year"), year)
        .value(
            Alias::new("metadata_revision"),
            Expr::col(Alias::new("metadata_revision")).add(1_i64),
        )
        .value(Alias::new("last_error"), Option::<String>::None)
        .and_where(Expr::col(Alias::new("id")).eq(id))
        .and_where(Expr::col(Alias::new("production_year")).is_null())
        .to_owned();
    Ok(transaction
        .execute(backend.build(&update))
        .await?
        .rows_affected()
        == 1)
}
