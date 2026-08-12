use sea_orm::{
    ConnectionTrait, DbErr,
    sea_query::{Alias, Cond, Condition, Expr, JoinType, Order, Query, SelectStatement},
};
use std::collections::{HashMap, HashSet};
use thiserror::Error;
use tjxy_common::{CatalogItemId, StorageObjectRecordId, StorageRootId};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CatalogStorageScope {
    storage_root_id: StorageRootId,
    storage_object_id: StorageObjectRecordId,
    parent_storage_object_id: Option<StorageObjectRecordId>,
    children_indexed: bool,
    children_revision: i64,
    reconciled_revision: i64,
    is_file: bool,
}

impl CatalogStorageScope {
    pub(crate) const fn storage_root_id(self) -> StorageRootId {
        self.storage_root_id
    }

    pub(crate) const fn storage_object_id(self) -> StorageObjectRecordId {
        self.storage_object_id
    }

    pub(crate) const fn sidecar_parent_object_id(self) -> StorageObjectRecordId {
        if self.is_file {
            match self.parent_storage_object_id {
                Some(parent) => parent,
                None => self.storage_object_id,
            }
        } else {
            self.storage_object_id
        }
    }

    pub(crate) const fn children_indexed(self) -> bool {
        self.children_indexed
    }

    pub(crate) const fn children_revision(self) -> i64 {
        self.children_revision
    }

    pub(crate) const fn metadata_input_revision(self) -> i64 {
        if self.children_indexed {
            self.children_revision
        } else {
            self.reconciled_revision
        }
    }

    pub(crate) const fn accepts_metadata_input(self, input_revision: i64) -> bool {
        self.reconciled_revision >= input_revision
            && (!self.children_indexed || self.children_revision == input_revision)
    }

    pub(crate) const fn reconciled_revision(self) -> i64 {
        self.reconciled_revision
    }

    pub(crate) const fn is_file(self) -> bool {
        self.is_file
    }

    pub(crate) fn has_same_inventory(self, other: Self) -> bool {
        self.storage_root_id == other.storage_root_id
            && self.storage_object_id == other.storage_object_id
            && self.parent_storage_object_id == other.parent_storage_object_id
            && self.children_indexed == other.children_indexed
            && self.children_revision == other.children_revision
    }
}

#[derive(Debug, Error)]
pub(crate) enum CatalogStorageScopeError {
    #[error("catalog item resolves to more than one authorized storage scope")]
    Ambiguous,
    #[error("catalog storage scope query failed: {0}")]
    Database(#[from] DbErr),
}

pub(crate) async fn resolve_catalog_storage_scope(
    connection: &impl ConnectionTrait,
    item_id: CatalogItemId,
    required_root: Option<StorageRootId>,
) -> Result<Option<CatalogStorageScope>, CatalogStorageScopeError> {
    let backend = connection.get_database_backend();
    let mut rows = connection
        .query_all(backend.build(&direct_scope_query(item_id, required_root)))
        .await?;
    if rows.is_empty() {
        rows = connection
            .query_all(backend.build(&structure_scope_query(item_id, required_root)))
            .await?;
    }
    if rows.len() > 1 {
        return Err(CatalogStorageScopeError::Ambiguous);
    }
    rows.first().map(scope_from_row).transpose()
}

fn scope_from_row(
    row: &sea_orm::QueryResult,
) -> Result<CatalogStorageScope, CatalogStorageScopeError> {
    Ok(CatalogStorageScope {
        storage_root_id: StorageRootId::from_uuid(row.try_get("", "storage_root_id")?),
        storage_object_id: StorageObjectRecordId::from_uuid(row.try_get("", "storage_object_id")?),
        parent_storage_object_id: row
            .try_get::<Option<Uuid>>("", "parent_storage_object_id")?
            .map(StorageObjectRecordId::from_uuid),
        children_indexed: row.try_get("", "children_indexed")?,
        children_revision: row.try_get("", "children_index_revision")?,
        reconciled_revision: row.try_get("", "reconciled_sync_revision")?,
        is_file: row.try_get::<String>("", "object_type")? == "File",
    })
}

fn scope_columns(query: &mut SelectStatement, relation: &Alias, object: &Alias, root: &Alias) {
    query
        .expr_as(
            Expr::col((relation.clone(), Alias::new("storage_root_id"))),
            Alias::new("storage_root_id"),
        )
        .expr_as(
            Expr::col((relation.clone(), Alias::new("storage_object_id"))),
            Alias::new("storage_object_id"),
        )
        .expr_as(
            Expr::col((relation.clone(), Alias::new("parent_storage_object_id"))),
            Alias::new("parent_storage_object_id"),
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
            Expr::col((root.clone(), Alias::new("reconciled_sync_revision"))),
            Alias::new("reconciled_sync_revision"),
        )
        .expr_as(
            Expr::col((object.clone(), Alias::new("object_type"))),
            Alias::new("object_type"),
        );
}

pub(crate) async fn storage_scope_is_reconciled(
    connection: &impl ConnectionTrait,
    scope: CatalogStorageScope,
    include_direct_children: bool,
) -> Result<bool, DbErr> {
    if include_direct_children && !scope.children_indexed() {
        return Ok(false);
    }
    if !storage_objects_are_reconciled(
        connection,
        &[scope.storage_object_id()],
        Some(scope.storage_root_id()),
    )
    .await?
    {
        return Ok(false);
    }
    let query = pending_storage_scope_facts_query(
        scope.storage_root_id(),
        &[scope.storage_object_id()],
        include_direct_children,
    );
    let backend = connection.get_database_backend();
    Ok(connection.query_one(backend.build(&query)).await?.is_none())
}

pub(crate) async fn storage_scope_pairs_are_reconciled(
    connection: &impl ConnectionTrait,
    pairs: &[(StorageRootId, StorageObjectRecordId)],
) -> Result<bool, DbErr> {
    let mut by_root = HashMap::<StorageRootId, HashSet<StorageObjectRecordId>>::new();
    for (root, object) in pairs {
        by_root.entry(*root).or_default().insert(*object);
    }
    let backend = connection.get_database_backend();
    for (root, objects) in by_root {
        let objects = objects.into_iter().collect::<Vec<_>>();
        for chunk in objects.chunks(500) {
            let reconciled = reconciled_storage_objects(connection, chunk, Some(root)).await?;
            if reconciled.len() != chunk.len() {
                return Ok(false);
            }
            let query = pending_storage_scope_facts_query(root, chunk, true);
            if connection.query_one(backend.build(&query)).await?.is_some() {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

fn pending_storage_scope_facts_query(
    storage_root_id: StorageRootId,
    scope_ids: &[StorageObjectRecordId],
    include_direct_children: bool,
) -> SelectStatement {
    let relation = Alias::new("fact_fence_relation");
    let object = Alias::new("fact_fence_object");
    let root = Alias::new("fact_fence_root");
    let fact_root = Alias::new("fact_fence_origin_root");
    let pending =
        pending_storage_fact_condition(storage_root_id, &relation, &object, &root, &fact_root);
    let scope_ids = scope_ids
        .iter()
        .map(|scope_id| scope_id.as_uuid())
        .collect::<Vec<_>>();
    let mut scope_rows = Cond::any().add(
        Expr::col((relation.clone(), Alias::new("storage_object_id"))).is_in(scope_ids.clone()),
    );
    if include_direct_children {
        scope_rows = scope_rows.add(
            Expr::col((relation.clone(), Alias::new("parent_storage_object_id")))
                .is_in(scope_ids.clone()),
        );
    }
    let pending = if include_direct_children {
        pending.add(
            Cond::all()
                .add(
                    Expr::col((relation.clone(), Alias::new("storage_object_id"))).is_in(scope_ids),
                )
                .add(Expr::col((relation.clone(), Alias::new("children_indexed"))).eq(false)),
        )
    } else {
        pending
    };
    Query::select()
        .expr(Expr::val(1))
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
            Expr::col((relation.clone(), Alias::new("storage_root_id")))
                .eq(storage_root_id.as_uuid()),
        )
        .cond_where(scope_rows)
        .cond_where(pending)
        .limit(1)
        .to_owned()
}

fn pending_storage_fact_condition(
    storage_root_id: StorageRootId,
    relation: &Alias,
    object: &Alias,
    root: &Alias,
    fact_root: &Alias,
) -> Condition {
    let other_relation = Alias::new("fact_fence_other_relation");
    let other_roots = Query::select()
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
    let relation_pending = Expr::col((relation.clone(), Alias::new("observed_sync_revision"))).gt(
        Expr::col((root.clone(), Alias::new("reconciled_sync_revision"))),
    );
    let known_origin_pending = Cond::all()
        .add(
            Expr::col((object.clone(), Alias::new("facts_observed_storage_root_id"))).is_not_null(),
        )
        .add(
            Cond::any()
                .add(Expr::col((fact_root.clone(), Alias::new("id"))).is_null())
                .add(
                    Expr::col((object.clone(), Alias::new("observed_sync_revision"))).gt(
                        Expr::col((fact_root.clone(), Alias::new("reconciled_sync_revision"))),
                    ),
                ),
        );
    let legacy_origin_pending = Cond::all()
        .add(Expr::col((object.clone(), Alias::new("facts_observed_storage_root_id"))).is_null())
        .add(
            Cond::any()
                .add(
                    Expr::col((object.clone(), Alias::new("observed_sync_revision"))).gt(
                        Expr::col((root.clone(), Alias::new("reconciled_sync_revision"))),
                    ),
                )
                .add(Expr::exists(other_roots)),
        );
    Cond::any()
        .add(relation_pending)
        .add(known_origin_pending)
        .add(legacy_origin_pending)
        .add(
            Cond::all()
                .add(
                    Expr::col((object.clone(), Alias::new("facts_observed_storage_root_id")))
                        .is_not_null(),
                )
                .add(
                    Expr::col((object.clone(), Alias::new("facts_observed_storage_root_id")))
                        .ne(storage_root_id.as_uuid()),
                ),
        )
}

#[allow(clippy::too_many_lines)] // Keeps the positive relation/object/origin proof in one auditable SQL query.
pub(crate) async fn storage_objects_are_reconciled(
    connection: &impl ConnectionTrait,
    object_ids: &[StorageObjectRecordId],
    required_root: Option<StorageRootId>,
) -> Result<bool, DbErr> {
    let requested = object_ids
        .iter()
        .map(|object_id| object_id.as_uuid())
        .collect::<HashSet<_>>();
    Ok(
        reconciled_storage_objects_with_presence(connection, object_ids, required_root, false)
            .await?
            .len()
            == requested.len(),
    )
}

pub(crate) async fn storage_objects_have_reconciled_playback_facts(
    connection: &impl ConnectionTrait,
    object_ids: &[StorageObjectRecordId],
    required_root: StorageRootId,
) -> Result<bool, DbErr> {
    let requested = object_ids
        .iter()
        .map(|object_id| object_id.as_uuid())
        .collect::<HashSet<_>>();
    Ok(
        reconciled_storage_objects_with_presence(connection, object_ids, Some(required_root), true)
            .await?
            .len()
            == requested.len(),
    )
}

pub(crate) async fn reconciled_storage_objects(
    connection: &impl ConnectionTrait,
    object_ids: &[StorageObjectRecordId],
    required_root: Option<StorageRootId>,
) -> Result<HashSet<Uuid>, DbErr> {
    reconciled_storage_objects_with_presence(connection, object_ids, required_root, false).await
}

#[allow(clippy::too_many_lines)] // Keeps the positive relation/object/origin proof in one auditable SQL query.
async fn reconciled_storage_objects_with_presence(
    connection: &impl ConnectionTrait,
    object_ids: &[StorageObjectRecordId],
    required_root: Option<StorageRootId>,
    allow_temporarily_unavailable: bool,
) -> Result<HashSet<Uuid>, DbErr> {
    let requested = object_ids
        .iter()
        .map(|object_id| object_id.as_uuid())
        .collect::<HashSet<_>>();
    let backend = connection.get_database_backend();
    let mut all_reconciled = HashSet::new();
    for chunk in requested.iter().copied().collect::<Vec<_>>().chunks(500) {
        let relation = Alias::new("publication_fact_relation");
        let object = Alias::new("publication_fact_object");
        let root = Alias::new("publication_fact_root");
        let fact_root = Alias::new("publication_fact_origin_root");
        let other_relation = Alias::new("publication_fact_other_relation");
        let other_roots = Query::select()
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
        let mut known_origin_ready = Cond::all()
            .add(
                Expr::col((object.clone(), Alias::new("facts_observed_storage_root_id")))
                    .is_not_null(),
            )
            .add(
                Expr::col((fact_root.clone(), Alias::new("reconciled_sync_revision"))).gte(
                    Expr::col((object.clone(), Alias::new("observed_sync_revision"))),
                ),
            );
        if let Some(required_root) = required_root {
            known_origin_ready = known_origin_ready.add(
                Expr::col((object.clone(), Alias::new("facts_observed_storage_root_id")))
                    .eq(required_root.as_uuid()),
            );
        }
        let legacy_origin_ready = Cond::all()
            .add(
                Expr::col((object.clone(), Alias::new("facts_observed_storage_root_id"))).is_null(),
            )
            .add(
                Expr::col((root.clone(), Alias::new("reconciled_sync_revision"))).gte(Expr::col((
                    object.clone(),
                    Alias::new("observed_sync_revision"),
                ))),
            )
            .add(Expr::exists(other_roots).not());
        let mut query = Query::select();
        query
            .distinct()
            .expr_as(
                Expr::col((object.clone(), Alias::new("id"))),
                Alias::new("storage_object_id"),
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
            .and_where(Expr::col((object.clone(), Alias::new("id"))).is_in(chunk.iter().copied()))
            .and_where(if allow_temporarily_unavailable {
                Expr::col((object.clone(), Alias::new("presence_state")))
                    .is_in(["Present", "TemporarilyUnavailable"])
            } else {
                Expr::col((object.clone(), Alias::new("presence_state"))).eq("Present")
            })
            .and_where(if allow_temporarily_unavailable {
                Expr::col((relation.clone(), Alias::new("presence_state")))
                    .is_in(["Present", "TemporarilyUnavailable"])
            } else {
                Expr::col((relation.clone(), Alias::new("presence_state"))).eq("Present")
            })
            .and_where(
                Expr::col((root.clone(), Alias::new("reconciled_sync_revision"))).gte(Expr::col((
                    relation.clone(),
                    Alias::new("observed_sync_revision"),
                ))),
            )
            .cond_where(Cond::any().add(known_origin_ready).add(legacy_origin_ready));
        if let Some(required_root) = required_root {
            query.and_where(
                Expr::col((relation, Alias::new("storage_root_id"))).eq(required_root.as_uuid()),
            );
        }
        let reconciled = connection
            .query_all(backend.build(&query))
            .await?
            .iter()
            .map(|row| row.try_get::<uuid::Uuid>("", "storage_object_id"))
            .collect::<Result<HashSet<_>, _>>()?;
        all_reconciled.extend(reconciled);
    }
    Ok(all_reconciled)
}

fn direct_scope_query(
    item_id: CatalogItemId,
    required_root: Option<StorageRootId>,
) -> SelectStatement {
    let item = Alias::new("catalog_scope_item");
    let identity = Alias::new("catalog_scope_identity");
    let object = Alias::new("catalog_scope_object");
    let relation = Alias::new("catalog_scope_relation");
    let root = Alias::new("catalog_scope_root");
    let library_root = Alias::new("catalog_scope_library_root");
    let membership = Alias::new("catalog_scope_membership");
    let library = Alias::new("catalog_scope_library");
    let mut query = Query::select();
    query.distinct();
    scope_columns(&mut query, &relation, &object, &root);
    query
        .from_as(Alias::new("catalog_items"), item.clone())
        .join_as(
            JoinType::InnerJoin,
            Alias::new("identity_matches"),
            identity.clone(),
            Condition::all()
                .add(
                    Expr::col((identity.clone(), Alias::new("candidate_catalog_item_id")))
                        .equals((item.clone(), Alias::new("id"))),
                )
                .add(Expr::col((identity.clone(), Alias::new("state"))).eq("Matched")),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("storage_objects"),
            object.clone(),
            Expr::col((object.clone(), Alias::new("id")))
                .equals((identity.clone(), Alias::new("storage_object_id"))),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("storage_root_objects"),
            relation.clone(),
            Expr::col((relation.clone(), Alias::new("storage_object_id")))
                .equals((object.clone(), Alias::new("id"))),
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
            library_root.clone(),
            Expr::col((library_root.clone(), Alias::new("storage_root_id")))
                .equals((root.clone(), Alias::new("id"))),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("library_catalog_items"),
            membership.clone(),
            Condition::all()
                .add(
                    Expr::col((membership.clone(), Alias::new("library_id")))
                        .equals((library_root, Alias::new("library_id"))),
                )
                .add(
                    Expr::col((membership.clone(), Alias::new("catalog_item_id")))
                        .equals((item.clone(), Alias::new("id"))),
                ),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("libraries"),
            library.clone(),
            Expr::col((library.clone(), Alias::new("id")))
                .equals((membership, Alias::new("library_id"))),
        )
        .and_where(Expr::col((item.clone(), Alias::new("id"))).eq(item_id.as_uuid()))
        .and_where(Expr::col((item.clone(), Alias::new("is_present"))).eq(true))
        .and_where(Expr::col((item, Alias::new("classification_state"))).eq("Matched"))
        .and_where(Expr::col((object, Alias::new("presence_state"))).eq("Present"))
        .and_where(Expr::col((relation.clone(), Alias::new("presence_state"))).eq("Present"))
        .and_where(Expr::col((library, Alias::new("is_enabled"))).eq(true));
    if let Some(required_root) = required_root {
        query.and_where(
            Expr::col((relation.clone(), Alias::new("storage_root_id")))
                .eq(required_root.as_uuid()),
        );
    }
    query
        .order_by((relation, Alias::new("storage_root_id")), Order::Asc)
        .limit(2)
        .to_owned()
}

#[allow(clippy::too_many_lines)] // Keeps the active publication and inherited authorization fence auditable.
fn structure_scope_query(
    item_id: CatalogItemId,
    required_root: Option<StorageRootId>,
) -> SelectStatement {
    let item = Alias::new("structure_scope_item");
    let owner = Alias::new("structure_scope_owner");
    let publication = Alias::new("structure_scope_publication");
    let projection = Alias::new("structure_scope_projection");
    let relation = Alias::new("structure_scope_relation");
    let object = Alias::new("structure_scope_object");
    let root = Alias::new("structure_scope_root");
    let library_root = Alias::new("structure_scope_library_root");
    let membership = Alias::new("structure_scope_membership");
    let library = Alias::new("structure_scope_library");
    let mut query = Query::select();
    query.distinct();
    scope_columns(&mut query, &relation, &object, &root);
    query
        .from_as(Alias::new("catalog_items"), item.clone())
        .join_as(
            JoinType::InnerJoin,
            Alias::new("catalog_items"),
            owner.clone(),
            Expr::col((owner.clone(), Alias::new("id")))
                .equals((item.clone(), Alias::new("structure_owner_item_id"))),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("catalog_publications"),
            publication.clone(),
            Expr::col((publication.clone(), Alias::new("id")))
                .equals((owner.clone(), Alias::new("active_structure_publication_id"))),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("publication_catalog_items"),
            projection.clone(),
            Condition::all()
                .add(
                    Expr::col((projection.clone(), Alias::new("publication_id")))
                        .equals((publication.clone(), Alias::new("id"))),
                )
                .add(
                    Expr::col((projection.clone(), Alias::new("catalog_item_id")))
                        .equals((item.clone(), Alias::new("id"))),
                ),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("storage_root_objects"),
            relation.clone(),
            Condition::all()
                .add(
                    Expr::col((relation.clone(), Alias::new("storage_root_id")))
                        .equals((projection.clone(), Alias::new("storage_root_id"))),
                )
                .add(
                    Expr::col((relation.clone(), Alias::new("storage_object_id")))
                        .equals((projection.clone(), Alias::new("scope_storage_object_id"))),
                ),
        )
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
            JoinType::InnerJoin,
            Alias::new("library_storage_roots"),
            library_root.clone(),
            Expr::col((library_root.clone(), Alias::new("storage_root_id")))
                .equals((root.clone(), Alias::new("id"))),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("library_catalog_items"),
            membership.clone(),
            Condition::all()
                .add(
                    Expr::col((membership.clone(), Alias::new("library_id")))
                        .equals((library_root, Alias::new("library_id"))),
                )
                .add(
                    Expr::col((membership.clone(), Alias::new("catalog_item_id")))
                        .equals((owner.clone(), Alias::new("id"))),
                ),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("libraries"),
            library.clone(),
            Expr::col((library.clone(), Alias::new("id")))
                .equals((membership, Alias::new("library_id"))),
        )
        .and_where(Expr::col((item.clone(), Alias::new("id"))).eq(item_id.as_uuid()))
        .and_where(Expr::col((item, Alias::new("is_present"))).eq(true))
        .and_where(Expr::col((owner.clone(), Alias::new("is_present"))).eq(true))
        .and_where(Expr::col((owner.clone(), Alias::new("classification_state"))).eq("Matched"))
        .and_where(Expr::col((publication.clone(), Alias::new("publication_kind"))).eq("Structure"))
        .and_where(Expr::col((publication.clone(), Alias::new("state"))).eq("Active"))
        .and_where(
            Expr::col((publication, Alias::new("expected_revision")))
                .equals((owner, Alias::new("structure_expansion_revision"))),
        )
        .and_where(Expr::col((object, Alias::new("presence_state"))).eq("Present"))
        .and_where(Expr::col((relation.clone(), Alias::new("presence_state"))).eq("Present"))
        .and_where(Expr::col((library, Alias::new("is_enabled"))).eq(true));
    if let Some(required_root) = required_root {
        query.and_where(
            Expr::col((relation.clone(), Alias::new("storage_root_id")))
                .eq(required_root.as_uuid()),
        );
    }
    query
        .order_by((relation, Alias::new("storage_root_id")), Order::Asc)
        .limit(2)
        .to_owned()
}
