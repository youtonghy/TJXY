use std::{
    path::{Component, Path, PathBuf},
    sync::Arc,
};

use axum::{
    Json,
    body::Bytes,
    extract::{Path as RoutePath, RawQuery, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Serialize;
use tjxy_api::UpdateLibraryFolderPathDto;
use tjxy_application::FilesystemBrowser;
use tjxy_common::{LibraryId, StorageRootId};
use tjxy_storage::StorageObjectId;
use tjxy_storage_filesystem::FilesystemBackend;
use uuid::Uuid;

use crate::{AppState, auth, library::library_error_response};

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct FolderDto {
    id: Uuid,
    name: String,
    path: Option<String>,
    provider: String,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct ContentsDto {
    items: Vec<EntryDto>,
    indexed: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct EntryDto {
    name: String,
    path: String,
    is_directory: bool,
    size: Option<u64>,
    modified_at: Option<String>,
}

pub(crate) async fn folders(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    RoutePath(library_id): RoutePath<Uuid>,
) -> Response {
    if let Err(response) =
        auth::authenticated_administrator(&state, &headers, query.as_deref()).await
    {
        return response;
    }
    let Some(libraries) = state.libraries.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match libraries.folders(library_id).await {
        Ok(folders) => Json(
            folders
                .into_iter()
                .map(|folder| FolderDto {
                    id: folder.id,
                    name: folder.name,
                    path: folder.path,
                    provider: folder.provider,
                })
                .collect::<Vec<_>>(),
        )
        .into_response(),
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}

pub(crate) async fn update_folder(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    RoutePath((library_id, root_id)): RoutePath<(Uuid, Uuid)>,
    body: Bytes,
) -> Response {
    if let Err(response) =
        auth::authenticated_administrator(&state, &headers, query.as_deref()).await
    {
        return response;
    }
    if !auth::is_json_content_type(&headers) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let request: UpdateLibraryFolderPathDto = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    let Some(libraries) = state.libraries.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let binding = match libraries.filesystem_root_binding(library_id, root_id).await {
        Ok(Some(binding)) => binding,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(_) => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    if binding.provider.as_deref() != Some("filesystem") {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let (Some(account_id), Some(current_path)) = (binding.account_id, binding.root_path) else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let requested = if let Some(selection) = request.filesystem_selection() {
        let Some(browser) = state.filesystem_browser.as_ref() else {
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        };
        match browser
            .resolve(selection.root_id(), Path::new(selection.relative_path()))
            .await
        {
            Ok(resolved) => resolved.path().to_path_buf(),
            Err(_) => return StatusCode::BAD_REQUEST.into_response(),
        }
    } else if let Some(path) = request.path().filter(|path| !path.trim().is_empty()) {
        PathBuf::from(path)
    } else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Ok(root_object_id) = StorageObjectId::new("filesystem", binding.provider_root_id) else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let backend = match FilesystemBackend::new_with_root_id(&requested, root_object_id).await {
        Ok(backend) => Arc::new(backend),
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    let Some(root_path) = backend.root_path().to_str().map(str::to_owned) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    if root_path == current_path {
        return StatusCode::NO_CONTENT.into_response();
    }
    let display_name = backend
        .root_path()
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("Media")
        .to_owned();
    let retargeted = match libraries
        .retarget_filesystem_root(
            LibraryId::from_uuid(library_id),
            StorageRootId::from_uuid(root_id),
            &root_path,
            &display_name,
        )
        .await
    {
        Ok(retargeted) => retargeted,
        Err(error) => return library_error_response(&error),
    };
    if let Some(runtime) = state.storage_runtime.as_ref() {
        if runtime.deactivate(account_id, "local").is_err() {
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
        if runtime
            .activate_filesystem(retargeted.account_id(), backend)
            .await
            .is_err()
        {
            if let Err(error) = libraries
                .disable_storage_account_after_activation_failure(retargeted.account_id())
                .await
            {
                return library_error_response(&error);
            }
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
    }
    StatusCode::NO_CONTENT.into_response()
}

pub(crate) async fn contents(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    RoutePath((library_id, root_id)): RoutePath<(Uuid, Uuid)>,
) -> Response {
    if let Err(response) =
        auth::authenticated_administrator(&state, &headers, query.as_deref()).await
    {
        return response;
    }
    let Ok(query) = auth::request_query(query.as_deref()) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let path = query.get("Path").map_or("", String::as_str);
    if path.len() > 4096
        || path.chars().any(char::is_control)
        || Path::new(path)
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let Some(libraries) = state.libraries.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let Ok(folders) = libraries.folders(library_id).await else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let Some(folder) = folders.into_iter().find(|folder| folder.id == root_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if folder.provider == "filesystem" {
        let Some(root_path) = folder.path else {
            return StatusCode::NOT_FOUND.into_response();
        };
        return filesystem_contents(&root_path, path).await;
    }
    let Some(mut parent_id) = folder.root_object_id else {
        return StatusCode::NOT_FOUND.into_response();
    };
    // Remote traversal uses root-scoped object IDs from prior listings, not ambiguous file names.
    for component in Path::new(path).components() {
        let Some(id) = component
            .as_os_str()
            .to_str()
            .and_then(|value| Uuid::parse_str(value).ok())
        else {
            return StatusCode::BAD_REQUEST.into_response();
        };
        let Ok(children) = libraries.folder_children(root_id, parent_id).await else {
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        };
        if !children
            .iter()
            .any(|child| child.id == id && child.is_directory)
        {
            return StatusCode::NOT_FOUND.into_response();
        }
        parent_id = id;
    }
    match libraries.folder_children(root_id, parent_id).await {
        Ok(children) if children.len() <= 10_000 => Json(ContentsDto {
            indexed: true,
            items: children
                .into_iter()
                .map(|child| EntryDto {
                    name: child.name,
                    path: if path.is_empty() {
                        child.id.to_string()
                    } else {
                        format!("{path}/{}", child.id)
                    },
                    is_directory: child.is_directory,
                    size: child.size.and_then(|value| u64::try_from(value).ok()),
                    modified_at: child.modified_at.map(|value| value.to_rfc3339()),
                })
                .collect(),
        })
        .into_response(),
        Ok(_) => StatusCode::PAYLOAD_TOO_LARGE.into_response(),
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}

async fn filesystem_contents(root_path: &str, path: &str) -> Response {
    let Ok(browser) = FilesystemBrowser::from_roots([root_path]).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let root_id = browser.roots()[0].id();
    match browser.contents(root_id, Path::new(path)).await {
        Ok(page) => Json(ContentsDto {
            indexed: false,
            items: page
                .entries()
                .iter()
                .map(|entry| EntryDto {
                    name: entry.name().to_owned(),
                    path: entry.relative_path().to_owned(),
                    is_directory: entry.is_directory(),
                    size: entry.size(),
                    modified_at: entry.modified_at().map(|value| value.to_rfc3339()),
                })
                .collect(),
        })
        .into_response(),
        Err(error) => crate::filesystem_admin::browser_error_response(&error),
    }
}
