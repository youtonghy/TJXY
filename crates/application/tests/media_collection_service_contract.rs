use sea_orm_migration::MigratorTrait;
use tjxy_application::{MediaCollectionService, MediaCollectionServiceError};
use tjxy_test_support::test_database;

#[tokio::test]
async fn shared_collection_creation_requires_an_administrator() {
    let database = test_database().await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    let service = MediaCollectionService::new(database);

    assert!(matches!(
        service.create_shared_collection(false, "Staff picks").await,
        Err(MediaCollectionServiceError::AdministratorRequired)
    ));
    let collection = service
        .create_shared_collection(true, "Staff picks")
        .await
        .unwrap();
    assert_eq!(collection.name(), "Staff picks");
    assert!(matches!(
        service
            .rename_shared_collection(false, collection.id(), "Updated picks")
            .await,
        Err(MediaCollectionServiceError::AdministratorRequired)
    ));
    assert!(matches!(
        service
            .delete_shared_collection(false, collection.id())
            .await,
        Err(MediaCollectionServiceError::AdministratorRequired)
    ));
    let renamed = service
        .rename_shared_collection(true, collection.id(), "Updated picks")
        .await
        .unwrap();
    assert_eq!(renamed.name(), "Updated picks");
    assert_eq!(service.shared_collections().await.unwrap().len(), 1);
    service
        .delete_shared_collection(true, collection.id())
        .await
        .unwrap();
    assert!(service.shared_collections().await.unwrap().is_empty());
}
