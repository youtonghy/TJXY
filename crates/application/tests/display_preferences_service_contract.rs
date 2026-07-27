use chrono::Utc;
use sea_orm::DatabaseConnection;
use sea_orm_migration::MigratorTrait;
use serde_json::json;
use tjxy_application::{DisplayPreferencesService, DisplayPreferencesServiceError};
use tjxy_common::{UserId, Username};
use tjxy_db::AuthRepository;
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
async fn preferences_are_private_to_the_authenticated_user() {
    let database = database().await;
    let alice = seed_user(&database, "alice").await;
    let bob = seed_user(&database, "bob").await;
    let preferences_id = Uuid::new_v4();
    let service = DisplayPreferencesService::new(database);

    service
        .replace(
            alice,
            None,
            preferences_id,
            "Findroid",
            &json!({"SortBy":"SortName"}),
        )
        .await
        .unwrap();
    assert_eq!(
        service
            .get(alice, Some(alice), preferences_id, "Findroid")
            .await
            .unwrap(),
        Some(json!({"SortBy":"SortName"}))
    );
    assert!(
        service
            .get(bob, None, preferences_id, "Findroid")
            .await
            .unwrap()
            .is_none()
    );

    assert!(matches!(
        service
            .get(bob, Some(alice), preferences_id, "Findroid")
            .await,
        Err(DisplayPreferencesServiceError::UnauthorizedUser)
    ));
    assert!(matches!(
        service
            .replace(
                bob,
                Some(alice),
                preferences_id,
                "Findroid",
                &json!({"SortBy":"DateCreated"}),
            )
            .await,
        Err(DisplayPreferencesServiceError::UnauthorizedUser)
    ));
}
