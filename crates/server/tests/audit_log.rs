//! Audit events must describe what happened without leaking secrets.
//!
//! This lives in its own test binary on purpose: `tracing` caches callsite
//! interest process-wide, so a scoped subscriber is only reliable when no other
//! test thread is emitting the same events concurrently.

use std::{
    io::Write,
    net::SocketAddr,
    sync::{Arc, Mutex},
};

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
use tjxy_server::{AppState, ServerIdentity, build_router};
use tjxy_test_support::test_database;
use tower::ServiceExt;
use uuid::Uuid;

#[derive(Clone, Default)]
struct CapturedLog(Arc<Mutex<Vec<u8>>>);

impl Write for CapturedLog {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for CapturedLog {
    type Writer = Self;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

async fn login(
    app: &axum::Router,
    username: &str,
    password: &str,
    device_id: &str,
) -> axum::response::Response {
    let mut request = Request::builder()
        .method("POST")
        .uri("/Users/AuthenticateByName")
        .header(
            header::AUTHORIZATION,
            format!(
                r#"MediaBrowser Client="Test", Device="Phone", DeviceId="{device_id}", Version="1.0""#
            ),
        )
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({"Username": username, "Pw": password}).to_string(),
        ))
        .unwrap();
    request.extensions_mut().insert(ConnectInfo(
        "198.51.100.44:1".parse::<SocketAddr>().unwrap(),
    ));
    app.clone().oneshot(request).await.unwrap()
}

async fn send(
    app: &axum::Router,
    method: Method,
    uri: &str,
    token: &str,
    body: Option<Value>,
) -> StatusCode {
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
    app.clone()
        .oneshot(request.unwrap())
        .await
        .unwrap()
        .status()
}

#[tokio::test]
async fn audit_log_records_authentication_events_without_secrets() {
    let log = CapturedLog::default();
    let subscriber = tracing_subscriber::fmt()
        .json()
        .with_max_level(tracing::Level::TRACE)
        .with_writer(log.clone())
        .finish();
    // A thread-local default is enough: the test runtime is single threaded.
    let _guard = tracing::subscriber::set_default(subscriber);

    let database = test_database().await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    let auth = Arc::new(
        AuthService::new(database, SystemClock, Some(Duration::days(30)), 2)
            .await
            .unwrap(),
    );
    let alice_password = "alice-correct-horse";
    auth.create_user("Alice", alice_password, true)
        .await
        .unwrap();
    let bob = auth
        .create_user("Bob", "bob-original-secret", false)
        .await
        .unwrap();
    let app = build_router(
        AppState::new(ServerIdentity::new(Uuid::new_v4(), "TJXY", "Linux"))
            .with_auth(auth)
            .with_ready(true),
    );
    let bob_id = bob.id().as_uuid();

    let wrong_password = "SecretWrongPw-91d3";
    let response = login(&app, "ALICE", wrong_password, "audit-device").await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let response = login(&app, "alice", alice_password, "audit-device").await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let access_token = serde_json::from_slice::<Value>(&body).unwrap()["AccessToken"]
        .as_str()
        .unwrap()
        .to_owned();

    let created_password = "CreatedUserPw-5521";
    let reset_password = "ResetUserPw-7788";
    assert_eq!(
        send(
            &app,
            Method::POST,
            "/Users/New",
            &access_token,
            Some(json!({"Name": "dave", "Password": created_password})),
        )
        .await,
        StatusCode::OK
    );
    assert_eq!(
        send(
            &app,
            Method::POST,
            &format!("/Users/{bob_id}/Password"),
            &access_token,
            Some(json!({"NewPw": reset_password, "ResetPassword": true})),
        )
        .await,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        send(
            &app,
            Method::DELETE,
            &format!("/Users/{bob_id}"),
            &access_token,
            None,
        )
        .await,
        StatusCode::NO_CONTENT
    );

    let output = String::from_utf8(log.0.lock().unwrap().clone()).unwrap();
    for expected in [
        "login_failure",
        "invalid_credentials",
        "login_success",
        "user_created",
        "password_reset_by_admin",
        "user_deleted",
        "tjxy_server::audit",
        "198.51.100.44",
        "audit-device",
        "\"actor\":\"alice\"",
    ] {
        assert!(output.contains(expected), "missing {expected}: {output}");
    }
    for secret in [
        wrong_password,
        alice_password,
        created_password,
        reset_password,
        "bob-original-secret",
        access_token.as_str(),
        "MediaBrowser Token",
    ] {
        assert!(!output.contains(secret), "log leaked a secret: {secret}");
    }
}
