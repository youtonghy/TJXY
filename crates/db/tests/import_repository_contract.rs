use chrono::Duration;
use sea_orm_migration::MigratorTrait;
use serde_json::json;
use tjxy_db::{
    ImportJobRepository, ImportJobState, ImportStagingItem, ImportStagingRepositoryError,
};
use tjxy_test_support::test_database;

#[tokio::test]
async fn import_job_is_leased_checkpointed_pauseable_and_staging_is_replay_safe() {
    let database = test_database().await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    let repository = ImportJobRepository::new(&database);
    let created = repository
        .create("EmbyApi", "server-instance", true)
        .await
        .unwrap();
    let claimed = repository
        .claim_next("EmbyApi", "import-worker", Duration::seconds(30))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(claimed.id(), created.id());
    let item = ImportStagingItem::new(
        "Movie",
        "legacy-movie",
        None,
        json!({"name":"Arrival","production_year":2016}),
    )
    .unwrap();

    let first = repository.stage_item(&claimed, &item).await.unwrap();
    let replay = repository.stage_item(&claimed, &item).await.unwrap();
    assert!(!first.replayed());
    assert!(replay.replayed());
    let changed =
        ImportStagingItem::new("Movie", "legacy-movie", None, json!({"name":"Different"})).unwrap();
    assert!(matches!(
        repository.stage_item(&claimed, &changed).await.unwrap_err(),
        ImportStagingRepositoryError::ReplayConflict
    ));
    repository
        .save_checkpoint(&claimed, json!({"start_index": 100}))
        .await
        .unwrap();

    repository.pause(created.id()).await.unwrap();
    assert_eq!(
        repository.get(created.id()).await.unwrap().unwrap().state(),
        ImportJobState::Paused
    );
    assert!(matches!(
        repository.stage_item(&claimed, &item).await.unwrap_err(),
        ImportStagingRepositoryError::LostLease
    ));
    repository.resume(created.id()).await.unwrap();
    let reclaimed = repository
        .claim_next("EmbyApi", "import-worker", Duration::seconds(30))
        .await
        .unwrap()
        .unwrap();
    repository
        .complete_dry_run(&reclaimed, json!({"items":1,"conflicts":0,"errors":0}))
        .await
        .unwrap();

    let completed = repository.get(created.id()).await.unwrap().unwrap();
    assert_eq!(completed.state(), ImportJobState::Completed);
    assert_eq!(completed.checkpoint(), &json!({"start_index":100}));
    assert_eq!(
        completed.counters(),
        &json!({"items":1,"conflicts":0,"errors":0})
    );
}

#[tokio::test]
async fn non_dry_run_seals_staging_before_catalog_publication() {
    let database = test_database().await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    let repository = ImportJobRepository::new(&database);
    let created = repository
        .create("EmbyApi", "server-instance", false)
        .await
        .unwrap();
    let claimed = repository
        .claim_next("EmbyApi", "import-worker", Duration::seconds(30))
        .await
        .unwrap()
        .unwrap();
    repository
        .seal_for_publication(&claimed, json!({"items":0,"conflicts":0,"errors":0}))
        .await
        .unwrap();

    let sealed = repository.get(created.id()).await.unwrap().unwrap();
    assert_eq!(sealed.state(), ImportJobState::ReadyToPublish);
    assert!(matches!(
        repository
            .stage_item(
                &claimed,
                &ImportStagingItem::new("Movie", "late-item", None, json!({})).unwrap()
            )
            .await
            .unwrap_err(),
        ImportStagingRepositoryError::LostLease
    ));
}

#[tokio::test]
async fn worker_failures_are_fenced_and_can_retry_with_backoff() {
    let database = test_database().await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    let repository = ImportJobRepository::new(&database);
    let created = repository
        .create("EmbyApi", "server-instance", true)
        .await
        .unwrap();
    let first = repository
        .claim_next("EmbyApi", "import-worker", Duration::seconds(30))
        .await
        .unwrap()
        .unwrap();
    repository
        .retry(&first, Duration::seconds(1), "temporary transport failure")
        .await
        .unwrap();
    assert!(matches!(
        repository.complete_dry_run(&first, json!({})).await,
        Err(ImportStagingRepositoryError::LostLease)
    ));
    tokio::time::sleep(std::time::Duration::from_millis(1_100)).await;
    let second = repository
        .claim_next("EmbyApi", "import-worker", Duration::seconds(30))
        .await
        .unwrap()
        .unwrap();
    repository
        .fail_terminal(&second, "credential payload is invalid")
        .await
        .unwrap();

    assert_eq!(
        repository.get(created.id()).await.unwrap().unwrap().state(),
        ImportJobState::Failed
    );
}
