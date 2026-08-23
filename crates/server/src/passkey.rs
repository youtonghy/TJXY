use crate::{AppState, auth};
use axum::{
    Json,
    body::Bytes,
    extract::{Path, RawQuery, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use tjxy_api::{AuthenticationResult, SessionInfoDto};
use tjxy_db::{PasskeyChallenge, PasskeyCredential, PasskeyRepository, SystemSettingsRecord};
use uuid::Uuid;
use webauthn_rs::prelude::*;

const CHALLENGE_TTL_MINUTES: i64 = 5;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Finish<T> {
    challenge_id: Uuid,
    response: T,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct PasskeySummary {
    id: Uuid,
    name: String,
    created_at: chrono::DateTime<Utc>,
    last_used_at: chrono::DateTime<Utc>,
}

async fn enabled_settings(state: &AppState) -> Result<SystemSettingsRecord, Response> {
    let Some(service) = state.system_settings.as_ref() else {
        return Err(StatusCode::SERVICE_UNAVAILABLE.into_response());
    };
    match service.get().await {
        Ok(Some(settings)) if settings.passkey_enabled() => Ok(settings),
        Ok(_) => Err(StatusCode::NOT_FOUND.into_response()),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR.into_response()),
    }
}

fn webauthn(settings: &SystemSettingsRecord) -> Result<Webauthn, Response> {
    let origin = settings
        .public_url()
        .map(str::to_owned)
        .unwrap_or_else(|| format!("http://127.0.0.1:{}", settings.port()));
    let origin =
        url::Url::parse(&origin).map_err(|_| StatusCode::SERVICE_UNAVAILABLE.into_response())?;
    let rp_id = origin
        .host_str()
        .ok_or_else(|| StatusCode::SERVICE_UNAVAILABLE.into_response())?;
    WebauthnBuilder::new(rp_id, &origin)
        .and_then(|builder| builder.rp_name(settings.site_title()).build())
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE.into_response())
}

fn repository(state: &AppState) -> Result<PasskeyRepository<'_>, Response> {
    state
        .auth
        .as_ref()
        .map(|service| PasskeyRepository::new(service.database()))
        .ok_or_else(|| StatusCode::SERVICE_UNAVAILABLE.into_response())
}

pub(crate) async fn register_start(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let settings = match enabled_settings(&state).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    let principal = match auth::authenticated_principal(&state, &headers, query.as_deref()).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    let repo = match repository(&state) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let existing = match repo.list(principal.user().id().as_uuid()).await {
        Ok(value) => value,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let exclude = existing
        .iter()
        .filter_map(|item| URL_SAFE_NO_PAD.decode(&item.credential_id).ok())
        .map(Into::into)
        .collect::<Vec<CredentialID>>();
    let engine = match webauthn(&settings) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let user = principal.user();
    let Ok((options, registration)) = engine.start_passkey_registration(
        user.id().as_uuid(),
        user.name(),
        user.name(),
        (!exclude.is_empty()).then_some(exclude),
    ) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let id = Uuid::new_v4();
    let now = Utc::now();
    let state_payload = match serde_json::to_vec(&registration) {
        Ok(value) => value,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let challenge = PasskeyChallenge {
        id,
        user_id: Some(user.id().as_uuid()),
        kind: "registration".to_owned(),
        state: state_payload,
        expires_at: now + Duration::minutes(CHALLENGE_TTL_MINUTES),
    };
    if repo.put_challenge(&challenge, now).await.is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    Json(serde_json::json!({"ChallengeId": id, "Options": options})).into_response()
}

pub(crate) async fn register_finish(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    body: Bytes,
) -> Response {
    let settings = match enabled_settings(&state).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    let principal = match auth::authenticated_principal(&state, &headers, query.as_deref()).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    let Ok(Finish {
        challenge_id,
        response,
    }) = serde_json::from_slice::<Finish<RegisterPublicKeyCredential>>(&body)
    else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let repo = match repository(&state) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let Ok(Some(challenge)) = repo.take_challenge(challenge_id, Utc::now()).await else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    if challenge.kind != "registration"
        || challenge.user_id != Some(principal.user().id().as_uuid())
    {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let Ok(registration) = serde_json::from_slice::<PasskeyRegistration>(&challenge.state) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let engine = match webauthn(&settings) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let Ok(passkey) = engine.finish_passkey_registration(&response, &registration) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    if matches!(
        repo.find_by_credential_id(&URL_SAFE_NO_PAD.encode(passkey.cred_id().as_ref()))
            .await,
        Ok(Some(_))
    ) {
        return StatusCode::CONFLICT.into_response();
    }
    let now = Utc::now();
    let payload = match serde_json::to_vec(&passkey) {
        Ok(value) => value,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let record = PasskeyCredential {
        id: Uuid::new_v4(),
        user_id: principal.user().id().as_uuid(),
        credential_id: URL_SAFE_NO_PAD.encode(passkey.cred_id().as_ref()),
        public_key: payload,
        counter: 0,
        name: "Passkey".to_owned(),
        created_at: now,
        last_used_at: now,
    };
    match repo.insert(&record).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(_) => StatusCode::CONFLICT.into_response(),
    }
}

pub(crate) async fn authenticate_start(State(state): State<AppState>) -> Response {
    let settings = match enabled_settings(&state).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    let engine = match webauthn(&settings) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let Ok((options, authentication)) = engine.start_discoverable_authentication() else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let id = Uuid::new_v4();
    let now = Utc::now();
    let payload = match serde_json::to_vec(&authentication) {
        Ok(value) => value,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let challenge = PasskeyChallenge {
        id,
        user_id: None,
        kind: "authentication".to_owned(),
        state: payload,
        expires_at: now + Duration::minutes(CHALLENGE_TTL_MINUTES),
    };
    let repo = match repository(&state) {
        Ok(value) => value,
        Err(response) => return response,
    };
    if repo.put_challenge(&challenge, now).await.is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    Json(serde_json::json!({"ChallengeId": id, "Options": options})).into_response()
}

pub(crate) async fn authenticate_finish(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let settings = match enabled_settings(&state).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    let Ok(Finish {
        challenge_id,
        response,
    }) = serde_json::from_slice::<Finish<PublicKeyCredential>>(&body)
    else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let repo = match repository(&state) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let Ok(Some(challenge)) = repo.take_challenge(challenge_id, Utc::now()).await else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    if challenge.kind != "authentication" {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let Ok(authentication) = serde_json::from_slice::<DiscoverableAuthentication>(&challenge.state)
    else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let engine = match webauthn(&settings) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let Ok((user_id, credential_id)) = engine.identify_discoverable_authentication(&response)
    else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let credential_id = URL_SAFE_NO_PAD.encode(credential_id);
    let Ok(Some(record)) = repo.find_by_credential_id(&credential_id).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if record.user_id != user_id {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Ok(mut passkey) = serde_json::from_slice::<Passkey>(&record.public_key) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let key = DiscoverableKey::from(&passkey);
    let Ok(result) = engine.finish_discoverable_authentication(&response, authentication, &[key])
    else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let _ = passkey.update_credential(&result);
    let payload = match serde_json::to_vec(&passkey) {
        Ok(value) => value,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    if repo
        .update_payload(record.id, payload, i64::from(result.counter()), Utc::now())
        .await
        .is_err()
    {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    let Some(service) = state.auth.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let Ok(Some(user)) = service
        .get_user(tjxy_common::UserId::from_uuid(user_id))
        .await
    else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let client = match auth::client_identity(&headers, state.legacy_auth_enabled) {
        Ok(value) => value,
        Err(error) => return error.into_response(),
    };
    let Ok(issued) = service.authenticate_verified_user(user, client).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let user = auth::user_dto(issued.user(), state.identity.id);
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
    Json(AuthenticationResult::new(
        user,
        session,
        issued.access_token().expose_secret(),
        state.identity.id,
    ))
    .into_response()
}

pub(crate) async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let principal = match auth::authenticated_principal(&state, &headers, query.as_deref()).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    let repo = match repository(&state) {
        Ok(value) => value,
        Err(response) => return response,
    };
    match repo.list(principal.user().id().as_uuid()).await {
        Ok(items) => Json(
            items
                .into_iter()
                .map(|item| PasskeySummary {
                    id: item.id,
                    name: item.name,
                    created_at: item.created_at,
                    last_used_at: item.last_used_at,
                })
                .collect::<Vec<_>>(),
        )
        .into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(crate) async fn delete(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let principal = match auth::authenticated_principal(&state, &headers, query.as_deref()).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    let repo = match repository(&state) {
        Ok(value) => value,
        Err(response) => return response,
    };
    match repo.delete(principal.user().id().as_uuid(), id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}
