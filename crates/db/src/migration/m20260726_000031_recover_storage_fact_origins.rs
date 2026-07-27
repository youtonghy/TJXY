use std::collections::{HashMap, HashSet};

use sea_orm::{ConnectionTrait, DatabaseTransaction, QueryResult, TransactionTrait};
use sea_orm_migration::prelude::{
    Alias, DbErr, DeriveMigrationName, Expr, JoinType, MigrationTrait, Query, SchemaManager,
};
use uuid::Uuid;

const RECOVERY_REASON: &str = "facts-origin-migration-required";

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(Default)]
struct CatalogInvalidation {
    metadata: bool,
    source: bool,
    structure: bool,
}

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let transaction = manager.get_connection().begin().await?;
        recover_fact_origins(&transaction).await?;
        transaction.commit().await
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        // Fact provenance cannot be reconstructed after the old ambiguous state is removed.
        Ok(())
    }
}

#[allow(clippy::too_many_lines)] // Recovery and all dependent revision invalidation share one transaction.
async fn recover_fact_origins(transaction: &DatabaseTransaction) -> Result<(), DbErr> {
    let backend = transaction.get_database_backend();
    let objects = transaction
        .query_all(
            backend.build(
                Query::select()
                    .columns([Alias::new("id"), Alias::new("normalized_name")])
                    .from(Alias::new("storage_objects"))
                    .and_where(Expr::col(Alias::new("facts_observed_storage_root_id")).is_null()),
            ),
        )
        .await?;
    let mut invalidations = HashMap::new();
    let mut catalog_changed = false;

    for object in objects {
        let object_id = migration_uuid(&object, "id")?;
        let normalized_name: String = object.try_get("", "normalized_name")?;
        let relations = object_relations(transaction, object_id).await?;
        if relations.len() == 1 {
            let root_id = migration_uuid(&relations[0], "storage_root_id")?;
            let update = Query::update()
                .table(Alias::new("storage_objects"))
                .value(Alias::new("facts_observed_storage_root_id"), root_id)
                .and_where(Expr::col(Alias::new("id")).eq(object_id))
                .and_where(Expr::col(Alias::new("facts_observed_storage_root_id")).is_null())
                .to_owned();
            transaction.execute(backend.build(&update)).await?;
            continue;
        }

        let mut affected_scopes = HashSet::from([object_id]);
        for relation in &relations {
            let root_id = migration_uuid(relation, "storage_root_id")?;
            if let Some(parent_id) = migration_optional_uuid(relation, "parent_storage_object_id")?
            {
                affected_scopes.insert(parent_id);
                let invalidate_parent = Query::update()
                    .table(Alias::new("storage_root_objects"))
                    .value(Alias::new("children_indexed"), false)
                    .and_where(Expr::col(Alias::new("storage_root_id")).eq(root_id))
                    .and_where(Expr::col(Alias::new("storage_object_id")).eq(parent_id))
                    .to_owned();
                transaction
                    .execute(backend.build(&invalidate_parent))
                    .await?;
            }
        }
        if !relations.is_empty() {
            let quarantine = Query::update()
                .table(Alias::new("storage_root_objects"))
                .value(Alias::new("presence_state"), "TemporarilyUnavailable")
                .value(Alias::new("availability_reason"), RECOVERY_REASON)
                .value(Alias::new("children_indexed"), false)
                .and_where(Expr::col(Alias::new("storage_object_id")).eq(object_id))
                .to_owned();
            transaction.execute(backend.build(&quarantine)).await?;
        }

        let nfo_changed = std::path::Path::new(&normalized_name)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("nfo"));
        collect_identity_invalidations(
            transaction,
            &affected_scopes,
            nfo_changed,
            &mut invalidations,
        )
        .await?;
        collect_location_invalidations(transaction, object_id, &mut invalidations).await?;
        collect_structure_invalidations(transaction, &affected_scopes, &mut invalidations).await?;
    }

    for (item_id, invalidation) in invalidations {
        let mut update = Query::update();
        update.table(Alias::new("catalog_items"));
        if invalidation.metadata {
            update.value(
                Alias::new("metadata_revision"),
                Expr::col(Alias::new("metadata_revision")).add(1),
            );
        }
        if invalidation.source {
            update
                .value(
                    Alias::new("source_index_revision"),
                    Expr::col(Alias::new("source_index_revision")).add(1),
                )
                .value(Alias::new("source_state"), "NotIndexed");
        }
        if invalidation.structure {
            update
                .value(
                    Alias::new("structure_expansion_revision"),
                    Expr::col(Alias::new("structure_expansion_revision")).add(1),
                )
                .value(Alias::new("structure_state"), "NotExpanded");
        }
        update.and_where(Expr::col(Alias::new("id")).eq(item_id));
        catalog_changed |= transaction
            .execute(backend.build(&update))
            .await?
            .rows_affected()
            == 1;
    }
    if catalog_changed {
        crate::advance_catalog_generation(transaction).await?;
    }
    Ok(())
}

async fn object_relations(
    transaction: &DatabaseTransaction,
    object_id: Uuid,
) -> Result<Vec<QueryResult>, DbErr> {
    let query = Query::select()
        .columns([
            Alias::new("storage_root_id"),
            Alias::new("parent_storage_object_id"),
        ])
        .from(Alias::new("storage_root_objects"))
        .and_where(Expr::col(Alias::new("storage_object_id")).eq(object_id))
        .to_owned();
    transaction
        .query_all(transaction.get_database_backend().build(&query))
        .await
}

async fn collect_identity_invalidations(
    transaction: &DatabaseTransaction,
    scopes: &HashSet<Uuid>,
    metadata_changed: bool,
    invalidations: &mut HashMap<Uuid, CatalogInvalidation>,
) -> Result<(), DbErr> {
    if scopes.is_empty() {
        return Ok(());
    }
    let identity = Alias::new("recovery_identity");
    let item = Alias::new("recovery_item");
    let query = Query::select()
        .distinct()
        .expr_as(
            Expr::col((item.clone(), Alias::new("id"))),
            Alias::new("catalog_item_id"),
        )
        .column((item.clone(), Alias::new("item_type")))
        .from_as(Alias::new("identity_matches"), identity.clone())
        .join_as(
            JoinType::InnerJoin,
            Alias::new("catalog_items"),
            item.clone(),
            Expr::col((item.clone(), Alias::new("id")))
                .equals((identity.clone(), Alias::new("candidate_catalog_item_id"))),
        )
        .and_where(
            Expr::col((identity.clone(), Alias::new("storage_object_id")))
                .is_in(scopes.iter().copied()),
        )
        .and_where(Expr::col((identity, Alias::new("state"))).eq("Matched"))
        .to_owned();
    let backend = transaction.get_database_backend();
    for row in transaction.query_all(backend.build(&query)).await? {
        let item_id = migration_uuid(&row, "catalog_item_id")?;
        let item_type: String = row.try_get("", "item_type")?;
        let source = matches!(item_type.as_str(), "Movie" | "Episode");
        let structure = item_type == "Series";
        if !metadata_changed && !source && !structure {
            continue;
        }
        let invalidation = invalidations.entry(item_id).or_default();
        invalidation.metadata |= metadata_changed;
        invalidation.source |= source;
        invalidation.structure |= structure;
    }
    Ok(())
}

async fn collect_location_invalidations(
    transaction: &DatabaseTransaction,
    object_id: Uuid,
    invalidations: &mut HashMap<Uuid, CatalogInvalidation>,
) -> Result<(), DbErr> {
    let location = Alias::new("recovery_location");
    let source = Alias::new("recovery_source");
    let query = Query::select()
        .distinct()
        .column((source.clone(), Alias::new("catalog_item_id")))
        .column((source.clone(), Alias::new("id")))
        .from_as(Alias::new("media_locations"), location.clone())
        .join_as(
            JoinType::InnerJoin,
            Alias::new("media_sources"),
            source.clone(),
            Expr::col((source.clone(), Alias::new("id")))
                .equals((location.clone(), Alias::new("media_source_id"))),
        )
        .and_where(Expr::col((location, Alias::new("storage_object_id"))).eq(object_id))
        .to_owned();
    let backend = transaction.get_database_backend();
    for row in transaction.query_all(backend.build(&query)).await? {
        let item_id = migration_uuid(&row, "catalog_item_id")?;
        let source_id = migration_uuid(&row, "id")?;
        invalidations.entry(item_id).or_default().source = true;
        let update = Query::update()
            .table(Alias::new("media_sources"))
            .value(Alias::new("probe_state"), "Stale")
            .and_where(Expr::col(Alias::new("id")).eq(source_id))
            .to_owned();
        transaction.execute(backend.build(&update)).await?;
    }
    let location_update = Query::update()
        .table(Alias::new("media_locations"))
        .value(Alias::new("availability_state"), "TemporarilyUnavailable")
        .and_where(Expr::col(Alias::new("storage_object_id")).eq(object_id))
        .to_owned();
    transaction.execute(backend.build(&location_update)).await?;
    Ok(())
}

async fn collect_structure_invalidations(
    transaction: &DatabaseTransaction,
    scopes: &HashSet<Uuid>,
    invalidations: &mut HashMap<Uuid, CatalogInvalidation>,
) -> Result<(), DbErr> {
    let projection = Alias::new("recovery_projection");
    let publication = Alias::new("recovery_publication");
    let query = Query::select()
        .distinct()
        .column((publication.clone(), Alias::new("owner_catalog_item_id")))
        .from_as(Alias::new("publication_catalog_items"), projection.clone())
        .join_as(
            JoinType::InnerJoin,
            Alias::new("catalog_publications"),
            publication.clone(),
            Expr::col((publication.clone(), Alias::new("id")))
                .equals((projection.clone(), Alias::new("publication_id"))),
        )
        .and_where(
            Expr::col((projection, Alias::new("scope_storage_object_id")))
                .is_in(scopes.iter().copied()),
        )
        .and_where(Expr::col((publication.clone(), Alias::new("publication_kind"))).eq("Structure"))
        .and_where(Expr::col((publication, Alias::new("state"))).eq("Active"))
        .to_owned();
    let backend = transaction.get_database_backend();
    for row in transaction.query_all(backend.build(&query)).await? {
        invalidations
            .entry(migration_uuid(&row, "owner_catalog_item_id")?)
            .or_default()
            .structure = true;
    }
    Ok(())
}

fn migration_uuid(row: &QueryResult, column: &str) -> Result<Uuid, DbErr> {
    match row.try_get::<Uuid>("", column) {
        Ok(value) => Ok(value),
        Err(uuid_error) => row
            .try_get::<String>("", column)
            .ok()
            .and_then(|value| Uuid::parse_str(&value).ok())
            .ok_or(uuid_error),
    }
}

fn migration_optional_uuid(row: &QueryResult, column: &str) -> Result<Option<Uuid>, DbErr> {
    match row.try_get::<Option<Uuid>>("", column) {
        Ok(value) => Ok(value),
        Err(uuid_error) => row
            .try_get::<Option<String>>("", column)
            .ok()
            .flatten()
            .map(|value| Uuid::parse_str(&value).map_err(|error| DbErr::Type(error.to_string())))
            .transpose()
            .map_err(|_| uuid_error),
    }
}
