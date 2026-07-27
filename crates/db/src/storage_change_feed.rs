use std::collections::HashSet;

use chrono::{DateTime, Utc};
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbErr, TransactionTrait,
    sea_query::{Alias, Expr, Query},
};
use serde_json::json;
use thiserror::Error;
use tjxy_common::{StorageObjectRecordId, StorageRootId, WorkJobId};
use tjxy_storage::{ChangeCursor, ChangePage, StorageChange, StorageObjectId};
use uuid::Uuid;

use crate::storage_sync::{
    OutboxEventDraft, RootRecord, insert_outbox_event, read_root, upsert_object, upsert_root_object,
};
use crate::{ClaimedWorkJob, WorkJobRepositoryError, WorkJobSpec, WorkScope, WorkTaskKind};

const CURSOR_TYPE: &str = "Changes";
const CURSOR_RECOVERY_PRIORITY: i32 = 100;

pub struct StorageChangeFeedRepository<'connection> {
    database: &'connection DatabaseConnection,
}

impl<'connection> StorageChangeFeedRepository<'connection> {
    #[must_use]
    pub const fn new(database: &'connection DatabaseConnection) -> Self {
        Self { database }
    }

    /// Reads the active opaque Changes cursor for one configured root scope.
    ///
    /// # Errors
    ///
    /// Returns an error for corrupt cursor data or database failures. A root that
    /// is not active for the requested account and drive returns `Ok(None)`.
    pub async fn active_cursor(
        &self,
        root_id: StorageRootId,
        account_id: Uuid,
        provider_drive_id: &str,
    ) -> Result<Option<ChangeCursor>, StorageChangeFeedRepositoryError> {
        if !scope_exists(self.database, root_id, account_id, provider_drive_id).await? {
            return Ok(None);
        }
        let query = Query::select()
            .column(Alias::new("cursor_value"))
            .from(Alias::new("storage_sync_cursors"))
            .and_where(Expr::col(Alias::new("storage_root_id")).eq(root_id.as_uuid()))
            .and_where(Expr::col(Alias::new("cursor_type")).eq(CURSOR_TYPE))
            .and_where(Expr::col(Alias::new("status")).eq("Active"))
            .limit(1)
            .to_owned();
        let backend = self.database.get_database_backend();
        self.database
            .query_one(backend.build(&query))
            .await?
            .as_ref()
            .map(|row| {
                ChangeCursor::new(row.try_get::<String>("", "cursor_value")?)
                    .map_err(|_| StorageChangeFeedRepositoryError::InvalidStoredCursor)
            })
            .transpose()
    }

    /// Lists roots with active Changes cursors for one configured account drive.
    ///
    /// # Errors
    ///
    /// Returns database failures without returning a partial root set.
    pub async fn active_roots(
        &self,
        account_id: Uuid,
        provider_drive_id: &str,
    ) -> Result<Vec<StorageRootId>, StorageChangeFeedRepositoryError> {
        let root = Alias::new("change_target_root");
        let cursor = Alias::new("change_target_cursor");
        let relation = Alias::new("change_target_relation");
        let object = Alias::new("change_target_object");
        let account = Alias::new("change_target_account");
        let query = Query::select()
            .distinct()
            .expr_as(
                Expr::col((root.clone(), Alias::new("id"))),
                Alias::new("root_id"),
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
                Alias::new("storage_sync_cursors"),
                cursor.clone(),
                Expr::col((cursor.clone(), Alias::new("storage_root_id")))
                    .equals((root.clone(), Alias::new("id"))),
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
                    .equals((relation, Alias::new("storage_object_id"))),
            )
            .and_where(Expr::col((root, Alias::new("storage_account_id"))).eq(account_id))
            .and_where(Expr::col((account, Alias::new("status"))).eq("Active"))
            .and_where(Expr::col((cursor.clone(), Alias::new("cursor_type"))).eq(CURSOR_TYPE))
            .and_where(Expr::col((cursor, Alias::new("status"))).eq("Active"))
            .and_where(Expr::col((object.clone(), Alias::new("storage_account_id"))).eq(account_id))
            .and_where(Expr::col((object, Alias::new("provider_drive_id"))).eq(provider_drive_id))
            .to_owned();
        let backend = self.database.get_database_backend();
        self.database
            .query_all(backend.build(&query))
            .await?
            .iter()
            .map(|row| {
                row.try_get("", "root_id")
                    .map(StorageRootId::from_uuid)
                    .map_err(Into::into)
            })
            .collect()
    }

    /// Commits one provider change page and its opaque successor cursor atomically.
    ///
    /// # Errors
    ///
    /// Returns an error for stale cursors, invalid root/backend identity, malformed
    /// object facts, or database failures. A failed commit never advances the cursor.
    pub async fn commit_page(
        &self,
        root_id: StorageRootId,
        account_id: Uuid,
        provider_drive_id: &str,
        expected_cursor: &ChangeCursor,
        page: &ChangePage,
    ) -> Result<CommittedChangePage, StorageChangeFeedRepositoryError> {
        let transaction = self.database.begin().await?;
        let result = commit_page(
            &transaction,
            root_id,
            account_id,
            provider_drive_id,
            expected_cursor,
            page,
        )
        .await;
        finish(transaction, result).await
    }

    /// Replaces an invalid active cursor with a paused fresh cursor and schedules root inventory.
    ///
    /// # Errors
    ///
    /// Returns an error when the active cursor changed, the root scope is invalid, or the durable
    /// recovery job cannot be created. No cursor state changes outside the transaction.
    pub async fn begin_recovery(
        &self,
        root_id: StorageRootId,
        account_id: Uuid,
        provider_drive_id: &str,
        expected_cursor: &ChangeCursor,
        fresh_cursor: &ChangeCursor,
    ) -> Result<(), StorageChangeFeedRepositoryError> {
        let transaction = self.database.begin().await?;
        let result = begin_recovery(
            &transaction,
            root_id,
            account_id,
            provider_drive_id,
            expected_cursor,
            fresh_cursor,
        )
        .await;
        finish(transaction, result).await
    }

    /// Requeues a root inventory after an operator has reviewed a terminal recovery failure.
    ///
    /// # Errors
    ///
    /// Returns an error unless the exact root scope has a `RecoveryFailed` Changes cursor.
    pub async fn resume_failed_recovery(
        &self,
        root_id: StorageRootId,
        account_id: Uuid,
        provider_drive_id: &str,
    ) -> Result<WorkJobId, StorageChangeFeedRepositoryError> {
        let transaction = self.database.begin().await?;
        let result =
            resume_failed_recovery(&transaction, root_id, account_id, provider_drive_id).await;
        finish(transaction, result).await
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommittedChangePage {
    sync_revision: i64,
    applied_changes: u64,
}

impl CommittedChangePage {
    #[must_use]
    pub const fn sync_revision(self) -> i64 {
        self.sync_revision
    }

    #[must_use]
    pub const fn applied_changes(self) -> u64 {
        self.applied_changes
    }
}

#[derive(Debug, Error)]
pub enum StorageChangeFeedRepositoryError {
    #[error("storage change cursor is stale or missing")]
    CursorConflict,
    #[error("stored storage change cursor is malformed")]
    InvalidStoredCursor,
    #[error("storage change root does not belong to the configured account or drive")]
    InvalidScope,
    #[error("storage change object provider does not match the root account")]
    ProviderMismatch,
    #[error("storage change object contains an unsupported size")]
    InvalidObject,
    #[error("storage root revision is outside the supported range")]
    InvalidRevision,
    #[error("database operation failed: {0}")]
    Database(#[from] DbErr),
    #[error("storage cursor recovery work could not be created: {0}")]
    WorkJob(#[from] WorkJobRepositoryError),
    #[error("rollback failed after {original}: {rollback}")]
    RollbackFailed { original: String, rollback: DbErr },
}

async fn begin_recovery(
    transaction: &DatabaseTransaction,
    root_id: StorageRootId,
    account_id: Uuid,
    provider_drive_id: &str,
    expected_cursor: &ChangeCursor,
    fresh_cursor: &ChangeCursor,
) -> Result<(), StorageChangeFeedRepositoryError> {
    if fresh_cursor == expected_cursor {
        return Err(StorageChangeFeedRepositoryError::CursorConflict);
    }
    let root = recovery_root(transaction, root_id, account_id, provider_drive_id)
        .await?
        .ok_or(StorageChangeFeedRepositoryError::InvalidScope)?;
    let submission = crate::work_job::enqueue_in_transaction(
        transaction,
        &WorkJobSpec::new(
            WorkTaskKind::RecoverStorageCursor,
            WorkScope::StorageRoot(root_id),
            root.sync_revision,
            CURSOR_RECOVERY_PRIORITY,
        )?
        .with_storage_root_affinity(root_id)?,
        Utc::now(),
    )
    .await?;
    let update = Query::update()
        .table(Alias::new("storage_sync_cursors"))
        .value(Alias::new("cursor_value"), fresh_cursor.as_str())
        .value(Alias::new("status"), "Recovering")
        .value(
            Alias::new("recovery_job_id"),
            submission.job().id().as_uuid(),
        )
        .and_where(Expr::col(Alias::new("storage_root_id")).eq(root_id.as_uuid()))
        .and_where(Expr::col(Alias::new("cursor_type")).eq(CURSOR_TYPE))
        .and_where(Expr::col(Alias::new("cursor_value")).eq(expected_cursor.as_str()))
        .and_where(Expr::col(Alias::new("status")).eq("Active"))
        .to_owned();
    let backend = transaction.get_database_backend();
    if transaction
        .execute(backend.build(&update))
        .await?
        .rows_affected()
        != 1
    {
        return Err(StorageChangeFeedRepositoryError::CursorConflict);
    }
    Ok(())
}

async fn resume_failed_recovery(
    transaction: &DatabaseTransaction,
    root_id: StorageRootId,
    account_id: Uuid,
    provider_drive_id: &str,
) -> Result<WorkJobId, StorageChangeFeedRepositoryError> {
    let root = recovery_root(transaction, root_id, account_id, provider_drive_id)
        .await?
        .ok_or(StorageChangeFeedRepositoryError::InvalidScope)?;
    let cursor = Query::select()
        .column(Alias::new("recovery_job_id"))
        .from(Alias::new("storage_sync_cursors"))
        .and_where(Expr::col(Alias::new("storage_root_id")).eq(root_id.as_uuid()))
        .and_where(Expr::col(Alias::new("cursor_type")).eq(CURSOR_TYPE))
        .and_where(Expr::col(Alias::new("status")).eq("RecoveryFailed"))
        .limit(1)
        .to_owned();
    let backend = transaction.get_database_backend();
    let failed_job_id = transaction
        .query_one(backend.build(&cursor))
        .await?
        .ok_or(StorageChangeFeedRepositoryError::CursorConflict)?
        .try_get::<Uuid>("", "recovery_job_id")?;
    let submission = crate::work_job::enqueue_in_transaction(
        transaction,
        &WorkJobSpec::new(
            WorkTaskKind::RecoverStorageCursor,
            WorkScope::StorageRoot(root_id),
            root.sync_revision,
            CURSOR_RECOVERY_PRIORITY,
        )?
        .with_storage_root_affinity(root_id)?,
        Utc::now(),
    )
    .await?;
    let resumed_job_id = submission.job().id();
    let update = Query::update()
        .table(Alias::new("storage_sync_cursors"))
        .value(Alias::new("status"), "Recovering")
        .value(Alias::new("recovery_job_id"), resumed_job_id.as_uuid())
        .and_where(Expr::col(Alias::new("storage_root_id")).eq(root_id.as_uuid()))
        .and_where(Expr::col(Alias::new("cursor_type")).eq(CURSOR_TYPE))
        .and_where(Expr::col(Alias::new("status")).eq("RecoveryFailed"))
        .and_where(Expr::col(Alias::new("recovery_job_id")).eq(failed_job_id))
        .to_owned();
    if transaction
        .execute(backend.build(&update))
        .await?
        .rows_affected()
        != 1
    {
        return Err(StorageChangeFeedRepositoryError::CursorConflict);
    }
    Ok(resumed_job_id)
}

struct RecoveryRoot {
    sync_revision: i64,
}

async fn recovery_root(
    transaction: &DatabaseTransaction,
    root_id: StorageRootId,
    account_id: Uuid,
    provider_drive_id: &str,
) -> Result<Option<RecoveryRoot>, DbErr> {
    let root = Alias::new("recovery_root");
    let relation = Alias::new("recovery_relation");
    let object = Alias::new("recovery_object");
    let query = Query::select()
        .expr_as(
            Expr::col((root.clone(), Alias::new("sync_revision"))),
            Alias::new("sync_revision"),
        )
        .from_as(Alias::new("storage_roots"), root.clone())
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
        .and_where(Expr::col((root, Alias::new("storage_account_id"))).eq(account_id))
        .and_where(Expr::col((relation.clone(), Alias::new("parent_storage_object_id"))).is_null())
        .and_where(Expr::col((relation, Alias::new("presence_state"))).eq("Present"))
        .and_where(Expr::col((object.clone(), Alias::new("storage_account_id"))).eq(account_id))
        .and_where(
            Expr::col((object.clone(), Alias::new("provider_drive_id"))).eq(provider_drive_id),
        )
        .and_where(Expr::col((object, Alias::new("presence_state"))).eq("Present"))
        .limit(1)
        .to_owned();
    let backend = transaction.get_database_backend();
    transaction
        .query_one(backend.build(&query))
        .await?
        .as_ref()
        .map(|row| {
            Ok(RecoveryRoot {
                sync_revision: row.try_get("", "sync_revision")?,
            })
        })
        .transpose()
}

/// Reactivates a paused Changes cursor only when this exact recovery job is completing.
///
/// # Errors
///
/// Returns a persistence error without changing cursor state when the SQL update fails.
pub async fn activate_storage_cursor_recovery(
    transaction: &DatabaseTransaction,
    claimed: &ClaimedWorkJob,
    root_id: StorageRootId,
) -> Result<(), StorageChangeFeedRepositoryError> {
    let WorkScope::StorageRoot(claimed_root) = claimed.job().scope() else {
        return Ok(());
    };
    let recovery_root = recovery_cursor_root(transaction, claimed).await?;
    let Some(recovery_root) = recovery_root else {
        return if claimed.job().task_kind() == WorkTaskKind::RecoverStorageCursor {
            Err(StorageChangeFeedRepositoryError::CursorConflict)
        } else {
            Ok(())
        };
    };
    if claimed_root != root_id || recovery_root != root_id {
        return Err(StorageChangeFeedRepositoryError::CursorConflict);
    }
    let now = Utc::now();
    let update = Query::update()
        .table(Alias::new("storage_sync_cursors"))
        .value(Alias::new("status"), "Active")
        .value(Alias::new("recovery_job_id"), Option::<Uuid>::None)
        .value(Alias::new("last_success_at"), now)
        .value(Alias::new("last_full_sync_at"), now)
        .and_where(Expr::col(Alias::new("storage_root_id")).eq(root_id.as_uuid()))
        .and_where(Expr::col(Alias::new("cursor_type")).eq(CURSOR_TYPE))
        .and_where(Expr::col(Alias::new("status")).eq("Recovering"))
        .and_where(Expr::col(Alias::new("recovery_job_id")).eq(claimed.id().as_uuid()))
        .to_owned();
    let backend = transaction.get_database_backend();
    if transaction
        .execute(backend.build(&update))
        .await?
        .rows_affected()
        != 1
    {
        return Err(StorageChangeFeedRepositoryError::CursorConflict);
    }
    let invalidate = Query::update()
        .table(Alias::new("storage_root_objects"))
        .value(Alias::new("children_indexed"), false)
        .value(Alias::new("children_index_revision"), 0_i64)
        .and_where(Expr::col(Alias::new("storage_root_id")).eq(root_id.as_uuid()))
        .and_where(Expr::col(Alias::new("parent_storage_object_id")).is_not_null())
        .to_owned();
    transaction.execute(backend.build(&invalidate)).await?;
    Ok(())
}

/// Marks an exact root-scoped cursor recovery as terminally failed.
///
/// Object-scoped inventory jobs are not cursor recovery work and leave cursor state unchanged.
///
/// # Errors
///
/// Returns [`StorageChangeFeedRepositoryError::CursorConflict`] when a root-scoped claim does not
/// own the currently recovering cursor.
pub async fn fail_storage_cursor_recovery(
    transaction: &DatabaseTransaction,
    claimed: &ClaimedWorkJob,
) -> Result<(), StorageChangeFeedRepositoryError> {
    let WorkScope::StorageRoot(claimed_root) = claimed.job().scope() else {
        return Ok(());
    };
    let Some(root_id) = recovery_cursor_root(transaction, claimed).await? else {
        return Ok(());
    };
    if root_id != claimed_root {
        return Err(StorageChangeFeedRepositoryError::CursorConflict);
    }
    let update = Query::update()
        .table(Alias::new("storage_sync_cursors"))
        .value(Alias::new("status"), "RecoveryFailed")
        .and_where(Expr::col(Alias::new("storage_root_id")).eq(root_id.as_uuid()))
        .and_where(Expr::col(Alias::new("cursor_type")).eq(CURSOR_TYPE))
        .and_where(Expr::col(Alias::new("status")).eq("Recovering"))
        .and_where(Expr::col(Alias::new("recovery_job_id")).eq(claimed.id().as_uuid()))
        .to_owned();
    let backend = transaction.get_database_backend();
    if transaction
        .execute(backend.build(&update))
        .await?
        .rows_affected()
        != 1
    {
        return Err(StorageChangeFeedRepositoryError::CursorConflict);
    }
    Ok(())
}

async fn recovery_cursor_root(
    transaction: &DatabaseTransaction,
    claimed: &ClaimedWorkJob,
) -> Result<Option<StorageRootId>, DbErr> {
    let query = Query::select()
        .column(Alias::new("storage_root_id"))
        .from(Alias::new("storage_sync_cursors"))
        .and_where(Expr::col(Alias::new("cursor_type")).eq(CURSOR_TYPE))
        .and_where(Expr::col(Alias::new("recovery_job_id")).eq(claimed.id().as_uuid()))
        .limit(1)
        .to_owned();
    let backend = transaction.get_database_backend();
    transaction
        .query_one(backend.build(&query))
        .await?
        .as_ref()
        .map(|row| {
            row.try_get("", "storage_root_id")
                .map(StorageRootId::from_uuid)
        })
        .transpose()
}

async fn commit_page(
    transaction: &DatabaseTransaction,
    root_id: StorageRootId,
    account_id: Uuid,
    provider_drive_id: &str,
    expected_cursor: &ChangeCursor,
    page: &ChangePage,
) -> Result<CommittedChangePage, StorageChangeFeedRepositoryError> {
    let root = read_root(transaction, root_id)
        .await
        .map_err(map_sync_error)?
        .ok_or(StorageChangeFeedRepositoryError::InvalidScope)?;
    if root.account_id != account_id
        || !scope_exists(transaction, root_id, account_id, provider_drive_id).await?
    {
        return Err(StorageChangeFeedRepositoryError::InvalidScope);
    }
    ensure_cursor(transaction, root_id, expected_cursor).await?;
    let sync_revision = root
        .sync_revision
        .checked_add(1)
        .ok_or(StorageChangeFeedRepositoryError::InvalidRevision)?;
    let now = Utc::now();
    let mut applied_changes = 0_u64;
    let context = ChangeCommitContext {
        transaction,
        root: &root,
        root_id,
        account_id,
        provider_drive_id,
        sync_revision,
        now,
    };
    for change in page.changes() {
        if context.apply(change).await? {
            applied_changes += 1;
        }
    }
    let marker_id = scope_marker_id(transaction, root_id)
        .await?
        .ok_or(StorageChangeFeedRepositoryError::InvalidScope)?;
    insert_outbox_event(
        transaction,
        OutboxEventDraft {
            root_id,
            sync_revision,
            object_id: marker_id,
            event_type: "ChangePageCommitted",
            before_revision: None,
            after_revision: None,
            payload: json!({
                "version": 1,
                "kind": "ChangePageCommitted",
                "changes": applied_changes,
                "has_more": page.has_more(),
            }),
            now,
        },
    )
    .await?;
    advance_cursor_and_root(
        transaction,
        root_id,
        expected_cursor,
        page.next_cursor(),
        root.sync_revision,
        sync_revision,
    )
    .await?;
    Ok(CommittedChangePage {
        sync_revision,
        applied_changes,
    })
}

struct ChangeCommitContext<'value> {
    transaction: &'value DatabaseTransaction,
    root: &'value RootRecord,
    root_id: StorageRootId,
    account_id: Uuid,
    provider_drive_id: &'value str,
    sync_revision: i64,
    now: DateTime<Utc>,
}

impl ChangeCommitContext<'_> {
    async fn apply(
        &self,
        change: &StorageChange,
    ) -> Result<bool, StorageChangeFeedRepositoryError> {
        let id = match change {
            StorageChange::Upsert(object) => object.id(),
            StorageChange::Removed(id) => id,
        };
        if id.provider() != self.root.provider {
            return Err(StorageChangeFeedRepositoryError::ProviderMismatch);
        }
        match change {
            StorageChange::Upsert(object) => self.apply_upsert(object).await,
            StorageChange::Removed(id) => self.apply_removal(id).await,
        }
    }

    async fn apply_upsert(
        &self,
        object: &tjxy_storage::StorageObject,
    ) -> Result<bool, StorageChangeFeedRepositoryError> {
        let existing = existing_relation(
            self.transaction,
            self.root_id,
            self.account_id,
            self.provider_drive_id,
            object.id(),
        )
        .await?;
        let Some(parent) = self.resolve_parent(object, existing.as_ref()).await? else {
            return match existing.as_ref() {
                Some(existing) => {
                    self.apply_move_out_of_materialized_scope(object, existing)
                        .await
                }
                None => Ok(false),
            };
        };
        if let Some(existing) = existing.as_ref()
            && existing.parent_record_id != parent.record_id
        {
            self.emit_moved_out(object, existing, "moved-to-materialized-parent")
                .await?;
        }
        let stored = upsert_object(
            self.transaction,
            self.root_id,
            self.root,
            &parent.provider_id,
            self.provider_drive_id,
            object,
            self.sync_revision,
            self.now,
        )
        .await
        .map_err(map_sync_error)?;
        let _previous_parent = upsert_root_object(
            self.transaction,
            self.root_id,
            stored.id,
            parent.record_id,
            self.sync_revision,
            self.now,
        )
        .await?;
        insert_outbox_event(
            self.transaction,
            OutboxEventDraft {
                root_id: self.root_id,
                sync_revision: self.sync_revision,
                object_id: stored.id,
                event_type: "Upserted",
                before_revision: stored.before_revision.as_deref(),
                after_revision: object.remote_revision(),
                payload: json!({
                    "version": 1,
                    "kind": "Upserted",
                    "relation": {
                        "storage_root_id": self.root_id,
                        "storage_object_id": stored.id,
                        "parent_storage_object_id": parent.record_id,
                    },
                    "before": {"remote_revision": stored.before_revision},
                    "after": {"presence_state": "Present"},
                }),
                now: self.now,
            },
        )
        .await?;
        Ok(true)
    }

    async fn emit_moved_out(
        &self,
        object: &tjxy_storage::StorageObject,
        existing: &ExistingRelation,
        reason: &str,
    ) -> Result<(), StorageChangeFeedRepositoryError> {
        insert_outbox_event(
            self.transaction,
            OutboxEventDraft {
                root_id: self.root_id,
                sync_revision: self.sync_revision,
                object_id: existing.object_id,
                event_type: "MovedOut",
                before_revision: existing.remote_revision.as_deref(),
                after_revision: object.remote_revision(),
                payload: json!({
                    "version": 1,
                    "kind": "MovedOut",
                    "relation": {
                        "storage_root_id": self.root_id,
                        "storage_object_id": existing.object_id,
                        "parent_storage_object_id": existing.parent_record_id,
                    },
                    "before": {"remote_revision": existing.remote_revision},
                    "after": {
                        "presence_state": "TemporarilyUnavailable",
                        "availability_reason": reason,
                    },
                }),
                now: self.now,
            },
        )
        .await?;
        Ok(())
    }

    async fn apply_move_out_of_materialized_scope(
        &self,
        object: &tjxy_storage::StorageObject,
        existing: &ExistingRelation,
    ) -> Result<bool, StorageChangeFeedRepositoryError> {
        let Some(parent_provider_id) = object
            .parents()
            .first()
            .map(StorageObjectId::provider_object_id)
        else {
            return Ok(false);
        };
        upsert_object(
            self.transaction,
            self.root_id,
            self.root,
            parent_provider_id,
            self.provider_drive_id,
            object,
            self.sync_revision,
            self.now,
        )
        .await
        .map_err(map_sync_error)?;
        mark_relation_temporarily_unavailable(
            self.transaction,
            self.root_id,
            existing.object_id,
            self.sync_revision,
            self.now,
        )
        .await?;
        cascade_descendant_relations(
            self.transaction,
            self.root_id,
            existing.object_id,
            self.sync_revision,
            self.now,
            DescendantLoss::MovedOut,
        )
        .await?;
        self.emit_moved_out(object, existing, "moved-to-unmaterialized-parent")
            .await?;
        Ok(true)
    }

    async fn resolve_parent(
        &self,
        object: &tjxy_storage::StorageObject,
        existing: Option<&ExistingRelation>,
    ) -> Result<Option<ParentRelation>, DbErr> {
        if object.parents().is_empty() {
            return Ok(existing.map(|existing| ParentRelation {
                record_id: existing.parent_record_id,
                provider_id: existing.parent_provider_id.clone(),
            }));
        }
        // Strict Lazy never expands an unmaterialized branch from a change feed.
        resolve_parent_relation(
            self.transaction,
            self.root_id,
            self.account_id,
            self.provider_drive_id,
            object.parents(),
        )
        .await
    }

    async fn apply_removal(
        &self,
        id: &StorageObjectId,
    ) -> Result<bool, StorageChangeFeedRepositoryError> {
        let Some(existing) = existing_relation(
            self.transaction,
            self.root_id,
            self.account_id,
            self.provider_drive_id,
            id,
        )
        .await?
        else {
            return Ok(false);
        };
        mark_removed(
            self.transaction,
            self.root_id,
            existing.object_id,
            self.sync_revision,
            self.now,
        )
        .await?;
        cascade_descendant_relations(
            self.transaction,
            self.root_id,
            existing.object_id,
            self.sync_revision,
            self.now,
            DescendantLoss::Removed,
        )
        .await?;
        insert_outbox_event(
            self.transaction,
            OutboxEventDraft {
                root_id: self.root_id,
                sync_revision: self.sync_revision,
                object_id: existing.object_id,
                event_type: "Removed",
                before_revision: existing.remote_revision.as_deref(),
                after_revision: None,
                payload: json!({
                    "version": 1,
                    "kind": "Removed",
                    "relation": {
                        "storage_root_id": self.root_id,
                        "storage_object_id": existing.object_id,
                        "parent_storage_object_id": existing.parent_record_id,
                    },
                    "before": {"remote_revision": existing.remote_revision},
                    "after": {"presence_state": "ConfirmedAbsent"},
                }),
                now: self.now,
            },
        )
        .await?;
        Ok(true)
    }
}

#[derive(Clone, Copy)]
pub(crate) enum DescendantLoss {
    MovedOut,
    Removed,
}

impl DescendantLoss {
    const fn presence_state(self) -> &'static str {
        match self {
            Self::MovedOut => "TemporarilyUnavailable",
            Self::Removed => "ConfirmedAbsent",
        }
    }

    const fn reason(self) -> &'static str {
        match self {
            Self::MovedOut => "ancestor-moved-to-unmaterialized-parent",
            Self::Removed => "ancestor-confirmed-absent",
        }
    }

    const fn event_type(self) -> &'static str {
        match self {
            Self::MovedOut => "AncestorMovedOut",
            Self::Removed => "AncestorRemoved",
        }
    }
}

struct DescendantRelation {
    object_id: StorageObjectRecordId,
    parent_id: StorageObjectRecordId,
    remote_revision: Option<String>,
}

pub(crate) async fn cascade_descendant_relations(
    transaction: &DatabaseTransaction,
    root_id: StorageRootId,
    ancestor_id: StorageObjectRecordId,
    sync_revision: i64,
    now: DateTime<Utc>,
    loss: DescendantLoss,
) -> Result<(), DbErr> {
    let descendants = descendant_relations(transaction, root_id, ancestor_id).await?;
    let backend = transaction.get_database_backend();
    for descendant in descendants {
        let update = Query::update()
            .table(Alias::new("storage_root_objects"))
            .value(Alias::new("presence_state"), loss.presence_state())
            .value(Alias::new("availability_reason"), loss.reason())
            .value(Alias::new("observed_sync_revision"), sync_revision)
            .value(Alias::new("children_indexed"), false)
            .value(Alias::new("last_listed_at"), now)
            .and_where(Expr::col(Alias::new("storage_root_id")).eq(root_id.as_uuid()))
            .and_where(
                Expr::col(Alias::new("storage_object_id")).eq(descendant.object_id.as_uuid()),
            )
            .to_owned();
        transaction.execute(backend.build(&update)).await?;
        insert_outbox_event(
            transaction,
            OutboxEventDraft {
                root_id,
                sync_revision,
                object_id: descendant.object_id,
                event_type: loss.event_type(),
                before_revision: descendant.remote_revision.as_deref(),
                after_revision: descendant.remote_revision.as_deref(),
                payload: json!({
                    "version": 1,
                    "kind": loss.event_type(),
                    "relation": {
                        "storage_root_id": root_id,
                        "storage_object_id": descendant.object_id,
                        "parent_storage_object_id": descendant.parent_id,
                    },
                    "before": {"remote_revision": descendant.remote_revision},
                    "after": {
                        "presence_state": loss.presence_state(),
                        "availability_reason": loss.reason(),
                    },
                }),
                now,
            },
        )
        .await?;
    }
    Ok(())
}

async fn descendant_relations(
    transaction: &DatabaseTransaction,
    root_id: StorageRootId,
    ancestor_id: StorageObjectRecordId,
) -> Result<Vec<DescendantRelation>, DbErr> {
    let backend = transaction.get_database_backend();
    let mut seen = HashSet::from([ancestor_id]);
    let mut frontier = vec![ancestor_id];
    let mut descendants = Vec::new();
    while !frontier.is_empty() {
        let mut next_frontier = Vec::new();
        for parents in frontier.chunks(500) {
            let relation = Alias::new("descendant_relation");
            let object = Alias::new("descendant_object");
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
                .from_as(Alias::new("storage_root_objects"), relation.clone())
                .join_as(
                    sea_orm::sea_query::JoinType::InnerJoin,
                    Alias::new("storage_objects"),
                    object.clone(),
                    Expr::col((object, Alias::new("id")))
                        .equals((relation.clone(), Alias::new("storage_object_id"))),
                )
                .and_where(
                    Expr::col((relation.clone(), Alias::new("storage_root_id")))
                        .eq(root_id.as_uuid()),
                )
                .and_where(
                    Expr::col((relation, Alias::new("parent_storage_object_id")))
                        .is_in(parents.iter().map(|parent| parent.as_uuid())),
                )
                .to_owned();
            for row in transaction.query_all(backend.build(&query)).await? {
                let object_id =
                    StorageObjectRecordId::from_uuid(row.try_get("", "storage_object_id")?);
                if !seen.insert(object_id) {
                    continue;
                }
                next_frontier.push(object_id);
                descendants.push(DescendantRelation {
                    object_id,
                    parent_id: StorageObjectRecordId::from_uuid(
                        row.try_get("", "parent_storage_object_id")?,
                    ),
                    remote_revision: row.try_get("", "remote_revision")?,
                });
            }
        }
        frontier = next_frontier;
    }
    Ok(descendants)
}

struct ParentRelation {
    record_id: StorageObjectRecordId,
    provider_id: String,
}

async fn resolve_parent_relation(
    transaction: &DatabaseTransaction,
    root_id: StorageRootId,
    account_id: Uuid,
    provider_drive_id: &str,
    parents: &[StorageObjectId],
) -> Result<Option<ParentRelation>, DbErr> {
    let relation = Alias::new("change_parent_relation");
    let object = Alias::new("change_parent_object");
    let provider_ids = parents
        .iter()
        .map(StorageObjectId::provider_object_id)
        .collect::<Vec<_>>();
    let query = Query::select()
        .expr_as(
            Expr::col((object.clone(), Alias::new("id"))),
            Alias::new("record_id"),
        )
        .expr_as(
            Expr::col((object.clone(), Alias::new("provider_object_id"))),
            Alias::new("provider_id"),
        )
        .from_as(Alias::new("storage_root_objects"), relation.clone())
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("storage_objects"),
            object.clone(),
            Expr::col((object.clone(), Alias::new("id")))
                .equals((relation.clone(), Alias::new("storage_object_id"))),
        )
        .and_where(
            Expr::col((relation.clone(), Alias::new("storage_root_id"))).eq(root_id.as_uuid()),
        )
        .and_where(Expr::col((relation, Alias::new("presence_state"))).eq("Present"))
        .and_where(Expr::col((object.clone(), Alias::new("storage_account_id"))).eq(account_id))
        .and_where(
            Expr::col((object.clone(), Alias::new("provider_drive_id"))).eq(provider_drive_id),
        )
        .and_where(Expr::col((object, Alias::new("provider_object_id"))).is_in(provider_ids))
        .limit(1)
        .to_owned();
    let backend = transaction.get_database_backend();
    transaction
        .query_one(backend.build(&query))
        .await?
        .as_ref()
        .map(|row| {
            Ok(ParentRelation {
                record_id: StorageObjectRecordId::from_uuid(row.try_get("", "record_id")?),
                provider_id: row.try_get("", "provider_id")?,
            })
        })
        .transpose()
}

struct ExistingRelation {
    object_id: StorageObjectRecordId,
    parent_record_id: StorageObjectRecordId,
    parent_provider_id: String,
    remote_revision: Option<String>,
}

async fn existing_relation(
    transaction: &DatabaseTransaction,
    root_id: StorageRootId,
    account_id: Uuid,
    provider_drive_id: &str,
    id: &StorageObjectId,
) -> Result<Option<ExistingRelation>, DbErr> {
    let relation = Alias::new("change_relation");
    let object = Alias::new("change_object");
    let query = Query::select()
        .expr_as(
            Expr::col((object.clone(), Alias::new("id"))),
            Alias::new("object_id"),
        )
        .expr_as(
            Expr::col((relation.clone(), Alias::new("parent_storage_object_id"))),
            Alias::new("parent_record_id"),
        )
        .expr_as(
            Expr::col((object.clone(), Alias::new("provider_parent_id"))),
            Alias::new("parent_provider_id"),
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
            Expr::col((object.clone(), Alias::new("id")))
                .equals((relation.clone(), Alias::new("storage_object_id"))),
        )
        .and_where(
            Expr::col((relation.clone(), Alias::new("storage_root_id"))).eq(root_id.as_uuid()),
        )
        .and_where(Expr::col((object.clone(), Alias::new("storage_account_id"))).eq(account_id))
        .and_where(
            Expr::col((object.clone(), Alias::new("provider_drive_id"))).eq(provider_drive_id),
        )
        .and_where(
            Expr::col((object, Alias::new("provider_object_id"))).eq(id.provider_object_id()),
        )
        .limit(1)
        .to_owned();
    let backend = transaction.get_database_backend();
    transaction
        .query_one(backend.build(&query))
        .await?
        .as_ref()
        .map(|row| {
            Ok(ExistingRelation {
                object_id: StorageObjectRecordId::from_uuid(row.try_get("", "object_id")?),
                parent_record_id: StorageObjectRecordId::from_uuid(
                    row.try_get("", "parent_record_id")?,
                ),
                parent_provider_id: row.try_get("", "parent_provider_id")?,
                remote_revision: row.try_get("", "remote_revision")?,
            })
        })
        .transpose()
}

async fn scope_exists(
    transaction: &impl ConnectionTrait,
    root_id: StorageRootId,
    account_id: Uuid,
    provider_drive_id: &str,
) -> Result<bool, DbErr> {
    let relation = Alias::new("scope_relation");
    let object = Alias::new("scope_object");
    let query = Query::select()
        .expr_as(
            Expr::col((relation.clone(), Alias::new("id"))).count(),
            Alias::new("count"),
        )
        .from_as(Alias::new("storage_root_objects"), relation.clone())
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("storage_objects"),
            object.clone(),
            Expr::col((object.clone(), Alias::new("id")))
                .equals((relation.clone(), Alias::new("storage_object_id"))),
        )
        .and_where(Expr::col((relation, Alias::new("storage_root_id"))).eq(root_id.as_uuid()))
        .and_where(Expr::col((object.clone(), Alias::new("storage_account_id"))).eq(account_id))
        .and_where(Expr::col((object, Alias::new("provider_drive_id"))).eq(provider_drive_id))
        .to_owned();
    let backend = transaction.get_database_backend();
    let count: i64 = transaction
        .query_one(backend.build(&query))
        .await?
        .ok_or(DbErr::RecordNotFound("storage root scope aggregate".into()))?
        .try_get("", "count")?;
    Ok(count > 0)
}

async fn scope_marker_id(
    connection: &impl ConnectionTrait,
    root_id: StorageRootId,
) -> Result<Option<StorageObjectRecordId>, DbErr> {
    let query = Query::select()
        .column(Alias::new("storage_object_id"))
        .from(Alias::new("storage_root_objects"))
        .and_where(Expr::col(Alias::new("storage_root_id")).eq(root_id.as_uuid()))
        .order_by(
            Alias::new("parent_storage_object_id"),
            sea_orm::sea_query::Order::Asc,
        )
        .limit(1)
        .to_owned();
    let backend = connection.get_database_backend();
    connection
        .query_one(backend.build(&query))
        .await?
        .as_ref()
        .map(|row| {
            row.try_get("", "storage_object_id")
                .map(StorageObjectRecordId::from_uuid)
        })
        .transpose()
}

async fn ensure_cursor(
    transaction: &DatabaseTransaction,
    root_id: StorageRootId,
    expected: &ChangeCursor,
) -> Result<(), StorageChangeFeedRepositoryError> {
    let query = Query::select()
        .column(Alias::new("id"))
        .from(Alias::new("storage_sync_cursors"))
        .and_where(Expr::col(Alias::new("storage_root_id")).eq(root_id.as_uuid()))
        .and_where(Expr::col(Alias::new("cursor_type")).eq(CURSOR_TYPE))
        .and_where(Expr::col(Alias::new("cursor_value")).eq(expected.as_str()))
        .and_where(Expr::col(Alias::new("status")).eq("Active"))
        .limit(1)
        .to_owned();
    let backend = transaction.get_database_backend();
    if transaction
        .query_one(backend.build(&query))
        .await?
        .is_none()
    {
        return Err(StorageChangeFeedRepositoryError::CursorConflict);
    }
    Ok(())
}

async fn mark_removed(
    transaction: &DatabaseTransaction,
    root_id: StorageRootId,
    object_id: StorageObjectRecordId,
    sync_revision: i64,
    now: chrono::DateTime<Utc>,
) -> Result<(), DbErr> {
    let backend = transaction.get_database_backend();
    transaction
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("storage_objects"))
                    .value(Alias::new("presence_state"), "ConfirmedAbsent")
                    .value(Alias::new("observed_sync_revision"), sync_revision)
                    .value(
                        Alias::new("facts_observed_storage_root_id"),
                        root_id.as_uuid(),
                    )
                    .value(Alias::new("last_listed_at"), now)
                    .and_where(Expr::col(Alias::new("id")).eq(object_id.as_uuid())),
            ),
        )
        .await?;
    transaction
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("storage_root_objects"))
                    .value(Alias::new("presence_state"), "ConfirmedAbsent")
                    .value(Alias::new("observed_sync_revision"), sync_revision)
                    .value(Alias::new("last_listed_at"), now)
                    .and_where(Expr::col(Alias::new("storage_root_id")).eq(root_id.as_uuid()))
                    .and_where(Expr::col(Alias::new("storage_object_id")).eq(object_id.as_uuid())),
            ),
        )
        .await?;
    Ok(())
}

async fn mark_relation_temporarily_unavailable(
    transaction: &DatabaseTransaction,
    root_id: StorageRootId,
    object_id: StorageObjectRecordId,
    sync_revision: i64,
    now: chrono::DateTime<Utc>,
) -> Result<(), StorageChangeFeedRepositoryError> {
    let update = Query::update()
        .table(Alias::new("storage_root_objects"))
        .value(Alias::new("presence_state"), "TemporarilyUnavailable")
        .value(
            Alias::new("availability_reason"),
            "moved-to-unmaterialized-parent",
        )
        .value(Alias::new("observed_sync_revision"), sync_revision)
        .value(Alias::new("last_listed_at"), now)
        .and_where(Expr::col(Alias::new("storage_root_id")).eq(root_id.as_uuid()))
        .and_where(Expr::col(Alias::new("storage_object_id")).eq(object_id.as_uuid()))
        .to_owned();
    let backend = transaction.get_database_backend();
    if transaction
        .execute(backend.build(&update))
        .await?
        .rows_affected()
        != 1
    {
        return Err(StorageChangeFeedRepositoryError::InvalidScope);
    }
    Ok(())
}

async fn advance_cursor_and_root(
    transaction: &DatabaseTransaction,
    root_id: StorageRootId,
    expected: &ChangeCursor,
    next: &ChangeCursor,
    previous_revision: i64,
    sync_revision: i64,
) -> Result<(), StorageChangeFeedRepositoryError> {
    let backend = transaction.get_database_backend();
    let cursor = Query::update()
        .table(Alias::new("storage_sync_cursors"))
        .value(Alias::new("cursor_value"), next.as_str())
        .value(Alias::new("last_success_at"), Utc::now())
        .and_where(Expr::col(Alias::new("storage_root_id")).eq(root_id.as_uuid()))
        .and_where(Expr::col(Alias::new("cursor_type")).eq(CURSOR_TYPE))
        .and_where(Expr::col(Alias::new("cursor_value")).eq(expected.as_str()))
        .and_where(Expr::col(Alias::new("status")).eq("Active"))
        .to_owned();
    if transaction
        .execute(backend.build(&cursor))
        .await?
        .rows_affected()
        != 1
    {
        return Err(StorageChangeFeedRepositoryError::CursorConflict);
    }
    let root = Query::update()
        .table(Alias::new("storage_roots"))
        .value(Alias::new("sync_revision"), sync_revision)
        .and_where(Expr::col(Alias::new("id")).eq(root_id.as_uuid()))
        .and_where(Expr::col(Alias::new("sync_revision")).eq(previous_revision))
        .to_owned();
    if transaction
        .execute(backend.build(&root))
        .await?
        .rows_affected()
        != 1
    {
        return Err(StorageChangeFeedRepositoryError::CursorConflict);
    }
    Ok(())
}

fn map_sync_error(error: crate::StorageSyncRepositoryError) -> StorageChangeFeedRepositoryError {
    match error {
        crate::StorageSyncRepositoryError::InvalidObjectSize => {
            StorageChangeFeedRepositoryError::InvalidObject
        }
        crate::StorageSyncRepositoryError::Database(error) => error.into(),
        _ => StorageChangeFeedRepositoryError::InvalidScope,
    }
}

async fn finish<T>(
    transaction: DatabaseTransaction,
    result: Result<T, StorageChangeFeedRepositoryError>,
) -> Result<T, StorageChangeFeedRepositoryError> {
    match result {
        Ok(value) => {
            transaction.commit().await?;
            Ok(value)
        }
        Err(original) => match transaction.rollback().await {
            Ok(()) => Err(original),
            Err(rollback) => Err(StorageChangeFeedRepositoryError::RollbackFailed {
                original: original.to_string(),
                rollback,
            }),
        },
    }
}
