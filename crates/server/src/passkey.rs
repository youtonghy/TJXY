use std::net::IpAddr;

use crate::{
    AppState,
    audit::{self, Event},
    auth,
    login_guard::{AccountKey, ClientAddr, LimitScope, Limited},
};
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
use tjxy_application::AuthError;
use tjxy_db::{PasskeyChallenge, PasskeyCredential, PasskeyRepository, SystemSettingsRecord};
use uuid::Uuid;
use webauthn_rs::prelude::*;

const CHALLENGE_TTL_MINUTES: i64 = 5;
const DISCOVERABLE_AUTHENTICATION_KIND: &str = "authentication";
const USER_AUTHENTICATION_KIND: &str = "user-auth";
/// Stored for names that have no usable passkey so a probe cannot tell them apart.
const DECOY_AUTHENTICATION_KIND: &str = "user-auth-decoy";
const CHALLENGE_CAPACITY_RETRY_SECONDS: u64 = 60;

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
struct RegistrationStart {
    current_password: String,
}

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

/// Registering an authenticator needs a real session and the current password,
/// so a stolen API key or token alone cannot plant a persistent credential.
async fn confirm_registration(
    state: &AppState,
    principal: &tjxy_application::AuthenticatedPrincipal,
    ip: IpAddr,
    body: &[u8],
) -> Result<(), Response> {
    if principal.session_id().is_none() {
        return Err(StatusCode::FORBIDDEN.into_response());
    }
    let Ok(request) = serde_json::from_slice::<RegistrationStart>(body) else {
        return Err(StatusCode::BAD_REQUEST.into_response());
    };
    let Some(service) = state.auth.as_ref() else {
        return Err(StatusCode::SERVICE_UNAVAILABLE.into_response());
    };
    let actor = principal.user().name();
    let attempt = auth::begin_attempt(state, "passkey_register", ip, Some(actor))?;
    let result = service
        .confirm_password(principal.user().id(), &request.current_password)
        .await;
    auth::finish_attempt(
        state,
        &attempt,
        matches!(result, Err(AuthError::InvalidCredentials)),
    );
    result.map_err(|error| {
        if matches!(error, AuthError::InvalidCredentials) {
            Event::new("password_verification_failed", "failure", ip)
                .actor(actor)
                .target("passkey_register")
                .reason("invalid_credentials")
                .emit();
        }
        auth::authentication_error_response(error)
    })
}

pub(crate) async fn register_start(
    State(state): State<AppState>,
    ClientAddr(ip): ClientAddr,
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
    if let Err(response) = confirm_registration(&state, &principal, ip, &body).await {
        return response;
    }
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
    ClientAddr(ip): ClientAddr,
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
    if principal.session_id().is_none() {
        return StatusCode::FORBIDDEN.into_response();
    }
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
        Ok(()) => {
            Event::new("passkey_registered", "success", ip)
                .actor(principal.user().name())
                .target(&record.id.to_string())
                .emit();
            StatusCode::NO_CONTENT.into_response()
        }
        Err(_) => StatusCode::CONFLICT.into_response(),
    }
}

/// How a start request resolved. Unknown names and names without a passkey are
/// indistinguishable from each other and from real accounts on the wire.
enum StartTarget {
    Discoverable,
    User {
        user_id: Uuid,
        passkeys: Vec<Passkey>,
    },
    Decoy {
        identity: Vec<u8>,
    },
}

fn capacity_limited() -> Limited {
    Limited {
        retry_after: std::time::Duration::from_secs(CHALLENGE_CAPACITY_RETRY_SECONDS),
        scope: LimitScope::Capacity,
    }
}

/// Re-shapes discoverable options so they list one stable, fake credential.
fn decoy_options(
    options: &RequestChallengeResponse,
    credential_id: [u8; 32],
) -> Option<serde_json::Value> {
    let mut value = serde_json::to_value(options).ok()?;
    // Username-bound options never carry the discoverable-only `mediation` hint.
    value.as_object_mut()?.remove("mediation");
    let public_key = value.get_mut("publicKey")?.as_object_mut()?;
    // Likewise the discoverable flow's extensions block is absent from user-bound options.
    public_key.remove("extensions");
    public_key.insert(
        "allowCredentials".to_owned(),
        serde_json::json!([{ "type": "public-key", "id": URL_SAFE_NO_PAD.encode(credential_id) }]),
    );
    Some(value)
}

/// Throttles an unauthenticated start: per-address budget, locked accounts, and
/// the cap on pending challenges.
#[allow(clippy::result_large_err)]
fn admit_start(state: &AppState, ip: IpAddr, account: Option<&AccountKey>) -> Result<(), Response> {
    // Every start is charged to the client address and never refunded.
    if let Err(limited) = state.login_guard.begin(None, ip).map(drop) {
        audit::rate_limited("passkey_login_start", ip, None, &limited);
        return Err(auth::rate_limited_response(&limited));
    }
    let limited = account
        .and_then(|account| state.login_guard.check_account(account).err())
        .or_else(|| (!state.login_guard.try_reserve_passkey_challenge()).then(capacity_limited));
    if let Some(limited) = limited {
        audit::rate_limited("passkey_login_start", ip, None, &limited);
        return Err(auth::rate_limited_response(&limited));
    }
    Ok(())
}

async fn start_target(
    state: &AppState,
    repo: &PasskeyRepository<'_>,
    username: &str,
    account: Option<&AccountKey>,
) -> Result<StartTarget, Response> {
    let Some(service) = state.auth.as_ref() else {
        return Err(StatusCode::SERVICE_UNAVAILABLE.into_response());
    };
    let decoy = || StartTarget::Decoy {
        identity: account.map_or_else(
            || username.as_bytes().to_vec(),
            |account| account.as_bytes().to_vec(),
        ),
    };
    let user = match service.find_user_by_name(username).await {
        Ok(Some(user)) => user,
        Ok(None) | Err(AuthError::InvalidUsername) => return Ok(decoy()),
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR.into_response()),
    };
    let Ok(records) = repo.list(user.id().as_uuid()).await else {
        return Err(StatusCode::INTERNAL_SERVER_ERROR.into_response());
    };
    if records.is_empty() {
        return Ok(decoy());
    }
    let Ok(passkeys) = records
        .iter()
        .map(|record| serde_json::from_slice::<Passkey>(&record.public_key))
        .collect::<Result<Vec<_>, _>>()
    else {
        return Err(StatusCode::INTERNAL_SERVER_ERROR.into_response());
    };
    Ok(StartTarget::User {
        user_id: user.id().as_uuid(),
        passkeys,
    })
}

/// Options, persisted ceremony state, owning user, and challenge kind.
type StartMaterial = (serde_json::Value, Vec<u8>, Option<Uuid>, &'static str);

#[allow(clippy::result_large_err)]
fn start_material(
    engine: &Webauthn,
    state: &AppState,
    target: StartTarget,
) -> Result<StartMaterial, Response> {
    let internal = || StatusCode::INTERNAL_SERVER_ERROR.into_response();
    let bad_request = || StatusCode::BAD_REQUEST.into_response();
    match target {
        StartTarget::User { user_id, passkeys } => {
            let (options, authentication) = engine
                .start_passkey_authentication(&passkeys)
                .map_err(|_| bad_request())?;
            Ok((
                serde_json::to_value(options).map_err(|_| internal())?,
                serde_json::to_vec(&authentication).map_err(|_| internal())?,
                Some(user_id),
                USER_AUTHENTICATION_KIND,
            ))
        }
        StartTarget::Decoy { identity } => {
            let (options, authentication) = engine
                .start_discoverable_authentication()
                .map_err(|_| bad_request())?;
            let options = decoy_options(&options, state.login_guard.fake_credential_id(&identity))
                .ok_or_else(internal)?;
            Ok((
                options,
                serde_json::to_vec(&authentication).map_err(|_| internal())?,
                None,
                DECOY_AUTHENTICATION_KIND,
            ))
        }
        StartTarget::Discoverable => {
            let (options, authentication) = engine
                .start_discoverable_authentication()
                .map_err(|_| bad_request())?;
            Ok((
                serde_json::to_value(options).map_err(|_| internal())?,
                serde_json::to_vec(&authentication).map_err(|_| internal())?,
                None,
                DISCOVERABLE_AUTHENTICATION_KIND,
            ))
        }
    }
}

pub(crate) async fn authenticate_start(
    State(state): State<AppState>,
    ClientAddr(ip): ClientAddr,
    body: Bytes,
) -> Response {
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
    let account = username.and_then(AccountKey::from_name);
    if let Err(response) = admit_start(&state, ip, account.as_ref()) {
        return response;
    }
    let target = match username {
        Some(username) => match start_target(&state, &repo, username, account.as_ref()).await {
            Ok(target) => target,
            Err(response) => return response,
        },
        None => StartTarget::Discoverable,
    };
    let (options, state_payload, user_id, kind) = match start_material(&engine, &state, target) {
        Ok(material) => material,
        Err(response) => return response,
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

/// Applies the throttle and audit consequences of a rejected passkey login.
fn reject_login(
    state: &AppState,
    attempt: &crate::login_guard::Attempt,
    ip: IpAddr,
    account: Option<&AccountKey>,
    response: Response,
) -> Response {
    let status = response.status();
    if status.is_client_error() {
        if let Some(account) = account {
            state.login_guard.record_account_failure(account);
        }
        Event::new("passkey_login_failure", "failure", ip)
            .reason("verification_failed")
            .emit();
    } else {
        state.login_guard.release(attempt);
        Event::new("passkey_login_failure", "failure", ip)
            .reason("internal_error")
            .emit();
    }
    response
}

pub(crate) async fn authenticate_finish(
    State(state): State<AppState>,
    ClientAddr(ip): ClientAddr,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    auth::no_store(authenticate_finish_inner(&state, ip, &headers, &body).await)
}

async fn authenticate_finish_inner(
    state: &AppState,
    ip: IpAddr,
    headers: &HeaderMap,
    body: &[u8],
) -> Response {
    let settings = match enabled_settings(state).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    let Ok(Finish {
        challenge_id,
        response,
    }) = serde_json::from_slice::<Finish<PublicKeyCredential>>(body)
    else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let repo = match repository(state) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let attempt = match state.login_guard.begin(None, ip) {
        Ok(attempt) => attempt,
        Err(limited) => {
            audit::rate_limited("passkey_login_finish", ip, None, &limited);
            return auth::rate_limited_response(&limited);
        }
    };
    let challenge = match repo.take_challenge(challenge_id, Utc::now()).await {
        Ok(Some(value)) => value,
        Ok(None) => {
            return reject_login(
                state,
                &attempt,
                ip,
                None,
                StatusCode::BAD_REQUEST.into_response(),
            );
        }
        Err(_) => {
            return reject_login(
                state,
                &attempt,
                ip,
                None,
                StatusCode::INTERNAL_SERVER_ERROR.into_response(),
            );
        }
    };
    let Some(service) = state.auth.as_ref() else {
        state.login_guard.release(&attempt);
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    // A user-bound challenge also answers to the account's own failure budget.
    let account = match challenge.user_id {
        Some(user_id) => match service
            .get_user(tjxy_common::UserId::from_uuid(user_id))
            .await
        {
            Ok(Some(user)) => AccountKey::from_name(user.name()),
            _ => None,
        },
        None => None,
    };
    if let Some(account) = &account
        && let Err(limited) = state.login_guard.check_account(account)
    {
        state.login_guard.release(&attempt);
        audit::rate_limited("passkey_login_finish", ip, None, &limited);
        return auth::rate_limited_response(&limited);
    }
    let engine = match webauthn(&settings) {
        Ok(value) => value,
        Err(response) => return reject_login(state, &attempt, ip, account.as_ref(), response),
    };
    let login = Login {
        state,
        attempt,
        ip,
        account,
    };
    complete_login(
        &login,
        &engine,
        &repo,
        service,
        (&response, &challenge, headers),
    )
    .await
}

/// Everything known about one in-flight passkey login, for throttling and audit.
struct Login<'a> {
    state: &'a AppState,
    attempt: crate::login_guard::Attempt,
    ip: IpAddr,
    account: Option<AccountKey>,
}

impl Login<'_> {
    fn reject(&self, response: Response) -> Response {
        reject_login(
            self.state,
            &self.attempt,
            self.ip,
            self.account.as_ref(),
            response,
        )
    }
}

async fn complete_login(
    login: &Login<'_>,
    engine: &Webauthn,
    repo: &PasskeyRepository<'_>,
    service: &tjxy_application::AuthService<tjxy_application::SystemClock>,
    request: (&PublicKeyCredential, &PasskeyChallenge, &HeaderMap),
) -> Response {
    let (response, challenge, headers) = request;
    let verified = match challenge.kind.as_str() {
        DISCOVERABLE_AUTHENTICATION_KIND => {
            finish_discoverable(engine, repo, response, challenge).await
        }
        USER_AUTHENTICATION_KIND => finish_for_user(engine, repo, response, challenge).await,
        // A decoy challenge can never verify; fail the same way a bad assertion does.
        DECOY_AUTHENTICATION_KIND => Err(StatusCode::UNAUTHORIZED.into_response()),
        _ => Err(StatusCode::BAD_REQUEST.into_response()),
    };
    let (user_id, record, mut passkey, result) = match verified {
        Ok(value) => value,
        Err(response) => {
            return login.reject(response);
        }
    };
    let _ = passkey.update_credential(&result);
    let Ok(payload) = serde_json::to_vec(&passkey) else {
        return login.reject(StatusCode::INTERNAL_SERVER_ERROR.into_response());
    };
    if repo
        .update_payload(record.id, payload, i64::from(result.counter()), Utc::now())
        .await
        .is_err()
    {
        return login.reject(StatusCode::INTERNAL_SERVER_ERROR.into_response());
    }
    let Ok(Some(user)) = service
        .get_user(tjxy_common::UserId::from_uuid(user_id))
        .await
    else {
        return login.reject(StatusCode::UNAUTHORIZED.into_response());
    };
    let client = match auth::client_identity(headers, login.state.legacy_auth_enabled) {
        Ok(value) => value,
        Err(error) => {
            login.state.login_guard.release(&login.attempt);
            return error.into_response();
        }
    };
    let Ok(issued) = service.authenticate_verified_user(user, client).await else {
        return login.reject(StatusCode::UNAUTHORIZED.into_response());
    };
    login.state.login_guard.release(&login.attempt);
    Event::new("passkey_login_success", "success", login.ip)
        .actor(issued.user().name())
        .device(issued.client().device_id())
        .emit();
    let user = auth::user_dto(issued.user(), login.state.identity.id);
    let session = SessionInfoDto::active(
        issued.session_id(),
        issued.user().id().as_uuid(),
        issued.user().name(),
        issued.client().client_name(),
        issued.client().device_id(),
        issued.client().device_name(),
        issued.client().client_version(),
        login.state.identity.id,
    );
    Json(ApiAuthenticationResult::new(
        user,
        session,
        issued.access_token().expose_secret(),
        login.state.identity.id,
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
    ClientAddr(ip): ClientAddr,
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
        Ok(true) => {
            Event::new("passkey_deleted", "success", ip)
                .actor(principal.user().name())
                .target(&id.to_string())
                .emit();
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}
