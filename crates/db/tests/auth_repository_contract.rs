use chrono::{Duration, TimeZone, Utc};
use sea_orm::{
    ConnectionTrait, Database, DatabaseConnection, DbBackend, Statement,
    sea_query::{Alias, Expr, Query},
};
use sea_orm_migration::MigratorTrait;
use tjxy_common::Username;
use tjxy_db::{AuthRepository, AuthRepositoryError, SessionDraft};
use uuid::Uuid;

async fn database() -> DatabaseConnection {
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

fn session(now: chrono::DateTime<Utc>, token_digest: [u8; 32]) -> SessionDraft {
    SessionDraft {
        id: Uuid::new_v4(),
        token_digest,
        device_id: "device-1".to_owned(),
        device_name: "Phone".to_owned(),
        client_name: "Findroid".to_owned(),
        client_version: "0.15.3".to_owned(),
        created_at: now,
        expires_at: Some(now + Duration::days(30)),
    }
}

#[tokio::test]
async fn session_commit_rechecks_snapshot_and_token_lookup_returns_principal() {
    let database = database().await;
    let repository = AuthRepository::new(&database);
    let username = Username::parse("Ａlice").unwrap();
    let now = Utc.with_ymd_and_hms(2026, 7, 17, 12, 0, 0).unwrap();
    let created = repository
        .create_user(&username, "$argon2id$test-only", true, true, now)
        .await
        .unwrap();
    let snapshot = repository
        .find_credential(&Username::parse("alice").unwrap())
        .await
        .unwrap()
        .unwrap();
    let digest = [7_u8; 32];

    let issued = repository
        .issue_session(&snapshot, session(now, digest))
        .await
        .unwrap();
    let principal = repository
        .find_principal_by_token_digest(&digest, now)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(created.id(), snapshot.user().id());
    assert_eq!(issued.user_id(), created.id());
    assert_eq!(principal.user().id(), created.id());
    assert_eq!(principal.user().name(), "Ａlice");
    assert!(principal.user().is_admin());
    assert_eq!(principal.session_id(), issued.id());
}

#[tokio::test]
async fn changed_auth_revision_prevents_stale_session_issue() {
    let database = database().await;
    let repository = AuthRepository::new(&database);
    let username = Username::parse("alice").unwrap();
    let now = Utc.with_ymd_and_hms(2026, 7, 17, 12, 0, 0).unwrap();
    repository
        .create_user(&username, "$argon2id$test-only", true, false, now)
        .await
        .unwrap();
    let snapshot = repository
        .find_credential(&username)
        .await
        .unwrap()
        .unwrap();
    let backend = database.get_database_backend();
    database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("users"))
                    .value(
                        Alias::new("auth_revision"),
                        Expr::col(Alias::new("auth_revision")).add(1_i64),
                    )
                    .and_where(Expr::col(Alias::new("id")).eq(snapshot.user().id().as_uuid())),
            ),
        )
        .await
        .unwrap();

    let error = repository
        .issue_session(&snapshot, session(now, [9_u8; 32]))
        .await
        .unwrap_err();

    assert!(matches!(error, AuthRepositoryError::CredentialChanged));
}

#[tokio::test]
async fn expired_session_is_not_authenticated() {
    let database = database().await;
    let repository = AuthRepository::new(&database);
    let username = Username::parse("alice").unwrap();
    let now = Utc.with_ymd_and_hms(2026, 7, 17, 12, 0, 0).unwrap();
    repository
        .create_user(&username, "$argon2id$test-only", true, false, now)
        .await
        .unwrap();
    let snapshot = repository
        .find_credential(&username)
        .await
        .unwrap()
        .unwrap();
    let digest = [11_u8; 32];
    let mut draft = session(now, digest);
    draft.expires_at = Some(now + Duration::seconds(1));
    repository.issue_session(&snapshot, draft).await.unwrap();

    assert!(
        repository
            .find_principal_by_token_digest(&digest, now + Duration::seconds(1))
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn initial_administrator_creation_is_serialized() {
    let database = database().await;
    let now = Utc.with_ymd_and_hms(2026, 7, 17, 12, 0, 0).unwrap();
    let first = Username::parse("first-admin").unwrap();
    let second = Username::parse("second-admin").unwrap();
    let first_repository = AuthRepository::new(&database);
    let second_repository = AuthRepository::new(&database);

    let (first_result, second_result) = tokio::join!(
        first_repository.create_initial_admin(&first, "$argon2id$first", now),
        second_repository.create_initial_admin(&second, "$argon2id$second", now),
    );
    let created = [first_result.unwrap(), second_result.unwrap()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();

    assert_eq!(created.len(), 1);
    assert!(
        AuthRepository::new(&database)
            .has_enabled_admin()
            .await
            .unwrap()
    );
}
