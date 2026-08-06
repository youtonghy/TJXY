use chrono::Utc;
use tjxy_common::Username;
use tjxy_db::{
    AuthRepository, InstallationRepository, InstallationRepositoryError, InstallationStatus,
    Migrator,
};
use tjxy_test_support::test_database;
use uuid::Uuid;

#[tokio::test]
async fn installation_state_is_singleton_revision_fenced_and_idempotent() {
    use sea_orm_migration::MigratorTrait;

    let database = test_database().await.unwrap();
    Migrator::up(&database, None).await.unwrap();
    let repository = InstallationRepository::new(&database);
    let installation_id = Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap();
    let server_id = Uuid::parse_str("22222222-2222-4222-8222-222222222222").unwrap();

    assert!(repository.find().await.unwrap().is_none());
    let begun = repository
        .begin(installation_id, server_id, Utc::now())
        .await
        .unwrap();
    assert_eq!(begun.status(), InstallationStatus::Pending);
    assert_eq!(begun.revision(), 1);
    assert_eq!(begun.installation_id(), installation_id);
    assert_eq!(begun.server_id(), server_id);
    assert_eq!(
        repository
            .begin(installation_id, server_id, Utc::now())
            .await
            .unwrap(),
        begun
    );

    assert!(matches!(
        repository
            .begin(Uuid::new_v4(), server_id, Utc::now())
            .await,
        Err(InstallationRepositoryError::Conflict)
    ));

    let username = Username::parse("setup-admin").unwrap();
    let administrator = AuthRepository::new(&database)
        .create_initial_admin(&username, "argon2-test-hash", Utc::now())
        .await
        .unwrap()
        .unwrap();
    let attached = repository
        .attach_initial_admin(installation_id, administrator.id(), 1, Utc::now())
        .await
        .unwrap();
    assert_eq!(attached.administrator_id(), Some(administrator.id()));
    assert_eq!(attached.revision(), 2);

    let completed = repository
        .complete(installation_id, 2, Utc::now())
        .await
        .unwrap();
    assert_eq!(completed.status(), InstallationStatus::Completed);
    assert_eq!(completed.revision(), 3);
    assert!(completed.completed_at().is_some());
    assert_eq!(
        repository
            .complete(installation_id, 2, Utc::now())
            .await
            .unwrap(),
        completed
    );

    assert!(matches!(
        repository
            .attach_initial_admin(installation_id, administrator.id(), 1, Utc::now())
            .await,
        Err(InstallationRepositoryError::Conflict)
    ));
}
