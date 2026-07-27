use std::collections::HashSet;

use sea_orm::{
    ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbErr, TransactionTrait,
    sea_query::{Alias, CaseStatement, Condition, Expr, JoinType, Query, SelectStatement},
};
use thiserror::Error;
use tjxy_common::{CatalogItemId, StorageObjectRecordId};
use uuid::Uuid;

use crate::{ClaimedOutboxEvent, OutboxCompletion, OutboxRepository, OutboxRepositoryError};

pub struct StorageChangeProjectionRepository<'connection> {
    database: &'connection DatabaseConnection,
}

impl<'connection> StorageChangeProjectionRepository<'connection> {
    #[must_use]
    pub const fn new(database: &'connection DatabaseConnection) -> Self {
        Self { database }
    }

    /// Applies one versioned storage event and completes its outbox claim atomically.
    ///
    /// # Errors
    ///
    /// Returns [`StorageChangeProjectionError`] for invalid payloads, SQL failures, or lost leases.
    pub async fn apply(
        &self,
        claimed: &ClaimedOutboxEvent,
    ) -> Result<OutboxCompletion, StorageChangeProjectionError> {
        validate_event(claimed)?;
        let transaction = self.database.begin().await?;
        let result = async {
            let mut catalog_changed = false;
            if matches!(
                claimed.event_type(),
                "Upserted" | "MovedOut" | "Removed" | "AncestorMovedOut" | "AncestorRemoved"
            ) {
                catalog_changed |= project_changed_relation(&transaction, claimed).await?;
                catalog_changed |= project_structure_scope(&transaction, claimed).await?;
                catalog_changed |=
                    project_location_and_catalog(&transaction, claimed, true).await?;
            } else if claimed.event_type() == "AvailabilityChanged" {
                catalog_changed |=
                    project_location_and_catalog(&transaction, claimed, false).await?;
            }
            if catalog_changed {
                crate::advance_catalog_generation(&transaction).await?;
            }
            OutboxRepository::new(self.database)
                .complete_in_transaction(&transaction, claimed)
                .await
                .map_err(Into::into)
        }
        .await;
        match result {
            Ok(completion) => {
                transaction.commit().await?;
                Ok(completion)
            }
            Err(original) => match transaction.rollback().await {
                Ok(()) => Err(original),
                Err(rollback) => Err(StorageChangeProjectionError::RollbackFailed {
                    original: original.to_string(),
                    rollback,
                }),
            },
        }
    }
}

#[allow(clippy::too_many_lines)] // The active-publication fence and all revision updates share one transaction.
async fn project_structure_scope(
    transaction: &DatabaseTransaction,
    claimed: &ClaimedOutboxEvent,
) -> Result<bool, StorageChangeProjectionError> {
    let parent_id = event_parent_id(claimed)?;
    let projection = Alias::new("changed_structure_projection");
    let publication = Alias::new("changed_structure_publication");
    let owner = Alias::new("changed_structure_owner");
    let item = Alias::new("changed_structure_item");
    let object = Alias::new("changed_structure_object");
    let relation = Alias::new("changed_structure_scope_relation");
    let query = Query::select()
        .distinct()
        .expr_as(
            Expr::col((owner.clone(), Alias::new("id"))),
            Alias::new("owner_catalog_item_id"),
        )
        .expr_as(
            Expr::col((item.clone(), Alias::new("id"))),
            Alias::new("catalog_item_id"),
        )
        .expr_as(
            Expr::col((item.clone(), Alias::new("item_type"))),
            Alias::new("item_type"),
        )
        .expr_as(
            Expr::col((object.clone(), Alias::new("normalized_name"))),
            Alias::new("normalized_name"),
        )
        .from_as(Alias::new("publication_catalog_items"), projection.clone())
        .join_as(
            JoinType::InnerJoin,
            Alias::new("catalog_publications"),
            publication.clone(),
            Expr::col((publication.clone(), Alias::new("id")))
                .equals((projection.clone(), Alias::new("publication_id"))),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("catalog_items"),
            owner.clone(),
            Expr::col((owner.clone(), Alias::new("id")))
                .equals((publication.clone(), Alias::new("owner_catalog_item_id"))),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("catalog_items"),
            item.clone(),
            Expr::col((item.clone(), Alias::new("id")))
                .equals((projection.clone(), Alias::new("catalog_item_id"))),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("storage_root_objects"),
            relation.clone(),
            Expr::col((relation.clone(), Alias::new("storage_root_id")))
                .equals((projection.clone(), Alias::new("storage_root_id")))
                .and(
                    Expr::col((relation.clone(), Alias::new("storage_object_id")))
                        .equals((projection.clone(), Alias::new("scope_storage_object_id"))),
                ),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("storage_objects"),
            object.clone(),
            Expr::col((object.clone(), Alias::new("id"))).eq(claimed.storage_object_id().as_uuid()),
        )
        .and_where(
            Expr::col((projection.clone(), Alias::new("storage_root_id")))
                .eq(claimed.storage_root_id().as_uuid()),
        )
        .and_where(
            Condition::any()
                .add(
                    Expr::col((projection.clone(), Alias::new("scope_storage_object_id")))
                        .eq(parent_id.as_uuid()),
                )
                .add(
                    Expr::col((projection, Alias::new("scope_storage_object_id")))
                        .eq(claimed.storage_object_id().as_uuid()),
                )
                .into(),
        )
        .and_where(Expr::col((publication.clone(), Alias::new("publication_kind"))).eq("Structure"))
        .and_where(Expr::col((publication.clone(), Alias::new("state"))).eq("Active"))
        .and_where(
            Expr::col((owner.clone(), Alias::new("active_structure_publication_id")))
                .equals((publication.clone(), Alias::new("id"))),
        )
        .and_where(
            Expr::col((item.clone(), Alias::new("structure_owner_item_id")))
                .equals((owner, Alias::new("id"))),
        )
        .and_where(Expr::col((item, Alias::new("is_present"))).eq(true))
        .to_owned();
    let backend = transaction.get_database_backend();
    let rows = transaction.query_all(backend.build(&query)).await?;
    let mut owners = HashSet::new();
    let mut items = HashSet::new();
    let mut changed = false;
    for row in rows {
        let owner_id: Uuid = row.try_get("", "owner_catalog_item_id")?;
        let item_id: Uuid = row.try_get("", "catalog_item_id")?;
        let item_type: String = row.try_get("", "item_type")?;
        let normalized_name: String = row.try_get("", "normalized_name")?;
        let nfo_changed = std::path::Path::new(&normalized_name)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("nfo"));
        owners.insert(owner_id);
        if !items.insert(item_id) {
            continue;
        }
        let mut update = Query::update();
        update.table(Alias::new("catalog_items"));
        if nfo_changed {
            update.value(
                Alias::new("metadata_revision"),
                Expr::col(Alias::new("metadata_revision")).add(1),
            );
        }
        if item_type == "Episode" {
            update
                .value(
                    Alias::new("source_index_revision"),
                    Expr::col(Alias::new("source_index_revision")).add(1),
                )
                .value(Alias::new("source_state"), "NotIndexed");
        } else if !nfo_changed {
            continue;
        }
        update.and_where(Expr::col(Alias::new("id")).eq(item_id));
        changed |= transaction
            .execute(backend.build(&update))
            .await?
            .rows_affected()
            == 1;
    }
    for owner_id in owners {
        let update = Query::update()
            .table(Alias::new("catalog_items"))
            .value(
                Alias::new("structure_expansion_revision"),
                Expr::col(Alias::new("structure_expansion_revision")).add(1),
            )
            .value(Alias::new("structure_state"), "NotExpanded")
            .and_where(Expr::col(Alias::new("id")).eq(owner_id))
            .to_owned();
        changed |= transaction
            .execute(backend.build(&update))
            .await?
            .rows_affected()
            == 1;
    }
    Ok(changed)
}

fn validate_event(claimed: &ClaimedOutboxEvent) -> Result<(), StorageChangeProjectionError> {
    if claimed.payload_version() != 1
        || !matches!(
            claimed.event_type(),
            "Upserted"
                | "MovedOut"
                | "Removed"
                | "AncestorMovedOut"
                | "AncestorRemoved"
                | "AvailabilityChanged"
                | "InventoryPageCommitted"
                | "ChangePageCommitted"
                | "ValidationCompleted"
        )
        || claimed
            .payload()
            .get("version")
            .and_then(serde_json::Value::as_i64)
            != Some(1)
        || claimed
            .payload()
            .get("kind")
            .and_then(serde_json::Value::as_str)
            != Some(claimed.event_type())
    {
        return Err(StorageChangeProjectionError::InvalidPayload);
    }
    Ok(())
}

#[allow(clippy::too_many_lines)] // Selection and revision updates must remain in the same transaction.
async fn project_changed_relation(
    transaction: &DatabaseTransaction,
    claimed: &ClaimedOutboxEvent,
) -> Result<bool, StorageChangeProjectionError> {
    let parent_id = event_parent_id(claimed)?;
    let relation = Alias::new("changed_parent_relation");
    let identity = Alias::new("changed_parent_identity");
    let item = Alias::new("changed_item");
    let object = Alias::new("changed_object");
    let query = Query::select()
        .expr_as(
            Expr::col((item.clone(), Alias::new("id"))),
            Alias::new("catalog_item_id"),
        )
        .expr_as(
            Expr::col((item.clone(), Alias::new("item_type"))),
            Alias::new("item_type"),
        )
        .expr_as(
            Expr::col((object.clone(), Alias::new("normalized_name"))),
            Alias::new("normalized_name"),
        )
        .from_as(Alias::new("identity_matches"), identity.clone())
        .join_as(
            JoinType::InnerJoin,
            Alias::new("storage_objects"),
            object.clone(),
            Expr::col((object, Alias::new("id"))).eq(claimed.storage_object_id().as_uuid()),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("storage_root_objects"),
            relation.clone(),
            Expr::col((relation.clone(), Alias::new("storage_object_id")))
                .equals((identity.clone(), Alias::new("storage_object_id"))),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("catalog_items"),
            item.clone(),
            Expr::col((item.clone(), Alias::new("id")))
                .equals((identity.clone(), Alias::new("candidate_catalog_item_id"))),
        )
        .and_where(
            Expr::col((identity.clone(), Alias::new("storage_object_id"))).eq(parent_id.as_uuid()),
        )
        .and_where(
            Expr::col((relation, Alias::new("storage_root_id")))
                .eq(claimed.storage_root_id().as_uuid()),
        )
        .and_where(Expr::col((identity, Alias::new("state"))).eq("Matched"))
        .and_where(Expr::col((item.clone(), Alias::new("is_present"))).eq(true))
        .and_where(Expr::col((item, Alias::new("classification_state"))).eq("Matched"))
        .to_owned();
    let backend = transaction.get_database_backend();
    let mut changed = false;
    for row in transaction.query_all(backend.build(&query)).await? {
        let id = CatalogItemId::from_uuid(row.try_get("", "catalog_item_id")?);
        let item_type: String = row.try_get("", "item_type")?;
        let normalized_name: String = row.try_get("", "normalized_name")?;
        let mut update = Query::update();
        update.table(Alias::new("catalog_items"));
        let nfo_changed = std::path::Path::new(&normalized_name)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("nfo"));
        if nfo_changed {
            update.value(
                Alias::new("metadata_revision"),
                Expr::col(Alias::new("metadata_revision")).add(1),
            );
        }
        match item_type.as_str() {
            "Movie" | "Episode" => {
                update
                    .value(
                        Alias::new("source_index_revision"),
                        Expr::col(Alias::new("source_index_revision")).add(1),
                    )
                    .value(Alias::new("source_state"), "NotIndexed");
            }
            "Series" => {
                update
                    .value(
                        Alias::new("structure_expansion_revision"),
                        Expr::col(Alias::new("structure_expansion_revision")).add(1),
                    )
                    .value(Alias::new("structure_state"), "NotExpanded");
            }
            _ if !nfo_changed => continue,
            _ => {}
        }
        update.and_where(Expr::col(Alias::new("id")).eq(id.as_uuid()));
        changed |= transaction
            .execute(backend.build(&update))
            .await?
            .rows_affected()
            == 1;
    }
    Ok(changed)
}

fn event_parent_id(
    claimed: &ClaimedOutboxEvent,
) -> Result<StorageObjectRecordId, StorageChangeProjectionError> {
    let value = claimed
        .payload()
        .pointer("/relation/parent_storage_object_id")
        .and_then(serde_json::Value::as_str)
        .ok_or(StorageChangeProjectionError::InvalidPayload)?;
    Uuid::parse_str(value)
        .map(StorageObjectRecordId::from_uuid)
        .map_err(|_| StorageChangeProjectionError::InvalidPayload)
}

async fn project_location_and_catalog(
    transaction: &DatabaseTransaction,
    claimed: &ClaimedOutboxEvent,
    invalidate_probe: bool,
) -> Result<bool, DbErr> {
    let object = claimed.storage_object_id().as_uuid();
    let Some(context) = transaction
        .query_one(
            transaction
                .get_database_backend()
                .build(&location_context_query(object)),
        )
        .await?
    else {
        return Ok(false);
    };
    let location_id: Uuid = context.try_get("", "location_id")?;
    let source_id: Uuid = context.try_get("", "media_source_id")?;
    let item_id: Uuid = context.try_get("", "catalog_item_id")?;
    let current_availability: String = context.try_get("", "availability_state")?;
    let current_probe: String = context.try_get("", "probe_state")?;
    let current_presence: bool = context.try_get("", "is_present")?;
    let presence = transaction
        .query_one(
            transaction
                .get_database_backend()
                .build(&storage_presence_query(object)),
        )
        .await?
        .ok_or_else(|| DbErr::Custom("storage presence query returned no row".into()))?
        .try_get::<String>("", "availability_state")?;
    let backend = transaction.get_database_backend();
    let mut changed = false;
    if current_availability != presence {
        changed |= transaction
            .execute(
                backend.build(
                    Query::update()
                        .table(Alias::new("media_locations"))
                        .value(Alias::new("availability_state"), presence)
                        .and_where(Expr::col(Alias::new("id")).eq(location_id)),
                ),
            )
            .await?
            .rows_affected()
            == 1;
    }
    if invalidate_probe && !matches!(current_probe.as_str(), "NotProbed" | "Stale") {
        changed |= transaction
            .execute(
                backend.build(
                    Query::update()
                        .table(Alias::new("media_sources"))
                        .value(Alias::new("probe_state"), "Stale")
                        .and_where(Expr::col(Alias::new("id")).eq(source_id)),
                ),
            )
            .await?
            .rows_affected()
            == 1;
    }
    let should_be_present = transaction
        .query_one(
            transaction
                .get_database_backend()
                .build(&catalog_presence_query(item_id)),
        )
        .await?
        .ok_or_else(|| DbErr::Custom("catalog presence query returned no row".into()))?
        .try_get::<bool>("", "is_present")?;
    if current_presence != should_be_present {
        changed |= transaction
            .execute(
                backend.build(
                    Query::update()
                        .table(Alias::new("catalog_items"))
                        .value(Alias::new("is_present"), should_be_present)
                        .and_where(Expr::col(Alias::new("id")).eq(item_id)),
                ),
            )
            .await?
            .rows_affected()
            == 1;
    }
    Ok(changed)
}

fn location_context_query(object_id: Uuid) -> SelectStatement {
    let location = Alias::new("location");
    let source = Alias::new("source");
    let item = Alias::new("item");
    Query::select()
        .expr_as(
            Expr::col((location.clone(), Alias::new("id"))),
            Alias::new("location_id"),
        )
        .column((location.clone(), Alias::new("media_source_id")))
        .column((source.clone(), Alias::new("catalog_item_id")))
        .column((location.clone(), Alias::new("availability_state")))
        .column((source.clone(), Alias::new("probe_state")))
        .column((item.clone(), Alias::new("is_present")))
        .from_as(Alias::new("media_locations"), location.clone())
        .join_as(
            JoinType::InnerJoin,
            Alias::new("media_sources"),
            source.clone(),
            Expr::col((source.clone(), Alias::new("id")))
                .equals((location.clone(), Alias::new("media_source_id"))),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("catalog_items"),
            item.clone(),
            Expr::col((item, Alias::new("id"))).equals((source, Alias::new("catalog_item_id"))),
        )
        .and_where(Expr::col((location, Alias::new("storage_object_id"))).eq(object_id))
        .to_owned()
}

fn storage_presence_query(object_id: Uuid) -> SelectStatement {
    let present = Query::select()
        .expr(Expr::val(1))
        .from(Alias::new("storage_root_objects"))
        .and_where(Expr::col(Alias::new("storage_object_id")).eq(object_id))
        .and_where(Expr::col(Alias::new("presence_state")).eq("Present"))
        .to_owned();
    let unavailable = Query::select()
        .expr(Expr::val(1))
        .from(Alias::new("storage_root_objects"))
        .and_where(Expr::col(Alias::new("storage_object_id")).eq(object_id))
        .and_where(Expr::col(Alias::new("presence_state")).eq("TemporarilyUnavailable"))
        .to_owned();
    Query::select()
        .expr_as(
            CaseStatement::new()
                .case(Expr::exists(present), "Available")
                .case(Expr::exists(unavailable), "TemporarilyUnavailable")
                .finally("ConfirmedAbsent"),
            Alias::new("availability_state"),
        )
        .to_owned()
}

fn catalog_presence_query(item_id: Uuid) -> SelectStatement {
    let source = Alias::new("source");
    let location = Alias::new("location");
    let available_location = Query::select()
        .expr(Expr::val(1))
        .from_as(Alias::new("media_sources"), source.clone())
        .join_as(
            JoinType::InnerJoin,
            Alias::new("media_locations"),
            location.clone(),
            Expr::col((location.clone(), Alias::new("media_source_id")))
                .equals((source.clone(), Alias::new("id"))),
        )
        .and_where(Expr::col((source, Alias::new("catalog_item_id"))).eq(item_id))
        .and_where(
            Expr::col((location, Alias::new("availability_state")))
                .is_in(["Available", "TemporarilyUnavailable"]),
        )
        .to_owned();
    Query::select()
        .expr_as(Expr::exists(available_location), Alias::new("is_present"))
        .to_owned()
}

#[derive(Debug, Error)]
pub enum StorageChangeProjectionError {
    #[error("storage outbox payload is unsupported or malformed")]
    InvalidPayload,
    #[error("storage outbox completion failed: {0}")]
    Outbox(#[from] OutboxRepositoryError),
    #[error("storage change projection query failed: {0}")]
    Database(#[from] DbErr),
    #[error("storage change projection rollback failed after {original}: {rollback}")]
    RollbackFailed { original: String, rollback: DbErr },
}

#[cfg(test)]
mod tests {
    use sea_orm::DbBackend;
    use uuid::Uuid;

    use super::{catalog_presence_query, location_context_query, storage_presence_query};

    #[test]
    fn projection_queries_use_backend_specific_bind_markers() {
        let object_id = Uuid::nil();
        let queries = [
            location_context_query(object_id),
            storage_presence_query(object_id),
            catalog_presence_query(object_id),
        ];

        for query in queries {
            let sqlite = DbBackend::Sqlite.build(&query);
            assert!(sqlite.sql.contains('?'));
            assert!(!sqlite.sql.contains("$1"));

            let postgres = DbBackend::Postgres.build(&query);
            assert!(postgres.sql.contains("$1"));
            assert!(!postgres.sql.contains('?'));
        }
    }
}
