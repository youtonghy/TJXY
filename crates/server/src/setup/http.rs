use std::{
    collections::VecDeque,
    convert::Infallible,
    net::{IpAddr, SocketAddr},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use axum::{
    Json, Router,
    body::Bytes,
    extract::{ConnectInfo, DefaultBodyLimit, Path as AxumPath, Query, Request, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    middleware::{self, Next},
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
    routing::{get, post, put},
};
use serde::{Deserialize, Serialize};
use url::Url;
use uuid::Uuid;

use crate::{
    CompleteSetupInput, DatabaseDraft, NetworkConfiguration, SetupCoordinator, SetupErrorCode,
    SetupState, SetupValidator,
};

const SESSION_COOKIE: &str = "tjxy_setup_session";
const CSRF_HEADER: &str = "x-tjxy-setup-csrf";
const MAX_SESSIONS: usize = 64;
const MAX_JSON_BODY_BYTES: usize = 64 * 1024;
const SESSION_LIFETIME: Duration = Duration::from_secs(30 * 60);
const MUTATION_WINDOW: Duration = Duration::from_secs(60);
const MAX_MUTATIONS_PER_WINDOW: usize = 60;

#[derive(Clone)]
struct SetupHttpState {
    coordinator: SetupCoordinator,
    validator: SetupValidator,
    sessions: Arc<Mutex<VecDeque<SetupSession>>>,
    branding_asset_dir: Arc<PathBuf>,
    managed_database: Option<DatabaseDraft>,
}

#[derive(Clone)]
struct SetupSession {
    id: String,
    csrf: String,
    installation_id: Uuid,
    created_at: Instant,
    mutation_attempts: VecDeque<Instant>,
    tested_database: Option<DatabaseDraft>,
}

pub fn build_setup_router(coordinator: SetupCoordinator, validator: SetupValidator) -> Router {
    build_setup_router_with_asset_dir(coordinator, validator, Path::new("./data/assets/branding"))
}

pub fn build_setup_router_with_asset_dir(
    coordinator: SetupCoordinator,
    validator: SetupValidator,
    branding_asset_dir: impl Into<PathBuf>,
) -> Router {
    build_setup_router_with_options(coordinator, validator, branding_asset_dir, None)
}

pub fn build_setup_router_with_options(
    coordinator: SetupCoordinator,
    validator: SetupValidator,
    branding_asset_dir: impl Into<PathBuf>,
    managed_database: Option<DatabaseDraft>,
) -> Router {
    Router::new()
        .route("/health/live", get(|| async { "live" }))
        .route("/health/ready", get(|| async { "setup" }))
        .route("/Setup/Status", get(status))
        .route("/Setup/Database/Test", post(test_database))
        .route("/Setup/Network/Validate", post(validate_network))
        .route(
            "/Setup/Branding/{kind}",
            put(upload_branding).layer(DefaultBodyLimit::max(
                crate::system_settings::MAX_BRAND_ASSET_BYTES,
            )),
        )
        .route("/Setup/Complete", post(complete))
        .route("/Setup/Recover", post(recover))
        .route("/Setup/Progress", get(progress))
        .layer(DefaultBodyLimit::max(MAX_JSON_BODY_BYTES))
        .layer(middleware::from_fn(add_no_store))
        .with_state(SetupHttpState {
            coordinator,
            validator,
            sessions: Arc::new(Mutex::new(VecDeque::new())),
            branding_asset_dir: Arc::new(branding_asset_dir.into()),
            managed_database,
        })
}

async fn add_no_store(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    no_store(response.headers_mut());
    response
}

async fn upload_branding(
    State(state): State<SetupHttpState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    AxumPath(kind): AxumPath<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if !is_private_source(peer.ip()) {
        return StatusCode::FORBIDDEN.into_response();
    }
    if let Err(status) = valid_csrf(&state, &headers) {
        return status.into_response();
    }
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok());
    match crate::system_settings::store_brand_asset(
        &state.branding_asset_dir,
        &kind,
        content_type,
        &body,
    )
    .await
    {
        Ok(asset_url) => {
            let mut response = Json(BrandingUploadResponse { asset_url }).into_response();
            no_store(response.headers_mut());
            response
        }
        Err(crate::system_settings::AssetUploadError::TooLarge) => {
            StatusCode::PAYLOAD_TOO_LARGE.into_response()
        }
        Err(crate::system_settings::AssetUploadError::Invalid) => {
            setup_error(SetupErrorCode::BrandingInvalid)
        }
        Err(crate::system_settings::AssetUploadError::Io(_)) => {
            setup_error(SetupErrorCode::BrandingWriteFailed)
        }
    }
}

async fn validate_network(
    State(state): State<SetupHttpState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(request): Json<NetworkRequest>,
) -> Response {
    if !is_private_source(peer.ip()) {
        return StatusCode::FORBIDDEN.into_response();
    }
    if let Err(status) = valid_csrf(&state, &headers) {
        return status.into_response();
    }
    let network = match request.validate() {
        Ok(network) => network,
        Err(code) => return setup_error(code),
    };
    let mut response = Json(NetworkValidationResponse {
        listen_host: network.listen_host(),
        port: network.port(),
        public_url: network.public_url(),
        destination_url: network.admin_login_url(),
    })
    .into_response();
    no_store(response.headers_mut());
    response
}

async fn progress(
    State(state): State<SetupHttpState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Query(query): Query<ProgressQuery>,
) -> Response {
    if !is_private_source(peer.ip()) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Some(session) = valid_session(&state, &headers) else {
        return StatusCode::FORBIDDEN.into_response();
    };
    if session.installation_id != query.installation_id {
        return StatusCode::FORBIDDEN.into_response();
    }
    let mut progress = state.coordinator.subscribe_progress();
    let installation_id = query.installation_id;
    let latest = state.coordinator.latest_progress(installation_id);
    let stream = async_stream::stream! {
        let mut last_stage = None;
        if let Some(update) = latest {
            let event = Event::default().event("stage").json_data(update)
                .unwrap_or_else(|_| Event::default().event("error").data("invalid-progress"));
            last_stage = Some(update.stage);
            yield Ok::<Event, Infallible>(event);
            if matches!(update.stage, crate::SetupProgressStage::Complete | crate::SetupProgressStage::Failed) {
                return;
            }
        }
        loop {
            match progress.recv().await {
                Ok(update) if update.installation_id == installation_id && last_stage != Some(update.stage) => {
                    let event = Event::default().event("stage").json_data(update)
                        .unwrap_or_else(|_| Event::default().event("error").data("invalid-progress"));
                    last_stage = Some(update.stage);
                    yield Ok::<Event, Infallible>(event);
                    if matches!(update.stage, crate::SetupProgressStage::Complete | crate::SetupProgressStage::Failed) {
                        break;
                    }
                }
                Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    };
    let mut response = Sse::new(stream)
        .keep_alive(
            KeepAlive::new()
                .interval(Duration::from_secs(15))
                .text("keep-alive"),
        )
        .into_response();
    no_store(response.headers_mut());
    response
}

async fn recover(
    State(state): State<SetupHttpState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(request): Json<RecoverSetupRequest>,
) -> Response {
    if !is_private_source(peer.ip()) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let session = match valid_csrf(&state, &headers) {
        Ok(session) => session,
        Err(status) => return status.into_response(),
    };
    match state
        .coordinator
        .recover(
            session.installation_id,
            &request.administrator_username,
            &request.administrator_password,
        )
        .await
    {
        Ok(completion) => completion_response(&completion),
        Err(error) => setup_error(error.code()),
    }
}

async fn complete(
    State(state): State<SetupHttpState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(request): Json<CompleteSetupRequest>,
) -> Response {
    if !is_private_source(peer.ip()) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let session = match valid_csrf(&state, &headers) {
        Ok(session) => session,
        Err(status) => return status.into_response(),
    };
    if request.validate_profile().is_err() {
        return setup_error(SetupErrorCode::SystemSettingsInvalid);
    }
    let database = match (state.managed_database.as_ref(), request.database) {
        (Some(managed), None) if session.tested_database.as_ref() == Some(managed) => {
            managed.clone()
        }
        (None, Some(database)) if session.tested_database.as_ref() == Some(&database) => database,
        _ => return setup_error(SetupErrorCode::DatabaseConfigurationInvalid),
    };
    let network = match request.network.validate() {
        Ok(network) => network,
        Err(code) => return setup_error(code),
    };
    match state
        .coordinator
        .complete(
            CompleteSetupInput::new(
                request.site_title,
                request.site_subtitle,
                request.locale,
                request.logo_url,
                request.icon_url,
                database,
                network,
                request.administrator_username,
                request.administrator_password,
            )
            .with_installation_id(session.installation_id),
        )
        .await
    {
        Ok(completion) => completion_response(&completion),
        Err(error) => setup_error(error.code()),
    }
}

fn completion_response(completion: &crate::SetupCompletion) -> Response {
    let mut response = Json(CompleteSetupResponse {
        destination_url: completion.destination_url(),
    })
    .into_response();
    no_store(response.headers_mut());
    response
}

async fn status(
    State(state): State<SetupHttpState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
) -> Response {
    if !is_private_source(peer.ip()) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let setup_status = match state.coordinator.status() {
        Ok(status) => status,
        Err(error) => return setup_error(error.code()),
    };
    let tested_database = if let Some(database) = state.managed_database.as_ref() {
        if let Err(error) = state.validator.test_database(database).await {
            return setup_error(error.code());
        }
        Some(database.clone())
    } else {
        None
    };
    let session = SetupSession {
        id: Uuid::new_v4().to_string(),
        csrf: format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple()),
        installation_id: setup_status.installation_id(),
        created_at: Instant::now(),
        mutation_attempts: VecDeque::new(),
        tested_database,
    };
    let Ok(mut sessions) = state.sessions.lock() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    if sessions.len() >= MAX_SESSIONS {
        sessions.pop_front();
    }
    sessions.push_back(session.clone());
    drop(sessions);
    let mut response = Json(SetupStatusResponse {
        state: match setup_status.state() {
            SetupState::Unconfigured => "unconfigured",
            SetupState::Pending => "pending",
        },
        installation_id: setup_status.installation_id(),
        csrf_token: &session.csrf,
        database_backends: ["sqlite", "postgresql", "mysql"],
        deployment_mode: if std::env::var("TJXY_CONTAINER").as_deref() == Ok("true") {
            "container"
        } else {
            "native"
        },
        version: env!("CARGO_PKG_VERSION"),
        configuration_writable: state.coordinator.configuration_writable(),
        source_eligible: true,
        blocking_overrides: blocking_environment_overrides(),
        managed_database_backend: state.managed_database.as_ref().map(database_backend_name),
    })
    .into_response();
    no_store(response.headers_mut());
    let cookie = format!(
        "{SESSION_COOKIE}={}; Path=/Setup; HttpOnly; SameSite=Strict",
        session.id
    );
    if let Ok(value) = HeaderValue::from_str(&cookie) {
        response.headers_mut().insert(header::SET_COOKIE, value);
    }
    response
}

fn database_backend_name(database: &DatabaseDraft) -> &'static str {
    match database {
        DatabaseDraft::Sqlite { .. } => "sqlite",
        DatabaseDraft::PostgreSql { .. } => "postgresql",
        DatabaseDraft::Mysql { .. } => "mysql",
    }
}

fn blocking_environment_overrides() -> Vec<&'static str> {
    [
        "TJXY_DATABASE_URL",
        "TJXY_SERVER_ID",
        "TJXY_CREDENTIAL_KEYRING",
        "TJXY_BIND",
        "TJXY_PUBLIC_ADDRESS",
    ]
    .into_iter()
    .filter(|name| std::env::var_os(name).is_some())
    .collect()
}

async fn test_database(
    State(state): State<SetupHttpState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(draft): Json<DatabaseDraft>,
) -> Response {
    if !is_private_source(peer.ip()) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let session = match valid_csrf(&state, &headers) {
        Ok(session) => session,
        Err(status) => return status.into_response(),
    };
    match state.validator.test_database(&draft).await {
        Ok(result) => {
            if remember_tested_database(&state, &session.id, draft).is_err() {
                return StatusCode::SERVICE_UNAVAILABLE.into_response();
            }
            let mut response = Json(DatabaseTestResponse {
                backend: result.backend(),
                version: result.version(),
                elapsed_milliseconds: result.elapsed_milliseconds(),
            })
            .into_response();
            no_store(response.headers_mut());
            response
        }
        Err(error) => setup_error(error.code()),
    }
}

fn remember_tested_database(
    state: &SetupHttpState,
    session_id: &str,
    draft: DatabaseDraft,
) -> Result<(), ()> {
    let mut sessions = state.sessions.lock().map_err(|_| ())?;
    let session = sessions
        .iter_mut()
        .find(|session| constant_time_equal(session.id.as_bytes(), session_id.as_bytes()))
        .ok_or(())?;
    session.tested_database = Some(draft);
    Ok(())
}

fn valid_csrf(state: &SetupHttpState, headers: &HeaderMap) -> Result<SetupSession, StatusCode> {
    let session_id = session_id(headers).ok_or(StatusCode::FORBIDDEN)?;
    let csrf = headers
        .get(CSRF_HEADER)
        .and_then(|value| value.to_str().ok())
        .ok_or(StatusCode::FORBIDDEN)?;
    let mut sessions = state
        .sessions
        .lock()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    sessions.retain(|session| session.created_at.elapsed() <= SESSION_LIFETIME);
    let session = sessions
        .iter_mut()
        .find(|session| constant_time_equal(session.id.as_bytes(), session_id.as_bytes()))
        .ok_or(StatusCode::FORBIDDEN)?;
    if !constant_time_equal(session.csrf.as_bytes(), csrf.as_bytes()) {
        return Err(StatusCode::FORBIDDEN);
    }
    session
        .mutation_attempts
        .retain(|attempt| attempt.elapsed() <= MUTATION_WINDOW);
    if session.mutation_attempts.len() >= MAX_MUTATIONS_PER_WINDOW {
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }
    session.mutation_attempts.push_back(Instant::now());
    Ok(session.clone())
}

fn valid_session(state: &SetupHttpState, headers: &HeaderMap) -> Option<SetupSession> {
    let session_id = session_id(headers)?;
    let Ok(mut sessions) = state.sessions.lock() else {
        return None;
    };
    sessions.retain(|session| session.created_at.elapsed() <= SESSION_LIFETIME);
    sessions
        .iter()
        .find(|session| constant_time_equal(session.id.as_bytes(), session_id.as_bytes()))
        .cloned()
}

fn session_id(headers: &HeaderMap) -> Option<&str> {
    let cookie = headers
        .get(header::COOKIE)
        .and_then(|value| value.to_str().ok())?;
    cookie.split(';').find_map(|part| {
        let (name, value) = part.trim().split_once('=')?;
        (name == SESSION_COOKIE).then_some(value)
    })
}

fn constant_time_equal(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        })
        == 0
}

fn is_private_source(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => {
            address.is_loopback() || address.is_private() || address.is_link_local()
        }
        IpAddr::V6(address) => {
            address.is_loopback() || address.is_unique_local() || address.is_unicast_link_local()
        }
    }
}

fn setup_error(code: SetupErrorCode) -> Response {
    let status = match code {
        SetupErrorCode::InstallationConflict | SetupErrorCode::AdministratorExists => {
            StatusCode::CONFLICT
        }
        SetupErrorCode::DatabaseUnavailable
        | SetupErrorCode::ConfigurationReadFailed
        | SetupErrorCode::ConfigurationWriteFailed
        | SetupErrorCode::BrandingWriteFailed
        | SetupErrorCode::InstallationFailed => StatusCode::SERVICE_UNAVAILABLE,
        SetupErrorCode::RecoveryAuthenticationFailed => StatusCode::FORBIDDEN,
        SetupErrorCode::UnsafeDatabasePath
        | SetupErrorCode::DatabaseConfigurationInvalid
        | SetupErrorCode::DatabaseResponseInvalid
        | SetupErrorCode::AdministratorInvalid
        | SetupErrorCode::SystemSettingsInvalid
        | SetupErrorCode::NetworkInvalid
        | SetupErrorCode::BrandingInvalid => StatusCode::BAD_REQUEST,
    };
    let mut response = (status, Json(SetupErrorResponse { code })).into_response();
    no_store(response.headers_mut());
    response
}

fn no_store(headers: &mut HeaderMap) {
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct SetupStatusResponse<'a> {
    state: &'static str,
    installation_id: Uuid,
    csrf_token: &'a str,
    database_backends: [&'static str; 3],
    deployment_mode: &'static str,
    version: &'static str,
    configuration_writable: bool,
    source_eligible: bool,
    blocking_overrides: Vec<&'static str>,
    managed_database_backend: Option<&'static str>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct DatabaseTestResponse<'a> {
    backend: crate::DatabaseBackend,
    version: &'a str,
    elapsed_milliseconds: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct NetworkValidationResponse<'a> {
    listen_host: &'a str,
    port: u16,
    public_url: Option<&'a str>,
    destination_url: String,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct BrandingUploadResponse {
    asset_url: String,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct SetupErrorResponse {
    code: SetupErrorCode,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
struct CompleteSetupRequest {
    site_title: String,
    site_subtitle: String,
    locale: String,
    logo_url: String,
    icon_url: String,
    database: Option<DatabaseDraft>,
    network: NetworkRequest,
    administrator_username: String,
    administrator_password: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
struct RecoverSetupRequest {
    administrator_username: String,
    administrator_password: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProgressQuery {
    installation_id: Uuid,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
struct NetworkRequest {
    listen_host: String,
    port: u16,
    public_url: Option<String>,
}

impl NetworkRequest {
    fn validate(self) -> Result<NetworkConfiguration, SetupErrorCode> {
        if self.listen_host.len() > 64
            || self.listen_host.parse::<IpAddr>().is_err()
            || self.port == 0
        {
            return Err(SetupErrorCode::NetworkInvalid);
        }
        if let Some(public_url) = self.public_url.as_deref() {
            if public_url.len() > 2_048 {
                return Err(SetupErrorCode::NetworkInvalid);
            }
            let url = Url::parse(public_url).map_err(|_| SetupErrorCode::NetworkInvalid)?;
            if !matches!(url.scheme(), "http" | "https")
                || url.host_str().is_none()
                || !url.username().is_empty()
                || url.password().is_some()
                || url.path() != "/"
                || url.query().is_some()
                || url.fragment().is_some()
            {
                return Err(SetupErrorCode::NetworkInvalid);
            }
        }
        Ok(NetworkConfiguration::new(
            self.listen_host,
            self.port,
            self.public_url,
        ))
    }
}

impl CompleteSetupRequest {
    fn validate_profile(&self) -> Result<(), SetupErrorCode> {
        let title_length = self.site_title.trim().chars().count();
        if title_length == 0
            || title_length > 120
            || self.site_subtitle.chars().count() > 240
            || !matches!(self.locale.as_str(), "zh-CN" | "en-US")
            || !crate::installation_config::valid_brand_asset_url("logo", &self.logo_url)
            || !crate::installation_config::valid_brand_asset_url("icon", &self.icon_url)
        {
            return Err(SetupErrorCode::SystemSettingsInvalid);
        }
        Ok(())
    }
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct CompleteSetupResponse<'a> {
    destination_url: &'a str,
}
