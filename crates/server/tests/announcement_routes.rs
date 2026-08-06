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
use tjxy_db::Migrator;
use tjxy_server::{AppState, ServerIdentity, build_router};
use tjxy_test_support::test_database;
use tower::ServiceExt;
use uuid::Uuid;

#[tokio::test]
async fn announcements_are_admin_managed_and_acknowledged_per_user_version() {
    let router = announcement_router().await;
    let admin_token = login(router.clone(), "Admin").await;
    let reader_token = login(router.clone(), "Reader").await;
    let other_token = login(router.clone(), "Other").await;

    let forbidden = send(
        &router,
        Method::GET,
        "/Admin/Announcements",
        &reader_token,
        None,
    )
    .await;
    assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);

    let created = send(
        &router,
        Method::POST,
        "/Admin/Announcements",
        &admin_token,
        Some(json!({
            "Title": "Library maintenance",
            "BodyMarkdown": "**Playback** will pause briefly.",
            "Kind": "Popup"
        })),
    )
    .await;
    assert_eq!(created.status(), StatusCode::CREATED);
    assert_eq!(
        created.headers().get(header::CACHE_CONTROL).unwrap(),
        "no-store"
    );
    let created = json_body(created).await;
    let id = created["Id"].as_str().unwrap();
    assert_eq!(created["Status"], "Draft");

    let published = send(
        &router,
        Method::POST,
        &format!("/Admin/Announcements/{id}/Publish"),
        &admin_token,
        Some(json!({"Revision": created["Revision"]})),
    )
    .await;
    assert_eq!(published.status(), StatusCode::OK);
    let published = json_body(published).await;
    assert_eq!(published["Status"], "Published");
    assert_eq!(published["ContentVersion"], 1);

    let list = send(
        &router,
        Method::GET,
        "/Announcements?startIndex=0&limit=20",
        &reader_token,
        None,
    )
    .await;
    assert_eq!(list.status(), StatusCode::OK);
    let list = json_body(list).await;
    assert_eq!(list["Total"], 1);
    assert_eq!(list["UnreadCount"], 1);
    assert_eq!(list["Items"][0]["IsRead"], false);

    let pending = send(
        &router,
        Method::GET,
        "/Announcements/NextPopup",
        &reader_token,
        None,
    )
    .await;
    assert_eq!(pending.status(), StatusCode::OK);
    assert_eq!(json_body(pending).await["Id"], id);

    let acknowledged = send(
        &router,
        Method::POST,
        &format!("/Announcements/{id}/Acknowledge"),
        &reader_token,
        Some(json!({"ContentVersion": 1})),
    )
    .await;
    assert_eq!(acknowledged.status(), StatusCode::NO_CONTENT);
    let no_pending = send(
        &router,
        Method::GET,
        "/Announcements/NextPopup",
        &reader_token,
        None,
    )
    .await;
    assert_eq!(no_pending.status(), StatusCode::NO_CONTENT);
    let other_pending = send(
        &router,
        Method::GET,
        "/Announcements/NextPopup",
        &other_token,
        None,
    )
    .await;
    assert_eq!(other_pending.status(), StatusCode::OK);
}

async fn announcement_router() -> axum::Router {
    let database = test_database().await.unwrap();
    Migrator::up(&database, None).await.unwrap();
    let auth = Arc::new(
        AuthService::new(database.clone(), SystemClock, Some(Duration::days(30)), 2)
            .await
            .unwrap(),
    );
    auth.create_user("Admin", "correct horse", true)
        .await
        .unwrap();
    auth.create_user("Reader", "correct horse", false)
        .await
        .unwrap();
    auth.create_user("Other", "correct horse", false)
        .await
        .unwrap();
    build_router(
        AppState::new(ServerIdentity::new(Uuid::new_v4(), "TJXY", "Linux"))
            .with_auth(auth)
            .with_announcements(database),
    )
}

async fn send(
    router: &axum::Router,
    method: Method,
    uri: &str,
    token: &str,
    body: Option<Value>,
) -> axum::response::Response {
    router
        .clone()
        .oneshot(authenticated_request(method, uri, token, body))
        .await
        .unwrap()
}

async fn login(router: axum::Router, username: &str) -> String {
    let response = router
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/Users/AuthenticateByName")
                .header(
                    header::AUTHORIZATION,
                    r#"MediaBrowser Client="Browser", Device="QA", DeviceId="announcement-qa", Version="1""#,
                )
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"Username": username, "Pw": "correct horse"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    json_body(response).await["AccessToken"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn authenticated_request(
    method: Method,
    uri: &str,
    token: &str,
    body: Option<Value>,
) -> Request<Body> {
    let builder = Request::builder().method(method).uri(uri).header(
        header::AUTHORIZATION,
        format!(r#"MediaBrowser Token="{token}""#),
    );
    match body {
        Some(body) => builder
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .unwrap(),
        None => builder.body(Body::empty()).unwrap(),
    }
}

async fn json_body(response: axum::response::Response) -> Value {
    let body = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&body).unwrap()
}
