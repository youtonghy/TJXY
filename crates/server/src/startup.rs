use std::{collections::HashSet, fmt, path::PathBuf, sync::Arc, time::Duration as StdDuration};

use chrono::Duration;
use sea_orm::{ConnectionTrait, Database, DbBackend, DbErr, Statement};
use sea_orm_migration::MigratorTrait;
use thiserror::Error;
use tjxy_application::{
    AssetReadError, AssetReadService, AssetWriteError, AssetWriteService, AuthError, AuthService,
    CatalogQueryService, DisplayPreferencesService, LibraryService, MediaCollectionService,
    MediaReadService, MetadataImageFetchError, MetadataImportService, MetadataResolveService,
    PlaystateService, ProbeService, ReqwestMetadataImageFetcher, StorageBackendRegistry,
    SystemClock, TaskService, UserDataService,
};
use tjxy_cache::{CacheRuntime, CacheStartupError, RedisCacheConfig};
use tjxy_credentials::{CredentialCipher, CredentialCipherError};
use tjxy_db::{
    ApiKeyRepositoryError, CredentialRefreshState, LibraryRepository, LibraryRepositoryError,
    StorageAccountRepository, StorageAccountRepositoryError, StorageCredentialRepository,
    StorageCredentialRepositoryError,
};
use tjxy_metadata::MetadataProvider;
use tjxy_storage::StorageBackend;
use tjxy_storage_filesystem::FilesystemBackend;
use tjxy_storage_google_drive::{
    GoogleDriveBackend, GoogleDriveScope, GoogleOAuthClient, GoogleOAuthCredentials,
    RefreshingAccessTokenProvider,
};
use tjxy_storage_onedrive::{
    MicrosoftOAuthClient, MicrosoftOAuthCredentials, OneDriveBackend, OneDriveScope,
    RefreshingMicrosoftAccessTokenProvider,
};
use uuid::Uuid;

use crate::{AppState, ServerIdentity, worker};

pub struct BootstrapAdmin {
    username: String,
    password: String,
}

impl BootstrapAdmin {
    #[must_use]
    pub fn new(username: impl Into<String>, password: impl Into<String>) -> Self {
        Self {
            username: username.into(),
            password: password.into(),
        }
    }
}

impl fmt::Debug for BootstrapAdmin {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BootstrapAdmin")
            .field("username", &self.username)
            .field("password", &"[REDACTED]")
            .finish()
    }
}

pub struct StartupOptions {
    database_url: String,
    identity: ServerIdentity,
    bootstrap_admin: Option<BootstrapAdmin>,
    legacy_auth_enabled: bool,
    session_lifetime: Option<Duration>,
    max_concurrent_password_hashes: usize,
    assets_dir: PathBuf,
    lazy_wait_timeout: StdDuration,
    filesystem_backends: Vec<(Uuid, PathBuf)>,
    filesystem_realtime_enabled: bool,
    storage_backends: Vec<ConfiguredStorageBackend>,
    credential_cipher: Option<Arc<CredentialCipher>>,
    google_oauth: Option<crate::storage_admin::GoogleDriveOAuthConfiguration>,
    onedrive_oauth: Option<crate::storage_admin::MicrosoftOneDriveOAuthConfiguration>,
    redis_cache: RedisCacheConfig,
    metadata_providers: Vec<Arc<dyn MetadataProvider>>,
    media_refresh_interval: Option<StdDuration>,
}

struct ConfiguredStorageBackend {
    account_id: Uuid,
    provider_drive_id: String,
    backend: Arc<dyn StorageBackend>,
}

struct PreparedFilesystemBackend {
    account_id: Uuid,
    backend: Arc<FilesystemBackend>,
    realtime_enabled: bool,
}

impl fmt::Debug for StartupOptions {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StartupOptions")
            .field("database_url", &self.database_url)
            .field("identity", &self.identity)
            .field("bootstrap_admin", &self.bootstrap_admin)
            .field("legacy_auth_enabled", &self.legacy_auth_enabled)
            .field("session_lifetime", &self.session_lifetime)
            .field(
                "max_concurrent_password_hashes",
                &self.max_concurrent_password_hashes,
            )
            .field("assets_dir", &self.assets_dir)
            .field("lazy_wait_timeout", &self.lazy_wait_timeout)
            .field("filesystem_backend_count", &self.filesystem_backends.len())
            .field(
                "filesystem_realtime_enabled",
                &self.filesystem_realtime_enabled,
            )
            .field("storage_backend_count", &self.storage_backends.len())
            .field(
                "credential_cipher_configured",
                &self.credential_cipher.is_some(),
            )
            .field("google_oauth_configured", &self.google_oauth.is_some())
            .field("onedrive_oauth_configured", &self.onedrive_oauth.is_some())
            .field("redis_cache", &self.redis_cache)
            .field("metadata_provider_count", &self.metadata_providers.len())
            .field("media_refresh_interval", &self.media_refresh_interval)
            .finish()
    }
}

impl StartupOptions {
    #[must_use]
    pub fn new(database_url: impl Into<String>, identity: ServerIdentity) -> Self {
        Self {
            database_url: database_url.into(),
            identity,
            bootstrap_admin: None,
            legacy_auth_enabled: true,
            session_lifetime: None,
            max_concurrent_password_hashes: 2,
            assets_dir: PathBuf::from("./data/assets"),
            lazy_wait_timeout: StdDuration::from_millis(2_500),
            filesystem_backends: Vec::new(),
            filesystem_realtime_enabled: true,
            storage_backends: Vec::new(),
            credential_cipher: None,
            google_oauth: None,
            onedrive_oauth: None,
            redis_cache: RedisCacheConfig::default(),
            metadata_providers: Vec::new(),
            media_refresh_interval: None,
        }
    }

    #[must_use]
    pub fn with_bootstrap_admin(mut self, admin: BootstrapAdmin) -> Self {
        self.bootstrap_admin = Some(admin);
        self
    }

    #[must_use]
    pub const fn with_legacy_auth_enabled(mut self, enabled: bool) -> Self {
        self.legacy_auth_enabled = enabled;
        self
    }

    #[must_use]
    pub fn with_assets_dir(mut self, assets_dir: impl Into<PathBuf>) -> Self {
        self.assets_dir = assets_dir.into();
        self
    }

    #[must_use]
    pub const fn with_lazy_wait_timeout(mut self, timeout: StdDuration) -> Self {
        self.lazy_wait_timeout = timeout;
        self
    }

    #[must_use]
    pub fn with_filesystem_backend(mut self, account_id: Uuid, root: impl Into<PathBuf>) -> Self {
        self.filesystem_backends.push((account_id, root.into()));
        self
    }

    #[must_use]
    pub const fn with_filesystem_realtime_enabled(mut self, enabled: bool) -> Self {
        self.filesystem_realtime_enabled = enabled;
        self
    }

    /// Registers a runtime-selected backend for one provider drive.
    #[must_use]
    pub fn with_storage_backend(
        mut self,
        account_id: Uuid,
        provider_drive_id: impl Into<String>,
        backend: Arc<dyn StorageBackend>,
    ) -> Self {
        self.storage_backends.push(ConfiguredStorageBackend {
            account_id,
            provider_drive_id: provider_drive_id.into(),
            backend,
        });
        self
    }

    /// Enables encrypted provider credential loading from SQL.
    #[must_use]
    pub fn with_credential_cipher(mut self, cipher: Arc<CredentialCipher>) -> Self {
        self.credential_cipher = Some(cipher);
        self
    }

    /// Enables server-side Google Drive OAuth for the Admin storage binding flow.
    #[must_use]
    pub fn with_google_oauth(
        mut self,
        configuration: crate::storage_admin::GoogleDriveOAuthConfiguration,
    ) -> Self {
        self.google_oauth = Some(configuration);
        self
    }

    /// Enables server-side `OneDrive` Personal OAuth for the Admin storage binding flow.
    #[must_use]
    pub fn with_onedrive_oauth(
        mut self,
        configuration: crate::storage_admin::MicrosoftOneDriveOAuthConfiguration,
    ) -> Self {
        self.onedrive_oauth = Some(configuration);
        self
    }

    #[must_use]
    pub fn with_redis_cache(mut self, config: RedisCacheConfig) -> Self {
        self.redis_cache = config;
        self
    }

    #[must_use]
    pub fn with_metadata_provider<Provider>(mut self, provider: Arc<Provider>) -> Self
    where
        Provider: MetadataProvider + 'static,
    {
        self.metadata_providers.push(provider);
        self
    }

    /// Enables periodic durable media refresh submission. A zero duration disables it.
    #[must_use]
    pub fn with_media_refresh_interval(mut self, interval: StdDuration) -> Self {
        self.media_refresh_interval = (!interval.is_zero()).then_some(interval);
        self
    }
}

/// Connects the SQL source of truth, applies migrations, optionally creates the
/// first administrator, and returns ready application state.
///
/// # Errors
///
/// Returns [`InitializationError`] without exposing bootstrap credentials when
/// connection, migration, or authentication setup fails.
#[allow(clippy::too_many_lines)] // Startup deliberately composes every long-lived service once.
pub async fn initialize(options: StartupOptions) -> Result<AppState, InitializationError> {
    let storage_admin_cipher = options.credential_cipher.clone();
    validate_storage_backends(&options.storage_backends)?;
    let database = Database::connect(&options.database_url).await?;
    if database.get_database_backend() == DbBackend::Sqlite {
        database
            .execute(Statement::from_string(
                DbBackend::Sqlite,
                "PRAGMA foreign_keys = ON".to_owned(),
            ))
            .await?;
    }
    tjxy_db::Migrator::up(&database, None).await?;
    let mut filesystem_backends = options.filesystem_backends;
    for configured in LibraryRepository::new(&database)
        .active_filesystem_roots()
        .await?
    {
        if filesystem_backends
            .iter()
            .all(|(account_id, _)| *account_id != configured.account_id())
        {
            filesystem_backends.push((
                configured.account_id(),
                PathBuf::from(configured.root_path()),
            ));
        }
    }
    let google_bindings = StorageAccountRepository::new(&database)
        .active_provider_bindings("google-drive")
        .await
        .map_err(GoogleBackendLoadError::from)?;
    let onedrive_bindings = StorageAccountRepository::new(&database)
        .active_provider_bindings("onedrive")
        .await
        .map_err(OneDriveBackendLoadError::from)?;
    let mut storage_backends = options.storage_backends;
    if !google_bindings.is_empty() || !onedrive_bindings.is_empty() {
        let cipher = options
            .credential_cipher
            .clone()
            .ok_or(InitializationError::MissingCredentialCipher)?;
        storage_backends.extend(load_google_backends(&database, &cipher).await?);
        storage_backends.extend(load_onedrive_backends(&database, cipher).await?);
    }
    validate_storage_backends(&storage_backends)?;
    validate_backend_accounts(&filesystem_backends, &storage_backends)?;
    let mut auth = AuthService::new(
        database.clone(),
        SystemClock,
        options.session_lifetime,
        options.max_concurrent_password_hashes,
    )
    .await?;
    if let Some(cipher) = options.credential_cipher.clone() {
        auth = auth.with_credential_cipher(cipher);
    }
    if !auth.has_enabled_admin().await? {
        let admin = options
            .bootstrap_admin
            .ok_or(InitializationError::MissingInitialAdministrator)?;
        if admin.password.is_empty() {
            return Err(InitializationError::EmptyBootstrapPassword);
        }
        auth.create_initial_admin(&admin.username, &admin.password)
            .await?;
    }
    let has_enabled_admin = auth.has_enabled_admin().await?;
    auth.validate_api_key_envelopes()
        .await
        .map_err(|error| api_key_validation_error(&error))?;
    let auth = Arc::new(auth);
    let cache_config = options.redis_cache;
    let cache = Arc::new(CacheRuntime::connect(cache_config.clone()).await?);
    let mut catalog = CatalogQueryService::new(database.clone())
        .with_lazy_wait_timeout(options.lazy_wait_timeout);
    if cache.is_enabled() {
        catalog = catalog.with_cache_ttls(
            cache.clone(),
            cache_config.keys().clone(),
            cache_config.home_ttl(),
            cache_config.item_ttl(),
            cache_config.empty_expansion_ttl(),
        );
    }
    let catalog = Arc::new(catalog);
    let libraries = Arc::new(LibraryService::new(database.clone()));
    let asset_writer =
        Arc::new(AssetWriteService::new(database.clone(), options.assets_dir.clone()).await?);
    let image_fetcher = Arc::new(ReqwestMetadataImageFetcher::new()?);
    let assets = Arc::new(AssetReadService::new(database.clone(), options.assets_dir).await?);
    let filesystem_backends =
        prepare_filesystem_backends(filesystem_backends, options.filesystem_realtime_enabled)
            .await?;
    let (media, storage_runtime) = configure_storage(
        &database,
        filesystem_backends,
        storage_backends,
        options.metadata_providers,
        asset_writer,
        image_fetcher,
        options.filesystem_realtime_enabled,
    )?;
    let realtime_events = Arc::new(crate::socket::RealtimeEvents::new());
    worker::spawn_cache_invalidation_worker(
        database.clone(),
        cache.clone(),
        Arc::clone(&realtime_events),
    );
    worker::spawn_storage_change_reconciler(database.clone());
    worker::spawn_source_index_worker(database.clone());
    worker::spawn_discover_worker(database.clone());
    worker::spawn_series_expand_worker(database.clone());
    worker::spawn_full_scan_worker(database.clone());
    let media = Arc::new(media);
    let playstate = Arc::new(PlaystateService::new(database.clone()));
    let tasks = Arc::new(TaskService::new(database.clone()));
    if let Some(interval) = options.media_refresh_interval {
        worker::spawn_media_refresh_scheduler(Arc::clone(&tasks), interval);
    }
    let (storage_admin, import_admin) = configure_admin_services(
        &database,
        storage_admin_cipher,
        options.google_oauth,
        options.onedrive_oauth,
        Arc::clone(&storage_runtime),
    );
    let metadata_import = Arc::new(MetadataImportService::new(database.clone()));
    let relink_admin = Arc::new(crate::relink_admin::RelinkAdminService::new(
        database.clone(),
    ));
    let media_collections = Arc::new(MediaCollectionService::new(database.clone()));
    let display_preferences = Arc::new(DisplayPreferencesService::new(database.clone()));
    let user_data = Arc::new(UserDataService::new(database));
    let warm_home_cache = cache.is_enabled();
    let state = AppState::new(
        options
            .identity
            .with_startup_wizard_completed(has_enabled_admin),
    )
    .with_auth(auth.clone())
    .with_catalog(catalog.clone())
    .with_libraries(libraries)
    .with_assets(assets)
    .with_media(media)
    .with_media_collections(media_collections)
    .with_display_preferences(display_preferences)
    .with_playstate(playstate)
    .with_tasks(tasks)
    .with_user_data(user_data)
    .with_storage_admin(storage_admin)
    .with_import_admin(import_admin)
    .with_metadata_import(metadata_import)
    .with_relink_admin(relink_admin)
    .with_storage_runtime(storage_runtime)
    .with_realtime_events(realtime_events)
    .with_legacy_auth_enabled(options.legacy_auth_enabled)
    .with_ready(true);
    if warm_home_cache {
        worker::spawn_home_cache_warm_worker(auth, catalog);
    }
    Ok(state)
}

fn configure_admin_services(
    database: &sea_orm::DatabaseConnection,
    cipher: Option<Arc<CredentialCipher>>,
    google_oauth: Option<crate::storage_admin::GoogleDriveOAuthConfiguration>,
    onedrive_oauth: Option<crate::storage_admin::MicrosoftOneDriveOAuthConfiguration>,
    storage_runtime: Arc<crate::runtime_storage::RuntimeStorageManager>,
) -> (
    Option<Arc<crate::storage_admin::StorageAdminService>>,
    Option<Arc<crate::import_admin::ImportAdminService>>,
) {
    let Some(cipher) = cipher else {
        return (None, None);
    };
    worker::spawn_import_worker(database.clone(), Arc::clone(&cipher));
    (
        Some(Arc::new(crate::storage_admin::StorageAdminService::new(
            database.clone(),
            Arc::clone(&cipher),
            google_oauth,
            onedrive_oauth,
            storage_runtime,
        ))),
        Some(Arc::new(crate::import_admin::ImportAdminService::new(
            database.clone(),
            cipher,
        ))),
    )
}

async fn load_onedrive_backends(
    database: &sea_orm::DatabaseConnection,
    cipher: Arc<CredentialCipher>,
) -> Result<Vec<ConfiguredStorageBackend>, OneDriveBackendLoadError> {
    let bindings = StorageAccountRepository::new(database)
        .active_provider_bindings("onedrive")
        .await?;
    let credentials = StorageCredentialRepository::new(database);
    let mut configured = Vec::with_capacity(bindings.len());
    for binding in bindings {
        let credential = credentials
            .get(binding.credential_id())
            .await?
            .ok_or(OneDriveBackendLoadError::MissingCredential)?;
        if credential.refresh_state() != CredentialRefreshState::Ready {
            continue;
        }
        let plaintext = cipher.open(credential.id(), "onedrive", credential.envelope())?;
        let oauth = MicrosoftOAuthCredentials::from_payload_json(&plaintext)?;
        let store = Arc::new(crate::storage_admin::SqlMicrosoftCredentialStore {
            database: database.clone(),
            cipher: Arc::clone(&cipher),
            credential_id: credential.id(),
        });
        let provider = RefreshingMicrosoftAccessTokenProvider::new(
            oauth,
            Arc::new(MicrosoftOAuthClient::new()?),
        )
        .with_credential_store(store);
        let backend = Arc::new(OneDriveBackend::new(
            provider,
            OneDriveScope::Personal,
            binding.provider_drive_id(),
        )?);
        configured.push(ConfiguredStorageBackend {
            account_id: binding.account_id(),
            provider_drive_id: binding.provider_drive_id().to_owned(),
            backend,
        });
    }
    Ok(configured)
}

async fn load_google_backends(
    database: &sea_orm::DatabaseConnection,
    cipher: &CredentialCipher,
) -> Result<Vec<ConfiguredStorageBackend>, GoogleBackendLoadError> {
    let bindings = StorageAccountRepository::new(database)
        .active_provider_bindings("google-drive")
        .await?;
    let credentials = StorageCredentialRepository::new(database);
    let mut configured = Vec::with_capacity(bindings.len());
    for binding in bindings {
        let credential = credentials
            .get(binding.credential_id())
            .await?
            .ok_or(GoogleBackendLoadError::MissingCredential)?;
        if credential.refresh_state() != CredentialRefreshState::Ready {
            continue;
        }
        let plaintext = cipher.open(credential.id(), "google-drive", credential.envelope())?;
        let oauth = GoogleOAuthCredentials::from_payload_json(&plaintext)?;
        let provider =
            RefreshingAccessTokenProvider::new(oauth, Arc::new(GoogleOAuthClient::new()?));
        let scope = if binding.provider_drive_id() == "my-drive" {
            GoogleDriveScope::MyDrive
        } else {
            GoogleDriveScope::SharedDrive(binding.provider_drive_id().to_owned())
        };
        let backend = Arc::new(GoogleDriveBackend::new(provider, scope)?);
        configured.push(ConfiguredStorageBackend {
            account_id: binding.account_id(),
            provider_drive_id: binding.provider_drive_id().to_owned(),
            backend,
        });
    }
    Ok(configured)
}

async fn prepare_filesystem_backends(
    configured: Vec<(Uuid, PathBuf)>,
    realtime_enabled: bool,
) -> Result<Vec<PreparedFilesystemBackend>, tjxy_storage::BackendError> {
    let mut prepared = Vec::with_capacity(configured.len());
    for (account_id, root) in configured {
        let backend = Arc::new(FilesystemBackend::new(root).await?);
        prepared.push(PreparedFilesystemBackend {
            account_id,
            backend,
            realtime_enabled,
        });
    }
    Ok(prepared)
}

fn configure_storage(
    database: &sea_orm::DatabaseConnection,
    filesystem_backends: Vec<PreparedFilesystemBackend>,
    storage_backends: Vec<ConfiguredStorageBackend>,
    metadata_providers: Vec<Arc<dyn MetadataProvider>>,
    asset_writer: Arc<AssetWriteService>,
    image_fetcher: Arc<ReqwestMetadataImageFetcher>,
    filesystem_realtime_enabled: bool,
) -> Result<
    (
        MediaReadService,
        Arc<crate::runtime_storage::RuntimeStorageManager>,
    ),
    crate::runtime_storage::RuntimeStorageError,
> {
    let backends = StorageBackendRegistry::new();
    let media = MediaReadService::new(database.clone()).with_backend_registry(backends.clone());
    let probe = ProbeService::new(database.clone()).with_backend_registry(backends.clone());
    let mut metadata = MetadataResolveService::new(database.clone())
        .with_backend_registry(backends.clone())
        .with_asset_writer(asset_writer)
        .with_image_fetcher(image_fetcher);
    for provider in metadata_providers {
        metadata = metadata.with_dyn_provider(provider);
    }
    let runtime = Arc::new(crate::runtime_storage::RuntimeStorageManager::new(
        database.clone(),
        backends,
        filesystem_realtime_enabled,
    ));
    for configured in filesystem_backends {
        debug_assert_eq!(configured.realtime_enabled, filesystem_realtime_enabled);
        runtime.activate_filesystem(configured.account_id, configured.backend)?;
    }
    for configured in storage_backends {
        runtime.activate_provider(
            configured.account_id,
            configured.provider_drive_id,
            configured.backend,
        )?;
    }
    worker::spawn_probe_worker(database.clone(), Arc::new(probe));
    worker::spawn_metadata_worker(database.clone(), Arc::new(metadata));
    Ok((media, runtime))
}

fn validate_storage_backends(
    configured: &[ConfiguredStorageBackend],
) -> Result<(), InitializationError> {
    let mut scopes = HashSet::with_capacity(configured.len());
    for backend in configured {
        if backend.provider_drive_id.trim().is_empty() {
            return Err(InitializationError::InvalidStorageBackend(
                "provider drive id must not be empty".into(),
            ));
        }
        if !scopes.insert((backend.account_id, backend.provider_drive_id.as_str())) {
            return Err(InitializationError::InvalidStorageBackend(format!(
                "duplicate storage backend for account {} and provider drive {}",
                backend.account_id, backend.provider_drive_id
            )));
        }
    }
    Ok(())
}

fn validate_backend_accounts(
    filesystem_backends: &[(Uuid, PathBuf)],
    storage_backends: &[ConfiguredStorageBackend],
) -> Result<(), InitializationError> {
    let mut accounts = HashSet::with_capacity(filesystem_backends.len() + storage_backends.len());
    for (account_id, _) in filesystem_backends {
        if !accounts.insert(*account_id) {
            return Err(InitializationError::InvalidStorageBackend(format!(
                "duplicate storage backend account {account_id}"
            )));
        }
    }
    for backend in storage_backends {
        if !accounts.insert(backend.account_id) {
            return Err(InitializationError::InvalidStorageBackend(format!(
                "duplicate storage backend account {}",
                backend.account_id
            )));
        }
    }
    Ok(())
}

fn api_key_validation_error(error: &AuthError) -> InitializationError {
    let category = match error {
        AuthError::CredentialCipherUnavailable => ApiKeyValidationError::KeyringUnavailable,
        AuthError::ApiKeyRepository(
            ApiKeyRepositoryError::InvalidStoredAppName
            | ApiKeyRepositoryError::InvalidStoredDigest
            | ApiKeyRepositoryError::InvalidStoredEnvelope(_)
            | ApiKeyRepositoryError::InvalidStoredRow(_)
            | ApiKeyRepositoryError::StoredCapacityExceeded,
        ) => ApiKeyValidationError::StoredStateInvalid,
        AuthError::CredentialCipher(CredentialCipherError::UnknownKeyVersion) => {
            ApiKeyValidationError::KeyringUnavailable
        }
        AuthError::CredentialCipher(_) => ApiKeyValidationError::EnvelopeUnreadable,
        _ => ApiKeyValidationError::PersistenceUnavailable,
    };
    InitializationError::ApiKeyValidation(category)
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum ApiKeyValidationError {
    #[error("API key credential keyring is unavailable")]
    KeyringUnavailable,
    #[error("persisted API key envelopes are unreadable")]
    EnvelopeUnreadable,
    #[error("persisted API key state is invalid")]
    StoredStateInvalid,
    #[error("persisted API key validation storage is unavailable")]
    PersistenceUnavailable,
}

#[derive(Debug, Error)]
pub enum InitializationError {
    #[error("a bootstrap administrator is required for a database with no users")]
    MissingInitialAdministrator,
    #[error("the bootstrap administrator password must not be empty")]
    EmptyBootstrapPassword,
    #[error("active encrypted storage accounts require a credential cipher")]
    MissingCredentialCipher,
    #[error("invalid storage backend configuration: {0}")]
    InvalidStorageBackend(String),
    #[error("database initialization failed: {0}")]
    Database(#[from] DbErr),
    #[error("filesystem storage configuration query failed: {0}")]
    FilesystemConfiguration(#[from] LibraryRepositoryError),
    #[error("authentication initialization failed: {0}")]
    Authentication(#[from] AuthError),
    #[error("API key validation failed")]
    ApiKeyValidation(#[source] ApiKeyValidationError),
    #[error("asset service initialization failed: {0}")]
    Asset(#[from] AssetReadError),
    #[error("asset writer initialization failed: {0}")]
    AssetWriter(#[from] AssetWriteError),
    #[error("metadata image client initialization failed: {0}")]
    MetadataImage(#[from] MetadataImageFetchError),
    #[error("filesystem storage backend initialization failed: {0}")]
    StorageBackend(#[from] tjxy_storage::BackendError),
    #[error("cache initialization failed: {0}")]
    Cache(#[from] CacheStartupError),
    #[error("Google storage backend loading failed: {0}")]
    GoogleStorage(#[from] GoogleBackendLoadError),
    #[error("OneDrive storage backend loading failed: {0}")]
    OneDriveStorage(#[from] OneDriveBackendLoadError),
    #[error("runtime storage activation failed: {0}")]
    RuntimeStorage(#[from] crate::runtime_storage::RuntimeStorageError),
}

#[derive(Debug, Error)]
pub enum GoogleBackendLoadError {
    #[error("active Google storage account query failed: {0}")]
    Accounts(#[from] StorageAccountRepositoryError),
    #[error("encrypted Google credential query failed: {0}")]
    Credential(#[from] StorageCredentialRepositoryError),
    #[error("active Google storage account references a missing credential")]
    MissingCredential,
    #[error("encrypted Google credential could not be authenticated: {0}")]
    Cipher(#[from] CredentialCipherError),
    #[error("Google storage backend configuration is invalid: {0}")]
    Backend(#[from] tjxy_storage::BackendError),
}

#[derive(Debug, Error)]
pub enum OneDriveBackendLoadError {
    #[error("active OneDrive storage account query failed: {0}")]
    Accounts(#[from] StorageAccountRepositoryError),
    #[error("encrypted OneDrive credential query failed: {0}")]
    Credential(#[from] StorageCredentialRepositoryError),
    #[error("active OneDrive storage account references a missing credential")]
    MissingCredential,
    #[error("encrypted OneDrive credential could not be authenticated: {0}")]
    Cipher(#[from] CredentialCipherError),
    #[error("OneDrive storage backend configuration is invalid: {0}")]
    Backend(#[from] tjxy_storage::BackendError),
}

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, sync::Arc};

    use sea_orm::{
        ConnectionTrait, Database, DbErr,
        sea_query::{Alias, Query},
    };
    use sea_orm_migration::MigratorTrait;
    use tempfile::TempDir;
    use tjxy_application::AuthError;
    use tjxy_credentials::{CredentialCipher, CredentialCipherError, CredentialKey};
    use tjxy_db::{ApiKeyRepositoryError, CredentialRefreshState, StorageCredentialRepository};
    use tjxy_storage_google_drive::GoogleOAuthCredentials;
    use tjxy_storage_onedrive::MicrosoftOAuthCredentials;
    use uuid::Uuid;

    use super::{
        ApiKeyValidationError, InitializationError, api_key_validation_error, load_google_backends,
        load_onedrive_backends, prepare_filesystem_backends,
    };

    #[test]
    fn api_key_validation_categories_distinguish_keyring_cipher_and_stored_type_errors() {
        for (error, expected) in [
            (
                AuthError::CredentialCipher(CredentialCipherError::UnknownKeyVersion),
                ApiKeyValidationError::KeyringUnavailable,
            ),
            (
                AuthError::CredentialCipher(CredentialCipherError::AuthenticationFailed),
                ApiKeyValidationError::EnvelopeUnreadable,
            ),
            (
                AuthError::ApiKeyRepository(ApiKeyRepositoryError::InvalidStoredRow(DbErr::Type(
                    "invalid stored field".to_owned(),
                ))),
                ApiKeyValidationError::StoredStateInvalid,
            ),
        ] {
            let InitializationError::ApiKeyValidation(actual) = api_key_validation_error(&error)
            else {
                panic!("expected API key validation category");
            };
            assert_eq!(actual, expected);
        }
    }

    #[tokio::test]
    async fn filesystem_backends_are_all_validated_before_worker_configuration() {
        let valid = TempDir::new().unwrap();
        let missing = PathBuf::from(format!("/definitely/missing/tjxy-{}", Uuid::new_v4()));

        let result = prepare_filesystem_backends(
            vec![
                (Uuid::new_v4(), valid.path().to_owned()),
                (Uuid::new_v4(), missing),
            ],
            false,
        )
        .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)] // Builds one encrypted provider binding at the composition root.
    async fn encrypted_google_binding_loads_without_plaintext_configuration() {
        let database = Database::connect("sqlite::memory:").await.unwrap();
        tjxy_db::Migrator::up(&database, None).await.unwrap();
        let account_id = Uuid::new_v4();
        let credential_id = Uuid::new_v4();
        let root_id = Uuid::new_v4();
        let object_id = Uuid::new_v4();
        let cipher =
            CredentialCipher::new(CredentialKey::new(3, [3_u8; 32]).unwrap(), Vec::new()).unwrap();
        let plaintext = GoogleOAuthCredentials::new("client", "secret", "refresh")
            .unwrap()
            .to_payload_json()
            .unwrap();
        let envelope = cipher
            .seal(credential_id, "google-drive", &plaintext)
            .unwrap();
        StorageCredentialRepository::new(&database)
            .put(credential_id, &envelope, CredentialRefreshState::Ready)
            .await
            .unwrap();
        let backend = database.get_database_backend();
        database
            .execute(
                backend.build(
                    Query::insert()
                        .into_table(Alias::new("storage_accounts"))
                        .columns([
                            Alias::new("id"),
                            Alias::new("provider"),
                            Alias::new("display_name"),
                            Alias::new("account_identity"),
                            Alias::new("credential_ref"),
                            Alias::new("status"),
                        ])
                        .values_panic([
                            account_id.into(),
                            "google-drive".into(),
                            "Drive".into(),
                            "account".into(),
                            credential_id.to_string().into(),
                            "Active".into(),
                        ]),
                ),
            )
            .await
            .unwrap();
        database
            .execute(
                backend.build(
                    Query::insert()
                        .into_table(Alias::new("storage_roots"))
                        .columns([
                            Alias::new("id"),
                            Alias::new("storage_account_id"),
                            Alias::new("provider_root_id"),
                            Alias::new("sync_revision"),
                            Alias::new("reconciled_sync_revision"),
                        ])
                        .values_panic([
                            root_id.into(),
                            account_id.into(),
                            "root".into(),
                            0_i64.into(),
                            0_i64.into(),
                        ]),
                ),
            )
            .await
            .unwrap();
        database
            .execute(
                backend.build(
                    Query::insert()
                        .into_table(Alias::new("storage_objects"))
                        .columns([
                            Alias::new("id"),
                            Alias::new("storage_account_id"),
                            Alias::new("provider_drive_id"),
                            Alias::new("provider_object_id"),
                            Alias::new("name"),
                            Alias::new("normalized_name"),
                            Alias::new("object_type"),
                            Alias::new("observed_sync_revision"),
                            Alias::new("children_indexed"),
                            Alias::new("children_index_revision"),
                            Alias::new("identity_quality"),
                            Alias::new("presence_state"),
                        ])
                        .values_panic([
                            object_id.into(),
                            account_id.into(),
                            "shared-drive-id".into(),
                            "root".into(),
                            "Root".into(),
                            "root".into(),
                            "Directory".into(),
                            0_i64.into(),
                            false.into(),
                            0_i64.into(),
                            "ProviderStableId".into(),
                            "Present".into(),
                        ]),
                ),
            )
            .await
            .unwrap();
        database
            .execute(
                backend.build(
                    Query::insert()
                        .into_table(Alias::new("storage_root_objects"))
                        .columns([
                            Alias::new("id"),
                            Alias::new("storage_root_id"),
                            Alias::new("storage_object_id"),
                            Alias::new("observed_sync_revision"),
                            Alias::new("children_indexed"),
                            Alias::new("children_index_revision"),
                            Alias::new("presence_state"),
                        ])
                        .values_panic([
                            Uuid::new_v4().into(),
                            root_id.into(),
                            object_id.into(),
                            0_i64.into(),
                            false.into(),
                            0_i64.into(),
                            "Present".into(),
                        ]),
                ),
            )
            .await
            .unwrap();

        let configured = load_google_backends(&database, &cipher).await.unwrap();

        assert_eq!(configured.len(), 1);
        assert_eq!(configured[0].account_id, account_id);
        assert_eq!(configured[0].provider_drive_id, "shared-drive-id");
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)] // Builds one encrypted Personal OneDrive binding.
    async fn encrypted_onedrive_binding_loads_as_personal_only() {
        let database = Database::connect("sqlite::memory:").await.unwrap();
        tjxy_db::Migrator::up(&database, None).await.unwrap();
        let account_id = Uuid::new_v4();
        let credential_id = Uuid::new_v4();
        let root_id = Uuid::new_v4();
        let object_id = Uuid::new_v4();
        let cipher = Arc::new(
            CredentialCipher::new(CredentialKey::new(5, [5_u8; 32]).unwrap(), Vec::new()).unwrap(),
        );
        let plaintext = MicrosoftOAuthCredentials::new("client", None, "refresh")
            .unwrap()
            .to_payload_json()
            .unwrap();
        let envelope = cipher.seal(credential_id, "onedrive", &plaintext).unwrap();
        StorageCredentialRepository::new(&database)
            .put(credential_id, &envelope, CredentialRefreshState::Ready)
            .await
            .unwrap();
        let backend = database.get_database_backend();
        database
            .execute(
                backend.build(
                    Query::insert()
                        .into_table(Alias::new("storage_accounts"))
                        .columns([
                            Alias::new("id"),
                            Alias::new("provider"),
                            Alias::new("display_name"),
                            Alias::new("account_identity"),
                            Alias::new("credential_ref"),
                            Alias::new("status"),
                        ])
                        .values_panic([
                            account_id.into(),
                            "onedrive".into(),
                            "OneDrive".into(),
                            "account".into(),
                            credential_id.to_string().into(),
                            "Active".into(),
                        ]),
                ),
            )
            .await
            .unwrap();
        database
            .execute(
                backend.build(
                    Query::insert()
                        .into_table(Alias::new("storage_roots"))
                        .columns([
                            Alias::new("id"),
                            Alias::new("storage_account_id"),
                            Alias::new("provider_root_id"),
                            Alias::new("sync_revision"),
                            Alias::new("reconciled_sync_revision"),
                        ])
                        .values_panic([
                            root_id.into(),
                            account_id.into(),
                            "root".into(),
                            0_i64.into(),
                            0_i64.into(),
                        ]),
                ),
            )
            .await
            .unwrap();
        database
            .execute(
                backend.build(
                    Query::insert()
                        .into_table(Alias::new("storage_objects"))
                        .columns([
                            Alias::new("id"),
                            Alias::new("storage_account_id"),
                            Alias::new("provider_drive_id"),
                            Alias::new("provider_object_id"),
                            Alias::new("name"),
                            Alias::new("normalized_name"),
                            Alias::new("object_type"),
                            Alias::new("observed_sync_revision"),
                            Alias::new("children_indexed"),
                            Alias::new("children_index_revision"),
                            Alias::new("identity_quality"),
                            Alias::new("presence_state"),
                        ])
                        .values_panic([
                            object_id.into(),
                            account_id.into(),
                            "personal-drive-id".into(),
                            "root".into(),
                            "Root".into(),
                            "root".into(),
                            "Directory".into(),
                            0_i64.into(),
                            false.into(),
                            0_i64.into(),
                            "ProviderStableId".into(),
                            "Present".into(),
                        ]),
                ),
            )
            .await
            .unwrap();
        database
            .execute(
                backend.build(
                    Query::insert()
                        .into_table(Alias::new("storage_root_objects"))
                        .columns([
                            Alias::new("id"),
                            Alias::new("storage_root_id"),
                            Alias::new("storage_object_id"),
                            Alias::new("observed_sync_revision"),
                            Alias::new("children_indexed"),
                            Alias::new("children_index_revision"),
                            Alias::new("presence_state"),
                        ])
                        .values_panic([
                            Uuid::new_v4().into(),
                            root_id.into(),
                            object_id.into(),
                            0_i64.into(),
                            false.into(),
                            0_i64.into(),
                            "Present".into(),
                        ]),
                ),
            )
            .await
            .unwrap();

        let configured = load_onedrive_backends(&database, cipher).await.unwrap();

        assert_eq!(configured.len(), 1);
        assert_eq!(configured[0].account_id, account_id);
        assert_eq!(configured[0].provider_drive_id, "personal-drive-id");
    }
}
