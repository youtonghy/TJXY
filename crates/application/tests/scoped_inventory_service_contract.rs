use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use sea_orm::{
    ConnectionTrait, DatabaseConnection, Statement,
    sea_query::{Alias, Expr, Query},
};
use sea_orm_migration::MigratorTrait;
use tjxy_application::{DiscoverTitlesService, FullValidateStorageService, ScopedInventoryService};
use tjxy_common::{CatalogItemId, SortKey, StorageObjectRecordId, StorageRootId};
use tjxy_db::{StorageSyncRepository, WorkJobRepository, WorkJobSpec, WorkScope, WorkTaskKind};
use tjxy_storage::{
    BackendError, ByteRange, ByteStream, ChangeCursor, ChangePage, ObjectPage, PageToken,
    StorageBackend, StorageCapabilities, StorageObject, StorageObjectId,
};
use tjxy_test_support::test_database;
use uuid::Uuid;

struct TwoPageBackend {
    parent_id: StorageObjectId,
    requests: Mutex<Vec<Option<String>>>,
    fail_second_page: bool,
    first_name: &'static str,
}

#[async_trait::async_trait]
impl StorageBackend for TwoPageBackend {
    async fn get_object(&self, _id: &StorageObjectId) -> Result<StorageObject, BackendError> {
        Err(BackendError::NotFound)
    }

    async fn list_children(
        &self,
        parent: &StorageObjectId,
        page: Option<PageToken>,
    ) -> Result<ObjectPage, BackendError> {
        assert_eq!(parent, &self.parent_id);
        self.requests
            .lock()
            .unwrap()
            .push(page.as_ref().map(|token| token.as_str().to_owned()));
        match page {
            None => Ok(ObjectPage {
                objects: vec![StorageObject::directory(
                    StorageObjectId::new("filesystem", "series").unwrap(),
                    self.first_name,
                )],
                next_page: Some(PageToken::new("page-2").unwrap()),
            }),
            Some(token) if token.as_str() == "page-2" && self.fail_second_page => {
                Err(BackendError::TemporarilyUnavailable {
                    message: "fixture failure".to_owned(),
                })
            }
            Some(token) if token.as_str() == "page-2" => {
                Ok(ObjectPage::complete(vec![StorageObject::file(
                    StorageObjectId::new("filesystem", "movie").unwrap(),
                    "Movie.mkv",
                    8,
                )]))
            }
            _ => panic!("unexpected page token"),
        }
    }

    async fn list_changes(&self, _cursor: ChangeCursor) -> Result<ChangePage, BackendError> {
        Err(BackendError::unsupported_capability("changes"))
    }

    async fn open_range(
        &self,
        _id: &StorageObjectId,
        _range: ByteRange,
    ) -> Result<ByteStream, BackendError> {
        Err(BackendError::unsupported_capability("range reads"))
    }

    fn capabilities(&self) -> StorageCapabilities {
        StorageCapabilities::new()
    }
}

struct SinglePageBackend {
    parent_id: StorageObjectId,
}

struct TreeBackend {
    requests: Mutex<Vec<String>>,
}

struct SequenceBackend {
    parent_id: StorageObjectId,
    pages: Mutex<VecDeque<Vec<StorageObject>>>,
}

#[async_trait::async_trait]
impl StorageBackend for SequenceBackend {
    async fn get_object(&self, _id: &StorageObjectId) -> Result<StorageObject, BackendError> {
        Err(BackendError::NotFound)
    }

    async fn list_children(
        &self,
        parent: &StorageObjectId,
        page: Option<PageToken>,
    ) -> Result<ObjectPage, BackendError> {
        assert_eq!(parent, &self.parent_id);
        assert!(page.is_none());
        Ok(ObjectPage::complete(
            self.pages.lock().unwrap().pop_front().unwrap(),
        ))
    }

    async fn list_changes(&self, _cursor: ChangeCursor) -> Result<ChangePage, BackendError> {
        Err(BackendError::unsupported_capability("changes"))
    }

    async fn open_range(
        &self,
        _id: &StorageObjectId,
        _range: ByteRange,
    ) -> Result<ByteStream, BackendError> {
        Err(BackendError::unsupported_capability("range reads"))
    }

    fn capabilities(&self) -> StorageCapabilities {
        StorageCapabilities::new()
    }
}

#[async_trait::async_trait]
impl StorageBackend for TreeBackend {
    async fn get_object(&self, _id: &StorageObjectId) -> Result<StorageObject, BackendError> {
        Err(BackendError::NotFound)
    }

    async fn list_children(
        &self,
        parent: &StorageObjectId,
        page: Option<PageToken>,
    ) -> Result<ObjectPage, BackendError> {
        assert!(page.is_none());
        self.requests
            .lock()
            .unwrap()
            .push(parent.provider_object_id().to_owned());
        match parent.provider_object_id() {
            "root" => Ok(ObjectPage::complete(vec![StorageObject::directory(
                StorageObjectId::new("filesystem", "series").unwrap(),
                "Series",
            )])),
            "series" => Ok(ObjectPage::complete(vec![StorageObject::file(
                StorageObjectId::new("filesystem", "episode").unwrap(),
                "Episode.mkv",
                8,
            )])),
            other => panic!("unexpected validation scope {other}"),
        }
    }

    async fn list_changes(&self, _cursor: ChangeCursor) -> Result<ChangePage, BackendError> {
        Err(BackendError::unsupported_capability("changes"))
    }

    async fn open_range(
        &self,
        _id: &StorageObjectId,
        _range: ByteRange,
    ) -> Result<ByteStream, BackendError> {
        Err(BackendError::unsupported_capability("range reads"))
    }

    fn capabilities(&self) -> StorageCapabilities {
        StorageCapabilities::new()
    }
}

#[async_trait::async_trait]
impl StorageBackend for SinglePageBackend {
    async fn get_object(&self, _id: &StorageObjectId) -> Result<StorageObject, BackendError> {
        Err(BackendError::NotFound)
    }

    async fn list_children(
        &self,
        parent: &StorageObjectId,
        page: Option<PageToken>,
    ) -> Result<ObjectPage, BackendError> {
        assert_eq!(parent, &self.parent_id);
        assert!(page.is_none());
        Ok(ObjectPage::complete(vec![StorageObject::file(
            StorageObjectId::new("filesystem", "movie").unwrap(),
            "Movie changed.mkv",
            8,
        )]))
    }

    async fn list_changes(&self, _cursor: ChangeCursor) -> Result<ChangePage, BackendError> {
        Err(BackendError::unsupported_capability("changes"))
    }

    async fn open_range(
        &self,
        _id: &StorageObjectId,
        _range: ByteRange,
    ) -> Result<ByteStream, BackendError> {
        Err(BackendError::unsupported_capability("range reads"))
    }

    fn capabilities(&self) -> StorageCapabilities {
        StorageCapabilities::new()
    }
}

struct Fixture {
    database: DatabaseConnection,
    account_id: Uuid,
    library_id: Uuid,
    root_id: StorageRootId,
    parent_record_id: StorageObjectRecordId,
    parent_backend_id: StorageObjectId,
}

#[allow(clippy::too_many_lines)] // Mirrors the normalized account/root/object binding boundary.
async fn fixture() -> Fixture {
    let database = test_database().await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    let account_id = Uuid::new_v4();
    let root_id = StorageRootId::new();
    let parent_record_id = StorageObjectRecordId::new();
    let parent_backend_id = StorageObjectId::new("filesystem", "root").unwrap();
    let library_id = Uuid::new_v4();
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
                        "Filesystem".into(),
                        Uuid::new_v4().to_string().into(),
                        "local".into(),
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
                        "Movies".into(),
                        "Lazy".into(),
                        "title_layer".into(),
                        "basic".into(),
                        "on_browse".into(),
                        "on_playback".into(),
                        1.into(),
                        "movies".into(),
                        SortKey::from_text("Movies").into_bytes().into(),
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
                        parent_record_id.as_uuid().into(),
                        account_id.into(),
                        "fixture-drive".into(),
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
                        Alias::new("observed_sync_revision"),
                        Alias::new("children_indexed"),
                        Alias::new("children_index_revision"),
                        Alias::new("presence_state"),
                    ])
                    .values_panic([
                        Uuid::new_v4().into(),
                        root_id.as_uuid().into(),
                        parent_record_id.as_uuid().into(),
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
        account_id,
        library_id,
        root_id,
        parent_record_id,
        parent_backend_id,
    }
}

#[tokio::test]
async fn filesystem_event_scopes_schedule_only_materialized_directories_and_join_bursts() {
    let fixture = fixture().await;
    let unknown = StorageObjectId::new("filesystem", "unknown").unwrap();
    let repository = StorageSyncRepository::new(&fixture.database);

    let first = repository
        .enqueue_event_scopes(
            fixture.account_id,
            "fixture-drive",
            &[fixture.parent_backend_id.clone(), unknown.clone()],
            90,
        )
        .await
        .unwrap();
    let repeated = repository
        .enqueue_event_scopes(
            fixture.account_id,
            "fixture-drive",
            std::slice::from_ref(&fixture.parent_backend_id),
            90,
        )
        .await
        .unwrap();
    let wrong_account = repository
        .enqueue_event_scopes(Uuid::new_v4(), "fixture-drive", &[unknown], 90)
        .await
        .unwrap();

    assert_eq!(first.len(), 1);
    assert!(first[0].created());
    assert_eq!(repeated.len(), 1);
    assert!(!repeated[0].created());
    assert_eq!(first[0].job().id(), repeated[0].job().id());
    assert!(wrong_account.is_empty());
}

#[tokio::test]
async fn path_weak_replacement_creates_a_pending_relink_candidate_without_merging_ids() {
    let fixture = fixture().await;
    let modified = chrono::Utc::now();
    let old = StorageObject::file_with_identity(
        StorageObjectId::new("filesystem", "old-path").unwrap(),
        "Old.mkv",
        8,
        tjxy_storage::IdentityQuality::PathWeak,
    )
    .with_remote_modified_at(modified)
    .with_remote_revision("old-revision")
    .unwrap();
    let replacement = StorageObject::file_with_identity(
        StorageObjectId::new("filesystem", "new-path").unwrap(),
        "Renamed.mkv",
        8,
        tjxy_storage::IdentityQuality::PathWeak,
    )
    .with_remote_modified_at(modified)
    .with_remote_revision("new-revision")
    .unwrap();
    let backend = Arc::new(SequenceBackend {
        parent_id: fixture.parent_backend_id.clone(),
        pages: Mutex::new(VecDeque::from([vec![old], vec![replacement]])),
    });
    let service = ScopedInventoryService::new(fixture.database.clone(), backend);
    let work = WorkJobRepository::new(&fixture.database);

    let mut revision = 0;
    for owner in ["weak-first", "weak-second"] {
        work.enqueue_or_join(
            &WorkJobSpec::new(
                WorkTaskKind::ScopedStorageSync,
                WorkScope::StorageObject(fixture.parent_record_id),
                revision,
                90,
            )
            .unwrap(),
        )
        .await
        .unwrap();
        let claimed = work
            .claim_next(
                &[WorkTaskKind::ScopedStorageSync],
                owner,
                chrono::Duration::minutes(1),
            )
            .await
            .unwrap()
            .unwrap();
        revision = service
            .run_claimed(&claimed, fixture.account_id)
            .await
            .unwrap()
            .sync_revision();
    }

    let rows = fixture
        .database
        .query_all(Statement::from_string(
            fixture.database.get_database_backend(),
            "SELECT c.state, old.provider_object_id AS old_id, new.provider_object_id AS new_id \
             FROM storage_relink_candidates c \
             JOIN storage_objects old ON old.id = c.previous_storage_object_id \
             JOIN storage_objects new ON new.id = c.replacement_storage_object_id"
                .to_owned(),
        ))
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].try_get::<String>("", "state").unwrap(), "Pending");
    assert_eq!(rows[0].try_get::<String>("", "old_id").unwrap(), "old-path");
    assert_eq!(rows[0].try_get::<String>("", "new_id").unwrap(), "new-path");
}

async fn scope_presence(fixture: &Fixture) -> (String, Option<String>) {
    let query = Query::select()
        .columns([
            Alias::new("presence_state"),
            Alias::new("availability_reason"),
        ])
        .from(Alias::new("storage_root_objects"))
        .and_where(Expr::col(Alias::new("storage_root_id")).eq(fixture.root_id.as_uuid()))
        .and_where(
            Expr::col(Alias::new("storage_object_id")).eq(fixture.parent_record_id.as_uuid()),
        )
        .to_owned();
    let row = fixture
        .database
        .query_one(fixture.database.get_database_backend().build(&query))
        .await
        .unwrap()
        .unwrap();
    (
        row.try_get("", "presence_state").unwrap(),
        row.try_get("", "availability_reason").unwrap(),
    )
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Keeps recursive inventory, final sweep, and catalog projection in one contract.
async fn full_validate_recurses_and_confirms_unreachable_subtrees_absent() {
    let fixture = fixture().await;
    let backend = fixture.database.get_database_backend();
    let stale_directory = StorageObjectRecordId::new();
    let stale_file = StorageObjectRecordId::new();
    for (id, provider_id, name, object_type, parent) in [
        (
            stale_directory,
            "stale-directory",
            "Stale Directory",
            "Directory",
            fixture.parent_record_id,
        ),
        (
            stale_file,
            "stale-file",
            "Stale.mkv",
            "File",
            stale_directory,
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
                            name.into(),
                            name.to_lowercase().into(),
                            object_type.into(),
                            0_i64.into(),
                            (object_type == "Directory").into(),
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
                            parent.as_uuid().into(),
                            0_i64.into(),
                            (object_type == "Directory").into(),
                            0_i64.into(),
                            "Present".into(),
                        ]),
                ),
            )
            .await
            .unwrap();
    }
    let stale_item = CatalogItemId::new();
    let stale_source = Uuid::new_v4();
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
                        stale_item.as_uuid().into(),
                        "Movie".into(),
                        "Stale".into(),
                        "stale".into(),
                        SortKey::from_text("Stale").into_bytes().into(),
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
                    .into_table(Alias::new("media_sources"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("catalog_item_id"),
                        Alias::new("presentation_key"),
                        Alias::new("probe_state"),
                        Alias::new("probe_revision"),
                    ])
                    .values_panic([
                        stale_source.into(),
                        stale_item.as_uuid().into(),
                        Uuid::new_v4().into(),
                        "Probed".into(),
                        1_i64.into(),
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
                        stale_source.into(),
                        stale_file.as_uuid().into(),
                        0_i32.into(),
                        "Available".into(),
                    ]),
            ),
        )
        .await
        .unwrap();
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
            "validate-worker",
            chrono::Duration::minutes(1),
        )
        .await
        .unwrap()
        .unwrap();
    let storage = Arc::new(TreeBackend {
        requests: Mutex::new(Vec::new()),
    });

    let result = FullValidateStorageService::new(fixture.database.clone(), Arc::clone(&storage))
        .run_claimed(&claimed, fixture.account_id)
        .await
        .unwrap();

    assert_eq!(result.directory_count(), 2);
    assert_eq!(result.object_count(), 2);
    assert_eq!(
        *storage.requests.lock().unwrap(),
        vec!["root".to_owned(), "series".to_owned()]
    );
    let stale_query = Query::select()
        .columns([
            Alias::new("storage_object_id"),
            Alias::new("presence_state"),
        ])
        .from(Alias::new("storage_root_objects"))
        .and_where(Expr::col(Alias::new("storage_root_id")).eq(fixture.root_id.as_uuid()))
        .and_where(
            Expr::col(Alias::new("storage_object_id"))
                .is_in([stale_directory.as_uuid(), stale_file.as_uuid()]),
        )
        .order_by(
            Alias::new("storage_object_id"),
            sea_orm::sea_query::Order::Asc,
        )
        .to_owned();
    let stale = fixture
        .database
        .query_all(fixture.database.get_database_backend().build(&stale_query))
        .await
        .unwrap();
    assert_eq!(stale.len(), 2);
    assert!(
        stale.iter().all(|row| {
            row.try_get::<String>("", "presence_state").unwrap() == "ConfirmedAbsent"
        })
    );
    let item = Alias::new("catalog_items");
    let source = Alias::new("media_sources");
    let location = Alias::new("media_locations");
    let projected_query = Query::select()
        .column((item.clone(), Alias::new("is_present")))
        .column((source.clone(), Alias::new("probe_state")))
        .column((location.clone(), Alias::new("availability_state")))
        .from(item.clone())
        .inner_join(
            source.clone(),
            Expr::col((source.clone(), Alias::new("catalog_item_id")))
                .equals((item.clone(), Alias::new("id"))),
        )
        .inner_join(
            location.clone(),
            Expr::col((location.clone(), Alias::new("media_source_id")))
                .equals((source, Alias::new("id"))),
        )
        .and_where(Expr::col((item, Alias::new("id"))).eq(stale_item.as_uuid()))
        .to_owned();
    let projected = fixture
        .database
        .query_one(
            fixture
                .database
                .get_database_backend()
                .build(&projected_query),
        )
        .await
        .unwrap()
        .unwrap();
    assert!(!projected.try_get::<bool>("", "is_present").unwrap());
    assert_eq!(
        projected.try_get::<String>("", "probe_state").unwrap(),
        "Stale"
    );
    assert_eq!(
        projected
            .try_get::<String>("", "availability_state")
            .unwrap(),
        "ConfirmedAbsent"
    );
    assert_eq!(
        jobs.get(claimed.id()).await.unwrap().unwrap().state(),
        tjxy_db::WorkJobState::Completed
    );
    let root = fixture
        .database
        .query_one(Statement::from_string(
            fixture.database.get_database_backend(),
            "SELECT sync_revision, reconciled_sync_revision FROM storage_roots".to_owned(),
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        root.try_get::<i64>("", "sync_revision").unwrap(),
        result.sync_revision()
    );
    assert_eq!(
        root.try_get::<i64>("", "reconciled_sync_revision").unwrap(),
        result.sync_revision()
    );
}

#[tokio::test]
async fn inventory_lists_only_the_requested_scope_commits_pages_and_completes_the_job() {
    let fixture = fixture().await;
    let work = WorkJobRepository::new(&fixture.database);
    work.enqueue_or_join(
        &WorkJobSpec::new(
            WorkTaskKind::ScopedStorageSync,
            WorkScope::StorageObject(fixture.parent_record_id),
            0,
            100,
        )
        .unwrap(),
    )
    .await
    .unwrap();
    let claimed = work
        .claim_next(
            &[WorkTaskKind::ScopedStorageSync],
            "inventory-worker",
            chrono::Duration::minutes(1),
        )
        .await
        .unwrap()
        .unwrap();
    let backend = Arc::new(TwoPageBackend {
        parent_id: fixture.parent_backend_id.clone(),
        requests: Mutex::new(Vec::new()),
        fail_second_page: false,
        first_name: "Series",
    });
    let service = ScopedInventoryService::new(fixture.database.clone(), Arc::clone(&backend));

    let result = service
        .run_claimed(&claimed, fixture.account_id)
        .await
        .unwrap();

    assert_eq!(result.sync_revision(), 2);
    assert_eq!(result.object_count(), 2);
    assert_eq!(
        *backend.requests.lock().unwrap(),
        vec![None, Some("page-2".to_owned())]
    );
    assert_eq!(
        work.get(claimed.id()).await.unwrap().unwrap().state(),
        tjxy_db::WorkJobState::Completed
    );
    let counts = fixture
        .database
        .query_one(Statement::from_string(
            fixture.database.get_database_backend(),
            "SELECT (SELECT COUNT(*) FROM storage_objects) AS objects, \
             (SELECT COUNT(*) FROM storage_change_outbox) AS events"
                .to_owned(),
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(counts.try_get::<i64>("", "objects").unwrap(), 3);
    assert_eq!(counts.try_get::<i64>("", "events").unwrap(), 4);
    let reconciled: i64 = fixture
        .database
        .query_one(Statement::from_string(
            fixture.database.get_database_backend(),
            "SELECT reconciled_sync_revision FROM storage_roots".to_owned(),
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get("", "reconciled_sync_revision")
        .unwrap();
    assert_eq!(reconciled, result.sync_revision());
    let discovery = work
        .claim_next(
            &[WorkTaskKind::DiscoverTitles],
            "discover-worker",
            chrono::Duration::minutes(1),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        discovery.job().scope(),
        WorkScope::StorageRoot(fixture.root_id)
    );
    assert_eq!(discovery.job().expected_revision(), result.sync_revision());
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Keeps the failed-page transaction boundary explicit.
async fn failed_later_page_does_not_complete_the_scope_or_work_job() {
    let fixture = fixture().await;
    fixture
        .database
        .execute(
            fixture.database.get_database_backend().build(
                Query::update()
                    .table(Alias::new("storage_root_objects"))
                    .value(Alias::new("children_indexed"), true)
                    .value(Alias::new("children_index_revision"), 1_i64)
                    .and_where(
                        sea_orm::sea_query::Expr::col(Alias::new("storage_object_id"))
                            .eq(fixture.parent_record_id.as_uuid()),
                    ),
            ),
        )
        .await
        .unwrap();
    let work = WorkJobRepository::new(&fixture.database);
    work.enqueue_or_join(
        &WorkJobSpec::new(
            WorkTaskKind::ScopedStorageSync,
            WorkScope::StorageObject(fixture.parent_record_id),
            0,
            100,
        )
        .unwrap(),
    )
    .await
    .unwrap();
    let claimed = work
        .claim_next(
            &[WorkTaskKind::ScopedStorageSync],
            "inventory-worker",
            chrono::Duration::minutes(1),
        )
        .await
        .unwrap()
        .unwrap();
    let backend = Arc::new(TwoPageBackend {
        parent_id: fixture.parent_backend_id.clone(),
        requests: Mutex::new(Vec::new()),
        fail_second_page: true,
        first_name: "Series",
    });
    let service = ScopedInventoryService::new(fixture.database.clone(), backend);

    assert!(
        service
            .run(
                &claimed,
                fixture.root_id,
                fixture.parent_record_id,
                "fixture-drive",
                &fixture.parent_backend_id,
            )
            .await
            .is_err()
    );
    assert_eq!(
        work.get(claimed.id()).await.unwrap().unwrap().state(),
        tjxy_db::WorkJobState::Running
    );
    let root = Alias::new("storage_roots");
    let relation = Alias::new("storage_root_objects");
    let state_query = Query::select()
        .column((root.clone(), Alias::new("sync_revision")))
        .column((root.clone(), Alias::new("reconciled_sync_revision")))
        .column((relation.clone(), Alias::new("children_indexed")))
        .column((relation.clone(), Alias::new("presence_state")))
        .column((relation.clone(), Alias::new("availability_reason")))
        .from(root.clone())
        .inner_join(
            relation.clone(),
            Expr::col((relation.clone(), Alias::new("storage_root_id")))
                .equals((root, Alias::new("id"))),
        )
        .and_where(
            Expr::col((relation, Alias::new("storage_object_id")))
                .eq(fixture.parent_record_id.as_uuid()),
        )
        .to_owned();
    let state = fixture
        .database
        .query_one(fixture.database.get_database_backend().build(&state_query))
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
    assert!(!state.try_get::<bool>("", "children_indexed").unwrap());
    assert_eq!(
        state.try_get::<String>("", "presence_state").unwrap(),
        "TemporarilyUnavailable"
    );
    assert_eq!(
        state.try_get::<String>("", "availability_reason").unwrap(),
        "backend-temporarily-unavailable"
    );
    assert!(
        work.claim_next(
            &[WorkTaskKind::DiscoverTitles],
            "discover-worker",
            chrono::Duration::minutes(1),
        )
        .await
        .unwrap()
        .is_none()
    );
}

#[tokio::test]
async fn retry_after_a_partial_inventory_uses_a_new_page_generation() {
    let fixture = fixture().await;
    let work = WorkJobRepository::new(&fixture.database);
    work.enqueue_or_join(
        &WorkJobSpec::new(
            WorkTaskKind::ScopedStorageSync,
            WorkScope::StorageObject(fixture.parent_record_id),
            0,
            100,
        )
        .unwrap(),
    )
    .await
    .unwrap();
    let first_claim = work
        .claim_next(
            &[WorkTaskKind::ScopedStorageSync],
            "inventory-worker-one",
            chrono::Duration::minutes(1),
        )
        .await
        .unwrap()
        .unwrap();
    let failing_backend = Arc::new(TwoPageBackend {
        parent_id: fixture.parent_backend_id.clone(),
        requests: Mutex::new(Vec::new()),
        fail_second_page: true,
        first_name: "Series",
    });
    assert!(
        ScopedInventoryService::new(fixture.database.clone(), failing_backend)
            .run_claimed(&first_claim, fixture.account_id)
            .await
            .is_err()
    );
    assert_eq!(
        scope_presence(&fixture).await,
        (
            "TemporarilyUnavailable".to_owned(),
            Some("backend-temporarily-unavailable".to_owned()),
        )
    );
    work.retry(
        &first_claim,
        chrono::Duration::zero(),
        "retry partial inventory",
    )
    .await
    .unwrap();
    let retry_claim = work
        .claim_next(
            &[WorkTaskKind::ScopedStorageSync],
            "inventory-worker-two",
            chrono::Duration::minutes(1),
        )
        .await
        .unwrap()
        .unwrap();
    let healthy_backend = Arc::new(SinglePageBackend {
        parent_id: fixture.parent_backend_id.clone(),
    });

    ScopedInventoryService::new(fixture.database.clone(), healthy_backend)
        .run_claimed(&retry_claim, fixture.account_id)
        .await
        .unwrap();

    let identities = fixture
        .database
        .query_all(Statement::from_string(
            fixture.database.get_database_backend(),
            "SELECT page_identity FROM storage_sync_pages ORDER BY sync_revision".to_owned(),
        ))
        .await
        .unwrap()
        .into_iter()
        .map(|row| row.try_get::<String>("", "page_identity").unwrap())
        .collect::<Vec<_>>();
    assert_eq!(identities, ["attempt:1:initial", "attempt:2:initial"]);
    let stale_series = fixture
        .database
        .query_one(Statement::from_string(
            fixture.database.get_database_backend(),
            "SELECT ro.presence_state \
             FROM storage_root_objects ro \
             JOIN storage_objects o ON o.id = ro.storage_object_id \
             WHERE o.provider_object_id = 'series'"
                .to_owned(),
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        stale_series
            .try_get::<String>("", "presence_state")
            .unwrap(),
        "ConfirmedAbsent"
    );
    assert_eq!(scope_presence(&fixture).await, ("Present".to_owned(), None));
    assert_eq!(
        work.get(retry_claim.id()).await.unwrap().unwrap().state(),
        tjxy_db::WorkJobState::Completed
    );
}

#[tokio::test]
async fn ordinary_root_scoped_inventory_completes_without_a_recovery_cursor() {
    let fixture = fixture().await;
    let work = WorkJobRepository::new(&fixture.database);
    work.enqueue_or_join(
        &WorkJobSpec::new(
            WorkTaskKind::ScopedStorageSync,
            WorkScope::StorageRoot(fixture.root_id),
            0,
            100,
        )
        .unwrap(),
    )
    .await
    .unwrap();
    let claimed = work
        .claim_next(
            &[WorkTaskKind::ScopedStorageSync],
            "root-inventory-worker",
            chrono::Duration::minutes(1),
        )
        .await
        .unwrap()
        .unwrap();
    let backend = Arc::new(TwoPageBackend {
        parent_id: fixture.parent_backend_id.clone(),
        requests: Mutex::new(Vec::new()),
        fail_second_page: false,
        first_name: "Series",
    });

    ScopedInventoryService::new(fixture.database.clone(), backend)
        .run_claimed(&claimed, fixture.account_id)
        .await
        .unwrap();

    assert_eq!(
        work.get(claimed.id()).await.unwrap().unwrap().state(),
        tjxy_db::WorkJobState::Completed
    );
}

#[tokio::test]
async fn manual_named_title_layer_sync_still_schedules_title_discovery() {
    let fixture = fixture().await;
    let sql = fixture.database.get_database_backend();
    fixture
        .database
        .execute(
            sql.build(
                Query::update()
                    .table(Alias::new("libraries"))
                    .value(Alias::new("scan_profile"), "Manual"),
            ),
        )
        .await
        .unwrap();
    let work = WorkJobRepository::new(&fixture.database);
    work.enqueue_or_join(
        &WorkJobSpec::new(
            WorkTaskKind::ScopedStorageSync,
            WorkScope::StorageObject(fixture.parent_record_id),
            0,
            100,
        )
        .unwrap(),
    )
    .await
    .unwrap();
    let claimed = work
        .claim_next(
            &[WorkTaskKind::ScopedStorageSync],
            "inventory-worker",
            chrono::Duration::minutes(1),
        )
        .await
        .unwrap()
        .unwrap();
    let backend = Arc::new(TwoPageBackend {
        parent_id: fixture.parent_backend_id.clone(),
        requests: Mutex::new(Vec::new()),
        fail_second_page: false,
        first_name: "Series",
    });

    ScopedInventoryService::new(fixture.database.clone(), backend)
        .run_claimed(&claimed, fixture.account_id)
        .await
        .unwrap();

    let discovery = work
        .claim_next(
            &[WorkTaskKind::DiscoverTitles],
            "discover-worker",
            chrono::Duration::minutes(1),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        discovery.job().scope(),
        WorkScope::StorageRoot(fixture.root_id)
    );
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Keeps the shared-root policy boundary explicit.
async fn automatic_discovery_does_not_publish_into_a_manual_library_sharing_the_root() {
    let fixture = fixture().await;
    let sql = fixture.database.get_database_backend();
    let manual_library = Uuid::new_v4();
    fixture
        .database
        .execute(
            sql.build(
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
                        manual_library.into(),
                        "Manual Movies".into(),
                        "Manual".into(),
                        "library_roots".into(),
                        "none".into(),
                        "manual".into(),
                        "manual".into(),
                        1.into(),
                        "movies".into(),
                        SortKey::from_text("Manual Movies").into_bytes().into(),
                        true.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    fixture
        .database
        .execute(
            sql.build(
                Query::insert()
                    .into_table(Alias::new("library_storage_roots"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("library_id"),
                        Alias::new("storage_root_id"),
                    ])
                    .values_panic([
                        Uuid::new_v4().into(),
                        manual_library.into(),
                        fixture.root_id.as_uuid().into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    let work = WorkJobRepository::new(&fixture.database);
    work.enqueue_or_join(
        &WorkJobSpec::new(
            WorkTaskKind::ScopedStorageSync,
            WorkScope::StorageObject(fixture.parent_record_id),
            0,
            100,
        )
        .unwrap(),
    )
    .await
    .unwrap();
    let sync = work
        .claim_next(
            &[WorkTaskKind::ScopedStorageSync],
            "inventory-worker",
            chrono::Duration::minutes(1),
        )
        .await
        .unwrap()
        .unwrap();
    ScopedInventoryService::new(
        fixture.database.clone(),
        Arc::new(TwoPageBackend {
            parent_id: fixture.parent_backend_id.clone(),
            requests: Mutex::new(Vec::new()),
            fail_second_page: false,
            first_name: "Series",
        }),
    )
    .run_claimed(&sync, fixture.account_id)
    .await
    .unwrap();
    let discovery = work
        .claim_next(
            &[WorkTaskKind::DiscoverTitles],
            "discover-worker",
            chrono::Duration::minutes(1),
        )
        .await
        .unwrap()
        .unwrap();

    DiscoverTitlesService::new(fixture.database.clone())
        .execute(&discovery)
        .await
        .unwrap();

    let memberships = fixture
        .database
        .query_all(Statement::from_string(
            sql,
            "SELECT library_id FROM library_catalog_items ORDER BY library_id".to_owned(),
        ))
        .await
        .unwrap();
    assert_eq!(memberships.len(), 1);
    assert_eq!(
        memberships[0].try_get::<Uuid>("", "library_id").unwrap(),
        fixture.library_id
    );
}

#[tokio::test]
async fn library_roots_scope_does_not_implicitly_schedule_title_discovery() {
    let fixture = fixture().await;
    let sql = fixture.database.get_database_backend();
    fixture
        .database
        .execute(
            sql.build(
                Query::update()
                    .table(Alias::new("libraries"))
                    .value(Alias::new("object_selection_scope"), "library_roots"),
            ),
        )
        .await
        .unwrap();
    let work = WorkJobRepository::new(&fixture.database);
    work.enqueue_or_join(
        &WorkJobSpec::new(
            WorkTaskKind::ScopedStorageSync,
            WorkScope::StorageObject(fixture.parent_record_id),
            0,
            100,
        )
        .unwrap(),
    )
    .await
    .unwrap();
    let claimed = work
        .claim_next(
            &[WorkTaskKind::ScopedStorageSync],
            "inventory-worker",
            chrono::Duration::minutes(1),
        )
        .await
        .unwrap()
        .unwrap();
    let backend = Arc::new(TwoPageBackend {
        parent_id: fixture.parent_backend_id.clone(),
        requests: Mutex::new(Vec::new()),
        fail_second_page: false,
        first_name: "Series",
    });

    ScopedInventoryService::new(fixture.database.clone(), backend)
        .run_claimed(&claimed, fixture.account_id)
        .await
        .unwrap();

    assert!(
        work.claim_next(
            &[WorkTaskKind::DiscoverTitles],
            "discover-worker",
            chrono::Duration::minutes(1),
        )
        .await
        .unwrap()
        .is_none()
    );
}
