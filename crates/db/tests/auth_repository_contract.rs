use chrono::{Duration, TimeZone, Utc};
use sea_orm::{
    ConnectionTrait, DatabaseConnection,
    sea_query::{Alias, Expr, Query},
};
use sea_orm_migration::MigratorTrait;
use tjxy_common::Username;
use tjxy_db::{AuthRepository, AuthRepositoryError, AuthUser, SessionDraft};
use tjxy_test_support::test_database;
use uuid::Uuid;

async fn database() -> DatabaseConnection {
    let database = test_database().await.unwrap();
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
    assert_eq!(principal.session_id(), Some(issued.id()));
    assert_eq!(principal.device_id(), Some("device-1"));
    assert_eq!(principal.api_key_id(), None);
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
async fn enabled_user_ids_are_bounded_and_exclude_disabled_accounts() {
    let database = database().await;
    let repository = AuthRepository::new(&database);
    let now = Utc.with_ymd_and_hms(2026, 7, 17, 12, 0, 0).unwrap();
    let disabled = repository
        .create_user(
            &Username::parse("alpha-disabled").unwrap(),
            "$argon2id$test-only",
            true,
            false,
            now,
        )
        .await
        .unwrap();
    let first_enabled = repository
        .create_user(
            &Username::parse("bravo-enabled").unwrap(),
            "$argon2id$test-only",
            true,
            false,
            now,
        )
        .await
        .unwrap();
    repository
        .create_user(
            &Username::parse("charlie-enabled").unwrap(),
            "$argon2id$test-only",
            true,
            false,
            now,
        )
        .await
        .unwrap();
    repository
        .update_policy(disabled.id(), false, true, now + Duration::seconds(1))
        .await
        .unwrap();

    assert_eq!(
        repository.enabled_user_ids(1).await.unwrap(),
        vec![first_enabled.id()]
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

async fn two_users() -> (
    DatabaseConnection,
    AuthUser,
    AuthUser,
    chrono::DateTime<Utc>,
) {
    let database = database().await;
    let repository = AuthRepository::new(&database);
    let now = Utc.with_ymd_and_hms(2026, 7, 20, 12, 0, 0).unwrap();
    let alice = repository
        .create_user(
            &Username::parse("Alice").unwrap(),
            "$argon2id$alice",
            true,
            true,
            now,
        )
        .await
        .unwrap();
    let bob = repository
        .create_user(
            &Username::parse("Bob").unwrap(),
            "$argon2id$bob",
            true,
            false,
            now,
        )
        .await
        .unwrap();
    (database, alice, bob, now)
}

#[tokio::test]
async fn administrator_credential_mutations_revoke_existing_sessions() {
    let (database, _alice, bob, now) = two_users().await;
    let repository = AuthRepository::new(&database);
    let bob_credential = repository
        .find_credential(&Username::parse("Bob").unwrap())
        .await
        .unwrap()
        .unwrap();
    repository
        .issue_session(&bob_credential, session(now, [17_u8; 32]))
        .await
        .unwrap();

    let users = repository.list_users().await.unwrap();
    assert_eq!(
        users.iter().map(AuthUser::name).collect::<Vec<_>>(),
        ["Alice", "Bob"]
    );
    assert_eq!(repository.get_user(bob.id()).await.unwrap().unwrap(), bob);

    let renamed = repository
        .rename_user(
            bob.id(),
            &Username::parse("Robert").unwrap(),
            now + Duration::seconds(1),
        )
        .await
        .unwrap();
    assert_eq!(renamed.name(), "Robert");
    assert!(renamed.auth_revision() > bob.auth_revision());
    assert!(
        repository
            .find_principal_by_token_digest(&[17_u8; 32], now)
            .await
            .unwrap()
            .is_none()
    );
    let renamed_credential = repository
        .find_credential(&Username::parse("Robert").unwrap())
        .await
        .unwrap()
        .unwrap();
    repository
        .issue_session(
            &renamed_credential,
            session(now + Duration::seconds(1), [18_u8; 32]),
        )
        .await
        .unwrap();
    repository
        .update_password(
            bob.id(),
            "$argon2id$robert-new",
            true,
            now + Duration::seconds(2),
        )
        .await
        .unwrap();
    assert!(
        repository
            .find_principal_by_token_digest(&[18_u8; 32], now + Duration::seconds(2))
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn administrator_policy_mutations_preserve_an_enabled_admin() {
    let (database, alice, bob, now) = two_users().await;
    let repository = AuthRepository::new(&database);
    for result in [
        repository
            .update_policy(alice.id(), false, false, now + Duration::seconds(3))
            .await
            .map(|_| ()),
        repository
            .update_policy(alice.id(), true, true, now + Duration::seconds(3))
            .await
            .map(|_| ()),
        repository.delete_user(alice.id()).await,
    ] {
        assert!(matches!(result, Err(AuthRepositoryError::LastEnabledAdmin)));
    }

    repository
        .update_policy(bob.id(), true, false, now + Duration::seconds(4))
        .await
        .unwrap();
    repository
        .update_policy(alice.id(), false, false, now + Duration::seconds(5))
        .await
        .unwrap();
    repository.delete_user(alice.id()).await.unwrap();
    assert!(repository.get_user(alice.id()).await.unwrap().is_none());
}
