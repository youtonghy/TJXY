use axum::{
    body::Bytes,
    extract::{RawQuery, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use tjxy_api::PlaybackStateRequest;
use tjxy_application::{PlaybackEvent, PlaystateServiceError};
use tjxy_common::{CatalogItemId, PresentationKey, UserId};
use tjxy_db::PlaystateRepositoryError;
use uuid::Uuid;

use crate::{AppState, auth};

pub(crate) async fn started(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
    body: Bytes,
) -> Response {
    apply(state, headers, raw_query, body, PlaybackEvent::Started).await
}

pub(crate) async fn progress(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
    body: Bytes,
) -> Response {
    apply(state, headers, raw_query, body, PlaybackEvent::Progress).await
}

pub(crate) async fn stopped(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
    body: Bytes,
) -> Response {
    apply(state, headers, raw_query, body, PlaybackEvent::Stopped).await
}

pub(crate) async fn ping(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    let principal =
        match auth::authenticated_principal(&state, &headers, raw_query.as_deref()).await {
            Ok(principal) => principal,
            Err(response) => return response,
        };
    let session_id = match auth::authenticated_session_id(&principal) {
        Ok(session_id) => session_id,
        Err(response) => return response,
    };
    let Ok(mut query) = auth::request_query(raw_query.as_deref()) else {
        return error(StatusCode::BAD_REQUEST, "invalid playback ping");
    };
    query.remove("ApiKey");
    query.remove("api_key");
    let lower = query.remove("playSessionId");
    let upper = query.remove("PlaySessionId");
    if lower.is_some() && upper.is_some() {
        return error(StatusCode::BAD_REQUEST, "invalid playback ping");
    }
    if !query.is_empty() {
        return error(StatusCode::BAD_REQUEST, "invalid playback ping");
    }
    let Some(play_session_id) = lower
        .or(upper)
        .and_then(|value| uuid::Uuid::parse_str(&value).ok())
    else {
        return error(StatusCode::BAD_REQUEST, "invalid playback ping");
    };
    let Some(service) = state.playstate.as_ref() else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "playstate is unavailable");
    };
    match service.ping(session_id, play_session_id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => error(StatusCode::NOT_FOUND, "playback session was not found"),
        Err(_) => error(StatusCode::SERVICE_UNAVAILABLE, "playstate is unavailable"),
    }
}

async fn apply(
    state: AppState,
    headers: HeaderMap,
    raw_query: Option<String>,
    body: Bytes,
    event: PlaybackEvent,
) -> Response {
    let principal =
        match auth::authenticated_principal(&state, &headers, raw_query.as_deref()).await {
            Ok(principal) => principal,
            Err(response) => return response,
        };
    let session_id = match auth::authenticated_session_id(&principal) {
        Ok(session_id) => session_id,
        Err(response) => return response,
    };
    if !valid_query(raw_query.as_deref()) || !auth::is_json_content_type(&headers) {
        return error(StatusCode::BAD_REQUEST, "invalid playback event");
    }
    let request: PlaybackStateRequest = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(_) => return error(StatusCode::BAD_REQUEST, "invalid playback event"),
    };
    let requested_user = request.user_id.map(UserId::from_uuid);
    if requested_user.is_some_and(|user| user != principal.user().id()) {
        return error(StatusCode::FORBIDDEN, "requested user is not authorized");
    }
    let Some(item_id) = request.item_id.map(CatalogItemId::from_uuid) else {
        // Jellyfin's playback DTO permits session-only telemetry without an item identity.
        return StatusCode::NO_CONTENT.into_response();
    };
    let requested_source = request
        .media_source_id
        .filter(|source_id| *source_id != item_id.as_uuid());
    let presentation_key = if let Some(source_id) = requested_source {
        PresentationKey::from_uuid(source_id)
    } else {
        let Some(catalog) = state.catalog.as_ref() else {
            return error(StatusCode::SERVICE_UNAVAILABLE, "catalog is unavailable");
        };
        match catalog
            .playback_sources(principal.user().id(), requested_user, item_id)
            .await
        {
            Ok(Some(sources)) => match sources.first() {
                Some(source) => source.presentation_key(),
                None => return error(StatusCode::NOT_FOUND, "media source was not found"),
            },
            Ok(None) => return error(StatusCode::NOT_FOUND, "item was not found"),
            Err(_) => return error(StatusCode::SERVICE_UNAVAILABLE, "catalog is unavailable"),
        }
    };
    let play_session_id = request.play_session_id.unwrap_or_else(|| {
        Uuid::new_v5(
            &session_id,
            format!("tjxy-playstate:{item_id}:{presentation_key}").as_bytes(),
        )
    });
    let Some(service) = state.playstate.as_ref() else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "playstate is unavailable");
    };
    match service
        .apply(
            principal.user().id(),
            session_id,
            requested_user,
            event,
            play_session_id,
            item_id,
            presentation_key,
            request.position_ticks,
        )
        .await
    {
        Ok(Some(commit)) => {
            if let Some(user_data) = commit.user_data() {
                state
                    .realtime_events()
                    .publish_user_data_changed(principal.user().id(), user_data.user_revision);
            }
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(None) => error(StatusCode::NOT_FOUND, "item was not found"),
        Err(error_value) => service_error(&error_value),
    }
}

fn valid_query(raw_query: Option<&str>) -> bool {
    let Ok(mut query) = auth::request_query(raw_query) else {
        return false;
    };
    query.remove("ApiKey");
    query.remove("api_key");
    query.is_empty()
}

fn service_error(error_value: &PlaystateServiceError) -> Response {
    match error_value {
        PlaystateServiceError::UnauthorizedUser => {
            error(StatusCode::FORBIDDEN, "requested user is not authorized")
        }
        PlaystateServiceError::Repository(
            PlaystateRepositoryError::NegativePosition
            | PlaystateRepositoryError::SessionIdentityMismatch,
        ) => error(StatusCode::BAD_REQUEST, "invalid playback event"),
        PlaystateServiceError::Repository(PlaystateRepositoryError::InvalidPresentation) => {
            error(StatusCode::NOT_FOUND, "media source was not found")
        }
        PlaystateServiceError::Repository(PlaystateRepositoryError::MissingSession) => {
            error(StatusCode::NOT_FOUND, "playback session was not found")
        }
        PlaystateServiceError::Repository(PlaystateRepositoryError::SessionStopped) => {
            error(StatusCode::CONFLICT, "playback session is stopped")
        }
        PlaystateServiceError::Repository(_) => {
            error(StatusCode::SERVICE_UNAVAILABLE, "playstate is unavailable")
        }
    }
}

fn error(status: StatusCode, message: &'static str) -> Response {
    (status, message).into_response()
}
