use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use thiserror::Error;
use tjxy_storage::{BackendError, StorageBackend, StorageObject, StorageObjectId};
use uuid::Uuid;

#[derive(Clone, Default)]
pub struct StorageBackendRegistry {
    entries: Arc<RwLock<HashMap<Uuid, RegisteredStorageBackend>>>,
}

struct RegisteredStorageBackend {
    provider_drive_id: Option<String>,
    backend: Arc<dyn StorageBackend>,
}

impl StorageBackendRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers one account backend without replacing an already active account.
    ///
    /// Returns `true` when a new entry was inserted and `false` when the same account/drive
    /// was already active.
    ///
    /// # Errors
    ///
    /// Returns [`StorageBackendRegistryError`] for an empty drive id or when the account is
    /// already registered for another provider drive.
    pub fn register(
        &self,
        account_id: Uuid,
        provider_drive_id: impl Into<String>,
        backend: Arc<dyn StorageBackend>,
    ) -> Result<bool, StorageBackendRegistryError> {
        let provider_drive_id = provider_drive_id.into();
        if provider_drive_id.trim().is_empty() {
            return Err(StorageBackendRegistryError::EmptyProviderDriveId);
        }
        self.register_inner(account_id, Some(provider_drive_id), backend)
    }

    pub(crate) fn insert_unscoped(&self, account_id: Uuid, backend: Arc<dyn StorageBackend>) {
        self.insert_for_builder(account_id, None, backend);
    }

    pub(crate) fn insert_scoped(
        &self,
        account_id: Uuid,
        provider_drive_id: impl Into<String>,
        backend: Arc<dyn StorageBackend>,
    ) {
        self.insert_for_builder(account_id, Some(provider_drive_id.into()), backend);
    }

    fn insert_for_builder(
        &self,
        account_id: Uuid,
        provider_drive_id: Option<String>,
        backend: Arc<dyn StorageBackend>,
    ) {
        self.entries
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(
                account_id,
                RegisteredStorageBackend {
                    provider_drive_id,
                    backend,
                },
            );
    }

    fn register_inner(
        &self,
        account_id: Uuid,
        provider_drive_id: Option<String>,
        backend: Arc<dyn StorageBackend>,
    ) -> Result<bool, StorageBackendRegistryError> {
        let mut entries = self
            .entries
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(existing) = entries.get(&account_id) {
            if existing.provider_drive_id == provider_drive_id {
                return Ok(false);
            }
            return Err(StorageBackendRegistryError::AccountDriveConflict {
                account_id,
                active_drive: existing.provider_drive_id.clone(),
                requested_drive: provider_drive_id,
            });
        }
        entries.insert(
            account_id,
            RegisteredStorageBackend {
                provider_drive_id,
                backend,
            },
        );
        Ok(true)
    }

    #[must_use]
    pub fn backend(&self, account_id: Uuid) -> Option<Arc<dyn StorageBackend>> {
        self.entries
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&account_id)
            .map(|entry| Arc::clone(&entry.backend))
    }

    #[must_use]
    pub fn backend_for_drive(
        &self,
        account_id: Uuid,
        provider_drive_id: &str,
    ) -> Option<Arc<dyn StorageBackend>> {
        self.entries
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&account_id)
            .filter(|entry| {
                entry
                    .provider_drive_id
                    .as_deref()
                    .is_none_or(|active| active == provider_drive_id)
            })
            .map(|entry| Arc::clone(&entry.backend))
    }

    pub(crate) async fn resolve_local_reference(
        &self,
        preferred_account: Uuid,
        allowed_accounts: &[Uuid],
        descriptor: &StorageObjectId,
        reference: &str,
    ) -> Result<ResolvedLocalReference, BackendError> {
        let mut account_ids = Vec::with_capacity(allowed_accounts.len() + 1);
        account_ids.push(preferred_account);
        account_ids.extend(
            allowed_accounts
                .iter()
                .copied()
                .filter(|account_id| *account_id != preferred_account),
        );
        for account_id in account_ids {
            let Some(backend) = self.backend(account_id) else {
                continue;
            };
            match backend.resolve_local_reference(descriptor, reference).await {
                Ok(object) => {
                    return Ok(ResolvedLocalReference {
                        account_id,
                        backend,
                        object,
                    });
                }
                Err(BackendError::NotFound | BackendError::UnsupportedCapability { .. }) => {}
                Err(error) => return Err(error),
            }
        }
        Err(BackendError::NotFound)
    }

    /// Removes an account only when its active provider drive matches the requested drive.
    #[must_use]
    pub fn deactivate(&self, account_id: Uuid, provider_drive_id: &str) -> bool {
        let mut entries = self
            .entries
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let matches = entries.get(&account_id).is_some_and(|entry| {
            entry
                .provider_drive_id
                .as_deref()
                .is_none_or(|active| active == provider_drive_id)
        });
        matches && entries.remove(&account_id).is_some()
    }
}

pub(crate) struct ResolvedLocalReference {
    pub(crate) account_id: Uuid,
    pub(crate) backend: Arc<dyn StorageBackend>,
    pub(crate) object: StorageObject,
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum StorageBackendRegistryError {
    #[error("provider drive id must not be empty")]
    EmptyProviderDriveId,
    #[error(
        "storage account {account_id} is already active for drive {active_drive:?}, not {requested_drive:?}"
    )]
    AccountDriveConflict {
        account_id: Uuid,
        active_drive: Option<String>,
        requested_drive: Option<String>,
    },
}
