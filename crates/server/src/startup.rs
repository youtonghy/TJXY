use std::{collections::HashSet, fmt, path::PathBuf, sync::Arc, time::Duration as StdDuration};

use chrono::Duration;
use sea_orm::{ConnectionTrait, Database, DbBackend, DbErr, Statement};
use thiserror::Error;
use tjxy_application::{
    AssetReadError, AssetReadService, AssetWriteError, AssetWriteService, AuthError, AuthService,
    CatalogQueryService, DirectMetadataReadService, DisplayPreferencesService, FilesystemBrowser,
    FilesystemBrowserError, LibraryService, MediaCollectionService, MediaReadService,
    MetadataImageFetchError, MetadataImportService, MetadataResolveService, PlaybackTicketService,
    PlaystateService, ProbeService, ReqwestMetadataImageFetcher, StorageBackendRegistry,
    SystemClock, TaskService, UserDataService,
};
use tjxy_cache::{CacheRuntime, CacheStartupError, RedisCacheConfig};
use tjxy_credentials::{CredentialCipher, CredentialCipherError};
use tjxy_db::{
    ApiKeyRepositoryError, CredentialRefreshState, LibraryRepository, LibraryRepositoryError,
    MetadataProviderSettingsRepository, MetadataProviderSettingsRepositoryError,
    StorageAccountRepository, StorageAccountRepositoryError, StorageCredentialRepository,
    StorageCredentialRepositoryError, SystemSettingsRepository, SystemSettingsRepositoryError,
};
use tjxy_metadata::{
    MetadataError, MetadataProvider, MusicBrainzProvider, ReloadableMetadataProvider,
    TheAudioDbProvider, TmdbProvider,
};
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

use crate::{AiAdmissionConfig, AppState, ServerIdentity, worker};

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
    assets_dir_source: &'static str,
    lazy_wait_timeout: StdDuration,
    filesystem_backends: Vec<(Uuid, PathBuf)>,
    filesystem_browser_roots: Option<Vec<PathBuf>>,
    filesystem_realtime_enabled: bool,
    storage_backends: Vec<ConfiguredStorageBackend>,
    credential_cipher: Option<Arc<CredentialCipher>>,
    google_oauth: Option<crate::storage_admin::GoogleDriveOAuthConfiguration>,
    onedrive_oauth: Option<crate::storage_admin::MicrosoftOneDriveOAuthConfiguration>,
    redis_cache: RedisCacheConfig,
    metadata_providers: Vec<Arc<dyn MetadataProvider>>,
    tmdb_provider: Arc<ReloadableMetadataProvider>,
    tmdb_environment_fallback: Option<crate::metadata_settings_admin::TmdbEnvironmentFallback>,
    tmdb_provider_factory: Arc<crate::metadata_settings_admin::TmdbProviderFactory>,
    the_audio_db_provider: Arc<ReloadableMetadataProvider>,
    the_audio_db_environment_fallback:
        Option<crate::metadata_settings_admin::MusicProviderEnvironmentFallback>,
    the_audio_db_provider_factory: Arc<crate::metadata_settings_admin::MusicProviderFactory>,
    musicbrainz_provider: Arc<ReloadableMetadataProvider>,
    musicbrainz_environment_fallback:
        Option<crate::metadata_settings_admin::MusicProviderEnvironmentFallback>,
    musicbrainz_provider_factory: Arc<crate::metadata_settings_admin::MusicProviderFactory>,
    media_refresh_interval: Option<StdDuration>,
    ai_admission: AiAdmissionConfig,
    logging_runtime: Option<Arc<crate::LoggingRuntime>>,
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
            .field(
                "database_backend",
                &database_backend_label(&self.database_url),
            )
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
                "filesystem_browser_root_count",
                &self.filesystem_browser_roots.as_ref().map_or(0, Vec::len),
            )
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
            .field("tmdb_provider", &"[RELOADABLE]")
            .field(
                "tmdb_environment_fallback_configured",
                &self.tmdb_environment_fallback.is_some(),
            )
            .field("tmdb_provider_factory", &"[CONFIGURED]")
            .field("the_audio_db_provider", &"[RELOADABLE]")
            .field("musicbrainz_provider", &"[RELOADABLE]")
            .field("media_refresh_interval", &self.media_refresh_interval)
            .field("ai_admission", &self.ai_admission)
            .finish_non_exhaustive()
    }
}

fn database_backend_label(database_url: &str) -> &'static str {
    match database_url.split(':').next() {
        Some("sqlite") => "sqlite",
        Some("postgres" | "postgresql") => "postgresql",
        Some("mysql") => "mysql",
        _ => "unknown",
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
            assets_dir_source: "Default",
            lazy_wait_timeout: StdDuration::from_millis(2_500),
            filesystem_backends: Vec::new(),
            filesystem_browser_roots: None,
            filesystem_realtime_enabled: true,
            storage_backends: Vec::new(),
            credential_cipher: None,
            google_oauth: None,
            onedrive_oauth: None,
            redis_cache: RedisCacheConfig::default(),
            metadata_providers: Vec::new(),
            tmdb_provider: Arc::new(ReloadableMetadataProvider::new("Tmdb")),
            tmdb_environment_fallback: None,
            tmdb_provider_factory: Arc::new(|access_token, language| {
                TmdbProvider::new(access_token.to_owned(), language.to_owned())
            }),
            the_audio_db_provider: Arc::new(ReloadableMetadataProvider::new("TheAudioDB")),
            the_audio_db_environment_fallback: None,
            the_audio_db_provider_factory: Arc::new(|api_key| {
                TheAudioDbProvider::new(api_key.to_owned())
                    .map(|provider| Arc::new(provider) as Arc<dyn MetadataProvider>)
            }),
            musicbrainz_provider: Arc::new(ReloadableMetadataProvider::new("MusicBrainz")),
            musicbrainz_environment_fallback: None,
            musicbrainz_provider_factory: Arc::new(|user_agent| {
                MusicBrainzProvider::new(user_agent.to_owned())
                    .map(|provider| Arc::new(provider) as Arc<dyn MetadataProvider>)
            }),
            media_refresh_interval: None,
            ai_admission: AiAdmissionConfig::default(),
            logging_runtime: None,
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
    pub const fn with_ai_admission_config(mut self, config: AiAdmissionConfig) -> Self {
        self.ai_admission = config;
        self
    }

    #[must_use]
    pub fn with_logging_runtime(mut self, runtime: Arc<crate::LoggingRuntime>) -> Self {
        self.logging_runtime = Some(runtime);
        self
    }

    #[must_use]
    pub fn with_assets_dir(mut self, assets_dir: impl Into<PathBuf>) -> Self {
        self.assets_dir = assets_dir.into();
        self.assets_dir_source = "Database";
        self
    }

    #[must_use]
    pub fn with_assets_dir_from_environment(mut self, assets_dir: impl Into<PathBuf>) -> Self {
        self.assets_dir = assets_dir.into();
        self.assets_dir_source = "Environment";
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
    pub fn with_filesystem_browser_roots<I, P>(mut self, roots: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: Into<PathBuf>,
    {
        self.filesystem_browser_roots = Some(roots.into_iter().map(Into::into).collect());
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

    /// Uses one shared reloadable TMDB provider for startup, metadata work, and admin updates.
    #[must_use]
    pub fn with_tmdb_provider(mut self, provider: Arc<ReloadableMetadataProvider>) -> Self {
        self.tmdb_provider = provider;
        self
    }

    /// Records the already-loaded environment fallback restored when database settings are absent.
    #[must_use]
    pub fn with_tmdb_environment_fallback(
        mut self,
        provider: Arc<TmdbProvider>,
        language: impl Into<String>,
    ) -> Self {
        self.tmdb_environment_fallback = Some(
            crate::metadata_settings_admin::TmdbEnvironmentFallback::new(provider, language.into()),
        );
        self
    }

    /// Overrides TMDB construction for alternate transports and deterministic integration tests.
    #[must_use]
    pub fn with_tmdb_provider_factory<Factory>(mut self, factory: Factory) -> Self
    where
        Factory: Fn(&str, &str) -> Result<TmdbProvider, MetadataError> + Send + Sync + 'static,
    {
        self.tmdb_provider_factory = Arc::new(factory);
        self
    }

    #[must_use]
    pub fn with_theaudiodb_provider(mut self, provider: Arc<ReloadableMetadataProvider>) -> Self {
        self.the_audio_db_provider = provider;
        self
    }

    #[must_use]
    pub fn with_theaudiodb_environment_fallback<Provider>(mut self, provider: Arc<Provider>) -> Self
    where
        Provider: MetadataProvider + 'static,
    {
        let provider: Arc<dyn MetadataProvider> = provider;
        self.the_audio_db_provider
            .replace(Some(Arc::clone(&provider)));
        self.the_audio_db_environment_fallback = Some(
            crate::metadata_settings_admin::MusicProviderEnvironmentFallback::new(provider, None),
        );
        self
    }

    #[must_use]
    pub fn with_theaudiodb_provider_factory<Factory>(mut self, factory: Factory) -> Self
    where
        Factory:
            Fn(&str) -> Result<Arc<dyn MetadataProvider>, MetadataError> + Send + Sync + 'static,
    {
        self.the_audio_db_provider_factory = Arc::new(factory);
        self
    }

    #[must_use]
    pub fn with_musicbrainz_provider(mut self, provider: Arc<ReloadableMetadataProvider>) -> Self {
        self.musicbrainz_provider = provider;
        self
    }

    #[must_use]
    pub fn with_musicbrainz_environment_fallback<Provider>(
        mut self,
        provider: Arc<Provider>,
        user_agent: impl Into<String>,
    ) -> Self
    where
        Provider: MetadataProvider + 'static,
    {
        let provider: Arc<dyn MetadataProvider> = provider;
        self.musicbrainz_provider
            .replace(Some(Arc::clone(&provider)));
        self.musicbrainz_environment_fallback = Some(
            crate::metadata_settings_admin::MusicProviderEnvironmentFallback::new(
                provider,
                Some(user_agent.into()),
            ),
        );
        self
    }

    #[must_use]
    pub fn with_musicbrainz_provider_factory<Factory>(mut self, factory: Factory) -> Self
    where
        Factory:
            Fn(&str) -> Result<Arc<dyn MetadataProvider>, MetadataError> + Send + Sync + 'static,
    {
        self.musicbrainz_provider_factory = Arc::new(factory);
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
pub async fn initialize(mut options: StartupOptions) -> Result<AppState, InitializationError> {
    let storage_admin_cipher = options.credential_cipher.clone();
    let metadata_settings_cipher = options.credential_cipher.clone();
    let ai_settings_cipher = options.credential_cipher.clone();
    let tmdb_provider = Arc::clone(&options.tmdb_provider);
    let tmdb_environment_fallback = options.tmdb_environment_fallback.clone();
    let tmdb_provider_factory = Arc::clone(&options.tmdb_provider_factory);
    let the_audio_db_provider = Arc::clone(&options.the_audio_db_provider);
    let the_audio_db_environment_fallback = options.the_audio_db_environment_fallback.clone();
    let the_audio_db_provider_factory = Arc::clone(&options.the_audio_db_provider_factory);
    let musicbrainz_provider = Arc::clone(&options.musicbrainz_provider);
    let musicbrainz_environment_fallback = options.musicbrainz_environment_fallback.clone();
    let musicbrainz_provider_factory = Arc::clone(&options.musicbrainz_provider_factory);
    validate_storage_backends(&options.storage_backends)?;
    let mut database = Database::connect(&options.database_url).await?;
    if database.get_database_backend() == DbBackend::Sqlite {
        database
            .execute(Statement::from_string(
                DbBackend::Sqlite,
                "PRAGMA foreign_keys = ON".to_owned(),
            ))
            .await?;
    }
    tjxy_db::migrate_database(&database).await?;
    if database.get_database_backend() == DbBackend::MySql {
        database.close().await?;
        database = Database::connect(&options.database_url).await?;
    }
    if let Some(runtime) = options.logging_runtime.as_ref() {
        let settings = tjxy_db::LoggingSettingsRepository::new(&database)
            .get()
            .await?;
        let mode = settings
            .as_ref()
            .map_or(tjxy_db::LogMode::Error, |value| value.mode());
        let retention_days = settings
            .as_ref()
            .map_or(tjxy_db::DEFAULT_LOG_RETENTION_DAYS, |value| {
                value.retention_days()
            });
        runtime.set_mode(mode)?;
        runtime.cleanup(retention_days)?;
        tokio::spawn(Arc::clone(runtime).run_retention_scheduler());
    }
    if options.assets_dir_source == "Default" {
        let roots = tjxy_db::AssetStorageRepository::new(&database)
            .roots()
            .await?;
        if let Some(persisted) = roots
            .iter()
            .find(|root| root.state() == "Pending")
            .or_else(|| roots.iter().find(|root| root.state() == "Current"))
        {
            options.assets_dir = PathBuf::from(persisted.canonical_path());
            options.assets_dir_source = "Database";
        }
    }
    let filesystem_browser_roots = match options.filesystem_browser_roots.clone() {
        Some(roots) => roots,
        None => SystemSettingsRepository::new(&database)
            .get()
            .await?
            .map_or_else(Vec::new, |settings| {
                settings
                    .media_browser_roots()
                    .iter()
                    .map(PathBuf::from)
                    .collect()
            }),
    };
    let (filesystem_browser, invalid_root_indexes) =
        FilesystemBrowser::from_available_roots(filesystem_browser_roots).await;
    if !invalid_root_indexes.is_empty() {
        tracing::error!(
            "Filesystem browser skipped unavailable root indexes: {}",
            invalid_root_indexes
                .iter()
                .map(|index| (index + 1).to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    let filesystem_browser = filesystem_browser.map(Arc::new);
    load_persisted_tmdb_settings(
        &database,
        metadata_settings_cipher.as_deref(),
        &tmdb_provider,
        tmdb_provider_factory.as_ref(),
    )
    .await
    .map_err(InitializationError::MetadataSettingsValidation)?;
    load_persisted_music_settings(
        &database,
        metadata_settings_cipher.as_deref(),
        crate::metadata_settings_admin::THEAUDIODB_PROVIDER_KEY,
        &the_audio_db_provider,
        the_audio_db_provider_factory.as_ref(),
    )
    .await
    .map_err(InitializationError::MetadataSettingsValidation)?;
    load_persisted_music_settings(
        &database,
        metadata_settings_cipher.as_deref(),
        crate::metadata_settings_admin::MUSICBRAINZ_PROVIDER_KEY,
        &musicbrainz_provider,
        musicbrainz_provider_factory.as_ref(),
    )
    .await
    .map_err(InitializationError::MetadataSettingsValidation)?;
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
    let libraries = Arc::new(LibraryService::new(database.clone()));
    let branding_assets_dir = options.assets_dir.join("branding");
    let asset_writer = Arc::new(if options.assets_dir_source == "Environment" {
        AssetWriteService::new_environment_override(database.clone(), options.assets_dir.clone())
            .await?
    } else {
        AssetWriteService::new(database.clone(), options.assets_dir.clone()).await?
    });
    let canonical_assets_dir = tokio::fs::canonicalize(&options.assets_dir)
        .await
        .map_err(AssetWriteError::Root)?;
    let image_fetcher = Arc::new(ReqwestMetadataImageFetcher::new()?);
    let assets =
        Arc::new(AssetReadService::new(database.clone(), options.assets_dir.clone()).await?);
    let (filesystem_backends, unavailable_filesystem_accounts) =
        prepare_filesystem_backends(filesystem_backends, options.filesystem_realtime_enabled).await;
    if !unavailable_filesystem_accounts.is_empty() {
        tracing::error!(
            "{} filesystem storage account(s) remain offline until their roots are restored and TJXY is restarted",
            unavailable_filesystem_accounts.len()
        );
    }
    let mut metadata_providers = options.metadata_providers;
    metadata_providers.insert(
        0,
        Arc::clone(&musicbrainz_provider) as Arc<dyn MetadataProvider>,
    );
    metadata_providers.insert(
        0,
        Arc::clone(&the_audio_db_provider) as Arc<dyn MetadataProvider>,
    );
    metadata_providers.insert(0, Arc::clone(&tmdb_provider) as Arc<dyn MetadataProvider>);
    let (media, direct_metadata, storage_runtime) = configure_storage(
        &database,
        filesystem_backends,
        storage_backends,
        metadata_providers,
        asset_writer,
        image_fetcher,
        options.filesystem_realtime_enabled,
    )?;
    let direct_metadata = Arc::new(direct_metadata);
    let mut catalog = CatalogQueryService::new(database.clone())
        .with_lazy_wait_timeout(options.lazy_wait_timeout)
        .with_direct_metadata(Arc::clone(&direct_metadata));
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
    let metadata_settings_admin = Arc::new(
        crate::metadata_settings_admin::MetadataSettingsAdminService::new(
            database.clone(),
            metadata_settings_cipher,
            tmdb_provider,
            tmdb_environment_fallback,
            tmdb_provider_factory,
            the_audio_db_provider,
            the_audio_db_environment_fallback,
            the_audio_db_provider_factory,
            musicbrainz_provider,
            musicbrainz_environment_fallback,
            musicbrainz_provider_factory,
        ),
    );
    let local_metadata_admin =
        Arc::new(crate::local_metadata_admin::LocalMetadataAdminService::new(
            database.clone(),
            canonical_assets_dir,
            options.assets_dir_source,
        ));
    let relink_admin = Arc::new(crate::relink_admin::RelinkAdminService::new(
        database.clone(),
    ));
    let media_collections = Arc::new(MediaCollectionService::new(database.clone()));
    let playback_tickets = Arc::new(PlaybackTicketService::new(database.clone(), SystemClock));
    let display_preferences = Arc::new(DisplayPreferencesService::new(database.clone()));
    let user_data = Arc::new(UserDataService::new(database.clone()));
    let warm_home_cache = cache.is_enabled();
    let mut state = AppState::new(
        options
            .identity
            .with_startup_wizard_completed(has_enabled_admin),
    )
    .with_auth(auth.clone())
    .with_announcements(database.clone())
    .with_ai_config(database.clone(), ai_settings_cipher, options.ai_admission)
    .with_catalog(catalog.clone())
    .with_libraries(libraries)
    .with_assets(assets)
    .with_direct_metadata(direct_metadata)
    .with_media(media)
    .with_playback_tickets(playback_tickets)
    .with_media_collections(media_collections)
    .with_display_preferences(display_preferences)
    .with_dashboard(database.clone())
    .with_client_portal(database.clone())
    .with_playstate(playstate)
    .with_tasks(tasks)
    .with_user_data(user_data)
    .with_storage_admin(storage_admin)
    .with_import_admin(import_admin)
    .with_metadata_import(metadata_import)
    .with_metadata_settings_admin(metadata_settings_admin)
    .with_local_metadata_admin(local_metadata_admin)
    .with_system_settings_assets(database.clone(), branding_assets_dir)
    .with_logging_runtime(options.logging_runtime)
    .with_relink_admin(relink_admin)
    .with_storage_runtime(storage_runtime)
    .with_realtime_events(realtime_events)
    .with_legacy_auth_enabled(options.legacy_auth_enabled)
    .with_ready(true);
    if let Some(browser) = filesystem_browser {
        state = state.with_filesystem_browser(browser);
    }
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

async fn load_persisted_tmdb_settings(
    database: &sea_orm::DatabaseConnection,
    cipher: Option<&CredentialCipher>,
    runtime: &ReloadableMetadataProvider,
    provider_factory: &crate::metadata_settings_admin::TmdbProviderFactory,
) -> Result<(), MetadataSettingsValidationError> {
    let stored = MetadataProviderSettingsRepository::new(database)
        .get(crate::metadata_settings_admin::TMDB_PROVIDER_KEY)
        .await
        .map_err(|error| metadata_settings_repository_error(&error))?;
    let Some(stored) = stored else {
        return Ok(());
    };
    let cipher = cipher.ok_or(MetadataSettingsValidationError::KeyringUnavailable)?;
    let plaintext = cipher
        .open(stored.credential_id(), stored.provider(), stored.envelope())
        .map_err(|error| metadata_settings_cipher_error(&error))?;
    let access_token = std::str::from_utf8(&plaintext)
        .map_err(|_| MetadataSettingsValidationError::StoredStateInvalid)?;
    let provider = provider_factory(access_token, stored.language())
        .map(Arc::new)
        .map_err(|_| MetadataSettingsValidationError::StoredStateInvalid)?;
    runtime.replace(stored.enabled().then_some(provider as Arc<_>));
    Ok(())
}

async fn load_persisted_music_settings(
    database: &sea_orm::DatabaseConnection,
    cipher: Option<&CredentialCipher>,
    provider_key: &str,
    runtime: &ReloadableMetadataProvider,
    provider_factory: &crate::metadata_settings_admin::MusicProviderFactory,
) -> Result<(), MetadataSettingsValidationError> {
    let stored = MetadataProviderSettingsRepository::new(database)
        .get(provider_key)
        .await
        .map_err(|error| metadata_settings_repository_error(&error))?;
    let Some(stored) = stored else {
        return Ok(());
    };
    let cipher = cipher.ok_or(MetadataSettingsValidationError::KeyringUnavailable)?;
    let plaintext = cipher
        .open(stored.credential_id(), stored.provider(), stored.envelope())
        .map_err(|error| metadata_settings_cipher_error(&error))?;
    let value = std::str::from_utf8(&plaintext)
        .map_err(|_| MetadataSettingsValidationError::StoredStateInvalid)?;
    let provider =
        provider_factory(value).map_err(|_| MetadataSettingsValidationError::StoredStateInvalid)?;
    runtime.replace(stored.enabled().then_some(provider));
    Ok(())
}

fn metadata_settings_repository_error(
    error: &MetadataProviderSettingsRepositoryError,
) -> MetadataSettingsValidationError {
    match error {
        MetadataProviderSettingsRepositoryError::InvalidStoredEnvelope => {
            MetadataSettingsValidationError::EnvelopeUnreadable
        }
        MetadataProviderSettingsRepositoryError::Database(_)
        | MetadataProviderSettingsRepositoryError::RollbackFailed { .. } => {
            MetadataSettingsValidationError::PersistenceUnavailable
        }
        MetadataProviderSettingsRepositoryError::InvalidProvider
        | MetadataProviderSettingsRepositoryError::InvalidLanguage
        | MetadataProviderSettingsRepositoryError::InvalidRevision
        | MetadataProviderSettingsRepositoryError::RevisionConflict
        | MetadataProviderSettingsRepositoryError::CredentialIdentityConflict => {
            MetadataSettingsValidationError::StoredStateInvalid
        }
    }
}

fn metadata_settings_cipher_error(
    error: &CredentialCipherError,
) -> MetadataSettingsValidationError {
    match error {
        CredentialCipherError::UnknownKeyVersion => {
            MetadataSettingsValidationError::KeyringUnavailable
        }
        CredentialCipherError::InvalidKeyVersion
        | CredentialCipherError::DuplicateKeyVersion
        | CredentialCipherError::InvalidEnvelope
        | CredentialCipherError::InvalidInput => {
            MetadataSettingsValidationError::StoredStateInvalid
        }
        CredentialCipherError::EncryptionFailed | CredentialCipherError::AuthenticationFailed => {
            MetadataSettingsValidationError::EnvelopeUnreadable
        }
    }
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
) -> (Vec<PreparedFilesystemBackend>, Vec<Uuid>) {
    let mut prepared = Vec::with_capacity(configured.len());
    let mut unavailable = Vec::new();
    for (account_id, root) in configured {
        match FilesystemBackend::new(&root).await {
            Ok(backend) => prepared.push(PreparedFilesystemBackend {
                account_id,
                backend: Arc::new(backend),
                realtime_enabled,
            }),
            Err(error) => {
                unavailable.push(account_id);
                tracing::error!(
                    "Filesystem storage account {account_id} is offline ({})",
                    backend_error_category(&error)
                );
            }
        }
    }
    (prepared, unavailable)
}

fn backend_error_category(error: &tjxy_storage::BackendError) -> &'static str {
    match error {
        tjxy_storage::BackendError::NotFound => "not found",
        tjxy_storage::BackendError::InvalidValue { .. } => "invalid root",
        tjxy_storage::BackendError::TemporarilyUnavailable { .. } => "temporarily unavailable",
        _ => "initialization failed",
    }
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
        DirectMetadataReadService,
        Arc<crate::runtime_storage::RuntimeStorageManager>,
    ),
    crate::runtime_storage::RuntimeStorageError,
> {
    let backends = StorageBackendRegistry::new();
    let media = MediaReadService::new(database.clone()).with_backend_registry(backends.clone());
    let direct_metadata =
        DirectMetadataReadService::new(database.clone()).with_backend_registry(backends.clone());
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
    Ok((media, direct_metadata, runtime))
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
    #[error("database schema is incompatible: {0}")]
    DatabaseSchema(#[from] tjxy_db::SchemaMigrationError),
    #[error("filesystem storage configuration query failed: {0}")]
    FilesystemConfiguration(#[from] LibraryRepositoryError),
    #[error("system settings query failed: {0}")]
    SystemSettings(#[from] SystemSettingsRepositoryError),
    #[error("logging settings query failed: {0}")]
    LoggingSettings(#[from] tjxy_db::LoggingSettingsRepositoryError),
    #[error("logging runtime update failed: {0}")]
    LoggingRuntime(#[from] crate::LoggingRuntimeError),
    #[error("authentication initialization failed: {0}")]
    Authentication(#[from] AuthError),
    #[error("API key validation failed")]
    ApiKeyValidation(#[source] ApiKeyValidationError),
    #[error("metadata provider settings validation failed")]
    MetadataSettingsValidation(#[source] MetadataSettingsValidationError),
    #[error("asset service initialization failed: {0}")]
    Asset(#[from] AssetReadError),
    #[error("asset writer initialization failed: {0}")]
    AssetWriter(#[from] AssetWriteError),
    #[error("asset storage configuration failed: {0}")]
    AssetStorage(#[from] tjxy_db::AssetStorageError),
    #[error("metadata image client initialization failed: {0}")]
    MetadataImage(#[from] MetadataImageFetchError),
    #[error("filesystem storage backend initialization failed: {0}")]
    StorageBackend(#[from] tjxy_storage::BackendError),
    #[error("filesystem browser configuration is invalid: {0}")]
    FilesystemBrowser(#[from] FilesystemBrowserError),
    #[error("cache initialization failed: {0}")]
    Cache(#[from] CacheStartupError),
    #[error("Google storage backend loading failed: {0}")]
    GoogleStorage(#[from] GoogleBackendLoadError),
    #[error("OneDrive storage backend loading failed: {0}")]
    OneDriveStorage(#[from] OneDriveBackendLoadError),
    #[error("runtime storage activation failed: {0}")]
    RuntimeStorage(#[from] crate::runtime_storage::RuntimeStorageError),
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum MetadataSettingsValidationError {
    #[error("metadata provider credential keyring is unavailable")]
    KeyringUnavailable,
    #[error("persisted metadata provider credential envelope is unreadable")]
    EnvelopeUnreadable,
    #[error("persisted metadata provider settings are invalid")]
    StoredStateInvalid,
    #[error("persisted metadata provider settings storage is unavailable")]
    PersistenceUnavailable,
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
    async fn filesystem_backends_keep_valid_roots_and_report_unavailable_accounts() {
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

        assert_eq!(result.0.len(), 1);
        assert_eq!(result.1.len(), 1);
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
