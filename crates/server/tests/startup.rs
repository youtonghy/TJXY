use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tjxy_server::{BootstrapAdmin, ServerIdentity, StartupOptions, build_router, initialize};
use tower::ServiceExt;
use uuid::Uuid;

#[tokio::test]
async fn initialization_migrates_bootstraps_auth_and_only_then_reports_ready() {
    let identity = ServerIdentity::new(Uuid::new_v4(), "TJXY", "Linux");
    let state = initialize(
        StartupOptions::new("sqlite::memory:", identity)
            .with_bootstrap_admin(BootstrapAdmin::new("Admin", "first password")),
    )
    .await
    .unwrap();
    let app = build_router(state);

    let ready = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/health/ready")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ready.status(), StatusCode::OK);

    let info = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/System/Info/Public")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = info.into_body().collect().await.unwrap().to_bytes();
    let info: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(info["StartupWizardCompleted"], true);

    let login = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/Users/AuthenticateByName")
                .header(
                    header::AUTHORIZATION,
                    r#"MediaBrowser Client="Findroid", Device="Phone", DeviceId="1", Version="1""#,
                )
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"Username": "admin", "Pw": "first password"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let body = login.into_body().collect().await.unwrap().to_bytes();
    let authentication: Value = serde_json::from_slice(&body).unwrap();
    let token = authentication["AccessToken"].as_str().unwrap();
    let browse = app
        .oneshot(
            Request::builder()
                .uri("/UserViews")
                .header(
                    header::AUTHORIZATION,
                    format!(r#"MediaBrowser Token="{token}""#),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(browse.status(), StatusCode::OK);
    let body = browse.into_body().collect().await.unwrap().to_bytes();
    let result: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(result["Items"], json!([]));
    assert_eq!(result["TotalRecordCount"], 0);
}

#[tokio::test]
async fn a_new_database_cannot_report_ready_without_an_initial_administrator() {
    let identity = ServerIdentity::new(Uuid::new_v4(), "TJXY", "Linux");
    let Err(error) = initialize(StartupOptions::new("sqlite::memory:", identity)).await else {
        panic!("new database unexpectedly initialized without an administrator");
    };
    assert!(error.to_string().contains("bootstrap administrator"));
}
