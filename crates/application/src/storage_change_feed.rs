use std::{collections::HashSet, sync::Arc};

use sea_orm::DatabaseConnection;
use thiserror::Error;
use tjxy_common::{StorageRootId, WorkJobId};
use tjxy_db::{StorageChangeFeedRepository, StorageChangeFeedRepositoryError};
use tjxy_storage::{BackendError, StorageBackend};
use uuid::Uuid;

use crate::{StorageChangeProjector, StorageChangeProjectorError};

const MAX_PAGES_PER_RUN: usize = 10_000;

pub struct StorageChangeFeedService<Backend: ?Sized> {
    database: DatabaseConnection,
    backend: Arc<Backend>,
}

impl<Backend> StorageChangeFeedService<Backend>
where
    Backend: StorageBackend + ?Sized,
{
    #[must_use]
    pub const fn new(database: DatabaseConnection, backend: Arc<Backend>) -> Self {
        Self { database, backend }
    }

    /// Consumes one root's Changes feed through the provider's terminal cursor.
    ///
    /// # Errors
    ///
    /// Returns an error without advancing an uncommitted page when the backend,
    /// SQL cursor commit, pagination, or outbox projection fails.
    pub async fn run_root(
        &self,
        root_id: StorageRootId,
        account_id: Uuid,
        provider_drive_id: &str,
    ) -> Result<StorageChangeFeedResult, StorageChangeFeedError> {
        if !self.backend.capabilities().changes() {
            return Err(BackendError::unsupported_capability("changes").into());
        }
        let repository = StorageChangeFeedRepository::new(&self.database);
        let mut cursor = repository
            .active_cursor(root_id, account_id, provider_drive_id)
            .await?
            .ok_or(StorageChangeFeedError::MissingCursor)?;
        let mut seen = HashSet::new();
        let mut pages = 0_u64;
        let mut changes = 0_u64;
        let final_revision = loop {
            if !seen.insert(cursor.as_str().to_owned()) || seen.len() > MAX_PAGES_PER_RUN {
                return Err(StorageChangeFeedError::InvalidPagination);
            }
            let page = match self.backend.list_changes(cursor.clone()).await {
                Ok(page) => page,
                Err(BackendError::ChangeCursorInvalid) => {
                    let fresh_cursor = self.backend.latest_change_cursor().await?;
                    repository
                        .begin_recovery(
                            root_id,
                            account_id,
                            provider_drive_id,
                            &cursor,
                            &fresh_cursor,
                        )
                        .await?;
                    return Ok(StorageChangeFeedResult {
                        pages: 0,
                        changes: 0,
                        recovery_scheduled: true,
                    });
                }
                Err(error) => return Err(error.into()),
            };
            let has_more = page.has_more();
            let next_cursor = page.next_cursor().clone();
            let committed = repository
                .commit_page(root_id, account_id, provider_drive_id, &cursor, &page)
                .await?;
            pages = pages
                .checked_add(1)
                .ok_or(StorageChangeFeedError::CountOverflow)?;
            changes = changes
                .checked_add(committed.applied_changes())
                .ok_or(StorageChangeFeedError::CountOverflow)?;
            cursor = next_cursor;
            if !has_more {
                break committed.sync_revision();
            }
        };
        StorageChangeProjector::new(self.database.clone())
            .drain_root(root_id, final_revision)
            .await?;
        Ok(StorageChangeFeedResult {
            pages,
            changes,
            recovery_scheduled: false,
        })
    }

    /// Runs all active Changes cursors bound to one configured provider drive.
    ///
    /// # Errors
    ///
    /// Returns the first root failure; pages already committed for earlier roots
    /// remain durable and replay-safe.
    pub async fn run_active_roots(
        &self,
        account_id: Uuid,
        provider_drive_id: &str,
    ) -> Result<Vec<(StorageRootId, StorageChangeFeedResult)>, StorageChangeFeedError> {
        let roots = StorageChangeFeedRepository::new(&self.database)
            .active_roots(account_id, provider_drive_id)
            .await?;
        let mut results = Vec::with_capacity(roots.len());
        for root_id in roots {
            let result = self
                .run_root(root_id, account_id, provider_drive_id)
                .await?;
            results.push((root_id, result));
        }
        Ok(results)
    }

    /// Resumes a terminally failed cursor recovery after operational review.
    ///
    /// # Errors
    ///
    /// Returns an error when the root identity is invalid or its cursor is not `RecoveryFailed`.
    pub async fn resume_failed_recovery(
        &self,
        root_id: StorageRootId,
        account_id: Uuid,
        provider_drive_id: &str,
    ) -> Result<WorkJobId, StorageChangeFeedError> {
        StorageChangeFeedRepository::new(&self.database)
            .resume_failed_recovery(root_id, account_id, provider_drive_id)
            .await
            .map_err(Into::into)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StorageChangeFeedResult {
    pages: u64,
    changes: u64,
    recovery_scheduled: bool,
}

impl StorageChangeFeedResult {
    #[must_use]
    pub const fn pages(self) -> u64 {
        self.pages
    }

    #[must_use]
    pub const fn changes(self) -> u64 {
        self.changes
    }

    #[must_use]
    pub const fn recovery_scheduled(self) -> bool {
        self.recovery_scheduled
    }
}

#[derive(Debug, Error)]
pub enum StorageChangeFeedError {
    #[error("storage root has no active Changes cursor")]
    MissingCursor,
    #[error("storage backend returned a cyclic or excessive Changes page sequence")]
    InvalidPagination,
    #[error("storage Changes page or object count overflowed")]
    CountOverflow,
    #[error("storage Changes backend failed: {0}")]
    Backend(#[from] BackendError),
    #[error("storage Changes persistence failed: {0}")]
    Persistence(#[from] StorageChangeFeedRepositoryError),
    #[error("storage Changes projection failed: {0}")]
    Projection(#[from] StorageChangeProjectorError),
}
