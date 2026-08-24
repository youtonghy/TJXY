use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use chrono::{Duration, TimeZone, Utc};
use sea_orm::{
    ConnectionTrait, DatabaseConnection, TransactionTrait,
    sea_query::{Alias, Expr, Func, JoinType, Order, Query},
};
use sea_orm_migration::MigratorTrait;
use serde_json::json;
use sha2::{Digest, Sha256};
use tjxy_common::{CatalogItemId, SortKey, StorageObjectRecordId, StorageRootId};
use tjxy_db::{
    OutboxClock, OutboxRepository, StorageChangeFeedRepository, StorageChangeProjectionRepository,
    StorageSyncPage, StorageSyncRepository, StorageSyncRepositoryError,
    TemporaryAvailabilityReason, WorkJobClock, WorkJobRepository, WorkJobResult, WorkJobSpec,
    WorkScope, WorkTaskKind,
};
use tjxy_storage::{
    ChangeCursor, ChangePage, IdentityQuality, StorageChange, StorageObject, StorageObjectId,
};
use tjxy_test_support::test_database;
use uuid::Uuid;

#[derive(Clone)]
struct ManualClock(Arc<Mutex<chrono::DateTime<Utc>>>);

impl ManualClock {
    fn new(now: chrono::DateTime<Utc>) -> Self {
        Self(Arc::new(Mutex::new(now)))
    }

    fn set(&self, now: chrono::DateTime<Utc>) {
        *self.0.lock().unwrap() = now;
    }
}

impl WorkJobClock for ManualClock {
    fn now(&self) -> chrono::DateTime<Utc> {
        *self.0.lock().unwrap()
    }
}

impl OutboxClock for ManualClock {
    fn now(&self) -> chrono::DateTime<Utc> {
        *self.0.lock().unwrap()
    }
}

#[derive(Clone)]
struct AdvancingClock(Arc<Mutex<VecDeque<chrono::DateTime<Utc>>>>);

impl WorkJobClock for AdvancingClock {
    fn now(&self) -> chrono::DateTime<Utc> {
        self.0.lock().unwrap().pop_front().unwrap()
    }
}

struct Fixture {
    database: DatabaseConnection,
    library_id: Uuid,
    account_id: Uuid,
    root_id: StorageRootId,
    parent_id: StorageObjectRecordId,
}

fn identity_key(provider_drive_id: &str, provider_object_id: &str) -> String {
    let mut digest = Sha256::new();
    for part in [provider_drive_id, provider_object_id] {
        digest.update((part.len() as u64).to_be_bytes());
        digest.update(part.as_bytes());
    }
    format!("{:x}", digest.finalize())
}

#[allow(clippy::too_many_lines)] // The SQL fixture mirrors the normalized storage boundary.
async fn fixture() -> Fixture {
    let database = test_database().await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    let account_id = Uuid::new_v4();
    let library_id = Uuid::new_v4();
    let root_id = StorageRootId::new();
    let parent_id = StorageObjectRecordId::new();
    let backend = database.get_database_backend();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("storage_accounts"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("provider"),
                        Alias::new("display_name"),
                        Alias::new("account_identity"),
                        Alias::new("credential_ref"),
                        Alias::new("status"),
                    ])
                    .values_panic([
                        account_id.into(),
                        "filesystem".into(),
                        "Fixture".into(),
                        Uuid::new_v4().to_string().into(),
                        "fixture".into(),
                        "Active".into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("libraries"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("name"),
                        Alias::new("scan_profile"),
                        Alias::new("object_selection_scope"),
                        Alias::new("metadata_policy"),
                        Alias::new("expansion_policy"),
                        Alias::new("probe_policy"),
                        Alias::new("profile_version"),
                        Alias::new("collection_type"),
                        Alias::new("sort_key"),
                        Alias::new("is_enabled"),
                    ])
                    .values_panic([
                        library_id.into(),
                        "Fixture".into(),
                        "Lazy".into(),
                        "title_layer".into(),
                        "basic".into(),
                        "on_browse".into(),
                        "on_playback".into(),
                        1.into(),
                        "movies".into(),
                        SortKey::from_text("Fixture").into_bytes().into(),
                        true.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("storage_roots"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("storage_account_id"),
                        Alias::new("provider_root_id"),
                        Alias::new("sync_revision"),
                        Alias::new("reconciled_sync_revision"),
                    ])
                    .values_panic([
                        root_id.as_uuid().into(),
                        account_id.into(),
                        "root".into(),
                        0_i64.into(),
                        0_i64.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("library_storage_roots"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("library_id"),
                        Alias::new("storage_root_id"),
                    ])
                    .values_panic([
                        Uuid::new_v4().into(),
                        library_id.into(),
                        root_id.as_uuid().into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("storage_objects"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("storage_account_id"),
                        Alias::new("provider_drive_id"),
                        Alias::new("provider_object_id"),
                        Alias::new("identity_key"),
                        Alias::new("name"),
                        Alias::new("normalized_name"),
                        Alias::new("object_type"),
                        Alias::new("observed_sync_revision"),
                        Alias::new("children_indexed"),
                        Alias::new("children_index_revision"),
                        Alias::new("identity_quality"),
                        Alias::new("presence_state"),
                    ])
                    .values_panic([
                        parent_id.as_uuid().into(),
                        account_id.into(),
                        "fixture-drive".into(),
                        "root".into(),
                        identity_key("fixture-drive", "root").into(),
                        "Root".into(),
                        "root".into(),
                        "Directory".into(),
                        0_i64.into(),
                        false.into(),
                        0_i64.into(),
                        "StableFileId".into(),
                        "Present".into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("storage_root_objects"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("storage_root_id"),
                        Alias::new("storage_object_id"),
                        Alias::new("observed_sync_revision"),
                        Alias::new("children_indexed"),
                        Alias::new("children_index_revision"),
                        Alias::new("presence_state"),
                    ])
                    .values_panic([
                        Uuid::new_v4().into(),
                        root_id.as_uuid().into(),
                        parent_id.as_uuid().into(),
                        0_i64.into(),
                        false.into(),
                        0_i64.into(),
                        "Present".into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    Fixture {
        database,
        library_id,
        account_id,
        root_id,
        parent_id,
    }
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Keeps both root revisions and their outbox rows in one contract.
async fn ordinary_object_reads_update_each_root_local_availability_revision() {
    let fixture = fixture().await;
    let second_root = StorageRootId::new();
    let backend = fixture.database.get_database_backend();
    fixture
        .database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("storage_roots"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("storage_account_id"),
                        Alias::new("provider_root_id"),
                        Alias::new("sync_revision"),
                        Alias::new("reconciled_sync_revision"),
                    ])
                    .values_panic([
                        second_root.as_uuid().into(),
                        fixture.account_id.into(),
                        "second-root".into(),
                        0_i64.into(),
                        0_i64.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    fixture
        .database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("storage_root_objects"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("storage_root_id"),
                        Alias::new("storage_object_id"),
                        Alias::new("observed_sync_revision"),
                        Alias::new("children_indexed"),
                        Alias::new("children_index_revision"),
                        Alias::new("presence_state"),
                    ])
                    .values_panic([
                        Uuid::new_v4().into(),
                        second_root.as_uuid().into(),
                        fixture.parent_id.as_uuid().into(),
                        0_i64.into(),
                        false.into(),
                        0_i64.into(),
                        "Present".into(),
                    ]),
            ),
        )
        .await
        .unwrap();

    let repository = StorageSyncRepository::new(&fixture.database);
    let unavailable = repository
        .record_object_read_unavailable(
            fixture.parent_id,
            TemporaryAvailabilityReason::BackendRateLimited,
        )
        .await
        .unwrap();
    assert_eq!(unavailable.len(), 2);
    assert!(unavailable.iter().all(|update| update.sync_revision() == 1));
    assert_eq!(
        unavailable
            .iter()
            .map(|update| update.root_id())
            .collect::<std::collections::HashSet<_>>(),
        [fixture.root_id, second_root].into_iter().collect()
    );

    let repeated = repository
        .record_object_read_unavailable(
            fixture.parent_id,
            TemporaryAvailabilityReason::BackendRateLimited,
        )
        .await
        .unwrap();
    assert!(repeated.is_empty());

    let restored = repository
        .record_object_read_present(fixture.parent_id)
        .await
        .unwrap();
    assert_eq!(restored.len(), 2);
    assert!(restored.iter().all(|update| update.sync_revision() == 2));

    let rows = fixture
        .database
        .query_all(
            fixture.database.get_database_backend().build(
                Query::select()
                    .columns([
                        (Alias::new("root_object"), Alias::new("storage_root_id")),
                        (Alias::new("root_object"), Alias::new("presence_state")),
                        (Alias::new("root_object"), Alias::new("availability_reason")),
                        (Alias::new("root"), Alias::new("sync_revision")),
                    ])
                    .expr_as(
                        Func::count(Expr::col((Alias::new("outbox"), Alias::new("id")))),
                        Alias::new("outbox_count"),
                    )
                    .from_as(
                        Alias::new("storage_root_objects"),
                        Alias::new("root_object"),
                    )
                    .join_as(
                        JoinType::InnerJoin,
                        Alias::new("storage_roots"),
                        Alias::new("root"),
                        Expr::col((Alias::new("root"), Alias::new("id")))
                            .equals((Alias::new("root_object"), Alias::new("storage_root_id"))),
                    )
                    .join_as(
                        JoinType::LeftJoin,
                        Alias::new("storage_change_outbox"),
                        Alias::new("outbox"),
                        Expr::col((Alias::new("outbox"), Alias::new("storage_root_id")))
                            .equals((Alias::new("root_object"), Alias::new("storage_root_id"))),
                    )
                    .and_where(
                        Expr::col((Alias::new("root_object"), Alias::new("storage_object_id")))
                            .eq(fixture.parent_id.as_uuid()),
                    )
                    .group_by_columns([
                        (Alias::new("root_object"), Alias::new("storage_root_id")),
                        (Alias::new("root_object"), Alias::new("presence_state")),
                        (Alias::new("root_object"), Alias::new("availability_reason")),
                        (Alias::new("root"), Alias::new("sync_revision")),
                    ])
                    .order_by(
                        (Alias::new("root_object"), Alias::new("storage_root_id")),
                        Order::Asc,
                    ),
            ),
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);
    for row in rows {
        assert_eq!(
            row.try_get::<String>("", "presence_state").unwrap(),
            "Present"
        );
        assert_eq!(
            row.try_get::<Option<String>>("", "availability_reason")
                .unwrap(),
            None
        );
        assert_eq!(row.try_get::<i64>("", "sync_revision").unwrap(), 2);
        assert_eq!(row.try_get::<i64>("", "outbox_count").unwrap(), 2);
    }
}

#[tokio::test]
async fn claimed_scope_resolves_to_a_redacted_inventory_target() {
    let fixture = fixture().await;
    let now = Utc.with_ymd_and_hms(2026, 7, 17, 1, 0, 0).unwrap();
    let claimed = claimed_sync_job(&fixture, ManualClock::new(now)).await;

    let target = StorageSyncRepository::new(&fixture.database)
        .inventory_target(&claimed)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(target.account_id(), fixture.account_id);
    assert_eq!(target.root_id(), fixture.root_id);
    assert_eq!(target.parent_record_id(), fixture.parent_id);
    assert_eq!(target.provider_drive_id(), "fixture-drive");
    assert_eq!(target.backend_parent_id().provider(), "filesystem");
    assert_eq!(target.backend_parent_id().provider_object_id(), "root");
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // The pre-affinity graph covers unique recovery and ambiguous fail-closed behavior.
async fn inventory_target_uses_exact_affinity_and_rejects_ambiguous_legacy_work() {
    let fixture = fixture().await;
    let second_root = StorageRootId::new();
    let backend = fixture.database.get_database_backend();
    fixture
        .database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("storage_roots"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("storage_account_id"),
                        Alias::new("provider_root_id"),
                        Alias::new("sync_revision"),
                        Alias::new("reconciled_sync_revision"),
                    ])
                    .values_panic([
                        second_root.as_uuid().into(),
                        fixture.account_id.into(),
                        "second-root".into(),
                        0_i64.into(),
                        0_i64.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    for statement in [
        Query::insert()
            .into_table(Alias::new("library_storage_roots"))
            .columns([
                Alias::new("id"),
                Alias::new("library_id"),
                Alias::new("storage_root_id"),
            ])
            .values_panic([
                Uuid::new_v4().into(),
                fixture.library_id.into(),
                second_root.as_uuid().into(),
            ])
            .to_owned(),
        Query::insert()
            .into_table(Alias::new("storage_root_objects"))
            .columns([
                Alias::new("id"),
                Alias::new("storage_root_id"),
                Alias::new("storage_object_id"),
                Alias::new("observed_sync_revision"),
                Alias::new("children_indexed"),
                Alias::new("children_index_revision"),
                Alias::new("presence_state"),
            ])
            .values_panic([
                Uuid::new_v4().into(),
                second_root.as_uuid().into(),
                fixture.parent_id.as_uuid().into(),
                0_i64.into(),
                false.into(),
                0_i64.into(),
                "Present".into(),
            ])
            .to_owned(),
    ] {
        fixture
            .database
            .execute(backend.build(&statement))
            .await
            .unwrap();
    }
    let jobs = WorkJobRepository::new(&fixture.database);
    jobs.enqueue_or_join(
        &WorkJobSpec::new(
            WorkTaskKind::ScopedStorageSync,
            WorkScope::StorageObject(fixture.parent_id),
            0,
            100,
        )
        .unwrap()
        .with_storage_root_affinity(second_root)
        .unwrap(),
    )
    .await
    .unwrap();
    let exact = jobs
        .claim_next(
            &[WorkTaskKind::ScopedStorageSync],
            "exact-root-worker",
            Duration::seconds(30),
        )
        .await
        .unwrap()
        .unwrap();
    let target = StorageSyncRepository::new(&fixture.database)
        .inventory_target(&exact)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(target.root_id(), second_root);

    jobs.enqueue_or_join(
        &WorkJobSpec::new(
            WorkTaskKind::ScopedStorageSync,
            WorkScope::StorageObject(fixture.parent_id),
            1,
            100,
        )
        .unwrap(),
    )
    .await
    .unwrap();
    let legacy = jobs
        .claim_next(
            &[WorkTaskKind::ScopedStorageSync],
            "legacy-worker",
            Duration::seconds(30),
        )
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        StorageSyncRepository::new(&fixture.database)
            .inventory_target(&legacy)
            .await
            .unwrap_err(),
        StorageSyncRepositoryError::AmbiguousScope
    ));
}

#[tokio::test]
async fn inventory_target_and_commit_fail_closed_after_library_revocation() {
    let fixture = fixture().await;
    let now = Utc.with_ymd_and_hms(2026, 7, 17, 1, 30, 0).unwrap();
    let clock = ManualClock::new(now);
    let claimed = claimed_sync_job(&fixture, clock.clone()).await;
    let backend = fixture.database.get_database_backend();
    fixture
        .database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("libraries"))
                    .value(Alias::new("is_enabled"), false)
                    .and_where(Expr::col(Alias::new("id")).eq(fixture.library_id)),
            ),
        )
        .await
        .unwrap();

    let repository = StorageSyncRepository::with_clock(&fixture.database, clock);
    assert!(
        repository
            .inventory_target(&claimed)
            .await
            .unwrap()
            .is_none()
    );
    let error = repository
        .commit_inventory_page(
            &claimed,
            StorageSyncPage::new(
                fixture.root_id,
                fixture.parent_id,
                "fixture-drive",
                "revoked-binding-page",
                Vec::new(),
                true,
            )
            .unwrap(),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, StorageSyncRepositoryError::MissingScope));
}

#[tokio::test]
async fn scoped_sync_claim_is_filtered_by_storage_account() {
    let fixture = fixture().await;
    let now = Utc.with_ymd_and_hms(2026, 7, 17, 1, 0, 0).unwrap();
    let clock = ManualClock::new(now);
    let jobs = WorkJobRepository::with_clock(&fixture.database, clock);
    jobs.enqueue_or_join(
        &WorkJobSpec::new(
            WorkTaskKind::ScopedStorageSync,
            WorkScope::StorageObject(fixture.parent_id),
            0,
            100,
        )
        .unwrap(),
    )
    .await
    .unwrap();

    assert!(
        jobs.claim_next_scoped_sync(Uuid::new_v4(), "wrong-account", Duration::minutes(5))
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        jobs.claim_next_scoped_sync_for_drive(
            fixture.account_id,
            "other-drive",
            "wrong-drive",
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .is_none()
    );
    let claimed = jobs
        .claim_next_scoped_sync_for_drive(
            fixture.account_id,
            "fixture-drive",
            "filesystem-worker",
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        claimed.job().scope(),
        WorkScope::StorageObject(fixture.parent_id)
    );
}

async fn claimed_sync_job(fixture: &Fixture, clock: ManualClock) -> tjxy_db::ClaimedWorkJob {
    let repository = WorkJobRepository::with_clock(&fixture.database, clock);
    repository
        .enqueue_or_join(
            &WorkJobSpec::new(
                WorkTaskKind::ScopedStorageSync,
                WorkScope::StorageObject(fixture.parent_id),
                0,
                100,
            )
            .unwrap()
            .with_storage_root_affinity(fixture.root_id)
            .unwrap(),
        )
        .await
        .unwrap();
    repository
        .claim_next(
            &[WorkTaskKind::ScopedStorageSync],
            "sync-worker",
            Duration::seconds(30),
        )
        .await
        .unwrap()
        .unwrap()
}

fn object(id: &str, name: &str) -> StorageObject {
    object_revision(id, name, &format!("revision-{id}"))
}

fn object_revision(id: &str, name: &str, revision: &str) -> StorageObject {
    StorageObject::file_with_identity(
        StorageObjectId::new("filesystem", id).unwrap(),
        name,
        8,
        IdentityQuality::StableFileId,
    )
    .with_remote_revision(revision)
    .unwrap()
}

#[test]
fn inventory_page_rejects_unbounded_names_and_object_counts() {
    let root = StorageRootId::new();
    let parent = StorageObjectRecordId::new();
    let oversized_name = StorageObject::file(
        StorageObjectId::new("filesystem", "object").unwrap(),
        "x".repeat(2049),
        1,
    );
    assert!(
        StorageSyncPage::new(root, parent, "drive", "page", vec![oversized_name], true,).is_err()
    );
    let objects = (0..10_001)
        .map(|index| object(&format!("object-{index}"), "file"))
        .collect();
    assert!(StorageSyncPage::new(root, parent, "drive", "page", objects, true).is_err());
}

#[tokio::test]
async fn repeated_object_updates_preserve_before_and_after_revision_in_outbox() {
    let fixture = fixture().await;
    let now = Utc.with_ymd_and_hms(2026, 7, 18, 13, 30, 0).unwrap();
    let clock = ManualClock::new(now);
    let claimed = claimed_sync_job(&fixture, clock.clone()).await;
    let repository = StorageSyncRepository::with_clock(&fixture.database, clock);
    for (page_identity, revision, final_page) in [
        ("page-1", "revision-1", false),
        ("page-2", "revision-2", true),
    ] {
        repository
            .commit_inventory_page(
                &claimed,
                StorageSyncPage::new(
                    fixture.root_id,
                    fixture.parent_id,
                    "fixture-drive",
                    page_identity,
                    vec![object_revision("movie", "Movie.mkv", revision)],
                    final_page,
                )
                .unwrap(),
            )
            .await
            .unwrap();
    }

    let rows = fixture
        .database
        .query_all(
            fixture.database.get_database_backend().build(
                Query::select()
                    .columns([
                        Alias::new("before_object_revision"),
                        Alias::new("after_object_revision"),
                        Alias::new("payload"),
                    ])
                    .from(Alias::new("storage_change_outbox"))
                    .and_where(Expr::col(Alias::new("event_type")).eq("Upserted"))
                    .order_by(Alias::new("sync_revision"), Order::Asc),
            ),
        )
        .await
        .unwrap();
    assert_eq!(
        rows[0]
            .try_get::<Option<String>>("", "before_object_revision")
            .unwrap(),
        None
    );
    assert_eq!(
        rows[1]
            .try_get::<Option<String>>("", "before_object_revision")
            .unwrap()
            .as_deref(),
        Some("revision-1")
    );
    assert_eq!(
        rows[1]
            .try_get::<Option<String>>("", "after_object_revision")
            .unwrap()
            .as_deref(),
        Some("revision-2")
    );
    let payload = rows[1].try_get::<serde_json::Value>("", "payload").unwrap();
    assert_eq!(
        payload["relation"]["storage_root_id"],
        fixture.root_id.to_string()
    );
    assert_eq!(
        payload["relation"]["parent_storage_object_id"],
        fixture.parent_id.to_string()
    );
    assert_eq!(payload["after"]["provider_object_id"], "movie");
    assert_eq!(payload["after"]["remote_revision"], "revision-2");
    assert_eq!(payload["after"]["presence_state"], "Present");
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // One assertion chain verifies the atomic multi-table commit.
async fn page_commit_atomically_advances_revision_objects_and_outbox_marker() {
    let fixture = fixture().await;
    let now = Utc.with_ymd_and_hms(2026, 7, 18, 13, 0, 0).unwrap();
    let clock = ManualClock::new(now);
    let claimed = claimed_sync_job(&fixture, clock.clone()).await;
    let repository = StorageSyncRepository::with_clock(&fixture.database, clock);
    let backend = fixture.database.get_database_backend();
    fixture
        .database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("storage_root_objects"))
                    .value(Alias::new("children_indexed"), true)
                    .and_where(
                        Expr::col(Alias::new("storage_root_id")).eq(fixture.root_id.as_uuid()),
                    )
                    .and_where(
                        Expr::col(Alias::new("storage_object_id")).eq(fixture.parent_id.as_uuid()),
                    ),
            ),
        )
        .await
        .unwrap();

    let first = repository
        .commit_inventory_page(
            &claimed,
            StorageSyncPage::new(
                fixture.root_id,
                fixture.parent_id,
                "fixture-drive",
                "page-1",
                vec![object("movie", "Movie.mkv")],
                false,
            )
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(first.sync_revision(), 1);
    assert!(!first.scope_completed());
    assert!(!first.replayed());
    let partial_parent = fixture
        .database
        .query_one(
            backend.build(
                Query::select()
                    .columns([
                        Alias::new("children_indexed"),
                        Alias::new("observed_sync_revision"),
                    ])
                    .from(Alias::new("storage_root_objects"))
                    .and_where(
                        Expr::col(Alias::new("storage_root_id")).eq(fixture.root_id.as_uuid()),
                    )
                    .and_where(
                        Expr::col(Alias::new("storage_object_id")).eq(fixture.parent_id.as_uuid()),
                    ),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert!(
        !partial_parent
            .try_get::<bool>("", "children_indexed")
            .unwrap()
    );
    assert_eq!(
        partial_parent
            .try_get::<i64>("", "observed_sync_revision")
            .unwrap(),
        1
    );

    let second = repository
        .commit_inventory_page(
            &claimed,
            StorageSyncPage::new(
                fixture.root_id,
                fixture.parent_id,
                "fixture-drive",
                "page-2",
                Vec::new(),
                true,
            )
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(second.sync_revision(), 2);
    assert!(second.scope_completed());

    let root = fixture
        .database
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("sync_revision"))
                    .from(Alias::new("storage_roots"))
                    .and_where(Expr::col(Alias::new("id")).eq(fixture.root_id.as_uuid())),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(root.try_get::<i64>("", "sync_revision").unwrap(), 2);
    let object_parent = fixture
        .database
        .query_one(
            backend.build(
                Query::select()
                    .columns([
                        Alias::new("provider_parent_id"),
                        Alias::new("facts_observed_storage_root_id"),
                    ])
                    .from(Alias::new("storage_objects"))
                    .and_where(Expr::col(Alias::new("provider_object_id")).eq("movie")),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        object_parent
            .try_get::<String>("", "provider_parent_id")
            .unwrap(),
        "root"
    );
    assert_eq!(
        object_parent
            .try_get::<Uuid>("", "facts_observed_storage_root_id")
            .unwrap(),
        fixture.root_id.as_uuid()
    );
    let parent = fixture
        .database
        .query_one(
            backend.build(
                Query::select()
                    .columns([
                        Alias::new("children_indexed"),
                        Alias::new("children_index_revision"),
                        Alias::new("observed_sync_revision"),
                    ])
                    .from(Alias::new("storage_root_objects"))
                    .and_where(
                        Expr::col(Alias::new("storage_root_id")).eq(fixture.root_id.as_uuid()),
                    )
                    .and_where(
                        Expr::col(Alias::new("storage_object_id")).eq(fixture.parent_id.as_uuid()),
                    ),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert!(parent.try_get::<bool>("", "children_indexed").unwrap());
    assert_eq!(
        parent
            .try_get::<i64>("", "children_index_revision")
            .unwrap(),
        2
    );
    assert_eq!(
        parent.try_get::<i64>("", "observed_sync_revision").unwrap(),
        2
    );
    let event_count: i64 = fixture
        .database
        .query_one(
            fixture.database.get_database_backend().build(
                Query::select()
                    .expr_as(Expr::col(Alias::new("id")).count(), Alias::new("count"))
                    .from(Alias::new("storage_change_outbox")),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get("", "count")
        .unwrap();
    assert_eq!(event_count, 3, "one object event plus one marker per page");
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // The fixture proves both parent projections are durably represented.
async fn inventory_reparent_fences_old_parent_and_emits_both_parent_events() {
    let fixture = fixture().await;
    let backend = fixture.database.get_database_backend();
    let new_parent = StorageObjectRecordId::new();
    let child = StorageObjectRecordId::new();
    for (id, provider_id, name, kind, parent_provider_id) in [
        (new_parent, "new-parent", "New Parent", "Directory", "root"),
        (child, "movie", "Movie.mkv", "File", "root"),
    ] {
        fixture
            .database
            .execute(
                backend.build(
                    Query::insert()
                        .into_table(Alias::new("storage_objects"))
                        .columns([
                            Alias::new("id"),
                            Alias::new("storage_account_id"),
                            Alias::new("provider_drive_id"),
                            Alias::new("provider_object_id"),
                            Alias::new("identity_key"),
                            Alias::new("provider_parent_id"),
                            Alias::new("name"),
                            Alias::new("normalized_name"),
                            Alias::new("object_type"),
                            Alias::new("observed_sync_revision"),
                            Alias::new("facts_observed_storage_root_id"),
                            Alias::new("children_indexed"),
                            Alias::new("children_index_revision"),
                            Alias::new("identity_quality"),
                            Alias::new("presence_state"),
                        ])
                        .values_panic([
                            id.as_uuid().into(),
                            fixture.account_id.into(),
                            "fixture-drive".into(),
                            provider_id.into(),
                            identity_key("fixture-drive", provider_id).into(),
                            parent_provider_id.into(),
                            name.into(),
                            name.to_lowercase().into(),
                            kind.into(),
                            0_i64.into(),
                            fixture.root_id.as_uuid().into(),
                            (kind == "Directory").into(),
                            0_i64.into(),
                            "StableFileId".into(),
                            "Present".into(),
                        ]),
                ),
            )
            .await
            .unwrap();
        fixture
            .database
            .execute(
                backend.build(
                    Query::insert()
                        .into_table(Alias::new("storage_root_objects"))
                        .columns([
                            Alias::new("id"),
                            Alias::new("storage_root_id"),
                            Alias::new("storage_object_id"),
                            Alias::new("parent_storage_object_id"),
                            Alias::new("observed_sync_revision"),
                            Alias::new("children_indexed"),
                            Alias::new("children_index_revision"),
                            Alias::new("presence_state"),
                        ])
                        .values_panic([
                            Uuid::new_v4().into(),
                            fixture.root_id.as_uuid().into(),
                            id.as_uuid().into(),
                            Some(fixture.parent_id.as_uuid()).into(),
                            0_i64.into(),
                            (kind == "Directory").into(),
                            0_i64.into(),
                            "Present".into(),
                        ]),
                ),
            )
            .await
            .unwrap();
    }
    let now = Utc.with_ymd_and_hms(2026, 7, 18, 13, 30, 0).unwrap();
    let clock = ManualClock::new(now);
    let jobs = WorkJobRepository::with_clock(&fixture.database, clock.clone());
    jobs.enqueue_or_join(
        &WorkJobSpec::new(
            WorkTaskKind::ScopedStorageSync,
            WorkScope::StorageObject(new_parent),
            0,
            100,
        )
        .unwrap(),
    )
    .await
    .unwrap();
    let claimed = jobs
        .claim_next(
            &[WorkTaskKind::ScopedStorageSync],
            "inventory-reparent",
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    let committed = StorageSyncRepository::with_clock(&fixture.database, clock)
        .commit_inventory_page(
            &claimed,
            StorageSyncPage::new(
                fixture.root_id,
                new_parent,
                "fixture-drive",
                "reparent-page",
                vec![object("movie", "Movie.mkv")],
                true,
            )
            .unwrap(),
        )
        .await
        .unwrap();

    let events = fixture
        .database
        .query_all(
            backend.build(
                Query::select()
                    .columns([Alias::new("event_type"), Alias::new("payload")])
                    .from(Alias::new("storage_change_outbox"))
                    .and_where(Expr::col(Alias::new("storage_object_id")).eq(child.as_uuid()))
                    .order_by(Alias::new("event_type"), Order::Asc),
            ),
        )
        .await
        .unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(
        events[0].try_get::<String>("", "event_type").unwrap(),
        "MovedOut"
    );
    assert_eq!(
        events[1].try_get::<String>("", "event_type").unwrap(),
        "Upserted"
    );
    let moved_out = events[0]
        .try_get::<serde_json::Value>("", "payload")
        .unwrap();
    let moved_in = events[1]
        .try_get::<serde_json::Value>("", "payload")
        .unwrap();
    assert_eq!(
        moved_out["relation"]["parent_storage_object_id"],
        fixture.parent_id.to_string()
    );
    assert_eq!(
        moved_in["relation"]["parent_storage_object_id"],
        new_parent.to_string()
    );
    let old_parent = fixture
        .database
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("observed_sync_revision"))
                    .from(Alias::new("storage_root_objects"))
                    .and_where(
                        Expr::col(Alias::new("storage_root_id")).eq(fixture.root_id.as_uuid()),
                    )
                    .and_where(
                        Expr::col(Alias::new("storage_object_id")).eq(fixture.parent_id.as_uuid()),
                    ),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        old_parent
            .try_get::<i64>("", "observed_sync_revision")
            .unwrap(),
        committed.sync_revision()
    );
}

#[tokio::test]
async fn replaying_the_same_page_identity_does_not_advance_revision_twice() {
    let fixture = fixture().await;
    let now = Utc.with_ymd_and_hms(2026, 7, 18, 14, 0, 0).unwrap();
    let clock = ManualClock::new(now);
    let claimed = claimed_sync_job(&fixture, clock.clone()).await;
    let repository = StorageSyncRepository::with_clock(&fixture.database, clock);
    let page = StorageSyncPage::new(
        fixture.root_id,
        fixture.parent_id,
        "fixture-drive",
        "only-page",
        vec![object("movie", "Movie.mkv")],
        true,
    )
    .unwrap();

    let committed = repository
        .commit_inventory_page(&claimed, page.clone())
        .await
        .unwrap();
    let replayed = repository
        .commit_inventory_page(&claimed, page)
        .await
        .unwrap();

    assert_eq!(replayed.sync_revision(), committed.sync_revision());
    assert!(replayed.replayed());
    let revision: i64 = fixture
        .database
        .query_one(
            fixture.database.get_database_backend().build(
                Query::select()
                    .column(Alias::new("sync_revision"))
                    .from(Alias::new("storage_roots")),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get("", "sync_revision")
        .unwrap();
    assert_eq!(revision, 1);
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Keeps the omitted relation and validation page boundary explicit.
async fn validation_pages_defer_absence_until_the_whole_tree_succeeds() {
    let fixture = fixture().await;
    let stale_directory = StorageObjectRecordId::new();
    let refreshed_descendant = StorageObjectRecordId::new();
    let backend = fixture.database.get_database_backend();
    for (id, provider_id, name, object_type) in [
        (stale_directory, "stale", "Stale", "Directory"),
        (
            refreshed_descendant,
            "refreshed-descendant",
            "Refreshed.mkv",
            "File",
        ),
    ] {
        fixture
            .database
            .execute(
                backend.build(
                    Query::insert()
                        .into_table(Alias::new("storage_objects"))
                        .columns([
                            Alias::new("id"),
                            Alias::new("storage_account_id"),
                            Alias::new("provider_drive_id"),
                            Alias::new("provider_object_id"),
                            Alias::new("identity_key"),
                            Alias::new("name"),
                            Alias::new("normalized_name"),
                            Alias::new("object_type"),
                            Alias::new("observed_sync_revision"),
                            Alias::new("children_indexed"),
                            Alias::new("children_index_revision"),
                            Alias::new("identity_quality"),
                            Alias::new("presence_state"),
                        ])
                        .values_panic([
                            id.as_uuid().into(),
                            fixture.account_id.into(),
                            "fixture-drive".into(),
                            provider_id.into(),
                            identity_key("fixture-drive", provider_id).into(),
                            name.into(),
                            name.to_lowercase().into(),
                            object_type.into(),
                            0_i64.into(),
                            false.into(),
                            0_i64.into(),
                            "ProviderStable".into(),
                            "Present".into(),
                        ]),
                ),
            )
            .await
            .unwrap();
    }
    for (id, parent_id) in [
        (stale_directory, fixture.parent_id),
        (refreshed_descendant, stale_directory),
    ] {
        fixture
            .database
            .execute(
                backend.build(
                    Query::insert()
                        .into_table(Alias::new("storage_root_objects"))
                        .columns([
                            Alias::new("id"),
                            Alias::new("storage_root_id"),
                            Alias::new("storage_object_id"),
                            Alias::new("parent_storage_object_id"),
                            Alias::new("observed_sync_revision"),
                            Alias::new("children_indexed"),
                            Alias::new("children_index_revision"),
                            Alias::new("presence_state"),
                        ])
                        .values_panic([
                            Uuid::new_v4().into(),
                            fixture.root_id.as_uuid().into(),
                            id.as_uuid().into(),
                            parent_id.as_uuid().into(),
                            0_i64.into(),
                            false.into(),
                            0_i64.into(),
                            "Present".into(),
                        ]),
                ),
            )
            .await
            .unwrap();
    }
    let jobs = WorkJobRepository::new(&fixture.database);
    jobs.enqueue_or_join(
        &WorkJobSpec::new(
            WorkTaskKind::ValidateStorageRoot,
            WorkScope::StorageRoot(fixture.root_id),
            0,
            10,
        )
        .unwrap(),
    )
    .await
    .unwrap();
    let claimed = jobs
        .claim_next(
            &[WorkTaskKind::ValidateStorageRoot],
            "validate",
            Duration::minutes(1),
        )
        .await
        .unwrap()
        .unwrap();

    let repository = StorageSyncRepository::new(&fixture.database);
    let page = repository
        .commit_inventory_page(
            &claimed,
            StorageSyncPage::new(
                fixture.root_id,
                fixture.parent_id,
                "fixture-drive",
                "attempt:0:root",
                Vec::new(),
                true,
            )
            .unwrap(),
        )
        .await
        .unwrap();

    let presences = fixture
        .database
        .query_all(
            fixture.database.get_database_backend().build(
                Query::select()
                    .column(Alias::new("presence_state"))
                    .from(Alias::new("storage_root_objects"))
                    .and_where(
                        Expr::col(Alias::new("storage_root_id")).eq(fixture.root_id.as_uuid()),
                    )
                    .and_where(
                        Expr::col(Alias::new("storage_object_id"))
                            .is_in([stale_directory.as_uuid(), refreshed_descendant.as_uuid()]),
                    )
                    .order_by(Alias::new("storage_object_id"), Order::Asc),
            ),
        )
        .await
        .unwrap()
        .into_iter()
        .map(|row| row.try_get::<String>("", "presence_state").unwrap())
        .collect::<Vec<_>>();
    assert_eq!(presences, ["Present", "Present"]);

    fixture
        .database
        .execute(
            fixture.database.get_database_backend().build(
                Query::update()
                    .table(Alias::new("storage_root_objects"))
                    .value(Alias::new("observed_sync_revision"), page.sync_revision())
                    .and_where(
                        Expr::col(Alias::new("storage_root_id")).eq(fixture.root_id.as_uuid()),
                    )
                    .and_where(
                        Expr::col(Alias::new("storage_object_id"))
                            .eq(refreshed_descendant.as_uuid()),
                    ),
            ),
        )
        .await
        .unwrap();

    repository
        .commit_validation_sweep(
            &claimed,
            fixture.root_id,
            fixture.parent_id,
            page.sync_revision(),
        )
        .await
        .unwrap();

    let confirmed_absent = fixture
        .database
        .query_all(
            fixture.database.get_database_backend().build(
                Query::select()
                    .column(Alias::new("presence_state"))
                    .from(Alias::new("storage_root_objects"))
                    .and_where(
                        Expr::col(Alias::new("storage_root_id")).eq(fixture.root_id.as_uuid()),
                    )
                    .and_where(
                        Expr::col(Alias::new("storage_object_id"))
                            .is_in([stale_directory.as_uuid(), refreshed_descendant.as_uuid()]),
                    )
                    .order_by(Alias::new("storage_object_id"), Order::Asc),
            ),
        )
        .await
        .unwrap()
        .into_iter()
        .map(|row| row.try_get::<String>("", "presence_state").unwrap())
        .collect::<Vec<_>>();
    assert_eq!(confirmed_absent, ["ConfirmedAbsent", "ConfirmedAbsent"]);
}

#[tokio::test]
async fn expired_work_claim_cannot_commit_a_storage_page() {
    let fixture = fixture().await;
    let now = Utc.with_ymd_and_hms(2026, 7, 18, 15, 0, 0).unwrap();
    let clock = ManualClock::new(now);
    let stale = claimed_sync_job(&fixture, clock.clone()).await;
    clock.set(now + Duration::seconds(31));
    let work = WorkJobRepository::with_clock(&fixture.database, clock.clone());
    work.claim_next(
        &[WorkTaskKind::ScopedStorageSync],
        "replacement",
        Duration::seconds(30),
    )
    .await
    .unwrap()
    .unwrap();
    let repository = StorageSyncRepository::with_clock(&fixture.database, clock);

    let error = repository
        .commit_inventory_page(
            &stale,
            StorageSyncPage::new(
                fixture.root_id,
                fixture.parent_id,
                "fixture-drive",
                "stale-page",
                Vec::new(),
                true,
            )
            .unwrap(),
        )
        .await
        .unwrap_err();

    assert!(matches!(error, StorageSyncRepositoryError::LostLease));
}

#[tokio::test]
async fn page_commit_rolls_back_when_lease_expires_during_the_transaction() {
    let fixture = fixture().await;
    let now = Utc.with_ymd_and_hms(2026, 7, 18, 15, 30, 0).unwrap();
    let claim_clock = ManualClock::new(now);
    let claimed = claimed_sync_job(&fixture, claim_clock).await;
    let clock = AdvancingClock(Arc::new(Mutex::new(VecDeque::from([
        now,
        now + Duration::seconds(31),
    ]))));
    let repository = StorageSyncRepository::with_clock(&fixture.database, clock);

    let error = repository
        .commit_inventory_page(
            &claimed,
            StorageSyncPage::new(
                fixture.root_id,
                fixture.parent_id,
                "fixture-drive",
                "slow-page",
                vec![object("movie", "Movie.mkv")],
                true,
            )
            .unwrap(),
        )
        .await
        .unwrap_err();

    assert!(matches!(error, StorageSyncRepositoryError::LostLease));
    let revision: i64 = fixture
        .database
        .query_one(
            fixture.database.get_database_backend().build(
                Query::select()
                    .column(Alias::new("sync_revision"))
                    .from(Alias::new("storage_roots")),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get("", "sync_revision")
        .unwrap();
    assert_eq!(revision, 0);
}

#[tokio::test]
async fn dependent_media_work_waits_for_children_and_reconciled_revision() {
    let fixture = fixture().await;
    let now = Utc.with_ymd_and_hms(2026, 7, 18, 16, 0, 0).unwrap();
    let clock = ManualClock::new(now);
    let sync_claim = claimed_sync_job(&fixture, clock.clone()).await;
    let sync_repository = StorageSyncRepository::with_clock(&fixture.database, clock.clone());
    let committed = sync_repository
        .commit_inventory_page(
            &sync_claim,
            StorageSyncPage::new(
                fixture.root_id,
                fixture.parent_id,
                "fixture-drive",
                "complete-page",
                vec![object("movie", "Movie.mkv")],
                true,
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let work = WorkJobRepository::with_clock(&fixture.database, clock.clone());
    let transaction = fixture.database.begin().await.unwrap();
    work.complete_in_transaction(
        &transaction,
        &sync_claim,
        WorkJobResult::success(json!({"objects": 1}), Vec::new())
            .with_sync_revision(committed.sync_revision())
            .unwrap(),
    )
    .await
    .unwrap();
    transaction.commit().await.unwrap();
    let media = WorkJobSpec::new(
        WorkTaskKind::ExpandItem,
        WorkScope::CatalogItem(tjxy_common::CatalogItemId::new()),
        1,
        100,
    )
    .unwrap()
    .with_required_sync(sync_claim.id(), committed.sync_revision());

    assert!(matches!(
        work.enqueue_or_join(&media).await.unwrap_err(),
        tjxy_db::WorkJobRepositoryError::DependencyNotReady
    ));

    let outbox = OutboxRepository::with_clock(&fixture.database, clock);
    while let Some(event) = outbox
        .claim_next(fixture.root_id, "reconciler", Duration::seconds(30))
        .await
        .unwrap()
    {
        let transaction = fixture.database.begin().await.unwrap();
        outbox
            .complete_in_transaction(&transaction, &event)
            .await
            .unwrap();
        transaction.commit().await.unwrap();
    }

    work.enqueue_or_join(&media).await.unwrap();
    assert!(
        work.claim_next(
            &[WorkTaskKind::ExpandItem],
            "media-worker",
            Duration::seconds(30),
        )
        .await
        .unwrap()
        .is_some()
    );
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Keeps the provider removal and complete root-local subtree visible.
async fn removing_a_directory_marks_every_materialized_descendant_absent() {
    let fixture = fixture().await;
    let directory = StorageObjectRecordId::new();
    let video = StorageObjectRecordId::new();
    let backend = fixture.database.get_database_backend();
    for (id, provider_id, parent_provider_id, name, object_type) in [
        (
            directory,
            "removed-directory",
            "root",
            "Removed Directory",
            "Directory",
        ),
        (
            video,
            "descendant-video",
            "removed-directory",
            "Episode.mkv",
            "File",
        ),
    ] {
        fixture
            .database
            .execute(
                backend.build(
                    Query::insert()
                        .into_table(Alias::new("storage_objects"))
                        .columns([
                            Alias::new("id"),
                            Alias::new("storage_account_id"),
                            Alias::new("provider_drive_id"),
                            Alias::new("provider_object_id"),
                            Alias::new("identity_key"),
                            Alias::new("provider_parent_id"),
                            Alias::new("name"),
                            Alias::new("normalized_name"),
                            Alias::new("object_type"),
                            Alias::new("observed_sync_revision"),
                            Alias::new("children_indexed"),
                            Alias::new("children_index_revision"),
                            Alias::new("identity_quality"),
                            Alias::new("presence_state"),
                        ])
                        .values_panic([
                            id.as_uuid().into(),
                            fixture.account_id.into(),
                            "fixture-drive".into(),
                            provider_id.into(),
                            identity_key("fixture-drive", provider_id).into(),
                            parent_provider_id.into(),
                            name.into(),
                            name.to_lowercase().into(),
                            object_type.into(),
                            0_i64.into(),
                            false.into(),
                            0_i64.into(),
                            "ProviderStableId".into(),
                            "Present".into(),
                        ]),
                ),
            )
            .await
            .unwrap();
    }
    for (object_id, parent_id) in [(directory, fixture.parent_id), (video, directory)] {
        fixture
            .database
            .execute(
                backend.build(
                    Query::insert()
                        .into_table(Alias::new("storage_root_objects"))
                        .columns([
                            Alias::new("id"),
                            Alias::new("storage_root_id"),
                            Alias::new("storage_object_id"),
                            Alias::new("parent_storage_object_id"),
                            Alias::new("observed_sync_revision"),
                            Alias::new("children_indexed"),
                            Alias::new("children_index_revision"),
                            Alias::new("presence_state"),
                        ])
                        .values_panic([
                            Uuid::new_v4().into(),
                            fixture.root_id.as_uuid().into(),
                            object_id.as_uuid().into(),
                            parent_id.as_uuid().into(),
                            0_i64.into(),
                            false.into(),
                            0_i64.into(),
                            "Present".into(),
                        ]),
                ),
            )
            .await
            .unwrap();
    }
    fixture
        .database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("storage_sync_cursors"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("storage_root_id"),
                        Alias::new("cursor_type"),
                        Alias::new("cursor_value"),
                        Alias::new("status"),
                    ])
                    .values_panic([
                        Uuid::new_v4().into(),
                        fixture.root_id.as_uuid().into(),
                        "Changes".into(),
                        "cursor-one".into(),
                        "Active".into(),
                    ]),
            ),
        )
        .await
        .unwrap();

    StorageChangeFeedRepository::new(&fixture.database)
        .commit_page(
            fixture.root_id,
            fixture.account_id,
            "fixture-drive",
            &ChangeCursor::new("cursor-one").unwrap(),
            &ChangePage::new(
                vec![StorageChange::Removed(
                    StorageObjectId::new("filesystem", "removed-directory").unwrap(),
                )],
                ChangeCursor::new("cursor-two").unwrap(),
            ),
        )
        .await
        .unwrap();

    let descendant = fixture
        .database
        .query_one(
            backend.build(
                Query::select()
                    .columns([
                        Alias::new("presence_state"),
                        Alias::new("observed_sync_revision"),
                    ])
                    .from(Alias::new("storage_root_objects"))
                    .and_where(
                        Expr::col(Alias::new("storage_root_id")).eq(fixture.root_id.as_uuid()),
                    )
                    .and_where(Expr::col(Alias::new("storage_object_id")).eq(video.as_uuid())),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        descendant.try_get::<String>("", "presence_state").unwrap(),
        "ConfirmedAbsent"
    );
    assert_eq!(
        descendant
            .try_get::<i64>("", "observed_sync_revision")
            .unwrap(),
        1
    );
    let descendant_event = fixture
        .database
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("event_type"))
                    .from(Alias::new("storage_change_outbox"))
                    .and_where(
                        Expr::col(Alias::new("storage_root_id")).eq(fixture.root_id.as_uuid()),
                    )
                    .and_where(Expr::col(Alias::new("storage_object_id")).eq(video.as_uuid())),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        descendant_event
            .try_get::<String>("", "event_type")
            .unwrap(),
        "AncestorRemoved"
    );
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Keeps the completed inventory boundary and subtree assertion explicit.
async fn completed_inventory_marks_unobserved_directory_descendants_absent() {
    let fixture = fixture().await;
    let directory = StorageObjectRecordId::new();
    let video = StorageObjectRecordId::new();
    let backend = fixture.database.get_database_backend();
    for (id, provider_id, parent_provider_id, name, object_type) in [
        (
            directory,
            "inventory-missing-directory",
            "root",
            "Missing Directory",
            "Directory",
        ),
        (
            video,
            "inventory-missing-video",
            "inventory-missing-directory",
            "Episode.mkv",
            "File",
        ),
    ] {
        fixture
            .database
            .execute(
                backend.build(
                    Query::insert()
                        .into_table(Alias::new("storage_objects"))
                        .columns([
                            Alias::new("id"),
                            Alias::new("storage_account_id"),
                            Alias::new("provider_drive_id"),
                            Alias::new("provider_object_id"),
                            Alias::new("identity_key"),
                            Alias::new("provider_parent_id"),
                            Alias::new("name"),
                            Alias::new("normalized_name"),
                            Alias::new("object_type"),
                            Alias::new("observed_sync_revision"),
                            Alias::new("children_indexed"),
                            Alias::new("children_index_revision"),
                            Alias::new("identity_quality"),
                            Alias::new("presence_state"),
                        ])
                        .values_panic([
                            id.as_uuid().into(),
                            fixture.account_id.into(),
                            "fixture-drive".into(),
                            provider_id.into(),
                            identity_key("fixture-drive", provider_id).into(),
                            parent_provider_id.into(),
                            name.into(),
                            name.to_lowercase().into(),
                            object_type.into(),
                            0_i64.into(),
                            false.into(),
                            0_i64.into(),
                            "ProviderStableId".into(),
                            "Present".into(),
                        ]),
                ),
            )
            .await
            .unwrap();
    }
    for (object_id, parent_id) in [(directory, fixture.parent_id), (video, directory)] {
        fixture
            .database
            .execute(
                backend.build(
                    Query::insert()
                        .into_table(Alias::new("storage_root_objects"))
                        .columns([
                            Alias::new("id"),
                            Alias::new("storage_root_id"),
                            Alias::new("storage_object_id"),
                            Alias::new("parent_storage_object_id"),
                            Alias::new("observed_sync_revision"),
                            Alias::new("children_indexed"),
                            Alias::new("children_index_revision"),
                            Alias::new("presence_state"),
                        ])
                        .values_panic([
                            Uuid::new_v4().into(),
                            fixture.root_id.as_uuid().into(),
                            object_id.as_uuid().into(),
                            parent_id.as_uuid().into(),
                            0_i64.into(),
                            false.into(),
                            0_i64.into(),
                            "Present".into(),
                        ]),
                ),
            )
            .await
            .unwrap();
    }

    let now = Utc.with_ymd_and_hms(2026, 7, 18, 13, 40, 0).unwrap();
    let clock = ManualClock::new(now);
    let claimed = claimed_sync_job(&fixture, clock.clone()).await;
    let committed = StorageSyncRepository::with_clock(&fixture.database, clock)
        .commit_inventory_page(
            &claimed,
            StorageSyncPage::new(
                fixture.root_id,
                fixture.parent_id,
                "fixture-drive",
                "completed-empty-page",
                Vec::new(),
                true,
            )
            .unwrap(),
        )
        .await
        .unwrap();

    let relations = fixture
        .database
        .query_all(
            backend.build(
                Query::select()
                    .columns([
                        Alias::new("storage_object_id"),
                        Alias::new("presence_state"),
                        Alias::new("observed_sync_revision"),
                    ])
                    .from(Alias::new("storage_root_objects"))
                    .and_where(
                        Expr::col(Alias::new("storage_root_id")).eq(fixture.root_id.as_uuid()),
                    )
                    .and_where(
                        Expr::col(Alias::new("storage_object_id"))
                            .is_in([directory.as_uuid(), video.as_uuid()]),
                    ),
            ),
        )
        .await
        .unwrap();
    assert_eq!(relations.len(), 2);
    assert!(relations.iter().all(|row| {
        row.try_get::<String>("", "presence_state").unwrap() == "ConfirmedAbsent"
            && row.try_get::<i64>("", "observed_sync_revision").unwrap()
                == committed.sync_revision()
    }));
    let descendant_event = fixture
        .database
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("event_type"))
                    .from(Alias::new("storage_change_outbox"))
                    .and_where(Expr::col(Alias::new("storage_object_id")).eq(video.as_uuid())),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        descendant_event
            .try_get::<String>("", "event_type")
            .unwrap(),
        "AncestorRemoved"
    );
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Mirrors one normalized root and its atomic change page.
async fn change_page_atomically_updates_objects_removals_cursor_and_root_revision() {
    let fixture = fixture().await;
    let existing_id = StorageObjectRecordId::new();
    let removed_id = StorageObjectRecordId::new();
    let moved_out_id = StorageObjectRecordId::new();
    let backend = fixture.database.get_database_backend();
    for (id, provider_id, name) in [
        (existing_id, "movie", "Old.mkv"),
        (removed_id, "removed", "Removed.mkv"),
        (moved_out_id, "moved-out", "Moved Out.mkv"),
    ] {
        fixture
            .database
            .execute(
                backend.build(
                    Query::insert()
                        .into_table(Alias::new("storage_objects"))
                        .columns([
                            Alias::new("id"),
                            Alias::new("storage_account_id"),
                            Alias::new("provider_drive_id"),
                            Alias::new("provider_object_id"),
                            Alias::new("identity_key"),
                            Alias::new("provider_parent_id"),
                            Alias::new("name"),
                            Alias::new("normalized_name"),
                            Alias::new("object_type"),
                            Alias::new("observed_sync_revision"),
                            Alias::new("children_indexed"),
                            Alias::new("children_index_revision"),
                            Alias::new("identity_quality"),
                            Alias::new("presence_state"),
                        ])
                        .values_panic([
                            id.as_uuid().into(),
                            fixture.account_id.into(),
                            "fixture-drive".into(),
                            provider_id.into(),
                            identity_key("fixture-drive", provider_id).into(),
                            "root".into(),
                            name.into(),
                            name.to_lowercase().into(),
                            "File".into(),
                            0_i64.into(),
                            false.into(),
                            0_i64.into(),
                            "ProviderStableId".into(),
                            "Present".into(),
                        ]),
                ),
            )
            .await
            .unwrap();
        fixture
            .database
            .execute(
                backend.build(
                    Query::insert()
                        .into_table(Alias::new("storage_root_objects"))
                        .columns([
                            Alias::new("id"),
                            Alias::new("storage_root_id"),
                            Alias::new("storage_object_id"),
                            Alias::new("parent_storage_object_id"),
                            Alias::new("observed_sync_revision"),
                            Alias::new("children_indexed"),
                            Alias::new("children_index_revision"),
                            Alias::new("presence_state"),
                        ])
                        .values_panic([
                            Uuid::new_v4().into(),
                            fixture.root_id.as_uuid().into(),
                            id.as_uuid().into(),
                            fixture.parent_id.as_uuid().into(),
                            0_i64.into(),
                            false.into(),
                            0_i64.into(),
                            "Present".into(),
                        ]),
                ),
            )
            .await
            .unwrap();
    }
    fixture
        .database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("storage_sync_cursors"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("storage_root_id"),
                        Alias::new("cursor_type"),
                        Alias::new("cursor_value"),
                        Alias::new("status"),
                    ])
                    .values_panic([
                        Uuid::new_v4().into(),
                        fixture.root_id.as_uuid().into(),
                        "Changes".into(),
                        "cursor-one".into(),
                        "Active".into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    let moved_parent_id = StorageObjectRecordId::new();
    fixture
        .database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("storage_objects"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("storage_account_id"),
                        Alias::new("provider_drive_id"),
                        Alias::new("provider_object_id"),
                        Alias::new("identity_key"),
                        Alias::new("provider_parent_id"),
                        Alias::new("name"),
                        Alias::new("normalized_name"),
                        Alias::new("object_type"),
                        Alias::new("observed_sync_revision"),
                        Alias::new("children_indexed"),
                        Alias::new("children_index_revision"),
                        Alias::new("identity_quality"),
                        Alias::new("presence_state"),
                    ])
                    .values_panic([
                        moved_parent_id.as_uuid().into(),
                        fixture.account_id.into(),
                        "fixture-drive".into(),
                        "new-parent".into(),
                        identity_key("fixture-drive", "new-parent").into(),
                        "root".into(),
                        "New Parent".into(),
                        "new parent".into(),
                        "Directory".into(),
                        0_i64.into(),
                        false.into(),
                        0_i64.into(),
                        "ProviderStableId".into(),
                        "Present".into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    fixture
        .database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("storage_root_objects"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("storage_root_id"),
                        Alias::new("storage_object_id"),
                        Alias::new("parent_storage_object_id"),
                        Alias::new("observed_sync_revision"),
                        Alias::new("children_indexed"),
                        Alias::new("children_index_revision"),
                        Alias::new("presence_state"),
                    ])
                    .values_panic([
                        Uuid::new_v4().into(),
                        fixture.root_id.as_uuid().into(),
                        moved_parent_id.as_uuid().into(),
                        fixture.parent_id.as_uuid().into(),
                        0_i64.into(),
                        false.into(),
                        0_i64.into(),
                        "Present".into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    let old_parent_item = CatalogItemId::new();
    fixture
        .database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("catalog_items"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("item_type"),
                        Alias::new("name"),
                        Alias::new("sort_name"),
                        Alias::new("sort_key"),
                        Alias::new("classification_state"),
                        Alias::new("metadata_state"),
                        Alias::new("structure_state"),
                        Alias::new("source_state"),
                        Alias::new("structure_expansion_revision"),
                        Alias::new("source_index_revision"),
                        Alias::new("is_present"),
                    ])
                    .values_panic([
                        old_parent_item.as_uuid().into(),
                        "Series".into(),
                        "Old Parent".into(),
                        "old parent".into(),
                        SortKey::from_text("Old Parent").into_bytes().into(),
                        "Matched".into(),
                        "Ready".into(),
                        "Expanded".into(),
                        "NotApplicable".into(),
                        0_i64.into(),
                        0_i64.into(),
                        true.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    fixture
        .database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("identity_matches"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("storage_object_id"),
                        Alias::new("candidate_catalog_item_id"),
                        Alias::new("confidence"),
                        Alias::new("state"),
                        Alias::new("evidence"),
                    ])
                    .values_panic([
                        Uuid::new_v4().into(),
                        fixture.parent_id.as_uuid().into(),
                        old_parent_item.as_uuid().into(),
                        1.0.into(),
                        "Matched".into(),
                        json!({}).into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    let metadata_item = CatalogItemId::new();
    fixture
        .database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("catalog_items"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("item_type"),
                        Alias::new("name"),
                        Alias::new("sort_name"),
                        Alias::new("sort_key"),
                        Alias::new("classification_state"),
                        Alias::new("metadata_state"),
                        Alias::new("structure_state"),
                        Alias::new("source_state"),
                        Alias::new("structure_expansion_revision"),
                        Alias::new("source_index_revision"),
                        Alias::new("is_present"),
                    ])
                    .values_panic([
                        metadata_item.as_uuid().into(),
                        "Movie".into(),
                        "New Movie".into(),
                        "new movie".into(),
                        SortKey::from_text("New Movie").into_bytes().into(),
                        "Matched".into(),
                        "Ready".into(),
                        "NotApplicable".into(),
                        "Indexed".into(),
                        0_i64.into(),
                        0_i64.into(),
                        true.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    fixture
        .database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("identity_matches"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("storage_object_id"),
                        Alias::new("candidate_catalog_item_id"),
                        Alias::new("confidence"),
                        Alias::new("state"),
                        Alias::new("evidence"),
                    ])
                    .values_panic([
                        Uuid::new_v4().into(),
                        moved_parent_id.as_uuid().into(),
                        metadata_item.as_uuid().into(),
                        1.0.into(),
                        "Matched".into(),
                        json!({}).into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    let media_source_id = Uuid::new_v4();
    fixture
        .database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("media_sources"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("catalog_item_id"),
                        Alias::new("presentation_key"),
                        Alias::new("probe_state"),
                        Alias::new("probe_revision"),
                    ])
                    .values_panic([
                        media_source_id.into(),
                        old_parent_item.as_uuid().into(),
                        Uuid::new_v4().into(),
                        "Probed".into(),
                        0_i64.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    fixture
        .database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("media_locations"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("media_source_id"),
                        Alias::new("storage_object_id"),
                        Alias::new("priority"),
                        Alias::new("availability_state"),
                    ])
                    .values_panic([
                        Uuid::new_v4().into(),
                        media_source_id.into(),
                        moved_out_id.as_uuid().into(),
                        10_i32.into(),
                        "Available".into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    let updated = StorageObject::file(
        StorageObjectId::new("filesystem", "movie").unwrap(),
        "Renamed.mkv",
        42,
    )
    .with_parents(vec![
        StorageObjectId::new("filesystem", "new-parent").unwrap(),
    ])
    .unwrap()
    .with_remote_revision("2")
    .unwrap();
    let page = ChangePage::new(
        vec![
            StorageChange::Upsert(updated),
            StorageChange::Upsert(
                StorageObject::file(
                    StorageObjectId::new("filesystem", "new-movie").unwrap(),
                    "movie.nfo",
                    24,
                )
                .with_parents(vec![
                    StorageObjectId::new("filesystem", "new-parent").unwrap(),
                ])
                .unwrap(),
            ),
            StorageChange::Removed(StorageObjectId::new("filesystem", "removed").unwrap()),
            StorageChange::Upsert(
                StorageObject::file(
                    StorageObjectId::new("filesystem", "moved-out").unwrap(),
                    "Moved Out Renamed.mkv",
                    84,
                )
                .with_parents(vec![
                    StorageObjectId::new("filesystem", "unmaterialized-parent").unwrap(),
                ])
                .unwrap()
                .with_remote_revision("3")
                .unwrap(),
            ),
        ],
        ChangeCursor::new("cursor-two").unwrap(),
    );

    let committed = StorageChangeFeedRepository::new(&fixture.database)
        .commit_page(
            fixture.root_id,
            fixture.account_id,
            "fixture-drive",
            &ChangeCursor::new("cursor-one").unwrap(),
            &page,
        )
        .await
        .unwrap();

    assert_eq!(committed.sync_revision(), 1);
    assert_eq!(committed.applied_changes(), 4);
    let rows = fixture
        .database
        .query_all(
            fixture.database.get_database_backend().build(
                Query::select()
                    .columns([
                        Alias::new("provider_object_id"),
                        Alias::new("name"),
                        Alias::new("presence_state"),
                    ])
                    .from(Alias::new("storage_objects"))
                    .and_where(
                        Expr::col(Alias::new("provider_object_id")).is_in(["movie", "removed"]),
                    )
                    .order_by(Alias::new("provider_object_id"), Order::Asc),
            ),
        )
        .await
        .unwrap();
    assert_eq!(
        rows[0].try_get::<String>("", "name").unwrap(),
        "Renamed.mkv"
    );
    assert_eq!(
        rows[0].try_get::<String>("", "presence_state").unwrap(),
        "Present"
    );
    assert_eq!(
        rows[1].try_get::<String>("", "presence_state").unwrap(),
        "ConfirmedAbsent"
    );
    let moved_out = fixture
        .database
        .query_one(
            fixture.database.get_database_backend().build(
                Query::select()
                    .columns([
                        (Alias::new("object"), Alias::new("name")),
                        (Alias::new("object"), Alias::new("provider_parent_id")),
                    ])
                    .expr_as(
                        Expr::col((Alias::new("object"), Alias::new("presence_state"))),
                        Alias::new("object_presence"),
                    )
                    .expr_as(
                        Expr::col((Alias::new("root_object"), Alias::new("presence_state"))),
                        Alias::new("relation_presence"),
                    )
                    .column((Alias::new("root_object"), Alias::new("availability_reason")))
                    .from_as(Alias::new("storage_objects"), Alias::new("object"))
                    .join_as(
                        JoinType::InnerJoin,
                        Alias::new("storage_root_objects"),
                        Alias::new("root_object"),
                        Expr::col((Alias::new("root_object"), Alias::new("storage_object_id")))
                            .equals((Alias::new("object"), Alias::new("id"))),
                    )
                    .and_where(
                        Expr::col((Alias::new("root_object"), Alias::new("storage_root_id")))
                            .eq(fixture.root_id.as_uuid()),
                    )
                    .and_where(
                        Expr::col((Alias::new("object"), Alias::new("id")))
                            .eq(moved_out_id.as_uuid()),
                    ),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        moved_out.try_get::<String>("", "name").unwrap(),
        "Moved Out Renamed.mkv"
    );
    assert_eq!(
        moved_out
            .try_get::<String>("", "provider_parent_id")
            .unwrap(),
        "unmaterialized-parent"
    );
    assert_eq!(
        moved_out.try_get::<String>("", "object_presence").unwrap(),
        "Present"
    );
    assert_eq!(
        moved_out
            .try_get::<String>("", "relation_presence")
            .unwrap(),
        "TemporarilyUnavailable"
    );
    assert_eq!(
        moved_out
            .try_get::<String>("", "availability_reason")
            .unwrap(),
        "moved-to-unmaterialized-parent"
    );
    let moved = fixture
        .database
        .query_one(
            backend.build(
                Query::select()
                    .expr_as(
                        Expr::col((Alias::new("object"), Alias::new("provider_parent_id"))),
                        Alias::new("provider_parent_id"),
                    )
                    .expr_as(
                        Expr::col((
                            Alias::new("relation"),
                            Alias::new("parent_storage_object_id"),
                        )),
                        Alias::new("parent_storage_object_id"),
                    )
                    .from_as(Alias::new("storage_objects"), Alias::new("object"))
                    .join_as(
                        sea_orm::sea_query::JoinType::InnerJoin,
                        Alias::new("storage_root_objects"),
                        Alias::new("relation"),
                        Expr::col((Alias::new("relation"), Alias::new("storage_object_id")))
                            .equals((Alias::new("object"), Alias::new("id"))),
                    )
                    .and_where(
                        Expr::col((Alias::new("object"), Alias::new("id")))
                            .eq(existing_id.as_uuid()),
                    ),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        moved.try_get::<String>("", "provider_parent_id").unwrap(),
        "new-parent"
    );
    assert_eq!(
        moved
            .try_get::<Uuid>("", "parent_storage_object_id")
            .unwrap(),
        moved_parent_id.as_uuid()
    );
    let old_parent_observation = fixture
        .database
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("observed_sync_revision"))
                    .from(Alias::new("storage_root_objects"))
                    .and_where(
                        Expr::col(Alias::new("storage_root_id")).eq(fixture.root_id.as_uuid()),
                    )
                    .and_where(
                        Expr::col(Alias::new("storage_object_id")).eq(fixture.parent_id.as_uuid()),
                    ),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        old_parent_observation
            .try_get::<i64>("", "observed_sync_revision")
            .unwrap(),
        committed.sync_revision()
    );
    let added = fixture
        .database
        .query_one(
            backend.build(
                Query::select()
                    .expr_as(
                        Expr::col((Alias::new("object"), Alias::new("name"))),
                        Alias::new("name"),
                    )
                    .expr_as(
                        Expr::col((
                            Alias::new("relation"),
                            Alias::new("parent_storage_object_id"),
                        )),
                        Alias::new("parent_storage_object_id"),
                    )
                    .from_as(Alias::new("storage_objects"), Alias::new("object"))
                    .join_as(
                        sea_orm::sea_query::JoinType::InnerJoin,
                        Alias::new("storage_root_objects"),
                        Alias::new("relation"),
                        Expr::col((Alias::new("relation"), Alias::new("storage_object_id")))
                            .equals((Alias::new("object"), Alias::new("id"))),
                    )
                    .and_where(
                        Expr::col((Alias::new("object"), Alias::new("provider_object_id")))
                            .eq("new-movie"),
                    ),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(added.try_get::<String>("", "name").unwrap(), "movie.nfo");
    assert_eq!(
        added
            .try_get::<Uuid>("", "parent_storage_object_id")
            .unwrap(),
        moved_parent_id.as_uuid()
    );
    let cursor = fixture
        .database
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("cursor_value"))
                    .from(Alias::new("storage_sync_cursors"))
                    .and_where(
                        Expr::col(Alias::new("storage_root_id")).eq(fixture.root_id.as_uuid()),
                    )
                    .and_where(Expr::col(Alias::new("cursor_type")).eq("Changes")),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        cursor.try_get::<String>("", "cursor_value").unwrap(),
        "cursor-two"
    );
    let outbox = OutboxRepository::new(&fixture.database);
    while let Some(claimed) = outbox
        .claim_next(fixture.root_id, "change-projector", Duration::seconds(30))
        .await
        .unwrap()
    {
        StorageChangeProjectionRepository::new(&fixture.database)
            .apply(&claimed)
            .await
            .unwrap();
    }
    assert_eq!(
        outbox.reconciled_revision(fixture.root_id).await.unwrap(),
        committed.sync_revision()
    );
    let generation = fixture
        .database
        .query_one(
            fixture.database.get_database_backend().build(
                Query::select()
                    .column(Alias::new("generation"))
                    .from(Alias::new("catalog_state"))
                    .and_where(Expr::col(Alias::new("id")).eq(1)),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get::<i64>("", "generation")
        .unwrap();
    assert!(generation > 0);
    let invalidations = fixture
        .database
        .query_one(
            fixture.database.get_database_backend().build(
                Query::select()
                    .expr_as(
                        Func::count(Expr::col(Alias::new("id"))),
                        Alias::new("count"),
                    )
                    .from(Alias::new("cache_invalidation_outbox")),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get::<i64>("", "count")
        .unwrap();
    assert_eq!(invalidations, 0);
    let metadata_revision = fixture
        .database
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("metadata_revision"))
                    .from(Alias::new("catalog_items"))
                    .and_where(Expr::col(Alias::new("id")).eq(metadata_item.as_uuid())),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get::<i64>("", "metadata_revision")
        .unwrap();
    assert_eq!(metadata_revision, 1);
    let old_parent_revision = fixture
        .database
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("structure_expansion_revision"))
                    .from(Alias::new("catalog_items"))
                    .and_where(Expr::col(Alias::new("id")).eq(old_parent_item.as_uuid())),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get::<i64>("", "structure_expansion_revision")
        .unwrap();
    assert_eq!(old_parent_revision, 3);
    let moved_out_availability = fixture
        .database
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("availability_state"))
                    .from(Alias::new("media_locations"))
                    .and_where(
                        Expr::col(Alias::new("storage_object_id")).eq(moved_out_id.as_uuid()),
                    ),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get::<String>("", "availability_state")
        .unwrap();
    assert_eq!(moved_out_availability, "TemporarilyUnavailable");
    let temporarily_unavailable_catalog = fixture
        .database
        .query_one(
            fixture.database.get_database_backend().build(
                Query::select()
                    .columns([
                        (Alias::new("item"), Alias::new("is_present")),
                        (Alias::new("source"), Alias::new("probe_state")),
                    ])
                    .from_as(Alias::new("catalog_items"), Alias::new("item"))
                    .join_as(
                        JoinType::InnerJoin,
                        Alias::new("media_sources"),
                        Alias::new("source"),
                        Expr::col((Alias::new("source"), Alias::new("catalog_item_id")))
                            .equals((Alias::new("item"), Alias::new("id"))),
                    )
                    .and_where(
                        Expr::col((Alias::new("item"), Alias::new("id")))
                            .eq(old_parent_item.as_uuid()),
                    ),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert!(
        temporarily_unavailable_catalog
            .try_get::<bool>("", "is_present")
            .unwrap()
    );
    assert_eq!(
        temporarily_unavailable_catalog
            .try_get::<String>("", "probe_state")
            .unwrap(),
        "Stale"
    );

    let sync_jobs = WorkJobRepository::new(&fixture.database);
    sync_jobs
        .enqueue_or_join(
            &WorkJobSpec::new(
                WorkTaskKind::ScopedStorageSync,
                WorkScope::StorageObject(fixture.parent_id),
                committed.sync_revision(),
                100,
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let sync_claim = sync_jobs
        .claim_next(
            &[WorkTaskKind::ScopedStorageSync],
            "confirm-old-scope-worker",
            Duration::seconds(30),
        )
        .await
        .unwrap()
        .unwrap();
    StorageSyncRepository::new(&fixture.database)
        .commit_inventory_page(
            &sync_claim,
            StorageSyncPage::new(
                fixture.root_id,
                fixture.parent_id,
                "fixture-drive",
                "confirm-old-scope",
                Vec::new(),
                true,
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let confirmed_old_relation = fixture
        .database
        .query_one(
            fixture.database.get_database_backend().build(
                Query::select()
                    .expr_as(
                        Expr::col((Alias::new("object"), Alias::new("presence_state"))),
                        Alias::new("object_presence"),
                    )
                    .expr_as(
                        Expr::col((Alias::new("root_object"), Alias::new("presence_state"))),
                        Alias::new("relation_presence"),
                    )
                    .from_as(Alias::new("storage_objects"), Alias::new("object"))
                    .join_as(
                        JoinType::InnerJoin,
                        Alias::new("storage_root_objects"),
                        Alias::new("root_object"),
                        Expr::col((Alias::new("root_object"), Alias::new("storage_object_id")))
                            .equals((Alias::new("object"), Alias::new("id"))),
                    )
                    .and_where(
                        Expr::col((Alias::new("root_object"), Alias::new("storage_root_id")))
                            .eq(fixture.root_id.as_uuid()),
                    )
                    .and_where(
                        Expr::col((Alias::new("object"), Alias::new("id")))
                            .eq(moved_out_id.as_uuid()),
                    ),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        confirmed_old_relation
            .try_get::<String>("", "object_presence")
            .unwrap(),
        "Present"
    );
    assert_eq!(
        confirmed_old_relation
            .try_get::<String>("", "relation_presence")
            .unwrap(),
        "ConfirmedAbsent"
    );
    while let Some(claimed) = outbox
        .claim_next(fixture.root_id, "change-projector", Duration::seconds(30))
        .await
        .unwrap()
    {
        StorageChangeProjectionRepository::new(&fixture.database)
            .apply(&claimed)
            .await
            .unwrap();
    }
    let moved_out_availability = fixture
        .database
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("availability_state"))
                    .from(Alias::new("media_locations"))
                    .and_where(
                        Expr::col(Alias::new("storage_object_id")).eq(moved_out_id.as_uuid()),
                    ),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get::<String>("", "availability_state")
        .unwrap();
    assert_eq!(moved_out_availability, "ConfirmedAbsent");
    let confirmed_absent_item = fixture
        .database
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("is_present"))
                    .from(Alias::new("catalog_items"))
                    .and_where(Expr::col(Alias::new("id")).eq(old_parent_item.as_uuid())),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert!(
        !confirmed_absent_item
            .try_get::<bool>("", "is_present")
            .unwrap()
    );
}
