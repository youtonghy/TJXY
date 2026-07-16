use std::collections::HashMap;

use axum::{
    Json,
    body::Bytes,
    extract::{RawQuery, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde_json::json;
use tjxy_api::{
    BaseItemDto, BaseItemDtoQueryResult, BaseItemKind, CollectionType, UserItemDataDto,
};
use tjxy_application::{
    CatalogItemType, CatalogPageRequest, CatalogServiceError, SessionCapabilities,
};
use tjxy_common::UserId;
use tjxy_db::{CatalogItemRecord, CatalogPage, LibraryViewRecord};
use uuid::Uuid;

use crate::{AppState, auth};

#[derive(Clone, Copy, Debug)]
struct HttpBrowseError {
    status: StatusCode,
    message: &'static str,
}

impl HttpBrowseError {
    const fn new(status: StatusCode, message: &'static str) -> Self {
        Self { status, message }
    }
}

impl IntoResponse for HttpBrowseError {
    fn into_response(self) -> Response {
        error(self.status, self.message)
    }
}

pub(crate) async fn full_capabilities(
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
    let requested_session = match parse_capability_session(raw_query.as_deref()) {
        Ok(session) => session,
        Err(error) => return error.into_response(),
    };
    if !auth::is_json_content_type(&headers) {
        return error(StatusCode::BAD_REQUEST, "invalid capabilities request");
    }
    let payload: tjxy_api::ClientCapabilitiesDto = match serde_json::from_slice(&body) {
        Ok(payload) => payload,
        Err(_) => return error(StatusCode::BAD_REQUEST, "invalid capabilities request"),
    };
    persist_capabilities(
        &state,
        &principal,
        requested_session,
        SessionCapabilities {
            playable_media_types: payload.playable_media_types,
            supported_commands: payload.supported_commands,
            supports_media_control: payload.supports_media_control,
            supports_persistent_identifier: payload.supports_persistent_identifier,
            device_profile: payload.device_profile,
            app_store_url: payload.app_store_url,
            icon_url: payload.icon_url,
        },
    )
    .await
}

pub(crate) async fn legacy_capabilities(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    let principal =
        match auth::authenticated_principal(&state, &headers, raw_query.as_deref()).await {
            Ok(principal) => principal,
            Err(response) => return response,
        };
    let mut parameters = match endpoint_parameters(raw_query.as_deref()) {
        Ok(parameters) => parameters,
        Err(error) => return error.into_response(),
    };
    let requested_session = match take_capability_session(&mut parameters) {
        Ok(session) => session,
        Err(error) => return error.into_response(),
    };
    let playable_media_types =
        match take_csv_alias(&mut parameters, "playableMediaTypes", "PlayableMediaTypes") {
            Ok(values) => values,
            Err(error) => return error.into_response(),
        };
    let supported_commands =
        match take_csv_alias(&mut parameters, "supportedCommands", "SupportedCommands") {
            Ok(values) => values,
            Err(error) => return error.into_response(),
        };
    let supports_media_control = match take_bool_alias(
        &mut parameters,
        "supportsMediaControl",
        "SupportsMediaControl",
    ) {
        Ok(value) => value.unwrap_or(false),
        Err(error) => return error.into_response(),
    };
    let supports_persistent_identifier = match take_bool_alias(
        &mut parameters,
        "supportsPersistentIdentifier",
        "SupportsPersistentIdentifier",
    ) {
        Ok(value) => value.unwrap_or(true),
        Err(error) => return error.into_response(),
    };
    if let Err(error) = reject_remaining(&parameters) {
        return error.into_response();
    }
    persist_capabilities(
        &state,
        &principal,
        requested_session,
        SessionCapabilities {
            playable_media_types,
            supported_commands,
            supports_media_control,
            supports_persistent_identifier,
            ..SessionCapabilities::default()
        },
    )
    .await
}

async fn persist_capabilities(
    state: &AppState,
    principal: &tjxy_application::AuthenticatedPrincipal,
    requested_session: Option<Uuid>,
    capabilities: SessionCapabilities,
) -> Response {
    let Some(auth_service) = state.auth.as_ref() else {
        return error(
            StatusCode::SERVICE_UNAVAILABLE,
            "authentication is unavailable",
        );
    };
    match auth_service
        .update_session_capabilities(principal, requested_session, capabilities)
        .await
    {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => error(StatusCode::NOT_FOUND, "session was not found"),
        Err(_) => error(
            StatusCode::SERVICE_UNAVAILABLE,
            "authentication is unavailable",
        ),
    }
}

pub(crate) async fn user_views(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    let principal =
        match auth::authenticated_principal(&state, &headers, raw_query.as_deref()).await {
            Ok(principal) => principal,
            Err(response) => return response,
        };
    let query = match parse_user_views_query(raw_query.as_deref()) {
        Ok(query) => query,
        Err(error) => return error.into_response(),
    };
    let Some(catalog) = state.catalog.as_ref() else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "catalog is unavailable");
    };
    match catalog
        .user_views(principal.user().id(), query.user_id)
        .await
    {
        Ok(views) => match library_result(&state, views) {
            Ok(result) => Json(result).into_response(),
            Err(error) => error.into_response(),
        },
        Err(error) => service_error(&error),
    }
}

pub(crate) async fn items(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    let principal =
        match auth::authenticated_principal(&state, &headers, raw_query.as_deref()).await {
            Ok(principal) => principal,
            Err(response) => return response,
        };
    let query = match parse_items_query(raw_query.as_deref()) {
        Ok(query) => query,
        Err(error) => return error.into_response(),
    };
    let Some(catalog) = state.catalog.as_ref() else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "catalog is unavailable");
    };
    if let Some(parent_id) = query.parent_id {
        return match catalog
            .items_by_parent_id(principal.user().id(), query.user_id, parent_id, query.page)
            .await
        {
            Ok(Some(page)) => match item_result(state.identity.id, &page) {
                Ok(result) => Json(result).into_response(),
                Err(error) => error.into_response(),
            },
            Ok(None) => error(StatusCode::NOT_FOUND, "catalog parent was not found"),
            Err(error) => service_error(&error),
        };
    }

    match catalog
        .user_views(principal.user().id(), query.user_id)
        .await
    {
        Ok(views) => match library_result(&state, views) {
            Ok(result) => Json(result).into_response(),
            Err(error) => error.into_response(),
        },
        Err(error) => service_error(&error),
    }
}

struct UserViewsQuery {
    user_id: Option<UserId>,
}

struct ItemsQuery {
    user_id: Option<UserId>,
    parent_id: Option<Uuid>,
    page: CatalogPageRequest,
}

fn parse_capability_session(raw_query: Option<&str>) -> Result<Option<Uuid>, HttpBrowseError> {
    let mut parameters = endpoint_parameters(raw_query)?;
    let session = take_capability_session(&mut parameters)?;
    reject_remaining(&parameters)?;
    Ok(session)
}

fn take_capability_session(
    parameters: &mut HashMap<String, String>,
) -> Result<Option<Uuid>, HttpBrowseError> {
    parameters
        .remove("id")
        .map(|value| {
            Uuid::parse_str(&value)
                .map_err(|_| HttpBrowseError::new(StatusCode::NOT_FOUND, "session was not found"))
        })
        .transpose()
}

fn take_csv_alias(
    parameters: &mut HashMap<String, String>,
    canonical: &str,
    legacy_alias: &str,
) -> Result<Vec<String>, HttpBrowseError> {
    let value = take_alias(parameters, canonical, legacy_alias)?;
    match value {
        None => Ok(Vec::new()),
        Some(value) if value.is_empty() => Ok(Vec::new()),
        Some(value) => Ok(value.split(',').map(str::to_owned).collect()),
    }
}

fn take_bool_alias(
    parameters: &mut HashMap<String, String>,
    canonical: &str,
    legacy_alias: &str,
) -> Result<Option<bool>, HttpBrowseError> {
    take_alias(parameters, canonical, legacy_alias)?.map_or(Ok(None), |value| {
        match value.as_str() {
            "true" => Ok(Some(true)),
            "false" => Ok(Some(false)),
            _ => Err(HttpBrowseError::new(
                StatusCode::BAD_REQUEST,
                "invalid capabilities boolean",
            )),
        }
    })
}

fn take_alias(
    parameters: &mut HashMap<String, String>,
    canonical: &str,
    legacy_alias: &str,
) -> Result<Option<String>, HttpBrowseError> {
    let canonical_value = parameters.remove(canonical);
    let legacy_value = parameters.remove(legacy_alias);
    if canonical_value.is_some() && legacy_value.is_some() {
        return Err(HttpBrowseError::new(
            StatusCode::BAD_REQUEST,
            "duplicate capabilities parameter",
        ));
    }
    Ok(canonical_value.or(legacy_value))
}

fn parse_user_views_query(raw_query: Option<&str>) -> Result<UserViewsQuery, HttpBrowseError> {
    let mut parameters = endpoint_parameters(raw_query)?;
    let user_id = take_user_id(&mut parameters)?;
    reject_remaining(&parameters)?;
    Ok(UserViewsQuery { user_id })
}

fn parse_items_query(raw_query: Option<&str>) -> Result<ItemsQuery, HttpBrowseError> {
    let mut parameters = endpoint_parameters(raw_query)?;
    let user_id = take_user_id(&mut parameters)?;
    let parent_id = take_uuid(&mut parameters, "parentId")?;
    let start_index = take_u64(&mut parameters, "startIndex")?.unwrap_or(0);
    let limit = take_u64(&mut parameters, "limit")?.unwrap_or(100);
    let item_types = take_item_types(&mut parameters)?;
    if take_bool(&mut parameters, "recursive")?.unwrap_or(false) {
        return Err(HttpBrowseError::new(
            StatusCode::BAD_REQUEST,
            "recursive catalog queries are not supported",
        ));
    }
    if parameters
        .remove("sortBy")
        .is_some_and(|value| value != "SortName")
    {
        return Err(HttpBrowseError::new(
            StatusCode::BAD_REQUEST,
            "unsupported catalog sort",
        ));
    }
    if parameters
        .remove("sortOrder")
        .is_some_and(|value| value != "Ascending")
    {
        return Err(HttpBrowseError::new(
            StatusCode::BAD_REQUEST,
            "unsupported catalog sort order",
        ));
    }
    reject_remaining(&parameters)?;
    let page = CatalogPageRequest::new(start_index, limit)
        .map_err(|_| HttpBrowseError::new(StatusCode::BAD_REQUEST, "invalid catalog page"))?
        .with_item_types(item_types);
    Ok(ItemsQuery {
        user_id,
        parent_id,
        page,
    })
}

fn endpoint_parameters(
    raw_query: Option<&str>,
) -> Result<HashMap<String, String>, HttpBrowseError> {
    let mut parameters = auth::request_query(raw_query)
        .map_err(|()| HttpBrowseError::new(StatusCode::BAD_REQUEST, "invalid query parameters"))?;
    parameters.remove("ApiKey");
    parameters.remove("api_key");
    Ok(parameters)
}

fn take_user_id(
    parameters: &mut HashMap<String, String>,
) -> Result<Option<UserId>, HttpBrowseError> {
    take_uuid(parameters, "userId").map(|value| value.map(UserId::from_uuid))
}

fn take_uuid(
    parameters: &mut HashMap<String, String>,
    name: &str,
) -> Result<Option<Uuid>, HttpBrowseError> {
    parameters
        .remove(name)
        .map(|value| {
            Uuid::parse_str(&value).map_err(|_| {
                HttpBrowseError::new(StatusCode::BAD_REQUEST, "invalid catalog identifier")
            })
        })
        .transpose()
}

fn take_u64(
    parameters: &mut HashMap<String, String>,
    name: &str,
) -> Result<Option<u64>, HttpBrowseError> {
    parameters
        .remove(name)
        .map(|value| {
            value.parse().map_err(|_| {
                HttpBrowseError::new(StatusCode::BAD_REQUEST, "invalid catalog number")
            })
        })
        .transpose()
}

fn take_bool(
    parameters: &mut HashMap<String, String>,
    name: &str,
) -> Result<Option<bool>, HttpBrowseError> {
    parameters
        .remove(name)
        .map(|value| match value.as_str() {
            "true" => Ok(true),
            "false" => Ok(false),
            _ => Err(HttpBrowseError::new(
                StatusCode::BAD_REQUEST,
                "invalid catalog boolean",
            )),
        })
        .transpose()
}

fn take_item_types(
    parameters: &mut HashMap<String, String>,
) -> Result<Vec<CatalogItemType>, HttpBrowseError> {
    let Some(value) = parameters.remove("includeItemTypes") else {
        return Ok(Vec::new());
    };
    if value.is_empty() {
        return Err(HttpBrowseError::new(
            StatusCode::BAD_REQUEST,
            "empty catalog item type",
        ));
    }
    value
        .split(',')
        .map(|value| match value {
            "Movie" => Ok(CatalogItemType::Movie),
            "Series" => Ok(CatalogItemType::Series),
            "Season" => Ok(CatalogItemType::Season),
            "Episode" => Ok(CatalogItemType::Episode),
            "Folder" => Ok(CatalogItemType::Folder),
            _ => Err(HttpBrowseError::new(
                StatusCode::BAD_REQUEST,
                "unsupported catalog item type",
            )),
        })
        .collect()
}

fn reject_remaining(parameters: &HashMap<String, String>) -> Result<(), HttpBrowseError> {
    if parameters.is_empty() {
        Ok(())
    } else {
        Err(HttpBrowseError::new(
            StatusCode::BAD_REQUEST,
            "unsupported query parameter",
        ))
    }
}

fn library_result(
    state: &AppState,
    views: Vec<LibraryViewRecord>,
) -> Result<BaseItemDtoQueryResult, HttpBrowseError> {
    let total = views.len() as u64;
    let items = views
        .into_iter()
        .map(|view| library_dto(state.identity.id, &view))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|()| {
            HttpBrowseError::new(StatusCode::INTERNAL_SERVER_ERROR, "catalog data is invalid")
        })?;
    Ok(BaseItemDtoQueryResult::new(items, 0, total))
}

fn library_dto(server_id: Uuid, view: &LibraryViewRecord) -> Result<BaseItemDto, ()> {
    let collection_type = match view.collection_type() {
        "unknown" => CollectionType::Unknown,
        "folders" => CollectionType::Folders,
        "movies" => CollectionType::Movies,
        "tvshows" => CollectionType::TvShows,
        _ => return Err(()),
    };
    Ok(BaseItemDto::library_view(
        view.id(),
        view.name(),
        server_id,
        collection_type,
    ))
}

fn item_result(
    server_id: Uuid,
    page: &CatalogPage,
) -> Result<BaseItemDtoQueryResult, HttpBrowseError> {
    let items = page
        .items()
        .iter()
        .map(|item| item_dto(server_id, item))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(BaseItemDtoQueryResult::new(
        items,
        page.start_index(),
        page.total_record_count(),
    ))
}

fn item_dto(server_id: Uuid, item: &CatalogItemRecord) -> Result<BaseItemDto, HttpBrowseError> {
    let item_type = match item.item_type() {
        "Movie" => BaseItemKind::Movie,
        "Series" => BaseItemKind::Series,
        "Season" => BaseItemKind::Season,
        "Episode" => BaseItemKind::Episode,
        "Folder" => BaseItemKind::Folder,
        _ => {
            return Err(HttpBrowseError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "catalog data is invalid",
            ));
        }
    };
    Ok(BaseItemDto::catalog_item(
        item.id().as_uuid(),
        item.name(),
        server_id,
        item_type,
        item.parent_id().map(tjxy_common::CatalogItemId::as_uuid),
        item.production_year(),
        item.overview().map(str::to_owned),
        Some(UserItemDataDto::new(
            item.id().as_uuid(),
            item.is_favorite(),
            item.is_played(),
            item.play_count(),
            item.playback_position_ticks(),
        )),
    ))
}

fn service_error(error_value: &CatalogServiceError) -> Response {
    match error_value {
        CatalogServiceError::ForbiddenUser => {
            error(StatusCode::FORBIDDEN, "catalog access is not permitted")
        }
        CatalogServiceError::Query(_) => {
            error(StatusCode::SERVICE_UNAVAILABLE, "catalog is unavailable")
        }
    }
}

fn error(status: StatusCode, message: &'static str) -> Response {
    (status, Json(json!({"Message": message}))).into_response()
}
