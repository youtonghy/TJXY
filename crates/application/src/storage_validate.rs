use std::{collections::HashSet, collections::VecDeque, sync::Arc};

use sea_orm::{DatabaseConnection, TransactionTrait};
use serde_json::json;
use sha2::{Digest, Sha256};
use thiserror::Error;
use tjxy_common::{StorageObjectRecordId, StorageRootId};
use tjxy_db::{
    ClaimedWorkJob, ScopedInventoryTarget, StorageSyncPage, StorageSyncRepository,
    StorageSyncRepositoryError, WorkJobRepository, WorkJobRepositoryError, WorkJobResult,
    WorkTaskKind,
};
use tjxy_storage::{BackendError, ObjectPage, PageToken, StorageBackend};
use uuid::Uuid;

use crate::{
    StorageChangeProjector, StorageChangeProjectorError,
    storage_sync::temporary_availability_reason,
};

const MAX_DIRECTORIES_PER_RUN: usize = 100_000;
const MAX_PAGES_PER_DIRECTORY: usize = 10_000;

pub struct FullValidateStorageService<Backend: ?Sized> {
    database: DatabaseConnection,
    backend: Arc<Backend>,
}

impl<Backend> FullValidateStorageService<Backend>
where
    Backend: StorageBackend + ?Sized,
{
    #[must_use]
    pub fn new(database: DatabaseConnection, backend: Arc<Backend>) -> Self {
        Self { database, backend }
    }

    /// Recursively inventories one explicitly claimed root and reconciles unreachable relations.
    ///
    /// # Errors
    ///
    /// Returns [`FullValidateStorageError`] for an invalid claim, backend or pagination failure,
    /// lost lease, persistence failure, or projection failure.
    pub async fn run_claimed(
        &self,
        claimed: &ClaimedWorkJob,
        account_id: Uuid,
    ) -> Result<FullValidateStorageResult, FullValidateStorageError> {
        if claimed.job().task_kind() != WorkTaskKind::ValidateStorageRoot {
            return Err(FullValidateStorageError::InvalidClaim);
        }
        let repository = StorageSyncRepository::new(&self.database);
        let root = repository
            .inventory_target(claimed)
            .await?
            .ok_or(FullValidateStorageError::MissingInventoryTarget)?;
        if root.account_id() != account_id {
            return Err(FullValidateStorageError::WrongStorageAccount);
        }
        let root_id = root.root_id();
        let root_object_id = root.parent_record_id();
        let traversal = self.inventory_tree(claimed, &repository, root).await?;
        let committed = repository
            .commit_validation_sweep(claimed, root_id, root_object_id, traversal.first_revision)
            .await?;
        StorageChangeProjector::new(self.database.clone())
            .drain_root(root_id, committed.sync_revision())
            .await?;
        let jobs = WorkJobRepository::new(&self.database);
        let transaction = self.database.begin().await?;
        let completion: Result<(), FullValidateStorageError> = async {
            tjxy_db::enqueue_discovery_after_root_sync(
                &transaction,
                root_id,
                root_object_id,
                committed.sync_revision(),
                claimed.job().priority(),
                chrono::Utc::now(),
            )
            .await?;
            jobs.complete_in_transaction(
                &transaction,
                claimed,
                WorkJobResult::success(
                    json!({
                        "directories": traversal.directory_count,
                        "objects": traversal.object_count,
                    }),
                    Vec::new(),
                )
                .with_sync_revision(committed.sync_revision())?,
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
        Ok(FullValidateStorageResult {
            sync_revision: committed.sync_revision(),
            directory_count: traversal.directory_count,
            object_count: traversal.object_count,
        })
    }

    async fn list_validation_page(
        &self,
        claimed: &ClaimedWorkJob,
        repository: &StorageSyncRepository<'_>,
        root_id: StorageRootId,
        directory: &ScopedInventoryTarget,
        token: Option<PageToken>,
    ) -> Result<ObjectPage, FullValidateStorageError> {
        if !repository
            .inventory_scope_authorized(claimed, root_id, directory.parent_record_id())
            .await?
        {
            return Err(FullValidateStorageError::MissingInventoryTarget);
        }
        match self
            .backend
            .list_children(directory.backend_parent_id(), token)
            .await
        {
            Ok(page) => Ok(page),
            Err(error) => {
                if let Some(reason) = temporary_availability_reason(&error) {
                    let revision = repository
                        .mark_scope_temporarily_unavailable(
                            claimed,
                            root_id,
                            directory.parent_record_id(),
                            reason,
                        )
                        .await?;
                    StorageChangeProjector::new(self.database.clone())
                        .drain_root(root_id, revision)
                        .await?;
                }
                Err(error.into())
            }
        }
    }

    async fn inventory_tree(
        &self,
        claimed: &ClaimedWorkJob,
        repository: &StorageSyncRepository<'_>,
        root: ScopedInventoryTarget,
    ) -> Result<ValidationTraversal, FullValidateStorageError> {
        let root_id = root.root_id();
        let provider_drive_id = root.provider_drive_id().to_owned();
        let mut directories = VecDeque::from([root]);
        let mut visited = HashSet::new();
        let mut directory_count = 0_u64;
        let mut object_count = 0_u64;
        let mut first_validation_revision = None;
        while let Some(directory) = directories.pop_front() {
            if !visited.insert(directory.parent_record_id()) {
                continue;
            }
            if visited.len() > MAX_DIRECTORIES_PER_RUN {
                return Err(FullValidateStorageError::DirectoryLimit);
            }
            directory_count = directory_count
                .checked_add(1)
                .ok_or(FullValidateStorageError::CountOverflow)?;
            let mut token = None;
            let mut pages = HashSet::new();
            loop {
                let identity = validation_page_identity(
                    claimed.attempt_count(),
                    directory.parent_record_id(),
                    token.as_ref(),
                );
                if !pages.insert(identity.clone()) || pages.len() > MAX_PAGES_PER_DIRECTORY {
                    return Err(FullValidateStorageError::InvalidPagination);
                }
                let page = self
                    .list_validation_page(claimed, repository, root_id, &directory, token.clone())
                    .await?;
                let next_page = page.next_page;
                if next_page.as_ref().is_some_and(|next| {
                    pages.contains(&validation_page_identity(
                        claimed.attempt_count(),
                        directory.parent_record_id(),
                        Some(next),
                    ))
                }) {
                    return Err(FullValidateStorageError::InvalidPagination);
                }
                object_count = object_count
                    .checked_add(u64::try_from(page.objects.len()).unwrap_or(u64::MAX))
                    .ok_or(FullValidateStorageError::CountOverflow)?;
                let committed = repository
                    .commit_inventory_page(
                        claimed,
                        StorageSyncPage::new(
                            root_id,
                            directory.parent_record_id(),
                            &provider_drive_id,
                            identity,
                            page.objects,
                            next_page.is_none(),
                        )?,
                    )
                    .await?;
                first_validation_revision.get_or_insert(committed.sync_revision());
                let Some(next_page) = next_page else {
                    break;
                };
                token = Some(next_page);
            }
            for child in repository
                .present_child_directories(
                    root_id,
                    directory.parent_record_id(),
                    first_validation_revision
                        .ok_or(FullValidateStorageError::MissingValidationRevision)?,
                )
                .await?
            {
                if !visited.contains(&child.parent_record_id()) {
                    directories.push_back(child);
                }
            }
        }
        Ok(ValidationTraversal {
            first_revision: first_validation_revision
                .ok_or(FullValidateStorageError::MissingValidationRevision)?,
            directory_count,
            object_count,
        })
    }
}

struct ValidationTraversal {
    first_revision: i64,
    directory_count: u64,
    object_count: u64,
}

fn validation_page_identity(
    attempt_count: i32,
    directory: StorageObjectRecordId,
    token: Option<&PageToken>,
) -> String {
    token.map_or_else(
        || format!("attempt:{attempt_count}:scope:{directory}:initial"),
        |value| {
            format!(
                "attempt:{attempt_count}:scope:{directory}:page:{:x}",
                Sha256::digest(value.as_str().as_bytes())
            )
        },
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FullValidateStorageResult {
    sync_revision: i64,
    directory_count: u64,
    object_count: u64,
}

impl FullValidateStorageResult {
    #[must_use]
    pub const fn sync_revision(self) -> i64 {
        self.sync_revision
    }

    #[must_use]
    pub const fn directory_count(self) -> u64 {
        self.directory_count
    }

    #[must_use]
    pub const fn object_count(self) -> u64 {
        self.object_count
    }
}

#[derive(Debug, Error)]
pub enum FullValidateStorageError {
    #[error("claimed work is not a full storage validation")]
    InvalidClaim,
    #[error("claimed validation root is no longer available")]
    MissingInventoryTarget,
    #[error("claimed validation root belongs to another storage account")]
    WrongStorageAccount,
    #[error("full validation exceeded its directory bound")]
    DirectoryLimit,
    #[error("storage backend returned a cyclic or excessive page sequence")]
    InvalidPagination,
    #[error("full validation counters overflowed")]
    CountOverflow,
    #[error("full validation committed no inventory revision")]
    MissingValidationRevision,
    #[error("storage validation backend failed: {0}")]
    Backend(#[from] BackendError),
    #[error("storage validation persistence failed: {0}")]
    Persistence(#[from] StorageSyncRepositoryError),
    #[error("storage validation projection failed: {0}")]
    Projection(#[from] StorageChangeProjectorError),
    #[error("storage validation work failed: {0}")]
    Work(#[from] WorkJobRepositoryError),
    #[error("storage validation transaction failed: {0}")]
    Database(#[from] sea_orm::DbErr),
}
