use std::{collections::HashMap, net::IpAddr};

use axum::{
    Json,
    body::Bytes,
    extract::{Path, Query, RawQuery, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use tjxy_api::{
    AuthenticateUserByName, AuthenticationResult, CreateUserByName, SessionInfoDto, UpdateUserName,
    UpdateUserPassword, UpdateUserPolicy, UserDto, UserPolicy,
};
use tjxy_application::AuthenticatedPrincipal;
use tjxy_application::{AuthError, ClientIdentity};
use tjxy_common::{UserId, Username};
use tjxy_db::{AuthRepositoryError, AuthUser};
use uuid::Uuid;

use crate::{
    AppState,
    audit::{self, Event},
    login_guard::{AccountKey, Attempt, ClientAddr, Limited},
};

pub(crate) const SESSION_COOKIE: &str = "tjxy_session";
pub(crate) const SESSION_COOKIE_MAX_AGE: i64 = 30 * 24 * 60 * 60;
pub(crate) const REMEMBER_ME_MAX_AGE: i64 = 180 * 24 * 60 * 60;

pub(crate) async fn authenticate_by_name(
    State(state): State<AppState>,
    ClientAddr(ip): ClientAddr,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    no_store(authenticate_by_name_inner(&state, ip, &headers, &body).await)
}

pub(crate) fn no_store(mut response: Response) -> Response {
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    response
}

/// `429` for an exhausted failure budget, with the time until it reopens.
pub(crate) fn rate_limited_response(limited: &Limited) -> Response {
    let mut response = (
        StatusCode::TOO_MANY_REQUESTS,
        Json(ErrorBody {
            message: "too many failed authentication attempts",
        }),
    )
        .into_response();
    response.headers_mut().insert(
        header::RETRY_AFTER,
        axum::http::HeaderValue::from(limited.retry_after_seconds()),
    );
    response
}

/// Reserves a failure slot for a credential-bearing request, or answers `429`.
///
/// The slot counts as a failure unless [`finish_attempt`] releases it.
#[allow(clippy::result_large_err)] // The error is the ready-to-send Axum response.
pub(crate) fn begin_attempt(
    state: &AppState,
    action: &'static str,
    ip: IpAddr,
    account_name: Option<&str>,
) -> Result<Attempt, Response> {
    let account = account_name.and_then(AccountKey::from_name);
    state
        .login_guard
        .begin(account.as_ref(), ip)
        .map_err(|limited| {
            audit::rate_limited(action, ip, account_name, &limited);
            rate_limited_response(&limited)
        })
}

/// Keeps the reserved slot when credentials were wrong; otherwise gives it back.
pub(crate) fn finish_attempt(state: &AppState, attempt: &Attempt, credentials_failed: bool) {
    if !credentials_failed {
        state.login_guard.release(attempt);
    }
}

async fn authenticate_by_name_inner(
    state: &AppState,
    ip: IpAddr,
    headers: &HeaderMap,
    body: &[u8],
) -> Response {
    let client = match client_identity(headers, state.legacy_auth_enabled) {
        Ok(client) => client,
        Err(error) => return error.into_response(),
    };
    if !is_json_content_type(headers) {
        return HttpAuthError::BadRequest.into_response();
    }
    let payload: AuthenticateUserByName = match serde_json::from_slice(body) {
        Ok(payload) => payload,
        Err(_) => return HttpAuthError::BadRequest.into_response(),
    };
    let Some(auth) = state.auth.as_ref() else {
        return HttpAuthError::Unavailable.into_response();
    };
    let lifetime = if payload.remember_me {
        Some(chrono::Duration::days(180))
    } else {
        Some(chrono::Duration::days(30))
    };
    let device_id = client.device_id().to_owned();
    // Validate the name before touching the throttle tables: a name that cannot
    // be an account is only charged to the client address and is never stored.
    let Ok(username) = Username::parse(&payload.username) else {
        return match state.login_guard.begin(None, ip) {
            Ok(_attempt) => {
                Event::new("login_failure", "failure", ip)
                    .device(&device_id)
                    .reason("invalid_username")
                    .emit();
                HttpAuthError::Unauthorized.into_response()
            }
            Err(limited) => {
                audit::rate_limited("login", ip, None, &limited);
                rate_limited_response(&limited)
            }
        };
    };
    let account = AccountKey::from_username(&username);
    let normalized = String::from_utf8_lossy(account.as_bytes()).into_owned();
    let attempt = match state.login_guard.begin(Some(&account), ip) {
        Ok(attempt) => attempt,
        Err(limited) => {
            audit::rate_limited("login", ip, Some(&normalized), &limited);
            return rate_limited_response(&limited);
        }
    };
    let issued = match auth
        .authenticate_with_lifetime(&payload.username, &payload.password, client, lifetime)
        .await
    {
        Ok(issued) => issued,
        Err(error) => {
            let credentials_failed = matches!(error, AuthError::InvalidCredentials);
            finish_attempt(state, &attempt, credentials_failed);
            Event::new("login_failure", "failure", ip)
                .actor(&normalized)
                .device(&device_id)
                .reason(match error {
                    AuthError::InvalidCredentials => "invalid_credentials",
                    AuthError::Busy => "capacity_busy",
                    _ => "internal_error",
                })
                .emit();
            return HttpAuthError::from(error).into_response();
        }
    };
    state.login_guard.release(&attempt);
    state.login_guard.clear_account(&account);
    Event::new("login_success", "success", ip)
        .actor(&normalized)
        .device(&device_id)
        .emit();
    issued_response(state, &issued, payload.remember_me)
}

fn issued_response(
    state: &AppState,
    issued: &tjxy_application::IssuedAuthentication,
    remember_me: bool,
) -> Response {
    let user = user_dto(issued.user(), state.identity.id);
    let session = SessionInfoDto::active(
        issued.session_id(),
        issued.user().id().as_uuid(),
        issued.user().name(),
        issued.client().client_name(),
        issued.client().device_id(),
        issued.client().device_name(),
        issued.client().client_version(),
        state.identity.id,
    );
    let result = Json(AuthenticationResult::new(
        user,
        session,
        issued.access_token().expose_secret(),
        state.identity.id,
    ));
    if remember_me {
        let mut response = result.into_response();
        if let Ok(value) = format!(
            "{}={}; Path=/; Max-Age={}; HttpOnly; Secure; SameSite=Lax",
            SESSION_COOKIE,
            issued.access_token().expose_secret(),
            REMEMBER_ME_MAX_AGE
        )
        .parse()
        {
            response.headers_mut().append(header::SET_COOKIE, value);
        }
        response
    } else {
        let mut response = result.into_response();
        response.headers_mut().append(
            header::SET_COOKIE,
            format!("{SESSION_COOKIE}=; Path=/; Max-Age=0; HttpOnly; Secure; SameSite=Lax")
                .parse()
                .expect("valid cookie header"),
        );
        response
    }
}

pub(crate) async fn current_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    match authenticated_principal(&state, &headers, query.as_deref()).await {
        Ok(principal) => Json(user_dto(principal.user(), state.identity.id)).into_response(),
        Err(response) => response,
    }
}

pub(crate) async fn public_users() -> Json<Vec<UserDto>> {
    // TJXY does not expose account names on an unauthenticated endpoint.
    Json(Vec::new())
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
struct UpdateSelfProfileRequest {
    username: String,
    bio: String,
    current_password: String,
    new_password: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
struct UpdateSelfPasswordRequest {
    current_password: String,
    new_password: String,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct UserProfileDto {
    username: String,
    bio: String,
}

pub(crate) async fn current_user_profile(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let principal = match authenticated_principal(&state, &headers, query.as_deref()).await {
        Ok(principal) => principal,
        Err(response) => return response,
    };
    let Some(service) = state.auth.as_ref() else {
        return HttpAuthError::Unavailable.into_response();
    };
    match service.user_profile(principal.user().id()).await {
        Ok((user, bio)) => Json(UserProfileDto {
            username: user.name().to_owned(),
            bio,
        })
        .into_response(),
        Err(error) => HttpAuthError::from(error).into_response(),
    }
}

pub(crate) async fn update_current_user_profile(
    State(state): State<AppState>,
    ClientAddr(ip): ClientAddr,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    body: Bytes,
) -> Response {
    let principal = match authenticated_principal(&state, &headers, query.as_deref()).await {
        Ok(principal) => principal,
        Err(response) => return response,
    };
    let payload: UpdateSelfProfileRequest = match json_payload(&headers, &body) {
        Ok(payload) => payload,
        Err(status) => return status.into_response(),
    };
    let Some(service) = state.auth.as_ref() else {
        return HttpAuthError::Unavailable.into_response();
    };
    let actor = principal.user().name().to_owned();
    let attempt = match begin_attempt(&state, "profile_update", ip, Some(&actor)) {
        Ok(attempt) => attempt,
        Err(response) => return response,
    };
    let changes_password = payload
        .new_password
        .as_deref()
        .is_some_and(|value| !value.is_empty());
    let result = service
        .update_self_account(
            principal.user().id(),
            &payload.username,
            &payload.bio,
            &payload.current_password,
            payload.new_password.as_deref(),
        )
        .await;
    finish_attempt(
        &state,
        &attempt,
        matches!(result, Err(AuthError::InvalidCredentials)),
    );
    match result {
        Ok((user, bio)) => {
            let event = if changes_password {
                "password_changed"
            } else {
                "profile_updated"
            };
            Event::new(event, "success", ip).actor(&actor).emit();
            Json(UserProfileDto {
                username: user.name().to_owned(),
                bio,
            })
            .into_response()
        }
        Err(error) => {
            audit_credential_failure("profile_update", ip, &actor, &error);
            HttpAuthError::from(error).into_response()
        }
    }
}

/// Logs a failed sensitive self-service change without exposing why it failed.
fn audit_credential_failure(action: &'static str, ip: IpAddr, actor: &str, error: &AuthError) {
    if matches!(error, AuthError::InvalidCredentials) {
        Event::new("password_verification_failed", "failure", ip)
            .actor(actor)
            .target(action)
            .reason("invalid_credentials")
            .emit();
    }
}

pub(crate) async fn update_current_user_password(
    State(state): State<AppState>,
    ClientAddr(ip): ClientAddr,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    body: Bytes,
) -> Response {
    let principal = match authenticated_principal(&state, &headers, query.as_deref()).await {
        Ok(principal) => principal,
        Err(response) => return response,
    };
    let payload: UpdateSelfPasswordRequest = match json_payload(&headers, &body) {
        Ok(payload) => payload,
        Err(status) => return status.into_response(),
    };
    let Some(service) = state.auth.as_ref() else {
        return HttpAuthError::Unavailable.into_response();
    };
    let actor = principal.user().name().to_owned();
    let attempt = match begin_attempt(&state, "password_change", ip, Some(&actor)) {
        Ok(attempt) => attempt,
        Err(response) => return response,
    };
    let result = service
        .update_self_password(
            principal.user().id(),
            &payload.current_password,
            &payload.new_password,
        )
        .await;
    finish_attempt(
        &state,
        &attempt,
        matches!(result, Err(AuthError::InvalidCredentials)),
    );
    match result {
        Ok(_) => {
            Event::new("password_changed", "success", ip)
                .actor(&actor)
                .emit();
            StatusCode::NO_CONTENT.into_response()
        }
        Err(error) => {
            audit_credential_failure("password_change", ip, &actor, &error);
            HttpAuthError::from(error).into_response()
        }
    }
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UsersQuery {
    is_hidden: Option<bool>,
    is_disabled: Option<bool>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UpdateUserQuery {
    user_id: Uuid,
}

pub(crate) async fn users(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
    Query(query): Query<UsersQuery>,
) -> Response {
    if let Err(response) = authenticated_administrator(&state, &headers, raw_query.as_deref()).await
    {
        return response;
    }
    if query.is_hidden == Some(true) {
        return Json(Vec::<UserDto>::new()).into_response();
    }
    let Some(service) = state.auth.as_ref() else {
        return HttpAuthError::Unavailable.into_response();
    };
    match service.list_users().await {
        Ok(users) => Json(
            users
                .into_iter()
                .filter(|user| {
                    query
                        .is_disabled
                        .is_none_or(|disabled| user.is_disabled() == disabled)
                })
                .map(|user| user_dto(&user, state.identity.id))
                .collect::<Vec<_>>(),
        )
        .into_response(),
        Err(error) => admin_error_response(&error),
    }
}

pub(crate) async fn user(
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    let principal = match authenticated_principal(&state, &headers, raw_query.as_deref()).await {
        Ok(principal) => principal,
        Err(response) => return response,
    };
    if !principal.user().is_admin() && principal.user().id().as_uuid() != user_id {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Some(service) = state.auth.as_ref() else {
        return HttpAuthError::Unavailable.into_response();
    };
    match service.get_user(UserId::from_uuid(user_id)).await {
        Ok(Some(user)) => Json(user_dto(&user, state.identity.id)).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => admin_error_response(&error),
    }
}

pub(crate) async fn create_user(
    State(state): State<AppState>,
    ClientAddr(ip): ClientAddr,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
    body: Bytes,
) -> Response {
    let admin = match authenticated_administrator(&state, &headers, raw_query.as_deref()).await {
        Ok(principal) => principal,
        Err(response) => return response,
    };
    let payload: CreateUserByName = match json_payload(&headers, &body) {
        Ok(payload) => payload,
        Err(status) => return status.into_response(),
    };
    let Some(service) = state.auth.as_ref() else {
        return HttpAuthError::Unavailable.into_response();
    };
    match service
        .create_user(&payload.name, &payload.password, false)
        .await
    {
        Ok(user) => {
            Event::new("user_created", "success", ip)
                .actor(admin.user().name())
                .target(&user.id().as_uuid().to_string())
                .emit();
            Json(user_dto(&user, state.identity.id)).into_response()
        }
        Err(error) => admin_error_response(&error),
    }
}

pub(crate) async fn update_user(
    State(state): State<AppState>,
    Query(query): Query<UpdateUserQuery>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
    body: Bytes,
) -> Response {
    if let Err(response) = authenticated_administrator(&state, &headers, raw_query.as_deref()).await
    {
        return response;
    }
    let payload: UpdateUserName = match json_payload(&headers, &body) {
        Ok(payload) => payload,
        Err(status) => return status.into_response(),
    };
    let Some(service) = state.auth.as_ref() else {
        return HttpAuthError::Unavailable.into_response();
    };
    match service
        .rename_user(UserId::from_uuid(query.user_id), &payload.name)
        .await
    {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => admin_error_response(&error),
    }
}

pub(crate) async fn update_user_password(
    State(state): State<AppState>,
    ClientAddr(ip): ClientAddr,
    Path(user_id): Path<Uuid>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
    body: Bytes,
) -> Response {
    let admin = match authenticated_administrator(&state, &headers, raw_query.as_deref()).await {
        Ok(principal) => principal,
        Err(response) => return response,
    };
    let payload: UpdateUserPassword = match json_payload(&headers, &body) {
        Ok(payload) => payload,
        Err(status) => return status.into_response(),
    };
    let Some(service) = state.auth.as_ref() else {
        return HttpAuthError::Unavailable.into_response();
    };
    match service
        .update_user_password(
            UserId::from_uuid(user_id),
            &payload.new_password,
            payload.reset_password,
        )
        .await
    {
        Ok(_) => {
            let event = if payload.reset_password {
                "password_reset_by_admin"
            } else {
                "password_changed_by_admin"
            };
            Event::new(event, "success", ip)
                .actor(admin.user().name())
                .target(&user_id.to_string())
                .emit();
            StatusCode::NO_CONTENT.into_response()
        }
        Err(error) => admin_error_response(&error),
    }
}

pub(crate) async fn update_user_policy(
    State(state): State<AppState>,
    ClientAddr(ip): ClientAddr,
    Path(user_id): Path<Uuid>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
    body: Bytes,
) -> Response {
    let admin = match authenticated_administrator(&state, &headers, raw_query.as_deref()).await {
        Ok(principal) => principal,
        Err(response) => return response,
    };
    let payload: UpdateUserPolicy = match json_payload(&headers, &body) {
        Ok(payload) => payload,
        Err(status) => return status.into_response(),
    };
    if !supported_provider(
        payload.authentication_provider_id.as_deref(),
        "TJXY.LocalAuthentication",
    ) || !supported_provider(
        payload.password_reset_provider_id.as_deref(),
        "TJXY.LocalPasswordReset",
    ) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let Some(service) = state.auth.as_ref() else {
        return HttpAuthError::Unavailable.into_response();
    };
    match service
        .update_user_policy(
            UserId::from_uuid(user_id),
            payload.is_administrator,
            payload.is_disabled,
        )
        .await
    {
        Ok(_) => {
            let detail = format!(
                "administrator={} disabled={}",
                payload.is_administrator, payload.is_disabled
            );
            Event::new("user_policy_changed", "success", ip)
                .actor(admin.user().name())
                .target(&user_id.to_string())
                .detail(&detail)
                .emit();
            StatusCode::NO_CONTENT.into_response()
        }
        Err(error) => admin_error_response(&error),
    }
}

pub(crate) async fn delete_user(
    State(state): State<AppState>,
    ClientAddr(ip): ClientAddr,
    Path(user_id): Path<Uuid>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    let admin = match authenticated_administrator(&state, &headers, raw_query.as_deref()).await {
        Ok(principal) => principal,
        Err(response) => return response,
    };
    let Some(service) = state.auth.as_ref() else {
        return HttpAuthError::Unavailable.into_response();
    };
    match service.delete_user(UserId::from_uuid(user_id)).await {
        Ok(()) => {
            Event::new("user_deleted", "success", ip)
                .actor(admin.user().name())
                .target(&user_id.to_string())
                .emit();
            StatusCode::NO_CONTENT.into_response()
        }
        Err(error) => admin_error_response(&error),
    }
}

pub(crate) async fn authenticated_principal(
    state: &AppState,
    headers: &HeaderMap,
    query: Option<&str>,
) -> Result<AuthenticatedPrincipal, Response> {
    let token = access_token(
        headers,
        query,
        state.legacy_auth_enabled,
        state.legacy_query_token_enabled,
    )
    .map_err(IntoResponse::into_response)?;
    let auth = state
        .auth
        .as_ref()
        .ok_or_else(|| HttpAuthError::Unavailable.into_response())?;
    auth.authenticate_token(&token)
        .await
        .map_err(|error| HttpAuthError::from(error).into_response())
}

pub(crate) async fn authenticated_administrator(
    state: &AppState,
    headers: &HeaderMap,
    query: Option<&str>,
) -> Result<AuthenticatedPrincipal, Response> {
    let principal = authenticated_principal(state, headers, query).await?;
    if principal.user().is_admin() {
        Ok(principal)
    } else {
        Err(StatusCode::FORBIDDEN.into_response())
    }
}

#[allow(clippy::result_large_err)] // Route guards return the ready-to-send Axum response directly.
pub(crate) fn authenticated_session_id(
    principal: &AuthenticatedPrincipal,
) -> Result<Uuid, Response> {
    principal
        .session_id()
        .ok_or_else(|| StatusCode::FORBIDDEN.into_response())
}

pub(crate) fn request_query(query: Option<&str>) -> Result<HashMap<String, String>, ()> {
    query
        .map_or_else(|| Ok(HashMap::new()), parse_query)
        .map_err(|_| ())
}

pub(crate) fn request_query_pairs(query: Option<&str>) -> Result<Vec<(String, String)>, ()> {
    query
        .map_or_else(|| Ok(Vec::new()), parse_query_pairs)
        .map_err(|_| ())
}

pub(crate) fn user_dto(user: &AuthUser, server_id: uuid::Uuid) -> UserDto {
    UserDto::new(
        user.id().as_uuid(),
        user.name(),
        server_id,
        user.has_password(),
        UserPolicy::direct_play_only(user.is_admin()).with_disabled(user.is_disabled()),
    )
}

fn json_payload<Payload>(headers: &HeaderMap, body: &[u8]) -> Result<Payload, StatusCode>
where
    Payload: serde::de::DeserializeOwned,
{
    if !is_json_content_type(headers) {
        return Err(StatusCode::BAD_REQUEST);
    }
    serde_json::from_slice(body).map_err(|_| StatusCode::BAD_REQUEST)
}

fn supported_provider(value: Option<&str>, expected: &str) -> bool {
    value.is_none_or(|value| value == expected)
}

fn admin_error_response(error: &AuthError) -> Response {
    match error {
        AuthError::PasswordRequired => bad_request("password must not be empty"),
        AuthError::PasswordTooShort => bad_request("password must be at least 8 characters"),
        AuthError::PasswordResetRequiresNewPassword => {
            bad_request("resetting a password requires a new password (NewPw)")
        }
        AuthError::InvalidUsername | AuthError::InvalidPassword | AuthError::InvalidProfile => {
            StatusCode::BAD_REQUEST.into_response()
        }
        AuthError::Repository(AuthRepositoryError::UserNotFound) => {
            StatusCode::NOT_FOUND.into_response()
        }
        AuthError::Repository(
            AuthRepositoryError::LastEnabledAdmin
            | AuthRepositoryError::UserReferenced
            | AuthRepositoryError::UsernameConflict,
        ) => StatusCode::CONFLICT.into_response(),
        AuthError::Forbidden => StatusCode::FORBIDDEN.into_response(),
        _ => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}

fn bad_request(message: &'static str) -> Response {
    (StatusCode::BAD_REQUEST, Json(ErrorBody { message })).into_response()
}

#[allow(clippy::result_large_err)] // Axum responses are returned directly by all handler callers.
pub(crate) fn client_identity_response(
    headers: &HeaderMap,
    legacy_enabled: bool,
) -> Result<ClientIdentity, Response> {
    client_identity(headers, legacy_enabled).map_err(IntoResponse::into_response)
}

pub(crate) fn client_identity(
    headers: &HeaderMap,
    legacy_enabled: bool,
) -> Result<ClientIdentity, HttpAuthError> {
    let value = if let Some(value) = headers.get(header::AUTHORIZATION) {
        value
    } else if legacy_enabled {
        headers
            .get("X-Emby-Authorization")
            .ok_or(HttpAuthError::BadRequest)?
    } else {
        return Err(HttpAuthError::BadRequest);
    };
    let parameters = parse_authorization(
        std::str::from_utf8(value.as_bytes()).map_err(|_| HttpAuthError::BadRequest)?,
        legacy_enabled,
    )?;
    ClientIdentity::new(
        required(&parameters, "Client")?,
        required(&parameters, "Device")?,
        required(&parameters, "DeviceId")?,
        required(&parameters, "Version")?,
    )
    .map_err(|_| HttpAuthError::BadRequest)
}

fn access_token(
    headers: &HeaderMap,
    query: Option<&str>,
    legacy_enabled: bool,
    query_token_enabled: bool,
) -> Result<String, HttpAuthError> {
    if let Some(value) = headers.get(header::AUTHORIZATION) {
        let parameters = parse_authorization(
            std::str::from_utf8(value.as_bytes()).map_err(|_| HttpAuthError::Unauthorized)?,
            legacy_enabled,
        )
        .map_err(|_| HttpAuthError::Unauthorized)?;
        if let Some(token) = parameters.get("Token").filter(|token| !token.is_empty()) {
            return valid_token(token);
        }
    }
    if legacy_enabled {
        for name in ["X-Emby-Token", "X-MediaBrowser-Token"] {
            if let Some(value) = headers.get(name) {
                return valid_token(value.to_str().map_err(|_| HttpAuthError::Unauthorized)?);
            }
        }
    }
    if query_token_enabled && let Some(query) = query {
        let mut token = None;
        for (name, value) in parse_query_pairs(query)? {
            if (name == "ApiKey" || legacy_enabled && name == "api_key")
                && token.replace(value).is_some()
            {
                return Err(HttpAuthError::Unauthorized);
            }
        }
        if let Some(token) = token {
            return valid_token(&token);
        }
    }
    if let Some(cookie) = headers
        .get(header::COOKIE)
        .and_then(|value| value.to_str().ok())
        && let Some(token) = cookie
            .split(';')
            .map(str::trim)
            .find_map(|part| part.strip_prefix(&format!("{SESSION_COOKIE}=")))
    {
        return valid_token(token);
    }
    Err(HttpAuthError::Unauthorized)
}

pub(crate) fn is_json_content_type(headers: &HeaderMap) -> bool {
    let Ok(value) = headers
        .get(header::CONTENT_TYPE)
        .ok_or(())
        .and_then(|value| value.to_str().map_err(|_| ()))
    else {
        return false;
    };
    let media_type = value.split(';').next().unwrap_or_default().trim();
    media_type.eq_ignore_ascii_case("application/json")
        || media_type.eq_ignore_ascii_case("text/json")
        || media_type.starts_with("application/") && media_type.ends_with("+json")
}

fn valid_token(value: &str) -> Result<String, HttpAuthError> {
    if !valid_token_transport(value) {
        return Err(HttpAuthError::Unauthorized);
    }
    Ok(value.to_owned())
}

pub(crate) fn valid_token_transport(value: &str) -> bool {
    !value.is_empty() && value.len() <= 1_024 && !value.chars().any(char::is_control)
}

fn required<'value>(
    parameters: &'value HashMap<String, String>,
    name: &str,
) -> Result<&'value str, HttpAuthError> {
    parameters
        .get(name)
        .map(String::as_str)
        .filter(|value| !value.is_empty())
        .ok_or(HttpAuthError::BadRequest)
}

fn parse_authorization(
    value: &str,
    legacy_enabled: bool,
) -> Result<HashMap<String, String>, HttpAuthError> {
    let (scheme, parameters) = value
        .split_once(char::is_whitespace)
        .ok_or(HttpAuthError::BadRequest)?;
    if !(scheme.eq_ignore_ascii_case("MediaBrowser")
        || legacy_enabled && scheme.eq_ignore_ascii_case("Emby"))
    {
        return Err(HttpAuthError::BadRequest);
    }
    parse_parameters(parameters)
}

fn parse_parameters(mut input: &str) -> Result<HashMap<String, String>, HttpAuthError> {
    let mut output = HashMap::new();
    loop {
        input = input
            .trim_start_matches(|character: char| character.is_whitespace() || character == ',');
        if input.is_empty() {
            return Ok(output);
        }
        let equals = input.find('=').ok_or(HttpAuthError::BadRequest)?;
        let key = input[..equals].trim();
        if key.is_empty() || key.contains(',') {
            return Err(HttpAuthError::BadRequest);
        }
        input = &input[equals + 1..];
        let (value, remaining) = if let Some(quoted) = input.strip_prefix('"') {
            parse_quoted(quoted)?
        } else {
            let comma = input.find(',').unwrap_or(input.len());
            (input[..comma].trim().to_owned(), &input[comma..])
        };
        let value = percent_decode(&value, false)?;
        if output.insert(key.to_owned(), value).is_some() {
            return Err(HttpAuthError::BadRequest);
        }
        input = remaining;
    }
}

fn parse_query(input: &str) -> Result<HashMap<String, String>, HttpAuthError> {
    let mut output = HashMap::new();
    for (key, value) in parse_query_pairs(input)? {
        if output.insert(key, value).is_some() {
            return Err(HttpAuthError::BadRequest);
        }
    }
    Ok(output)
}

fn parse_query_pairs(input: &str) -> Result<Vec<(String, String)>, HttpAuthError> {
    input
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            Ok((percent_decode(key, true)?, percent_decode(value, true)?))
        })
        .collect()
}

fn percent_decode(value: &str, plus_as_space: bool) -> Result<String, HttpAuthError> {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 2 < bytes.len() => {
                let high = hex_digit(bytes[index + 1]).ok_or(HttpAuthError::BadRequest)?;
                let low = hex_digit(bytes[index + 2]).ok_or(HttpAuthError::BadRequest)?;
                output.push((high << 4) | low);
                index += 3;
            }
            b'%' => return Err(HttpAuthError::BadRequest),
            b'+' if plus_as_space => {
                output.push(b' ');
                index += 1;
            }
            byte => {
                output.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8(output).map_err(|_| HttpAuthError::BadRequest)
}

const fn hex_digit(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn parse_quoted(input: &str) -> Result<(String, &str), HttpAuthError> {
    let mut output = String::new();
    let mut escaped = false;
    for (index, character) in input.char_indices() {
        if escaped {
            output.push(character);
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == '"' {
            let remaining = &input[index + character.len_utf8()..];
            if !remaining.trim_start().is_empty() && !remaining.trim_start().starts_with(',') {
                return Err(HttpAuthError::BadRequest);
            }
            return Ok((output, remaining));
        } else {
            output.push(character);
        }
    }
    Err(HttpAuthError::BadRequest)
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum HttpAuthError {
    BadRequest,
    Unauthorized,
    Forbidden,
    Conflict,
    Unavailable,
    TooManyRequests,
}

impl From<AuthError> for HttpAuthError {
    fn from(error: AuthError) -> Self {
        match error {
            AuthError::InvalidCredentials | AuthError::InvalidToken => Self::Unauthorized,
            AuthError::Forbidden | AuthError::SessionRequired => Self::Forbidden,
            AuthError::InvalidUsername
            | AuthError::InvalidPassword
            | AuthError::InvalidProfile
            | AuthError::PasswordRequired
            | AuthError::PasswordTooShort
            | AuthError::PasswordResetRequiresNewPassword
            | AuthError::InvalidClientIdentity
            | AuthError::InvalidCapabilities
            | AuthError::InvalidSessionFilter
            | AuthError::InvalidDeviceRequest
            | AuthError::InvalidApiKeyRequest
            | AuthError::InvalidSessionLifetime
            | AuthError::InvalidPasswordConcurrency => Self::BadRequest,
            AuthError::ApiKeyCapacity => Self::Conflict,
            AuthError::TimestampOverflow
            | AuthError::PasswordEngine
            | AuthError::PasswordWorker
            | AuthError::Repository(_)
            | AuthError::DeviceRepository(_)
            | AuthError::CredentialCipherUnavailable
            | AuthError::ApiKeyRepository(_)
            | AuthError::CredentialCipher(_) => Self::Unavailable,
            AuthError::Busy => Self::TooManyRequests,
        }
    }
}

impl IntoResponse for HttpAuthError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::BadRequest => (StatusCode::BAD_REQUEST, "invalid authentication request"),
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "invalid credentials or token"),
            Self::Forbidden => (StatusCode::FORBIDDEN, "authentication is not permitted"),
            Self::Conflict => (StatusCode::CONFLICT, "authentication resource conflict"),
            Self::Unavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                "authentication is unavailable",
            ),
            Self::TooManyRequests => (
                StatusCode::TOO_MANY_REQUESTS,
                "authentication capacity is busy",
            ),
        };
        let mut response = (status, Json(ErrorBody { message })).into_response();
        if matches!(self, Self::TooManyRequests) {
            response.headers_mut().insert(
                header::RETRY_AFTER,
                axum::http::HeaderValue::from_static("1"),
            );
        }
        response
    }
}

pub(crate) fn authentication_error_response(error: AuthError) -> Response {
    HttpAuthError::from(error).into_response()
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct ErrorBody {
    message: &'static str,
}

#[cfg(test)]
mod tests {
    use std::{net::Ipv4Addr, sync::Arc};

    use chrono::Duration;
    use sea_orm_migration::MigratorTrait;
    use tjxy_application::{AuthService, SystemClock};
    use tjxy_test_support::test_database;

    use super::*;
    use crate::ServerIdentity;

    const AUTHORIZATION_HEADER: &str =
        r#"MediaBrowser Client="Test", Device="Phone", DeviceId="device-1", Version="1.0""#;

    #[test]
    fn parser_handles_quoted_commas_and_rejects_duplicates() {
        let parsed = parse_parameters(r#"Client="A, B", Device="Phone""#).unwrap();
        assert_eq!(parsed["Client"], "A, B");
        assert!(parse_parameters(r#"Client="A", Client="B""#).is_err());
    }

    async fn state() -> AppState {
        let database = test_database().await.unwrap();
        tjxy_db::Migrator::up(&database, None).await.unwrap();
        let auth = AuthService::new(database, SystemClock, Some(Duration::days(30)), 2)
            .await
            .unwrap();
        auth.create_user("Alice", "correct horse", false)
            .await
            .unwrap();
        AppState::new(ServerIdentity::new(Uuid::new_v4(), "TJXY", "Linux"))
            .with_auth(Arc::new(auth))
    }

    async fn login(state: &AppState, username: &str, password: &str) -> Response {
        let mut headers = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, AUTHORIZATION_HEADER.parse().unwrap());
        headers.insert(header::CONTENT_TYPE, "application/json".parse().unwrap());
        let body = serde_json::json!({"Username": username, "Pw": password}).to_string();
        authenticate_by_name_inner(
            state,
            IpAddr::V4(Ipv4Addr::new(192, 0, 2, 10)),
            &headers,
            body.as_bytes(),
        )
        .await
    }

    #[tokio::test]
    async fn unparseable_usernames_never_reach_the_account_table() {
        let state = state().await;
        for username in [
            "x".repeat(2 * 1024 * 1024 / 8),
            "y".repeat(129),
            " padded".to_owned(),
            "bad\u{0}name".to_owned(),
            String::new(),
        ] {
            assert_eq!(
                login(&state, &username, "whatever").await.status(),
                StatusCode::UNAUTHORIZED
            );
        }
        let (accounts, ips) = state.login_guard.table_sizes();
        assert_eq!(accounts, 0, "malformed names must not create account keys");
        assert_eq!(ips, 1, "they are charged to the client address only");
    }

    #[tokio::test]
    async fn valid_names_share_one_bucket_across_case_and_a_successful_login_clears_it() {
        let state = state().await;
        for username in ["alice", "ALICE", "Alice", "ＡＬＩＣＥ"] {
            assert_eq!(
                login(&state, username, "wrong password").await.status(),
                StatusCode::UNAUTHORIZED
            );
        }
        assert_eq!(state.login_guard.table_sizes().0, 1);
        assert_eq!(
            login(&state, "alice", "correct horse").await.status(),
            StatusCode::OK
        );
        assert_eq!(
            state.login_guard.table_sizes(),
            (0, 1),
            "success forgets the account's failures; the address key remains"
        );
    }
}
