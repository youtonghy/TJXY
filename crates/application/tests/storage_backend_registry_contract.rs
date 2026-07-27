use std::sync::Arc;

use tjxy_application::{StorageBackendRegistry, StorageBackendRegistryError};
use tjxy_storage::{
    BackendError, ByteRange, ByteStream, ChangeCursor, ChangePage, ObjectPage, PageToken,
    StorageBackend, StorageCapabilities, StorageObject, StorageObjectId,
};
use uuid::Uuid;

struct RegistryBackend;

#[async_trait::async_trait]
impl StorageBackend for RegistryBackend {
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

    fn capabilities(&self) -> StorageCapabilities {
        StorageCapabilities::new()
    }
}

#[test]
fn cloned_registry_observes_dynamic_registration_and_exact_revocation() {
    let registry = StorageBackendRegistry::new();
    let reader = registry.clone();
    let account = Uuid::new_v4();

    assert!(reader.backend(account).is_none());
    assert!(
        registry
            .register(account, "drive-a", Arc::new(RegistryBackend))
            .unwrap()
    );
    assert!(reader.backend(account).is_some());
    assert!(reader.backend_for_drive(account, "drive-a").is_some());
    assert!(reader.backend_for_drive(account, "drive-b").is_none());

    assert!(!registry.deactivate(account, "drive-b"));
    assert!(reader.backend(account).is_some());
    assert!(registry.deactivate(account, "drive-a"));
    assert!(reader.backend(account).is_none());
}

#[test]
fn registration_is_idempotent_for_one_drive_and_rejects_account_aliasing() {
    let registry = StorageBackendRegistry::new();
    let account = Uuid::new_v4();

    assert!(
        registry
            .register(account, "drive-a", Arc::new(RegistryBackend))
            .unwrap()
    );
    assert!(
        !registry
            .register(account, "drive-a", Arc::new(RegistryBackend))
            .unwrap()
    );
    assert_eq!(
        registry
            .register(account, "drive-b", Arc::new(RegistryBackend))
            .unwrap_err(),
        StorageBackendRegistryError::AccountDriveConflict {
            account_id: account,
            active_drive: Some("drive-a".to_owned()),
            requested_drive: Some("drive-b".to_owned()),
        }
    );
}
