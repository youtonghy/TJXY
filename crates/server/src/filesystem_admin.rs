use std::path::Path;

use axum::{
    Json,
    extract::{RawQuery, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use tjxy_api::{FilesystemDirectoryEntryDto, FilesystemDirectoryPageDto, FilesystemRootDto};
use tjxy_application::FilesystemBrowserError;
use uuid::Uuid;

use crate::{AppState, auth};

pub(crate) async fn roots(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    if let Err(response) =
        auth::authenticated_administrator(&state, &headers, raw_query.as_deref()).await
    {
        return response;
    }
    let Some(browser) = state.filesystem_browser.as_ref() else {
        return Json(Vec::<FilesystemRootDto>::new()).into_response();
    };
    Json(
        browser
            .roots()
            .into_iter()
            .map(|root| FilesystemRootDto::new(root.id(), root.label()))
            .collect::<Vec<_>>(),
    )
    .into_response()
}

pub(crate) async fn directories(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    if let Err(response) =
        auth::authenticated_administrator(&state, &headers, raw_query.as_deref()).await
    {
        return response;
    }
    let Ok((root_id, path)) = directory_query(raw_query.as_deref()) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Some(browser) = state.filesystem_browser.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match browser.list(root_id, Path::new(&path)).await {
        Ok(page) => Json(FilesystemDirectoryPageDto::new(
            page.entries()
                .iter()
                .map(|entry| {
                    FilesystemDirectoryEntryDto::new(
                        entry.name(),
                        entry.relative_path(),
                        entry.modified_at().map(|value| value.to_rfc3339()),
                    )
                })
                .collect(),
        ))
        .into_response(),
        Err(error) => browser_error_response(&error),
    }
}

fn directory_query(raw_query: Option<&str>) -> Result<(Uuid, String), ()> {
    let query = auth::request_query(raw_query)?;
    let root_id = query
        .get("RootId")
        .ok_or(())?
        .parse::<Uuid>()
        .map_err(|_| ())?;
    Ok((root_id, query.get("Path").cloned().unwrap_or_default()))
}

fn browser_error_response(error: &FilesystemBrowserError) -> Response {
    match error {
        FilesystemBrowserError::UnknownRoot | FilesystemBrowserError::DirectoryUnavailable => {
            StatusCode::NOT_FOUND.into_response()
        }
        FilesystemBrowserError::InvalidRelativePath | FilesystemBrowserError::EscapedRoot => {
            StatusCode::BAD_REQUEST.into_response()
        }
        FilesystemBrowserError::DirectoryLimit
        | FilesystemBrowserError::InvalidDirectoryName
        | FilesystemBrowserError::InvalidRoot { .. }
        | FilesystemBrowserError::DuplicateRoot { .. } => {
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        }
    }
}
