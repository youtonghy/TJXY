use std::{
    env,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
};

use axum::{
    Json,
    body::Bytes,
    extract::{Path as AxumPath, RawQuery, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tjxy_application::FilesystemBrowser;
use tjxy_db::{
    DEFAULT_SITE_THEME_ID, DEFAULT_SITE_THEME_SCHEMA_VERSION, DEFAULT_SYSTEM_LOCALE,
    SiteThemeSelectionInput, SiteThemeSettingsRecord, SiteThemeSettingsRepository,
    SiteThemeSettingsRepositoryError, SystemSettingsInput, SystemSettingsRecord,
    SystemSettingsRepository, SystemSettingsRepositoryError,
};
use tokio::sync::watch;

use crate::{AppState, auth};

pub(crate) const MAX_BRAND_ASSET_BYTES: usize = 2 * 1024 * 1024;

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct SystemLanguageDto {
    locale: String,
    revision: i64,
    supported_locales: [&'static str; 2],
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct PublicSystemSettingsDto {
    locale: String,
    site_title: String,
    site_subtitle: String,
    logo_url: String,
    icon_url: String,
    revision: i64,
    supported_locales: [&'static str; 2],
    theme: PublicThemeSettingsDto,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct PublicThemeSettingsDto {
    id: String,
    schema_version: u32,
    options: serde_json::Value,
    revision: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct AdminSystemSettingsDto {
    locale: String,
    site_title: String,
    site_subtitle: String,
    logo_url: String,
    icon_url: String,
    public_url: Option<String>,
    listen_host: String,
    port: u16,
    media_browser_roots: Vec<String>,
    invalid_media_browser_root_indexes: Vec<usize>,
    revision: i64,
    restart_required: bool,
    environment_overrides: EnvironmentOverridesDto,
    supported_locales: [&'static str; 2],
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
#[allow(clippy::struct_excessive_bools)] // Each field reports one independent environment override.
struct EnvironmentOverridesDto {
    site_title: bool,
    public_url: bool,
    listen_address: bool,
    media_browser_roots: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct UploadedAssetDto {
    url: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
struct UpdateSystemLanguageRequest {
    locale: String,
    revision: Option<i64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
struct UpdateSystemSettingsRequest {
    locale: String,
    site_title: String,
    site_subtitle: String,
    logo_url: String,
    icon_url: String,
    public_url: Option<String>,
    listen_host: String,
    port: u16,
    media_browser_roots: Option<Vec<String>>,
    revision: Option<i64>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct AdminThemeSettingsDto {
    active_theme_id: String,
    configurations: Vec<ThemeConfigurationDto>,
    revision: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct ThemeConfigurationDto {
    theme_id: String,
    schema_version: u32,
    options: serde_json::Value,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
struct UpdateThemeSettingsRequest {
    theme_id: String,
    schema_version: u32,
    options: serde_json::Value,
    revision: Option<i64>,
}

pub(crate) async fn get_public_language(State(state): State<AppState>) -> Response {
    let Some(service) = state.system_settings.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match service.get().await {
        Ok(record) => Json(language_dto(record.as_ref())).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(crate) async fn get_public_settings(State(state): State<AppState>) -> Response {
    let Some(service) = state.system_settings.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match (service.get().await, service.get_theme().await) {
        (Ok(record), Ok(theme)) => {
            Json(public_dto(record.as_ref(), theme.as_ref())).into_response()
        }
        (Err(_), _) | (_, Err(_)) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(crate) async fn get_admin(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    if let Err(response) =
        auth::authenticated_administrator(&state, &headers, query.as_deref()).await
    {
        return response;
    }
    let Some(service) = state.system_settings.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match service.get().await {
        Ok(record) => {
            let invalid_root_indexes = invalid_media_browser_root_indexes(record.as_ref()).await;
            Json(admin_dto(record.as_ref(), false, invalid_root_indexes)).into_response()
        }
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(crate) async fn get_admin_theme(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    if let Err(response) =
        auth::authenticated_administrator(&state, &headers, query.as_deref()).await
    {
        return response;
    }
    let Some(service) = state.system_settings.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match service.get_theme().await {
        Ok(record) => Json(admin_theme_dto(record.as_ref())).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(crate) async fn put_admin_theme(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    body: Bytes,
) -> Response {
    if let Err(response) =
        auth::authenticated_administrator(&state, &headers, query.as_deref()).await
    {
        return response;
    }
    let Ok(request) = serde_json::from_slice::<UpdateThemeSettingsRequest>(&body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Some(service) = state.system_settings.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let input = SiteThemeSelectionInput {
        theme_id: request.theme_id,
        schema_version: request.schema_version,
        options: request.options,
    };
    match service.put_theme(&input, request.revision).await {
        Ok(record) => Json(admin_theme_dto(Some(&record))).into_response(),
        Err(error) => theme_repository_error(&error),
    }
}

pub(crate) async fn put_setup(State(state): State<AppState>, body: Bytes) -> Response {
    let Some(auth_service) = state.auth.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match auth_service.has_users().await {
        Ok(true) => return StatusCode::FORBIDDEN.into_response(),
        Ok(false) => {}
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
    let Ok(request) = serde_json::from_slice::<UpdateSystemLanguageRequest>(&body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Some(service) = state.system_settings.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let revision = match service.get().await {
        Ok(record) => record.map(|value| value.revision()),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    match service.put_locale(&request.locale, revision).await {
        Ok(record) => Json(language_dto(Some(&record))).into_response(),
        Err(error) => repository_error(&error),
    }
}

pub(crate) async fn put_admin_language(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    body: Bytes,
) -> Response {
    if let Err(response) =
        auth::authenticated_administrator(&state, &headers, query.as_deref()).await
    {
        return response;
    }
    let Ok(request) = serde_json::from_slice::<UpdateSystemLanguageRequest>(&body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Some(service) = state.system_settings.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match service.put_locale(&request.locale, request.revision).await {
        Ok(record) => Json(language_dto(Some(&record))).into_response(),
        Err(error) => repository_error(&error),
    }
}

pub(crate) async fn put_admin(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    body: Bytes,
) -> Response {
    if let Err(response) =
        auth::authenticated_administrator(&state, &headers, query.as_deref()).await
    {
        return response;
    }
    let Ok(request) = serde_json::from_slice::<UpdateSystemSettingsRequest>(&body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Some(service) = state.system_settings.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let Ok(previous) = service.get().await else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let browser_roots_overridden = environment_media_browser_roots().is_some();
    let media_browser_roots = if browser_roots_overridden {
        previous
            .as_ref()
            .map_or_else(Vec::new, |value| value.media_browser_roots().to_vec())
    } else {
        request.media_browser_roots.unwrap_or_else(|| {
            previous
                .as_ref()
                .map_or_else(Vec::new, |value| value.media_browser_roots().to_vec())
        })
    };
    if !browser_roots_overridden && !media_browser_roots.is_empty() {
        let browser_roots = media_browser_roots
            .iter()
            .map(|root| PathBuf::from(root.as_str()))
            .collect::<Vec<_>>();
        if FilesystemBrowser::from_roots(browser_roots).await.is_err() {
            return StatusCode::BAD_REQUEST.into_response();
        }
    }
    let input = SystemSettingsInput {
        locale: request.locale,
        site_title: request.site_title,
        site_subtitle: request.site_subtitle,
        logo_url: request.logo_url,
        icon_url: request.icon_url,
        public_url: request.public_url,
        listen_host: request.listen_host,
        port: request.port,
        media_browser_roots,
    };
    match service.put(&input, request.revision).await {
        Ok(record) => {
            let restart_required = previous.as_ref().is_none_or(|value| {
                value.listen_host() != record.listen_host()
                    || value.port() != record.port()
                    || value.media_browser_roots() != record.media_browser_roots()
            });
            Json(admin_dto(Some(&record), restart_required, Vec::new())).into_response()
        }
        Err(error) => repository_error(&error),
    }
}

pub(crate) async fn upload_asset(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    AxumPath(kind): AxumPath<String>,
    body: Bytes,
) -> Response {
    if let Err(response) =
        auth::authenticated_administrator(&state, &headers, query.as_deref()).await
    {
        return response;
    }
    let Some(service) = state.system_settings.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok());
    match service.store_asset(&kind, content_type, &body).await {
        Ok(url) => Json(UploadedAssetDto { url }).into_response(),
        Err(AssetUploadError::Invalid) => StatusCode::BAD_REQUEST.into_response(),
        Err(AssetUploadError::TooLarge) => StatusCode::PAYLOAD_TOO_LARGE.into_response(),
        Err(AssetUploadError::Io(_)) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(crate) async fn get_asset(
    State(state): State<AppState>,
    AxumPath(file): AxumPath<String>,
) -> Response {
    let Some(service) = state.system_settings.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let Some((content_type, path)) = service.asset_path(&file) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Ok(bytes) = tokio::fs::read(path).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let mut response = bytes.into_response();
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=31536000, immutable"),
    );
    response
}

pub(crate) async fn restart(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    if let Err(response) =
        auth::authenticated_administrator(&state, &headers, query.as_deref()).await
    {
        return response;
    }
    state.restart.request();
    StatusCode::NO_CONTENT.into_response()
}

pub(crate) struct SystemSettingsService {
    database: sea_orm::DatabaseConnection,
    asset_dir: PathBuf,
}

impl SystemSettingsService {
    pub(crate) fn new(database: sea_orm::DatabaseConnection, asset_dir: PathBuf) -> Self {
        Self {
            database,
            asset_dir,
        }
    }
    pub(crate) const fn database(&self) -> &sea_orm::DatabaseConnection {
        &self.database
    }
    pub(crate) async fn get(
        &self,
    ) -> Result<Option<SystemSettingsRecord>, SystemSettingsRepositoryError> {
        SystemSettingsRepository::new(&self.database).get().await
    }
    async fn get_theme(
        &self,
    ) -> Result<Option<SiteThemeSettingsRecord>, SiteThemeSettingsRepositoryError> {
        SiteThemeSettingsRepository::new(&self.database).get().await
    }
    async fn put_theme(
        &self,
        input: &SiteThemeSelectionInput,
        revision: Option<i64>,
    ) -> Result<SiteThemeSettingsRecord, SiteThemeSettingsRepositoryError> {
        SiteThemeSettingsRepository::new(&self.database)
            .put(input, revision)
            .await
    }
    async fn put_locale(
        &self,
        locale: &str,
        revision: Option<i64>,
    ) -> Result<SystemSettingsRecord, SystemSettingsRepositoryError> {
        SystemSettingsRepository::new(&self.database)
            .put_locale(locale, revision)
            .await
    }
    async fn put(
        &self,
        input: &SystemSettingsInput,
        revision: Option<i64>,
    ) -> Result<SystemSettingsRecord, SystemSettingsRepositoryError> {
        SystemSettingsRepository::new(&self.database)
            .put(input, revision)
            .await
    }
    async fn store_asset(
        &self,
        kind: &str,
        content_type: Option<&str>,
        bytes: &[u8],
    ) -> Result<String, AssetUploadError> {
        store_brand_asset(&self.asset_dir, kind, content_type, bytes).await
    }
    fn asset_path(&self, file: &str) -> Option<(&'static str, PathBuf)> {
        if file.contains('/') || file.contains('\\') || file.starts_with('.') {
            return None;
        }
        let content_type = match Path::new(file).extension()?.to_str()? {
            "png" => "image/png",
            "jpg" => "image/jpeg",
            "webp" => "image/webp",
            "ico" => "image/x-icon",
            _ => return None,
        };
        Some((content_type, self.asset_dir.join(file)))
    }
}

#[derive(Clone)]
pub struct RestartController {
    sender: Arc<watch::Sender<bool>>,
}

impl Default for RestartController {
    fn default() -> Self {
        let (sender, _) = watch::channel(false);
        Self {
            sender: Arc::new(sender),
        }
    }
}

impl RestartController {
    fn request(&self) {
        let _ = self.sender.send(true);
    }
    pub async fn requested(&self) {
        let mut receiver = self.sender.subscribe();
        while !*receiver.borrow() && receiver.changed().await.is_ok() {}
    }
    #[must_use]
    pub fn is_requested(&self) -> bool {
        *self.sender.borrow()
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum AssetUploadError {
    #[error("invalid brand asset")]
    Invalid,
    #[error("brand asset is too large")]
    TooLarge,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub(crate) async fn store_brand_asset(
    asset_dir: &Path,
    kind: &str,
    content_type: Option<&str>,
    bytes: &[u8],
) -> Result<String, AssetUploadError> {
    if !matches!(kind, "logo" | "icon") || bytes.is_empty() {
        return Err(AssetUploadError::Invalid);
    }
    if bytes.len() > MAX_BRAND_ASSET_BYTES {
        return Err(AssetUploadError::TooLarge);
    }
    let (_, extension) = validated_image(content_type, bytes).ok_or(AssetUploadError::Invalid)?;
    let digest = Sha256::digest(bytes);
    let filename = format!("{kind}-{digest:x}.{extension}");
    tokio::fs::create_dir_all(asset_dir).await?;
    let path = asset_dir.join(&filename);
    if !path.exists() {
        let temporary = asset_dir.join(format!(".{filename}.tmp"));
        tokio::fs::write(&temporary, bytes).await?;
        tokio::fs::rename(temporary, &path).await?;
    }
    Ok(format!("/Branding/Assets/{filename}"))
}

fn validated_image(
    content_type: Option<&str>,
    bytes: &[u8],
) -> Option<(&'static str, &'static str)> {
    match content_type?.split(';').next()?.trim() {
        "image/png" if bytes.starts_with(b"\x89PNG\r\n\x1a\n") => Some(("image/png", "png")),
        "image/jpeg" if bytes.starts_with(b"\xff\xd8\xff") => Some(("image/jpeg", "jpg")),
        "image/webp" if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") => {
            Some(("image/webp", "webp"))
        }
        "image/x-icon" | "image/vnd.microsoft.icon" if bytes.starts_with(b"\0\0\x01\0") => {
            Some(("image/x-icon", "ico"))
        }
        _ => None,
    }
}

fn defaults() -> SystemSettingsInput {
    SystemSettingsInput::default()
}

fn language_dto(record: Option<&SystemSettingsRecord>) -> SystemLanguageDto {
    SystemLanguageDto {
        locale: record.map_or(DEFAULT_SYSTEM_LOCALE.to_owned(), |value| {
            value.locale().to_owned()
        }),
        revision: record.map_or(0, SystemSettingsRecord::revision),
        supported_locales: ["zh-CN", "en-US"],
    }
}

fn public_dto(
    record: Option<&SystemSettingsRecord>,
    theme: Option<&SiteThemeSettingsRecord>,
) -> PublicSystemSettingsDto {
    let fallback = defaults();
    PublicSystemSettingsDto {
        locale: record.map_or(fallback.locale, |value| value.locale().to_owned()),
        site_title: record.map_or(fallback.site_title, |value| value.site_title().to_owned()),
        site_subtitle: record.map_or(fallback.site_subtitle, |value| {
            value.site_subtitle().to_owned()
        }),
        logo_url: record.map_or(fallback.logo_url, |value| value.logo_url().to_owned()),
        icon_url: record.map_or(fallback.icon_url, |value| value.icon_url().to_owned()),
        revision: record.map_or(0, SystemSettingsRecord::revision),
        supported_locales: ["zh-CN", "en-US"],
        theme: public_theme_dto(theme),
    }
}

fn public_theme_dto(record: Option<&SiteThemeSettingsRecord>) -> PublicThemeSettingsDto {
    record.map_or_else(
        || PublicThemeSettingsDto {
            id: DEFAULT_SITE_THEME_ID.to_owned(),
            schema_version: DEFAULT_SITE_THEME_SCHEMA_VERSION,
            options: serde_json::json!({}),
            revision: 0,
        },
        |record| {
            let configuration = record.active_configuration();
            PublicThemeSettingsDto {
                id: record.active_theme_id().to_owned(),
                schema_version: configuration.schema_version(),
                options: configuration.options().clone(),
                revision: record.revision(),
            }
        },
    )
}

fn admin_theme_dto(record: Option<&SiteThemeSettingsRecord>) -> AdminThemeSettingsDto {
    record.map_or_else(
        || AdminThemeSettingsDto {
            active_theme_id: DEFAULT_SITE_THEME_ID.to_owned(),
            configurations: vec![ThemeConfigurationDto {
                theme_id: DEFAULT_SITE_THEME_ID.to_owned(),
                schema_version: DEFAULT_SITE_THEME_SCHEMA_VERSION,
                options: serde_json::json!({}),
            }],
            revision: 0,
        },
        |record| AdminThemeSettingsDto {
            active_theme_id: record.active_theme_id().to_owned(),
            configurations: record
                .configurations()
                .iter()
                .map(|(theme_id, configuration)| ThemeConfigurationDto {
                    theme_id: theme_id.clone(),
                    schema_version: configuration.schema_version(),
                    options: configuration.options().clone(),
                })
                .collect(),
            revision: record.revision(),
        },
    )
}

fn admin_dto(
    record: Option<&SystemSettingsRecord>,
    restart_required: bool,
    invalid_media_browser_root_indexes: Vec<usize>,
) -> AdminSystemSettingsDto {
    let fallback = defaults();
    AdminSystemSettingsDto {
        locale: record.map_or(fallback.locale, |value| value.locale().to_owned()),
        site_title: env::var("TJXY_SERVER_NAME").unwrap_or_else(|_| {
            record.map_or(fallback.site_title, |value| value.site_title().to_owned())
        }),
        site_subtitle: record.map_or(fallback.site_subtitle, |value| {
            value.site_subtitle().to_owned()
        }),
        logo_url: record.map_or(fallback.logo_url, |value| value.logo_url().to_owned()),
        icon_url: record.map_or(fallback.icon_url, |value| value.icon_url().to_owned()),
        public_url: env::var("TJXY_PUBLIC_ADDRESS").ok().or_else(|| {
            record
                .and_then(SystemSettingsRecord::public_url)
                .map(str::to_owned)
        }),
        listen_host: record.map_or(fallback.listen_host, |value| value.listen_host().to_owned()),
        port: record.map_or(fallback.port, SystemSettingsRecord::port),
        media_browser_roots: environment_media_browser_roots().unwrap_or_else(|| {
            record.map_or(fallback.media_browser_roots, |value| {
                value.media_browser_roots().to_vec()
            })
        }),
        invalid_media_browser_root_indexes,
        revision: record.map_or(0, SystemSettingsRecord::revision),
        restart_required,
        environment_overrides: EnvironmentOverridesDto {
            site_title: env::var_os("TJXY_SERVER_NAME").is_some(),
            public_url: env::var_os("TJXY_PUBLIC_ADDRESS").is_some(),
            listen_address: env::var_os("TJXY_BIND").is_some(),
            media_browser_roots: env::var_os("TJXY_MEDIA_BROWSER_ROOTS").is_some(),
        },
        supported_locales: ["zh-CN", "en-US"],
    }
}

async fn invalid_media_browser_root_indexes(record: Option<&SystemSettingsRecord>) -> Vec<usize> {
    let roots = environment_media_browser_roots().unwrap_or_else(|| {
        record.map_or_else(Vec::new, |value| value.media_browser_roots().to_vec())
    });
    let roots = roots.into_iter().map(PathBuf::from).collect::<Vec<_>>();
    let (_, invalid_root_indexes) = FilesystemBrowser::from_available_roots(roots).await;
    invalid_root_indexes
}

fn repository_error(error: &SystemSettingsRepositoryError) -> Response {
    match error {
        SystemSettingsRepositoryError::Conflict => StatusCode::CONFLICT.into_response(),
        SystemSettingsRepositoryError::InvalidLocale
        | SystemSettingsRepositoryError::InvalidBranding
        | SystemSettingsRepositoryError::InvalidPublicUrl
        | SystemSettingsRepositoryError::InvalidListenHost
        | SystemSettingsRepositoryError::InvalidPort
        | SystemSettingsRepositoryError::InvalidMediaBrowserRoots
        | SystemSettingsRepositoryError::InvalidRevision => StatusCode::BAD_REQUEST.into_response(),
        SystemSettingsRepositoryError::Database(_)
        | SystemSettingsRepositoryError::MissingPersistedSettings
        | SystemSettingsRepositoryError::RollbackFailed { .. } => {
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

fn theme_repository_error(error: &SiteThemeSettingsRepositoryError) -> Response {
    match error {
        SiteThemeSettingsRepositoryError::Conflict => StatusCode::CONFLICT.into_response(),
        SiteThemeSettingsRepositoryError::InvalidThemeId
        | SiteThemeSettingsRepositoryError::InvalidSchemaVersion
        | SiteThemeSettingsRepositoryError::InvalidOptions
        | SiteThemeSettingsRepositoryError::InvalidConfigurations
        | SiteThemeSettingsRepositoryError::InvalidRevision => {
            StatusCode::BAD_REQUEST.into_response()
        }
        SiteThemeSettingsRepositoryError::Database(_)
        | SiteThemeSettingsRepositoryError::InvalidPersistedSettings
        | SiteThemeSettingsRepositoryError::MissingPersistedSettings
        | SiteThemeSettingsRepositoryError::RollbackFailed { .. } => {
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

fn environment_media_browser_roots() -> Option<Vec<String>> {
    env::var_os("TJXY_MEDIA_BROWSER_ROOTS").map(|value| {
        env::split_paths(&value)
            .map(|path| path.to_string_lossy().into_owned())
            .collect()
    })
}

pub(crate) async fn persisted_bind_address(
    service: &SystemSettingsService,
) -> Result<Option<SocketAddr>, SystemSettingsRepositoryError> {
    let Some(record) = service.get().await? else {
        return Ok(None);
    };
    Ok(format!("{}:{}", record.listen_host(), record.port())
        .parse()
        .ok())
}

#[cfg(test)]
mod tests {
    use super::{RestartController, validated_image};

    #[tokio::test]
    async fn restart_controller_notifies_waiters_once_requested() {
        let controller = RestartController::default();
        let waiter = controller.clone();
        let pending = tokio::spawn(async move { waiter.requested().await });
        tokio::task::yield_now().await;
        controller.request();
        tokio::time::timeout(std::time::Duration::from_secs(1), pending)
            .await
            .expect("restart notification")
            .expect("restart waiter");
        assert!(controller.is_requested());
    }

    #[test]
    fn image_validation_requires_matching_content_type_and_signature() {
        assert_eq!(
            validated_image(Some("image/png"), b"\x89PNG\r\n\x1a\n"),
            Some(("image/png", "png"))
        );
        assert_eq!(validated_image(Some("image/png"), b"not-an-image"), None);
        assert_eq!(
            validated_image(Some("image/jpeg"), b"\xff\xd8\xfffixture"),
            Some(("image/jpeg", "jpg"))
        );
        assert_eq!(
            validated_image(Some("image/svg+xml"), b"<svg><script /></svg>"),
            None
        );
    }
}
