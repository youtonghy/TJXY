use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use tjxy_api::{PlaybackTicketRequest, PlaybackTicketResponse};
use tjxy_application::{CatalogServiceError, PlaybackTicketServiceError};
use tjxy_common::CatalogItemId;
use uuid::Uuid;

use crate::{AppState, auth};

pub(crate) async fn issue(
    State(state): State<AppState>,
    Path(item_id): Path<Uuid>,
    headers: HeaderMap,
    Json(request): Json<PlaybackTicketRequest>,
) -> Response {
    let principal = match auth::authenticated_principal(&state, &headers, None).await {
        Ok(principal) => principal,
        Err(response) => return response,
    };
    let Some(catalog) = state.catalog.as_ref() else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "catalog is unavailable");
    };
    let sources = match catalog
        .playback_sources(
            principal.user().id(),
            None,
            CatalogItemId::from_uuid(item_id),
        )
        .await
    {
        Ok(Some(sources)) => sources,
        Ok(None) => return error(StatusCode::NOT_FOUND, "catalog item was not found"),
        Err(error) => return catalog_error(&error),
    };
    let Some(source) = sources
        .iter()
        .find(|source| source.presentation_key() == request.media_source_id)
    else {
        return error(StatusCode::NOT_FOUND, "media source was not found");
    };
    let Some(service) = state.playback_tickets.as_ref() else {
        return error(
            StatusCode::SERVICE_UNAVAILABLE,
            "playback tickets are unavailable",
        );
    };
    let ticket = match service
        .issue(
            &principal,
            CatalogItemId::from_uuid(item_id),
            request.media_source_id,
            request.play_session_id,
        )
        .await
    {
        Ok(ticket) => ticket,
        Err(error) => return service_error(error),
    };
    let route = if source.is_audio() { "Audio" } else { "Videos" };
    let stream_url = format!(
        "/{route}/{item_id}/stream?static=true&mediaSourceId={}&PlaybackTicket={}",
        request.media_source_id,
        ticket.secret().expose_secret()
    );
    let response = Json(PlaybackTicketResponse {
        id: ticket.id(),
        ticket: ticket.secret().expose_secret().to_owned(),
        expires_at: ticket.expires_at(),
        stream_url,
    })
    .into_response();
    let mut response = response;
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    response
}

pub(crate) async fn revoke(
    State(state): State<AppState>,
    Path(ticket_id): Path<Uuid>,
    headers: HeaderMap,
) -> Response {
    let principal = match auth::authenticated_principal(&state, &headers, None).await {
        Ok(principal) => principal,
        Err(response) => return response,
    };
    let Some(service) = state.playback_tickets.as_ref() else {
        return error(
            StatusCode::SERVICE_UNAVAILABLE,
            "playback tickets are unavailable",
        );
    };
    match service.revoke(&principal, ticket_id).await {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => service_error(error),
    }
}

fn catalog_error(catalog_error: &CatalogServiceError) -> Response {
    match catalog_error {
        CatalogServiceError::ForbiddenUser => {
            error(StatusCode::FORBIDDEN, "catalog access is not permitted")
        }
        _ => error(StatusCode::SERVICE_UNAVAILABLE, "catalog is unavailable"),
    }
}

fn service_error(error_value: PlaybackTicketServiceError) -> Response {
    match error_value {
        PlaybackTicketServiceError::SessionRequired | PlaybackTicketServiceError::InvalidTicket => {
            error(StatusCode::UNAUTHORIZED, "authentication is required")
        }
        PlaybackTicketServiceError::Capacity => error(
            StatusCode::TOO_MANY_REQUESTS,
            "playback ticket capacity reached",
        ),
        PlaybackTicketServiceError::Repository(_) => error(
            StatusCode::SERVICE_UNAVAILABLE,
            "playback tickets are unavailable",
        ),
    }
}

fn error(status: StatusCode, message: &'static str) -> Response {
    (status, Json(serde_json::json!({"Message": message}))).into_response()
}
