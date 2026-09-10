use sea_orm_migration::MigratorTrait;
use tjxy_db::{
    LogMode, LoggingSettingsInput, LoggingSettingsRepository, LoggingSettingsRepositoryError,
    Migrator,
};
use tjxy_test_support::test_database;

#[tokio::test]
async fn logging_settings_follow_compare_and_swap_contract() {
    let database = test_database().await.unwrap();
    Migrator::up(&database, None).await.unwrap();
    let repository = LoggingSettingsRepository::new(&database);

    assert!(repository.get().await.unwrap().is_none());
    let created = repository
        .put(LoggingSettingsInput::default(), None)
        .await
        .unwrap();
    assert_eq!(created.mode(), LogMode::Info);
    assert_eq!(created.retention_days(), 30);
    assert_eq!(created.revision(), 1);

    let updated = repository
        .put(
            LoggingSettingsInput {
                mode: LogMode::Debug,
                retention_days: 7,
            },
            Some(1),
        )
        .await
        .unwrap();
    assert_eq!(updated.mode(), LogMode::Debug);
    assert_eq!(updated.retention_days(), 7);
    assert_eq!(updated.revision(), 2);
    assert!(matches!(
        repository
            .put(LoggingSettingsInput::default(), Some(1))
            .await,
        Err(LoggingSettingsRepositoryError::Conflict)
    ));
}

#[tokio::test]
async fn logging_retention_is_bounded() {
    let database = test_database().await.unwrap();
    Migrator::up(&database, None).await.unwrap();
    let repository = LoggingSettingsRepository::new(&database);
    for retention_days in [0, 366] {
        assert!(matches!(
            repository
                .put(
                    LoggingSettingsInput {
                        mode: LogMode::Error,
                        retention_days
                    },
                    None
                )
                .await,
            Err(LoggingSettingsRepositoryError::InvalidRetentionDays)
        ));
    }
}

#[tokio::test]
async fn debug_deadline_survives_reload_and_restores_previous_mode() {
    let database = test_database().await.unwrap();
    Migrator::up(&database, None).await.unwrap();
    let repository = LoggingSettingsRepository::new(&database);
    repository
        .put(
            LoggingSettingsInput {
                mode: LogMode::Error,
                retention_days: 30,
            },
            None,
        )
        .await
        .unwrap();
    let debug = repository
        .put_with_budget(
            LoggingSettingsInput {
                mode: LogMode::Debug,
                retention_days: 30,
            },
            Some(1),
            Some(tjxy_db::LogBudget {
                max_file_bytes: 1024 * 1024,
                max_directory_bytes: 2 * 1024 * 1024,
            }),
        )
        .await
        .unwrap();
    let restored = LoggingSettingsRepository::new(&database)
        .get()
        .await
        .unwrap()
        .unwrap();
    assert_eq!(debug, restored);
    let expiry = restored.debug_expires_at().unwrap();
    assert_eq!(
        restored.effective_mode_at(expiry - chrono::Duration::seconds(1)),
        LogMode::Debug
    );
    assert_eq!(restored.effective_mode_at(expiry), LogMode::Error);
    assert_eq!(restored.budget().max_directory_bytes, 2 * 1024 * 1024);
    assert!(matches!(
        repository
            .put_with_budget(
                LoggingSettingsInput::default(),
                Some(2),
                Some(tjxy_db::LogBudget {
                    max_file_bytes: 0,
                    max_directory_bytes: 1
                })
            )
            .await,
        Err(LoggingSettingsRepositoryError::InvalidBudget)
    ));
}
