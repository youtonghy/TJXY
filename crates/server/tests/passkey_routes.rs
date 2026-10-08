use std::sync::Arc;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use chrono::{Duration, Utc};
use http_body_util::BodyExt;
use sea_orm::{
    ConnectionTrait,
    sea_query::{Alias, Expr, Query},
};
use sea_orm_migration::MigratorTrait;
use serde_json::{Value, json};
use tjxy_application::{AuthService, SystemClock};
use tjxy_common::Username;
use tjxy_db::{
    AuthRepository, PasskeyCredential, PasskeyRepository, SystemSettingsInput,
    SystemSettingsRepository,
};
use tjxy_server::{AppState, ServerIdentity, build_router};
use tjxy_test_support::test_database;
use tower::ServiceExt;
use uuid::Uuid;
use webauthn_rs::prelude::Passkey;

const CREDENTIAL_ID: &str = "AQIDBA";

async fn app() -> (axum::Router, sea_orm::DatabaseConnection, Uuid) {
    let database = test_database().await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    SystemSettingsRepository::new(&database)
        .put(
            &SystemSettingsInput {
                passkey_enabled: true,
                ..SystemSettingsInput::default()
            },
            None,
        )
        .await
        .unwrap();
    let auth = Arc::new(
        AuthService::new(database.clone(), SystemClock, Some(Duration::days(30)), 2)
            .await
            .unwrap(),
    );
    let user = auth
        .create_user("Alice", "right-password", false)
        .await
        .unwrap();
    let passkey: Passkey = serde_json::from_value(json!({
        "cred": {
            "cred_id": CREDENTIAL_ID,
            "cred": {
                "type_": "ES256",
                "key": { "EC_EC2": {
                    "curve": "SECP256R1",
                    "x": [194,126,127,109,252,23,131,21,252,6,223,99,44,254,140,27,230,17,94,5,133,28,104,41,144,69,171,149,161,26,200,243],
                    "y": [143,123,183,156,24,178,21,248,117,159,162,69,171,52,188,252,26,59,6,47,103,92,19,58,117,103,249,0,219,8,95,196]
                }}
            },
            "counter": 0,
            "transports": null,
            "user_verified": true,
            "backup_eligible": false,
            "backup_state": false,
            "registration_policy": "required",
            "extensions": {},
            "attestation": { "data": "None", "metadata": "None" },
            "attestation_format": "None"
        }
    }))
    .unwrap();
    let now = Utc::now();
    PasskeyRepository::new(&database)
        .insert(&PasskeyCredential {
            id: Uuid::new_v4(),
            user_id: user.id().as_uuid(),
            credential_id: CREDENTIAL_ID.to_owned(),
            public_key: serde_json::to_vec(&passkey).unwrap(),
            counter: 0,
            name: "Security key".to_owned(),
            created_at: now,
            last_used_at: now,
        })
        .await
        .unwrap();
    let router = build_router(
        AppState::new(ServerIdentity::new(Uuid::new_v4(), "TJXY", "Linux"))
            .with_auth(auth)
            .with_system_settings(database.clone())
            .with_ready(true),
    );
    (router, database, user.id().as_uuid())
}

async fn start(app: axum::Router, body: Option<Value>) -> axum::response::Response {
    let request = Request::builder()
        .method("POST")
        .uri("/Auth/Passkey/Authenticate/Start");
    let request = match body {
        Some(value) => request
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(value.to_string())),
        None => request.body(Body::empty()),
    };
    app.oneshot(request.unwrap()).await.unwrap()
}

async fn json_response(response: axum::response::Response) -> Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn username_start_targets_the_users_stored_credential() {
    let (app, database, user_id) = app().await;

    let response = start(app, Some(json!({ "username": "alice" }))).await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_response(response).await;
    assert_eq!(
        body["Options"]["publicKey"]["allowCredentials"][0]["id"],
        CREDENTIAL_ID
    );
    assert!(body["Options"].get("mediation").is_none());
    let challenge_id = Uuid::parse_str(body["ChallengeId"].as_str().unwrap()).unwrap();
    let challenge = PasskeyRepository::new(&database)
        .take_challenge(challenge_id, Utc::now())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(challenge.kind, "user-auth");
    assert_eq!(challenge.user_id, Some(user_id));
}

#[tokio::test]
async fn empty_start_preserves_discoverable_authentication() {
    let (app, database, _) = app().await;

    let response = start(app, None).await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_response(response).await;
    assert_eq!(body["Options"]["publicKey"]["allowCredentials"], json!([]));
    assert_eq!(body["Options"]["mediation"], "conditional");
    let challenge_id = Uuid::parse_str(body["ChallengeId"].as_str().unwrap()).unwrap();
    let challenge = PasskeyRepository::new(&database)
        .take_challenge(challenge_id, Utc::now())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(challenge.kind, "authentication");
    assert_eq!(challenge.user_id, None);
}

fn keys(value: &Value) -> Vec<String> {
    let mut keys = value
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    keys.sort();
    keys
}

async fn finish_with_garbage(app: axum::Router, challenge_id: &str) -> StatusCode {
    app.oneshot(
        Request::builder()
            .method("POST")
            .uri("/Auth/Passkey/Authenticate/Finish")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({
                    "challengeId": challenge_id,
                    "response": {
                        "id": CREDENTIAL_ID,
                        "rawId": CREDENTIAL_ID,
                        "response": {
                            "authenticatorData": "",
                            "clientDataJSON": "",
                            "signature": "",
                            "userHandle": null
                        },
                        "type": "public-key",
                        "clientExtensionResults": {}
                    }
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await
    .unwrap()
    .status()
}

#[tokio::test]
async fn unknown_username_gets_a_decoy_challenge_shaped_like_a_real_one() {
    let (app, _, _) = app().await;
    let real = json_response(start(app.clone(), Some(json!({ "username": "alice" }))).await).await;

    let response = start(app.clone(), Some(json!({ "username": "missing" }))).await;
    assert_eq!(response.status(), StatusCode::OK);
    let decoy = json_response(response).await;

    assert_eq!(keys(&real), keys(&decoy));
    assert_eq!(keys(&real["Options"]), keys(&decoy["Options"]));
    assert_eq!(
        keys(&real["Options"]["publicKey"]),
        keys(&decoy["Options"]["publicKey"])
    );
    let real_credential = &real["Options"]["publicKey"]["allowCredentials"];
    let decoy_credential = &decoy["Options"]["publicKey"]["allowCredentials"];
    assert_eq!(decoy_credential.as_array().unwrap().len(), 1);
    assert_eq!(keys(&real_credential[0]), keys(&decoy_credential[0]));
    assert_eq!(real_credential[0]["type"], decoy_credential[0]["type"]);

    // The decoy credential id is stable per name and different between names.
    let again =
        json_response(start(app.clone(), Some(json!({ "username": "MISSING" }))).await).await;
    assert_eq!(
        again["Options"]["publicKey"]["allowCredentials"][0]["id"],
        decoy_credential[0]["id"]
    );
    let other = json_response(start(app.clone(), Some(json!({ "username": "other" }))).await).await;
    assert_ne!(
        other["Options"]["publicKey"]["allowCredentials"][0]["id"],
        decoy_credential[0]["id"]
    );

    // Finishing a decoy fails the same way a bad assertion for a real challenge does.
    assert_eq!(
        finish_with_garbage(app.clone(), decoy["ChallengeId"].as_str().unwrap()).await,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        finish_with_garbage(app, real["ChallengeId"].as_str().unwrap()).await,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn a_user_without_passkeys_is_indistinguishable_from_an_unknown_one() {
    let (app, database, _) = app().await;
    let auth = AuthService::new(database, SystemClock, Some(Duration::days(30)), 2)
        .await
        .unwrap();
    auth.create_user("Bob", "bob password", false)
        .await
        .unwrap();

    let response = start(app, Some(json!({ "username": "bob" }))).await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_response(response).await;
    assert_eq!(
        body["Options"]["publicKey"]["allowCredentials"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn unauthenticated_starts_are_limited_per_client_address() {
    let (app, _, _) = app().await;
    for _ in 0..30 {
        assert_eq!(start(app.clone(), None).await.status(), StatusCode::OK);
    }
    let response = start(app, None).await;
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(response.headers().contains_key(header::RETRY_AFTER));
}

#[tokio::test]
async fn credential_changes_remove_registered_passkeys() {
    for action in ["change", "admin_reset", "disable", "self_account"] {
        let (_, database, user_id) = app().await;
        let auth = AuthService::new(database.clone(), SystemClock, Some(Duration::days(30)), 2)
            .await
            .unwrap();
        // A second administrator keeps the policy change legal.
        auth.create_user("Root", "root password", true)
            .await
            .unwrap();
        let repository = PasskeyRepository::new(&database);
        assert_eq!(repository.list(user_id).await.unwrap().len(), 1, "{action}");
        let typed = tjxy_common::UserId::from_uuid(user_id);

        match action {
            "change" => {
                auth.update_self_password(typed, "right-password", "brand new password")
                    .await
                    .unwrap();
            }
            "admin_reset" => {
                auth.update_user_password(typed, "administrator chosen", true)
                    .await
                    .unwrap();
            }
            "disable" => {
                auth.update_user_policy(typed, false, true).await.unwrap();
            }
            _ => {
                auth.update_self_account(
                    typed,
                    "Alice",
                    "",
                    "right-password",
                    Some("brand new password"),
                )
                .await
                .unwrap();
            }
        }

        assert!(
            repository.list(user_id).await.unwrap().is_empty(),
            "{action}"
        );
    }
}

#[tokio::test]
async fn profile_edits_that_keep_the_password_keep_the_passkeys() {
    let (_, database, user_id) = app().await;
    let auth = AuthService::new(database.clone(), SystemClock, Some(Duration::days(30)), 2)
        .await
        .unwrap();

    auth.update_self_account(
        tjxy_common::UserId::from_uuid(user_id),
        "Alice",
        "a new bio",
        "right-password",
        None,
    )
    .await
    .unwrap();

    assert_eq!(
        PasskeyRepository::new(&database)
            .list(user_id)
            .await
            .unwrap()
            .len(),
        1
    );
}

async fn session_token(app: &axum::Router) -> String {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/Users/AuthenticateByName")
                .header(
                    header::AUTHORIZATION,
                    r#"MediaBrowser Client="Test", Device="Phone", DeviceId="reg-device", Version="1.0""#,
                )
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"Username": "alice", "Pw": "right-password"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    json_response(response).await["AccessToken"]
        .as_str()
        .unwrap()
        .to_owned()
}

async fn register_start(
    app: &axum::Router,
    token: &str,
    body: Option<Value>,
) -> axum::response::Response {
    let request = Request::builder()
        .method("POST")
        .uri("/Users/Me/Passkeys/Register/Start")
        .header(
            header::AUTHORIZATION,
            format!(r#"MediaBrowser Token="{token}""#),
        );
    let request = match body {
        Some(value) => request
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(value.to_string())),
        None => request.body(Body::empty()),
    };
    app.clone().oneshot(request.unwrap()).await.unwrap()
}

#[tokio::test]
async fn registering_a_passkey_requires_the_current_password() {
    let (app, _, _) = app().await;
    let token = session_token(&app).await;

    assert_eq!(
        register_start(&app, &token, None).await.status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        register_start(&app, &token, Some(json!({"CurrentPassword": "nope nope"})))
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let response = register_start(
        &app,
        &token,
        Some(json!({"CurrentPassword": "right-password"})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(json_response(response).await["ChallengeId"].is_string());
}

#[tokio::test]
async fn registering_a_passkey_requires_a_session_principal() {
    let database = test_database().await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    SystemSettingsRepository::new(&database)
        .put(
            &SystemSettingsInput {
                passkey_enabled: true,
                ..SystemSettingsInput::default()
            },
            None,
        )
        .await
        .unwrap();
    let key = tjxy_credentials::CredentialKey::new(1, [7_u8; 32]).unwrap();
    let auth = Arc::new(
        AuthService::new(database.clone(), SystemClock, Some(Duration::days(30)), 2)
            .await
            .unwrap()
            .with_credential_cipher(Arc::new(
                tjxy_credentials::CredentialCipher::new(key, Vec::new()).unwrap(),
            )),
    );
    auth.create_user("Admin", "admin password", true)
        .await
        .unwrap();
    let session = auth
        .authenticate(
            "admin",
            "admin password",
            tjxy_application::ClientIdentity::new("Test", "Phone", "device", "1.0").unwrap(),
        )
        .await
        .unwrap();
    let principal = auth
        .authenticate_token(session.access_token().expose_secret())
        .await
        .unwrap();
    auth.create_api_key(&principal, "Automation").await.unwrap();
    let api_key = auth.list_api_keys(&principal).await.unwrap()[0]
        .access_token()
        .expose_secret()
        .to_owned();
    let app = build_router(
        AppState::new(ServerIdentity::new(Uuid::new_v4(), "TJXY", "Linux"))
            .with_auth(auth)
            .with_system_settings(database)
            .with_ready(true),
    );

    // Even with the right password, an API key is not a session principal.
    let response = register_start(
        &app,
        &api_key,
        Some(json!({"CurrentPassword": "admin password"})),
    )
    .await;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn username_finish_rejects_a_credential_moved_to_another_user() {
    let (app, database, _) = app().await;
    let start_response = start(app.clone(), Some(json!({ "username": "alice" }))).await;
    let start_body = json_response(start_response).await;
    let challenge_id = Uuid::parse_str(start_body["ChallengeId"].as_str().unwrap()).unwrap();
    let other_user = AuthRepository::new(&database)
        .create_user(
            &Username::parse("Bob").unwrap(),
            "$argon2id$test-only",
            true,
            false,
            Utc::now(),
        )
        .await
        .unwrap();
    let update = Query::update()
        .table(Alias::new("passkey_credentials"))
        .value(Alias::new("user_id"), other_user.id().as_uuid())
        .and_where(Expr::col(Alias::new("credential_id")).eq(CREDENTIAL_ID))
        .to_owned();
    database
        .execute(database.get_database_backend().build(&update))
        .await
        .unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/Auth/Passkey/Authenticate/Finish")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "challengeId": challenge_id,
                        "response": {
                            "id": CREDENTIAL_ID,
                            "rawId": CREDENTIAL_ID,
                            "response": {
                                "authenticatorData": "",
                                "clientDataJSON": "",
                                "signature": "",
                                "userHandle": null
                            },
                            "type": "public-key",
                            "clientExtensionResults": {}
                        }
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
