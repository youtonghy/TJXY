use std::sync::{Arc, Mutex};

use chrono::{Duration, TimeZone, Utc};
use sea_orm::{ConnectionTrait, Database, DbBackend, Statement};
use sea_orm_migration::MigratorTrait;
use tjxy_application::{AuthClock, AuthError, AuthService, ClientIdentity};

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
    let database = Database::connect("sqlite::memory:").await.unwrap();
    database
        .execute(Statement::from_string(
            DbBackend::Sqlite,
            "PRAGMA foreign_keys = ON".to_owned(),
        ))
        .await
        .unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    database
}

fn client() -> ClientIdentity {
    ClientIdentity::new("Findroid", "Phone", "device-1", "0.15.3").unwrap()
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

    let row = database
        .query_one(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT password_hash, token_digest FROM users JOIN auth_sessions ON users.id = auth_sessions.user_id LIMIT 1"
                .to_owned(),
        ))
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
