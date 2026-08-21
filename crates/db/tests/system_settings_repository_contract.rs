use sea_orm::{
    ConnectOptions, ConnectionTrait, Database, DatabaseConnection, DbBackend, Statement,
};
use sea_orm_migration::MigratorTrait;
use tjxy_db::{
    Migrator, SystemSettingsInput, SystemSettingsRepository, SystemSettingsRepositoryError,
};
use tjxy_test_support::{reconnectable_test_database, test_database};

async fn database() -> sea_orm::DatabaseConnection {
    let database = test_database().await.unwrap();
    Migrator::up(&database, None).await.unwrap();
    database
}

async fn settings_database_connection(database_url: &str) -> DatabaseConnection {
    let mut options = ConnectOptions::new(database_url);
    options.max_connections(1).min_connections(1);
    let database = Database::connect(options).await.unwrap();
    if database.get_database_backend() == DbBackend::Sqlite {
        database
            .execute(Statement::from_string(
                DbBackend::Sqlite,
                "PRAGMA busy_timeout = 5000",
            ))
            .await
            .unwrap();
    }
    database
}

fn input(title: &str) -> SystemSettingsInput {
    SystemSettingsInput {
        site_title: title.to_owned(),
        ..SystemSettingsInput::default()
    }
}

#[tokio::test]
async fn settings_create_and_update_follow_compare_and_swap_contract() {
    let database = database().await;
    let repository = SystemSettingsRepository::new(&database);

    let created = repository.put(&input("Initial"), None).await.unwrap();
    assert_eq!(created.revision(), 1);
    assert!(matches!(
        repository.put(&input("Duplicate"), None).await,
        Err(SystemSettingsRepositoryError::Conflict)
    ));

    let updated = repository.put(&input("Updated"), Some(1)).await.unwrap();
    assert_eq!(updated.revision(), 2);
    assert_eq!(updated.site_title(), "Updated");
    assert!(matches!(
        repository.put(&input("Stale"), Some(1)).await,
        Err(SystemSettingsRepositoryError::Conflict)
    ));
    assert!(matches!(
        repository.put(&input("Invalid"), Some(0)).await,
        Err(SystemSettingsRepositoryError::InvalidRevision)
    ));
    assert!(matches!(
        repository.put(&input("Overflow"), Some(i64::MAX)).await,
        Err(SystemSettingsRepositoryError::InvalidRevision)
    ));
}

#[tokio::test]
async fn expected_revision_conflicts_when_singleton_is_missing() {
    let database = database().await;
    let repository = SystemSettingsRepository::new(&database);
    assert!(matches!(
        repository.put(&input("Missing"), Some(1)).await,
        Err(SystemSettingsRepositoryError::Conflict)
    ));
    assert!(matches!(
        repository.put_locale("invalid", Some(1)).await,
        Err(SystemSettingsRepositoryError::Conflict)
    ));
}

#[tokio::test]
async fn locale_create_only_conflicts_when_singleton_exists() {
    let database = database().await;
    let repository = SystemSettingsRepository::new(&database);
    repository.put(&input("Initial"), None).await.unwrap();

    assert!(matches!(
        repository.put_locale("en-US", None).await,
        Err(SystemSettingsRepositoryError::Conflict)
    ));
}

#[tokio::test]
async fn locale_update_preserves_other_fields_and_increments_revision() {
    let database = database().await;
    let repository = SystemSettingsRepository::new(&database);
    let mut initial = input("Cinema");
    initial.site_subtitle = "Private screenings".to_owned();
    initial.public_url = Some("https://media.example.com".to_owned());
    repository.put(&initial, None).await.unwrap();

    let updated = repository.put_locale("en-US", Some(1)).await.unwrap();
    assert_eq!(updated.revision(), 2);
    assert_eq!(updated.locale(), "en-US");
    assert_eq!(updated.site_title(), "Cinema");
    assert_eq!(updated.site_subtitle(), "Private screenings");
    assert_eq!(updated.public_url(), Some("https://media.example.com"));
}

#[tokio::test]
async fn concurrent_saves_have_exactly_one_revision_winner() {
    let fixture = reconnectable_test_database().await.unwrap();
    let database = fixture.connection();
    Migrator::up(database, None).await.unwrap();
    SystemSettingsRepository::new(database)
        .put(&input("Initial"), None)
        .await
        .unwrap();
    let first_database = settings_database_connection(fixture.database_url()).await;
    let second_database = settings_database_connection(fixture.database_url()).await;
    let first = input("First writer");
    let second = input("Second writer");

    let (first_result, second_result) = tokio::join!(
        async move {
            SystemSettingsRepository::new(&first_database)
                .put(&first, Some(1))
                .await
        },
        async move {
            SystemSettingsRepository::new(&second_database)
                .put(&second, Some(1))
                .await
        },
    );

    let results = [first_result, second_result];
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, Err(SystemSettingsRepositoryError::Conflict)))
            .count(),
        1
    );
    let winner = results
        .iter()
        .find_map(|result| result.as_ref().ok())
        .unwrap();
    assert_eq!(winner.revision(), 2);
    let stored = SystemSettingsRepository::new(database)
        .get()
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.revision(), 2);
    assert_eq!(stored.site_title(), winner.site_title());
}
