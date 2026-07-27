use axum::{
    Json,
    body::Bytes,
    extract::{Path, RawQuery, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use tjxy_application::MediaCollectionServiceError;
use tjxy_common::CatalogItemId;
use tjxy_db::{MediaCollectionEntry, MediaCollectionRepositoryError};
use uuid::Uuid;

use crate::{AppState, auth};

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
struct CreatePlaylistRequest {
    name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
struct RenameCollectionRequest {
    name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
struct AppendItemsRequest {
    item_ids: Vec<Uuid>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct PlaylistDto {
    id: Uuid,
    name: String,
    #[serde(rename = "Type")]
    item_type: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct CollectionListDto {
    items: Vec<PlaylistDto>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct PlaylistItemsDto {
    items: Vec<PlaylistItemDto>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct PlaylistItemDto {
    playlist_item_id: Uuid,
    id: Uuid,
    name: String,
    #[serde(rename = "Type")]
    item_type: String,
}

pub(crate) async fn create_playlist(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
    body: Bytes,
) -> Response {
    let principal =
        match auth::authenticated_principal(&state, &headers, raw_query.as_deref()).await {
            Ok(principal) => principal,
            Err(response) => return response,
        };
    if !auth_only_query(raw_query.as_deref()) || !auth::is_json_content_type(&headers) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let request: CreatePlaylistRequest = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    let Some(service) = state.media_collections.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match service
        .create_playlist(principal.user().id(), &request.name)
        .await
    {
        Ok(playlist) => (
            StatusCode::CREATED,
            Json(PlaylistDto {
                id: playlist.id(),
                name: playlist.name().to_owned(),
                item_type: "Playlist",
            }),
        )
            .into_response(),
        Err(error) => service_error(&error),
    }
}

pub(crate) async fn create_shared_collection(
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
    if !auth_only_query(raw_query.as_deref()) || !auth::is_json_content_type(&headers) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let request: CreatePlaylistRequest = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    let Some(service) = state.media_collections.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match service.create_shared_collection(true, &request.name).await {
        Ok(collection) => (
            StatusCode::CREATED,
            Json(PlaylistDto {
                id: collection.id(),
                name: collection.name().to_owned(),
                item_type: "Collection",
            }),
        )
            .into_response(),
        Err(error) => service_error(&error),
    }
}

pub(crate) async fn playlists(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    let principal =
        match auth::authenticated_principal(&state, &headers, raw_query.as_deref()).await {
            Ok(principal) => principal,
            Err(response) => return response,
        };
    if !auth_only_query(raw_query.as_deref()) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let Some(service) = state.media_collections.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match service.playlists(principal.user().id()).await {
        Ok(playlists) => Json(CollectionListDto {
            items: playlists
                .iter()
                .map(|playlist| PlaylistDto {
                    id: playlist.id(),
                    name: playlist.name().to_owned(),
                    item_type: "Playlist",
                })
                .collect(),
        })
        .into_response(),
        Err(error) => service_error(&error),
    }
}

pub(crate) async fn shared_collections(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    if let Err(response) =
        auth::authenticated_principal(&state, &headers, raw_query.as_deref()).await
    {
        return response;
    }
    if !auth_only_query(raw_query.as_deref()) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let Some(service) = state.media_collections.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match service.shared_collections().await {
        Ok(collections) => Json(CollectionListDto {
            items: collections
                .iter()
                .map(|collection| PlaylistDto {
                    id: collection.id(),
                    name: collection.name().to_owned(),
                    item_type: "Collection",
                })
                .collect(),
        })
        .into_response(),
        Err(error) => service_error(&error),
    }
}

pub(crate) async fn rename_playlist(
    State(state): State<AppState>,
    Path(playlist_id): Path<Uuid>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
    body: Bytes,
) -> Response {
    let principal =
        match auth::authenticated_principal(&state, &headers, raw_query.as_deref()).await {
            Ok(principal) => principal,
            Err(response) => return response,
        };
    if !auth_only_query(raw_query.as_deref()) || !auth::is_json_content_type(&headers) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let request: RenameCollectionRequest = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    let Some(service) = state.media_collections.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match service
        .rename_playlist(principal.user().id(), playlist_id, &request.name)
        .await
    {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => service_error(&error),
    }
}

pub(crate) async fn rename_shared_collection(
    State(state): State<AppState>,
    Path(collection_id): Path<Uuid>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
    body: Bytes,
) -> Response {
    if let Err(response) =
        auth::authenticated_administrator(&state, &headers, raw_query.as_deref()).await
    {
        return response;
    }
    if !auth_only_query(raw_query.as_deref()) || !auth::is_json_content_type(&headers) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let request: RenameCollectionRequest = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    let Some(service) = state.media_collections.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match service
        .rename_shared_collection(true, collection_id, &request.name)
        .await
    {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => service_error(&error),
    }
}

pub(crate) async fn delete_playlist(
    State(state): State<AppState>,
    Path(playlist_id): Path<Uuid>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    let principal =
        match auth::authenticated_principal(&state, &headers, raw_query.as_deref()).await {
            Ok(principal) => principal,
            Err(response) => return response,
        };
    if !auth_only_query(raw_query.as_deref()) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let Some(service) = state.media_collections.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match service
        .delete_playlist(principal.user().id(), playlist_id)
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => service_error(&error),
    }
}

pub(crate) async fn delete_shared_collection(
    State(state): State<AppState>,
    Path(collection_id): Path<Uuid>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    if let Err(response) =
        auth::authenticated_administrator(&state, &headers, raw_query.as_deref()).await
    {
        return response;
    }
    if !auth_only_query(raw_query.as_deref()) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let Some(service) = state.media_collections.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match service.delete_shared_collection(true, collection_id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => service_error(&error),
    }
}

pub(crate) async fn append_playlist_items(
    State(state): State<AppState>,
    Path(playlist_id): Path<Uuid>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
    body: Bytes,
) -> Response {
    let principal =
        match auth::authenticated_principal(&state, &headers, raw_query.as_deref()).await {
            Ok(principal) => principal,
            Err(response) => return response,
        };
    if !auth_only_query(raw_query.as_deref()) || !auth::is_json_content_type(&headers) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let request: AppendItemsRequest = match serde_json::from_slice::<AppendItemsRequest>(&body) {
        Ok(request) if !request.item_ids.is_empty() => request,
        _ => return StatusCode::BAD_REQUEST.into_response(),
    };
    let Some(service) = state.media_collections.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let item_ids = request
        .item_ids
        .into_iter()
        .map(CatalogItemId::from_uuid)
        .collect::<Vec<_>>();
    match service
        .append_playlist_items(principal.user().id(), playlist_id, &item_ids)
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => service_error(&error),
    }
}

pub(crate) async fn append_shared_collection_items(
    State(state): State<AppState>,
    Path(collection_id): Path<Uuid>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
    body: Bytes,
) -> Response {
    if let Err(response) =
        auth::authenticated_administrator(&state, &headers, raw_query.as_deref()).await
    {
        return response;
    }
    if !auth_only_query(raw_query.as_deref()) || !auth::is_json_content_type(&headers) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let request: AppendItemsRequest = match serde_json::from_slice::<AppendItemsRequest>(&body) {
        Ok(request) if !request.item_ids.is_empty() => request,
        _ => return StatusCode::BAD_REQUEST.into_response(),
    };
    let Some(service) = state.media_collections.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let item_ids = request
        .item_ids
        .into_iter()
        .map(CatalogItemId::from_uuid)
        .collect::<Vec<_>>();
    match service
        .append_shared_items(true, collection_id, &item_ids)
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => service_error(&error),
    }
}

pub(crate) async fn playlist_items(
    State(state): State<AppState>,
    Path(playlist_id): Path<Uuid>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    let principal =
        match auth::authenticated_principal(&state, &headers, raw_query.as_deref()).await {
            Ok(principal) => principal,
            Err(response) => return response,
        };
    if !auth_only_query(raw_query.as_deref()) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let Some(service) = state.media_collections.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match service
        .playlist_items(principal.user().id(), playlist_id)
        .await
    {
        Ok(items) => Json(PlaylistItemsDto {
            items: items.iter().map(item_dto).collect(),
        })
        .into_response(),
        Err(error) => service_error(&error),
    }
}

pub(crate) async fn shared_collection_items(
    State(state): State<AppState>,
    Path(collection_id): Path<Uuid>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    if let Err(response) =
        auth::authenticated_principal(&state, &headers, raw_query.as_deref()).await
    {
        return response;
    }
    if !auth_only_query(raw_query.as_deref()) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let Some(service) = state.media_collections.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match service.shared_items(collection_id).await {
        Ok(items) => Json(PlaylistItemsDto {
            items: items.iter().map(item_dto).collect(),
        })
        .into_response(),
        Err(error) => service_error(&error),
    }
}

pub(crate) async fn move_playlist_item(
    State(state): State<AppState>,
    Path((playlist_id, entry_id, new_index)): Path<(Uuid, Uuid, u64)>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    let principal =
        match auth::authenticated_principal(&state, &headers, raw_query.as_deref()).await {
            Ok(principal) => principal,
            Err(response) => return response,
        };
    if !auth_only_query(raw_query.as_deref()) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let Some(service) = state.media_collections.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match service
        .move_playlist_item(principal.user().id(), playlist_id, entry_id, new_index)
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => service_error(&error),
    }
}

pub(crate) async fn delete_playlist_item(
    State(state): State<AppState>,
    Path((playlist_id, entry_id)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    let principal =
        match auth::authenticated_principal(&state, &headers, raw_query.as_deref()).await {
            Ok(principal) => principal,
            Err(response) => return response,
        };
    if !auth_only_query(raw_query.as_deref()) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let Some(service) = state.media_collections.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match service
        .delete_playlist_item(principal.user().id(), playlist_id, entry_id)
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => service_error(&error),
    }
}

fn item_dto(entry: &MediaCollectionEntry) -> PlaylistItemDto {
    PlaylistItemDto {
        playlist_item_id: entry.id(),
        id: entry.item().id().as_uuid(),
        name: entry.item().name().to_owned(),
        item_type: entry.item().item_type().to_owned(),
    }
}

fn auth_only_query(raw_query: Option<&str>) -> bool {
    let Ok(mut query) = auth::request_query(raw_query) else {
        return false;
    };
    query.remove("ApiKey");
    query.remove("api_key");
    query.is_empty()
}

fn service_error(error: &MediaCollectionServiceError) -> Response {
    match error {
        MediaCollectionServiceError::AdministratorRequired
        | MediaCollectionServiceError::Repository(MediaCollectionRepositoryError::Forbidden) => {
            StatusCode::FORBIDDEN.into_response()
        }
        MediaCollectionServiceError::Repository(
            MediaCollectionRepositoryError::InvalidName
            | MediaCollectionRepositoryError::InvalidPosition,
        ) => StatusCode::BAD_REQUEST.into_response(),
        MediaCollectionServiceError::Repository(
            MediaCollectionRepositoryError::NotFound
            | MediaCollectionRepositoryError::EntryNotFound
            | MediaCollectionRepositoryError::ItemUnavailable(_),
        ) => StatusCode::NOT_FOUND.into_response(),
        MediaCollectionServiceError::Repository(_) => {
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        }
    }
}
