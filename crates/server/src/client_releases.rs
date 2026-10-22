//! Client installer distribution: administrators upload packages and pick the latest release per
//! platform and architecture; signed-in clients check for and download updates.

use std::{
    collections::HashMap,
    path::{Path as FsPath, PathBuf},
};

use axum::{
    Json,
    body::{Body, Bytes},
    extract::{Path, RawQuery, Request, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tjxy_db::{
    ClientArch, ClientPlatform, ClientReleaseInput, ClientReleaseRecord, ClientReleaseRepository,
    ClientReleaseRepositoryError, ClientVersion, client_package_extension,
};
use tokio::io::AsyncWriteExt;
use tower_http::services::ServeFile;
use uuid::Uuid;

use crate::{AppState, auth};

/// Installers are streamed to disk, so the cap only bounds disk usage per upload.
pub(crate) const MAX_CLIENT_PACKAGE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const NOTES_BODY_LIMIT_BYTES: usize = 64 * 1024;

pub(crate) struct ClientReleaseService {
    database: sea_orm::DatabaseConnection,
    directory: PathBuf,
}

impl ClientReleaseService {
    #[must_use]
    pub const fn new(database: sea_orm::DatabaseConnection, directory: PathBuf) -> Self {
        Self {
            database,
            directory,
        }
    }

    fn repository(&self) -> ClientReleaseRepository<'_> {
        ClientReleaseRepository::new(&self.database)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
struct NotesRequest {
    notes: String,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct ClientReleaseDto {
    id: Uuid,
    platform: &'static str,
    arch: &'static str,
    version: String,
    file_name: String,
    size: u64,
    sha256: String,
    notes: String,
    is_latest: bool,
    created_at: DateTime<Utc>,
    download_path: String,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct ClientUpdateCheckDto {
    update_available: bool,
    latest: Option<ClientReleaseDto>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct ClientReleaseListDto {
    items: Vec<ClientReleaseDto>,
}

fn dto(record: &ClientReleaseRecord) -> ClientReleaseDto {
    ClientReleaseDto {
        id: record.id(),
        platform: record.platform().as_str(),
        arch: record.arch().as_str(),
        version: record.version().as_str().to_owned(),
        file_name: record.file_name().to_owned(),
        size: record.size_bytes(),
        sha256: record.sha256().to_owned(),
        notes: record.notes().to_owned(),
        is_latest: record.is_latest(),
        created_at: record.created_at(),
        download_path: format!("/ClientUpdates/{}/Download", record.id()),
    }
}

pub(crate) async fn admin_list(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw): RawQuery,
) -> Response {
    if let Err(response) = auth::authenticated_administrator(&state, &headers, raw.as_deref()).await
    {
        return no_store(response);
    }
    if !clean_query(raw.as_deref()).is_some_and(|query| query.is_empty()) {
        return no_store(StatusCode::BAD_REQUEST.into_response());
    }
    let Some(service) = state.client_releases.as_ref() else {
        return no_store(StatusCode::SERVICE_UNAVAILABLE.into_response());
    };
    match service.repository().list().await {
        Ok(records) => no_store(
            Json(ClientReleaseListDto {
                items: records.iter().map(dto).collect(),
            })
            .into_response(),
        ),
        Err(error) => no_store(error_response(&error)),
    }
}

struct UploadTarget {
    platform: ClientPlatform,
    arch: ClientArch,
    version: ClientVersion,
    file_name: String,
    extension: String,
    mark_latest: bool,
}

fn upload_target(raw: Option<&str>) -> Option<UploadTarget> {
    let mut query = clean_query(raw)?;
    let platform = ClientPlatform::parse(&query.remove("Platform")?)?;
    let arch = ClientArch::parse(&query.remove("Arch")?)?;
    let version = ClientVersion::parse(&query.remove("Version")?)?;
    let file_name = query.remove("FileName")?;
    let extension = client_package_extension(platform, &file_name)?;
    let mark_latest = match query.remove("Latest").as_deref() {
        None | Some("false") => false,
        Some("true") => true,
        Some(_) => return None,
    };
    query.is_empty().then_some(UploadTarget {
        platform,
        arch,
        version,
        file_name,
        extension,
        mark_latest,
    })
}

/// Streams an installer to disk while hashing it, then records it. The body is never buffered in
/// memory, and partial files are removed on every failure path.
pub(crate) async fn admin_upload(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw): RawQuery,
    body: Body,
) -> Response {
    let principal = match auth::authenticated_administrator(&state, &headers, raw.as_deref()).await
    {
        Ok(principal) => principal,
        Err(response) => return no_store(response),
    };
    let Some(target) = upload_target(raw.as_deref()) else {
        return no_store(StatusCode::BAD_REQUEST.into_response());
    };
    let declared_too_large = headers
        .get(header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .is_some_and(|length| length > MAX_CLIENT_PACKAGE_BYTES);
    if declared_too_large {
        return no_store(StatusCode::PAYLOAD_TOO_LARGE.into_response());
    }
    let Some(service) = state.client_releases.as_ref() else {
        return no_store(StatusCode::SERVICE_UNAVAILABLE.into_response());
    };
    let id = Uuid::new_v4();
    let stored_name = format!("{id}.{}", target.extension);
    let path = service.directory.join(&stored_name);
    let (size, sha256) = match receive_package(&service.directory, &path, body).await {
        Ok(received) => received,
        Err(status) => return no_store(status.into_response()),
    };
    let input = ClientReleaseInput::new(
        id,
        target.platform,
        target.arch,
        target.version,
        target.file_name,
        stored_name,
        size,
        sha256,
    )
    .with_creator(principal.user().id())
    .marked_latest(target.mark_latest);
    match service.repository().create(&input).await {
        Ok(record) => no_store((StatusCode::CREATED, Json(dto(&record))).into_response()),
        Err(error) => {
            remove_package(&path).await;
            no_store(error_response(&error))
        }
    }
}

async fn receive_package(
    directory: &FsPath,
    path: &FsPath,
    body: Body,
) -> Result<(u64, String), StatusCode> {
    let internal = |error: std::io::Error| {
        tracing::error!(error = %error, "client package could not be stored");
        StatusCode::INTERNAL_SERVER_ERROR
    };
    tokio::fs::create_dir_all(directory)
        .await
        .map_err(internal)?;
    let temporary = path.with_extension("upload");
    let result = write_package(&temporary, body).await;
    let received = match result {
        Ok(received) => received,
        Err(error) => {
            remove_package(&temporary).await;
            return Err(match error {
                PackageWriteError::Io(error) => internal(error),
                PackageWriteError::Status(status) => status,
            });
        }
    };
    if let Err(error) = tokio::fs::rename(&temporary, path).await {
        remove_package(&temporary).await;
        return Err(internal(error));
    }
    Ok(received)
}

enum PackageWriteError {
    Io(std::io::Error),
    Status(StatusCode),
}

impl From<std::io::Error> for PackageWriteError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

async fn write_package(path: &FsPath, body: Body) -> Result<(u64, String), PackageWriteError> {
    let mut file = tokio::fs::File::create(path).await?;
    let mut hasher = Sha256::new();
    let mut size = 0_u64;
    let mut stream = body.into_data_stream();
    while let Some(chunk) = stream.next().await {
        let chunk: Bytes = chunk.map_err(|_| PackageWriteError::Status(StatusCode::BAD_REQUEST))?;
        size = size.saturating_add(chunk.len() as u64);
        if size > MAX_CLIENT_PACKAGE_BYTES {
            return Err(PackageWriteError::Status(StatusCode::PAYLOAD_TOO_LARGE));
        }
        hasher.update(&chunk);
        file.write_all(&chunk).await?;
    }
    if size == 0 {
        return Err(PackageWriteError::Status(StatusCode::BAD_REQUEST));
    }
    file.flush().await?;
    file.sync_all().await?;
    Ok((size, format!("{:x}", hasher.finalize())))
}

async fn remove_package(path: &FsPath) {
    match tokio::fs::remove_file(path).await {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            tracing::warn!(error = %error, path = %path.display(), "client package could not be removed");
        }
    }
}

pub(crate) async fn admin_update_notes(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    RawQuery(raw): RawQuery,
    body: Bytes,
) -> Response {
    if let Err(response) = auth::authenticated_administrator(&state, &headers, raw.as_deref()).await
    {
        return no_store(response);
    }
    if !clean_query(raw.as_deref()).is_some_and(|query| query.is_empty())
        || body.len() > NOTES_BODY_LIMIT_BYTES
    {
        return no_store(StatusCode::BAD_REQUEST.into_response());
    }
    let Ok(request) = serde_json::from_slice::<NotesRequest>(&body) else {
        return no_store(StatusCode::BAD_REQUEST.into_response());
    };
    let Some(service) = state.client_releases.as_ref() else {
        return no_store(StatusCode::SERVICE_UNAVAILABLE.into_response());
    };
    match service.repository().update_notes(id, &request.notes).await {
        Ok(record) => no_store(Json(dto(&record)).into_response()),
        Err(error) => no_store(error_response(&error)),
    }
}

pub(crate) async fn admin_mark_latest(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    RawQuery(raw): RawQuery,
) -> Response {
    if let Err(response) = auth::authenticated_administrator(&state, &headers, raw.as_deref()).await
    {
        return no_store(response);
    }
    if !clean_query(raw.as_deref()).is_some_and(|query| query.is_empty()) {
        return no_store(StatusCode::BAD_REQUEST.into_response());
    }
    let Some(service) = state.client_releases.as_ref() else {
        return no_store(StatusCode::SERVICE_UNAVAILABLE.into_response());
    };
    match service.repository().mark_latest(id).await {
        Ok(record) => no_store(Json(dto(&record)).into_response()),
        Err(error) => no_store(error_response(&error)),
    }
}

pub(crate) async fn admin_delete(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    RawQuery(raw): RawQuery,
) -> Response {
    if let Err(response) = auth::authenticated_administrator(&state, &headers, raw.as_deref()).await
    {
        return no_store(response);
    }
    if !clean_query(raw.as_deref()).is_some_and(|query| query.is_empty()) {
        return no_store(StatusCode::BAD_REQUEST.into_response());
    }
    let Some(service) = state.client_releases.as_ref() else {
        return no_store(StatusCode::SERVICE_UNAVAILABLE.into_response());
    };
    match service.repository().delete(id).await {
        Ok(record) => {
            remove_package(&service.directory.join(record.stored_name())).await;
            no_store(StatusCode::NO_CONTENT.into_response())
        }
        Err(error) => no_store(error_response(&error)),
    }
}

/// Reports the release a client should run. `CurrentVersion` is compared by `SemVer` precedence,
/// so an administrator rolling the latest pointer back never prompts a downgrade.
pub(crate) async fn check(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw): RawQuery,
) -> Response {
    if let Err(response) = auth::authenticated_principal(&state, &headers, raw.as_deref()).await {
        return no_store(response);
    }
    let Some((platform, arch, current)) = check_target(raw.as_deref()) else {
        return no_store(StatusCode::BAD_REQUEST.into_response());
    };
    let Some(service) = state.client_releases.as_ref() else {
        return no_store(StatusCode::SERVICE_UNAVAILABLE.into_response());
    };
    match service.repository().latest_for(platform, arch).await {
        Ok(latest) => no_store(
            Json(ClientUpdateCheckDto {
                update_available: latest
                    .as_ref()
                    .is_some_and(|record| record.version() > &current),
                latest: latest.as_ref().map(dto),
            })
            .into_response(),
        ),
        Err(error) => no_store(error_response(&error)),
    }
}

fn check_target(raw: Option<&str>) -> Option<(ClientPlatform, ClientArch, ClientVersion)> {
    let mut query = clean_query(raw)?;
    let platform = ClientPlatform::parse(&query.remove("Platform")?)?;
    let arch = ClientArch::parse(&query.remove("Arch")?)?;
    let current = ClientVersion::parse(&query.remove("CurrentVersion")?)?;
    query.is_empty().then_some((platform, arch, current))
}

/// Serves a package with `Range` support so interrupted downloads can resume.
pub(crate) async fn download(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    RawQuery(raw): RawQuery,
    request: Request,
) -> Response {
    if let Err(response) = auth::authenticated_principal(&state, &headers, raw.as_deref()).await {
        return response;
    }
    let Some(service) = state.client_releases.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let record = match service.repository().get(id).await {
        Ok(record) => record,
        Err(error) => return error_response(&error),
    };
    let path = service.directory.join(record.stored_name());
    let mut response = match ServeFile::new(&path).try_call(request).await {
        Ok(response) => response.map(Body::new),
        Err(error) => {
            tracing::error!(error = %error, release = %id, "client package could not be read");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    if response.status() == StatusCode::NOT_FOUND {
        tracing::error!(release = %id, path = %path.display(), "client package file is missing");
        return response;
    }
    let content_type = if record.platform() == ClientPlatform::Android {
        "application/vnd.android.package-archive"
    } else {
        "application/octet-stream"
    };
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    // File names are validated to a header-safe character set when uploaded.
    if let Ok(disposition) =
        HeaderValue::from_str(&format!("attachment; filename=\"{}\"", record.file_name()))
    {
        headers.insert(header::CONTENT_DISPOSITION, disposition);
    }
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-cache"),
    );
    response
}

fn clean_query(raw: Option<&str>) -> Option<HashMap<String, String>> {
    let mut query = auth::request_query(raw).ok()?;
    query.remove("ApiKey");
    query.remove("api_key");
    Some(query)
}

fn error_response(error: &ClientReleaseRepositoryError) -> Response {
    match error {
        ClientReleaseRepositoryError::InvalidInput => StatusCode::BAD_REQUEST.into_response(),
        ClientReleaseRepositoryError::NotFound => StatusCode::NOT_FOUND.into_response(),
        ClientReleaseRepositoryError::VersionConflict => StatusCode::CONFLICT.into_response(),
        _ => {
            tracing::error!(error = %error, "client release operation failed");
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        }
    }
}

fn no_store(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}
