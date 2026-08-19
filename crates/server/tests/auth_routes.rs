use std::sync::Arc;

use axum::{
    body::Body,
    http::{Method, Request, StatusCode, header},
};
use chrono::Duration;
use http_body_util::BodyExt;
use sea_orm_migration::MigratorTrait;
use serde_json::{Value, json};
use tjxy_application::{AuthService, SystemClock};
use tjxy_server::{AppState, ServerIdentity, build_router};
use tjxy_test_support::test_database;
use tower::ServiceExt;
use uuid::Uuid;

const SERVER_ID: &str = "018f17ac-4e99-7ec5-b4fd-8f15ca9f4f11";
const IDENTITY: &str =
    r#"MediaBrowser Client="Findroid", Device="Pixel", DeviceId="phone-1", Version="0.16.0""#;

async fn app() -> axum::Router {
    app_with_legacy(true).await
}

async fn app_with_legacy(legacy_auth_enabled: bool) -> axum::Router {
    let database = test_database().await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    let auth = Arc::new(
        AuthService::new(database, SystemClock, Some(Duration::days(30)), 2)
            .await
            .unwrap(),
    );
    auth.create_user("Alice", "correct horse", true)
        .await
        .unwrap();
    let identity = ServerIdentity::new(Uuid::parse_str(SERVER_ID).unwrap(), "TJXY", "Linux")
        .with_startup_wizard_completed(true);
    build_router(
        AppState::new(identity)
            .with_auth(auth)
            .with_legacy_auth_enabled(legacy_auth_enabled)
            .with_ready(true),
    )
}

async fn login(app: axum::Router, password: &str) -> axum::response::Response {
    login_as(app, "alice", password).await
}

async fn login_as(app: axum::Router, username: &str, password: &str) -> axum::response::Response {
    login_as_with_identity(app, username, password, IDENTITY).await
}

async fn login_as_with_identity(
    app: axum::Router,
    username: &str,
    password: &str,
    identity: &str,
) -> axum::response::Response {
    app.oneshot(
        Request::builder()
            .method("POST")
            .uri("/Users/AuthenticateByName")
            .header(header::AUTHORIZATION, identity)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({"Username": username, "Pw": password}).to_string(),
            ))
            .unwrap(),
    )
    .await
    .unwrap()
}

fn token_header(token: &str) -> String {
    format!(r#"MediaBrowser Token="{token}""#)
}

async fn token_request(
    app: axum::Router,
    method: Method,
    uri: impl AsRef<str>,
    token: &str,
    body: Option<Value>,
) -> axum::response::Response {
    let request = Request::builder()
        .method(method)
        .uri(uri.as_ref())
        .header(header::AUTHORIZATION, token_header(token));
    let request = match body {
        Some(value) => request
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(value.to_string())),
        None => request.body(Body::empty()),
    };

    app.oneshot(request.unwrap()).await.unwrap()
}

async fn json_response(response: axum::response::Response) -> Value {
    let body = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&body).unwrap()
}

#[tokio::test]
async fn canonical_login_returns_a_durable_session_and_me_resolves_it() {
    let app = app().await;
    let response = login(app.clone(), "correct horse").await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let authentication: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(authentication["User"]["Name"], "Alice");
    assert_eq!(authentication["User"]["Policy"]["IsAdministrator"], true);
    assert_eq!(authentication["ServerId"], SERVER_ID);
    assert_eq!(authentication["AccessToken"].as_str().unwrap().len(), 64);

    let token = authentication["AccessToken"].as_str().unwrap();
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/Users/Me")
                .header(
                    header::AUTHORIZATION,
                    format!(r#"MediaBrowser Token="{token}""#),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let user: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(user["Name"], "Alice");
    assert_eq!(user["Id"], authentication["User"]["Id"]);
}

#[tokio::test]
async fn ordinary_users_can_read_only_their_own_jellyfin_user_resource() {
    let database = test_database().await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    let service = Arc::new(
        AuthService::new(database, SystemClock, Some(Duration::days(30)), 2)
            .await
            .unwrap(),
    );
    service
        .create_user("Bob", "ordinary password", false)
        .await
        .unwrap();
    service
        .create_user("Alice", "correct horse", true)
        .await
        .unwrap();
    let identity = ServerIdentity::new(Uuid::parse_str(SERVER_ID).unwrap(), "TJXY", "Linux")
        .with_startup_wizard_completed(true);
    let app = build_router(AppState::new(identity).with_auth(service).with_ready(true));
    let alice = json_response(login(app.clone(), "correct horse").await).await;
    let alice_id = alice["User"]["Id"].as_str().unwrap();
    let bob = json_response(login_as(app.clone(), "bob", "ordinary password").await).await;
    let bob_id = bob["User"]["Id"].as_str().unwrap();
    let bob_token = bob["AccessToken"].as_str().unwrap();

    let own = token_request(
        app.clone(),
        Method::GET,
        format!("/Users/{bob_id}"),
        bob_token,
        None,
    )
    .await;
    assert_eq!(own.status(), StatusCode::OK);
    assert_eq!(json_response(own).await["Name"], "Bob");
    assert_eq!(
        token_request(
            app,
            Method::GET,
            format!("/Users/{alice_id}"),
            bob_token,
            None,
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn qr_login_requires_an_authenticated_approval_and_is_consumed_once() {
    let app = app().await;
    let approver = json_response(login(app.clone(), "correct horse").await).await;
    let approver_token = approver["AccessToken"].as_str().unwrap();
    let target_identity = r#"MediaBrowser Client="TJXY Web", Device="Browser", DeviceId="qr-target", Version="0.1.0""#;
    let challenge_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/Auth/Qr/Challenges")
                .header(header::AUTHORIZATION, target_identity)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(challenge_response.status(), StatusCode::OK);
    assert_eq!(
        challenge_response.headers()[header::CACHE_CONTROL],
        "no-store"
    );
    let challenge = json_response(challenge_response).await;
    let challenge_id = challenge["ChallengeId"].as_str().unwrap();
    let poll_token = challenge["PollToken"].as_str().unwrap();
    let approval_token = challenge["QrPayload"]
        .as_str()
        .unwrap()
        .rsplit(':')
        .next()
        .unwrap();

    let preview = token_request(
        app.clone(),
        Method::POST,
        "/Auth/Qr/Preview",
        approver_token,
        Some(json!({"Token": approval_token})),
    )
    .await;
    assert_eq!(preview.status(), StatusCode::OK);
    let approved_response = token_request(
        app.clone(),
        Method::POST,
        "/Auth/Qr/Approve",
        approver_token,
        Some(json!({"Token": approval_token})),
    )
    .await;
    assert_eq!(approved_response.status(), StatusCode::NO_CONTENT);

    let issued = token_request(
        app.clone(),
        Method::POST,
        format!("/Auth/Qr/Challenges/{challenge_id}/Poll"),
        "not-used",
        Some(json!({"Token": poll_token})),
    )
    .await;
    assert_eq!(issued.status(), StatusCode::OK);
    let issued_body = json_response(issued).await;
    assert_eq!(issued_body["State"], "Approved");
    assert_eq!(
        issued_body["Authentication"]["AccessToken"]
            .as_str()
            .unwrap()
            .len(),
        64
    );

    let consumed = token_request(
        app,
        Method::POST,
        format!("/Auth/Qr/Challenges/{challenge_id}/Poll"),
        "not-used",
        Some(json!({"Token": poll_token})),
    )
    .await;
    assert_eq!(consumed.status(), StatusCode::GONE);
}

#[allow(clippy::too_many_lines)] // Covers the Jellyfin QuickConnect issue, authorize, connect, and consume lifecycle.
#[tokio::test]
async fn jellyfin_quick_connect_issues_one_session_after_authenticated_approval() {
    let app = app().await;
    let enabled = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/QuickConnect/Enabled")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(enabled.status(), StatusCode::OK);
    assert_eq!(json_response(enabled).await, json!(true));
    assert_eq!(
        json_response(
            app.clone()
                .oneshot(
                    Request::builder()
                        .uri("/Users/Public")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap()
        )
        .await,
        json!([])
    );

    let initiated = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/QuickConnect/Initiate")
                .header(header::AUTHORIZATION, IDENTITY)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(initiated.status(), StatusCode::OK);
    assert_eq!(initiated.headers()[header::CACHE_CONTROL], "no-store");
    let challenge = json_response(initiated).await;
    let secret = challenge["Secret"].as_str().unwrap();
    let code = challenge["Code"].as_str().unwrap();
    assert_eq!(code.len(), 6);
    assert_eq!(challenge["Authenticated"], false);

    let pending = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/QuickConnect/Connect?secret={secret}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(pending.status(), StatusCode::OK);
    assert_eq!(json_response(pending).await["Authenticated"], false);

    let approver = json_response(login(app.clone(), "correct horse").await).await;
    let approver_token = approver["AccessToken"].as_str().unwrap();
    let approved_response = token_request(
        app.clone(),
        Method::POST,
        "/QuickConnect/Authorize",
        approver_token,
        Some(json!({"Code": code})),
    )
    .await;
    assert_eq!(approved_response.status(), StatusCode::OK);
    assert_eq!(json_response(approved_response).await, json!(true));

    let connected = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/QuickConnect/Connect?Secret={secret}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(connected.status(), StatusCode::OK);
    assert_eq!(json_response(connected).await["Authenticated"], true);

    let issued = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/Users/AuthenticateWithQuickConnect")
                .header(header::AUTHORIZATION, IDENTITY)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"Secret": secret}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(issued.status(), StatusCode::OK);
    let issued = json_response(issued).await;
    assert_eq!(issued["User"]["Name"], "Alice");
    assert_eq!(issued["AccessToken"].as_str().unwrap().len(), 64);

    let consumed = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/Users/AuthenticateWithQuickConnect")
                .header(header::AUTHORIZATION, IDENTITY)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"Secret": secret}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(consumed.status(), StatusCode::GONE);
}

#[tokio::test]
async fn jellyfin_web_lowercase_auth_route_aliases_are_supported() {
    let app = app().await;
    let public = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/users/public")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(public.status(), StatusCode::OK);
    let body = public.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(serde_json::from_slice::<Value>(&body).unwrap(), json!([]));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/Users/authenticatebyname")
                .header(header::AUTHORIZATION, IDENTITY)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"Username": "alice", "Pw": "correct horse"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn jellyfin_media_player_accepts_a_unicode_device_name_for_login_and_token_auth() {
    let app = app().await;
    let identity = axum::http::HeaderValue::from_bytes(
        r#"MediaBrowser Client="Jellyfin Media Player", Device="有童的洋算盘 (2)", DeviceId="jmp-macos", Version="1.12.0""#
            .as_bytes(),
    )
    .unwrap();
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/Users/authenticatebyname")
                .header(header::AUTHORIZATION, identity.clone())
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"Username": "alice", "Pw": "correct horse"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let authentication = json_response(response).await;
    assert_eq!(
        authentication["SessionInfo"]["DeviceName"],
        "有童的洋算盘 (2)"
    );
    let token = authentication["AccessToken"].as_str().unwrap();
    let authenticated_identity = axum::http::HeaderValue::from_bytes(
        format!(
            r#"MediaBrowser Client="Jellyfin Media Player", Device="有童的洋算盘 (2)", DeviceId="jmp-macos", Version="1.12.0", Token="{token}""#
        )
        .as_bytes(),
    )
    .unwrap();

    let current_user = app
        .oneshot(
            Request::builder()
                .uri("/Users/Me")
                .header(header::AUTHORIZATION, authenticated_identity)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(current_user.status(), StatusCode::OK);
    assert_eq!(json_response(current_user).await["Name"], "Alice");
}

#[tokio::test]
async fn current_user_can_read_and_update_their_profile_with_password_confirmation() {
    let app = app().await;
    let authentication = json_response(login(app.clone(), "correct horse").await).await;
    let token = authentication["AccessToken"].as_str().unwrap();

    let response = token_request(app.clone(), Method::GET, "/Users/Me/Profile", token, None).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        json_response(response).await,
        json!({"Username": "Alice", "Bio": ""})
    );

    let response = token_request(
        app.clone(),
        Method::PATCH,
        "/Users/Me/Profile",
        token,
        Some(json!({
            "Username": "AliceTwo",
            "Bio": "Thrillers and science fiction.",
            "CurrentPassword": "correct horse",
            "NewPassword": "new correct horse"
        })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        json_response(response).await,
        json!({"Username": "AliceTwo", "Bio": "Thrillers and science fiction."})
    );

    let stale = token_request(app.clone(), Method::GET, "/Users/Me", token, None).await;
    assert_eq!(stale.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        login_as(app.clone(), "AliceTwo", "correct horse")
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        login_as(app, "AliceTwo", "new correct horse")
            .await
            .status(),
        StatusCode::OK
    );
}

#[tokio::test]
async fn current_user_password_change_requires_the_existing_password() {
    let app = app().await;
    let authentication = json_response(login(app.clone(), "correct horse").await).await;
    let token = authentication["AccessToken"].as_str().unwrap();

    let rejected = token_request(
        app.clone(),
        Method::POST,
        "/Users/Me/Password",
        token,
        Some(json!({"CurrentPassword": "wrong", "NewPassword": "new correct horse"})),
    )
    .await;
    assert_eq!(rejected.status(), StatusCode::UNAUTHORIZED);

    let accepted = token_request(
        app.clone(),
        Method::POST,
        "/Users/Me/Password",
        token,
        Some(json!({"CurrentPassword": "correct horse", "NewPassword": "new correct horse"})),
    )
    .await;
    assert_eq!(accepted.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        login(app.clone(), "correct horse").await.status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        login(app, "new correct horse").await.status(),
        StatusCode::OK
    );
}

#[tokio::test]
async fn wrong_and_unknown_credentials_have_the_same_response() {
    let wrong = login(app().await, "wrong").await;
    let unknown = app()
        .await
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/Users/AuthenticateByName")
                .header(header::AUTHORIZATION, IDENTITY)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"Username": "nobody", "Pw": "wrong"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(wrong.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(unknown.status(), StatusCode::UNAUTHORIZED);
    let wrong_body = wrong.into_body().collect().await.unwrap().to_bytes();
    let unknown_body = unknown.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(wrong_body, unknown_body);
}

#[tokio::test]
async fn client_identity_is_required_and_exact_legacy_aliases_are_supported() {
    let response = app()
        .await
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/Users/AuthenticateByName")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"Username":"Alice","Pw":"correct horse"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let response = app()
        .await
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/Users/AuthenticateByName")
                .header(
                    "X-Emby-Authorization",
                    IDENTITY.replacen("MediaBrowser", "Emby", 1),
                )
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"Username":"Alice","Pw":"correct horse"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let response = app()
        .await
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/Users/AuthenticateByName")
                .header(
                    header::AUTHORIZATION,
                    r#"MediaBrowser Client="Jellyfin%20Web", Device="Browser", DeviceId="web-1", Version="1""#,
                )
                .header(header::CONTENT_TYPE, "text/json")
                .body(Body::from(
                    r#"{"Username":"Alice","Pw":"correct horse"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let authentication: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(authentication["SessionInfo"]["Client"], "Jellyfin Web");
}

#[tokio::test]
async fn token_header_aliases_work_but_invalid_tokens_do_not() {
    let app = app().await;
    let login = login(app.clone(), "correct horse").await;
    let body = login.into_body().collect().await.unwrap().to_bytes();
    let authentication: Value = serde_json::from_slice(&body).unwrap();
    let token = authentication["AccessToken"].as_str().unwrap();

    for name in ["X-Emby-Token", "X-MediaBrowser-Token"] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/Users/Me")
                    .header(name, token)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "header {name}");
    }

    for query in [format!("ApiKey={token}"), format!("api_key={token}")] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/Users/Me?{query}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "query {query}");
    }

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/Users/Me?ApiKey={token}"))
                .header(header::AUTHORIZATION, IDENTITY)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/Users/Me")
                .header("X-Emby-Token", "invalid")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/Users/Me")
                .header(header::AUTHORIZATION, r#"MediaBrowser Client="Findroid""#)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn legacy_auth_can_be_disabled_without_disabling_canonical_auth() {
    let response = app_with_legacy(false)
        .await
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/Users/AuthenticateByName")
                .header(
                    "X-Emby-Authorization",
                    IDENTITY.replacen("MediaBrowser", "Emby", 1),
                )
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"Username":"Alice","Pw":"correct horse"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let response = login(app_with_legacy(false).await, "correct horse").await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn administrator_can_manage_users_without_leaving_the_server_without_an_admin() {
    let app = app().await;
    let response = login(app.clone(), "correct horse").await;
    let authentication = json_response(response).await;
    let alice_id = authentication["User"]["Id"].as_str().unwrap();
    let alice_token = authentication["AccessToken"].as_str().unwrap();

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/Users")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let response = token_request(app.clone(), Method::GET, "/Users", alice_token, None).await;
    assert_eq!(response.status(), StatusCode::OK);
    let users = json_response(response).await;
    assert_eq!(users.as_array().unwrap().len(), 1);
    assert_eq!(users[0]["Name"], "Alice");

    let response = token_request(
        app.clone(),
        Method::POST,
        "/Users/New",
        alice_token,
        Some(json!({"Name": "Bob", "Password": "bob password"})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let bob = json_response(response).await;
    let bob_id = bob["Id"].as_str().unwrap().to_owned();
    assert_eq!(bob["Policy"]["IsAdministrator"], false);

    let response = login_as(app.clone(), "Bob", "bob password").await;
    assert_eq!(response.status(), StatusCode::OK);
    let bob_authentication = json_response(response).await;
    let bob_token = bob_authentication["AccessToken"].as_str().unwrap();
    let response = token_request(app.clone(), Method::GET, "/Users", bob_token, None).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let response = token_request(
        app.clone(),
        Method::POST,
        format!("/Users?userId={bob_id}"),
        alice_token,
        Some(json!({"Name": "Robert"})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let response = token_request(
        app.clone(),
        Method::POST,
        format!("/Users/{bob_id}/Password"),
        alice_token,
        Some(json!({"NewPw": "new password", "ResetPassword": false})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        login_as(app.clone(), "Robert", "bob password")
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );

    let response = token_request(
        app.clone(),
        Method::POST,
        format!("/Users/{bob_id}/Policy"),
        alice_token,
        Some(json!({
            "IsAdministrator": true,
            "IsDisabled": false,
            "AuthenticationProviderId": "TJXY.LocalAuthentication",
            "PasswordResetProviderId": "TJXY.LocalPasswordReset"
        })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let response = token_request(
        app.clone(),
        Method::DELETE,
        format!("/Users/{alice_id}"),
        alice_token,
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let response = login_as(app.clone(), "Robert", "new password").await;
    assert_eq!(response.status(), StatusCode::OK);
    let authentication = json_response(response).await;
    let bob_token = authentication["AccessToken"].as_str().unwrap();
    let response = token_request(
        app,
        Method::DELETE,
        format!("/Users/{bob_id}"),
        bob_token,
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Keeps the scoped list, filters, and logout lifecycle in one flow.
async fn sessions_are_scoped_filterable_and_logout_revokes_only_the_current_token() {
    let app = app().await;
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/Sessions")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let alice_authentication = json_response(login(app.clone(), "correct horse").await).await;
    let alice_token = alice_authentication["AccessToken"].as_str().unwrap();
    let alice_id = alice_authentication["User"]["Id"].as_str().unwrap();

    let response = token_request(
        app.clone(),
        Method::POST,
        "/Users/New",
        alice_token,
        Some(json!({"Name": "Bob", "Password": "bob password"})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let bob_authentication = json_response(
        login_as_with_identity(
            app.clone(),
            "Bob",
            "bob password",
            r#"MediaBrowser Client="Findroid", Device="Tablet", DeviceId="tablet-2", Version="0.16.0""#,
        )
        .await,
    )
    .await;
    let bob_token = bob_authentication["AccessToken"].as_str().unwrap();
    let bob_id = bob_authentication["User"]["Id"].as_str().unwrap();
    let bob_session_id = bob_authentication["SessionInfo"]["Id"].as_str().unwrap();

    let response = token_request(
        app.clone(),
        Method::POST,
        format!("/Sessions/Capabilities/Full?id={bob_session_id}"),
        bob_token,
        Some(json!({
            "PlayableMediaTypes": ["Video", "Audio"],
            "SupportedCommands": ["Play", "Stop"],
            "SupportsMediaControl": true,
            "SupportsPersistentIdentifier": true
        })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let response = token_request(app.clone(), Method::GET, "/Sessions", bob_token, None).await;
    assert_eq!(response.status(), StatusCode::OK);
    let sessions = json_response(response).await;
    let sessions = sessions.as_array().unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0]["Id"], bob_session_id);
    assert_eq!(sessions[0]["UserId"], bob_id);
    assert_eq!(sessions[0]["DeviceId"], "tablet-2");
    assert_eq!(sessions[0]["PlayableMediaTypes"], json!(["Video", "Audio"]));
    assert_eq!(sessions[0]["SupportedCommands"], json!(["Play", "Stop"]));
    assert_eq!(sessions[0]["SupportsMediaControl"], true);
    assert_eq!(
        sessions[0]["Capabilities"]["SupportsPersistentIdentifier"],
        true
    );
    assert!(sessions[0]["LastActivityDate"].is_string());

    let response = token_request(
        app.clone(),
        Method::GET,
        format!("/Sessions?controllableByUserId={alice_id}"),
        bob_token,
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let response = token_request(app.clone(), Method::GET, "/Sessions", alice_token, None).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(json_response(response).await.as_array().unwrap().len(), 2);

    let response = token_request(
        app.clone(),
        Method::GET,
        "/Sessions?deviceId=tablet-2",
        alice_token,
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(json_response(response).await.as_array().unwrap().len(), 1);

    let response = token_request(
        app.clone(),
        Method::GET,
        format!("/Sessions?controllableByUserId={bob_id}"),
        alice_token,
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(json_response(response).await.as_array().unwrap().len(), 1);

    for uri in [
        "/Sessions?unexpected=1",
        "/Sessions?activeWithinSeconds=not-a-number",
        "/Sessions?activeWithinSeconds=2592001",
    ] {
        let response = token_request(app.clone(), Method::GET, uri, alice_token, None).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "uri {uri}");
    }

    let response = token_request(
        app.clone(),
        Method::POST,
        "/Sessions/Logout",
        bob_token,
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let response = token_request(app.clone(), Method::GET, "/Users/Me", bob_token, None).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let response = token_request(app.clone(), Method::GET, "/Users/Me", alice_token, None).await;
    assert_eq!(response.status(), StatusCode::OK);

    let response = token_request(app, Method::GET, "/Sessions", alice_token, None).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(json_response(response).await.as_array().unwrap().len(), 1);
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Covers the Jellyfin device list, options, and revoke lifecycle.
async fn administrator_can_manage_devices_without_exposing_tokens() {
    let app = app().await;
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/Devices")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let admin_authentication = json_response(login(app.clone(), "correct horse").await).await;
    let admin_token = admin_authentication["AccessToken"]
        .as_str()
        .unwrap()
        .to_owned();
    let response = token_request(
        app.clone(),
        Method::POST,
        "/Users/New",
        &admin_token,
        Some(json!({"Name": "Bob", "Password": "bob password"})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let bob_authentication = json_response(
        login_as_with_identity(
            app.clone(),
            "Bob",
            "bob password",
            r#"MediaBrowser Client="Findroid", Device="Tablet", DeviceId="tablet-2", Version="0.16.0""#,
        )
        .await,
    )
    .await;
    let bob_token = bob_authentication["AccessToken"]
        .as_str()
        .unwrap()
        .to_owned();
    let bob_id = bob_authentication["User"]["Id"].as_str().unwrap();
    let bob_session_id = bob_authentication["SessionInfo"]["Id"].as_str().unwrap();

    let response = token_request(
        app.clone(),
        Method::POST,
        format!("/Sessions/Capabilities/Full?id={bob_session_id}"),
        &bob_token,
        Some(json!({
            "PlayableMediaTypes": ["Video", "Audio"],
            "SupportedCommands": ["Play", "Stop"],
            "SupportsMediaControl": true,
            "SupportsPersistentIdentifier": true,
            "IconUrl": "https://example.invalid/findroid.png"
        })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let response = token_request(app.clone(), Method::GET, "/Devices", &bob_token, None).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let response = token_request(app.clone(), Method::GET, "/Devices", &admin_token, None).await;
    assert_eq!(response.status(), StatusCode::OK);
    let devices = json_response(response).await;
    assert_eq!(devices["TotalRecordCount"], 2);
    assert_eq!(devices["StartIndex"], 0);
    let bob_device = devices["Items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|device| device["Id"] == "tablet-2")
        .unwrap();
    assert_eq!(bob_device["Name"], "Tablet");
    assert_eq!(bob_device["LastUserName"], "Bob");
    assert_eq!(
        bob_device["Capabilities"]["PlayableMediaTypes"],
        json!(["Video", "Audio"])
    );
    assert_eq!(bob_device["Capabilities"]["SupportsMediaControl"], true);
    assert_eq!(
        bob_device["IconUrl"],
        "https://example.invalid/findroid.png"
    );
    assert!(bob_device.get("AccessToken").is_none());

    let response = token_request(
        app.clone(),
        Method::GET,
        format!("/Devices?userId={bob_id}"),
        &admin_token,
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(json_response(response).await["TotalRecordCount"], 2);
    let response = token_request(
        app.clone(),
        Method::GET,
        format!("/Devices?UserId={bob_id}"),
        &admin_token,
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(json_response(response).await["TotalRecordCount"], 2);
    let response = token_request(
        app.clone(),
        Method::GET,
        format!("/Devices?userId={}", Uuid::new_v4()),
        &admin_token,
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let response = token_request(
        app.clone(),
        Method::GET,
        "/Devices/Info?Id=tablet-2",
        &admin_token,
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(json_response(response).await["Id"], "tablet-2");

    let response = token_request(
        app.clone(),
        Method::GET,
        "/Devices/Options?Id=tablet-2",
        &admin_token,
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let response = token_request(
        app.clone(),
        Method::POST,
        "/Devices/Options?Id=tablet-2",
        &admin_token,
        Some(json!({
            "id": 0,
            "deviceId": "ignored-by-controller",
            "customName": "Living room tablet",
            "FutureField": true
        })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let response = token_request(
        app.clone(),
        Method::GET,
        "/Devices/Options?id=tablet-2",
        &admin_token,
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let options = json_response(response).await;
    assert_eq!(options["DeviceId"], "tablet-2");
    assert_eq!(options["CustomName"], "Living room tablet");

    for uri in [
        "/Devices?unexpected=1",
        "/Devices/Info",
        "/Devices/Info?id=",
    ] {
        let response = token_request(app.clone(), Method::GET, uri, &admin_token, None).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "uri {uri}");
    }

    let response = token_request(
        app.clone(),
        Method::DELETE,
        "/Devices?Id=tablet-2&id=missing",
        &admin_token,
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        token_request(app.clone(), Method::GET, "/Users/Me", &bob_token, None)
            .await
            .status(),
        StatusCode::OK
    );

    let response = token_request(
        app.clone(),
        Method::DELETE,
        "/Devices?id=tablet-2",
        &admin_token,
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        token_request(app.clone(), Method::GET, "/Users/Me", &bob_token, None)
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        token_request(app, Method::GET, "/Users/Me", &admin_token, None)
            .await
            .status(),
        StatusCode::OK
    );
}
