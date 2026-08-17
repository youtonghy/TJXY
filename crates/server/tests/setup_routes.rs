use std::net::SocketAddr;

use axum::{
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tempfile::tempdir;
use tjxy_server::{
    InstallationConfigStore, SetupCoordinator, SetupValidator, build_setup_router,
    build_setup_router_with_asset_dir, build_setup_router_with_options,
};
use tower::ServiceExt;

fn private_request(method: &str, path: &str, body: Body) -> Request<Body> {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .body(body)
        .unwrap();
    request.extensions_mut().insert(ConnectInfo(
        "192.168.10.20:41000".parse::<SocketAddr>().unwrap(),
    ));
    request
}

#[tokio::test]
async fn managed_database_is_tested_server_side_and_completed_without_browser_credentials() {
    let directory = tempdir().unwrap();
    let validator = SetupValidator::new(vec![directory.path().to_path_buf()]).unwrap();
    let database_path = directory.path().join("managed.db");
    let router = build_setup_router_with_options(
        SetupCoordinator::new(
            InstallationConfigStore::at(directory.path().join("config/tjxy.toml")),
            validator.clone(),
        ),
        validator,
        directory.path().join("assets/branding"),
        Some(tjxy_server::DatabaseDraft::Sqlite {
            path: database_path.clone(),
        }),
    );
    let status = router
        .clone()
        .oneshot(private_request("GET", "/Setup/Status", Body::empty()))
        .await
        .unwrap();
    assert_eq!(status.status(), StatusCode::OK);
    let cookie = status.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let status_bytes = status.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&status_bytes).unwrap();
    assert_eq!(body["ManagedDatabaseBackend"], "sqlite");
    assert!(!String::from_utf8_lossy(&status_bytes).contains(&database_path.display().to_string()));

    let mut request = private_request(
        "POST",
        "/Setup/Complete",
        Body::from(
            json!({
                "SiteTitle": "TJXY",
                "SiteSubtitle": "Managed database",
                "Locale": "en-US",
                "LogoUrl": "/brand/tjxy-mark.webp",
                "IconUrl": "/brand/favicon.svg",
                "Database": null,
                "Network": { "ListenHost": "127.0.0.1", "Port": 8096, "PublicUrl": null },
                "AdministratorUsername": "admin",
                "AdministratorPassword": "correct horse battery staple"
            })
            .to_string(),
        ),
    );
    request
        .headers_mut()
        .insert(header::CONTENT_TYPE, "application/json".parse().unwrap());
    request
        .headers_mut()
        .insert(header::COOKIE, cookie.parse().unwrap());
    request.headers_mut().insert(
        "x-tjxy-setup-csrf",
        body["CsrfToken"].as_str().unwrap().parse().unwrap(),
    );
    assert_eq!(
        router.oneshot(request).await.unwrap().status(),
        StatusCode::OK
    );
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn setup_status_issues_csrf_and_mutations_require_it() {
    let directory = tempdir().unwrap();
    let validator = SetupValidator::new(vec![directory.path().to_path_buf()]).unwrap();
    let router = build_setup_router_with_asset_dir(
        SetupCoordinator::new(
            InstallationConfigStore::at(directory.path().join("config/tjxy.toml")),
            validator.clone(),
        ),
        validator,
        directory.path().join("assets/branding"),
    );

    let response = router
        .clone()
        .oneshot(private_request("GET", "/Setup/Status", Body::empty()))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    let cookie = response.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .to_owned();
    assert!(cookie.contains("HttpOnly"));
    assert!(cookie.contains("SameSite=Strict"));
    let body: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let csrf = body["CsrfToken"].as_str().unwrap();
    let installation_id = body["InstallationId"].as_str().unwrap();

    let mut progress_request = private_request(
        "GET",
        &format!("/Setup/Progress?installationId={installation_id}"),
        Body::empty(),
    );
    progress_request.headers_mut().insert(
        header::COOKIE,
        cookie.split(';').next().unwrap().parse().unwrap(),
    );
    let progress = router.clone().oneshot(progress_request).await.unwrap();
    assert_eq!(progress.status(), StatusCode::OK);
    assert_eq!(
        progress.headers()[header::CONTENT_TYPE],
        "text/event-stream"
    );
    assert_eq!(progress.headers()[header::CACHE_CONTROL], "no-store");

    let payload = json!({
        "Backend": "sqlite",
        "Path": directory.path().join("tjxy.db")
    });
    let mut denied_request = private_request(
        "POST",
        "/Setup/Database/Test",
        Body::from(payload.to_string()),
    );
    denied_request
        .headers_mut()
        .insert(header::CONTENT_TYPE, "application/json".parse().unwrap());
    let denied = router.clone().oneshot(denied_request).await.unwrap();
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);

    let mut allowed = private_request(
        "POST",
        "/Setup/Database/Test",
        Body::from(payload.to_string()),
    );
    allowed
        .headers_mut()
        .insert(header::CONTENT_TYPE, "application/json".parse().unwrap());
    allowed.headers_mut().insert(
        header::COOKIE,
        cookie.split(';').next().unwrap().parse().unwrap(),
    );
    allowed
        .headers_mut()
        .insert("x-tjxy-setup-csrf", csrf.parse().unwrap());
    let response = router.clone().oneshot(allowed).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["Backend"], "sqlite");

    let changed_database = json!({
        "SiteTitle": "TJXY",
        "SiteSubtitle": "Private media",
        "Locale": "en-US",
        "LogoUrl": "/brand/tjxy-mark.webp",
        "IconUrl": "/brand/favicon.svg",
        "Database": {
            "Backend": "sqlite",
            "Path": directory.path().join("untested.db")
        },
        "Network": {
            "ListenHost": "127.0.0.1",
            "Port": 8096,
            "PublicUrl": null
        },
        "AdministratorUsername": "admin",
        "AdministratorPassword": "correct horse battery staple"
    });
    let mut changed_database_request = private_request(
        "POST",
        "/Setup/Complete",
        Body::from(changed_database.to_string()),
    );
    changed_database_request
        .headers_mut()
        .insert(header::CONTENT_TYPE, "application/json".parse().unwrap());
    changed_database_request.headers_mut().insert(
        header::COOKIE,
        cookie.split(';').next().unwrap().parse().unwrap(),
    );
    changed_database_request
        .headers_mut()
        .insert("x-tjxy-setup-csrf", csrf.parse().unwrap());
    let response = router
        .clone()
        .oneshot(changed_database_request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let network_payload = json!({
        "ListenHost": "127.0.0.1",
        "Port": 8096,
        "PublicUrl": "https://media.example.test"
    });
    let mut network_request = private_request(
        "POST",
        "/Setup/Network/Validate",
        Body::from(network_payload.to_string()),
    );
    network_request
        .headers_mut()
        .insert(header::CONTENT_TYPE, "application/json".parse().unwrap());
    network_request.headers_mut().insert(
        header::COOKIE,
        cookie.split(';').next().unwrap().parse().unwrap(),
    );
    network_request
        .headers_mut()
        .insert("x-tjxy-setup-csrf", csrf.parse().unwrap());
    let response = router.clone().oneshot(network_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    let body: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["ListenHost"], "127.0.0.1");
    assert_eq!(body["Port"], 8096);
    assert_eq!(
        body["DestinationUrl"],
        "https://media.example.test/app/login?redirect=%2Fadmin"
    );

    let invalid_network_payload = json!({
        "ListenHost": "localhost",
        "Port": 8096,
        "PublicUrl": null
    });
    let mut invalid_network_request = private_request(
        "POST",
        "/Setup/Network/Validate",
        Body::from(invalid_network_payload.to_string()),
    );
    invalid_network_request
        .headers_mut()
        .insert(header::CONTENT_TYPE, "application/json".parse().unwrap());
    invalid_network_request.headers_mut().insert(
        header::COOKIE,
        cookie.split(';').next().unwrap().parse().unwrap(),
    );
    invalid_network_request
        .headers_mut()
        .insert("x-tjxy-setup-csrf", csrf.parse().unwrap());
    let response = router
        .clone()
        .oneshot(invalid_network_request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let mut branding_request = private_request(
        "PUT",
        "/Setup/Branding/logo",
        Body::from(b"\x89PNG\r\n\x1a\nfixture".as_slice()),
    );
    branding_request
        .headers_mut()
        .insert(header::CONTENT_TYPE, "image/png".parse().unwrap());
    branding_request.headers_mut().insert(
        header::COOKIE,
        cookie.split(';').next().unwrap().parse().unwrap(),
    );
    branding_request
        .headers_mut()
        .insert("x-tjxy-setup-csrf", csrf.parse().unwrap());
    let response = router.clone().oneshot(branding_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    let body: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let asset_url = body["AssetUrl"].as_str().unwrap();
    assert!(asset_url.starts_with("/Branding/Assets/logo-"));
    assert!(
        std::path::Path::new(asset_url)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("png"))
    );
    assert_eq!(
        std::fs::read_dir(directory.path().join("assets/branding"))
            .unwrap()
            .count(),
        1
    );

    let isolated = router
        .oneshot(private_request("GET", "/app/", Body::empty()))
        .await
        .unwrap();
    assert_eq!(isolated.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn setup_rejects_public_source_addresses() {
    let directory = tempdir().unwrap();
    let validator = SetupValidator::new(vec![directory.path().to_path_buf()]).unwrap();
    let router = build_setup_router(
        SetupCoordinator::new(
            InstallationConfigStore::at(directory.path().join("tjxy.toml")),
            validator.clone(),
        ),
        validator,
    );
    let mut request = Request::builder()
        .uri("/Setup/Status")
        .body(Body::empty())
        .unwrap();
    request
        .extensions_mut()
        .insert(ConnectInfo("8.8.8.8:41000".parse::<SocketAddr>().unwrap()));
    assert_eq!(
        router.oneshot(request).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn setup_mutations_are_rate_limited_per_session() {
    let directory = tempdir().unwrap();
    let validator = SetupValidator::new(vec![directory.path().to_path_buf()]).unwrap();
    let router = build_setup_router(
        SetupCoordinator::new(
            InstallationConfigStore::at(directory.path().join("tjxy.toml")),
            validator.clone(),
        ),
        validator,
    );
    let status = router
        .clone()
        .oneshot(private_request("GET", "/Setup/Status", Body::empty()))
        .await
        .unwrap();
    let cookie = status.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let body: Value =
        serde_json::from_slice(&status.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let csrf = body["CsrfToken"].as_str().unwrap();

    for attempt in 0..=60 {
        let mut request = private_request(
            "POST",
            "/Setup/Network/Validate",
            Body::from(
                json!({ "ListenHost": "127.0.0.1", "Port": 8096, "PublicUrl": null }).to_string(),
            ),
        );
        request
            .headers_mut()
            .insert(header::CONTENT_TYPE, "application/json".parse().unwrap());
        request
            .headers_mut()
            .insert(header::COOKIE, cookie.parse().unwrap());
        request
            .headers_mut()
            .insert("x-tjxy-setup-csrf", csrf.parse().unwrap());
        let response = router.clone().oneshot(request).await.unwrap();
        assert_eq!(
            response.status(),
            if attempt < 60 {
                StatusCode::OK
            } else {
                StatusCode::TOO_MANY_REQUESTS
            }
        );
    }
}
