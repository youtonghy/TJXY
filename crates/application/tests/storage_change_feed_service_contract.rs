use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use sea_orm::{
    ConnectionTrait, Statement, TransactionTrait,
    sea_query::{Alias, Expr, Query},
};
use sea_orm_migration::MigratorTrait;
use tjxy_application::{ScopedInventoryService, StorageChangeFeedService};
use tjxy_common::{SortKey, StorageObjectRecordId, StorageRootId};
use tjxy_db::{WorkJobRepository, WorkJobSpec, WorkJobState, WorkScope, WorkTaskKind};
use tjxy_storage::{
    BackendError, ByteRange, ByteStream, ChangeCursor, ChangePage, ObjectPage, PageToken,
    StorageBackend, StorageCapabilities, StorageObject, StorageObjectId,
};
use tjxy_test_support::test_database;
use uuid::Uuid;

struct ChangesBackend {
    cursors: Mutex<Vec<String>>,
    pages: Mutex<VecDeque<ChangePage>>,
    cursor_invalid: Mutex<bool>,
    fresh_cursors: Mutex<VecDeque<ChangeCursor>>,
    inventory_pages: Mutex<VecDeque<ObjectPage>>,
}

#[async_trait::async_trait]
impl StorageBackend for ChangesBackend {
    async fn get_object(&self, _id: &StorageObjectId) -> Result<StorageObject, BackendError> {
        Err(BackendError::NotFound)
    }

    async fn list_children(
        &self,
        _parent: &StorageObjectId,
        _page: Option<PageToken>,
    ) -> Result<ObjectPage, BackendError> {
        self.inventory_pages
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| BackendError::TemporarilyUnavailable {
                message: "no inventory page configured".into(),
            })
    }

    async fn list_changes(&self, cursor: ChangeCursor) -> Result<ChangePage, BackendError> {
        self.cursors
            .lock()
            .unwrap()
            .push(cursor.as_str().to_owned());
        if *self.cursor_invalid.lock().unwrap() {
            return Err(BackendError::ChangeCursorInvalid);
        }
        self.pages
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| BackendError::TemporarilyUnavailable {
                message: "unexpected extra page".into(),
            })
    }

    async fn open_range(
        &self,
        _id: &StorageObjectId,
        _range: ByteRange,
    ) -> Result<ByteStream, BackendError> {
        Err(BackendError::unsupported_capability("range"))
    }

    fn capabilities(&self) -> StorageCapabilities {
        StorageCapabilities::new().with_changes(true)
    }

    async fn latest_change_cursor(&self) -> Result<ChangeCursor, BackendError> {
        self.fresh_cursors
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| BackendError::TemporarilyUnavailable {
                message: "no fresh cursor configured".into(),
            })
    }
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Builds one normalized active Changes root end to end.
async fn service_consumes_all_pages_and_durably_reconciles_terminal_cursor() {
    let database = test_database().await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    let account_id = Uuid::new_v4();
    let root_id = StorageRootId::new();
    let root_object_id = StorageObjectRecordId::new();
    let stale_child_id = StorageObjectRecordId::new();
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
                        "google-drive".into(),
                        "Drive".into(),
                        "account".into(),
                        "credential".into(),
                        "Active".into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    let library_id = Uuid::new_v4();
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
                    .into_table(Alias::new("storage_objects"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("storage_account_id"),
                        Alias::new("provider_drive_id"),
                        Alias::new("provider_object_id"),
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
                        stale_child_id.as_uuid().into(),
                        account_id.into(),
                        "my-drive".into(),
                        "stale-title".into(),
                        "Stale Title".into(),
                        "stale title".into(),
                        "Directory".into(),
                        0_i64.into(),
                        true.into(),
                        1_i64.into(),
                        "ProviderStableId".into(),
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
                        root_object_id.as_uuid().into(),
                        account_id.into(),
                        "my-drive".into(),
                        "root".into(),
                        "Root".into(),
                        "root".into(),
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
    database
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
                        root_id.as_uuid().into(),
                        stale_child_id.as_uuid().into(),
                        root_object_id.as_uuid().into(),
                        0_i64.into(),
                        true.into(),
                        1_i64.into(),
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
                        root_object_id.as_uuid().into(),
                        0_i64.into(),
                        false.into(),
                        0_i64.into(),
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
                        root_id.as_uuid().into(),
                        "Changes".into(),
                        "cursor-one".into(),
                        "Active".into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    let provider = Arc::new(ChangesBackend {
        cursors: Mutex::new(Vec::new()),
        pages: Mutex::new(VecDeque::from([
            ChangePage::continuation(Vec::new(), ChangeCursor::new("cursor-two").unwrap()),
            ChangePage::new(Vec::new(), ChangeCursor::new("cursor-three").unwrap()),
        ])),
        cursor_invalid: Mutex::new(false),
        fresh_cursors: Mutex::new(VecDeque::from([ChangeCursor::new("cursor-fresh").unwrap()])),
        inventory_pages: Mutex::new(VecDeque::from([ObjectPage::complete(Vec::new())])),
    });

    let results = StorageChangeFeedService::new(database.clone(), provider.clone())
        .run_active_roots(account_id, "my-drive")
        .await
        .unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].0, root_id);
    let result = results[0].1;
    assert_eq!(result.pages(), 2);
    assert_eq!(
        provider.cursors.lock().unwrap().as_slice(),
        ["cursor-one", "cursor-two"]
    );
    let state = database
        .query_one(Statement::from_string(
            database.get_database_backend(),
            "SELECT sync_revision, reconciled_sync_revision FROM storage_roots".to_owned(),
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(state.try_get::<i64>("", "sync_revision").unwrap(), 2);
    assert_eq!(
        state
            .try_get::<i64>("", "reconciled_sync_revision")
            .unwrap(),
        2
    );
    let cursor = database
        .query_one(Statement::from_string(
            database.get_database_backend(),
            "SELECT cursor_value FROM storage_sync_cursors".to_owned(),
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        cursor.try_get::<String>("", "cursor_value").unwrap(),
        "cursor-three"
    );

    let jobs = WorkJobRepository::new(&database);
    let ordinary_root_sync = jobs
        .enqueue_or_join(
            &WorkJobSpec::new(
                WorkTaskKind::ScopedStorageSync,
                WorkScope::StorageRoot(root_id),
                2,
                1,
            )
            .unwrap(),
        )
        .await
        .unwrap()
        .job()
        .id();
    *provider.cursor_invalid.lock().unwrap() = true;
    let recovery = StorageChangeFeedService::new(database.clone(), provider.clone())
        .run_root(root_id, account_id, "my-drive")
        .await
        .unwrap();

    assert!(recovery.recovery_scheduled());
    assert_eq!(recovery.pages(), 0);
    let state = database
        .query_one(Statement::from_string(
            database.get_database_backend(),
            "SELECT sync_revision FROM storage_roots".to_owned(),
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(state.try_get::<i64>("", "sync_revision").unwrap(), 2);
    let cursor = database
        .query_one(Statement::from_string(
            database.get_database_backend(),
            "SELECT cursor_value, status, recovery_job_id FROM storage_sync_cursors".to_owned(),
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        cursor.try_get::<String>("", "cursor_value").unwrap(),
        "cursor-fresh"
    );
    assert_eq!(
        cursor.try_get::<String>("", "status").unwrap(),
        "Recovering"
    );
    let recovery_job_id = cursor.try_get::<Uuid>("", "recovery_job_id").unwrap();
    assert_ne!(recovery_job_id, ordinary_root_sync.as_uuid());
    let job = database
        .query_one(Statement::from_string(
            database.get_database_backend(),
            "SELECT task_kind, scope_type, scope_id, expected_revision, state FROM work_jobs WHERE id = (SELECT recovery_job_id FROM storage_sync_cursors)".to_owned(),
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        job.try_get::<String>("", "task_kind").unwrap(),
        "RecoverStorageCursor"
    );
    assert_eq!(
        job.try_get::<String>("", "scope_type").unwrap(),
        "StorageRoot"
    );
    assert_eq!(
        job.try_get::<Uuid>("", "scope_id").unwrap(),
        root_id.as_uuid()
    );
    assert_eq!(job.try_get::<i64>("", "expected_revision").unwrap(), 2);
    assert_eq!(job.try_get::<String>("", "state").unwrap(), "Pending");

    *provider.cursor_invalid.lock().unwrap() = false;
    let claimed = jobs
        .claim_next_scoped_sync_for_drive(
            account_id,
            "my-drive",
            "recovery-worker",
            chrono::Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    let wrong_root = database.begin().await.unwrap();
    assert!(
        tjxy_db::activate_storage_cursor_recovery(&wrong_root, &claimed, StorageRootId::new(),)
            .await
            .is_err()
    );
    wrong_root.rollback().await.unwrap();
    let replacement_owner = Uuid::new_v4();
    let replace_owner = Query::update()
        .table(Alias::new("storage_sync_cursors"))
        .value(Alias::new("recovery_job_id"), replacement_owner)
        .and_where(Expr::col(Alias::new("storage_root_id")).eq(root_id.as_uuid()))
        .to_owned();
    database
        .execute(database.get_database_backend().build(&replace_owner))
        .await
        .unwrap();
    let error = ScopedInventoryService::new(database.clone(), provider.clone())
        .run_claimed(&claimed, account_id)
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        tjxy_application::ScopedInventoryError::CursorRecovery(
            tjxy_db::StorageChangeFeedRepositoryError::CursorConflict
        )
    ));
    assert_eq!(
        jobs.get(claimed.id()).await.unwrap().unwrap().state(),
        WorkJobState::Running
    );
    let cursor = database
        .query_one(Statement::from_string(
            database.get_database_backend(),
            "SELECT status, recovery_job_id FROM storage_sync_cursors".to_owned(),
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        cursor.try_get::<String>("", "status").unwrap(),
        "Recovering"
    );
    assert_eq!(
        cursor.try_get::<Uuid>("", "recovery_job_id").unwrap(),
        replacement_owner
    );
    let restore_owner = Query::update()
        .table(Alias::new("storage_sync_cursors"))
        .value(Alias::new("recovery_job_id"), claimed.id().as_uuid())
        .and_where(Expr::col(Alias::new("storage_root_id")).eq(root_id.as_uuid()))
        .to_owned();
    database
        .execute(database.get_database_backend().build(&restore_owner))
        .await
        .unwrap();
    provider
        .inventory_pages
        .lock()
        .unwrap()
        .push_back(ObjectPage::complete(Vec::new()));
    ScopedInventoryService::new(database.clone(), provider.clone())
        .run_claimed(&claimed, account_id)
        .await
        .unwrap();

    assert_eq!(
        jobs.get(claimed.id()).await.unwrap().unwrap().state(),
        WorkJobState::Completed
    );
    let cursor = database
        .query_one(Statement::from_string(
            database.get_database_backend(),
            "SELECT status, recovery_job_id, last_success_at, last_full_sync_at FROM storage_sync_cursors".to_owned(),
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(cursor.try_get::<String>("", "status").unwrap(), "Active");
    assert!(
        cursor
            .try_get::<Option<Uuid>>("", "recovery_job_id")
            .unwrap()
            .is_none()
    );
    assert!(
        cursor
            .try_get::<Option<chrono::DateTime<chrono::Utc>>>("", "last_success_at")
            .unwrap()
            .is_some()
    );
    assert!(
        cursor
            .try_get::<Option<chrono::DateTime<chrono::Utc>>>("", "last_full_sync_at")
            .unwrap()
            .is_some()
    );
    let stale_relation_query = Query::select()
        .columns([Alias::new("presence_state"), Alias::new("children_indexed")])
        .from(Alias::new("storage_root_objects"))
        .and_where(Expr::col(Alias::new("storage_object_id")).eq(stale_child_id.as_uuid()))
        .to_owned();
    let stale_relation = database
        .query_one(database.get_database_backend().build(&stale_relation_query))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        stale_relation
            .try_get::<String>("", "presence_state")
            .unwrap(),
        "ConfirmedAbsent"
    );
    assert!(
        !stale_relation
            .try_get::<bool>("", "children_indexed")
            .unwrap()
    );

    *provider.cursor_invalid.lock().unwrap() = true;
    provider
        .fresh_cursors
        .lock()
        .unwrap()
        .push_back(ChangeCursor::new("cursor-fresh-two").unwrap());
    StorageChangeFeedService::new(database.clone(), provider.clone())
        .run_root(root_id, account_id, "my-drive")
        .await
        .unwrap();
    *provider.cursor_invalid.lock().unwrap() = false;
    let failed_claim = jobs
        .claim_next_scoped_sync_for_drive(
            account_id,
            "my-drive",
            "failing-recovery-worker",
            chrono::Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    ScopedInventoryService::new(database.clone(), provider.clone())
        .fail_terminal(&failed_claim, "invalid recovery pagination")
        .await
        .unwrap();

    assert_eq!(
        jobs.get(failed_claim.id()).await.unwrap().unwrap().state(),
        WorkJobState::Failed
    );
    let failed_cursor = database
        .query_one(Statement::from_string(
            database.get_database_backend(),
            "SELECT status, recovery_job_id FROM storage_sync_cursors".to_owned(),
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        failed_cursor.try_get::<String>("", "status").unwrap(),
        "RecoveryFailed"
    );
    assert_eq!(
        failed_cursor
            .try_get::<Uuid>("", "recovery_job_id")
            .unwrap(),
        failed_claim.id().as_uuid()
    );

    let resumed_job = StorageChangeFeedService::new(database.clone(), provider)
        .resume_failed_recovery(root_id, account_id, "my-drive")
        .await
        .unwrap();
    assert_ne!(resumed_job, failed_claim.id());
    let resumed_cursor = database
        .query_one(Statement::from_string(
            database.get_database_backend(),
            "SELECT status, recovery_job_id FROM storage_sync_cursors".to_owned(),
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        resumed_cursor.try_get::<String>("", "status").unwrap(),
        "Recovering"
    );
    assert_eq!(
        resumed_cursor
            .try_get::<Uuid>("", "recovery_job_id")
            .unwrap(),
        resumed_job.as_uuid()
    );
}
