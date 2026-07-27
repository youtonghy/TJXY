use std::collections::HashSet;

use sea_orm::{
    ConnectionTrait, DbErr,
    sea_query::{Alias, Expr, Query},
};
use tjxy_common::{StorageObjectRecordId, StorageRootId};
use uuid::Uuid;

const MAX_STORAGE_PATH_DEPTH: usize = 4096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StoragePathAvailability {
    Present,
    Playback,
}

#[allow(clippy::too_many_lines)] // Fact provenance and every root-local ancestor must be checked together.
pub(crate) async fn storage_path_is_authorized(
    connection: &impl ConnectionTrait,
    root_id: StorageRootId,
    leaf_id: StorageObjectRecordId,
    availability: StoragePathAvailability,
) -> Result<bool, DbErr> {
    let facts_are_reconciled = match availability {
        StoragePathAvailability::Present => {
            crate::catalog_storage_scope::storage_objects_are_reconciled(
                connection,
                &[leaf_id],
                Some(root_id),
            )
            .await?
        }
        StoragePathAvailability::Playback => {
            crate::catalog_storage_scope::storage_objects_have_reconciled_playback_facts(
                connection,
                &[leaf_id],
                root_id,
            )
            .await?
        }
    };
    if !facts_are_reconciled {
        return Ok(false);
    }
    let backend = connection.get_database_backend();
    let mut current = leaf_id;
    let mut seen = HashSet::new();
    for _ in 0..MAX_STORAGE_PATH_DEPTH {
        if !seen.insert(current) {
            return Ok(false);
        }
        let relation = Alias::new("authorized_path_relation");
        let object = Alias::new("authorized_path_object");
        let root = Alias::new("authorized_path_root");
        let account = Alias::new("authorized_path_account");
        let query = Query::select()
            .expr_as(
                Expr::col((relation.clone(), Alias::new("parent_storage_object_id"))),
                Alias::new("parent_storage_object_id"),
            )
            .expr_as(
                Expr::col((relation.clone(), Alias::new("presence_state"))),
                Alias::new("relation_presence_state"),
            )
            .expr_as(
                Expr::col((relation.clone(), Alias::new("availability_reason"))),
                Alias::new("availability_reason"),
            )
            .expr_as(
                Expr::col((relation.clone(), Alias::new("observed_sync_revision"))),
                Alias::new("relation_observed_sync_revision"),
            )
            .expr_as(
                Expr::col((root.clone(), Alias::new("reconciled_sync_revision"))),
                Alias::new("reconciled_sync_revision"),
            )
            .from_as(Alias::new("storage_root_objects"), relation.clone())
            .join_as(
                sea_orm::sea_query::JoinType::InnerJoin,
                Alias::new("storage_objects"),
                object.clone(),
                Expr::col((object.clone(), Alias::new("id")))
                    .equals((relation.clone(), Alias::new("storage_object_id"))),
            )
            .join_as(
                sea_orm::sea_query::JoinType::InnerJoin,
                Alias::new("storage_roots"),
                root.clone(),
                sea_orm::sea_query::Cond::all()
                    .add(
                        Expr::col((root.clone(), Alias::new("id")))
                            .equals((relation.clone(), Alias::new("storage_root_id"))),
                    )
                    .add(
                        Expr::col((root.clone(), Alias::new("storage_account_id")))
                            .equals((object.clone(), Alias::new("storage_account_id"))),
                    ),
            )
            .join_as(
                sea_orm::sea_query::JoinType::InnerJoin,
                Alias::new("storage_accounts"),
                account.clone(),
                Expr::col((account.clone(), Alias::new("id")))
                    .equals((root.clone(), Alias::new("storage_account_id"))),
            )
            .and_where(
                Expr::col((relation.clone(), Alias::new("storage_root_id"))).eq(root_id.as_uuid()),
            )
            .and_where(Expr::col((relation, Alias::new("storage_object_id"))).eq(current.as_uuid()))
            .and_where(Expr::col((account, Alias::new("status"))).is_in(["Active", "Ready"]))
            .limit(1)
            .to_owned();
        let Some(row) = connection.query_one(backend.build(&query)).await? else {
            return Ok(false);
        };
        let reconciled: i64 = row.try_get("", "reconciled_sync_revision")?;
        let relation_observed: i64 = row.try_get("", "relation_observed_sync_revision")?;
        if relation_observed > reconciled {
            return Ok(false);
        }
        let presence: String = row.try_get("", "relation_presence_state")?;
        let reason: Option<String> = row.try_get("", "availability_reason")?;
        if !relation_is_available(&presence, reason.as_deref(), availability) {
            return Ok(false);
        }
        let parent = row
            .try_get::<Option<Uuid>>("", "parent_storage_object_id")?
            .map(StorageObjectRecordId::from_uuid);
        let Some(parent) = parent else {
            return Ok(true);
        };
        current = parent;
    }
    Ok(false)
}

fn relation_is_available(
    presence: &str,
    reason: Option<&str>,
    availability: StoragePathAvailability,
) -> bool {
    match availability {
        StoragePathAvailability::Present => presence == "Present",
        StoragePathAvailability::Playback => {
            presence == "Present"
                || (presence == "TemporarilyUnavailable"
                    && !reason.is_some_and(topology_unavailable_reason))
        }
    }
}

fn topology_unavailable_reason(reason: &str) -> bool {
    reason == "moved-to-unmaterialized-parent"
        || reason == "ancestor-moved-to-unmaterialized-parent"
        || reason.starts_with("ancestor-moved-out:")
}
