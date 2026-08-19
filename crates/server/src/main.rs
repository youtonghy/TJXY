use std::{env, fs, net::SocketAddr, path::PathBuf, process::Command, sync::Arc, time::Duration};

use thiserror::Error;
use tjxy_cache::{CacheConfigurationError, RedisCacheConfig, RedisMode};
use tjxy_metadata::{
    MusicBrainzProvider, ReloadableMetadataProvider, TheAudioDbProvider, TmdbProvider,
};
use tjxy_server::{
    AdminAssetsError, AiAdmissionConfig, AiAdmissionConfigError, BootstrapAdmin, DatabaseDraft,
    DatabaseTlsMode, GoogleDriveOAuthConfiguration, InitializationError, InstallationConfigError,
    InstallationConfigStore, InstallationState, LoggingRuntime,
    MicrosoftOneDriveOAuthConfiguration, SecretString, ServerIdentity, SetupCoordinator,
    SetupError, SetupValidator, StartupOptions, build_router_with_admin_and_jellyfin_web_dist,
    build_router_with_admin_dist, build_setup_router_with_admin_dist_assets_and_database,
    initialize, parse_credential_keyring,
};
use uuid::Uuid;
use zeroize::Zeroizing;

#[derive(Debug, Error)]
enum StartupError {
    #[error("logging initialization failed: {0}")]
    Logging(#[source] tjxy_server::LoggingRuntimeError),
    #[error("TJXY_SERVER_ID must contain a persistent UUID")]
    MissingServerId,
    #[error("TJXY_SERVER_ID is not a valid UUID: {0}")]
    InvalidServerId(#[source] uuid::Error),
    #[error("TJXY_BIND is not a valid socket address: {0}")]
    InvalidBindAddress(#[source] std::net::AddrParseError),
    #[error("TJXY_SETUP_BIND is not a valid socket address: {0}")]
    InvalidSetupBindAddress(#[source] std::net::AddrParseError),
    #[error("TJXY_SETUP_POSTGRES_PORT must be an integer from 1 through 65535")]
    InvalidSetupPostgresPort,
    #[error(
        "TJXY_SETUP_POSTGRES_HOST, TJXY_SETUP_POSTGRES_PORT, TJXY_SETUP_POSTGRES_DATABASE, TJXY_SETUP_POSTGRES_USERNAME, and TJXY_SETUP_POSTGRES_PASSWORD must be set together"
    )]
    IncompleteSetupPostgres,
    #[error("installation configuration is unavailable: {0}")]
    InstallationConfig(#[from] InstallationConfigError),
    #[error("setup validation failed: {0}")]
    Setup(#[from] SetupError),
    #[error("system settings could not be read: {0}")]
    SystemSettings(#[from] tjxy_db::SystemSettingsRepositoryError),
    #[error("the current TJXY executable could not be resolved: {0}")]
    CurrentExecutable(#[source] std::io::Error),
    #[error("TJXY could not restart itself: {0}")]
    Restart(#[source] std::io::Error),
    #[error("TJXY_BOOTSTRAP_ADMIN_USERNAME and TJXY_BOOTSTRAP_ADMIN_PASSWORD must be set together")]
    IncompleteBootstrapAdmin,
    #[error("TJXY_LEGACY_AUTH must be true or false")]
    InvalidLegacyAuth,
    #[error("TJXY_ENABLE_REMOTE_PROVIDERS must be true or false")]
    InvalidRemoteProviders,
    #[error("TMDb provider configuration is invalid")]
    InvalidTmdbConfiguration,
    #[error("music metadata provider configuration is invalid")]
    InvalidMusicMetadataConfiguration,
    #[error("TJXY_LAZY_WAIT_MS must be an integer from 0 through 30000")]
    InvalidLazyWait,
    #[error("TJXY_MEDIA_REFRESH_INTERVAL_SECONDS must be an integer from 0 through 2592000")]
    InvalidMediaRefreshInterval,
    #[error("TJXY_FILESYSTEM_ACCOUNT_ID is not a valid UUID: {0}")]
    InvalidFilesystemAccount(#[source] uuid::Error),
    #[error("TJXY_FILESYSTEM_ACCOUNT_ID and TJXY_FILESYSTEM_ROOT must be set together")]
    IncompleteFilesystemBackend,
    #[error("TJXY_FILESYSTEM_REALTIME must be true or false")]
    InvalidFilesystemRealtime,
    #[error("Redis cache configuration is invalid: {0}")]
    RedisConfiguration(#[from] CacheConfigurationError),
    #[error("{0} must be a positive integer")]
    InvalidRedisNumber(&'static str),
    #[error("TJXY_CREDENTIAL_KEYRING must contain an active version and Base64 32-byte keys")]
    InvalidCredentialKeyring,
    #[error(
        "TJXY_GOOGLE_OAUTH_CLIENT_ID, TJXY_GOOGLE_OAUTH_CLIENT_SECRET, and TJXY_GOOGLE_OAUTH_REDIRECT_URI must be set together"
    )]
    IncompleteGoogleOAuth,
    #[error("Google Drive OAuth configuration is invalid")]
    InvalidGoogleOAuth,
    #[error(
        "TJXY_ONEDRIVE_OAUTH_CLIENT_ID and TJXY_ONEDRIVE_OAUTH_REDIRECT_URI must be set together"
    )]
    IncompleteOneDriveOAuth,
    #[error("OneDrive OAuth configuration is invalid")]
    InvalidOneDriveOAuth,
    #[error("{0} must be an integer from 1 through {1}")]
    InvalidAiAdmissionNumber(&'static str, u64),
    #[error("AI admission configuration is invalid: {0}")]
    InvalidAiAdmissionConfiguration(#[source] AiAdmissionConfigError),
    #[error("service initialization failed: {0}")]
    Initialization(#[from] InitializationError),
    #[error("TJXY admin assets are invalid: {0}")]
    AdminAssets(#[from] AdminAssetsError),
    #[error("TJXY_JELLYFIN_WEB_DIST_DIR is not valid Unicode")]
    InvalidJellyfinWebDistPath,
    #[error("failed to bind or serve HTTP: {0}")]
    Io(#[from] std::io::Error),
}

#[tokio::main]
#[allow(clippy::too_many_lines)] // Environment-backed startup intentionally composes one service instance.
async fn main() -> Result<(), StartupError> {
    let log_directory =
        env::var_os("TJXY_LOG_DIR").map_or_else(|| PathBuf::from("./data/logs"), PathBuf::from);
    let (logging, _logging_guard) =
        LoggingRuntime::initialize(log_directory).map_err(StartupError::Logging)?;
    let logging = Arc::new(logging);
    let config_store = InstallationConfigStore::discover()?;
    let installation_state = config_store.load()?;
    if !matches!(installation_state, InstallationState::Completed(_)) {
        serve_setup(config_store.clone()).await?;
    }
    serve_application(config_store, logging).await
}

#[allow(clippy::too_many_lines)] // Environment-backed startup intentionally composes one service instance.
async fn serve_application(
    config_store: InstallationConfigStore,
    logging: Arc<LoggingRuntime>,
) -> Result<(), StartupError> {
    let installation_state = config_store.load()?;
    let completed = match &installation_state {
        InstallationState::Completed(completed) => Some(completed),
        InstallationState::Unconfigured | InstallationState::Pending(_) => None,
    };
    let server_id = env::var("TJXY_SERVER_ID")
        .ok()
        .or_else(|| completed.map(|value| value.server_id().to_string()))
        .ok_or(StartupError::MissingServerId)
        .and_then(|value| Uuid::parse_str(&value).map_err(StartupError::InvalidServerId))?;
    let server_name = env::var("TJXY_SERVER_NAME").unwrap_or_else(|_| "TJXY".to_owned());
    let mut identity = ServerIdentity::new(server_id, server_name, env::consts::OS);
    if let Some(local_address) = env::var("TJXY_PUBLIC_ADDRESS")
        .ok()
        .or_else(|| completed.and_then(|value| value.network().public_url().map(str::to_owned)))
    {
        identity = identity.with_local_address(local_address);
    }
    let database_url = match env::var("TJXY_DATABASE_URL") {
        Ok(value) => value,
        Err(_) => completed.map_or_else(
            || Ok("sqlite://tjxy.db?mode=rwc".to_owned()),
            |value| {
                value
                    .database()
                    .connection_url()
                    .map(|url| url.as_str().to_owned())
            },
        )?,
    };
    let tmdb = Arc::new(ReloadableMetadataProvider::new("Tmdb"));
    let the_audio_db = Arc::new(ReloadableMetadataProvider::new("TheAudioDB"));
    let musicbrainz = Arc::new(ReloadableMetadataProvider::new("MusicBrainz"));
    let mut startup = StartupOptions::new(database_url, identity)
        .with_logging_runtime(logging)
        .with_tmdb_provider(Arc::clone(&tmdb))
        .with_theaudiodb_provider(Arc::clone(&the_audio_db))
        .with_musicbrainz_provider(Arc::clone(&musicbrainz));
    startup = startup.with_ai_admission_config(ai_admission_config(env::var)?);
    let audio_db_key = env::var("TJXY_THEAUDIODB_API_KEY").unwrap_or_else(|_| "2".to_owned());
    let musicbrainz_user_agent = env::var("TJXY_MUSICBRAINZ_USER_AGENT").unwrap_or_else(|_| {
        format!(
            "TJXY/{} (https://github.com/youtonghy/TJXY)",
            env!("CARGO_PKG_VERSION")
        )
    });
    startup = startup
        .with_theaudiodb_environment_fallback(Arc::new(
            TheAudioDbProvider::new(audio_db_key)
                .map_err(|_| StartupError::InvalidMusicMetadataConfiguration)?,
        ))
        .with_musicbrainz_environment_fallback(
            Arc::new(
                MusicBrainzProvider::new(musicbrainz_user_agent.clone())
                    .map_err(|_| StartupError::InvalidMusicMetadataConfiguration)?,
            ),
            musicbrainz_user_agent,
        );
    let configured_keyring = env::var("TJXY_CREDENTIAL_KEYRING")
        .ok()
        .or_else(|| completed.map(|value| value.credential_keyring().to_owned()));
    if let Some(encoded) = configured_keyring {
        let encoded = Zeroizing::new(encoded);
        startup = startup.with_credential_cipher(Arc::new(
            parse_credential_keyring(&encoded)
                .map_err(|_| StartupError::InvalidCredentialKeyring)?,
        ));
    }
    if let Some(oauth) = google_oauth_configuration()? {
        startup = startup.with_google_oauth(oauth);
    }
    if let Some(oauth) = onedrive_oauth_configuration()? {
        startup = startup.with_onedrive_oauth(oauth);
    }
    let redis_mode = env::var("TJXY_REDIS_MODE")
        .unwrap_or_else(|_| "auto".to_owned())
        .parse::<RedisMode>()?;
    let redis_url =
        env::var("TJXY_REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_owned());
    let redis_prefix = env::var("TJXY_REDIS_KEY_PREFIX").unwrap_or_else(|_| "tjxy".to_owned());
    let redis_timeout = redis_number("TJXY_REDIS_CONNECT_TIMEOUT_MS", 200)?;
    let redis_home_ttl = redis_number("TJXY_REDIS_HOME_TTL_SECONDS", 300)?;
    let redis_item_ttl = redis_number("TJXY_REDIS_ITEM_TTL_SECONDS", 1_800)?;
    let redis_empty_ttl = redis_number("TJXY_REDIS_EMPTY_TTL_SECONDS", 3)?;
    let redis = RedisCacheConfig::new(
        redis_mode,
        redis_url,
        redis_prefix,
        Duration::from_millis(redis_timeout),
    )?
    .with_ttls(
        Duration::from_secs(redis_home_ttl),
        Duration::from_secs(redis_item_ttl),
        Duration::from_secs(redis_empty_ttl),
    )?;
    startup = startup.with_redis_cache(redis);
    if let Ok(assets_dir) = env::var("TJXY_ASSETS_DIR") {
        startup = startup.with_assets_dir_from_environment(assets_dir);
    }
    let remote_providers = env::var("TJXY_ENABLE_REMOTE_PROVIDERS")
        .map_or(Ok(false), |value| value.parse::<bool>())
        .map_err(|_| StartupError::InvalidRemoteProviders)?;
    if remote_providers && let Ok(token) = env::var("TJXY_TMDB_ACCESS_TOKEN") {
        let language = env::var("TJXY_TMDB_LANGUAGE").unwrap_or_else(|_| "zh-CN".to_owned());
        let provider = Arc::new(
            TmdbProvider::new(token, language.clone())
                .map_err(|_| StartupError::InvalidTmdbConfiguration)?,
        );
        tmdb.replace(Some(provider.clone()));
        startup = startup.with_tmdb_environment_fallback(provider, language);
    }
    if let Ok(value) = env::var("TJXY_LAZY_WAIT_MS") {
        let milliseconds = value
            .parse::<u64>()
            .ok()
            .filter(|value| *value <= 30_000)
            .ok_or(StartupError::InvalidLazyWait)?;
        startup = startup.with_lazy_wait_timeout(Duration::from_millis(milliseconds));
    }
    if let Some(interval) =
        media_refresh_interval(|| env::var("TJXY_MEDIA_REFRESH_INTERVAL_SECONDS"))?
    {
        startup = startup.with_media_refresh_interval(interval);
    }
    match (
        env::var("TJXY_FILESYSTEM_ACCOUNT_ID").ok(),
        env::var("TJXY_FILESYSTEM_ROOT").ok(),
    ) {
        (Some(account_id), Some(root)) => {
            let account_id =
                Uuid::parse_str(&account_id).map_err(StartupError::InvalidFilesystemAccount)?;
            startup = startup.with_filesystem_backend(account_id, root);
        }
        (None, None) => {}
        _ => return Err(StartupError::IncompleteFilesystemBackend),
    }
    let filesystem_realtime = env::var("TJXY_FILESYSTEM_REALTIME")
        .map_or(Ok(true), |value| value.parse::<bool>())
        .map_err(|_| StartupError::InvalidFilesystemRealtime)?;
    startup = startup.with_filesystem_realtime_enabled(filesystem_realtime);
    if let Some(value) = env::var_os("TJXY_MEDIA_BROWSER_ROOTS") {
        let roots = env::split_paths(&value).collect::<Vec<_>>();
        if !roots.is_empty() {
            startup = startup.with_filesystem_browser_roots(roots);
        }
    }
    if let Ok(value) = env::var("TJXY_LEGACY_AUTH") {
        let enabled = value
            .parse::<bool>()
            .map_err(|_| StartupError::InvalidLegacyAuth)?;
        startup = startup.with_legacy_auth_enabled(enabled);
    }
    match (
        env::var("TJXY_BOOTSTRAP_ADMIN_USERNAME").ok(),
        env::var("TJXY_BOOTSTRAP_ADMIN_PASSWORD").ok(),
    ) {
        (Some(username), Some(password)) => {
            startup = startup.with_bootstrap_admin(BootstrapAdmin::new(username, password));
        }
        (None, None) => {}
        _ => return Err(StartupError::IncompleteBootstrapAdmin),
    }
    let state = initialize(startup).await?;
    let bind_address = match env::var("TJXY_BIND") {
        Ok(value) => value
            .parse::<SocketAddr>()
            .map_err(StartupError::InvalidBindAddress)?,
        Err(_) => match completed {
            Some(completed) => completed.network().socket_address()?,
            None => state.persisted_bind_address().await?.unwrap_or_else(|| {
                "127.0.0.1:8096"
                    .parse()
                    .expect("default bind address is valid")
            }),
        },
    };
    let restart = state.restart_controller();
    let shutdown = restart.clone();
    let admin_dist = admin_dist_dir(|| env::var("TJXY_ADMIN_DIST_DIR"));
    let router = match env::var("TJXY_JELLYFIN_WEB_DIST_DIR") {
        Ok(jellyfin_web_dist) => build_router_with_admin_and_jellyfin_web_dist(
            state,
            admin_dist,
            PathBuf::from(jellyfin_web_dist),
        )?,
        Err(env::VarError::NotPresent) => build_router_with_admin_dist(state, admin_dist)?,
        Err(env::VarError::NotUnicode(_)) => {
            return Err(StartupError::InvalidJellyfinWebDistPath);
        }
    };
    let listener = tokio::net::TcpListener::bind(bind_address).await?;
    axum::serve(
        listener,
        router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(async move { shutdown.requested().await })
    .await?;
    if restart.is_requested() {
        let executable = env::current_exe().map_err(StartupError::CurrentExecutable)?;
        Command::new(executable)
            .args(env::args_os().skip(1))
            .spawn()
            .map_err(StartupError::Restart)?;
    }
    Ok(())
}

async fn serve_setup(config_store: InstallationConfigStore) -> Result<(), StartupError> {
    let data_dir =
        env::var("TJXY_SETUP_DATA_DIR").map_or_else(|_| PathBuf::from("./data"), PathBuf::from);
    fs::create_dir_all(&data_dir)?;
    let validator = SetupValidator::new(vec![data_dir.clone()])?;
    let coordinator = SetupCoordinator::new(config_store, validator.clone());
    let shutdown = coordinator.clone();
    let bind_address = env::var("TJXY_SETUP_BIND")
        .or_else(|_| env::var("TJXY_BIND"))
        .unwrap_or_else(|_| "127.0.0.1:8096".to_owned())
        .parse::<SocketAddr>()
        .map_err(StartupError::InvalidSetupBindAddress)?;
    let admin_dist = admin_dist_dir(|| env::var("TJXY_ADMIN_DIST_DIR"));
    let branding_asset_dir = env::var("TJXY_ASSETS_DIR")
        .map_or_else(|_| data_dir.join("assets"), PathBuf::from)
        .join("branding");
    let router = build_setup_router_with_admin_dist_assets_and_database(
        coordinator,
        validator,
        admin_dist,
        branding_asset_dir,
        managed_setup_database()?,
    )?;
    let listener = tokio::net::TcpListener::bind(bind_address).await?;
    axum::serve(
        listener,
        router.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async move { shutdown.wait_until_completed().await })
    .await?;
    Ok(())
}

fn managed_setup_database() -> Result<Option<DatabaseDraft>, StartupError> {
    let values = [
        env::var("TJXY_SETUP_POSTGRES_HOST").ok(),
        env::var("TJXY_SETUP_POSTGRES_PORT").ok(),
        env::var("TJXY_SETUP_POSTGRES_DATABASE").ok(),
        env::var("TJXY_SETUP_POSTGRES_USERNAME").ok(),
        env::var("TJXY_SETUP_POSTGRES_PASSWORD").ok(),
    ];
    if values.iter().all(Option::is_none) {
        return Ok(None);
    }
    let [
        Some(host),
        Some(port),
        Some(database),
        Some(username),
        Some(password),
    ] = values
    else {
        return Err(StartupError::IncompleteSetupPostgres);
    };
    let port = port
        .parse::<u16>()
        .ok()
        .filter(|port| *port > 0)
        .ok_or(StartupError::InvalidSetupPostgresPort)?;
    Ok(Some(DatabaseDraft::PostgreSql {
        host,
        port,
        database,
        username,
        password: SecretString::new(password),
        tls: DatabaseTlsMode::Disable,
    }))
}

fn google_oauth_configuration() -> Result<Option<GoogleDriveOAuthConfiguration>, StartupError> {
    match (
        env::var("TJXY_GOOGLE_OAUTH_CLIENT_ID").ok(),
        env::var("TJXY_GOOGLE_OAUTH_CLIENT_SECRET").ok(),
        env::var("TJXY_GOOGLE_OAUTH_REDIRECT_URI").ok(),
    ) {
        (Some(client_id), Some(client_secret), Some(redirect_uri)) => {
            GoogleDriveOAuthConfiguration::new(client_id, client_secret, redirect_uri)
                .map(Some)
                .map_err(|_| StartupError::InvalidGoogleOAuth)
        }
        (None, None, None) => Ok(None),
        _ => Err(StartupError::IncompleteGoogleOAuth),
    }
}

fn onedrive_oauth_configuration()
-> Result<Option<MicrosoftOneDriveOAuthConfiguration>, StartupError> {
    match (
        env::var("TJXY_ONEDRIVE_OAUTH_CLIENT_ID").ok(),
        env::var("TJXY_ONEDRIVE_OAUTH_REDIRECT_URI").ok(),
    ) {
        (Some(client_id), Some(redirect_uri)) => MicrosoftOneDriveOAuthConfiguration::new(
            client_id,
            env::var("TJXY_ONEDRIVE_OAUTH_CLIENT_SECRET").ok(),
            redirect_uri,
        )
        .map(Some)
        .map_err(|_| StartupError::InvalidOneDriveOAuth),
        (None, None) => Ok(None),
        _ => Err(StartupError::IncompleteOneDriveOAuth),
    }
}

fn admin_dist_dir(lookup: impl FnOnce() -> Result<String, env::VarError>) -> PathBuf {
    lookup().map_or_else(|_| PathBuf::from("admin/dist"), PathBuf::from)
}

fn ai_admission_config(
    mut lookup: impl FnMut(&'static str) -> Result<String, env::VarError>,
) -> Result<AiAdmissionConfig, StartupError> {
    let requests_per_minute =
        ai_admission_number(&mut lookup, "TJXY_AI_REQUESTS_PER_MINUTE", 10, 1_000)?;
    let max_user_concurrent_sse = ai_admission_number(
        &mut lookup,
        "TJXY_AI_MAX_CONCURRENT_STREAMS_PER_USER",
        2,
        100,
    )?;
    let max_global_concurrent_sse =
        ai_admission_number(&mut lookup, "TJXY_AI_MAX_CONCURRENT_STREAMS", 8, 1_000)?;
    let daily_quota = ai_admission_number(&mut lookup, "TJXY_AI_DAILY_QUOTA", 100, 100_000)?;
    AiAdmissionConfig::new(
        u32::try_from(requests_per_minute).map_err(|_| {
            StartupError::InvalidAiAdmissionNumber("TJXY_AI_REQUESTS_PER_MINUTE", 1_000)
        })?,
        usize::try_from(max_user_concurrent_sse).map_err(|_| {
            StartupError::InvalidAiAdmissionNumber("TJXY_AI_MAX_CONCURRENT_STREAMS_PER_USER", 100)
        })?,
        usize::try_from(max_global_concurrent_sse).map_err(|_| {
            StartupError::InvalidAiAdmissionNumber("TJXY_AI_MAX_CONCURRENT_STREAMS", 1_000)
        })?,
        u32::try_from(daily_quota)
            .map_err(|_| StartupError::InvalidAiAdmissionNumber("TJXY_AI_DAILY_QUOTA", 100_000))?,
    )
    .map_err(StartupError::InvalidAiAdmissionConfiguration)
}

fn ai_admission_number(
    lookup: &mut impl FnMut(&'static str) -> Result<String, env::VarError>,
    name: &'static str,
    default: u64,
    maximum: u64,
) -> Result<u64, StartupError> {
    match lookup(name) {
        Ok(value) => value
            .parse::<u64>()
            .ok()
            .filter(|value| (1..=maximum).contains(value))
            .ok_or(StartupError::InvalidAiAdmissionNumber(name, maximum)),
        Err(env::VarError::NotPresent) => Ok(default),
        Err(env::VarError::NotUnicode(_)) => {
            Err(StartupError::InvalidAiAdmissionNumber(name, maximum))
        }
    }
}

fn media_refresh_interval(
    lookup: impl FnOnce() -> Result<String, env::VarError>,
) -> Result<Option<Duration>, StartupError> {
    let seconds = match lookup() {
        Ok(value) => value
            .parse::<u64>()
            .ok()
            .filter(|value| *value <= 2_592_000)
            .ok_or(StartupError::InvalidMediaRefreshInterval)?,
        Err(env::VarError::NotPresent) => 900,
        Err(env::VarError::NotUnicode(_)) => {
            return Err(StartupError::InvalidMediaRefreshInterval);
        }
    };
    Ok((seconds != 0).then(|| Duration::from_secs(seconds)))
}

fn redis_number(name: &'static str, default: u64) -> Result<u64, StartupError> {
    match env::var(name) {
        Ok(value) => value
            .parse::<u64>()
            .ok()
            .filter(|value| *value > 0)
            .ok_or(StartupError::InvalidRedisNumber(name)),
        Err(_) => Ok(default),
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, env::VarError, path::PathBuf, time::Duration};

    use base64::{Engine as _, engine::general_purpose::STANDARD};

    use super::{
        admin_dist_dir, ai_admission_config, media_refresh_interval, parse_credential_keyring,
    };

    #[test]
    fn ai_admission_configuration_defaults_and_accepts_overrides() {
        let defaults = ai_admission_config(|_| Err(VarError::NotPresent)).unwrap();
        assert_eq!(defaults.requests_per_minute(), 10);
        assert_eq!(defaults.max_user_concurrent_sse(), 2);
        assert_eq!(defaults.max_global_concurrent_sse(), 8);
        assert_eq!(defaults.daily_quota(), 100);

        let values = HashMap::from([
            ("TJXY_AI_REQUESTS_PER_MINUTE", "12"),
            ("TJXY_AI_MAX_CONCURRENT_STREAMS_PER_USER", "3"),
            ("TJXY_AI_MAX_CONCURRENT_STREAMS", "9"),
            ("TJXY_AI_DAILY_QUOTA", "250"),
        ]);
        let configured = ai_admission_config(|name| {
            values
                .get(name)
                .map(|value| (*value).to_owned())
                .ok_or(VarError::NotPresent)
        })
        .unwrap();
        assert_eq!(configured.requests_per_minute(), 12);
        assert_eq!(configured.max_user_concurrent_sse(), 3);
        assert_eq!(configured.max_global_concurrent_sse(), 9);
        assert_eq!(configured.daily_quota(), 250);
    }

    #[test]
    fn ai_admission_configuration_rejects_invalid_environment_values() {
        for (name, value) in [
            ("TJXY_AI_REQUESTS_PER_MINUTE", "0"),
            ("TJXY_AI_REQUESTS_PER_MINUTE", "1001"),
            ("TJXY_AI_MAX_CONCURRENT_STREAMS_PER_USER", "101"),
            ("TJXY_AI_MAX_CONCURRENT_STREAMS", "1001"),
            ("TJXY_AI_DAILY_QUOTA", "100001"),
            ("TJXY_AI_DAILY_QUOTA", "invalid"),
        ] {
            assert!(
                ai_admission_config(|requested| {
                    if requested == name {
                        Ok(value.to_owned())
                    } else {
                        Err(VarError::NotPresent)
                    }
                })
                .is_err(),
                "{name}={value} must be rejected"
            );
        }
    }

    #[test]
    fn admin_distribution_path_defaults_and_accepts_an_override() {
        assert_eq!(
            admin_dist_dir(|| Err(VarError::NotPresent)),
            PathBuf::from("admin/dist")
        );
        assert_eq!(
            admin_dist_dir(|| Ok("/srv/tjxy/admin".to_owned())),
            PathBuf::from("/srv/tjxy/admin")
        );
    }

    #[test]
    fn media_refresh_interval_defaults_disables_and_rejects_invalid_values() {
        assert_eq!(
            media_refresh_interval(|| Err(VarError::NotPresent)).unwrap(),
            Some(Duration::from_secs(900))
        );
        assert_eq!(media_refresh_interval(|| Ok("0".to_owned())).unwrap(), None);
        assert!(media_refresh_interval(|| Ok("invalid".to_owned())).is_err());
        assert!(media_refresh_interval(|| Ok("2592001".to_owned())).is_err());
    }

    #[test]
    fn credential_keyring_parses_active_and_historical_base64_keys() {
        let first = STANDARD.encode([1_u8; 32]);
        let second = STANDARD.encode([2_u8; 32]);
        let encoded = format!(r#"{{"active_version":2,"keys":{{"1":"{first}","2":"{second}"}}}}"#);

        let cipher = parse_credential_keyring(&encoded).unwrap();

        assert!(format!("{cipher:?}").contains("active_version: 2"));
        assert!(format!("{cipher:?}").contains("key_count: 2"));
        assert!(!format!("{cipher:?}").contains(&second));
    }

    #[test]
    fn credential_keyring_rejects_missing_active_and_wrong_key_lengths() {
        let key = STANDARD.encode([1_u8; 31]);
        assert!(
            parse_credential_keyring(&format!(r#"{{"active_version":1,"keys":{{"1":"{key}"}}}}"#))
                .is_err()
        );
        let key = STANDARD.encode([1_u8; 32]);
        assert!(
            parse_credential_keyring(&format!(r#"{{"active_version":2,"keys":{{"1":"{key}"}}}}"#))
                .is_err()
        );
    }
}
