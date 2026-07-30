use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::{
    body::Body,
    http::{Method, Request, StatusCode, header},
};
use chrono::Duration;
use http_body_util::BodyExt;
use sea_orm::{ConnectionTrait, DatabaseConnection};
use sea_orm_migration::MigratorTrait;
use serde_json::{Value, json};
use tempfile::TempDir;
use tjxy_application::{AuthService, SystemClock};
use tjxy_credentials::{CredentialCipher, CredentialKey};
use tjxy_db::MetadataProviderSettingsRepository;
use tjxy_metadata::{
    MetadataItemKind, MetadataLookup, MetadataProvider, MetadataProviderError,
    ReloadableMetadataProvider, TmdbProvider, TmdbSearchItem, TmdbTransport,
};
use tjxy_server::{ServerIdentity, StartupOptions, build_router, initialize};
use tjxy_test_support::{ReconnectableTestDatabase, reconnectable_test_database};
use tower::ServiceExt;
use uuid::Uuid;

const ADMIN_PASSWORD: &str = "admin password";
const VIEWER_PASSWORD: &str = "viewer password";

type ValidationCall = (String, String);

struct FixtureTransport {
    label: String,
    language: String,
    validation_calls: Arc<Mutex<Vec<ValidationCall>>>,
}

#[async_trait]
impl TmdbTransport for FixtureTransport {
    async fn validate(&self) -> Result<(), MetadataProviderError> {
        self.validation_calls
            .lock()
            .unwrap()
            .push((self.label.clone(), self.language.clone()));
        if self.label == "rejected" {
            Err(MetadataProviderError::Rejected)
        } else {
            Ok(())
        }
    }

    async fn search(
        &self,
        _kind: MetadataItemKind,
        _query: &str,
        _year: Option<i32>,
        _language: &str,
    ) -> Result<Vec<TmdbSearchItem>, MetadataProviderError> {
        Ok(vec![TmdbSearchItem::new(
            1,
            format!("{}:{}", self.label, self.language),
        )])
    }
}

struct Fixture {
    app: axum::Router,
    database: DatabaseConnection,
    _database_owner: ReconnectableTestDatabase,
    _assets: TempDir,
    cipher: Option<Arc<CredentialCipher>>,
    tmdb: Arc<ReloadableMetadataProvider>,
    validation_calls: Arc<Mutex<Vec<ValidationCall>>>,
    admin_token: String,
    viewer_token: String,
}

async fn fixture(with_cipher: bool, with_environment_fallback: bool) -> Fixture {
    let database_fixture = reconnectable_test_database().await.unwrap();
    let database = database_fixture.connection().clone();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    seed_users(&database).await;
    let cipher = with_cipher.then(test_cipher);
    let tmdb = Arc::new(ReloadableMetadataProvider::new("Tmdb"));
    let validation_calls = Arc::new(Mutex::new(Vec::new()));
    let calls = Arc::clone(&validation_calls);
    let mut options = StartupOptions::new(
        database_fixture.database_url(),
        ServerIdentity::new(Uuid::new_v4(), "TJXY", "Linux"),
    )
    .with_tmdb_provider(Arc::clone(&tmdb))
    .with_tmdb_provider_factory(move |access_token, language| {
        fixture_provider(access_token, language, Arc::clone(&calls))
    });
    let assets = TempDir::new().unwrap();
    options = options.with_assets_dir(assets.path());
    if let Some(cipher) = cipher.as_ref() {
        options = options.with_credential_cipher(Arc::clone(cipher));
    }
    if with_environment_fallback {
        let provider = Arc::new(
            fixture_provider("environment", "zh-CN", Arc::clone(&validation_calls)).unwrap(),
        );
        tmdb.replace(Some(provider.clone()));
        options = options.with_tmdb_environment_fallback(provider, "zh-CN");
    }
    let app = build_router(initialize(options).await.unwrap());
    let admin_token = login_token(&app, "Admin", ADMIN_PASSWORD, "admin-device").await;
    let viewer_token = login_token(&app, "Viewer", VIEWER_PASSWORD, "viewer-device").await;
    Fixture {
        app,
        database,
        _database_owner: database_fixture,
        _assets: assets,
        cipher,
        tmdb,
        validation_calls,
        admin_token,
        viewer_token,
    }
}

fn fixture_provider(
    access_token: &str,
    language: &str,
    validation_calls: Arc<Mutex<Vec<ValidationCall>>>,
) -> Result<TmdbProvider, tjxy_metadata::MetadataError> {
    TmdbProvider::with_transport(
        language,
        Arc::new(FixtureTransport {
            label: access_token.to_owned(),
            language: language.to_owned(),
            validation_calls,
        }),
    )
}

fn test_cipher() -> Arc<CredentialCipher> {
    Arc::new(
        CredentialCipher::new(CredentialKey::new(1, [77_u8; 32]).unwrap(), Vec::new()).unwrap(),
    )
}

async fn seed_users(database: &DatabaseConnection) {
    let auth = AuthService::new(database.clone(), SystemClock, Some(Duration::days(30)), 2)
        .await
        .unwrap();
    auth.create_user("Admin", ADMIN_PASSWORD, true)
        .await
        .unwrap();
    auth.create_user("Viewer", VIEWER_PASSWORD, false)
        .await
        .unwrap();
}

async fn login_token(
    app: &axum::Router,
    username: &str,
    password: &str,
    device_id: &str,
) -> String {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/Users/AuthenticateByName")
                .header(
                    header::AUTHORIZATION,
                    format!(
                        r#"MediaBrowser Client="Test", Device="Browser", DeviceId="{device_id}", Version="1.0""#
                    ),
                )
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"Username": username, "Pw": password}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    json_response(response).await["AccessToken"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn token_header(token: &str) -> String {
    format!(r#"MediaBrowser Token="{token}""#)
}

async fn request(
    app: axum::Router,
    method: Method,
    token: Option<&str>,
    content_type: Option<&str>,
    body: impl Into<Body>,
) -> axum::response::Response {
    let mut request = Request::builder()
        .method(method)
        .uri("/Admin/Metadata/Providers/Tmdb");
    if let Some(token) = token {
        request = request.header(header::AUTHORIZATION, token_header(token));
    }
    if let Some(content_type) = content_type {
        request = request.header(header::CONTENT_TYPE, content_type);
    }
    app.oneshot(request.body(body.into()).unwrap())
        .await
        .unwrap()
}

async fn test_request(app: axum::Router, token: &str, body: Value) -> axum::response::Response {
    app.oneshot(
        Request::builder()
            .method(Method::POST)
            .uri("/Admin/Metadata/Providers/Tmdb/Test")
            .header(header::AUTHORIZATION, token_header(token))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .unwrap(),
    )
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

async fn active_title(provider: &ReloadableMetadataProvider) -> Option<String> {
    let lookup = MetadataLookup::new(MetadataItemKind::Movie, "Fixture", None).unwrap();
    let candidate = provider.resolve(&lookup).await.unwrap()?;
    let resolution = tjxy_metadata::MetadataResolution::from_candidate(&lookup, candidate).unwrap();
    Some(resolution.title().to_owned())
}

#[tokio::test]
async fn routes_require_an_administrator_and_strict_json_without_becoming_cacheable() {
    let fixture = fixture(true, false).await;

    let unauthenticated =
        request(fixture.app.clone(), Method::GET, None, None, Body::empty()).await;
    assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);
    assert_no_store(&unauthenticated);

    let forbidden = request(
        fixture.app.clone(),
        Method::GET,
        Some(&fixture.viewer_token),
        None,
        Body::empty(),
    )
    .await;
    assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);
    assert_no_store(&forbidden);

    for (content_type, body) in [
        (None, json!({"Enabled": true, "Language": "en-AU"})),
        (
            Some("text/plain"),
            json!({"Enabled": true, "Language": "en-AU"}),
        ),
        (
            Some("application/json"),
            json!({
                "Enabled": true,
                "Language": "en-AU",
                "AccessToken": "draft",
                "Unexpected": true
            }),
        ),
    ] {
        let response = request(
            fixture.app.clone(),
            Method::PUT,
            Some(&fixture.admin_token),
            content_type,
            Body::from(body.to_string()),
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_no_store(&response);
    }
}

#[tokio::test]
async fn unconfigured_state_and_missing_cipher_fail_closed_without_exposing_credentials() {
    let fixture = fixture(false, false).await;
    let get = request(
        fixture.app.clone(),
        Method::GET,
        Some(&fixture.admin_token),
        None,
        Body::empty(),
    )
    .await;
    assert_eq!(get.status(), StatusCode::OK);
    assert_no_store(&get);
    assert_eq!(
        json_response(get).await,
        json!({
            "Provider": "Tmdb",
            "Configured": false,
            "Enabled": false,
            "Language": "zh-CN",
            "Revision": null,
            "Source": "None",
            "EncryptionAvailable": false
        })
    );

    let put = request(
        fixture.app.clone(),
        Method::PUT,
        Some(&fixture.admin_token),
        Some("application/json; charset=utf-8"),
        Body::from(
            json!({
                "Enabled": true,
                "Language": "en-AU",
                "AccessToken": "must-not-leak"
            })
            .to_string(),
        ),
    )
    .await;
    assert_eq!(put.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_no_store(&put);
    let bytes = put.into_body().collect().await.unwrap().to_bytes();
    assert!(
        !bytes
            .windows(b"must-not-leak".len())
            .any(|window| window == b"must-not-leak")
    );

    let external_cipher = test_cipher();
    let sealed = external_cipher
        .seal_bound(Uuid::new_v4(), "tmdb", b"configured-token")
        .unwrap();
    MetadataProviderSettingsRepository::new(&fixture.database)
        .put(&sealed, true, "en-US", None)
        .await
        .unwrap();
    let configured_test = test_request(fixture.app, &fixture.admin_token, json!({})).await;
    assert_eq!(configured_test.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_no_store(&configured_test);
}

#[tokio::test]
async fn configured_environment_token_test_fails_closed_without_a_cipher() {
    let fixture = fixture(false, true).await;

    let configured_test = test_request(fixture.app, &fixture.admin_token, json!({})).await;

    assert_eq!(configured_test.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_no_store(&configured_test);
    assert!(fixture.validation_calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn encrypted_updates_rotate_with_revision_fencing_and_hot_apply_only_after_persistence() {
    let fixture = fixture(true, false).await;
    let cipher = fixture.cipher.as_ref().unwrap();

    let created = request(
        fixture.app.clone(),
        Method::PUT,
        Some(&fixture.admin_token),
        Some("application/json"),
        Body::from(
            json!({
                "Enabled": true,
                "Language": "en-AU",
                "AccessToken": "first-token"
            })
            .to_string(),
        ),
    )
    .await;
    assert_eq!(created.status(), StatusCode::OK);
    assert_no_store(&created);
    let created = json_response(created).await;
    assert_eq!(created["Revision"], 1);
    assert_eq!(created["Source"], "Database");
    assert_eq!(created["Configured"], true);
    assert_eq!(created["Enabled"], true);
    assert!(created.get("AccessToken").is_none());
    assert_eq!(
        active_title(&fixture.tmdb).await.as_deref(),
        Some("first-token:en-AU")
    );
    let repository = MetadataProviderSettingsRepository::new(&fixture.database);
    let first = repository.get("tmdb").await.unwrap().unwrap();
    assert_eq!(
        cipher
            .open(first.credential_id(), first.provider(), first.envelope())
            .unwrap()
            .as_slice(),
        b"first-token"
    );
    let raw = fixture
        .database
        .query_one(sea_orm::Statement::from_string(
            fixture.database.get_database_backend(),
            "SELECT encrypted_payload FROM metadata_provider_settings WHERE provider = 'tmdb'",
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get::<Vec<u8>>("", "encrypted_payload")
        .unwrap();
    assert!(
        !raw.windows(b"first-token".len())
            .any(|window| window == b"first-token")
    );

    let disabled = request(
        fixture.app.clone(),
        Method::PUT,
        Some(&fixture.admin_token),
        Some("application/json"),
        Body::from(json!({"Enabled": false, "Language": "fr-FR", "Revision": 1}).to_string()),
    )
    .await;
    assert_eq!(disabled.status(), StatusCode::OK);
    assert_eq!(json_response(disabled).await["Revision"], 2);
    assert!(active_title(&fixture.tmdb).await.is_none());

    let rotated = request(
        fixture.app.clone(),
        Method::PUT,
        Some(&fixture.admin_token),
        Some("application/json"),
        Body::from(
            json!({
                "Enabled": true,
                "Language": "de-DE",
                "AccessToken": "replacement-token",
                "Revision": 2
            })
            .to_string(),
        ),
    )
    .await;
    assert_eq!(rotated.status(), StatusCode::OK);
    assert_eq!(json_response(rotated).await["Revision"], 3);
    assert_eq!(
        active_title(&fixture.tmdb).await.as_deref(),
        Some("replacement-token:de-DE")
    );
    let replacement = repository.get("tmdb").await.unwrap().unwrap();
    assert_eq!(replacement.credential_id(), first.credential_id());
    assert_eq!(
        cipher
            .open(
                replacement.credential_id(),
                replacement.provider(),
                replacement.envelope()
            )
            .unwrap()
            .as_slice(),
        b"replacement-token"
    );

    let stale = request(
        fixture.app.clone(),
        Method::PUT,
        Some(&fixture.admin_token),
        Some("application/json"),
        Body::from(
            json!({
                "Enabled": true,
                "Language": "it-IT",
                "AccessToken": "stale-token",
                "Revision": 2
            })
            .to_string(),
        ),
    )
    .await;
    assert_eq!(stale.status(), StatusCode::CONFLICT);
    assert_no_store(&stale);
    assert_eq!(
        active_title(&fixture.tmdb).await.as_deref(),
        Some("replacement-token:de-DE")
    );

    let get = request(
        fixture.app,
        Method::GET,
        Some(&fixture.admin_token),
        None,
        Body::empty(),
    )
    .await;
    let body = json_response(get).await;
    assert_eq!(body["Revision"], 3);
    assert_eq!(body["Language"], "de-DE");
    assert!(body.get("AccessToken").is_none());
    assert!(!body.to_string().contains("replacement-token"));
}

#[tokio::test]
async fn draft_and_configured_connection_tests_use_the_right_token_without_persisting_the_draft() {
    let fixture = fixture(true, false).await;

    let draft = test_request(
        fixture.app.clone(),
        &fixture.admin_token,
        json!({"AccessToken": "draft-token", "Language": "it-IT"}),
    )
    .await;
    assert_eq!(draft.status(), StatusCode::OK);
    assert_no_store(&draft);
    assert_eq!(json_response(draft).await, json!({"Status": "Success"}));
    assert!(
        MetadataProviderSettingsRepository::new(&fixture.database)
            .get("tmdb")
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        fixture.validation_calls.lock().unwrap().as_slice(),
        [("draft-token".to_owned(), "it-IT".to_owned())]
    );

    let saved = request(
        fixture.app.clone(),
        Method::PUT,
        Some(&fixture.admin_token),
        Some("application/json"),
        Body::from(
            json!({
                "Enabled": true,
                "Language": "en-NZ",
                "AccessToken": "configured-token"
            })
            .to_string(),
        ),
    )
    .await;
    assert_eq!(saved.status(), StatusCode::OK);
    let configured = test_request(fixture.app, &fixture.admin_token, json!({})).await;
    assert_eq!(configured.status(), StatusCode::OK);
    assert_eq!(
        fixture.validation_calls.lock().unwrap().as_slice(),
        [
            ("draft-token".to_owned(), "it-IT".to_owned()),
            ("configured-token".to_owned(), "en-NZ".to_owned())
        ]
    );
}

#[tokio::test]
async fn delete_is_idempotent_and_restores_the_environment_fallback() {
    let fixture = fixture(true, true).await;
    assert_eq!(
        active_title(&fixture.tmdb).await.as_deref(),
        Some("environment:zh-CN")
    );
    let saved = request(
        fixture.app.clone(),
        Method::PUT,
        Some(&fixture.admin_token),
        Some("application/json"),
        Body::from(
            json!({
                "Enabled": true,
                "Language": "en-US",
                "AccessToken": "database-token"
            })
            .to_string(),
        ),
    )
    .await;
    assert_eq!(saved.status(), StatusCode::OK);
    assert_eq!(
        active_title(&fixture.tmdb).await.as_deref(),
        Some("database-token:en-US")
    );

    for _ in 0..2 {
        let deleted = request(
            fixture.app.clone(),
            Method::DELETE,
            Some(&fixture.admin_token),
            None,
            Body::empty(),
        )
        .await;
        assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
        assert_no_store(&deleted);
    }
    assert_eq!(
        active_title(&fixture.tmdb).await.as_deref(),
        Some("environment:zh-CN")
    );
    let get = request(
        fixture.app,
        Method::GET,
        Some(&fixture.admin_token),
        None,
        Body::empty(),
    )
    .await;
    assert_eq!(
        json_response(get).await,
        json!({
            "Provider": "Tmdb",
            "Configured": true,
            "Enabled": true,
            "Language": "zh-CN",
            "Revision": null,
            "Source": "Environment",
            "EncryptionAvailable": true
        })
    );
}
