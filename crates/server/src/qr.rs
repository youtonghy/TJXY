use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use axum::{
    Json,
    body::Bytes,
    extract::{Path, RawQuery, State},
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

#[derive(Clone, Default)]
pub(crate) struct QrLoginStore(Arc<Mutex<HashMap<Uuid, Challenge>>>);

struct Challenge {
    poll_digest: [u8; 32],
    approval_digest: [u8; 32],
    client: ClientIdentity,
    expires_at: chrono::DateTime<Utc>,
    approved: Option<AuthenticatedPrincipal>,
    consumed: bool,
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
    let challenge_id = Uuid::new_v4();
    let poll_token = random_token();
    let approval_token = random_token();
    let expires_at = Utc::now() + Duration::seconds(QR_TTL_SECONDS);
    let challenge = Challenge {
        poll_digest: digest(&poll_token),
        approval_digest: digest(&approval_token),
        client: client.clone(),
        expires_at,
        approved: None,
        consumed: false,
    };
    if let Ok(mut challenges) = state.qr_login.0.lock() {
        challenges.retain(|_, value| value.expires_at > Utc::now() && !value.consumed);
        challenges.insert(challenge_id, challenge);
    } else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    let qr_payload = format!("tjxy-login:v1:{challenge_id}:{approval_token}");
    no_store(Json(CreateResponse {
        challenge_id,
        poll_token,
        qr_payload,
        expires_at,
    }))
    .into_response()
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
    let Some(service) = state.auth.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let issued = match service
        .issue_approved_session(principal.user(), client)
        .await
    {
        Ok(value) => value,
        Err(AuthError::Repository(_) | AuthError::InvalidToken) => {
            return StatusCode::GONE.into_response();
        }
        Err(_) => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
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
    no_store(Json(PollResponse {
        state: "Approved",
        expires_at,
        authentication: Some(tjxy_api::AuthenticationResult::new(
            user,
            session,
            issued.access_token().expose_secret(),
            state.identity.id,
        )),
    }))
    .into_response()
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
fn digest(value: &str) -> [u8; 32] {
    Sha256::digest(value.as_bytes()).into()
}

fn no_store<T>(response: Json<T>) -> ([(header::HeaderName, &'static str); 1], Json<T>) {
    ([(header::CACHE_CONTROL, "no-store")], response)
}
