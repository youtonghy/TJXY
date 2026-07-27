use axum::{
    Json,
    extract::{Path, RawQuery, State, rejection::PathRejection},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use tjxy_api::{AuthenticationInfoDto, AuthenticationInfoQueryResult};

use crate::{AppState, auth};

pub(crate) async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    if validate_query(raw_query.as_deref(), false).is_err() {
        return no_store(StatusCode::BAD_REQUEST.into_response());
    }
    let principal =
        match auth::authenticated_administrator(&state, &headers, raw_query.as_deref()).await {
            Ok(principal) => principal,
            Err(response) => return no_store(response),
        };
    let Some(service) = state.auth.as_ref() else {
        return no_store(StatusCode::SERVICE_UNAVAILABLE.into_response());
    };
    match service.list_api_keys(&principal).await {
        Ok(keys) => no_store(
            Json(AuthenticationInfoQueryResult::new(
                keys.iter().map(authentication_info).collect(),
            ))
            .into_response(),
        ),
        Err(error) => no_store(auth::authentication_error_response(error)),
    }
}

pub(crate) async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    let Ok(Some(app_name)) = validate_query(raw_query.as_deref(), true) else {
        return no_store(StatusCode::BAD_REQUEST.into_response());
    };
    let principal =
        match auth::authenticated_administrator(&state, &headers, raw_query.as_deref()).await {
            Ok(principal) => principal,
            Err(response) => return no_store(response),
        };
    let Some(service) = state.auth.as_ref() else {
        return no_store(StatusCode::SERVICE_UNAVAILABLE.into_response());
    };
    match service.create_api_key(&principal, &app_name).await {
        Ok(()) => no_store(StatusCode::NO_CONTENT.into_response()),
        Err(error) => no_store(auth::authentication_error_response(error)),
    }
}

pub(crate) async fn delete(
    State(state): State<AppState>,
    path: Result<Path<String>, PathRejection>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    let Ok(Path(raw_key)) = path else {
        return no_store(StatusCode::BAD_REQUEST.into_response());
    };
    if validate_query(raw_query.as_deref(), false).is_err() {
        return no_store(StatusCode::BAD_REQUEST.into_response());
    }
    let principal =
        match auth::authenticated_administrator(&state, &headers, raw_query.as_deref()).await {
            Ok(principal) => principal,
            Err(response) => return no_store(response),
        };
    let Some(service) = state.auth.as_ref() else {
        return no_store(StatusCode::SERVICE_UNAVAILABLE.into_response());
    };
    match service.delete_api_key(&principal, &raw_key).await {
        Ok(()) => no_store(StatusCode::NO_CONTENT.into_response()),
        Err(error) => no_store(auth::authentication_error_response(error)),
    }
}

pub(crate) async fn method_not_allowed() -> Response {
    no_store(StatusCode::METHOD_NOT_ALLOWED.into_response())
}

fn authentication_info(key: &tjxy_application::ApiKeyInfo) -> AuthenticationInfoDto {
    AuthenticationInfoDto::new(
        key.id(),
        key.access_token().expose_secret(),
        key.app_name(),
        key.creator_user_id().as_uuid(),
        key.creator_user_name(),
        key.created_at(),
        key.last_used_at(),
    )
}

fn validate_query(raw_query: Option<&str>, app_required: bool) -> Result<Option<String>, ()> {
    let mut app_name = None;
    let mut auth_parameter_seen = false;
    for (name, value) in auth::request_query_pairs(raw_query)? {
        match name.as_str() {
            "app" if app_required => {
                if !tjxy_application::valid_api_key_app_name(&value)
                    || app_name.replace(value).is_some()
                {
                    return Err(());
                }
            }
            "ApiKey" | "api_key" => {
                if auth_parameter_seen || !auth::valid_token_transport(&value) {
                    return Err(());
                }
                auth_parameter_seen = true;
            }
            _ => return Err(()),
        }
    }
    if app_required && app_name.is_none() {
        return Err(());
    }
    Ok(app_name)
}

fn no_store(mut response: Response) -> Response {
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    response
}
