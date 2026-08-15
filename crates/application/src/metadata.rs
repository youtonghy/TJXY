use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::{Client, StatusCode, header::CONTENT_TYPE};
use sea_orm::DatabaseConnection;
use thiserror::Error;
use tjxy_common::{CatalogItemId, ImageType};
use tjxy_db::{
    ClaimedWorkJob, MetadataImageCandidate, MetadataPublicationError,
    MetadataPublicationRepository, MetadataWorkError, MetadataWorkRepository,
    StorageSyncRepositoryError, WorkScope,
};
use tjxy_domain::{LocalMetadataAccessMode, MetadataSourceMode};
use tjxy_metadata::{
    MetadataError, MetadataImageReference, MetadataItemKind, MetadataProvider,
    MetadataProviderError, MetadataResolution, MetadataResolver, MetadataState, NfoDocument,
};
use tjxy_storage::{BackendError, ByteRange, StorageBackend, StorageObject, StorageObjectId};
use uuid::Uuid;

use crate::{
    AssetWriteError, AssetWriteService, PreparedAssetPublication, StorageBackendRegistry,
    StorageChangeProjectorError,
    storage_read::{self, StorageReadError},
};

const MAX_METADATA_IMAGE_BYTES: usize = 20 * 1024 * 1024;

#[async_trait]
pub trait MetadataImageFetcher: Send + Sync {
    async fn fetch(
        &self,
        reference: &MetadataImageReference,
    ) -> Result<MetadataImageBytes, MetadataImageFetchError>;
}

pub struct MetadataImageBytes {
    mime_type: String,
    bytes: Vec<u8>,
}

impl MetadataImageBytes {
    /// Defines one bounded encoded image returned by a metadata image fetcher.
    ///
    /// # Errors
    ///
    /// Returns [`MetadataImageFetchError::InvalidResponse`] for unsupported MIME types or bounds.
    pub fn new(
        mime_type: impl Into<String>,
        bytes: Vec<u8>,
    ) -> Result<Self, MetadataImageFetchError> {
        let mime_type = mime_type.into();
        if !valid_image_mime(&mime_type)
            || bytes.is_empty()
            || bytes.len() > MAX_METADATA_IMAGE_BYTES
        {
            return Err(MetadataImageFetchError::InvalidResponse);
        }
        Ok(Self { mime_type, bytes })
    }

    #[must_use]
    pub fn mime_type(&self) -> &str {
        &self.mime_type
    }

    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

pub struct ReqwestMetadataImageFetcher {
    client: Client,
}

impl ReqwestMetadataImageFetcher {
    /// Creates the bounded HTTPS-only metadata image client.
    ///
    /// # Errors
    ///
    /// Returns [`MetadataImageFetchError::Client`] when the HTTP client cannot be built.
    pub fn new() -> Result<Self, MetadataImageFetchError> {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .read_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::none())
            .https_only(true)
            .build()?;
        Ok(Self { client })
    }
}

#[async_trait]
impl MetadataImageFetcher for ReqwestMetadataImageFetcher {
    async fn fetch(
        &self,
        reference: &MetadataImageReference,
    ) -> Result<MetadataImageBytes, MetadataImageFetchError> {
        let url = reqwest::Url::parse(reference.url())
            .map_err(|_| MetadataImageFetchError::InvalidReference)?;
        let allowed_host = match reference.provider() {
            "Tmdb" => url.host_str() == Some("image.tmdb.org"),
            "TheAudioDB" => matches!(
                url.host_str(),
                Some(
                    "theaudiodb.com"
                        | "www.theaudiodb.com"
                        | "r2.theaudiodb.com"
                        | "media.theaudiodb.com"
                )
            ),
            _ => false,
        };
        if !allowed_host
            || url.scheme() != "https"
            || url.port().is_some_and(|port| port != 443)
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(MetadataImageFetchError::InvalidReference);
        }
        let response = self.client.get(url).send().await?;
        if response.status() != StatusCode::OK {
            return Err(MetadataImageFetchError::InvalidResponse);
        }
        if response
            .content_length()
            .is_some_and(|length| length > MAX_METADATA_IMAGE_BYTES as u64)
        {
            return Err(MetadataImageFetchError::InvalidResponse);
        }
        let mime_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.split(';').next())
            .map(str::trim)
            .filter(|value| valid_image_mime(value))
            .ok_or(MetadataImageFetchError::InvalidResponse)?
            .to_owned();
        let mut bytes = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            if bytes.len().saturating_add(chunk.len()) > MAX_METADATA_IMAGE_BYTES {
                return Err(MetadataImageFetchError::InvalidResponse);
            }
            bytes.extend_from_slice(&chunk);
        }
        MetadataImageBytes::new(mime_type, bytes)
    }
}

#[derive(Debug, Error)]
pub enum MetadataImageFetchError {
    #[error("metadata image reference is not permitted")]
    InvalidReference,
    #[error("metadata image response is invalid")]
    InvalidResponse,
    #[error("metadata image HTTP client failed: {0}")]
    Client(#[from] reqwest::Error),
}

fn valid_image_mime(mime_type: &str) -> bool {
    matches!(
        mime_type,
        "image/jpeg" | "image/png" | "image/gif" | "image/webp" | "image/bmp"
    )
}

pub struct MetadataImportService {
    database: DatabaseConnection,
}

impl MetadataImportService {
    #[must_use]
    pub const fn new(database: DatabaseConnection) -> Self {
        Self { database }
    }

    /// Parses and atomically publishes one NFO document for an existing catalog item.
    ///
    /// # Errors
    ///
    /// Returns parsing, item-kind, validation, identity-conflict, or database errors.
    pub async fn import_nfo(
        &self,
        item_id: CatalogItemId,
        bytes: &[u8],
        source_reference: &str,
    ) -> Result<MetadataImportReport, MetadataImportError> {
        let document = NfoDocument::parse(bytes, source_reference)?;
        let repository = MetadataPublicationRepository::new(&self.database);
        let lookup = repository.lookup(item_id).await?;
        if document.kind() != lookup.kind() {
            return Err(MetadataImportError::NfoKindMismatch);
        }
        let resolution = MetadataResolution::from_candidate(&lookup, document.into_candidate())?;
        let publication = repository.publish(item_id, &resolution).await?;
        Ok(MetadataImportReport {
            changed: publication.changed(),
            state: resolution.state(),
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MetadataImportReport {
    changed: bool,
    state: MetadataState,
}

impl MetadataImportReport {
    #[must_use]
    pub const fn changed(self) -> bool {
        self.changed
    }

    #[must_use]
    pub const fn state(self) -> MetadataState {
        self.state
    }
}

#[derive(Debug, Error)]
pub enum MetadataImportError {
    #[error("NFO metadata is invalid: {0}")]
    Nfo(#[from] MetadataError),
    #[error("NFO document kind does not match the catalog item")]
    NfoKindMismatch,
    #[error("metadata publication failed: {0}")]
    Publication(#[from] MetadataPublicationError),
}

pub struct MetadataResolveService {
    database: DatabaseConnection,
    backends: StorageBackendRegistry,
    providers: Vec<Arc<dyn MetadataProvider>>,
    asset_writer: Option<Arc<AssetWriteService>>,
    image_fetcher: Option<Arc<dyn MetadataImageFetcher>>,
}

impl MetadataResolveService {
    #[must_use]
    pub fn new(database: DatabaseConnection) -> Self {
        Self {
            database,
            backends: StorageBackendRegistry::new(),
            providers: Vec::new(),
            asset_writer: None,
            image_fetcher: None,
        }
    }

    #[must_use]
    pub fn with_backend_registry(mut self, backends: StorageBackendRegistry) -> Self {
        self.backends = backends;
        self
    }

    #[must_use]
    pub fn with_backend<Backend>(
        self,
        account_id: Uuid,
        provider_drive_id: impl Into<String>,
        backend: Arc<Backend>,
    ) -> Self
    where
        Backend: StorageBackend + 'static,
    {
        self.backends
            .insert_scoped(account_id, provider_drive_id, backend);
        self
    }

    #[must_use]
    pub fn with_dyn_backend(
        self,
        account_id: Uuid,
        provider_drive_id: impl Into<String>,
        backend: Arc<dyn StorageBackend>,
    ) -> Self {
        self.backends
            .insert_scoped(account_id, provider_drive_id, backend);
        self
    }

    #[must_use]
    pub fn with_provider<Provider>(mut self, provider: Arc<Provider>) -> Self
    where
        Provider: MetadataProvider + 'static,
    {
        self.providers.push(provider);
        self
    }

    #[must_use]
    pub fn with_dyn_provider(mut self, provider: Arc<dyn MetadataProvider>) -> Self {
        self.providers.push(provider);
        self
    }

    #[must_use]
    pub fn with_asset_writer(mut self, asset_writer: Arc<AssetWriteService>) -> Self {
        self.asset_writer = Some(asset_writer);
        self
    }

    #[must_use]
    pub fn with_image_fetcher(mut self, image_fetcher: Arc<dyn MetadataImageFetcher>) -> Self {
        self.image_fetcher = Some(image_fetcher);
        self
    }

    /// Resolves one durable metadata job from its SQL-selected sidecar or naming fallback.
    ///
    /// # Errors
    ///
    /// Returns an error without completing the job when its snapshot, object revision,
    /// NFO document, or fenced publication is invalid.
    #[allow(clippy::too_many_lines)] // Keeps one durable metadata job's snapshot-to-publication flow together.
    pub async fn execute(
        &self,
        claimed: &ClaimedWorkJob,
    ) -> Result<MetadataResolveReport, MetadataResolveError> {
        let repository = MetadataWorkRepository::new(&self.database);
        let snapshot = repository.snapshot(claimed).await?;
        if claimed.job().local_metadata_access_mode() == Some(LocalMetadataAccessMode::Direct) {
            if claimed.job().metadata_source_mode() != Some(MetadataSourceMode::LocalOnly) {
                return Err(MetadataResolveError::Work(MetadataWorkError::InvalidClaim));
            }
            let publication = repository.commit_direct(claimed, &snapshot).await?;
            tracing::debug!(
                job_id = %claimed.job().id().as_uuid(),
                scope_id = %claimed.job().scope().id(),
                nfo = snapshot.sidecar().is_some(),
                images = snapshot.images().len(),
                "indexed direct local metadata references"
            );
            return Ok(MetadataResolveReport {
                changed: publication.changed(),
                state: MetadataState::Partial,
                used_nfo: false,
            });
        }
        let providers = match claimed.job().metadata_source_mode() {
            Some(MetadataSourceMode::AutomaticScrape) => self.providers.clone(),
            Some(MetadataSourceMode::LocalOnly) => Vec::new(),
            None => {
                return Err(MetadataResolveError::Work(MetadataWorkError::InvalidClaim));
            }
        };
        let requires_remote_details =
            matches!(
                snapshot.lookup().kind(),
                MetadataItemKind::Movie | MetadataItemKind::Series
            ) && providers.iter().any(|provider| provider.name() == "Tmdb");
        let resolver = MetadataResolver::new(providers)?;
        let mut execution_warnings = Vec::new();
        let (mut resolution, used_nfo) = if let Some(sidecar) = snapshot.sidecar() {
            let backend = self
                .backends
                .backend_for_drive(sidecar.storage_account_id(), sidecar.provider_drive_id())
                .ok_or(MetadataResolveError::BackendUnavailable)?;
            let object_id = StorageObjectId::new(
                sidecar.provider().to_owned(),
                sidecar.provider_object_id().to_owned(),
            )?;
            let before = storage_read::get_object(
                &self.database,
                backend.as_ref(),
                sidecar.record_id(),
                &object_id,
            )
            .await
            .map_err(metadata_storage_read_error)?;
            validate_sidecar(sidecar, &object_id, &before)?;
            let bytes = read_sidecar(
                &self.database,
                backend.as_ref(),
                sidecar.record_id(),
                &object_id,
                sidecar.size(),
            )
            .await?;
            let after = storage_read::get_object(
                &self.database,
                backend.as_ref(),
                sidecar.record_id(),
                &object_id,
            )
            .await
            .map_err(metadata_storage_read_error)?;
            validate_sidecar(sidecar, &object_id, &after)?;
            if object_revision(&before) != object_revision(&after) || before.size() != after.size()
            {
                return Err(MetadataResolveError::ObjectChanged);
            }
            let source_reference = format!("storage-object:{}", sidecar.record_id());
            match NfoDocument::parse(&bytes, &source_reference) {
                Ok(document) => {
                    if document.kind() != snapshot.lookup().kind() {
                        return Err(MetadataResolveError::NfoKindMismatch);
                    }
                    (
                        resolver
                            .resolve_with_candidate(snapshot.lookup(), document.into_candidate())
                            .await?,
                        true,
                    )
                }
                Err(error) => {
                    execution_warnings.push(format!("Nfo: {error}"));
                    (resolver.resolve(snapshot.lookup()).await, false)
                }
            }
        } else {
            (resolver.resolve(snapshot.lookup()).await, false)
        };
        if requires_remote_details {
            resolution = resolution.require_complete_details();
        }
        if requires_remote_details
            && let Some(warning) = resolution
                .warnings()
                .iter()
                .find(|warning| warning.provider() == "Tmdb")
        {
            return Err(MetadataResolveError::Provider(warning.error()));
        }
        execution_warnings.extend(
            resolution
                .warnings()
                .iter()
                .map(|warning| format!("{}: {}", warning.provider(), warning.error())),
        );
        let WorkScope::CatalogItem(item_id) = claimed.job().scope() else {
            return Err(MetadataResolveError::Work(MetadataWorkError::InvalidClaim));
        };
        let mut prepared_assets = self
            .prepare_local_images(item_id, snapshot.images(), &mut execution_warnings)
            .await?;
        if !prepared_assets
            .iter()
            .any(|asset| asset.publication().image_type() == ImageType::Primary)
            && let Some(remote) = self
                .prepare_primary_image(item_id, &resolution, &mut execution_warnings)
                .await?
        {
            prepared_assets.push(remote);
        }
        let asset_publications = prepared_assets
            .iter()
            .map(PreparedAssetPublication::publication)
            .collect::<Vec<_>>();
        let publication = repository
            .commit(
                claimed,
                &snapshot,
                &resolution,
                &asset_publications,
                used_nfo,
                execution_warnings,
            )
            .await?;
        Ok(MetadataResolveReport {
            changed: publication.changed(),
            state: resolution.state(),
            used_nfo,
        })
    }

    async fn prepare_local_images(
        &self,
        item_id: CatalogItemId,
        candidates: &[MetadataImageCandidate],
        warnings: &mut Vec<String>,
    ) -> Result<Vec<PreparedAssetPublication>, MetadataResolveError> {
        let Some(writer) = self.asset_writer.as_ref() else {
            return Ok(Vec::new());
        };
        let mut prepared = Vec::new();
        for candidate in candidates {
            let file = candidate.file();
            if file.size() > MAX_METADATA_IMAGE_BYTES as u64 {
                warnings.push(format!("Local image {} is too large", file.name()));
                continue;
            }
            let Some(mime_type) = local_image_mime(file.name()) else {
                continue;
            };
            let backend = self
                .backends
                .backend_for_drive(file.storage_account_id(), file.provider_drive_id())
                .ok_or(MetadataResolveError::BackendUnavailable)?;
            let object_id = StorageObjectId::new(
                file.provider().to_owned(),
                file.provider_object_id().to_owned(),
            )?;
            let before = storage_read::get_object(
                &self.database,
                backend.as_ref(),
                file.record_id(),
                &object_id,
            )
            .await
            .map_err(metadata_storage_read_error)?;
            validate_sidecar(file, &object_id, &before)?;
            let bytes = read_sidecar(
                &self.database,
                backend.as_ref(),
                file.record_id(),
                &object_id,
                file.size(),
            )
            .await?;
            let after = storage_read::get_object(
                &self.database,
                backend.as_ref(),
                file.record_id(),
                &object_id,
            )
            .await
            .map_err(metadata_storage_read_error)?;
            validate_sidecar(file, &object_id, &after)?;
            if object_revision(&before) != object_revision(&after) || before.size() != after.size()
            {
                return Err(MetadataResolveError::ObjectChanged);
            }
            let source_reference = format!("storage-object:{}", file.record_id());
            match writer
                .prepare_original(
                    item_id,
                    candidate.image_type(),
                    0,
                    "Local",
                    Some(&source_reference),
                    mime_type,
                    &bytes,
                )
                .await
            {
                Ok(asset) => prepared.push(asset),
                Err(error) if invalid_remote_image(&error) => {
                    warnings.push(format!("Local image {}: {error}", file.name()));
                }
                Err(error) => return Err(MetadataResolveError::Asset(error)),
            }
        }
        Ok(prepared)
    }

    async fn prepare_primary_image(
        &self,
        item_id: CatalogItemId,
        resolution: &MetadataResolution,
        warnings: &mut Vec<String>,
    ) -> Result<Option<PreparedAssetPublication>, MetadataResolveError> {
        let (Some(writer), Some(fetcher), Some(reference)) = (
            self.asset_writer.as_ref(),
            self.image_fetcher.as_ref(),
            resolution.primary_image(),
        ) else {
            return Ok(None);
        };
        let image = match fetcher.fetch(reference).await {
            Ok(image) => image,
            Err(error) => {
                warnings.push(format!("{} image: {error}", reference.provider()));
                return Ok(None);
            }
        };
        match writer
            .prepare_original(
                item_id,
                ImageType::Primary,
                0,
                reference.provider(),
                Some(reference.reference()),
                image.mime_type(),
                image.bytes(),
            )
            .await
        {
            Ok(prepared) => Ok(Some(prepared)),
            Err(error) if invalid_remote_image(&error) => {
                warnings.push(format!("{} image: {error}", reference.provider()));
                Ok(None)
            }
            Err(error) => Err(MetadataResolveError::Asset(error)),
        }
    }
}

fn local_image_mime(name: &str) -> Option<&'static str> {
    match std::path::Path::new(name)
        .extension()?
        .to_str()?
        .to_ascii_lowercase()
        .as_str()
    {
        "jpg" | "jpeg" => Some("image/jpeg"),
        "png" => Some("image/png"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        "bmp" => Some("image/bmp"),
        _ => None,
    }
}

fn invalid_remote_image(error: &AssetWriteError) -> bool {
    matches!(
        error,
        AssetWriteError::InvalidBytes
            | AssetWriteError::EncodedTooLarge
            | AssetWriteError::UnsupportedFormat
            | AssetWriteError::FormatMismatch
            | AssetWriteError::DimensionsTooLarge
            | AssetWriteError::Repository(_)
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MetadataResolveReport {
    changed: bool,
    state: MetadataState,
    used_nfo: bool,
}

impl MetadataResolveReport {
    #[must_use]
    pub const fn changed(self) -> bool {
        self.changed
    }

    #[must_use]
    pub const fn state(self) -> MetadataState {
        self.state
    }

    #[must_use]
    pub const fn used_nfo(self) -> bool {
        self.used_nfo
    }
}

#[derive(Debug, Error)]
pub enum MetadataResolveError {
    #[error("metadata storage backend is not configured")]
    BackendUnavailable,
    #[error("metadata sidecar changed while it was being read")]
    ObjectChanged,
    #[error("metadata NFO kind does not match the catalog item")]
    NfoKindMismatch,
    #[error("metadata provider failed while loading complete details: {0}")]
    Provider(MetadataProviderError),
    #[error("metadata sidecar storage operation failed: {0}")]
    Storage(#[from] BackendError),
    #[error("metadata storage availability persistence failed: {0}")]
    Availability(#[from] StorageSyncRepositoryError),
    #[error("metadata storage availability projection failed: {0}")]
    AvailabilityProjection(#[from] StorageChangeProjectorError),
    #[error("metadata document is invalid: {0}")]
    Metadata(#[from] MetadataError),
    #[error("metadata work persistence failed: {0}")]
    Work(#[from] MetadataWorkError),
    #[error("metadata image storage failed: {0}")]
    Asset(#[from] AssetWriteError),
}

async fn read_sidecar(
    database: &DatabaseConnection,
    backend: &dyn StorageBackend,
    record_id: tjxy_common::StorageObjectRecordId,
    object_id: &StorageObjectId,
    size: u64,
) -> Result<Vec<u8>, MetadataResolveError> {
    let range = ByteRange::new(0, size)?;
    let mut stream = storage_read::open_range(database, backend, record_id, object_id, range)
        .await
        .map_err(metadata_storage_read_error)?;
    let expected = usize::try_from(size).map_err(|_| MetadataResolveError::ObjectChanged)?;
    let mut bytes = Vec::with_capacity(expected);
    while let Some(chunk) = stream.next().await {
        bytes.extend_from_slice(&chunk?);
        if bytes.len() > expected {
            return Err(MetadataResolveError::ObjectChanged);
        }
    }
    if bytes.len() != expected {
        return Err(MetadataResolveError::ObjectChanged);
    }
    Ok(bytes)
}

fn metadata_storage_read_error(error: StorageReadError) -> MetadataResolveError {
    match error {
        StorageReadError::Backend(error) => MetadataResolveError::Storage(error),
        StorageReadError::Availability(error) => MetadataResolveError::Availability(error),
        StorageReadError::Projection(error) => MetadataResolveError::AvailabilityProjection(error),
    }
}

fn validate_sidecar(
    candidate: &tjxy_db::MetadataSidecarCandidate,
    expected_id: &StorageObjectId,
    object: &StorageObject,
) -> Result<(), MetadataResolveError> {
    if object.id() != expected_id
        || object.size() != Some(candidate.size())
        || candidate
            .remote_revision()
            .is_some_and(|revision| object.remote_revision() != Some(revision))
    {
        return Err(MetadataResolveError::ObjectChanged);
    }
    Ok(())
}

fn object_revision(object: &StorageObject) -> (Option<&str>, Option<&str>, Option<&str>) {
    (object.remote_revision(), object.etag(), object.checksum())
}
