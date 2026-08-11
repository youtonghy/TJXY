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

use crate::worker;

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
        }
    }

    pub(crate) fn activate_filesystem(
        &self,
        account_id: Uuid,
        backend: Arc<FilesystemBackend>,
    ) -> Result<bool, RuntimeStorageError> {
        let key = RuntimeStorageKey {
            account_id,
            provider_drive_id: "local".to_owned(),
        };
        let mut workers = self
            .workers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if workers.contains_key(&key) {
            return Ok(false);
        }
        let dyn_backend: Arc<dyn StorageBackend> = backend.clone();
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
        let removed = self
            .workers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&key);
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
    use uuid::Uuid;

    use super::RuntimeStorageManager;

    #[tokio::test]
    async fn filesystem_activation_is_idempotent_and_deactivation_revokes_reads() {
        let database = Database::connect("sqlite::memory:").await.unwrap();
        tjxy_db::Migrator::up(&database, None).await.unwrap();
        let root = TempDir::new().unwrap();
        let backend = Arc::new(FilesystemBackend::new(root.path()).await.unwrap());
        let registry = StorageBackendRegistry::new();
        let manager = RuntimeStorageManager::new(database, registry.clone(), false);
        let account_id = Uuid::new_v4();

        assert!(
            manager
                .activate_filesystem(account_id, Arc::clone(&backend))
                .unwrap()
        );
        assert!(manager.is_active(account_id));
        assert!(!manager.activate_filesystem(account_id, backend).unwrap());
        assert!(registry.backend_for_drive(account_id, "local").is_some());
        assert!(manager.deactivate(account_id, "local").unwrap());
        assert!(!manager.is_active(account_id));
        assert!(registry.backend(account_id).is_none());
        assert!(!manager.deactivate(account_id, "local").unwrap());
    }
}
