use std::{
    io::SeekFrom,
    path::{Path, PathBuf},
};

use axum::{
    Json,
    body::Body,
    extract::{Path as AxumPath, RawQuery, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use tjxy_db::{
    DEFAULT_LOG_RETENTION_DAYS, LogMode, LoggingSettingsInput, LoggingSettingsRepository,
    LoggingSettingsRepositoryError,
};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;

use crate::{AppState, auth, logging_runtime::log_file_date};

const MAX_READ_BYTES: u64 = 256 * 1024;

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct LoggingSettingsDto {
    mode: &'static str,
    retention_days: u16,
    revision: i64,
    directory: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub(crate) struct UpdateLoggingSettingsRequest {
    mode: String,
    retention_days: u16,
    revision: Option<i64>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct LogFileDto {
    date: String,
    size_bytes: u64,
    current: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct LogPageDto {
    date: String,
    lines: Vec<String>,
    offset: u64,
    next_offset: u64,
    size_bytes: u64,
    has_older: bool,
}

pub(crate) async fn get_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    if let Err(response) = administrator(&state, &headers, query.as_deref()).await {
        return response;
    }
    settings_response(&state).await
}

pub(crate) async fn put_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    Json(request): Json<UpdateLoggingSettingsRequest>,
) -> Response {
    if let Err(response) = administrator(&state, &headers, query.as_deref()).await {
        return response;
    }
    let Some(runtime) = state.logging_runtime.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let Some(system) = state.system_settings.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let Ok(mode) = request.mode.parse::<LogMode>() else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let repository = LoggingSettingsRepository::new(system.database());
    match repository
        .put(
            LoggingSettingsInput {
                mode,
                retention_days: request.retention_days,
            },
            request.revision,
        )
        .await
    {
        Ok(record) => {
            if runtime.set_mode(record.mode()).is_err()
                || runtime.cleanup(record.retention_days()).is_err()
            {
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
            Json(settings_dto(&record, runtime.directory())).into_response()
        }
        Err(LoggingSettingsRepositoryError::Conflict) => StatusCode::CONFLICT.into_response(),
        Err(
            LoggingSettingsRepositoryError::InvalidMode
            | LoggingSettingsRepositoryError::InvalidRetentionDays,
        ) => StatusCode::BAD_REQUEST.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(crate) async fn list_files(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    if let Err(response) = administrator(&state, &headers, query.as_deref()).await {
        return response;
    }
    let Some(runtime) = state.logging_runtime.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match collect_files(runtime.directory()).await {
        Ok(files) => Json(files).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(crate) async fn read_file(
    State(state): State<AppState>,
    AxumPath(date): AxumPath<String>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    if let Err(response) = administrator(&state, &headers, raw_query.as_deref()).await {
        return response;
    }
    let Some(runtime) = state.logging_runtime.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let Some(path) = file_path(runtime.directory(), &date) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Ok(mut query) = auth::request_query(raw_query.as_deref()) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    query.remove("ApiKey");
    query.remove("api_key");
    let before = match query.remove("Before") {
        Some(value) => match value.parse::<u64>() {
            Ok(value) => Some(value),
            Err(_) => return StatusCode::BAD_REQUEST.into_response(),
        },
        None => None,
    };
    if !query.is_empty() {
        return StatusCode::BAD_REQUEST.into_response();
    }
    match read_page(&path, &date, before).await {
        Ok(page) => Json(page).into_response(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            StatusCode::NOT_FOUND.into_response()
        }
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(crate) async fn download_file(
    State(state): State<AppState>,
    AxumPath(date): AxumPath<String>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    if let Err(response) = administrator(&state, &headers, query.as_deref()).await {
        return response;
    }
    let Some(runtime) = state.logging_runtime.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let Some(path) = file_path(runtime.directory(), &date) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let file = match tokio::fs::File::open(path).await {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return StatusCode::NOT_FOUND.into_response();
        }
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    (
        [
            (header::CONTENT_TYPE, "application/x-ndjson".to_owned()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"tjxy.{date}.log\""),
            ),
        ],
        Body::from_stream(ReaderStream::new(file)),
    )
        .into_response()
}

async fn settings_response(state: &AppState) -> Response {
    let Some(runtime) = state.logging_runtime.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let Some(system) = state.system_settings.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match LoggingSettingsRepository::new(system.database())
        .get()
        .await
    {
        Ok(Some(record)) => Json(settings_dto(&record, runtime.directory())).into_response(),
        Ok(None) => Json(LoggingSettingsDto {
            mode: LogMode::Error.as_str(),
            retention_days: DEFAULT_LOG_RETENTION_DAYS,
            revision: 0,
            directory: runtime.directory().display().to_string(),
        })
        .into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

fn settings_dto(record: &tjxy_db::LoggingSettingsRecord, directory: &Path) -> LoggingSettingsDto {
    LoggingSettingsDto {
        mode: record.mode().as_str(),
        retention_days: record.retention_days(),
        revision: record.revision(),
        directory: directory.display().to_string(),
    }
}

async fn collect_files(directory: &Path) -> std::io::Result<Vec<LogFileDto>> {
    let mut entries = tokio::fs::read_dir(directory).await?;
    let today = chrono::Utc::now().date_naive();
    let mut files = Vec::new();
    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(date) = log_file_date(&name) else {
            continue;
        };
        let metadata = entry.metadata().await?;
        if metadata.is_file() {
            files.push(LogFileDto {
                date: date.to_string(),
                size_bytes: metadata.len(),
                current: date == today,
            });
        }
    }
    files.sort_by(|left, right| right.date.cmp(&left.date));
    Ok(files)
}

fn file_path(directory: &Path, date: &str) -> Option<PathBuf> {
    let parsed = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
    if parsed.to_string() != date {
        return None;
    }
    Some(directory.join(format!("tjxy.{date}.log")))
}

async fn read_page(path: &Path, date: &str, before: Option<u64>) -> std::io::Result<LogPageDto> {
    let mut file = tokio::fs::File::open(path).await?;
    let size = file.metadata().await?.len();
    let end = before.unwrap_or(size).min(size);
    let requested_offset = end.saturating_sub(MAX_READ_BYTES);
    file.seek(SeekFrom::Start(requested_offset)).await?;
    let mut bytes = vec![0; usize::try_from(end - requested_offset).unwrap_or(0)];
    file.read_exact(&mut bytes).await?;
    let mut text = String::from_utf8_lossy(&bytes).into_owned();
    let offset = if requested_offset > 0 {
        match text.find('\n') {
            Some(newline) => {
                text.drain(..=newline);
                requested_offset + u64::try_from(newline).unwrap_or(0) + 1
            }
            None => end,
        }
    } else {
        requested_offset
    };
    let lines = text
        .lines()
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect();
    Ok(LogPageDto {
        date: date.to_owned(),
        lines,
        offset,
        next_offset: end,
        size_bytes: size,
        has_older: offset > 0 && offset < end,
    })
}

async fn administrator(
    state: &AppState,
    headers: &HeaderMap,
    query: Option<&str>,
) -> Result<(), Response> {
    auth::authenticated_administrator(state, headers, query)
        .await
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::{MAX_READ_BYTES, file_path, read_page};

    #[test]
    fn file_path_accepts_only_canonical_dates() {
        let directory = std::path::Path::new("logs");
        assert_eq!(
            file_path(directory, "2026-08-13"),
            Some(directory.join("tjxy.2026-08-13.log"))
        );
        assert_eq!(file_path(directory, "2026-8-13"), None);
        assert_eq!(file_path(directory, "../2026-08-13"), None);
    }

    #[tokio::test]
    async fn pages_begin_at_complete_json_lines() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("tjxy.2026-08-13.log");
        let first = format!("{{\"message\":\"{}\"}}\n", "a".repeat(1024));
        let second = format!(
            "{{\"message\":\"{}\"}}\n",
            "b".repeat(usize::try_from(MAX_READ_BYTES - 512).unwrap())
        );
        let third = "{\"message\":\"latest\"}\n";
        std::fs::write(&path, format!("{first}{second}{third}")).unwrap();

        let latest = read_page(&path, "2026-08-13", None).await.unwrap();
        assert_eq!(latest.lines.len(), 2);
        assert!(latest.lines[0].contains("bbb"));
        assert_eq!(latest.offset, u64::try_from(first.len()).unwrap());
        assert!(latest.has_older);

        let older = read_page(&path, "2026-08-13", Some(latest.offset))
            .await
            .unwrap();
        assert_eq!(older.lines, vec![first.trim_end()]);
        assert!(!older.has_older);
    }
}
