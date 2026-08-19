use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use axum::{
    Json,
    body::Bytes,
    extract::{Path, Query, RawQuery, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tjxy_application::{AuthError, AuthenticatedPrincipal, ClientIdentity};
use uuid::Uuid;

use crate::{AppState, auth};

const QR_TTL_SECONDS: i64 = 180;
const QUICK_CONNECT_CODE_ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
const QUICK_CONNECT_CODE_LENGTH: usize = 6;

#[derive(Clone, Default)]
pub(crate) struct QrLoginStore(Arc<Mutex<HashMap<Uuid, Challenge>>>);

struct Challenge {
    poll_digest: [u8; 32],
    approval_digest: [u8; 32],
    quick_connect_code: String,
    client: ClientIdentity,
    expires_at: chrono::DateTime<Utc>,
    approved: Option<AuthenticatedPrincipal>,
    consumed: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct QuickConnectResult {
    authenticated: bool,
    secret: String,
    code: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct QuickConnectQuery {
    #[serde(alias = "secret")]
    secret: Option<String>,
    #[serde(alias = "code")]
    code: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
struct QuickConnectSecretRequest {
    secret: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
struct QuickConnectCodeRequest {
    code: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
struct CreateRequest {}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct CreateResponse {
    challenge_id: Uuid,
    poll_token: String,
    qr_payload: String,
    expires_at: chrono::DateTime<Utc>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
struct TokenRequest {
    token: String,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct PreviewResponse {
    challenge_id: Uuid,
    device_name: String,
    client_name: String,
    application_version: String,
    expires_at: chrono::DateTime<Utc>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct PollResponse {
    state: &'static str,
    expires_at: chrono::DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    authentication: Option<tjxy_api::AuthenticationResult>,
}

pub(crate) async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if !auth::is_json_content_type(&headers)
        || serde_json::from_slice::<CreateRequest>(&body).is_err()
    {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let client = match auth::client_identity_response(&headers, state.legacy_auth_enabled) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let Some((challenge_id, poll_token, approval_token, _code, expires_at)) =
        insert_challenge(&state, client)
    else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let qr_payload = format!("tjxy-login:v1:{challenge_id}:{approval_token}");
    no_store(Json(CreateResponse {
        challenge_id,
        poll_token,
        qr_payload,
        expires_at,
    }))
    .into_response()
}

pub(crate) async fn enabled(State(state): State<AppState>) -> Json<bool> {
    Json(state.auth.is_some())
}

pub(crate) async fn quick_connect_initiate(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    let client = match auth::client_identity_response(&headers, state.legacy_auth_enabled) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let Some((_id, secret, _approval_token, code, _expires_at)) = insert_challenge(&state, client)
    else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    no_store(Json(QuickConnectResult {
        authenticated: false,
        secret,
        code,
    }))
    .into_response()
}

pub(crate) async fn connect(
    State(state): State<AppState>,
    Query(query): Query<QuickConnectQuery>,
) -> Response {
    let Some(secret) = query.secret.as_deref() else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let secret_digest = digest(secret);
    let Ok(challenges) = state.qr_login.0.lock() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let Some(challenge) = challenges
        .values()
        .find(|value| value.poll_digest == secret_digest)
    else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if challenge.expires_at <= Utc::now() || challenge.consumed {
        return StatusCode::GONE.into_response();
    }
    no_store(Json(QuickConnectResult {
        authenticated: challenge.approved.is_some(),
        secret: secret.to_owned(),
        code: challenge.quick_connect_code.clone(),
    }))
    .into_response()
}

pub(crate) async fn authorize(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
    Query(query): Query<QuickConnectQuery>,
    body: Bytes,
) -> Response {
    let principal =
        match auth::authenticated_principal(&state, &headers, raw_query.as_deref()).await {
            Ok(value) => value,
            Err(response) => return response,
        };
    if principal.session_id().is_none() {
        return StatusCode::FORBIDDEN.into_response();
    }
    let code = query.code.or_else(|| {
        serde_json::from_slice::<QuickConnectCodeRequest>(&body)
            .ok()
            .map(|request| request.code)
    });
    let Some(code) = code else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Ok(mut challenges) = state.qr_login.0.lock() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let Some(challenge) = challenges
        .values_mut()
        .find(|value| value.quick_connect_code.eq_ignore_ascii_case(&code))
    else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if challenge.expires_at <= Utc::now() || challenge.consumed || challenge.approved.is_some() {
        return StatusCode::GONE.into_response();
    }
    challenge.approved = Some(principal);
    Json(true).into_response()
}

pub(crate) async fn authenticate(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if !auth::is_json_content_type(&headers) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let client = match auth::client_identity_response(&headers, state.legacy_auth_enabled) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let request: QuickConnectSecretRequest = match serde_json::from_slice(&body) {
        Ok(value) => value,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    let secret_digest = digest(&request.secret);
    let approved = {
        let Ok(mut challenges) = state.qr_login.0.lock() else {
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        };
        let Some(challenge) = challenges
            .values_mut()
            .find(|value| value.poll_digest == secret_digest)
        else {
            return StatusCode::NOT_FOUND.into_response();
        };
        if challenge.expires_at <= Utc::now() || challenge.consumed {
            return StatusCode::GONE.into_response();
        }
        let Some(approved) = challenge.approved.clone() else {
            return StatusCode::UNAUTHORIZED.into_response();
        };
        challenge.consumed = true;
        approved
    };
    match issue_authentication(&state, approved, client).await {
        Ok(authentication) => no_store(Json(authentication)).into_response(),
        Err(status) => status.into_response(),
    }
}

pub(crate) async fn preview(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    body: Bytes,
) -> Response {
    let principal = match auth::authenticated_principal(&state, &headers, query.as_deref()).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    if principal.session_id().is_none() || !auth::is_json_content_type(&headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let payload: TokenRequest = match serde_json::from_slice(&body) {
        Ok(value) => value,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    let Some((id, challenge)) = find_by_approval(&state, &payload.token) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if challenge.expires_at <= Utc::now() || challenge.consumed || challenge.approved {
        return StatusCode::GONE.into_response();
    }
    no_store(Json(PreviewResponse {
        challenge_id: id,
        device_name: challenge.client.device_name().to_owned(),
        client_name: challenge.client.client_name().to_owned(),
        application_version: challenge.client.client_version().to_owned(),
        expires_at: challenge.expires_at,
    }))
    .into_response()
}

pub(crate) async fn approve(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    body: Bytes,
) -> Response {
    let principal = match auth::authenticated_principal(&state, &headers, query.as_deref()).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    if principal.session_id().is_none() || !auth::is_json_content_type(&headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let payload: TokenRequest = match serde_json::from_slice(&body) {
        Ok(value) => value,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    let Some((_id, _)) = find_by_approval(&state, &payload.token) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let digest = digest(&payload.token);
    let Ok(mut challenges) = state.qr_login.0.lock() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let Some(challenge) = challenges
        .values_mut()
        .find(|value| value.approval_digest == digest)
    else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if challenge.expires_at <= Utc::now() || challenge.consumed || challenge.approved.is_some() {
        return StatusCode::GONE.into_response();
    }
    challenge.approved = Some(principal);
    StatusCode::NO_CONTENT.into_response()
}

pub(crate) async fn poll(
    State(state): State<AppState>,
    Path(challenge_id): Path<Uuid>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if !auth::is_json_content_type(&headers) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let payload: TokenRequest = match serde_json::from_slice(&body) {
        Ok(value) => value,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    let digest = digest(&payload.token);
    let (client, approved, expires_at) = {
        let Ok(mut challenges) = state.qr_login.0.lock() else {
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        };
        let Some(challenge) = challenges.get_mut(&challenge_id) else {
            return StatusCode::NOT_FOUND.into_response();
        };
        if challenge.poll_digest != digest {
            return StatusCode::NOT_FOUND.into_response();
        }
        if challenge.expires_at <= Utc::now() || challenge.consumed {
            return StatusCode::GONE.into_response();
        }
        let approved = challenge.approved.clone();
        if approved.is_some() {
            challenge.consumed = true;
        }
        (challenge.client.clone(), approved, challenge.expires_at)
    };
    let Some(principal) = approved else {
        return no_store(Json(PollResponse {
            state: "Pending",
            expires_at,
            authentication: None,
        }))
        .into_response();
    };
    let authentication = match issue_authentication(&state, principal, client).await {
        Ok(value) => value,
        Err(status) => return status.into_response(),
    };
    no_store(Json(PollResponse {
        state: "Approved",
        expires_at,
        authentication: Some(authentication),
    }))
    .into_response()
}

async fn issue_authentication(
    state: &AppState,
    approver: AuthenticatedPrincipal,
    client: ClientIdentity,
) -> Result<tjxy_api::AuthenticationResult, StatusCode> {
    let Some(service) = state.auth.as_ref() else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };
    let issued = match service
        .issue_approved_session(approver.user(), client)
        .await
    {
        Ok(value) => value,
        Err(AuthError::Repository(_) | AuthError::InvalidToken) => {
            return Err(StatusCode::GONE);
        }
        Err(_) => return Err(StatusCode::SERVICE_UNAVAILABLE),
    };
    let user = super::auth::user_dto(issued.user(), state.identity.id);
    let session = tjxy_api::SessionInfoDto::active(
        issued.session_id(),
        issued.user().id().as_uuid(),
        issued.user().name(),
        issued.client().client_name(),
        issued.client().device_id(),
        issued.client().device_name(),
        issued.client().client_version(),
        state.identity.id,
    );
    Ok(tjxy_api::AuthenticationResult::new(
        user,
        session,
        issued.access_token().expose_secret(),
        state.identity.id,
    ))
}

fn insert_challenge(
    state: &AppState,
    client: ClientIdentity,
) -> Option<(Uuid, String, String, String, chrono::DateTime<Utc>)> {
    state.auth.as_ref()?;
    let challenge_id = Uuid::new_v4();
    let poll_token = random_token();
    let approval_token = random_token();
    let expires_at = Utc::now() + Duration::seconds(QR_TTL_SECONDS);
    let mut challenges = state.qr_login.0.lock().ok()?;
    challenges.retain(|_, value| value.expires_at > Utc::now() && !value.consumed);
    let quick_connect_code = loop {
        let candidate = quick_connect_code();
        if challenges
            .values()
            .all(|value| value.quick_connect_code != candidate)
        {
            break candidate;
        }
    };
    challenges.insert(
        challenge_id,
        Challenge {
            poll_digest: digest(&poll_token),
            approval_digest: digest(&approval_token),
            quick_connect_code: quick_connect_code.clone(),
            client,
            expires_at,
            approved: None,
            consumed: false,
        },
    );
    Some((
        challenge_id,
        poll_token,
        approval_token,
        quick_connect_code,
        expires_at,
    ))
}

fn find_by_approval(state: &AppState, token: &str) -> Option<(Uuid, ChallengeView)> {
    let digest = digest(token);
    let challenges = state.qr_login.0.lock().ok()?;
    challenges
        .iter()
        .find(|(_, value)| value.approval_digest == digest)
        .map(|(id, value)| {
            (
                *id,
                ChallengeView {
                    client: value.client.clone(),
                    expires_at: value.expires_at,
                    consumed: value.consumed,
                    approved: value.approved.is_some(),
                },
            )
        })
}

struct ChallengeView {
    client: ClientIdentity,
    expires_at: chrono::DateTime<Utc>,
    consumed: bool,
    approved: bool,
}

fn random_token() -> String {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).expect("OS randomness unavailable");
    URL_SAFE_NO_PAD.encode(bytes)
}

fn quick_connect_code() -> String {
    let mut bytes = [0_u8; QUICK_CONNECT_CODE_LENGTH];
    getrandom::fill(&mut bytes).expect("OS randomness unavailable");
    bytes
        .into_iter()
        .map(|byte| {
            QUICK_CONNECT_CODE_ALPHABET[usize::from(byte) % QUICK_CONNECT_CODE_ALPHABET.len()]
        })
        .map(char::from)
        .collect()
}
fn digest(value: &str) -> [u8; 32] {
    Sha256::digest(value.as_bytes()).into()
}

fn no_store<T>(response: Json<T>) -> ([(header::HeaderName, &'static str); 1], Json<T>) {
    ([(header::CACHE_CONTROL, "no-store")], response)
}
