use chrono::{Duration, Utc};
use sea_orm_migration::MigratorTrait;
use tjxy_common::Username;
use tjxy_db::{AuthRepository, PasskeyChallenge, PasskeyCredential, PasskeyRepository};
use tjxy_test_support::test_database;
use uuid::Uuid;

#[tokio::test]
async fn challenge_is_consumed_once_and_expired_state_is_rejected() {
    let database = test_database().await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    let repository = PasskeyRepository::new(&database);
    let now = Utc::now();
    let active = PasskeyChallenge {
        id: Uuid::new_v4(),
        user_id: None,
        kind: "authentication".to_owned(),
        state: vec![1, 2, 3],
        expires_at: now + Duration::minutes(5),
    };
    repository.put_challenge(&active, now).await.unwrap();

    assert_eq!(
        repository.take_challenge(active.id, now).await.unwrap(),
        Some(active.clone())
    );
    assert_eq!(
        repository.take_challenge(active.id, now).await.unwrap(),
        None
    );

    let expired = PasskeyChallenge {
        id: Uuid::new_v4(),
        expires_at: now - Duration::seconds(1),
        ..active
    };
    repository
        .put_challenge(&expired, now - Duration::minutes(10))
        .await
        .unwrap();
    assert_eq!(
        repository.take_challenge(expired.id, now).await.unwrap(),
        None
    );
}

#[tokio::test]
async fn deleting_a_user_cascades_passkey_state() {
    let database = test_database().await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    let now = Utc::now();
    let user = AuthRepository::new(&database)
        .create_user(
            &Username::parse("passkey-owner").unwrap(),
            "$argon2id$test-only",
            true,
            false,
            now,
        )
        .await
        .unwrap();
    let repository = PasskeyRepository::new(&database);
    repository
        .insert(&PasskeyCredential {
            id: Uuid::new_v4(),
            user_id: user.id().as_uuid(),
            credential_id: "credential-id".to_owned(),
            public_key: vec![1, 2, 3],
            counter: 0,
            name: "Passkey".to_owned(),
            created_at: now,
            last_used_at: now,
        })
        .await
        .unwrap();
    let challenge_id = Uuid::new_v4();
    repository
        .put_challenge(
            &PasskeyChallenge {
                id: challenge_id,
                user_id: Some(user.id().as_uuid()),
                kind: "registration".to_owned(),
                state: vec![4, 5, 6],
                expires_at: now + Duration::minutes(5),
            },
            now,
        )
        .await
        .unwrap();

    AuthRepository::new(&database)
        .delete_user(user.id())
        .await
        .unwrap();

    assert!(
        repository
            .list(user.id().as_uuid())
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        repository.take_challenge(challenge_id, now).await.unwrap(),
        None
    );
}
