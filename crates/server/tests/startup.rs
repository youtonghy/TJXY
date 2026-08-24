use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use chrono::{TimeZone, Utc};
use http_body_util::BodyExt;
use sea_orm::{
    ConnectionTrait, DatabaseConnection, Statement, TransactionTrait,
    sea_query::{Alias, Expr, Query},
};
use sea_orm_migration::MigratorTrait;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tempfile::TempDir;
use tjxy_application::{AuthService, ClientIdentity, SystemClock};
use tjxy_cache::{RedisCacheConfig, RedisMode};
use tjxy_common::{CatalogItemId, SortKey, StorageObjectRecordId, StorageRootId};
use tjxy_credentials::{CredentialCipher, CredentialKey};
use tjxy_db::{
    CatalogPublicationRepository, FilesystemRootDraft, LibraryPolicyUpdate, LibraryRepository,
    MetadataProviderSettingsRepository, SchemaMigrationError, SystemSettingsInput,
    SystemSettingsRepository, WorkJobRepository, WorkJobSpec, WorkJobState, WorkScope,
    WorkTaskKind,
};
use tjxy_metadata::{
    MetadataItemKind, MetadataLookup, MetadataProvider, MetadataProviderError,
    ReloadableMetadataProvider, TmdbProvider, TmdbSearchItem, TmdbTransport,
};
use tjxy_server::{
    ApiKeyValidationError, BootstrapAdmin, InitializationError, ServerIdentity, StartupOptions,
    build_router, initialize,
};
use tjxy_storage::{
    BackendError, ByteRange, ByteStream, ChangeCursor, ChangePage, ObjectPage, PageToken,
    StorageBackend, StorageCapabilities, StorageObject, StorageObjectId,
};
use tjxy_storage_filesystem::FilesystemBackend;
use tjxy_test_support::{ReconnectableTestDatabase, reconnectable_test_database};
use tower::ServiceExt;
use uuid::Uuid;

struct ChangeAwareFilesystem {
    filesystem: FilesystemBackend,
    change_calls: AtomicUsize,
}

struct StartupTmdbTransport {
    label: String,
    language: String,
}

#[async_trait::async_trait]
impl TmdbTransport for StartupTmdbTransport {
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

    async fn detail(
        &self,
        _kind: MetadataItemKind,
        _id: u64,
        _language: &str,
    ) -> Result<tjxy_metadata::MetadataCandidate, MetadataProviderError> {
        let source = tjxy_metadata::MetadataSource::new("Tmdb", Some("movie:1"), 8_000)
            .map_err(|_| MetadataProviderError::InvalidResponse)?;
        Ok(tjxy_metadata::MetadataCandidate::new(source)
            .with_title(format!("{}:{}", self.label, self.language))
            .with_provider_id("tmdb", "1")
            .with_details_loaded())
    }
}

#[async_trait::async_trait]
impl StorageBackend for ChangeAwareFilesystem {
    async fn get_object(&self, id: &StorageObjectId) -> Result<StorageObject, BackendError> {
        self.filesystem.get_object(id).await
    }

    async fn list_children(
        &self,
        parent: &StorageObjectId,
        page: Option<PageToken>,
    ) -> Result<ObjectPage, BackendError> {
        self.filesystem.list_children(parent, page).await
    }

    async fn list_changes(&self, _cursor: ChangeCursor) -> Result<ChangePage, BackendError> {
        self.change_calls.fetch_add(1, Ordering::SeqCst);
        Ok(ChangePage::new(
            Vec::new(),
            ChangeCursor::new("change-terminal").unwrap(),
        ))
    }

    async fn open_range(
        &self,
        id: &StorageObjectId,
        range: ByteRange,
    ) -> Result<ByteStream, BackendError> {
        self.filesystem.open_range(id, range).await
    }

    fn capabilities(&self) -> StorageCapabilities {
        self.filesystem.capabilities().with_changes(true)
    }
}

fn api_key_cipher(
    active_version: i32,
    active_byte: u8,
    historical: &[(i32, u8)],
) -> Arc<CredentialCipher> {
    let active = CredentialKey::new(active_version, [active_byte; 32]).unwrap();
    let historical = historical
        .iter()
        .map(|(version, byte)| CredentialKey::new(*version, [*byte; 32]).unwrap())
        .collect();
    Arc::new(CredentialCipher::new(active, historical).unwrap())
}

fn startup_tmdb_provider(
    access_token: &str,
    language: &str,
) -> Result<TmdbProvider, tjxy_metadata::MetadataError> {
    TmdbProvider::with_transport(
        language,
        Arc::new(StartupTmdbTransport {
            label: access_token.to_owned(),
            language: language.to_owned(),
        }),
    )
}

async fn active_tmdb_title(provider: &ReloadableMetadataProvider) -> Option<String> {
    let lookup = MetadataLookup::new(MetadataItemKind::Movie, "Fixture", None).unwrap();
    let candidate = provider.resolve(&lookup).await.unwrap()?;
    let resolution = tjxy_metadata::MetadataResolution::from_candidate(&lookup, candidate).unwrap();
    Some(resolution.title().to_owned())
}

async fn seed_tmdb_settings(
    database: &DatabaseConnection,
    cipher: &CredentialCipher,
    enabled: bool,
    language: &str,
    token: &[u8],
) {
    let sealed = cipher.seal_bound(Uuid::new_v4(), "tmdb", token).unwrap();
    MetadataProviderSettingsRepository::new(database)
        .put(&sealed, enabled, language, None)
        .await
        .unwrap();
}

fn tmdb_startup_options(
    fixture: &ReconnectableTestDatabase,
    assets: &TempDir,
    runtime: Arc<ReloadableMetadataProvider>,
    fallback: Arc<TmdbProvider>,
) -> StartupOptions {
    StartupOptions::new(
        fixture.database_url(),
        ServerIdentity::new(Uuid::new_v4(), "TJXY", "Linux"),
    )
    .with_assets_dir(assets.path())
    .with_bootstrap_admin(BootstrapAdmin::new("Admin", "password"))
    .with_tmdb_provider(runtime)
    .with_tmdb_environment_fallback(fallback, "zh-CN")
    .with_tmdb_provider_factory(startup_tmdb_provider)
}

async fn seed_api_keys(
    database: &DatabaseConnection,
    cipher: Arc<CredentialCipher>,
    app_names: &[&str],
) {
    let service = AuthService::new(
        database.clone(),
        SystemClock,
        Some(chrono::Duration::days(30)),
        2,
    )
    .await
    .unwrap()
    .with_credential_cipher(cipher);
    service
        .create_user("Admin", "password", true)
        .await
        .unwrap();
    let issued = service
        .authenticate(
            "Admin",
            "password",
            ClientIdentity::new("Test", "Browser", "startup-key-test", "1.0").unwrap(),
        )
        .await
        .unwrap();
    let admin = service
        .authenticate_token(issued.access_token().expose_secret())
        .await
        .unwrap();
    for app_name in app_names {
        service.create_api_key(&admin, app_name).await.unwrap();
    }
}

fn api_key_startup_options(
    fixture: &ReconnectableTestDatabase,
    assets: &TempDir,
) -> StartupOptions {
    StartupOptions::new(
        fixture.database_url(),
        ServerIdentity::new(Uuid::new_v4(), "TJXY", "Linux"),
    )
    .with_assets_dir(assets.path())
}

fn assert_api_key_validation_error(error: &InitializationError, expected: ApiKeyValidationError) {
    let InitializationError::ApiKeyValidation(actual) = error else {
        panic!("expected API key validation error, got {error:?}");
    };
    assert_eq!(*actual, expected);
    assert_eq!(error.to_string(), "API key validation failed");
    assert_eq!(
        format!("{error:?}"),
        format!("ApiKeyValidation({expected:?})")
    );
    let source = std::error::Error::source(error).unwrap();
    assert_eq!(source.to_string(), expected.to_string());
    assert_eq!(format!("{source:?}"), format!("{expected:?}"));
    assert!(source.source().is_none());
}

#[tokio::test]
async fn api_key_startup_without_persisted_keys_does_not_require_a_keyring() {
    let fixture = reconnectable_test_database().await.unwrap();
    tjxy_db::Migrator::up(fixture.connection(), None)
        .await
        .unwrap();
    let service = AuthService::new(
        fixture.connection().clone(),
        SystemClock,
        Some(chrono::Duration::days(30)),
        2,
    )
    .await
    .unwrap();
    service
        .create_user("Admin", "password", true)
        .await
        .unwrap();
    drop(service);
    let assets = TempDir::new().unwrap();

    initialize(api_key_startup_options(&fixture, &assets))
        .await
        .unwrap();
}

#[tokio::test]
async fn persisted_enabled_tmdb_settings_replace_the_environment_fallback() {
    let fixture = reconnectable_test_database().await.unwrap();
    tjxy_db::Migrator::up(fixture.connection(), None)
        .await
        .unwrap();
    let cipher = api_key_cipher(1, 81, &[]);
    seed_tmdb_settings(
        fixture.connection(),
        &cipher,
        true,
        "en-AU",
        b"database-token",
    )
    .await;
    let runtime = Arc::new(ReloadableMetadataProvider::new("Tmdb"));
    let fallback = Arc::new(startup_tmdb_provider("environment-token", "zh-CN").unwrap());
    runtime.replace(Some(fallback.clone()));
    let assets = TempDir::new().unwrap();

    initialize(
        tmdb_startup_options(&fixture, &assets, Arc::clone(&runtime), fallback)
            .with_credential_cipher(cipher),
    )
    .await
    .unwrap();

    assert_eq!(
        active_tmdb_title(&runtime).await.as_deref(),
        Some("database-token:en-AU")
    );
}

#[tokio::test]
async fn persisted_disabled_tmdb_settings_suppress_the_environment_fallback() {
    let fixture = reconnectable_test_database().await.unwrap();
    tjxy_db::Migrator::up(fixture.connection(), None)
        .await
        .unwrap();
    let cipher = api_key_cipher(1, 82, &[]);
    seed_tmdb_settings(
        fixture.connection(),
        &cipher,
        false,
        "en-US",
        b"disabled-database-token",
    )
    .await;
    let runtime = Arc::new(ReloadableMetadataProvider::new("Tmdb"));
    let fallback = Arc::new(startup_tmdb_provider("environment-token", "zh-CN").unwrap());
    runtime.replace(Some(fallback.clone()));
    let assets = TempDir::new().unwrap();

    initialize(
        tmdb_startup_options(&fixture, &assets, Arc::clone(&runtime), fallback)
            .with_credential_cipher(cipher),
    )
    .await
    .unwrap();

    assert!(active_tmdb_title(&runtime).await.is_none());
}

#[tokio::test]
async fn missing_database_tmdb_settings_preserve_the_environment_fallback() {
    let fixture = reconnectable_test_database().await.unwrap();
    let runtime = Arc::new(ReloadableMetadataProvider::new("Tmdb"));
    let fallback = Arc::new(startup_tmdb_provider("environment-token", "zh-CN").unwrap());
    runtime.replace(Some(fallback.clone()));
    let assets = TempDir::new().unwrap();

    initialize(tmdb_startup_options(
        &fixture,
        &assets,
        Arc::clone(&runtime),
        fallback,
    ))
    .await
    .unwrap();

    assert_eq!(
        active_tmdb_title(&runtime).await.as_deref(),
        Some("environment-token:zh-CN")
    );
}

#[tokio::test]
async fn unreadable_persisted_tmdb_settings_prevent_readiness_without_exposing_plaintext() {
    let fixture = reconnectable_test_database().await.unwrap();
    tjxy_db::Migrator::up(fixture.connection(), None)
        .await
        .unwrap();
    let cipher = api_key_cipher(1, 83, &[]);
    let token = b"startup-token-must-not-leak";
    seed_tmdb_settings(fixture.connection(), &cipher, true, "en-GB", token).await;
    let row = fixture
        .connection()
        .query_one(
            fixture.connection().get_database_backend().build(
                &Query::select()
                    .column(Alias::new("encrypted_payload"))
                    .from(Alias::new("metadata_provider_settings"))
                    .and_where(Expr::col(Alias::new("provider")).eq("tmdb"))
                    .to_owned(),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    let mut payload = row.try_get::<Vec<u8>>("", "encrypted_payload").unwrap();
    payload[12] ^= 1;
    fixture
        .connection()
        .execute(
            fixture.connection().get_database_backend().build(
                &Query::update()
                    .table(Alias::new("metadata_provider_settings"))
                    .value(Alias::new("encrypted_payload"), payload)
                    .and_where(Expr::col(Alias::new("provider")).eq("tmdb"))
                    .to_owned(),
            ),
        )
        .await
        .unwrap();
    let runtime = Arc::new(ReloadableMetadataProvider::new("Tmdb"));
    let fallback = Arc::new(startup_tmdb_provider("environment-token", "zh-CN").unwrap());
    runtime.replace(Some(fallback.clone()));
    let assets = TempDir::new().unwrap();

    let Err(error) = initialize(
        tmdb_startup_options(&fixture, &assets, runtime, fallback).with_credential_cipher(cipher),
    )
    .await
    else {
        panic!("unreadable persisted TMDB settings unexpectedly reached readiness");
    };

    assert!(matches!(
        error,
        InitializationError::MetadataSettingsValidation(_)
    ));
    assert_eq!(
        error.to_string(),
        "metadata provider settings validation failed"
    );
    assert!(
        !error
            .to_string()
            .as_bytes()
            .windows(token.len())
            .any(|window| window == token)
    );
    assert!(
        !format!("{error:?}")
            .as_bytes()
            .windows(token.len())
            .any(|window| window == token)
    );
}

#[tokio::test]
async fn api_key_startup_rejects_persisted_keys_without_a_keyring() {
    let fixture = reconnectable_test_database().await.unwrap();
    tjxy_db::Migrator::up(fixture.connection(), None)
        .await
        .unwrap();
    seed_api_keys(
        fixture.connection(),
        api_key_cipher(1, 11, &[]),
        &["Automation"],
    )
    .await;
    let assets = TempDir::new().unwrap();

    let Err(error) = initialize(api_key_startup_options(&fixture, &assets)).await else {
        panic!("persisted API keys unexpectedly started without a keyring");
    };

    assert_api_key_validation_error(&error, ApiKeyValidationError::KeyringUnavailable);
}

#[tokio::test]
async fn startup_classifies_schema_drift_without_sql_details() {
    let fixture = reconnectable_test_database().await.unwrap();
    tjxy_db::Migrator::up(fixture.connection(), None)
        .await
        .unwrap();
    seed_api_keys(
        fixture.connection(),
        api_key_cipher(1, 12, &[]),
        &["Automation"],
    )
    .await;
    fixture
        .connection()
        .execute(Statement::from_string(
            fixture.connection().get_database_backend(),
            "DROP TABLE api_keys",
        ))
        .await
        .unwrap();
    let assets = TempDir::new().unwrap();

    let Err(error) = initialize(api_key_startup_options(&fixture, &assets)).await else {
        panic!("startup unexpectedly ignored schema drift");
    };

    let message = error.to_string();
    assert!(!message.contains("DROP TABLE"));
    assert!(!message.contains("SELECT "));
    let InitializationError::DatabaseSchema(SchemaMigrationError::SchemaDrift { missing }) = error
    else {
        panic!("expected schema drift error, got {error:?}");
    };
    assert_eq!(missing, vec!["table api_keys"]);
}

#[tokio::test]
async fn api_key_startup_accepts_keys_encrypted_by_the_current_key() {
    let fixture = reconnectable_test_database().await.unwrap();
    tjxy_db::Migrator::up(fixture.connection(), None)
        .await
        .unwrap();
    let cipher = api_key_cipher(1, 21, &[]);
    seed_api_keys(fixture.connection(), Arc::clone(&cipher), &["Automation"]).await;
    let assets = TempDir::new().unwrap();

    initialize(api_key_startup_options(&fixture, &assets).with_credential_cipher(cipher))
        .await
        .unwrap();
}

#[tokio::test]
async fn api_key_startup_accepts_historical_keys_after_keyring_rotation() {
    let fixture = reconnectable_test_database().await.unwrap();
    tjxy_db::Migrator::up(fixture.connection(), None)
        .await
        .unwrap();
    seed_api_keys(
        fixture.connection(),
        api_key_cipher(1, 31, &[]),
        &["Automation"],
    )
    .await;
    let rotated = api_key_cipher(2, 32, &[(1, 31)]);
    let assets = TempDir::new().unwrap();

    initialize(api_key_startup_options(&fixture, &assets).with_credential_cipher(rotated))
        .await
        .unwrap();
}

#[tokio::test]
async fn api_key_startup_rejects_a_missing_historical_key_version() {
    let fixture = reconnectable_test_database().await.unwrap();
    tjxy_db::Migrator::up(fixture.connection(), None)
        .await
        .unwrap();
    seed_api_keys(
        fixture.connection(),
        api_key_cipher(1, 41, &[]),
        &["Automation"],
    )
    .await;
    let assets = TempDir::new().unwrap();
    let active_v2 = api_key_cipher(2, 42, &[]);

    let Err(error) =
        initialize(api_key_startup_options(&fixture, &assets).with_credential_cipher(active_v2))
            .await
    else {
        panic!("persisted API keys unexpectedly started without a historical key");
    };

    assert_api_key_validation_error(&error, ApiKeyValidationError::KeyringUnavailable);
}

#[tokio::test]
async fn api_key_startup_rejects_a_corrupted_encrypted_payload() {
    let fixture = reconnectable_test_database().await.unwrap();
    tjxy_db::Migrator::up(fixture.connection(), None)
        .await
        .unwrap();
    let cipher = api_key_cipher(1, 51, &[]);
    seed_api_keys(fixture.connection(), Arc::clone(&cipher), &["Automation"]).await;
    let table = Alias::new("api_keys");
    let row = fixture
        .connection()
        .query_one(
            fixture.connection().get_database_backend().build(
                &Query::select()
                    .columns([Alias::new("id"), Alias::new("encrypted_payload")])
                    .from(table.clone())
                    .limit(1)
                    .to_owned(),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    let id = row.try_get::<i64>("", "id").unwrap();
    let mut payload = row.try_get::<Vec<u8>>("", "encrypted_payload").unwrap();
    payload[12] ^= 1;
    let update = Query::update()
        .table(table)
        .value(Alias::new("encrypted_payload"), payload)
        .and_where(Expr::col(Alias::new("id")).eq(id))
        .to_owned();
    fixture
        .connection()
        .execute(fixture.connection().get_database_backend().build(&update))
        .await
        .unwrap();
    let assets = TempDir::new().unwrap();

    let Err(error) =
        initialize(api_key_startup_options(&fixture, &assets).with_credential_cipher(cipher)).await
    else {
        panic!("persisted API keys unexpectedly started with a corrupted payload");
    };

    assert_api_key_validation_error(&error, ApiKeyValidationError::EnvelopeUnreadable);
}

#[tokio::test]
async fn api_key_startup_rejects_swapped_envelope_identities() {
    let fixture = reconnectable_test_database().await.unwrap();
    tjxy_db::Migrator::up(fixture.connection(), None)
        .await
        .unwrap();
    let cipher = api_key_cipher(1, 61, &[]);
    seed_api_keys(
        fixture.connection(),
        Arc::clone(&cipher),
        &["First", "Second"],
    )
    .await;
    let table = Alias::new("api_keys");
    let rows = fixture
        .connection()
        .query_all(
            fixture.connection().get_database_backend().build(
                &Query::select()
                    .columns([Alias::new("id"), Alias::new("envelope_id")])
                    .from(table.clone())
                    .order_by(Alias::new("id"), sea_orm::sea_query::Order::Asc)
                    .to_owned(),
            ),
        )
        .await
        .unwrap();
    let first_id = rows[0].try_get::<i64>("", "id").unwrap();
    let first_envelope = rows[0].try_get::<Uuid>("", "envelope_id").unwrap();
    let second_id = rows[1].try_get::<i64>("", "id").unwrap();
    let second_envelope = rows[1].try_get::<Uuid>("", "envelope_id").unwrap();
    for (id, envelope_id) in [
        (first_id, Uuid::new_v4()),
        (second_id, first_envelope),
        (first_id, second_envelope),
    ] {
        let update = Query::update()
            .table(table.clone())
            .value(Alias::new("envelope_id"), envelope_id)
            .and_where(Expr::col(Alias::new("id")).eq(id))
            .to_owned();
        fixture
            .connection()
            .execute(fixture.connection().get_database_backend().build(&update))
            .await
            .unwrap();
    }
    let assets = TempDir::new().unwrap();

    let Err(error) =
        initialize(api_key_startup_options(&fixture, &assets).with_credential_cipher(cipher)).await
    else {
        panic!("persisted API keys unexpectedly started with swapped envelope IDs");
    };

    assert_api_key_validation_error(&error, ApiKeyValidationError::EnvelopeUnreadable);
}

#[tokio::test]
async fn api_key_startup_rejects_more_than_256_persisted_keys() {
    let fixture = reconnectable_test_database().await.unwrap();
    tjxy_db::Migrator::up(fixture.connection(), None)
        .await
        .unwrap();
    let cipher = api_key_cipher(1, 71, &[]);
    seed_api_keys(
        fixture.connection(),
        Arc::clone(&cipher),
        &vec!["Automation"; 256],
    )
    .await;
    let table = Alias::new("api_keys");
    let creator = fixture
        .connection()
        .query_one(
            fixture.connection().get_database_backend().build(
                &Query::select()
                    .columns([
                        Alias::new("creator_user_id"),
                        Alias::new("creator_auth_revision"),
                    ])
                    .from(table.clone())
                    .limit(1)
                    .to_owned(),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    let creator_user_id = creator.try_get::<Uuid>("", "creator_user_id").unwrap();
    let creator_auth_revision = creator.try_get::<i64>("", "creator_auth_revision").unwrap();
    let raw_key = "ff".repeat(32);
    let token_digest: [u8; 32] = Sha256::digest(raw_key.as_bytes()).into();
    let envelope_id = Uuid::new_v4();
    let envelope = cipher
        .seal(
            envelope_id,
            "tjxy-api-key/access-token/v1",
            raw_key.as_bytes(),
        )
        .unwrap();
    let insert = Query::insert()
        .into_table(table)
        .columns([
            Alias::new("envelope_id"),
            Alias::new("creator_user_id"),
            Alias::new("creator_auth_revision"),
            Alias::new("token_digest"),
            Alias::new("encrypted_payload"),
            Alias::new("key_version"),
            Alias::new("app_name"),
            Alias::new("created_at"),
        ])
        .values_panic([
            envelope_id.into(),
            creator_user_id.into(),
            creator_auth_revision.into(),
            token_digest.to_vec().into(),
            envelope.payload().to_vec().into(),
            envelope.key_version().into(),
            "Old valid key".into(),
            Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap().into(),
        ])
        .to_owned();
    fixture
        .connection()
        .execute(fixture.connection().get_database_backend().build(&insert))
        .await
        .unwrap();
    let service = AuthService::new(
        fixture.connection().clone(),
        SystemClock,
        Some(chrono::Duration::days(30)),
        2,
    )
    .await
    .unwrap()
    .with_credential_cipher(Arc::clone(&cipher));
    let principal = service.authenticate_token(&raw_key).await.unwrap();
    assert!(principal.user().is_admin());
    drop(service);
    let assets = TempDir::new().unwrap();

    let Err(error) =
        initialize(api_key_startup_options(&fixture, &assets).with_credential_cipher(cipher)).await
    else {
        panic!("startup unexpectedly ignored a persisted API key beyond the capacity limit");
    };

    assert_api_key_validation_error(&error, ApiKeyValidationError::StoredStateInvalid);
    assert!(!format!("{error:?}").contains(&raw_key));
}

#[tokio::test]
async fn initialization_migrates_bootstraps_auth_and_only_then_reports_ready() {
    let identity = ServerIdentity::new(Uuid::new_v4(), "TJXY", "Linux");
    let assets = TempDir::new().unwrap();
    let database = reconnectable_test_database().await.unwrap();
    let state = initialize(
        StartupOptions::new(database.database_url(), identity)
            .with_assets_dir(assets.path())
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
async fn media_browser_roots_load_from_database_and_skip_unavailable_entries() {
    let database = reconnectable_test_database().await.unwrap();
    tjxy_db::Migrator::up(database.connection(), None)
        .await
        .unwrap();
    let database_root = TempDir::new().unwrap();
    let settings = SystemSettingsInput {
        media_browser_roots: vec![database_root.path().to_string_lossy().into_owned()],
        ..SystemSettingsInput::default()
    };
    SystemSettingsRepository::new(database.connection())
        .put(&settings, None)
        .await
        .unwrap();
    let assets = TempDir::new().unwrap();
    let state = initialize(
        StartupOptions::new(
            database.database_url(),
            ServerIdentity::new(Uuid::new_v4(), "TJXY", "Linux"),
        )
        .with_assets_dir(assets.path())
        .with_bootstrap_admin(BootstrapAdmin::new("Admin", "first password")),
    )
    .await
    .unwrap();
    let app = build_router(state);
    let login = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/Users/AuthenticateByName")
                .header(
                    header::AUTHORIZATION,
                    r#"MediaBrowser Client="Test", Device="Test", DeviceId="startup", Version="1""#,
                )
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"Username": "Admin", "Pw": "first password"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let login: Value =
        serde_json::from_slice(&login.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let token = login["AccessToken"].as_str().unwrap();
    let roots = app
        .oneshot(
            Request::builder()
                .uri("/Admin/Filesystem/Roots")
                .header(
                    header::AUTHORIZATION,
                    format!(r#"MediaBrowser Token="{token}""#),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let roots: Value =
        serde_json::from_slice(&roots.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(roots.as_array().unwrap().len(), 1);
    assert_eq!(
        roots[0]["Name"],
        database_root.path().file_name().unwrap().to_str().unwrap()
    );

    let settings = SystemSettingsInput {
        media_browser_roots: vec![format!("/definitely/missing/tjxy-{}", Uuid::new_v4())],
        ..settings
    };
    SystemSettingsRepository::new(database.connection())
        .put(&settings, Some(1))
        .await
        .unwrap();

    initialize(
        StartupOptions::new(
            database.database_url(),
            ServerIdentity::new(Uuid::new_v4(), "TJXY", "Linux"),
        )
        .with_assets_dir(assets.path()),
    )
    .await
    .expect("unavailable persisted roots must not prevent startup");
}

#[tokio::test]
async fn configured_media_refresh_scheduler_enqueues_lowest_priority_library_scan() {
    let assets = TempDir::new().unwrap();
    let database_fixture = reconnectable_test_database().await.unwrap();
    let database = database_fixture.connection().clone();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    let policy =
        LibraryPolicyUpdate::new("Full", "all_synced_objects", "full", "eager", "eager", true)
            .unwrap();
    let library_id = LibraryRepository::new(&database)
        .create("Scheduled Movies", "movies", &policy)
        .await
        .unwrap();

    initialize(
        StartupOptions::new(
            database_fixture.database_url(),
            ServerIdentity::new(Uuid::new_v4(), "TJXY", "Linux"),
        )
        .with_assets_dir(assets.path())
        .with_bootstrap_admin(BootstrapAdmin::new("Admin", "first password"))
        .with_media_refresh_interval(Duration::from_millis(20)),
    )
    .await
    .unwrap();

    let scheduled = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let jobs = WorkJobRepository::new(&database)
                .recent_jobs(20)
                .await
                .unwrap();
            if let Some(job) = jobs.into_iter().find(|record| {
                record.job().task_kind() == WorkTaskKind::FullMediaScan
                    && record.job().scope() == WorkScope::Library(library_id)
            }) {
                break job;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();

    assert_eq!(scheduled.job().priority(), 0);
}

#[tokio::test]
async fn a_new_database_cannot_report_ready_without_an_initial_administrator() {
    let identity = ServerIdentity::new(Uuid::new_v4(), "TJXY", "Linux");
    let database = reconnectable_test_database().await.unwrap();
    let Err(error) = initialize(StartupOptions::new(database.database_url(), identity)).await
    else {
        panic!("new database unexpectedly initialized without an administrator");
    };
    assert!(error.to_string().contains("bootstrap administrator"));
}

#[tokio::test]
async fn persisted_filesystem_root_is_loaded_and_invalid_root_is_reported_offline() {
    let database = reconnectable_test_database().await.unwrap();
    tjxy_db::Migrator::up(database.connection(), None)
        .await
        .unwrap();
    let policy = LibraryPolicyUpdate::new(
        "Lazy",
        "title_layer",
        "basic",
        "on_browse",
        "on_playback",
        true,
    )
    .unwrap();
    let missing = format!("/definitely/missing/tjxy-{}", Uuid::new_v4());
    let root = FilesystemRootDraft::new(missing, "persisted-root-id/persisted-root-id", "Missing")
        .unwrap();
    LibraryRepository::new(database.connection())
        .create_with_filesystem_root("Movies", "movies", &policy, &root)
        .await
        .unwrap();
    let assets = TempDir::new().unwrap();

    let state = initialize(
        StartupOptions::new(
            database.database_url(),
            ServerIdentity::new(Uuid::new_v4(), "TJXY", "Linux"),
        )
        .with_assets_dir(assets.path())
        .with_bootstrap_admin(BootstrapAdmin::new("Admin", "first password")),
    )
    .await
    .expect("persisted invalid filesystem root must not block readiness");
    let app = build_router(state);
    let login = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/Users/AuthenticateByName")
                .header(
                    header::AUTHORIZATION,
                    r#"MediaBrowser Client="Test", Device="Browser", DeviceId="offline-root", Version="1""#,
                )
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"Username": "Admin", "Pw": "first password"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let login: Value =
        serde_json::from_slice(&login.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let token = login["AccessToken"].as_str().unwrap();
    let folders = app
        .oneshot(
            Request::builder()
                .uri("/Library/VirtualFolders")
                .header(
                    header::AUTHORIZATION,
                    format!(r#"MediaBrowser Token="{token}""#),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(folders.status(), StatusCode::OK);
    let folders: Value =
        serde_json::from_slice(&folders.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(folders[0]["UnavailableLocations"], folders[0]["Locations"]);
}

#[tokio::test]
async fn required_redis_failure_prevents_ready_state() {
    let identity = ServerIdentity::new(Uuid::new_v4(), "TJXY", "Linux");
    let database = reconnectable_test_database().await.unwrap();
    let redis = RedisCacheConfig::new(
        RedisMode::Enabled,
        "redis://127.0.0.1:1",
        "tjxy",
        Duration::from_millis(20),
    )
    .unwrap();
    let Err(error) = initialize(
        StartupOptions::new(database.database_url(), identity)
            .with_bootstrap_admin(BootstrapAdmin::new("Admin", "first password"))
            .with_redis_cache(redis),
    )
    .await
    else {
        panic!("required Redis unexpectedly allowed startup");
    };

    assert!(error.to_string().contains("cache initialization failed"));
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Seeds a pre-start durable backlog without configuring a backend.
async fn initialization_reconciles_preexisting_storage_outbox_without_a_backend() {
    let directory = TempDir::new().unwrap();
    let database_fixture = reconnectable_test_database().await.unwrap();
    let database_url = database_fixture.database_url();
    let database = database_fixture.connection().clone();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    let account_id = Uuid::new_v4();
    let root_id = StorageRootId::new();
    let object_id = StorageObjectRecordId::new();
    let backend = database.get_database_backend();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("storage_accounts"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("provider"),
                        Alias::new("display_name"),
                        Alias::new("account_identity"),
                        Alias::new("credential_ref"),
                        Alias::new("status"),
                    ])
                    .values_panic([
                        account_id.into(),
                        "filesystem".into(),
                        "Disk".into(),
                        Uuid::new_v4().to_string().into(),
                        "fixture-ref".into(),
                        "Active".into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("storage_roots"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("storage_account_id"),
                        Alias::new("provider_root_id"),
                        Alias::new("sync_revision"),
                        Alias::new("reconciled_sync_revision"),
                    ])
                    .values_panic([
                        root_id.as_uuid().into(),
                        account_id.into(),
                        "root".into(),
                        1_i64.into(),
                        0_i64.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("storage_objects"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("storage_account_id"),
                        Alias::new("provider_drive_id"),
                        Alias::new("provider_object_id"),
                        Alias::new("name"),
                        Alias::new("normalized_name"),
                        Alias::new("object_type"),
                        Alias::new("observed_sync_revision"),
                        Alias::new("children_indexed"),
                        Alias::new("children_index_revision"),
                        Alias::new("identity_quality"),
                        Alias::new("presence_state"),
                    ])
                    .values_panic([
                        object_id.as_uuid().into(),
                        account_id.into(),
                        "local".into(),
                        "root".into(),
                        "Root".into(),
                        "root".into(),
                        "Directory".into(),
                        1_i64.into(),
                        true.into(),
                        1_i64.into(),
                        "ProviderStableId".into(),
                        "Present".into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("storage_change_outbox"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("storage_root_id"),
                        Alias::new("sync_revision"),
                        Alias::new("event_type"),
                        Alias::new("storage_object_id"),
                        Alias::new("payload_version"),
                        Alias::new("payload"),
                        Alias::new("dedupe_key"),
                        Alias::new("state"),
                        Alias::new("attempt_count"),
                    ])
                    .values_panic([
                        Uuid::new_v4().into(),
                        root_id.as_uuid().into(),
                        1_i64.into(),
                        "InventoryPageCommitted".into(),
                        object_id.as_uuid().into(),
                        1_i32.into(),
                        json!({"version": 1, "kind": "InventoryPageCommitted"}).into(),
                        format!("{root_id}:marker").into(),
                        "Pending".into(),
                        0_i32.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    let transaction = database.begin().await.unwrap();
    tjxy_db::advance_catalog_generation(&transaction)
        .await
        .unwrap();
    transaction.commit().await.unwrap();

    initialize(
        StartupOptions::new(
            database_url,
            ServerIdentity::new(Uuid::new_v4(), "TJXY", "Linux"),
        )
        .with_assets_dir(directory.path().join("assets"))
        .with_bootstrap_admin(BootstrapAdmin::new("Admin", "first password")),
    )
    .await
    .unwrap();

    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            let backend = database.get_database_backend();
            let revision = database
                .query_one(
                    backend.build(
                        Query::select()
                            .column(Alias::new("reconciled_sync_revision"))
                            .from(Alias::new("storage_roots"))
                            .and_where(Expr::col(Alias::new("id")).eq(root_id.as_uuid())),
                    ),
                )
                .await
                .unwrap()
                .unwrap()
                .try_get::<i64>("", "reconciled_sync_revision")
                .unwrap();
            if revision == 1 {
                let invalidation = database
                    .query_one(
                        backend.build(
                            Query::select()
                                .column(Alias::new("state"))
                                .from(Alias::new("cache_invalidation_outbox")),
                        ),
                    )
                    .await
                    .unwrap()
                    .unwrap()
                    .try_get::<String>("", "state")
                    .unwrap();
                if invalidation == "Processed" {
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Mirrors the persisted filesystem scope consumed at startup.
async fn initialization_starts_the_provider_drive_scoped_inventory_worker() {
    let directory = TempDir::new().unwrap();
    let storage_root = directory.path().join("media");
    std::fs::create_dir(&storage_root).unwrap();
    std::fs::write(storage_root.join("Arrival.mkv"), b"fixture").unwrap();
    let filesystem = FilesystemBackend::new(&storage_root).await.unwrap();
    let account_id = Uuid::new_v4();
    let root_id = StorageRootId::new();
    let parent_id = StorageObjectRecordId::new();
    let library_id = Uuid::new_v4();
    let item_id = CatalogItemId::new();
    let database_fixture = reconnectable_test_database().await.unwrap();
    let database_url = database_fixture.database_url();
    let database = database_fixture.connection().clone();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    let backend = database.get_database_backend();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("storage_accounts"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("provider"),
                        Alias::new("display_name"),
                        Alias::new("account_identity"),
                        Alias::new("credential_ref"),
                        Alias::new("status"),
                    ])
                    .values_panic([
                        account_id.into(),
                        "filesystem".into(),
                        "Local".into(),
                        Uuid::new_v4().to_string().into(),
                        "local".into(),
                        "Active".into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("libraries"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("name"),
                        Alias::new("scan_profile"),
                        Alias::new("object_selection_scope"),
                        Alias::new("metadata_policy"),
                        Alias::new("expansion_policy"),
                        Alias::new("probe_policy"),
                        Alias::new("profile_version"),
                        Alias::new("collection_type"),
                        Alias::new("sort_key"),
                        Alias::new("is_enabled"),
                    ])
                    .values_panic([
                        library_id.into(),
                        "Movies".into(),
                        "Lazy".into(),
                        "title_layer".into(),
                        "basic".into(),
                        "on_browse".into(),
                        "on_playback".into(),
                        1.into(),
                        "movies".into(),
                        SortKey::from_text("Movies").into_bytes().into(),
                        true.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("catalog_items"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("item_type"),
                        Alias::new("name"),
                        Alias::new("sort_name"),
                        Alias::new("sort_key"),
                        Alias::new("classification_state"),
                        Alias::new("metadata_state"),
                        Alias::new("structure_state"),
                        Alias::new("source_state"),
                        Alias::new("structure_expansion_revision"),
                        Alias::new("source_index_revision"),
                        Alias::new("is_present"),
                    ])
                    .values_panic([
                        item_id.as_uuid().into(),
                        "Movie".into(),
                        "Arrival".into(),
                        "arrival".into(),
                        SortKey::from_text("Arrival").into_bytes().into(),
                        "Matched".into(),
                        "Ready".into(),
                        "NotApplicable".into(),
                        "NotIndexed".into(),
                        0_i64.into(),
                        0_i64.into(),
                        true.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("storage_roots"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("storage_account_id"),
                        Alias::new("provider_root_id"),
                        Alias::new("sync_revision"),
                        Alias::new("reconciled_sync_revision"),
                    ])
                    .values_panic([
                        root_id.as_uuid().into(),
                        account_id.into(),
                        filesystem.root_id().provider_object_id().into(),
                        0_i64.into(),
                        0_i64.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("library_storage_roots"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("library_id"),
                        Alias::new("storage_root_id"),
                    ])
                    .values_panic([
                        Uuid::new_v4().into(),
                        library_id.into(),
                        root_id.as_uuid().into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("library_catalog_items"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("library_id"),
                        Alias::new("catalog_item_id"),
                    ])
                    .values_panic([
                        Uuid::new_v4().into(),
                        library_id.into(),
                        item_id.as_uuid().into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("storage_objects"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("storage_account_id"),
                        Alias::new("provider_drive_id"),
                        Alias::new("provider_object_id"),
                        Alias::new("name"),
                        Alias::new("normalized_name"),
                        Alias::new("object_type"),
                        Alias::new("observed_sync_revision"),
                        Alias::new("children_indexed"),
                        Alias::new("children_index_revision"),
                        Alias::new("identity_quality"),
                        Alias::new("presence_state"),
                    ])
                    .values_panic([
                        parent_id.as_uuid().into(),
                        account_id.into(),
                        "local".into(),
                        filesystem.root_id().provider_object_id().into(),
                        "Media".into(),
                        "media".into(),
                        "Directory".into(),
                        0_i64.into(),
                        false.into(),
                        0_i64.into(),
                        "StableFileId".into(),
                        "Present".into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("identity_matches"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("storage_object_id"),
                        Alias::new("candidate_catalog_item_id"),
                        Alias::new("confidence"),
                        Alias::new("state"),
                        Alias::new("evidence"),
                    ])
                    .values_panic([
                        Uuid::new_v4().into(),
                        parent_id.as_uuid().into(),
                        item_id.as_uuid().into(),
                        1.0.into(),
                        "Matched".into(),
                        serde_json::json!({}).into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("storage_root_objects"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("storage_root_id"),
                        Alias::new("storage_object_id"),
                        Alias::new("observed_sync_revision"),
                        Alias::new("children_indexed"),
                        Alias::new("children_index_revision"),
                        Alias::new("presence_state"),
                    ])
                    .values_panic([
                        Uuid::new_v4().into(),
                        root_id.as_uuid().into(),
                        parent_id.as_uuid().into(),
                        0_i64.into(),
                        false.into(),
                        0_i64.into(),
                        "Present".into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    let submission = WorkJobRepository::new(&database)
        .enqueue_or_join(
            &WorkJobSpec::new(
                WorkTaskKind::ScopedStorageSync,
                WorkScope::StorageObject(parent_id),
                0,
                100,
            )
            .unwrap(),
        )
        .await
        .unwrap();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("storage_sync_cursors"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("storage_root_id"),
                        Alias::new("cursor_type"),
                        Alias::new("cursor_value"),
                        Alias::new("status"),
                    ])
                    .values_panic([
                        Uuid::new_v4().into(),
                        root_id.as_uuid().into(),
                        "Changes".into(),
                        "change-initial".into(),
                        "Active".into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    let provider = Arc::new(ChangeAwareFilesystem {
        filesystem,
        change_calls: AtomicUsize::new(0),
    });

    let _state = initialize(
        StartupOptions::new(
            database_url,
            ServerIdentity::new(Uuid::new_v4(), "TJXY", "Linux"),
        )
        .with_assets_dir(directory.path().join("assets"))
        .with_bootstrap_admin(BootstrapAdmin::new("Admin", "first password"))
        .with_storage_backend(account_id, "local", provider.clone()),
    )
    .await
    .unwrap();

    tokio::time::timeout(Duration::from_secs(3), async {
        while provider.change_calls.load(Ordering::SeqCst) == 0 {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();

    let completion = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if WorkJobRepository::new(&database)
                .get(submission.job().id())
                .await
                .unwrap()
                .unwrap()
                .state()
                == WorkJobState::Completed
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    if completion.is_err() {
        let job = database
            .query_one(
                backend.build(
                    Query::select()
                        .columns([
                            Alias::new("state"),
                            Alias::new("attempt_count"),
                            Alias::new("last_error"),
                            Alias::new("available_at"),
                            Alias::new("lease_owner"),
                        ])
                        .from(Alias::new("work_jobs"))
                        .and_where(
                            sea_orm::sea_query::Expr::col(Alias::new("id"))
                                .eq(submission.job().id().as_uuid()),
                        ),
                ),
            )
            .await
            .unwrap()
            .unwrap();
        panic!(
            "inventory job stalled: state={}, attempts={}, error={:?}, available_at={:?}, lease={:?}",
            job.try_get::<String>("", "state").unwrap(),
            job.try_get::<i32>("", "attempt_count").unwrap(),
            job.try_get::<Option<String>>("", "last_error").unwrap(),
            job.try_get::<Option<String>>("", "available_at").unwrap(),
            job.try_get::<Option<String>>("", "lease_owner").unwrap(),
        );
    }
    let count = database
        .query_one(
            backend.build(
                Query::select()
                    .expr_as(Expr::col(Alias::new("id")).count(), Alias::new("count"))
                    .from(Alias::new("storage_objects"))
                    .and_where(Expr::col(Alias::new("name")).eq("Arrival.mkv")),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(count.try_get::<i64>("", "count").unwrap(), 1);
    let inventory_revision = WorkJobRepository::new(&database)
        .completed_sync_revision(submission.job().id())
        .await
        .unwrap()
        .unwrap();
    let indexed = WorkJobRepository::new(&database)
        .enqueue_or_join(
            &WorkJobSpec::new(
                WorkTaskKind::IndexMediaSources,
                WorkScope::CatalogItem(item_id),
                1,
                100,
            )
            .unwrap()
            .with_required_sync(submission.job().id(), inventory_revision),
        )
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if WorkJobRepository::new(&database)
                .get(indexed.job().id())
                .await
                .unwrap()
                .unwrap()
                .state()
                == WorkJobState::Completed
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    let sources = CatalogPublicationRepository::new(&database)
        .active_sources(item_id)
        .await
        .unwrap();
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].container(), Some("mkv"));
}
use std::time::Duration;
