use std::sync::Arc;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use chrono::Duration;
use http_body_util::BodyExt;
use sea_orm::{ConnectionTrait, Database, DbBackend, Statement};
use sea_orm_migration::MigratorTrait;
use serde_json::{Value, json};
use tjxy_application::{AuthService, SystemClock};
use tjxy_server::{AppState, ServerIdentity, build_router};
use tower::ServiceExt;
use uuid::Uuid;

const SERVER_ID: &str = "018f17ac-4e99-7ec5-b4fd-8f15ca9f4f11";
const IDENTITY: &str =
    r#"MediaBrowser Client="Findroid", Device="Pixel", DeviceId="phone-1", Version="0.16.0""#;

async fn app() -> axum::Router {
    app_with_legacy(true).await
}

async fn app_with_legacy(legacy_auth_enabled: bool) -> axum::Router {
    let database = Database::connect("sqlite::memory:").await.unwrap();
    database
        .execute(Statement::from_string(
            DbBackend::Sqlite,
            "PRAGMA foreign_keys = ON".to_owned(),
        ))
        .await
        .unwrap();
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
    app.oneshot(
        Request::builder()
            .method("POST")
            .uri("/Users/AuthenticateByName")
            .header(header::AUTHORIZATION, IDENTITY)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({"Username": "alice", "Pw": password}).to_string(),
            ))
            .unwrap(),
    )
    .await
    .unwrap()
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
