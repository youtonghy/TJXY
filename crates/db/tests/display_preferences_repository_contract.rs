use chrono::Utc;
use sea_orm::DatabaseConnection;
use sea_orm_migration::MigratorTrait;
use serde_json::json;
use tjxy_common::{UserId, Username};
use tjxy_db::{AuthRepository, DisplayPreferencesRepository};
use tjxy_test_support::test_database;
use uuid::Uuid;

async fn database() -> DatabaseConnection {
    let database = test_database().await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    database
}

async fn seed_user(database: &DatabaseConnection, name: &str) -> UserId {
    AuthRepository::new(database)
        .create_user(
            &Username::parse(name).unwrap(),
            "$argon2id$test",
            true,
            false,
            Utc::now(),
        )
        .await
        .unwrap()
        .id()
}

#[tokio::test]
async fn display_preferences_are_replaced_atomically_and_scoped_to_one_user() {
    let database = database().await;
    let alice = seed_user(&database, "alice").await;
    let bob = seed_user(&database, "bob").await;
    let preferences_id = Uuid::new_v4();
    let repository = DisplayPreferencesRepository::new(&database);

    assert!(
        repository
            .get(alice, preferences_id, "Findroid")
            .await
            .unwrap()
            .is_none()
    );
    repository
        .replace(
            alice,
            preferences_id,
            "Findroid",
            &json!({"SortBy":"SortName","CustomPrefs":{"homesection0":"resume"}}),
        )
        .await
        .unwrap();
    assert_eq!(
        repository
            .get(alice, preferences_id, "Findroid")
            .await
            .unwrap(),
        Some(json!({"SortBy":"SortName","CustomPrefs":{"homesection0":"resume"}}))
    );
    repository
        .replace(
            alice,
            preferences_id,
            "Findroid",
            &json!({"SortBy":"DateCreated","CustomPrefs":{}}),
        )
        .await
        .unwrap();
    assert_eq!(
        repository
            .get(alice, preferences_id, "Findroid")
            .await
            .unwrap(),
        Some(json!({"SortBy":"DateCreated","CustomPrefs":{}}))
    );
    assert!(
        repository
            .get(bob, preferences_id, "Findroid")
            .await
            .unwrap()
            .is_none()
    );
}
