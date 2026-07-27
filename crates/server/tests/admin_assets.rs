use std::path::Path;

use axum::{
    Router,
    body::Body,
    http::{Method, Request, Response, StatusCode, header},
};
use http_body_util::BodyExt;
use tempfile::TempDir;
use tjxy_server::{AdminAssetsError, AppState, ServerIdentity, build_router_with_admin_dist};
use tower::ServiceExt;
use uuid::Uuid;

fn state() -> AppState {
    AppState::new(ServerIdentity::new(
        Uuid::parse_str("018f17ac-4e99-7ec5-b4fd-8f15ca9f4f11").unwrap(),
        "Living Room",
        "Linux",
    ))
}

fn distribution() -> TempDir {
    let directory = tempfile::tempdir().unwrap();
    std::fs::create_dir(directory.path().join("assets")).unwrap();
    std::fs::write(
        directory.path().join("index.html"),
        "<!doctype html><title>TJXY Admin</title><div id=\"root\"></div>",
    )
    .unwrap();
    std::fs::write(
        directory.path().join("assets/app.js"),
        "console.log('admin');",
    )
    .unwrap();
    directory
}

async fn request(app: &Router, method: Method, path: &str) -> Response<Body> {
    app.clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
}

async fn body_text(response: Response<Body>) -> String {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8(bytes.to_vec()).unwrap()
}

#[tokio::test]
async fn serves_real_files_and_scoped_html_fallbacks() {
    let distribution = distribution();
    let app = build_router_with_admin_dist(state(), distribution.path()).unwrap();

    let redirect = request(&app, Method::GET, "/admin").await;
    assert_eq!(redirect.status(), StatusCode::PERMANENT_REDIRECT);
    assert_eq!(redirect.headers()[header::LOCATION], "/admin/");

    for path in ["/admin/", "/admin/users/u1"] {
        let response = request(&app, Method::GET, path).await;
        assert_eq!(response.status(), StatusCode::OK, "path {path}");
        assert!(
            response.headers()[header::CONTENT_TYPE]
                .to_str()
                .unwrap()
                .starts_with("text/html")
        );
        assert!(body_text(response).await.contains("TJXY Admin"));
    }

    let response = request(&app, Method::GET, "/admin/assets/app.js").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CONTENT_TYPE], "text/javascript");
    assert!(body_text(response).await.contains("console.log"));
}

#[tokio::test]
async fn does_not_rewrite_assets_methods_or_api_misses() {
    let distribution = distribution();
    let app = build_router_with_admin_dist(state(), distribution.path()).unwrap();

    assert_eq!(
        request(&app, Method::GET, "/admin/assets/missing.js")
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        request(&app, Method::POST, "/admin/users/u1")
            .await
            .status(),
        StatusCode::METHOD_NOT_ALLOWED
    );
    assert_eq!(
        request(&app, Method::GET, "/not-an-api").await.status(),
        StatusCode::NOT_FOUND
    );
    let json_navigation = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/admin/users/u1")
                .header(header::ACCEPT, "application/json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(json_navigation.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        request(&app, Method::GET, "/Users").await.status(),
        StatusCode::UNAUTHORIZED
    );
}

#[test]
fn rejects_missing_or_invalid_distributions_without_path_disclosure() {
    let missing_root = Path::new("/private/deployment/secret/admin-dist");
    let error = build_router_with_admin_dist(state(), missing_root).unwrap_err();
    assert!(matches!(error, AdminAssetsError::MissingDistribution));
    assert!(!format!("{error:?}").contains("private/deployment"));

    let no_index = tempfile::tempdir().unwrap();
    let error = build_router_with_admin_dist(state(), no_index.path()).unwrap_err();
    assert!(matches!(error, AdminAssetsError::MissingIndex));
    assert!(!format!("{error:?}").contains(&no_index.path().display().to_string()));

    let index_directory = tempfile::tempdir().unwrap();
    std::fs::create_dir(index_directory.path().join("index.html")).unwrap();
    let error = build_router_with_admin_dist(state(), index_directory.path()).unwrap_err();
    assert!(matches!(error, AdminAssetsError::MissingIndex));
    assert!(!format!("{error:?}").contains(&index_directory.path().display().to_string()));
}
