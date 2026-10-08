//! Regression tests for login throttling, password policy, and
//! trusted-proxy client address handling.

use std::{net::SocketAddr, sync::Arc};

use axum::{
    body::Body,
    extract::ConnectInfo,
    http::{Method, Request, StatusCode, header},
};
use chrono::Duration;
use http_body_util::BodyExt;
use sea_orm_migration::MigratorTrait;
use serde_json::{Value, json};
use tjxy_application::{AuthService, SystemClock};
use tjxy_server::{AppState, ServerIdentity, TrustedProxies, build_router};
use tjxy_test_support::test_database;
use tower::ServiceExt;
use uuid::Uuid;

const ALICE_PASSWORD: &str = "correct horse";
const BOB_PASSWORD: &str = "bob password";

struct Fixture {
    app: axum::Router,
    bob_id: Uuid,
}

async fn fixture_with_proxies(proxies: &str) -> Fixture {
    let database = test_database().await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    let auth = Arc::new(
        AuthService::new(database, SystemClock, Some(Duration::days(30)), 2)
            .await
            .unwrap(),
    );
    auth.create_user("Alice", ALICE_PASSWORD, true)
        .await
        .unwrap();
    let bob = auth.create_user("Bob", BOB_PASSWORD, false).await.unwrap();
    let identity =
        ServerIdentity::new(Uuid::new_v4(), "TJXY", "Linux").with_startup_wizard_completed(true);
    let app = build_router(
        AppState::new(identity)
            .with_auth(auth)
            .with_trusted_proxies(TrustedProxies::parse(proxies).unwrap())
            .with_ready(true),
    );
    Fixture {
        app,
        bob_id: bob.id().as_uuid(),
    }
}

async fn fixture() -> Fixture {
    fixture_with_proxies("").await
}

fn identity(device_id: &str) -> String {
    format!(r#"MediaBrowser Client="Test", Device="Phone", DeviceId="{device_id}", Version="1.0""#)
}

struct Login<'a> {
    username: &'a str,
    password: &'a str,
    device_id: &'a str,
    peer: &'a str,
    forwarded_for: Option<&'a str>,
}

impl<'a> Login<'a> {
    fn new(username: &'a str, password: &'a str) -> Self {
        Self {
            username,
            password,
            device_id: "device-1",
            peer: "198.51.100.200:5000",
            forwarded_for: None,
        }
    }

    fn device(mut self, device_id: &'a str) -> Self {
        self.device_id = device_id;
        self
    }

    fn peer(mut self, peer: &'a str) -> Self {
        self.peer = peer;
        self
    }

    fn forwarded_for(mut self, value: &'a str) -> Self {
        self.forwarded_for = Some(value);
        self
    }

    async fn send(self, app: &axum::Router) -> axum::response::Response {
        let mut request = Request::builder()
            .method("POST")
            .uri("/Users/AuthenticateByName")
            .header(header::AUTHORIZATION, identity(self.device_id))
            .header(header::CONTENT_TYPE, "application/json");
        if let Some(value) = self.forwarded_for {
            request = request.header("x-forwarded-for", value);
        }
        let mut request = request
            .body(Body::from(
                json!({"Username": self.username, "Pw": self.password}).to_string(),
            ))
            .unwrap();
        request
            .extensions_mut()
            .insert(ConnectInfo(self.peer.parse::<SocketAddr>().unwrap()));
        app.clone().oneshot(request).await.unwrap()
    }
}

async fn token_for(app: &axum::Router, username: &str, password: &str, device: &str) -> String {
    let response = Login::new(username, password)
        .device(device)
        .peer("203.0.113.77:1")
        .send(app)
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    json_body(response).await["AccessToken"]
        .as_str()
        .unwrap()
        .to_owned()
}

async fn json_body(response: axum::response::Response) -> Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

async fn send(
    app: &axum::Router,
    method: Method,
    uri: &str,
    token: &str,
    body: Option<Value>,
) -> axum::response::Response {
    let request = Request::builder().method(method).uri(uri).header(
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
async fn changing_device_id_or_username_case_does_not_escape_the_account_lock() {
    let Fixture { app, .. } = fixture().await;
    for attempt in 0..10 {
        let username = ["alice", "ALICE", "Alice"][attempt % 3];
        let device = format!("device-{attempt}");
        let response = Login::new(username, "wrong password")
            .device(&device)
            .send(&app)
            .await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{attempt}");
    }

    // A fresh device and a differently cased name still hit the same account bucket,
    // even with the correct password.
    let response = Login::new("aLiCe", ALICE_PASSWORD)
        .device("brand-new-device")
        .send(&app)
        .await;
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    let retry_after: u64 = response.headers()[header::RETRY_AFTER]
        .to_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!((1..=15 * 60).contains(&retry_after), "{retry_after}");
    assert!(response.headers().contains_key(header::CACHE_CONTROL));

    // Other accounts are unaffected.
    let response = Login::new("bob", BOB_PASSWORD).send(&app).await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn login_bodies_over_sixteen_kibibytes_are_rejected() {
    let Fixture { app, .. } = fixture().await;
    let body = json!({"Username": "a".repeat(17 * 1024), "Pw": "x"}).to_string();
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/Users/AuthenticateByName")
                .header(header::AUTHORIZATION, identity("device-1"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn token_issuing_responses_are_not_cacheable() {
    let Fixture { app, .. } = fixture().await;
    let success = Login::new("alice", ALICE_PASSWORD).send(&app).await;
    assert_eq!(success.status(), StatusCode::OK);
    assert_eq!(success.headers()[header::CACHE_CONTROL], "no-store");
    let failure = Login::new("alice", "wrong password").send(&app).await;
    assert_eq!(failure.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(failure.headers()[header::CACHE_CONTROL], "no-store");
}

#[tokio::test]
async fn disabled_account_is_indistinguishable_from_a_wrong_password() {
    let Fixture { app, bob_id } = fixture().await;
    let admin = token_for(&app, "alice", ALICE_PASSWORD, "admin-device").await;
    let response = send(
        &app,
        Method::POST,
        &format!("/Users/{bob_id}/Policy"),
        &admin,
        Some(json!({"IsAdministrator": false, "IsDisabled": true})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let correct = Login::new("bob", BOB_PASSWORD).send(&app).await;
    let wrong = Login::new("bob", "not the password").send(&app).await;
    let unknown = Login::new("nobody", BOB_PASSWORD).send(&app).await;

    assert_eq!(correct.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(wrong.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(unknown.status(), StatusCode::UNAUTHORIZED);
    let correct_body = json_body(correct).await;
    assert_eq!(correct_body, json_body(wrong).await);
    assert_eq!(correct_body, json_body(unknown).await);
}

#[tokio::test]
async fn empty_and_short_passwords_are_rejected_and_reset_never_blanks_the_password() {
    let Fixture { app, bob_id } = fixture().await;
    let admin = token_for(&app, "alice", ALICE_PASSWORD, "admin-device").await;
    let bob_session = token_for(&app, "bob", BOB_PASSWORD, "bob-device").await;

    for password in ["", "short"] {
        let response = send(
            &app,
            Method::POST,
            "/Users/New",
            &admin,
            Some(json!({"Name": "carol", "Password": password})),
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{password:?}");
    }
    let response = send(
        &app,
        Method::POST,
        "/Users/New",
        &admin,
        Some(json!({"Name": "carol", "Password": "long enough"})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    for new_password in ["", "short"] {
        let response = send(
            &app,
            Method::POST,
            "/Users/Me/Password",
            &bob_session,
            Some(json!({"CurrentPassword": BOB_PASSWORD, "NewPassword": new_password})),
        )
        .await;
        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "{new_password:?}"
        );
    }

    // A reset without a replacement is refused with an explicit message ...
    let response = send(
        &app,
        Method::POST,
        &format!("/Users/{bob_id}/Password"),
        &admin,
        Some(json!({"ResetPassword": true})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let message = json_body(response).await["Message"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(message.contains("NewPw"), "{message}");
    // ... and nothing changed: the old password and session still work, and the
    // empty password does not.
    assert_eq!(
        Login::new("bob", BOB_PASSWORD).send(&app).await.status(),
        StatusCode::OK
    );
    assert_eq!(
        send(&app, Method::GET, "/Users/Me", &bob_session, None)
            .await
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        Login::new("bob", "").send(&app).await.status(),
        StatusCode::UNAUTHORIZED
    );

    // A reset that carries a new password sets it and revokes existing sessions.
    let response = send(
        &app,
        Method::POST,
        &format!("/Users/{bob_id}/Password"),
        &admin,
        Some(json!({"NewPw": "administrator chosen", "ResetPassword": true})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        send(&app, Method::GET, "/Users/Me", &bob_session, None)
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        Login::new("bob", "administrator chosen")
            .send(&app)
            .await
            .status(),
        StatusCode::OK
    );
}

#[tokio::test]
async fn forwarded_for_is_ignored_without_a_trusted_proxy() {
    let Fixture { app, .. } = fixture().await;
    // A name with surrounding whitespace is rejected before any hashing, so only
    // the per-address budget is exercised.
    for index in 0..30 {
        let spoofed = format!("198.51.100.{}", index + 1);
        let response = Login::new(" bad", "x")
            .peer("203.0.113.9:1000")
            .forwarded_for(&spoofed)
            .send(&app)
            .await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
    let response = Login::new(" bad", "x")
        .peer("203.0.113.9:1000")
        .forwarded_for("192.0.2.77")
        .send(&app)
        .await;
    assert_eq!(
        response.status(),
        StatusCode::TOO_MANY_REQUESTS,
        "a spoofed X-Forwarded-For must not mint a fresh budget"
    );
    // A different peer is still independent.
    let response = Login::new(" bad", "x")
        .peer("203.0.113.10:1000")
        .send(&app)
        .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn forwarded_for_selects_the_client_behind_a_trusted_proxy() {
    let Fixture { app, .. } = fixture_with_proxies("127.0.0.1, 10.0.0.0/8").await;
    for _ in 0..30 {
        let response = Login::new(" bad", "x")
            .peer("127.0.0.1:1000")
            .forwarded_for("1.1.1.1, 198.51.100.1, 10.0.0.5")
            .send(&app)
            .await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
    // Same proxy, same attacker-controlled left side, but a different real client.
    let other = Login::new(" bad", "x")
        .peer("127.0.0.1:1000")
        .forwarded_for("1.1.1.1, 198.51.100.2, 10.0.0.5")
        .send(&app)
        .await;
    assert_eq!(other.status(), StatusCode::UNAUTHORIZED);
    let same = Login::new(" bad", "x")
        .peer("127.0.0.1:1000")
        .forwarded_for("9.9.9.9, 198.51.100.1")
        .send(&app)
        .await;
    assert_eq!(same.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn failed_password_confirmation_spends_the_same_budget_as_login() {
    let Fixture { app, .. } = fixture().await;
    let token = token_for(&app, "alice", ALICE_PASSWORD, "device-a").await;
    for _ in 0..10 {
        let response = send(
            &app,
            Method::POST,
            "/Users/Me/Password",
            &token,
            Some(json!({"CurrentPassword": "wrong password", "NewPassword": "another secret"})),
        )
        .await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
    let response = send(
        &app,
        Method::POST,
        "/Users/Me/Password",
        &token,
        Some(json!({"CurrentPassword": ALICE_PASSWORD, "NewPassword": "another secret"})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(response.headers().contains_key(header::RETRY_AFTER));
    let response = Login::new("alice", ALICE_PASSWORD).send(&app).await;
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn quick_connect_code_guessing_is_throttled() {
    let Fixture { app, .. } = fixture().await;
    let token = token_for(&app, "bob", BOB_PASSWORD, "device-b").await;
    for _ in 0..10 {
        let response = send(
            &app,
            Method::POST,
            "/QuickConnect/Authorize?code=ZZZZZZ",
            &token,
            None,
        )
        .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
    let response = send(
        &app,
        Method::POST,
        "/QuickConnect/Authorize?code=ZZZZZZ",
        &token,
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(response.headers().contains_key(header::RETRY_AFTER));
}
