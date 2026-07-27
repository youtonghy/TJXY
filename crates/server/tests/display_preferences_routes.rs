use std::sync::Arc;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use chrono::Duration;
use http_body_util::BodyExt;
use sea_orm_migration::MigratorTrait;
use serde_json::{Value, json};
use tjxy_application::{AuthService, DisplayPreferencesService, SystemClock};
use tjxy_server::{AppState, ServerIdentity, build_router};
use tjxy_test_support::test_database;
use tower::ServiceExt;
use uuid::Uuid;

const IDENTITY: &str =
    r#"MediaBrowser Client="Findroid", Device="Pixel", DeviceId="phone-1", Version="0.16.0""#;

async fn test_app() -> axum::Router {
    let database = test_database().await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    let auth = Arc::new(
        AuthService::new(database.clone(), SystemClock, Some(Duration::days(30)), 2)
            .await
            .unwrap(),
    );
    auth.create_user("Alice", "correct horse", true)
        .await
        .unwrap();
    auth.create_user("Bob", "ordinary password", false)
        .await
        .unwrap();
    let preferences = Arc::new(DisplayPreferencesService::new(database));
    build_router(
        AppState::new(ServerIdentity::new(Uuid::new_v4(), "TJXY", "Linux"))
            .with_auth(auth)
            .with_display_preferences(preferences)
            .with_ready(true),
    )
}

async fn login(router: &axum::Router, username: &str, password: &str) -> (Uuid, String) {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/Users/AuthenticateByName")
                .header(header::AUTHORIZATION, IDENTITY)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"Username": username, "Pw": password}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let authentication: Value = serde_json::from_slice(&body).unwrap();
    (
        Uuid::parse_str(authentication["User"]["Id"].as_str().unwrap()).unwrap(),
        authentication["AccessToken"].as_str().unwrap().to_owned(),
    )
}

async fn response_json(response: axum::response::Response) -> Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn display_preferences_require_authentication_and_return_jellyfin_defaults() {
    let router = test_app().await;
    let unauthenticated = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/DisplayPreferences/usersettings?client=Findroid")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);

    let (_, token) = login(&router, "alice", "correct horse").await;
    let response = router
        .oneshot(
            Request::builder()
                .uri("/DisplayPreferences/usersettings?client=Findroid")
                .header("X-Emby-Token", token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({
            "Id": "3ce5b65d-e116-d731-65d1-efc4a30ec35c",
            "ViewType": null,
            "SortBy": null,
            "IndexBy": null,
            "RememberIndexing": false,
            "PrimaryImageHeight": 250,
            "PrimaryImageWidth": 250,
            "CustomPrefs": {},
            "ScrollDirection": "Horizontal",
            "ShowBackdrop": true,
            "RememberSorting": false,
            "SortOrder": "Ascending",
            "ShowSidebar": false,
            "Client": "Findroid"
        })
    );
}

#[tokio::test]
async fn display_preferences_replace_the_document_and_reject_impersonation() {
    let router = test_app().await;
    let (alice_id, alice_token) = login(&router, "alice", "correct horse").await;
    let (_, bob_token) = login(&router, "bob", "ordinary password").await;

    let update = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/DisplayPreferences/usersettings?client=Findroid")
                .header("X-Emby-Token", &alice_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "SortBy": "SortName",
                        "RememberSorting": true,
                        "ShowBackdrop": false,
                        "CustomPrefs": {"homesection0": "resume"}
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(update.status(), StatusCode::NO_CONTENT);

    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/DisplayPreferences/usersettings?client=Findroid")
                .header("X-Emby-Token", &alice_token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["SortBy"], "SortName");
    assert_eq!(body["RememberSorting"], true);
    assert_eq!(body["ShowBackdrop"], false);
    assert_eq!(body["CustomPrefs"]["homesection0"], "resume");
    assert_eq!(body["PrimaryImageHeight"], 250);
    assert_eq!(body["Client"], "Findroid");

    let forbidden = router
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/DisplayPreferences/usersettings?client=Findroid&userId={alice_id}"
                ))
                .header("X-Emby-Token", &bob_token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);

    let bob = router
        .oneshot(
            Request::builder()
                .uri("/DisplayPreferences/usersettings?client=Findroid")
                .header("X-Emby-Token", bob_token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response_json(bob).await["SortBy"], Value::Null);
}

#[tokio::test]
async fn display_preferences_reject_invalid_queries_and_oversized_custom_values() {
    let router = test_app().await;
    let (_, token) = login(&router, "alice", "correct horse").await;

    for uri in [
        "/DisplayPreferences/usersettings",
        "/DisplayPreferences/usersettings?client=Findroid&client=Other",
        "/DisplayPreferences/usersettings?client=Findroid&unknown=true",
    ] {
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri(uri)
                    .header("X-Emby-Token", &token)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "uri {uri}");
    }

    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/DisplayPreferences/usersettings?client=Findroid")
                .header("X-Emby-Token", token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"CustomPrefs":{"too-large":"x".repeat(2049)}}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
