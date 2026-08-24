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
    let user = auth.create_user("Alice", "right", false).await.unwrap();
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

#[tokio::test]
async fn unknown_username_is_rejected() {
    let (app, _, _) = app().await;

    let response = start(app, Some(json!({ "username": "missing" }))).await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
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
