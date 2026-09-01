use std::{collections::HashSet, path::PathBuf};

use sea_orm::{ConnectionTrait, DbBackend, DbErr, Statement, Value};
use tjxy_common::{StorageObjectRecordId, StorageRootId};
use uuid::Uuid;

const MAX_FILESYSTEM_PATH_DEPTH: usize = 4096;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FilesystemObjectPath {
    root_id: StorageRootId,
    object_id: StorageObjectRecordId,
    relative_path: PathBuf,
    reconciled_revision: i64,
}

impl FilesystemObjectPath {
    #[must_use]
    pub const fn root_id(&self) -> StorageRootId {
        self.root_id
    }

    #[must_use]
    pub const fn object_id(&self) -> StorageObjectRecordId {
        self.object_id
    }

    #[must_use]
    pub fn relative_path(&self) -> &std::path::Path {
        &self.relative_path
    }

    #[must_use]
    pub const fn reconciled_revision(&self) -> i64 {
        self.reconciled_revision
    }
}

pub struct FilesystemPathRepository<'connection> {
    database: &'connection sea_orm::DatabaseConnection,
}

impl<'connection> FilesystemPathRepository<'connection> {
    #[must_use]
    pub const fn new(database: &'connection sea_orm::DatabaseConnection) -> Self {
        Self { database }
    }

    /// Resolves one persisted filesystem provider identity to a root-relative path.
    ///
    /// The normalized root relation is authoritative so directory renames do not require
    /// rewriting every descendant path.
    ///
    /// # Errors
    ///
    /// Returns a database error for ambiguous roots, cycles, stale relations, or invalid names.
    pub async fn resolve(
        &self,
        account_id: Uuid,
        provider_object_id: &str,
    ) -> Result<Option<FilesystemObjectPath>, DbErr> {
        resolve_path(self.database, account_id, provider_object_id).await
    }
}

#[derive(Debug)]
struct PathRow {
    root_id: StorageRootId,
    object_id: StorageObjectRecordId,
    parent_id: Option<StorageObjectRecordId>,
    name: String,
    depth: usize,
    reconciled_revision: i64,
}

async fn resolve_path(
    connection: &impl ConnectionTrait,
    account_id: Uuid,
    provider_object_id: &str,
) -> Result<Option<FilesystemObjectPath>, DbErr> {
    let backend = connection.get_database_backend();
    let account_placeholder = if backend == DbBackend::Postgres {
        "$1"
    } else {
        "?"
    };
    let object_placeholder = if backend == DbBackend::Postgres {
        "$2"
    } else {
        "?"
    };
    let sql = format!(
        "WITH RECURSIVE filesystem_path (storage_root_id, storage_object_id, parent_storage_object_id, name, presence_state, observed_sync_revision, reconciled_sync_revision, depth) AS (\
         SELECT r.storage_root_id, r.storage_object_id, r.parent_storage_object_id, o.name, r.presence_state, r.observed_sync_revision, sr.reconciled_sync_revision, CAST(0 AS BIGINT) \
         FROM storage_root_objects r \
         JOIN storage_objects o ON o.id = r.storage_object_id \
         JOIN storage_roots sr ON sr.id = r.storage_root_id AND sr.storage_account_id = o.storage_account_id \
         WHERE o.storage_account_id = {account_placeholder} AND o.provider_drive_id = 'local' AND o.provider_object_id = {object_placeholder} \
         UNION ALL \
         SELECT r.storage_root_id, r.storage_object_id, r.parent_storage_object_id, o.name, r.presence_state, r.observed_sync_revision, p.reconciled_sync_revision, p.depth + CAST(1 AS BIGINT) \
         FROM storage_root_objects r \
         JOIN storage_objects o ON o.id = r.storage_object_id \
         JOIN filesystem_path p ON p.storage_root_id = r.storage_root_id AND p.parent_storage_object_id = r.storage_object_id \
         WHERE p.depth < {MAX_FILESYSTEM_PATH_DEPTH} \
         ) SELECT storage_root_id, storage_object_id, parent_storage_object_id, name, presence_state, observed_sync_revision, reconciled_sync_revision, depth FROM filesystem_path"
    );
    let rows = connection
        .query_all(Statement::from_sql_and_values(
            backend,
            sql,
            vec![
                Value::Uuid(Some(Box::new(account_id))),
                Value::String(Some(Box::new(provider_object_id.to_owned()))),
            ],
        ))
        .await?;
    if rows.is_empty() {
        return Ok(None);
    }
    let mut path_rows = Vec::with_capacity(rows.len());
    for row in rows {
        let presence: String = row.try_get("", "presence_state")?;
        let observed_revision: i64 = row.try_get("", "observed_sync_revision")?;
        let reconciled_revision: i64 = row.try_get("", "reconciled_sync_revision")?;
        if presence != "Present" || observed_revision > reconciled_revision {
            return Err(DbErr::Custom(
                "filesystem path relation is not reconciled and present".to_owned(),
            ));
        }
        let depth: i64 = row.try_get("", "depth")?;
        let depth = usize::try_from(depth)
            .map_err(|_| DbErr::Custom("filesystem path depth is invalid".to_owned()))?;
        path_rows.push(PathRow {
            root_id: StorageRootId::from_uuid(row.try_get("", "storage_root_id")?),
            object_id: StorageObjectRecordId::from_uuid(row.try_get("", "storage_object_id")?),
            parent_id: row
                .try_get::<Option<Uuid>>("", "parent_storage_object_id")?
                .map(StorageObjectRecordId::from_uuid),
            name: row.try_get("", "name")?,
            depth,
            reconciled_revision,
        });
    }
    build_path(path_rows).map(Some)
}

fn build_path(mut rows: Vec<PathRow>) -> Result<FilesystemObjectPath, DbErr> {
    let root_id = rows[0].root_id;
    let object_id = rows[0].object_id;
    let reconciled_revision = rows[0].reconciled_revision;
    if rows
        .iter()
        .any(|row| row.root_id != root_id || row.reconciled_revision != reconciled_revision)
    {
        return Err(DbErr::Custom(
            "filesystem object belongs to an ambiguous path index".to_owned(),
        ));
    }
    rows.sort_by_key(|row| std::cmp::Reverse(row.depth));
    let mut seen = HashSet::with_capacity(rows.len());
    let mut relative_path = PathBuf::new();
    let mut reached_root = false;
    for row in rows {
        if !seen.insert(row.object_id) {
            return Err(DbErr::Custom("filesystem path contains a cycle".to_owned()));
        }
        if row.depth >= MAX_FILESYSTEM_PATH_DEPTH {
            return Err(DbErr::Custom(
                "filesystem path exceeded its depth limit".to_owned(),
            ));
        }
        if row.parent_id.is_none() {
            if reached_root {
                return Err(DbErr::Custom(
                    "filesystem path contains multiple roots".to_owned(),
                ));
            }
            reached_root = true;
            continue;
        }
        let mut components = std::path::Path::new(&row.name).components();
        let Some(std::path::Component::Normal(component)) = components.next() else {
            return Err(DbErr::Custom(
                "filesystem path contains an invalid object name".to_owned(),
            ));
        };
        if components.next().is_some() {
            return Err(DbErr::Custom(
                "filesystem path contains a multi-component object name".to_owned(),
            ));
        }
        relative_path.push(component);
    }
    if !reached_root {
        return Err(DbErr::Custom(
            "filesystem path did not reach its storage root".to_owned(),
        ));
    }
    Ok(FilesystemObjectPath {
        root_id,
        object_id,
        relative_path,
        reconciled_revision,
    })
}
