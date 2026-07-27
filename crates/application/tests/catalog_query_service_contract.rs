use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use async_trait::async_trait;
use chrono::Utc;
use sea_orm::{
    ConnectionTrait, DatabaseConnection,
    sea_query::{Alias, Expr, Query},
};
use sea_orm_migration::MigratorTrait;
use tjxy_application::{CatalogQueryService, CatalogServiceError};
use tjxy_cache::{CacheKeyBuilder, CacheStore};
use tjxy_common::{CatalogItemId, SortKey, UserId, Username};
use tjxy_db::{
    AuthRepository, BrowseParent, CatalogPageRequest, UserDataPatch, UserDataRepository,
};
use tjxy_test_support::test_database;
use tokio::sync::Barrier;
use uuid::Uuid;

async fn first_library_id(database: &DatabaseConnection) -> Uuid {
    database
        .query_one(
            database.get_database_backend().build(
                Query::select()
                    .column(Alias::new("id"))
                    .from(Alias::new("libraries"))
                    .limit(1),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get("", "id")
        .unwrap()
}

async fn increment_catalog_generation(database: &DatabaseConnection) {
    database
        .execute(
            database.get_database_backend().build(
                Query::update()
                    .table(Alias::new("catalog_state"))
                    .value(
                        Alias::new("generation"),
                        Expr::col(Alias::new("generation")).add(1),
                    )
                    .and_where(Expr::col(Alias::new("id")).eq(1)),
            ),
        )
        .await
        .unwrap();
}

async fn service_fixture() -> (CatalogQueryService, DatabaseConnection) {
    let database = test_database().await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    let backend = database.get_database_backend();
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
                        Uuid::new_v4().into(),
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
    (CatalogQueryService::new(database.clone()), database)
}

async fn service() -> CatalogQueryService {
    service_fixture().await.0
}

#[allow(clippy::too_many_lines)] // Builds the minimum active/probed source graph for cache reads.
async fn seed_playback_cache_fixture(database: &DatabaseConnection) -> CatalogItemId {
    let backend = database.get_database_backend();
    let library = first_library_id(database).await;
    let item = CatalogItemId::new();
    let account = Uuid::new_v4();
    let object = Uuid::new_v4();
    let job = Uuid::new_v4();
    let publication = Uuid::new_v4();
    let source = Uuid::new_v4();
    let presentation = Uuid::new_v4();
    let location = Uuid::new_v4();

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
                        item.as_uuid().into(),
                        "Movie".into(),
                        "Cache fixture".into(),
                        "cache fixture".into(),
                        SortKey::from_text("Cache fixture").into_bytes().into(),
                        "Matched".into(),
                        "Ready".into(),
                        "Expanded".into(),
                        "Indexed".into(),
                        1_i64.into(),
                        1_i64.into(),
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
                        account.into(),
                        "filesystem".into(),
                        "Cache fixture".into(),
                        format!("account-{account}").into(),
                        "fixture".into(),
                        "Ready".into(),
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
                        Alias::new("size"),
                        Alias::new("observed_sync_revision"),
                        Alias::new("children_indexed"),
                        Alias::new("children_index_revision"),
                        Alias::new("identity_quality"),
                        Alias::new("presence_state"),
                    ])
                    .values_panic([
                        object.into(),
                        account.into(),
                        "fixture-drive".into(),
                        format!("object-{object}").into(),
                        "Cache fixture.mkv".into(),
                        "cache fixture.mkv".into(),
                        "File".into(),
                        1_i64.into(),
                        1_i64.into(),
                        false.into(),
                        0_i64.into(),
                        "ProviderStable".into(),
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
                    .into_table(Alias::new("work_jobs"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("task_kind"),
                        Alias::new("scope_type"),
                        Alias::new("scope_id"),
                        Alias::new("expected_revision"),
                        Alias::new("state"),
                        Alias::new("priority"),
                        Alias::new("attempt_count"),
                    ])
                    .values_panic([
                        job.into(),
                        "IndexMediaSources".into(),
                        "CatalogItem".into(),
                        item.as_uuid().into(),
                        1_i64.into(),
                        "Completed".into(),
                        100.into(),
                        1.into(),
                    ]),
            ),
        )
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
                        Alias::new("activated_generation"),
                    ])
                    .values_panic([
                        publication.into(),
                        job.into(),
                        item.as_uuid().into(),
                        "Sources".into(),
                        1_i64.into(),
                        "Active".into(),
                        "a".repeat(64).into(),
                        1_i64.into(),
                        1_i64.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("media_sources"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("catalog_item_id"),
                        Alias::new("presentation_key"),
                        Alias::new("container"),
                        Alias::new("probe_state"),
                        Alias::new("probe_revision"),
                    ])
                    .values_panic([
                        source.into(),
                        item.as_uuid().into(),
                        presentation.into(),
                        "mkv".into(),
                        "Probed".into(),
                        1_i64.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("publication_media_sources"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("publication_id"),
                        Alias::new("media_source_id"),
                        Alias::new("catalog_item_id"),
                        Alias::new("presentation_key"),
                        Alias::new("container"),
                        Alias::new("row_sha256"),
                    ])
                    .values_panic([
                        Uuid::new_v4().into(),
                        publication.into(),
                        source.into(),
                        item.as_uuid().into(),
                        presentation.into(),
                        "mkv".into(),
                        "b".repeat(64).into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
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
                        location.into(),
                        source.into(),
                        object.into(),
                        1.into(),
                        "Available".into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("publication_media_locations"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("publication_id"),
                        Alias::new("media_location_id"),
                        Alias::new("media_source_id"),
                        Alias::new("storage_object_id"),
                        Alias::new("priority"),
                        Alias::new("row_sha256"),
                    ])
                    .values_panic([
                        Uuid::new_v4().into(),
                        publication.into(),
                        location.into(),
                        source.into(),
                        object.into(),
                        1.into(),
                        "c".repeat(64).into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("catalog_items"))
                    .value(Alias::new("active_source_publication_id"), publication)
                    .and_where(Expr::col(Alias::new("id")).eq(item.as_uuid())),
            ),
        )
        .await
        .unwrap();
    item
}

#[derive(Default)]
struct MemoryCache(Mutex<HashMap<String, Vec<u8>>>);

#[async_trait]
impl CacheStore for MemoryCache {
    async fn get(&self, key: &str) -> Option<Vec<u8>> {
        self.0.lock().unwrap().get(key).cloned()
    }

    async fn put(&self, key: &str, value: &[u8], _ttl: Duration) {
        self.0
            .lock()
            .unwrap()
            .insert(key.to_owned(), value.to_vec());
    }

    async fn delete(&self, key: &str) {
        self.0.lock().unwrap().remove(key);
    }
}

#[derive(Default)]
struct TtlRecordingCache(Mutex<Vec<Duration>>);

#[async_trait]
impl CacheStore for TtlRecordingCache {
    async fn get(&self, _key: &str) -> Option<Vec<u8>> {
        None
    }

    async fn put(&self, _key: &str, _value: &[u8], ttl: Duration) {
        self.0.lock().unwrap().push(ttl);
    }

    async fn delete(&self, _key: &str) {}
}

struct BarrierCache {
    data: Mutex<HashMap<String, Vec<u8>>>,
    initial_gets: AtomicUsize,
    initial_get_barrier: Barrier,
    puts: AtomicUsize,
}

impl BarrierCache {
    fn new() -> Self {
        Self {
            data: Mutex::new(HashMap::new()),
            initial_gets: AtomicUsize::new(0),
            initial_get_barrier: Barrier::new(2),
            puts: AtomicUsize::new(0),
        }
    }
}

#[async_trait]
impl CacheStore for BarrierCache {
    async fn get(&self, key: &str) -> Option<Vec<u8>> {
        if self.initial_gets.fetch_add(1, Ordering::SeqCst) < 2 {
            self.initial_get_barrier.wait().await;
        }
        self.data.lock().unwrap().get(key).cloned()
    }

    async fn put(&self, key: &str, value: &[u8], _ttl: Duration) {
        self.puts.fetch_add(1, Ordering::SeqCst);
        self.data
            .lock()
            .unwrap()
            .insert(key.to_owned(), value.to_vec());
    }

    async fn delete(&self, key: &str) {
        self.data.lock().unwrap().remove(key);
    }
}

#[tokio::test]
async fn requested_user_must_match_the_authenticated_principal() {
    let service = service().await;
    let principal = UserId::new();
    let other = UserId::new();

    let views = service.user_views(principal, Some(other)).await;
    let items = service
        .items(
            principal,
            Some(other),
            BrowseParent::Library(Uuid::new_v4()),
            CatalogPageRequest::new(0, 20).unwrap(),
        )
        .await;
    let latest = service
        .latest_items(principal, Some(other), None, Vec::new(), 20)
        .await;
    let next_up = service
        .next_up_items(
            principal,
            Some(other),
            None,
            false,
            CatalogPageRequest::new(0, 20).unwrap(),
        )
        .await;

    assert!(matches!(views, Err(CatalogServiceError::ForbiddenUser)));
    assert!(matches!(items, Err(CatalogServiceError::ForbiddenUser)));
    assert!(matches!(latest, Err(CatalogServiceError::ForbiddenUser)));
    assert!(matches!(next_up, Err(CatalogServiceError::ForbiddenUser)));
}

#[tokio::test]
async fn omitted_or_matching_user_reads_the_principals_catalog() {
    let service = service().await;
    let principal = UserId::new();

    let omitted = service.user_views(principal, None).await.unwrap();
    let matching = service
        .user_views(principal, Some(principal))
        .await
        .unwrap();

    assert_eq!(omitted, matching);
    assert_eq!(omitted.len(), 1);
    assert_eq!(omitted[0].name(), "Movies");
}

#[tokio::test]
async fn search_hints_cache_uses_a_distinct_digest_without_the_search_term() {
    let (service, _) = service_fixture().await;
    let cache = Arc::new(MemoryCache::default());
    let service = service.with_cache(
        cache.clone(),
        CacheKeyBuilder::new("tjxy").unwrap(),
        Duration::from_secs(300),
    );
    let principal = UserId::new();

    service
        .search_hints(
            principal,
            None,
            "private search term",
            CatalogPageRequest::new(0, 20).unwrap(),
        )
        .await
        .unwrap();
    service
        .search_hints(
            principal,
            None,
            "different search term",
            CatalogPageRequest::new(0, 20).unwrap(),
        )
        .await
        .unwrap();

    let keys = cache.0.lock().unwrap().keys().cloned().collect::<Vec<_>>();
    assert_eq!(keys.len(), 2);
    assert!(keys.iter().all(|key| key.contains(":search:")));
    assert!(keys.iter().all(|key| !key.contains("private search term")));
    assert!(
        keys.iter()
            .all(|key| !key.contains("different search term"))
    );
}

#[tokio::test]
async fn user_views_cache_is_isolated_by_catalog_and_user_sql_revisions() {
    let (service, database) = service_fixture().await;
    let cache = Arc::new(MemoryCache::default());
    let service = service.with_cache(
        cache.clone(),
        CacheKeyBuilder::new("tjxy").unwrap(),
        Duration::from_secs(300),
    );
    let principal = UserId::new();

    assert_eq!(service.user_views(principal, None).await.unwrap().len(), 1);
    database
        .execute(
            database.get_database_backend().build(
                Query::update()
                    .table(Alias::new("libraries"))
                    .value(Alias::new("is_enabled"), false),
            ),
        )
        .await
        .unwrap();
    assert_eq!(service.user_views(principal, None).await.unwrap().len(), 1);
    increment_catalog_generation(&database).await;
    assert!(
        service
            .user_views(principal, None)
            .await
            .unwrap()
            .is_empty()
    );

    let keys = cache.0.lock().unwrap().keys().cloned().collect::<Vec<_>>();
    assert!(keys.iter().any(|key| key.contains(":g:0:")));
    assert!(keys.iter().any(|key| key.contains(":g:1:")));
    assert!(
        keys.iter()
            .all(|key| key.contains(":r:0:user-views:") && key.len() >= 64)
    );
}

#[tokio::test]
async fn concurrent_user_view_misses_share_one_bounded_cache_fill() {
    let (service, _) = service_fixture().await;
    let cache = Arc::new(BarrierCache::new());
    let service = service.with_cache(
        cache.clone(),
        CacheKeyBuilder::new("tjxy").unwrap(),
        Duration::from_secs(300),
    );
    let principal = UserId::new();

    let (first, second) = tokio::join!(
        service.user_views(principal, None),
        service.user_views(principal, None)
    );

    assert_eq!(first.unwrap(), second.unwrap());
    assert_eq!(cache.puts.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn playback_sources_cache_by_catalog_user_and_probe_revisions() {
    let (service, database) = service_fixture().await;
    let item = seed_playback_cache_fixture(&database).await;
    let cache = Arc::new(MemoryCache::default());
    let service = service.with_cache(
        cache.clone(),
        CacheKeyBuilder::new("tjxy").unwrap(),
        Duration::from_secs(300),
    );
    let principal = UserId::new();

    let first = service
        .playback_sources(principal, None, item)
        .await
        .unwrap();
    assert_eq!(first.as_ref().map(Vec::len), Some(1));
    let first_keys = cache.0.lock().unwrap().keys().cloned().collect::<Vec<_>>();
    assert_eq!(first_keys.len(), 1);
    assert!(first_keys[0].contains(&format!(":playback:{item}:p:")));

    let second = service
        .playback_sources(principal, None, item)
        .await
        .unwrap();
    assert_eq!(second, first);
    assert_eq!(cache.0.lock().unwrap().len(), 1);

    database
        .execute(
            database.get_database_backend().build(
                Query::update()
                    .table(Alias::new("media_sources"))
                    .value(Alias::new("probe_revision"), 2_i64)
                    .and_where(Expr::col(Alias::new("catalog_item_id")).eq(item.as_uuid())),
            ),
        )
        .await
        .unwrap();

    let third = service
        .playback_sources(principal, None, item)
        .await
        .unwrap();
    assert_eq!(third, first);
    assert_eq!(cache.0.lock().unwrap().len(), 2);
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Keeps the generation transition in one explicit fixture.
async fn item_pages_cache_by_parent_page_and_catalog_revision() {
    let (service, database) = service_fixture().await;
    let backend = database.get_database_backend();
    let library = first_library_id(&database).await;
    let item = CatalogItemId::new();
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
                        item.as_uuid().into(),
                        "Movie".into(),
                        "Arrival".into(),
                        "arrival".into(),
                        SortKey::from_text("Arrival").into_bytes().into(),
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
    database
        .execute(
            backend.build(
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
    let cache = Arc::new(MemoryCache::default());
    let service = service.with_cache(
        cache,
        CacheKeyBuilder::new("tjxy").unwrap(),
        Duration::from_secs(300),
    );
    let principal = UserId::new();
    let page = CatalogPageRequest::new(0, 20).unwrap();

    assert_eq!(
        service
            .items(
                principal,
                None,
                BrowseParent::Library(library),
                page.clone()
            )
            .await
            .unwrap()
            .items()
            .len(),
        1
    );
    assert_eq!(
        service
            .item(principal, None, item)
            .await
            .unwrap()
            .unwrap()
            .name(),
        "Arrival"
    );
    database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("catalog_items"))
                    .value(Alias::new("is_present"), false)
                    .value(Alias::new("name"), "Blade Runner")
                    .and_where(Expr::col(Alias::new("id")).eq(item.as_uuid())),
            ),
        )
        .await
        .unwrap();
    assert_eq!(
        service
            .items(
                principal,
                None,
                BrowseParent::Library(library),
                page.clone()
            )
            .await
            .unwrap()
            .items()
            .len(),
        1
    );
    assert_eq!(
        service
            .item(principal, None, item)
            .await
            .unwrap()
            .unwrap()
            .name(),
        "Arrival"
    );
    increment_catalog_generation(&database).await;
    assert!(
        service
            .items(principal, None, BrowseParent::Library(library), page)
            .await
            .unwrap()
            .items()
            .is_empty()
    );
    assert!(service.item(principal, None, item).await.unwrap().is_none());
}

#[tokio::test]
async fn home_warmup_caches_default_rows_only_for_requested_users() {
    let (service, _) = service_fixture().await;
    let cache = Arc::new(MemoryCache::default());
    let service = service.with_cache(
        cache.clone(),
        CacheKeyBuilder::new("tjxy").unwrap(),
        Duration::from_secs(300),
    );
    let first = UserId::new();
    let skipped = UserId::new();
    let second = UserId::new();

    service.warm_home(&[first, second]).await.unwrap();

    let keys = cache.0.lock().unwrap().keys().cloned().collect::<Vec<_>>();
    assert_eq!(keys.len(), 10);
    assert_eq!(
        keys.iter()
            .filter(|key| key.contains(":user-views:"))
            .count(),
        2
    );
    assert_eq!(
        keys.iter().filter(|key| key.contains(":latest:")).count(),
        4
    );
    assert_eq!(
        keys.iter().filter(|key| key.contains(":resume:")).count(),
        2
    );
    assert_eq!(
        keys.iter().filter(|key| key.contains(":next-up:")).count(),
        2
    );
    assert!(keys.iter().any(|key| key.contains(&first.to_string())));
    assert!(keys.iter().any(|key| key.contains(&second.to_string())));
    assert!(!keys.iter().any(|key| key.contains(&skipped.to_string())));
}

#[tokio::test]
async fn home_warmup_bounds_per_library_latest_rows() {
    let (service, database) = service_fixture().await;
    let backend = database.get_database_backend();
    for index in 0..65 {
        let name = format!("Library {index:02}");
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
                            Uuid::new_v4().into(),
                            name.clone().into(),
                            "Lazy".into(),
                            "title_layer".into(),
                            "basic".into(),
                            "on_browse".into(),
                            "on_playback".into(),
                            1.into(),
                            "movies".into(),
                            SortKey::from_text(&name).into_bytes().into(),
                            true.into(),
                        ]),
                ),
            )
            .await
            .unwrap();
    }
    let cache = Arc::new(MemoryCache::default());
    let service = service.with_cache(
        cache.clone(),
        CacheKeyBuilder::new("tjxy").unwrap(),
        Duration::from_secs(300),
    );

    service.warm_home(&[UserId::new()]).await.unwrap();

    let keys = cache.0.lock().unwrap().keys().cloned().collect::<Vec<_>>();
    assert_eq!(keys.len(), 68);
    assert_eq!(
        keys.iter().filter(|key| key.contains(":latest:")).count(),
        65
    );
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Keeps the cache revision fixture explicit and auditable.
async fn resume_pages_cache_by_page_and_user_revision() {
    let (service, database) = service_fixture().await;
    let backend = database.get_database_backend();
    let principal = AuthRepository::new(&database)
        .create_user(
            &Username::parse("alice").unwrap(),
            "$argon2id$test-only",
            true,
            false,
            Utc::now(),
        )
        .await
        .unwrap()
        .id();
    let library = first_library_id(&database).await;
    let item = CatalogItemId::new();
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
                        item.as_uuid().into(),
                        "Movie".into(),
                        "Arrival".into(),
                        "arrival".into(),
                        SortKey::from_text("Arrival").into_bytes().into(),
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
    database
        .execute(
            backend.build(
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
    UserDataRepository::new(&database)
        .commit(
            principal,
            item,
            UserDataPatch::default().with_playback_position_ticks(100),
        )
        .await
        .unwrap();
    let service = service.with_cache(
        Arc::new(MemoryCache::default()),
        CacheKeyBuilder::new("tjxy").unwrap(),
        Duration::from_secs(300),
    );
    let page = CatalogPageRequest::new(0, 20).unwrap();

    assert_eq!(
        service
            .resume_items(principal, None, page.clone())
            .await
            .unwrap()
            .items()
            .len(),
        1
    );
    database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("user_data"))
                    .value(Alias::new("playback_position_ticks"), 0_i64)
                    .and_where(Expr::col(Alias::new("user_id")).eq(principal.as_uuid())),
            ),
        )
        .await
        .unwrap();
    assert_eq!(
        service
            .resume_items(principal, None, page.clone())
            .await
            .unwrap()
            .items()
            .len(),
        1
    );
    database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("user_catalog_state"))
                    .value(
                        Alias::new("revision"),
                        Expr::col(Alias::new("revision")).add(1),
                    )
                    .and_where(Expr::col(Alias::new("user_id")).eq(principal.as_uuid())),
            ),
        )
        .await
        .unwrap();

    assert!(
        service
            .resume_items(principal, None, page)
            .await
            .unwrap()
            .items()
            .is_empty()
    );
}

#[tokio::test]
async fn unknown_parent_is_distinct_from_a_known_empty_parent() {
    let service = service().await;
    let principal = UserId::new();

    let page = service
        .items_by_parent_id(
            principal,
            None,
            Uuid::new_v4(),
            CatalogPageRequest::new(0, 20).unwrap(),
        )
        .await
        .unwrap();

    assert!(page.is_none());
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Keeps both TTL cases in one explicit catalog fixture.
async fn empty_item_pages_use_the_short_ttl_without_shortening_nonempty_pages() {
    let (service, database) = service_fixture().await;
    let library = first_library_id(&database).await;
    let cache = Arc::new(TtlRecordingCache::default());
    let service = service.with_cache_ttls(
        cache.clone(),
        CacheKeyBuilder::new("tjxy").unwrap(),
        Duration::from_secs(300),
        Duration::from_secs(1_800),
        Duration::from_secs(3),
    );
    let principal = UserId::new();
    let page = CatalogPageRequest::new(0, 20).unwrap();

    assert!(
        service
            .items(
                principal,
                None,
                BrowseParent::Library(library),
                page.clone(),
            )
            .await
            .unwrap()
            .items()
            .is_empty()
    );

    let backend = database.get_database_backend();
    let item = CatalogItemId::new();
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
                        item.as_uuid().into(),
                        "Movie".into(),
                        "Arrival".into(),
                        "arrival".into(),
                        SortKey::from_text("Arrival").into_bytes().into(),
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
    database
        .execute(
            backend.build(
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
    increment_catalog_generation(&database).await;

    assert_eq!(
        service
            .items(principal, None, BrowseParent::Library(library), page)
            .await
            .unwrap()
            .items()
            .len(),
        1
    );
    assert_eq!(
        *cache.0.lock().unwrap(),
        vec![Duration::from_secs(3), Duration::from_secs(300)]
    );
}

#[allow(clippy::too_many_lines)] // Keeps the visible lazy item fixture in one auditable setup.
async fn lazy_service(
    item_type: &str,
    visible: bool,
) -> (
    CatalogQueryService,
    DatabaseConnection,
    CatalogItemId,
    UserId,
) {
    let database = test_database().await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    let backend = database.get_database_backend();
    let library = Uuid::new_v4();
    let item = CatalogItemId::new();
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
                        library.into(),
                        "Lazy".into(),
                        "Lazy".into(),
                        "title_layer".into(),
                        "basic".into(),
                        "on_browse".into(),
                        "on_playback".into(),
                        1.into(),
                        "movies".into(),
                        SortKey::from_text("Lazy").into_bytes().into(),
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
                        item_type.into(),
                        "Lazy item".into(),
                        "lazy item".into(),
                        "Matched".into(),
                        "Ready".into(),
                        "Unexpanded".into(),
                        "Unknown".into(),
                        7_i64.into(),
                        9_i64.into(),
                        true.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    if visible {
        database
            .execute(
                backend.build(
                    Query::insert()
                        .into_table(Alias::new("library_catalog_items"))
                        .columns([
                            Alias::new("id"),
                            Alias::new("library_id"),
                            Alias::new("catalog_item_id"),
                        ])
                        .values_panic([
                            Uuid::new_v4().into(),
                            library.into(),
                            item.as_uuid().into(),
                        ]),
                ),
            )
            .await
            .unwrap();
        seed_ready_lazy_scope(&database, library, item).await;
    }
    (
        CatalogQueryService::new(database.clone()),
        database,
        item,
        UserId::new(),
    )
}

#[allow(clippy::too_many_lines)] // Builds the complete reconciled title scope used by the coordinator.
async fn seed_ready_lazy_scope(database: &DatabaseConnection, library: Uuid, item: CatalogItemId) {
    let backend = database.get_database_backend();
    let account = Uuid::new_v4();
    let object = Uuid::new_v4();
    let root = Uuid::new_v4();
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
                        account.into(),
                        "filesystem".into(),
                        "Fixture".into(),
                        format!("account-{account}").into(),
                        "fixture".into(),
                        "Ready".into(),
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
                        object.into(),
                        account.into(),
                        "drive".into(),
                        format!("object-{object}").into(),
                        "Lazy item".into(),
                        "lazy item".into(),
                        "Directory".into(),
                        4_i64.into(),
                        true.into(),
                        4_i64.into(),
                        "ProviderStable".into(),
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
                        root.into(),
                        account.into(),
                        format!("root-{root}").into(),
                        4_i64.into(),
                        4_i64.into(),
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
                    .values_panic([Uuid::new_v4().into(), library.into(), root.into()]),
            ),
        )
        .await
        .unwrap();
    database
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
                        object.into(),
                        item.as_uuid().into(),
                        1.0_f64.into(),
                        "Matched".into(),
                        serde_json::json!({"fixture": true}).into(),
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
                        root.into(),
                        object.into(),
                        4_i64.into(),
                        true.into(),
                        4_i64.into(),
                        "Present".into(),
                    ]),
            ),
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn lazy_requests_join_one_durable_job_and_hidden_items_schedule_nothing() {
    let (movie_service, movie_db, movie, user) = lazy_service("Movie", true).await;
    movie_service.item(user, None, movie).await.unwrap();
    movie_service.item(user, None, movie).await.unwrap();
    let (series_service, series_db, series, user) = lazy_service("Series", true).await;
    let page = CatalogPageRequest::new(0, 20).unwrap();
    series_service
        .items_by_parent_id(user, None, series.as_uuid(), page.clone())
        .await
        .unwrap();
    series_service
        .items_by_parent_id(user, None, series.as_uuid(), page)
        .await
        .unwrap();
    let (sync_service, sync_db, sync_item, user) = lazy_service("Movie", true).await;
    let backend = sync_db.get_database_backend();
    sync_db
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("storage_root_objects"))
                    .value(Alias::new("children_indexed"), false),
            ),
        )
        .await
        .unwrap();
    sync_service.item(user, None, sync_item).await.unwrap();
    let (hidden_service, hidden_db, hidden, user) = lazy_service("Movie", false).await;
    assert!(
        hidden_service
            .item(user, None, hidden)
            .await
            .unwrap()
            .is_none()
    );

    for (database, kind, revision) in [
        (&movie_db, "IndexMediaSources", 9_i64),
        (&series_db, "ExpandItem", 7_i64),
    ] {
        let rows = database
            .query_all(
                database.get_database_backend().build(
                    Query::select()
                        .columns([Alias::new("task_kind"), Alias::new("expected_revision")])
                        .from(Alias::new("work_jobs")),
                ),
            )
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(row.try_get::<String>("", "task_kind").unwrap(), kind);
        assert_eq!(
            row.try_get::<i64>("", "expected_revision").unwrap(),
            revision
        );
    }
    let sync_jobs = sync_db
        .query_all(
            sync_db.get_database_backend().build(
                Query::select()
                    .column(Alias::new("task_kind"))
                    .from(Alias::new("work_jobs")),
            ),
        )
        .await
        .unwrap();
    assert_eq!(sync_jobs.len(), 1);
    assert_eq!(
        sync_jobs[0].try_get::<String>("", "task_kind").unwrap(),
        "ScopedStorageSync"
    );
    let hidden_jobs = hidden_db
        .query_all(
            hidden_db.get_database_backend().build(
                Query::select()
                    .column(Alias::new("id"))
                    .from(Alias::new("work_jobs")),
            ),
        )
        .await
        .unwrap();
    assert!(hidden_jobs.is_empty());
}
