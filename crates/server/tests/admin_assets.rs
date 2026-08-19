use std::path::Path;

use axum::{
    Router,
    body::Body,
    http::{Method, Request, Response, StatusCode, header},
};
use http_body_util::BodyExt;
use tempfile::TempDir;
use tjxy_server::{
    AdminAssetsError, AppState, InstallationConfigStore, ServerIdentity, SetupCoordinator,
    SetupValidator, build_router_with_admin_and_jellyfin_web_dist, build_router_with_admin_dist,
    build_setup_router_with_admin_dist,
};
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
    std::fs::create_dir(directory.path().join("brand")).unwrap();
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
    std::fs::write(
        directory.path().join("brand/tjxy-mark.webp"),
        b"RIFF-brand-fixture-WEBP",
    )
    .unwrap();
    directory
}

fn jellyfin_web_distribution() -> TempDir {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("index.html"),
        "<!doctype html><title>Jellyfin Web</title><script src=\"main.js\"></script>",
    )
    .unwrap();
    std::fs::write(
        directory.path().join("main.js"),
        "console.log('jellyfin-web');",
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

    let root = request(&app, Method::GET, "/").await;
    assert_eq!(root.status(), StatusCode::PERMANENT_REDIRECT);
    assert_eq!(root.headers()[header::LOCATION], "/app/");
    for path in ["/app/", "/app/items/item-1"] {
        let response = request(&app, Method::GET, path).await;
        assert_eq!(response.status(), StatusCode::OK, "path {path}");
        assert!(
            response.headers()[header::CONTENT_TYPE]
                .to_str()
                .unwrap()
                .starts_with("text/html")
        );
    }
    let response = request(&app, Method::GET, "/assets/app.js").await;
    assert_eq!(response.status(), StatusCode::OK);

    let response = request(&app, Method::GET, "/brand/tjxy-mark.webp").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CONTENT_TYPE], "image/webp");
    assert_eq!(body_text(response).await, "RIFF-brand-fixture-WEBP");
}

#[tokio::test]
async fn operator_supplied_jellyfin_web_owns_only_root_and_web_routes() {
    let admin = distribution();
    let jellyfin_web = jellyfin_web_distribution();
    let app =
        build_router_with_admin_and_jellyfin_web_dist(state(), admin.path(), jellyfin_web.path())
            .unwrap();

    let root = request(&app, Method::GET, "/").await;
    assert_eq!(root.status(), StatusCode::PERMANENT_REDIRECT);
    assert_eq!(root.headers()[header::LOCATION], "/web/");
    let web = request(&app, Method::GET, "/web/").await;
    assert_eq!(web.status(), StatusCode::OK);
    assert!(body_text(web).await.contains("Jellyfin Web"));
    let script = request(&app, Method::GET, "/web/main.js").await;
    assert_eq!(script.status(), StatusCode::OK);
    assert!(body_text(script).await.contains("jellyfin-web"));

    for path in ["/app/", "/admin/"] {
        let response = request(&app, Method::GET, path).await;
        assert_eq!(response.status(), StatusCode::OK, "path {path}");
        assert!(body_text(response).await.contains("TJXY Admin"));
    }
}

#[tokio::test]
async fn redirects_setup_pages_to_the_installed_application() {
    let distribution = distribution();
    let app = build_router_with_admin_dist(state(), distribution.path()).unwrap();

    for path in ["/setup", "/setup/", "/setup/recovery"] {
        let response = request(&app, Method::GET, path).await;
        assert_eq!(
            response.status(),
            StatusCode::TEMPORARY_REDIRECT,
            "path {path}"
        );
        assert_eq!(response.headers()[header::LOCATION], "/app/");
    }
}

#[tokio::test]
async fn redirects_application_pages_to_setup_before_installation() {
    let distribution = distribution();
    let configuration = tempfile::tempdir().unwrap();
    let validator = SetupValidator::new(vec![configuration.path().to_path_buf()]).unwrap();
    let app = build_setup_router_with_admin_dist(
        SetupCoordinator::new(
            InstallationConfigStore::at(configuration.path().join("tjxy.toml")),
            validator.clone(),
        ),
        validator,
        distribution.path(),
    )
    .unwrap();

    for path in [
        "/app",
        "/app/",
        "/app/items/item-1",
        "/admin",
        "/admin/libraries",
    ] {
        let response = request(&app, Method::GET, path).await;
        assert_eq!(
            response.status(),
            StatusCode::TEMPORARY_REDIRECT,
            "path {path}"
        );
        assert_eq!(response.headers()[header::LOCATION], "/setup/");
    }

    assert_eq!(
        request(&app, Method::GET, "/unknown-api").await.status(),
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn spa_fallback_reads_the_current_distribution_index() {
    let distribution = distribution();
    let app = build_router_with_admin_dist(state(), distribution.path()).unwrap();
    std::fs::write(
        distribution.path().join("index.html"),
        "<!doctype html><script src=\"/assets/current-build.js\"></script>",
    )
    .unwrap();

    let response = request(&app, Method::GET, "/app/ai").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(body_text(response).await.contains("current-build.js"));
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

    let admin = distribution();
    let missing_web = Path::new("/private/deployment/secret/jellyfin-web");
    let error = build_router_with_admin_and_jellyfin_web_dist(state(), admin.path(), missing_web)
        .unwrap_err();
    assert!(matches!(
        error,
        AdminAssetsError::MissingJellyfinWebDistribution
    ));
    assert!(!format!("{error:?}").contains("private/deployment"));
}
