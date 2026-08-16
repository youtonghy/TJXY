use axum::{
    Json,
    extract::{Path, RawQuery, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Serialize;
use tjxy_api::SessionInfoDto;
use tjxy_application::{AuthError, SessionListFilter};
use tjxy_common::UserId;
use uuid::Uuid;

use crate::{AppState, auth};

pub(crate) async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    let principal =
        match auth::authenticated_principal(&state, &headers, raw_query.as_deref()).await {
            Ok(principal) => principal,
            Err(response) => return response,
        };
    let Ok(filter) = parse_filter(raw_query.as_deref()) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Some(service) = state.auth.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match service.sessions(&principal, filter).await {
        Ok(sessions) => Json(
            sessions
                .into_iter()
                .map(|session| {
                    SessionInfoDto::listed(
                        session.id(),
                        session.user_id().as_uuid(),
                        session.user_name(),
                        session.client_name(),
                        session.device_id(),
                        session.device_name(),
                        session.client_version(),
                        state.identity.id,
                        session.last_activity_at(),
                        session.playable_media_types().to_vec(),
                        session.supported_commands().to_vec(),
                        session.supports_media_control(),
                        session.supports_persistent_identifier(),
                    )
                })
                .collect::<Vec<_>>(),
        )
        .into_response(),
        Err(AuthError::InvalidSessionFilter) => StatusCode::BAD_REQUEST.into_response(),
        Err(AuthError::Forbidden) => StatusCode::FORBIDDEN.into_response(),
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}

pub(crate) async fn logout(
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
    if auth_parameters(raw_query.as_deref()).is_err() {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let Some(service) = state.auth.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match service.logout(&principal).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(AuthError::SessionRequired) => StatusCode::FORBIDDEN.into_response(),
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct PersonalSessionDto {
    id: Uuid,
    device_id: String,
    device_name: String,
    client_name: String,
    application_version: String,
    created_at: chrono::DateTime<chrono::Utc>,
    last_activity_date: chrono::DateTime<chrono::Utc>,
    is_current: bool,
}

pub(crate) async fn list_personal(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    let principal =
        match auth::authenticated_principal(&state, &headers, raw_query.as_deref()).await {
            Ok(principal) => principal,
            Err(response) => return response,
        };
    if principal.session_id().is_none() {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Some(service) = state.auth.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match service
        .sessions(&principal, SessionListFilter::default())
        .await
    {
        Ok(sessions) => Json(
            sessions
                .into_iter()
                .map(|session| PersonalSessionDto {
                    is_current: principal.session_id() == Some(session.id()),
                    id: session.id(),
                    device_id: session.device_id().to_owned(),
                    device_name: session.device_name().to_owned(),
                    client_name: session.client_name().to_owned(),
                    application_version: session.client_version().to_owned(),
                    created_at: session.created_at(),
                    last_activity_date: session.last_activity_at(),
                })
                .collect::<Vec<_>>(),
        )
        .into_response(),
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}

pub(crate) async fn revoke_personal(
    State(state): State<AppState>,
    Path(session_id): Path<Uuid>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    let principal =
        match auth::authenticated_principal(&state, &headers, raw_query.as_deref()).await {
            Ok(principal) => principal,
            Err(response) => return response,
        };
    let Some(service) = state.auth.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match service.revoke_user_session(&principal, session_id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(AuthError::SessionRequired) => StatusCode::FORBIDDEN.into_response(),
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}

fn parse_filter(raw_query: Option<&str>) -> Result<SessionListFilter, ()> {
    let mut parameters = auth_parameters(raw_query)?;
    let mut filter = SessionListFilter::default();
    if let Some(device_id) = parameters.remove("deviceId") {
        filter = filter.with_device_id(device_id);
    }
    if let Some(seconds) = parameters.remove("activeWithinSeconds") {
        filter = filter.with_active_within_seconds(seconds.parse().map_err(|_| ())?);
    }
    if let Some(user_id) = parameters.remove("controllableByUserId") {
        filter = filter.with_controllable_by_user_id(UserId::from_uuid(
            Uuid::parse_str(&user_id).map_err(|_| ())?,
        ));
    }
    if parameters.is_empty() {
        Ok(filter)
    } else {
        Err(())
    }
}

fn auth_parameters(
    raw_query: Option<&str>,
) -> Result<std::collections::HashMap<String, String>, ()> {
    let mut parameters = auth::request_query(raw_query)?;
    parameters.remove("ApiKey");
    parameters.remove("api_key");
    Ok(parameters)
}
