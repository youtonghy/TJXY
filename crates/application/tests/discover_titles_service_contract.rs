use sea_orm::{
    ConnectionTrait,
    sea_query::{Alias, Expr, JoinType, Query},
};
use sea_orm_migration::MigratorTrait;
use std::sync::Arc;

use async_trait::async_trait;
use tjxy_application::{
    DiscoverTitlesService, MetadataResolveService, SourceIndexService, TaskService,
    TaskServiceError,
};
use tjxy_common::{CatalogItemId, SortKey, StorageObjectRecordId, StorageRootId};
use tjxy_db::{
    CatalogPublicationRepository, DiscoverTitlesError, DiscoverTitlesRepository,
    MetadataRequirement, WorkJobRepository, WorkJobSpec, WorkJobState, WorkScope, WorkTaskKind,
};
use tjxy_domain::MetadataSourceMode;
use tjxy_metadata::{
    MetadataCandidate, MetadataLookup, MetadataProvider, MetadataProviderError, MetadataSource,
};
use tjxy_test_support::test_database;
use uuid::Uuid;

struct CompleteProvider;

#[async_trait]
impl MetadataProvider for CompleteProvider {
    fn name(&self) -> &'static str {
        "Fixture"
    }

    async fn resolve(
        &self,
        _lookup: &MetadataLookup,
    ) -> Result<Option<MetadataCandidate>, MetadataProviderError> {
        let source = MetadataSource::new("Fixture", Some("movie:329865"), 8_000).unwrap();
        Ok(Some(
            MetadataCandidate::new(source)
                .with_title("Arrival")
                .with_year(2016)
                .with_overview("A linguist meets visitors.")
                .with_provider_id("tmdb", "329865"),
        ))
    }
}

struct DiscoveryFixture {
    database: sea_orm::DatabaseConnection,
    library_id: Uuid,
    root: StorageRootId,
    title_object: StorageObjectRecordId,
    claimed: tjxy_db::ClaimedWorkJob,
}

#[allow(clippy::too_many_lines)] // Keeps one complete root-to-title SQL fixture reusable.
async fn discovery_fixture(metadata_policy: &str) -> DiscoveryFixture {
    let database = test_database().await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    let sql = database.get_database_backend();
    let library = Uuid::new_v4();
    let account = Uuid::new_v4();
    let root = StorageRootId::new();
    let root_object = StorageObjectRecordId::new();
    let title_object = StorageObjectRecordId::new();
    insert_library(&database, library, metadata_policy).await;
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
    insert_relation(
        &database,
        "library_storage_roots",
        [
            ("id", Uuid::new_v4()),
            ("library_id", library),
            ("storage_root_id", root.as_uuid()),
        ],
    )
    .await;
    insert_object(
        &database,
        account,
        root_object,
        "root",
        "Movies",
        "Directory",
        true,
    )
    .await;
    insert_object(
        &database,
        account,
        title_object,
        "arrival",
        "Arrival (2016)",
        "Directory",
        false,
    )
    .await;
    insert_root_relation(&database, root, root_object, None).await;
    insert_root_relation(&database, root, title_object, Some(root_object)).await;
    let jobs = WorkJobRepository::new(&database);
    DiscoverTitlesRepository::new(&database)
        .enqueue(root, 30)
        .await
        .unwrap();
    let claimed = jobs
        .claim_next(
            &[WorkTaskKind::DiscoverTitles],
            "discover-worker",
            chrono::Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    DiscoveryFixture {
        database,
        library_id: library,
        root,
        title_object,
        claimed,
    }
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Keeps the complete root-to-metadata contract in one fixture.
async fn discover_titles_publishes_root_children_without_reading_a_backend() {
    let DiscoveryFixture {
        database,
        library_id: _,
        root,
        title_object,
        claimed,
    } = discovery_fixture("basic").await;
    let sql = database.get_database_backend();
    let jobs = WorkJobRepository::new(&database);

    let report = DiscoverTitlesService::new(database.clone())
        .execute(&claimed)
        .await
        .unwrap();

    assert_eq!(report.discovered(), 1);
    assert_eq!(count(&database, "cache_invalidation_outbox").await, 1);
    assert_eq!(
        jobs.get(claimed.id()).await.unwrap().unwrap().state(),
        WorkJobState::Completed
    );
    let catalog = Alias::new("catalog_items");
    let identities = Alias::new("identity_matches");
    let row = database
        .query_one(
            sql.build(
                Query::select()
                    .expr_as(
                        Expr::col((catalog.clone(), Alias::new("id"))),
                        Alias::new("item_id"),
                    )
                    .columns([
                        (catalog.clone(), Alias::new("item_type")),
                        (catalog.clone(), Alias::new("name")),
                        (catalog.clone(), Alias::new("production_year")),
                        (catalog.clone(), Alias::new("metadata_state")),
                    ])
                    .from(catalog.clone())
                    .join(
                        JoinType::InnerJoin,
                        identities.clone(),
                        Expr::col((identities.clone(), Alias::new("candidate_catalog_item_id")))
                            .equals((catalog, Alias::new("id"))),
                    )
                    .and_where(
                        Expr::col((identities, Alias::new("storage_object_id")))
                            .eq(title_object.as_uuid()),
                    ),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    let item_id = CatalogItemId::from_uuid(row.try_get("", "item_id").unwrap());
    assert_eq!(row.try_get::<String>("", "item_type").unwrap(), "Movie");
    assert_eq!(row.try_get::<String>("", "name").unwrap(), "Arrival");
    assert_eq!(row.try_get::<i32>("", "production_year").unwrap(), 2016);
    assert_eq!(
        row.try_get::<String>("", "metadata_state").unwrap(),
        "Partial"
    );
    let root_row = database
        .query_one(
            sql.build(
                Query::select()
                    .column(Alias::new("discovered_sync_revision"))
                    .from(Alias::new("storage_roots"))
                    .and_where(Expr::col(Alias::new("id")).eq(root.as_uuid())),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        root_row
            .try_get::<i64>("", "discovered_sync_revision")
            .unwrap(),
        1
    );
    let metadata = jobs
        .claim_next(
            &[WorkTaskKind::ResolveMetadata],
            "metadata-worker",
            chrono::Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(metadata.job().expected_revision(), 0);
    assert_eq!(metadata.job().input_sync_revision(), Some(1));
    assert_eq!(
        metadata.job().metadata_requirement(),
        Some(MetadataRequirement::Basic)
    );
    let report = MetadataResolveService::new(database.clone())
        .with_provider(Arc::new(CompleteProvider))
        .execute(&metadata)
        .await
        .unwrap();
    assert_eq!(report.state().as_str(), "Ready");
    assert_eq!(count(&database, "cache_invalidation_outbox").await, 2);
    let overview = database
        .query_one(
            sql.build(
                Query::select()
                    .column(Alias::new("overview"))
                    .from(Alias::new("catalog_items"))
                    .and_where(Expr::col(Alias::new("id")).eq(item_id.as_uuid())),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get::<String>("", "overview")
        .unwrap();
    assert_eq!(overview, "A linguist meets visitors.");

    let manual = TaskService::new(database.clone())
        .resolve_metadata(item_id)
        .await
        .unwrap();
    assert_eq!(manual.job().task_kind(), WorkTaskKind::ResolveMetadata);
    assert_eq!(manual.job().scope(), WorkScope::CatalogItem(item_id));
    assert_eq!(
        manual.job().metadata_requirement(),
        Some(MetadataRequirement::Full)
    );
    assert!(matches!(
        TaskService::new(database).discover_titles(root).await,
        Err(TaskServiceError::Discover(
            DiscoverTitlesError::AlreadyCurrent
        ))
    ));
}

#[tokio::test]
async fn discover_titles_recursively_publishes_music_files_as_audio_items() {
    let fixture = discovery_fixture("basic").await;
    let sql = fixture.database.get_database_backend();
    let track = StorageObjectRecordId::new();
    fixture
        .database
        .execute(
            sql.build(
                Query::update()
                    .table(Alias::new("libraries"))
                    .value(Alias::new("collection_type"), "music")
                    .value(Alias::new("name"), "Music")
                    .and_where(Expr::col(Alias::new("id")).eq(fixture.library_id)),
            ),
        )
        .await
        .unwrap();
    for (table, id_column) in [
        ("storage_objects", "id"),
        ("storage_root_objects", "storage_object_id"),
    ] {
        fixture
            .database
            .execute(
                sql.build(
                    Query::update()
                        .table(Alias::new(table))
                        .value(Alias::new("children_indexed"), true)
                        .value(Alias::new("children_index_revision"), 1_i64)
                        .and_where(
                            Expr::col(Alias::new(id_column)).eq(fixture.title_object.as_uuid()),
                        ),
                ),
            )
            .await
            .unwrap();
    }
    insert_object(
        &fixture.database,
        account_id(&fixture.database).await,
        track,
        "track-01",
        "01 - First Light.flac",
        "File",
        false,
    )
    .await;
    insert_root_relation(
        &fixture.database,
        fixture.root,
        track,
        Some(fixture.title_object),
    )
    .await;

    let report = DiscoverTitlesService::new(fixture.database.clone())
        .execute(&fixture.claimed)
        .await
        .unwrap();

    assert_eq!(report.discovered(), 1);
    let identity = Alias::new("music_identity");
    let catalog = Alias::new("music_catalog");
    let row = fixture
        .database
        .query_one(
            sql.build(
                Query::select()
                    .expr_as(
                        Expr::col((catalog.clone(), Alias::new("id"))),
                        Alias::new("item_id"),
                    )
                    .columns([
                        (catalog.clone(), Alias::new("item_type")),
                        (catalog.clone(), Alias::new("name")),
                    ])
                    .from_as(Alias::new("identity_matches"), identity.clone())
                    .join_as(
                        JoinType::InnerJoin,
                        Alias::new("catalog_items"),
                        catalog.clone(),
                        Expr::col((catalog.clone(), Alias::new("id")))
                            .equals((identity.clone(), Alias::new("candidate_catalog_item_id"))),
                    )
                    .and_where(
                        Expr::col((identity, Alias::new("storage_object_id"))).eq(track.as_uuid()),
                    ),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.try_get::<String>("", "item_type").unwrap(), "Audio");
    assert_eq!(
        row.try_get::<String>("", "name").unwrap(),
        "01 - First Light"
    );
    let item = CatalogItemId::from_uuid(row.try_get("", "item_id").unwrap());
    let jobs = WorkJobRepository::new(&fixture.database);
    let metadata = jobs
        .claim_next(
            &[WorkTaskKind::ResolveMetadata],
            "music-metadata",
            chrono::Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        metadata.job().metadata_requirement(),
        Some(MetadataRequirement::Basic)
    );
    assert_eq!(
        metadata.job().metadata_source_mode(),
        Some(MetadataSourceMode::AutomaticScrape)
    );
    let metadata_report = MetadataResolveService::new(fixture.database.clone())
        .execute(&metadata)
        .await
        .unwrap();
    assert!(metadata_report.changed());
    assert!(!metadata_report.used_nfo());
    jobs.enqueue_or_join(
        &WorkJobSpec::new(
            WorkTaskKind::IndexMediaSources,
            WorkScope::CatalogItem(item),
            0,
            100,
        )
        .unwrap()
        .with_input_sync_revision(1)
        .unwrap(),
    )
    .await
    .unwrap();
    let source_job = jobs
        .claim_next(
            &[WorkTaskKind::IndexMediaSources],
            "music-source-index",
            chrono::Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();

    SourceIndexService::new(fixture.database.clone())
        .execute(&source_job)
        .await
        .unwrap();

    let sources = CatalogPublicationRepository::new(&fixture.database)
        .active_sources(item)
        .await
        .unwrap();
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].container(), Some("flac"));
    assert_eq!(sources[0].locations()[0].storage_object_id(), track);
}

#[tokio::test]
async fn full_metadata_policy_is_preserved_by_title_discovery() {
    let fixture = discovery_fixture("full").await;
    let jobs = WorkJobRepository::new(&fixture.database);

    DiscoverTitlesService::new(fixture.database.clone())
        .execute(&fixture.claimed)
        .await
        .unwrap();
    let metadata = jobs
        .claim_next(
            &[WorkTaskKind::ResolveMetadata],
            "full-metadata-worker",
            chrono::Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        metadata.job().metadata_requirement(),
        Some(MetadataRequirement::Full)
    );
}

#[tokio::test]
async fn title_discovery_captures_local_only_metadata_source_mode() {
    let fixture = discovery_fixture("basic").await;
    fixture
        .database
        .execute(
            fixture.database.get_database_backend().build(
                Query::update()
                    .table(Alias::new("libraries"))
                    .value(Alias::new("metadata_source_mode"), "local_only")
                    .and_where(Expr::col(Alias::new("id")).eq(fixture.library_id)),
            ),
        )
        .await
        .unwrap();
    DiscoverTitlesService::new(fixture.database.clone())
        .execute(&fixture.claimed)
        .await
        .unwrap();
    let metadata = WorkJobRepository::new(&fixture.database)
        .claim_next(
            &[WorkTaskKind::ResolveMetadata],
            "local-only-discovery-worker",
            chrono::Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        metadata.job().metadata_source_mode(),
        Some(MetadataSourceMode::LocalOnly)
    );
}

#[tokio::test]
async fn metadata_none_discovers_titles_without_scheduling_resolution() {
    let fixture = discovery_fixture("none").await;
    let jobs = WorkJobRepository::new(&fixture.database);

    let report = DiscoverTitlesService::new(fixture.database.clone())
        .execute(&fixture.claimed)
        .await
        .unwrap();

    assert_eq!(report.discovered(), 1);
    assert_eq!(count(&fixture.database, "catalog_items").await, 1);
    assert!(
        jobs.claim_next(
            &[WorkTaskKind::ResolveMetadata],
            "unexpected-metadata",
            chrono::Duration::minutes(5),
        )
        .await
        .unwrap()
        .is_none()
    );
}

#[tokio::test]
async fn profile_change_after_snapshot_fences_discovery_publication() {
    let fixture = discovery_fixture("basic").await;
    let repository = DiscoverTitlesRepository::new(&fixture.database);
    let snapshot = repository.snapshot(&fixture.claimed).await.unwrap();
    fixture
        .database
        .execute(
            fixture.database.get_database_backend().build(
                Query::update()
                    .table(Alias::new("libraries"))
                    .value(Alias::new("metadata_policy"), "none")
                    .value(Alias::new("profile_version"), 2_i32)
                    .and_where(Expr::col(Alias::new("id")).eq(fixture.library_id)),
            ),
        )
        .await
        .unwrap();

    assert!(matches!(
        repository.publish(&fixture.claimed, &snapshot).await,
        Err(DiscoverTitlesError::StaleLibraryPolicy)
    ));
    assert_eq!(count(&fixture.database, "catalog_items").await, 0);
    assert_eq!(count(&fixture.database, "work_results").await, 0);
    assert!(
        WorkJobRepository::new(&fixture.database)
            .claim_next(
                &[WorkTaskKind::ResolveMetadata],
                "unexpected-metadata",
                chrono::Duration::minutes(5),
            )
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn discovery_snapshot_rejects_legacy_facts_shared_by_multiple_roots() {
    let fixture = discovery_fixture("basic").await;
    attach_title_to_second_root(&fixture).await;

    assert!(matches!(
        DiscoverTitlesRepository::new(&fixture.database)
            .snapshot(&fixture.claimed)
            .await,
        Err(DiscoverTitlesError::StorageInputPending)
    ));
}

#[tokio::test]
async fn discovery_publish_rechecks_fact_origin_after_snapshot() {
    let fixture = discovery_fixture("basic").await;
    let repository = DiscoverTitlesRepository::new(&fixture.database);
    let snapshot = repository.snapshot(&fixture.claimed).await.unwrap();
    attach_title_to_second_root(&fixture).await;

    assert!(matches!(
        repository.publish(&fixture.claimed, &snapshot).await,
        Err(DiscoverTitlesError::StorageInputPending)
    ));
    assert_eq!(count(&fixture.database, "catalog_items").await, 0);
}

async fn attach_title_to_second_root(fixture: &DiscoveryFixture) {
    let backend = fixture.database.get_database_backend();
    let account: Uuid = fixture
        .database
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("storage_account_id"))
                    .from(Alias::new("storage_roots"))
                    .and_where(Expr::col(Alias::new("id")).eq(fixture.root.as_uuid())),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get("", "storage_account_id")
        .unwrap();
    let second_root = StorageRootId::new();
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
                        account.into(),
                        format!("second-{}", second_root.as_uuid()).into(),
                        1_i64.into(),
                        1_i64.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    insert_root_relation(&fixture.database, second_root, fixture.title_object, None).await;
}

async fn count(database: &sea_orm::DatabaseConnection, table: &str) -> i64 {
    let backend = database.get_database_backend();
    database
        .query_one(
            backend.build(
                Query::select()
                    .expr_as(Expr::col(Alias::new("id")).count(), Alias::new("count"))
                    .from(Alias::new(table)),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get("", "count")
        .unwrap()
}

async fn insert_library(database: &sea_orm::DatabaseConnection, id: Uuid, metadata_policy: &str) {
    let sql = database.get_database_backend();
    database
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
                        id.into(),
                        "Movies".into(),
                        "Lazy".into(),
                        "title_layer".into(),
                        metadata_policy.into(),
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
}

async fn insert_relation(
    database: &sea_orm::DatabaseConnection,
    table: &str,
    values: [(&str, Uuid); 3],
) {
    let sql = database.get_database_backend();
    database
        .execute(
            sql.build(
                Query::insert()
                    .into_table(Alias::new(table))
                    .columns(values.iter().map(|(name, _)| Alias::new(*name)))
                    .values_panic(values.map(|(_, value)| value.into())),
            ),
        )
        .await
        .unwrap();
}

async fn insert_object(
    database: &sea_orm::DatabaseConnection,
    account: Uuid,
    id: StorageObjectRecordId,
    provider_id: &str,
    name: &str,
    object_type: &str,
    indexed: bool,
) {
    let sql = database.get_database_backend();
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
                        id.as_uuid().into(),
                        account.into(),
                        "local".into(),
                        provider_id.into(),
                        name.into(),
                        name.to_lowercase().into(),
                        object_type.into(),
                        1_i64.into(),
                        indexed.into(),
                        1_i64.into(),
                        "StableFileId".into(),
                        "Present".into(),
                    ]),
            ),
        )
        .await
        .unwrap();
}

async fn account_id(database: &sea_orm::DatabaseConnection) -> Uuid {
    database
        .query_one(
            database.get_database_backend().build(
                Query::select()
                    .column(Alias::new("id"))
                    .from(Alias::new("storage_accounts"))
                    .limit(1),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get("", "id")
        .unwrap()
}

async fn insert_root_relation(
    database: &sea_orm::DatabaseConnection,
    root: StorageRootId,
    object: StorageObjectRecordId,
    parent: Option<StorageObjectRecordId>,
) {
    let sql = database.get_database_backend();
    let is_root = parent.is_none();
    database
        .execute(
            sql.build(
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
                        root.as_uuid().into(),
                        object.as_uuid().into(),
                        parent.map(StorageObjectRecordId::as_uuid).into(),
                        1_i64.into(),
                        is_root.into(),
                        i64::from(is_root).into(),
                        "Present".into(),
                    ]),
            ),
        )
        .await
        .unwrap();
}
