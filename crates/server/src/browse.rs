use std::collections::HashMap;

use axum::{
    Json,
    body::Bytes,
    extract::{Path, RawQuery, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde_json::json;
use tjxy_api::{
    BaseItemDto, BaseItemDtoQueryResult, BaseItemKind, CollectionType, DeliveryMethod,
    MediaSourceInfo, MediaStream, MediaStreamType, PlaybackInfoResponse, SearchHint,
    SearchHintResult, UserItemDataDto,
};
use tjxy_application::{
    AuthError, CatalogItemType, CatalogPageRequest, CatalogServiceError, DeviceProfile,
    PlaybackSource, SessionCapabilities,
};
use tjxy_common::{CatalogItemId, PresentationKey, UserId};
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
    if let Err(response) = auth::authenticated_session_id(&principal) {
        return response;
    }
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
    if let Err(response) = auth::authenticated_session_id(&principal) {
        return response;
    }
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
        Err(AuthError::InvalidCapabilities) => {
            error(StatusCode::BAD_REQUEST, "invalid capabilities request")
        }
        Err(AuthError::SessionRequired) => {
            error(StatusCode::FORBIDDEN, "session authentication is required")
        }
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
        Ok(views) => match library_result(&state, views, None) {
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
            Ok(Some(page)) => match item_result(state.identity.id, parent_id, &page) {
                Ok(result) => Json(result).into_response(),
                Err(error) => error.into_response(),
            },
            Ok(None) => error(StatusCode::NOT_FOUND, "catalog parent was not found"),
            Err(error) => service_error(&error),
        };
    }

    if query.page.has_item_type_filter() {
        return HttpBrowseError::new(
            StatusCode::BAD_REQUEST,
            "item type filters require a catalog parent",
        )
        .into_response();
    }
    match catalog
        .user_views(principal.user().id(), query.user_id)
        .await
    {
        Ok(views) => match library_result(&state, views, Some(&query.page)) {
            Ok(result) => Json(result).into_response(),
            Err(error) => error.into_response(),
        },
        Err(error) => service_error(&error),
    }
}

pub(crate) async fn search_hints(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    let principal =
        match auth::authenticated_principal(&state, &headers, raw_query.as_deref()).await {
            Ok(principal) => principal,
            Err(response) => return response,
        };
    let query = match parse_search_query(raw_query.as_deref()) {
        Ok(query) => query,
        Err(error) => return error.into_response(),
    };
    let Some(catalog) = state.catalog.as_ref() else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "catalog is unavailable");
    };
    match catalog
        .search_hints(
            principal.user().id(),
            query.user_id,
            &query.search_term,
            query.page,
        )
        .await
    {
        Ok(page) => match search_hint_result(&page) {
            Ok(result) => Json(result).into_response(),
            Err(error) => error.into_response(),
        },
        Err(error) => service_error(&error),
    }
}

pub(crate) async fn resume_items(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    let principal =
        match auth::authenticated_principal(&state, &headers, raw_query.as_deref()).await {
            Ok(principal) => principal,
            Err(response) => return response,
        };
    let query = match parse_resume_query(raw_query.as_deref()) {
        Ok(query) => query,
        Err(error) => return error.into_response(),
    };
    let Some(catalog) = state.catalog.as_ref() else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "catalog is unavailable");
    };
    match catalog
        .resume_items(principal.user().id(), query.user_id, query.page)
        .await
    {
        Ok(page) => match resume_result(state.identity.id, &page) {
            Ok(result) => Json(result).into_response(),
            Err(error) => error.into_response(),
        },
        Err(error) => service_error(&error),
    }
}

pub(crate) async fn latest_items(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    let principal =
        match auth::authenticated_principal(&state, &headers, raw_query.as_deref()).await {
            Ok(principal) => principal,
            Err(response) => return response,
        };
    let query = match parse_latest_query(raw_query.as_deref()) {
        Ok(query) => query,
        Err(error) => return error.into_response(),
    };
    let Some(catalog) = state.catalog.as_ref() else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "catalog is unavailable");
    };
    match catalog
        .latest_items(
            principal.user().id(),
            query.user_id,
            query.parent_id,
            query.item_types,
            query.limit,
        )
        .await
    {
        Ok(items) => match latest_result(state.identity.id, &items) {
            Ok(result) => Json(result).into_response(),
            Err(error) => error.into_response(),
        },
        Err(error) => service_error(&error),
    }
}

pub(crate) async fn next_up_items(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    let principal =
        match auth::authenticated_principal(&state, &headers, raw_query.as_deref()).await {
            Ok(principal) => principal,
            Err(response) => return response,
        };
    let query = match parse_next_up_query(raw_query.as_deref()) {
        Ok(query) => query,
        Err(error) => return error.into_response(),
    };
    let Some(catalog) = state.catalog.as_ref() else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "catalog is unavailable");
    };
    match catalog
        .next_up_items(
            principal.user().id(),
            query.user_id,
            query.series_id,
            query.include_resumable,
            query.page,
        )
        .await
    {
        Ok(page) => match resume_result(state.identity.id, &page) {
            Ok(result) => Json(result).into_response(),
            Err(error) => error.into_response(),
        },
        Err(error) => service_error(&error),
    }
}

pub(crate) async fn item_detail(
    State(state): State<AppState>,
    Path(item_id): Path<Uuid>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    let principal =
        match auth::authenticated_principal(&state, &headers, raw_query.as_deref()).await {
            Ok(principal) => principal,
            Err(response) => return response,
        };
    let requested_user = match parse_item_detail_query(raw_query.as_deref()) {
        Ok(user) => user,
        Err(error) => return error.into_response(),
    };
    let Some(catalog) = state.catalog.as_ref() else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "catalog is unavailable");
    };
    match catalog
        .item(
            principal.user().id(),
            requested_user,
            CatalogItemId::from_uuid(item_id),
        )
        .await
    {
        Ok(Some(item)) => match item_dto(
            state.identity.id,
            item.parent_id().map(CatalogItemId::as_uuid),
            &item,
        ) {
            Ok(item) => Json(item).into_response(),
            Err(error) => error.into_response(),
        },
        Ok(None) => error(StatusCode::NOT_FOUND, "catalog item was not found"),
        Err(error) => service_error(&error),
    }
}

pub(crate) async fn playback_info_get(
    State(state): State<AppState>,
    Path(item_id): Path<Uuid>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    playback_info(
        state,
        item_id,
        headers,
        raw_query,
        PlaybackInfoRequest::default(),
    )
    .await
}

pub(crate) async fn playback_info_post(
    State(state): State<AppState>,
    Path(item_id): Path<Uuid>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
    body: Bytes,
) -> Response {
    if !auth::is_json_content_type(&headers) {
        return error(StatusCode::BAD_REQUEST, "invalid playback request");
    }
    let payload: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(serde_json::Value::Object(payload)) => serde_json::Value::Object(payload),
        _ => return error(StatusCode::BAD_REQUEST, "invalid playback request"),
    };
    let profile = payload
        .get("DeviceProfile")
        .or_else(|| payload.get("deviceProfile"))
        .cloned();
    let user_id = match json_uuid(&payload, "UserId", "userId") {
        Ok(user_id) => user_id.map(UserId::from_uuid),
        Err(error) => return error.into_response(),
    };
    let media_source_id = match json_uuid(&payload, "MediaSourceId", "mediaSourceId") {
        Ok(media_source_id) => media_source_id.map(PresentationKey::from_uuid),
        Err(error) => return error.into_response(),
    };
    let enable_direct_play = match json_bool(&payload, "EnableDirectPlay", "enableDirectPlay") {
        Ok(enable_direct_play) => enable_direct_play,
        Err(error) => return error.into_response(),
    };
    playback_info(
        state,
        item_id,
        headers,
        raw_query,
        PlaybackInfoRequest {
            user_id,
            media_source_id,
            enable_direct_play,
            profile,
        },
    )
    .await
}

async fn playback_info(
    state: AppState,
    item_id: Uuid,
    headers: HeaderMap,
    raw_query: Option<String>,
    mut request: PlaybackInfoRequest,
) -> Response {
    let principal =
        match auth::authenticated_principal(&state, &headers, raw_query.as_deref()).await {
            Ok(principal) => principal,
            Err(response) => return response,
        };
    let query = match parse_playback_info_query(raw_query.as_deref()) {
        Ok(query) => query,
        Err(error) => return error.into_response(),
    };
    request.user_id = query.user_id.or(request.user_id);
    request.media_source_id = query.media_source_id.or(request.media_source_id);
    request.enable_direct_play = query.enable_direct_play.or(request.enable_direct_play);
    let Some(catalog) = state.catalog.as_ref() else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "catalog is unavailable");
    };
    let profile = if request.profile.is_some() {
        request.profile
    } else {
        let Some(auth_service) = state.auth.as_ref() else {
            return error(
                StatusCode::SERVICE_UNAVAILABLE,
                "authentication is unavailable",
            );
        };
        match auth_service.session_device_profile(&principal).await {
            Ok(profile) => profile,
            Err(_) => {
                return error(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "authentication is unavailable",
                );
            }
        }
    };
    let profile =
        profile.map(|profile| serde_json::from_value::<DeviceProfile>(profile).unwrap_or_default());
    match catalog
        .playback_sources(
            principal.user().id(),
            request.user_id,
            CatalogItemId::from_uuid(item_id),
        )
        .await
    {
        Ok(Some(sources)) => {
            let mut sources = sources
                .into_iter()
                .filter(|source| {
                    request
                        .media_source_id
                        .is_none_or(|requested| source.presentation_key() == requested)
                })
                .map(|source| {
                    let compatible = request.enable_direct_play.unwrap_or(true)
                        && profile
                            .as_ref()
                            .is_none_or(|profile| profile.supports_direct_play(&source));
                    let codec_compatibility = profile
                        .as_ref()
                        .map_or(0, |profile| profile.codec_compatibility_rank(&source));
                    (compatible, codec_compatibility, source)
                })
                .collect::<Vec<_>>();
            sort_playback_sources(&mut sources);
            let media_sources = sources
                .into_iter()
                .map(|(compatible, _, source)| media_source_info(item_id, &source, compatible))
                .collect::<Result<Vec<_>, _>>();
            match media_sources {
                Ok(media_sources) => Json(PlaybackInfoResponse {
                    media_sources,
                    play_session_id: Uuid::new_v4().to_string(),
                })
                .into_response(),
                Err(_) => error(StatusCode::SERVICE_UNAVAILABLE, "playback is unavailable"),
            }
        }
        Ok(None) => error(StatusCode::NOT_FOUND, "catalog item was not found"),
        Err(error) => service_error(&error),
    }
}

fn sort_playback_sources(sources: &mut [(bool, u8, PlaybackSource)]) {
    sources.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then_with(|| right.2.is_last_used().cmp(&left.2.is_last_used()))
            .then_with(|| right.2.is_default().cmp(&left.2.is_default()))
            .then_with(|| right.2.admin_priority().cmp(&left.2.admin_priority()))
            .then_with(|| right.2.resolution_pixels().cmp(&left.2.resolution_pixels()))
            .then_with(|| right.1.cmp(&left.1))
            .then_with(|| right.2.account_health().cmp(&left.2.account_health()))
            .then_with(|| right.2.location_priority().cmp(&left.2.location_priority()))
            .then_with(|| {
                left.2
                    .presentation_key()
                    .as_uuid()
                    .cmp(&right.2.presentation_key().as_uuid())
            })
    });
}

fn media_source_info(
    item_id: Uuid,
    source: &PlaybackSource,
    supports_direct_play: bool,
) -> Result<MediaSourceInfo, tjxy_api::PlaybackInfoError> {
    let presentation = source.presentation_key();
    let mut streams = source
        .streams()
        .iter()
        .filter_map(|stream| {
            let stream_type = match stream.stream_type() {
                "Video" => MediaStreamType::Video,
                "Audio" => MediaStreamType::Audio,
                _ => return None,
            };
            Some(MediaStream {
                codec: stream.codec().map(str::to_owned),
                language: stream.language().map(str::to_owned),
                width: stream.width(),
                height: stream.height(),
                channels: stream.channels(),
                profile: stream.profile().map(str::to_owned),
                level: stream.level(),
                stream_type,
                index: stream.delivery_index(),
                is_external: false,
                delivery_method: Some(DeliveryMethod::Embed),
                delivery_url: None,
                is_external_url: false,
                is_text_subtitle_stream: false,
                supports_external_stream: false,
            })
        })
        .collect::<Vec<_>>();
    streams.extend(source.subtitles().iter().map(|subtitle| MediaStream {
        codec: Some(subtitle.format().to_owned()),
        language: subtitle.language().map(str::to_owned),
        width: None,
        height: None,
        channels: None,
        profile: None,
        level: None,
        stream_type: MediaStreamType::Subtitle,
        index: subtitle.delivery_index(),
        is_external: true,
        delivery_method: Some(DeliveryMethod::External),
        delivery_url: Some(format!(
            "/Videos/{item_id}/{presentation}/Subtitles/{}/Stream.{}",
            subtitle.delivery_index(),
            subtitle.format()
        )),
        is_external_url: false,
        is_text_subtitle_stream: true,
        supports_external_stream: true,
    }));
    let route = if source.is_audio() { "Audio" } else { "Videos" };
    MediaSourceInfo::direct_play(
        presentation,
        source.container(),
        format!("/{route}/{item_id}/stream?static=true&mediaSourceId={presentation}"),
        streams,
        supports_direct_play,
    )
}

#[derive(Default)]
struct PlaybackInfoRequest {
    user_id: Option<UserId>,
    media_source_id: Option<PresentationKey>,
    enable_direct_play: Option<bool>,
    profile: Option<serde_json::Value>,
}

struct PlaybackInfoQuery {
    user_id: Option<UserId>,
    media_source_id: Option<PresentationKey>,
    enable_direct_play: Option<bool>,
}

fn parse_playback_info_query(
    raw_query: Option<&str>,
) -> Result<PlaybackInfoQuery, HttpBrowseError> {
    let mut parameters = endpoint_parameters(raw_query)?;
    let user_id = take_user_id(&mut parameters)?;
    let media_source_id =
        take_uuid(&mut parameters, "mediaSourceId")?.map(PresentationKey::from_uuid);
    let enable_direct_play = take_bool(&mut parameters, "enableDirectPlay")?;
    for parameter in [
        "maxStreamingBitrate",
        "startTimeTicks",
        "audioStreamIndex",
        "subtitleStreamIndex",
        "maxAudioChannels",
        "liveStreamId",
        "autoOpenLiveStream",
        "enableDirectStream",
        "enableTranscoding",
        "allowVideoStreamCopy",
        "allowAudioStreamCopy",
    ] {
        parameters.remove(parameter);
    }
    reject_remaining(&parameters)?;
    Ok(PlaybackInfoQuery {
        user_id,
        media_source_id,
        enable_direct_play,
    })
}

fn json_uuid(
    payload: &serde_json::Value,
    pascal_name: &str,
    camel_name: &str,
) -> Result<Option<Uuid>, HttpBrowseError> {
    let Some(value) = payload.get(pascal_name).or_else(|| payload.get(camel_name)) else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    value
        .as_str()
        .and_then(|value| Uuid::parse_str(value).ok())
        .map(Some)
        .ok_or_else(|| HttpBrowseError::new(StatusCode::BAD_REQUEST, "invalid playback identifier"))
}

fn json_bool(
    payload: &serde_json::Value,
    pascal_name: &str,
    camel_name: &str,
) -> Result<Option<bool>, HttpBrowseError> {
    let Some(value) = payload.get(pascal_name).or_else(|| payload.get(camel_name)) else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    value
        .as_bool()
        .map(Some)
        .ok_or_else(|| HttpBrowseError::new(StatusCode::BAD_REQUEST, "invalid playback option"))
}

struct UserViewsQuery {
    user_id: Option<UserId>,
}

struct ItemsQuery {
    user_id: Option<UserId>,
    parent_id: Option<Uuid>,
    page: CatalogPageRequest,
}

struct SearchQuery {
    user_id: Option<UserId>,
    search_term: String,
    page: CatalogPageRequest,
}

struct ResumeQuery {
    user_id: Option<UserId>,
    page: CatalogPageRequest,
}

struct LatestQuery {
    user_id: Option<UserId>,
    parent_id: Option<Uuid>,
    item_types: Vec<CatalogItemType>,
    limit: u64,
}

struct NextUpQuery {
    user_id: Option<UserId>,
    series_id: Option<CatalogItemId>,
    include_resumable: bool,
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

fn parse_search_query(raw_query: Option<&str>) -> Result<SearchQuery, HttpBrowseError> {
    let mut parameters = endpoint_parameters(raw_query)?;
    let user_id = take_user_id(&mut parameters)?;
    let search_term = parameters
        .remove("searchTerm")
        .map(|value| value.trim().to_owned())
        .filter(|value| {
            !value.is_empty()
                && value.chars().count() <= 256
                && !value.chars().any(char::is_control)
        })
        .ok_or_else(|| HttpBrowseError::new(StatusCode::BAD_REQUEST, "invalid search term"))?;
    let start_index = take_u64(&mut parameters, "startIndex")?.unwrap_or(0);
    let limit = take_u64(&mut parameters, "limit")?.unwrap_or(100);
    let item_types = take_item_types(&mut parameters)?;
    reject_remaining(&parameters)?;
    let page = CatalogPageRequest::new(start_index, limit)
        .map_err(|_| HttpBrowseError::new(StatusCode::BAD_REQUEST, "invalid catalog page"))?
        .with_item_types(item_types);
    Ok(SearchQuery {
        user_id,
        search_term,
        page,
    })
}

fn parse_resume_query(raw_query: Option<&str>) -> Result<ResumeQuery, HttpBrowseError> {
    let mut parameters = endpoint_parameters(raw_query)?;
    let user_id = take_user_id(&mut parameters)?;
    let start_index = take_u64(&mut parameters, "startIndex")?.unwrap_or(0);
    let limit = take_u64(&mut parameters, "limit")?.unwrap_or(100);
    if parameters
        .remove("mediaTypes")
        .is_some_and(|value| value != "Video")
    {
        return Err(HttpBrowseError::new(
            StatusCode::BAD_REQUEST,
            "unsupported resume media type",
        ));
    }
    for boolean in ["enableUserData", "enableImages", "enableTotalRecordCount"] {
        take_bool(&mut parameters, boolean)?;
    }
    if parameters.contains_key("imageTypeLimit") {
        take_u64(&mut parameters, "imageTypeLimit")?;
    }
    parameters.remove("fields");
    parameters.remove("enableImageTypes");
    reject_remaining(&parameters)?;
    let page = CatalogPageRequest::new(start_index, limit)
        .map_err(|_| HttpBrowseError::new(StatusCode::BAD_REQUEST, "invalid catalog page"))?;
    Ok(ResumeQuery { user_id, page })
}

fn parse_latest_query(raw_query: Option<&str>) -> Result<LatestQuery, HttpBrowseError> {
    let mut parameters = endpoint_parameters(raw_query)?;
    let user_id = take_user_id(&mut parameters)?;
    let parent_id = take_uuid(&mut parameters, "parentId")?;
    let limit = take_u64(&mut parameters, "limit")?.unwrap_or(20);
    let item_types = take_item_types(&mut parameters)?;
    if take_bool(&mut parameters, "groupItems")?.unwrap_or(false) {
        return Err(HttpBrowseError::new(
            StatusCode::BAD_REQUEST,
            "grouped latest items are not supported",
        ));
    }
    for boolean in ["enableUserData", "enableImages"] {
        take_bool(&mut parameters, boolean)?;
    }
    if parameters.contains_key("imageTypeLimit") {
        take_u64(&mut parameters, "imageTypeLimit")?;
    }
    if parameters.contains_key("isPlayed") {
        return Err(HttpBrowseError::new(
            StatusCode::BAD_REQUEST,
            "played-state latest filtering is not supported",
        ));
    }
    parameters.remove("fields");
    parameters.remove("enableImageTypes");
    reject_remaining(&parameters)?;
    CatalogPageRequest::new(0, limit)
        .map_err(|_| HttpBrowseError::new(StatusCode::BAD_REQUEST, "invalid catalog page"))?;
    Ok(LatestQuery {
        user_id,
        parent_id,
        item_types,
        limit,
    })
}

fn parse_next_up_query(raw_query: Option<&str>) -> Result<NextUpQuery, HttpBrowseError> {
    let mut parameters = endpoint_parameters(raw_query)?;
    let user_id = take_user_id(&mut parameters)?;
    let series_id = take_uuid(&mut parameters, "seriesId")?.map(CatalogItemId::from_uuid);
    let start_index = take_u64(&mut parameters, "startIndex")?.unwrap_or(0);
    let limit = take_u64(&mut parameters, "limit")?.unwrap_or(100);
    let include_resumable = take_bool(&mut parameters, "enableResumable")?.unwrap_or(false);
    for boolean in ["enableUserData", "enableImages", "enableTotalRecordCount"] {
        take_bool(&mut parameters, boolean)?;
    }
    for unsupported in ["disableFirstEpisode", "enableRewatching"] {
        if take_bool(&mut parameters, unsupported)?.unwrap_or(false) {
            return Err(HttpBrowseError::new(
                StatusCode::BAD_REQUEST,
                "unsupported next-up option",
            ));
        }
    }
    for unsupported in ["parentId", "nextUpDateCutoff"] {
        if parameters.remove(unsupported).is_some() {
            return Err(HttpBrowseError::new(
                StatusCode::BAD_REQUEST,
                "unsupported next-up scope",
            ));
        }
    }
    if parameters.contains_key("imageTypeLimit") {
        take_u64(&mut parameters, "imageTypeLimit")?;
    }
    parameters.remove("fields");
    parameters.remove("enableImageTypes");
    reject_remaining(&parameters)?;
    let page = CatalogPageRequest::new(start_index, limit)
        .map_err(|_| HttpBrowseError::new(StatusCode::BAD_REQUEST, "invalid catalog page"))?;
    Ok(NextUpQuery {
        user_id,
        series_id,
        include_resumable,
        page,
    })
}

fn parse_item_detail_query(raw_query: Option<&str>) -> Result<Option<UserId>, HttpBrowseError> {
    let mut parameters = endpoint_parameters(raw_query)?;
    let user_id = take_user_id(&mut parameters)?;
    reject_remaining(&parameters)?;
    Ok(user_id)
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
            "Audio" => Ok(CatalogItemType::Audio),
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
    page: Option<&CatalogPageRequest>,
) -> Result<BaseItemDtoQueryResult, HttpBrowseError> {
    let total = views.len() as u64;
    let start_index = page.map_or(0, CatalogPageRequest::start_index);
    let limit = page.map_or(total, CatalogPageRequest::limit);
    let skip = usize::try_from(start_index).unwrap_or(usize::MAX);
    let take = usize::try_from(limit).unwrap_or(usize::MAX);
    let items = views
        .into_iter()
        .skip(skip)
        .take(take)
        .map(|view| library_dto(state.identity.id, &view))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|()| {
            HttpBrowseError::new(StatusCode::INTERNAL_SERVER_ERROR, "catalog data is invalid")
        })?;
    Ok(BaseItemDtoQueryResult::new(items, start_index, total))
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
    parent_id: Uuid,
    page: &CatalogPage,
) -> Result<BaseItemDtoQueryResult, HttpBrowseError> {
    let items = page
        .items()
        .iter()
        .map(|item| item_dto(server_id, Some(parent_id), item))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(BaseItemDtoQueryResult::new(
        items,
        page.start_index(),
        page.total_record_count(),
    ))
}

fn search_hint_result(page: &CatalogPage) -> Result<SearchHintResult, HttpBrowseError> {
    let search_hints = page
        .items()
        .iter()
        .map(|item| {
            Ok(SearchHint::new(
                item.id().as_uuid(),
                item.name(),
                item_kind(item)?,
            ))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(SearchHintResult::new(
        search_hints,
        page.start_index(),
        page.total_record_count(),
    ))
}

fn resume_result(
    server_id: Uuid,
    page: &CatalogPage,
) -> Result<BaseItemDtoQueryResult, HttpBrowseError> {
    let items = page
        .items()
        .iter()
        .map(|item| {
            item_dto(
                server_id,
                item.parent_id().map(CatalogItemId::as_uuid),
                item,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(BaseItemDtoQueryResult::new(
        items,
        page.start_index(),
        page.total_record_count(),
    ))
}

fn latest_result(
    server_id: Uuid,
    items: &[CatalogItemRecord],
) -> Result<Vec<BaseItemDto>, HttpBrowseError> {
    items
        .iter()
        .map(|item| {
            item_dto(
                server_id,
                item.parent_id().map(CatalogItemId::as_uuid),
                item,
            )
        })
        .collect()
}

fn item_dto(
    server_id: Uuid,
    parent_id: Option<Uuid>,
    item: &CatalogItemRecord,
) -> Result<BaseItemDto, HttpBrowseError> {
    let item_type = item_kind(item)?;
    Ok(BaseItemDto::catalog_item(
        item.id().as_uuid(),
        item.name(),
        server_id,
        item_type,
        parent_id,
        item.production_year(),
        item.overview().map(str::to_owned),
        Some(UserItemDataDto::new(
            item.id().as_uuid(),
            item.is_favorite(),
            item.is_played(),
            item.play_count(),
            item.playback_position_ticks(),
        )),
    )
    .with_image_tags(item.image_tags().clone()))
}

fn item_kind(item: &CatalogItemRecord) -> Result<BaseItemKind, HttpBrowseError> {
    match item.item_type() {
        "Movie" => Ok(BaseItemKind::Movie),
        "Audio" => Ok(BaseItemKind::Audio),
        "Series" => Ok(BaseItemKind::Series),
        "Season" => Ok(BaseItemKind::Season),
        "Episode" => Ok(BaseItemKind::Episode),
        "Folder" => Ok(BaseItemKind::Folder),
        _ => Err(HttpBrowseError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "catalog data is invalid",
        )),
    }
}

fn service_error(error_value: &CatalogServiceError) -> Response {
    match error_value {
        CatalogServiceError::ForbiddenUser => {
            error(StatusCode::FORBIDDEN, "catalog access is not permitted")
        }
        CatalogServiceError::Query(_)
        | CatalogServiceError::Work(_)
        | CatalogServiceError::Publication(_)
        | CatalogServiceError::Playstate(_)
        | CatalogServiceError::InvalidLazyTask => {
            error(StatusCode::SERVICE_UNAVAILABLE, "catalog is unavailable")
        }
    }
}

fn error(status: StatusCode, message: &'static str) -> Response {
    (status, Json(json!({"Message": message}))).into_response()
}
