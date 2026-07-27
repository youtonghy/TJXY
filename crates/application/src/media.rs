use std::sync::Arc;

use sea_orm::DatabaseConnection;
use sha2::{Digest, Sha256};
use thiserror::Error;
use tjxy_common::{CatalogItemId, PresentationKey, StorageObjectRecordId, UserId};
use tjxy_db::{
    CatalogPublicationError, CatalogPublicationRepository, PlaybackLocation,
    StorageSyncRepositoryError,
};
use tjxy_storage::{BackendError, ByteRange, ByteStream, StorageBackend, StorageObjectId};
use uuid::Uuid;

use crate::{
    StorageBackendRegistry, StorageChangeProjectorError,
    storage_read::{self, StorageReadError},
};

pub struct MediaReadService {
    database: DatabaseConnection,
    backends: StorageBackendRegistry,
}

impl MediaReadService {
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

    /// Binds one configured storage account to its provider backend.
    #[must_use]
    pub fn with_backend<Backend>(self, account_id: Uuid, backend: Arc<Backend>) -> Self
    where
        Backend: StorageBackend + 'static,
    {
        self.backends.insert_unscoped(account_id, backend);
        self
    }

    /// Binds a provider-neutral backend selected at runtime.
    #[must_use]
    pub fn with_dyn_backend(self, account_id: Uuid, backend: Arc<dyn StorageBackend>) -> Self {
        self.backends.insert_unscoped(account_id, backend);
        self
    }

    /// Resolves and opens one bounded range from an active, authorized media location.
    ///
    /// # Errors
    ///
    /// Returns [`MediaReadError`] for unavailable locations, invalid backend identity,
    /// or storage failures.
    pub async fn open_range(
        &self,
        principal: UserId,
        item_id: CatalogItemId,
        presentation_key: PresentationKey,
        range: ByteRange,
    ) -> Result<Option<OpenedMediaRange>, MediaReadError> {
        let Some(resolved) = self.resolve(principal, item_id, presentation_key).await? else {
            return Ok(None);
        };
        resolved.open_range(range).await.map(Some)
    }

    /// Resolves active media metadata without opening backend bytes.
    ///
    /// # Errors
    ///
    /// Returns [`MediaReadError`] for unavailable locations or invalid identities.
    pub async fn resolve(
        &self,
        _principal: UserId,
        item_id: CatalogItemId,
        presentation_key: PresentationKey,
    ) -> Result<Option<ResolvedMedia>, MediaReadError> {
        let Some(location) = CatalogPublicationRepository::new(&self.database)
            .playback_location(item_id, presentation_key)
            .await?
        else {
            return Ok(None);
        };
        self.resolve_location(&location).map(Some)
    }

    /// Resolves one active external subtitle without exposing its storage identity.
    ///
    /// # Errors
    ///
    /// Returns [`MediaReadError`] for unavailable locations or invalid identities.
    pub async fn resolve_subtitle(
        &self,
        _principal: UserId,
        item_id: CatalogItemId,
        presentation_key: PresentationKey,
        delivery_index: i32,
    ) -> Result<Option<ResolvedSubtitle>, MediaReadError> {
        let Some(subtitle) = CatalogPublicationRepository::new(&self.database)
            .subtitle_location(item_id, presentation_key, delivery_index)
            .await?
        else {
            return Ok(None);
        };
        Ok(Some(ResolvedSubtitle {
            media: self.resolve_location(subtitle.location())?,
            format: subtitle.format().to_owned(),
        }))
    }

    fn resolve_location(
        &self,
        location: &PlaybackLocation,
    ) -> Result<ResolvedMedia, MediaReadError> {
        let backend = self
            .backends
            .backend(location.storage_account_id())
            .ok_or(MediaReadError::BackendUnavailable)?;
        let object_id = StorageObjectId::new(
            location.provider().to_owned(),
            location.provider_object_id().to_owned(),
        )?;
        Ok(ResolvedMedia {
            database: self.database.clone(),
            backend,
            storage_object_id: location.storage_object_id(),
            object_id,
            size: location.size(),
            etag: media_etag(
                location.storage_account_id(),
                location.provider_object_id(),
                location.remote_revision(),
                location.size(),
            ),
        })
    }
}

pub struct ResolvedSubtitle {
    media: ResolvedMedia,
    format: String,
}

impl ResolvedSubtitle {
    #[must_use]
    pub fn media(&self) -> &ResolvedMedia {
        &self.media
    }

    #[must_use]
    pub fn format(&self) -> &str {
        &self.format
    }

    #[must_use]
    pub fn into_media(self) -> ResolvedMedia {
        self.media
    }
}

pub struct ResolvedMedia {
    database: DatabaseConnection,
    backend: Arc<dyn StorageBackend>,
    storage_object_id: StorageObjectRecordId,
    object_id: StorageObjectId,
    size: u64,
    etag: String,
}

impl ResolvedMedia {
    #[must_use]
    pub const fn size(&self) -> u64 {
        self.size
    }

    #[must_use]
    pub fn etag(&self) -> &str {
        &self.etag
    }

    /// Opens a validated half-open byte range.
    ///
    /// # Errors
    ///
    /// Returns [`MediaReadError`] for invalid ranges or backend failures.
    pub async fn open_range(&self, range: ByteRange) -> Result<OpenedMediaRange, MediaReadError> {
        if range.end_exclusive() > self.size {
            return Err(MediaReadError::RangeNotSatisfiable { size: self.size });
        }
        let stream = match storage_read::open_range(
            &self.database,
            self.backend.as_ref(),
            self.storage_object_id,
            &self.object_id,
            range,
        )
        .await
        {
            Ok(stream) => stream,
            Err(StorageReadError::Backend(BackendError::RangeNotSatisfiable { size })) => {
                return Err(MediaReadError::RangeNotSatisfiable { size });
            }
            Err(StorageReadError::Backend(error)) => return Err(MediaReadError::Backend(error)),
            Err(StorageReadError::Availability(error)) => return Err(error.into()),
            Err(StorageReadError::Projection(error)) => return Err(error.into()),
        };
        Ok(OpenedMediaRange {
            stream,
            total_size: self.size,
            etag: self.etag.clone(),
        })
    }
}

pub struct OpenedMediaRange {
    stream: ByteStream,
    total_size: u64,
    etag: String,
}

impl OpenedMediaRange {
    #[must_use]
    pub fn into_stream(self) -> ByteStream {
        self.stream
    }

    #[must_use]
    pub const fn total_size(&self) -> u64 {
        self.total_size
    }

    #[must_use]
    pub fn etag(&self) -> &str {
        &self.etag
    }
}

#[derive(Debug, Error)]
pub enum MediaReadError {
    #[error("catalog publication failed: {0}")]
    Publication(#[from] CatalogPublicationError),
    #[error("storage backend is not configured")]
    BackendUnavailable,
    #[error("storage object identity is invalid: {0}")]
    InvalidObject(#[from] BackendError),
    #[error("requested range is not satisfiable for {size} bytes")]
    RangeNotSatisfiable { size: u64 },
    #[error("storage read failed: {0}")]
    Backend(BackendError),
    #[error("storage availability persistence failed: {0}")]
    Availability(#[from] StorageSyncRepositoryError),
    #[error("storage availability projection failed: {0}")]
    AvailabilityProjection(#[from] StorageChangeProjectorError),
}

fn media_etag(
    account_id: Uuid,
    provider_object_id: &str,
    remote_revision: Option<&str>,
    size: u64,
) -> String {
    let mut digest = Sha256::new();
    digest.update(account_id.as_bytes());
    digest.update(provider_object_id.as_bytes());
    digest.update(remote_revision.unwrap_or_default().as_bytes());
    digest.update(size.to_be_bytes());
    format!("\"{:x}\"", digest.finalize())
}
