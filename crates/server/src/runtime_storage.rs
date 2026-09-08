use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use sea_orm::DatabaseConnection;
use thiserror::Error;
use tjxy_application::{StorageBackendRegistry, StorageBackendRegistryError};
use tjxy_storage::StorageBackend;
use tjxy_storage_filesystem::FilesystemBackend;
use tokio::task::JoinHandle;
use uuid::Uuid;

use crate::{filesystem_read::IndexedFilesystemBackend, worker};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct RuntimeStorageKey {
    account_id: Uuid,
    provider_drive_id: String,
}

struct ActiveStorageWorkers {
    handles: Vec<JoinHandle<()>>,
}

pub(crate) struct RuntimeStorageManager {
    database: DatabaseConnection,
    backends: StorageBackendRegistry,
    workers: Mutex<HashMap<RuntimeStorageKey, ActiveStorageWorkers>>,
    filesystem_realtime_enabled: bool,
    activation: tokio::sync::Mutex<()>,
    generations: Mutex<HashMap<RuntimeStorageKey, u64>>,
}

impl RuntimeStorageManager {
    pub(crate) fn new(
        database: DatabaseConnection,
        backends: StorageBackendRegistry,
        filesystem_realtime_enabled: bool,
    ) -> Self {
        Self {
            database,
            backends,
            workers: Mutex::new(HashMap::new()),
            filesystem_realtime_enabled,
            activation: tokio::sync::Mutex::new(()),
            generations: Mutex::new(HashMap::new()),
        }
    }

    pub(crate) async fn activate_filesystem(
        &self,
        account_id: Uuid,
        backend: Arc<FilesystemBackend>,
    ) -> Result<bool, RuntimeStorageError> {
        let key = RuntimeStorageKey {
            account_id,
            provider_drive_id: "local".to_owned(),
        };
        let generation = *self
            .generations
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .entry(key.clone())
            .or_default();
        let _activation = self.activation.lock().await;
        if self
            .workers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .contains_key(&key)
        {
            return Ok(false);
        }
        tjxy_db::FilesystemIndexRepository::new(&self.database)
            .prepare_mount(
                account_id,
                backend.physical_root_identity(),
                backend.root_identity_changed(),
            )
            .await?;
        let mut workers = self
            .workers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if self
            .generations
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&key)
            .copied()
            .unwrap_or_default()
            != generation
        {
            return Err(RuntimeStorageError::ActivationCancelled);
        }
        let dyn_backend: Arc<dyn StorageBackend> = Arc::new(IndexedFilesystemBackend::new(
            self.database.clone(),
            account_id,
            Arc::clone(&backend),
        ));
        self.backends.register(account_id, "local", dyn_backend)?;
        let mut handles = vec![worker::spawn_storage_worker(
            self.database.clone(),
            account_id,
            Arc::clone(&backend),
        )];
        if self.filesystem_realtime_enabled {
            handles.push(worker::spawn_filesystem_event_worker(
                self.database.clone(),
                account_id,
                backend,
            ));
        }
        workers.insert(key, ActiveStorageWorkers { handles });
        Ok(true)
    }

    pub(crate) fn activate_provider(
        &self,
        account_id: Uuid,
        provider_drive_id: impl Into<String>,
        backend: Arc<dyn StorageBackend>,
    ) -> Result<bool, RuntimeStorageError> {
        let provider_drive_id = provider_drive_id.into();
        let key = RuntimeStorageKey {
            account_id,
            provider_drive_id: provider_drive_id.clone(),
        };
        let mut workers = self
            .workers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if workers.contains_key(&key) {
            return Ok(false);
        }
        self.backends
            .register(account_id, provider_drive_id.clone(), Arc::clone(&backend))?;
        let mut handles = Vec::with_capacity(2);
        if backend.capabilities().changes() {
            handles.push(worker::spawn_storage_change_worker(
                self.database.clone(),
                account_id,
                provider_drive_id.clone(),
                Arc::clone(&backend),
            ));
        }
        handles.push(worker::spawn_storage_worker_for_drive(
            self.database.clone(),
            account_id,
            provider_drive_id,
            backend,
        ));
        workers.insert(key, ActiveStorageWorkers { handles });
        Ok(true)
    }

    pub(crate) fn deactivate(
        &self,
        account_id: Uuid,
        provider_drive_id: &str,
    ) -> Result<bool, RuntimeStorageError> {
        let key = RuntimeStorageKey {
            account_id,
            provider_drive_id: provider_drive_id.to_owned(),
        };
        let mut registry = self
            .workers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *self
            .generations
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .entry(key.clone())
            .or_default() += 1;
        let removed = registry.remove(&key);
        if let Some(workers) = removed.as_ref() {
            for handle in &workers.handles {
                handle.abort();
            }
        }
        let revoked = self.backends.deactivate(account_id, provider_drive_id);
        if removed.is_some() && !revoked {
            return Err(RuntimeStorageError::WorkerRegistryMismatch {
                account_id,
                provider_drive_id: provider_drive_id.to_owned(),
            });
        }
        Ok(removed.is_some() || revoked)
    }

    pub(crate) fn is_active(&self, account_id: Uuid) -> bool {
        self.backends.backend(account_id).is_some()
    }
}

impl Drop for RuntimeStorageManager {
    fn drop(&mut self) {
        let workers = self
            .workers
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for (key, active) in workers.iter_mut() {
            for handle in active.handles.drain(..) {
                handle.abort();
            }
            let _ = self
                .backends
                .deactivate(key.account_id, &key.provider_drive_id);
        }
    }
}

#[derive(Debug, Error)]
pub enum RuntimeStorageError {
    #[error("filesystem activation was cancelled by a concurrent deactivation")]
    ActivationCancelled,
    #[error("filesystem index initialization failed: {0}")]
    Database(#[from] sea_orm::DbErr),
    #[error("runtime storage registry rejected the backend: {0}")]
    Registry(#[from] StorageBackendRegistryError),
    #[error(
        "runtime storage workers existed without a matching backend for account {account_id} drive {provider_drive_id}"
    )]
    WorkerRegistryMismatch {
        account_id: Uuid,
        provider_drive_id: String,
    },
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sea_orm::Database;
    use sea_orm_migration::MigratorTrait;
    use tempfile::TempDir;
    use tjxy_application::StorageBackendRegistry;
    use tjxy_storage_filesystem::FilesystemBackend;

    use super::RuntimeStorageManager;

    #[tokio::test]
    async fn filesystem_activation_is_idempotent_and_deactivation_revokes_reads() {
        let database = Database::connect("sqlite::memory:").await.unwrap();
        tjxy_db::Migrator::up(&database, None).await.unwrap();
        let root = TempDir::new().unwrap();
        let backend = Arc::new(FilesystemBackend::new(root.path()).await.unwrap());
        let registry = StorageBackendRegistry::new();
        let manager = RuntimeStorageManager::new(database, registry.clone(), false);
        let policy = tjxy_db::LibraryPolicyUpdate::new(
            "Lazy",
            "title_layer",
            "basic",
            "on_browse",
            "on_playback",
            true,
        )
        .unwrap();
        let draft = tjxy_db::FilesystemRootDraft::new(
            root.path().to_str().unwrap(),
            backend.root_id().provider_object_id(),
            "Media",
        )
        .unwrap();
        let created = tjxy_db::LibraryRepository::new(&manager.database)
            .create_with_filesystem_root("Movies", "movies", &policy, &draft)
            .await
            .unwrap();
        let account_id = created.account_id();
        assert_eq!(
            tjxy_db::FilesystemIndexRepository::new(&manager.database)
                .state(account_id)
                .await
                .unwrap(),
            tjxy_db::FilesystemIndexState::Uninitialized
        );

        assert!(
            manager
                .activate_filesystem(account_id, Arc::clone(&backend))
                .await
                .unwrap()
        );
        assert_eq!(
            tjxy_db::FilesystemIndexRepository::new(&manager.database)
                .state(account_id)
                .await
                .unwrap(),
            tjxy_db::FilesystemIndexState::Ready
        );
        assert!(manager.is_active(account_id));
        assert!(
            !manager
                .activate_filesystem(account_id, backend)
                .await
                .unwrap()
        );
        assert!(registry.backend_for_drive(account_id, "local").is_some());
        assert!(manager.deactivate(account_id, "local").unwrap());
        assert!(!manager.is_active(account_id));
        assert!(registry.backend(account_id).is_none());
        assert!(!manager.deactivate(account_id, "local").unwrap());
    }
    #[tokio::test]
    async fn failed_validation_can_be_requeued_without_restarting_the_backend() {
        let database = Database::connect("sqlite::memory:").await.unwrap();
        tjxy_db::Migrator::up(&database, None).await.unwrap();
        let root = TempDir::new().unwrap();
        let backend = FilesystemBackend::new(root.path()).await.unwrap();
        let policy = tjxy_db::LibraryPolicyUpdate::new(
            "Lazy",
            "title_layer",
            "basic",
            "on_browse",
            "on_playback",
            true,
        )
        .unwrap();
        let draft = tjxy_db::FilesystemRootDraft::new(
            root.path().to_str().unwrap(),
            backend.root_id().provider_object_id(),
            "Media",
        )
        .unwrap();
        let created = tjxy_db::LibraryRepository::new(&database)
            .create_with_filesystem_root("Movies", "movies", &policy, &draft)
            .await
            .unwrap();
        let indexes = tjxy_db::FilesystemIndexRepository::new(&database);
        assert!(
            indexes
                .prepare_mount(created.account_id(), "changed-identity", true)
                .await
                .unwrap()
        );
        let jobs = tjxy_db::WorkJobRepository::new(&database);
        let validation = jobs
            .claim_next(
                &[tjxy_db::WorkTaskKind::ValidateStorageRoot],
                "validation",
                chrono::Duration::minutes(1),
            )
            .await
            .unwrap()
            .unwrap();
        jobs.fail_terminal(&validation, "validation failed")
            .await
            .unwrap();
        assert_eq!(
            indexes.state(created.account_id()).await.unwrap(),
            tjxy_db::FilesystemIndexState::Failed
        );
        let next = tjxy_db::StorageSyncRepository::new(&database)
            .enqueue_validation(created.root_id(), 100)
            .await
            .unwrap();
        assert_ne!(next.job().id(), validation.id());
        assert_eq!(
            indexes.state(created.account_id()).await.unwrap(),
            tjxy_db::FilesystemIndexState::Rebuilding
        );
    }
}
