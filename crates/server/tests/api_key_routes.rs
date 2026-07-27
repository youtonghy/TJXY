use std::sync::Arc;

use axum::{
    body::Body,
    http::{Method, Request, StatusCode, header},
};
use chrono::Duration;
use http_body_util::BodyExt;
use sea_orm::{ConnectionTrait, DatabaseConnection, Statement};
use sea_orm_migration::MigratorTrait;
use serde_json::{Value, json};
use tjxy_application::{AuthService, ClientIdentity, SystemClock};
use tjxy_credentials::{CredentialCipher, CredentialKey};
use tjxy_server::{AppState, ServerIdentity, build_router};
use tjxy_test_support::test_database;
use tower::ServiceExt;
use uuid::Uuid;

struct Fixture {
    app: axum::Router,
    auth: Arc<AuthService<SystemClock>>,
    database: DatabaseConnection,
    admin_token: String,
    user_token: String,
}

async fn fixture(with_cipher: bool) -> Fixture {
    fixture_with_legacy(with_cipher, true).await
}

async fn fixture_with_legacy(with_cipher: bool, legacy_auth_enabled: bool) -> Fixture {
    let database = test_database().await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    let mut auth = AuthService::new(database.clone(), SystemClock, Some(Duration::days(30)), 2)
        .await
        .unwrap();
    if with_cipher {
        let key = CredentialKey::new(1, [91_u8; 32]).unwrap();
        auth =
            auth.with_credential_cipher(Arc::new(CredentialCipher::new(key, Vec::new()).unwrap()));
    }
    let auth = Arc::new(auth);
    auth.create_user("Admin", "admin password", true)
        .await
        .unwrap();
    auth.create_user("Viewer", "viewer password", false)
        .await
        .unwrap();
    let admin_token = login_token(&auth, "Admin", "admin password", "admin-device").await;
    let user_token = login_token(&auth, "Viewer", "viewer password", "viewer-device").await;
    let identity =
        ServerIdentity::new(Uuid::new_v4(), "TJXY", "Linux").with_startup_wizard_completed(true);
    let app = build_router(
        AppState::new(identity)
            .with_auth(Arc::clone(&auth))
            .with_legacy_auth_enabled(legacy_auth_enabled)
            .with_ready(true),
    );
    Fixture {
        app,
        auth,
        database,
        admin_token,
        user_token,
    }
}

async fn login_token(
    auth: &AuthService<SystemClock>,
    username: &str,
    password: &str,
    device_id: &str,
) -> String {
    auth.authenticate(
        username,
        password,
        ClientIdentity::new("Test", "Browser", device_id, "1.0").unwrap(),
    )
    .await
    .unwrap()
    .access_token()
    .expose_secret()
    .to_owned()
}

fn token_header(token: &str) -> String {
    format!(r#"MediaBrowser Token="{token}""#)
}

async fn request(
    app: axum::Router,
    method: Method,
    uri: impl AsRef<str>,
    token: Option<&str>,
) -> axum::response::Response {
    let mut request = Request::builder().method(method).uri(uri.as_ref());
    if let Some(token) = token {
        request = request.header(header::AUTHORIZATION, token_header(token));
    }
    app.oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap()
}

async fn json_response(response: axum::response::Response) -> Value {
    let body = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&body).unwrap()
}

fn assert_no_store(response: &axum::response::Response) {
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
}

#[tokio::test]
async fn administrator_can_create_list_authenticate_and_delete_an_api_key() {
    let fixture = fixture(true).await;

    let created = request(
        fixture.app.clone(),
        Method::POST,
        "/Auth/Keys?app=Kodi%20Sync",
        Some(&fixture.admin_token),
    )
    .await;
    assert_eq!(created.status(), StatusCode::NO_CONTENT);
    assert_no_store(&created);
    assert!(
        created
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .is_empty()
    );

    let listed = request(
        fixture.app.clone(),
        Method::GET,
        "/Auth/Keys",
        Some(&fixture.admin_token),
    )
    .await;
    assert_eq!(listed.status(), StatusCode::OK);
    assert_no_store(&listed);
    let body = json_response(listed).await;
    assert_eq!(body["TotalRecordCount"], 1);
    assert_eq!(body["StartIndex"], 0);
    assert_eq!(body["Items"][0]["AppName"], "Kodi Sync");
    assert_eq!(body["Items"][0]["UserName"], "Admin");
    assert_eq!(body["Items"][0]["IsActive"], true);
    assert_eq!(body["Items"][0]["DeviceId"], Value::Null);
    assert_eq!(body["Items"][0]["AppVersion"], Value::Null);
    assert_eq!(body["Items"][0]["DeviceName"], Value::Null);
    assert_eq!(body["Items"][0]["DateRevoked"], Value::Null);
    assert_eq!(body["Items"][0]["DateLastActivity"], Value::Null);
    assert_eq!(body["Items"][0].as_object().unwrap().len(), 12);
    let raw_key = body["Items"][0]["AccessToken"].as_str().unwrap();
    assert_eq!(raw_key.len(), 64);

    let me = request(fixture.app.clone(), Method::GET, "/Users/Me", Some(raw_key)).await;
    assert_eq!(me.status(), StatusCode::OK);
    assert_eq!(json_response(me).await["Name"], json!("Admin"));

    let encoded_key = format!("%{:02X}{}", raw_key.as_bytes()[0], &raw_key[1..]);
    let deleted = request(
        fixture.app.clone(),
        Method::DELETE,
        format!("/Auth/Keys/{encoded_key}"),
        Some(&fixture.admin_token),
    )
    .await;
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
    assert_no_store(&deleted);

    let revoked = request(fixture.app, Method::GET, "/Users/Me", Some(raw_key)).await;
    assert_eq!(revoked.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn lifecycle_errors_have_exact_statuses_and_are_never_cacheable() {
    let fixture = fixture(true).await;

    let unauthenticated = request(fixture.app.clone(), Method::GET, "/Auth/Keys", None).await;
    assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);
    assert_no_store(&unauthenticated);

    for uri in ["/Auth/Keys", "/Auth/Keys/unknown"] {
        let method_not_allowed = request(
            fixture.app.clone(),
            Method::PUT,
            uri,
            Some(&fixture.admin_token),
        )
        .await;
        assert_eq!(method_not_allowed.status(), StatusCode::METHOD_NOT_ALLOWED);
        assert_no_store(&method_not_allowed);
    }

    let forbidden = request(
        fixture.app.clone(),
        Method::POST,
        "/Auth/Keys?app=Viewer",
        Some(&fixture.user_token),
    )
    .await;
    assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);
    assert_no_store(&forbidden);

    let overlong = "x".repeat(257);
    for uri in [
        "/Auth/Keys".to_owned(),
        "/Auth/Keys?app=".to_owned(),
        "/Auth/Keys?app=one&app=two".to_owned(),
        "/Auth/Keys?app=one&unexpected=1".to_owned(),
        "/Auth/Keys?app=%".to_owned(),
        format!("/Auth/Keys?app={overlong}"),
    ] {
        let response = request(
            fixture.app.clone(),
            Method::POST,
            uri,
            Some(&fixture.admin_token),
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_no_store(&response);
    }

    for uri in [
        "/Auth/Keys?unexpected=1",
        "/Auth/Keys?ApiKey=",
        "/Auth/Keys?ApiKey=one&ApiKey=two",
    ] {
        let invalid_query = request(
            fixture.app.clone(),
            Method::GET,
            uri,
            Some(&fixture.admin_token),
        )
        .await;
        assert_eq!(invalid_query.status(), StatusCode::BAD_REQUEST);
        assert_no_store(&invalid_query);
    }

    for uri in ["/Auth/Keys?ApiKey=", "/Auth/Keys?ApiKey=one&ApiKey=two"] {
        let query_only = request(fixture.app.clone(), Method::GET, uri, None).await;
        assert_eq!(query_only.status(), StatusCode::BAD_REQUEST);
        assert_no_store(&query_only);
    }

    let path_secret = "this-secret-must-not-appear";
    let invalid_delete = request(
        fixture.app.clone(),
        Method::DELETE,
        format!("/Auth/Keys/{path_secret}"),
        Some(&fixture.admin_token),
    )
    .await;
    assert_eq!(invalid_delete.status(), StatusCode::BAD_REQUEST);
    assert_no_store(&invalid_delete);
    let body = invalid_delete
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes();
    assert!(!String::from_utf8_lossy(&body).contains(path_secret));

    let invalid_encoding = request(
        fixture.app.clone(),
        Method::DELETE,
        "/Auth/Keys/%FF",
        Some(&fixture.admin_token),
    )
    .await;
    assert_eq!(invalid_encoding.status(), StatusCode::BAD_REQUEST);
    assert_no_store(&invalid_encoding);

    let unknown = "00".repeat(32);
    let unknown_delete = request(
        fixture.app.clone(),
        Method::DELETE,
        format!("/Auth/Keys/{unknown}"),
        Some(&fixture.admin_token),
    )
    .await;
    assert_eq!(unknown_delete.status(), StatusCode::NO_CONTENT);
    assert_no_store(&unknown_delete);
}

#[tokio::test]
async fn malformed_app_query_is_rejected_before_authentication() {
    let fixture = fixture(true).await;
    let overlong = "x".repeat(257);

    for uri in [
        "/Auth/Keys?app=".to_owned(),
        "/Auth/Keys?app=%20%20".to_owned(),
        "/Auth/Keys?app=%0A".to_owned(),
        format!("/Auth/Keys?app={overlong}"),
    ] {
        let response = request(fixture.app.clone(), Method::POST, uri, None).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_no_store(&response);
    }
}

#[tokio::test]
async fn bounded_capacity_returns_no_store_409() {
    let fixture = fixture(true).await;
    let principal = fixture
        .auth
        .authenticate_token(&fixture.admin_token)
        .await
        .unwrap();
    for index in 0..256 {
        fixture
            .auth
            .create_api_key(&principal, &format!("Capacity {index}"))
            .await
            .unwrap();
    }

    let response = request(
        fixture.app,
        Method::POST,
        "/Auth/Keys?app=One%20Too%20Many",
        Some(&fixture.admin_token),
    )
    .await;

    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_no_store(&response);
}

#[tokio::test]
async fn missing_keyring_and_persistence_failures_return_no_store_503() {
    let missing_cipher = fixture(false).await;
    for method in [Method::GET, Method::POST] {
        let uri = if method == Method::POST {
            "/Auth/Keys?app=Automation"
        } else {
            "/Auth/Keys"
        };
        let response = request(
            missing_cipher.app.clone(),
            method,
            uri,
            Some(&missing_cipher.admin_token),
        )
        .await;
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_no_store(&response);
    }

    let persistence = fixture(true).await;
    persistence
        .database
        .execute(Statement::from_string(
            persistence.database.get_database_backend(),
            "DROP TABLE api_keys",
        ))
        .await
        .unwrap();
    let response = request(
        persistence.app,
        Method::GET,
        "/Auth/Keys",
        Some(&persistence.admin_token),
    )
    .await;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_no_store(&response);
}

#[tokio::test]
async fn api_key_principal_uses_aliases_but_session_only_operations_stay_forbidden() {
    let fixture = fixture(true).await;
    let created = request(
        fixture.app.clone(),
        Method::POST,
        "/Auth/Keys?app=Automation",
        Some(&fixture.admin_token),
    )
    .await;
    assert_eq!(created.status(), StatusCode::NO_CONTENT);
    let listed = request(
        fixture.app.clone(),
        Method::GET,
        "/Auth/Keys",
        Some(&fixture.admin_token),
    )
    .await;
    let body = json_response(listed).await;
    let raw_key = body["Items"][0]["AccessToken"].as_str().unwrap();

    for uri in [
        format!("/Users/Me?ApiKey={raw_key}"),
        format!("/Users/Me?api_key={raw_key}"),
        format!("/Auth/Keys?ApiKey={raw_key}"),
    ] {
        let response = request(fixture.app.clone(), Method::GET, uri, None).await;
        assert_eq!(response.status(), StatusCode::OK);
    }

    for uri in ["/Sessions", "/Devices"] {
        let response = request(fixture.app.clone(), Method::GET, uri, Some(raw_key)).await;
        assert_eq!(response.status(), StatusCode::OK, "{uri}");
    }

    for (method, uri) in [
        (Method::POST, "/Sessions/Capabilities/Full"),
        (Method::POST, "/Sessions/Capabilities?unexpected=1"),
        (Method::POST, "/Sessions/Logout?unexpected=1"),
        (Method::POST, "/Sessions/Playing"),
        (Method::POST, "/Sessions/Playing/Ping"),
        (Method::POST, "/Admin/Storage/OAuth/GoogleDrive/Start"),
        (Method::POST, "/Admin/Storage/OAuth/OneDrive/Start"),
    ] {
        let response = request(fixture.app.clone(), method, uri, Some(raw_key)).await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN, "{uri}");
    }

    let self_deleted = request(
        fixture.app.clone(),
        Method::DELETE,
        format!("/Auth/Keys/{raw_key}"),
        Some(raw_key),
    )
    .await;
    assert_eq!(self_deleted.status(), StatusCode::NO_CONTENT);
    assert_no_store(&self_deleted);

    let revoked = request(fixture.app, Method::GET, "/Users/Me", Some(raw_key)).await;
    assert_eq!(revoked.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn disabling_legacy_auth_keeps_canonical_api_key_query_authentication() {
    let fixture = fixture_with_legacy(true, false).await;
    let created = request(
        fixture.app.clone(),
        Method::POST,
        "/Auth/Keys?app=Canonical",
        Some(&fixture.admin_token),
    )
    .await;
    assert_eq!(created.status(), StatusCode::NO_CONTENT);
    let listed = request(
        fixture.app.clone(),
        Method::GET,
        "/Auth/Keys",
        Some(&fixture.admin_token),
    )
    .await;
    let body = json_response(listed).await;
    let raw_key = body["Items"][0]["AccessToken"].as_str().unwrap();

    let canonical = request(
        fixture.app.clone(),
        Method::GET,
        format!("/Auth/Keys?ApiKey={raw_key}"),
        None,
    )
    .await;
    assert_eq!(canonical.status(), StatusCode::OK);
    assert_no_store(&canonical);

    let legacy = request(
        fixture.app,
        Method::GET,
        format!("/Auth/Keys?api_key={raw_key}"),
        None,
    )
    .await;
    assert_eq!(legacy.status(), StatusCode::UNAUTHORIZED);
    assert_no_store(&legacy);
}
