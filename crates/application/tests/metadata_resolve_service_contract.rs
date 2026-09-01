use std::{
    collections::{HashMap, VecDeque},
    io::Cursor,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use futures_util::stream;
use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use sea_orm::{
    ConnectionTrait, DatabaseConnection,
    sea_query::{Alias, Expr, Query},
};
use sea_orm_migration::MigratorTrait;
use tempfile::TempDir;
use tjxy_application::{
    AssetWriteService, DirectMetadataReadService, MetadataImageBytes, MetadataImageFetchError,
    MetadataImageFetcher, MetadataResolveError, MetadataResolveService,
};
use tjxy_common::{CatalogItemId, SortKey, StorageObjectRecordId, StorageRootId};
use tjxy_db::{
    MetadataPublicationRepository, MetadataRequirement, MetadataWorkError, MetadataWorkRepository,
    WorkJobRepository, WorkJobSpec, WorkJobState, WorkScope, WorkTaskKind,
};
use tjxy_domain::{LocalMetadataAccessMode, MetadataSourceMode};
use tjxy_metadata::{MetadataCandidate, MetadataResolution, MetadataSource};
use tjxy_storage::{
    BackendError, ByteRange, ByteStream, ChangeCursor, ChangePage, ObjectPage, PageToken,
    StorageBackend, StorageCapabilities, StorageObject, StorageObjectId,
};
use tjxy_test_support::test_database;
use uuid::Uuid;

struct PosterProvider;

struct ForbiddenRemoteProvider;

struct FailingTmdbProvider;

#[async_trait]
impl tjxy_metadata::MetadataProvider for ForbiddenRemoteProvider {
    fn name(&self) -> &'static str {
        "ForbiddenRemote"
    }

    async fn resolve(
        &self,
        _lookup: &tjxy_metadata::MetadataLookup,
    ) -> Result<Option<MetadataCandidate>, tjxy_metadata::MetadataProviderError> {
        panic!("LocalOnly metadata resolution must not invoke remote providers")
    }
}

#[async_trait]
impl tjxy_metadata::MetadataProvider for FailingTmdbProvider {
    fn name(&self) -> &'static str {
        "Tmdb"
    }

    async fn resolve(
        &self,
        _lookup: &tjxy_metadata::MetadataLookup,
    ) -> Result<Option<MetadataCandidate>, tjxy_metadata::MetadataProviderError> {
        Err(tjxy_metadata::MetadataProviderError::TemporarilyUnavailable)
    }
}

#[async_trait]
impl tjxy_metadata::MetadataProvider for PosterProvider {
    fn name(&self) -> &'static str {
        "Tmdb"
    }

    async fn resolve(
        &self,
        _lookup: &tjxy_metadata::MetadataLookup,
    ) -> Result<Option<tjxy_metadata::MetadataCandidate>, tjxy_metadata::MetadataProviderError>
    {
        let source =
            tjxy_metadata::MetadataSource::new("Tmdb", Some("movie:329865"), 8_000).unwrap();
        Ok(Some(
            tjxy_metadata::MetadataCandidate::new(source)
                .with_provider_id("tmdb", "329865")
                .with_primary_image("/arrival.jpg")
                .with_details_loaded(),
        ))
    }
}

struct FixtureImageFetcher {
    bytes: Vec<u8>,
}

#[async_trait]
impl MetadataImageFetcher for FixtureImageFetcher {
    async fn fetch(
        &self,
        _reference: &tjxy_metadata::MetadataImageReference,
    ) -> Result<MetadataImageBytes, MetadataImageFetchError> {
        MetadataImageBytes::new("image/png", self.bytes.clone())
    }
}

type ExtraObject = (String, Vec<u8>, String);

struct NfoBackend {
    object_id: StorageObjectId,
    bytes: Vec<u8>,
    get_errors: Mutex<VecDeque<BackendError>>,
    ranges: Mutex<Vec<ByteRange>>,
    extra_objects: Mutex<HashMap<String, ExtraObject>>,
}

#[async_trait::async_trait]
impl StorageBackend for NfoBackend {
    async fn get_object(&self, id: &StorageObjectId) -> Result<StorageObject, BackendError> {
        if id == &self.object_id {
            if let Some(error) = self.get_errors.lock().unwrap().pop_front() {
                return Err(error);
            }
            return StorageObject::file(id.clone(), "movie.nfo", self.bytes.len() as u64)
                .with_remote_revision("nfo-r1");
        }
        let objects = self.extra_objects.lock().unwrap();
        let (name, bytes, revision) = objects
            .get(id.provider_object_id())
            .ok_or(BackendError::NotFound)?;
        StorageObject::file(id.clone(), name, bytes.len() as u64).with_remote_revision(revision)
    }

    async fn list_children(
        &self,
        _parent: &StorageObjectId,
        _page: Option<PageToken>,
    ) -> Result<ObjectPage, BackendError> {
        panic!("metadata resolution must not enumerate the backend")
    }

    async fn list_changes(&self, _cursor: ChangeCursor) -> Result<ChangePage, BackendError> {
        Err(BackendError::unsupported_capability("changes"))
    }

    async fn open_range(
        &self,
        id: &StorageObjectId,
        range: ByteRange,
    ) -> Result<ByteStream, BackendError> {
        self.ranges.lock().unwrap().push(range);
        let extra = self.extra_objects.lock().unwrap();
        let source = if id == &self.object_id {
            self.bytes.as_slice()
        } else {
            extra
                .get(id.provider_object_id())
                .ok_or(BackendError::NotFound)?
                .1
                .as_slice()
        };
        let start = usize::try_from(range.start()).unwrap();
        let end = usize::try_from(range.end_exclusive()).unwrap();
        let bytes = source[start..end].to_vec();
        Ok(Box::pin(stream::once(async move { Ok(bytes.into()) })))
    }

    fn capabilities(&self) -> StorageCapabilities {
        StorageCapabilities::new()
    }
}

struct Fixture {
    database: DatabaseConnection,
    item: CatalogItemId,
    account: Uuid,
    root: StorageRootId,
    nfo_record: StorageObjectRecordId,
    parent: StorageObjectRecordId,
    backend: Arc<NfoBackend>,
}

#[tokio::test]
async fn legacy_compact_title_is_split_before_provider_resolution() {
    let fixture = fixture().await;
    let backend = fixture.database.get_database_backend();
    fixture
        .database
        .execute(
            backend.build(
                &Query::update()
                    .table(Alias::new("catalog_items"))
                    .value(Alias::new("name"), "玩具总动员5(2026)")
                    .and_where(Expr::col(Alias::new("id")).eq(fixture.item.as_uuid()))
                    .to_owned(),
            ),
        )
        .await
        .unwrap();
    let jobs = WorkJobRepository::new(&fixture.database);
    jobs.enqueue_or_join(
        &WorkJobSpec::new(
            WorkTaskKind::ResolveMetadata,
            WorkScope::CatalogItem(fixture.item),
            1,
            20,
        )
        .unwrap()
        .with_metadata_source_mode(MetadataSourceMode::AutomaticScrape)
        .unwrap()
        .with_input_sync_revision(1)
        .unwrap(),
    )
    .await
    .unwrap();
    let claimed = jobs
        .claim_next(
            &[WorkTaskKind::ResolveMetadata],
            "legacy-title-year-snapshot",
            chrono::Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();

    let snapshot = MetadataWorkRepository::new(&fixture.database)
        .snapshot(&claimed)
        .await
        .unwrap();

    assert_eq!(snapshot.lookup().fallback_title(), "玩具总动员5");
    assert_eq!(snapshot.lookup().fallback_year(), Some(2026));
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Keeps the running-upgrade rollback and successful Full retry in one race contract.
async fn running_basic_job_rolls_back_when_its_requirement_is_upgraded_to_full() {
    let fixture = fixture().await;
    let jobs = WorkJobRepository::new(&fixture.database);
    let basic = WorkJobSpec::new(
        WorkTaskKind::ResolveMetadata,
        WorkScope::CatalogItem(fixture.item),
        1,
        20,
    )
    .unwrap()
    .with_metadata_requirement(MetadataRequirement::Basic)
    .unwrap()
    .with_input_sync_revision(1)
    .unwrap();
    jobs.enqueue_or_join(&basic).await.unwrap();
    let claimed_basic = jobs
        .claim_next(
            &[WorkTaskKind::ResolveMetadata],
            "metadata-basic-worker",
            chrono::Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    let full = WorkJobSpec::new(
        WorkTaskKind::ResolveMetadata,
        WorkScope::CatalogItem(fixture.item),
        1,
        100,
    )
    .unwrap()
    .with_metadata_requirement(MetadataRequirement::Full)
    .unwrap()
    .with_input_sync_revision(1)
    .unwrap();
    let upgraded = jobs.enqueue_or_join(&full).await.unwrap();
    assert_eq!(upgraded.job().id(), claimed_basic.id());
    assert_eq!(
        upgraded.job().metadata_requirement(),
        Some(MetadataRequirement::Full)
    );

    let service = MetadataResolveService::new(fixture.database.clone()).with_backend(
        fixture.account,
        "local",
        Arc::clone(&fixture.backend),
    );
    assert!(matches!(
        service.execute(&claimed_basic).await,
        Err(MetadataResolveError::Work(
            MetadataWorkError::RequirementUpgraded
        ))
    ));
    let backend = fixture.database.get_database_backend();
    let item = fixture
        .database
        .query_one(
            backend.build(
                Query::select()
                    .columns([
                        Alias::new("metadata_resolved_revision"),
                        Alias::new("metadata_resolved_requirement"),
                    ])
                    .from(Alias::new("catalog_items"))
                    .and_where(Expr::col(Alias::new("id")).eq(fixture.item.as_uuid())),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        item.try_get::<i64>("", "metadata_resolved_revision")
            .unwrap(),
        -1
    );
    assert_eq!(
        item.try_get::<Option<i32>>("", "metadata_resolved_requirement")
            .unwrap(),
        None
    );

    jobs.retry(
        &claimed_basic,
        chrono::Duration::zero(),
        "metadata requirement upgraded",
    )
    .await
    .unwrap();
    let claimed_full = jobs
        .claim_next(
            &[WorkTaskKind::ResolveMetadata],
            "metadata-full-worker",
            chrono::Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        claimed_full.job().metadata_requirement(),
        Some(MetadataRequirement::Full)
    );
    service.execute(&claimed_full).await.unwrap();
    let item = fixture
        .database
        .query_one(
            backend.build(
                Query::select()
                    .columns([
                        Alias::new("metadata_resolved_revision"),
                        Alias::new("metadata_resolved_requirement"),
                    ])
                    .from(Alias::new("catalog_items"))
                    .and_where(Expr::col(Alias::new("id")).eq(fixture.item.as_uuid())),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        item.try_get::<i64>("", "metadata_resolved_revision")
            .unwrap(),
        1
    );
    assert_eq!(
        item.try_get::<Option<i32>>("", "metadata_resolved_requirement")
            .unwrap(),
        Some(MetadataRequirement::Full.as_i32())
    );
    assert_eq!(association_count(&fixture, "item_genres").await, 1);
    assert_eq!(association_count(&fixture, "item_studios").await, 1);
    assert_eq!(association_count(&fixture, "item_people").await, 1);
}

#[tokio::test]
async fn metadata_get_failure_is_transient_and_the_same_claim_can_restore_presence() {
    let fixture = fixture().await;
    fixture
        .backend
        .get_errors
        .lock()
        .unwrap()
        .push_back(BackendError::TemporarilyUnavailable {
            message: "fixture detail must not be persisted".to_owned(),
        });
    let jobs = WorkJobRepository::new(&fixture.database);
    jobs.enqueue_or_join(
        &WorkJobSpec::new(
            WorkTaskKind::ResolveMetadata,
            WorkScope::CatalogItem(fixture.item),
            1,
            20,
        )
        .unwrap()
        .with_input_sync_revision(1)
        .unwrap(),
    )
    .await
    .unwrap();
    let claimed = jobs
        .claim_next(
            &[WorkTaskKind::ResolveMetadata],
            "metadata-worker",
            chrono::Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    let service = MetadataResolveService::new(fixture.database.clone()).with_backend(
        fixture.account,
        "local",
        Arc::clone(&fixture.backend),
    );

    assert!(service.execute(&claimed).await.is_err());
    assert_eq!(
        metadata_object_availability(&fixture).await,
        (
            "TemporarilyUnavailable".to_owned(),
            Some("backend-temporarily-unavailable".to_owned()),
            2,
            2,
        )
    );

    let report = service.execute(&claimed).await.unwrap();
    assert!(report.used_nfo());
    assert_eq!(
        metadata_object_availability(&fixture).await,
        ("Present".to_owned(), None, 3, 3)
    );
    assert_eq!(
        jobs.get(claimed.id()).await.unwrap().unwrap().state(),
        WorkJobState::Completed
    );
    assert_eq!(association_count(&fixture, "item_genres").await, 1);
    assert_eq!(association_count(&fixture, "item_studios").await, 1);
    assert_eq!(association_count(&fixture, "item_people").await, 1);
}

async fn association_count(fixture: &Fixture, table: &str) -> i64 {
    fixture
        .database
        .query_one(
            fixture.database.get_database_backend().build(
                Query::select()
                    .expr_as(Expr::col(Alias::new("id")).count(), Alias::new("count"))
                    .from(Alias::new(table))
                    .and_where(Expr::col(Alias::new("catalog_item_id")).eq(fixture.item.as_uuid())),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get("", "count")
        .unwrap()
}

async fn metadata_object_availability(fixture: &Fixture) -> (String, Option<String>, i64, i64) {
    let relation = Alias::new("storage_root_objects");
    let root = Alias::new("storage_roots");
    let query = Query::select()
        .column((relation.clone(), Alias::new("presence_state")))
        .column((relation.clone(), Alias::new("availability_reason")))
        .column((root.clone(), Alias::new("sync_revision")))
        .column((root.clone(), Alias::new("reconciled_sync_revision")))
        .from(relation.clone())
        .inner_join(
            root.clone(),
            Expr::col((root, Alias::new("id")))
                .equals((relation.clone(), Alias::new("storage_root_id"))),
        )
        .and_where(
            Expr::col((relation, Alias::new("storage_object_id"))).eq(fixture.nfo_record.as_uuid()),
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
        row.try_get("", "sync_revision").unwrap(),
        row.try_get("", "reconciled_sync_revision").unwrap(),
    )
}

#[tokio::test]
async fn resolve_metadata_reads_only_the_sql_selected_nfo_and_completes_durably() {
    let fixture = fixture().await;
    let jobs = WorkJobRepository::new(&fixture.database);
    jobs.enqueue_or_join(
        &WorkJobSpec::new(
            WorkTaskKind::ResolveMetadata,
            WorkScope::CatalogItem(fixture.item),
            1,
            20,
        )
        .unwrap()
        .with_input_sync_revision(1)
        .unwrap(),
    )
    .await
    .unwrap();
    let claimed = jobs
        .claim_next(
            &[WorkTaskKind::ResolveMetadata],
            "metadata-worker",
            chrono::Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    let service = MetadataResolveService::new(fixture.database.clone()).with_backend(
        fixture.account,
        "local",
        Arc::clone(&fixture.backend),
    );

    let report = service.execute(&claimed).await.unwrap();

    assert!(report.changed());
    assert!(report.used_nfo());
    assert_eq!(report.state().as_str(), "Ready");
    assert_eq!(
        jobs.get(claimed.id()).await.unwrap().unwrap().state(),
        WorkJobState::Completed
    );
    assert_eq!(
        fixture.backend.ranges.lock().unwrap().as_slice(),
        [ByteRange::new(0, fixture.backend.bytes.len() as u64).unwrap()]
    );
    let row = fixture
        .database
        .query_one(
            fixture.database.get_database_backend().build(
                Query::select()
                    .columns([
                        Alias::new("name"),
                        Alias::new("production_year"),
                        Alias::new("overview"),
                        Alias::new("metadata_state"),
                    ])
                    .from(Alias::new("catalog_items")),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.try_get::<String>("", "name").unwrap(), "Arrival");
    assert_eq!(row.try_get::<i32>("", "production_year").unwrap(), 2016);
    assert_eq!(
        row.try_get::<String>("", "metadata_state").unwrap(),
        "Ready"
    );
    let provenance = fixture
        .database
        .query_one(
            fixture.database.get_database_backend().build(
                Query::select()
                    .column(Alias::new("source_reference"))
                    .from(Alias::new("metadata_provenance"))
                    .and_where(Expr::col(Alias::new("field_name")).eq("title")),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        provenance
            .try_get::<String>("", "source_reference")
            .unwrap(),
        format!("storage-object:{}", fixture.nfo_record)
    );
}

#[tokio::test]
async fn local_only_metadata_uses_nfo_without_invoking_configured_remote_providers() {
    let fixture = fixture().await;
    let jobs = WorkJobRepository::new(&fixture.database);
    jobs.enqueue_or_join(
        &WorkJobSpec::new(
            WorkTaskKind::ResolveMetadata,
            WorkScope::CatalogItem(fixture.item),
            1,
            20,
        )
        .unwrap()
        .with_metadata_source_mode(MetadataSourceMode::LocalOnly)
        .unwrap()
        .with_input_sync_revision(1)
        .unwrap(),
    )
    .await
    .unwrap();
    let claimed = jobs
        .claim_next(
            &[WorkTaskKind::ResolveMetadata],
            "local-only-metadata-worker",
            chrono::Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    let service = MetadataResolveService::new(fixture.database.clone())
        .with_backend(fixture.account, "local", Arc::clone(&fixture.backend))
        .with_provider(Arc::new(ForbiddenRemoteProvider));

    let report = service.execute(&claimed).await.unwrap();

    assert!(report.used_nfo());
    assert_eq!(report.state().as_str(), "Ready");
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // The contract verifies one end-to-end direct metadata transaction.
async fn direct_local_metadata_indexes_refs_without_importing_catalog_or_asset_bytes() {
    let fixture = fixture().await;
    let backend = fixture.database.get_database_backend();
    fixture
        .database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("libraries"))
                    .value(Alias::new("metadata_source_mode"), "local_only")
                    .value(Alias::new("local_metadata_access_mode"), "direct"),
            ),
        )
        .await
        .unwrap();
    let jobs = WorkJobRepository::new(&fixture.database);
    jobs.enqueue_or_join(
        &WorkJobSpec::new(
            WorkTaskKind::ResolveMetadata,
            WorkScope::CatalogItem(fixture.item),
            1,
            20,
        )
        .unwrap()
        .with_metadata_source_mode(MetadataSourceMode::LocalOnly)
        .unwrap()
        .with_local_metadata_access_mode(LocalMetadataAccessMode::Direct)
        .unwrap()
        .with_input_sync_revision(1)
        .unwrap(),
    )
    .await
    .unwrap();
    let claimed = jobs
        .claim_next(
            &[WorkTaskKind::ResolveMetadata],
            "direct-worker",
            chrono::Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();

    let report = MetadataResolveService::new(fixture.database.clone())
        .with_backend(fixture.account, "local", Arc::clone(&fixture.backend))
        .with_provider(Arc::new(ForbiddenRemoteProvider))
        .execute(&claimed)
        .await
        .unwrap();

    assert!(!report.used_nfo());
    let item = fixture
        .database
        .query_one(
            backend.build(
                Query::select()
                    .columns([Alias::new("name"), Alias::new("metadata_state")])
                    .from(Alias::new("catalog_items"))
                    .and_where(Expr::col(Alias::new("id")).eq(fixture.item.as_uuid())),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        item.try_get::<String>("", "name").unwrap(),
        "Arrival (2016)"
    );
    assert_eq!(
        item.try_get::<String>("", "metadata_state").unwrap(),
        "Empty"
    );
    let refs = fixture
        .database
        .query_all(
            backend.build(
                Query::select()
                    .column(Alias::new("resource_kind"))
                    .from(Alias::new("direct_metadata_refs"))
                    .and_where(Expr::col(Alias::new("catalog_item_id")).eq(fixture.item.as_uuid())),
            ),
        )
        .await
        .unwrap();
    assert_eq!(refs.len(), 1);
    assert_eq!(
        refs[0].try_get::<String>("", "resource_kind").unwrap(),
        "Nfo"
    );
    // A later catalog revision must not hide an already indexed direct reference.
    fixture
        .database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("catalog_items"))
                    .value(Alias::new("metadata_revision"), 2_i64)
                    .and_where(Expr::col(Alias::new("id")).eq(fixture.item.as_uuid())),
            ),
        )
        .await
        .unwrap();
    let registry = tjxy_application::StorageBackendRegistry::new();
    let storage_backend: Arc<dyn StorageBackend> = fixture.backend.clone();
    registry
        .register(fixture.account, "local", storage_backend)
        .unwrap();
    let document = DirectMetadataReadService::new(fixture.database.clone())
        .with_backend_registry(registry)
        .nfo(fixture.item)
        .await
        .unwrap();
    assert_eq!(
        document.as_ref().map(|value| value.title()),
        Some(Some("Arrival"))
    );
    assert!(
        fixture
            .database
            .query_one(
                backend.build(
                    Query::select()
                        .column(Alias::new("id"))
                        .from(Alias::new("asset_blobs"))
                        .limit(1)
                )
            )
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // The contract verifies one end-to-end mixed metadata transaction.
async fn metadata_only_import_keeps_local_images_directly_readable() {
    let fixture = fixture().await;
    let backend = fixture.database.get_database_backend();
    let poster_record = StorageObjectRecordId::new();
    let poster_bytes = png_bytes();
    fixture.backend.extra_objects.lock().unwrap().insert(
        "direct-poster-object".to_owned(),
        (
            "poster.png".to_owned(),
            poster_bytes.clone(),
            "poster-r1".to_owned(),
        ),
    );
    insert_storage_object(
        &fixture.database,
        fixture.account,
        poster_record,
        "direct-poster-object",
        "poster.png",
        "File",
        Some(i64::try_from(poster_bytes.len()).unwrap()),
        Some("poster-r1"),
    )
    .await;
    insert_root_object(
        &fixture.database,
        fixture.root,
        poster_record,
        Some(fixture.parent),
        false,
    )
    .await;
    fixture
        .database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("libraries"))
                    .value(Alias::new("metadata_source_mode"), "local_only")
                    .value(
                        Alias::new("local_metadata_access_mode"),
                        "import_metadata_only",
                    ),
            ),
        )
        .await
        .unwrap();
    let jobs = WorkJobRepository::new(&fixture.database);
    jobs.enqueue_or_join(
        &WorkJobSpec::new(
            WorkTaskKind::ResolveMetadata,
            WorkScope::CatalogItem(fixture.item),
            1,
            20,
        )
        .unwrap()
        .with_metadata_source_mode(MetadataSourceMode::LocalOnly)
        .unwrap()
        .with_local_metadata_access_mode(LocalMetadataAccessMode::ImportMetadataOnly)
        .unwrap()
        .with_input_sync_revision(1)
        .unwrap(),
    )
    .await
    .unwrap();
    let claimed = jobs
        .claim_next(
            &[WorkTaskKind::ResolveMetadata],
            "metadata-only-import-worker",
            chrono::Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();

    let registry = tjxy_application::StorageBackendRegistry::new();
    let storage_backend: Arc<dyn StorageBackend> = fixture.backend.clone();
    registry
        .register(fixture.account, "local", storage_backend)
        .unwrap();
    let service =
        DirectMetadataReadService::new(fixture.database.clone()).with_backend_registry(registry);
    let report = MetadataResolveService::new(fixture.database.clone())
        .with_backend(fixture.account, "local", Arc::clone(&fixture.backend))
        .with_provider(Arc::new(ForbiddenRemoteProvider))
        .execute(&claimed)
        .await
        .unwrap();

    assert!(report.used_nfo());
    assert!(service.nfo(fixture.item).await.unwrap().is_none());
    // Direct images must remain readable even when the library's metadata
    // source is automatic; only the image import toggle controls shadowing.
    fixture
        .database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("libraries"))
                    .value(Alias::new("metadata_source_mode"), "automatic_scrape"),
            ),
        )
        .await
        .unwrap();
    let image = service
        .image(fixture.item, tjxy_common::ImageType::Primary, 0)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(image.mime_type(), "image/png");
    assert_eq!(image.size(), poster_bytes.len() as u64);
    assert!(
        fixture
            .database
            .query_one(
                backend.build(
                    Query::select()
                        .column(Alias::new("id"))
                        .from(Alias::new("item_assets"))
                        .limit(1),
                ),
            )
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn newer_scope_observation_does_not_stale_an_unchanged_children_snapshot() {
    let fixture = fixture().await;
    let backend = fixture.database.get_database_backend();
    let scope_object: Uuid = fixture
        .database
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("storage_object_id"))
                    .from(Alias::new("identity_matches"))
                    .and_where(
                        Expr::col(Alias::new("candidate_catalog_item_id"))
                            .eq(fixture.item.as_uuid()),
                    ),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get("", "storage_object_id")
        .unwrap();
    fixture
        .database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("storage_root_objects"))
                    .value(Alias::new("observed_sync_revision"), 2_i64)
                    .and_where(Expr::col(Alias::new("storage_object_id")).eq(scope_object)),
            ),
        )
        .await
        .unwrap();
    fixture
        .database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("storage_roots"))
                    .value(Alias::new("sync_revision"), 2_i64)
                    .value(Alias::new("reconciled_sync_revision"), 2_i64),
            ),
        )
        .await
        .unwrap();
    let jobs = WorkJobRepository::new(&fixture.database);
    jobs.enqueue_or_join(
        &WorkJobSpec::new(
            WorkTaskKind::ResolveMetadata,
            WorkScope::CatalogItem(fixture.item),
            1,
            20,
        )
        .unwrap()
        .with_input_sync_revision(1)
        .unwrap(),
    )
    .await
    .unwrap();
    let claimed = jobs
        .claim_next(
            &[WorkTaskKind::ResolveMetadata],
            "metadata-worker",
            chrono::Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();

    let report = MetadataResolveService::new(fixture.database.clone())
        .with_backend(fixture.account, "local", Arc::clone(&fixture.backend))
        .execute(&claimed)
        .await
        .unwrap();

    assert!(report.used_nfo());
}

#[tokio::test]
async fn pending_sidecar_fact_rejects_a_metadata_snapshot() {
    let fixture = fixture().await;
    let jobs = WorkJobRepository::new(&fixture.database);
    jobs.enqueue_or_join(
        &WorkJobSpec::new(
            WorkTaskKind::ResolveMetadata,
            WorkScope::CatalogItem(fixture.item),
            1,
            20,
        )
        .unwrap()
        .with_input_sync_revision(1)
        .unwrap(),
    )
    .await
    .unwrap();
    let claimed = jobs
        .claim_next(
            &[WorkTaskKind::ResolveMetadata],
            "metadata-pending-root-snapshot",
            chrono::Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    mark_object_fact_pending(&fixture.database, fixture.root, fixture.nfo_record).await;

    let error = MetadataWorkRepository::new(&fixture.database)
        .snapshot(&claimed)
        .await
        .unwrap_err();

    assert!(matches!(error, MetadataWorkError::StaleOrUnavailable));
}

#[tokio::test]
async fn pending_sidecar_fact_rejects_metadata_commit_after_snapshot() {
    let fixture = fixture().await;
    let jobs = WorkJobRepository::new(&fixture.database);
    jobs.enqueue_or_join(
        &WorkJobSpec::new(
            WorkTaskKind::ResolveMetadata,
            WorkScope::CatalogItem(fixture.item),
            1,
            20,
        )
        .unwrap()
        .with_input_sync_revision(1)
        .unwrap(),
    )
    .await
    .unwrap();
    let claimed = jobs
        .claim_next(
            &[WorkTaskKind::ResolveMetadata],
            "metadata-pending-root-commit",
            chrono::Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    let repository = MetadataWorkRepository::new(&fixture.database);
    let snapshot = repository.snapshot(&claimed).await.unwrap();
    let resolution = MetadataResolution::from_candidate(
        snapshot.lookup(),
        MetadataCandidate::new(MetadataSource::new("Fixture", None::<String>, 1_000).unwrap())
            .with_title("Arrival"),
    )
    .unwrap();
    mark_object_fact_pending(&fixture.database, fixture.root, fixture.nfo_record).await;

    let error = repository
        .commit(&claimed, &snapshot, &resolution, &[], false, Vec::new())
        .await
        .unwrap_err();

    assert!(matches!(error, MetadataWorkError::StaleOrUnavailable));
    assert_eq!(
        jobs.get(claimed.id()).await.unwrap().unwrap().state(),
        WorkJobState::Running
    );
}

#[tokio::test]
async fn direct_metadata_publication_stales_an_older_running_resolver() {
    let fixture = fixture().await;
    let jobs = WorkJobRepository::new(&fixture.database);
    jobs.enqueue_or_join(
        &WorkJobSpec::new(
            WorkTaskKind::ResolveMetadata,
            WorkScope::CatalogItem(fixture.item),
            1,
            20,
        )
        .unwrap()
        .with_metadata_requirement(MetadataRequirement::Full)
        .unwrap()
        .with_input_sync_revision(1)
        .unwrap(),
    )
    .await
    .unwrap();
    let claimed = jobs
        .claim_next(
            &[WorkTaskKind::ResolveMetadata],
            "metadata-before-admin-import",
            chrono::Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    let repository = MetadataWorkRepository::new(&fixture.database);
    let snapshot = repository.snapshot(&claimed).await.unwrap();
    let worker_resolution = MetadataResolution::from_candidate(
        snapshot.lookup(),
        MetadataCandidate::new(MetadataSource::new("Worker", None::<String>, 1_000).unwrap())
            .with_title("Worker Title"),
    )
    .unwrap();
    let admin_resolution = MetadataResolution::from_candidate(
        snapshot.lookup(),
        MetadataCandidate::new(MetadataSource::new("Admin", None::<String>, 10_000).unwrap())
            .with_title("Administrator Title"),
    )
    .unwrap();
    MetadataPublicationRepository::new(&fixture.database)
        .publish(fixture.item, &admin_resolution)
        .await
        .unwrap();

    let error = repository
        .commit(
            &claimed,
            &snapshot,
            &worker_resolution,
            &[],
            false,
            Vec::new(),
        )
        .await
        .unwrap_err();

    assert!(matches!(error, MetadataWorkError::StaleOrUnavailable));
    let item = fixture
        .database
        .query_one(
            fixture.database.get_database_backend().build(
                Query::select()
                    .columns([Alias::new("name"), Alias::new("metadata_revision")])
                    .from(Alias::new("catalog_items"))
                    .and_where(Expr::col(Alias::new("id")).eq(fixture.item.as_uuid())),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        item.try_get::<String>("", "name").unwrap(),
        "Administrator Title"
    );
    assert_eq!(item.try_get::<i64>("", "metadata_revision").unwrap(), 2);
}

#[tokio::test]
async fn reconciled_sidecar_fact_change_rejects_metadata_commit_after_snapshot() {
    let fixture = fixture().await;
    let backend = fixture.database.get_database_backend();
    fixture
        .database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("storage_objects"))
                    .value(Alias::new("remote_revision"), Option::<String>::None)
                    .and_where(Expr::col(Alias::new("id")).eq(fixture.nfo_record.as_uuid())),
            ),
        )
        .await
        .unwrap();
    let jobs = WorkJobRepository::new(&fixture.database);
    jobs.enqueue_or_join(
        &WorkJobSpec::new(
            WorkTaskKind::ResolveMetadata,
            WorkScope::CatalogItem(fixture.item),
            1,
            20,
        )
        .unwrap()
        .with_input_sync_revision(1)
        .unwrap(),
    )
    .await
    .unwrap();
    let claimed = jobs
        .claim_next(
            &[WorkTaskKind::ResolveMetadata],
            "metadata-reconciled-fact-change",
            chrono::Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    let repository = MetadataWorkRepository::new(&fixture.database);
    let snapshot = repository.snapshot(&claimed).await.unwrap();
    let resolution = MetadataResolution::from_candidate(
        snapshot.lookup(),
        MetadataCandidate::new(MetadataSource::new("Fixture", None::<String>, 1_000).unwrap())
            .with_title("Arrival"),
    )
    .unwrap();
    fixture
        .database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("storage_roots"))
                    .value(Alias::new("sync_revision"), 2_i64)
                    .value(Alias::new("reconciled_sync_revision"), 2_i64)
                    .and_where(Expr::col(Alias::new("id")).eq(fixture.root.as_uuid())),
            ),
        )
        .await
        .unwrap();
    fixture
        .database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("storage_objects"))
                    .value(Alias::new("observed_sync_revision"), 2_i64)
                    .value(
                        Alias::new("facts_observed_storage_root_id"),
                        fixture.root.as_uuid(),
                    )
                    .and_where(Expr::col(Alias::new("id")).eq(fixture.nfo_record.as_uuid())),
            ),
        )
        .await
        .unwrap();
    fixture
        .database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("storage_root_objects"))
                    .value(Alias::new("observed_sync_revision"), 2_i64)
                    .and_where(Expr::col(Alias::new("storage_root_id")).eq(fixture.root.as_uuid()))
                    .and_where(
                        Expr::col(Alias::new("storage_object_id")).eq(fixture.nfo_record.as_uuid()),
                    ),
            ),
        )
        .await
        .unwrap();

    let error = repository
        .commit(&claimed, &snapshot, &resolution, &[], false, Vec::new())
        .await
        .unwrap_err();

    assert!(matches!(error, MetadataWorkError::StaleOrUnavailable));
}

#[tokio::test]
async fn unrelated_pending_root_revision_does_not_reject_metadata_snapshot() {
    let fixture = fixture().await;
    let jobs = WorkJobRepository::new(&fixture.database);
    jobs.enqueue_or_join(
        &WorkJobSpec::new(
            WorkTaskKind::ResolveMetadata,
            WorkScope::CatalogItem(fixture.item),
            1,
            20,
        )
        .unwrap()
        .with_input_sync_revision(1)
        .unwrap(),
    )
    .await
    .unwrap();
    let claimed = jobs
        .claim_next(
            &[WorkTaskKind::ResolveMetadata],
            "metadata-unrelated-pending-root",
            chrono::Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    fixture
        .database
        .execute(
            fixture.database.get_database_backend().build(
                Query::update()
                    .table(Alias::new("storage_roots"))
                    .value(Alias::new("sync_revision"), 2_i64)
                    .and_where(Expr::col(Alias::new("id")).eq(fixture.root.as_uuid())),
            ),
        )
        .await
        .unwrap();

    MetadataWorkRepository::new(&fixture.database)
        .snapshot(&claimed)
        .await
        .unwrap();
}

#[tokio::test]
async fn resolve_metadata_without_nfo_uses_naming_fallback_without_a_backend() {
    let fixture = fixture().await;
    let sql = fixture.database.get_database_backend();
    fixture
        .database
        .execute(
            sql.build(
                Query::delete()
                    .from_table(Alias::new("storage_root_objects"))
                    .and_where(
                        sea_orm::sea_query::Expr::col(Alias::new("storage_object_id"))
                            .eq(fixture.nfo_record.as_uuid()),
                    ),
            ),
        )
        .await
        .unwrap();
    fixture
        .database
        .execute(
            sql.build(
                Query::delete()
                    .from_table(Alias::new("storage_objects"))
                    .and_where(
                        sea_orm::sea_query::Expr::col(Alias::new("id"))
                            .eq(fixture.nfo_record.as_uuid()),
                    ),
            ),
        )
        .await
        .unwrap();
    let jobs = WorkJobRepository::new(&fixture.database);
    jobs.enqueue_or_join(
        &WorkJobSpec::new(
            WorkTaskKind::ResolveMetadata,
            WorkScope::CatalogItem(fixture.item),
            1,
            20,
        )
        .unwrap()
        .with_input_sync_revision(1)
        .unwrap(),
    )
    .await
    .unwrap();
    let claimed = jobs
        .claim_next(
            &[WorkTaskKind::ResolveMetadata],
            "metadata-worker",
            chrono::Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();

    let report = MetadataResolveService::new(fixture.database.clone())
        .execute(&claimed)
        .await
        .unwrap();

    assert!(report.changed());
    assert!(!report.used_nfo());
    assert_eq!(report.state().as_str(), "Partial");
    assert_eq!(
        jobs.get(claimed.id()).await.unwrap().unwrap().state(),
        WorkJobState::Completed
    );
    let resolved_revision = fixture
        .database
        .query_one(
            sql.build(
                Query::select()
                    .column(Alias::new("metadata_resolved_revision"))
                    .from(Alias::new("catalog_items"))
                    .and_where(Expr::col(Alias::new("id")).eq(fixture.item.as_uuid())),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get::<i64>("", "metadata_resolved_revision")
        .unwrap();
    assert_eq!(resolved_revision, 1);
    assert!(fixture.backend.ranges.lock().unwrap().is_empty());
}

#[tokio::test]
async fn malformed_nfo_records_a_warning_and_completes_with_fallback_metadata() {
    let mut fixture = fixture().await;
    let declared_size = fixture.backend.bytes.len();
    let mut malformed = b"<movie><title>broken".to_vec();
    malformed.resize(declared_size, b' ');
    Arc::get_mut(&mut fixture.backend).unwrap().bytes = malformed;
    let jobs = WorkJobRepository::new(&fixture.database);
    jobs.enqueue_or_join(
        &WorkJobSpec::new(
            WorkTaskKind::ResolveMetadata,
            WorkScope::CatalogItem(fixture.item),
            1,
            20,
        )
        .unwrap()
        .with_input_sync_revision(1)
        .unwrap(),
    )
    .await
    .unwrap();
    let claimed = jobs
        .claim_next(
            &[WorkTaskKind::ResolveMetadata],
            "metadata-worker",
            chrono::Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    let service = MetadataResolveService::new(fixture.database.clone()).with_backend(
        fixture.account,
        "local",
        Arc::clone(&fixture.backend),
    );

    let report = service.execute(&claimed).await.unwrap();

    assert!(!report.used_nfo());
    assert_eq!(report.state().as_str(), "Partial");
    assert_eq!(
        jobs.get(claimed.id()).await.unwrap().unwrap().state(),
        WorkJobState::Completed
    );
    let result = fixture
        .database
        .query_one(
            fixture.database.get_database_backend().build(
                Query::select()
                    .column(Alias::new("warnings"))
                    .from(Alias::new("work_results"))
                    .and_where(Expr::col(Alias::new("job_id")).eq(claimed.id().as_uuid())),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    let warnings: serde_json::Value = result.try_get("", "warnings").unwrap();
    assert!(warnings[0].as_str().unwrap().starts_with("Nfo: "));
}

#[tokio::test]
async fn resolve_metadata_atomically_publishes_tmdb_primary_image_with_one_generation_bump() {
    let fixture = fixture().await;
    let jobs = WorkJobRepository::new(&fixture.database);
    jobs.enqueue_or_join(
        &WorkJobSpec::new(
            WorkTaskKind::ResolveMetadata,
            WorkScope::CatalogItem(fixture.item),
            1,
            20,
        )
        .unwrap()
        .with_input_sync_revision(1)
        .unwrap(),
    )
    .await
    .unwrap();
    let claimed = jobs
        .claim_next(
            &[WorkTaskKind::ResolveMetadata],
            "metadata-worker",
            chrono::Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    let root = TempDir::new().unwrap();
    let writer = Arc::new(
        AssetWriteService::new(fixture.database.clone(), root.path())
            .await
            .unwrap(),
    );
    let service = MetadataResolveService::new(fixture.database.clone())
        .with_backend(fixture.account, "local", Arc::clone(&fixture.backend))
        .with_provider(Arc::new(PosterProvider))
        .with_asset_writer(writer)
        .with_image_fetcher(Arc::new(FixtureImageFetcher { bytes: png_bytes() }));

    let report = service.execute(&claimed).await.unwrap();

    assert!(report.changed());
    let item_asset = Alias::new("item_assets");
    let asset_blob = Alias::new("asset_blobs");
    let asset_query = Query::select()
        .column((item_asset.clone(), Alias::new("source_provider")))
        .column((item_asset.clone(), Alias::new("source_reference")))
        .column((asset_blob.clone(), Alias::new("mime_type")))
        .from(item_asset.clone())
        .inner_join(
            asset_blob.clone(),
            Expr::col((asset_blob, Alias::new("id")))
                .equals((item_asset, Alias::new("asset_blob_id"))),
        )
        .to_owned();
    let asset = fixture
        .database
        .query_one(fixture.database.get_database_backend().build(&asset_query))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        asset.try_get::<String>("", "source_provider").unwrap(),
        "Tmdb"
    );
    assert_eq!(
        asset.try_get::<String>("", "source_reference").unwrap(),
        "/arrival.jpg"
    );
    assert_eq!(
        asset.try_get::<String>("", "mime_type").unwrap(),
        "image/png"
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
    assert_eq!(generation, 1);
    let result = fixture
        .database
        .query_one(
            fixture.database.get_database_backend().build(
                Query::select()
                    .column(Alias::new("counters"))
                    .from(Alias::new("work_results"))
                    .and_where(Expr::col(Alias::new("job_id")).eq(claimed.id().as_uuid())),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    let result: serde_json::Value = result.try_get("", "counters").unwrap();
    assert_eq!(result["image_changed"], true);
}

#[tokio::test]
async fn tmdb_detail_failure_does_not_complete_or_publish_partial_metadata() {
    let fixture = fixture().await;
    let jobs = WorkJobRepository::new(&fixture.database);
    jobs.enqueue_or_join(
        &WorkJobSpec::new(
            WorkTaskKind::ResolveMetadata,
            WorkScope::CatalogItem(fixture.item),
            1,
            20,
        )
        .unwrap()
        .with_input_sync_revision(1)
        .unwrap(),
    )
    .await
    .unwrap();
    let claimed = jobs
        .claim_next(
            &[WorkTaskKind::ResolveMetadata],
            "metadata-worker",
            chrono::Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    let service = MetadataResolveService::new(fixture.database.clone())
        .with_backend(fixture.account, "local", Arc::clone(&fixture.backend))
        .with_provider(Arc::new(FailingTmdbProvider));

    assert!(matches!(
        service.execute(&claimed).await,
        Err(MetadataResolveError::Provider(
            tjxy_metadata::MetadataProviderError::TemporarilyUnavailable
        ))
    ));
    assert_eq!(
        jobs.get(claimed.id()).await.unwrap().unwrap().state(),
        WorkJobState::Running
    );
    let row = fixture
        .database
        .query_one(
            fixture.database.get_database_backend().build(
                Query::select()
                    .columns([Alias::new("name"), Alias::new("metadata_resolved_revision")])
                    .from(Alias::new("catalog_items"))
                    .and_where(Expr::col(Alias::new("id")).eq(fixture.item.as_uuid())),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.try_get::<String>("", "name").unwrap(), "Arrival (2016)");
    assert_eq!(
        row.try_get::<i64>("", "metadata_resolved_revision")
            .unwrap(),
        -1
    );
}

#[tokio::test]
async fn invalid_remote_image_records_a_warning_without_losing_text_metadata() {
    let fixture = fixture().await;
    let jobs = WorkJobRepository::new(&fixture.database);
    jobs.enqueue_or_join(
        &WorkJobSpec::new(
            WorkTaskKind::ResolveMetadata,
            WorkScope::CatalogItem(fixture.item),
            1,
            20,
        )
        .unwrap()
        .with_input_sync_revision(1)
        .unwrap(),
    )
    .await
    .unwrap();
    let claimed = jobs
        .claim_next(
            &[WorkTaskKind::ResolveMetadata],
            "metadata-worker",
            chrono::Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    let root = TempDir::new().unwrap();
    let writer = Arc::new(
        AssetWriteService::new(fixture.database.clone(), root.path())
            .await
            .unwrap(),
    );
    let service = MetadataResolveService::new(fixture.database.clone())
        .with_backend(fixture.account, "local", Arc::clone(&fixture.backend))
        .with_provider(Arc::new(PosterProvider))
        .with_asset_writer(writer)
        .with_image_fetcher(Arc::new(FixtureImageFetcher {
            bytes: b"not an image".to_vec(),
        }));

    let report = service.execute(&claimed).await.unwrap();

    assert!(report.changed());
    assert_eq!(report.state().as_str(), "Ready");
    assert!(
        fixture
            .database
            .query_one(
                fixture.database.get_database_backend().build(
                    Query::select()
                        .column(Alias::new("id"))
                        .from(Alias::new("item_assets")),
                ),
            )
            .await
            .unwrap()
            .is_none()
    );
    let result = fixture
        .database
        .query_one(
            fixture.database.get_database_backend().build(
                Query::select()
                    .column(Alias::new("warnings"))
                    .from(Alias::new("work_results"))
                    .and_where(Expr::col(Alias::new("job_id")).eq(claimed.id().as_uuid())),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    let warnings: serde_json::Value = result.try_get("", "warnings").unwrap();
    assert!(warnings[0].as_str().unwrap().starts_with("Tmdb image: "));
}

#[tokio::test]
async fn local_only_metadata_publishes_a_sibling_poster_without_remote_fetching() {
    let fixture = fixture().await;
    let poster_record = StorageObjectRecordId::new();
    let poster_bytes = png_bytes();
    fixture.backend.extra_objects.lock().unwrap().insert(
        "poster-object".to_owned(),
        (
            "poster.png".to_owned(),
            poster_bytes.clone(),
            "poster-r1".to_owned(),
        ),
    );
    insert_storage_object(
        &fixture.database,
        fixture.account,
        poster_record,
        "poster-object",
        "poster.png",
        "File",
        Some(i64::try_from(poster_bytes.len()).unwrap()),
        Some("poster-r1"),
    )
    .await;
    insert_root_object(
        &fixture.database,
        fixture.root,
        poster_record,
        Some(fixture.parent),
        false,
    )
    .await;
    let jobs = WorkJobRepository::new(&fixture.database);
    jobs.enqueue_or_join(
        &WorkJobSpec::new(
            WorkTaskKind::ResolveMetadata,
            WorkScope::CatalogItem(fixture.item),
            1,
            20,
        )
        .unwrap()
        .with_metadata_source_mode(MetadataSourceMode::LocalOnly)
        .unwrap()
        .with_input_sync_revision(1)
        .unwrap(),
    )
    .await
    .unwrap();
    let claimed = jobs
        .claim_next(
            &[WorkTaskKind::ResolveMetadata],
            "local-image-worker",
            chrono::Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    let assets = TempDir::new().unwrap();
    let writer = Arc::new(
        AssetWriteService::new(fixture.database.clone(), assets.path())
            .await
            .unwrap(),
    );
    let service = MetadataResolveService::new(fixture.database.clone())
        .with_backend(fixture.account, "local", Arc::clone(&fixture.backend))
        .with_provider(Arc::new(ForbiddenRemoteProvider))
        .with_asset_writer(writer);

    let report = service.execute(&claimed).await.unwrap();

    assert!(report.used_nfo());
    let asset = fixture
        .database
        .query_one(
            fixture.database.get_database_backend().build(
                Query::select()
                    .columns([
                        Alias::new("image_type"),
                        Alias::new("source_provider"),
                        Alias::new("source_reference"),
                    ])
                    .from(Alias::new("item_assets")),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        asset.try_get::<String>("", "image_type").unwrap(),
        "Primary"
    );
    assert_eq!(
        asset.try_get::<String>("", "source_provider").unwrap(),
        "Local"
    );
    assert_eq!(
        asset.try_get::<String>("", "source_reference").unwrap(),
        format!("storage-object:{poster_record}")
    );
}

fn png_bytes() -> Vec<u8> {
    let image = DynamicImage::ImageRgba8(RgbaImage::from_pixel(2, 3, Rgba([1, 2, 3, 255])));
    let mut bytes = Cursor::new(Vec::new());
    image.write_to(&mut bytes, ImageFormat::Png).unwrap();
    bytes.into_inner()
}

#[allow(clippy::too_many_lines)]
async fn fixture() -> Fixture {
    let database = test_database().await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    let sql = database.get_database_backend();
    let library = Uuid::new_v4();
    let item = CatalogItemId::new();
    let account = Uuid::new_v4();
    let root = StorageRootId::new();
    let parent = StorageObjectRecordId::new();
    let nfo_record = StorageObjectRecordId::new();
    let nfo_bytes = br#"<movie><title>Arrival</title><year>2016</year><plot>A linguist meets visitors.</plot><uniqueid type="tmdb">329865</uniqueid><genre>Science Fiction</genre><studio>Paramount</studio><actor><name>Amy Adams</name><role>Louise Banks</role><order>1</order></actor></movie>"#.to_vec();
    let nfo_backend_id = StorageObjectId::new("filesystem", "nfo-object").unwrap();
    let backend = Arc::new(NfoBackend {
        object_id: nfo_backend_id.clone(),
        bytes: nfo_bytes.clone(),
        get_errors: Mutex::new(VecDeque::new()),
        ranges: Mutex::new(Vec::new()),
        extra_objects: Mutex::new(HashMap::new()),
    });
    insert_library(&database, library).await;
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
                        Alias::new("structure_state"),
                        Alias::new("source_state"),
                        Alias::new("structure_expansion_revision"),
                        Alias::new("source_index_revision"),
                        Alias::new("metadata_revision"),
                        Alias::new("is_present"),
                    ])
                    .values_panic([
                        item.as_uuid().into(),
                        "Movie".into(),
                        "Arrival (2016)".into(),
                        "arrival (2016)".into(),
                        SortKey::from_text("Arrival (2016)").into_bytes().into(),
                        "Matched".into(),
                        "Empty".into(),
                        "NotApplicable".into(),
                        "Unknown".into(),
                        0_i64.into(),
                        0_i64.into(),
                        1_i64.into(),
                        true.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    insert_simple_relation(
        &database,
        "library_catalog_items",
        [
            ("id", Uuid::new_v4()),
            ("library_id", library),
            ("catalog_item_id", item.as_uuid()),
        ],
    )
    .await;
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
    insert_simple_relation(
        &database,
        "library_storage_roots",
        [
            ("id", Uuid::new_v4()),
            ("library_id", library),
            ("storage_root_id", root.as_uuid()),
        ],
    )
    .await;
    insert_storage_object(
        &database,
        account,
        parent,
        "parent",
        "Arrival",
        "Directory",
        None,
        None,
    )
    .await;
    insert_storage_object(
        &database,
        account,
        nfo_record,
        nfo_backend_id.provider_object_id(),
        "movie.nfo",
        "File",
        Some(i64::try_from(nfo_bytes.len()).unwrap()),
        Some("nfo-r1"),
    )
    .await;
    insert_root_object(&database, root, parent, None, true).await;
    insert_root_object(&database, root, nfo_record, Some(parent), false).await;
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
    Fixture {
        database,
        item,
        account,
        root,
        nfo_record,
        parent,
        backend,
    }
}

async fn mark_object_fact_pending(
    database: &DatabaseConnection,
    root: StorageRootId,
    object: StorageObjectRecordId,
) {
    let sql = database.get_database_backend();
    database
        .execute(
            sql.build(
                Query::update()
                    .table(Alias::new("storage_roots"))
                    .value(Alias::new("sync_revision"), 2_i64)
                    .and_where(Expr::col(Alias::new("id")).eq(root.as_uuid())),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            sql.build(
                Query::update()
                    .table(Alias::new("storage_objects"))
                    .value(Alias::new("observed_sync_revision"), 2_i64)
                    .value(Alias::new("facts_observed_storage_root_id"), root.as_uuid())
                    .and_where(Expr::col(Alias::new("id")).eq(object.as_uuid())),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            sql.build(
                Query::update()
                    .table(Alias::new("storage_root_objects"))
                    .value(Alias::new("observed_sync_revision"), 2_i64)
                    .and_where(Expr::col(Alias::new("storage_root_id")).eq(root.as_uuid()))
                    .and_where(Expr::col(Alias::new("storage_object_id")).eq(object.as_uuid())),
            ),
        )
        .await
        .unwrap();
}

async fn insert_library(database: &DatabaseConnection, library: Uuid) {
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
                        library.into(),
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
}

async fn insert_simple_relation(
    database: &DatabaseConnection,
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

#[allow(clippy::too_many_arguments)]
async fn insert_storage_object(
    database: &DatabaseConnection,
    account: Uuid,
    id: StorageObjectRecordId,
    provider_id: &str,
    name: &str,
    kind: &str,
    size: Option<i64>,
    revision: Option<&str>,
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
                        Alias::new("size"),
                        Alias::new("remote_revision"),
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
                        kind.into(),
                        size.into(),
                        revision.into(),
                        1_i64.into(),
                        (kind == "Directory").into(),
                        1_i64.into(),
                        "StableFileId".into(),
                        "Present".into(),
                    ]),
            ),
        )
        .await
        .unwrap();
}

async fn insert_root_object(
    database: &DatabaseConnection,
    root: StorageRootId,
    object: StorageObjectRecordId,
    parent: Option<StorageObjectRecordId>,
    indexed: bool,
) {
    let sql = database.get_database_backend();
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
                        indexed.into(),
                        1_i64.into(),
                        "Present".into(),
                    ]),
            ),
        )
        .await
        .unwrap();
}
