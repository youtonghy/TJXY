use sea_orm::{
    ConnectOptions, ConnectionTrait, Database, DatabaseConnection, DbBackend, Statement,
};
use sea_orm_migration::MigratorTrait;
use serde_json::json;
use tjxy_db::{
    Migrator, SiteThemeSelectionInput, SiteThemeSettingsRepository,
    SiteThemeSettingsRepositoryError,
};
use tjxy_test_support::{reconnectable_test_database, test_database};

async fn database() -> DatabaseConnection {
    let database = test_database().await.unwrap();
    Migrator::up(&database, None).await.unwrap();
    database
}

fn selection(theme_id: &str, options: serde_json::Value) -> SiteThemeSelectionInput {
    SiteThemeSelectionInput {
        theme_id: theme_id.to_owned(),
        schema_version: 1,
        options,
    }
}

#[tokio::test]
async fn selection_round_trip_preserves_each_theme_configuration() {
    let database = database().await;
    let repository = SiteThemeSettingsRepository::new(&database);

    let created = repository
        .put(
            &selection("classic", json!({"contentWidth": "standard"})),
            None,
        )
        .await
        .unwrap();
    assert_eq!(created.active_theme_id(), "classic");
    assert_eq!(created.revision(), 1);

    let updated = repository
        .put(&selection("cinema", json!({"density": "compact"})), Some(1))
        .await
        .unwrap();
    assert_eq!(updated.active_theme_id(), "cinema");
    assert_eq!(updated.revision(), 2);
    assert_eq!(updated.configurations().len(), 2);
    assert_eq!(
        updated.configurations()["classic"].options(),
        &json!({"contentWidth": "standard"})
    );

    let persisted = repository.get().await.unwrap().unwrap();
    assert_eq!(persisted.active_theme_id(), "cinema");
    assert_eq!(persisted.revision(), 2);
    assert_eq!(persisted.configurations().len(), 2);
    assert_eq!(
        persisted.configurations()["cinema"].options(),
        &json!({"density": "compact"})
    );
}

#[tokio::test]
async fn invalid_ids_options_and_revisions_are_rejected() {
    let database = database().await;
    let repository = SiteThemeSettingsRepository::new(&database);

    assert!(matches!(
        repository
            .put(&selection("Invalid Theme", json!({})), None)
            .await,
        Err(SiteThemeSettingsRepositoryError::InvalidThemeId)
    ));
    assert!(matches!(
        repository
            .put(&selection("classic", json!(["not", "an", "object"])), None)
            .await,
        Err(SiteThemeSettingsRepositoryError::InvalidOptions)
    ));
    repository
        .put(&selection("classic", json!({})), None)
        .await
        .unwrap();
    assert!(matches!(
        repository
            .put(&selection("cinema", json!({})), Some(0))
            .await,
        Err(SiteThemeSettingsRepositoryError::InvalidRevision)
    ));
}

#[tokio::test]
async fn concurrent_saves_have_one_revision_winner() {
    let fixture = reconnectable_test_database().await.unwrap();
    let database = fixture.connection();
    Migrator::up(database, None).await.unwrap();
    SiteThemeSettingsRepository::new(database)
        .put(&selection("classic", json!({})), None)
        .await
        .unwrap();
    let first_database = settings_database_connection(fixture.database_url()).await;
    let second_database = settings_database_connection(fixture.database_url()).await;

    let (first, second) = tokio::join!(
        async move {
            SiteThemeSettingsRepository::new(&first_database)
                .put(&selection("cinema", json!({"density": "compact"})), Some(1))
                .await
        },
        async move {
            SiteThemeSettingsRepository::new(&second_database)
                .put(
                    &selection("cinema", json!({"density": "comfortable"})),
                    Some(1),
                )
                .await
        }
    );

    let results = [first, second];
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, Err(SiteThemeSettingsRepositoryError::Conflict)))
            .count(),
        1
    );
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
