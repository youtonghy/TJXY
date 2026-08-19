use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use serde_json::Value;
use tjxy_server::{AppState, ServerIdentity, build_router};
use tower::ServiceExt;
use uuid::Uuid;

fn state() -> AppState {
    AppState::new(ServerIdentity::new(
        Uuid::parse_str("018f17ac-4e99-7ec5-b4fd-8f15ca9f4f11").unwrap(),
        "Living Room",
        "Linux",
    ))
}

#[tokio::test]
async fn public_system_info_distinguishes_api_compatibility_from_product_version() {
    let response = build_router(state())
        .oneshot(
            Request::builder()
                .uri("/System/Info/Public")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CONTENT_TYPE], "application/json");
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let info: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(info["ProductName"], "TJXY");
    assert_eq!(info["LocalAddress"], Value::Null);
    assert_eq!(info["Version"], "10.11.11");
    assert_eq!(info["ProductVersion"], env!("CARGO_PKG_VERSION"));
    assert_eq!(info["ServerName"], "Living Room");
    assert_eq!(info["OperatingSystem"], "Linux");
    assert_eq!(info["Id"], "018f17ac-4e99-7ec5-b4fd-8f15ca9f4f11");
    assert_eq!(info["StartupWizardCompleted"], false);
}

#[tokio::test]
async fn authenticated_system_info_reuses_the_honest_public_identity() {
    let response = build_router(state())
        .oneshot(
            Request::builder()
                .uri("/System/Info")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn quick_connect_is_disabled_without_an_authentication_service() {
    let response = build_router(state())
        .oneshot(
            Request::builder()
                .uri("/QuickConnect/Enabled")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert!(!serde_json::from_slice::<bool>(&body).unwrap());
}

#[tokio::test]
async fn ping_and_liveness_are_available_without_authentication() {
    for path in ["/System/Ping", "/health/live"] {
        let response = build_router(state())
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "path {path}");
    }
}

#[tokio::test]
async fn default_branding_is_public_and_contains_no_custom_content() {
    let response = build_router(state())
        .oneshot(
            Request::builder()
                .uri("/Branding/Configuration")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let branding: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        branding,
        serde_json::json!({
            "LoginDisclaimer": null,
            "CustomCss": null,
            "SplashscreenEnabled": false
        })
    );
}

#[tokio::test]
async fn readiness_reports_service_dependency_state() {
    let response = build_router(state())
        .oneshot(
            Request::builder()
                .uri("/health/ready")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);

    let response = build_router(state().with_ready(true))
        .oneshot(
            Request::builder()
                .uri("/health/ready")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}
