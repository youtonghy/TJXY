use futures_util::StreamExt;
use sea_orm::DatabaseConnection;
use sha2::{Digest, Sha256};
use thiserror::Error;
use tjxy_common::{CatalogItemId, ImageType, LibraryId};
use tjxy_db::{DirectMetadataObjectRecord, DirectMetadataRepository};
use tjxy_metadata::NfoDocument;
use tjxy_storage::{BackendError, ByteRange, StorageObjectId};

use crate::{
    StorageBackendRegistry,
    storage_read::{self, StorageReadError},
};

pub struct DirectMetadataReadService {
    database: DatabaseConnection,
    backends: StorageBackendRegistry,
}

impl DirectMetadataReadService {
    #[must_use]
    pub fn new(database: DatabaseConnection) -> Self {
        Self {
            database,
            backends: StorageBackendRegistry::new(),
        }
    }

    #[must_use]
    pub fn with_backend_registry(mut self, backends: StorageBackendRegistry) -> Self {
        self.backends = backends;
        self
    }

    /// Reads and parses the direct NFO sidecar for a catalog item.
    ///
    /// # Errors
    ///
    /// Returns an error when the reference is invalid, unavailable, or cannot be parsed.
    pub async fn nfo(
        &self,
        item_id: CatalogItemId,
    ) -> Result<Option<NfoDocument>, DirectMetadataReadError> {
        let Some(object) = DirectMetadataRepository::new(&self.database)
            .object(item_id, "Nfo", 0)
            .await?
        else {
            return Ok(None);
        };
        let bytes = self
            .read_all(&object, NfoDocument::MAX_BYTES as u64)
            .await?;
        Ok(Some(NfoDocument::parse(
            &bytes,
            &format!("storage-object:{}", object.storage_object_id()),
        )?))
    }

    /// Opens a direct metadata image from its authorized storage object.
    ///
    /// # Errors
    ///
    /// Returns an error when the image reference, backend, range, or format is invalid.
    pub async fn image(
        &self,
        item_id: CatalogItemId,
        image_type: ImageType,
        priority: i32,
    ) -> Result<Option<OpenedDirectImage>, DirectMetadataReadError> {
        self.image_with_library(item_id, image_type, priority, None)
            .await
    }

    /// Opens a direct image scoped to one selected library.
    ///
    /// # Errors
    ///
    /// Returns an error when the image reference, backend, range, or format is invalid.
    pub async fn image_in_library(
        &self,
        item_id: CatalogItemId,
        image_type: ImageType,
        priority: i32,
        library_id: LibraryId,
    ) -> Result<Option<OpenedDirectImage>, DirectMetadataReadError> {
        self.image_with_library(item_id, image_type, priority, Some(library_id))
            .await
    }

    async fn image_with_library(
        &self,
        item_id: CatalogItemId,
        image_type: ImageType,
        priority: i32,
        library_id: Option<LibraryId>,
    ) -> Result<Option<OpenedDirectImage>, DirectMetadataReadError> {
        let kind = match image_type {
            ImageType::Primary => "Primary",
            ImageType::Backdrop => "Backdrop",
            _ => return Ok(None),
        };
        let repository = DirectMetadataRepository::new(&self.database);
        let object = match library_id {
            Some(library_id) => {
                repository
                    .object_in_library(item_id, kind, priority, library_id)
                    .await?
            }
            None => repository.object(item_id, kind, priority).await?,
        };
        let Some(object) = object else {
            return Ok(None);
        };
        let mime_type =
            image_mime(object.name()).ok_or(DirectMetadataReadError::UnsupportedImage)?;
        let backend = self
            .backends
            .backend_for_drive(object.storage_account_id(), object.provider_drive_id())
            .ok_or(DirectMetadataReadError::BackendUnavailable)?;
        let object_id = StorageObjectId::new(
            object.provider().to_owned(),
            object.provider_object_id().to_owned(),
        )?;
        let stream = storage_read::open_range(
            &self.database,
            backend.as_ref(),
            object.storage_object_id(),
            &object_id,
            ByteRange::new(0, object.size())?,
        )
        .await
        .map_err(map_storage_read)?;
        Ok(Some(OpenedDirectImage {
            stream,
            size: object.size(),
            mime_type,
            etag: direct_etag(&object),
        }))
    }

    async fn read_all(
        &self,
        object: &DirectMetadataObjectRecord,
        limit: u64,
    ) -> Result<Vec<u8>, DirectMetadataReadError> {
        if object.size() == 0 || object.size() > limit {
            return Err(DirectMetadataReadError::InputTooLarge);
        }
        let backend = self
            .backends
            .backend_for_drive(object.storage_account_id(), object.provider_drive_id())
            .ok_or(DirectMetadataReadError::BackendUnavailable)?;
        let object_id = StorageObjectId::new(
            object.provider().to_owned(),
            object.provider_object_id().to_owned(),
        )?;
        let mut stream = storage_read::open_range(
            &self.database,
            backend.as_ref(),
            object.storage_object_id(),
            &object_id,
            ByteRange::new(0, object.size())?,
        )
        .await
        .map_err(map_storage_read)?;
        let mut bytes = Vec::with_capacity(
            usize::try_from(object.size()).map_err(|_| DirectMetadataReadError::InputTooLarge)?,
        );
        while let Some(chunk) = stream.next().await {
            bytes.extend_from_slice(&chunk?);
        }
        if bytes.len() as u64 != object.size() {
            return Err(DirectMetadataReadError::ObjectChanged);
        }
        Ok(bytes)
    }
}

pub struct OpenedDirectImage {
    stream: tjxy_storage::ByteStream,
    size: u64,
    mime_type: &'static str,
    etag: String,
}

impl OpenedDirectImage {
    #[must_use]
    pub fn into_stream(self) -> tjxy_storage::ByteStream {
        self.stream
    }
    #[must_use]
    pub const fn size(&self) -> u64 {
        self.size
    }
    #[must_use]
    pub const fn mime_type(&self) -> &'static str {
        self.mime_type
    }
    #[must_use]
    pub fn etag(&self) -> &str {
        &self.etag
    }
}

fn image_mime(name: &str) -> Option<&'static str> {
    match std::path::Path::new(name)
        .extension()?
        .to_str()?
        .to_ascii_lowercase()
        .as_str()
    {
        "jpg" | "jpeg" => Some("image/jpeg"),
        "png" => Some("image/png"),
        "webp" => Some("image/webp"),
        "gif" => Some("image/gif"),
        _ => None,
    }
}

fn direct_etag(object: &DirectMetadataObjectRecord) -> String {
    let digest = Sha256::digest(
        format!(
            "{}:{}:{}:{}",
            object.storage_object_id(),
            object.remote_revision().unwrap_or(""),
            object.size(),
            object.input_revision()
        )
        .as_bytes(),
    );
    format!("direct-{digest:x}")
}

#[allow(clippy::needless_pass_by_value)] // Error conversion consumes no large owned payload and keeps map_err ergonomic.
fn map_storage_read(error: StorageReadError) -> DirectMetadataReadError {
    match error {
        StorageReadError::Backend(
            BackendError::TemporarilyUnavailable { .. }
            | BackendError::BackendNotReady { .. }
            | BackendError::RateLimited { .. },
        )
        | StorageReadError::Availability(_)
        | StorageReadError::Projection(_) => DirectMetadataReadError::TemporarilyUnavailable,
        StorageReadError::Backend(error) => DirectMetadataReadError::Storage(error.to_string()),
    }
}

#[derive(Debug, Error)]
pub enum DirectMetadataReadError {
    #[error("direct metadata query failed: {0}")]
    Query(#[from] sea_orm::DbErr),
    #[error("direct metadata backend is unavailable")]
    BackendUnavailable,
    #[error("direct metadata storage is temporarily unavailable")]
    TemporarilyUnavailable,
    #[error("direct metadata read failed: {0}")]
    Storage(String),
    #[error("direct NFO is invalid: {0}")]
    Nfo(#[from] tjxy_metadata::MetadataError),
    #[error("direct metadata input is too large")]
    InputTooLarge,
    #[error("direct image format is unsupported")]
    UnsupportedImage,
    #[error("direct metadata object changed during read")]
    ObjectChanged,
    #[error("direct metadata backend failed: {0}")]
    Backend(#[from] BackendError),
}

#[cfg(test)]
mod tests {
    use tjxy_storage::BackendError;

    use super::{DirectMetadataReadError, map_storage_read};
    use crate::storage_read::StorageReadError;

    #[test]
    fn backend_not_ready_remains_a_typed_service_unavailable_error() {
        let mapped = map_storage_read(StorageReadError::Backend(BackendError::BackendNotReady {
            message: "rebuilding".to_owned(),
        }));

        assert!(matches!(
            mapped,
            DirectMetadataReadError::TemporarilyUnavailable
        ));
    }
}
