use std::{collections::HashSet, sync::Arc};

use sea_orm::{DatabaseConnection, TransactionTrait};
use serde_json::json;
use sha2::{Digest, Sha256};
use thiserror::Error;
use tjxy_common::{StorageObjectRecordId, StorageRootId};
use tjxy_db::{
    ClaimedWorkJob, StorageChangeFeedRepositoryError, StorageSyncPage, StorageSyncRepository,
    StorageSyncRepositoryError, TemporaryAvailabilityReason, WorkJobRepository,
    WorkJobRepositoryError, WorkJobResult,
};
use tjxy_storage::{BackendError, PageToken, StorageBackend, StorageObjectId};
use uuid::Uuid;

use crate::{StorageChangeProjector, StorageChangeProjectorError};

const MAX_PAGES_PER_RUN: usize = 10_000;

pub struct ScopedInventoryService<Backend: ?Sized> {
    database: DatabaseConnection,
    backend: Arc<Backend>,
}

impl<Backend> ScopedInventoryService<Backend>
where
    Backend: StorageBackend + ?Sized,
{
    #[must_use]
    pub fn new(database: DatabaseConnection, backend: Arc<Backend>) -> Self {
        Self { database, backend }
    }

    /// Resolves and inventories the exact scope held by a claimed job.
    ///
    /// # Errors
    ///
    /// Returns [`ScopedInventoryError`] when the claim no longer resolves, belongs to another
    /// configured account, or inventory/persistence fails.
    pub async fn run_claimed(
        &self,
        claimed: &ClaimedWorkJob,
        account_id: Uuid,
    ) -> Result<ScopedInventoryResult, ScopedInventoryError> {
        let target = StorageSyncRepository::new(&self.database)
            .inventory_target(claimed)
            .await?
            .ok_or(ScopedInventoryError::MissingInventoryTarget)?;
        if target.account_id() != account_id {
            return Err(ScopedInventoryError::WrongStorageAccount);
        }
        self.run(
            claimed,
            target.root_id(),
            target.parent_record_id(),
            target.provider_drive_id(),
            target.backend_parent_id(),
        )
        .await
    }

    /// Inventories exactly one backend directory and completes its durable sync job.
    ///
    /// This method follows provider pagination but never recurses into returned directories.
    ///
    /// # Errors
    ///
    /// Returns [`ScopedInventoryError`] for backend, pagination, persistence, or lease failures.
    #[allow(clippy::too_many_lines)] // Pagination, presence recording, and final completion form one scoped-sync workflow.
    pub async fn run(
        &self,
        claimed: &ClaimedWorkJob,
        root_id: StorageRootId,
        parent_record_id: StorageObjectRecordId,
        provider_drive_id: &str,
        backend_parent_id: &StorageObjectId,
    ) -> Result<ScopedInventoryResult, ScopedInventoryError> {
        let repository = StorageSyncRepository::new(&self.database);
        let mut token = None;
        let mut seen_pages = HashSet::new();
        let mut object_count = 0_u64;
        let final_revision = loop {
            if !repository
                .inventory_scope_authorized(claimed, root_id, parent_record_id)
                .await?
            {
                return Err(ScopedInventoryError::MissingInventoryTarget);
            }
            let current_page_identity = page_identity(claimed.attempt_count(), token.as_ref());
            if !seen_pages.insert(current_page_identity.clone())
                || seen_pages.len() > MAX_PAGES_PER_RUN
            {
                return Err(ScopedInventoryError::InvalidPagination);
            }
            let page = match self
                .backend
                .list_children(backend_parent_id, token.clone())
                .await
            {
                Ok(page) => page,
                Err(error) => {
                    if let Some(reason) = temporary_availability_reason(&error) {
                        let revision = repository
                            .mark_scope_temporarily_unavailable(
                                claimed,
                                root_id,
                                parent_record_id,
                                reason,
                            )
                            .await?;
                        StorageChangeProjector::new(self.database.clone())
                            .drain_root(root_id, revision)
                            .await?;
                    }
                    return Err(error.into());
                }
            };
            let next_page = page.next_page;
            if next_page.as_ref().is_some_and(|next| {
                seen_pages.contains(&page_identity(claimed.attempt_count(), Some(next)))
            }) {
                return Err(ScopedInventoryError::InvalidPagination);
            }
            object_count = object_count
                .checked_add(u64::try_from(page.objects.len()).unwrap_or(u64::MAX))
                .ok_or(ScopedInventoryError::ObjectCountOverflow)?;
            let committed = repository
                .commit_inventory_page(
                    claimed,
                    StorageSyncPage::new(
                        root_id,
                        parent_record_id,
                        provider_drive_id,
                        current_page_identity,
                        page.objects,
                        next_page.is_none(),
                    )?,
                )
                .await?;
            let Some(next_page) = next_page else {
                break committed.sync_revision();
            };
            token = Some(next_page);
        };
        let work = WorkJobRepository::new(&self.database);
        StorageChangeProjector::new(self.database.clone())
            .drain_root(root_id, final_revision)
            .await?;
        let transaction = self.database.begin().await?;
        let completion: Result<(), ScopedInventoryError> = async {
            tjxy_db::activate_storage_cursor_recovery(&transaction, claimed, root_id).await?;
            tjxy_db::enqueue_discovery_after_root_sync(
                &transaction,
                root_id,
                parent_record_id,
                final_revision,
                claimed.job().priority(),
                chrono::Utc::now(),
            )
            .await?;
            work.complete_in_transaction(
                &transaction,
                claimed,
                WorkJobResult::success(json!({"objects": object_count}), Vec::new())
                    .with_sync_revision(final_revision)?,
            )
            .await?;
            Ok(())
        }
        .await;
        match completion {
            Ok(()) => transaction.commit().await?,
            Err(error) => {
                transaction.rollback().await?;
                return Err(error);
            }
        }
        Ok(ScopedInventoryResult {
            sync_revision: final_revision,
            object_count,
        })
    }

    /// Atomically fails a storage job and records a terminal cursor recovery state when applicable.
    ///
    /// # Errors
    ///
    /// Returns [`ScopedInventoryError`] when cursor fencing, job lease validation, or transaction
    /// persistence fails.
    pub async fn fail_terminal(
        &self,
        claimed: &ClaimedWorkJob,
        error: &str,
    ) -> Result<(), ScopedInventoryError> {
        let work = WorkJobRepository::new(&self.database);
        let transaction = self.database.begin().await?;
        let result: Result<(), ScopedInventoryError> = async {
            tjxy_db::fail_storage_cursor_recovery(&transaction, claimed).await?;
            work.fail_terminal_in_transaction(&transaction, claimed, error)
                .await?;
            Ok(())
        }
        .await;
        match result {
            Ok(()) => transaction.commit().await?,
            Err(error) => {
                transaction.rollback().await?;
                return Err(error);
            }
        }
        Ok(())
    }
}

fn page_identity(attempt_count: i32, token: Option<&PageToken>) -> String {
    token.map_or_else(
        || format!("attempt:{attempt_count}:initial"),
        |value| {
            format!(
                "attempt:{attempt_count}:page:{:x}",
                Sha256::digest(value.as_str().as_bytes())
            )
        },
    )
}

pub(crate) const fn temporary_availability_reason(
    error: &BackendError,
) -> Option<TemporaryAvailabilityReason> {
    match error {
        BackendError::TemporarilyUnavailable { .. } => {
            Some(TemporaryAvailabilityReason::BackendTemporarilyUnavailable)
        }
        BackendError::RateLimited { .. } => Some(TemporaryAvailabilityReason::BackendRateLimited),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScopedInventoryResult {
    sync_revision: i64,
    object_count: u64,
}

impl ScopedInventoryResult {
    #[must_use]
    pub const fn sync_revision(self) -> i64 {
        self.sync_revision
    }

    #[must_use]
    pub const fn object_count(self) -> u64 {
        self.object_count
    }
}

#[derive(Debug, Error)]
pub enum ScopedInventoryError {
    #[error("storage backend inventory failed: {0}")]
    Backend(#[from] BackendError),
    #[error("storage inventory persistence failed: {0}")]
    Persistence(#[from] StorageSyncRepositoryError),
    #[error("storage change reconciliation failed: {0}")]
    Projection(#[from] StorageChangeProjectorError),
    #[error("storage cursor recovery persistence failed: {0}")]
    CursorRecovery(#[from] StorageChangeFeedRepositoryError),
    #[error("claimed storage inventory scope is no longer available")]
    MissingInventoryTarget,
    #[error("claimed storage inventory scope belongs to another storage account")]
    WrongStorageAccount,
    #[error("storage work completion failed: {0}")]
    WorkJob(#[from] WorkJobRepositoryError),
    #[error("storage backend returned a cyclic or excessive page sequence")]
    InvalidPagination,
    #[error("storage inventory object count overflowed")]
    ObjectCountOverflow,
    #[error("storage inventory transaction failed: {0}")]
    Database(#[from] sea_orm::DbErr),
}
