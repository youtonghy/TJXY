use std::sync::Arc;

use futures_util::StreamExt;
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
    strm::{MAX_STRM_BYTES, StrmError, parse_strm},
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
        self.resolve_location(item_id, &location).await.map(Some)
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
            media: self.resolve_location(item_id, subtitle.location()).await?,
            format: subtitle.format().to_owned(),
        }))
    }

    async fn resolve_location(
        &self,
        item_id: CatalogItemId,
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
        if location.locator_kind() == "strm" {
            return self
                .resolve_strm_location(item_id, location, backend, object_id)
                .await;
        }
        Ok(ResolvedMedia {
            database: self.database.clone(),
            backend,
            storage_object_id: Some(location.storage_object_id()),
            object_id,
            size: location.size(),
            content_type: media_content_type(location.container(), location.is_audio()),
            etag: media_etag(
                location.storage_account_id(),
                location.provider_object_id(),
                location.remote_revision(),
                location.size(),
            ),
        })
    }

    async fn resolve_strm_location(
        &self,
        item_id: CatalogItemId,
        location: &PlaybackLocation,
        backend: Arc<dyn StorageBackend>,
        descriptor_id: StorageObjectId,
    ) -> Result<ResolvedMedia, MediaReadError> {
        let size = usize::try_from(location.size())
            .map_err(|_| MediaReadError::Strm(StrmError::TooLarge.to_string()))?;
        if size > MAX_STRM_BYTES {
            return Err(MediaReadError::Strm(StrmError::TooLarge.to_string()));
        }
        let mut bytes = Vec::with_capacity(size);
        if size > 0 {
            let range = ByteRange::new(0, location.size())?;
            let mut stream = storage_read::open_range(
                &self.database,
                backend.as_ref(),
                location.storage_object_id(),
                &descriptor_id,
                range,
            )
            .await
            .map_err(map_storage_read_error)?;
            while let Some(chunk) = stream.next().await {
                bytes.extend_from_slice(&chunk?);
                if bytes.len() > MAX_STRM_BYTES {
                    return Err(MediaReadError::Strm(StrmError::TooLarge.to_string()));
                }
            }
        }
        let target = parse_strm(&bytes).map_err(|error| MediaReadError::Strm(error.to_string()))?;
        let allowed_accounts = CatalogPublicationRepository::new(&self.database)
            .playback_storage_accounts(item_id)
            .await?;
        let resolved = self
            .backends
            .resolve_local_reference(
                location.storage_account_id(),
                &allowed_accounts,
                &descriptor_id,
                target,
            )
            .await?;
        let object = resolved.object;
        let target_size = object.size().ok_or(MediaReadError::InvalidStrmTarget)?;
        let target_id = object.id().clone();
        let target_name = object.name().to_owned();
        Ok(ResolvedMedia {
            database: self.database.clone(),
            backend: resolved.backend,
            storage_object_id: None,
            object_id: target_id.clone(),
            size: target_size,
            content_type: media_content_type(
                std::path::Path::new(&target_name)
                    .extension()
                    .and_then(|extension| extension.to_str()),
                location.is_audio(),
            ),
            etag: media_etag(
                resolved.account_id,
                target_id.provider_object_id(),
                object.remote_revision(),
                target_size,
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
    storage_object_id: Option<StorageObjectRecordId>,
    object_id: StorageObjectId,
    size: u64,
    content_type: &'static str,
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

    #[must_use]
    pub const fn content_type(&self) -> &'static str {
        self.content_type
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
        let opened = if let Some(record_id) = self.storage_object_id {
            storage_read::open_range(
                &self.database,
                self.backend.as_ref(),
                record_id,
                &self.object_id,
                range,
            )
            .await
            .map_err(map_storage_read_error)
        } else {
            self.backend
                .open_range(&self.object_id, range)
                .await
                .map_err(map_backend_error)
        };
        let stream = match opened {
            Ok(stream) => stream,
            Err(MediaReadError::RangeNotSatisfiable { size }) => {
                return Err(MediaReadError::RangeNotSatisfiable { size });
            }
            Err(error) => return Err(error),
        };
        Ok(OpenedMediaRange {
            stream,
            total_size: self.size,
            etag: self.etag.clone(),
        })
    }
}

fn media_content_type(container: Option<&str>, is_audio: bool) -> &'static str {
    match (is_audio, container.map(str::to_ascii_lowercase).as_deref()) {
        (false, Some("mp4" | "m4v")) => "video/mp4",
        (false, Some("webm")) => "video/webm",
        (false, Some("mkv" | "matroska")) => "video/x-matroska",
        (false, Some("ogg" | "ogv")) => "video/ogg",
        (true, Some("mp3")) => "audio/mpeg",
        (true, Some("m4a" | "mp4")) => "audio/mp4",
        (true, Some("aac")) => "audio/aac",
        (true, Some("ogg" | "oga")) => "audio/ogg",
        (true, Some("webm")) => "audio/webm",
        (true, Some("flac")) => "audio/flac",
        (true, Some("wav" | "wave")) => "audio/wav",
        (true, Some("mkv" | "matroska")) => "audio/x-matroska",
        _ => "application/octet-stream",
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
    #[error("invalid STRM descriptor: {0}")]
    Strm(String),
    #[error("STRM target is not a regular media file")]
    InvalidStrmTarget,
}

fn map_storage_read_error(error: StorageReadError) -> MediaReadError {
    match error {
        StorageReadError::Backend(BackendError::RangeNotSatisfiable { size }) => {
            MediaReadError::RangeNotSatisfiable { size }
        }
        StorageReadError::Backend(error) => MediaReadError::Backend(error),
        StorageReadError::Availability(error) => MediaReadError::Availability(error),
        StorageReadError::Projection(error) => MediaReadError::AvailabilityProjection(error),
    }
}

fn map_backend_error(error: BackendError) -> MediaReadError {
    match error {
        BackendError::RangeNotSatisfiable { size } => MediaReadError::RangeNotSatisfiable { size },
        error => MediaReadError::Backend(error),
    }
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
