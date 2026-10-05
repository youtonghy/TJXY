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
    local_reference_fallback: Arc<RwLock<Option<Arc<dyn StorageBackend>>>>,
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

    /// Installs a read-only fallback for absolute local references such as STRM targets.
    ///
    /// The fallback is not registered as a storage account and therefore cannot participate in
    /// inventory, synchronization, or library authorization.
    pub fn set_local_reference_fallback(&self, backend: Arc<dyn StorageBackend>) {
        *self
            .local_reference_fallback
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
            Some(crate::io_admission::governed(backend));
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
                    backend: crate::io_admission::governed(backend),
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
                backend: crate::io_admission::governed(backend),
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
        let mut pending_error = None;
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
                Err(error) => {
                    pending_error.get_or_insert(error);
                }
            }
        }
        let fallback = self
            .local_reference_fallback
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .map(Arc::clone);
        if std::path::Path::new(reference).is_absolute()
            && let Some(backend) = fallback
        {
            let object = backend
                .resolve_local_reference(descriptor, reference)
                .await?;
            return Ok(ResolvedLocalReference {
                account_id: preferred_account,
                backend,
                object,
            });
        }
        Err(pending_error.unwrap_or(BackendError::NotFound))
    }

    pub(crate) async fn resolve_local_reference_for_probe(
        &self,
        preferred_account: Uuid,
        allowed_accounts: &[Uuid],
        descriptor: &StorageObjectId,
        reference: &str,
        budget: &mut crate::probe_budget::ProbeBudget,
    ) -> Result<ResolvedLocalReference, crate::ProbeServiceError> {
        let mut account_ids = Vec::with_capacity(allowed_accounts.len() + 1);
        account_ids.push(preferred_account);
        account_ids.extend(
            allowed_accounts
                .iter()
                .copied()
                .filter(|account_id| *account_id != preferred_account),
        );
        let mut pending_error = None;
        for account_id in account_ids {
            let Some(backend) = self.backend(account_id) else {
                continue;
            };
            budget.request(0)?;
            match backend.resolve_local_reference(descriptor, reference).await {
                Ok(object) => {
                    return Ok(ResolvedLocalReference {
                        account_id,
                        backend,
                        object,
                    });
                }
                Err(BackendError::NotFound | BackendError::UnsupportedCapability { .. }) => {}
                Err(error) => {
                    pending_error.get_or_insert(error);
                }
            }
        }
        let fallback = self
            .local_reference_fallback
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .map(Arc::clone);
        if std::path::Path::new(reference).is_absolute()
            && let Some(backend) = fallback
        {
            budget.request(0)?;
            let object = backend
                .resolve_local_reference(descriptor, reference)
                .await?;
            return Ok(ResolvedLocalReference {
                account_id: preferred_account,
                backend,
                object,
            });
        }
        Err(pending_error.map_or_else(
            || BackendError::NotFound.into(),
            crate::ProbeServiceError::from,
        ))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::probe_budget::{ProbeBudget, ProbeLimits};
    use tjxy_storage::{ByteRange, ByteStream, ChangeCursor, ChangePage, ObjectPage, PageToken};

    struct ScriptedBackend {
        outcome: Result<StorageObject, BackendError>,
    }

    impl ScriptedBackend {
        fn missing() -> Self {
            Self {
                outcome: Err(BackendError::NotFound),
            }
        }

        fn failing(error: BackendError) -> Self {
            Self {
                outcome: Err(error),
            }
        }

        fn resolving(name: &str) -> Self {
            Self {
                outcome: Ok(StorageObject::file(
                    StorageObjectId::new("filesystem", format!("local:{name}")).unwrap(),
                    name,
                    42,
                )),
            }
        }
    }

    #[async_trait::async_trait]
    impl StorageBackend for ScriptedBackend {
        async fn get_object(&self, _id: &StorageObjectId) -> Result<StorageObject, BackendError> {
            Err(BackendError::NotFound)
        }

        async fn list_children(
            &self,
            _parent: &StorageObjectId,
            _page: Option<PageToken>,
        ) -> Result<ObjectPage, BackendError> {
            Err(BackendError::unsupported_capability("list children"))
        }

        async fn list_changes(&self, _cursor: ChangeCursor) -> Result<ChangePage, BackendError> {
            Err(BackendError::unsupported_capability("changes"))
        }

        async fn open_range(
            &self,
            _id: &StorageObjectId,
            _range: ByteRange,
        ) -> Result<ByteStream, BackendError> {
            Err(BackendError::unsupported_capability("range reads"))
        }

        async fn resolve_local_reference(
            &self,
            _descriptor: &StorageObjectId,
            _reference: &str,
        ) -> Result<StorageObject, BackendError> {
            self.outcome.clone()
        }

        fn capabilities(&self) -> tjxy_storage::StorageCapabilities {
            tjxy_storage::StorageCapabilities::new()
        }
    }

    fn descriptor() -> StorageObjectId {
        StorageObjectId::new("filesystem", "local:descriptor").unwrap()
    }

    #[tokio::test]
    async fn account_errors_do_not_mask_the_absolute_path_fallback() {
        let registry = StorageBackendRegistry::new();
        let preferred = Uuid::new_v4();
        let sibling = Uuid::new_v4();
        registry
            .register(preferred, "local", Arc::new(ScriptedBackend::missing()))
            .unwrap();
        registry
            .register(
                sibling,
                "local",
                Arc::new(ScriptedBackend::failing(BackendError::BackendNotReady {
                    message: "filesystem object path is not indexed".to_owned(),
                })),
            )
            .unwrap();
        registry.set_local_reference_fallback(Arc::new(ScriptedBackend::resolving("target.mkv")));

        let resolved = registry
            .resolve_local_reference(
                preferred,
                &[preferred, sibling],
                &descriptor(),
                "/reference/target.mkv",
            )
            .await
            .expect("fallback resolves the absolute reference");

        assert_eq!(resolved.object.name(), "target.mkv");
        assert_eq!(resolved.account_id, preferred);
    }

    #[tokio::test]
    async fn account_errors_surface_when_no_fallback_applies() {
        let registry = StorageBackendRegistry::new();
        let preferred = Uuid::new_v4();
        let sibling = Uuid::new_v4();
        registry
            .register(preferred, "local", Arc::new(ScriptedBackend::missing()))
            .unwrap();
        registry
            .register(
                sibling,
                "local",
                Arc::new(ScriptedBackend::failing(
                    BackendError::FilesystemIndexFailed,
                )),
            )
            .unwrap();
        registry.set_local_reference_fallback(Arc::new(ScriptedBackend::missing()));

        let Err(error) = registry
            .resolve_local_reference(preferred, &[sibling], &descriptor(), "relative.mkv")
            .await
        else {
            panic!("relative references have no fallback")
        };

        assert_eq!(error, BackendError::FilesystemIndexFailed);
    }

    #[tokio::test]
    async fn probe_resolution_still_reaches_the_fallback_after_account_errors() {
        let registry = StorageBackendRegistry::new();
        let preferred = Uuid::new_v4();
        let sibling = Uuid::new_v4();
        registry
            .register(preferred, "local", Arc::new(ScriptedBackend::missing()))
            .unwrap();
        registry
            .register(
                sibling,
                "local",
                Arc::new(ScriptedBackend::failing(BackendError::BackendNotReady {
                    message: "filesystem object path is not indexed".to_owned(),
                })),
            )
            .unwrap();
        registry.set_local_reference_fallback(Arc::new(ScriptedBackend::resolving("target.mkv")));
        let mut budget = ProbeBudget::new(ProbeLimits::default());

        let resolved = registry
            .resolve_local_reference_for_probe(
                preferred,
                &[preferred, sibling],
                &descriptor(),
                "/reference/target.mkv",
                &mut budget,
            )
            .await
            .expect("fallback resolves the absolute reference");

        assert_eq!(resolved.object.name(), "target.mkv");
    }

    #[tokio::test]
    async fn probe_resolution_reports_the_first_backend_error() {
        let registry = StorageBackendRegistry::new();
        let preferred = Uuid::new_v4();
        let sibling = Uuid::new_v4();
        registry
            .register(preferred, "local", Arc::new(ScriptedBackend::missing()))
            .unwrap();
        registry
            .register(
                sibling,
                "local",
                Arc::new(ScriptedBackend::failing(
                    BackendError::FilesystemIndexFailed,
                )),
            )
            .unwrap();
        let mut budget = ProbeBudget::new(ProbeLimits::default());

        let Err(error) = registry
            .resolve_local_reference_for_probe(
                preferred,
                &[sibling],
                &descriptor(),
                "relative.mkv",
                &mut budget,
            )
            .await
        else {
            panic!("relative references have no fallback")
        };

        assert!(matches!(
            error,
            crate::ProbeServiceError::Storage(BackendError::FilesystemIndexFailed)
        ));
    }
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
