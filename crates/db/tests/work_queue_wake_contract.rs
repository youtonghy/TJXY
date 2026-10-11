//! Runs alone in its own test binary: the wake channel is process-wide, and parallel tests that
//! commit work would wake the waiter under test.

use std::time::Duration;

use sea_orm_migration::MigratorTrait;
use tjxy_common::CatalogItemId;
use tjxy_db::{WorkJobRepository, WorkJobSpec, WorkQueueWaiter, WorkScope, WorkTaskKind};
use tjxy_test_support::test_database;

/// A notified wait returns at once; an unnotified one polls after at least 200 ms.
async fn woken(waiter: &mut WorkQueueWaiter) -> bool {
    tokio::time::timeout(Duration::from_millis(150), waiter.wait())
        .await
        .is_ok()
}

#[tokio::test]
async fn only_new_or_foreground_work_wakes_idle_workers() {
    let database = test_database().await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    let jobs = WorkJobRepository::new(&database);
    let item = CatalogItemId::new();
    let spec = |priority| {
        WorkJobSpec::new(
            WorkTaskKind::ExpandItem,
            WorkScope::CatalogItem(item),
            1,
            priority,
        )
        .unwrap()
    };
    let mut waiter = WorkQueueWaiter::default();
    waiter.reset();

    jobs.enqueue_or_join(&spec(0)).await.unwrap();
    assert!(
        woken(&mut waiter).await,
        "a created job is new claimable work"
    );

    waiter.reset();
    jobs.enqueue_or_join(&spec(0)).await.unwrap();
    assert!(
        !woken(&mut waiter).await,
        "joining queued work makes nothing new claimable"
    );

    waiter.reset();
    jobs.enqueue_or_join(&spec(200)).await.unwrap();
    assert!(
        woken(&mut waiter).await,
        "a foreground request must reach the foreground workers at once"
    );
}
