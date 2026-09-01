use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use sea_orm::DatabaseConnection;
use tjxy_db::{
    FilesystemIndexRepository, FilesystemIndexState, FilesystemObjectPath, FilesystemPathRepository,
};
use tjxy_storage::{
    BackendError, ByteRange, ByteStream, ChangeCursor, ChangePage, ObjectPage, PageToken,
    StorageBackend, StorageCapabilities, StorageObject, StorageObjectId,
};
use tjxy_storage_filesystem::FilesystemBackend;
use uuid::Uuid;

const PATH_RESOLUTION_TIMEOUT: Duration = Duration::from_secs(5);

pub(crate) struct IndexedFilesystemBackend {
    database: DatabaseConnection,
    account_id: Uuid,
    backend: Arc<FilesystemBackend>,
}

impl IndexedFilesystemBackend {
    pub(crate) const fn new(
        database: DatabaseConnection,
        account_id: Uuid,
        backend: Arc<FilesystemBackend>,
    ) -> Self {
        Self {
            database,
            account_id,
            backend,
        }
    }

    async fn resolve_path(
        &self,
        object: &StorageObjectId,
    ) -> Result<FilesystemObjectPath, BackendError> {
        if object.provider() != "filesystem" {
            return Err(BackendError::NotFound);
        }
        let started = std::time::Instant::now();
        let state = tokio::time::timeout(
            PATH_RESOLUTION_TIMEOUT,
            FilesystemIndexRepository::new(&self.database).state(self.account_id),
        )
        .await
        .map_err(|_| index_unavailable("filesystem path index state lookup timed out"))?
        .map_err(|error| {
            tracing::warn!(
                storage_account_id = %self.account_id,
                error = %error,
                "filesystem path index state lookup failed"
            );
            index_unavailable("filesystem path index state is unavailable")
        })?;
        if state != FilesystemIndexState::Ready {
            tracing::warn!(
                storage_account_id = %self.account_id,
                index_state = ?state,
                elapsed_ms = started.elapsed().as_millis(),
                "filesystem read rejected while its path index is unavailable"
            );
            return Err(index_unavailable(
                "filesystem path index is rebuilding or failed",
            ));
        }
        let result = tokio::time::timeout(
            PATH_RESOLUTION_TIMEOUT,
            FilesystemPathRepository::new(&self.database)
                .resolve(self.account_id, object.provider_object_id()),
        )
        .await;
        let resolved = match result {
            Ok(Ok(Some(path))) => path,
            Ok(Ok(None)) => {
                tracing::warn!(
                    storage_account_id = %self.account_id,
                    provider_object_id = object.provider_object_id(),
                    elapsed_ms = started.elapsed().as_millis(),
                    outcome = "missing",
                    "filesystem path index lookup failed"
                );
                return Err(index_unavailable("filesystem object path is not indexed"));
            }
            Ok(Err(error)) => {
                tracing::warn!(
                    storage_account_id = %self.account_id,
                    provider_object_id = object.provider_object_id(),
                    elapsed_ms = started.elapsed().as_millis(),
                    outcome = "invalid",
                    error = %error,
                    "filesystem path index lookup failed"
                );
                return Err(index_unavailable(
                    "filesystem object path index is not ready",
                ));
            }
            Err(_) => {
                tracing::warn!(
                    storage_account_id = %self.account_id,
                    provider_object_id = object.provider_object_id(),
                    elapsed_ms = started.elapsed().as_millis(),
                    outcome = "timeout",
                    "filesystem path index lookup timed out"
                );
                return Err(index_unavailable("filesystem object path lookup timed out"));
            }
        };
        let elapsed = started.elapsed();
        if elapsed >= Duration::from_millis(100) {
            tracing::warn!(
                storage_account_id = %self.account_id,
                storage_root_id = %resolved.root_id(),
                storage_revision = resolved.reconciled_revision(),
                path_depth = resolved.relative_path().components().count(),
                elapsed_ms = elapsed.as_millis(),
                outcome = "slow",
                "filesystem path index lookup completed slowly"
            );
        }
        Ok(resolved)
    }
}

#[async_trait]
impl StorageBackend for IndexedFilesystemBackend {
    async fn get_object(&self, id: &StorageObjectId) -> Result<StorageObject, BackendError> {
        let path = self.resolve_path(id).await?;
        self.backend.get_object_at(id, path.relative_path()).await
    }

    async fn list_children(
        &self,
        parent: &StorageObjectId,
        page: Option<PageToken>,
    ) -> Result<ObjectPage, BackendError> {
        let path = self.resolve_path(parent).await?;
        self.backend
            .list_children_at(parent, path.relative_path(), page)
            .await
    }

    async fn list_changes(&self, cursor: ChangeCursor) -> Result<ChangePage, BackendError> {
        self.backend.list_changes(cursor).await
    }

    async fn latest_change_cursor(&self) -> Result<ChangeCursor, BackendError> {
        self.backend.latest_change_cursor().await
    }

    async fn open_range(
        &self,
        id: &StorageObjectId,
        range: ByteRange,
    ) -> Result<ByteStream, BackendError> {
        let path = self.resolve_path(id).await?;
        self.backend
            .open_range_at(id, path.relative_path(), range)
            .await
    }

    async fn resolve_local_reference(
        &self,
        descriptor: &StorageObjectId,
        reference: &str,
    ) -> Result<StorageObject, BackendError> {
        let path = self.resolve_path(descriptor).await?;
        self.backend
            .resolve_local_reference_at(descriptor, path.relative_path(), reference)
            .await
    }

    fn capabilities(&self) -> StorageCapabilities {
        self.backend.capabilities()
    }
}

fn index_unavailable(message: &str) -> BackendError {
    BackendError::BackendNotReady {
        message: message.to_owned(),
    }
}
