//! Axum composition root, system discovery, and Jellyfin-compatible authentication.

mod admin_assets;
mod ai;
mod ai_admission;
mod ai_provider;
mod ai_settings;
mod announcements;
mod api_key;
mod auth;
mod browse;
mod client_portal;
mod configuration;
mod dashboard_admin;
mod device;
mod display_preferences;
mod filesystem_admin;
mod image;
mod import_admin;
mod installation_config;
mod library;
mod media_collection;
mod metadata_admin;
mod metadata_settings_admin;
mod playback_ticket;
mod playstate;
mod relink_admin;
mod runtime_storage;
mod session;
mod setup;
mod socket;
mod source_admin;
mod startup;
mod storage_admin;
mod storage_admin_cursor;
mod stream;
mod subtitle;
mod system_settings;
mod task;
mod user_data;
mod worker;

use std::{
    net::{IpAddr, SocketAddr},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use axum::{
    Json, Router,
    extract::{ConnectInfo, RawQuery, Request, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
};
use tjxy_api::{BrandingConfiguration, EndpointInfo, PublicSystemInfo};
use tjxy_application::{
    AssetReadService, AuthService, CatalogQueryService, DisplayPreferencesService,
    FilesystemBrowser, LibraryService, MediaCollectionService, MediaReadService,
    MetadataImportService, PlaybackTicketService, PlaystateService, SystemClock, TaskService,
    UserDataService,
};
use uuid::Uuid;

pub use admin_assets::AdminAssetsError;
pub use ai_admission::{AiAdmissionConfig, AiAdmissionConfigError};
pub use ai_provider::{
    AiProviderSession, AiProviderTransport, AiProviderTransportError, ProviderDnsResolver,
    ProviderMethod, ProviderResponse, SafeReqwestTransport,
};
pub use configuration::{CredentialKeyringError, parse_credential_keyring};
pub use installation_config::{
    CompletedInstallation, DatabaseConfiguration, DatabaseTlsMode, InstallationConfigError,
    InstallationConfigStore, InstallationProfile, InstallationState, NetworkConfiguration,
    PendingInstallation, SecretString,
};
pub use runtime_storage::RuntimeStorageError;
pub use setup::{
    CompleteSetupInput, DatabaseBackend, DatabaseDraft, DatabaseTestResult, SetupCompletion,
    SetupCoordinator, SetupError, SetupErrorCode, SetupProgress, SetupProgressStage, SetupState,
    SetupStatus, SetupValidator, build_setup_router, build_setup_router_with_asset_dir,
};
pub use startup::{
    ApiKeyValidationError, BootstrapAdmin, InitializationError, MetadataSettingsValidationError,
    StartupOptions, initialize,
};
pub use storage_admin::{GoogleDriveOAuthConfiguration, MicrosoftOneDriveOAuthConfiguration};
pub use system_settings::RestartController;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServerIdentity {
    id: Uuid,
    local_address: Option<String>,
    server_name: String,
    operating_system: String,
    startup_wizard_completed: bool,
}

impl ServerIdentity {
    #[must_use]
    pub fn new(
        id: Uuid,
        server_name: impl Into<String>,
        operating_system: impl Into<String>,
    ) -> Self {
        Self {
            id,
            local_address: None,
            server_name: server_name.into(),
            operating_system: operating_system.into(),
            startup_wizard_completed: false,
        }
    }

    #[must_use]
    pub const fn with_startup_wizard_completed(mut self, completed: bool) -> Self {
        self.startup_wizard_completed = completed;
        self
    }

    #[must_use]
    pub fn with_local_address(mut self, local_address: impl Into<String>) -> Self {
        self.local_address = Some(local_address.into());
        self
    }

    fn public_info(&self) -> PublicSystemInfo {
        PublicSystemInfo {
            local_address: self.local_address.clone(),
            server_name: self.server_name.clone(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            product_name: "TJXY".to_owned(),
            operating_system: self.operating_system.clone(),
            id: self.id,
            startup_wizard_completed: self.startup_wizard_completed,
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    identity: Arc<ServerIdentity>,
    ready: Arc<AtomicBool>,
    auth: Option<Arc<AuthService<SystemClock>>>,
    ai: Option<Arc<ai::AiService>>,
    ai_encryption_available: bool,
    announcements: Option<Arc<announcements::AnnouncementService>>,
    catalog: Option<Arc<CatalogQueryService>>,
    display_preferences: Option<Arc<DisplayPreferencesService>>,
    dashboard_admin: Option<Arc<dashboard_admin::DashboardAdminService>>,
    client_portal: Option<Arc<client_portal::ClientPortalService>>,
    libraries: Option<Arc<LibraryService>>,
    filesystem_browser: Option<Arc<FilesystemBrowser>>,
    assets: Option<Arc<AssetReadService>>,
    media: Option<Arc<MediaReadService>>,
    playback_tickets: Option<Arc<PlaybackTicketService<SystemClock>>>,
    media_collections: Option<Arc<MediaCollectionService>>,
    playstate: Option<Arc<PlaystateService>>,
    tasks: Option<Arc<TaskService>>,
    user_data: Option<Arc<UserDataService>>,
    storage_admin: Option<Arc<storage_admin::StorageAdminService>>,
    import_admin: Option<Arc<import_admin::ImportAdminService>>,
    metadata_import: Option<Arc<MetadataImportService>>,
    metadata_settings_admin: Option<Arc<metadata_settings_admin::MetadataSettingsAdminService>>,
    system_settings: Option<Arc<system_settings::SystemSettingsService>>,
    restart: RestartController,
    relink_admin: Option<Arc<relink_admin::RelinkAdminService>>,
    storage_runtime: Option<Arc<runtime_storage::RuntimeStorageManager>>,
    realtime_events: Arc<socket::RealtimeEvents>,
    legacy_auth_enabled: bool,
}

impl AppState {
    #[must_use]
    pub fn new(identity: ServerIdentity) -> Self {
        Self {
            identity: Arc::new(identity),
            ready: Arc::new(AtomicBool::new(false)),
            auth: None,
            ai: None,
            ai_encryption_available: false,
            announcements: None,
            catalog: None,
            display_preferences: None,
            dashboard_admin: None,
            client_portal: None,
            libraries: None,
            filesystem_browser: None,
            assets: None,
            media: None,
            playback_tickets: None,
            media_collections: None,
            playstate: None,
            tasks: None,
            user_data: None,
            storage_admin: None,
            import_admin: None,
            metadata_import: None,
            metadata_settings_admin: None,
            system_settings: None,
            restart: RestartController::default(),
            relink_admin: None,
            storage_runtime: None,
            realtime_events: Arc::new(socket::RealtimeEvents::new()),
            legacy_auth_enabled: true,
        }
    }

    #[must_use]
    pub fn with_ready(self, ready: bool) -> Self {
        self.set_ready(ready);
        self
    }

    #[must_use]
    pub fn with_auth(mut self, auth: Arc<AuthService<SystemClock>>) -> Self {
        self.auth = Some(auth);
        self
    }

    #[must_use]
    pub fn with_announcements(mut self, database: sea_orm::DatabaseConnection) -> Self {
        self.announcements = Some(Arc::new(announcements::AnnouncementService::new(database)));
        self
    }

    #[must_use]
    pub fn with_ai(
        self,
        database: sea_orm::DatabaseConnection,
        cipher: Option<Arc<tjxy_credentials::CredentialCipher>>,
    ) -> Self {
        self.with_ai_config(database, cipher, AiAdmissionConfig::default())
    }

    #[must_use]
    pub fn with_ai_config(
        self,
        database: sea_orm::DatabaseConnection,
        cipher: Option<Arc<tjxy_credentials::CredentialCipher>>,
        admission_config: AiAdmissionConfig,
    ) -> Self {
        self.with_ai_transport_config(
            database,
            cipher,
            Arc::new(SafeReqwestTransport::new()),
            admission_config,
        )
    }

    #[must_use]
    pub fn with_ai_transport(
        self,
        database: sea_orm::DatabaseConnection,
        cipher: Option<Arc<tjxy_credentials::CredentialCipher>>,
        transport: Arc<dyn AiProviderTransport>,
    ) -> Self {
        self.with_ai_transport_config(database, cipher, transport, AiAdmissionConfig::default())
    }

    #[must_use]
    pub fn with_ai_transport_config(
        mut self,
        database: sea_orm::DatabaseConnection,
        cipher: Option<Arc<tjxy_credentials::CredentialCipher>>,
        transport: Arc<dyn AiProviderTransport>,
        admission_config: AiAdmissionConfig,
    ) -> Self {
        self.ai_encryption_available = cipher.is_some();
        self.ai = Some(Arc::new(ai::AiService::new_with_transport_config(
            database,
            cipher,
            transport,
            admission_config,
        )));
        self
    }

    #[must_use]
    pub fn with_catalog(mut self, catalog: Arc<CatalogQueryService>) -> Self {
        self.catalog = Some(catalog);
        self
    }

    #[must_use]
    pub fn with_display_preferences(
        mut self,
        display_preferences: Arc<DisplayPreferencesService>,
    ) -> Self {
        self.display_preferences = Some(display_preferences);
        self
    }

    #[must_use]
    pub fn with_dashboard(mut self, database: sea_orm::DatabaseConnection) -> Self {
        self.dashboard_admin = Some(Arc::new(dashboard_admin::DashboardAdminService::new(
            database,
        )));
        self
    }

    #[must_use]
    pub fn with_client_portal(mut self, database: sea_orm::DatabaseConnection) -> Self {
        self.client_portal = Some(Arc::new(client_portal::ClientPortalService::new(database)));
        self
    }

    #[must_use]
    pub fn with_libraries(mut self, libraries: Arc<LibraryService>) -> Self {
        self.libraries = Some(libraries);
        self
    }

    #[must_use]
    pub fn with_filesystem_browser(mut self, browser: Arc<FilesystemBrowser>) -> Self {
        self.filesystem_browser = Some(browser);
        self
    }

    #[must_use]
    pub fn with_assets(mut self, assets: Arc<AssetReadService>) -> Self {
        self.assets = Some(assets);
        self
    }

    #[must_use]
    pub fn with_media(mut self, media: Arc<MediaReadService>) -> Self {
        self.media = Some(media);
        self
    }

    #[must_use]
    pub fn with_playback_tickets(
        mut self,
        playback_tickets: Arc<PlaybackTicketService<SystemClock>>,
    ) -> Self {
        self.playback_tickets = Some(playback_tickets);
        self
    }

    #[must_use]
    pub fn with_media_collections(mut self, service: Arc<MediaCollectionService>) -> Self {
        self.media_collections = Some(service);
        self
    }

    #[must_use]
    pub fn with_playstate(mut self, playstate: Arc<PlaystateService>) -> Self {
        self.playstate = Some(playstate);
        self
    }

    #[must_use]
    pub fn with_tasks(mut self, tasks: Arc<TaskService>) -> Self {
        self.tasks = Some(tasks);
        self
    }

    #[must_use]
    pub fn with_user_data(mut self, user_data: Arc<UserDataService>) -> Self {
        self.user_data = Some(user_data);
        self
    }

    fn with_relink_admin(mut self, service: Arc<relink_admin::RelinkAdminService>) -> Self {
        self.relink_admin = Some(service);
        self
    }

    #[must_use]
    fn with_storage_runtime(
        mut self,
        storage_runtime: Arc<runtime_storage::RuntimeStorageManager>,
    ) -> Self {
        self.storage_runtime = Some(storage_runtime);
        self
    }

    #[must_use]
    fn with_storage_admin(
        mut self,
        storage_admin: Option<Arc<storage_admin::StorageAdminService>>,
    ) -> Self {
        self.storage_admin = storage_admin;
        self
    }

    #[must_use]
    fn with_import_admin(
        mut self,
        import_admin: Option<Arc<import_admin::ImportAdminService>>,
    ) -> Self {
        self.import_admin = import_admin;
        self
    }

    #[must_use]
    fn with_metadata_import(mut self, metadata_import: Arc<MetadataImportService>) -> Self {
        self.metadata_import = Some(metadata_import);
        self
    }

    #[must_use]
    fn with_metadata_settings_admin(
        mut self,
        service: Arc<metadata_settings_admin::MetadataSettingsAdminService>,
    ) -> Self {
        self.metadata_settings_admin = Some(service);
        self
    }

    #[must_use]
    pub fn with_system_settings(mut self, database: sea_orm::DatabaseConnection) -> Self {
        self.system_settings = Some(Arc::new(system_settings::SystemSettingsService::new(
            database,
            std::path::PathBuf::from("./data/assets/branding"),
        )));
        self
    }

    #[must_use]
    pub(crate) fn with_system_settings_assets(
        mut self,
        database: sea_orm::DatabaseConnection,
        asset_dir: std::path::PathBuf,
    ) -> Self {
        self.system_settings = Some(Arc::new(system_settings::SystemSettingsService::new(
            database, asset_dir,
        )));
        self
    }

    #[must_use]
    pub fn restart_controller(&self) -> RestartController {
        self.restart.clone()
    }

    /// Reads the persisted listen address, if system settings are configured.
    ///
    /// # Errors
    ///
    /// Returns a repository error when persisted settings cannot be read or parsed.
    pub async fn persisted_bind_address(
        &self,
    ) -> Result<Option<SocketAddr>, tjxy_db::SystemSettingsRepositoryError> {
        let Some(service) = self.system_settings.as_ref() else {
            return Ok(None);
        };
        system_settings::persisted_bind_address(service).await
    }

    #[must_use]
    pub const fn with_legacy_auth_enabled(mut self, enabled: bool) -> Self {
        self.legacy_auth_enabled = enabled;
        self
    }

    pub fn set_ready(&self, ready: bool) {
        self.ready.store(ready, Ordering::Release);
    }

    #[must_use]
    pub(crate) fn with_realtime_events(mut self, events: Arc<socket::RealtimeEvents>) -> Self {
        self.realtime_events = events;
        self
    }

    pub(crate) fn realtime_events(&self) -> &Arc<socket::RealtimeEvents> {
        &self.realtime_events
    }
}

#[allow(clippy::too_many_lines)] // Keeps all public route bindings visible in the composition root.
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/System/Info/Public", get(public_system_info))
        .route("/System/Endpoint", get(endpoint_info))
        .route("/System/Ping", get(system_ping))
        .route(
            "/System/Language",
            get(system_settings::get_public_language).put(system_settings::put_setup),
        )
        .route(
            "/System/Settings",
            get(system_settings::get_public_settings),
        )
        .route("/Branding/Configuration", get(branding_configuration))
        .route("/Branding/Assets/{file}", get(system_settings::get_asset))
        .route(
            "/Users/AuthenticateByName",
            post(auth::authenticate_by_name),
        )
        .route("/Users/Me", get(auth::current_user))
        .route("/Ai/Models", get(ai_settings::models))
        .route("/Ai/Conversations", get(ai::list_conversations))
        .route(
            "/Ai/Conversations/{conversation_id}",
            get(ai::get_conversation).delete(ai::delete_conversation),
        )
        .route("/Ai/Chat", post(ai::chat))
        .route("/Announcements", get(announcements::client_list))
        .route("/Announcements/NextPopup", get(announcements::next_popup))
        .route(
            "/Announcements/{id}/Acknowledge",
            post(announcements::acknowledge),
        )
        .route("/Users/Me/Insights", get(client_portal::insights))
        .route(
            "/Users/Me/Profile",
            get(auth::current_user_profile).patch(auth::update_current_user_profile),
        )
        .route(
            "/Users/Me/Password",
            post(auth::update_current_user_password),
        )
        .route(
            "/Auth/Keys",
            get(api_key::list)
                .post(api_key::create)
                .fallback(api_key::method_not_allowed),
        )
        .route(
            "/Auth/Keys/{key}",
            delete(api_key::delete).fallback(api_key::method_not_allowed),
        )
        .route("/Users", get(auth::users).post(auth::update_user))
        .route("/Users/New", post(auth::create_user))
        .route(
            "/Users/{user_id}",
            get(auth::user).delete(auth::delete_user),
        )
        .route(
            "/Users/{user_id}/Password",
            post(auth::update_user_password),
        )
        .route("/Users/{user_id}/Policy", post(auth::update_user_policy))
        .route("/UserViews", get(browse::user_views))
        .route("/Discover/Popular", get(client_portal::popular))
        .route("/Discover/Tmdb/Popular", get(client_portal::tmdb_top))
        .route("/Discover/Server/Top", get(client_portal::server_top))
        .route("/Search/Hints", get(browse::search_hints))
        .route(
            "/DisplayPreferences/{display_preferences_id}",
            get(display_preferences::get).post(display_preferences::post),
        )
        .route("/socket", get(socket::connect))
        .merge(media_collection_routes())
        .route("/Items/Filters", get(browse::item_filters))
        .route("/Items/Latest", get(browse::latest_items))
        .route("/UserItems/Resume", get(browse::resume_items))
        .route("/Shows/NextUp", get(browse::next_up_items))
        .route("/Items", get(browse::items))
        .route("/Items/{item_id}/Similar", get(browse::similar_items))
        .route("/Items/{item_id}", get(browse::item_detail))
        .route(
            "/Items/{item_id}/PlaybackInfo",
            get(browse::playback_info_get).post(browse::playback_info_post),
        )
        .route(
            "/Items/{item_id}/PlaybackTicket",
            post(playback_ticket::issue),
        )
        .route(
            "/PlaybackTickets/{ticket_id}",
            delete(playback_ticket::revoke),
        )
        .route(
            "/UserItems/{item_id}/UserData",
            get(user_data::get).post(user_data::post),
        )
        .route(
            "/Users/{user_id}/FavoriteItems/{item_id}",
            post(user_data::favorite).delete(user_data::unfavorite),
        )
        .route(
            "/Users/{user_id}/PlayedItems/{item_id}",
            post(user_data::played).delete(user_data::unplayed),
        )
        .route(
            "/Videos/{item_id}/stream",
            get(stream::get).head(stream::head),
        )
        .route(
            "/Audio/{item_id}/stream",
            get(stream::get).head(stream::head),
        )
        .route(
            "/Videos/{item_id}/{media_source_id}/Subtitles/{index}/{stream}",
            get(subtitle::get),
        )
        .route(
            "/Videos/{item_id}/{media_source_id}/Subtitles/{index}/{start_position_ticks}/{stream}",
            get(subtitle::get_with_offset),
        )
        .route(
            "/Items/{item_id}/Images/{image_type}",
            get(image::get_original).head(image::head_original),
        )
        .route(
            "/Sessions/Capabilities/Full",
            post(browse::full_capabilities),
        )
        .route("/Sessions", get(session::list))
        .route("/Admin/Dashboard/Summary", get(dashboard_admin::summary))
        .route(
            "/Admin/Dashboard/NowPlaying",
            get(dashboard_admin::now_playing),
        )
        .route(
            "/Admin/Dashboard/LoginHistory",
            get(dashboard_admin::login_history),
        )
        .route(
            "/Admin/Dashboard/WatchHistory",
            get(dashboard_admin::watch_history),
        )
        .route("/Sessions/Logout", post(session::logout))
        .route("/Devices", get(device::list).delete(device::delete))
        .route("/Devices/Info", get(device::info))
        .route(
            "/Devices/Options",
            get(device::options).post(device::update_options),
        )
        .route("/Sessions/Playing", post(playstate::started))
        .route("/Sessions/Playing/Progress", post(playstate::progress))
        .route("/Sessions/Playing/Stopped", post(playstate::stopped))
        .route("/Sessions/Playing/Ping", post(playstate::ping))
        .route("/Library/Refresh", post(task::refresh_library))
        .merge(admin_task_routes())
        .merge(admin_filesystem_routes())
        .merge(admin_storage_routes())
        .merge(admin_source_routes())
        .route("/Admin/Imports/Emby", post(import_admin::create_emby))
        .route("/Admin/Imports/{job_id}", get(import_admin::status))
        .route("/Admin/Imports/{job_id}/Pause", post(import_admin::pause))
        .route("/Admin/Imports/{job_id}/Resume", post(import_admin::resume))
        .route(
            "/Admin/Items/{item_id}/Metadata/Nfo",
            post(metadata_admin::import_nfo),
        )
        .route(
            "/Admin/Metadata/Providers/Tmdb",
            get(metadata_settings_admin::get)
                .put(metadata_settings_admin::put)
                .delete(metadata_settings_admin::delete),
        )
        .route(
            "/Admin/Metadata/Providers/Tmdb/Test",
            post(metadata_settings_admin::test),
        )
        .route(
            "/Admin/Metadata/Providers/TheAudioDB",
            get(metadata_settings_admin::get_the_audio_db)
                .put(metadata_settings_admin::put_the_audio_db)
                .delete(metadata_settings_admin::delete_the_audio_db),
        )
        .route(
            "/Admin/Metadata/Providers/TheAudioDB/Test",
            post(metadata_settings_admin::test_the_audio_db),
        )
        .route(
            "/Admin/Metadata/Providers/MusicBrainz",
            get(metadata_settings_admin::get_musicbrainz)
                .put(metadata_settings_admin::put_musicbrainz)
                .delete(metadata_settings_admin::delete_musicbrainz),
        )
        .route(
            "/Admin/Metadata/Providers/MusicBrainz/Test",
            post(metadata_settings_admin::test_musicbrainz),
        )
        .route(
            "/Admin/Ai/Settings",
            get(ai_settings::get)
                .put(ai_settings::put)
                .delete(ai_settings::delete),
        )
        .route("/Admin/Ai/Settings/Test", post(ai_settings::test))
        .route("/Admin/Ai/Analytics", get(ai_settings::analytics))
        .route(
            "/Admin/Announcements",
            get(announcements::admin_list).post(announcements::admin_create),
        )
        .route(
            "/Admin/Announcements/{id}",
            put(announcements::admin_update).delete(announcements::admin_delete),
        )
        .route(
            "/Admin/Announcements/{id}/Publish",
            post(announcements::admin_publish),
        )
        .route(
            "/Admin/Announcements/{id}/Archive",
            post(announcements::admin_archive),
        )
        .route(
            "/Admin/Ai/Settings/Models",
            post(ai_settings::discover_models),
        )
        .route(
            "/Admin/System/Language",
            get(system_settings::get_admin).put(system_settings::put_admin_language),
        )
        .route(
            "/Admin/System/Settings",
            get(system_settings::get_admin).put(system_settings::put_admin),
        )
        .route(
            "/Admin/System/Branding/{kind}",
            put(system_settings::upload_asset),
        )
        .route("/Admin/System/Restart", post(system_settings::restart))
        .route(
            "/Admin/Imports/{job_id}/Publish",
            post(import_admin::publish),
        )
        .merge(library_routes())
        .route("/ScheduledTasks", get(task::scheduled_tasks))
        .route("/ScheduledTasks/{task_id}", get(task::scheduled_task))
        .route(
            "/ScheduledTasks/Running/{task_id}",
            post(task::start_scheduled_task).delete(task::cancel_scheduled_task),
        )
        .route("/Sessions/Capabilities", post(browse::legacy_capabilities))
        .route("/health/live", get(liveness))
        .route("/health/ready", get(readiness))
        .with_state(state)
}

fn playlist_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/Playlists",
            get(media_collection::playlists).post(media_collection::create_playlist),
        )
        .route(
            "/Playlists/{playlist_id}",
            put(media_collection::rename_playlist).delete(media_collection::delete_playlist),
        )
        .route(
            "/Playlists/{playlist_id}/Items",
            get(media_collection::playlist_items).post(media_collection::append_playlist_items),
        )
        .route(
            "/Playlists/{playlist_id}/Items/{entry_id}",
            delete(media_collection::delete_playlist_item),
        )
        .route(
            "/Playlists/{playlist_id}/Items/{entry_id}/Move/{new_index}",
            post(media_collection::move_playlist_item),
        )
}

fn media_collection_routes() -> Router<AppState> {
    playlist_routes().merge(shared_collection_routes())
}

fn shared_collection_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/Admin/Collections",
            post(media_collection::create_shared_collection),
        )
        .route(
            "/Admin/Collections/{collection_id}/Items",
            post(media_collection::append_shared_collection_items),
        )
        .route(
            "/Admin/Collections/{collection_id}",
            put(media_collection::rename_shared_collection)
                .delete(media_collection::delete_shared_collection),
        )
        .route("/Collections", get(media_collection::shared_collections))
        .route(
            "/Collections/{collection_id}/Items",
            get(media_collection::shared_collection_items),
        )
}

/// Builds the API router with the production administrator application mounted at `/admin`.
///
/// # Errors
///
/// Returns [`AdminAssetsError`] when the distribution directory is missing, its
/// `index.html` is not a regular file, or the entry document cannot be read.
pub fn build_router_with_admin_dist(
    state: AppState,
    dist_dir: impl AsRef<std::path::Path>,
) -> Result<Router, AdminAssetsError> {
    Ok(build_router(state).merge(admin_assets::router(dist_dir.as_ref())?))
}

/// Builds the database-independent first-run router and setup-only static application.
///
/// # Errors
///
/// Returns [`AdminAssetsError`] when the distribution entry document is unavailable.
pub fn build_setup_router_with_admin_dist(
    coordinator: SetupCoordinator,
    validator: SetupValidator,
    dist_dir: impl AsRef<std::path::Path>,
) -> Result<Router, AdminAssetsError> {
    Ok(build_setup_router(coordinator, validator)
        .merge(admin_assets::setup_router(dist_dir.as_ref())?))
}

/// Builds the first-run router with explicit durable branding storage.
///
/// # Errors
///
/// Returns [`AdminAssetsError`] when the distribution entry document is unavailable.
pub fn build_setup_router_with_admin_dist_and_assets(
    coordinator: SetupCoordinator,
    validator: SetupValidator,
    dist_dir: impl AsRef<std::path::Path>,
    branding_asset_dir: impl Into<std::path::PathBuf>,
) -> Result<Router, AdminAssetsError> {
    Ok(
        build_setup_router_with_asset_dir(coordinator, validator, branding_asset_dir)
            .merge(admin_assets::setup_router(dist_dir.as_ref())?),
    )
}

fn library_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/Library/VirtualFolders",
            get(library::virtual_folders)
                .post(library::add_virtual_folder)
                .delete(library::delete_virtual_folder),
        )
        .route(
            "/Library/VirtualFolders/LibraryOptions",
            post(library::update_library_options),
        )
        .route(
            "/Library/VirtualFolders/Name",
            post(library::rename_virtual_folder),
        )
        .route(
            "/Library/VirtualFolders/Paths",
            post(library::attach_virtual_folder_path).delete(library::detach_virtual_folder_path),
        )
}

fn admin_task_routes() -> Router<AppState> {
    Router::new()
        .route("/Admin/Tasks/Jobs", get(task::recent_jobs))
        .route(
            "/Admin/Tasks/ValidateStorage/{id}",
            post(task::validate_storage),
        )
        .route(
            "/Admin/Tasks/DiscoverTitles/{id}",
            post(task::discover_titles),
        )
        .route(
            "/Admin/Tasks/FullScan/{library_id}/{root_id}",
            post(task::full_scan_root),
        )
        .route(
            "/Admin/Tasks/ResolveMetadata/{id}",
            post(task::resolve_metadata),
        )
        .route("/Admin/Tasks/ExpandItem/{id}", post(task::expand_item))
        .route(
            "/Admin/Tasks/IndexMediaSources/{id}",
            post(task::index_media_sources),
        )
        .route("/Admin/Tasks/ProbeMedia/{id}", post(task::probe_media))
}

fn admin_filesystem_routes() -> Router<AppState> {
    Router::new()
        .route("/Admin/Filesystem/Roots", get(filesystem_admin::roots))
        .route(
            "/Admin/Filesystem/Directories",
            get(filesystem_admin::directories),
        )
}

fn admin_source_routes() -> Router<AppState> {
    Router::new().route(
        "/Admin/Items/{item_id}/MediaSources/{media_source_id}/PlaybackPolicy",
        put(source_admin::update),
    )
}

fn admin_storage_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/Admin/Storage/OAuth/GoogleDrive/Start",
            post(storage_admin::start_google_drive_oauth),
        )
        .route(
            "/Admin/Storage/OAuth/GoogleDrive/Callback",
            get(storage_admin::google_drive_oauth_callback),
        )
        .route(
            "/Admin/Storage/OAuth/GoogleDrive/{state}/Directories",
            get(storage_admin::google_drive_directories),
        )
        .route(
            "/Admin/Storage/OAuth/GoogleDrive/{state}/SharedDrives",
            get(storage_admin::google_shared_drives),
        )
        .route(
            "/Admin/Storage/OAuth/GoogleDrive/{state}/Bind",
            post(storage_admin::bind_google_drive),
        )
        .route(
            "/Admin/Storage/OAuth/OneDrive/Start",
            post(storage_admin::start_onedrive_oauth),
        )
        .route(
            "/Admin/Storage/OAuth/OneDrive/Callback",
            get(storage_admin::onedrive_oauth_callback),
        )
        .route(
            "/Admin/Storage/OAuth/OneDrive/{state}/Directories",
            get(storage_admin::onedrive_directories),
        )
        .route(
            "/Admin/Storage/OAuth/OneDrive/{state}/Bind",
            post(storage_admin::bind_onedrive),
        )
        .route(
            "/Admin/Storage/RelinkCandidates",
            get(relink_admin::pending),
        )
        .route(
            "/Admin/Storage/RelinkCandidates/{id}/Confirm",
            post(relink_admin::confirm),
        )
        .route(
            "/Admin/Storage/RelinkCandidates/{id}/Reject",
            post(relink_admin::reject),
        )
}

async fn public_system_info(State(state): State<AppState>) -> Json<PublicSystemInfo> {
    let mut info = state.identity.public_info();
    if let Some(service) = state.system_settings.as_ref()
        && let Ok(Some(settings)) = service.get().await
    {
        settings.site_title().clone_into(&mut info.server_name);
        if let Some(public_url) = settings.public_url() {
            info.local_address = Some(public_url.to_owned());
        }
    }
    Json(info)
}

async fn endpoint_info(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
    request: Request,
) -> Response {
    if let Err(response) =
        auth::authenticated_principal(&state, &headers, raw_query.as_deref()).await
    {
        return response;
    }
    if !endpoint_auth_only_query(raw_query.as_deref()) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let peer = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|connect_info| connect_info.0.ip());
    Json(endpoint_info_for(peer)).into_response()
}

fn endpoint_info_for(peer: Option<IpAddr>) -> EndpointInfo {
    EndpointInfo {
        is_local: peer.is_some_and(|address| address.is_loopback()),
        is_in_network: peer.is_some_and(is_in_network),
    }
}

fn is_in_network(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => {
            address.is_loopback() || address.is_private() || address.is_link_local()
        }
        IpAddr::V6(address) => {
            address.is_loopback() || address.is_unique_local() || address.is_unicast_link_local()
        }
    }
}

fn endpoint_auth_only_query(raw_query: Option<&str>) -> bool {
    let Ok(mut query) = auth::request_query(raw_query) else {
        return false;
    };
    query.remove("ApiKey");
    query.remove("api_key");
    query.is_empty()
}

async fn system_ping() -> &'static str {
    "TJXY Server"
}

async fn branding_configuration() -> Json<BrandingConfiguration> {
    Json(BrandingConfiguration::default())
}

async fn liveness() -> &'static str {
    "live"
}

async fn readiness(State(state): State<AppState>) -> impl IntoResponse {
    let dependencies_ready = match state.auth.as_ref() {
        Some(auth) => auth.check_health().await.is_ok(),
        None => true,
    };
    if state.ready.load(Ordering::Acquire) && dependencies_ready {
        (StatusCode::OK, "ready")
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, "not ready")
    }
}
