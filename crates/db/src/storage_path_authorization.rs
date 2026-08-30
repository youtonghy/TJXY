use std::collections::HashSet;

use sea_orm::{ConnectionTrait, DbBackend, DbErr, Statement, Value};
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
    let placeholder = if backend == DbBackend::Postgres {
        "$1"
    } else {
        "?"
    };
    let leaf_placeholder = if backend == DbBackend::Postgres {
        "$2"
    } else {
        "?"
    };
    let sql = format!(
        "WITH RECURSIVE authorized_path (storage_root_id, storage_object_id, parent_storage_object_id, presence_state, availability_reason, observed_sync_revision, reconciled_sync_revision, depth) AS (\
         SELECT r.storage_root_id, r.storage_object_id, r.parent_storage_object_id, r.presence_state, r.availability_reason, r.observed_sync_revision, sr.reconciled_sync_revision, CAST(1 AS BIGINT) \
         FROM storage_root_objects r \
         JOIN storage_objects o ON o.id = r.storage_object_id \
         JOIN storage_roots sr ON sr.id = r.storage_root_id AND sr.storage_account_id = o.storage_account_id \
         JOIN storage_accounts sa ON sa.id = sr.storage_account_id \
         WHERE r.storage_root_id = {placeholder} AND r.storage_object_id = {leaf_placeholder} AND sa.status IN ('Active', 'Ready') \
         UNION ALL \
         SELECT r.storage_root_id, r.storage_object_id, r.parent_storage_object_id, r.presence_state, r.availability_reason, r.observed_sync_revision, p.reconciled_sync_revision, p.depth + CAST(1 AS BIGINT) \
         FROM storage_root_objects r \
         JOIN authorized_path p ON p.storage_root_id = r.storage_root_id AND p.parent_storage_object_id = r.storage_object_id \
         WHERE p.depth < {MAX_STORAGE_PATH_DEPTH} \
         ) SELECT storage_object_id, parent_storage_object_id, presence_state, availability_reason, observed_sync_revision, reconciled_sync_revision, depth FROM authorized_path"
    );
    let statement = Statement::from_sql_and_values(
        backend,
        sql,
        vec![
            Value::Uuid(Some(Box::new(root_id.as_uuid()))),
            Value::Uuid(Some(Box::new(leaf_id.as_uuid()))),
        ],
    );
    let rows = connection.query_all(statement).await?;
    let mut seen = HashSet::new();
    let mut last_depth = 0_usize;
    for row in rows {
        let current = StorageObjectRecordId::from_uuid(row.try_get("", "storage_object_id")?);
        if !seen.insert(current) {
            return Ok(false);
        }
        let depth: i64 = row.try_get("", "depth")?;
        last_depth = usize::try_from(depth).unwrap_or(MAX_STORAGE_PATH_DEPTH);
        let reconciled: i64 = row.try_get("", "reconciled_sync_revision")?;
        let relation_observed: i64 = row.try_get("", "observed_sync_revision")?;
        if relation_observed > reconciled {
            return Ok(false);
        }
        let presence: String = row.try_get("", "presence_state")?;
        let reason: Option<String> = row.try_get("", "availability_reason")?;
        if !relation_is_available(&presence, reason.as_deref(), availability) {
            return Ok(false);
        }
        if row
            .try_get::<Option<Uuid>>("", "parent_storage_object_id")?
            .is_none()
        {
            return Ok(true);
        }
    }
    if last_depth >= MAX_STORAGE_PATH_DEPTH {
        return Ok(false);
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
