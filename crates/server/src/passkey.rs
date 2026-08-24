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
use tjxy_api::{AuthenticationResult as ApiAuthenticationResult, SessionInfoDto};
use tjxy_db::{PasskeyChallenge, PasskeyCredential, PasskeyRepository, SystemSettingsRecord};
use uuid::Uuid;
use webauthn_rs::prelude::*;

const CHALLENGE_TTL_MINUTES: i64 = 5;
const DISCOVERABLE_AUTHENTICATION_KIND: &str = "authentication";
const USER_AUTHENTICATION_KIND: &str = "user-auth";

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AuthenticationStart {
    username: Option<String>,
}

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

#[allow(clippy::result_large_err)]
fn webauthn(settings: &SystemSettingsRecord) -> Result<Webauthn, Response> {
    let origin = settings.public_url().map_or_else(
        || format!("http://127.0.0.1:{}", settings.port()),
        str::to_owned,
    );
    let origin =
        url::Url::parse(&origin).map_err(|_| StatusCode::SERVICE_UNAVAILABLE.into_response())?;
    let rp_id = origin
        .host_str()
        .ok_or_else(|| StatusCode::SERVICE_UNAVAILABLE.into_response())?;
    WebauthnBuilder::new(rp_id, &origin)
        .and_then(|builder| builder.rp_name(settings.site_title()).build())
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE.into_response())
}

#[allow(clippy::result_large_err)]
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
    let Ok(existing) = repo.list(principal.user().id().as_uuid()).await else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
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
    let Ok(state_payload) = serde_json::to_vec(&registration) else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
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
    let Ok(payload) = serde_json::to_vec(&passkey) else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
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

pub(crate) async fn authenticate_start(State(state): State<AppState>, body: Bytes) -> Response {
    let settings = match enabled_settings(&state).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    let request = if body.is_empty() {
        AuthenticationStart::default()
    } else {
        match serde_json::from_slice::<AuthenticationStart>(&body) {
            Ok(value) => value,
            Err(_) => return StatusCode::BAD_REQUEST.into_response(),
        }
    };
    let engine = match webauthn(&settings) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let repo = match repository(&state) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let username = request
        .username
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let (options, state_payload, user_id, kind) = if let Some(username) = username {
        let Some(service) = state.auth.as_ref() else {
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        };
        let user = match service.find_user_by_name(username).await {
            Ok(Some(value)) => value,
            Ok(None) | Err(tjxy_application::AuthError::InvalidUsername) => {
                return StatusCode::UNAUTHORIZED.into_response();
            }
            Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        };
        let records = match repo.list(user.id().as_uuid()).await {
            Ok(value) if value.is_empty() => return StatusCode::UNAUTHORIZED.into_response(),
            Ok(value) => value,
            Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        };
        let Ok(passkeys) = records
            .iter()
            .map(|record| serde_json::from_slice::<Passkey>(&record.public_key))
            .collect::<Result<Vec<_>, _>>()
        else {
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        };
        let Ok((options, authentication)) = engine.start_passkey_authentication(&passkeys) else {
            return StatusCode::BAD_REQUEST.into_response();
        };
        let Ok(payload) = serde_json::to_vec(&authentication) else {
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        };
        (
            options,
            payload,
            Some(user.id().as_uuid()),
            USER_AUTHENTICATION_KIND,
        )
    } else {
        let Ok((options, authentication)) = engine.start_discoverable_authentication() else {
            return StatusCode::BAD_REQUEST.into_response();
        };
        let Ok(payload) = serde_json::to_vec(&authentication) else {
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        };
        (options, payload, None, DISCOVERABLE_AUTHENTICATION_KIND)
    };
    let id = Uuid::new_v4();
    let now = Utc::now();
    let challenge = PasskeyChallenge {
        id,
        user_id,
        kind: kind.to_owned(),
        state: state_payload,
        expires_at: now + Duration::minutes(CHALLENGE_TTL_MINUTES),
    };
    if repo.put_challenge(&challenge, now).await.is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    Json(serde_json::json!({"ChallengeId": id, "Options": options})).into_response()
}

type VerifiedPasskeyAuthentication = (
    Uuid,
    PasskeyCredential,
    Passkey,
    webauthn_rs::prelude::AuthenticationResult,
);

#[allow(clippy::result_large_err)]
async fn credential_for_user(
    repo: &PasskeyRepository<'_>,
    credential_id: &[u8],
    user_id: Uuid,
) -> Result<(PasskeyCredential, Passkey), Response> {
    let credential_id = URL_SAFE_NO_PAD.encode(credential_id);
    let record = match repo.find_by_credential_id(&credential_id).await {
        Ok(Some(value)) if value.user_id == user_id => value,
        Ok(Some(_) | None) => return Err(StatusCode::UNAUTHORIZED.into_response()),
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR.into_response()),
    };
    let passkey = serde_json::from_slice::<Passkey>(&record.public_key)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())?;
    Ok((record, passkey))
}

#[allow(clippy::result_large_err)]
async fn finish_discoverable(
    engine: &Webauthn,
    repo: &PasskeyRepository<'_>,
    response: &PublicKeyCredential,
    challenge: &PasskeyChallenge,
) -> Result<VerifiedPasskeyAuthentication, Response> {
    let authentication = serde_json::from_slice::<DiscoverableAuthentication>(&challenge.state)
        .map_err(|_| StatusCode::BAD_REQUEST.into_response())?;
    let (user_id, credential_id) = engine
        .identify_discoverable_authentication(response)
        .map_err(|_| StatusCode::UNAUTHORIZED.into_response())?;
    let (record, passkey) = credential_for_user(repo, credential_id, user_id).await?;
    let key = DiscoverableKey::from(&passkey);
    let result = engine
        .finish_discoverable_authentication(response, authentication, &[key])
        .map_err(|_| StatusCode::UNAUTHORIZED.into_response())?;
    Ok((user_id, record, passkey, result))
}

#[allow(clippy::result_large_err)]
async fn finish_for_user(
    engine: &Webauthn,
    repo: &PasskeyRepository<'_>,
    response: &PublicKeyCredential,
    challenge: &PasskeyChallenge,
) -> Result<VerifiedPasskeyAuthentication, Response> {
    let user_id = challenge
        .user_id
        .ok_or_else(|| StatusCode::BAD_REQUEST.into_response())?;
    let authentication = serde_json::from_slice::<PasskeyAuthentication>(&challenge.state)
        .map_err(|_| StatusCode::BAD_REQUEST.into_response())?;
    let (record, passkey) =
        credential_for_user(repo, response.get_credential_id(), user_id).await?;
    let result = engine
        .finish_passkey_authentication(response, &authentication)
        .map_err(|_| StatusCode::UNAUTHORIZED.into_response())?;
    Ok((user_id, record, passkey, result))
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
    let challenge = match repo.take_challenge(challenge_id, Utc::now()).await {
        Ok(Some(value)) => value,
        Ok(None) => return StatusCode::BAD_REQUEST.into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let engine = match webauthn(&settings) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let verified = match challenge.kind.as_str() {
        DISCOVERABLE_AUTHENTICATION_KIND => {
            finish_discoverable(&engine, &repo, &response, &challenge).await
        }
        USER_AUTHENTICATION_KIND => finish_for_user(&engine, &repo, &response, &challenge).await,
        _ => Err(StatusCode::BAD_REQUEST.into_response()),
    };
    let (user_id, record, mut passkey, result) = match verified {
        Ok(value) => value,
        Err(response) => return response,
    };
    let _ = passkey.update_credential(&result);
    let Ok(payload) = serde_json::to_vec(&passkey) else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
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
    Json(ApiAuthenticationResult::new(
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
