use std::sync::{Arc, Mutex};

use chrono::{Duration, Utc};
use sea_orm::{
    ConnectionTrait, DatabaseConnection,
    sea_query::{Alias, Expr, Query},
};
use sea_orm_migration::MigratorTrait;
use tjxy_common::{CatalogItemId, StorageObjectRecordId};
use tjxy_db::{
    WorkJobClock, WorkJobRepository, WorkJobSpec, WorkRetentionRepository, WorkRetentionRun,
    WorkScope, WorkTaskKind,
};
use tjxy_test_support::test_database;

#[derive(Clone)]
struct ManualClock(Arc<Mutex<chrono::DateTime<Utc>>>);

impl WorkJobClock for ManualClock {
    fn now(&self) -> chrono::DateTime<Utc> {
        *self.0.lock().unwrap()
    }
}

async fn database() -> DatabaseConnection {
    let database = test_database().await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    database
}

#[tokio::test]
async fn newly_terminal_job_is_scheduled_and_removed_after_retention() {
    let database = database().await;
    let terminal_at = Utc::now() - Duration::days(31);
    let jobs =
        WorkJobRepository::with_clock(&database, ManualClock(Arc::new(Mutex::new(terminal_at))));
    let _submitted = jobs
        .enqueue_or_join(
            &WorkJobSpec::new(
                WorkTaskKind::ProbeMedia,
                WorkScope::CatalogItem(CatalogItemId::new()),
                1,
                100,
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let claimed = jobs
        .claim_next(
            &[WorkTaskKind::ProbeMedia],
            "retention-contract",
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    jobs.fail_terminal(&claimed, "fixture failure")
        .await
        .unwrap();

    assert_eq!(
        table_count(&database, "work_job_retention_queue", "job_id").await,
        1
    );
    assert_eq!(table_count(&database, "work_results", "id").await, 1);

    let run = WorkRetentionRepository::new(&database)
        .run_once(
            "retention-worker",
            Duration::days(30),
            Duration::seconds(30),
        )
        .await
        .unwrap();

    assert_eq!(
        run,
        WorkRetentionRun::Processed {
            deleted: 1,
            compacted: 0,
            purged: 0,
            deferred: 0,
        }
    );
    assert_eq!(table_count(&database, "work_jobs", "id").await, 0);
    assert_eq!(table_count(&database, "work_results", "id").await, 0);
    assert_eq!(
        table_count(&database, "work_job_retention_queue", "job_id").await,
        0
    );
}

#[tokio::test]
async fn active_dependency_defers_retention() {
    let database = database().await;
    let terminal_at = Utc::now() - Duration::days(31);
    let jobs =
        WorkJobRepository::with_clock(&database, ManualClock(Arc::new(Mutex::new(terminal_at))));
    let sync = jobs
        .enqueue_or_join(
            &WorkJobSpec::new(
                WorkTaskKind::ScopedStorageSync,
                WorkScope::StorageObject(StorageObjectRecordId::new()),
                1,
                100,
            )
            .unwrap(),
        )
        .await
        .unwrap();
    jobs.enqueue_or_join(
        &WorkJobSpec::new(
            WorkTaskKind::ExpandItem,
            WorkScope::CatalogItem(CatalogItemId::new()),
            1,
            100,
        )
        .unwrap()
        .with_pending_required_sync(sync.job().id()),
    )
    .await
    .unwrap();
    let claimed = jobs
        .claim_next(
            &[WorkTaskKind::ScopedStorageSync],
            "dependency-retention-contract",
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    jobs.fail_terminal(&claimed, "fixture failure")
        .await
        .unwrap();

    let run = WorkRetentionRepository::new(&database)
        .run_once(
            "retention-worker",
            Duration::days(30),
            Duration::seconds(30),
        )
        .await
        .unwrap();

    assert_eq!(
        run,
        WorkRetentionRun::Processed {
            deleted: 0,
            compacted: 0,
            purged: 0,
            deferred: 1,
        }
    );
    assert_eq!(table_count(&database, "work_jobs", "id").await, 2);
    assert_eq!(table_count(&database, "work_results", "id").await, 1);
}

#[tokio::test]
async fn retention_deletes_multiple_terminal_jobs_in_one_batch() {
    let database = database().await;
    let terminal_at = Utc::now() - Duration::days(31);
    let jobs =
        WorkJobRepository::with_clock(&database, ManualClock(Arc::new(Mutex::new(terminal_at))));
    for number in 0..3 {
        let submitted = jobs
            .enqueue_or_join(
                &WorkJobSpec::new(
                    WorkTaskKind::ProbeMedia,
                    WorkScope::CatalogItem(CatalogItemId::new()),
                    number,
                    100,
                )
                .unwrap(),
            )
            .await
            .unwrap();
        let claimed = jobs
            .claim_next(
                &[WorkTaskKind::ProbeMedia],
                "retention-batch-contract",
                Duration::minutes(5),
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(claimed.id(), submitted.job().id());
        jobs.fail_terminal(&claimed, "batch fixture").await.unwrap();
    }

    assert_eq!(
        WorkRetentionRepository::new(&database)
            .run_once(
                "retention-worker",
                Duration::days(30),
                Duration::seconds(30),
            )
            .await
            .unwrap(),
        WorkRetentionRun::Processed {
            deleted: 3,
            compacted: 0,
            purged: 0,
            deferred: 0,
        }
    );
    assert_eq!(table_count(&database, "work_jobs", "id").await, 0);
    assert_eq!(table_count(&database, "work_results", "id").await, 0);
    assert_eq!(
        table_count(&database, "work_job_retention_queue", "job_id").await,
        0
    );
}

#[tokio::test]
async fn retention_mixes_deleted_and_deferred_jobs_in_one_batch() {
    let database = database().await;
    let terminal_at = Utc::now() - Duration::days(31);
    let jobs =
        WorkJobRepository::with_clock(&database, ManualClock(Arc::new(Mutex::new(terminal_at))));
    let sync = jobs
        .enqueue_or_join(
            &WorkJobSpec::new(
                WorkTaskKind::ScopedStorageSync,
                WorkScope::StorageObject(StorageObjectRecordId::new()),
                1,
                100,
            )
            .unwrap(),
        )
        .await
        .unwrap();
    jobs.enqueue_or_join(
        &WorkJobSpec::new(
            WorkTaskKind::ExpandItem,
            WorkScope::CatalogItem(CatalogItemId::new()),
            1,
            100,
        )
        .unwrap()
        .with_pending_required_sync(sync.job().id()),
    )
    .await
    .unwrap();
    let sync_claim = jobs
        .claim_next(
            &[WorkTaskKind::ScopedStorageSync],
            "retention-mixed-sync",
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    jobs.fail_terminal(&sync_claim, "mixed fixture")
        .await
        .unwrap();
    let probe = jobs
        .enqueue_or_join(
            &WorkJobSpec::new(
                WorkTaskKind::ProbeMedia,
                WorkScope::CatalogItem(CatalogItemId::new()),
                1,
                100,
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let probe_claim = jobs
        .claim_next(
            &[WorkTaskKind::ProbeMedia],
            "retention-mixed-probe",
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(probe_claim.id(), probe.job().id());
    jobs.fail_terminal(&probe_claim, "mixed fixture")
        .await
        .unwrap();

    assert_eq!(
        WorkRetentionRepository::new(&database)
            .run_once(
                "retention-worker",
                Duration::days(30),
                Duration::seconds(30),
            )
            .await
            .unwrap(),
        WorkRetentionRun::Processed {
            deleted: 1,
            compacted: 0,
            purged: 0,
            deferred: 1,
        }
    );
    assert_eq!(table_count(&database, "work_jobs", "id").await, 2);
    assert_eq!(table_count(&database, "work_results", "id").await, 1);
    assert_eq!(
        table_count(&database, "work_job_retention_queue", "job_id").await,
        1
    );
}

#[tokio::test]
async fn legacy_terminal_job_is_enrolled_then_deleted() {
    let database = database().await;
    let terminal_at = Utc::now() - Duration::days(31);
    let jobs =
        WorkJobRepository::with_clock(&database, ManualClock(Arc::new(Mutex::new(terminal_at))));
    let submitted = jobs
        .enqueue_or_join(
            &WorkJobSpec::new(
                WorkTaskKind::ProbeMedia,
                WorkScope::CatalogItem(CatalogItemId::new()),
                1,
                100,
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let claimed = jobs
        .claim_next(
            &[WorkTaskKind::ProbeMedia],
            "legacy-retention-contract",
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    jobs.fail_terminal(&claimed, "legacy fixture")
        .await
        .unwrap();
    let backend = database.get_database_backend();
    database
        .execute(
            backend.build(
                Query::delete()
                    .from_table(Alias::new("work_job_retention_queue"))
                    .and_where(Expr::col(Alias::new("job_id")).eq(submitted.job().id().as_uuid())),
            ),
        )
        .await
        .unwrap();
    let retention = WorkRetentionRepository::new(&database);

    assert_eq!(
        retention
            .run_once(
                "retention-worker",
                Duration::days(30),
                Duration::seconds(30),
            )
            .await
            .unwrap(),
        WorkRetentionRun::EnrolledLegacy { count: 1 }
    );
    assert_eq!(
        retention
            .run_once(
                "retention-worker",
                Duration::days(30),
                Duration::seconds(30),
            )
            .await
            .unwrap(),
        WorkRetentionRun::Processed {
            deleted: 1,
            compacted: 0,
            purged: 0,
            deferred: 0,
        }
    );
    assert_eq!(table_count(&database, "work_jobs", "id").await, 0);
    assert_eq!(table_count(&database, "work_results", "id").await, 0);
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Keeps the publication retention lifecycle in one fixture.
async fn published_job_is_compacted_once_without_legacy_reenrollment() {
    let database = database().await;
    let terminal_at = Utc::now() - Duration::days(31);
    let item = CatalogItemId::new();
    let backend = database.get_database_backend();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("catalog_items"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("item_type"),
                        Alias::new("name"),
                        Alias::new("sort_name"),
                        Alias::new("classification_state"),
                        Alias::new("metadata_state"),
                        Alias::new("structure_state"),
                        Alias::new("source_state"),
                        Alias::new("structure_expansion_revision"),
                        Alias::new("source_index_revision"),
                        Alias::new("is_present"),
                    ])
                    .values_panic([
                        item.as_uuid().into(),
                        "Movie".into(),
                        "Published item".into(),
                        "published item".into(),
                        "Matched".into(),
                        "Ready".into(),
                        "Expanded".into(),
                        "Ready".into(),
                        1_i64.into(),
                        1_i64.into(),
                        true.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    let jobs =
        WorkJobRepository::with_clock(&database, ManualClock(Arc::new(Mutex::new(terminal_at))));
    let submitted = jobs
        .enqueue_or_join(
            &WorkJobSpec::new(
                WorkTaskKind::ProbeMedia,
                WorkScope::CatalogItem(item),
                1,
                100,
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let claimed = jobs
        .claim_next(
            &[WorkTaskKind::ProbeMedia],
            "published-retention-contract",
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    jobs.fail_terminal(&claimed, "published fixture")
        .await
        .unwrap();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("catalog_publications"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("job_id"),
                        Alias::new("owner_catalog_item_id"),
                        Alias::new("publication_kind"),
                        Alias::new("expected_revision"),
                        Alias::new("state"),
                        Alias::new("manifest_sha256"),
                        Alias::new("expected_row_count"),
                    ])
                    .values_panic([
                        uuid::Uuid::new_v4().into(),
                        submitted.job().id().as_uuid().into(),
                        item.as_uuid().into(),
                        "Sources".into(),
                        1_i64.into(),
                        "Active".into(),
                        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
                        0_i64.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    let retention = WorkRetentionRepository::new(&database);

    assert_eq!(
        retention
            .run_once(
                "retention-worker",
                Duration::days(30),
                Duration::seconds(30),
            )
            .await
            .unwrap(),
        WorkRetentionRun::Processed {
            deleted: 0,
            compacted: 1,
            purged: 0,
            deferred: 0,
        }
    );
    assert_eq!(table_count(&database, "work_results", "id").await, 0);
    assert_eq!(
        retention
            .run_once(
                "retention-worker",
                Duration::days(30),
                Duration::seconds(30),
            )
            .await
            .unwrap(),
        WorkRetentionRun::Idle
    );
    assert_eq!(table_count(&database, "work_jobs", "id").await, 1);
    assert_eq!(
        table_count(&database, "work_job_retention_queue", "job_id").await,
        0
    );
}

async fn table_count(database: &DatabaseConnection, table: &str, column: &str) -> i64 {
    let backend = database.get_database_backend();
    database
        .query_one(
            backend.build(
                Query::select()
                    .expr_as(Expr::col(Alias::new(column)).count(), Alias::new("count"))
                    .from(Alias::new(table)),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get("", "count")
        .unwrap()
}

/// Seeds a terminal job plus a publication in the given state and returns the
/// job id and publication id. The publication owns one outbox change event.
#[allow(clippy::too_many_lines)] // One seeding flow per classification scenario keeps the fixtures readable.
async fn seed_job_with_publication(
    database: &DatabaseConnection,
    publication_state: &str,
    referenced_by_item: bool,
) -> (uuid::Uuid, uuid::Uuid) {
    let terminal_at = Utc::now() - Duration::days(31);
    let jobs =
        WorkJobRepository::with_clock(database, ManualClock(Arc::new(Mutex::new(terminal_at))));
    let submitted = jobs
        .enqueue_or_join(
            &WorkJobSpec::new(
                WorkTaskKind::IndexMediaSources,
                WorkScope::CatalogItem(CatalogItemId::new()),
                1,
                100,
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let job_id = submitted.job().id().as_uuid();
    let claimed = jobs
        .claim_next(
            &[WorkTaskKind::IndexMediaSources],
            "retention-contract",
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    jobs.fail_terminal(&claimed, "fixture failure")
        .await
        .unwrap();

    let backend = database.get_database_backend();
    let item_id = uuid::Uuid::new_v4();
    let publication_id = uuid::Uuid::new_v4();
    database
        .execute(
            backend.build(
                &Query::insert()
                    .into_table(Alias::new("catalog_items"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("item_type"),
                        Alias::new("name"),
                        Alias::new("sort_name"),
                        Alias::new("classification_state"),
                        Alias::new("metadata_state"),
                        Alias::new("structure_state"),
                        Alias::new("source_state"),
                        Alias::new("structure_expansion_revision"),
                        Alias::new("source_index_revision"),
                        Alias::new("is_present"),
                        Alias::new("active_source_publication_id"),
                    ])
                    .values_panic([
                        item_id.into(),
                        "Movie".into(),
                        "Retention Purge".into(),
                        "retention purge".into(),
                        "Matched".into(),
                        "Ready".into(),
                        "Unexpanded".into(),
                        "Unknown".into(),
                        0_i64.into(),
                        0_i64.into(),
                        true.into(),
                        referenced_by_item.then_some(publication_id).into(),
                    ])
                    .to_owned(),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            backend.build(
                &Query::insert()
                    .into_table(Alias::new("catalog_publications"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("job_id"),
                        Alias::new("owner_catalog_item_id"),
                        Alias::new("publication_kind"),
                        Alias::new("expected_revision"),
                        Alias::new("state"),
                        Alias::new("manifest_sha256"),
                        Alias::new("expected_row_count"),
                        Alias::new("created_at"),
                    ])
                    .values_panic([
                        publication_id.into(),
                        job_id.into(),
                        item_id.into(),
                        "Source".into(),
                        0_i64.into(),
                        publication_state.into(),
                        "0000000000000000000000000000000000000000000000000000000000000000".into(),
                        0_i64.into(),
                        terminal_at.into(),
                    ])
                    .to_owned(),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            backend.build(
                &Query::insert()
                    .into_table(Alias::new("catalog_change_outbox"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("generation"),
                        Alias::new("event_type"),
                        Alias::new("catalog_item_id"),
                        Alias::new("publication_id"),
                        Alias::new("created_at"),
                    ])
                    .values_panic([
                        uuid::Uuid::new_v4().into(),
                        1_i64.into(),
                        "SourcesChanged".into(),
                        item_id.into(),
                        publication_id.into(),
                        terminal_at.into(),
                    ])
                    .to_owned(),
            ),
        )
        .await
        .unwrap();
    (job_id, publication_id)
}

#[tokio::test]
async fn retired_unreferenced_publications_are_purged_with_their_job() {
    let database = database().await;
    let (job_id, publication_id) = seed_job_with_publication(&database, "Retired", false).await;

    let run = WorkRetentionRepository::new(&database)
        .run_once(
            "retention-worker",
            Duration::days(30),
            Duration::seconds(30),
        )
        .await
        .unwrap();

    assert_eq!(
        run,
        WorkRetentionRun::Processed {
            deleted: 1,
            compacted: 0,
            purged: 1,
            deferred: 0,
        }
    );
    assert_eq!(
        table_count(&database, "catalog_publications", "id").await,
        0
    );
    assert_eq!(
        table_count(&database, "catalog_change_outbox", "id").await,
        0
    );
    assert_eq!(
        table_count(&database, "publication_media_sources", "id").await,
        0
    );
    assert_eq!(table_count(&database, "work_jobs", "id").await, 0);
    assert_eq!(
        table_count(&database, "work_job_retention_queue", "job_id").await,
        0
    );
    assert!(!job_id.is_nil());
    assert!(!publication_id.is_nil());
}

#[tokio::test]
async fn retired_publications_still_referenced_by_active_pointers_are_compacted() {
    let database = database().await;
    let (job_id, publication_id) = seed_job_with_publication(&database, "Retired", true).await;

    let run = WorkRetentionRepository::new(&database)
        .run_once(
            "retention-worker",
            Duration::days(30),
            Duration::seconds(30),
        )
        .await
        .unwrap();

    assert_eq!(
        run,
        WorkRetentionRun::Processed {
            deleted: 0,
            compacted: 1,
            purged: 0,
            deferred: 0,
        }
    );
    assert_eq!(
        table_count(&database, "catalog_publications", "id").await,
        1,
        "active publications must survive retention"
    );
    assert_eq!(table_count(&database, "work_jobs", "id").await, 1);
    assert_eq!(
        table_count(&database, "work_job_retention_queue", "job_id").await,
        0
    );
    assert!(!job_id.is_nil());
    assert!(!publication_id.is_nil());
}

#[tokio::test]
async fn ready_publications_are_compacted_not_purged() {
    let database = database().await;
    let (_job_id, _publication_id) = seed_job_with_publication(&database, "Ready", false).await;

    let run = WorkRetentionRepository::new(&database)
        .run_once(
            "retention-worker",
            Duration::days(30),
            Duration::seconds(30),
        )
        .await
        .unwrap();

    assert_eq!(
        run,
        WorkRetentionRun::Processed {
            deleted: 0,
            compacted: 1,
            purged: 0,
            deferred: 0,
        }
    );
    assert_eq!(
        table_count(&database, "catalog_publications", "id").await,
        1
    );
    assert_eq!(table_count(&database, "work_jobs", "id").await, 1);
}

#[tokio::test]
async fn compacted_publication_is_reenrolled_only_after_retirement_and_reference_release() {
    let database = database().await;
    let (_, publication_id) = seed_job_with_publication(&database, "Active", true).await;
    let retention = WorkRetentionRepository::new(&database);
    let run = retention
        .run_once("worker", Duration::days(30), Duration::seconds(30))
        .await
        .unwrap();
    assert!(matches!(
        run,
        WorkRetentionRun::Processed { compacted: 1, .. }
    ));
    assert_eq!(
        retention
            .run_once("worker", Duration::days(30), Duration::seconds(30))
            .await
            .unwrap(),
        WorkRetentionRun::Idle
    );
    let backend = database.get_database_backend();
    database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("catalog_publications"))
                    .value(Alias::new("state"), "Retired")
                    .and_where(Expr::col(Alias::new("id")).eq(publication_id)),
            ),
        )
        .await
        .unwrap();
    assert_eq!(
        retention
            .run_once("worker", Duration::days(30), Duration::seconds(30))
            .await
            .unwrap(),
        WorkRetentionRun::Idle
    );
    database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("catalog_items"))
                    .value(
                        Alias::new("active_source_publication_id"),
                        Option::<uuid::Uuid>::None,
                    )
                    .and_where(
                        Expr::col(Alias::new("active_source_publication_id")).eq(publication_id),
                    ),
            ),
        )
        .await
        .unwrap();
    assert_eq!(
        retention
            .run_once("worker", Duration::days(30), Duration::seconds(30))
            .await
            .unwrap(),
        WorkRetentionRun::EnrolledLegacy { count: 1 }
    );
    assert!(matches!(
        retention
            .run_once("worker", Duration::days(30), Duration::seconds(30))
            .await
            .unwrap(),
        WorkRetentionRun::Processed {
            purged: 1,
            deleted: 1,
            ..
        }
    ));
    assert_eq!(
        table_count(&database, "catalog_publications", "id").await,
        0
    );
    assert_eq!(
        retention
            .run_once("worker", Duration::days(30), Duration::seconds(30))
            .await
            .unwrap(),
        WorkRetentionRun::Idle
    );
}

#[tokio::test]
async fn oversized_job_is_trimmed_in_bounded_batches_before_parent_deletion() {
    let database = database().await;
    let (job, publication) = seed_job_with_publication(&database, "Retired", false).await;
    let backend = database.get_database_backend();
    for index in 0..501 {
        database
            .execute(
                backend.build(
                    Query::insert()
                        .into_table(Alias::new("work_staging_rows"))
                        .columns([
                            "id",
                            "job_id",
                            "publication_id",
                            "entity_kind",
                            "natural_key",
                            "payload",
                            "validation_state",
                        ])
                        .values_panic([
                            uuid::Uuid::new_v4().into(),
                            job.into(),
                            publication.into(),
                            "Fixture".into(),
                            index.to_string().into(),
                            serde_json::json!({}).into(),
                            "Valid".into(),
                        ]),
                ),
            )
            .await
            .unwrap();
    }
    let retention = WorkRetentionRepository::new(&database);
    assert!(matches!(
        retention
            .run_once("batch-worker", Duration::days(30), Duration::seconds(30))
            .await
            .unwrap(),
        WorkRetentionRun::Processed {
            deleted: 0,
            deferred: 1,
            ..
        }
    ));
    assert_eq!(table_count(&database, "work_staging_rows", "id").await, 1);
    assert_eq!(table_count(&database, "work_jobs", "id").await, 1);
    // Advance the persisted retry eligibility without sleeping or changing its original terminal age.
    database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("work_job_retention_queue"))
                    .value(
                        Alias::new("available_at"),
                        Utc::now() - Duration::seconds(1),
                    )
                    .and_where(Expr::col(Alias::new("job_id")).eq(job)),
            ),
        )
        .await
        .unwrap();
    assert!(matches!(
        retention
            .run_once("batch-worker", Duration::days(30), Duration::seconds(30))
            .await
            .unwrap(),
        WorkRetentionRun::Processed {
            deleted: 1,
            purged: 1,
            ..
        }
    ));
    assert_eq!(table_count(&database, "work_staging_rows", "id").await, 0);
    assert_eq!(
        table_count(&database, "catalog_publications", "id").await,
        0
    );
}

#[tokio::test]
async fn health_counts_real_rows_and_keeps_unavailable_space_unknown() {
    let database = database().await;
    seed_job_with_publication(&database, "Retired", false).await;
    let health = tjxy_db::sample_work_health(&database, Some(Duration::days(30)))
        .await
        .unwrap();
    assert_eq!(
        health
            .tables
            .iter()
            .find(|table| table.name == "catalog_items")
            .unwrap()
            .rows,
        1
    );
    assert_eq!(
        health
            .tables
            .iter()
            .find(|table| table.name == "work_jobs")
            .unwrap()
            .rows,
        1
    );
    assert_eq!(health.retention_candidates, Some(1));
    assert_eq!(health.pending_jobs, 0);
    assert!(health.oldest_pending_at.is_none());
    if database.get_database_backend() == sea_orm::DbBackend::Sqlite {
        assert!(health.allocated_bytes.unwrap() > 0);
        assert_eq!(health.wal_bytes, None);
    }
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Covers the real user/session foreign keys and both retention outcomes.
async fn retired_publication_survives_active_playback_then_is_purged_after_stop() {
    let database = database().await;
    let (_, publication) = seed_job_with_publication(&database, "Retired", false).await;
    let backend = database.get_database_backend();
    let item: uuid::Uuid = database
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("owner_catalog_item_id"))
                    .from(Alias::new("catalog_publications"))
                    .and_where(Expr::col(Alias::new("id")).eq(publication)),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get("", "owner_catalog_item_id")
        .unwrap();
    let now = Utc::now();
    let auth = tjxy_db::AuthRepository::new(&database);
    let user = auth
        .create_user(
            &tjxy_common::Username::parse("retention-player").unwrap(),
            "$argon2id$test-only",
            false,
            false,
            now,
        )
        .await
        .unwrap();
    let session = auth
        .issue_session_for_user(
            user.id(),
            user.auth_revision(),
            tjxy_db::SessionDraft {
                id: uuid::Uuid::new_v4(),
                token_digest: [7; 32],
                device_id: "retention".into(),
                device_name: "retention".into(),
                client_name: "retention".into(),
                client_version: "1".into(),
                created_at: now,
                expires_at: Some(now + Duration::hours(1)),
            },
        )
        .await
        .unwrap();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("playback_sessions"))
                    .columns([
                        "id",
                        "auth_session_id",
                        "play_session_id",
                        "user_id",
                        "catalog_item_id",
                        "presentation_key",
                        "last_position_ticks",
                        "started_at",
                        "last_event_at",
                    ])
                    .values_panic([
                        uuid::Uuid::new_v4().into(),
                        session.id().into(),
                        uuid::Uuid::new_v4().into(),
                        user.id().as_uuid().into(),
                        item.into(),
                        uuid::Uuid::new_v4().into(),
                        0_i64.into(),
                        now.into(),
                        now.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    let retention = WorkRetentionRepository::new(&database);
    assert_eq!(
        retention
            .run_once(
                "playback-retention",
                Duration::days(30),
                Duration::seconds(30)
            )
            .await
            .unwrap(),
        WorkRetentionRun::Processed {
            deleted: 0,
            compacted: 0,
            purged: 0,
            deferred: 1
        }
    );
    assert_eq!(
        table_count(&database, "catalog_publications", "id").await,
        1
    );
    database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("playback_sessions"))
                    .value(Alias::new("stopped_at"), now),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("work_job_retention_queue"))
                    .value(Alias::new("available_at"), now - Duration::seconds(1)),
            ),
        )
        .await
        .unwrap();
    assert_eq!(
        retention
            .run_once(
                "playback-retention",
                Duration::days(30),
                Duration::seconds(30)
            )
            .await
            .unwrap(),
        WorkRetentionRun::Processed {
            deleted: 1,
            compacted: 0,
            purged: 1,
            deferred: 0
        }
    );
    assert_eq!(
        table_count(&database, "catalog_publications", "id").await,
        0
    );
}
