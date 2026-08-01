use std::sync::Arc;

use axum::{
    Json,
    body::Bytes,
    extract::{RawQuery, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use tjxy_api::{
    AddVirtualFolderDto, AttachVirtualFolderPathDto, FilesystemSelectionDto, LibraryOptionsDto,
    UpdateLibraryOptionsDto, VirtualFolderInfo,
};
use tjxy_application::{LibraryPolicyOverrides, LibraryService, LibraryServiceError};
use tjxy_common::{LibraryId, StorageRootId};
use tjxy_db::{FilesystemRootDraft, LibraryRepositoryError};
use tjxy_storage_filesystem::FilesystemBackend;

use crate::{AppState, auth};

struct AddVirtualFolderQuery {
    name: String,
    collection_type: String,
    paths: Option<String>,
}

struct DeleteVirtualFolderQuery {
    name: String,
}

struct RenameVirtualFolderQuery {
    name: String,
    new_name: String,
}

struct DetachVirtualFolderPathQuery {
    name: String,
    root_id: StorageRootId,
}

pub(crate) async fn virtual_folders(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    if let Err(response) =
        auth::authenticated_administrator(&state, &headers, raw_query.as_deref()).await
    {
        return response;
    }
    let Some(libraries) = state.libraries.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match libraries.virtual_folders().await {
        Ok(folders) => Json(
            folders
                .into_iter()
                .map(|folder| {
                    let locations = folder.roots().iter().map(|root| root.location()).collect();
                    VirtualFolderInfo::new(
                        folder.name(),
                        locations,
                        folder.collection_type(),
                        LibraryOptionsDto::new(
                            folder.is_enabled(),
                            folder.scan_profile(),
                            folder.profile_version(),
                            folder.object_selection_scope(),
                            folder.metadata_policy(),
                            folder.metadata_source_mode(),
                            folder.expansion_policy(),
                            folder.probe_policy(),
                        ),
                        folder.id().as_uuid(),
                    )
                })
                .collect::<Vec<_>>(),
        )
        .into_response(),
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}

pub(crate) async fn add_virtual_folder(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
    body: Bytes,
) -> Response {
    if let Err(response) =
        auth::authenticated_administrator(&state, &headers, raw_query.as_deref()).await
    {
        return response;
    }
    let Ok(query) = add_virtual_folder_query(raw_query.as_deref()) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    if !auth::is_json_content_type(&headers) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let request: AddVirtualFolderDto = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    let Some(libraries) = state.libraries.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let (profile, enabled, metadata_source_mode) =
        request
            .library_options()
            .map_or(("Lazy", true, "automatic_scrape"), |options| {
                (
                    options.scan_profile(),
                    options.enabled(),
                    options.metadata_source_mode(),
                )
            });
    let selection = request.filesystem_selection();
    let result = if let Some(selection) = selection {
        let Ok((backend, draft)) = browser_filesystem_root(&state, selection).await else {
            return StatusCode::BAD_REQUEST.into_response();
        };
        let result = libraries
            .create_virtual_folder_with_filesystem_root(
                &query.name,
                &query.collection_type,
                profile,
                enabled,
                metadata_source_mode,
                &draft,
            )
            .await;
        activate_created_filesystem_root(&state, libraries, backend, result).await
    } else if let Some(path) = query.paths.filter(|path| !path.is_empty()) {
        let Ok(backend) = FilesystemBackend::new(&path).await.map(Arc::new) else {
            return StatusCode::BAD_REQUEST.into_response();
        };
        let Some(root_path) = backend.root_path().to_str() else {
            return StatusCode::BAD_REQUEST.into_response();
        };
        let display_name = backend
            .root_path()
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(&query.name);
        let Ok(draft) = FilesystemRootDraft::new(
            root_path,
            backend.root_id().provider_object_id(),
            display_name,
        ) else {
            return StatusCode::BAD_REQUEST.into_response();
        };
        let result = libraries
            .create_virtual_folder_with_filesystem_root(
                &query.name,
                &query.collection_type,
                profile,
                enabled,
                metadata_source_mode,
                &draft,
            )
            .await;
        match result {
            Ok(created) => {
                if let Some(runtime) = state.storage_runtime.as_ref()
                    && runtime
                        .activate_filesystem(created.account_id(), backend)
                        .is_err()
                {
                    if let Err(error) = libraries
                        .disable_storage_account_after_activation_failure(created.account_id())
                        .await
                    {
                        return library_error_response(&error);
                    }
                    Err(LibraryServiceError::RuntimeStorageActivation)
                } else {
                    Ok(())
                }
            }
            Err(error) => Err(error),
        }
    } else {
        libraries
            .create_virtual_folder(
                &query.name,
                &query.collection_type,
                profile,
                enabled,
                metadata_source_mode,
            )
            .await
            .map(|_| ())
    };
    match result {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => library_error_response(&error),
    }
}

pub(crate) async fn attach_virtual_folder_path(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
    body: Bytes,
) -> Response {
    if let Err(response) =
        auth::authenticated_administrator(&state, &headers, raw_query.as_deref()).await
    {
        return response;
    }
    if !auth::is_json_content_type(&headers) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let request: AttachVirtualFolderPathDto = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    let Some(libraries) = state.libraries.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let Ok((backend, draft)) =
        browser_filesystem_root(&state, request.filesystem_selection()).await
    else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let result = libraries
        .attach_filesystem_root(LibraryId::from_uuid(request.library_id()), &draft)
        .await;
    match activate_created_filesystem_root(&state, libraries, backend, result).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => library_error_response(&error),
    }
}

async fn browser_filesystem_root(
    state: &AppState,
    selection: &FilesystemSelectionDto,
) -> Result<(Arc<FilesystemBackend>, FilesystemRootDraft), ()> {
    let browser = state.filesystem_browser.as_ref().ok_or(())?;
    let resolved = browser
        .resolve(
            selection.root_id(),
            std::path::Path::new(selection.relative_path()),
        )
        .await
        .map_err(|_| ())?;
    let backend = Arc::new(
        FilesystemBackend::new(resolved.path())
            .await
            .map_err(|_| ())?,
    );
    let root_path = backend.root_path().to_str().ok_or(())?;
    let display_name = backend
        .root_path()
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("Media");
    let draft = FilesystemRootDraft::new(
        root_path,
        backend.root_id().provider_object_id(),
        display_name,
    )
    .map_err(|_| ())?;
    Ok((backend, draft))
}

async fn activate_created_filesystem_root(
    state: &AppState,
    libraries: &LibraryService,
    backend: Arc<FilesystemBackend>,
    result: Result<tjxy_db::CreatedFilesystemLibrary, LibraryServiceError>,
) -> Result<(), LibraryServiceError> {
    let created = result?;
    if let Some(runtime) = state.storage_runtime.as_ref()
        && runtime
            .activate_filesystem(created.account_id(), backend)
            .is_err()
    {
        libraries
            .disable_storage_account_after_activation_failure(created.account_id())
            .await?;
        return Err(LibraryServiceError::RuntimeStorageActivation);
    }
    Ok(())
}

pub(crate) async fn rename_virtual_folder(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    if let Err(response) =
        auth::authenticated_administrator(&state, &headers, raw_query.as_deref()).await
    {
        return response;
    }
    let Ok(query) = rename_virtual_folder_query(raw_query.as_deref()) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Some(libraries) = state.libraries.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match libraries
        .rename_virtual_folder(&query.name, &query.new_name)
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => library_error_response(&error),
    }
}

pub(crate) async fn detach_virtual_folder_path(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    if let Err(response) =
        auth::authenticated_administrator(&state, &headers, raw_query.as_deref()).await
    {
        return response;
    }
    let Ok(query) = detach_virtual_folder_path_query(raw_query.as_deref()) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Some(libraries) = state.libraries.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match libraries.detach_root(&query.name, query.root_id).await {
        Ok(disabled) => {
            if deactivate_storage_runtimes(&state, &disabled).is_ok() {
                StatusCode::NO_CONTENT.into_response()
            } else {
                StatusCode::SERVICE_UNAVAILABLE.into_response()
            }
        }
        Err(error) => library_error_response(&error),
    }
}

pub(crate) async fn delete_virtual_folder(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    if let Err(response) =
        auth::authenticated_administrator(&state, &headers, raw_query.as_deref()).await
    {
        return response;
    }
    let Ok(query) = delete_virtual_folder_query(raw_query.as_deref()) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Some(libraries) = state.libraries.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match libraries.delete_virtual_folder(&query.name).await {
        Ok(disabled) => {
            if deactivate_storage_runtimes(&state, &disabled).is_ok() {
                StatusCode::NO_CONTENT.into_response()
            } else {
                StatusCode::SERVICE_UNAVAILABLE.into_response()
            }
        }
        Err(error) => library_error_response(&error),
    }
}

fn deactivate_storage_runtimes(
    state: &AppState,
    disabled: &[tjxy_db::DisabledStorageRuntime],
) -> Result<(), crate::RuntimeStorageError> {
    let Some(runtime) = state.storage_runtime.as_ref() else {
        return Ok(());
    };
    for storage in disabled {
        runtime.deactivate(storage.account_id(), storage.provider_drive_id())?;
    }
    Ok(())
}

fn add_virtual_folder_query(raw_query: Option<&str>) -> Result<AddVirtualFolderQuery, ()> {
    let mut parameters = library_parameters(raw_query)?;
    let name = parameters
        .remove("name")
        .filter(|name| !name.is_empty())
        .ok_or(())?;
    let collection_type = parameters
        .remove("collectionType")
        .unwrap_or_else(|| "mixed".to_owned());
    let paths = parameters.remove("paths");
    take_refresh_library(&mut parameters)?;
    if !parameters.is_empty() {
        return Err(());
    }
    Ok(AddVirtualFolderQuery {
        name,
        collection_type,
        paths,
    })
}

fn delete_virtual_folder_query(raw_query: Option<&str>) -> Result<DeleteVirtualFolderQuery, ()> {
    let mut parameters = library_parameters(raw_query)?;
    let name = parameters
        .remove("name")
        .filter(|name| !name.is_empty())
        .ok_or(())?;
    take_refresh_library(&mut parameters)?;
    if !parameters.is_empty() {
        return Err(());
    }
    Ok(DeleteVirtualFolderQuery { name })
}

fn rename_virtual_folder_query(raw_query: Option<&str>) -> Result<RenameVirtualFolderQuery, ()> {
    let mut parameters = library_parameters(raw_query)?;
    let name = parameters
        .remove("name")
        .filter(|name| !name.is_empty())
        .ok_or(())?;
    let new_name = parameters
        .remove("newName")
        .filter(|name| !name.is_empty())
        .ok_or(())?;
    take_refresh_library(&mut parameters)?;
    if !parameters.is_empty() {
        return Err(());
    }
    Ok(RenameVirtualFolderQuery { name, new_name })
}

fn detach_virtual_folder_path_query(
    raw_query: Option<&str>,
) -> Result<DetachVirtualFolderPathQuery, ()> {
    let mut parameters = library_parameters(raw_query)?;
    let name = parameters
        .remove("name")
        .filter(|name| !name.is_empty())
        .ok_or(())?;
    let location = parameters.remove("path").ok_or(())?;
    let root_id = location
        .strip_prefix("tjxy://storage-root/")
        .and_then(|id| uuid::Uuid::parse_str(id).ok())
        .map(StorageRootId::from_uuid)
        .ok_or(())?;
    take_refresh_library(&mut parameters)?;
    if !parameters.is_empty() {
        return Err(());
    }
    Ok(DetachVirtualFolderPathQuery { name, root_id })
}

fn library_parameters(
    raw_query: Option<&str>,
) -> Result<std::collections::HashMap<String, String>, ()> {
    let mut parameters = auth::request_query(raw_query)?;
    parameters.remove("ApiKey");
    parameters.remove("api_key");
    Ok(parameters)
}

fn take_refresh_library(
    parameters: &mut std::collections::HashMap<String, String>,
) -> Result<(), ()> {
    if parameters
        .remove("refreshLibrary")
        .is_some_and(|value| !matches!(value.as_str(), "true" | "false"))
    {
        return Err(());
    }
    Ok(())
}

pub(crate) async fn update_library_options(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
    body: Bytes,
) -> Response {
    if let Err(response) =
        auth::authenticated_administrator(&state, &headers, raw_query.as_deref()).await
    {
        return response;
    }
    if !auth::is_json_content_type(&headers) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let request: UpdateLibraryOptionsDto = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    let Some(libraries) = state.libraries.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let options = request.library_options();
    let overrides = match (
        options.object_selection_scope(),
        options.metadata_policy(),
        options.expansion_policy(),
        options.probe_policy(),
    ) {
        (None, None, None, None) => None,
        (
            Some(object_selection_scope),
            Some(metadata_policy),
            Some(expansion_policy),
            Some(probe_policy),
        ) => Some(LibraryPolicyOverrides {
            object_selection_scope,
            metadata_policy,
            expansion_policy,
            probe_policy,
        }),
        _ => return StatusCode::BAD_REQUEST.into_response(),
    };
    match libraries
        .update_profile(
            LibraryId::from_uuid(request.id()),
            options.scan_profile(),
            options.profile_version(),
            options.enabled(),
            options.metadata_source_mode(),
            overrides,
        )
        .await
    {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => library_error_response(&error),
    }
}

fn library_error_response(error: &LibraryServiceError) -> Response {
    match error {
        LibraryServiceError::InvalidProfile
        | LibraryServiceError::Repository(
            LibraryRepositoryError::InvalidName
            | LibraryRepositoryError::InvalidCollectionType
            | LibraryRepositoryError::InvalidFilesystemRoot
            | LibraryRepositoryError::InvalidProfileVersion
            | LibraryRepositoryError::InvalidStoredPolicy,
        ) => StatusCode::BAD_REQUEST.into_response(),
        LibraryServiceError::Repository(
            LibraryRepositoryError::NotFound | LibraryRepositoryError::RootNotAttached,
        ) => StatusCode::NOT_FOUND.into_response(),
        LibraryServiceError::Repository(
            LibraryRepositoryError::StaleProfile
            | LibraryRepositoryError::NameConflict
            | LibraryRepositoryError::FilesystemRootIdentityChanged
            | LibraryRepositoryError::Referenced,
        ) => StatusCode::CONFLICT.into_response(),
        LibraryServiceError::Repository(
            LibraryRepositoryError::Database(_) | LibraryRepositoryError::WorkJob(_),
        )
        | LibraryServiceError::StorageAccount(_)
        | LibraryServiceError::RuntimeStorageActivation => {
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        }
    }
}
