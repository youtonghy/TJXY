use std::{
    env,
    ffi::OsString,
    fmt, fs,
    fs::OpenOptions,
    io,
    io::{Read, Write},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;
use url::Url;
use uuid::Uuid;
use zeroize::Zeroizing;

const CURRENT_FORMAT_VERSION: u16 = 1;
const CONFIG_FILE_ENVIRONMENT: &str = "TJXY_CONFIG_FILE";
const HOME_ENVIRONMENT: &str = "HOME";
const DEFAULT_CONFIG_SUFFIX: &str = ".config/tjxy/tjxy.toml";
const MAX_CONFIG_BYTES: u64 = 64 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InstallationState {
    Unconfigured,
    Pending(PendingInstallation),
    Completed(CompletedInstallation),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingInstallation {
    installation_id: Uuid,
    server_id: Uuid,
    credential_keyring: SecretString,
    database: DatabaseConfiguration,
    network: NetworkConfiguration,
    profile: InstallationProfile,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompletedInstallation {
    installation_id: Uuid,
    server_id: Uuid,
    credential_keyring: SecretString,
    database: DatabaseConfiguration,
    network: NetworkConfiguration,
    profile: InstallationProfile,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstallationProfile {
    site_title: String,
    site_subtitle: String,
    locale: String,
    logo_url: String,
    icon_url: String,
    administrator_username: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "backend", rename_all = "lowercase", deny_unknown_fields)]
pub enum DatabaseConfiguration {
    Sqlite {
        path: PathBuf,
    },
    #[serde(rename = "postgresql")]
    PostgreSql {
        host: String,
        port: u16,
        database: String,
        username: String,
        password: SecretString,
        tls: DatabaseTlsMode,
    },
    Mysql {
        host: String,
        port: u16,
        database: String,
        username: String,
        password: SecretString,
        tls: DatabaseTlsMode,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DatabaseTlsMode {
    Disable,
    Prefer,
    Require,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NetworkConfiguration {
    listen_host: String,
    port: u16,
    public_url: Option<String>,
}

#[derive(Clone, Eq, PartialEq)]
pub struct SecretString(Zeroizing<String>);

impl fmt::Debug for SecretString {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[REDACTED]")
    }
}

impl Serialize for SecretString {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for SecretString {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer).map(|value| Self(Zeroizing::new(value)))
    }
}

#[derive(Clone, Debug)]
pub struct InstallationConfigStore {
    path: PathBuf,
}

impl InstallationConfigStore {
    #[must_use]
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Resolves `TJXY_CONFIG_FILE` or the native `~/.config/tjxy/tjxy.toml` default.
    ///
    /// # Errors
    ///
    /// Returns an error when no override is set and the home directory is unavailable.
    pub fn discover() -> Result<Self, InstallationConfigError> {
        Ok(Self::at(config_path(
            env::var_os(CONFIG_FILE_ENVIRONMENT),
            env::var_os(HOME_ENVIRONMENT),
        )?))
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    #[must_use]
    pub fn parent_is_writable(&self) -> bool {
        let mut candidate = self.path.parent().unwrap_or_else(|| Path::new("."));
        while !candidate.exists() {
            let Some(parent) = candidate.parent() else {
                return false;
            };
            candidate = parent;
        }
        directory_is_writable(candidate)
    }

    /// Loads the durable installation state without exposing configuration contents in errors.
    ///
    /// # Errors
    ///
    /// Returns a bounded error when the file cannot be read or is invalid.
    pub fn load(&self) -> Result<InstallationState, InstallationConfigError> {
        match fs::symlink_metadata(&self.path) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
                return Err(InstallationConfigError::UnsafeTarget);
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(InstallationState::Unconfigured);
            }
            Err(_) => return Err(InstallationConfigError::Read),
        }
        let contents = read_bounded_regular_file(&self.path)?;
        let stored: StoredInstallationConfig =
            toml::from_str(&contents).map_err(|_| InstallationConfigError::Invalid)?;
        stored.validate()?;
        stored.try_into()
    }

    /// Persists a recoverable pre-database installation configuration.
    ///
    /// # Errors
    ///
    /// Returns an error when the target is unsafe or the atomic write fails.
    pub fn write_pending(
        &self,
        pending: &PendingInstallation,
    ) -> Result<(), InstallationConfigError> {
        match self.load()? {
            InstallationState::Unconfigured => {}
            InstallationState::Pending(existing) if existing == *pending => return Ok(()),
            InstallationState::Pending(_) | InstallationState::Completed(_) => {
                return Err(InstallationConfigError::Conflict);
            }
        }
        self.write(&StoredInstallationConfig::from(pending))
    }

    /// Atomically replaces the matching pending configuration with a completed one.
    ///
    /// # Errors
    ///
    /// Returns an error when the pending identity does not match or the write fails.
    pub fn complete(
        &self,
        completed: &CompletedInstallation,
    ) -> Result<(), InstallationConfigError> {
        match self.load()? {
            InstallationState::Pending(pending)
                if pending.installation_id == completed.installation_id
                    && pending.server_id == completed.server_id => {}
            _ => return Err(InstallationConfigError::Conflict),
        }
        self.write(&StoredInstallationConfig::from(completed))
    }

    fn write(&self, config: &StoredInstallationConfig) -> Result<(), InstallationConfigError> {
        if fs::symlink_metadata(&self.path)
            .is_ok_and(|metadata| metadata.file_type().is_symlink() || !metadata.is_file())
        {
            return Err(InstallationConfigError::UnsafeTarget);
        }
        let parent = self
            .path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent).map_err(|_| InstallationConfigError::Write)?;
        let file_name = self
            .path
            .file_name()
            .ok_or(InstallationConfigError::UnsafeTarget)?
            .to_string_lossy();
        let temporary = parent.join(format!(".{file_name}.{}.tmp", Uuid::new_v4().simple()));
        let result = (|| {
            let mut options = OpenOptions::new();
            options.create_new(true).write(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options
                .open(&temporary)
                .map_err(|_| InstallationConfigError::Write)?;
            let serialized =
                toml::to_string_pretty(config).map_err(|_| InstallationConfigError::Invalid)?;
            if u64::try_from(serialized.len()).map_or(true, |length| length > MAX_CONFIG_BYTES) {
                return Err(InstallationConfigError::Invalid);
            }
            file.write_all(serialized.as_bytes())
                .map_err(|_| InstallationConfigError::Write)?;
            file.sync_all()
                .map_err(|_| InstallationConfigError::Write)?;
            drop(file);
            fs::rename(&temporary, &self.path).map_err(|_| InstallationConfigError::Write)?;
            FileSync::sync_directory(parent)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }
}

fn config_path(
    override_path: Option<OsString>,
    home: Option<OsString>,
) -> Result<PathBuf, InstallationConfigError> {
    if let Some(path) = override_path {
        return Ok(PathBuf::from(path));
    }
    home.map(|path| PathBuf::from(path).join(DEFAULT_CONFIG_SUFFIX))
        .ok_or(InstallationConfigError::ConfigurationDirectoryUnavailable)
}

#[cfg(unix)]
fn directory_is_writable(path: &Path) -> bool {
    use rustix::fs::{Access, access};

    path.is_dir() && access(path, Access::WRITE_OK | Access::EXEC_OK).is_ok()
}

#[cfg(not(unix))]
fn directory_is_writable(path: &Path) -> bool {
    path.metadata()
        .is_ok_and(|metadata| metadata.is_dir() && !metadata.permissions().readonly())
}

#[cfg(test)]
mod config_path_tests {
    use super::{InstallationConfigError, config_path};
    use std::{ffi::OsString, path::PathBuf};

    #[test]
    fn uses_system_default_without_an_override() {
        assert_eq!(
            config_path(None, Some(OsString::from("/home/media"))).unwrap(),
            PathBuf::from("/home/media/.config/tjxy/tjxy.toml")
        );
        assert_eq!(
            config_path(None, None),
            Err(InstallationConfigError::ConfigurationDirectoryUnavailable)
        );
    }

    #[test]
    fn uses_the_explicit_override_verbatim() {
        assert_eq!(
            config_path(Some(OsString::from("relative/tjxy.toml")), None,).unwrap(),
            PathBuf::from("relative/tjxy.toml")
        );
        assert_eq!(
            config_path(Some(OsString::from("/config/tjxy.toml")), None).unwrap(),
            PathBuf::from("/config/tjxy.toml")
        );
    }
}

#[cfg(unix)]
fn read_bounded_regular_file(path: &Path) -> Result<String, InstallationConfigError> {
    use rustix::fs::{FileType, Mode, OFlags, fstat, open};

    let descriptor = open(
        path,
        OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
        Mode::empty(),
    )
    .map_err(|_| InstallationConfigError::Read)?;
    let stat = fstat(&descriptor).map_err(|_| InstallationConfigError::Read)?;
    if !FileType::from_raw_mode(stat.st_mode).is_file()
        || u64::try_from(stat.st_size).map_or(true, |size| size > MAX_CONFIG_BYTES)
    {
        return Err(InstallationConfigError::UnsafeTarget);
    }
    let mut contents = String::new();
    fs::File::from(descriptor)
        .take(MAX_CONFIG_BYTES + 1)
        .read_to_string(&mut contents)
        .map_err(|_| InstallationConfigError::Read)?;
    if u64::try_from(contents.len()).map_or(true, |length| length > MAX_CONFIG_BYTES) {
        return Err(InstallationConfigError::Invalid);
    }
    Ok(contents)
}

#[cfg(not(unix))]
fn read_bounded_regular_file(path: &Path) -> Result<String, InstallationConfigError> {
    let metadata = fs::metadata(path).map_err(|_| InstallationConfigError::Read)?;
    if !metadata.is_file() || metadata.len() > MAX_CONFIG_BYTES {
        return Err(InstallationConfigError::UnsafeTarget);
    }
    fs::read_to_string(path).map_err(|_| InstallationConfigError::Read)
}

struct FileSync;

impl FileSync {
    #[cfg(unix)]
    fn sync_directory(path: &Path) -> Result<(), InstallationConfigError> {
        fs::File::open(path)
            .and_then(|directory| directory.sync_all())
            .map_err(|_| InstallationConfigError::Write)
    }

    #[cfg(not(unix))]
    fn sync_directory(_path: &Path) -> Result<(), InstallationConfigError> {
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredInstallationConfig {
    format_version: u16,
    state: StoredInstallationState,
    installation_id: Uuid,
    server_id: Uuid,
    credential_keyring: SecretString,
    database: DatabaseConfiguration,
    network: NetworkConfiguration,
    profile: InstallationProfile,
}

impl StoredInstallationConfig {
    fn validate(&self) -> Result<(), InstallationConfigError> {
        if self.installation_id.is_nil()
            || self.server_id.is_nil()
            || self.installation_id == self.server_id
            || crate::parse_credential_keyring(self.credential_keyring.expose()).is_err()
            || !valid_profile(&self.profile)
            || !valid_network(&self.network)
            || !valid_database(&self.database)
        {
            return Err(InstallationConfigError::Invalid);
        }
        Ok(())
    }
}

fn valid_profile(profile: &InstallationProfile) -> bool {
    let title_length = profile.site_title.chars().count();
    let subtitle_length = profile.site_subtitle.chars().count();
    title_length > 0
        && title_length <= 120
        && subtitle_length <= 240
        && matches!(profile.locale.as_str(), "zh-CN" | "en-US")
        && valid_brand_asset_url("logo", &profile.logo_url)
        && valid_brand_asset_url("icon", &profile.icon_url)
        && tjxy_common::Username::parse(&profile.administrator_username).is_ok()
}

fn valid_network(network: &NetworkConfiguration) -> bool {
    if network.socket_address().is_err() || network.listen_host.len() > 64 {
        return false;
    }
    network.public_url.as_deref().is_none_or(|value| {
        value.len() <= 2_048
            && Url::parse(value).is_ok_and(|url| {
                matches!(url.scheme(), "http" | "https")
                    && url.host_str().is_some()
                    && url.username().is_empty()
                    && url.password().is_none()
                    && url.path() == "/"
                    && url.query().is_none()
                    && url.fragment().is_none()
            })
    })
}

fn valid_database(database: &DatabaseConfiguration) -> bool {
    let fields_valid = match database {
        DatabaseConfiguration::Sqlite { path } => path.is_absolute(),
        DatabaseConfiguration::PostgreSql {
            host,
            port,
            database,
            username,
            password,
            ..
        }
        | DatabaseConfiguration::Mysql {
            host,
            port,
            database,
            username,
            password,
            ..
        } => {
            !host.is_empty()
                && host.len() <= 255
                && *port > 0
                && !database.is_empty()
                && database.len() <= 128
                && !username.is_empty()
                && username.len() <= 128
                && password.expose().len() <= 4_096
        }
    };
    fields_valid && database.connection_url().is_ok()
}

pub(crate) fn valid_brand_asset_url(kind: &str, value: &str) -> bool {
    let default = match kind {
        "logo" => "/brand/tjxy-mark.webp",
        "icon" => "/brand/favicon.svg",
        _ => return false,
    };
    if value == default {
        return true;
    }
    let Some(file) = value.strip_prefix(&format!("/Branding/Assets/{kind}-")) else {
        return false;
    };
    let Some((digest, extension)) = file.rsplit_once('.') else {
        return false;
    };
    digest.len() == 64
        && digest
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        && matches!(extension, "png" | "jpg" | "webp" | "ico")
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum StoredInstallationState {
    Pending,
    Completed,
}

impl From<&PendingInstallation> for StoredInstallationConfig {
    fn from(value: &PendingInstallation) -> Self {
        Self {
            format_version: CURRENT_FORMAT_VERSION,
            state: StoredInstallationState::Pending,
            installation_id: value.installation_id,
            server_id: value.server_id,
            credential_keyring: value.credential_keyring.clone(),
            database: value.database.clone(),
            network: value.network.clone(),
            profile: value.profile.clone(),
        }
    }
}

impl From<&CompletedInstallation> for StoredInstallationConfig {
    fn from(value: &CompletedInstallation) -> Self {
        Self {
            format_version: CURRENT_FORMAT_VERSION,
            state: StoredInstallationState::Completed,
            installation_id: value.installation_id,
            server_id: value.server_id,
            credential_keyring: value.credential_keyring.clone(),
            database: value.database.clone(),
            network: value.network.clone(),
            profile: value.profile.clone(),
        }
    }
}

impl PendingInstallation {
    #[must_use]
    pub fn new(
        installation_id: Uuid,
        server_id: Uuid,
        credential_keyring: SecretString,
        database: DatabaseConfiguration,
        network: NetworkConfiguration,
        profile: InstallationProfile,
    ) -> Self {
        Self {
            installation_id,
            server_id,
            credential_keyring,
            database,
            network,
            profile,
        }
    }

    #[must_use]
    pub fn complete(self) -> CompletedInstallation {
        CompletedInstallation {
            installation_id: self.installation_id,
            server_id: self.server_id,
            credential_keyring: self.credential_keyring,
            database: self.database,
            network: self.network,
            profile: self.profile,
        }
    }
}

impl CompletedInstallation {
    #[must_use]
    pub const fn installation_id(&self) -> Uuid {
        self.installation_id
    }

    #[must_use]
    pub const fn server_id(&self) -> Uuid {
        self.server_id
    }

    #[must_use]
    pub fn credential_keyring(&self) -> &str {
        self.credential_keyring.expose()
    }

    #[must_use]
    pub const fn database(&self) -> &DatabaseConfiguration {
        &self.database
    }

    #[must_use]
    pub const fn network(&self) -> &NetworkConfiguration {
        &self.network
    }

    #[must_use]
    pub const fn profile(&self) -> &InstallationProfile {
        &self.profile
    }
}

impl PendingInstallation {
    #[must_use]
    pub const fn installation_id(&self) -> Uuid {
        self.installation_id
    }

    #[must_use]
    pub const fn server_id(&self) -> Uuid {
        self.server_id
    }

    #[must_use]
    pub const fn database(&self) -> &DatabaseConfiguration {
        &self.database
    }

    #[must_use]
    pub const fn network(&self) -> &NetworkConfiguration {
        &self.network
    }

    #[must_use]
    pub const fn profile(&self) -> &InstallationProfile {
        &self.profile
    }
}

impl InstallationProfile {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        site_title: impl Into<String>,
        site_subtitle: impl Into<String>,
        locale: impl Into<String>,
        logo_url: impl Into<String>,
        icon_url: impl Into<String>,
        administrator_username: impl Into<String>,
    ) -> Self {
        Self {
            site_title: site_title.into(),
            site_subtitle: site_subtitle.into(),
            locale: locale.into(),
            logo_url: logo_url.into(),
            icon_url: icon_url.into(),
            administrator_username: administrator_username.into(),
        }
    }

    #[must_use]
    pub fn site_title(&self) -> &str {
        &self.site_title
    }
    #[must_use]
    pub fn site_subtitle(&self) -> &str {
        &self.site_subtitle
    }
    #[must_use]
    pub fn locale(&self) -> &str {
        &self.locale
    }
    #[must_use]
    pub fn logo_url(&self) -> &str {
        &self.logo_url
    }
    #[must_use]
    pub fn icon_url(&self) -> &str {
        &self.icon_url
    }
    #[must_use]
    pub fn administrator_username(&self) -> &str {
        &self.administrator_username
    }
}

impl DatabaseConfiguration {
    /// Builds the driver URL in zeroizing memory.
    ///
    /// # Errors
    ///
    /// Returns an invalid-configuration error for values that cannot form a safe URL.
    pub fn connection_url(&self) -> Result<Zeroizing<String>, InstallationConfigError> {
        match self {
            Self::Sqlite { path } => Ok(Zeroizing::new(format!(
                "sqlite://{}?mode=rwc",
                path.display()
            ))),
            Self::PostgreSql {
                host,
                port,
                database,
                username,
                password,
                tls,
            } => connection_url(
                "postgresql",
                host,
                *port,
                database,
                username,
                password,
                ("sslmode", postgres_tls(*tls)),
            ),
            Self::Mysql {
                host,
                port,
                database,
                username,
                password,
                tls,
            } => connection_url(
                "mysql",
                host,
                *port,
                database,
                username,
                password,
                ("ssl-mode", mysql_tls(*tls)),
            ),
        }
    }
}

fn connection_url(
    scheme: &str,
    host: &str,
    port: u16,
    database: &str,
    username: &str,
    password: &SecretString,
    tls: (&str, &str),
) -> Result<Zeroizing<String>, InstallationConfigError> {
    for value in [host, database, username] {
        if value.is_empty() || value.chars().count() > 255 || value.chars().any(char::is_control) {
            return Err(InstallationConfigError::Invalid);
        }
    }
    let mut url = Url::parse(&format!("{scheme}://localhost"))
        .map_err(|_| InstallationConfigError::Invalid)?;
    url.set_host(Some(host))
        .map_err(|_| InstallationConfigError::Invalid)?;
    url.set_port(Some(port))
        .map_err(|()| InstallationConfigError::Invalid)?;
    url.set_username(username)
        .map_err(|()| InstallationConfigError::Invalid)?;
    url.set_password(Some(password.expose()))
        .map_err(|()| InstallationConfigError::Invalid)?;
    url.set_path(&format!("/{database}"));
    url.query_pairs_mut().append_pair(tls.0, tls.1);
    Ok(Zeroizing::new(url.into()))
}

const fn postgres_tls(mode: DatabaseTlsMode) -> &'static str {
    match mode {
        DatabaseTlsMode::Disable => "disable",
        DatabaseTlsMode::Prefer => "prefer",
        DatabaseTlsMode::Require => "require",
    }
}

const fn mysql_tls(mode: DatabaseTlsMode) -> &'static str {
    match mode {
        DatabaseTlsMode::Disable => "DISABLED",
        DatabaseTlsMode::Prefer => "PREFERRED",
        DatabaseTlsMode::Require => "REQUIRED",
    }
}

impl NetworkConfiguration {
    #[must_use]
    pub fn new(listen_host: impl Into<String>, port: u16, public_url: Option<String>) -> Self {
        Self {
            listen_host: listen_host.into(),
            port,
            public_url,
        }
    }

    #[must_use]
    pub fn listen_host(&self) -> &str {
        &self.listen_host
    }

    #[must_use]
    pub const fn port(&self) -> u16 {
        self.port
    }

    #[must_use]
    pub fn public_url(&self) -> Option<&str> {
        self.public_url.as_deref()
    }

    #[must_use]
    pub fn admin_login_url(&self) -> String {
        self.public_url.clone().unwrap_or_else(|| {
            let host = if self.listen_host == "0.0.0.0" || self.listen_host == "::" {
                "127.0.0.1"
            } else {
                &self.listen_host
            };
            format!("http://{host}:{}", self.port)
        }) + "/login?redirect=%2Fadmin"
    }

    /// Resolves the configured listener.
    ///
    /// # Errors
    ///
    /// Returns an invalid-configuration error for malformed IP addresses.
    pub fn socket_address(&self) -> Result<std::net::SocketAddr, InstallationConfigError> {
        format!("{}:{}", self.listen_host, self.port)
            .parse()
            .map_err(|_| InstallationConfigError::Invalid)
    }
}

impl SecretString {
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(Zeroizing::new(value.into()))
    }

    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}

impl TryFrom<StoredInstallationConfig> for InstallationState {
    type Error = InstallationConfigError;

    fn try_from(value: StoredInstallationConfig) -> Result<Self, Self::Error> {
        if value.format_version != CURRENT_FORMAT_VERSION {
            return Err(InstallationConfigError::UnsupportedFormat);
        }
        let StoredInstallationConfig {
            installation_id,
            server_id,
            credential_keyring,
            database,
            network,
            profile,
            state,
            ..
        } = value;
        Ok(match state {
            StoredInstallationState::Pending => Self::Pending(PendingInstallation {
                installation_id,
                server_id,
                credential_keyring,
                database,
                network,
                profile,
            }),
            StoredInstallationState::Completed => Self::Completed(CompletedInstallation {
                installation_id,
                server_id,
                credential_keyring,
                database,
                network,
                profile,
            }),
        })
    }
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum InstallationConfigError {
    #[error("the home configuration directory is unavailable")]
    ConfigurationDirectoryUnavailable,
    #[error("the installation configuration could not be read")]
    Read,
    #[error("the installation configuration is invalid")]
    Invalid,
    #[error("the installation configuration format is unsupported")]
    UnsupportedFormat,
    #[error("the installation configuration target is unsafe")]
    UnsafeTarget,
    #[error("the installation configuration could not be written")]
    Write,
    #[error("the installation configuration changed or is not pending")]
    Conflict,
}
