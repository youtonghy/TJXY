use std::{collections::HashSet, net::IpAddr, path::Path};

use chrono::{DateTime, Utc};
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DbErr,
    sea_query::{Alias, Expr, Query},
};
use thiserror::Error;
use url::Url;

pub const DEFAULT_SYSTEM_LOCALE: &str = "zh-CN";
pub const DEFAULT_SITE_TITLE: &str = "TJXY";
pub const DEFAULT_SITE_SUBTITLE: &str = "Your media library";
pub const DEFAULT_LOGO_URL: &str = "/brand/tjxy-mark.webp";
pub const DEFAULT_ICON_URL: &str = "/brand/favicon.svg";
pub const DEFAULT_LISTEN_HOST: &str = "127.0.0.1";
pub const DEFAULT_PORT: u16 = 8096;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SystemSettingsRecord {
    locale: String,
    site_title: String,
    site_subtitle: String,
    logo_url: String,
    icon_url: String,
    public_url: Option<String>,
    listen_host: String,
    port: u16,
    media_browser_roots: Vec<String>,
    revision: i64,
    updated_at: DateTime<Utc>,
}

impl SystemSettingsRecord {
    #[must_use]
    pub fn locale(&self) -> &str {
        &self.locale
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
    pub fn logo_url(&self) -> &str {
        &self.logo_url
    }
    #[must_use]
    pub fn icon_url(&self) -> &str {
        &self.icon_url
    }
    #[must_use]
    pub fn public_url(&self) -> Option<&str> {
        self.public_url.as_deref()
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
    pub fn media_browser_roots(&self) -> &[String] {
        &self.media_browser_roots
    }
    #[must_use]
    pub const fn revision(&self) -> i64 {
        self.revision
    }
    #[must_use]
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SystemSettingsInput {
    pub locale: String,
    pub site_title: String,
    pub site_subtitle: String,
    pub logo_url: String,
    pub icon_url: String,
    pub public_url: Option<String>,
    pub listen_host: String,
    pub port: u16,
    pub media_browser_roots: Vec<String>,
}

impl Default for SystemSettingsInput {
    fn default() -> Self {
        Self {
            locale: DEFAULT_SYSTEM_LOCALE.to_owned(),
            site_title: DEFAULT_SITE_TITLE.to_owned(),
            site_subtitle: DEFAULT_SITE_SUBTITLE.to_owned(),
            logo_url: DEFAULT_LOGO_URL.to_owned(),
            icon_url: DEFAULT_ICON_URL.to_owned(),
            public_url: None,
            listen_host: DEFAULT_LISTEN_HOST.to_owned(),
            port: DEFAULT_PORT,
            media_browser_roots: Vec::new(),
        }
    }
}

impl From<&SystemSettingsRecord> for SystemSettingsInput {
    fn from(value: &SystemSettingsRecord) -> Self {
        Self {
            locale: value.locale.clone(),
            site_title: value.site_title.clone(),
            site_subtitle: value.site_subtitle.clone(),
            logo_url: value.logo_url.clone(),
            icon_url: value.icon_url.clone(),
            public_url: value.public_url.clone(),
            listen_host: value.listen_host.clone(),
            port: value.port,
            media_browser_roots: value.media_browser_roots.clone(),
        }
    }
}

pub struct SystemSettingsRepository<'connection> {
    database: &'connection DatabaseConnection,
}

impl<'connection> SystemSettingsRepository<'connection> {
    #[must_use]
    pub const fn new(database: &'connection DatabaseConnection) -> Self {
        Self { database }
    }

    /// Loads the singleton system settings row.
    ///
    /// # Errors
    ///
    /// Returns a repository error when the database cannot be queried or stored values are invalid.
    pub async fn get(&self) -> Result<Option<SystemSettingsRecord>, SystemSettingsRepositoryError> {
        let query = Query::select()
            .columns([
                Alias::new("locale"),
                Alias::new("site_title"),
                Alias::new("site_subtitle"),
                Alias::new("logo_url"),
                Alias::new("icon_url"),
                Alias::new("public_url"),
                Alias::new("listen_host"),
                Alias::new("port"),
                Alias::new("media_browser_roots"),
                Alias::new("revision"),
                Alias::new("updated_at"),
            ])
            .from(Alias::new("system_settings"))
            .and_where(Expr::col(Alias::new("id")).eq(1_i32))
            .to_owned();
        let Some(row) = self
            .database
            .query_one(self.database.get_database_backend().build(&query))
            .await?
        else {
            return Ok(None);
        };
        let port: i32 = row.try_get("", "port")?;
        let media_browser_roots = row
            .try_get::<Option<serde_json::Value>>("", "media_browser_roots")?
            .map_or_else(
                || Ok(Vec::new()),
                |value| {
                    serde_json::from_value(value)
                        .map_err(|_| SystemSettingsRepositoryError::InvalidMediaBrowserRoots)
                },
            )?;
        Ok(Some(SystemSettingsRecord {
            locale: row.try_get("", "locale")?,
            site_title: row.try_get("", "site_title")?,
            site_subtitle: row.try_get("", "site_subtitle")?,
            logo_url: row.try_get("", "logo_url")?,
            icon_url: row.try_get("", "icon_url")?,
            public_url: row.try_get("", "public_url")?,
            listen_host: row.try_get("", "listen_host")?,
            port: u16::try_from(port).map_err(|_| SystemSettingsRepositoryError::InvalidPort)?,
            media_browser_roots,
            revision: row.try_get("", "revision")?,
            updated_at: row.try_get("", "updated_at")?,
        }))
    }

    /// Updates only the interface locale while preserving the remaining settings.
    ///
    /// # Errors
    ///
    /// Returns a repository error when validation, revision fencing, or persistence fails.
    pub async fn put_locale(
        &self,
        locale: &str,
        expected_revision: Option<i64>,
    ) -> Result<SystemSettingsRecord, SystemSettingsRepositoryError> {
        let current = self.get().await?;
        let mut input = current
            .as_ref()
            .map_or_else(SystemSettingsInput::default, Into::into);
        input.locale = locale.to_owned();
        self.put(&input, expected_revision).await
    }

    /// Validates and persists the complete system settings document.
    ///
    /// # Errors
    ///
    /// Returns a repository error when validation, revision fencing, or persistence fails.
    pub async fn put(
        &self,
        input: &SystemSettingsInput,
        expected_revision: Option<i64>,
    ) -> Result<SystemSettingsRecord, SystemSettingsRepositoryError> {
        let input = validate(input)?;
        if expected_revision.is_some_and(|value| value <= 0) {
            return Err(SystemSettingsRepositoryError::InvalidRevision);
        }
        let current = self.get().await?;
        if current.as_ref().map(SystemSettingsRecord::revision) != expected_revision
            && current.is_some()
        {
            return Err(SystemSettingsRepositoryError::Conflict);
        }
        let now = Utc::now();
        let revision = current.as_ref().map_or(1, |value| value.revision + 1);
        let statement = Query::insert()
            .into_table(Alias::new("system_settings"))
            .columns([
                Alias::new("id"),
                Alias::new("locale"),
                Alias::new("site_title"),
                Alias::new("site_subtitle"),
                Alias::new("logo_url"),
                Alias::new("icon_url"),
                Alias::new("public_url"),
                Alias::new("listen_host"),
                Alias::new("port"),
                Alias::new("media_browser_roots"),
                Alias::new("revision"),
                Alias::new("created_at"),
                Alias::new("updated_at"),
            ])
            .values_panic([
                1_i32.into(),
                input.locale.clone().into(),
                input.site_title.clone().into(),
                input.site_subtitle.clone().into(),
                input.logo_url.clone().into(),
                input.icon_url.clone().into(),
                input.public_url.clone().into(),
                input.listen_host.clone().into(),
                i32::from(input.port).into(),
                serde_json::Value::Array(
                    input
                        .media_browser_roots
                        .iter()
                        .cloned()
                        .map(serde_json::Value::String)
                        .collect(),
                )
                .into(),
                revision.into(),
                now.into(),
                now.into(),
            ])
            .on_conflict(
                sea_orm::sea_query::OnConflict::column(Alias::new("id"))
                    .update_columns([
                        Alias::new("locale"),
                        Alias::new("site_title"),
                        Alias::new("site_subtitle"),
                        Alias::new("logo_url"),
                        Alias::new("icon_url"),
                        Alias::new("public_url"),
                        Alias::new("listen_host"),
                        Alias::new("port"),
                        Alias::new("media_browser_roots"),
                        Alias::new("revision"),
                        Alias::new("updated_at"),
                    ])
                    .to_owned(),
            )
            .to_owned();
        self.database
            .execute(self.database.get_database_backend().build(&statement))
            .await?;
        Ok(SystemSettingsRecord {
            locale: input.locale,
            site_title: input.site_title,
            site_subtitle: input.site_subtitle,
            logo_url: input.logo_url,
            icon_url: input.icon_url,
            public_url: input.public_url,
            listen_host: input.listen_host,
            port: input.port,
            media_browser_roots: input.media_browser_roots,
            revision,
            updated_at: now,
        })
    }
}

fn validate(
    input: &SystemSettingsInput,
) -> Result<SystemSettingsInput, SystemSettingsRepositoryError> {
    if !matches!(input.locale.as_str(), "zh-CN" | "en-US") {
        return Err(SystemSettingsRepositoryError::InvalidLocale);
    }
    let site_title = input.site_title.trim();
    let site_subtitle = input.site_subtitle.trim();
    if site_title.is_empty()
        || site_title.len() > 120
        || site_subtitle.len() > 240
        || input.port == 0
    {
        return Err(SystemSettingsRepositoryError::InvalidBranding);
    }
    if !valid_asset_url(&input.logo_url) || !valid_asset_url(&input.icon_url) {
        return Err(SystemSettingsRepositoryError::InvalidBranding);
    }
    let public_url = input
        .public_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if public_url.is_some_and(|value| !valid_http_url(value)) {
        return Err(SystemSettingsRepositoryError::InvalidPublicUrl);
    }
    let listen_host = input.listen_host.trim();
    if listen_host.parse::<IpAddr>().is_err() {
        return Err(SystemSettingsRepositoryError::InvalidListenHost);
    }
    if input.media_browser_roots.len() > 64 {
        return Err(SystemSettingsRepositoryError::InvalidMediaBrowserRoots);
    }
    let mut roots = Vec::with_capacity(input.media_browser_roots.len());
    let mut unique = HashSet::with_capacity(input.media_browser_roots.len());
    for root in &input.media_browser_roots {
        let root = root.trim();
        if root.is_empty()
            || root.len() > 4096
            || root.chars().any(char::is_control)
            || !Path::new(root).is_absolute()
            || !unique.insert(root.to_owned())
        {
            return Err(SystemSettingsRepositoryError::InvalidMediaBrowserRoots);
        }
        roots.push(root.to_owned());
    }
    Ok(SystemSettingsInput {
        locale: input.locale.clone(),
        site_title: site_title.to_owned(),
        site_subtitle: site_subtitle.to_owned(),
        logo_url: input.logo_url.trim().to_owned(),
        icon_url: input.icon_url.trim().to_owned(),
        public_url: public_url.map(str::to_owned),
        listen_host: listen_host.to_owned(),
        port: input.port,
        media_browser_roots: roots,
    })
}

fn valid_asset_url(value: &str) -> bool {
    let value = value.trim();
    value.len() <= 2048
        && !value.chars().any(char::is_whitespace)
        && ((value.starts_with('/') && !value.starts_with("//")) || valid_http_url(value))
}

fn valid_http_url(value: &str) -> bool {
    if value.len() > 2048 || value.chars().any(char::is_whitespace) {
        return false;
    }
    let Ok(url) = Url::parse(value) else {
        return false;
    };
    matches!(url.scheme(), "http" | "https")
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
}

#[derive(Debug, Error)]
pub enum SystemSettingsRepositoryError {
    #[error("system locale is invalid")]
    InvalidLocale,
    #[error("system branding is invalid")]
    InvalidBranding,
    #[error("system public URL is invalid")]
    InvalidPublicUrl,
    #[error("system listen host is invalid")]
    InvalidListenHost,
    #[error("system port is invalid")]
    InvalidPort,
    #[error("system media browser roots are invalid")]
    InvalidMediaBrowserRoots,
    #[error("system settings revision is invalid")]
    InvalidRevision,
    #[error("system settings revision conflict")]
    Conflict,
    #[error(transparent)]
    Database(#[from] DbErr),
}

#[cfg(test)]
mod tests {
    use super::valid_http_url;

    #[test]
    fn public_urls_require_a_web_scheme_and_real_host() {
        assert!(valid_http_url("https://media.example.com/library"));
        assert!(!valid_http_url("not-a-url"));
        assert!(!valid_http_url("https://user:secret@media.example.com"));
        assert!(!valid_http_url("https://media.example.com/?token=secret"));
    }
}
