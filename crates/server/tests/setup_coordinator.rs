use base64::{Engine as _, engine::general_purpose::STANDARD};
use sea_orm::{ConnectionTrait, Database, DbBackend, Statement};
use sea_orm_migration::MigratorTrait;
use tempfile::tempdir;
use tjxy_application::{AuthService, SystemClock};
use tjxy_db::{InstallationRepository, InstallationStatus, Migrator, SystemSettingsRepository};
use tjxy_server::{
    CompleteSetupInput, DatabaseConfiguration, DatabaseDraft, InstallationConfigStore,
    InstallationProfile, InstallationState, NetworkConfiguration, PendingInstallation,
    SecretString, SetupCoordinator, SetupErrorCode, SetupProgressStage, SetupValidator,
};
use uuid::Uuid;

#[tokio::test]
async fn setup_completion_creates_the_database_admin_settings_and_completed_config() {
    let directory = tempdir().unwrap();
    let config = InstallationConfigStore::at(directory.path().join("config/tjxy.toml"));
    let database_path = directory.path().join("tjxy.db");
    let coordinator = SetupCoordinator::new(
        config.clone(),
        SetupValidator::new(vec![directory.path().to_path_buf()]).unwrap(),
    );
    let mut progress = coordinator.subscribe_progress();

    let completion = coordinator
        .complete(CompleteSetupInput::new(
            "Cinema",
            "Private screenings",
            "zh-CN",
            "/brand/tjxy-mark.webp",
            "/brand/favicon.svg",
            DatabaseDraft::Sqlite {
                path: database_path.clone(),
            },
            NetworkConfiguration::new("127.0.0.1", 18096, None),
            "setup-admin",
            "correct horse battery staple",
        ))
        .await
        .unwrap();

    let mut stages = Vec::new();
    loop {
        let update = progress.recv().await.unwrap();
        stages.push(update.stage);
        if update.stage == SetupProgressStage::Complete {
            break;
        }
    }
    assert_eq!(
        stages,
        vec![
            SetupProgressStage::ConnectingDatabase,
            SetupProgressStage::MigratingDatabase,
            SetupProgressStage::CreatingAdministrator,
            SetupProgressStage::SavingSettings,
            SetupProgressStage::CompletingInstallation,
            SetupProgressStage::Complete,
        ]
    );

    assert_eq!(
        completion.destination_url(),
        "http://127.0.0.1:18096/app/login?redirect=%2Fadmin"
    );
    let completed = match config.load().unwrap() {
        InstallationState::Completed(completed) => completed,
        state => panic!("expected completed configuration, got {state:?}"),
    };
    let mut derivable_key = [0_u8; 32];
    derivable_key[..16].copy_from_slice(completed.installation_id().as_bytes());
    derivable_key[16..].copy_from_slice(completed.server_id().as_bytes());
    assert!(
        !completed
            .credential_keyring()
            .contains(&STANDARD.encode(derivable_key)),
        "credential encryption key must not be derivable from stored identifiers"
    );

    let database = Database::connect(format!("sqlite://{}?mode=rw", database_path.display()))
        .await
        .unwrap();
    database
        .execute(Statement::from_string(
            DbBackend::Sqlite,
            "PRAGMA foreign_keys = ON".to_owned(),
        ))
        .await
        .unwrap();
    Migrator::up(&database, None).await.unwrap();
    let installation = InstallationRepository::new(&database)
        .find()
        .await
        .unwrap()
        .unwrap();
    assert_eq!(installation.status(), InstallationStatus::Completed);
    assert!(installation.administrator_id().is_some());
    let settings = SystemSettingsRepository::new(&database)
        .get()
        .await
        .unwrap()
        .unwrap();
    assert_eq!(settings.site_title(), "Cinema");
    assert_eq!(settings.port(), 18096);

    let admin_count: i64 = database
        .query_one(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT COUNT(*) AS count FROM users WHERE username = 'setup-admin' AND is_admin = TRUE AND disabled_at IS NULL".to_owned(),
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get("", "count")
        .unwrap();
    assert_eq!(admin_count, 1);
}

#[tokio::test]
async fn pending_installation_recovers_only_with_the_same_administrator_credentials() {
    let directory = tempdir().unwrap();
    let config = InstallationConfigStore::at(directory.path().join("config/tjxy.toml"));
    let database_path = directory.path().join("recover.db");
    let installation_id = Uuid::new_v4();
    let server_id = Uuid::new_v4();
    let pending = PendingInstallation::new(
        installation_id,
        server_id,
        SecretString::new(
            r#"{"active_version":1,"keys":{"1":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="}}"#,
        ),
        DatabaseConfiguration::Sqlite {
            path: database_path.clone(),
        },
        NetworkConfiguration::new("127.0.0.1", 18097, None),
        InstallationProfile::new(
            "Recovered Cinema",
            "Crash-safe setup",
            "en-US",
            "/brand/tjxy-mark.webp",
            "/brand/favicon.svg",
            "setup-admin",
        ),
    );
    config.write_pending(&pending).unwrap();

    let database = Database::connect(format!("sqlite://{}?mode=rwc", database_path.display()))
        .await
        .unwrap();
    Migrator::up(&database, None).await.unwrap();
    let installation = InstallationRepository::new(&database)
        .begin(installation_id, server_id, chrono::Utc::now())
        .await
        .unwrap();
    let auth = AuthService::new(database.clone(), SystemClock, None, 2)
        .await
        .unwrap();
    let administrator = auth
        .create_initial_admin("setup-admin", "correct horse battery staple")
        .await
        .unwrap()
        .unwrap();
    InstallationRepository::new(&database)
        .attach_initial_admin(
            installation_id,
            administrator.id(),
            installation.revision(),
            chrono::Utc::now(),
        )
        .await
        .unwrap();
    database.close().await.unwrap();

    let coordinator = SetupCoordinator::new(
        config.clone(),
        SetupValidator::new(vec![directory.path().to_path_buf()]).unwrap(),
    );
    let wrong = coordinator
        .recover(installation_id, "setup-admin", "wrong password")
        .await
        .unwrap_err();
    assert_eq!(wrong.code(), SetupErrorCode::RecoveryAuthenticationFailed);
    assert!(matches!(
        config.load().unwrap(),
        InstallationState::Pending(_)
    ));

    let completion = coordinator
        .recover(
            installation_id,
            "setup-admin",
            "correct horse battery staple",
        )
        .await
        .unwrap();
    assert_eq!(
        completion.destination_url(),
        "http://127.0.0.1:18097/app/login?redirect=%2Fadmin"
    );
    assert!(matches!(
        config.load().unwrap(),
        InstallationState::Completed(_)
    ));
}
