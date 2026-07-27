use std::collections::{HashMap, HashSet, VecDeque};

use chrono::{DateTime, Utc};
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbErr, QueryResult, TransactionTrait,
    sea_query::{Alias, Expr, OnConflict, Query},
};
use serde_json::json;
use sha2::{Digest, Sha256};
use thiserror::Error;
use tjxy_common::{SortKey, StorageObjectRecordId, StorageRootId};
use tjxy_storage::{IdentityQuality, ObjectType, StorageObject, StorageObjectId};
use uuid::Uuid;

use crate::natural_key;
use crate::work_job::{
    ClaimedWorkJob, WorkJobClock, WorkJobRepository, WorkJobRepositoryError, WorkJobSpec,
    WorkJobSubmission, WorkJobSystemClock, WorkScope, WorkTaskKind,
};

const MAX_IDENTITY_CHARS: usize = 2048;
const MAX_OBJECTS_PER_PAGE: usize = 10_000;
const MAX_ENCODED_PAGE_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct StorageSyncPage {
    storage_root_id: StorageRootId,
    parent_id: StorageObjectRecordId,
    provider_drive_id: String,
    page_identity: String,
    objects: Vec<StorageObject>,
    scope_completed: bool,
    payload_sha256: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TemporaryAvailabilityReason {
    BackendObjectNotFoundUnconfirmed,
    BackendTemporarilyUnavailable,
    BackendRateLimited,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObjectAvailabilityUpdate {
    root_id: StorageRootId,
    sync_revision: i64,
}

impl ObjectAvailabilityUpdate {
    #[must_use]
    pub const fn root_id(self) -> StorageRootId {
        self.root_id
    }

    #[must_use]
    pub const fn sync_revision(self) -> i64 {
        self.sync_revision
    }
}

impl TemporaryAvailabilityReason {
    const fn as_str(self) -> &'static str {
        match self {
            Self::BackendObjectNotFoundUnconfirmed => "backend-object-not-found-unconfirmed",
            Self::BackendTemporarilyUnavailable => "backend-temporarily-unavailable",
            Self::BackendRateLimited => "backend-rate-limited",
        }
    }
}

impl StorageSyncPage {
    /// Defines one replay-safe inventory page for a root-local scope.
    ///
    /// # Errors
    ///
    /// Returns [`StorageSyncRepositoryError::InvalidPage`] for empty, oversized,
    /// or duplicate provider identities.
    pub fn new(
        storage_root_id: StorageRootId,
        parent_id: StorageObjectRecordId,
        provider_drive_id: impl Into<String>,
        page_identity: impl Into<String>,
        objects: Vec<StorageObject>,
        scope_completed: bool,
    ) -> Result<Self, StorageSyncRepositoryError> {
        let provider_drive_id = provider_drive_id.into();
        let page_identity = page_identity.into();
        if !valid_identity(&provider_drive_id) || !valid_identity(&page_identity) {
            return Err(StorageSyncRepositoryError::InvalidPage);
        }
        let mut identities = HashSet::new();
        if objects.len() > MAX_OBJECTS_PER_PAGE
            || objects.iter().any(|object| {
                !valid_identity(object.name())
                    || !identities.insert((
                        object.id().provider().to_owned(),
                        object.id().provider_object_id().to_owned(),
                    ))
            })
        {
            return Err(StorageSyncRepositoryError::InvalidPage);
        }
        let encoded = serde_json::to_vec(&(
            storage_root_id.as_uuid(),
            parent_id.as_uuid(),
            &provider_drive_id,
            &page_identity,
            &objects,
            scope_completed,
        ))
        .map_err(|_| StorageSyncRepositoryError::InvalidPage)?;
        if encoded.len() > MAX_ENCODED_PAGE_BYTES {
            return Err(StorageSyncRepositoryError::InvalidPage);
        }
        let payload_sha256 = format!("{:x}", Sha256::digest(encoded));
        Ok(Self {
            storage_root_id,
            parent_id,
            provider_drive_id,
            page_identity,
            objects,
            scope_completed,
            payload_sha256,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommittedStoragePage {
    sync_revision: i64,
    scope_completed: bool,
    replayed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScopedInventoryTarget {
    account: Uuid,
    root: StorageRootId,
    parent_record: StorageObjectRecordId,
    provider_drive: String,
    backend_parent: StorageObjectId,
}

impl ScopedInventoryTarget {
    #[must_use]
    pub const fn account_id(&self) -> Uuid {
        self.account
    }

    #[must_use]
    pub const fn root_id(&self) -> StorageRootId {
        self.root
    }

    #[must_use]
    pub const fn parent_record_id(&self) -> StorageObjectRecordId {
        self.parent_record
    }

    #[must_use]
    pub fn provider_drive_id(&self) -> &str {
        &self.provider_drive
    }

    #[must_use]
    pub const fn backend_parent_id(&self) -> &StorageObjectId {
        &self.backend_parent
    }
}

impl CommittedStoragePage {
    #[must_use]
    pub const fn sync_revision(self) -> i64 {
        self.sync_revision
    }

    #[must_use]
    pub const fn scope_completed(self) -> bool {
        self.scope_completed
    }

    #[must_use]
    pub const fn replayed(self) -> bool {
        self.replayed
    }
}

pub struct StorageSyncRepository<'connection, Clock = WorkJobSystemClock> {
    database: &'connection DatabaseConnection,
    clock: Clock,
}

impl<'connection> StorageSyncRepository<'connection, WorkJobSystemClock> {
    #[must_use]
    pub const fn new(database: &'connection DatabaseConnection) -> Self {
        Self {
            database,
            clock: WorkJobSystemClock,
        }
    }
}

impl<'connection, Clock> StorageSyncRepository<'connection, Clock>
where
    Clock: WorkJobClock,
{
    #[must_use]
    pub const fn with_clock(database: &'connection DatabaseConnection, clock: Clock) -> Self {
        Self { database, clock }
    }

    /// Enqueues or joins an explicit full validation for one live storage root.
    ///
    /// # Errors
    ///
    /// Returns [`StorageSyncRepositoryError`] when the root is unavailable or work cannot be
    /// durably enqueued.
    pub async fn enqueue_validation(
        &self,
        root_id: StorageRootId,
        priority: i32,
    ) -> Result<WorkJobSubmission, StorageSyncRepositoryError> {
        let root = Alias::new("validation_enqueue_root");
        let account = Alias::new("validation_enqueue_account");
        let relation = Alias::new("validation_enqueue_relation");
        let object = Alias::new("validation_enqueue_object");
        let query = Query::select()
            .expr_as(
                Expr::col((root.clone(), Alias::new("sync_revision"))),
                Alias::new("sync_revision"),
            )
            .from_as(Alias::new("storage_roots"), root.clone())
            .join_as(
                sea_orm::sea_query::JoinType::InnerJoin,
                Alias::new("storage_accounts"),
                account.clone(),
                Expr::col((account.clone(), Alias::new("id")))
                    .equals((root.clone(), Alias::new("storage_account_id"))),
            )
            .join_as(
                sea_orm::sea_query::JoinType::InnerJoin,
                Alias::new("storage_root_objects"),
                relation.clone(),
                Expr::col((relation.clone(), Alias::new("storage_root_id")))
                    .equals((root.clone(), Alias::new("id"))),
            )
            .join_as(
                sea_orm::sea_query::JoinType::InnerJoin,
                Alias::new("storage_objects"),
                object.clone(),
                Expr::col((object.clone(), Alias::new("id")))
                    .equals((relation.clone(), Alias::new("storage_object_id"))),
            )
            .and_where(Expr::col((root.clone(), Alias::new("id"))).eq(root_id.as_uuid()))
            .and_where(Expr::col((account, Alias::new("status"))).eq("Active"))
            .and_where(
                Expr::col((object.clone(), Alias::new("storage_account_id")))
                    .equals((root, Alias::new("storage_account_id"))),
            )
            .and_where(
                Expr::col((relation.clone(), Alias::new("parent_storage_object_id"))).is_null(),
            )
            .and_where(
                Expr::col((relation, Alias::new("presence_state")))
                    .is_in(["Present", "TemporarilyUnavailable"]),
            )
            .and_where(Expr::col((object.clone(), Alias::new("object_type"))).eq("Directory"))
            .and_where(Expr::col((object, Alias::new("presence_state"))).eq("Present"))
            .limit(1)
            .to_owned();
        let revision = self
            .database
            .query_one(self.database.get_database_backend().build(&query))
            .await?
            .ok_or(StorageSyncRepositoryError::MissingScope)?
            .try_get::<i64>("", "sync_revision")?;
        let spec = WorkJobSpec::new(
            WorkTaskKind::ValidateStorageRoot,
            WorkScope::StorageRoot(root_id),
            revision,
            priority,
        )
        .map_err(StorageSyncRepositoryError::WorkJob)?
        .with_storage_root_affinity(root_id)
        .map_err(StorageSyncRepositoryError::WorkJob)?;
        WorkJobRepository::new(self.database)
            .enqueue_or_join(&spec)
            .await
            .map_err(StorageSyncRepositoryError::WorkJob)
    }

    /// Enqueues scoped inventory for native filesystem event hints that resolve to live,
    /// materialized directories.
    ///
    /// Unknown identities and scopes outside the selected account/drive are ignored. Repeated
    /// hints join the same active natural-key job, bounding event bursts without losing durable
    /// work once the batch is committed.
    ///
    /// # Errors
    ///
    /// Returns [`StorageSyncRepositoryError`] for invalid identities, work invariants, or SQL
    /// failures.
    #[allow(clippy::too_many_lines)] // Keeps the account/root/materialization fence in one query.
    pub async fn enqueue_event_scopes(
        &self,
        account_id: Uuid,
        provider_drive_id: &str,
        scope_ids: &[StorageObjectId],
        priority: i32,
    ) -> Result<Vec<WorkJobSubmission>, StorageSyncRepositoryError> {
        if scope_ids.len() > MAX_OBJECTS_PER_PAGE
            || !valid_identity(provider_drive_id)
            || scope_ids.iter().any(|id| {
                !valid_identity(id.provider()) || !valid_identity(id.provider_object_id())
            })
        {
            return Err(StorageSyncRepositoryError::InvalidEventScopes);
        }
        let Some(provider) = scope_ids.first().map(StorageObjectId::provider) else {
            return Ok(Vec::new());
        };
        if scope_ids.iter().any(|id| id.provider() != provider) {
            return Err(StorageSyncRepositoryError::InvalidEventScopes);
        }

        let object = Alias::new("event_scope_object");
        let relation = Alias::new("event_scope_relation");
        let root = Alias::new("event_scope_root");
        let account = Alias::new("event_scope_account");
        let library_root = Alias::new("event_scope_library_root");
        let library = Alias::new("event_scope_library");
        let query = Query::select()
            .expr_as(
                Expr::col((object.clone(), Alias::new("id"))),
                Alias::new("scope_id"),
            )
            .expr_as(
                Expr::col((relation.clone(), Alias::new("children_index_revision"))),
                Alias::new("children_index_revision"),
            )
            .expr_as(
                Expr::col((root.clone(), Alias::new("id"))),
                Alias::new("root_id"),
            )
            .from_as(Alias::new("storage_objects"), object.clone())
            .join_as(
                sea_orm::sea_query::JoinType::InnerJoin,
                Alias::new("storage_root_objects"),
                relation.clone(),
                Expr::col((relation.clone(), Alias::new("storage_object_id")))
                    .equals((object.clone(), Alias::new("id"))),
            )
            .join_as(
                sea_orm::sea_query::JoinType::InnerJoin,
                Alias::new("storage_roots"),
                root.clone(),
                Expr::col((root.clone(), Alias::new("id")))
                    .equals((relation.clone(), Alias::new("storage_root_id"))),
            )
            .join_as(
                sea_orm::sea_query::JoinType::InnerJoin,
                Alias::new("storage_accounts"),
                account.clone(),
                Expr::col((account.clone(), Alias::new("id")))
                    .equals((root.clone(), Alias::new("storage_account_id"))),
            )
            .join_as(
                sea_orm::sea_query::JoinType::InnerJoin,
                Alias::new("library_storage_roots"),
                library_root.clone(),
                Expr::col((library_root.clone(), Alias::new("storage_root_id")))
                    .equals((root, Alias::new("id"))),
            )
            .join_as(
                sea_orm::sea_query::JoinType::InnerJoin,
                Alias::new("libraries"),
                library.clone(),
                Expr::col((library.clone(), Alias::new("id")))
                    .equals((library_root, Alias::new("library_id"))),
            )
            .and_where(Expr::col((object.clone(), Alias::new("storage_account_id"))).eq(account_id))
            .and_where(
                Expr::col((object.clone(), Alias::new("provider_drive_id"))).eq(provider_drive_id),
            )
            .and_where(Expr::col((object.clone(), Alias::new("object_type"))).eq("Directory"))
            .and_where(Expr::col((object.clone(), Alias::new("presence_state"))).eq("Present"))
            .and_where(
                Expr::col((object.clone(), Alias::new("provider_object_id"))).is_in(
                    scope_ids
                        .iter()
                        .map(|id| id.provider_object_id().to_owned()),
                ),
            )
            .and_where(Expr::col((relation, Alias::new("presence_state"))).eq("Present"))
            .and_where(Expr::col((account.clone(), Alias::new("provider"))).eq(provider))
            .and_where(Expr::col((account, Alias::new("status"))).eq("Active"))
            .and_where(Expr::col((library, Alias::new("is_enabled"))).eq(true))
            .to_owned();
        let transaction = self.database.begin().await?;
        let result: Result<Vec<WorkJobSubmission>, StorageSyncRepositoryError> = async {
            let rows = transaction
                .query_all(transaction.get_database_backend().build(&query))
                .await?;
            let mut scopes = HashMap::<(StorageObjectRecordId, StorageRootId), i64>::new();
            for row in rows {
                let scope = StorageObjectRecordId::from_uuid(row.try_get("", "scope_id")?);
                let root = StorageRootId::from_uuid(row.try_get("", "root_id")?);
                let revision = row.try_get::<i64>("", "children_index_revision")?;
                scopes
                    .entry((scope, root))
                    .and_modify(|current| *current = (*current).max(revision))
                    .or_insert(revision);
            }
            let mut scopes = scopes.into_iter().collect::<Vec<_>>();
            scopes.sort_by_key(|((scope, root), _)| (root.as_uuid(), scope.as_uuid()));
            let mut submissions = Vec::with_capacity(scopes.len());
            for ((scope, root), revision) in scopes {
                let spec = WorkJobSpec::new(
                    WorkTaskKind::ScopedStorageSync,
                    WorkScope::StorageObject(scope),
                    revision,
                    priority,
                )
                .map_err(StorageSyncRepositoryError::WorkJob)?
                .with_storage_root_affinity(root)
                .map_err(StorageSyncRepositoryError::WorkJob)?;
                submissions.push(
                    crate::work_job::enqueue_in_transaction(&transaction, &spec, self.clock.now())
                        .await
                        .map_err(StorageSyncRepositoryError::WorkJob)?,
                );
            }
            Ok(submissions)
        }
        .await;
        finish(transaction, result).await
    }

    /// Resolves a live scoped-sync claim to the redacted identities needed by a backend worker.
    ///
    /// # Errors
    ///
    /// Returns [`StorageSyncRepositoryError`] for an invalid claim, corrupt stored identity, or
    /// database failure. Missing, disabled, or no-longer-present scopes return `Ok(None)`.
    #[allow(clippy::too_many_lines)] // The query pins both object and root scoped identity fences.
    pub async fn inventory_target(
        &self,
        claimed: &ClaimedWorkJob,
    ) -> Result<Option<ScopedInventoryTarget>, StorageSyncRepositoryError> {
        if !matches!(
            claimed.job().task_kind(),
            WorkTaskKind::ScopedStorageSync
                | WorkTaskKind::RecoverStorageCursor
                | WorkTaskKind::ValidateStorageRoot
        ) {
            return Err(StorageSyncRepositoryError::InvalidClaimScope);
        }
        let scope = claimed.job().scope();
        let relation = Alias::new("inventory_target_relation");
        let root = Alias::new("inventory_target_root");
        let object = Alias::new("inventory_target_object");
        let account = Alias::new("inventory_target_account");
        let enabled_binding = enabled_library_binding_for_root(&root, "inventory_target");
        let mut query = Query::select()
            .expr_as(
                Expr::col((root.clone(), Alias::new("id"))),
                Alias::new("root_id"),
            )
            .expr_as(
                Expr::col((relation.clone(), Alias::new("storage_object_id"))),
                Alias::new("parent_record_id"),
            )
            .expr_as(
                Expr::col((object.clone(), Alias::new("storage_account_id"))),
                Alias::new("account_id"),
            )
            .expr_as(
                Expr::col((object.clone(), Alias::new("provider_drive_id"))),
                Alias::new("provider_drive_id"),
            )
            .expr_as(
                Expr::col((account.clone(), Alias::new("provider"))),
                Alias::new("provider"),
            )
            .expr_as(
                Expr::col((object.clone(), Alias::new("provider_object_id"))),
                Alias::new("provider_object_id"),
            )
            .from_as(Alias::new("storage_root_objects"), relation.clone())
            .join_as(
                sea_orm::sea_query::JoinType::InnerJoin,
                Alias::new("storage_roots"),
                root.clone(),
                Expr::col((root.clone(), Alias::new("id")))
                    .equals((relation.clone(), Alias::new("storage_root_id"))),
            )
            .join_as(
                sea_orm::sea_query::JoinType::InnerJoin,
                Alias::new("storage_objects"),
                object.clone(),
                Expr::col((object.clone(), Alias::new("id")))
                    .equals((relation.clone(), Alias::new("storage_object_id"))),
            )
            .join_as(
                sea_orm::sea_query::JoinType::InnerJoin,
                Alias::new("storage_accounts"),
                account.clone(),
                Expr::col((account.clone(), Alias::new("id")))
                    .equals((root.clone(), Alias::new("storage_account_id"))),
            )
            .and_where(
                Expr::col((relation.clone(), Alias::new("presence_state")))
                    .is_in(["Present", "TemporarilyUnavailable"]),
            )
            .and_where(Expr::col((object.clone(), Alias::new("presence_state"))).eq("Present"))
            .and_where(Expr::col((object.clone(), Alias::new("object_type"))).eq("Directory"))
            .and_where(
                Expr::col((object.clone(), Alias::new("storage_account_id")))
                    .equals((account.clone(), Alias::new("id"))),
            )
            .and_where(Expr::col((account, Alias::new("status"))).eq("Active"))
            .and_where(Expr::exists(enabled_binding))
            .to_owned();
        if let Some(root_id) = claimed.job().storage_root_affinity() {
            query.and_where(Expr::col((root.clone(), Alias::new("id"))).eq(root_id.as_uuid()));
        }
        match scope {
            WorkScope::StorageObject(parent_record_id) => {
                query.and_where(
                    Expr::col((
                        Alias::new("inventory_target_relation"),
                        Alias::new("storage_object_id"),
                    ))
                    .eq(parent_record_id.as_uuid()),
                );
            }
            WorkScope::StorageRoot(root_id) => {
                query
                    .and_where(Expr::col((root, Alias::new("id"))).eq(root_id.as_uuid()))
                    .and_where(
                        Expr::col((relation, Alias::new("parent_storage_object_id"))).is_null(),
                    );
            }
            _ => return Err(StorageSyncRepositoryError::InvalidClaimScope),
        }
        let backend = self.database.get_database_backend();
        let rows = self.database.query_all(backend.build(&query)).await?;
        if rows.len() > 1 {
            return Err(StorageSyncRepositoryError::AmbiguousScope);
        }
        rows.first()
            .map(scoped_inventory_target_from_row)
            .transpose()
    }

    /// Rechecks that a claimed storage job may access one root-local directory.
    ///
    /// Workers call this immediately before each backend request so account or Library binding
    /// revocation fences subsequent pages as well as the database commit.
    ///
    /// # Errors
    ///
    /// Returns [`StorageSyncRepositoryError`] for database or stored-scope corruption.
    pub async fn inventory_scope_authorized(
        &self,
        claimed: &ClaimedWorkJob,
        root_id: StorageRootId,
        scope_id: StorageObjectRecordId,
    ) -> Result<bool, StorageSyncRepositoryError> {
        storage_claim_authorizes_scope(self.database, claimed, root_id, scope_id)
            .await
            .map_err(Into::into)
    }

    /// Commits one inventory page, root revision, and outbox events atomically.
    ///
    /// # Errors
    ///
    /// Returns [`StorageSyncRepositoryError`] for lost leases, replay conflicts,
    /// root/object invariant violations, or database failures.
    pub async fn commit_inventory_page(
        &self,
        claimed: &ClaimedWorkJob,
        page: StorageSyncPage,
    ) -> Result<CommittedStoragePage, StorageSyncRepositoryError> {
        let transaction = self.database.begin().await?;
        let result = commit_inventory_page(&transaction, claimed, &page, self.clock.now()).await;
        let result = match result {
            Ok(committed) => ensure_storage_claim(&transaction, claimed, self.clock.now())
                .await
                .map(|()| committed),
            Err(error) => Err(error),
        };
        finish(transaction, result).await
    }

    /// Records a retryable backend failure against the exact root-local inventory scope.
    ///
    /// The relation change, root revision, and availability outbox event commit atomically.
    /// Repeating the same failure while the scope is already unavailable is a fenced no-op.
    ///
    /// # Errors
    ///
    /// Returns [`StorageSyncRepositoryError`] for an invalid scope, lost lease, revision conflict,
    /// or database failure.
    pub async fn mark_scope_temporarily_unavailable(
        &self,
        claimed: &ClaimedWorkJob,
        root_id: StorageRootId,
        scope_id: StorageObjectRecordId,
        reason: TemporaryAvailabilityReason,
    ) -> Result<i64, StorageSyncRepositoryError> {
        let transaction = self.database.begin().await?;
        let result = mark_scope_temporarily_unavailable(
            &transaction,
            claimed,
            root_id,
            scope_id,
            reason,
            self.clock.now(),
        )
        .await;
        finish(transaction, result).await
    }

    /// Records a retryable ordinary object-read failure for every root that still references it.
    ///
    /// Each affected root advances through its own availability event. Repeating an identical
    /// observation is a no-op, and confirmed-absent relations are never resurrected.
    ///
    /// # Errors
    ///
    /// Returns [`StorageSyncRepositoryError`] for revision conflicts or database failures.
    pub async fn record_object_read_unavailable(
        &self,
        object_id: StorageObjectRecordId,
        reason: TemporaryAvailabilityReason,
    ) -> Result<Vec<ObjectAvailabilityUpdate>, StorageSyncRepositoryError> {
        let transaction = self.database.begin().await?;
        let result = record_object_read_availability(
            &transaction,
            object_id,
            ObjectReadAvailability::Unavailable(reason),
            self.clock.now(),
        )
        .await;
        finish(transaction, result).await
    }

    /// Restores every temporarily unavailable root relation after an ordinary object read opens.
    ///
    /// # Errors
    ///
    /// Returns [`StorageSyncRepositoryError`] for revision conflicts or database failures.
    pub async fn record_object_read_present(
        &self,
        object_id: StorageObjectRecordId,
    ) -> Result<Vec<ObjectAvailabilityUpdate>, StorageSyncRepositoryError> {
        let transaction = self.database.begin().await?;
        let result = record_object_read_availability(
            &transaction,
            object_id,
            ObjectReadAvailability::Present,
            self.clock.now(),
        )
        .await;
        finish(transaction, result).await
    }

    /// Lists direct child directories observed in the current validation.
    ///
    /// # Errors
    ///
    /// Returns [`StorageSyncRepositoryError`] for corrupt stored identities or SQL failures.
    pub async fn present_child_directories(
        &self,
        root_id: StorageRootId,
        parent_id: StorageObjectRecordId,
        first_validation_revision: i64,
    ) -> Result<Vec<ScopedInventoryTarget>, StorageSyncRepositoryError> {
        if first_validation_revision <= 0 {
            return Err(StorageSyncRepositoryError::InvalidRevision);
        }
        let relation = Alias::new("validation_child_relation");
        let root = Alias::new("validation_child_root");
        let object = Alias::new("validation_child_object");
        let account = Alias::new("validation_child_account");
        let query = Query::select()
            .expr_as(
                Expr::col((root.clone(), Alias::new("id"))),
                Alias::new("root_id"),
            )
            .expr_as(
                Expr::col((relation.clone(), Alias::new("storage_object_id"))),
                Alias::new("parent_record_id"),
            )
            .expr_as(
                Expr::col((object.clone(), Alias::new("storage_account_id"))),
                Alias::new("account_id"),
            )
            .expr_as(
                Expr::col((object.clone(), Alias::new("provider_drive_id"))),
                Alias::new("provider_drive_id"),
            )
            .expr_as(
                Expr::col((account.clone(), Alias::new("provider"))),
                Alias::new("provider"),
            )
            .expr_as(
                Expr::col((object.clone(), Alias::new("provider_object_id"))),
                Alias::new("provider_object_id"),
            )
            .from_as(Alias::new("storage_root_objects"), relation.clone())
            .join_as(
                sea_orm::sea_query::JoinType::InnerJoin,
                Alias::new("storage_roots"),
                root.clone(),
                Expr::col((root.clone(), Alias::new("id")))
                    .equals((relation.clone(), Alias::new("storage_root_id"))),
            )
            .join_as(
                sea_orm::sea_query::JoinType::InnerJoin,
                Alias::new("storage_objects"),
                object.clone(),
                Expr::col((object.clone(), Alias::new("id")))
                    .equals((relation.clone(), Alias::new("storage_object_id"))),
            )
            .join_as(
                sea_orm::sea_query::JoinType::InnerJoin,
                Alias::new("storage_accounts"),
                account.clone(),
                Expr::col((account.clone(), Alias::new("id")))
                    .equals((object.clone(), Alias::new("storage_account_id"))),
            )
            .and_where(Expr::col((root, Alias::new("id"))).eq(root_id.as_uuid()))
            .and_where(
                Expr::col((relation.clone(), Alias::new("parent_storage_object_id")))
                    .eq(parent_id.as_uuid()),
            )
            .and_where(Expr::col((relation.clone(), Alias::new("presence_state"))).eq("Present"))
            .and_where(
                Expr::col((relation, Alias::new("observed_sync_revision")))
                    .gte(first_validation_revision),
            )
            .and_where(Expr::col((object.clone(), Alias::new("object_type"))).eq("Directory"))
            .and_where(Expr::col((object, Alias::new("presence_state"))).eq("Present"))
            .and_where(Expr::col((account, Alias::new("status"))).eq("Active"))
            .order_by(
                Alias::new("parent_record_id"),
                sea_orm::sea_query::Order::Asc,
            )
            .to_owned();
        let backend = self.database.get_database_backend();
        self.database
            .query_all(backend.build(&query))
            .await?
            .iter()
            .map(scoped_inventory_target_from_row)
            .collect()
    }

    /// Commits the final unreachable-subtree sweep for a full root validation.
    ///
    /// # Errors
    ///
    /// Returns [`StorageSyncRepositoryError`] for an invalid claim, revision conflict, lost lease,
    /// corrupt relation, or SQL failure.
    pub async fn commit_validation_sweep(
        &self,
        claimed: &ClaimedWorkJob,
        root_id: StorageRootId,
        root_object_id: StorageObjectRecordId,
        first_validation_revision: i64,
    ) -> Result<CommittedStoragePage, StorageSyncRepositoryError> {
        let transaction = self.database.begin().await?;
        let result = commit_validation_sweep(
            &transaction,
            claimed,
            root_id,
            root_object_id,
            first_validation_revision,
            self.clock.now(),
        )
        .await;
        finish(transaction, result).await
    }
}

fn scoped_inventory_target_from_row(
    row: &QueryResult,
) -> Result<ScopedInventoryTarget, StorageSyncRepositoryError> {
    let provider: String = row.try_get("", "provider")?;
    let provider_object_id: String = row.try_get("", "provider_object_id")?;
    Ok(ScopedInventoryTarget {
        account: row.try_get("", "account_id")?,
        root: StorageRootId::from_uuid(row.try_get("", "root_id")?),
        parent_record: StorageObjectRecordId::from_uuid(row.try_get("", "parent_record_id")?),
        provider_drive: row.try_get("", "provider_drive_id")?,
        backend_parent: StorageObjectId::new(provider, provider_object_id)
            .map_err(|_| StorageSyncRepositoryError::InvalidStoredIdentity)?,
    })
}

fn enabled_library_binding_for_root(
    root: &Alias,
    alias_prefix: &str,
) -> sea_orm::sea_query::SelectStatement {
    let binding = Alias::new(format!("{alias_prefix}_enabled_binding"));
    let library = Alias::new(format!("{alias_prefix}_enabled_library"));
    Query::select()
        .expr(Expr::val(1_i32))
        .from_as(Alias::new("library_storage_roots"), binding.clone())
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("libraries"),
            library.clone(),
            Expr::col((library.clone(), Alias::new("id")))
                .equals((binding.clone(), Alias::new("library_id"))),
        )
        .and_where(
            Expr::col((binding, Alias::new("storage_root_id")))
                .equals((root.clone(), Alias::new("id"))),
        )
        .and_where(Expr::col((library, Alias::new("is_enabled"))).eq(true))
        .to_owned()
}

async fn storage_claim_authorizes_scope(
    connection: &impl ConnectionTrait,
    claimed: &ClaimedWorkJob,
    root_id: StorageRootId,
    scope_id: StorageObjectRecordId,
) -> Result<bool, DbErr> {
    if claimed
        .job()
        .storage_root_affinity()
        .is_some_and(|affinity| affinity != root_id)
    {
        return Ok(false);
    }
    let relation = Alias::new("authorized_scope_relation");
    let root = Alias::new("authorized_scope_root");
    let object = Alias::new("authorized_scope_object");
    let account = Alias::new("authorized_scope_account");
    let enabled_binding = enabled_library_binding_for_root(&root, "authorized_scope");
    let query = Query::select()
        .expr_as(
            Expr::col((root.clone(), Alias::new("id"))),
            Alias::new("root_id"),
        )
        .from_as(Alias::new("storage_root_objects"), relation.clone())
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("storage_roots"),
            root.clone(),
            Expr::col((root.clone(), Alias::new("id")))
                .equals((relation.clone(), Alias::new("storage_root_id"))),
        )
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("storage_objects"),
            object.clone(),
            Expr::col((object.clone(), Alias::new("id")))
                .equals((relation.clone(), Alias::new("storage_object_id"))),
        )
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("storage_accounts"),
            account.clone(),
            Expr::col((account.clone(), Alias::new("id")))
                .equals((root.clone(), Alias::new("storage_account_id"))),
        )
        .and_where(
            Expr::col((relation.clone(), Alias::new("storage_object_id"))).eq(scope_id.as_uuid()),
        )
        .and_where(
            Expr::col((relation, Alias::new("presence_state")))
                .is_in(["Present", "TemporarilyUnavailable"]),
        )
        .and_where(Expr::col((object.clone(), Alias::new("presence_state"))).eq("Present"))
        .and_where(Expr::col((object.clone(), Alias::new("object_type"))).eq("Directory"))
        .and_where(
            Expr::col((object, Alias::new("storage_account_id")))
                .equals((account.clone(), Alias::new("id"))),
        )
        .and_where(Expr::col((account, Alias::new("status"))).eq("Active"))
        .and_where(Expr::exists(enabled_binding))
        .order_by((root, Alias::new("id")), sea_orm::sea_query::Order::Asc)
        .limit(2)
        .to_owned();
    let backend = connection.get_database_backend();
    let roots = connection
        .query_all(backend.build(&query))
        .await?
        .iter()
        .map(|row| {
            row.try_get::<Uuid>("", "root_id")
                .map(StorageRootId::from_uuid)
        })
        .collect::<Result<Vec<_>, _>>()?;
    if !roots.contains(&root_id) {
        return Ok(false);
    }
    Ok(claimed.job().storage_root_affinity().is_some()
        || !matches!(claimed.job().scope(), WorkScope::StorageObject(_))
        || roots.len() == 1)
}

async fn ensure_storage_claim(
    transaction: &DatabaseTransaction,
    claimed: &ClaimedWorkJob,
    now: DateTime<Utc>,
) -> Result<(), StorageSyncRepositoryError> {
    match crate::work_job::fence_live_claim(transaction, claimed, now).await {
        Ok(()) => Ok(()),
        Err(WorkJobRepositoryError::LostLease) => Err(StorageSyncRepositoryError::LostLease),
        Err(error) => Err(StorageSyncRepositoryError::WorkJob(error)),
    }
}

#[derive(Debug, Error)]
pub enum StorageSyncRepositoryError {
    #[error("storage inventory page contains invalid identities or duplicate objects")]
    InvalidPage,
    #[error("filesystem event scopes contain invalid or excessive identities")]
    InvalidEventScopes,
    #[error("claimed job is not a scoped sync for this storage root")]
    InvalidClaimScope,
    #[error("storage sync work lease is expired or no longer owned by this claim")]
    LostLease,
    #[error("storage root or scoped parent is missing")]
    MissingScope,
    #[error("storage work resolves to more than one authorized root")]
    AmbiguousScope,
    #[error("stored storage scope contains an invalid backend identity")]
    InvalidStoredIdentity,
    #[error("storage object provider does not match the root account")]
    ProviderMismatch,
    #[error("storage object size is outside the supported database range")]
    InvalidObjectSize,
    #[error("storage root revision is outside the supported range")]
    InvalidRevision,
    #[error("storage root revision changed during page commit")]
    RevisionConflict,
    #[error("replayed page identity has different content or scope")]
    PageReplayConflict,
    #[error("work job validation failed: {0}")]
    WorkJob(WorkJobRepositoryError),
    #[error("database operation failed: {0}")]
    Database(#[from] DbErr),
    #[error("rollback failed after {original}: {rollback}")]
    RollbackFailed { original: String, rollback: DbErr },
}

async fn commit_inventory_page(
    transaction: &DatabaseTransaction,
    claimed: &ClaimedWorkJob,
    page: &StorageSyncPage,
    now: DateTime<Utc>,
) -> Result<CommittedStoragePage, StorageSyncRepositoryError> {
    let valid_scope = match claimed.job().scope() {
        WorkScope::StorageObject(parent_id) => parent_id == page.parent_id,
        WorkScope::StorageRoot(root_id) => root_id == page.storage_root_id,
        _ => false,
    };
    if !matches!(
        claimed.job().task_kind(),
        WorkTaskKind::ScopedStorageSync
            | WorkTaskKind::RecoverStorageCursor
            | WorkTaskKind::ValidateStorageRoot
    ) || !valid_scope
    {
        return Err(StorageSyncRepositoryError::InvalidClaimScope);
    }
    ensure_storage_claim(transaction, claimed, now).await?;
    if !storage_claim_authorizes_scope(transaction, claimed, page.storage_root_id, page.parent_id)
        .await?
    {
        return Err(StorageSyncRepositoryError::MissingScope);
    }
    if let Some(replayed) = replayed_page(transaction, claimed, page).await? {
        return Ok(replayed);
    }
    let root = read_root(transaction, page.storage_root_id)
        .await?
        .ok_or(StorageSyncRepositoryError::MissingScope)?;
    let parent_provider_id = read_scope_parent(
        transaction,
        page.storage_root_id,
        page.parent_id,
        root.account_id,
        &page.provider_drive_id,
    )
    .await?
    .ok_or(StorageSyncRepositoryError::MissingScope)?;
    let sync_revision = root
        .sync_revision
        .checked_add(1)
        .ok_or(StorageSyncRepositoryError::InvalidRevision)?;
    let first_job_revision = first_job_revision(transaction, claimed)
        .await?
        .unwrap_or(sync_revision);
    let absence_policy = if claimed.job().task_kind() == WorkTaskKind::ValidateStorageRoot {
        AbsencePolicy::Deferred
    } else {
        AbsencePolicy::Immediate { first_job_revision }
    };
    persist_page_contents(
        transaction,
        page,
        &root,
        &parent_provider_id,
        sync_revision,
        absence_policy,
        now,
    )
    .await?;
    advance_root_and_record_page(
        transaction,
        claimed,
        page,
        root.sync_revision,
        sync_revision,
        now,
    )
    .await?;
    Ok(CommittedStoragePage {
        sync_revision,
        scope_completed: page.scope_completed,
        replayed: false,
    })
}

async fn mark_scope_temporarily_unavailable(
    transaction: &DatabaseTransaction,
    claimed: &ClaimedWorkJob,
    root_id: StorageRootId,
    scope_id: StorageObjectRecordId,
    reason: TemporaryAvailabilityReason,
    now: DateTime<Utc>,
) -> Result<i64, StorageSyncRepositoryError> {
    let valid_scope = match (claimed.job().task_kind(), claimed.job().scope()) {
        (WorkTaskKind::ScopedStorageSync, WorkScope::StorageObject(claimed_scope)) => {
            claimed_scope == scope_id
        }
        (
            WorkTaskKind::RecoverStorageCursor | WorkTaskKind::ValidateStorageRoot,
            WorkScope::StorageRoot(claimed_root),
        ) => claimed_root == root_id,
        _ => false,
    };
    if !valid_scope {
        return Err(StorageSyncRepositoryError::InvalidClaimScope);
    }
    ensure_storage_claim(transaction, claimed, now).await?;
    if !storage_claim_authorizes_scope(transaction, claimed, root_id, scope_id).await? {
        return Err(StorageSyncRepositoryError::MissingScope);
    }
    let root = read_root(transaction, root_id)
        .await?
        .ok_or(StorageSyncRepositoryError::MissingScope)?;
    let relation = Query::select()
        .columns([
            Alias::new("presence_state"),
            Alias::new("availability_reason"),
        ])
        .from(Alias::new("storage_root_objects"))
        .and_where(Expr::col(Alias::new("storage_root_id")).eq(root_id.as_uuid()))
        .and_where(Expr::col(Alias::new("storage_object_id")).eq(scope_id.as_uuid()))
        .limit(1)
        .to_owned();
    let backend = transaction.get_database_backend();
    let row = transaction
        .query_one(backend.build(&relation))
        .await?
        .ok_or(StorageSyncRepositoryError::MissingScope)?;
    let presence: String = row.try_get("", "presence_state")?;
    let current_reason: Option<String> = row.try_get("", "availability_reason")?;
    if presence == "TemporarilyUnavailable" && current_reason.as_deref() == Some(reason.as_str()) {
        ensure_storage_claim(transaction, claimed, now).await?;
        return Ok(root.sync_revision);
    }
    if !matches!(presence.as_str(), "Present" | "TemporarilyUnavailable") {
        return Err(StorageSyncRepositoryError::MissingScope);
    }
    let sync_revision = root
        .sync_revision
        .checked_add(1)
        .ok_or(StorageSyncRepositoryError::InvalidRevision)?;
    let update = Query::update()
        .table(Alias::new("storage_root_objects"))
        .value(Alias::new("presence_state"), "TemporarilyUnavailable")
        .value(Alias::new("availability_reason"), reason.as_str())
        .value(Alias::new("children_indexed"), false)
        .value(Alias::new("children_index_revision"), 0_i64)
        .value(Alias::new("observed_sync_revision"), sync_revision)
        .value(Alias::new("last_listed_at"), now)
        .and_where(Expr::col(Alias::new("storage_root_id")).eq(root_id.as_uuid()))
        .and_where(Expr::col(Alias::new("storage_object_id")).eq(scope_id.as_uuid()))
        .and_where(Expr::col(Alias::new("presence_state")).eq(presence))
        .to_owned();
    if transaction
        .execute(backend.build(&update))
        .await?
        .rows_affected()
        != 1
    {
        return Err(StorageSyncRepositoryError::RevisionConflict);
    }
    insert_availability_changed_event(
        transaction,
        root_id,
        scope_id,
        sync_revision,
        "TemporarilyUnavailable",
        Some(reason.as_str()),
        now,
    )
    .await?;
    advance_root_revision(transaction, root_id, root.sync_revision, sync_revision).await?;
    ensure_storage_claim(transaction, claimed, now).await?;
    Ok(sync_revision)
}

#[derive(Clone, Copy)]
enum ObjectReadAvailability {
    Present,
    Unavailable(TemporaryAvailabilityReason),
}

async fn record_object_read_availability(
    transaction: &DatabaseTransaction,
    object_id: StorageObjectRecordId,
    availability: ObjectReadAvailability,
    now: DateTime<Utc>,
) -> Result<Vec<ObjectAvailabilityUpdate>, StorageSyncRepositoryError> {
    let relation = Alias::new("read_availability_relation");
    let root = Alias::new("read_availability_root");
    let query = Query::select()
        .expr_as(
            Expr::col((root.clone(), Alias::new("id"))),
            Alias::new("root_id"),
        )
        .expr_as(
            Expr::col((root.clone(), Alias::new("sync_revision"))),
            Alias::new("sync_revision"),
        )
        .expr_as(
            Expr::col((relation.clone(), Alias::new("presence_state"))),
            Alias::new("presence_state"),
        )
        .expr_as(
            Expr::col((relation.clone(), Alias::new("availability_reason"))),
            Alias::new("availability_reason"),
        )
        .from_as(Alias::new("storage_root_objects"), relation.clone())
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("storage_roots"),
            root,
            Expr::col((relation.clone(), Alias::new("storage_root_id")))
                .equals((Alias::new("read_availability_root"), Alias::new("id"))),
        )
        .and_where(
            Expr::col((relation.clone(), Alias::new("storage_object_id"))).eq(object_id.as_uuid()),
        )
        .and_where(
            Expr::col((relation, Alias::new("presence_state")))
                .is_in(["Present", "TemporarilyUnavailable"]),
        )
        .to_owned();
    let backend = transaction.get_database_backend();
    let rows = transaction.query_all(backend.build(&query)).await?;
    let mut updates = Vec::new();
    for row in rows {
        let root_id = StorageRootId::from_uuid(row.try_get("", "root_id")?);
        let previous_revision: i64 = row.try_get("", "sync_revision")?;
        let presence: String = row.try_get("", "presence_state")?;
        let current_reason: Option<String> = row.try_get("", "availability_reason")?;
        let (next_presence, next_reason) = match availability {
            ObjectReadAvailability::Present if presence == "TemporarilyUnavailable" => {
                ("Present", None)
            }
            ObjectReadAvailability::Unavailable(reason)
                if presence != "TemporarilyUnavailable"
                    || current_reason.as_deref() != Some(reason.as_str()) =>
            {
                ("TemporarilyUnavailable", Some(reason.as_str()))
            }
            ObjectReadAvailability::Present | ObjectReadAvailability::Unavailable(_) => continue,
        };
        let sync_revision = previous_revision
            .checked_add(1)
            .ok_or(StorageSyncRepositoryError::InvalidRevision)?;
        let mut update = Query::update();
        update
            .table(Alias::new("storage_root_objects"))
            .value(Alias::new("presence_state"), next_presence)
            .value(Alias::new("availability_reason"), next_reason)
            .and_where(Expr::col(Alias::new("storage_root_id")).eq(root_id.as_uuid()))
            .and_where(Expr::col(Alias::new("storage_object_id")).eq(object_id.as_uuid()))
            .and_where(Expr::col(Alias::new("presence_state")).eq(presence));
        if matches!(availability, ObjectReadAvailability::Unavailable(_)) {
            update
                .value(Alias::new("children_indexed"), false)
                .value(Alias::new("children_index_revision"), 0_i64);
        }
        if transaction
            .execute(backend.build(&update))
            .await?
            .rows_affected()
            != 1
        {
            return Err(StorageSyncRepositoryError::RevisionConflict);
        }
        insert_availability_changed_event(
            transaction,
            root_id,
            object_id,
            sync_revision,
            next_presence,
            next_reason,
            now,
        )
        .await?;
        advance_root_revision(transaction, root_id, previous_revision, sync_revision).await?;
        updates.push(ObjectAvailabilityUpdate {
            root_id,
            sync_revision,
        });
    }
    Ok(updates)
}

#[allow(clippy::too_many_lines)] // The root sweep and its outbox marker form one revision boundary.
async fn commit_validation_sweep(
    transaction: &DatabaseTransaction,
    claimed: &ClaimedWorkJob,
    root_id: StorageRootId,
    root_object_id: StorageObjectRecordId,
    first_validation_revision: i64,
    now: DateTime<Utc>,
) -> Result<CommittedStoragePage, StorageSyncRepositoryError> {
    if claimed.job().task_kind() != WorkTaskKind::ValidateStorageRoot
        || claimed.job().scope() != WorkScope::StorageRoot(root_id)
        || claimed
            .job()
            .storage_root_affinity()
            .is_some_and(|affinity| affinity != root_id)
        || first_validation_revision <= 0
    {
        return Err(StorageSyncRepositoryError::InvalidClaimScope);
    }
    ensure_storage_claim(transaction, claimed, now).await?;
    if !storage_claim_authorizes_scope(transaction, claimed, root_id, root_object_id).await? {
        return Err(StorageSyncRepositoryError::MissingScope);
    }
    let root = read_root(transaction, root_id)
        .await?
        .ok_or(StorageSyncRepositoryError::MissingScope)?;
    let sync_revision = root
        .sync_revision
        .checked_add(1)
        .ok_or(StorageSyncRepositoryError::InvalidRevision)?;
    let targets = validation_sweep_targets(transaction, root_id, first_validation_revision).await?;
    let backend = transaction.get_database_backend();
    for target in targets {
        let update = Query::update()
            .table(Alias::new("storage_root_objects"))
            .value(Alias::new("presence_state"), "ConfirmedAbsent")
            .value(
                Alias::new("availability_reason"),
                "not-observed-in-full-validation",
            )
            .value(Alias::new("observed_sync_revision"), sync_revision)
            .value(Alias::new("last_listed_at"), now)
            .and_where(Expr::col(Alias::new("storage_root_id")).eq(root_id.as_uuid()))
            .and_where(Expr::col(Alias::new("storage_object_id")).eq(target.object_id.as_uuid()))
            .and_where(
                Expr::col(Alias::new("parent_storage_object_id")).eq(target.parent_id.as_uuid()),
            )
            .and_where(
                Expr::col(Alias::new("presence_state"))
                    .is_in(["Present", "TemporarilyUnavailable"]),
            )
            .to_owned();
        if transaction
            .execute(backend.build(&update))
            .await?
            .rows_affected()
            != 1
        {
            return Err(StorageSyncRepositoryError::RevisionConflict);
        }
        insert_outbox_event(
            transaction,
            OutboxEventDraft {
                root_id,
                sync_revision,
                object_id: target.object_id,
                event_type: "Removed",
                before_revision: target.remote_revision.as_deref(),
                after_revision: None,
                payload: json!({
                    "version": 1,
                    "kind": "Removed",
                    "relation": {
                        "storage_root_id": root_id,
                        "storage_object_id": target.object_id,
                        "parent_storage_object_id": target.parent_id,
                    },
                    "before": {"remote_revision": target.remote_revision},
                    "after": {"presence_state": "ConfirmedAbsent"},
                    "cause": "full-validation",
                }),
                now,
            },
        )
        .await?;
    }
    insert_outbox_event(
        transaction,
        OutboxEventDraft {
            root_id,
            sync_revision,
            object_id: root_object_id,
            event_type: "ValidationCompleted",
            before_revision: None,
            after_revision: None,
            payload: json!({
                "version": 1,
                "kind": "ValidationCompleted",
                "storage_root_id": root_id,
                "first_validation_revision": first_validation_revision,
                "sync_revision": sync_revision,
            }),
            now,
        },
    )
    .await?;
    let advance = Query::update()
        .table(Alias::new("storage_roots"))
        .value(Alias::new("sync_revision"), sync_revision)
        .and_where(Expr::col(Alias::new("id")).eq(root_id.as_uuid()))
        .and_where(Expr::col(Alias::new("sync_revision")).eq(root.sync_revision))
        .to_owned();
    if transaction
        .execute(backend.build(&advance))
        .await?
        .rows_affected()
        != 1
    {
        return Err(StorageSyncRepositoryError::RevisionConflict);
    }
    ensure_storage_claim(transaction, claimed, now).await?;
    Ok(CommittedStoragePage {
        sync_revision,
        scope_completed: true,
        replayed: false,
    })
}

struct ValidationSweepRelation {
    object_id: StorageObjectRecordId,
    parent_id: StorageObjectRecordId,
    observed_revision: i64,
    remote_revision: Option<String>,
}

async fn validation_sweep_targets(
    transaction: &DatabaseTransaction,
    root_id: StorageRootId,
    first_validation_revision: i64,
) -> Result<Vec<ValidationSweepRelation>, StorageSyncRepositoryError> {
    let relation = Alias::new("validation_sweep_relation");
    let object = Alias::new("validation_sweep_object");
    let query = Query::select()
        .expr_as(
            Expr::col((relation.clone(), Alias::new("storage_object_id"))),
            Alias::new("storage_object_id"),
        )
        .expr_as(
            Expr::col((relation.clone(), Alias::new("parent_storage_object_id"))),
            Alias::new("parent_storage_object_id"),
        )
        .expr_as(
            Expr::col((object.clone(), Alias::new("remote_revision"))),
            Alias::new("remote_revision"),
        )
        .expr_as(
            Expr::col((relation.clone(), Alias::new("observed_sync_revision"))),
            Alias::new("observed_sync_revision"),
        )
        .from_as(Alias::new("storage_root_objects"), relation.clone())
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("storage_objects"),
            object.clone(),
            Expr::col((object, Alias::new("id")))
                .equals((relation.clone(), Alias::new("storage_object_id"))),
        )
        .and_where(
            Expr::col((relation.clone(), Alias::new("storage_root_id"))).eq(root_id.as_uuid()),
        )
        .and_where(
            Expr::col((relation.clone(), Alias::new("parent_storage_object_id"))).is_not_null(),
        )
        .and_where(
            Expr::col((relation.clone(), Alias::new("presence_state")))
                .is_in(["Present", "TemporarilyUnavailable"]),
        )
        .order_by(
            (relation, Alias::new("storage_object_id")),
            sea_orm::sea_query::Order::Asc,
        )
        .to_owned();
    let backend = transaction.get_database_backend();
    let relations = transaction
        .query_all(backend.build(&query))
        .await?
        .into_iter()
        .map(|row| {
            Ok(ValidationSweepRelation {
                object_id: StorageObjectRecordId::from_uuid(row.try_get("", "storage_object_id")?),
                parent_id: StorageObjectRecordId::from_uuid(
                    row.try_get("", "parent_storage_object_id")?,
                ),
                observed_revision: row.try_get("", "observed_sync_revision")?,
                remote_revision: row.try_get("", "remote_revision")?,
            })
        })
        .collect::<Result<Vec<_>, StorageSyncRepositoryError>>()?;
    let mut children = HashMap::<StorageObjectRecordId, Vec<StorageObjectRecordId>>::new();
    let mut absent = HashSet::new();
    let mut pending = VecDeque::new();
    for relation in &relations {
        children
            .entry(relation.parent_id)
            .or_default()
            .push(relation.object_id);
        if relation.observed_revision < first_validation_revision
            && absent.insert(relation.object_id)
        {
            pending.push_back(relation.object_id);
        }
    }
    while let Some(parent_id) = pending.pop_front() {
        for child_id in children.get(&parent_id).into_iter().flatten() {
            if absent.insert(*child_id) {
                pending.push_back(*child_id);
            }
        }
    }
    Ok(relations
        .into_iter()
        .filter(|relation| absent.contains(&relation.object_id))
        .collect())
}

async fn persist_page_contents(
    transaction: &DatabaseTransaction,
    page: &StorageSyncPage,
    root: &RootRecord,
    parent_provider_id: &str,
    sync_revision: i64,
    absence_policy: AbsencePolicy,
    now: DateTime<Utc>,
) -> Result<(), StorageSyncRepositoryError> {
    restore_scope_presence(
        transaction,
        page.storage_root_id,
        page.parent_id,
        sync_revision,
        now,
    )
    .await?;
    for object in &page.objects {
        persist_inventory_object(
            transaction,
            page,
            root,
            parent_provider_id,
            object,
            sync_revision,
            now,
        )
        .await?;
    }
    insert_outbox_event(
        transaction,
        OutboxEventDraft {
            root_id: page.storage_root_id,
            sync_revision,
            object_id: page.parent_id,
            event_type: "InventoryPageCommitted",
            before_revision: None,
            after_revision: None,
            payload: json!({
                "version": 1,
                "kind": "InventoryPageCommitted",
                "relation": {
                    "storage_root_id": page.storage_root_id,
                    "parent_storage_object_id": page.parent_id,
                },
                "scope_completed": page.scope_completed,
            }),
            now,
        },
    )
    .await?;
    complete_inventory_scope(transaction, page, absence_policy, sync_revision, now).await?;
    Ok(())
}

async fn persist_inventory_object(
    transaction: &DatabaseTransaction,
    page: &StorageSyncPage,
    root: &RootRecord,
    parent_provider_id: &str,
    object: &StorageObject,
    sync_revision: i64,
    now: DateTime<Utc>,
) -> Result<(), StorageSyncRepositoryError> {
    if object.id().provider() != root.provider {
        return Err(StorageSyncRepositoryError::ProviderMismatch);
    }
    let stored = upsert_object(
        transaction,
        page.storage_root_id,
        root,
        parent_provider_id,
        &page.provider_drive_id,
        object,
        sync_revision,
        now,
    )
    .await?;
    let previous_parent = upsert_root_object(
        transaction,
        page.storage_root_id,
        stored.id,
        page.parent_id,
        sync_revision,
        now,
    )
    .await?;
    if let Some(previous_parent) = previous_parent
        && previous_parent != page.parent_id
    {
        insert_outbox_event(
            transaction,
            OutboxEventDraft {
                root_id: page.storage_root_id,
                sync_revision,
                object_id: stored.id,
                event_type: "MovedOut",
                before_revision: stored.before_revision.as_deref(),
                after_revision: object.remote_revision(),
                payload: json!({
                    "version": 1,
                    "kind": "MovedOut",
                    "relation": {
                        "storage_root_id": page.storage_root_id,
                        "storage_object_id": stored.id,
                        "parent_storage_object_id": previous_parent,
                    },
                    "before": {"remote_revision": stored.before_revision.clone()},
                    "after": {
                        "presence_state": "TemporarilyUnavailable",
                        "availability_reason": "moved-to-materialized-parent",
                    },
                }),
                now,
            },
        )
        .await?;
    }
    insert_outbox_event(
        transaction,
        OutboxEventDraft {
            root_id: page.storage_root_id,
            sync_revision,
            object_id: stored.id,
            event_type: "Upserted",
            before_revision: stored.before_revision.as_deref(),
            after_revision: object.remote_revision(),
            payload: json!({
                "version": 1,
                "kind": "Upserted",
                "relation": {
                    "storage_root_id": page.storage_root_id,
                    "storage_object_id": stored.id,
                    "parent_storage_object_id": page.parent_id,
                },
                "before": {"remote_revision": stored.before_revision.clone()},
                "after": {
                    "provider_drive_id": page.provider_drive_id,
                    "provider_object_id": object.id().provider_object_id(),
                    "name": object.name(),
                    "object_type": object_type(object.object_type()),
                    "size": object.size(),
                    "checksum": object.checksum(),
                    "etag": object.etag(),
                    "remote_revision": object.remote_revision(),
                    "identity_quality": identity_quality(object.identity_quality()),
                    "presence_state": "Present",
                }
            }),
            now,
        },
    )
    .await?;
    Ok(())
}

async fn restore_scope_presence(
    transaction: &DatabaseTransaction,
    root_id: StorageRootId,
    scope_id: StorageObjectRecordId,
    sync_revision: i64,
    now: DateTime<Utc>,
) -> Result<(), StorageSyncRepositoryError> {
    let update = Query::update()
        .table(Alias::new("storage_root_objects"))
        .value(Alias::new("presence_state"), "Present")
        .value(Alias::new("availability_reason"), Option::<String>::None)
        .value(Alias::new("observed_sync_revision"), sync_revision)
        .value(Alias::new("last_listed_at"), now)
        .and_where(Expr::col(Alias::new("storage_root_id")).eq(root_id.as_uuid()))
        .and_where(Expr::col(Alias::new("storage_object_id")).eq(scope_id.as_uuid()))
        .and_where(Expr::col(Alias::new("presence_state")).eq("TemporarilyUnavailable"))
        .to_owned();
    let backend = transaction.get_database_backend();
    if transaction
        .execute(backend.build(&update))
        .await?
        .rows_affected()
        == 1
    {
        insert_availability_changed_event(
            transaction,
            root_id,
            scope_id,
            sync_revision,
            "Present",
            None,
            now,
        )
        .await?;
    }
    Ok(())
}

async fn insert_availability_changed_event(
    transaction: &DatabaseTransaction,
    root_id: StorageRootId,
    object_id: StorageObjectRecordId,
    sync_revision: i64,
    presence_state: &str,
    availability_reason: Option<&str>,
    now: DateTime<Utc>,
) -> Result<(), DbErr> {
    insert_outbox_event(
        transaction,
        OutboxEventDraft {
            root_id,
            sync_revision,
            object_id,
            event_type: "AvailabilityChanged",
            before_revision: None,
            after_revision: None,
            payload: json!({
                "version": 1,
                "kind": "AvailabilityChanged",
                "relation": {
                    "storage_root_id": root_id,
                    "storage_object_id": object_id,
                },
                "after": {
                    "presence_state": presence_state,
                    "availability_reason": availability_reason,
                },
            }),
            now,
        },
    )
    .await
}

#[derive(Clone, Copy)]
enum AbsencePolicy {
    Immediate { first_job_revision: i64 },
    Deferred,
}

async fn complete_inventory_scope(
    transaction: &DatabaseTransaction,
    page: &StorageSyncPage,
    absence_policy: AbsencePolicy,
    sync_revision: i64,
    now: DateTime<Utc>,
) -> Result<(), StorageSyncRepositoryError> {
    if !page.scope_completed {
        return mark_scope_incomplete(
            transaction,
            page.storage_root_id,
            page.parent_id,
            sync_revision,
            now,
        )
        .await;
    }
    if let AbsencePolicy::Immediate { first_job_revision } = absence_policy {
        mark_unobserved_children_absent(transaction, page, first_job_revision, sync_revision, now)
            .await?;
        insert_path_weak_relink_candidates(transaction, page.storage_root_id, sync_revision, now)
            .await?;
    }
    mark_scope_completed(
        transaction,
        page.storage_root_id,
        page.parent_id,
        sync_revision,
        now,
    )
    .await
}

async fn mark_scope_incomplete(
    transaction: &DatabaseTransaction,
    root_id: StorageRootId,
    parent_id: StorageObjectRecordId,
    sync_revision: i64,
    now: DateTime<Utc>,
) -> Result<(), StorageSyncRepositoryError> {
    let update = Query::update()
        .table(Alias::new("storage_root_objects"))
        .value(Alias::new("children_indexed"), false)
        .value(Alias::new("observed_sync_revision"), sync_revision)
        .value(Alias::new("last_listed_at"), now)
        .and_where(Expr::col(Alias::new("storage_root_id")).eq(root_id.as_uuid()))
        .and_where(Expr::col(Alias::new("storage_object_id")).eq(parent_id.as_uuid()))
        .to_owned();
    let backend = transaction.get_database_backend();
    if transaction
        .execute(backend.build(&update))
        .await?
        .rows_affected()
        != 1
    {
        return Err(StorageSyncRepositoryError::MissingScope);
    }
    Ok(())
}

#[allow(clippy::too_many_lines)] // Keeps evidence selection and candidate insert visibly paired.
async fn insert_path_weak_relink_candidates(
    transaction: &DatabaseTransaction,
    root_id: StorageRootId,
    sync_revision: i64,
    now: DateTime<Utc>,
) -> Result<(), StorageSyncRepositoryError> {
    let previous_relation = Alias::new("relink_previous_relation");
    let previous = Alias::new("relink_previous_object");
    let replacement_relation = Alias::new("relink_replacement_relation");
    let replacement = Alias::new("relink_replacement_object");
    let recent_revision = sync_revision.saturating_sub(1);
    let query = Query::select()
        .expr_as(
            Expr::col((previous.clone(), Alias::new("id"))),
            Alias::new("previous_id"),
        )
        .expr_as(
            Expr::col((replacement.clone(), Alias::new("id"))),
            Alias::new("replacement_id"),
        )
        .expr_as(
            Expr::col((previous.clone(), Alias::new("normalized_name"))),
            Alias::new("previous_name"),
        )
        .expr_as(
            Expr::col((replacement.clone(), Alias::new("normalized_name"))),
            Alias::new("replacement_name"),
        )
        .expr_as(
            Expr::col((previous.clone(), Alias::new("size"))),
            Alias::new("object_size"),
        )
        .expr_as(
            Expr::col((previous.clone(), Alias::new("remote_modified_at"))),
            Alias::new("previous_modified_at"),
        )
        .expr_as(
            Expr::col((replacement.clone(), Alias::new("remote_modified_at"))),
            Alias::new("replacement_modified_at"),
        )
        .expr_as(
            Expr::col((previous.clone(), Alias::new("checksum"))),
            Alias::new("previous_checksum"),
        )
        .expr_as(
            Expr::col((replacement.clone(), Alias::new("checksum"))),
            Alias::new("replacement_checksum"),
        )
        .from_as(
            Alias::new("storage_root_objects"),
            previous_relation.clone(),
        )
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("storage_objects"),
            previous.clone(),
            Expr::col((previous.clone(), Alias::new("id")))
                .equals((previous_relation.clone(), Alias::new("storage_object_id"))),
        )
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("storage_root_objects"),
            replacement_relation.clone(),
            Expr::col((replacement_relation.clone(), Alias::new("storage_root_id")))
                .equals((previous_relation.clone(), Alias::new("storage_root_id"))),
        )
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("storage_objects"),
            replacement.clone(),
            Expr::col((replacement.clone(), Alias::new("id"))).equals((
                replacement_relation.clone(),
                Alias::new("storage_object_id"),
            )),
        )
        .and_where(
            Expr::col((previous_relation.clone(), Alias::new("storage_root_id")))
                .eq(root_id.as_uuid()),
        )
        .and_where(
            Expr::col((previous_relation, Alias::new("presence_state"))).eq("ConfirmedAbsent"),
        )
        .and_where(Expr::col((replacement_relation, Alias::new("presence_state"))).eq("Present"))
        .and_where(Expr::col((previous.clone(), Alias::new("identity_quality"))).eq("PathWeak"))
        .and_where(Expr::col((replacement.clone(), Alias::new("identity_quality"))).eq("PathWeak"))
        .and_where(
            Expr::col((replacement.clone(), Alias::new("observed_sync_revision")))
                .gte(recent_revision),
        )
        .and_where(
            Expr::col((previous.clone(), Alias::new("object_type")))
                .equals((replacement.clone(), Alias::new("object_type"))),
        )
        .and_where(
            Expr::col((previous.clone(), Alias::new("size")))
                .equals((replacement.clone(), Alias::new("size"))),
        )
        .and_where(Expr::col((previous.clone(), Alias::new("size"))).is_not_null())
        .and_where(
            Expr::col((previous, Alias::new("id"))).ne(Expr::col((replacement, Alias::new("id")))),
        )
        .limit(1_000)
        .to_owned();
    let backend = transaction.get_database_backend();
    for row in transaction.query_all(backend.build(&query)).await? {
        let previous_id: Uuid = row.try_get("", "previous_id")?;
        let replacement_id: Uuid = row.try_get("", "replacement_id")?;
        let previous_name: String = row.try_get("", "previous_name")?;
        let replacement_name: String = row.try_get("", "replacement_name")?;
        let object_size: i64 = row.try_get("", "object_size")?;
        let previous_modified: Option<DateTime<Utc>> = row.try_get("", "previous_modified_at")?;
        let replacement_modified: Option<DateTime<Utc>> =
            row.try_get("", "replacement_modified_at")?;
        let previous_checksum: Option<String> = row.try_get("", "previous_checksum")?;
        let replacement_checksum: Option<String> = row.try_get("", "replacement_checksum")?;
        let same_name = previous_name == replacement_name;
        let same_modified =
            previous_modified.is_some() && previous_modified == replacement_modified;
        let same_checksum =
            previous_checksum.is_some() && previous_checksum == replacement_checksum;
        if !same_name && !same_modified && !same_checksum {
            continue;
        }
        let confidence = 0.35
            + if same_modified { 0.25 } else { 0.0 }
            + if same_name { 0.2 } else { 0.0 }
            + if same_checksum { 0.2 } else { 0.0 };
        let insert = Query::insert()
            .into_table(Alias::new("storage_relink_candidates"))
            .columns([
                Alias::new("id"),
                Alias::new("storage_root_id"),
                Alias::new("previous_storage_object_id"),
                Alias::new("replacement_storage_object_id"),
                Alias::new("confidence"),
                Alias::new("evidence"),
                Alias::new("state"),
                Alias::new("created_at"),
            ])
            .values_panic([
                Uuid::new_v4().into(),
                root_id.as_uuid().into(),
                previous_id.into(),
                replacement_id.into(),
                confidence.into(),
                json!({
                    "version": 1,
                    "size": object_size,
                    "same_modified_at": same_modified,
                    "same_normalized_name": same_name,
                    "same_checksum": same_checksum,
                })
                .into(),
                "Pending".into(),
                now.into(),
            ])
            .on_conflict(idempotent_insert_conflict(backend, "state"))
            .to_owned();
        transaction.execute(backend.build(&insert)).await?;
    }
    Ok(())
}

async fn first_job_revision(
    transaction: &DatabaseTransaction,
    claimed: &ClaimedWorkJob,
) -> Result<Option<i64>, DbErr> {
    let attempt_prefix = format!("attempt:{}:%", claimed.attempt_count());
    let query = Query::select()
        .expr_as(
            Expr::col(Alias::new("sync_revision")).min(),
            Alias::new("first_revision"),
        )
        .from(Alias::new("storage_sync_pages"))
        .and_where(Expr::col(Alias::new("job_id")).eq(claimed.id().as_uuid()))
        .and_where(Expr::col(Alias::new("page_identity")).like(attempt_prefix))
        .to_owned();
    let backend = transaction.get_database_backend();
    let Some(row) = transaction.query_one(backend.build(&query)).await? else {
        return Ok(None);
    };
    let revision: Option<i64> = row.try_get("", "first_revision")?;
    if revision.is_some() {
        return Ok(revision);
    }
    let generated_page = Query::select()
        .column(Alias::new("id"))
        .from(Alias::new("storage_sync_pages"))
        .and_where(Expr::col(Alias::new("job_id")).eq(claimed.id().as_uuid()))
        .and_where(Expr::col(Alias::new("page_identity")).like("attempt:%"))
        .limit(1)
        .to_owned();
    if transaction
        .query_one(backend.build(&generated_page))
        .await?
        .is_some()
    {
        return Ok(None);
    }
    // Direct repository callers predating attempt-qualified identities retain replay semantics.
    let fallback = Query::select()
        .expr_as(
            Expr::col(Alias::new("sync_revision")).min(),
            Alias::new("first_revision"),
        )
        .from(Alias::new("storage_sync_pages"))
        .and_where(Expr::col(Alias::new("job_id")).eq(claimed.id().as_uuid()))
        .to_owned();
    transaction
        .query_one(backend.build(&fallback))
        .await?
        .map(|row| row.try_get("", "first_revision"))
        .transpose()
        .map(Option::flatten)
}

#[allow(clippy::too_many_lines)] // Keeps direct and descendant absence events in one transaction boundary.
async fn mark_unobserved_children_absent(
    transaction: &DatabaseTransaction,
    page: &StorageSyncPage,
    first_job_revision: i64,
    sync_revision: i64,
    now: DateTime<Utc>,
) -> Result<(), StorageSyncRepositoryError> {
    let relation = Alias::new("unobserved_relation");
    let object = Alias::new("unobserved_object");
    let query = Query::select()
        .expr_as(
            Expr::col((relation.clone(), Alias::new("storage_object_id"))),
            Alias::new("storage_object_id"),
        )
        .expr_as(
            Expr::col((object.clone(), Alias::new("remote_revision"))),
            Alias::new("remote_revision"),
        )
        .from_as(Alias::new("storage_root_objects"), relation.clone())
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("storage_objects"),
            object.clone(),
            Expr::col((relation.clone(), Alias::new("storage_object_id")))
                .equals((object, Alias::new("id"))),
        )
        .and_where(
            Expr::col((relation.clone(), Alias::new("storage_root_id")))
                .eq(page.storage_root_id.as_uuid()),
        )
        .and_where(
            Expr::col((relation.clone(), Alias::new("parent_storage_object_id")))
                .eq(page.parent_id.as_uuid()),
        )
        .and_where(
            Expr::col((relation.clone(), Alias::new("presence_state")))
                .is_in(["Present", "TemporarilyUnavailable"]),
        )
        .and_where(
            Expr::col((relation, Alias::new("observed_sync_revision"))).lt(first_job_revision),
        )
        .to_owned();
    let backend = transaction.get_database_backend();
    let missing = transaction.query_all(backend.build(&query)).await?;
    for row in missing {
        let object_id = StorageObjectRecordId::from_uuid(row.try_get("", "storage_object_id")?);
        let remote_revision: Option<String> = row.try_get("", "remote_revision")?;
        let update = Query::update()
            .table(Alias::new("storage_root_objects"))
            .value(Alias::new("presence_state"), "ConfirmedAbsent")
            .value(
                Alias::new("availability_reason"),
                "not-observed-in-completed-inventory",
            )
            .value(Alias::new("observed_sync_revision"), sync_revision)
            .value(Alias::new("last_listed_at"), now)
            .and_where(Expr::col(Alias::new("storage_root_id")).eq(page.storage_root_id.as_uuid()))
            .and_where(Expr::col(Alias::new("storage_object_id")).eq(object_id.as_uuid()))
            .and_where(
                Expr::col(Alias::new("presence_state"))
                    .is_in(["Present", "TemporarilyUnavailable"]),
            )
            .and_where(Expr::col(Alias::new("observed_sync_revision")).lt(first_job_revision))
            .to_owned();
        if transaction
            .execute(backend.build(&update))
            .await?
            .rows_affected()
            != 1
        {
            continue;
        }
        insert_outbox_event(
            transaction,
            OutboxEventDraft {
                root_id: page.storage_root_id,
                sync_revision,
                object_id,
                event_type: "Removed",
                before_revision: remote_revision.as_deref(),
                after_revision: None,
                payload: json!({
                    "version": 1,
                    "kind": "Removed",
                    "relation": {
                        "storage_root_id": page.storage_root_id,
                        "storage_object_id": object_id,
                        "parent_storage_object_id": page.parent_id,
                    },
                    "before": {"remote_revision": remote_revision},
                    "after": {"presence_state": "ConfirmedAbsent"},
                    "cause": "completed-inventory"
                }),
                now,
            },
        )
        .await?;
        crate::storage_change_feed::cascade_descendant_relations(
            transaction,
            page.storage_root_id,
            object_id,
            sync_revision,
            now,
            crate::storage_change_feed::DescendantLoss::Removed,
        )
        .await?;
    }
    Ok(())
}

async fn advance_root_and_record_page(
    transaction: &DatabaseTransaction,
    claimed: &ClaimedWorkJob,
    page: &StorageSyncPage,
    previous_revision: i64,
    sync_revision: i64,
    now: DateTime<Utc>,
) -> Result<(), StorageSyncRepositoryError> {
    let backend = transaction.get_database_backend();
    let update_root = Query::update()
        .table(Alias::new("storage_roots"))
        .value(Alias::new("sync_revision"), sync_revision)
        .and_where(Expr::col(Alias::new("id")).eq(page.storage_root_id.as_uuid()))
        .and_where(Expr::col(Alias::new("sync_revision")).eq(previous_revision))
        .to_owned();
    if transaction
        .execute(backend.build(&update_root))
        .await?
        .rows_affected()
        != 1
    {
        return Err(StorageSyncRepositoryError::RevisionConflict);
    }
    let page_insert = Query::insert()
        .into_table(Alias::new("storage_sync_pages"))
        .columns([
            Alias::new("id"),
            Alias::new("job_id"),
            Alias::new("storage_root_id"),
            Alias::new("scope_storage_object_id"),
            Alias::new("page_identity"),
            Alias::new("payload_sha256"),
            Alias::new("sync_revision"),
            Alias::new("scope_completed"),
            Alias::new("created_at"),
        ])
        .values_panic([
            Uuid::new_v4().into(),
            claimed.id().as_uuid().into(),
            page.storage_root_id.as_uuid().into(),
            page.parent_id.as_uuid().into(),
            page.page_identity.clone().into(),
            page.payload_sha256.clone().into(),
            sync_revision.into(),
            page.scope_completed.into(),
            now.into(),
        ])
        .to_owned();
    transaction.execute(backend.build(&page_insert)).await?;
    Ok(())
}

async fn advance_root_revision(
    transaction: &DatabaseTransaction,
    root_id: StorageRootId,
    previous_revision: i64,
    sync_revision: i64,
) -> Result<(), StorageSyncRepositoryError> {
    let update = Query::update()
        .table(Alias::new("storage_roots"))
        .value(Alias::new("sync_revision"), sync_revision)
        .and_where(Expr::col(Alias::new("id")).eq(root_id.as_uuid()))
        .and_where(Expr::col(Alias::new("sync_revision")).eq(previous_revision))
        .to_owned();
    let backend = transaction.get_database_backend();
    if transaction
        .execute(backend.build(&update))
        .await?
        .rows_affected()
        != 1
    {
        return Err(StorageSyncRepositoryError::RevisionConflict);
    }
    Ok(())
}

pub(crate) struct RootRecord {
    pub(crate) account_id: Uuid,
    pub(crate) provider: String,
    pub(crate) sync_revision: i64,
}

pub(crate) async fn read_root(
    transaction: &DatabaseTransaction,
    root_id: StorageRootId,
) -> Result<Option<RootRecord>, StorageSyncRepositoryError> {
    let root = Alias::new("root");
    let account = Alias::new("account");
    let query = Query::select()
        .from_as(Alias::new("storage_roots"), root.clone())
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("storage_accounts"),
            account.clone(),
            Expr::col((account.clone(), Alias::new("id")))
                .equals((root.clone(), Alias::new("storage_account_id"))),
        )
        .expr_as(
            Expr::col((root.clone(), Alias::new("storage_account_id"))),
            Alias::new("account_id"),
        )
        .expr_as(
            Expr::col((root.clone(), Alias::new("sync_revision"))),
            Alias::new("sync_revision"),
        )
        .expr_as(
            Expr::col((account, Alias::new("provider"))),
            Alias::new("provider"),
        )
        .and_where(Expr::col((root, Alias::new("id"))).eq(root_id.as_uuid()))
        .limit(1)
        .to_owned();
    let backend = transaction.get_database_backend();
    transaction
        .query_one(backend.build(&query))
        .await?
        .as_ref()
        .map(|row| {
            Ok(RootRecord {
                account_id: row.try_get("", "account_id")?,
                provider: row.try_get("", "provider")?,
                sync_revision: row.try_get("", "sync_revision")?,
            })
        })
        .transpose()
}

async fn read_scope_parent(
    transaction: &DatabaseTransaction,
    root_id: StorageRootId,
    parent_id: StorageObjectRecordId,
    account_id: Uuid,
    provider_drive_id: &str,
) -> Result<Option<String>, DbErr> {
    let root_object = Alias::new("root_object");
    let object = Alias::new("object");
    let query = Query::select()
        .expr_as(
            Expr::col((object.clone(), Alias::new("provider_object_id"))),
            Alias::new("provider_object_id"),
        )
        .from_as(Alias::new("storage_root_objects"), root_object.clone())
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("storage_objects"),
            object.clone(),
            Expr::col((object.clone(), Alias::new("id")))
                .equals((root_object.clone(), Alias::new("storage_object_id"))),
        )
        .and_where(
            Expr::col((root_object.clone(), Alias::new("storage_root_id"))).eq(root_id.as_uuid()),
        )
        .and_where(
            Expr::col((root_object.clone(), Alias::new("storage_object_id")))
                .eq(parent_id.as_uuid()),
        )
        .and_where(
            Expr::col((root_object, Alias::new("presence_state")))
                .is_in(["Present", "TemporarilyUnavailable"]),
        )
        .and_where(Expr::col((object.clone(), Alias::new("storage_account_id"))).eq(account_id))
        .and_where(Expr::col((object, Alias::new("provider_drive_id"))).eq(provider_drive_id))
        .limit(1)
        .to_owned();
    let backend = transaction.get_database_backend();
    transaction
        .query_one(backend.build(&query))
        .await?
        .as_ref()
        .map(|row| row.try_get("", "provider_object_id"))
        .transpose()
}

async fn replayed_page(
    transaction: &DatabaseTransaction,
    claimed: &ClaimedWorkJob,
    page: &StorageSyncPage,
) -> Result<Option<CommittedStoragePage>, StorageSyncRepositoryError> {
    let query = Query::select()
        .columns([
            Alias::new("storage_root_id"),
            Alias::new("scope_storage_object_id"),
            Alias::new("payload_sha256"),
            Alias::new("sync_revision"),
            Alias::new("scope_completed"),
        ])
        .from(Alias::new("storage_sync_pages"))
        .and_where(Expr::col(Alias::new("job_id")).eq(claimed.id().as_uuid()))
        .and_where(Expr::col(Alias::new("page_identity")).eq(&page.page_identity))
        .limit(1)
        .to_owned();
    let backend = transaction.get_database_backend();
    let Some(row) = transaction.query_one(backend.build(&query)).await? else {
        return Ok(None);
    };
    let root_id: Uuid = row.try_get("", "storage_root_id")?;
    let parent_id: Uuid = row.try_get("", "scope_storage_object_id")?;
    let hash: String = row.try_get("", "payload_sha256")?;
    let scope_completed: bool = row.try_get("", "scope_completed")?;
    if root_id != page.storage_root_id.as_uuid()
        || parent_id != page.parent_id.as_uuid()
        || hash != page.payload_sha256
        || scope_completed != page.scope_completed
    {
        return Err(StorageSyncRepositoryError::PageReplayConflict);
    }
    Ok(Some(CommittedStoragePage {
        sync_revision: row.try_get("", "sync_revision")?,
        scope_completed,
        replayed: true,
    }))
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)] // Object identity and its complete provider fact set commit together.
pub(crate) async fn upsert_object(
    transaction: &DatabaseTransaction,
    root_id: StorageRootId,
    root: &RootRecord,
    parent_provider_id: &str,
    provider_drive_id: &str,
    object: &StorageObject,
    sync_revision: i64,
    now: DateTime<Utc>,
) -> Result<StoredObject, StorageSyncRepositoryError> {
    let size = object
        .size()
        .map(i64::try_from)
        .transpose()
        .map_err(|_| StorageSyncRepositoryError::InvalidObjectSize)?;
    let object_id = StorageObjectRecordId::new();
    let normalized_name = String::from_utf8(SortKey::from_text(object.name()).into_bytes())
        .map_err(|_| StorageSyncRepositoryError::InvalidPage)?;
    let identity_key = natural_key::hash(&[provider_drive_id, object.id().provider_object_id()]);
    let backend = transaction.get_database_backend();
    let existing = Query::select()
        .columns([Alias::new("id"), Alias::new("remote_revision")])
        .from(Alias::new("storage_objects"))
        .and_where(Expr::col(Alias::new("storage_account_id")).eq(root.account_id))
        .and_where(Expr::col(Alias::new("identity_key")).eq(identity_key.clone()))
        .limit(1)
        .to_owned();
    let existing_before = transaction.query_one(backend.build(&existing)).await?;
    let insert = Query::insert()
        .into_table(Alias::new("storage_objects"))
        .columns([
            Alias::new("id"),
            Alias::new("storage_account_id"),
            Alias::new("provider_drive_id"),
            Alias::new("provider_object_id"),
            Alias::new("identity_key"),
            Alias::new("provider_parent_id"),
            Alias::new("name"),
            Alias::new("normalized_name"),
            Alias::new("object_type"),
            Alias::new("mime_type"),
            Alias::new("size"),
            Alias::new("checksum"),
            Alias::new("etag"),
            Alias::new("remote_revision"),
            Alias::new("remote_modified_at"),
            Alias::new("observed_sync_revision"),
            Alias::new("facts_observed_storage_root_id"),
            Alias::new("children_indexed"),
            Alias::new("children_index_revision"),
            Alias::new("identity_quality"),
            Alias::new("presence_state"),
            Alias::new("last_listed_at"),
        ])
        .values_panic([
            object_id.as_uuid().into(),
            root.account_id.into(),
            provider_drive_id.into(),
            object.id().provider_object_id().into(),
            identity_key.clone().into(),
            parent_provider_id.into(),
            object.name().into(),
            normalized_name.clone().into(),
            object_type(object.object_type()).into(),
            object.mime_type().into(),
            size.into(),
            object.checksum().into(),
            object.etag().into(),
            object.remote_revision().into(),
            object.remote_modified_at().into(),
            sync_revision.into(),
            root_id.as_uuid().into(),
            false.into(),
            0_i64.into(),
            identity_quality(object.identity_quality()).into(),
            "Present".into(),
            now.into(),
        ])
        .on_conflict(idempotent_insert_conflict(backend, "identity_key"))
        .to_owned();
    transaction.execute(backend.build(&insert)).await?;
    let created = existing_before.is_none();
    let row = match existing_before {
        Some(row) => row,
        None => transaction
            .query_one(backend.build(&existing))
            .await?
            .ok_or(StorageSyncRepositoryError::MissingScope)?,
    };
    let existing_id: Uuid = row.try_get("", "id")?;
    let before_revision = if created {
        None
    } else {
        row.try_get("", "remote_revision")?
    };
    update_object_facts(
        transaction,
        existing_id,
        parent_provider_id,
        object,
        size,
        normalized_name,
        root_id,
        sync_revision,
        now,
    )
    .await?;
    Ok(StoredObject {
        id: StorageObjectRecordId::from_uuid(existing_id),
        before_revision,
    })
}

pub(crate) struct StoredObject {
    pub(crate) id: StorageObjectRecordId,
    pub(crate) before_revision: Option<String>,
}

#[allow(clippy::too_many_arguments)]
async fn update_object_facts(
    transaction: &DatabaseTransaction,
    object_id: Uuid,
    parent_provider_id: &str,
    object: &StorageObject,
    size: Option<i64>,
    normalized_name: String,
    root_id: StorageRootId,
    sync_revision: i64,
    now: DateTime<Utc>,
) -> Result<(), DbErr> {
    let update = Query::update()
        .table(Alias::new("storage_objects"))
        .values([
            (Alias::new("provider_parent_id"), parent_provider_id.into()),
            (Alias::new("name"), object.name().into()),
            (Alias::new("normalized_name"), normalized_name.into()),
            (
                Alias::new("object_type"),
                object_type(object.object_type()).into(),
            ),
            (Alias::new("mime_type"), object.mime_type().into()),
            (Alias::new("size"), size.into()),
            (Alias::new("checksum"), object.checksum().into()),
            (Alias::new("etag"), object.etag().into()),
            (
                Alias::new("remote_revision"),
                object.remote_revision().into(),
            ),
            (
                Alias::new("remote_modified_at"),
                object.remote_modified_at().into(),
            ),
            (Alias::new("observed_sync_revision"), sync_revision.into()),
            (
                Alias::new("facts_observed_storage_root_id"),
                root_id.as_uuid().into(),
            ),
            (
                Alias::new("identity_quality"),
                identity_quality(object.identity_quality()).into(),
            ),
            (Alias::new("presence_state"), "Present".into()),
            (
                Alias::new("availability_reason"),
                Option::<String>::None.into(),
            ),
            (Alias::new("last_listed_at"), now.into()),
        ])
        .and_where(Expr::col(Alias::new("id")).eq(object_id))
        .to_owned();
    let backend = transaction.get_database_backend();
    transaction.execute(backend.build(&update)).await?;
    Ok(())
}

pub(crate) async fn upsert_root_object(
    transaction: &DatabaseTransaction,
    root_id: StorageRootId,
    object_id: StorageObjectRecordId,
    parent_id: StorageObjectRecordId,
    sync_revision: i64,
    now: DateTime<Utc>,
) -> Result<Option<StorageObjectRecordId>, DbErr> {
    let backend = transaction.get_database_backend();
    let previous_parent = transaction
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("parent_storage_object_id"))
                    .from(Alias::new("storage_root_objects"))
                    .and_where(Expr::col(Alias::new("storage_root_id")).eq(root_id.as_uuid()))
                    .and_where(Expr::col(Alias::new("storage_object_id")).eq(object_id.as_uuid()))
                    .limit(1),
            ),
        )
        .await?
        .map(|row| row.try_get::<Option<Uuid>>("", "parent_storage_object_id"))
        .transpose()?
        .flatten()
        .map(StorageObjectRecordId::from_uuid);
    let statement = Query::insert()
        .into_table(Alias::new("storage_root_objects"))
        .columns([
            Alias::new("id"),
            Alias::new("storage_root_id"),
            Alias::new("storage_object_id"),
            Alias::new("parent_storage_object_id"),
            Alias::new("observed_sync_revision"),
            Alias::new("children_indexed"),
            Alias::new("children_index_revision"),
            Alias::new("presence_state"),
            Alias::new("availability_reason"),
            Alias::new("last_listed_at"),
        ])
        .values_panic([
            Uuid::new_v4().into(),
            root_id.as_uuid().into(),
            object_id.as_uuid().into(),
            parent_id.as_uuid().into(),
            sync_revision.into(),
            false.into(),
            0_i64.into(),
            "Present".into(),
            Option::<String>::None.into(),
            now.into(),
        ])
        .on_conflict(
            OnConflict::columns([
                Alias::new("storage_root_id"),
                Alias::new("storage_object_id"),
            ])
            .update_columns([
                Alias::new("parent_storage_object_id"),
                Alias::new("observed_sync_revision"),
                Alias::new("presence_state"),
                Alias::new("availability_reason"),
                Alias::new("last_listed_at"),
            ])
            .to_owned(),
        )
        .to_owned();
    transaction.execute(backend.build(&statement)).await?;
    if let Some(previous_parent) = previous_parent
        && previous_parent != parent_id
    {
        let update = Query::update()
            .table(Alias::new("storage_root_objects"))
            .value(Alias::new("observed_sync_revision"), sync_revision)
            .value(Alias::new("children_indexed"), false)
            .value(Alias::new("last_listed_at"), now)
            .and_where(Expr::col(Alias::new("storage_root_id")).eq(root_id.as_uuid()))
            .and_where(Expr::col(Alias::new("storage_object_id")).eq(previous_parent.as_uuid()))
            .and_where(Expr::col(Alias::new("observed_sync_revision")).lt(sync_revision))
            .to_owned();
        transaction.execute(backend.build(&update)).await?;
    }
    Ok(previous_parent)
}

async fn mark_scope_completed(
    transaction: &DatabaseTransaction,
    root_id: StorageRootId,
    parent_id: StorageObjectRecordId,
    sync_revision: i64,
    now: DateTime<Utc>,
) -> Result<(), StorageSyncRepositoryError> {
    let update = Query::update()
        .table(Alias::new("storage_root_objects"))
        .value(Alias::new("children_indexed"), true)
        .value(Alias::new("children_index_revision"), sync_revision)
        .value(Alias::new("observed_sync_revision"), sync_revision)
        .value(Alias::new("last_listed_at"), now)
        .and_where(Expr::col(Alias::new("storage_root_id")).eq(root_id.as_uuid()))
        .and_where(Expr::col(Alias::new("storage_object_id")).eq(parent_id.as_uuid()))
        .and_where(Expr::col(Alias::new("presence_state")).eq("Present"))
        .to_owned();
    let backend = transaction.get_database_backend();
    if transaction
        .execute(backend.build(&update))
        .await?
        .rows_affected()
        != 1
    {
        return Err(StorageSyncRepositoryError::MissingScope);
    }
    Ok(())
}

pub(crate) struct OutboxEventDraft<'value> {
    pub(crate) root_id: StorageRootId,
    pub(crate) sync_revision: i64,
    pub(crate) object_id: StorageObjectRecordId,
    pub(crate) event_type: &'value str,
    pub(crate) before_revision: Option<&'value str>,
    pub(crate) after_revision: Option<&'value str>,
    pub(crate) payload: serde_json::Value,
    pub(crate) now: DateTime<Utc>,
}

pub(crate) async fn insert_outbox_event(
    transaction: &DatabaseTransaction,
    event: OutboxEventDraft<'_>,
) -> Result<(), DbErr> {
    let statement = Query::insert()
        .into_table(Alias::new("storage_change_outbox"))
        .columns([
            Alias::new("id"),
            Alias::new("storage_root_id"),
            Alias::new("sync_revision"),
            Alias::new("event_type"),
            Alias::new("storage_object_id"),
            Alias::new("before_object_revision"),
            Alias::new("after_object_revision"),
            Alias::new("payload_version"),
            Alias::new("payload"),
            Alias::new("dedupe_key"),
            Alias::new("state"),
            Alias::new("attempt_count"),
            Alias::new("created_at"),
        ])
        .values_panic([
            Uuid::new_v4().into(),
            event.root_id.as_uuid().into(),
            event.sync_revision.into(),
            event.event_type.into(),
            event.object_id.as_uuid().into(),
            event.before_revision.into(),
            event.after_revision.into(),
            1.into(),
            event.payload.into(),
            format!(
                "{}:{}:{}:{}",
                event.root_id, event.sync_revision, event.object_id, event.event_type
            )
            .into(),
            "Pending".into(),
            0.into(),
            event.now.into(),
        ])
        .to_owned();
    let backend = transaction.get_database_backend();
    transaction.execute(backend.build(&statement)).await?;
    Ok(())
}

fn idempotent_insert_conflict(backend: sea_orm::DbBackend, column: &'static str) -> OnConflict {
    if backend == sea_orm::DbBackend::MySql {
        OnConflict::new()
            .update_column(Alias::new(column))
            .to_owned()
    } else {
        OnConflict::new().do_nothing().to_owned()
    }
}

const fn object_type(value: ObjectType) -> &'static str {
    match value {
        ObjectType::File => "File",
        ObjectType::Directory => "Directory",
    }
}

const fn identity_quality(value: IdentityQuality) -> &'static str {
    match value {
        IdentityQuality::StableFileId => "StableFileId",
        IdentityQuality::PathWeak => "PathWeak",
        IdentityQuality::ProviderStableId => "ProviderStableId",
    }
}

fn valid_identity(value: &str) -> bool {
    !value.trim().is_empty()
        && value.chars().count() <= MAX_IDENTITY_CHARS
        && !value.chars().any(char::is_control)
}

async fn finish<T>(
    transaction: DatabaseTransaction,
    result: Result<T, StorageSyncRepositoryError>,
) -> Result<T, StorageSyncRepositoryError> {
    match result {
        Ok(value) => {
            transaction.commit().await?;
            Ok(value)
        }
        Err(original) => match transaction.rollback().await {
            Ok(()) => Err(original),
            Err(rollback) => Err(StorageSyncRepositoryError::RollbackFailed {
                original: original.to_string(),
                rollback,
            }),
        },
    }
}
