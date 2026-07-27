use std::collections::{HashMap, HashSet, VecDeque};

use sea_orm::{
    ConnectionTrait, DatabaseConnection, DbErr,
    sea_query::{Alias, Cond, Expr, JoinType, Order, Query},
};
use thiserror::Error;
use tjxy_common::{CatalogItemId, StorageObjectRecordId, StorageRootId};
use uuid::Uuid;

use crate::{ClaimedWorkJob, WorkScope, WorkTaskKind};

const MAX_OBJECTS: usize = 100_000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SeriesStorageObject {
    id: StorageObjectRecordId,
    parent: Option<StorageObjectRecordId>,
    name: String,
    object_type: String,
    checksum: Option<String>,
    observed_revision: i64,
    object_observed_revision: i64,
    facts_observed_storage_root_id: Option<Uuid>,
    facts_origin_reconciled_revision: Option<i64>,
    root_reconciled_revision: i64,
    has_other_root: bool,
    relation_presence: String,
    object_presence: String,
    children_indexed: bool,
    children_revision: i64,
}

impl SeriesStorageObject {
    #[must_use]
    pub const fn id(&self) -> StorageObjectRecordId {
        self.id
    }
    #[must_use]
    pub const fn parent(&self) -> Option<StorageObjectRecordId> {
        self.parent
    }
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
    #[must_use]
    pub fn object_type(&self) -> &str {
        &self.object_type
    }
    #[must_use]
    pub fn checksum(&self) -> Option<&str> {
        self.checksum.as_deref()
    }
    #[must_use]
    pub const fn observed_revision(&self) -> i64 {
        self.observed_revision
    }
    #[must_use]
    pub const fn children_indexed(&self) -> bool {
        self.children_indexed
    }
    #[must_use]
    pub const fn children_revision(&self) -> i64 {
        self.children_revision
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SeriesExpandSnapshot {
    owner: CatalogItemId,
    root_object: StorageObjectRecordId,
    storage_root: StorageRootId,
    sync_revision: i64,
    objects: Vec<SeriesStorageObject>,
}

impl SeriesExpandSnapshot {
    #[must_use]
    pub const fn owner(&self) -> CatalogItemId {
        self.owner
    }
    #[must_use]
    pub const fn root_object(&self) -> StorageObjectRecordId {
        self.root_object
    }
    #[must_use]
    pub const fn storage_root(&self) -> StorageRootId {
        self.storage_root
    }
    #[must_use]
    pub const fn sync_revision(&self) -> i64 {
        self.sync_revision
    }
    #[must_use]
    pub fn objects(&self) -> &[SeriesStorageObject] {
        &self.objects
    }
}

pub struct SeriesExpandRepository<'connection> {
    database: &'connection DatabaseConnection,
}

impl<'connection> SeriesExpandRepository<'connection> {
    #[must_use]
    pub const fn new(database: &'connection DatabaseConnection) -> Self {
        Self { database }
    }

    /// Reads the reconciled root-local object graph for one Series Expand claim.
    ///
    /// # Errors
    ///
    /// Returns [`SeriesExpandRepositoryError`] for invalid work, ambiguous scope, bounds, or SQL failures.
    pub async fn snapshot(
        &self,
        claimed: &ClaimedWorkJob,
    ) -> Result<SeriesExpandSnapshot, SeriesExpandRepositoryError> {
        let WorkScope::CatalogItem(owner) = claimed.job().scope() else {
            return Err(SeriesExpandRepositoryError::InvalidClaim);
        };
        if claimed.job().task_kind() != WorkTaskKind::ExpandItem {
            return Err(SeriesExpandRepositoryError::InvalidClaim);
        }
        let input_revision = claimed
            .job()
            .input_sync_revision()
            .ok_or(SeriesExpandRepositoryError::MissingSyncRevision)?;
        let roots = self
            .database
            .query_all(self.database.get_database_backend().build(&root_query(
                owner,
                input_revision,
                claimed.job().storage_root_affinity(),
            )))
            .await?;
        if roots.len() != 1 {
            return Err(SeriesExpandRepositoryError::AmbiguousScope);
        }
        let storage_root = StorageRootId::from_uuid(roots[0].try_get("", "storage_root_id")?);
        let root_object =
            StorageObjectRecordId::from_uuid(roots[0].try_get("", "storage_object_id")?);
        let sync_revision = roots[0].try_get("", "reconciled_sync_revision")?;
        let rows = self
            .database
            .query_all(
                self.database
                    .get_database_backend()
                    .build(&objects_query(storage_root)),
            )
            .await?;
        if rows.len() > MAX_OBJECTS {
            return Err(SeriesExpandRepositoryError::ObjectLimit);
        }
        let objects = rows
            .iter()
            .map(|row| {
                Ok(SeriesStorageObject {
                    id: StorageObjectRecordId::from_uuid(row.try_get("", "storage_object_id")?),
                    parent: row
                        .try_get::<Option<Uuid>>("", "parent_storage_object_id")?
                        .map(StorageObjectRecordId::from_uuid),
                    name: row.try_get("", "name")?,
                    object_type: row.try_get("", "object_type")?,
                    checksum: row.try_get("", "checksum")?,
                    observed_revision: row.try_get("", "observed_sync_revision")?,
                    object_observed_revision: row.try_get("", "object_observed_sync_revision")?,
                    facts_observed_storage_root_id: row
                        .try_get("", "facts_observed_storage_root_id")?,
                    facts_origin_reconciled_revision: row
                        .try_get("", "facts_origin_reconciled_sync_revision")?,
                    root_reconciled_revision: row.try_get("", "root_reconciled_sync_revision")?,
                    has_other_root: row.try_get("", "has_other_root")?,
                    relation_presence: row.try_get("", "relation_presence_state")?,
                    object_presence: row.try_get("", "object_presence_state")?,
                    children_indexed: row.try_get("", "children_indexed")?,
                    children_revision: row.try_get("", "children_index_revision")?,
                })
            })
            .collect::<Result<Vec<_>, DbErr>>()?;
        let objects = reconciled_subtree(root_object, storage_root, objects)?;
        Ok(SeriesExpandSnapshot {
            owner,
            root_object,
            storage_root,
            sync_revision,
            objects,
        })
    }
}

#[derive(Debug, Error)]
pub enum SeriesExpandRepositoryError {
    #[error("claimed work is not a Series Expand job")]
    InvalidClaim,
    #[error("Series Expand job does not record its synchronized input revision")]
    MissingSyncRevision,
    #[error("Series matched storage scope is missing, ambiguous, or unreconciled")]
    AmbiguousScope,
    #[error("Series storage input contains facts not yet reconciled")]
    StorageInputPending,
    #[error("Series storage graph exceeds the bounded object limit")]
    ObjectLimit,
    #[error("Series storage graph query failed: {0}")]
    Database(#[from] DbErr),
}

fn root_query(
    owner: CatalogItemId,
    revision: i64,
    storage_root: Option<StorageRootId>,
) -> sea_orm::sea_query::SelectStatement {
    let identity = Alias::new("series_identity");
    let relation = Alias::new("series_relation");
    let root = Alias::new("series_root");
    let lr = Alias::new("series_library_root");
    let membership = Alias::new("series_membership");
    let library = Alias::new("series_library");
    Query::select()
        .distinct()
        .expr_as(
            Expr::col((relation.clone(), Alias::new("storage_root_id"))),
            Alias::new("storage_root_id"),
        )
        .expr_as(
            Expr::col((relation.clone(), Alias::new("storage_object_id"))),
            Alias::new("storage_object_id"),
        )
        .expr_as(
            Expr::col((root.clone(), Alias::new("reconciled_sync_revision"))),
            Alias::new("reconciled_sync_revision"),
        )
        .from_as(Alias::new("identity_matches"), identity.clone())
        .join_as(
            JoinType::InnerJoin,
            Alias::new("storage_root_objects"),
            relation.clone(),
            Expr::col((relation.clone(), Alias::new("storage_object_id")))
                .equals((identity.clone(), Alias::new("storage_object_id"))),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("storage_roots"),
            root.clone(),
            Expr::col((root.clone(), Alias::new("id")))
                .equals((relation.clone(), Alias::new("storage_root_id"))),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("library_storage_roots"),
            lr.clone(),
            Expr::col((lr.clone(), Alias::new("storage_root_id")))
                .equals((root.clone(), Alias::new("id"))),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("library_catalog_items"),
            membership.clone(),
            Cond::all()
                .add(
                    Expr::col((membership.clone(), Alias::new("library_id")))
                        .equals((lr, Alias::new("library_id"))),
                )
                .add(
                    Expr::col((membership.clone(), Alias::new("catalog_item_id")))
                        .eq(owner.as_uuid()),
                ),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("libraries"),
            library.clone(),
            Expr::col((library.clone(), Alias::new("id")))
                .equals((membership, Alias::new("library_id"))),
        )
        .and_where(
            Expr::col((identity.clone(), Alias::new("candidate_catalog_item_id")))
                .eq(owner.as_uuid()),
        )
        .and_where(Expr::col((identity, Alias::new("state"))).eq("Matched"))
        .and_where(Expr::col((relation.clone(), Alias::new("presence_state"))).eq("Present"))
        .cond_where(storage_root.map_or_else(Cond::all, |storage_root| {
            Cond::all().add(
                Expr::col((relation.clone(), Alias::new("storage_root_id")))
                    .eq(storage_root.as_uuid()),
            )
        }))
        .and_where(Expr::col((relation, Alias::new("children_indexed"))).eq(true))
        .and_where(Expr::col((root, Alias::new("reconciled_sync_revision"))).gte(revision))
        .and_where(Expr::col((library, Alias::new("is_enabled"))).eq(true))
        .to_owned()
}

#[allow(clippy::too_many_lines)] // One row carries the complete relation and object fact fence for subtree validation.
fn objects_query(root_id: StorageRootId) -> sea_orm::sea_query::SelectStatement {
    let relation = Alias::new("series_object_relation");
    let object = Alias::new("series_object");
    let root = Alias::new("series_object_root");
    let fact_root = Alias::new("series_object_fact_root");
    let other_relation = Alias::new("series_object_other_relation");
    let other_root = Query::select()
        .expr(Expr::val(1))
        .from_as(Alias::new("storage_root_objects"), other_relation.clone())
        .and_where(
            Expr::col((other_relation.clone(), Alias::new("storage_object_id")))
                .equals((object.clone(), Alias::new("id"))),
        )
        .and_where(
            Expr::col((other_relation, Alias::new("storage_root_id")))
                .ne(Expr::col((relation.clone(), Alias::new("storage_root_id")))),
        )
        .to_owned();
    Query::select()
        .expr_as(
            Expr::col((relation.clone(), Alias::new("storage_object_id"))),
            Alias::new("storage_object_id"),
        )
        .expr_as(
            Expr::col((relation.clone(), Alias::new("parent_storage_object_id"))),
            Alias::new("parent_storage_object_id"),
        )
        .expr_as(
            Expr::col((relation.clone(), Alias::new("observed_sync_revision"))),
            Alias::new("observed_sync_revision"),
        )
        .expr_as(
            Expr::col((object.clone(), Alias::new("observed_sync_revision"))),
            Alias::new("object_observed_sync_revision"),
        )
        .expr_as(
            Expr::col((object.clone(), Alias::new("facts_observed_storage_root_id"))),
            Alias::new("facts_observed_storage_root_id"),
        )
        .expr_as(
            Expr::col((fact_root.clone(), Alias::new("reconciled_sync_revision"))),
            Alias::new("facts_origin_reconciled_sync_revision"),
        )
        .expr_as(
            Expr::col((root.clone(), Alias::new("reconciled_sync_revision"))),
            Alias::new("root_reconciled_sync_revision"),
        )
        .expr_as(Expr::exists(other_root), Alias::new("has_other_root"))
        .expr_as(
            Expr::col((relation.clone(), Alias::new("presence_state"))),
            Alias::new("relation_presence_state"),
        )
        .expr_as(
            Expr::col((object.clone(), Alias::new("presence_state"))),
            Alias::new("object_presence_state"),
        )
        .expr_as(
            Expr::col((relation.clone(), Alias::new("children_indexed"))),
            Alias::new("children_indexed"),
        )
        .expr_as(
            Expr::col((relation.clone(), Alias::new("children_index_revision"))),
            Alias::new("children_index_revision"),
        )
        .expr_as(
            Expr::col((object.clone(), Alias::new("name"))),
            Alias::new("name"),
        )
        .expr_as(
            Expr::col((object.clone(), Alias::new("object_type"))),
            Alias::new("object_type"),
        )
        .expr_as(
            Expr::col((object.clone(), Alias::new("checksum"))),
            Alias::new("checksum"),
        )
        .from_as(Alias::new("storage_root_objects"), relation.clone())
        .join_as(
            JoinType::InnerJoin,
            Alias::new("storage_objects"),
            object.clone(),
            Expr::col((object.clone(), Alias::new("id")))
                .equals((relation.clone(), Alias::new("storage_object_id"))),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("storage_roots"),
            root.clone(),
            Expr::col((root.clone(), Alias::new("id")))
                .equals((relation.clone(), Alias::new("storage_root_id"))),
        )
        .join_as(
            JoinType::LeftJoin,
            Alias::new("storage_roots"),
            fact_root.clone(),
            Expr::col((fact_root.clone(), Alias::new("id")))
                .equals((object.clone(), Alias::new("facts_observed_storage_root_id"))),
        )
        .and_where(
            Expr::col((relation.clone(), Alias::new("storage_root_id"))).eq(root_id.as_uuid()),
        )
        .order_by(Alias::new("storage_object_id"), Order::Asc)
        .to_owned()
}

fn reconciled_subtree(
    root_object: StorageObjectRecordId,
    storage_root: StorageRootId,
    objects: Vec<SeriesStorageObject>,
) -> Result<Vec<SeriesStorageObject>, SeriesExpandRepositoryError> {
    let by_id = objects
        .iter()
        .map(|object| (object.id.as_uuid(), object))
        .collect::<HashMap<_, _>>();
    let mut children = HashMap::<Uuid, Vec<Uuid>>::new();
    for object in &objects {
        if let Some(parent) = object.parent {
            children
                .entry(parent.as_uuid())
                .or_default()
                .push(object.id.as_uuid());
        }
    }
    let mut pending = VecDeque::from([root_object.as_uuid()]);
    let mut visited = HashSet::new();
    let mut present = HashSet::new();
    while let Some(id) = pending.pop_front() {
        if !visited.insert(id) {
            continue;
        }
        let object = by_id
            .get(&id)
            .ok_or(SeriesExpandRepositoryError::StorageInputPending)?;
        if !object.fact_is_reconciled(storage_root) {
            return Err(SeriesExpandRepositoryError::StorageInputPending);
        }
        match object.relation_presence.as_str() {
            "ConfirmedAbsent" if id != root_object.as_uuid() => continue,
            "ConfirmedAbsent" => {
                return Err(SeriesExpandRepositoryError::StorageInputPending);
            }
            "Present" if object.object_presence == "Present" => {}
            _ => return Err(SeriesExpandRepositoryError::StorageInputPending),
        }
        present.insert(id);
        pending.extend(children.get(&id).into_iter().flatten().copied());
    }
    Ok(objects
        .into_iter()
        .filter(|object| present.contains(&object.id.as_uuid()))
        .collect())
}

impl SeriesStorageObject {
    fn fact_is_reconciled(&self, storage_root: StorageRootId) -> bool {
        if self.observed_revision > self.root_reconciled_revision {
            return false;
        }
        match self.facts_observed_storage_root_id {
            Some(origin) => {
                origin == storage_root.as_uuid()
                    && self
                        .facts_origin_reconciled_revision
                        .is_some_and(|revision| revision >= self.object_observed_revision)
            }
            None => {
                !self.has_other_root
                    && self.object_observed_revision <= self.root_reconciled_revision
            }
        }
    }
}
