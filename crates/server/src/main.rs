use std::{collections::BTreeMap, env, net::SocketAddr, path::PathBuf, sync::Arc, time::Duration};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::Deserialize;
use thiserror::Error;
use tjxy_cache::{CacheConfigurationError, RedisCacheConfig, RedisMode};
use tjxy_credentials::{CredentialCipher, CredentialKey};
use tjxy_metadata::TmdbProvider;
use tjxy_server::{
    AdminAssetsError, BootstrapAdmin, GoogleDriveOAuthConfiguration, InitializationError,
    MicrosoftOneDriveOAuthConfiguration, ServerIdentity, StartupOptions,
    build_router_with_admin_dist, initialize,
};
use uuid::Uuid;
use zeroize::Zeroizing;

#[derive(Debug, Error)]
enum StartupError {
    #[error("TJXY_SERVER_ID must contain a persistent UUID")]
    MissingServerId,
    #[error("TJXY_SERVER_ID is not a valid UUID: {0}")]
    InvalidServerId(#[source] uuid::Error),
    #[error("TJXY_BIND is not a valid socket address: {0}")]
    InvalidBindAddress(#[source] std::net::AddrParseError),
    #[error("TJXY_BOOTSTRAP_ADMIN_USERNAME and TJXY_BOOTSTRAP_ADMIN_PASSWORD must be set together")]
    IncompleteBootstrapAdmin,
    #[error("TJXY_LEGACY_AUTH must be true or false")]
    InvalidLegacyAuth,
    #[error("TJXY_ENABLE_REMOTE_PROVIDERS must be true or false")]
    InvalidRemoteProviders,
    #[error("TMDb provider configuration is invalid")]
    InvalidTmdbConfiguration,
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
    #[error("service initialization failed: {0}")]
    Initialization(#[from] InitializationError),
    #[error("TJXY admin assets are invalid: {0}")]
    AdminAssets(#[from] AdminAssetsError),
    #[error("failed to bind or serve HTTP: {0}")]
    Io(#[from] std::io::Error),
}

#[tokio::main]
#[allow(clippy::too_many_lines)] // Environment-backed startup intentionally composes one service instance.
async fn main() -> Result<(), StartupError> {
    let server_id = env::var("TJXY_SERVER_ID")
        .map_err(|_| StartupError::MissingServerId)
        .and_then(|value| Uuid::parse_str(&value).map_err(StartupError::InvalidServerId))?;
    let server_name = env::var("TJXY_SERVER_NAME").unwrap_or_else(|_| "TJXY".to_owned());
    let bind_address = env::var("TJXY_BIND").unwrap_or_else(|_| "127.0.0.1:8096".to_owned());
    let bind_address = bind_address
        .parse::<SocketAddr>()
        .map_err(StartupError::InvalidBindAddress)?;
    let mut identity = ServerIdentity::new(server_id, server_name, env::consts::OS);
    if let Ok(local_address) = env::var("TJXY_PUBLIC_ADDRESS") {
        identity = identity.with_local_address(local_address);
    }
    let database_url =
        env::var("TJXY_DATABASE_URL").unwrap_or_else(|_| "sqlite://tjxy.db?mode=rwc".to_owned());
    let mut startup = StartupOptions::new(database_url, identity);
    if let Ok(encoded) = env::var("TJXY_CREDENTIAL_KEYRING") {
        let encoded = Zeroizing::new(encoded);
        startup = startup.with_credential_cipher(Arc::new(parse_credential_keyring(&encoded)?));
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
    let assets_dir = env::var("TJXY_ASSETS_DIR").unwrap_or_else(|_| "./data/assets".to_owned());
    startup = startup.with_assets_dir(assets_dir);
    let remote_providers = env::var("TJXY_ENABLE_REMOTE_PROVIDERS")
        .map_or(Ok(false), |value| value.parse::<bool>())
        .map_err(|_| StartupError::InvalidRemoteProviders)?;
    if remote_providers && let Ok(token) = env::var("TJXY_TMDB_ACCESS_TOKEN") {
        let language = env::var("TJXY_TMDB_LANGUAGE").unwrap_or_else(|_| "zh-CN".to_owned());
        let provider = TmdbProvider::new(token, language)
            .map_err(|_| StartupError::InvalidTmdbConfiguration)?;
        startup = startup.with_metadata_provider(Arc::new(provider));
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
    let admin_dist = admin_dist_dir(|| env::var("TJXY_ADMIN_DIST_DIR"));
    let router = build_router_with_admin_dist(state, admin_dist)?;
    let listener = tokio::net::TcpListener::bind(bind_address).await?;
    axum::serve(
        listener,
        router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await?;
    Ok(())
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SerializedCredentialKeyring {
    active_version: i32,
    keys: BTreeMap<i32, Zeroizing<String>>,
}

fn parse_credential_keyring(value: &str) -> Result<CredentialCipher, StartupError> {
    let serialized: SerializedCredentialKeyring =
        serde_json::from_str(value).map_err(|_| StartupError::InvalidCredentialKeyring)?;
    let mut active = None;
    let mut historical = Vec::with_capacity(serialized.keys.len().saturating_sub(1));
    for (version, encoded) in serialized.keys {
        let decoded = Zeroizing::new(
            STANDARD
                .decode(encoded.as_bytes())
                .map_err(|_| StartupError::InvalidCredentialKeyring)?,
        );
        let bytes: [u8; 32] = decoded
            .as_slice()
            .try_into()
            .map_err(|_| StartupError::InvalidCredentialKeyring)?;
        let key = CredentialKey::new(version, bytes)
            .map_err(|_| StartupError::InvalidCredentialKeyring)?;
        if version == serialized.active_version {
            active = Some(key);
        } else {
            historical.push(key);
        }
    }
    CredentialCipher::new(
        active.ok_or(StartupError::InvalidCredentialKeyring)?,
        historical,
    )
    .map_err(|_| StartupError::InvalidCredentialKeyring)
}

#[cfg(test)]
mod tests {
    use std::{env::VarError, path::PathBuf, time::Duration};

    use base64::{Engine as _, engine::general_purpose::STANDARD};

    use super::{admin_dist_dir, media_refresh_interval, parse_credential_keyring};

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
