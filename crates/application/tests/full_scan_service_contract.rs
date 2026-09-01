use chrono::{Duration, Utc};
use sea_orm::{
    ConnectionTrait, DatabaseConnection, TransactionTrait,
    sea_query::{Alias, Expr, Query},
};
use sea_orm_migration::MigratorTrait;
use tjxy_application::{FullScanError, FullScanService, MetadataResolveService, TaskService};
use tjxy_common::{
    CatalogItemId, LibraryId, LibraryRootBindingId, MediaSourceId, SortKey, StorageObjectRecordId,
    StorageRootId, UserId,
};
use tjxy_db::{
    CatalogQueryError, CatalogQueryRepository, DiscoverTitlesRepository, FullScanChildSubmission,
    FullScanRepository, MetadataRequirement, WorkJobRepository, WorkJobResult, WorkJobSpec,
    WorkJobState, WorkScope, WorkTaskKind,
};
use tjxy_domain::MetadataSourceMode;
use tjxy_test_support::test_database;
use uuid::Uuid;

async fn database() -> DatabaseConnection {
    let database = test_database().await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    database
}

async fn seed_library(database: &DatabaseConnection) -> Uuid {
    seed_library_with_policy(
        database,
        "Full",
        "all_synced_objects",
        "full",
        "eager",
        "eager",
    )
    .await
}

async fn seed_library_with_policy(
    database: &DatabaseConnection,
    profile: &str,
    object_selection: &str,
    metadata: &str,
    expansion: &str,
    probe: &str,
) -> Uuid {
    let library = Uuid::new_v4();
    database
        .execute(
            database.get_database_backend().build(
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
                        library.into(),
                        "Movies".into(),
                        profile.into(),
                        object_selection.into(),
                        metadata.into(),
                        expansion.into(),
                        probe.into(),
                        1.into(),
                        "movies".into(),
                        SortKey::from_text("Movies").into_bytes().into(),
                        true.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    library
}

#[allow(clippy::too_many_lines)] // Seeds one complete reconciled root boundary.
async fn seed_root(
    database: &DatabaseConnection,
    library: Uuid,
) -> (StorageRootId, StorageObjectRecordId) {
    let sql = database.get_database_backend();
    let account = Uuid::new_v4();
    let root = StorageRootId::new();
    let root_object = StorageObjectRecordId::new();
    database
        .execute(
            sql.build(
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
                        account.into(),
                        "filesystem".into(),
                        "Local".into(),
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
            sql.build(
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
                        root.as_uuid().into(),
                        account.into(),
                        "root".into(),
                        1_i64.into(),
                        1_i64.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            sql.build(
                Query::insert()
                    .into_table(Alias::new("library_storage_roots"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("library_id"),
                        Alias::new("storage_root_id"),
                    ])
                    .values_panic([Uuid::new_v4().into(), library.into(), root.as_uuid().into()]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            sql.build(
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
                        root_object.as_uuid().into(),
                        account.into(),
                        "local".into(),
                        "root".into(),
                        "Movies".into(),
                        "movies".into(),
                        "Directory".into(),
                        1_i64.into(),
                        true.into(),
                        1_i64.into(),
                        "StableFileId".into(),
                        "Present".into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            sql.build(
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
                        root.as_uuid().into(),
                        root_object.as_uuid().into(),
                        1_i64.into(),
                        true.into(),
                        1_i64.into(),
                        "Present".into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    (root, root_object)
}

async fn advance_root_inventory_watermarks(
    database: &DatabaseConnection,
    root: StorageRootId,
    root_object: StorageObjectRecordId,
    revision: i64,
) {
    let backend = database.get_database_backend();
    database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("storage_roots"))
                    .values([
                        (Alias::new("sync_revision"), revision.into()),
                        (Alias::new("reconciled_sync_revision"), revision.into()),
                    ])
                    .and_where(Expr::col(Alias::new("id")).eq(root.as_uuid())),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("storage_root_objects"))
                    .value(Alias::new("children_index_revision"), revision)
                    .and_where(Expr::col(Alias::new("storage_root_id")).eq(root.as_uuid()))
                    .and_where(
                        Expr::col(Alias::new("storage_object_id")).eq(root_object.as_uuid()),
                    ),
            ),
        )
        .await
        .unwrap();
}

async fn advance_library_discovery_watermark(
    database: &DatabaseConnection,
    library: Uuid,
    revision: i64,
) {
    database
        .execute(
            database.get_database_backend().build(
                Query::update()
                    .table(Alias::new("library_storage_roots"))
                    .value(Alias::new("discovered_sync_revision"), revision)
                    .and_where(Expr::col(Alias::new("library_id")).eq(library)),
            ),
        )
        .await
        .unwrap();
}

#[allow(clippy::too_many_lines)] // Seeds one active immutable Source publication.
async fn seed_indexed_movie_with_source(
    database: &DatabaseConnection,
    library: Uuid,
) -> (CatalogItemId, MediaSourceId) {
    let sql = database.get_database_backend();
    let item = CatalogItemId::new();
    database
        .execute(
            sql.build(
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
                        Alias::new("metadata_resolved_revision"),
                        Alias::new("metadata_resolved_requirement"),
                        Alias::new("metadata_payload_version"),
                        Alias::new("structure_state"),
                        Alias::new("source_state"),
                        Alias::new("structure_expansion_revision"),
                        Alias::new("source_index_revision"),
                        Alias::new("is_present"),
                    ])
                    .values_panic([
                        item.as_uuid().into(),
                        "Movie".into(),
                        "Arrival".into(),
                        "arrival".into(),
                        SortKey::from_text("Arrival").into_bytes().into(),
                        "Matched".into(),
                        "Ready".into(),
                        0_i64.into(),
                        MetadataRequirement::Full.as_i32().into(),
                        1_i32.into(),
                        "NotApplicable".into(),
                        "Indexed".into(),
                        0_i64.into(),
                        1_i64.into(),
                        true.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            sql.build(
                Query::insert()
                    .into_table(Alias::new("library_catalog_items"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("library_id"),
                        Alias::new("catalog_item_id"),
                    ])
                    .values_panic([Uuid::new_v4().into(), library.into(), item.as_uuid().into()]),
            ),
        )
        .await
        .unwrap();
    let jobs = WorkJobRepository::new(database);
    jobs.enqueue_or_join(
        &WorkJobSpec::new(
            WorkTaskKind::IndexMediaSources,
            WorkScope::CatalogItem(item),
            1,
            20,
        )
        .unwrap(),
    )
    .await
    .unwrap();
    let index = jobs
        .claim_next(
            &[WorkTaskKind::IndexMediaSources],
            "fixture-index",
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    let transaction = database.begin().await.unwrap();
    jobs.complete_in_transaction(
        &transaction,
        &index,
        WorkJobResult::success(serde_json::json!({"sources": 1}), Vec::new()),
    )
    .await
    .unwrap();
    transaction.commit().await.unwrap();

    let publication = Uuid::new_v4();
    let source = MediaSourceId::new();
    let presentation = Uuid::new_v4();
    database
        .execute(
            sql.build(
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
                        Alias::new("activated_generation"),
                    ])
                    .values_panic([
                        publication.into(),
                        index.id().as_uuid().into(),
                        item.as_uuid().into(),
                        "Sources".into(),
                        1_i64.into(),
                        "Active".into(),
                        "0".repeat(64).into(),
                        1_i64.into(),
                        1_i64.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            sql.build(
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
                        source.as_uuid().into(),
                        item.as_uuid().into(),
                        presentation.into(),
                        "NotProbed".into(),
                        0_i64.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            sql.build(
                Query::insert()
                    .into_table(Alias::new("publication_media_sources"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("publication_id"),
                        Alias::new("media_source_id"),
                        Alias::new("catalog_item_id"),
                        Alias::new("presentation_key"),
                        Alias::new("row_sha256"),
                    ])
                    .values_panic([
                        Uuid::new_v4().into(),
                        publication.into(),
                        source.as_uuid().into(),
                        item.as_uuid().into(),
                        presentation.into(),
                        "1".repeat(64).into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            sql.build(
                Query::update()
                    .table(Alias::new("catalog_items"))
                    .value(Alias::new("active_source_publication_id"), publication)
                    .and_where(Expr::col(Alias::new("id")).eq(item.as_uuid())),
            ),
        )
        .await
        .unwrap();
    (item, source)
}

async fn seed_unindexed_movie(database: &DatabaseConnection, library: Uuid) -> CatalogItemId {
    seed_unindexed_item(database, library, "Movie", "Arrival").await
}

async fn seed_unindexed_audio(database: &DatabaseConnection, library: Uuid) -> CatalogItemId {
    seed_unindexed_item(database, library, "Audio", "First Light").await
}

async fn seed_unindexed_item(
    database: &DatabaseConnection,
    library: Uuid,
    item_type: &str,
    name: &str,
) -> CatalogItemId {
    let item = CatalogItemId::new();
    let sql = database.get_database_backend();
    let metadata_state = if item_type == "Audio" {
        "Partial"
    } else {
        "Ready"
    };
    let metadata_requirement = (item_type != "Audio").then_some(MetadataRequirement::Full.as_i32());
    database
        .execute(
            sql.build(
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
                        Alias::new("metadata_resolved_revision"),
                        Alias::new("metadata_resolved_requirement"),
                        Alias::new("structure_state"),
                        Alias::new("source_state"),
                        Alias::new("structure_expansion_revision"),
                        Alias::new("source_index_revision"),
                        Alias::new("is_present"),
                    ])
                    .values_panic([
                        item.as_uuid().into(),
                        item_type.into(),
                        name.into(),
                        name.to_lowercase().into(),
                        SortKey::from_text(name).into_bytes().into(),
                        "Matched".into(),
                        metadata_state.into(),
                        0_i64.into(),
                        metadata_requirement.into(),
                        "NotApplicable".into(),
                        "NotIndexed".into(),
                        0_i64.into(),
                        1_i64.into(),
                        true.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            sql.build(
                Query::insert()
                    .into_table(Alias::new("library_catalog_items"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("library_id"),
                        Alias::new("catalog_item_id"),
                    ])
                    .values_panic([Uuid::new_v4().into(), library.into(), item.as_uuid().into()]),
            ),
        )
        .await
        .unwrap();
    item
}

async fn seed_background_candidate(
    database: &DatabaseConnection,
    library: Uuid,
    name: &str,
    date_created: chrono::DateTime<Utc>,
) -> CatalogItemId {
    let item = CatalogItemId::new();
    let sql = database.get_database_backend();
    database
        .execute(
            sql.build(
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
                        Alias::new("metadata_resolved_revision"),
                        Alias::new("metadata_resolved_requirement"),
                        Alias::new("structure_state"),
                        Alias::new("source_state"),
                        Alias::new("structure_expansion_revision"),
                        Alias::new("source_index_revision"),
                        Alias::new("is_present"),
                        Alias::new("date_created"),
                    ])
                    .values_panic([
                        item.as_uuid().into(),
                        "Series".into(),
                        name.into(),
                        name.to_lowercase().into(),
                        SortKey::from_text(name).into_bytes().into(),
                        "Matched".into(),
                        "Ready".into(),
                        0_i64.into(),
                        MetadataRequirement::Full.as_i32().into(),
                        "NotExpanded".into(),
                        "NotApplicable".into(),
                        1_i64.into(),
                        0_i64.into(),
                        true.into(),
                        date_created.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            sql.build(
                Query::insert()
                    .into_table(Alias::new("library_catalog_items"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("library_id"),
                        Alias::new("catalog_item_id"),
                    ])
                    .values_panic([Uuid::new_v4().into(), library.into(), item.as_uuid().into()]),
            ),
        )
        .await
        .unwrap();
    item
}

async fn seed_structure_child(
    database: &DatabaseConnection,
    owner: CatalogItemId,
) -> CatalogItemId {
    let item = CatalogItemId::new();
    database
        .execute(
            database.get_database_backend().build(
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
                        Alias::new("structure_owner_item_id"),
                        Alias::new("is_present"),
                    ])
                    .values_panic([
                        item.as_uuid().into(),
                        "Episode".into(),
                        "Watching Episode".into(),
                        "watching episode".into(),
                        SortKey::from_text("Watching Episode").into_bytes().into(),
                        "Matched".into(),
                        "Ready".into(),
                        "NotApplicable".into(),
                        "Indexed".into(),
                        0_i64.into(),
                        1_i64.into(),
                        owner.as_uuid().into(),
                        true.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    item
}

async fn seed_stale_active_structure_publication(
    database: &DatabaseConnection,
    owner: CatalogItemId,
    children: &[CatalogItemId],
    job_id: Uuid,
    scope: Option<(StorageRootId, StorageObjectRecordId)>,
) {
    let publication = Uuid::new_v4();
    let backend = database.get_database_backend();
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
                        Alias::new("activated_generation"),
                    ])
                    .values_panic([
                        publication.into(),
                        job_id.into(),
                        owner.as_uuid().into(),
                        "Structure".into(),
                        1_i64.into(),
                        "Active".into(),
                        "0".repeat(64).into(),
                        i64::try_from(children.len()).unwrap().into(),
                        1_i64.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    for child in children {
        database
            .execute(
                backend.build(
                    Query::insert()
                        .into_table(Alias::new("publication_catalog_items"))
                        .columns([
                            Alias::new("id"),
                            Alias::new("publication_id"),
                            Alias::new("catalog_item_id"),
                            Alias::new("parent_catalog_item_id"),
                            Alias::new("item_type"),
                            Alias::new("name"),
                            Alias::new("sort_name"),
                            Alias::new("sort_key"),
                            Alias::new("source_state"),
                            Alias::new("source_index_revision"),
                            Alias::new("storage_root_id"),
                            Alias::new("scope_storage_object_id"),
                            Alias::new("row_sha256"),
                        ])
                        .values_panic([
                            Uuid::new_v4().into(),
                            publication.into(),
                            child.as_uuid().into(),
                            owner.as_uuid().into(),
                            "Episode".into(),
                            "Episode".into(),
                            "episode".into(),
                            SortKey::from_text("Episode").into_bytes().into(),
                            "Indexed".into(),
                            1_i64.into(),
                            scope.map(|(root, _)| root.as_uuid()).into(),
                            scope.map(|(_, object)| object.as_uuid()).into(),
                            "0".repeat(64).into(),
                        ]),
                ),
            )
            .await
            .unwrap();
    }
    database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("catalog_items"))
                    .values([
                        (
                            Alias::new("active_structure_publication_id"),
                            publication.into(),
                        ),
                        (Alias::new("structure_expansion_revision"), 2_i64.into()),
                    ])
                    .and_where(Expr::col(Alias::new("id")).eq(owner.as_uuid())),
            ),
        )
        .await
        .unwrap();
}

async fn claimed_full_scan(
    database: &DatabaseConnection,
    library: Uuid,
) -> tjxy_db::ClaimedWorkJob {
    let jobs = WorkJobRepository::new(database);
    jobs.enqueue_or_join(
        &WorkJobSpec::new(
            WorkTaskKind::FullMediaScan,
            WorkScope::Library(tjxy_common::LibraryId::from_uuid(library)),
            1,
            20,
        )
        .unwrap(),
    )
    .await
    .unwrap();
    jobs.claim_next(
        &[WorkTaskKind::FullMediaScan],
        "full-scan",
        chrono::Duration::minutes(5),
    )
    .await
    .unwrap()
    .unwrap()
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Keeps the binding-scoped Validate/Discover/completion lifecycle in one contract.
async fn manual_root_full_scan_uses_fixed_full_policy_and_is_not_a_scheduled_scan() {
    let database = database().await;
    let library = seed_library_with_policy(
        &database,
        "Manual",
        "library_roots",
        "none",
        "manual",
        "on_playback",
    )
    .await;
    let (root, root_object) = seed_root(&database, library).await;
    let sibling_library = seed_library_with_policy(
        &database,
        "Manual",
        "library_roots",
        "none",
        "manual",
        "on_playback",
    )
    .await;
    database
        .execute(
            database.get_database_backend().build(
                Query::insert()
                    .into_table(Alias::new("library_storage_roots"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("library_id"),
                        Alias::new("storage_root_id"),
                    ])
                    .values_panic([
                        Uuid::new_v4().into(),
                        sibling_library.into(),
                        root.as_uuid().into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    let jobs = WorkJobRepository::new(&database);
    let submission = TaskService::new(database.clone())
        .full_scan_root(LibraryId::from_uuid(library), root)
        .await
        .unwrap();
    assert_eq!(
        submission.job().task_kind(),
        WorkTaskKind::FullLibraryRootScan
    );
    let WorkScope::LibraryRootBinding(binding_id) = submission.job().scope() else {
        panic!("manual root Full must use the binding identity");
    };
    assert_ne!(binding_id, LibraryRootBindingId::from_uuid(root.as_uuid()));
    let claimed = jobs
        .claim_next(
            &[WorkTaskKind::FullLibraryRootScan],
            "manual-root-full",
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    let scans = FullScanRepository::new(&database);
    let policy = scans.policy(&claimed).await.unwrap();
    assert!(policy.selects_all_synced_objects());
    assert!(policy.resolves_metadata());
    assert!(policy.expands_eagerly());
    assert!(policy.probes_eagerly());
    let roots = scans.roots(&claimed).await.unwrap();
    assert_eq!(roots.len(), 1);
    assert_eq!(roots[0].binding_id(), binding_id);
    assert_eq!(roots[0].root_id(), root);

    assert!(matches!(
        FullScanService::new(database.clone())
            .execute(&claimed)
            .await,
        Err(FullScanError::ChildrenPending { scheduled: 1 })
    ));
    let validation = jobs
        .claim_next(
            &[WorkTaskKind::ValidateStorageRoot],
            "manual-root-validation",
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(validation.job().scope(), WorkScope::StorageRoot(root));
    let transaction = database.begin().await.unwrap();
    jobs.complete_in_transaction(
        &transaction,
        &validation,
        WorkJobResult::success(serde_json::json!({"directories": 1}), Vec::new())
            .with_sync_revision(2)
            .unwrap(),
    )
    .await
    .unwrap();
    transaction.commit().await.unwrap();
    advance_root_inventory_watermarks(&database, root, root_object, 2).await;
    jobs.retry(&claimed, Duration::zero(), "waiting for validation")
        .await
        .unwrap();
    let resumed = jobs
        .claim_next(
            &[WorkTaskKind::FullLibraryRootScan],
            "manual-root-full-resume",
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        FullScanService::new(database.clone())
            .execute(&resumed)
            .await,
        Err(FullScanError::ChildrenPending { scheduled: 1 })
    ));
    let discovery = jobs
        .claim_next(
            &[WorkTaskKind::DiscoverTitles],
            "binding-discovery",
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(discovery.job().scope(), WorkScope::StorageRoot(root));
    let discovery_repository = DiscoverTitlesRepository::new(&database);
    let snapshot = discovery_repository.snapshot(&discovery).await.unwrap();
    assert_eq!(snapshot.title_count(), 0);
    discovery_repository
        .publish(&discovery, &snapshot)
        .await
        .unwrap();
    let sibling_watermark = database
        .query_one(
            database.get_database_backend().build(
                &Query::select()
                    .column(Alias::new("discovered_sync_revision"))
                    .from(Alias::new("library_storage_roots"))
                    .and_where(Expr::col(Alias::new("library_id")).eq(sibling_library))
                    .and_where(Expr::col(Alias::new("storage_root_id")).eq(root.as_uuid()))
                    .limit(1)
                    .to_owned(),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get::<i64>("", "discovered_sync_revision")
        .unwrap();
    assert_eq!(sibling_watermark, 2);
    jobs.retry(&resumed, Duration::zero(), "waiting for discovery")
        .await
        .unwrap();
    let completed = jobs
        .claim_next(
            &[WorkTaskKind::FullLibraryRootScan],
            "manual-root-full-complete",
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    FullScanService::new(database.clone())
        .execute(&completed)
        .await
        .unwrap();
    assert_eq!(
        jobs.get(completed.id()).await.unwrap().unwrap().state(),
        WorkJobState::Completed
    );
    assert_eq!(
        TaskService::new(database.clone())
            .cancel_full_media_scan()
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Keeps the shared-item, dual-root selection boundary in one contract.
async fn manual_root_full_scan_targets_only_items_matched_under_the_selected_root() {
    let database = database().await;
    let library = seed_library_with_policy(
        &database,
        "Manual",
        "library_roots",
        "none",
        "manual",
        "on_playback",
    )
    .await;
    let (selected_root, selected_object) = seed_root(&database, library).await;
    let (other_root, other_object) = seed_root(&database, library).await;
    let selected_item = seed_unindexed_movie(&database, library).await;
    let other_item = seed_unindexed_movie(&database, library).await;
    for (object, item) in [(selected_object, selected_item), (other_object, other_item)] {
        database
            .execute(
                database.get_database_backend().build(
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
                            object.as_uuid().into(),
                            item.as_uuid().into(),
                            1.0.into(),
                            "Matched".into(),
                            serde_json::json!({}).into(),
                        ]),
                ),
            )
            .await
            .unwrap();
    }
    database
        .execute(
            database.get_database_backend().build(
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
                        other_object.as_uuid().into(),
                        selected_item.as_uuid().into(),
                        1.0.into(),
                        "Matched".into(),
                        serde_json::json!({}).into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    TaskService::new(database.clone())
        .full_scan_root(LibraryId::from_uuid(library), selected_root)
        .await
        .unwrap();
    let claimed = WorkJobRepository::new(&database)
        .claim_next(
            &[WorkTaskKind::FullLibraryRootScan],
            "selected-root-full",
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    let other_root_projected = seed_structure_child(&database, selected_item).await;
    seed_stale_active_structure_publication(
        &database,
        selected_item,
        &[other_root_projected],
        claimed.id().as_uuid(),
        Some((other_root, other_object)),
    )
    .await;

    assert_eq!(
        FullScanRepository::new(&database)
            .targets(&claimed)
            .await
            .unwrap(),
        vec![selected_item]
    );
    let query = CatalogQueryRepository::new(&database);
    assert!(matches!(
        query.lazy_work_target(UserId::new(), selected_item).await,
        Err(CatalogQueryError::AmbiguousStorageScope(_))
    ));
    let selected_scope = query
        .lazy_work_target_in_storage_root(UserId::new(), selected_item, selected_root)
        .await
        .unwrap()
        .unwrap()
        .storage_scope()
        .unwrap();
    assert_eq!(selected_scope.storage_object_id(), selected_object);
    let other_scope = query
        .lazy_work_target_in_storage_root(UserId::new(), selected_item, other_root)
        .await
        .unwrap()
        .unwrap()
        .storage_scope()
        .unwrap();
    assert_eq!(other_scope.storage_object_id(), other_object);
}

#[tokio::test]
async fn full_scan_targets_follow_the_persisted_object_selection_scope() {
    for (profile, object_selection, metadata, expansion, probe, includes_projected) in [
        (
            "Lazy",
            "title_layer",
            "basic",
            "on_browse",
            "on_playback",
            false,
        ),
        ("Full", "all_synced_objects", "full", "eager", "eager", true),
    ] {
        let database = database().await;
        let library = seed_library_with_policy(
            &database,
            profile,
            object_selection,
            metadata,
            expansion,
            probe,
        )
        .await;
        let owner = seed_background_candidate(&database, library, "Series", Utc::now()).await;
        let projected = seed_structure_child(&database, owner).await;
        let claimed = claimed_full_scan(&database, library).await;
        seed_stale_active_structure_publication(
            &database,
            owner,
            &[projected],
            claimed.id().as_uuid(),
            None,
        )
        .await;
        database
            .execute(
                database.get_database_backend().build(
                    Query::update()
                        .table(Alias::new("catalog_items"))
                        .value(Alias::new("structure_expansion_revision"), 1_i64)
                        .and_where(Expr::col(Alias::new("id")).eq(owner.as_uuid())),
                ),
            )
            .await
            .unwrap();

        let targets = FullScanRepository::new(&database)
            .targets(&claimed)
            .await
            .unwrap();

        assert!(targets.contains(&owner), "profile: {profile}");
        assert_eq!(
            targets.contains(&projected),
            includes_projected,
            "profile: {profile}"
        );
    }
}

#[tokio::test]
async fn empty_library_full_scan_completes_durably() {
    let database = database().await;
    let library = seed_library(&database).await;
    let claimed = claimed_full_scan(&database, library).await;

    let result = FullScanService::new(database.clone())
        .execute(&claimed)
        .await
        .unwrap();

    assert_eq!(result.scheduled(), 0);
    assert_eq!(
        WorkJobRepository::new(&database)
            .get(claimed.id())
            .await
            .unwrap()
            .unwrap()
            .state(),
        WorkJobState::Completed
    );
}

#[tokio::test]
async fn profile_version_change_fences_the_claim_before_scheduling_children() {
    let database = database().await;
    let library = seed_library(&database).await;
    seed_root(&database, library).await;
    let claimed = claimed_full_scan(&database, library).await;
    database
        .execute(
            database.get_database_backend().build(
                Query::update()
                    .table(Alias::new("libraries"))
                    .value(Alias::new("profile_version"), 2_i64)
                    .and_where(Expr::col(Alias::new("id")).eq(library)),
            ),
        )
        .await
        .unwrap();

    assert!(matches!(
        FullScanService::new(database.clone())
            .execute(&claimed)
            .await,
        Err(FullScanError::Repository(
            tjxy_db::FullScanRepositoryError::StaleLibrary
        ))
    ));
    assert!(
        WorkJobRepository::new(&database)
            .claim_next(
                &[
                    WorkTaskKind::ValidateStorageRoot,
                    WorkTaskKind::ScopedStorageSync,
                    WorkTaskKind::DiscoverTitles,
                ],
                "unexpected-child",
                Duration::minutes(5),
            )
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn title_layer_refresh_schedules_one_non_recursive_sync_per_root() {
    let database = database().await;
    let library = seed_library_with_policy(
        &database,
        "Full",
        "title_layer",
        "basic",
        "on_browse",
        "on_playback",
    )
    .await;
    let (_first_root, first_root_object) = seed_root(&database, library).await;
    let (_second_root, second_root_object) = seed_root(&database, library).await;
    let claimed = claimed_full_scan(&database, library).await;

    assert!(matches!(
        FullScanService::new(database.clone())
            .execute(&claimed)
            .await
            .unwrap_err(),
        FullScanError::ChildrenPending { scheduled: 2 }
    ));

    let jobs = WorkJobRepository::new(&database);
    let mut scopes = Vec::new();
    for owner in ["root-sync-1", "root-sync-2"] {
        let child = jobs
            .claim_next(
                &[WorkTaskKind::ScopedStorageSync],
                owner,
                Duration::minutes(5),
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(child.job().expected_revision(), 1);
        assert_eq!(child.job().priority(), claimed.job().priority());
        scopes.push(child.job().scope());
    }
    assert!(scopes.contains(&WorkScope::StorageObject(first_root_object)));
    assert!(scopes.contains(&WorkScope::StorageObject(second_root_object)));
    assert!(
        jobs.claim_next(
            &[WorkTaskKind::ValidateStorageRoot],
            "unexpected-validation",
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .is_none()
    );
    let staged = database
        .query_one(
            database.get_database_backend().build(
                Query::select()
                    .expr_as(Expr::col(Alias::new("id")).count(), Alias::new("count"))
                    .from(Alias::new("work_staging_rows"))
                    .and_where(Expr::col(Alias::new("job_id")).eq(claimed.id().as_uuid()))
                    .and_where(Expr::col(Alias::new("entity_kind")).eq("FullScanChild")),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(staged.try_get::<i64>("", "count").unwrap(), 2);
}

#[tokio::test]
async fn cancelling_a_full_scan_terminates_a_child_created_by_that_scan() {
    let database = database().await;
    let library = seed_library_with_policy(
        &database,
        "Lazy",
        "title_layer",
        "basic",
        "on_browse",
        "on_playback",
    )
    .await;
    seed_root(&database, library).await;
    let claimed = claimed_full_scan(&database, library).await;
    assert!(matches!(
        FullScanService::new(database.clone())
            .execute(&claimed)
            .await,
        Err(FullScanError::ChildrenPending { scheduled: 1 })
    ));
    let jobs = WorkJobRepository::new(&database);
    let child = jobs
        .claim_next(
            &[WorkTaskKind::ScopedStorageSync],
            "storage-worker",
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        TaskService::new(database.clone())
            .cancel_full_media_scan()
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        jobs.get(claimed.id()).await.unwrap().unwrap().state(),
        WorkJobState::Failed
    );
    assert_eq!(
        jobs.get(child.id()).await.unwrap().unwrap().state(),
        WorkJobState::Failed
    );
}

#[tokio::test]
async fn cancelling_scheduled_full_preserves_a_child_shared_with_manual_root_full() {
    let database = database().await;
    let library = seed_library(&database).await;
    let (root, _) = seed_root(&database, library).await;
    let scheduled = claimed_full_scan(&database, library).await;
    let jobs = WorkJobRepository::new(&database);
    TaskService::new(database.clone())
        .full_scan_root(LibraryId::from_uuid(library), root)
        .await
        .unwrap();
    let manual = jobs
        .claim_next(
            &[WorkTaskKind::FullLibraryRootScan],
            "manual-full",
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    let validation = WorkJobSpec::new(
        WorkTaskKind::ValidateStorageRoot,
        WorkScope::StorageRoot(root),
        1,
        20,
    )
    .unwrap();
    let FullScanChildSubmission::Job(created) = jobs
        .enqueue_full_scan_child(&scheduled, "shared-validation", &validation)
        .await
        .unwrap()
    else {
        panic!("scheduled Full must create the validation child");
    };
    assert!(created.created());
    let FullScanChildSubmission::Job(joined) = jobs
        .enqueue_full_scan_child(&manual, "shared-validation", &validation)
        .await
        .unwrap()
    else {
        panic!("manual root Full must join the validation child");
    };
    assert!(!joined.created());
    assert_eq!(created.job().id(), joined.job().id());

    assert_eq!(
        TaskService::new(database.clone())
            .cancel_full_media_scan()
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        jobs.get(scheduled.id()).await.unwrap().unwrap().state(),
        WorkJobState::Failed
    );
    assert_eq!(
        jobs.get(manual.id()).await.unwrap().unwrap().state(),
        WorkJobState::Running
    );
    assert_eq!(
        jobs.get(created.job().id()).await.unwrap().unwrap().state(),
        WorkJobState::Pending
    );
}

#[tokio::test]
async fn non_eager_expansion_policies_do_not_index_existing_movies() {
    for expansion in ["on_browse", "manual"] {
        let database = database().await;
        let library = seed_library_with_policy(
            &database,
            "Full",
            "library_roots",
            "none",
            expansion,
            "on_playback",
        )
        .await;
        seed_unindexed_movie(&database, library).await;
        let claimed = claimed_full_scan(&database, library).await;

        let result = FullScanService::new(database.clone())
            .execute(&claimed)
            .await
            .unwrap();

        assert_eq!(result.scheduled(), 0, "expansion policy: {expansion}");
        assert!(
            WorkJobRepository::new(&database)
                .claim_next(
                    &[WorkTaskKind::IndexMediaSources],
                    "unexpected-index",
                    Duration::minutes(5),
                )
                .await
                .unwrap()
                .is_none(),
            "expansion policy: {expansion}"
        );
    }
}

#[tokio::test]
async fn eager_expansion_resolves_music_metadata_before_indexing_tracks() {
    let database = database().await;
    let library = seed_library_with_policy(
        &database,
        "Full",
        "library_roots",
        "basic",
        "eager",
        "on_playback",
    )
    .await;
    database
        .execute(
            database.get_database_backend().build(
                Query::update()
                    .table(Alias::new("libraries"))
                    .value(Alias::new("collection_type"), "music")
                    .and_where(Expr::col(Alias::new("id")).eq(library)),
            ),
        )
        .await
        .unwrap();
    let (_, storage_object) = seed_root(&database, library).await;
    advance_library_discovery_watermark(&database, library, 1).await;
    let item = seed_unindexed_audio(&database, library).await;
    database
        .execute(
            database.get_database_backend().build(
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
                        storage_object.as_uuid().into(),
                        item.as_uuid().into(),
                        1.0.into(),
                        "Matched".into(),
                        serde_json::json!({}).into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    let claimed = claimed_full_scan(&database, library).await;

    let result = FullScanService::new(database.clone())
        .execute(&claimed)
        .await
        .unwrap_err();

    assert!(matches!(result, FullScanError::ChildrenPending { .. }));
    assert!(
        WorkJobRepository::new(&database)
            .claim_next(
                &[WorkTaskKind::ResolveMetadata],
                "music-metadata",
                Duration::minutes(5),
            )
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn on_playback_refresh_does_not_eagerly_probe_current_sources() {
    let database = database().await;
    let library = seed_library_with_policy(
        &database,
        "Full",
        "library_roots",
        "none",
        "manual",
        "on_playback",
    )
    .await;
    seed_indexed_movie_with_source(&database, library).await;
    let claimed = claimed_full_scan(&database, library).await;

    let result = FullScanService::new(database.clone())
        .execute(&claimed)
        .await
        .unwrap();

    assert_eq!(result.scheduled(), 0);
    assert!(
        WorkJobRepository::new(&database)
            .claim_next(
                &[WorkTaskKind::ProbeMedia],
                "unexpected-probe",
                Duration::minutes(5),
            )
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn eager_probe_policy_still_schedules_unprobed_current_sources() {
    let database = database().await;
    let library = seed_library_with_policy(
        &database,
        "Manual",
        "library_roots",
        "none",
        "manual",
        "eager",
    )
    .await;
    let (_, source) = seed_indexed_movie_with_source(&database, library).await;
    let claimed = claimed_full_scan(&database, library).await;

    assert!(matches!(
        FullScanService::new(database.clone())
            .execute(&claimed)
            .await
            .unwrap_err(),
        FullScanError::ChildrenPending { scheduled: 1 }
    ));
    let probe = WorkJobRepository::new(&database)
        .claim_next(&[WorkTaskKind::ProbeMedia], "probe", Duration::minutes(5))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(probe.job().scope(), WorkScope::MediaSource(source));
}

#[tokio::test]
async fn failed_eager_probe_is_propagated_to_the_full_scan_parent() {
    let database = database().await;
    let library = seed_library_with_policy(
        &database,
        "Full",
        "library_roots",
        "none",
        "manual",
        "eager",
    )
    .await;
    let (_, source) = seed_indexed_movie_with_source(&database, library).await;
    let claimed = claimed_full_scan(&database, library).await;
    assert!(matches!(
        FullScanService::new(database.clone())
            .execute(&claimed)
            .await,
        Err(FullScanError::ChildrenPending { scheduled: 1 })
    ));
    let jobs = WorkJobRepository::new(&database);
    let probe = jobs
        .claim_next(&[WorkTaskKind::ProbeMedia], "probe", Duration::minutes(5))
        .await
        .unwrap()
        .unwrap();
    jobs.fail_terminal(&probe, "fixture probe failure")
        .await
        .unwrap();
    jobs.retry(&claimed, Duration::zero(), "waiting for probe")
        .await
        .unwrap();
    let resumed = jobs
        .claim_next(
            &[WorkTaskKind::FullMediaScan],
            "full-scan-resumed",
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();

    assert!(matches!(
        FullScanService::new(database).execute(&resumed).await,
        Err(FullScanError::ChildFailed {
            task: WorkTaskKind::ProbeMedia,
            scope: WorkScope::MediaSource(id),
        }) if id == source
    ));
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Covers scheduling, Partial publication, watermark advance, and parent resume as one workflow.
async fn basic_metadata_policy_waits_for_resolution_at_the_current_revision() {
    let database = database().await;
    let library = seed_library_with_policy(
        &database,
        "Full",
        "library_roots",
        "basic",
        "on_browse",
        "on_playback",
    )
    .await;
    database
        .execute(
            database.get_database_backend().build(
                Query::update()
                    .table(Alias::new("libraries"))
                    .value(Alias::new("metadata_source_mode"), "local_only")
                    .and_where(Expr::col(Alias::new("id")).eq(library)),
            ),
        )
        .await
        .unwrap();
    let (item, _) = seed_indexed_movie_with_source(&database, library).await;
    let (_, storage_object) = seed_root(&database, library).await;
    let sql = database.get_database_backend();
    database
        .execute(
            sql.build(
                Query::update()
                    .table(Alias::new("catalog_items"))
                    .value(Alias::new("metadata_state"), "Partial")
                    .value(Alias::new("metadata_revision"), 2_i64)
                    .and_where(Expr::col(Alias::new("id")).eq(item.as_uuid())),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            sql.build(
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
                        storage_object.as_uuid().into(),
                        item.as_uuid().into(),
                        1.0.into(),
                        "Matched".into(),
                        serde_json::json!({}).into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    let claimed = claimed_full_scan(&database, library).await;

    assert!(matches!(
        FullScanService::new(database.clone())
            .execute(&claimed)
            .await,
        Err(FullScanError::ChildrenPending { scheduled: 1 })
    ));
    let metadata = WorkJobRepository::new(&database)
        .claim_next(
            &[WorkTaskKind::ResolveMetadata],
            "metadata",
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(metadata.job().scope(), WorkScope::CatalogItem(item));
    assert_eq!(metadata.job().expected_revision(), 2);
    assert_eq!(metadata.job().input_sync_revision(), Some(1));
    assert_eq!(
        metadata.job().metadata_requirement(),
        Some(MetadataRequirement::Basic)
    );
    assert_eq!(
        metadata.job().metadata_source_mode(),
        Some(MetadataSourceMode::LocalOnly)
    );

    let metadata_report = MetadataResolveService::new(database.clone())
        .execute(&metadata)
        .await
        .unwrap();
    assert_eq!(metadata_report.state().as_str(), "Partial");
    let jobs = WorkJobRepository::new(&database);
    jobs.retry(&claimed, Duration::zero(), "waiting for metadata")
        .await
        .unwrap();
    let resumed = jobs
        .claim_next(
            &[WorkTaskKind::FullMediaScan],
            "full-scan-resumed",
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        FullScanService::new(database.clone())
            .execute(&resumed)
            .await
            .unwrap()
            .scheduled(),
        0
    );
    let row = database
        .query_one(
            sql.build(
                Query::select()
                    .columns([
                        Alias::new("metadata_state"),
                        Alias::new("metadata_resolved_revision"),
                        Alias::new("metadata_resolved_requirement"),
                    ])
                    .from(Alias::new("catalog_items"))
                    .and_where(Expr::col(Alias::new("id")).eq(item.as_uuid())),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        row.try_get::<String>("", "metadata_state").unwrap(),
        "Partial"
    );
    assert_eq!(
        row.try_get::<i64>("", "metadata_resolved_revision")
            .unwrap(),
        2
    );
    assert_eq!(
        row.try_get::<Option<i32>>("", "metadata_resolved_requirement")
            .unwrap(),
        Some(MetadataRequirement::Basic.as_i32())
    );
}

#[tokio::test]
async fn lazy_profile_does_not_resolve_metadata_during_library_refresh() {
    let database = database().await;
    let library = seed_library_with_policy(
        &database,
        "Lazy",
        "title_layer",
        "basic",
        "on_browse",
        "on_playback",
    )
    .await;
    seed_root(&database, library).await;
    advance_library_discovery_watermark(&database, library, 1).await;
    let (item, _) = seed_indexed_movie_with_source(&database, library).await;
    database
        .execute(
            database.get_database_backend().build(
                Query::update()
                    .table(Alias::new("catalog_items"))
                    .value(Alias::new("metadata_state"), "Partial")
                    .value(Alias::new("metadata_revision"), 2_i64)
                    .and_where(Expr::col(Alias::new("id")).eq(item.as_uuid())),
            ),
        )
        .await
        .unwrap();
    let claimed = claimed_full_scan(&database, library).await;

    assert!(matches!(
        FullScanService::new(database.clone())
            .execute(&claimed)
            .await,
        Err(FullScanError::ChildrenPending { scheduled: 1 })
    ));
    let jobs = WorkJobRepository::new(&database);
    assert!(
        jobs.claim_next(
            &[WorkTaskKind::ResolveMetadata],
            "unexpected-lazy-metadata-before-inventory",
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .is_none()
    );
    let inventory = jobs
        .claim_next(
            &[WorkTaskKind::ScopedStorageSync],
            "lazy-root-inventory",
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    let transaction = database.begin().await.unwrap();
    jobs.complete_in_transaction(
        &transaction,
        &inventory,
        WorkJobResult::success(serde_json::json!({"objects": 0}), Vec::new())
            .with_sync_revision(1)
            .unwrap(),
    )
    .await
    .unwrap();
    transaction.commit().await.unwrap();
    jobs.retry(&claimed, Duration::zero(), "waiting for root inventory")
        .await
        .unwrap();
    let resumed = jobs
        .claim_next(
            &[WorkTaskKind::FullMediaScan],
            "lazy-refresh-parent",
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        FullScanService::new(database.clone())
            .execute(&resumed)
            .await
            .unwrap()
            .scheduled(),
        0
    );
    assert!(
        jobs.claim_next(
            &[WorkTaskKind::ResolveMetadata],
            "unexpected-lazy-metadata-after-inventory",
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .is_none()
    );
}

#[tokio::test]
async fn full_metadata_policy_does_not_accept_a_basic_watermark_at_the_same_revision() {
    let database = database().await;
    let library = seed_library_with_policy(
        &database,
        "Full",
        "library_roots",
        "full",
        "on_browse",
        "on_playback",
    )
    .await;
    let (item, _) = seed_indexed_movie_with_source(&database, library).await;
    let (_, storage_object) = seed_root(&database, library).await;
    let sql = database.get_database_backend();
    database
        .execute(
            sql.build(
                Query::update()
                    .table(Alias::new("catalog_items"))
                    .value(Alias::new("metadata_revision"), 2_i64)
                    .value(Alias::new("metadata_resolved_revision"), 2_i64)
                    .value(
                        Alias::new("metadata_resolved_requirement"),
                        MetadataRequirement::Basic.as_i32(),
                    )
                    .and_where(Expr::col(Alias::new("id")).eq(item.as_uuid())),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            sql.build(
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
                        storage_object.as_uuid().into(),
                        item.as_uuid().into(),
                        1.0.into(),
                        "Matched".into(),
                        serde_json::json!({}).into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    let claimed = claimed_full_scan(&database, library).await;

    assert!(matches!(
        FullScanService::new(database.clone())
            .execute(&claimed)
            .await,
        Err(FullScanError::ChildrenPending { scheduled: 1 })
    ));
    let metadata = WorkJobRepository::new(&database)
        .claim_next(
            &[WorkTaskKind::ResolveMetadata],
            "full-metadata",
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(metadata.job().scope(), WorkScope::CatalogItem(item));
    assert_eq!(
        metadata.job().metadata_requirement(),
        Some(MetadataRequirement::Full)
    );
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Seeds one complete reconciled root boundary.
async fn full_scan_requires_a_new_validation_even_for_a_reconciled_indexed_root() {
    let database = database().await;
    let library = seed_library(&database).await;
    let sql = database.get_database_backend();
    let account = Uuid::new_v4();
    let root = StorageRootId::new();
    let root_object = StorageObjectRecordId::new();
    database
        .execute(
            sql.build(
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
                        account.into(),
                        "filesystem".into(),
                        "Local".into(),
                        "local".into(),
                        "local".into(),
                        "Active".into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            sql.build(
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
                        root.as_uuid().into(),
                        account.into(),
                        "root".into(),
                        1_i64.into(),
                        1_i64.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            sql.build(
                Query::insert()
                    .into_table(Alias::new("library_storage_roots"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("library_id"),
                        Alias::new("storage_root_id"),
                    ])
                    .values_panic([Uuid::new_v4().into(), library.into(), root.as_uuid().into()]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            sql.build(
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
                        root_object.as_uuid().into(),
                        account.into(),
                        "local".into(),
                        "root".into(),
                        "Movies".into(),
                        "movies".into(),
                        "Directory".into(),
                        1_i64.into(),
                        true.into(),
                        1_i64.into(),
                        "StableFileId".into(),
                        "Present".into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            sql.build(
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
                        root.as_uuid().into(),
                        root_object.as_uuid().into(),
                        1_i64.into(),
                        true.into(),
                        1_i64.into(),
                        "Present".into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    let claimed = claimed_full_scan(&database, library).await;

    let error = FullScanService::new(database.clone())
        .execute(&claimed)
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        FullScanError::ChildrenPending { scheduled: 1 }
    ));
    let child = WorkJobRepository::new(&database)
        .claim_next(
            &[WorkTaskKind::ValidateStorageRoot],
            "validate",
            chrono::Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(child.job().scope(), WorkScope::StorageRoot(root));
    assert_eq!(child.job().expected_revision(), 1);
    assert!(
        WorkJobRepository::new(&database)
            .claim_next(
                &[
                    WorkTaskKind::DiscoverTitles,
                    WorkTaskKind::ExpandItem,
                    WorkTaskKind::IndexMediaSources,
                    WorkTaskKind::ProbeMedia,
                ],
                "media",
                chrono::Duration::minutes(5),
            )
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Mirrors one reconciled matched Movie scan target.
async fn full_scan_schedules_source_index_before_it_can_complete() {
    let database = database().await;
    let library = seed_library(&database).await;
    let sql = database.get_database_backend();
    let item = CatalogItemId::new();
    let account = Uuid::new_v4();
    let root = StorageRootId::new();
    let parent = StorageObjectRecordId::new();
    database
        .execute(
            sql.build(
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
                        Alias::new("metadata_resolved_revision"),
                        Alias::new("metadata_resolved_requirement"),
                        Alias::new("metadata_payload_version"),
                        Alias::new("structure_state"),
                        Alias::new("source_state"),
                        Alias::new("structure_expansion_revision"),
                        Alias::new("source_index_revision"),
                        Alias::new("is_present"),
                    ])
                    .values_panic([
                        item.as_uuid().into(),
                        "Movie".into(),
                        "Arrival".into(),
                        "arrival".into(),
                        SortKey::from_text("Arrival").into_bytes().into(),
                        "Matched".into(),
                        "Ready".into(),
                        0_i64.into(),
                        MetadataRequirement::Full.as_i32().into(),
                        1_i32.into(),
                        "NotApplicable".into(),
                        "NotIndexed".into(),
                        0_i64.into(),
                        1_i64.into(),
                        true.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            sql.build(
                Query::insert()
                    .into_table(Alias::new("library_catalog_items"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("library_id"),
                        Alias::new("catalog_item_id"),
                    ])
                    .values_panic([Uuid::new_v4().into(), library.into(), item.as_uuid().into()]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            sql.build(
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
                        account.into(),
                        "filesystem".into(),
                        "Local".into(),
                        "local-account".into(),
                        "local".into(),
                        "Active".into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            sql.build(
                Query::insert()
                    .into_table(Alias::new("storage_roots"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("storage_account_id"),
                        Alias::new("provider_root_id"),
                        Alias::new("sync_revision"),
                        Alias::new("reconciled_sync_revision"),
                        Alias::new("discovered_sync_revision"),
                    ])
                    .values_panic([
                        root.as_uuid().into(),
                        account.into(),
                        "root".into(),
                        1_i64.into(),
                        1_i64.into(),
                        1_i64.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            sql.build(
                Query::insert()
                    .into_table(Alias::new("library_storage_roots"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("library_id"),
                        Alias::new("storage_root_id"),
                    ])
                    .values_panic([Uuid::new_v4().into(), library.into(), root.as_uuid().into()]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            sql.build(
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
                        parent.as_uuid().into(),
                        account.into(),
                        "local".into(),
                        "arrival".into(),
                        "Arrival".into(),
                        "arrival".into(),
                        "Directory".into(),
                        1_i64.into(),
                        true.into(),
                        1_i64.into(),
                        "StableFileId".into(),
                        "Present".into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            sql.build(
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
                        root.as_uuid().into(),
                        parent.as_uuid().into(),
                        1_i64.into(),
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
            sql.build(
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
                        parent.as_uuid().into(),
                        item.as_uuid().into(),
                        1.0.into(),
                        "Matched".into(),
                        serde_json::json!({}).into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    let claimed = claimed_full_scan(&database, library).await;

    let error = FullScanService::new(database.clone())
        .execute(&claimed)
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        FullScanError::ChildrenPending { scheduled: 1 }
    ));
    assert_eq!(
        WorkJobRepository::new(&database)
            .get(claimed.id())
            .await
            .unwrap()
            .unwrap()
            .state(),
        WorkJobState::Running
    );
    let jobs = WorkJobRepository::new(&database);
    let validation = jobs
        .claim_next(
            &[WorkTaskKind::ValidateStorageRoot],
            "validate",
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    assert!(
        jobs.claim_next(
            &[WorkTaskKind::IndexMediaSources],
            "too-early",
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .is_none()
    );
    let transaction = database.begin().await.unwrap();
    jobs.complete_in_transaction(
        &transaction,
        &validation,
        WorkJobResult::success(serde_json::json!({"directories": 1}), Vec::new())
            .with_sync_revision(1)
            .unwrap(),
    )
    .await
    .unwrap();
    transaction.commit().await.unwrap();
    let backend = database.get_database_backend();
    database
        .execute(
            backend.build(Query::update().table(Alias::new("storage_roots")).value(
                Alias::new("discovered_sync_revision"),
                Expr::col(Alias::new("reconciled_sync_revision")),
            )),
        )
        .await
        .unwrap();
    database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("library_storage_roots"))
                    .value(Alias::new("discovered_sync_revision"), 1_i64)
                    .and_where(Expr::col(Alias::new("library_id")).eq(library)),
            ),
        )
        .await
        .unwrap();
    jobs.retry(&claimed, Duration::zero(), "waiting for validation")
        .await
        .unwrap();
    let claimed = jobs
        .claim_next(
            &[WorkTaskKind::FullMediaScan],
            "full-scan-resume",
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        FullScanService::new(database.clone())
            .execute(&claimed)
            .await
            .unwrap_err(),
        FullScanError::ChildrenPending { scheduled: 1 }
    ));
    let child = WorkJobRepository::new(&database)
        .claim_next(
            &[WorkTaskKind::IndexMediaSources],
            "index",
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(child.job().scope(), WorkScope::CatalogItem(item));
}
