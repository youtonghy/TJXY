use std::sync::{Arc, Mutex};

use chrono::{Duration, TimeZone, Utc};
use sea_orm::{
    ConnectionTrait,
    sea_query::{Alias, Expr, JoinType, Query},
};
use sea_orm_migration::MigratorTrait;
use tjxy_application::{
    AuthClock, AuthError, AuthService, AuthenticatedPrincipal, ClientIdentity, SessionCapabilities,
    SessionListFilter,
};
use tjxy_credentials::{CredentialCipher, CredentialKey};
use tjxy_test_support::test_database;
use uuid::Uuid;

#[derive(Clone)]
struct ManualClock(Arc<Mutex<chrono::DateTime<Utc>>>);

impl ManualClock {
    fn new(now: chrono::DateTime<Utc>) -> Self {
        Self(Arc::new(Mutex::new(now)))
    }

    fn set(&self, now: chrono::DateTime<Utc>) {
        *self.0.lock().unwrap() = now;
    }
}

impl AuthClock for ManualClock {
    fn now(&self) -> chrono::DateTime<Utc> {
        *self.0.lock().unwrap()
    }
}

async fn service() -> (
    AuthService<ManualClock>,
    ManualClock,
    sea_orm::DatabaseConnection,
) {
    let database = database().await;
    let now = Utc.with_ymd_and_hms(2026, 7, 17, 13, 0, 0).unwrap();
    let clock = ManualClock::new(now);
    let service = AuthService::new(database.clone(), clock.clone(), Some(Duration::days(30)), 2)
        .await
        .unwrap();
    (service, clock, database)
}

async fn database() -> sea_orm::DatabaseConnection {
    let database = test_database().await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    database
}

fn client() -> ClientIdentity {
    ClientIdentity::new("Findroid", "Phone", "device-1", "0.15.3").unwrap()
}

fn credential_cipher() -> Arc<CredentialCipher> {
    Arc::new(CredentialCipher::new(CredentialKey::new(1, [9; 32]).unwrap(), Vec::new()).unwrap())
}

async fn principal(
    service: &AuthService<ManualClock>,
    username: &str,
    password: &str,
    device_id: &str,
) -> AuthenticatedPrincipal {
    let issued = service
        .authenticate(
            username,
            password,
            ClientIdentity::new("Test", "Browser", device_id, "1.0").unwrap(),
        )
        .await
        .unwrap();
    service
        .authenticate_token(issued.access_token().expose_secret())
        .await
        .unwrap()
}

#[tokio::test]
async fn password_auth_issues_redacted_token_and_token_auth_resolves_user() {
    let (service, _clock, database) = service().await;
    let created = service
        .create_user("Ａlice", "correct horse battery staple", true)
        .await
        .unwrap();

    let issued = service
        .authenticate("alice", "correct horse battery staple", client())
        .await
        .unwrap();

    assert_eq!(issued.user().id(), created.id());
    assert_eq!(
        format!("{:?}", issued.access_token()),
        "SecretSessionToken([REDACTED])"
    );
    assert_eq!(issued.access_token().expose_secret().len(), 64);
    let principal = service
        .authenticate_token(issued.access_token().expose_secret())
        .await
        .unwrap();
    assert_eq!(principal.user().name(), "Ａlice");

    let users = Alias::new("users");
    let sessions = Alias::new("auth_sessions");
    let statement = Query::select()
        .expr_as(
            Expr::col((users.clone(), Alias::new("password_hash"))),
            Alias::new("password_hash"),
        )
        .expr_as(
            Expr::col((sessions.clone(), Alias::new("token_digest"))),
            Alias::new("token_digest"),
        )
        .from(users.clone())
        .join(
            JoinType::InnerJoin,
            sessions.clone(),
            Expr::col((users, Alias::new("id"))).equals((sessions, Alias::new("user_id"))),
        )
        .limit(1)
        .to_owned();
    let backend = database.get_database_backend();
    let row = database
        .query_one(backend.build(&statement))
        .await
        .unwrap()
        .unwrap();
    let password_hash = row.try_get::<String>("", "password_hash").unwrap();
    let token_digest = row.try_get::<Vec<u8>>("", "token_digest").unwrap();
    assert!(password_hash.starts_with("$argon2id$v=19$"));
    assert_eq!(token_digest.len(), 32);
    assert_ne!(
        token_digest,
        issued.access_token().expose_secret().as_bytes()
    );
}

#[tokio::test]
async fn unknown_user_and_wrong_password_share_the_same_external_error() {
    let (service, _clock, _) = service().await;
    service.create_user("alice", "right", false).await.unwrap();

    let wrong = service
        .authenticate("alice", "wrong", client())
        .await
        .unwrap_err();
    let unknown = service
        .authenticate("missing", "wrong", client())
        .await
        .unwrap_err();

    assert_eq!(wrong, AuthError::InvalidCredentials);
    assert_eq!(unknown, AuthError::InvalidCredentials);
}

#[tokio::test]
async fn empty_password_is_valid_and_session_expires_at_the_exact_boundary() {
    let (service, clock, _) = service().await;
    service.create_user("alice", "", false).await.unwrap();
    let issued = service.authenticate("alice", "", client()).await.unwrap();
    assert!(!issued.user().has_password());

    clock.set(issued.expires_at().unwrap());

    let error = service
        .authenticate_token(issued.access_token().expose_secret())
        .await
        .unwrap_err();
    assert_eq!(error, AuthError::InvalidToken);
}

#[tokio::test]
async fn persistent_session_remains_valid_without_an_expiry() {
    let database = database().await;
    let initial = Utc.with_ymd_and_hms(2026, 7, 17, 12, 0, 0).unwrap();
    let clock = ManualClock::new(initial);
    let service = AuthService::new(database, clock.clone(), None, 2)
        .await
        .unwrap();
    service
        .create_user("alice", "password", false)
        .await
        .unwrap();
    let issued = service
        .authenticate("alice", "password", client())
        .await
        .unwrap();
    assert_eq!(issued.expires_at(), None);

    clock.set(initial + Duration::days(3650));
    assert!(
        service
            .authenticate_token(issued.access_token().expose_secret())
            .await
            .is_ok()
    );
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Keeps session visibility, activity, and logout in one lifecycle.
async fn session_listing_is_scoped_recent_and_logout_revokes_the_current_token() {
    let (service, clock, _) = service().await;
    let alice = service
        .create_user("alice", "admin password", true)
        .await
        .unwrap();
    let bob = service
        .create_user("bob", "user password", false)
        .await
        .unwrap();
    let alice_authentication = service
        .authenticate(
            "alice",
            "admin password",
            ClientIdentity::new("Jellyfin Web", "Browser", "browser-1", "10.10.0").unwrap(),
        )
        .await
        .unwrap();
    let bob_authentication = service
        .authenticate(
            "bob",
            "user password",
            ClientIdentity::new("Findroid", "Pixel", "phone-1", "0.16.0").unwrap(),
        )
        .await
        .unwrap();
    let bob_token = bob_authentication.access_token().expose_secret().to_owned();
    let bob_principal = service.authenticate_token(&bob_token).await.unwrap();
    assert_eq!(
        bob_principal.session_id(),
        Some(bob_authentication.session_id())
    );
    assert_eq!(bob_principal.device_id(), Some("phone-1"));
    assert_eq!(bob_principal.api_key_id(), None);
    let device_profile = serde_json::json!({
        "DirectPlayProfiles": [{ "Container": "mp4" }]
    });
    assert!(
        service
            .update_session_capabilities(
                &bob_principal,
                None,
                SessionCapabilities {
                    playable_media_types: vec!["Video".to_owned(), "Audio".to_owned()],
                    supported_commands: vec!["Play".to_owned()],
                    supports_media_control: true,
                    supports_persistent_identifier: true,
                    device_profile: Some(device_profile.clone()),
                    ..SessionCapabilities::default()
                },
            )
            .await
            .unwrap()
    );
    assert_eq!(
        service
            .session_device_profile(&bob_principal)
            .await
            .unwrap(),
        Some(device_profile)
    );
    let alice_principal = service
        .authenticate_token(alice_authentication.access_token().expose_secret())
        .await
        .unwrap();

    let administrator_sessions = service
        .sessions(&alice_principal, SessionListFilter::default())
        .await
        .unwrap();
    assert_eq!(administrator_sessions.len(), 2);
    assert!(
        administrator_sessions
            .iter()
            .any(|session| session.user_id() == alice.id())
    );
    let bob_session = administrator_sessions
        .iter()
        .find(|session| session.user_id() == bob.id())
        .unwrap();
    assert_eq!(bob_session.device_id(), "phone-1");
    assert_eq!(bob_session.playable_media_types(), ["Video", "Audio"]);
    assert!(bob_session.supports_media_control());

    let bob_sessions = service
        .sessions(&bob_principal, SessionListFilter::default())
        .await
        .unwrap();
    assert_eq!(bob_sessions.len(), 1);
    assert_eq!(bob_sessions[0].user_id(), bob.id());

    clock.set(clock.now() + Duration::minutes(10));
    let bob_principal = service.authenticate_token(&bob_token).await.unwrap();
    let recent = service
        .sessions(
            &alice_principal,
            SessionListFilter::default().with_active_within_seconds(60),
        )
        .await
        .unwrap();
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].user_id(), bob.id());
    let active_now = service
        .sessions(
            &alice_principal,
            SessionListFilter::default().with_active_within_seconds(0),
        )
        .await
        .unwrap();
    assert_eq!(active_now.len(), 1);
    assert_eq!(active_now[0].user_id(), bob.id());

    service.logout(&bob_principal).await.unwrap();
    assert_eq!(
        service.authenticate_token(&bob_token).await.unwrap_err(),
        AuthError::InvalidToken
    );
    let remaining = service
        .sessions(&alice_principal, SessionListFilter::default())
        .await
        .unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].user_id(), alice.id());
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Keeps authorization, options, and atomic revoke in one lifecycle.
async fn device_management_is_admin_only_and_delete_is_atomic() {
    let (service, _clock, _) = service().await;
    let admin = service
        .create_user("admin", "admin password", true)
        .await
        .unwrap();
    let user = service
        .create_user("alice", "user password", false)
        .await
        .unwrap();
    let admin_authentication = service
        .authenticate(
            "admin",
            "admin password",
            ClientIdentity::new("Jellyfin Web", "Browser", "browser-1", "10.10.0").unwrap(),
        )
        .await
        .unwrap();
    let user_authentication = service
        .authenticate(
            "alice",
            "user password",
            ClientIdentity::new("Findroid", "Pixel", "phone-1", "0.16.0").unwrap(),
        )
        .await
        .unwrap();
    let admin_principal = service
        .authenticate_token(admin_authentication.access_token().expose_secret())
        .await
        .unwrap();
    let user_token = user_authentication
        .access_token()
        .expose_secret()
        .to_owned();
    let user_principal = service.authenticate_token(&user_token).await.unwrap();

    assert_eq!(
        service.devices(&user_principal, None).await.unwrap_err(),
        AuthError::Forbidden
    );
    assert_eq!(
        service
            .device(&user_principal, "phone-1")
            .await
            .unwrap_err(),
        AuthError::Forbidden
    );

    let devices = service.devices(&admin_principal, None).await.unwrap();
    assert_eq!(devices.len(), 2);
    let user_devices = service
        .devices(&admin_principal, Some(user.id()))
        .await
        .unwrap();
    assert_eq!(user_devices.len(), 2);
    assert_eq!(
        service
            .devices(
                &admin_principal,
                Some(tjxy_common::UserId::from_uuid(uuid::Uuid::new_v4()))
            )
            .await
            .unwrap_err(),
        AuthError::Repository(tjxy_db::AuthRepositoryError::UserNotFound)
    );
    assert_eq!(
        service
            .device(&admin_principal, "phone-1")
            .await
            .unwrap()
            .unwrap()
            .user_id(),
        user.id()
    );
    assert!(
        service
            .device_options(&admin_principal, "phone-1")
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        service
            .update_device_options(&admin_principal, "phone-1", Some("Alice's phone"))
            .await
            .unwrap()
    );
    assert_eq!(
        service
            .device_options(&admin_principal, "phone-1")
            .await
            .unwrap()
            .unwrap()
            .custom_name(),
        Some("Alice's phone")
    );

    assert!(
        !service
            .delete_devices(
                &admin_principal,
                &["phone-1".to_owned(), "missing".to_owned()]
            )
            .await
            .unwrap()
    );
    assert!(service.authenticate_token(&user_token).await.is_ok());

    assert!(
        service
            .delete_devices(&admin_principal, &["phone-1".to_owned()])
            .await
            .unwrap()
    );
    assert_eq!(
        service.authenticate_token(&user_token).await.unwrap_err(),
        AuthError::InvalidToken
    );
    assert!(
        service
            .device_options(&admin_principal, "phone-1")
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(admin_principal.user().id(), admin.id());
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // One end-to-end contract keeps the recovered secret in scope once.
async fn api_key_lifecycle_encrypts_the_secret_and_delete_is_idempotent() {
    let (service, clock, database) = service().await;
    let service = service.with_credential_cipher(credential_cipher());
    let admin = service
        .create_user("Admin", "admin password", true)
        .await
        .unwrap();
    let admin_principal = principal(&service, "Admin", "admin password", "admin-browser").await;

    service
        .create_api_key(&admin_principal, "Kodi Sync")
        .await
        .unwrap();
    let keys = service.list_api_keys(&admin_principal).await.unwrap();
    assert_eq!(keys.len(), 1);
    let key = &keys[0];
    assert_eq!(key.app_name(), "Kodi Sync");
    assert_eq!(key.creator_user_id(), admin.id());
    assert_eq!(key.creator_user_name(), "Admin");
    assert_eq!(key.created_at(), clock.now());
    assert_eq!(key.last_used_at(), None);
    assert_eq!(key.access_token().expose_secret().len(), 64);
    assert!(
        key.access_token()
            .expose_secret()
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    );
    assert_eq!(
        format!("{:?}", key.access_token()),
        "SecretApiKey([REDACTED])"
    );

    let key_principal = service
        .authenticate_token(key.access_token().expose_secret())
        .await
        .unwrap();
    assert_eq!(key_principal.api_key_id(), Some(key.id()));
    assert_eq!(key_principal.session_id(), None);
    assert_eq!(key_principal.device_id(), None);
    assert_eq!(
        service
            .session_device_profile(&key_principal)
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        service
            .update_session_capabilities(&key_principal, None, SessionCapabilities::default(),)
            .await
            .unwrap_err(),
        AuthError::SessionRequired
    );
    assert_eq!(
        service.logout(&key_principal).await.unwrap_err(),
        AuthError::SessionRequired
    );

    let listed_after_use = service.list_api_keys(&admin_principal).await.unwrap();
    assert_eq!(listed_after_use[0].last_used_at(), Some(clock.now()));

    let table = Alias::new("api_keys");
    let row = database
        .query_one(
            database.get_database_backend().build(
                &Query::select()
                    .columns([
                        Alias::new("id"),
                        Alias::new("envelope_id"),
                        Alias::new("creator_user_id"),
                        Alias::new("creator_auth_revision"),
                        Alias::new("token_digest"),
                        Alias::new("encrypted_payload"),
                        Alias::new("key_version"),
                        Alias::new("app_name"),
                        Alias::new("created_at"),
                        Alias::new("last_used_at"),
                    ])
                    .from(table)
                    .limit(1)
                    .to_owned(),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    let persisted_columns = [
        row.try_get::<i64>("", "id")
            .unwrap()
            .to_string()
            .into_bytes(),
        row.try_get::<Uuid>("", "envelope_id")
            .unwrap()
            .to_string()
            .into_bytes(),
        row.try_get::<Uuid>("", "creator_user_id")
            .unwrap()
            .to_string()
            .into_bytes(),
        row.try_get::<i64>("", "creator_auth_revision")
            .unwrap()
            .to_string()
            .into_bytes(),
        row.try_get::<Vec<u8>>("", "token_digest").unwrap(),
        row.try_get::<Vec<u8>>("", "encrypted_payload").unwrap(),
        row.try_get::<i32>("", "key_version")
            .unwrap()
            .to_string()
            .into_bytes(),
        row.try_get::<String>("", "app_name").unwrap().into_bytes(),
        row.try_get::<chrono::DateTime<Utc>>("", "created_at")
            .unwrap()
            .to_rfc3339()
            .into_bytes(),
        row.try_get::<Option<chrono::DateTime<Utc>>>("", "last_used_at")
            .unwrap()
            .unwrap()
            .to_rfc3339()
            .into_bytes(),
    ];
    for raw_window in key.access_token().expose_secret().as_bytes().windows(8) {
        assert!(persisted_columns.iter().all(|column| {
            !column
                .windows(raw_window.len())
                .any(|window| window == raw_window)
        }));
    }

    service
        .delete_api_key(&key_principal, key.access_token().expose_secret())
        .await
        .unwrap();
    service
        .delete_api_key(&admin_principal, key.access_token().expose_secret())
        .await
        .unwrap();
    assert_eq!(
        service
            .authenticate_token(key.access_token().expose_secret())
            .await
            .unwrap_err(),
        AuthError::InvalidToken
    );
    assert!(
        service
            .list_api_keys(&admin_principal)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn api_key_lifecycle_requires_an_admin_and_cipher_and_validates_inputs() {
    let (service, clock, database) = service().await;
    service
        .create_user("Admin", "admin password", true)
        .await
        .unwrap();
    service
        .create_user("Viewer", "viewer password", false)
        .await
        .unwrap();
    let admin = principal(&service, "Admin", "admin password", "admin-browser").await;
    let viewer = principal(&service, "Viewer", "viewer password", "viewer-browser").await;

    assert_eq!(
        service
            .create_api_key(&admin, "Missing cipher")
            .await
            .unwrap_err(),
        AuthError::CredentialCipherUnavailable
    );
    assert_eq!(
        service.list_api_keys(&admin).await.unwrap_err(),
        AuthError::CredentialCipherUnavailable
    );
    assert_eq!(
        service
            .create_api_key(&viewer, "Rejected")
            .await
            .unwrap_err(),
        AuthError::Forbidden
    );
    assert_eq!(
        service.list_api_keys(&viewer).await.unwrap_err(),
        AuthError::Forbidden
    );
    assert_eq!(
        service
            .delete_api_key(&viewer, &"a".repeat(64))
            .await
            .unwrap_err(),
        AuthError::Forbidden
    );
    assert!(service.validate_api_key_envelopes().await.is_ok());

    let service = AuthService::new(database.clone(), clock.clone(), Some(Duration::days(30)), 2)
        .await
        .unwrap()
        .with_credential_cipher(credential_cipher());
    for invalid in ["", "   ", "bad\nname"] {
        assert_eq!(
            service.create_api_key(&admin, invalid).await.unwrap_err(),
            AuthError::InvalidApiKeyRequest
        );
    }
    assert_eq!(
        service
            .create_api_key(&admin, &"x".repeat(257))
            .await
            .unwrap_err(),
        AuthError::InvalidApiKeyRequest
    );
    assert_eq!(
        service.delete_api_key(&admin, "short").await.unwrap_err(),
        AuthError::InvalidApiKeyRequest
    );
    service.create_api_key(&admin, "Recoverable").await.unwrap();
    let keys = service.list_api_keys(&admin).await.unwrap();
    let service_without_cipher = AuthService::new(database, clock, Some(Duration::days(30)), 2)
        .await
        .unwrap();
    assert_eq!(
        service_without_cipher
            .validate_api_key_envelopes()
            .await
            .unwrap_err(),
        AuthError::CredentialCipherUnavailable
    );
    service_without_cipher
        .delete_api_key(&admin, keys[0].access_token().expose_secret())
        .await
        .unwrap();
    assert!(
        service_without_cipher
            .validate_api_key_envelopes()
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn api_key_capacity_is_mapped_at_the_application_boundary() {
    let (service, _clock, _) = service().await;
    let service = service.with_credential_cipher(credential_cipher());
    service
        .create_user("Admin", "admin password", true)
        .await
        .unwrap();
    let admin = principal(&service, "Admin", "admin password", "admin-browser").await;

    for index in 0..256 {
        service
            .create_api_key(&admin, &format!("Automation {index}"))
            .await
            .unwrap();
    }
    let keys = service.list_api_keys(&admin).await.unwrap();
    assert_eq!(keys.len(), 256);
    assert!(
        keys.iter().any(|key| {
            let token = key.access_token().expose_secret().as_bytes();
            token[12] != b'4'
                || !matches!(token[16], b'8' | b'9' | b'a' | b'b')
                || token[44] != b'4'
                || !matches!(token[48], b'8' | b'9' | b'a' | b'b')
        }),
        "256-bit tokens must not retain UUIDv4 version and variant markers"
    );
    assert_eq!(
        service
            .create_api_key(&admin, "One too many")
            .await
            .unwrap_err(),
        AuthError::ApiKeyCapacity
    );
}

#[tokio::test]
async fn creator_name_password_and_policy_changes_invalidate_api_keys() {
    let (service, _clock, _) = service().await;
    let service = service.with_credential_cipher(credential_cipher());
    let target = service
        .create_user("Target Admin", "original password", true)
        .await
        .unwrap();
    service
        .create_user("Other Admin", "other password", true)
        .await
        .unwrap();
    let mut target_principal =
        principal(&service, "Target Admin", "original password", "target-1").await;
    let other_principal =
        principal(&service, "Other Admin", "other password", "other-browser").await;

    service
        .create_api_key(&target_principal, "Rename")
        .await
        .unwrap();
    let renamed_keys = service.list_api_keys(&other_principal).await.unwrap();
    service
        .rename_user(target.id(), "Renamed Admin")
        .await
        .unwrap();
    assert_eq!(
        service
            .authenticate_token(renamed_keys[0].access_token().expose_secret())
            .await
            .unwrap_err(),
        AuthError::InvalidToken
    );
    assert!(
        service
            .list_api_keys(&other_principal)
            .await
            .unwrap()
            .is_empty()
    );

    target_principal = principal(&service, "Renamed Admin", "original password", "target-2").await;
    service
        .create_api_key(&target_principal, "Password")
        .await
        .unwrap();
    let password_keys = service.list_api_keys(&other_principal).await.unwrap();
    service
        .update_user_password(target.id(), "new password", false)
        .await
        .unwrap();
    assert_eq!(
        service
            .authenticate_token(password_keys[0].access_token().expose_secret())
            .await
            .unwrap_err(),
        AuthError::InvalidToken
    );
    assert!(
        service
            .list_api_keys(&other_principal)
            .await
            .unwrap()
            .is_empty()
    );

    target_principal = principal(&service, "Renamed Admin", "new password", "target-3").await;
    service
        .create_api_key(&target_principal, "Policy")
        .await
        .unwrap();
    let policy_keys = service.list_api_keys(&other_principal).await.unwrap();
    service
        .update_user_policy(target.id(), false, false)
        .await
        .unwrap();
    assert_eq!(
        service
            .authenticate_token(policy_keys[0].access_token().expose_secret())
            .await
            .unwrap_err(),
        AuthError::InvalidToken
    );
    assert!(
        service
            .list_api_keys(&other_principal)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn api_key_listing_and_startup_validation_fail_the_whole_set_on_ciphertext_swapping() {
    let (service, _clock, database) = service().await;
    let service = service.with_credential_cipher(credential_cipher());
    service
        .create_user("Admin", "admin password", true)
        .await
        .unwrap();
    let admin = principal(&service, "Admin", "admin password", "admin-browser").await;
    service.create_api_key(&admin, "Valid").await.unwrap();
    service.create_api_key(&admin, "Damaged").await.unwrap();
    assert_eq!(service.list_api_keys(&admin).await.unwrap().len(), 2);
    assert!(service.validate_api_key_envelopes().await.is_ok());

    let table = Alias::new("api_keys");
    let query = Query::select()
        .columns([Alias::new("id"), Alias::new("encrypted_payload")])
        .from(table.clone())
        .order_by(Alias::new("id"), sea_orm::sea_query::Order::Asc)
        .to_owned();
    let rows = database
        .query_all(database.get_database_backend().build(&query))
        .await
        .unwrap();
    let first_id = rows[0].try_get::<i64>("", "id").unwrap();
    let first_payload = rows[0].try_get::<Vec<u8>>("", "encrypted_payload").unwrap();
    let second_id = rows[1].try_get::<i64>("", "id").unwrap();
    let second_payload = rows[1].try_get::<Vec<u8>>("", "encrypted_payload").unwrap();
    for (id, payload) in [(first_id, second_payload), (second_id, first_payload)] {
        let update = Query::update()
            .table(table.clone())
            .value(Alias::new("encrypted_payload"), payload)
            .and_where(Expr::col(Alias::new("id")).eq(id))
            .to_owned();
        database
            .execute(database.get_database_backend().build(&update))
            .await
            .unwrap();
    }

    assert!(matches!(
        service.list_api_keys(&admin).await.unwrap_err(),
        AuthError::CredentialCipher(_)
    ));
    assert!(matches!(
        service.validate_api_key_envelopes().await.unwrap_err(),
        AuthError::CredentialCipher(_)
    ));
}

#[tokio::test]
async fn api_key_envelopes_cannot_be_swapped_between_aad_identities() {
    let (service, _clock, database) = service().await;
    let service = service.with_credential_cipher(credential_cipher());
    service
        .create_user("Admin", "admin password", true)
        .await
        .unwrap();
    let admin = principal(&service, "Admin", "admin password", "admin-browser").await;
    service.create_api_key(&admin, "First").await.unwrap();
    service.create_api_key(&admin, "Second").await.unwrap();

    let table = Alias::new("api_keys");
    let rows = database
        .query_all(
            database.get_database_backend().build(
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
    let temporary_envelope = Uuid::new_v4();
    for (id, envelope_id) in [
        (first_id, temporary_envelope),
        (second_id, first_envelope),
        (first_id, second_envelope),
    ] {
        let update = Query::update()
            .table(table.clone())
            .value(Alias::new("envelope_id"), envelope_id)
            .and_where(Expr::col(Alias::new("id")).eq(id))
            .to_owned();
        database
            .execute(database.get_database_backend().build(&update))
            .await
            .unwrap();
    }

    assert!(matches!(
        service.list_api_keys(&admin).await.unwrap_err(),
        AuthError::CredentialCipher(_)
    ));
    assert!(matches!(
        service.validate_api_key_envelopes().await.unwrap_err(),
        AuthError::CredentialCipher(_)
    ));
}
