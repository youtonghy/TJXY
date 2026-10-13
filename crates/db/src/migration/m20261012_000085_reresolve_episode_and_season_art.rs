use std::collections::BTreeSet;

use sea_orm::{ConnectionTrait, TransactionTrait};
use sea_orm_migration::prelude::{
    Alias, Cond, DbErr, DeriveMigrationName, Expr, JoinType, MigrationTrait, Query, SchemaManager,
    SelectStatement,
};
use uuid::Uuid;

/// Metadata resolution used to ignore `<video>-thumb.jpg` episode art and the series folder's
/// `seasonNN-poster.jpg`/`seasonNN-fanart.jpg`. Image changes alone never advance an item's
/// metadata revision, so this re-resolves the projected episodes and seasons whose folders
/// hold such art; the next scan picks them up through the usual revision gate.
#[derive(DeriveMigrationName)]
pub struct Migration;

const IMAGE_EXTENSIONS: [&str; 6] = ["jpg", "jpeg", "png", "webp", "bmp", "gif"];
const UPDATE_CHUNK: usize = 500;

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        reresolve_items_with_art(manager).await
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        // A revision bump only schedules another resolution; there is nothing to restore.
        Ok(())
    }
}

/// Advances the metadata revision of every projected episode and season whose folders hold
/// episode thumbs or `seasonNN` art.
pub(super) async fn reresolve_items_with_art(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let transaction = manager.get_connection().begin().await?;
    let backend = transaction.get_database_backend();
    let mut items = BTreeSet::new();
    for query in [episodes_with_thumbs(), seasons_with_named_art()] {
        for row in transaction.query_all(backend.build(&query)).await? {
            items.insert(row.try_get::<Uuid>("", "catalog_item_id")?);
        }
    }
    // Selected first and updated by id: MySQL rejects an UPDATE whose subquery reads
    // `catalog_items` itself.
    let items = items.into_iter().collect::<Vec<_>>();
    let mut changed = false;
    for chunk in items.chunks(UPDATE_CHUNK) {
        let update = Query::update()
            .table(Alias::new("catalog_items"))
            .value(
                Alias::new("metadata_revision"),
                Expr::col(Alias::new("metadata_revision")).add(1),
            )
            .and_where(Expr::col(Alias::new("id")).is_in(chunk.iter().copied()))
            .and_where(Expr::col(Alias::new("is_present")).eq(true))
            .and_where(Expr::col(Alias::new("classification_state")).eq("Matched"))
            .to_owned();
        changed |= transaction
            .execute(backend.build(&update))
            .await?
            .rows_affected()
            > 0;
    }
    if changed {
        crate::advance_catalog_generation(&transaction).await?;
    }
    transaction.commit().await
}

fn name_matches(object: &Alias, patterns: &[&str]) -> Cond {
    let mut condition = Cond::any();
    for pattern in patterns {
        for extension in IMAGE_EXTENSIONS {
            condition = condition.add(
                Expr::col((object.clone(), Alias::new("normalized_name")))
                    .like(format!("{pattern}.{extension}")),
            );
        }
    }
    condition
}

/// Active structure projections of `item_type`, with each item's scope relation.
fn projected_items(item_type: &str) -> (SelectStatement, Alias) {
    let projection = Alias::new("art_projection");
    let owner = Alias::new("art_owner");
    let scope = Alias::new("art_scope");
    let mut query = Query::select();
    query
        .distinct()
        .expr_as(
            Expr::col((projection.clone(), Alias::new("catalog_item_id"))),
            Alias::new("catalog_item_id"),
        )
        .from_as(Alias::new("publication_catalog_items"), projection.clone())
        .join_as(
            JoinType::InnerJoin,
            Alias::new("catalog_items"),
            owner.clone(),
            Expr::col((owner, Alias::new("active_structure_publication_id")))
                .equals((projection.clone(), Alias::new("publication_id"))),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("storage_root_objects"),
            scope.clone(),
            Cond::all()
                .add(
                    Expr::col((scope.clone(), Alias::new("storage_root_id")))
                        .equals((projection.clone(), Alias::new("storage_root_id"))),
                )
                .add(
                    Expr::col((scope.clone(), Alias::new("storage_object_id")))
                        .equals((projection.clone(), Alias::new("scope_storage_object_id"))),
                ),
        )
        .and_where(Expr::col((projection, Alias::new("item_type"))).eq(item_type));
    (query, scope)
}

/// Joins the present files whose parent matches `parent` and whose name matches `names`.
fn join_present_files(query: &mut SelectStatement, scope: &Alias, parent: Cond, names: Cond) {
    let child = Alias::new("art_child");
    let object = Alias::new("art_object");
    query
        .join_as(
            JoinType::InnerJoin,
            Alias::new("storage_root_objects"),
            child.clone(),
            Cond::all()
                .add(
                    Expr::col((child.clone(), Alias::new("storage_root_id")))
                        .equals((scope.clone(), Alias::new("storage_root_id"))),
                )
                .add(parent),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("storage_objects"),
            object.clone(),
            Expr::col((object.clone(), Alias::new("id")))
                .equals((child, Alias::new("storage_object_id"))),
        )
        .and_where(Expr::col((object.clone(), Alias::new("object_type"))).eq("File"))
        .and_where(Expr::col((object, Alias::new("presence_state"))).eq("Present"))
        .cond_where(names);
}

fn episodes_with_thumbs() -> SelectStatement {
    let (mut query, scope) = projected_items("Episode");
    let child = Alias::new("art_child");
    join_present_files(
        &mut query,
        &scope,
        Cond::all().add(
            Expr::col((child, Alias::new("parent_storage_object_id")))
                .equals((scope.clone(), Alias::new("storage_object_id"))),
        ),
        name_matches(&Alias::new("art_object"), &["%-thumb"]),
    );
    query
}

fn seasons_with_named_art() -> SelectStatement {
    let (mut query, scope) = projected_items("Season");
    let child = Alias::new("art_child");
    // The season folder itself (flat layouts) or its series folder.
    join_present_files(
        &mut query,
        &scope,
        Cond::any()
            .add(
                Expr::col((child.clone(), Alias::new("parent_storage_object_id")))
                    .equals((scope.clone(), Alias::new("storage_object_id"))),
            )
            .add(
                Expr::col((child, Alias::new("parent_storage_object_id")))
                    .equals((scope.clone(), Alias::new("parent_storage_object_id"))),
            ),
        name_matches(
            &Alias::new("art_object"),
            &["season%-poster", "season%-fanart"],
        ),
    );
    query
}
