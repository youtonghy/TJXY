use std::{path::Path, sync::Arc};

use axum::{
    body::Body,
    http::{Method, Request, StatusCode, header},
};
use chrono::Duration;
use http_body_util::BodyExt;
use sea_orm_migration::MigratorTrait;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tjxy_application::{AuthService, SystemClock};
use tjxy_db::Migrator;
use tjxy_server::{AppState, ServerIdentity, build_router};
use tjxy_test_support::test_database;
use tower::ServiceExt;
use uuid::Uuid;

const UPLOAD: &str = "/Admin/ClientReleases?Platform=windows&Arch=x64&FileName=TJXY-setup.exe";

#[tokio::test]
#[allow(clippy::too_many_lines)] // Keeps the upload, latest, check, download, and delete lifecycle together.
async fn administrators_publish_releases_that_clients_check_and_download() {
    let directory = tempfile::tempdir().unwrap();
    let router = release_router(directory.path()).await;
    let admin = login(router.clone(), "Admin").await;
    let reader = login(router.clone(), "Reader").await;

    let forbidden = upload(&router, &reader, &format!("{UPLOAD}&Version=1.0.0"), b"x").await;
    assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);

    let first = upload(
        &router,
        &admin,
        &format!("{UPLOAD}&Version=1.0.0&Latest=true"),
        b"first installer",
    )
    .await;
    assert_eq!(first.status(), StatusCode::CREATED);
    let first = json_body(first).await;
    assert_eq!(first["IsLatest"], true);
    assert_eq!(first["Size"], 15);
    assert_eq!(
        first["Sha256"],
        format!("{:x}", Sha256::digest(b"first installer"))
    );

    let duplicate = upload(
        &router,
        &admin,
        &format!("{UPLOAD}&Version=1.0.0"),
        b"again",
    )
    .await;
    assert_eq!(duplicate.status(), StatusCode::CONFLICT);
    assert_eq!(
        std::fs::read_dir(directory.path()).unwrap().count(),
        1,
        "rejected uploads must not leave files behind"
    );

    let second = json_body(
        upload(
            &router,
            &admin,
            &format!("{UPLOAD}&Version=1.1.0"),
            b"second installer",
        )
        .await,
    )
    .await;
    assert_eq!(second["IsLatest"], false);
    let second_id = second["Id"].as_str().unwrap();

    let check_uri = "/ClientUpdates/Latest?Platform=windows&Arch=x64&CurrentVersion=1.0.0";
    let check = json_body(send(&router, Method::GET, check_uri, &reader, None).await).await;
    assert_eq!(check["UpdateAvailable"], false);
    assert_eq!(check["Latest"]["Version"], "1.0.0");

    let marked = send(
        &router,
        Method::POST,
        &format!("/Admin/ClientReleases/{second_id}/Latest"),
        &admin,
        None,
    )
    .await;
    assert_eq!(marked.status(), StatusCode::OK);
    let notes = send(
        &router,
        Method::PUT,
        &format!("/Admin/ClientReleases/{second_id}"),
        &admin,
        Some(json!({"Notes": "Faster playback"})),
    )
    .await;
    assert_eq!(json_body(notes).await["Notes"], "Faster playback");

    let check = json_body(send(&router, Method::GET, check_uri, &reader, None).await).await;
    assert_eq!(check["UpdateAvailable"], true);
    assert_eq!(check["Latest"]["Version"], "1.1.0");
    assert_eq!(check["Latest"]["Notes"], "Faster playback");
    let newer = json_body(
        send(
            &router,
            Method::GET,
            "/ClientUpdates/Latest?Platform=windows&Arch=x64&CurrentVersion=2.0.0-beta.1",
            &reader,
            None,
        )
        .await,
    )
    .await;
    assert_eq!(newer["UpdateAvailable"], false, "never offer a downgrade");

    let list =
        json_body(send(&router, Method::GET, "/Admin/ClientReleases", &admin, None).await).await;
    let latest_flags = list["Items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| (item["Version"].as_str().unwrap(), item["IsLatest"] == true))
        .collect::<Vec<_>>();
    assert_eq!(latest_flags, [("1.1.0", true), ("1.0.0", false)]);

    let download_path = check["Latest"]["DownloadPath"].as_str().unwrap();
    let download = send(&router, Method::GET, download_path, &reader, None).await;
    assert_eq!(download.status(), StatusCode::OK);
    assert_eq!(
        download.headers()[header::CONTENT_DISPOSITION],
        "attachment; filename=\"TJXY-setup.exe\""
    );
    assert_eq!(body_bytes(download).await, b"second installer");
    let ranged = router
        .clone()
        .oneshot(
            Request::builder()
                .uri(download_path)
                .header(header::AUTHORIZATION, token_header(&reader))
                .header(header::RANGE, "bytes=7-")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ranged.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(body_bytes(ranged).await, b"installer");
    let anonymous = router
        .clone()
        .oneshot(
            Request::builder()
                .uri(download_path)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);

    let deleted = send(
        &router,
        Method::DELETE,
        &format!("/Admin/ClientReleases/{second_id}"),
        &admin,
        None,
    )
    .await;
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    let check = json_body(send(&router, Method::GET, check_uri, &reader, None).await).await;
    assert_eq!(check["UpdateAvailable"], false);
    assert!(check["Latest"].is_null());
}

#[tokio::test]
async fn universal_packages_serve_architectures_without_a_dedicated_release() {
    let directory = tempfile::tempdir().unwrap();
    let router = release_router(directory.path()).await;
    let admin = login(router.clone(), "Admin").await;
    for (arch, version) in [("universal", "1.2.0"), ("arm64", "1.1.0")] {
        let response = upload(
            &router,
            &admin,
            &format!(
                "/Admin/ClientReleases?Platform=android&Arch={arch}&Version={version}&FileName=TJXY.apk&Latest=true"
            ),
            b"apk",
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);
    }
    let latest = |arch: &str| {
        format!("/ClientUpdates/Latest?Platform=android&Arch={arch}&CurrentVersion=1.0.0")
    };
    let x64 = json_body(send(&router, Method::GET, &latest("x64"), &admin, None).await).await;
    assert_eq!(x64["Latest"]["Version"], "1.2.0");
    let arm64 = json_body(send(&router, Method::GET, &latest("arm64"), &admin, None).await).await;
    assert_eq!(arm64["Latest"]["Version"], "1.1.0");
}

#[tokio::test]
async fn invalid_uploads_are_rejected_without_storing_files() {
    let directory = tempfile::tempdir().unwrap();
    let router = release_router(directory.path()).await;
    let admin = login(router.clone(), "Admin").await;
    for uri in [
        "/Admin/ClientReleases?Platform=windows&Arch=x64&Version=1.0.0&FileName=TJXY.apk",
        "/Admin/ClientReleases?Platform=windows&Arch=x64&Version=1.0&FileName=TJXY.exe",
        "/Admin/ClientReleases?Platform=ios&Arch=x64&Version=1.0.0&FileName=TJXY.ipa",
        "/Admin/ClientReleases?Platform=windows&Arch=x64&Version=1.0.0&FileName=../TJXY.exe",
        "/Admin/ClientReleases?Platform=windows&Arch=x64&Version=1.0.0&FileName=TJXY.exe&Extra=1",
    ] {
        let response = upload(&router, &admin, uri, b"installer").await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{uri}");
    }
    let empty = upload(&router, &admin, &format!("{UPLOAD}&Version=1.0.0"), b"").await;
    assert_eq!(empty.status(), StatusCode::BAD_REQUEST);
    let malformed_check = send(
        &router,
        Method::GET,
        "/ClientUpdates/Latest?Platform=windows&Arch=x64&CurrentVersion=latest",
        &admin,
        None,
    )
    .await;
    assert_eq!(malformed_check.status(), StatusCode::BAD_REQUEST);
    assert!(
        !directory.path().exists() || std::fs::read_dir(directory.path()).unwrap().count() == 0
    );
}

async fn release_router(directory: &Path) -> axum::Router {
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
    build_router(
        AppState::new(ServerIdentity::new(Uuid::new_v4(), "TJXY", "Linux"))
            .with_auth(auth)
            .with_client_releases(database, directory.to_path_buf()),
    )
}

async fn upload(
    router: &axum::Router,
    token: &str,
    uri: &str,
    bytes: &'static [u8],
) -> axum::response::Response {
    router
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(uri)
                .header(header::AUTHORIZATION, token_header(token))
                .header(header::CONTENT_TYPE, "application/octet-stream")
                .body(Body::from(bytes))
                .unwrap(),
        )
        .await
        .unwrap()
}

async fn send(
    router: &axum::Router,
    method: Method,
    uri: &str,
    token: &str,
    body: Option<Value>,
) -> axum::response::Response {
    let builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::AUTHORIZATION, token_header(token));
    let request = match body {
        Some(body) => builder
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .unwrap(),
        None => builder.body(Body::empty()).unwrap(),
    };
    router.clone().oneshot(request).await.unwrap()
}

async fn login(router: axum::Router, username: &str) -> String {
    let response = router
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/Users/AuthenticateByName")
                .header(
                    header::AUTHORIZATION,
                    r#"MediaBrowser Client="Browser", Device="QA", DeviceId="release-qa", Version="1""#,
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

fn token_header(token: &str) -> String {
    format!(r#"MediaBrowser Token="{token}""#)
}

async fn body_bytes(response: axum::response::Response) -> Vec<u8> {
    response
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes()
        .to_vec()
}

async fn json_body(response: axum::response::Response) -> Value {
    serde_json::from_slice(&body_bytes(response).await).unwrap()
}
