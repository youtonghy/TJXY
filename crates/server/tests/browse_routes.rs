use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{Arc, Mutex},
};

use axum::{
    body::Body,
    http::{HeaderMap, Request, StatusCode, header},
};
use bytes::Bytes;
use chrono::{Duration, Utc};
use futures_util::StreamExt;
use http_body_util::BodyExt;
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DbBackend, Statement,
    sea_query::{Alias, Expr, Query},
};
use sea_orm_migration::MigratorTrait;
use serde_json::{Value, json};
use tempfile::TempDir;
use tjxy_application::{
    AssetReadService, AuthService, CatalogQueryService, FilesystemBrowser, LibraryService,
    MediaCollectionService, MediaInspector, MediaReadService, PlaybackTicketService,
    PlaystateService, ProbeInput, ProbeService, ProbeServiceError, SourceIndexService, SystemClock,
    TaskService, UserDataService,
};
use tjxy_common::{CatalogItemId, SortKey};
use tjxy_db::{SystemSettingsInput, SystemSettingsRepository};
use tjxy_server::{AppState, ServerIdentity, build_router};
use tjxy_storage::{
    BackendError, ByteRange, ByteStream, ChangeCursor, ChangePage, ObjectPage, PageToken,
    StorageBackend, StorageCapabilities, StorageObject, StorageObjectId,
};
use tjxy_storage_filesystem::FilesystemBackend;
use tjxy_test_support::test_database;
use tower::ServiceExt;
use uuid::Uuid;

const SERVER_ID: &str = "018f17ac-4e99-7ec5-b4fd-8f15ca9f4f11";
const IDENTITY: &str =
    r#"MediaBrowser Client="Findroid", Device="Pixel", DeviceId="phone-1", Version="0.16.0""#;
const CLOUD_DEFAULT_BYTES: &[u8] = b"cloud-byte-stream";
const CLOUD_ALTERNATE_BYTES: &[u8] = b"other-byte-stream";
const CLOUD_SUBTITLE_BYTES: &[u8] = b"1\n00:00:01,000 --> 00:00:02,000\nCloud\n\n\n";
const CLOUD_PROVIDER: &str = "cloud-test";
const CLOUD_DRIVE: &str = "drive-secret-marker";
const CLOUD_ROOT_OBJECT_ID: &str = "cloud-root-secret";
const CLOUD_DIRECTORY_OBJECT_ID: &str = "remote-directory-secret";
const CLOUD_ACCOUNT_IDENTITY: &str =
    "https://upstream.invalid/secret?account=account-secret-marker";
const CLOUD_DISPLAY_NAME: &str = "Cloud Secret Display";
const CLOUD_CREDENTIAL_REF: &str = "credential-secret-marker:upstream-token-secret";
const CLOUD_PLAYBACK_REQUEST: &str =
    include_str!("golden/playback/cloud-multi-source-playback-info.request.json");
const CLOUD_PLAYBACK_RESPONSE: &str =
    include_str!("golden/playback/cloud-multi-source-playback-info.response.json");

struct TestApp {
    router: axum::Router,
    database: DatabaseConnection,
    assets: TempDir,
    media: TempDir,
    media_account: Uuid,
    media_object_id: String,
    empty_media_object_id: String,
    subtitle_object_id: String,
    cloud_account: Uuid,
    cloud_object_id: String,
    cloud_alternate_object_id: String,
    cloud_subtitle_object_id: String,
    cloud_backend: Arc<MemoryCloudBackend>,
}

struct TcpTestServer {
    base_url: String,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    task: Option<tokio::task::JoinHandle<Result<(), std::io::Error>>>,
}

impl TcpTestServer {
    async fn start(router: axum::Router) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind cloud playback test server");
        let address = listener.local_addr().expect("read test server address");
        let (shutdown, receiver) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(async move {
                    let _ = receiver.await;
                })
                .await
        });
        Self {
            base_url: format!("http://{address}"),
            shutdown: Some(shutdown),
            task: Some(task),
        }
    }

    async fn stop(mut self) {
        self.shutdown
            .take()
            .expect("shutdown sender")
            .send(())
            .expect("test server receives shutdown");
        let task = self.task.take().expect("test server task");
        tokio::time::timeout(std::time::Duration::from_secs(2), task)
            .await
            .expect("test server stops before timeout")
            .expect("test server task joins")
            .expect("test server exits cleanly");
    }
}

impl Drop for TcpTestServer {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
    }
}

struct MemoryCloudBackend {
    objects: HashMap<String, Vec<u8>>,
    reads: Mutex<VecDeque<CloudReadBehavior>>,
    ranges: Mutex<Vec<(String, u64, u64)>>,
}

enum CloudReadBehavior {
    OpenNotFound,
    OpenUnavailable,
    StreamUnavailable,
    StreamPendingAfterChunk,
}

impl MemoryCloudBackend {
    fn enqueue_read(&self, behavior: CloudReadBehavior) {
        self.reads.lock().unwrap().push_back(behavior);
    }

    fn ranges(&self) -> Vec<(String, u64, u64)> {
        self.ranges.lock().unwrap().clone()
    }

    fn take_ranges(&self) -> Vec<(String, u64, u64)> {
        self.ranges.lock().unwrap().drain(..).collect()
    }
}

#[async_trait::async_trait]
impl StorageBackend for MemoryCloudBackend {
    async fn get_object(&self, id: &StorageObjectId) -> Result<StorageObject, BackendError> {
        let bytes = self
            .objects
            .get(id.provider_object_id())
            .filter(|_| id.provider() == "cloud-test")
            .ok_or(BackendError::NotFound)?;
        Ok(StorageObject::file(
            id.clone(),
            "remote-object",
            bytes.len() as u64,
        ))
    }

    async fn list_children(
        &self,
        _parent: &StorageObjectId,
        _page: Option<PageToken>,
    ) -> Result<ObjectPage, BackendError> {
        Err(BackendError::unsupported_capability("list children"))
    }

    async fn list_changes(&self, _cursor: ChangeCursor) -> Result<ChangePage, BackendError> {
        Err(BackendError::unsupported_capability("changes"))
    }

    async fn open_range(
        &self,
        id: &StorageObjectId,
        range: ByteRange,
    ) -> Result<ByteStream, BackendError> {
        self.ranges.lock().unwrap().push((
            id.provider_object_id().to_owned(),
            range.start(),
            range.end_exclusive(),
        ));
        let behavior = self.reads.lock().unwrap().pop_front();
        if matches!(behavior, Some(CloudReadBehavior::OpenNotFound)) {
            return Err(BackendError::NotFound);
        }
        if matches!(behavior, Some(CloudReadBehavior::OpenUnavailable)) {
            return Err(BackendError::TemporarilyUnavailable {
                message: "fixture detail must not be persisted".to_owned(),
            });
        }
        let bytes = self
            .objects
            .get(id.provider_object_id())
            .filter(|_| id.provider() == "cloud-test")
            .ok_or(BackendError::NotFound)?;
        let start =
            usize::try_from(range.start()).map_err(|_| BackendError::RangeNotSatisfiable {
                size: bytes.len() as u64,
            })?;
        let end = usize::try_from(range.end_exclusive()).map_err(|_| {
            BackendError::RangeNotSatisfiable {
                size: bytes.len() as u64,
            }
        })?;
        let chunk = Bytes::copy_from_slice(bytes.get(start..end).ok_or(
            BackendError::RangeNotSatisfiable {
                size: bytes.len() as u64,
            },
        )?);
        match behavior {
            Some(CloudReadBehavior::StreamUnavailable) => Ok(Box::pin(async_stream::try_stream! {
                Err(BackendError::RateLimited { retry_after: None })?;
                yield chunk;
            })),
            Some(CloudReadBehavior::StreamPendingAfterChunk) => {
                Ok(Box::pin(async_stream::try_stream! {
                    yield chunk;
                    std::future::pending::<()>().await;
                }))
            }
            Some(CloudReadBehavior::OpenNotFound | CloudReadBehavior::OpenUnavailable) | None => {
                Ok(Box::pin(async_stream::try_stream! { yield chunk; }))
            }
        }
    }

    fn capabilities(&self) -> StorageCapabilities {
        StorageCapabilities::new().with_range_reads(true)
    }
}

async fn filesystem_fixture() -> (TempDir, Arc<FilesystemBackend>, String, String, String) {
    let media = TempDir::new().unwrap();
    tokio::fs::write(media.path().join("Arrival.mkv"), b"0123456789")
        .await
        .unwrap();
    tokio::fs::write(media.path().join("Empty.mkv"), b"")
        .await
        .unwrap();
    tokio::fs::write(
        media.path().join("Arrival.srt"),
        b"1\n00:00:01,000 --> 00:00:02,000\nArrival\n",
    )
    .await
    .unwrap();
    let backend = Arc::new(FilesystemBackend::new(media.path()).await.unwrap());
    let page = backend
        .list_children(backend.root_id(), None)
        .await
        .unwrap();
    let object_id = |name: &str| {
        page.objects
            .iter()
            .find(|object| object.name() == name)
            .unwrap()
            .id()
            .provider_object_id()
            .to_owned()
    };
    (
        media,
        backend,
        object_id("Arrival.mkv"),
        object_id("Empty.mkv"),
        object_id("Arrival.srt"),
    )
}

fn cloud_fixture() -> (Uuid, String, String, String, Arc<MemoryCloudBackend>) {
    let account = Uuid::new_v4();
    let object = "remote-object-secret".to_owned();
    let alternate = "remote-alternate-secret".to_owned();
    let subtitle = "remote-subtitle-secret".to_owned();
    let backend = Arc::new(MemoryCloudBackend {
        objects: HashMap::from([
            (object.clone(), CLOUD_DEFAULT_BYTES.to_vec()),
            (alternate.clone(), CLOUD_ALTERNATE_BYTES.to_vec()),
            (subtitle.clone(), CLOUD_SUBTITLE_BYTES.to_vec()),
        ]),
        reads: Mutex::new(VecDeque::new()),
        ranges: Mutex::new(Vec::new()),
    });
    (account, object, alternate, subtitle, backend)
}

struct CloudProbeInspector;

impl MediaInspector for CloudProbeInspector {
    fn inspect(&self, input: ProbeInput) -> Result<tjxy_db::ProbeResult, ProbeServiceError> {
        if input.size() != 17 {
            return Err(ProbeServiceError::Inspection(
                "unexpected cloud probe size".to_owned(),
            ));
        }
        let stream = tjxy_db::ProbedStream::new(
            "cloud-video",
            "Video",
            0,
            Some("h264".to_owned()),
            None,
            Some(1920),
            Some(1080),
            None,
            true,
            false,
        )
        .map_err(|_| ProbeServiceError::Inspection("invalid cloud probe stream".to_owned()))?;
        tjxy_db::ProbeResult::new("mkv", vec![stream])
            .map_err(|_| ProbeServiceError::Inspection("invalid cloud probe result".to_owned()))
    }
}

async fn test_app() -> TestApp {
    test_app_with_user(true).await
}

async fn test_app_with_user(create_user: bool) -> TestApp {
    let database = test_database().await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    let auth = Arc::new(
        AuthService::new(database.clone(), SystemClock, Some(Duration::days(30)), 2)
            .await
            .unwrap(),
    );
    if create_user {
        auth.create_user("Alice", "correct horse", true)
            .await
            .unwrap();
    }
    let catalog = Arc::new(CatalogQueryService::new(database.clone()));
    let libraries = Arc::new(LibraryService::new(database.clone()));
    let assets = TempDir::new().unwrap();
    let asset_reader = Arc::new(
        AssetReadService::new(database.clone(), assets.path())
            .await
            .unwrap(),
    );
    let identity = ServerIdentity::new(Uuid::parse_str(SERVER_ID).unwrap(), "TJXY", "Linux")
        .with_startup_wizard_completed(true);
    let (media, backend, media_object_id, empty_media_object_id, subtitle_object_id) =
        filesystem_fixture().await;
    tokio::fs::create_dir(media.path().join("Movies"))
        .await
        .unwrap();
    let filesystem_browser = Arc::new(FilesystemBrowser::from_roots([media.path()]).await.unwrap());
    let media_account = Uuid::new_v4();
    let (
        cloud_account,
        cloud_object_id,
        cloud_alternate_object_id,
        cloud_subtitle_object_id,
        cloud_backend,
    ) = cloud_fixture();
    let media_reader = Arc::new(
        MediaReadService::new(database.clone())
            .with_backend(media_account, backend)
            .with_backend(cloud_account, Arc::clone(&cloud_backend)),
    );
    let media_collections = Arc::new(MediaCollectionService::new(database.clone()));
    let playback_tickets = Arc::new(PlaybackTicketService::new(database.clone(), SystemClock));
    let user_data = Arc::new(UserDataService::new(database.clone()));
    let playstate = Arc::new(PlaystateService::new(database.clone()));
    let tasks = Arc::new(TaskService::new(database.clone()));
    TestApp {
        router: build_router(
            AppState::new(identity)
                .with_auth(auth)
                .with_catalog(catalog)
                .with_libraries(libraries)
                .with_filesystem_browser(filesystem_browser)
                .with_assets(asset_reader)
                .with_media(media_reader)
                .with_playback_tickets(playback_tickets)
                .with_media_collections(media_collections)
                .with_playstate(playstate)
                .with_tasks(tasks)
                .with_user_data(user_data)
                .with_dashboard(database.clone())
                .with_client_portal(database.clone())
                .with_system_settings(database.clone())
                .with_ready(true),
        ),
        database,
        assets,
        media,
        media_account,
        media_object_id,
        empty_media_object_id,
        subtitle_object_id,
        cloud_account,
        cloud_object_id,
        cloud_alternate_object_id,
        cloud_subtitle_object_id,
        cloud_backend,
    }
}

#[tokio::test]
async fn filesystem_browser_requires_an_administrator_and_exposes_only_relative_paths() {
    let app = test_app().await;
    let response = get(&app.router, "/Admin/Filesystem/Roots", None).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let (_, _, token) = login(&app.router).await;
    let response = get(&app.router, "/Admin/Filesystem/Roots", Some(&token)).await;
    assert_eq!(response.status(), StatusCode::OK);
    let roots: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(roots.as_array().unwrap().len(), 1);
    assert_eq!(
        roots[0]["Name"],
        app.media.path().file_name().unwrap().to_str().unwrap()
    );
    assert!(roots[0].get("Path").is_none());
    let root_id = roots[0]["Id"].as_str().unwrap();

    let response = get(
        &app.router,
        &format!("/Admin/Filesystem/Directories?RootId={root_id}&Path="),
        Some(&token),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let page: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(page["Items"][0]["Name"], "Movies");
    assert_eq!(page["Items"][0]["RelativePath"], "Movies");
    assert!(
        !page
            .to_string()
            .contains(app.media.path().to_str().unwrap())
    );
}

#[tokio::test]
async fn administrator_can_attach_a_validated_browser_selection_to_an_existing_library() {
    let app = test_app().await;
    let (_, _, token) = login(&app.router).await;
    assert_eq!(
        post(
            &app.router,
            "/Library/VirtualFolders?name=Movies&collectionType=movies",
            &token,
            "{}",
        )
        .await
        .status(),
        StatusCode::NO_CONTENT
    );
    let response = get(&app.router, "/Library/VirtualFolders", Some(&token)).await;
    let folders: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let library_id = folders[0]["ItemId"].as_str().unwrap();
    let response = get(&app.router, "/Admin/Filesystem/Roots", Some(&token)).await;
    let roots: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let root_id = roots[0]["Id"].as_str().unwrap();

    let response = post(
        &app.router,
        "/Library/VirtualFolders/Paths",
        &token,
        json!({
            "LibraryId": library_id,
            "FilesystemSelection": {"RootId": root_id, "RelativePath": "Movies"}
        })
        .to_string(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let response = get(&app.router, "/Library/VirtualFolders", Some(&token)).await;
    let folders: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(folders[0]["Locations"].as_array().unwrap().len(), 1);
    assert!(!folders[0]["Locations"][0].as_str().unwrap().is_empty());
}

async fn login(router: &axum::Router) -> (Uuid, Uuid, String) {
    login_as(router, "alice", "correct horse").await
}

async fn login_as(router: &axum::Router, username: &str, password: &str) -> (Uuid, Uuid, String) {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/Users/AuthenticateByName")
                .header(header::AUTHORIZATION, IDENTITY)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"Username": username, "Pw": password}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let authentication: Value = serde_json::from_slice(&body).unwrap();
    (
        Uuid::parse_str(authentication["User"]["Id"].as_str().unwrap()).unwrap(),
        Uuid::parse_str(authentication["SessionInfo"]["Id"].as_str().unwrap()).unwrap(),
        authentication["AccessToken"].as_str().unwrap().to_owned(),
    )
}

async fn seed_library(database: &DatabaseConnection, name: &str, enabled: bool) -> Uuid {
    let id = Uuid::new_v4();
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
                        id.into(),
                        name.into(),
                        "Lazy".into(),
                        "title_layer".into(),
                        "basic".into(),
                        "on_browse".into(),
                        "on_playback".into(),
                        1.into(),
                        "movies".into(),
                        SortKey::from_text(name).into_bytes().into(),
                        enabled.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    id
}

async fn seed_item(
    database: &DatabaseConnection,
    library_id: Uuid,
    name: &str,
    item_type: &str,
) -> CatalogItemId {
    let id = CatalogItemId::new();
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
                        Alias::new("sort_key"),
                        Alias::new("classification_state"),
                        Alias::new("metadata_state"),
                        Alias::new("metadata_payload_version"),
                        Alias::new("structure_state"),
                        Alias::new("source_state"),
                        Alias::new("structure_expansion_revision"),
                        Alias::new("source_index_revision"),
                        Alias::new("is_present"),
                    ])
                    .values_panic([
                        id.as_uuid().into(),
                        item_type.into(),
                        name.into(),
                        name.to_lowercase().into(),
                        SortKey::from_text(name).into_bytes().into(),
                        "Matched".into(),
                        "Ready".into(),
                        1_i32.into(),
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
                    .values_panic([
                        Uuid::new_v4().into(),
                        library_id.into(),
                        id.as_uuid().into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    id
}

async fn add_shared_genre(database: &DatabaseConnection, name: &str, item_ids: &[CatalogItemId]) {
    let genre_id = Uuid::new_v4();
    let backend = database.get_database_backend();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("genres"))
                    .columns([Alias::new("id"), Alias::new("name")])
                    .values_panic([genre_id.into(), name.into()]),
            ),
        )
        .await
        .unwrap();
    for item_id in item_ids {
        database
            .execute(
                backend.build(
                    Query::insert()
                        .into_table(Alias::new("item_genres"))
                        .columns([
                            Alias::new("id"),
                            Alias::new("catalog_item_id"),
                            Alias::new("genre_id"),
                        ])
                        .values_panic([
                            Uuid::new_v4().into(),
                            item_id.as_uuid().into(),
                            genre_id.into(),
                        ]),
                ),
            )
            .await
            .unwrap();
    }
}

struct CloudMultiSourceFixture {
    item: CatalogItemId,
    root: Uuid,
    default_object: Uuid,
    alternate_object: Uuid,
    subtitle_object: Uuid,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CloudPresentations {
    default_source: tjxy_common::MediaSourceId,
    default: Uuid,
    alternate_source: tjxy_common::MediaSourceId,
    alternate: Uuid,
}

#[derive(Debug, Eq, PartialEq)]
struct CloudPlaybackSnapshot {
    source_order: Vec<Uuid>,
    direct_urls: Vec<String>,
    subtitle_index: i64,
    subtitle_url: String,
}

#[allow(clippy::too_many_lines)] // Mirrors one reconciled directory and its complete child inventory.
async fn seed_cloud_multi_source_inventory(app: &TestApp) -> CloudMultiSourceFixture {
    let library = seed_library(&app.database, "Cloud Movies", true).await;
    let item = seed_item(&app.database, library, "Remote Default", "Movie").await;
    let root = Uuid::new_v4();
    let parent = Uuid::new_v4();
    let default_object = Uuid::new_v4();
    let alternate_object = Uuid::new_v4();
    let subtitle_object = Uuid::new_v4();
    let backend = app.database.get_database_backend();
    app.database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("catalog_items"))
                    .value(Alias::new("source_state"), "NotIndexed")
                    .value(Alias::new("source_index_revision"), 1_i64)
                    .and_where(Expr::col(Alias::new("id")).eq(item.as_uuid())),
            ),
        )
        .await
        .unwrap();
    for statement in [
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
                app.cloud_account.into(),
                CLOUD_PROVIDER.into(),
                CLOUD_DISPLAY_NAME.into(),
                CLOUD_ACCOUNT_IDENTITY.into(),
                CLOUD_CREDENTIAL_REF.into(),
                "Active".into(),
            ])
            .to_owned(),
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
                app.cloud_account.into(),
                CLOUD_ROOT_OBJECT_ID.into(),
                1_i64.into(),
                1_i64.into(),
            ])
            .to_owned(),
        Query::insert()
            .into_table(Alias::new("library_storage_roots"))
            .columns([
                Alias::new("id"),
                Alias::new("library_id"),
                Alias::new("storage_root_id"),
            ])
            .values_panic([Uuid::new_v4().into(), library.into(), root.into()])
            .to_owned(),
    ] {
        app.database
            .execute(backend.build(&statement))
            .await
            .unwrap();
    }
    for (id, provider_object_id, name, object_type, size) in [
        (
            parent,
            CLOUD_DIRECTORY_OBJECT_ID,
            "Remote Default",
            "Directory",
            0_i64,
        ),
        (
            default_object,
            app.cloud_object_id.as_str(),
            "Remote Default.mkv",
            "File",
            i64::try_from(CLOUD_DEFAULT_BYTES.len()).unwrap(),
        ),
        (
            alternate_object,
            app.cloud_alternate_object_id.as_str(),
            "Remote Alternate.mkv",
            "File",
            i64::try_from(CLOUD_ALTERNATE_BYTES.len()).unwrap(),
        ),
        (
            subtitle_object,
            app.cloud_subtitle_object_id.as_str(),
            "Remote Default.eng.srt",
            "File",
            i64::try_from(CLOUD_SUBTITLE_BYTES.len()).unwrap(),
        ),
    ] {
        app.database
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
                            Alias::new("facts_observed_storage_root_id"),
                        ])
                        .values_panic([
                            id.into(),
                            app.cloud_account.into(),
                            CLOUD_DRIVE.into(),
                            provider_object_id.into(),
                            name.into(),
                            name.to_lowercase().into(),
                            object_type.into(),
                            size.into(),
                            1_i64.into(),
                            (id == parent).into(),
                            i64::from(id == parent).into(),
                            "ProviderStable".into(),
                            "Present".into(),
                            root.into(),
                        ]),
                ),
            )
            .await
            .unwrap();
        app.database
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
                            root.into(),
                            id.into(),
                            if id == parent { None } else { Some(parent) }.into(),
                            1_i64.into(),
                            (id == parent).into(),
                            i64::from(id == parent).into(),
                            "Present".into(),
                        ]),
                ),
            )
            .await
            .unwrap();
    }
    app.database
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
                        parent.into(),
                        item.as_uuid().into(),
                        1.0.into(),
                        "Matched".into(),
                        json!({}).into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    CloudMultiSourceFixture {
        item,
        root,
        default_object,
        alternate_object,
        subtitle_object,
    }
}

async fn index_cloud_sources(
    database: &DatabaseConnection,
    item: CatalogItemId,
    task_revision: i64,
    input_sync_revision: i64,
) -> i64 {
    let jobs = tjxy_db::WorkJobRepository::new(database);
    let submission = jobs
        .enqueue_or_join(
            &tjxy_db::WorkJobSpec::new(
                tjxy_db::WorkTaskKind::IndexMediaSources,
                tjxy_db::WorkScope::CatalogItem(item),
                task_revision,
                100,
            )
            .unwrap()
            .with_input_sync_revision(input_sync_revision)
            .unwrap(),
        )
        .await
        .unwrap();
    let claimed = jobs
        .claim_next(
            &[tjxy_db::WorkTaskKind::IndexMediaSources],
            "cloud-multi-source-index",
            Duration::minutes(1),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(claimed.id(), submission.job().id());
    SourceIndexService::new(database.clone())
        .execute(&claimed)
        .await
        .unwrap()
}

async fn cloud_presentations(
    app: &TestApp,
    fixture: &CloudMultiSourceFixture,
) -> CloudPresentations {
    let sources = tjxy_db::CatalogPublicationRepository::new(&app.database)
        .active_sources(fixture.item)
        .await
        .unwrap();
    let mut default = None;
    let mut alternate = None;
    for source in sources {
        assert_eq!(source.locations().len(), 1);
        let location = &source.locations()[0];
        let provider_object_id: String = app
            .database
            .query_one(
                app.database.get_database_backend().build(
                    Query::select()
                        .column(Alias::new("provider_object_id"))
                        .from(Alias::new("storage_objects"))
                        .and_where(
                            Expr::col(Alias::new("id")).eq(location.storage_object_id().as_uuid()),
                        ),
                ),
            )
            .await
            .unwrap()
            .unwrap()
            .try_get("", "provider_object_id")
            .unwrap();
        let value = (source.id(), source.presentation_key().as_uuid());
        match provider_object_id.as_str() {
            object if object == app.cloud_object_id => {
                assert!(default.replace(value).is_none());
            }
            object if object == app.cloud_alternate_object_id => {
                assert!(alternate.replace(value).is_none());
            }
            object => panic!("unexpected cloud provider object {object}"),
        }
    }
    let (default_source, default) = default.expect("default cloud source");
    let (alternate_source, alternate) = alternate.expect("alternate cloud source");
    assert_ne!(default, alternate);
    CloudPresentations {
        default_source,
        default,
        alternate_source,
        alternate,
    }
}

async fn probe_cloud_sources(app: &TestApp, fixture: &CloudMultiSourceFixture) {
    let jobs = tjxy_db::WorkJobRepository::new(&app.database);
    for (owner, use_default) in [
        ("cloud-default-probe", true),
        ("cloud-alternate-probe", false),
    ] {
        let current = cloud_presentations(app, fixture).await;
        let source_id = if use_default {
            current.default_source
        } else {
            current.alternate_source
        };
        let active = tjxy_db::CatalogPublicationRepository::new(&app.database)
            .active_sources(fixture.item)
            .await
            .unwrap();
        let expected_revision = active
            .iter()
            .find(|source| source.id() == source_id)
            .expect("active Probe source")
            .probe_revision();
        let submission = jobs
            .enqueue_or_join(
                &tjxy_db::WorkJobSpec::new(
                    tjxy_db::WorkTaskKind::ProbeMedia,
                    tjxy_db::WorkScope::MediaSource(source_id),
                    expected_revision,
                    200,
                )
                .unwrap(),
            )
            .await
            .unwrap();
        let claimed = jobs
            .claim_next(
                &[tjxy_db::WorkTaskKind::ProbeMedia],
                owner,
                Duration::minutes(1),
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(claimed.id(), submission.job().id());
        ProbeService::new(app.database.clone())
            .with_backend(app.cloud_account, Arc::clone(&app.cloud_backend))
            .with_inspector(Arc::new(CloudProbeInspector))
            .execute(&claimed)
            .await
            .unwrap_or_else(|error| panic!("{owner} failed: {error:?}"));
    }
}

async fn effective_source_publication(
    database: &DatabaseConnection,
    item: CatalogItemId,
) -> (Uuid, i64) {
    let catalog_item = Alias::new("catalog_items");
    let publication = Alias::new("catalog_publications");
    let query = Query::select()
        .column((publication.clone(), Alias::new("id")))
        .column((publication.clone(), Alias::new("activated_generation")))
        .from(catalog_item.clone())
        .inner_join(
            publication.clone(),
            Expr::col((publication.clone(), Alias::new("id"))).equals((
                catalog_item.clone(),
                Alias::new("active_source_publication_id"),
            )),
        )
        .and_where(Expr::col((catalog_item, Alias::new("id"))).eq(item.as_uuid()))
        .and_where(Expr::col((publication.clone(), Alias::new("state"))).eq("Active"))
        .and_where(Expr::col((publication, Alias::new("publication_kind"))).eq("Sources"))
        .to_owned();
    let row = database
        .query_one(database.get_database_backend().build(&query))
        .await
        .unwrap()
        .unwrap();
    (
        row.try_get("", "id").unwrap(),
        row.try_get("", "activated_generation").unwrap(),
    )
}

#[allow(clippy::too_many_lines)] // Mirrors the normalized lazy CatalogItem-to-root scope.
async fn seed_manual_storage_scope(
    database: &DatabaseConnection,
    library_id: Uuid,
    item_id: CatalogItemId,
    children_indexed: bool,
) -> Uuid {
    let account = Uuid::new_v4();
    let root = Uuid::new_v4();
    let object = Uuid::new_v4();
    let backend = database.get_database_backend();
    for statement in [
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
                "Manual fixture".into(),
                format!("manual-{account}").into(),
                format!("manual-ref-{account}").into(),
                "Active".into(),
            ])
            .to_owned(),
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
                format!("manual-root-{root}").into(),
                3_i64.into(),
                3_i64.into(),
            ])
            .to_owned(),
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
                "local".into(),
                format!("manual-object-{object}").into(),
                "Manual item".into(),
                "manual item".into(),
                "Directory".into(),
                3_i64.into(),
                children_indexed.into(),
                3_i64.into(),
                "ProviderStable".into(),
                "Present".into(),
            ])
            .to_owned(),
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
                root.into(),
                object.into(),
                Option::<Uuid>::None.into(),
                3_i64.into(),
                children_indexed.into(),
                3_i64.into(),
                "Present".into(),
            ])
            .to_owned(),
        Query::insert()
            .into_table(Alias::new("library_storage_roots"))
            .columns([
                Alias::new("id"),
                Alias::new("library_id"),
                Alias::new("storage_root_id"),
            ])
            .values_panic([Uuid::new_v4().into(), library_id.into(), root.into()])
            .to_owned(),
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
                item_id.as_uuid().into(),
                1.0.into(),
                "Matched".into(),
                json!({}).into(),
            ])
            .to_owned(),
    ] {
        database.execute(backend.build(&statement)).await.unwrap();
    }
    object
}

async fn seed_asset(app: &TestApp, item_id: CatalogItemId, bytes: &[u8]) -> String {
    let sha256 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let relative_path = "posters/arrival.jpg";
    tokio::fs::create_dir(app.assets.path().join("posters"))
        .await
        .unwrap();
    tokio::fs::write(app.assets.path().join(relative_path), bytes)
        .await
        .unwrap();
    let blob_id = Uuid::new_v4();
    let backend = app.database.get_database_backend();
    app.database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("asset_blobs"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("sha256"),
                        Alias::new("mime_type"),
                        Alias::new("width"),
                        Alias::new("height"),
                        Alias::new("byte_size"),
                        Alias::new("local_relative_path"),
                    ])
                    .values_panic([
                        blob_id.into(),
                        sha256.into(),
                        "image/jpeg".into(),
                        2_i32.into(),
                        1_i32.into(),
                        i64::try_from(bytes.len()).unwrap().into(),
                        relative_path.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    app.database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("item_assets"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("item_id"),
                        Alias::new("asset_blob_id"),
                        Alias::new("image_type"),
                        Alias::new("priority"),
                        Alias::new("source_provider"),
                    ])
                    .values_panic([
                        Uuid::new_v4().into(),
                        item_id.as_uuid().into(),
                        blob_id.into(),
                        "Primary".into(),
                        0.into(),
                        "fixture".into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    sha256.to_owned()
}

#[allow(clippy::too_many_lines)] // Builds the complete active/probed source read model for HTTP integration.
async fn seed_playable_source(
    database: &DatabaseConnection,
    item: CatalogItemId,
    account: Uuid,
    provider_object_id: &str,
    media_size: i64,
    subtitle_provider_object_id: &str,
) -> Uuid {
    seed_playable_source_for_provider(
        database,
        item,
        account,
        "filesystem",
        provider_object_id,
        media_size,
        subtitle_provider_object_id,
    )
    .await
}

#[allow(clippy::too_many_arguments)] // Test fixture mirrors persisted media_stream fields.
async fn seed_embedded_stream(
    database: &DatabaseConnection,
    presentation: Uuid,
    stream_type: &str,
    stream_index: i32,
    codec: &str,
    width: Option<i32>,
    height: Option<i32>,
    channels: Option<i32>,
    profile: Option<&str>,
    level: Option<i32>,
) {
    let backend = database.get_database_backend();
    let source = database
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("id"))
                    .from(Alias::new("media_sources"))
                    .and_where(Expr::col(Alias::new("presentation_key")).eq(presentation)),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get::<Uuid>("", "id")
        .unwrap();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("media_streams"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("media_source_id"),
                        Alias::new("stream_type"),
                        Alias::new("stream_index"),
                        Alias::new("delivery_index"),
                        Alias::new("codec"),
                        Alias::new("width"),
                        Alias::new("height"),
                        Alias::new("channels"),
                        Alias::new("profile"),
                        Alias::new("level"),
                    ])
                    .values_panic([
                        Uuid::new_v4().into(),
                        source.into(),
                        stream_type.into(),
                        stream_index.into(),
                        stream_index.into(),
                        codec.into(),
                        width.into(),
                        height.into(),
                        channels.into(),
                        profile.into(),
                        level.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
async fn seed_playable_source_for_provider(
    database: &DatabaseConnection,
    item: CatalogItemId,
    account: Uuid,
    provider: &str,
    provider_object_id: &str,
    media_size: i64,
    subtitle_provider_object_id: &str,
) -> Uuid {
    let backend = database.get_database_backend();
    let job = Uuid::new_v4();
    let publication = Uuid::new_v4();
    let source = Uuid::new_v4();
    let presentation = Uuid::new_v4();
    let location = Uuid::new_v4();
    let object = Uuid::new_v4();
    let subtitle_object = Uuid::new_v4();
    let subtitle_id = Uuid::new_v4();
    let media_root = Uuid::new_v4();
    let library_id = database
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("library_id"))
                    .from(Alias::new("library_catalog_items"))
                    .and_where(Expr::col(Alias::new("catalog_item_id")).eq(item.as_uuid())),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get::<Uuid>("", "library_id")
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
                        provider.into(),
                        "Private disk".into(),
                        format!("account-{account}").into(),
                        "secret-ref".into(),
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
                        Alias::new("storage_root_affinity"),
                    ])
                    .values_panic([
                        job.into(),
                        "IndexMediaSources".into(),
                        "CatalogItem".into(),
                        item.as_uuid().into(),
                        0_i64.into(),
                        "Completed".into(),
                        100.into(),
                        1.into(),
                        media_root.into(),
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
                        0_i64.into(),
                        "Active".into(),
                        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
                        3_i64.into(),
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
                        Alias::new("facts_observed_storage_root_id"),
                    ])
                    .values_panic([
                        subtitle_object.into(),
                        account.into(),
                        "private-drive".into(),
                        subtitle_provider_object_id.into(),
                        "Arrival.srt".into(),
                        "arrival.srt".into(),
                        "File".into(),
                        40_i64.into(),
                        1_i64.into(),
                        false.into(),
                        0_i64.into(),
                        "ProviderStable".into(),
                        "Present".into(),
                        media_root.into(),
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
                        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("subtitles"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("media_source_id"),
                        Alias::new("storage_object_id"),
                        Alias::new("format"),
                        Alias::new("language"),
                        Alias::new("delivery_index"),
                        Alias::new("is_default"),
                        Alias::new("is_forced"),
                    ])
                    .values_panic([
                        subtitle_id.into(),
                        source.into(),
                        subtitle_object.into(),
                        "srt".into(),
                        "eng".into(),
                        3.into(),
                        false.into(),
                        false.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("publication_subtitles"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("publication_id"),
                        Alias::new("subtitle_id"),
                        Alias::new("media_source_id"),
                        Alias::new("storage_object_id"),
                        Alias::new("format"),
                        Alias::new("language"),
                        Alias::new("delivery_index"),
                        Alias::new("is_default"),
                        Alias::new("is_forced"),
                        Alias::new("row_sha256"),
                    ])
                    .values_panic([
                        Uuid::new_v4().into(),
                        publication.into(),
                        subtitle_id.into(),
                        source.into(),
                        subtitle_object.into(),
                        "srt".into(),
                        "eng".into(),
                        3.into(),
                        false.into(),
                        false.into(),
                        "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd".into(),
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
                        Alias::new("facts_observed_storage_root_id"),
                    ])
                    .values_panic([
                        object.into(),
                        account.into(),
                        "private-drive".into(),
                        provider_object_id.into(),
                        "Arrival.mkv".into(),
                        "arrival.mkv".into(),
                        "File".into(),
                        media_size.into(),
                        1_i64.into(),
                        false.into(),
                        0_i64.into(),
                        "ProviderStable".into(),
                        "Present".into(),
                        media_root.into(),
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
                        media_root.into(),
                        account.into(),
                        provider_object_id.into(),
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
                    .into_table(Alias::new("library_storage_roots"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("library_id"),
                        Alias::new("storage_root_id"),
                    ])
                    .values_panic([Uuid::new_v4().into(), library_id.into(), media_root.into()]),
            ),
        )
        .await
        .unwrap();
    for storage_object in [object, subtitle_object] {
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
                            media_root.into(),
                            storage_object.into(),
                            1_i64.into(),
                            false.into(),
                            0_i64.into(),
                            "Present".into(),
                        ]),
                ),
            )
            .await
            .unwrap();
    }
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
                        10.into(),
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
                        10.into(),
                        "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc".into(),
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
                    .and_where(sea_orm::sea_query::Expr::col(Alias::new("id")).eq(item.as_uuid())),
            ),
        )
        .await
        .unwrap();
    presentation
}

async fn get(router: &axum::Router, uri: &str, token: Option<&str>) -> axum::response::Response {
    let mut request = Request::builder().uri(uri);
    if let Some(token) = token {
        request = request.header(
            header::AUTHORIZATION,
            format!(r#"MediaBrowser Token="{token}""#),
        );
    }
    router
        .clone()
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap()
}

async fn post_empty(
    router: &axum::Router,
    uri: &str,
    token: Option<&str>,
) -> axum::response::Response {
    let mut request = Request::builder().method("POST").uri(uri);
    if let Some(token) = token {
        request = request.header(
            header::AUTHORIZATION,
            format!(r#"MediaBrowser Token="{token}""#),
        );
    }
    router
        .clone()
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap()
}

async fn delete_empty(
    router: &axum::Router,
    uri: &str,
    token: Option<&str>,
) -> axum::response::Response {
    let mut request = Request::builder().method("DELETE").uri(uri);
    if let Some(token) = token {
        request = request.header(
            header::AUTHORIZATION,
            format!(r#"MediaBrowser Token="{token}""#),
        );
    }
    router
        .clone()
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap()
}

async fn stream_request(
    router: &axum::Router,
    method: &str,
    uri: &str,
    token: Option<&str>,
    range: Option<&str>,
    if_range: Option<&str>,
) -> axum::response::Response {
    let mut request = Request::builder().method(method).uri(uri);
    if let Some(token) = token {
        request = request.header(
            header::AUTHORIZATION,
            format!(r#"MediaBrowser Token="{token}""#),
        );
    }
    if let Some(range) = range {
        request = request.header(header::RANGE, range);
    }
    if let Some(if_range) = if_range {
        request = request.header(header::IF_RANGE, if_range);
    }
    router
        .clone()
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap()
}

fn token_header(token: &str) -> String {
    format!(r#"MediaBrowser Token="{token}""#)
}

fn cloud_leak_markers(app: &TestApp, fixture: &CloudMultiSourceFixture) -> Vec<String> {
    [
        CLOUD_PROVIDER.to_owned(),
        app.cloud_object_id.clone(),
        app.cloud_alternate_object_id.clone(),
        app.cloud_subtitle_object_id.clone(),
        CLOUD_DRIVE.to_owned(),
        CLOUD_ROOT_OBJECT_ID.to_owned(),
        CLOUD_DIRECTORY_OBJECT_ID.to_owned(),
        app.cloud_account.to_string(),
        CLOUD_ACCOUNT_IDENTITY.to_owned(),
        "account-secret-marker".to_owned(),
        CLOUD_DISPLAY_NAME.to_owned(),
        CLOUD_CREDENTIAL_REF.to_owned(),
        "credential-secret-marker".to_owned(),
        "https://upstream.invalid/secret".to_owned(),
        "upstream-token-secret".to_owned(),
        fixture.root.to_string(),
        fixture.default_object.to_string(),
        fixture.alternate_object.to_string(),
        fixture.subtitle_object.to_string(),
    ]
    .into_iter()
    .collect()
}

fn assert_no_markers(encoded: &str, markers: &[String], context: &str) {
    for marker in markers {
        assert!(
            !encoded.contains(marker),
            "{context} leaked marker {marker:?}"
        );
    }
}

fn assert_headers_do_not_leak(
    headers: &reqwest::header::HeaderMap,
    markers: &[String],
    context: &str,
) {
    for (name, value) in headers {
        for marker in markers {
            let marker_bytes = marker.as_bytes();
            assert!(!marker_bytes.is_empty(), "leak marker must not be empty");
            let name_leaks = name
                .as_str()
                .as_bytes()
                .windows(marker_bytes.len())
                .any(|window| window == marker_bytes);
            let value_leaks = value
                .as_bytes()
                .windows(marker_bytes.len())
                .any(|window| window == marker_bytes);
            assert!(
                !name_leaks && !value_leaks,
                "{context} leaked marker {marker:?} in header {name}:{}",
                String::from_utf8_lossy(value.as_bytes())
            );
        }
    }
}

async fn tcp_login(
    client: &reqwest::Client,
    server: &TcpTestServer,
    markers: &[String],
) -> (Uuid, String) {
    let response = client
        .post(format!("{}/Users/AuthenticateByName", server.base_url))
        .header(header::AUTHORIZATION, IDENTITY)
        .json(&json!({"Username": "alice", "Pw": "correct horse"}))
        .send()
        .await
        .expect("authenticate cloud playback administrator");
    assert_eq!(response.status(), StatusCode::OK);
    assert_headers_do_not_leak(response.headers(), markers, "authentication header");
    let authentication: Value = response.json().await.expect("authentication response JSON");
    (
        Uuid::parse_str(
            authentication["User"]["Id"]
                .as_str()
                .expect("authenticated user ID"),
        )
        .expect("authenticated user ID is a UUID"),
        authentication["AccessToken"]
            .as_str()
            .expect("authentication token")
            .to_owned(),
    )
}

fn normalize_cloud_playback(
    playback: &mut Value,
    item: CatalogItemId,
    presentations: CloudPresentations,
) {
    let _play_session = playback["PlaySessionId"]
        .as_str()
        .and_then(|value| Uuid::parse_str(value).ok())
        .expect("PlaybackInfo PlaySessionId is a UUID");
    playback["PlaySessionId"] = json!("{{play_session_id}}");
    let sources = playback["MediaSources"]
        .as_array_mut()
        .expect("PlaybackInfo media source list");
    assert_eq!(sources.len(), 2, "complete cloud source list");
    let mut seen = HashSet::new();
    for source in sources {
        let source_id = Uuid::parse_str(source["Id"].as_str().expect("media source ID"))
            .expect("media source ID is a UUID");
        assert!(seen.insert(source_id), "duplicate media source ID");
        let placeholder = if source_id == presentations.default {
            "{{default_source_id}}"
        } else if source_id == presentations.alternate {
            "{{alternate_source_id}}"
        } else {
            panic!("unknown media source ID {source_id}");
        };
        let expected_direct = format!(
            "/Videos/{}/stream?static=true&mediaSourceId={source_id}",
            item.as_uuid()
        );
        assert_eq!(
            source["DirectStreamUrl"].as_str(),
            Some(expected_direct.as_str())
        );
        source["Id"] = json!(placeholder);
        source["DirectStreamUrl"] = json!(format!(
            "/Videos/{{{{item_id}}}}/stream?static=true&mediaSourceId={placeholder}"
        ));
        for stream in source["MediaStreams"]
            .as_array_mut()
            .expect("media stream list")
        {
            if stream["IsExternal"] == true {
                assert_eq!(source_id, presentations.default);
                let index = stream["Index"].as_i64().expect("subtitle index");
                let codec = stream["Codec"].as_str().expect("subtitle codec");
                let expected_subtitle = format!(
                    "/Videos/{}/{source_id}/Subtitles/{index}/Stream.{codec}",
                    item.as_uuid()
                );
                assert_eq!(
                    stream["DeliveryUrl"].as_str(),
                    Some(expected_subtitle.as_str())
                );
                stream["DeliveryUrl"] = json!(format!(
                    "/Videos/{{{{item_id}}}}/{placeholder}/Subtitles/{index}/Stream.{codec}"
                ));
            }
        }
    }
    assert_eq!(seen.len(), 2);
}

async fn read_cloud_playback(
    client: &reqwest::Client,
    server: &TcpTestServer,
    token: &str,
    user: Uuid,
    fixture: &CloudMultiSourceFixture,
    presentations: CloudPresentations,
    markers: &[String],
) -> CloudPlaybackSnapshot {
    let request: Value =
        serde_json::from_str(CLOUD_PLAYBACK_REQUEST).expect("cloud PlaybackInfo request golden");
    let response = client
        .post(format!(
            "{}/Items/{}/PlaybackInfo?userId={user}",
            server.base_url,
            fixture.item.as_uuid()
        ))
        .header(header::AUTHORIZATION, token_header(token))
        .json(&request)
        .send()
        .await
        .expect("cloud PlaybackInfo request");
    assert_eq!(response.status(), StatusCode::OK);
    assert_headers_do_not_leak(response.headers(), markers, "PlaybackInfo header");
    let playback: Value = response
        .json()
        .await
        .expect("cloud PlaybackInfo response JSON");
    assert_no_markers(
        &serde_json::to_string(&playback).unwrap(),
        markers,
        "PlaybackInfo JSON",
    );
    let sources = playback["MediaSources"]
        .as_array()
        .expect("PlaybackInfo media source list");
    assert_eq!(sources.len(), 2);
    let source_order = sources
        .iter()
        .map(|source| {
            Uuid::parse_str(source["Id"].as_str().expect("media source ID"))
                .expect("media source ID is a UUID")
        })
        .collect::<Vec<_>>();
    assert_eq!(
        source_order,
        vec![presentations.default, presentations.alternate]
    );
    let direct_urls = sources
        .iter()
        .map(|source| {
            source["DirectStreamUrl"]
                .as_str()
                .expect("direct stream URL")
                .to_owned()
        })
        .collect::<Vec<_>>();
    let subtitles = sources
        .iter()
        .flat_map(|source| {
            source["MediaStreams"]
                .as_array()
                .expect("media stream list")
                .iter()
                .filter(|stream| stream["IsExternal"] == true)
        })
        .collect::<Vec<_>>();
    assert_eq!(subtitles.len(), 1);
    let subtitle_index = subtitles[0]["Index"].as_i64().expect("subtitle index");
    let subtitle_url = subtitles[0]["DeliveryUrl"]
        .as_str()
        .expect("subtitle URL")
        .to_owned();
    let mut normalized = playback;
    normalize_cloud_playback(&mut normalized, fixture.item, presentations);
    let expected: Value =
        serde_json::from_str(CLOUD_PLAYBACK_RESPONSE).expect("cloud PlaybackInfo response golden");
    assert_eq!(normalized, expected);
    CloudPlaybackSnapshot {
        source_order,
        direct_urls,
        subtitle_index,
        subtitle_url,
    }
}

fn assert_cloud_representation_headers(
    headers: &HeaderMap,
    content_length: &str,
    content_range: Option<&str>,
) {
    assert_eq!(headers[header::CONTENT_TYPE], "video/x-matroska");
    assert_eq!(headers[header::CACHE_CONTROL], "private, no-cache");
    assert_eq!(headers[header::ACCEPT_RANGES], "bytes");
    assert_eq!(headers[header::CONTENT_LENGTH], content_length);
    let etag = headers
        .get(header::ETAG)
        .expect("cloud media ETag header")
        .to_str()
        .expect("cloud media ETag is ASCII");
    assert!(
        etag.len() > 2 && etag.starts_with('"') && etag.ends_with('"'),
        "cloud media ETag is a non-empty strong tag"
    );
    match content_range {
        Some(expected) => assert_eq!(headers[header::CONTENT_RANGE], expected),
        None => assert!(
            headers.get(header::CONTENT_RANGE).is_none(),
            "full cloud media response has no Content-Range header"
        ),
    }
}

fn assert_matching_cloud_delivery_headers(actual: &HeaderMap, expected: &HeaderMap) {
    for name in [
        header::CONTENT_TYPE,
        header::CACHE_CONTROL,
        header::ACCEPT_RANGES,
        header::ETAG,
        header::CONTENT_LENGTH,
        header::CONTENT_RANGE,
    ] {
        assert_eq!(actual.get(&name), expected.get(&name), "{name} header");
    }
}

async fn assert_cloud_default_delivery(
    client: &reqwest::Client,
    server: &TcpTestServer,
    authorization: &str,
    direct_url: &str,
    markers: &[String],
) {
    let full = client
        .get(format!("{}{}", server.base_url, direct_url))
        .header(header::AUTHORIZATION, authorization)
        .send()
        .await
        .expect("default full media request");
    assert_eq!(full.status(), StatusCode::OK);
    assert_headers_do_not_leak(full.headers(), markers, "default full media header");
    assert_cloud_representation_headers(full.headers(), "17", None);
    let full_headers = full.headers().clone();
    assert_eq!(full.bytes().await.unwrap().as_ref(), CLOUD_DEFAULT_BYTES);

    let head = client
        .head(format!("{}{}", server.base_url, direct_url))
        .header(header::AUTHORIZATION, authorization)
        .send()
        .await
        .expect("default HEAD request");
    assert_eq!(head.status(), StatusCode::OK);
    assert_headers_do_not_leak(head.headers(), markers, "default HEAD header");
    assert_cloud_representation_headers(head.headers(), "17", None);
    assert_matching_cloud_delivery_headers(head.headers(), &full_headers);
    assert!(head.bytes().await.unwrap().is_empty());

    let range = client
        .get(format!("{}{}", server.base_url, direct_url))
        .header(header::AUTHORIZATION, authorization)
        .header(header::RANGE, "bytes=6-9")
        .send()
        .await
        .expect("default ranged media request");
    assert_eq!(range.status(), StatusCode::PARTIAL_CONTENT);
    assert_headers_do_not_leak(range.headers(), markers, "default range header");
    assert_cloud_representation_headers(range.headers(), "4", Some("bytes 6-9/17"));
    let range_headers = range.headers().clone();
    assert_eq!(
        range.bytes().await.unwrap().as_ref(),
        &CLOUD_DEFAULT_BYTES[6..10]
    );

    let range_head = client
        .head(format!("{}{}", server.base_url, direct_url))
        .header(header::AUTHORIZATION, authorization)
        .header(header::RANGE, "bytes=6-9")
        .send()
        .await
        .expect("default ranged HEAD request");
    assert_eq!(range_head.status(), StatusCode::PARTIAL_CONTENT);
    assert_headers_do_not_leak(range_head.headers(), markers, "default range HEAD header");
    assert_cloud_representation_headers(range_head.headers(), "4", Some("bytes 6-9/17"));
    assert_matching_cloud_delivery_headers(range_head.headers(), &range_headers);
    assert!(range_head.bytes().await.unwrap().is_empty());
}

async fn assert_cloud_delivery(
    app: &TestApp,
    client: &reqwest::Client,
    server: &TcpTestServer,
    token: &str,
    snapshot: &CloudPlaybackSnapshot,
    markers: &[String],
) {
    let authorization = token_header(token);
    assert_cloud_default_delivery(
        client,
        server,
        &authorization,
        &snapshot.direct_urls[0],
        markers,
    )
    .await;

    let subtitle = client
        .get(format!("{}{}", server.base_url, snapshot.subtitle_url))
        .header(header::AUTHORIZATION, &authorization)
        .send()
        .await
        .expect("advertised cloud subtitle request");
    assert_eq!(subtitle.status(), StatusCode::OK);
    assert_headers_do_not_leak(subtitle.headers(), markers, "subtitle header");
    assert_eq!(
        subtitle.headers()[header::CONTENT_LENGTH],
        CLOUD_SUBTITLE_BYTES.len().to_string()
    );
    assert_eq!(
        subtitle.bytes().await.unwrap().as_ref(),
        CLOUD_SUBTITLE_BYTES
    );

    let alternate = client
        .get(format!("{}{}", server.base_url, snapshot.direct_urls[1]))
        .header(header::AUTHORIZATION, &authorization)
        .send()
        .await
        .expect("alternate full media request");
    assert_eq!(alternate.status(), StatusCode::OK);
    assert_headers_do_not_leak(alternate.headers(), markers, "alternate media header");
    assert_eq!(alternate.headers()[header::CONTENT_LENGTH], "17");
    assert_eq!(
        alternate.bytes().await.unwrap().as_ref(),
        CLOUD_ALTERNATE_BYTES
    );

    assert_eq!(
        app.cloud_backend.take_ranges(),
        vec![
            (app.cloud_object_id.clone(), 0, 17),
            (app.cloud_object_id.clone(), 6, 10),
            (
                app.cloud_subtitle_object_id.clone(),
                0,
                u64::try_from(CLOUD_SUBTITLE_BYTES.len()).unwrap(),
            ),
            (app.cloud_alternate_object_id.clone(), 0, 17),
        ]
    );
}

#[tokio::test]
async fn browse_routes_require_a_valid_session() {
    let app = test_app().await;

    assert_eq!(
        get(&app.router, "/UserViews", None).await.status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        get(&app.router, "/Items", None).await.status(),
        StatusCode::UNAUTHORIZED
    );
    for path in ["/Items/Latest", "/UserItems/Resume", "/Shows/NextUp"] {
        assert_eq!(
            get(&app.router, path, None).await.status(),
            StatusCode::UNAUTHORIZED,
            "{path}"
        );
    }
    for path in ["/Sessions/Capabilities/Full", "/Sessions/Capabilities"] {
        let response = app
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(path)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{path}");
    }
}

#[tokio::test]
async fn socket_upgrade_requires_a_valid_session() {
    let app = test_app().await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, app.router).await.unwrap();
    });

    let error = tokio_tungstenite::connect_async(format!("ws://{address}/socket"))
        .await
        .unwrap_err();
    server.abort();
    match error {
        tokio_tungstenite::tungstenite::Error::Http(response) => {
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        }
        error => panic!("expected an HTTP authentication failure, got {error}"),
    }
}

#[tokio::test]
async fn jellyfin_media_player_socket_accepts_only_its_authenticated_device_id() {
    let app = test_app().await;
    let (_, _, token) = login(&app.router).await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, app.router).await.unwrap();
    });

    let (socket, _) = tokio_tungstenite::connect_async(format!(
        "ws://{address}/socket?api_key={token}&deviceId=phone-1"
    ))
    .await
    .unwrap();
    drop(socket);
    let error = tokio_tungstenite::connect_async(format!(
        "ws://{address}/socket?api_key={token}&deviceId=another-device"
    ))
    .await
    .unwrap_err();
    server.abort();
    match error {
        tokio_tungstenite::tungstenite::Error::Http(response) => {
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        }
        error => panic!("expected an HTTP device mismatch, got {error}"),
    }
}

#[tokio::test]
async fn system_endpoint_requires_auth_and_does_not_guess_missing_connection_info() {
    let app = test_app().await;
    assert_eq!(
        get(&app.router, "/System/Endpoint", None).await.status(),
        StatusCode::UNAUTHORIZED
    );
    let (_, _, token) = login(&app.router).await;
    let response = get(&app.router, "/System/Endpoint", Some(&token)).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(
        serde_json::from_slice::<Value>(&body).unwrap(),
        json!({"IsLocal": false, "IsInNetwork": false})
    );
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Keeps the real JMP bootstrap request dialect in one regression contract.
async fn jellyfin_media_player_bootstrap_routes_match_its_request_dialect() {
    let app = test_app().await;
    let library = seed_library(&app.database, "JMP TV", true).await;
    let item = seed_item(&app.database, library, "Smoke Show", "Series").await;
    let season = seed_item(&app.database, library, "Season 01", "Season").await;
    let episode = seed_item(&app.database, library, "S01E01", "Episode").await;
    let backend = app.database.get_database_backend();
    for (child, parent) in [(season, item), (episode, season)] {
        app.database
            .execute(
                backend.build(
                    Query::update()
                        .table(Alias::new("catalog_items"))
                        .value(Alias::new("parent_id"), parent.as_uuid())
                        .and_where(Expr::col(Alias::new("id")).eq(child.as_uuid())),
                ),
            )
            .await
            .unwrap();
    }
    let (user_id, _, token) = login(&app.router).await;

    let library_detail = get(
        &app.router,
        &format!("/Users/{user_id}/Items/{library}"),
        Some(&token),
    )
    .await;
    assert_eq!(library_detail.status(), StatusCode::OK);
    assert_eq!(
        serde_json::from_slice::<Value>(
            &library_detail
                .into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes()
        )
        .unwrap()["Name"],
        "JMP TV"
    );

    let items = get(
        &app.router,
        &format!(
            "/Users/{user_id}/Items?ParentId={library}&IncludeItemTypes=Series&Recursive=true&Fields=PrimaryImageAspectRatio&ImageTypeLimit=1&EnableImageTypes=Primary%2CBackdrop&EnableUserData=true"
        ),
        Some(&token),
    )
    .await;
    assert_eq!(items.status(), StatusCode::OK);
    let items: Value =
        serde_json::from_slice(&items.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(items["Items"][0]["Id"], item.to_string());

    assert_eq!(
        get(
            &app.router,
            &format!(
                "/Users/{user_id}/Items/Latest?ParentId={library}&Fields=BasicSyncInfo%2CCanDelete%2CContainer%2CPrimaryImageAspectRatio%2CProductionYear%2CStatus%2CEndDate&Recursive=true&MediaTypes=Video&Limit=20&ImageTypeLimit=1&IsPlayed=false&EnableImageTypes=Primary%2CBackdrop%2CThumb"
            ),
            Some(&token),
        )
        .await
        .status(),
        StatusCode::OK
    );

    for media_type in ["Audio", "Book"] {
        let resume = get(
            &app.router,
            &format!(
                "/Users/{user_id}/Items/Resume?MediaTypes={media_type}&Limit=12&Recursive=true&Fields=PrimaryImageAspectRatio&ImageTypeLimit=1&EnableImageTypes=Primary%2CBackdrop&EnableTotalRecordCount=false"
            ),
            Some(&token),
        )
        .await;
        assert_eq!(resume.status(), StatusCode::OK, "{media_type}");
        let resume: Value =
            serde_json::from_slice(&resume.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert_eq!(
            resume,
            json!({"Items": [], "TotalRecordCount": 0, "StartIndex": 0})
        );
    }

    assert_eq!(
        get(
            &app.router,
            &format!(
                "/Shows/NextUp?Fields=PrimaryImageAspectRatio%2CDateCreated%2CBasicSyncInfo%2CPath%2CMediaSourceCount&UserId={user_id}&MediaTypes=Video&Limit=20&ImageTypeLimit=1&EnableTotalRecordCount=false&DisableFirstEpisode=false&EnableRewatching=false&EnableImageTypes=Primary%2CBackdrop%2CThumb"
            ),
            Some(&token),
        )
        .await
        .status(),
        StatusCode::OK
    );

    let seasons = get(
        &app.router,
        &format!(
            "/Shows/{item}/Seasons?UserId={user_id}&Fields=ItemCounts%2CPrimaryImageAspectRatio%2CCanDelete%2CMediaSourceCount"
        ),
        Some(&token),
    )
    .await;
    assert_eq!(seasons.status(), StatusCode::OK);
    let seasons: Value =
        serde_json::from_slice(&seasons.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(seasons["Items"][0]["Id"], season.to_string());
    assert_eq!(seasons["Items"][0]["SeriesId"], item.to_string());

    let episodes = get(
        &app.router,
        &format!(
            "/Shows/{item}/Episodes?UserId={user_id}&SeasonId={season}&Fields=PrimaryImageAspectRatio%2CMediaSourceCount&EnableUserData=true"
        ),
        Some(&token),
    )
    .await;
    assert_eq!(episodes.status(), StatusCode::OK);
    let episodes: Value =
        serde_json::from_slice(&episodes.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(episodes["Items"][0]["Id"], episode.to_string());
    assert_eq!(episodes["Items"][0]["SeasonId"], season.to_string());
    assert_eq!(
        get(
            &app.router,
            &format!("/Shows/{item}/Episodes?UserId={user_id}&Season=1"),
            Some(&token),
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );
    let live_tv = get(
        &app.router,
        &format!("/LiveTv/Programs?UserId={user_id}&ImageTypeLimit=1&HasAired=false&Limit=50"),
        Some(&token),
    )
    .await;
    assert_eq!(live_tv.status(), StatusCode::OK);
    assert_eq!(
        serde_json::from_slice::<Value>(&live_tv.into_body().collect().await.unwrap().to_bytes())
            .unwrap(),
        json!({"Items": [], "TotalRecordCount": 0, "StartIndex": 0})
    );
    assert_eq!(
        get(
            &app.router,
            &format!("/LiveTv/Programs?UserId={}", Uuid::new_v4()),
            Some(&token),
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );

    let bitrate = get(
        &app.router,
        "/Playback/BitrateTest?Size=500000",
        Some(&token),
    )
    .await;
    assert_eq!(bitrate.status(), StatusCode::OK);
    assert_eq!(
        bitrate.headers()[header::CONTENT_TYPE],
        "application/octet-stream"
    );
    assert_eq!(
        bitrate
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .len(),
        524_288
    );
}

#[tokio::test]
async fn user_data_commit_notifies_the_authenticated_users_socket() {
    let app = test_app().await;
    let library = seed_library(&app.database, "Library", true).await;
    let item = seed_item(&app.database, library, "Arrival", "Movie").await;
    let (_, _, token) = login(&app.router).await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, app.router).await.unwrap();
    });

    let (mut socket, _) =
        tokio_tungstenite::connect_async(format!("ws://{address}/socket?api_key={token}"))
            .await
            .unwrap();
    let response = reqwest::Client::new()
        .post(format!("http://{address}/UserItems/{item}/UserData"))
        .header(
            header::AUTHORIZATION,
            format!(r#"MediaBrowser Token="{token}""#),
        )
        .json(&json!({"IsFavorite": true}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let received = tokio::time::timeout(std::time::Duration::from_secs(1), socket.next()).await;
    server.abort();
    let message = received
        .expect("user data change must notify the active socket")
        .expect("socket must remain open")
        .expect("socket message must be valid");
    let tokio_tungstenite::tungstenite::Message::Text(payload) = message else {
        panic!("expected a text event");
    };
    let event: Value = serde_json::from_str(&payload).unwrap();
    assert_eq!(event["MessageType"], "UserDataChanged");
    assert_eq!(event["Data"]["UserRevision"], 1);
}

#[tokio::test]
async fn user_data_commit_does_not_notify_another_users_socket() {
    let app = test_app().await;
    let library = seed_library(&app.database, "Library", true).await;
    let item = seed_item(&app.database, library, "Arrival", "Movie").await;
    AuthService::new(
        app.database.clone(),
        SystemClock,
        Some(Duration::days(30)),
        2,
    )
    .await
    .unwrap()
    .create_user("Bob", "ordinary password", false)
    .await
    .unwrap();
    let (_, _, alice_token) = login(&app.router).await;
    let (_, _, bob_token) = login_as(&app.router, "bob", "ordinary password").await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, app.router).await.unwrap();
    });

    let (mut alice_socket, _) =
        tokio_tungstenite::connect_async(format!("ws://{address}/socket?api_key={alice_token}"))
            .await
            .unwrap();
    let (mut bob_socket, _) =
        tokio_tungstenite::connect_async(format!("ws://{address}/socket?api_key={bob_token}"))
            .await
            .unwrap();
    let response = reqwest::Client::new()
        .post(format!("http://{address}/UserItems/{item}/UserData"))
        .header(
            header::AUTHORIZATION,
            format!(r#"MediaBrowser Token="{alice_token}""#),
        )
        .json(&json!({"IsFavorite": true}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let alice_event =
        tokio::time::timeout(std::time::Duration::from_secs(1), alice_socket.next()).await;
    let bob_event =
        tokio::time::timeout(std::time::Duration::from_millis(250), bob_socket.next()).await;
    server.abort();
    assert!(alice_event.is_ok(), "the owner must receive the event");
    assert!(
        bob_event.is_err(),
        "another user must not receive the event"
    );
}

#[tokio::test]
async fn user_views_and_root_items_return_enabled_libraries_in_the_query_wrapper() {
    let app = test_app().await;
    seed_library(&app.database, "Zeta", true).await;
    seed_library(&app.database, "Alpha", true).await;
    seed_library(&app.database, "Hidden", false).await;
    let (user_id, _, token) = login(&app.router).await;

    for path in [
        format!("/UserViews?userId={user_id}"),
        format!("/Users/{user_id}/Views?IncludeExternalContent=false"),
        format!("/Items?userId={user_id}"),
    ] {
        let response = get(&app.router, &path, Some(&token)).await;
        assert_eq!(response.status(), StatusCode::OK, "{path}");
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let result: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(result["TotalRecordCount"], 2);
        assert_eq!(result["StartIndex"], 0);
        assert_eq!(result["Items"][0]["Name"], "Alpha");
        assert_eq!(result["Items"][0]["Type"], "CollectionFolder");
        assert_eq!(result["Items"][0]["CollectionType"], "movies");
        assert_eq!(result["Items"][1]["Name"], "Zeta");
    }

    let response = get(&app.router, "/Items?startIndex=1&limit=1", Some(&token)).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let result: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(result["TotalRecordCount"], 2);
    assert_eq!(result["StartIndex"], 1);
    assert_eq!(result["Items"].as_array().unwrap().len(), 1);
    assert_eq!(result["Items"][0]["Name"], "Zeta");
}

#[tokio::test]
async fn vidhub_favorite_filter_returns_only_favorites() {
    let app = test_app().await;
    let library = seed_library(&app.database, "Library", true).await;
    let favorite = seed_item(&app.database, library, "Favorite", "Movie").await;
    seed_item(&app.database, library, "Ordinary", "Movie").await;
    let (user_id, _, token) = login(&app.router).await;

    let response = post(
        &app.router,
        &format!("/UserItems/{favorite}/UserData"),
        &token,
        json!({"IsFavorite": true}).to_string(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let response = get(
        &app.router,
        &format!(
            "/Users/{user_id}/Items?Filters=IsFavorite&Recursive=true&IncludeItemTypes=Movie&SortBy=SortName&SortOrder=Ascending"
        ),
        Some(&token),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let result: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(result["TotalRecordCount"], 1);
    assert_eq!(result["Items"][0]["Id"], favorite.to_string());

    let unsupported = get(
        &app.router,
        &format!(
            "/Users/{user_id}/Items?Filters=IsFavorite&Recursive=true&IncludeItemTypes=BoxSet"
        ),
        Some(&token),
    )
    .await;
    assert_eq!(unsupported.status(), StatusCode::OK);
    let unsupported: Value =
        serde_json::from_slice(&unsupported.into_body().collect().await.unwrap().to_bytes())
            .unwrap();
    assert_eq!(unsupported["TotalRecordCount"], 0);
}

#[tokio::test]
async fn items_apply_parent_paging_and_findroid_type_filter() {
    let app = test_app().await;
    let library = seed_library(&app.database, "Library", true).await;
    seed_item(&app.database, library, "Arrival", "Movie").await;
    let blade_runner = seed_item(&app.database, library, "Blade Runner", "Movie").await;
    let sha256 = seed_asset(&app, blade_runner, b"jpeg").await;
    seed_item(&app.database, library, "Dark", "Series").await;
    let (user_id, _, token) = login(&app.router).await;

    let path = format!(
        "/Items?userId={user_id}&parentId={library}&includeItemTypes=Movie&recursive=false&sortBy=SortName&sortOrder=Ascending&startIndex=1&limit=1"
    );
    let response = get(&app.router, &path, Some(&token)).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let result: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(result["TotalRecordCount"], 2);
    assert_eq!(result["StartIndex"], 1);
    assert_eq!(result["Items"].as_array().unwrap().len(), 1);
    assert_eq!(result["Items"][0]["Name"], "Blade Runner");
    assert_eq!(result["Items"][0]["Type"], "Movie");
    assert_eq!(result["Items"][0]["MediaType"], "Video");
    assert_eq!(result["Items"][0]["ParentId"], library.to_string());
    assert_eq!(result["Items"][0]["ImageTags"]["Primary"], sha256);
}

#[tokio::test]
async fn items_filter_genre_and_production_year_before_counting_and_paging() {
    let app = test_app().await;
    let library = seed_library(&app.database, "Library", true).await;
    let arrival = seed_item(&app.database, library, "Arrival", "Movie").await;
    let dune = seed_item(&app.database, library, "Dune", "Movie").await;
    let drama = Uuid::new_v4();
    let backend = app.database.get_database_backend();
    app.database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("genres"))
                    .columns([Alias::new("id"), Alias::new("name")])
                    .values_panic([drama.into(), "Drama".into()]),
            ),
        )
        .await
        .unwrap();
    app.database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("item_genres"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("catalog_item_id"),
                        Alias::new("genre_id"),
                    ])
                    .values_panic([
                        Uuid::new_v4().into(),
                        arrival.as_uuid().into(),
                        drama.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    for (item, year) in [(arrival, 2016_i32), (dune, 2021_i32)] {
        app.database
            .execute(
                backend.build(
                    Query::update()
                        .table(Alias::new("catalog_items"))
                        .value(Alias::new("production_year"), year)
                        .and_where(Expr::col(Alias::new("id")).eq(item.as_uuid())),
                ),
            )
            .await
            .unwrap();
    }
    let (_, _, token) = login(&app.router).await;

    let response = get(
        &app.router,
        &format!(
            "/Items?parentId={library}&includeItemTypes=Movie&genre=Drama&productionYear=2016&startIndex=0&limit=1"
        ),
        Some(&token),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let result: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(result["TotalRecordCount"], 1);
    assert_eq!(result["Items"][0]["Name"], "Arrival");

    let response = get(
        &app.router,
        &format!("/Items/Filters?parentId={library}"),
        Some(&token),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let facets: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(facets["Genres"], json!(["Drama"]));
    assert_eq!(facets["ProductionYears"], json!([2021, 2016]));
}

#[tokio::test]
async fn items_search_term_returns_visible_type_filtered_items() {
    let app = test_app().await;
    let library = seed_library(&app.database, "Library", true).await;
    seed_item(&app.database, library, "Arrival", "Movie").await;
    seed_item(&app.database, library, "Alpine", "Movie").await;
    seed_item(&app.database, library, "Alpha Song", "Audio").await;
    let (_, _, token) = login(&app.router).await;

    let response = get(
        &app.router,
        "/Items?searchTerm=Alp&includeItemTypes=Movie",
        Some(&token),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let result: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(result["TotalRecordCount"], 1);
    assert_eq!(result["Items"][0]["Name"], "Alpine");
}

#[tokio::test]
async fn items_without_parent_filter_visible_items_by_type() {
    let app = test_app().await;
    let first = seed_library(&app.database, "Movies", true).await;
    let second = seed_library(&app.database, "Music", true).await;
    seed_item(&app.database, first, "Arrival", "Movie").await;
    seed_item(&app.database, second, "Alpha Song", "Audio").await;
    let (_, _, token) = login(&app.router).await;

    let response = get(&app.router, "/Items?includeItemTypes=Movie", Some(&token)).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let result: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(result["TotalRecordCount"], 1);
    assert_eq!(result["Items"][0]["Name"], "Arrival");
}

#[tokio::test]
async fn items_library_type_filter_defaults_to_recursive_unless_explicitly_disabled() {
    let app = test_app().await;
    let library = seed_library(&app.database, "Movies", true).await;
    let folder = seed_item(&app.database, library, "Folder", "Folder").await;
    let movie = seed_item(&app.database, library, "Arrival", "Movie").await;
    let backend = app.database.get_database_backend();
    app.database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("catalog_items"))
                    .value(Alias::new("parent_id"), folder.as_uuid())
                    .and_where(Expr::col(Alias::new("id")).eq(movie.as_uuid())),
            ),
        )
        .await
        .unwrap();
    let (_, _, token) = login(&app.router).await;

    let recursive = get(
        &app.router,
        &format!("/Items?parentId={library}&includeItemTypes=Movie"),
        Some(&token),
    )
    .await;
    let body = recursive.into_body().collect().await.unwrap().to_bytes();
    let recursive: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(recursive["TotalRecordCount"], 1);
    assert_eq!(recursive["Items"][0]["Id"], movie.to_string());

    let direct = get(
        &app.router,
        &format!("/Items?parentId={library}&includeItemTypes=Movie&recursive=false"),
        Some(&token),
    )
    .await;
    let body = direct.into_body().collect().await.unwrap().to_bytes();
    let direct: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(direct["TotalRecordCount"], 0);
}

#[tokio::test]
async fn items_tolerate_projection_flags_and_unknown_client_hints() {
    let app = test_app().await;
    seed_library(&app.database, "Library", true).await;
    let (_, _, token) = login(&app.router).await;

    let response = get(
        &app.router,
        "/Items?fields=Genres,ProviderIds&enableImages=true&enableUserData=true&imageTypeLimit=1&enableImageTypes=Primary&enableTotalRecordCount=true&clientHint=ignored",
        Some(&token),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn items_sort_name_descending_reverses_the_stable_name_order() {
    let app = test_app().await;
    let library = seed_library(&app.database, "Library", true).await;
    seed_item(&app.database, library, "Arrival", "Movie").await;
    seed_item(&app.database, library, "Blade Runner", "Movie").await;
    let (_, _, token) = login(&app.router).await;

    let response = get(
        &app.router,
        &format!("/Items?parentId={library}&sortBy=SortName&sortOrder=Descending"),
        Some(&token),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let result: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(result["Items"][0]["Name"], "Blade Runner");
    assert_eq!(result["Items"][1]["Name"], "Arrival");
}

#[tokio::test]
async fn items_preserve_sort_positions_and_put_null_runtime_last() {
    let app = test_app().await;
    let library = seed_library(&app.database, "Library", true).await;
    let older = seed_item(&app.database, library, "Older", "Movie").await;
    let newer = seed_item(&app.database, library, "Newer", "Movie").await;
    seed_item(&app.database, library, "Unknown runtime", "Movie").await;
    let backend = app.database.get_database_backend();
    for (item, age, runtime) in [(older, 2_i64, 100_i64), (newer, 1_i64, 200_i64)] {
        app.database
            .execute(
                backend.build(
                    Query::update()
                        .table(Alias::new("catalog_items"))
                        .values([
                            (
                                Alias::new("date_created"),
                                (Utc::now() - Duration::days(age)).into(),
                            ),
                            (Alias::new("runtime_ticks"), runtime.into()),
                        ])
                        .and_where(Expr::col(Alias::new("id")).eq(item.as_uuid())),
                ),
            )
            .await
            .unwrap();
    }
    let (_, _, token) = login(&app.router).await;

    let response = get(
        &app.router,
        &format!(
            "/Items?parentId={library}&sortBy=Unsupported,DateCreated&sortOrder=Descending,Ascending"
        ),
        Some(&token),
    )
    .await;
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let by_date: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(by_date["Items"][0]["Id"], older.to_string());

    let response = get(
        &app.router,
        &format!("/Items?parentId={library}&sortBy=Runtime&sortOrder=Descending"),
        Some(&token),
    )
    .await;
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let by_runtime: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(by_runtime["Items"][0]["Id"], newer.to_string());
    assert_eq!(by_runtime["Items"][1]["Id"], older.to_string());
    assert_eq!(by_runtime["Items"][2]["Name"], "Unknown runtime");
}

#[tokio::test]
async fn items_recursive_parent_query_returns_matching_descendants() {
    let app = test_app().await;
    let library = seed_library(&app.database, "Library", true).await;
    let folder = seed_item(&app.database, library, "Shows", "Folder").await;
    let season = seed_item(&app.database, library, "Season 1", "Season").await;
    let episode = seed_item(&app.database, library, "Pilot", "Episode").await;
    let backend = app.database.get_database_backend();
    for (child, parent) in [(season, folder), (episode, season)] {
        app.database
            .execute(
                backend.build(
                    Query::update()
                        .table(Alias::new("catalog_items"))
                        .value(Alias::new("parent_id"), parent.as_uuid())
                        .and_where(Expr::col(Alias::new("id")).eq(child.as_uuid())),
                ),
            )
            .await
            .unwrap();
    }
    let (_, _, token) = login(&app.router).await;

    let response = get(
        &app.router,
        &format!(
            "/Items?parentId={folder}&recursive=true&includeItemTypes=Episode&sortBy=SortName"
        ),
        Some(&token),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let result: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(result["TotalRecordCount"], 1);
    assert_eq!(result["Items"][0]["Id"], episode.to_string());
}

#[tokio::test]
async fn search_hints_returns_authenticated_visible_type_filtered_pages() {
    let app = test_app().await;
    let library = seed_library(&app.database, "Library", true).await;
    let disabled = seed_library(&app.database, "Disabled", false).await;
    let alpha = seed_item(&app.database, library, "Alpha", "Movie").await;
    seed_item(&app.database, library, "Alpine", "Movie").await;
    seed_item(&app.database, library, "Alpha Song", "Audio").await;
    seed_item(&app.database, disabled, "Alpha Disabled", "Movie").await;
    app.database
        .execute(
            app.database.get_database_backend().build(
                Query::update()
                    .table(Alias::new("catalog_items"))
                    .value(Alias::new("is_present"), false)
                    .and_where(Expr::col(Alias::new("id")).eq(alpha.as_uuid())),
            ),
        )
        .await
        .unwrap();
    let (user_id, _, token) = login(&app.router).await;

    let path = format!(
        "/Search/Hints?userId={user_id}&searchTerm=Alp&includeItemTypes=Movie&startIndex=0&limit=1"
    );
    let response = get(&app.router, &path, Some(&token)).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let result: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(result["TotalRecordCount"], 1);
    assert_eq!(result["StartIndex"], 0);
    assert_eq!(result["SearchHints"].as_array().unwrap().len(), 1);
    assert_eq!(result["SearchHints"][0]["Name"], "Alpine");
    assert_eq!(result["SearchHints"][0]["Type"], "Movie");
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Covers the Playlist lifecycle and stable-entry mutation contract.
async fn playlists_create_append_and_read_visible_entries_for_the_owner() {
    let app = test_app().await;
    let library = seed_library(&app.database, "Library", true).await;
    let item = seed_item(&app.database, library, "Arrival", "Movie").await;
    let (_, _, token) = login(&app.router).await;

    let response = post(
        &app.router,
        "/Playlists",
        &token,
        json!({"Name": "Road trip"}).to_string(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let playlist: Value = serde_json::from_slice(&body).unwrap();
    let playlist_id = playlist["Id"].as_str().unwrap();
    assert_eq!(playlist["Name"], "Road trip");
    assert_eq!(playlist["Type"], "Playlist");

    let response = get(&app.router, "/Playlists", Some(&token)).await;
    assert_eq!(response.status(), StatusCode::OK);
    let playlists: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(playlists["Items"].as_array().unwrap().len(), 1);
    assert_eq!(playlists["Items"][0]["Id"], playlist_id);

    assert_eq!(
        put(
            &app.router,
            &format!("/Playlists/{playlist_id}"),
            &token,
            json!({"Name": "Driving songs"}).to_string(),
        )
        .await
        .status(),
        StatusCode::NO_CONTENT
    );

    let response = post(
        &app.router,
        &format!("/Playlists/{playlist_id}/Items"),
        &token,
        json!({"ItemIds": [item, item]}).to_string(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let response = get(
        &app.router,
        &format!("/Playlists/{playlist_id}/Items"),
        Some(&token),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let entries: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(entries["Items"].as_array().unwrap().len(), 2);
    assert_eq!(entries["Items"][0]["Name"], "Arrival");
    assert_eq!(entries["Items"][1]["Name"], "Arrival");
    assert_ne!(
        entries["Items"][0]["PlaylistItemId"],
        entries["Items"][1]["PlaylistItemId"]
    );
    let first_id = entries["Items"][0]["PlaylistItemId"].as_str().unwrap();
    let second_id = entries["Items"][1]["PlaylistItemId"].as_str().unwrap();

    let response = post(
        &app.router,
        &format!("/Playlists/{playlist_id}/Items/{second_id}/Move/0"),
        &token,
        Body::empty(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let response = get(
        &app.router,
        &format!("/Playlists/{playlist_id}/Items"),
        Some(&token),
    )
    .await;
    let entries: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(entries["Items"][0]["PlaylistItemId"], second_id);

    let response = delete_empty(
        &app.router,
        &format!("/Playlists/{playlist_id}/Items/{second_id}"),
        Some(&token),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let response = get(
        &app.router,
        &format!("/Playlists/{playlist_id}/Items"),
        Some(&token),
    )
    .await;
    let entries: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(entries["Items"].as_array().unwrap().len(), 1);
    assert_eq!(entries["Items"][0]["PlaylistItemId"], first_id);

    assert_eq!(
        delete_empty(
            &app.router,
            &format!("/Playlists/{playlist_id}"),
            Some(&token)
        )
        .await
        .status(),
        StatusCode::NO_CONTENT
    );
    let response = get(&app.router, "/Playlists", Some(&token)).await;
    assert_eq!(response.status(), StatusCode::OK);
    let playlists: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert!(playlists["Items"].as_array().unwrap().is_empty());
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Covers shared Collection reads and the administrator-only write boundary.
async fn shared_collections_require_administrators_for_writes_and_allow_authenticated_reads() {
    let app = test_app().await;
    let library = seed_library(&app.database, "Library", true).await;
    let item = seed_item(&app.database, library, "Arrival", "Movie").await;
    let auth = AuthService::new(
        app.database.clone(),
        SystemClock,
        Some(Duration::days(30)),
        2,
    )
    .await
    .unwrap();
    auth.create_user("Bob", "ordinary password", false)
        .await
        .unwrap();
    let (_, _, user_token) = login_as(&app.router, "bob", "ordinary password").await;
    let (_, _, admin_token) = login(&app.router).await;

    assert_eq!(
        post(
            &app.router,
            "/Admin/Collections",
            &user_token,
            json!({"Name": "Staff picks"}).to_string(),
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    let response = post(
        &app.router,
        "/Admin/Collections",
        &admin_token,
        json!({"Name": "Staff picks"}).to_string(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let collection: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let collection_id = collection["Id"].as_str().unwrap();
    assert_eq!(collection["Type"], "Collection");

    let response = get(&app.router, "/Collections", Some(&user_token)).await;
    assert_eq!(response.status(), StatusCode::OK);
    let collections: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(collections["Items"].as_array().unwrap().len(), 1);
    assert_eq!(collections["Items"][0]["Id"], collection_id);

    assert_eq!(
        put(
            &app.router,
            &format!("/Admin/Collections/{collection_id}"),
            &user_token,
            json!({"Name": "Updated picks"}).to_string(),
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        put(
            &app.router,
            &format!("/Admin/Collections/{collection_id}"),
            &admin_token,
            json!({"Name": "Updated picks"}).to_string(),
        )
        .await
        .status(),
        StatusCode::NO_CONTENT
    );

    assert_eq!(
        post(
            &app.router,
            &format!("/Admin/Collections/{collection_id}/Items"),
            &user_token,
            json!({"ItemIds": [item]}).to_string(),
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        post(
            &app.router,
            &format!("/Admin/Collections/{collection_id}/Items"),
            &admin_token,
            json!({"ItemIds": [item]}).to_string(),
        )
        .await
        .status(),
        StatusCode::NO_CONTENT
    );
    let response = get(
        &app.router,
        &format!("/Collections/{collection_id}/Items"),
        Some(&user_token),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let entries: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(entries["Items"][0]["Name"], "Arrival");

    assert_eq!(
        delete_empty(
            &app.router,
            &format!("/Admin/Collections/{collection_id}"),
            Some(&user_token),
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        delete_empty(
            &app.router,
            &format!("/Admin/Collections/{collection_id}"),
            Some(&admin_token),
        )
        .await
        .status(),
        StatusCode::NO_CONTENT
    );
    let response = get(&app.router, "/Collections", Some(&user_token)).await;
    assert_eq!(response.status(), StatusCode::OK);
    let collections: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert!(collections["Items"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn latest_and_next_up_return_user_scoped_home_rows() {
    let app = test_app().await;
    let library = seed_library(&app.database, "Library", true).await;
    let older = seed_item(&app.database, library, "Older", "Movie").await;
    let newer = seed_item(&app.database, library, "Newer", "Movie").await;
    let series = seed_item(&app.database, library, "Series", "Series").await;
    let series_image_tag = seed_asset(&app, series, b"series-poster").await;
    let first = seed_item(&app.database, library, "S01E01", "Episode").await;
    let second = seed_item(&app.database, library, "S01E02", "Episode").await;
    let backend = app.database.get_database_backend();
    for (item, date) in [
        (older, Utc::now() - Duration::days(2)),
        (newer, Utc::now() - Duration::days(1)),
    ] {
        app.database
            .execute(
                backend.build(
                    Query::update()
                        .table(Alias::new("catalog_items"))
                        .value(Alias::new("date_created"), date)
                        .and_where(
                            sea_orm::sea_query::Expr::col(Alias::new("id")).eq(item.as_uuid()),
                        ),
                ),
            )
            .await
            .unwrap();
    }
    for episode in [first, second] {
        app.database
            .execute(
                backend.build(
                    Query::update()
                        .table(Alias::new("catalog_items"))
                        .values([
                            (Alias::new("parent_id"), series.as_uuid().into()),
                            (
                                Alias::new("structure_owner_item_id"),
                                series.as_uuid().into(),
                            ),
                        ])
                        .and_where(
                            sea_orm::sea_query::Expr::col(Alias::new("id")).eq(episode.as_uuid()),
                        ),
                ),
            )
            .await
            .unwrap();
    }
    let (user_id, _, token) = login(&app.router).await;
    assert_eq!(
        post_empty(
            &app.router,
            &format!("/Users/{user_id}/PlayedItems/{first}"),
            Some(&token),
        )
        .await
        .status(),
        StatusCode::OK
    );

    let latest = get(
        &app.router,
        &format!(
            "/Items/Latest?userId={user_id}&parentId={library}&includeItemTypes=Movie&limit=2"
        ),
        Some(&token),
    )
    .await;
    assert_eq!(latest.status(), StatusCode::OK);
    let latest: Value =
        serde_json::from_slice(&latest.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(latest[0]["Id"], newer.to_string());
    assert_eq!(latest[1]["Id"], older.to_string());

    let next_up = get(
        &app.router,
        &format!("/Shows/NextUp?userId={user_id}&seriesId={series}&limit=20"),
        Some(&token),
    )
    .await;
    assert_eq!(next_up.status(), StatusCode::OK);
    let next_up: Value =
        serde_json::from_slice(&next_up.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(next_up["TotalRecordCount"], 1);
    assert_eq!(next_up["Items"][0]["Id"], second.to_string());
    assert_eq!(next_up["Items"][0]["Type"], "Episode");
    assert_eq!(next_up["Items"][0]["Name"], "S01E02");
    assert_eq!(next_up["Items"][0]["SeriesId"], series.to_string());
    assert_eq!(next_up["Items"][0]["SeriesName"], "Series");
    assert_eq!(
        next_up["Items"][0]["SeriesPrimaryImageTag"],
        series_image_tag
    );
    assert_eq!(
        next_up["Items"][0]["ParentPrimaryImageItemId"],
        series.to_string()
    );
    assert_eq!(
        next_up["Items"][0]["ParentPrimaryImageTag"],
        series_image_tag
    );
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // The fixture expresses three TV grouping scenarios end to end.
async fn latest_tv_items_group_recent_batches_before_applying_limit() {
    let app = test_app().await;
    let library = seed_library(&app.database, "TV", true).await;
    let first_series = seed_item(&app.database, library, "The Simpsons", "Series").await;
    let first_season = seed_item(&app.database, library, "Season 1", "Season").await;
    let first_episode = seed_item(&app.database, library, "S01E01", "Episode").await;
    let second_episode = seed_item(&app.database, library, "S01E02", "Episode").await;
    let second_series = seed_item(&app.database, library, "Severance", "Series").await;
    let second_season = seed_item(&app.database, library, "Season 1", "Season").await;
    let third_episode = seed_item(&app.database, library, "S01E01", "Episode").await;
    let third_series = seed_item(&app.database, library, "Slow Horses", "Series").await;
    let third_season = seed_item(&app.database, library, "Season 1", "Season").await;
    let old_episode = seed_item(&app.database, library, "S01E01", "Episode").await;
    let latest_episode = seed_item(&app.database, library, "S01E02", "Episode").await;
    let backend = app.database.get_database_backend();
    app.database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("libraries"))
                    .value(Alias::new("collection_type"), "tvshows")
                    .and_where(Expr::col(Alias::new("id")).eq(library)),
            ),
        )
        .await
        .unwrap();
    for (season, series) in [
        (first_season, first_series),
        (second_season, second_series),
        (third_season, third_series),
    ] {
        app.database
            .execute(
                backend.build(
                    Query::update()
                        .table(Alias::new("catalog_items"))
                        .values([
                            (Alias::new("parent_id"), series.as_uuid().into()),
                            (
                                Alias::new("structure_owner_item_id"),
                                series.as_uuid().into(),
                            ),
                        ])
                        .and_where(Expr::col(Alias::new("id")).eq(season.as_uuid())),
                ),
            )
            .await
            .unwrap();
    }
    for (episode, season, series) in [
        (first_episode, first_season, first_series),
        (second_episode, first_season, first_series),
        (third_episode, second_season, second_series),
        (old_episode, third_season, third_series),
        (latest_episode, third_season, third_series),
    ] {
        app.database
            .execute(
                backend.build(
                    Query::update()
                        .table(Alias::new("catalog_items"))
                        .values([
                            (Alias::new("parent_id"), season.as_uuid().into()),
                            (
                                Alias::new("structure_owner_item_id"),
                                series.as_uuid().into(),
                            ),
                        ])
                        .and_where(Expr::col(Alias::new("id")).eq(episode.as_uuid())),
                ),
            )
            .await
            .unwrap();
    }
    let now = Utc::now();
    for (item, date) in [
        (first_episode, now),
        (second_episode, now - Duration::hours(1)),
        (third_episode, now - Duration::hours(2)),
        (latest_episode, now - Duration::hours(3)),
        (old_episode, now - Duration::hours(28)),
    ] {
        app.database
            .execute(
                backend.build(
                    Query::update()
                        .table(Alias::new("catalog_items"))
                        .value(Alias::new("date_created"), date)
                        .and_where(Expr::col(Alias::new("id")).eq(item.as_uuid())),
                ),
            )
            .await
            .unwrap();
    }

    let (user_id, _, token) = login(&app.router).await;
    let grouped = get(
        &app.router,
        &format!("/Users/{user_id}/Items/Latest?ParentId={library}&Limit=3"),
        Some(&token),
    )
    .await;
    assert_eq!(grouped.status(), StatusCode::OK);
    let grouped: Value =
        serde_json::from_slice(&grouped.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(grouped.as_array().unwrap().len(), 3);
    assert_eq!(grouped[0]["Id"], first_series.to_string());
    assert_eq!(grouped[0]["Type"], "Series");
    assert_eq!(grouped[0]["ChildCount"], 2);
    assert_eq!(grouped[1]["Id"], second_series.to_string());
    assert_eq!(grouped[1]["ChildCount"], 1);
    assert_eq!(grouped[2]["Id"], latest_episode.to_string());
    assert_eq!(grouped[2]["Type"], "Episode");
    assert!(grouped[2].get("ChildCount").is_none());

    let ungrouped = get(
        &app.router,
        &format!("/Users/{user_id}/Items/Latest?ParentId={library}&Limit=3&GroupItems=false"),
        Some(&token),
    )
    .await;
    assert_eq!(ungrouped.status(), StatusCode::OK);
    let ungrouped: Value =
        serde_json::from_slice(&ungrouped.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(ungrouped[0]["Id"], first_episode.to_string());
    assert_eq!(ungrouped[1]["Id"], second_episode.to_string());
    assert_eq!(ungrouped[2]["Id"], third_episode.to_string());
    assert!(
        ungrouped
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["Type"] == "Episode")
    );

    assert_eq!(
        post_empty(
            &app.router,
            &format!("/Users/{user_id}/PlayedItems/{first_episode}"),
            Some(&token),
        )
        .await
        .status(),
        StatusCode::OK
    );
    let played = get(
        &app.router,
        &format!(
            "/Users/{user_id}/Items/Latest?ParentId={library}&Limit=3&GroupItems=false&IsPlayed=true"
        ),
        Some(&token),
    )
    .await;
    assert_eq!(played.status(), StatusCode::OK);
    let played: Value =
        serde_json::from_slice(&played.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(played.as_array().unwrap().len(), 1);
    assert_eq!(played[0]["Id"], first_episode.to_string());
}

#[tokio::test]
async fn item_detail_requires_auth_and_returns_only_visible_catalog_items() {
    let app = test_app().await;
    let enabled = seed_library(&app.database, "Movies", true).await;
    let disabled = seed_library(&app.database, "Hidden", false).await;
    let visible = seed_item(&app.database, enabled, "Arrival", "Movie").await;
    let hidden = seed_item(&app.database, disabled, "Secret", "Movie").await;
    let sha256 = seed_asset(&app, visible, b"jpeg").await;
    let presentation = seed_playable_source(
        &app.database,
        visible,
        app.media_account,
        &app.media_object_id,
        10,
        &app.subtitle_object_id,
    )
    .await;
    let (user_id, _, token) = login(&app.router).await;

    assert_eq!(
        get(&app.router, &format!("/Items/{visible}"), None)
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let response = get(
        &app.router,
        &format!("/Items/{visible}?userId={user_id}"),
        Some(&token),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let item: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(item["Id"], visible.to_string());
    assert_eq!(item["Name"], "Arrival");
    assert_eq!(item["Type"], "Movie");
    assert_eq!(item["MediaType"], "Video");
    assert_eq!(item["ImageTags"]["Primary"], sha256);
    assert!(item["DateCreated"].is_string());
    assert_eq!(item["LocationType"], "FileSystem");
    assert_eq!(item["PrimaryImageAspectRatio"], 2.0);
    assert_eq!(item["MediaSources"][0]["Id"], visible.to_string());
    assert!(!item["MediaStreams"].as_array().unwrap().is_empty());
    assert_eq!(
        item["MediaSources"][0]["DirectStreamUrl"],
        format!("/Videos/{visible}/stream?static=true&mediaSourceId={presentation}")
    );

    for item_id in [hidden, CatalogItemId::new()] {
        assert_eq!(
            get(&app.router, &format!("/Items/{item_id}"), Some(&token))
                .await
                .status(),
            StatusCode::NOT_FOUND
        );
    }
    assert_eq!(
        get(
            &app.router,
            &format!("/Items/{visible}?userId={}", Uuid::new_v4()),
            Some(&token),
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn similar_items_require_auth_and_return_a_bounded_standard_item_page() {
    let app = test_app().await;
    let library = seed_library(&app.database, "Movies", true).await;
    let hidden_library = seed_library(&app.database, "Hidden", false).await;
    let source = seed_item(&app.database, library, "Source", "Movie").await;
    let candidate = seed_item(&app.database, library, "Candidate", "Movie").await;
    let unrelated = seed_item(&app.database, library, "Unrelated", "Movie").await;
    let unsupported = seed_item(&app.database, library, "Season", "Season").await;
    let hidden = seed_item(&app.database, hidden_library, "Hidden", "Movie").await;
    let sha256 = seed_asset(&app, candidate, b"similar-poster").await;
    add_shared_genre(&app.database, "Drama", &[source, candidate]).await;
    let (user_id, _, token) = login(&app.router).await;
    let path = format!("/Items/{source}/Similar?limit=4");

    assert_eq!(
        get(&app.router, &path, None).await.status(),
        StatusCode::UNAUTHORIZED
    );
    let response = get(&app.router, &path, Some(&token)).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["TotalRecordCount"], 1);
    assert_eq!(body["Items"][0]["Id"], candidate.to_string());
    assert_eq!(body["Items"][0]["Name"], "Candidate");
    assert_eq!(body["Items"][0]["ImageTags"]["Primary"], sha256);
    assert_ne!(body["Items"][0]["Id"], unrelated.to_string());

    for missing in [CatalogItemId::new(), hidden] {
        assert_eq!(
            get(
                &app.router,
                &format!("/Items/{missing}/Similar"),
                Some(&token),
            )
            .await
            .status(),
            StatusCode::NOT_FOUND
        );
    }
    let response = get(
        &app.router,
        &format!("/Items/{unsupported}/Similar"),
        Some(&token),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["Items"], json!([]));
    assert_eq!(body["TotalRecordCount"], 0);

    assert_eq!(
        get(
            &app.router,
            &format!("/Items/{source}/Similar?limit=4&userId={}", Uuid::new_v4()),
            Some(&token),
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    for limit in [0, 21] {
        assert_eq!(
            get(
                &app.router,
                &format!("/Items/{source}/Similar?limit={limit}&userId={user_id}"),
                Some(&token),
            )
            .await
            .status(),
            StatusCode::BAD_REQUEST
        );
    }
}

#[tokio::test]
async fn item_detail_omits_unprobed_sources_without_scheduling_probe_work() {
    let app = test_app().await;
    let library = seed_library(&app.database, "Movies", true).await;
    let item = seed_item(&app.database, library, "Arrival", "Movie").await;
    let presentation = seed_playable_source(
        &app.database,
        item,
        app.media_account,
        &app.media_object_id,
        10,
        &app.subtitle_object_id,
    )
    .await;
    let backend = app.database.get_database_backend();
    app.database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("media_sources"))
                    .value(Alias::new("probe_state"), "Pending")
                    .and_where(Expr::col(Alias::new("presentation_key")).eq(presentation)),
            ),
        )
        .await
        .unwrap();
    let (_, _, token) = login(&app.router).await;

    let response = get(&app.router, &format!("/Items/{item}"), Some(&token)).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let detail: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(detail["MediaSources"], json!([]));
    assert_eq!(detail["MediaStreams"], json!([]));
    let row = app
        .database
        .query_one(Statement::from_string(
            backend,
            "SELECT COUNT(*) AS count FROM work_jobs WHERE task_kind = 'ProbeMedia'".to_owned(),
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.try_get::<i64>("", "count").unwrap(), 0);
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Covers GET, HEAD, revalidation, and JMP image query hints together.
async fn image_get_and_head_stream_original_bytes_with_private_revalidation() {
    let app = test_app().await;
    let library = seed_library(&app.database, "Library", true).await;
    let item = seed_item(&app.database, library, "Arrival", "Movie").await;
    let sha256 = seed_asset(&app, item, b"jpeg").await;
    let (_, _, token) = login(&app.router).await;
    let path = format!("/Items/{item}/Images/Primary");

    let anonymous = get(&app.router, &path, None).await;
    assert_eq!(anonymous.status(), StatusCode::OK);
    assert_eq!(anonymous.headers()[header::CONTENT_TYPE], "image/jpeg");
    assert_eq!(
        anonymous.into_body().collect().await.unwrap().to_bytes(),
        b"jpeg"[..]
    );

    let response = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri(&path)
                .header(
                    header::AUTHORIZATION,
                    format!(r#"MediaBrowser Token="{token}""#),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CONTENT_TYPE], "image/jpeg");
    assert_eq!(response.headers()[header::CONTENT_LENGTH], "4");
    assert_eq!(response.headers()[header::ETAG], format!("\"{sha256}\""));
    assert_eq!(
        response.headers()[header::CACHE_CONTROL],
        "private, max-age=0, must-revalidate"
    );
    assert_eq!(
        response.into_body().collect().await.unwrap().to_bytes(),
        b"jpeg"[..]
    );

    let jmp_image = get(
        &app.router,
        &format!(
            "{path}?MaxWidth=480&MaxHeight=720&Quality=90&Tag={sha256}&Format=jpg&ImageIndex=0"
        ),
        Some(&token),
    )
    .await;
    assert_eq!(jmp_image.status(), StatusCode::OK);
    assert_eq!(jmp_image.headers()[header::CONTENT_TYPE], "image/jpeg");
    assert_eq!(
        jmp_image.into_body().collect().await.unwrap().to_bytes(),
        b"jpeg"[..]
    );

    let head = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method("HEAD")
                .uri(&path)
                .header(
                    header::AUTHORIZATION,
                    format!(r#"MediaBrowser Token="{token}""#),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(head.status(), StatusCode::OK);
    assert_eq!(head.headers()[header::CONTENT_LENGTH], "4");
    assert!(
        head.into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .is_empty()
    );

    let not_modified = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri(&path)
                .header(
                    header::AUTHORIZATION,
                    format!(r#"MediaBrowser Token="{token}""#),
                )
                .header(header::IF_NONE_MATCH, format!("\"{sha256}\""))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(not_modified.status(), StatusCode::NOT_MODIFIED);
    assert_eq!(
        not_modified.headers()[header::ETAG],
        format!("\"{sha256}\"")
    );
    assert!(
        not_modified
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .is_empty()
    );
}

#[tokio::test]
async fn image_route_conceals_unknown_assets_and_rejects_unsupported_inputs() {
    let app = test_app().await;
    let (_, _, token) = login(&app.router).await;
    let unknown = CatalogItemId::new();

    assert_eq!(
        get(
            &app.router,
            &format!("/Items/{unknown}/Images/Primary"),
            None,
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        get(
            &app.router,
            &format!("/Items/{unknown}/Images/Primary"),
            Some(&token),
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
    for path in [
        format!("/Items/{unknown}/Images/primary"),
        format!("/Items/{unknown}/Images/Primary?width=invalid"),
        format!("/Items/{unknown}/Images/Primary?format=tiff"),
        format!("/Items/{unknown}/Images/Primary?unexpected=1"),
        format!("/Items/{unknown}/Images/Primary?tag=a&tag=b"),
    ] {
        assert_eq!(
            get(&app.router, &path, Some(&token)).await.status(),
            StatusCode::BAD_REQUEST,
            "{path}"
        );
    }
}

#[tokio::test]
async fn browse_queries_reject_impersonation_and_invalid_pages() {
    let app = test_app().await;
    let (_, _, token) = login(&app.router).await;

    for (path, expected) in [
        (
            format!("/UserViews?userId={}", Uuid::new_v4()),
            StatusCode::FORBIDDEN,
        ),
        (
            "/UserViews?unexpected=1".to_owned(),
            StatusCode::BAD_REQUEST,
        ),
        ("/UserViews?userId=bad".to_owned(), StatusCode::BAD_REQUEST),
        ("/Items?limit=0".to_owned(), StatusCode::BAD_REQUEST),
        ("/Items?limit=201".to_owned(), StatusCode::BAD_REQUEST),
        ("/Items?limit=1&limit=2".to_owned(), StatusCode::BAD_REQUEST),
    ] {
        let response = get(&app.router, &path, Some(&token)).await;
        assert_eq!(response.status(), expected, "{path}");
    }
}

async fn post(
    router: &axum::Router,
    uri: &str,
    token: &str,
    body: impl Into<Body>,
) -> axum::response::Response {
    router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header(
                    header::AUTHORIZATION,
                    format!(r#"MediaBrowser Token="{token}""#),
                )
                .header(header::CONTENT_TYPE, "application/json")
                .body(body.into())
                .unwrap(),
        )
        .await
        .unwrap()
}

async fn put(
    router: &axum::Router,
    uri: &str,
    token: &str,
    body: impl Into<Body>,
) -> axum::response::Response {
    router
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(uri)
                .header(
                    header::AUTHORIZATION,
                    format!(r#"MediaBrowser Token="{token}""#),
                )
                .header(header::CONTENT_TYPE, "application/json")
                .body(body.into())
                .unwrap(),
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn full_capabilities_are_persisted_atomically_for_the_authenticated_session() {
    let app = test_app().await;
    let (_, session_id, token) = login(&app.router).await;
    let full = json!({
        "PlayableMediaTypes": ["Video", "Audio"],
        "SupportedCommands": ["Play", "Stop"],
        "SupportsMediaControl": true,
        "SupportsPersistentIdentifier": true,
        "DeviceProfile": {"Name": "Findroid"},
        "AppStoreUrl": "https://example.invalid/app",
        "IconUrl": "https://example.invalid/icon"
    });

    let response = post(
        &app.router,
        &format!("/Sessions/Capabilities/Full?id={session_id}"),
        &token,
        full.to_string(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .len(),
        0
    );

    let backend = app.database.get_database_backend();
    let row = app
        .database
        .query_one(
            backend.build(
                Query::select()
                    .columns([
                        Alias::new("playable_media_types"),
                        Alias::new("supported_commands"),
                        Alias::new("supports_media_control"),
                        Alias::new("supports_persistent_identifier"),
                        Alias::new("device_profile"),
                        Alias::new("app_store_url"),
                        Alias::new("icon_url"),
                    ])
                    .from(Alias::new("auth_sessions"))
                    .and_where(sea_orm::sea_query::Expr::col(Alias::new("id")).eq(session_id)),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        row.try_get::<Value>("", "playable_media_types").unwrap(),
        json!(["Video", "Audio"])
    );
    assert_eq!(
        row.try_get::<Value>("", "supported_commands").unwrap(),
        json!(["Play", "Stop"])
    );
    assert!(row.try_get::<bool>("", "supports_media_control").unwrap());
    assert!(
        row.try_get::<bool>("", "supports_persistent_identifier")
            .unwrap()
    );
    assert_eq!(
        row.try_get::<Value>("", "device_profile").unwrap(),
        json!({"Name": "Findroid"})
    );

    let malformed = post(&app.router, "/Sessions/Capabilities/Full", &token, "{").await;
    assert_eq!(malformed.status(), StatusCode::BAD_REQUEST);
    let row = app
        .database
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("device_profile"))
                    .from(Alias::new("auth_sessions"))
                    .and_where(sea_orm::sea_query::Expr::col(Alias::new("id")).eq(session_id)),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        row.try_get::<Value>("", "device_profile").unwrap(),
        json!({"Name": "Findroid"})
    );
}

#[tokio::test]
async fn legacy_capabilities_and_full_query_boundaries_are_explicit() {
    let app = test_app().await;
    let (_, _, token) = login(&app.router).await;

    let response = post(
        &app.router,
        "/Sessions/Capabilities?playableMediaTypes=Video&supportedCommands=Play,Pause&supportsMediaControl=false",
        &token,
        Body::empty(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let response = post(
        &app.router,
        &format!("/Sessions/Capabilities/Full?id={}", Uuid::new_v4()),
        &token,
        "{}",
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let response = post(
        &app.router,
        "/Sessions/Capabilities/Full?unexpected=1",
        &token,
        "{}",
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let invalid_url = "x".repeat(256);
    let response = post(
        &app.router,
        "/Sessions/Capabilities/Full",
        &token,
        json!({"AppStoreUrl": invalid_url}).to_string(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn empty_full_capabilities_persist_protocol_defaults() {
    let app = test_app().await;
    let (_, session_id, token) = login(&app.router).await;

    let response = post(&app.router, "/Sessions/Capabilities/Full", &token, "{}").await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let backend = app.database.get_database_backend();
    let row = app
        .database
        .query_one(
            backend.build(
                Query::select()
                    .columns([
                        Alias::new("playable_media_types"),
                        Alias::new("supported_commands"),
                        Alias::new("supports_media_control"),
                        Alias::new("supports_persistent_identifier"),
                        Alias::new("device_profile"),
                        Alias::new("app_store_url"),
                        Alias::new("icon_url"),
                    ])
                    .from(Alias::new("auth_sessions"))
                    .and_where(sea_orm::sea_query::Expr::col(Alias::new("id")).eq(session_id)),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        row.try_get::<Value>("", "playable_media_types").unwrap(),
        json!([])
    );
    assert_eq!(
        row.try_get::<Value>("", "supported_commands").unwrap(),
        json!([])
    );
    assert!(!row.try_get::<bool>("", "supports_media_control").unwrap());
    assert!(
        !row.try_get::<bool>("", "supports_persistent_identifier")
            .unwrap()
    );
    assert!(
        row.try_get::<Option<Value>>("", "device_profile")
            .unwrap()
            .is_none()
    );
    assert!(
        row.try_get::<Option<String>>("", "app_store_url")
            .unwrap()
            .is_none()
    );
    assert!(
        row.try_get::<Option<String>>("", "icon_url")
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn playback_info_requires_auth_and_never_invents_unprobed_sources() {
    let app = test_app().await;
    let library = seed_library(&app.database, "Movies", true).await;
    let item = seed_item(&app.database, library, "Arrival", "Movie").await;
    let (user_id, _, token) = login(&app.router).await;
    let uri = format!("/Items/{item}/PlaybackInfo");

    let response = get(&app.router, &uri, None).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let response = post(
        &app.router,
        &uri,
        &token,
        json!({
            "DeviceProfile": {
                "DirectPlayProfiles": [{"Type": "Video", "Container": "mkv,mp4"}]
            }
        })
        .to_string(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let payload: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(payload["MediaSources"], json!([]));
    assert!(Uuid::parse_str(payload["PlaySessionId"].as_str().unwrap()).is_ok());

    let response = get(
        &app.router,
        &format!("{uri}?userId={}", Uuid::new_v4()),
        Some(&token),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let response = get(
        &app.router,
        &format!("{uri}?userId={user_id}"),
        Some(&token),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn playback_info_exposes_only_stable_ids_and_local_routes() {
    let app = test_app().await;
    let library = seed_library(&app.database, "Movies", true).await;
    let item = seed_item(&app.database, library, "Arrival", "Movie").await;
    let presentation = seed_playable_source(
        &app.database,
        item,
        app.media_account,
        &app.media_object_id,
        10,
        &app.subtitle_object_id,
    )
    .await;
    let backend = app.database.get_database_backend();
    app.database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("media_sources"))
                    .values([
                        (Alias::new("edition"), "Director's Cut".into()),
                        (Alias::new("bitrate"), 8_000_000_i64.into()),
                        (Alias::new("runtime_ticks"), 72_000_000_000_i64.into()),
                        (Alias::new("is_default"), true.into()),
                    ])
                    .and_where(Expr::col(Alias::new("presentation_key")).eq(presentation)),
            ),
        )
        .await
        .unwrap();
    app.database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("publication_media_sources"))
                    .value(Alias::new("edition"), "Director's Cut")
                    .and_where(Expr::col(Alias::new("presentation_key")).eq(presentation)),
            ),
        )
        .await
        .unwrap();
    app.database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("subtitles"))
                    .value(Alias::new("is_default"), true)
                    .and_where(
                        Expr::col(Alias::new("media_source_id")).in_subquery(
                            Query::select()
                                .column(Alias::new("id"))
                                .from(Alias::new("media_sources"))
                                .and_where(
                                    Expr::col(Alias::new("presentation_key")).eq(presentation),
                                )
                                .to_owned(),
                        ),
                    ),
            ),
        )
        .await
        .unwrap();
    let (_, _, token) = login(&app.router).await;
    let response = post(
        &app.router,
        &format!("/Items/{item}/PlaybackInfo"),
        &token,
        json!({
            "DeviceProfile": {
                "DirectPlayProfiles": [{"Type": "Video"}],
                "CodecProfiles": [{
                    "Type": "Video",
                    "Conditions": [{
                        "Condition": "NotEquals",
                        "Property": "VideoRangeType",
                        "Value": "DOVI"
                    }]
                }]
            }
        })
        .to_string(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let payload: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(payload["MediaSources"].as_array().unwrap().len(), 1);
    assert_eq!(payload["MediaSources"][0]["Id"], presentation.to_string());
    assert_eq!(payload["MediaSources"][0]["Protocol"], "File");
    assert_eq!(payload["MediaSources"][0]["Name"], "Director's Cut");
    assert_eq!(payload["MediaSources"][0]["Bitrate"], 8_000_000);
    assert_eq!(
        payload["MediaSources"][0]["RunTimeTicks"],
        72_000_000_000_i64
    );
    assert_eq!(payload["MediaSources"][0]["IsDefault"], true);
    assert_eq!(payload["MediaSources"][0]["SupportsDirectPlay"], true);
    assert_eq!(payload["MediaSources"][0]["SupportsDirectStream"], true);
    assert_eq!(
        payload["MediaSources"][0]["DirectStreamUrl"],
        format!("/Videos/{item}/stream?static=true&mediaSourceId={presentation}")
    );
    assert_eq!(
        payload["MediaSources"][0]["MediaStreams"][0]["DeliveryUrl"],
        format!("/Videos/{item}/{presentation}/Subtitles/3/Stream.srt")
    );
    assert_eq!(
        payload["MediaSources"][0]["MediaStreams"][0]["IsDefault"],
        true
    );
    assert_eq!(
        payload["MediaSources"][0]["MediaStreams"][0]["IsExternalUrl"],
        true
    );
    let encoded = String::from_utf8(body.to_vec()).unwrap();
    for secret in [
        "private-drive",
        app.media_object_id.as_str(),
        "Arrival.mkv",
        "secret-ref",
    ] {
        assert!(!encoded.contains(secret));
    }
    let capabilities = post(
        &app.router,
        "/Sessions/Capabilities/Full",
        &token,
        json!({
            "DeviceProfile": {
                "DirectPlayProfiles": [{"Type": "Video", "Container": "mkv"}]
            }
        })
        .to_string(),
    )
    .await;
    assert_eq!(capabilities.status(), StatusCode::NO_CONTENT);
    let response = get(
        &app.router,
        &format!("/Items/{item}/PlaybackInfo"),
        Some(&token),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let payload: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(payload["MediaSources"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn playback_info_without_a_device_profile_returns_available_direct_play_sources() {
    let app = test_app().await;
    let library = seed_library(&app.database, "Movies", true).await;
    let item = seed_item(&app.database, library, "Arrival", "Movie").await;
    let presentation = seed_playable_source(
        &app.database,
        item,
        app.media_account,
        &app.media_object_id,
        10,
        &app.subtitle_object_id,
    )
    .await;
    let (_, _, token) = login(&app.router).await;

    let response = get(
        &app.router,
        &format!("/Items/{item}/PlaybackInfo"),
        Some(&token),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let payload: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(payload["MediaSources"].as_array().unwrap().len(), 1);
    assert_eq!(payload["MediaSources"][0]["Id"], presentation.to_string());

    let response = post(
        &app.router,
        &format!("/Items/{item}/PlaybackInfo"),
        &token,
        json!({"DeviceProfile": {"DirectPlayProfiles": []}}).to_string(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let payload: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(payload["MediaSources"].as_array().unwrap().len(), 1);
    assert_eq!(payload["MediaSources"][0]["Id"], presentation.to_string());
    assert_eq!(payload["MediaSources"][0]["SupportsDirectPlay"], false);
}

#[tokio::test]
async fn playback_info_keeps_incompatible_sources_and_evaluates_codec_conditions() {
    let app = test_app().await;
    let library = seed_library(&app.database, "Movies", true).await;
    let item = seed_item(&app.database, library, "Arrival", "Movie").await;
    let presentation = seed_playable_source(
        &app.database,
        item,
        app.media_account,
        &app.media_object_id,
        10,
        &app.subtitle_object_id,
    )
    .await;
    seed_embedded_stream(
        &app.database,
        presentation,
        "Video",
        0,
        "h264",
        Some(1920),
        Some(1080),
        None,
        Some("High"),
        Some(41),
    )
    .await;
    seed_embedded_stream(
        &app.database,
        presentation,
        "Audio",
        1,
        "aac",
        None,
        None,
        Some(2),
        None,
        None,
    )
    .await;
    let (_, _, token) = login(&app.router).await;
    let uri = format!("/Items/{item}/PlaybackInfo");

    for (video_codec, max_width, profile, max_level, expected) in [
        ("hevc", "3840", "High", "41", false),
        ("h264", "1280", "High", "41", false),
        ("h264", "1920", "Baseline", "41", false),
        ("h264", "1920", "High", "40", false),
        ("h264", "1920", "High", "41", true),
    ] {
        let response = post(
            &app.router,
            &uri,
            &token,
            json!({
                "DeviceProfile": {
                    "DirectPlayProfiles": [{
                        "Type": "Video",
                        "Container": "mkv",
                        "VideoCodec": video_codec,
                        "AudioCodec": "aac"
                    }],
                    "CodecProfiles": [{
                        "Type": "Video",
                        "Codec": "h264",
                        "Conditions": [
                            {
                                "Condition": "LessThanEqual",
                                "Property": "Width",
                                "Value": max_width,
                                "IsRequired": true
                            },
                            {
                                "Condition": "Equals",
                                "Property": "VideoProfile",
                                "Value": profile,
                                "IsRequired": true
                            },
                            {
                                "Condition": "LessThanEqual",
                                "Property": "VideoLevel",
                                "Value": max_level,
                                "IsRequired": true
                            }
                        ]
                    }]
                }
            })
            .to_string(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let payload: Value =
            serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert_eq!(payload["MediaSources"].as_array().unwrap().len(), 1);
        assert_eq!(payload["MediaSources"][0]["Id"], presentation.to_string());
        assert_eq!(payload["MediaSources"][0]["SupportsDirectPlay"], expected);
        assert_eq!(payload["MediaSources"][0]["SupportsDirectStream"], expected);
    }
}

#[tokio::test]
async fn playback_info_query_overrides_body_identity_source_and_direct_play_flags() {
    let app = test_app().await;
    let library = seed_library(&app.database, "Movies", true).await;
    let item = seed_item(&app.database, library, "Arrival", "Movie").await;
    let presentation = seed_playable_source(
        &app.database,
        item,
        app.media_account,
        &app.media_object_id,
        10,
        &app.subtitle_object_id,
    )
    .await;
    let (user_id, _, token) = login(&app.router).await;
    let wrong_user = Uuid::new_v4();
    let wrong_source = Uuid::new_v4();
    let body = json!({
        "UserId": wrong_user,
        "MediaSourceId": wrong_source,
        "EnableDirectPlay": true,
        "DeviceProfile": {
            "DirectPlayProfiles": [{"Type": "Video", "Container": "mkv"}]
        }
    })
    .to_string();

    let forbidden = post(
        &app.router,
        &format!("/Items/{item}/PlaybackInfo"),
        &token,
        body.clone(),
    )
    .await;
    assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);

    let selected = post(
        &app.router,
        &format!("/Items/{item}/PlaybackInfo?userId={user_id}&mediaSourceId={presentation}"),
        &token,
        body.clone(),
    )
    .await;
    assert_eq!(selected.status(), StatusCode::OK);
    let body_bytes = selected.into_body().collect().await.unwrap().to_bytes();
    let payload: Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(payload["MediaSources"].as_array().unwrap().len(), 1);
    assert_eq!(payload["MediaSources"][0]["Id"], presentation.to_string());

    let disabled = post(
        &app.router,
        &format!(
            "/Items/{item}/PlaybackInfo?userId={user_id}&mediaSourceId={presentation}&enableDirectPlay=false"
        ),
        &token,
        body,
    )
    .await;
    assert_eq!(disabled.status(), StatusCode::OK);
    let body = disabled.into_body().collect().await.unwrap().to_bytes();
    let payload: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(payload["MediaSources"].as_array().unwrap().len(), 1);
    assert_eq!(payload["MediaSources"][0]["Id"], presentation.to_string());
    assert_eq!(payload["MediaSources"][0]["SupportsDirectPlay"], false);
}

#[allow(clippy::too_many_lines)] // Covers GET, HEAD, range slicing, If-Range, and 416 range error semantics.
#[tokio::test]
async fn media_stream_supports_get_head_range_if_range_and_416() {
    let app = test_app().await;
    let library = seed_library(&app.database, "Movies", true).await;
    let item = seed_item(&app.database, library, "Arrival", "Movie").await;
    let presentation = seed_playable_source(
        &app.database,
        item,
        app.media_account,
        &app.media_object_id,
        10,
        &app.subtitle_object_id,
    )
    .await;
    let (_, _, token) = login(&app.router).await;
    let uri = format!("/Videos/{item}/stream?static=true&mediaSourceId={presentation}");

    let unauthorized = stream_request(&app.router, "GET", &uri, None, None, None).await;
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

    let full = stream_request(&app.router, "GET", &uri, Some(&token), None, None).await;
    assert_eq!(full.status(), StatusCode::OK);
    assert_eq!(full.headers()[header::CONTENT_TYPE], "video/x-matroska");
    assert_eq!(full.headers()[header::ACCEPT_RANGES], "bytes");
    assert_eq!(full.headers()[header::CONTENT_LENGTH], "10");
    let etag = full.headers()[header::ETAG].to_str().unwrap().to_owned();
    let body = full.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&body[..], b"0123456789");

    let jmp_uri = format!(
        "/Videos/{item}/stream?Static=true&MediaSourceId={presentation}&DeviceId=jmp-device&PlaySessionId={}&MaxStreamingBitrate=120000000&AudioStreamIndex=1&SubtitleStreamIndex=3&StartTimeTicks=10000000&Tag=fixture",
        Uuid::new_v4()
    );
    assert_eq!(
        stream_request(&app.router, "GET", &jmp_uri, Some(&token), None, None)
            .await
            .status(),
        StatusCode::OK
    );
    let jmp_container_uri = format!(
        "/Videos/{item}/stream.mkv?Static=true&MediaSourceId={presentation}&DeviceId=jmp-device"
    );
    assert_eq!(
        stream_request(
            &app.router,
            "GET",
            &jmp_container_uri,
            Some(&token),
            None,
            None,
        )
        .await
        .status(),
        StatusCode::OK
    );
    let duplicate_source = format!(
        "/Videos/{item}/stream?static=true&mediaSourceId={presentation}&MediaSourceId={presentation}"
    );
    assert_eq!(
        stream_request(
            &app.router,
            "GET",
            &duplicate_source,
            Some(&token),
            None,
            None,
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );

    let partial = stream_request(
        &app.router,
        "GET",
        &uri,
        Some(&token),
        Some("bytes=2-5"),
        Some(&etag),
    )
    .await;
    assert_eq!(partial.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(partial.headers()[header::CONTENT_RANGE], "bytes 2-5/10");
    assert_eq!(partial.headers()[header::CONTENT_LENGTH], "4");
    let body = partial.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&body[..], b"2345");

    let head = stream_request(
        &app.router,
        "HEAD",
        &uri,
        Some(&token),
        Some("bytes=-3"),
        None,
    )
    .await;
    assert_eq!(head.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(head.headers()[header::CONTENT_RANGE], "bytes 7-9/10");
    assert_eq!(head.headers()[header::CONTENT_LENGTH], "3");
    assert!(
        head.into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .is_empty()
    );

    let mismatch = stream_request(
        &app.router,
        "GET",
        &uri,
        Some(&token),
        Some("bytes=2-5"),
        Some("\"different\""),
    )
    .await;
    assert_eq!(mismatch.status(), StatusCode::OK);
    assert_eq!(mismatch.headers()[header::CONTENT_LENGTH], "10");

    let unsatisfied = stream_request(
        &app.router,
        "GET",
        &uri,
        Some(&token),
        Some("bytes=10-"),
        None,
    )
    .await;
    assert_eq!(unsatisfied.status(), StatusCode::RANGE_NOT_SATISFIABLE);
    assert_eq!(unsatisfied.headers()[header::CONTENT_RANGE], "bytes */10");

    let backend = app.database.get_database_backend();
    app.database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("storage_accounts"))
                    .value(Alias::new("status"), "Disabled")
                    .and_where(
                        sea_orm::sea_query::Expr::col(Alias::new("id")).eq(app.media_account),
                    ),
            ),
        )
        .await
        .unwrap();
    assert_eq!(
        stream_request(&app.router, "GET", &uri, Some(&token), None, None)
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn audio_stream_reuses_the_authenticated_original_byte_range_contract() {
    let app = test_app().await;
    let library = seed_library(&app.database, "Music", true).await;
    let item = seed_item(&app.database, library, "Track", "Audio").await;
    let presentation = seed_playable_source(
        &app.database,
        item,
        app.media_account,
        &app.media_object_id,
        10,
        &app.subtitle_object_id,
    )
    .await;
    let (_, _, token) = login(&app.router).await;
    let playback = post(
        &app.router,
        &format!("/Items/{item}/PlaybackInfo"),
        &token,
        "{}".to_owned(),
    )
    .await;
    assert_eq!(playback.status(), StatusCode::OK);
    let playback = playback.into_body().collect().await.unwrap().to_bytes();
    let payload: Value = serde_json::from_slice(&playback).unwrap();
    assert_eq!(
        payload["MediaSources"][0]["DirectStreamUrl"],
        format!("/Audio/{item}/stream?static=true&mediaSourceId={presentation}")
    );
    let uri = format!("/Audio/{item}/stream?static=true&mediaSourceId={presentation}");

    let unauthorized = stream_request(&app.router, "GET", &uri, None, None, None).await;
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

    let full = stream_request(&app.router, "GET", &uri, Some(&token), None, None).await;
    assert_eq!(full.status(), StatusCode::OK);
    assert_eq!(full.headers()[header::CONTENT_TYPE], "audio/x-matroska");
    assert_eq!(full.headers()[header::ACCEPT_RANGES], "bytes");
    assert_eq!(full.headers()[header::CONTENT_LENGTH], "10");
    let body = full.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&body[..], b"0123456789");

    let partial = stream_request(
        &app.router,
        "GET",
        &uri,
        Some(&token),
        Some("bytes=3-6"),
        None,
    )
    .await;
    assert_eq!(partial.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(partial.headers()[header::CONTENT_RANGE], "bytes 3-6/10");
    let body = partial.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&body[..], b"3456");

    let head = stream_request(
        &app.router,
        "HEAD",
        &uri,
        Some(&token),
        Some("bytes=-2"),
        None,
    )
    .await;
    assert_eq!(head.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(head.headers()[header::CONTENT_RANGE], "bytes 8-9/10");
    assert_eq!(head.headers()[header::CONTENT_LENGTH], "2");
    assert!(
        head.into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .is_empty()
    );
}

#[tokio::test]
async fn playback_ticket_is_scoped_revocable_and_authorizes_range_streaming() {
    let app = test_app().await;
    let library = seed_library(&app.database, "Movies", true).await;
    let item = seed_item(&app.database, library, "Arrival", "Movie").await;
    let other_item = seed_item(&app.database, library, "Dune", "Movie").await;
    let presentation = seed_playable_source(
        &app.database,
        item,
        app.media_account,
        &app.media_object_id,
        10,
        &app.subtitle_object_id,
    )
    .await;
    let (_, _, login_token) = login(&app.router).await;
    let playback = post(
        &app.router,
        &format!("/Items/{item}/PlaybackInfo"),
        &login_token,
        "{}".to_owned(),
    )
    .await;
    assert_eq!(playback.status(), StatusCode::OK);
    let playback = playback.into_body().collect().await.unwrap().to_bytes();
    let playback: Value = serde_json::from_slice(&playback).unwrap();
    let play_session_id = playback["PlaySessionId"].as_str().unwrap();

    let issued = post(
        &app.router,
        &format!("/Items/{item}/PlaybackTicket"),
        &login_token,
        json!({
            "MediaSourceId": presentation,
            "PlaySessionId": play_session_id,
        })
        .to_string(),
    )
    .await;
    assert_eq!(issued.status(), StatusCode::OK);
    assert_eq!(issued.headers()[header::CACHE_CONTROL], "no-store");
    assert_eq!(issued.headers()[header::REFERRER_POLICY], "no-referrer");
    let issued = issued.into_body().collect().await.unwrap().to_bytes();
    let issued: Value = serde_json::from_slice(&issued).unwrap();
    let ticket_id = issued["Id"].as_str().unwrap();
    let ticket = issued["Ticket"].as_str().unwrap();
    let stream_url = issued["StreamUrl"].as_str().unwrap();
    assert_eq!(ticket.len(), 64);
    assert_ne!(ticket, login_token);
    assert!(issued["ExpiresAt"].as_str().is_some());

    let partial = stream_request(
        &app.router,
        "GET",
        stream_url,
        None,
        Some("bytes=2-5"),
        None,
    )
    .await;
    assert_eq!(partial.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(partial.headers()[header::CONTENT_TYPE], "video/x-matroska");
    let body = partial.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&body[..], b"2345");

    let tampered_url = stream_url.replacen(&item.to_string(), &other_item.to_string(), 1);
    assert_eq!(
        stream_request(&app.router, "GET", &tampered_url, None, None, None)
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );

    let revoked = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/PlaybackTickets/{ticket_id}"))
                .header(header::AUTHORIZATION, token_header(&login_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(revoked.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        stream_request(&app.router, "GET", stream_url, None, None, None)
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Keeps the full cloud publication and HTTP contract together.
async fn cloud_multi_source_playback_is_complete_local_and_stable_across_reindex() {
    let app = test_app().await;
    let fixture = seed_cloud_multi_source_inventory(&app).await;
    assert_eq!(
        index_cloud_sources(&app.database, fixture.item, 1, 1).await,
        1
    );
    let indexed = cloud_presentations(&app, &fixture).await;
    probe_cloud_sources(&app, &fixture).await;
    let presentations = cloud_presentations(&app, &fixture).await;
    assert_eq!(presentations, indexed);
    assert_eq!(
        app.cloud_backend.take_ranges(),
        vec![
            (app.cloud_object_id.clone(), 0, 17),
            (app.cloud_alternate_object_id.clone(), 0, 17),
        ]
    );
    let active = tjxy_db::CatalogPublicationRepository::new(&app.database)
        .active_sources(fixture.item)
        .await
        .unwrap();
    assert_eq!(active.len(), 2);
    for source in &active {
        assert_eq!(source.probe_state(), "Probed");
        assert_eq!(source.container(), Some("mkv"));
        assert_eq!(source.streams().len(), 1);
        assert_eq!(source.streams()[0].stream_type(), "Video");
        assert_eq!(source.streams()[0].codec(), Some("h264"));
        assert_eq!(source.streams()[0].width(), Some(1920));
        assert_eq!(source.streams()[0].height(), Some(1080));
    }
    let default_source = active
        .iter()
        .find(|source| source.presentation_key().as_uuid() == presentations.default)
        .unwrap();
    assert_eq!(default_source.subtitles().len(), 1);
    assert_eq!(default_source.subtitles()[0].format(), "srt");
    assert_eq!(default_source.subtitles()[0].language(), Some("eng"));
    assert_eq!(
        default_source.subtitles()[0].storage_object_id().as_uuid(),
        fixture.subtitle_object
    );
    let alternate_source = active
        .iter()
        .find(|source| source.presentation_key().as_uuid() == presentations.alternate)
        .unwrap();
    assert!(alternate_source.subtitles().is_empty());

    let server = TcpTestServer::start(app.router.clone()).await;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("build no-redirect cloud playback client");
    let markers = cloud_leak_markers(&app, &fixture);
    let (user, token) = tcp_login(&client, &server, &markers).await;
    let policy = client
        .put(format!(
            "{}/Admin/Items/{}/MediaSources/{}/PlaybackPolicy",
            server.base_url,
            fixture.item.as_uuid(),
            presentations.default
        ))
        .header(header::AUTHORIZATION, token_header(&token))
        .json(&json!({
            "AdminPriority": 100,
            "IsDefault": true,
            "IsHidden": false
        }))
        .send()
        .await
        .expect("set cloud default playback policy");
    assert_eq!(policy.status(), StatusCode::NO_CONTENT);
    assert_headers_do_not_leak(policy.headers(), &markers, "playback policy header");
    let detail = client
        .get(format!(
            "{}/Items/{}?userId={user}",
            server.base_url,
            fixture.item.as_uuid()
        ))
        .header(header::AUTHORIZATION, token_header(&token))
        .send()
        .await
        .expect("read cloud Movie detail");
    assert_eq!(detail.status(), StatusCode::OK);
    assert_headers_do_not_leak(detail.headers(), &markers, "Movie detail header");
    let detail: Value = detail.json().await.expect("cloud Movie detail JSON");
    assert_eq!(detail["Id"], fixture.item.as_uuid().to_string());
    assert_eq!(detail["Type"], "Movie");

    let before_publication = effective_source_publication(&app.database, fixture.item).await;
    let before = read_cloud_playback(
        &client,
        &server,
        &token,
        user,
        &fixture,
        presentations,
        &markers,
    )
    .await;
    assert_cloud_delivery(&app, &client, &server, &token, &before, &markers).await;

    app.database
        .execute(
            app.database.get_database_backend().build(
                Query::update()
                    .table(Alias::new("catalog_items"))
                    .value(Alias::new("source_index_revision"), 2_i64)
                    .value(Alias::new("source_state"), "Stale")
                    .and_where(Expr::col(Alias::new("id")).eq(fixture.item.as_uuid())),
            ),
        )
        .await
        .unwrap();
    let reindex_generation = index_cloud_sources(&app.database, fixture.item, 2, 1).await;
    let after_publication = effective_source_publication(&app.database, fixture.item).await;
    assert_ne!(before_publication.0, after_publication.0);
    assert!(after_publication.1 > before_publication.1);
    assert_eq!(reindex_generation, after_publication.1);
    let reindexed_presentations = cloud_presentations(&app, &fixture).await;
    assert_eq!(reindexed_presentations, presentations);
    let active = tjxy_db::CatalogPublicationRepository::new(&app.database)
        .active_sources(fixture.item)
        .await
        .unwrap();
    let default_source = active
        .iter()
        .find(|source| source.presentation_key().as_uuid() == presentations.default)
        .unwrap();
    let alternate_source = active
        .iter()
        .find(|source| source.presentation_key().as_uuid() == presentations.alternate)
        .unwrap();
    assert!(default_source.is_default());
    assert_eq!(default_source.admin_priority(), 100);
    assert!(!default_source.is_hidden());
    assert!(!alternate_source.is_default());
    assert!(app.cloud_backend.take_ranges().is_empty());

    let after = read_cloud_playback(
        &client,
        &server,
        &token,
        user,
        &fixture,
        reindexed_presentations,
        &markers,
    )
    .await;
    assert_eq!(after, before);
    assert_cloud_delivery(&app, &client, &server, &token, &after, &markers).await;
    server.stop().await;
}

#[tokio::test]
async fn cloud_media_is_proxied_through_local_routes_without_identity_leaks() {
    let app = test_app().await;
    let library = seed_library(&app.database, "Cloud Movies", true).await;
    let item = seed_item(&app.database, library, "Remote", "Movie").await;
    let presentation = seed_playable_source_for_provider(
        &app.database,
        item,
        app.cloud_account,
        "cloud-test",
        &app.cloud_object_id,
        17,
        &app.cloud_subtitle_object_id,
    )
    .await;
    let (_, _, token) = login(&app.router).await;
    let playback = post(
        &app.router,
        &format!("/Items/{item}/PlaybackInfo"),
        &token,
        json!({
            "DeviceProfile": {
                "DirectPlayProfiles": [{"Type":"Video","Container":"mkv"}]
            }
        })
        .to_string(),
    )
    .await;
    assert_eq!(playback.status(), StatusCode::OK);
    let playback = playback.into_body().collect().await.unwrap().to_bytes();
    let payload: Value = serde_json::from_slice(&playback).unwrap();
    assert_eq!(
        payload["MediaSources"][0]["DirectStreamUrl"],
        format!("/Videos/{item}/stream?static=true&mediaSourceId={presentation}")
    );
    let encoded = String::from_utf8(playback.to_vec()).unwrap();
    for hidden in ["cloud-test", app.cloud_object_id.as_str(), "secret-ref"] {
        assert!(!encoded.contains(hidden));
    }

    let stream = stream_request(
        &app.router,
        "GET",
        &format!("/Videos/{item}/stream?static=true&mediaSourceId={presentation}"),
        Some(&token),
        Some("bytes=6-9"),
        None,
    )
    .await;
    assert_eq!(stream.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(stream.headers()[header::CONTENT_RANGE], "bytes 6-9/17");
    let body = stream.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&body[..], b"byte");

    let subtitle = get(
        &app.router,
        &format!("/Videos/{item}/{presentation}/Subtitles/3/Stream.srt"),
        Some(&token),
    )
    .await;
    assert_eq!(subtitle.status(), StatusCode::OK);
    let body = subtitle.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&body[..], b"1\n00:00:01,000 --> 00:00:02,000\nCloud\n\n\n");
}

#[tokio::test]
async fn cloud_source_probe_uses_the_registered_backend_and_a_bounded_range() {
    let app = test_app().await;
    let library = seed_library(&app.database, "Cloud probe", true).await;
    let item = seed_item(&app.database, library, "Remote probe", "Movie").await;
    let presentation = seed_playable_source_for_provider(
        &app.database,
        item,
        app.cloud_account,
        "cloud-test",
        &app.cloud_object_id,
        17,
        &app.cloud_subtitle_object_id,
    )
    .await;
    let backend = app.database.get_database_backend();
    let source_id: Uuid = app
        .database
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("id"))
                    .from(Alias::new("media_sources"))
                    .and_where(Expr::col(Alias::new("presentation_key")).eq(presentation)),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get("", "id")
        .unwrap();
    app.database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("media_sources"))
                    .value(Alias::new("probe_state"), "NotProbed")
                    .and_where(Expr::col(Alias::new("id")).eq(source_id)),
            ),
        )
        .await
        .unwrap();
    let jobs = tjxy_db::WorkJobRepository::new(&app.database);
    let submission = jobs
        .enqueue_or_join(
            &tjxy_db::WorkJobSpec::new(
                tjxy_db::WorkTaskKind::ProbeMedia,
                tjxy_db::WorkScope::MediaSource(tjxy_common::MediaSourceId::from_uuid(source_id)),
                1,
                200,
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let claimed = jobs
        .claim_next(
            &[tjxy_db::WorkTaskKind::ProbeMedia],
            "cloud-probe-test",
            Duration::minutes(1),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(claimed.id(), submission.job().id());

    ProbeService::new(app.database.clone())
        .with_backend(app.cloud_account, Arc::clone(&app.cloud_backend))
        .with_inspector(Arc::new(CloudProbeInspector))
        .execute(&claimed)
        .await
        .unwrap();

    assert_eq!(
        app.cloud_backend.ranges(),
        vec![(app.cloud_object_id.clone(), 0, 17)]
    );
    let source = app
        .database
        .query_one(
            backend.build(
                Query::select()
                    .columns([
                        Alias::new("probe_state"),
                        Alias::new("container"),
                        Alias::new("probe_revision"),
                    ])
                    .from(Alias::new("media_sources"))
                    .and_where(Expr::col(Alias::new("id")).eq(source_id)),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        source.try_get::<String>("", "probe_state").unwrap(),
        "Probed"
    );
    assert_eq!(source.try_get::<String>("", "container").unwrap(), "mkv");
    assert_eq!(source.try_get::<i64>("", "probe_revision").unwrap(), 2);
}

async fn cloud_object_availability(
    database: &DatabaseConnection,
    provider_object_id: &str,
) -> (String, Option<String>, String, i64, i64) {
    let object = Alias::new("storage_objects");
    let relation = Alias::new("storage_root_objects");
    let root = Alias::new("storage_roots");
    let location = Alias::new("media_locations");
    let query = Query::select()
        .column((relation.clone(), Alias::new("presence_state")))
        .column((relation.clone(), Alias::new("availability_reason")))
        .column((location.clone(), Alias::new("availability_state")))
        .column((root.clone(), Alias::new("sync_revision")))
        .column((root.clone(), Alias::new("reconciled_sync_revision")))
        .from(object.clone())
        .inner_join(
            relation.clone(),
            Expr::col((relation.clone(), Alias::new("storage_object_id")))
                .equals((object.clone(), Alias::new("id"))),
        )
        .inner_join(
            root.clone(),
            Expr::col((root, Alias::new("id"))).equals((relation, Alias::new("storage_root_id"))),
        )
        .inner_join(
            location.clone(),
            Expr::col((location, Alias::new("storage_object_id")))
                .equals((object.clone(), Alias::new("id"))),
        )
        .and_where(Expr::col((object, Alias::new("provider_object_id"))).eq(provider_object_id))
        .to_owned();
    let row = database
        .query_one(database.get_database_backend().build(&query))
        .await
        .unwrap()
        .unwrap();
    (
        row.try_get("", "presence_state").unwrap(),
        row.try_get("", "availability_reason").unwrap(),
        row.try_get("", "availability_state").unwrap(),
        row.try_get("", "sync_revision").unwrap(),
        row.try_get("", "reconciled_sync_revision").unwrap(),
    )
}

#[tokio::test]
async fn cloud_read_failures_update_availability_and_successful_retry_restores_it() {
    let app = test_app().await;
    let library = seed_library(&app.database, "Cloud recovery", true).await;
    let item = seed_item(&app.database, library, "Remote recovery", "Movie").await;
    let presentation = seed_playable_source_for_provider(
        &app.database,
        item,
        app.cloud_account,
        "cloud-test",
        &app.cloud_object_id,
        17,
        &app.cloud_subtitle_object_id,
    )
    .await;
    let (_, _, token) = login(&app.router).await;
    let uri = format!("/Videos/{item}/stream?static=true&mediaSourceId={presentation}");

    app.cloud_backend
        .enqueue_read(CloudReadBehavior::OpenUnavailable);
    let unavailable = stream_request(&app.router, "GET", &uri, Some(&token), None, None).await;
    assert_eq!(unavailable.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        cloud_object_availability(&app.database, &app.cloud_object_id).await,
        (
            "TemporarilyUnavailable".to_owned(),
            Some("backend-temporarily-unavailable".to_owned()),
            "TemporarilyUnavailable".to_owned(),
            2,
            2,
        )
    );

    let recovered = stream_request(&app.router, "GET", &uri, Some(&token), None, None).await;
    assert_eq!(recovered.status(), StatusCode::OK);
    assert_eq!(
        recovered.into_body().collect().await.unwrap().to_bytes(),
        Bytes::from_static(b"cloud-byte-stream")
    );
    assert_eq!(
        cloud_object_availability(&app.database, &app.cloud_object_id).await,
        ("Present".to_owned(), None, "Available".to_owned(), 3, 3)
    );

    app.cloud_backend
        .enqueue_read(CloudReadBehavior::StreamUnavailable);
    let streamed_failure = stream_request(&app.router, "GET", &uri, Some(&token), None, None).await;
    assert_eq!(streamed_failure.status(), StatusCode::OK);
    assert!(streamed_failure.into_body().collect().await.is_err());
    assert_eq!(
        cloud_object_availability(&app.database, &app.cloud_object_id).await,
        (
            "TemporarilyUnavailable".to_owned(),
            Some("backend-rate-limited".to_owned()),
            "TemporarilyUnavailable".to_owned(),
            4,
            4,
        )
    );

    app.cloud_backend
        .enqueue_read(CloudReadBehavior::StreamPendingAfterChunk);
    let cancelled = stream_request(&app.router, "GET", &uri, Some(&token), None, None).await;
    assert_eq!(cancelled.status(), StatusCode::OK);
    let mut body = cancelled.into_body();
    let frame = body.frame().await.unwrap().unwrap();
    assert_eq!(
        frame.into_data().unwrap(),
        Bytes::from_static(b"cloud-byte-stream")
    );
    drop(body);
    assert_eq!(
        cloud_object_availability(&app.database, &app.cloud_object_id).await,
        ("Present".to_owned(), None, "Available".to_owned(), 5, 5)
    );

    app.cloud_backend
        .enqueue_read(CloudReadBehavior::OpenNotFound);
    let not_found = stream_request(&app.router, "GET", &uri, Some(&token), None, None).await;
    assert_eq!(not_found.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        cloud_object_availability(&app.database, &app.cloud_object_id).await,
        (
            "TemporarilyUnavailable".to_owned(),
            Some("backend-object-not-found-unconfirmed".to_owned()),
            "TemporarilyUnavailable".to_owned(),
            6,
            6,
        )
    );
}

#[tokio::test]
async fn empty_media_rejects_byte_ranges_with_a_zero_size_content_range() {
    let app = test_app().await;
    let library = seed_library(&app.database, "Movies", true).await;
    let item = seed_item(&app.database, library, "Empty", "Movie").await;
    let presentation = seed_playable_source(
        &app.database,
        item,
        app.media_account,
        &app.empty_media_object_id,
        0,
        &app.subtitle_object_id,
    )
    .await;
    let (_, _, token) = login(&app.router).await;
    let uri = format!("/Videos/{item}/stream?static=true&mediaSourceId={presentation}");

    let full = stream_request(&app.router, "GET", &uri, Some(&token), None, None).await;
    assert_eq!(full.status(), StatusCode::OK);
    assert_eq!(full.headers()[header::CONTENT_LENGTH], "0");
    let ranged = stream_request(
        &app.router,
        "GET",
        &uri,
        Some(&token),
        Some("bytes=0-"),
        None,
    )
    .await;
    assert_eq!(ranged.status(), StatusCode::RANGE_NOT_SATISFIABLE);
    assert_eq!(ranged.headers()[header::CONTENT_RANGE], "bytes */0");
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Keeps the multi-generation Probe/index/failure race in one state sequence.
async fn probe_commit_publishes_canonical_metadata_and_never_reuses_delivery_indexes() {
    let app = test_app().await;
    let library = seed_library(&app.database, "Movies", true).await;
    let item = seed_item(&app.database, library, "Arrival", "Movie").await;
    let presentation = seed_playable_source(
        &app.database,
        item,
        app.media_account,
        &app.media_object_id,
        10,
        &app.subtitle_object_id,
    )
    .await;
    let backend = app.database.get_database_backend();
    let source_row = app
        .database
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("id"))
                    .from(Alias::new("media_sources"))
                    .and_where(
                        sea_orm::sea_query::Expr::col(Alias::new("presentation_key"))
                            .eq(presentation),
                    ),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    let source_id = tjxy_common::MediaSourceId::from_uuid(source_row.try_get("", "id").unwrap());
    app.database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("media_sources"))
                    .value(Alias::new("container"), Option::<String>::None)
                    .value(Alias::new("probe_state"), "NotProbed")
                    .and_where(
                        sea_orm::sea_query::Expr::col(Alias::new("id")).eq(source_id.as_uuid()),
                    ),
            ),
        )
        .await
        .unwrap();
    let jobs = tjxy_db::WorkJobRepository::new(&app.database);
    let submission = jobs
        .enqueue_or_join(
            &tjxy_db::WorkJobSpec::new(
                tjxy_db::WorkTaskKind::ProbeMedia,
                tjxy_db::WorkScope::MediaSource(source_id),
                1,
                200,
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let claimed = jobs
        .claim_next(
            &[tjxy_db::WorkTaskKind::ProbeMedia],
            "probe-test",
            Duration::minutes(1),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(claimed.id(), submission.job().id());
    let probes = tjxy_db::ProbeRepository::new(&app.database);
    let candidate = probes.candidate(&claimed).await.unwrap().unwrap();
    let video = tjxy_db::ProbedStream::new(
        "track-uid-7",
        "Video",
        3,
        Some("h264".to_owned()),
        None,
        Some(1920),
        Some(1080),
        None,
        true,
        false,
    )
    .unwrap()
    .with_video_compatibility(Some("High".to_owned()), Some(41))
    .unwrap();
    let result = tjxy_db::ProbeResult::new("mkv", vec![video])
        .unwrap()
        .with_video(Some("h264".to_owned()), Some("1920x1080".to_owned()));
    probes
        .commit_success(&claimed, &candidate, &result)
        .await
        .unwrap();

    let active = tjxy_db::CatalogPublicationRepository::new(&app.database)
        .active_sources(item)
        .await
        .unwrap();
    assert_eq!(active[0].container(), Some("mkv"));
    assert_eq!(active[0].probe_state(), "Probed");
    assert_eq!(active[0].streams()[0].profile(), Some("High"));
    assert_eq!(active[0].streams()[0].level(), Some(41));
    assert_eq!(active[0].subtitles()[0].delivery_index(), Some(3));

    let second = jobs
        .enqueue_or_join(
            &tjxy_db::WorkJobSpec::new(
                tjxy_db::WorkTaskKind::ProbeMedia,
                tjxy_db::WorkScope::MediaSource(source_id),
                2,
                200,
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let claimed = jobs
        .claim_next(
            &[tjxy_db::WorkTaskKind::ProbeMedia],
            "probe-test",
            Duration::minutes(1),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(claimed.id(), second.job().id());
    let candidate = probes.candidate(&claimed).await.unwrap().unwrap();
    let audio = tjxy_db::ProbedStream::new(
        "track-uid-8",
        "Audio",
        0,
        Some("aac".to_owned()),
        Some("eng".to_owned()),
        None,
        None,
        Some(2),
        true,
        false,
    )
    .unwrap();
    probes
        .commit_success(
            &claimed,
            &candidate,
            &tjxy_db::ProbeResult::new("mkv", vec![audio]).unwrap(),
        )
        .await
        .unwrap();
    let rows = app
        .database
        .query_all(
            backend.build(
                Query::select()
                    .columns([
                        Alias::new("stream_identity"),
                        Alias::new("delivery_index"),
                        Alias::new("is_present"),
                    ])
                    .from(Alias::new("media_stream_index_map"))
                    .and_where(
                        sea_orm::sea_query::Expr::col(Alias::new("media_source_id"))
                            .eq(source_id.as_uuid()),
                    )
                    .order_by(Alias::new("delivery_index"), sea_orm::sea_query::Order::Asc),
            ),
        )
        .await
        .unwrap();
    let indexes = rows
        .iter()
        .map(|row| {
            (
                row.try_get::<String>("", "stream_identity").unwrap(),
                row.try_get::<i32>("", "delivery_index").unwrap(),
                row.try_get::<bool>("", "is_present").unwrap(),
            )
        })
        .collect::<Vec<_>>();
    assert!(indexes.contains(&("embedded:track-uid-7".to_owned(), 0, false)));
    assert!(indexes.contains(&("embedded:track-uid-8".to_owned(), 1, true)));
    assert!(
        indexes
            .iter()
            .any(|row| row.0.starts_with("external:") && row.1 == 3 && row.2)
    );

    let failed = jobs
        .enqueue_or_join(
            &tjxy_db::WorkJobSpec::new(
                tjxy_db::WorkTaskKind::ProbeMedia,
                tjxy_db::WorkScope::MediaSource(source_id),
                3,
                200,
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let claimed = jobs
        .claim_next(
            &[tjxy_db::WorkTaskKind::ProbeMedia],
            "probe-test",
            Duration::minutes(1),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(claimed.id(), failed.job().id());
    let candidate = probes.candidate(&claimed).await.unwrap().unwrap();
    let failed_generation = probes
        .commit_failure(&claimed, &candidate, "unsupported media container")
        .await
        .unwrap();
    let failed_source = app
        .database
        .query_one(
            backend.build(
                Query::select()
                    .columns([
                        Alias::new("probe_state"),
                        Alias::new("probe_revision"),
                        Alias::new("last_probe_error"),
                    ])
                    .from(Alias::new("media_sources"))
                    .and_where(
                        sea_orm::sea_query::Expr::col(Alias::new("id")).eq(source_id.as_uuid()),
                    ),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        failed_source.try_get::<String>("", "probe_state").unwrap(),
        "ProbeFailed"
    );
    assert_eq!(
        failed_source.try_get::<i64>("", "probe_revision").unwrap(),
        3
    );
    assert_eq!(
        failed_source
            .try_get::<String>("", "last_probe_error")
            .unwrap(),
        "unsupported media container"
    );
    assert_eq!(
        jobs.get(failed.job().id()).await.unwrap().unwrap().state(),
        tjxy_db::WorkJobState::Failed
    );
    let outbox = app
        .database
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("generation"))
                    .from(Alias::new("catalog_change_outbox"))
                    .and_where(
                        sea_orm::sea_query::Expr::col(Alias::new("event_type")).eq("ProbeFailed"),
                    )
                    .order_by(Alias::new("generation"), sea_orm::sea_query::Order::Desc)
                    .limit(1),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        outbox.try_get::<i64>("", "generation").unwrap(),
        failed_generation
    );

    let third = jobs
        .enqueue_or_join(
            &tjxy_db::WorkJobSpec::new(
                tjxy_db::WorkTaskKind::ProbeMedia,
                tjxy_db::WorkScope::MediaSource(source_id),
                3,
                200,
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let claimed = jobs
        .claim_next(
            &[tjxy_db::WorkTaskKind::ProbeMedia],
            "probe-test",
            Duration::minutes(1),
        )
        .await
        .unwrap()
        .unwrap();
    let candidate = probes.candidate(&claimed).await.unwrap().unwrap();
    app.database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("storage_objects"))
                    .value(Alias::new("remote_revision"), "changed-during-probe")
                    .and_where(
                        sea_orm::sea_query::Expr::col(Alias::new("provider_object_id"))
                            .eq(app.media_object_id.as_str()),
                    ),
            ),
        )
        .await
        .unwrap();
    let error = probes
        .commit_success(
            &claimed,
            &candidate,
            &tjxy_db::ProbeResult::new("mkv", Vec::new()).unwrap(),
        )
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        tjxy_db::ProbeRepositoryError::StaleSnapshot
    ));
    assert_eq!(
        jobs.get(third.job().id()).await.unwrap().unwrap().state(),
        tjxy_db::WorkJobState::Running
    );
}

#[tokio::test]
async fn library_refresh_requires_an_admin_and_enqueues_enabled_libraries() {
    let app = test_app().await;
    let enabled = seed_library(&app.database, "Movies", true).await;
    seed_library(&app.database, "Archive", false).await;
    let manual = seed_library(&app.database, "Curated", true).await;
    let set_manual = Query::update()
        .table(Alias::new("libraries"))
        .value(Alias::new("scan_profile"), "Manual")
        .value(Alias::new("object_selection_scope"), "library_roots")
        .value(Alias::new("metadata_policy"), "none")
        .value(Alias::new("expansion_policy"), "manual")
        .value(Alias::new("probe_policy"), "on_playback")
        .and_where(Expr::col(Alias::new("id")).eq(manual))
        .to_owned();
    app.database
        .execute(app.database.get_database_backend().build(&set_manual))
        .await
        .unwrap();

    assert_eq!(
        post_empty(&app.router, "/Library/Refresh", None)
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let (_, _, token) = login(&app.router).await;
    assert_eq!(
        post_empty(&app.router, "/Library/Refresh", Some(&token))
            .await
            .status(),
        StatusCode::NO_CONTENT
    );

    let row = app
        .database
        .query_one(Statement::from_string(
            app.database.get_database_backend(),
            "SELECT task_kind, scope_type, scope_id, state FROM work_jobs".to_owned(),
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        row.try_get::<String>("", "task_kind").unwrap(),
        "FullMediaScan"
    );
    assert_eq!(row.try_get::<String>("", "scope_type").unwrap(), "Library");
    assert_eq!(row.try_get::<Uuid>("", "scope_id").unwrap(), enabled);
    assert_eq!(row.try_get::<String>("", "state").unwrap(), "Pending");
    let count = app
        .database
        .query_one(Statement::from_string(
            app.database.get_database_backend(),
            "SELECT COUNT(*) AS count FROM work_jobs".to_owned(),
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get::<i64>("", "count")
        .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn manual_media_tasks_require_admin_and_reject_unknown_scopes() {
    let app = test_app().await;
    let root = Uuid::new_v4();
    let item = Uuid::new_v4();
    let validate_uri = format!("/Admin/Tasks/ValidateStorage/{root}");
    let discover_uri = format!("/Admin/Tasks/DiscoverTitles/{root}");
    let metadata_uri = format!("/Admin/Tasks/ResolveMetadata/{item}");
    let expand_uri = format!("/Admin/Tasks/ExpandItem/{item}");
    let index_uri = format!("/Admin/Tasks/IndexMediaSources/{item}");
    let probe_uri = format!("/Admin/Tasks/ProbeMedia/{item}");

    assert_eq!(
        post_empty(&app.router, &validate_uri, None).await.status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        post_empty(&app.router, &discover_uri, None).await.status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        post_empty(&app.router, &metadata_uri, None).await.status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        post_empty(&app.router, &expand_uri, None).await.status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        post_empty(&app.router, &index_uri, None).await.status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        post_empty(&app.router, &probe_uri, None).await.status(),
        StatusCode::UNAUTHORIZED
    );

    let auth = AuthService::new(
        app.database.clone(),
        SystemClock,
        Some(Duration::days(30)),
        2,
    )
    .await
    .unwrap();
    auth.create_user("Bob", "ordinary password", false)
        .await
        .unwrap();
    let (_, _, user_token) = login_as(&app.router, "bob", "ordinary password").await;
    assert_eq!(
        post_empty(&app.router, &probe_uri, Some(&user_token))
            .await
            .status(),
        StatusCode::FORBIDDEN
    );

    let (_, _, token) = login(&app.router).await;
    assert_eq!(
        post_empty(&app.router, &validate_uri, Some(&token))
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        post_empty(&app.router, &discover_uri, Some(&token))
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        post_empty(&app.router, &metadata_uri, Some(&token))
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        post_empty(&app.router, &expand_uri, Some(&token))
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        post_empty(&app.router, &index_uri, Some(&token))
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        post_empty(&app.router, &probe_uri, Some(&token))
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Keeps the final-job, retry, and type-boundary HTTP contract together.
async fn manual_expand_and_index_return_the_final_durable_media_jobs() {
    let app = test_app().await;
    let library = seed_library(&app.database, "Manual", true).await;
    let series = seed_item(&app.database, library, "Example Series", "Series").await;
    let movie = seed_item(&app.database, library, "Example Movie", "Movie").await;
    let series_scope = seed_manual_storage_scope(&app.database, library, series, false).await;
    seed_manual_storage_scope(&app.database, library, movie, true).await;
    let (_, _, token) = login(&app.router).await;

    let expand = post_empty(
        &app.router,
        &format!("/Admin/Tasks/ExpandItem/{series}"),
        Some(&token),
    )
    .await;
    assert_eq!(expand.status(), StatusCode::ACCEPTED);
    let expand: Value =
        serde_json::from_slice(&expand.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let expand_job = Uuid::parse_str(expand["JobId"].as_str().unwrap()).unwrap();

    let index = post_empty(
        &app.router,
        &format!("/Admin/Tasks/IndexMediaSources/{movie}"),
        Some(&token),
    )
    .await;
    assert_eq!(index.status(), StatusCode::ACCEPTED);
    let index: Value =
        serde_json::from_slice(&index.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let index_job = Uuid::parse_str(index["JobId"].as_str().unwrap()).unwrap();
    let expand_retry = post_empty(
        &app.router,
        &format!("/Admin/Tasks/ExpandItem/{series}"),
        Some(&token),
    )
    .await;
    let expand_retry: Value =
        serde_json::from_slice(&expand_retry.into_body().collect().await.unwrap().to_bytes())
            .unwrap();
    assert_eq!(expand_retry["JobId"], expand_job.to_string());
    assert_eq!(
        post_empty(
            &app.router,
            &format!("/Admin/Tasks/ExpandItem/{movie}"),
            Some(&token),
        )
        .await
        .status(),
        StatusCode::CONFLICT
    );
    assert_eq!(
        post_empty(
            &app.router,
            &format!("/Admin/Tasks/IndexMediaSources/{series}"),
            Some(&token),
        )
        .await
        .status(),
        StatusCode::CONFLICT
    );

    let rows = app
        .database
        .query_all(Statement::from_string(
            app.database.get_database_backend(),
            "SELECT id, task_kind, scope_id, required_sync_job_id, input_sync_revision \
             FROM work_jobs ORDER BY task_kind"
                .to_owned(),
        ))
        .await
        .unwrap();
    assert_eq!(rows.len(), 3);
    let expand_row = rows
        .iter()
        .find(|row| row.try_get::<Uuid>("", "id").unwrap() == expand_job)
        .unwrap();
    assert_eq!(
        expand_row.try_get::<String>("", "task_kind").unwrap(),
        "ExpandItem"
    );
    assert_eq!(
        expand_row.try_get::<Uuid>("", "scope_id").unwrap(),
        series.as_uuid()
    );
    assert!(
        expand_row
            .try_get::<Option<Uuid>>("", "required_sync_job_id")
            .unwrap()
            .is_some()
    );
    assert_eq!(
        expand_row
            .try_get::<Option<i64>>("", "input_sync_revision")
            .unwrap(),
        None
    );
    let sync_row = rows
        .iter()
        .find(|row| row.try_get::<String>("", "task_kind").unwrap() == "ScopedStorageSync")
        .unwrap();
    assert_eq!(
        sync_row.try_get::<Uuid>("", "scope_id").unwrap(),
        series_scope
    );

    let index_row = rows
        .iter()
        .find(|row| row.try_get::<Uuid>("", "id").unwrap() == index_job)
        .unwrap();
    assert_eq!(
        index_row.try_get::<String>("", "task_kind").unwrap(),
        "IndexMediaSources"
    );
    assert_eq!(
        index_row
            .try_get::<Option<Uuid>>("", "required_sync_job_id")
            .unwrap(),
        None
    );
    assert_eq!(
        index_row
            .try_get::<Option<i64>>("", "input_sync_revision")
            .unwrap(),
        Some(3)
    );
}

#[tokio::test]
async fn manual_probe_reprobes_available_active_sources_and_joins_retries() {
    let app = test_app().await;
    let library = seed_library(&app.database, "Movies", true).await;
    let item = seed_item(&app.database, library, "Arrival", "Movie").await;
    seed_playable_source(
        &app.database,
        item,
        Uuid::new_v4(),
        "arrival-video",
        1_024,
        "arrival-subtitle",
    )
    .await;
    let (_, _, token) = login(&app.router).await;
    let uri = format!("/Admin/Tasks/ProbeMedia/{item}");

    let first = post_empty(&app.router, &uri, Some(&token)).await;
    assert_eq!(first.status(), StatusCode::ACCEPTED);
    let first_body = first.into_body().collect().await.unwrap().to_bytes();
    let first: Value = serde_json::from_slice(&first_body).unwrap();
    assert_eq!(first["Jobs"].as_array().unwrap().len(), 1);
    assert_eq!(first["Jobs"][0]["Created"], true);
    let first_job = first["Jobs"][0]["JobId"].as_str().unwrap();

    let retry = post_empty(&app.router, &uri, Some(&token)).await;
    assert_eq!(retry.status(), StatusCode::ACCEPTED);
    let retry_body = retry.into_body().collect().await.unwrap().to_bytes();
    let retry: Value = serde_json::from_slice(&retry_body).unwrap();
    assert_eq!(retry["Jobs"].as_array().unwrap().len(), 1);
    assert_eq!(retry["Jobs"][0]["Created"], false);
    assert_eq!(retry["Jobs"][0]["JobId"], first_job);

    let row = app
        .database
        .query_one(Statement::from_string(
            app.database.get_database_backend(),
            "SELECT task_kind, scope_type, scope_id, expected_revision, priority, state \
             FROM work_jobs WHERE task_kind = 'ProbeMedia'"
                .to_owned(),
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        row.try_get::<String>("", "task_kind").unwrap(),
        "ProbeMedia"
    );
    assert_eq!(
        row.try_get::<String>("", "scope_type").unwrap(),
        "MediaSource"
    );
    assert_eq!(row.try_get::<i64>("", "expected_revision").unwrap(), 1);
    assert_eq!(row.try_get::<i32>("", "priority").unwrap(), 100);
    assert_eq!(row.try_get::<String>("", "state").unwrap(), "Pending");
    assert_eq!(
        row.try_get::<Uuid>("", "scope_id").unwrap().to_string(),
        first["Jobs"][0]["MediaSourceId"]
    );
}

#[tokio::test]
async fn manual_probe_does_not_implicitly_index_an_item_without_active_sources() {
    let app = test_app().await;
    let library = seed_library(&app.database, "Movies", true).await;
    let item = seed_item(&app.database, library, "Arrival", "Movie").await;
    let (_, _, token) = login(&app.router).await;

    let response = post_empty(
        &app.router,
        &format!("/Admin/Tasks/ProbeMedia/{item}"),
        Some(&token),
    )
    .await;

    assert_eq!(response.status(), StatusCode::CONFLICT);
    let count = app
        .database
        .query_one(Statement::from_string(
            app.database.get_database_backend(),
            "SELECT COUNT(*) AS count FROM work_jobs".to_owned(),
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get::<i64>("", "count")
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Keeps live-root setup, HTTP submission, and durable job assertions together.
async fn manual_storage_validation_enqueues_a_root_scoped_job() {
    let app = test_app().await;
    let account = Uuid::new_v4();
    let root = Uuid::new_v4();
    let root_object = Uuid::new_v4();
    let root_relation = Uuid::new_v4();
    let backend = app.database.get_database_backend();
    for statement in [
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
                "Disk".into(),
                "validate-account".into(),
                "validate-ref".into(),
                "Active".into(),
            ])
            .to_owned(),
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
                "validate-root".into(),
                7_i64.into(),
                7_i64.into(),
            ])
            .to_owned(),
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
                root_object.into(),
                account.into(),
                "local".into(),
                "validate-root".into(),
                "Root".into(),
                "root".into(),
                "Directory".into(),
                7_i64.into(),
                true.into(),
                7_i64.into(),
                "ProviderStable".into(),
                "Present".into(),
            ])
            .to_owned(),
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
                root_relation.into(),
                root.into(),
                root_object.into(),
                Option::<Uuid>::None.into(),
                7_i64.into(),
                true.into(),
                7_i64.into(),
                "Present".into(),
            ])
            .to_owned(),
    ] {
        app.database
            .execute(backend.build(&statement))
            .await
            .unwrap();
    }

    let (_, _, token) = login(&app.router).await;
    let response = post_empty(
        &app.router,
        &format!("/Admin/Tasks/ValidateStorage/{root}"),
        Some(&token),
    )
    .await;
    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let row = app
        .database
        .query_one(Statement::from_string(
            app.database.get_database_backend(),
            "SELECT task_kind, scope_type, scope_id, expected_revision, state FROM work_jobs"
                .to_owned(),
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        row.try_get::<String>("", "task_kind").unwrap(),
        "ValidateStorageRoot"
    );
    assert_eq!(
        row.try_get::<String>("", "scope_type").unwrap(),
        "StorageRoot"
    );
    assert_eq!(row.try_get::<Uuid>("", "scope_id").unwrap(), root);
    assert_eq!(row.try_get::<i64>("", "expected_revision").unwrap(), 7);
    assert_eq!(row.try_get::<String>("", "state").unwrap(), "Pending");
}

#[tokio::test]
async fn manual_root_full_scan_enqueues_a_library_root_binding_job() {
    let app = test_app().await;
    let library = seed_library(&app.database, "Manual", true).await;
    let item = seed_item(&app.database, library, "Example Series", "Series").await;
    seed_manual_storage_scope(&app.database, library, item, true).await;
    let binding = app
        .database
        .query_one(
            app.database.get_database_backend().build(
                &Query::select()
                    .columns([Alias::new("id"), Alias::new("storage_root_id")])
                    .from(Alias::new("library_storage_roots"))
                    .and_where(Expr::col(Alias::new("library_id")).eq(library))
                    .limit(1)
                    .to_owned(),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    let binding_id: Uuid = binding.try_get("", "id").unwrap();
    let root_id: Uuid = binding.try_get("", "storage_root_id").unwrap();
    let (_, _, token) = login(&app.router).await;

    let response = post_empty(
        &app.router,
        &format!("/Admin/Tasks/FullScan/{library}/{root_id}"),
        Some(&token),
    )
    .await;

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let row = app
        .database
        .query_one(Statement::from_string(
            app.database.get_database_backend(),
            "SELECT task_kind, scope_type, scope_id, expected_revision FROM work_jobs".to_owned(),
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        row.try_get::<String>("", "task_kind").unwrap(),
        "FullLibraryRootScan"
    );
    assert_eq!(
        row.try_get::<String>("", "scope_type").unwrap(),
        "LibraryRootBinding"
    );
    assert_eq!(row.try_get::<Uuid>("", "scope_id").unwrap(), binding_id);
    assert_eq!(row.try_get::<i64>("", "expected_revision").unwrap(), 1);
}

#[tokio::test]
async fn scheduled_tasks_expose_start_and_cancel_for_full_library_scans() {
    let app = test_app().await;
    seed_library(&app.database, "Movies", true).await;
    assert_eq!(
        get(&app.router, "/ScheduledTasks", None).await.status(),
        StatusCode::UNAUTHORIZED
    );
    let (_, _, token) = login(&app.router).await;

    let initial = get(&app.router, "/ScheduledTasks", Some(&token)).await;
    assert_eq!(initial.status(), StatusCode::OK);
    let body = initial.into_body().collect().await.unwrap().to_bytes();
    let tasks: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(tasks.as_array().unwrap().len(), 1);
    assert_eq!(tasks[0]["Key"], "FullMediaScan");
    assert_eq!(tasks[0]["State"], "Idle");
    let task_id = tasks[0]["Id"].as_str().unwrap();

    assert_eq!(
        post_empty(
            &app.router,
            &format!("/ScheduledTasks/Running/{task_id}"),
            Some(&token),
        )
        .await
        .status(),
        StatusCode::NO_CONTENT
    );
    let running = get(
        &app.router,
        &format!("/ScheduledTasks/{task_id}"),
        Some(&token),
    )
    .await;
    let body = running.into_body().collect().await.unwrap().to_bytes();
    let running: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(running["State"], "Running");

    assert_eq!(
        delete_empty(
            &app.router,
            &format!("/ScheduledTasks/Running/{task_id}"),
            Some(&token),
        )
        .await
        .status(),
        StatusCode::NO_CONTENT
    );
    let stopped = get(
        &app.router,
        &format!("/ScheduledTasks/{task_id}"),
        Some(&token),
    )
    .await;
    let body = stopped.into_body().collect().await.unwrap().to_bytes();
    let stopped: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(stopped["State"], "Idle");
    let cancelled = app
        .database
        .query_one(Statement::from_string(
            app.database.get_database_backend(),
            "SELECT j.state, r.error_summary FROM work_jobs j JOIN work_results r ON r.job_id = j.id"
                .to_owned(),
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(cancelled.try_get::<String>("", "state").unwrap(), "Failed");
    assert_eq!(
        cancelled.try_get::<String>("", "error_summary").unwrap(),
        "cancelled by administrator"
    );
}

#[tokio::test]
async fn recent_admin_jobs_require_admin_validate_limits_and_hide_persisted_errors() {
    let app = test_app().await;
    seed_library(&app.database, "Movies", true).await;
    assert_eq!(
        get(&app.router, "/Admin/Tasks/Jobs", None).await.status(),
        StatusCode::UNAUTHORIZED
    );
    let (_, _, token) = login(&app.router).await;

    let tasks = get(&app.router, "/ScheduledTasks", Some(&token)).await;
    let body = tasks.into_body().collect().await.unwrap().to_bytes();
    let tasks: Value = serde_json::from_slice(&body).unwrap();
    let task_id = tasks[0]["Id"].as_str().unwrap();
    post_empty(
        &app.router,
        &format!("/ScheduledTasks/Running/{task_id}"),
        Some(&token),
    )
    .await;
    delete_empty(
        &app.router,
        &format!("/ScheduledTasks/Running/{task_id}"),
        Some(&token),
    )
    .await;
    let cancelled = get(&app.router, "/Admin/Tasks/Jobs?Limit=1", Some(&token)).await;
    let body = cancelled.into_body().collect().await.unwrap().to_bytes();
    let cancelled: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(cancelled[0]["Status"], "Cancelled");

    app.database
        .execute(Statement::from_string(
            app.database.get_database_backend(),
            "UPDATE work_jobs SET last_error = 'token=secret must remain private'".to_owned(),
        ))
        .await
        .unwrap();

    let response = get(&app.router, "/Admin/Tasks/Jobs?Limit=1", Some(&token)).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_text = String::from_utf8(body.to_vec()).unwrap();
    assert!(!body_text.contains("secret"));
    assert!(!body_text.contains("LastError"));
    assert!(!body_text.contains("Lease"));
    let jobs: Value = serde_json::from_str(&body_text).unwrap();
    assert_eq!(jobs.as_array().unwrap().len(), 1);
    assert_eq!(jobs[0]["TaskKind"], "FullMediaScan");
    assert_eq!(jobs[0]["ScopeType"], "Library");
    assert_eq!(jobs[0]["Status"], "Failed");
    assert!(jobs[0]["CreatedAt"].is_string());
    assert!(jobs[0]["CompletedAt"].is_string());

    for uri in [
        "/Admin/Tasks/Jobs?Limit=0",
        "/Admin/Tasks/Jobs?Limit=101",
        "/Admin/Tasks/Jobs?Limit=not-a-number",
        "/Admin/Tasks/Jobs?Unexpected=true",
    ] {
        assert_eq!(
            get(&app.router, uri, Some(&token)).await.status(),
            StatusCode::BAD_REQUEST,
            "{uri}"
        );
    }
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Keeps admin auth, secret exclusion, policy CAS, and readback in one HTTP flow.
async fn virtual_folders_require_admin_and_return_sql_effective_policy_without_backend_secrets() {
    let app = test_app().await;
    let library = seed_library(&app.database, "Movies", true).await;
    let account = Uuid::new_v4();
    let root = Uuid::new_v4();
    let backend = app.database.get_database_backend();
    app.database
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
                        "GoogleDrive".into(),
                        "Cloud".into(),
                        "private-account".into(),
                        "private-credential".into(),
                        "Ready".into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    app.database
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
                        "private-provider-root".into(),
                        0.into(),
                        0.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    app.database
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

    assert_eq!(
        get(&app.router, "/Library/VirtualFolders", None)
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let (_, _, token) = login(&app.router).await;
    let response = get(&app.router, "/Library/VirtualFolders", Some(&token)).await;
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(!body.contains("private-"));
    let folders: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(folders[0]["ItemId"], library.to_string());
    assert_eq!(folders[0]["LibraryOptions"]["ScanProfile"], "Lazy");
    assert_eq!(
        folders[0]["Locations"][0],
        format!("tjxy://storage-root/{root}")
    );

    let update = json!({
        "Id": library,
        "LibraryOptions": {
            "Enabled": false,
            "ScanProfile": "Full",
            "ProfileVersion": 1,
            "ObjectSelectionScope": "all_synced_objects",
            "MetadataPolicy": "full",
            "MetadataSourceMode": "local_only",
            "ExpansionPolicy": "manual",
            "ProbePolicy": "manual"
        }
    });
    assert_eq!(
        post(
            &app.router,
            "/Library/VirtualFolders/LibraryOptions",
            &token,
            update.to_string(),
        )
        .await
        .status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        post(
            &app.router,
            "/Library/VirtualFolders/LibraryOptions",
            &token,
            update.to_string(),
        )
        .await
        .status(),
        StatusCode::CONFLICT
    );
    let response = get(&app.router, "/Library/VirtualFolders", Some(&token)).await;
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let folders: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(folders[0]["LibraryOptions"]["Enabled"], false);
    assert_eq!(folders[0]["LibraryOptions"]["ScanProfile"], "Full");
    assert_eq!(folders[0]["LibraryOptions"]["ProfileVersion"], 2);
    assert_eq!(
        folders[0]["LibraryOptions"]["ObjectSelectionScope"],
        "all_synced_objects"
    );
    assert_eq!(folders[0]["LibraryOptions"]["MetadataPolicy"], "full");
    assert_eq!(
        folders[0]["LibraryOptions"]["MetadataSourceMode"],
        "local_only"
    );
    assert_eq!(folders[0]["LibraryOptions"]["ExpansionPolicy"], "manual");
    assert_eq!(folders[0]["LibraryOptions"]["ProbePolicy"], "manual");
}

#[tokio::test]
async fn administrator_can_create_and_delete_an_empty_virtual_folder() {
    let app = test_app().await;
    let response = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/Library/VirtualFolders")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let (_, _, token) = login(&app.router).await;

    let response = post(
        &app.router,
        "/Library/VirtualFolders?name=Documentaries&collectionType=movies&refreshLibrary=false",
        &token,
        json!({"LibraryOptions": {
            "Enabled": true,
            "ScanProfile": "Lazy",
            "MetadataSourceMode": "local_only"
        }})
        .to_string(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let response = get(&app.router, "/Library/VirtualFolders", Some(&token)).await;
    let folders: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(folders.as_array().unwrap().len(), 1);
    assert_eq!(folders[0]["Name"], "Documentaries");
    assert_eq!(folders[0]["LibraryOptions"]["ScanProfile"], "Lazy");
    assert_eq!(
        folders[0]["LibraryOptions"]["MetadataSourceMode"],
        "local_only"
    );

    let response = post(
        &app.router,
        "/Library/VirtualFolders?name=Rejected&collectionType=movies&paths=/private/media",
        &token,
        "{}",
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let response = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/Library/VirtualFolders?name=Documentaries&refreshLibrary=false")
                .header(
                    header::AUTHORIZATION,
                    format!(r#"MediaBrowser Token="{token}""#),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let response = get(&app.router, "/Library/VirtualFolders", Some(&token)).await;
    let folders: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert!(folders.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn administrator_can_persist_rename_and_safely_detach_a_filesystem_root() {
    let app = test_app().await;
    let (_, _, token) = login(&app.router).await;
    let path = app.media.path().display();
    let response = post(
        &app.router,
        &format!("/Library/VirtualFolders?name=Local&collectionType=movies&paths={path}"),
        &token,
        "{}",
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let response = get(&app.router, "/Library/VirtualFolders", Some(&token)).await;
    let folders: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let location = folders[0]["Locations"][0].as_str().unwrap();
    assert!(location.starts_with("tjxy://storage-root/"));
    assert!(!location.contains(app.media.path().to_str().unwrap()));

    assert_eq!(
        post_empty(
            &app.router,
            "/Library/VirtualFolders/Name?name=Local&newName=Renamed",
            Some(&token),
        )
        .await
        .status(),
        StatusCode::NO_CONTENT
    );
    let detached = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!(
                    "/Library/VirtualFolders/Paths?name=Renamed&path={location}"
                ))
                .header(
                    header::AUTHORIZATION,
                    format!(r#"MediaBrowser Token="{token}""#),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(detached.status(), StatusCode::NO_CONTENT);
    let status = app
        .database
        .query_one(
            app.database.get_database_backend().build(
                Query::select()
                    .column(Alias::new("status"))
                    .from(Alias::new("storage_accounts"))
                    .and_where(Expr::col(Alias::new("provider")).eq("filesystem")),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get::<String>("", "status")
        .unwrap();
    assert_eq!(status, "Disabled");
}

#[tokio::test]
async fn external_subtitles_require_auth_and_stream_only_the_indexed_format() {
    let app = test_app().await;
    let library = seed_library(&app.database, "Movies", true).await;
    let item = seed_item(&app.database, library, "Arrival", "Movie").await;
    let presentation = seed_playable_source(
        &app.database,
        item,
        app.media_account,
        &app.media_object_id,
        10,
        &app.subtitle_object_id,
    )
    .await;
    let (_, _, token) = login(&app.router).await;
    let path = format!("/Videos/{item}/{presentation}/Subtitles/3/Stream.srt");

    assert_eq!(
        get(&app.router, &path, None).await.status(),
        StatusCode::UNAUTHORIZED
    );
    let response = get(&app.router, &path, Some(&token)).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()[header::CONTENT_TYPE],
        "application/x-subrip"
    );
    assert_eq!(response.headers()[header::CONTENT_LENGTH], "40");
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&body[..], b"1\n00:00:01,000 --> 00:00:02,000\nArrival\n");

    let jmp_path = format!(
        "{path}?DeviceId=jmp-device&PlaySessionId={}&Tag=fixture",
        Uuid::new_v4()
    );
    assert_eq!(
        get(&app.router, &jmp_path, Some(&token)).await.status(),
        StatusCode::OK
    );

    let with_zero_offset = format!("/Videos/{item}/{presentation}/Subtitles/3/0/Stream.srt");
    assert_eq!(
        get(&app.router, &with_zero_offset, Some(&token))
            .await
            .status(),
        StatusCode::OK
    );
    for (path, expected) in [
        (
            format!("/Videos/{item}/{presentation}/Subtitles/3/1/Stream.srt"),
            StatusCode::BAD_REQUEST,
        ),
        (
            format!("/Videos/{item}/{presentation}/Subtitles/3/Stream.vtt"),
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
        ),
        (
            format!("/Videos/{item}/{presentation}/Subtitles/4/Stream.srt"),
            StatusCode::NOT_FOUND,
        ),
    ] {
        assert_eq!(
            get(&app.router, &path, Some(&token)).await.status(),
            expected,
            "{path}"
        );
    }
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Exercises one stateful GET/partial-POST/revision HTTP contract.
async fn user_data_get_and_post_are_authorized_patch_based_and_revisioned() {
    let app = test_app().await;
    let library = seed_library(&app.database, "Movies", true).await;
    let item = seed_item(&app.database, library, "Arrival", "Movie").await;
    let (user_id, _, token) = login(&app.router).await;
    let path = format!("/UserItems/{item}/UserData?userId={user_id}");

    assert_eq!(
        get(&app.router, &path, None).await.status(),
        StatusCode::UNAUTHORIZED
    );
    let response = get(&app.router, &path, Some(&token)).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let data: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(data["ItemId"], item.to_string());
    assert_eq!(data["IsFavorite"], false);
    assert_eq!(data["Played"], false);
    assert_eq!(data["PlayCount"], 0);
    assert_eq!(data["PlaybackPositionTicks"], 0);

    let response = post(
        &app.router,
        &path,
        &token,
        json!({
            "IsFavorite": true,
            "Played": true,
            "PlayCount": 2,
            "PlaybackPositionTicks": 900_000
        })
        .to_string(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let data: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(data["IsFavorite"], true);
    assert_eq!(data["Played"], true);
    assert_eq!(data["PlayCount"], 2);
    assert_eq!(data["PlaybackPositionTicks"], 900_000);

    let response = post(
        &app.router,
        &path,
        &token,
        json!({"IsFavorite": false}).to_string(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let data: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(data["IsFavorite"], false);
    assert_eq!(data["Played"], true);
    assert_eq!(data["PlayCount"], 2);
    assert_eq!(data["PlaybackPositionTicks"], 900_000);

    let repository = tjxy_db::UserDataRepository::new(&app.database);
    assert_eq!(
        repository
            .revision(tjxy_common::UserId::from_uuid(user_id))
            .await
            .unwrap(),
        Some(2)
    );
    assert_eq!(
        get(
            &app.router,
            &format!("/UserItems/{item}/UserData?userId={}", Uuid::new_v4()),
            Some(&token),
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );

    for invalid in [
        json!({}),
        json!({"PlaybackPositionTicks": -1}),
        json!({"Unknown": true}),
    ] {
        assert_eq!(
            post(&app.router, &path, &token, invalid.to_string())
                .await
                .status(),
            StatusCode::BAD_REQUEST
        );
    }
    assert_eq!(
        repository
            .revision(tjxy_common::UserId::from_uuid(user_id))
            .await
            .unwrap(),
        Some(2)
    );

    let hidden_library = seed_library(&app.database, "Hidden", false).await;
    let hidden = seed_item(&app.database, hidden_library, "Secret", "Movie").await;
    assert_eq!(
        get(
            &app.router,
            &format!("/UserItems/{hidden}/UserData"),
            Some(&token),
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );

    for (method, resource, expected_favorite, expected_played) in [
        ("POST", "FavoriteItems", true, true),
        ("DELETE", "FavoriteItems", false, true),
        ("POST", "PlayedItems", false, true),
        ("DELETE", "PlayedItems", false, false),
    ] {
        let response = stream_request(
            &app.router,
            method,
            &format!("/Users/{user_id}/{resource}/{item}"),
            Some(&token),
            None,
            None,
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let data: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(data["IsFavorite"], expected_favorite);
        assert_eq!(data["Played"], expected_played);
    }
    let played_with_jmp_date = stream_request(
        &app.router,
        "POST",
        &format!("/Users/{user_id}/PlayedItems/{item}?DatePlayed=2026-08-19T04%3A19%3A13.286Z"),
        Some(&token),
        None,
        None,
    )
    .await;
    assert_eq!(played_with_jmp_date.status(), StatusCode::OK);
    assert_eq!(
        stream_request(
            &app.router,
            "POST",
            &format!("/Users/{user_id}/PlayedItems/{item}?DatePlayed=invalid"),
            Some(&token),
            None,
            None,
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        repository
            .revision(tjxy_common::UserId::from_uuid(user_id))
            .await
            .unwrap(),
        Some(6)
    );
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Covers the ordered, retryable playback session lifecycle.
async fn playstate_events_are_durable_idempotent_and_revisioned_by_real_changes() {
    let app = test_app().await;
    let library = seed_library(&app.database, "Movies", true).await;
    let item = seed_item(&app.database, library, "Arrival", "Movie").await;
    let backend = app.database.get_database_backend();
    app.database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("catalog_items"))
                    .value(Alias::new("runtime_ticks"), 6_000_000_000_i64)
                    .and_where(Expr::col(Alias::new("id")).eq(item.as_uuid())),
            ),
        )
        .await
        .unwrap();
    let presentation = seed_playable_source(
        &app.database,
        item,
        app.media_account,
        &app.media_object_id,
        10,
        &app.subtitle_object_id,
    )
    .await;
    let (user_id, _, token) = login(&app.router).await;
    let play_session = Uuid::new_v4();
    let event = |position: i64| {
        json!({
            "ItemId": item,
            "MediaSourceId": presentation,
            "PlaySessionId": play_session,
            "PositionTicks": position,
            "UserId": user_id,
            "CanSeek": true,
            "PlayMethod": "DirectPlay"
        })
        .to_string()
    };
    let repository = tjxy_db::UserDataRepository::new(&app.database);
    let user = tjxy_common::UserId::from_uuid(user_id);

    for (invalid, expected) in [
        (
            json!({
                "ItemId": item,
                "MediaSourceId": Uuid::new_v4(),
                "PlaySessionId": Uuid::new_v4(),
                "PositionTicks": 0
            }),
            StatusCode::NOT_FOUND,
        ),
        (
            json!({
                "ItemId": item,
                "MediaSourceId": presentation,
                "PlaySessionId": Uuid::new_v4(),
                "PositionTicks": -1
            }),
            StatusCode::BAD_REQUEST,
        ),
        (
            json!({
                "ItemId": item,
                "MediaSourceId": presentation,
                "PlaySessionId": Uuid::new_v4(),
                "PositionTicks": 0,
                "UserId": Uuid::new_v4()
            }),
            StatusCode::FORBIDDEN,
        ),
    ] {
        assert_eq!(
            post(
                &app.router,
                "/Sessions/Playing",
                &token,
                invalid.to_string(),
            )
            .await
            .status(),
            expected
        );
    }
    assert_eq!(repository.revision(user).await.unwrap(), None);

    for _ in 0..2 {
        assert_eq!(
            post(&app.router, "/Sessions/Playing", &token, event(600_000_000),)
                .await
                .status(),
            StatusCode::NO_CONTENT
        );
    }
    let data = repository.get(user, item).await.unwrap().unwrap();
    assert_eq!(data.play_count, 1);
    assert_eq!(data.playback_position_ticks, 600_000_000);
    assert_eq!(repository.revision(user).await.unwrap(), Some(1));
    assert_eq!(
        tjxy_db::PlaystateRepository::new(&app.database)
            .last_presentation_key(user, item)
            .await
            .unwrap(),
        Some(tjxy_common::PresentationKey::from_uuid(presentation))
    );
    let response = get(
        &app.router,
        &format!("/UserItems/{item}/UserData"),
        Some(&token),
    )
    .await;
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let user_data: Value = serde_json::from_slice(&body).unwrap();
    assert!(user_data["LastPlayedDate"].as_str().is_some());

    for _ in 0..2 {
        assert_eq!(
            post(
                &app.router,
                "/Sessions/Playing/Progress",
                &token,
                event(1_200_000_000),
            )
            .await
            .status(),
            StatusCode::NO_CONTENT
        );
    }
    assert_eq!(repository.revision(user).await.unwrap(), Some(2));
    assert_eq!(
        post(
            &app.router,
            &format!("/Sessions/Playing/Ping?playSessionId={play_session}"),
            &token,
            Body::empty(),
        )
        .await
        .status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(repository.revision(user).await.unwrap(), Some(2));

    for _ in 0..2 {
        assert_eq!(
            post(
                &app.router,
                "/Sessions/Playing/Stopped",
                &token,
                event(1_800_000_000),
            )
            .await
            .status(),
            StatusCode::NO_CONTENT
        );
    }
    let data = repository.get(user, item).await.unwrap().unwrap();
    assert_eq!(data.play_count, 1);
    assert_eq!(data.playback_position_ticks, 1_800_000_000);
    assert_eq!(repository.revision(user).await.unwrap(), Some(3));

    assert_eq!(
        post(
            &app.router,
            "/Sessions/Playing/Progress",
            &token,
            event(2_400_000_000),
        )
        .await
        .status(),
        StatusCode::CONFLICT
    );
    assert_eq!(repository.revision(user).await.unwrap(), Some(3));
    assert_eq!(
        post(
            &app.router,
            &format!("/Sessions/Playing/Ping?playSessionId={play_session}"),
            &token,
            Body::empty(),
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
    let resume_path =
        format!("/UserItems/Resume?userId={user_id}&mediaTypes=Video&limit=10&enableUserData=true");
    assert_eq!(
        get(&app.router, &resume_path, None).await.status(),
        StatusCode::UNAUTHORIZED
    );
    let response = get(&app.router, &resume_path, Some(&token)).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let result: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(result["TotalRecordCount"], 1);
    assert_eq!(result["Items"][0]["Id"], item.to_string());
    assert_eq!(result["Items"][0]["RunTimeTicks"], 6_000_000_000_i64);
    assert_eq!(
        result["Items"][0]["UserData"]["PlaybackPositionTicks"],
        1_800_000_000_i64
    );
    let completed_session = Uuid::new_v4();
    assert_eq!(
        post(
            &app.router,
            "/Sessions/Playing",
            &token,
            json!({
                "ItemId": item,
                "MediaSourceId": presentation,
                "PlaySessionId": completed_session,
                "PositionTicks": 5_400_000_000_i64,
                "UserId": user_id
            })
            .to_string(),
        )
        .await
        .status(),
        StatusCode::NO_CONTENT
    );
    let data = repository.get(user, item).await.unwrap().unwrap();
    assert!(data.is_played);
    assert_eq!(data.playback_position_ticks, 0);
    let response = get(&app.router, &resume_path, Some(&token)).await;
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let result: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(result["TotalRecordCount"], 0);
}

#[tokio::test]
async fn administrator_dashboard_reports_real_catalog_playback_and_session_activity() {
    let app = test_app().await;
    let library = seed_library(&app.database, "Movies", true).await;
    let item = seed_item(&app.database, library, "Arrival", "Movie").await;
    let presentation = seed_playable_source(
        &app.database,
        item,
        app.media_account,
        &app.media_object_id,
        10,
        &app.subtitle_object_id,
    )
    .await;
    let (user_id, _, token) = login(&app.router).await;
    let play_session = Uuid::new_v4();
    assert_eq!(
        post(
            &app.router,
            "/Sessions/Playing",
            &token,
            json!({
                "ItemId": item,
                "MediaSourceId": presentation,
                "PlaySessionId": play_session,
                "PositionTicks": 250,
                "UserId": user_id,
                "CanSeek": true,
                "PlayMethod": "DirectPlay"
            })
            .to_string(),
        )
        .await
        .status(),
        StatusCode::NO_CONTENT
    );

    let now = Utc::now();
    let summary_path = format!(
        "/Admin/Dashboard/Summary?from={}&to={}&topLimit=5",
        (now - Duration::hours(1)).to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        (now + Duration::hours(1)).to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
    );
    assert_eq!(
        get(&app.router, &summary_path, None).await.status(),
        StatusCode::UNAUTHORIZED
    );
    let response = get(&app.router, &summary_path, Some(&token)).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let summary: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(summary["UsersTotal"], 1);
    assert_eq!(summary["Movies"], 1);
    assert_eq!(summary["PlayCount"], 1);
    assert_eq!(summary["UniqueViewers"], 1);
    assert_eq!(summary["CurrentlyWatching"], 1);
    assert_eq!(summary["TopItems"][0]["Name"], "Arrival");
    assert_eq!(summary["TopItems"][0]["PlayCount"], 1);

    let response = get(
        &app.router,
        "/Admin/Dashboard/NowPlaying?activeWithinSeconds=60",
        Some(&token),
    )
    .await;
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let now_playing: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(now_playing[0]["UserName"], "Alice");
    assert_eq!(now_playing[0]["ItemName"], "Arrival");

    let response = get(
        &app.router,
        "/Admin/Dashboard/LoginHistory?startIndex=0&limit=25",
        Some(&token),
    )
    .await;
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let logins: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(logins["TotalRecordCount"], 1);
    assert_eq!(logins["Items"][0]["UserName"], "Alice");

    let response = get(
        &app.router,
        "/Admin/Dashboard/WatchHistory?startIndex=0&limit=25",
        Some(&token),
    )
    .await;
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let watches: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(watches["TotalRecordCount"], 1);
    assert_eq!(watches["Items"][0]["ItemName"], "Arrival");
}

#[tokio::test]
async fn popular_fallback_includes_primary_image_tags() {
    let app = test_app().await;
    let library = seed_library(&app.database, "Movies", true).await;
    let item = seed_item(&app.database, library, "Arrival", "Movie").await;
    let poster_tag = seed_asset(&app, item, b"poster").await;
    let (_, _, token) = login(&app.router).await;

    let response = get(&app.router, "/Discover/Popular?limit=12", Some(&token)).await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let page: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(page["Items"][0]["Id"], item.to_string());
    assert_eq!(page["Items"][0]["PrimaryImageTag"], poster_tag);
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Keeps personal insights and both ranking endpoints on one fixture.
async fn client_portal_reports_personal_insights_and_discover_rankings() {
    let app = test_app().await;
    let library = seed_library(&app.database, "Movies", true).await;
    let item = seed_item(&app.database, library, "Arrival", "Movie").await;
    let poster_tag = seed_asset(&app, item, b"poster").await;
    let metadata_update = Query::update()
        .table(Alias::new("catalog_items"))
        .value(
            Alias::new("overview"),
            "A linguist learns to communicate with visitors.",
        )
        .and_where(Expr::col(Alias::new("id")).eq(item.as_uuid()))
        .to_owned();
    app.database
        .execute(app.database.get_database_backend().build(&metadata_update))
        .await
        .unwrap();
    let presentation = seed_playable_source(
        &app.database,
        item,
        app.media_account,
        &app.media_object_id,
        10,
        &app.subtitle_object_id,
    )
    .await;
    let (user_id, _, token) = login(&app.router).await;
    let play_session = Uuid::new_v4();
    assert_eq!(
        post(
            &app.router,
            "/Sessions/Playing",
            &token,
            json!({
                "ItemId": item,
                "MediaSourceId": presentation,
                "PlaySessionId": play_session,
                "PositionTicks": 0,
                "UserId": user_id,
                "CanSeek": true,
                "PlayMethod": "DirectPlay"
            })
            .to_string(),
        )
        .await
        .status(),
        StatusCode::NO_CONTENT
    );
    let yesterday = Utc::now() - Duration::days(1);
    let update = Query::update()
        .table(Alias::new("playback_sessions"))
        .value(Alias::new("watched_ticks"), 1_200_000_000_i64)
        .value(Alias::new("started_at"), yesterday)
        .value(
            Alias::new("last_event_at"),
            yesterday + Duration::minutes(2),
        )
        .value(Alias::new("stopped_at"), yesterday + Duration::minutes(2))
        .and_where(Expr::col(Alias::new("play_session_id")).eq(play_session))
        .to_owned();
    app.database
        .execute(app.database.get_database_backend().build(&update))
        .await
        .unwrap();

    let response = get(&app.router, "/Users/Me/Insights?range=all", Some(&token)).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let insights: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(insights["WatchedTicks"], 1_200_000_000_i64);
    assert_eq!(insights["PlayCount"], 1);
    assert_eq!(insights["UniqueTitles"], 1);
    assert_eq!(insights["Media"]["Movies"], 1);
    assert_eq!(insights["Recent"][0]["Name"], "Arrival");
    assert_eq!(insights["Timeline"][0]["Kind"], "MovieWatched");
    assert_eq!(insights["Timeline"][0]["Name"], "Arrival");

    let response = get(
        &app.router,
        "/Discover/Server/Top?period=yesterday&limit=20",
        Some(&token),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let ranking: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(ranking["Items"][0]["Name"], "Arrival");
    assert_eq!(ranking["Items"][0]["PlayCount"], 1);
    assert_eq!(
        ranking["Items"][0]["Overview"],
        "A linguist learns to communicate with visitors."
    );
    assert_eq!(
        ranking["Items"][0]["PosterUrl"],
        format!("/Items/{item}/Images/Primary?tag={poster_tag}")
    );
    assert_eq!(ranking["Items"][0]["PrimaryImageTag"], poster_tag);

    let dashboard_path = format!(
        "/Admin/Dashboard/Summary?from={}&to={}&topLimit=5",
        (yesterday - Duration::hours(1)).to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        (yesterday + Duration::hours(3)).to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
    );
    let backend = app.database.get_database_backend();
    app.database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("libraries"))
                    .value(Alias::new("is_enabled"), false)
                    .and_where(Expr::col(Alias::new("id")).eq(library)),
            ),
        )
        .await
        .unwrap();
    assert_visibility_sensitive_routes_hide_item(&app.router, &token, &dashboard_path).await;

    app.database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("libraries"))
                    .value(Alias::new("is_enabled"), true)
                    .and_where(Expr::col(Alias::new("id")).eq(library)),
            ),
        )
        .await
        .unwrap();
    app.database
        .execute(
            backend.build(
                Query::delete()
                    .from_table(Alias::new("library_catalog_items"))
                    .and_where(Expr::col(Alias::new("catalog_item_id")).eq(item.as_uuid())),
            ),
        )
        .await
        .unwrap();
    assert_visibility_sensitive_routes_hide_item(&app.router, &token, &dashboard_path).await;
}

#[tokio::test]
async fn personal_insights_do_not_scan_unrelated_episode_catalog() {
    let app = test_app().await;
    if app.database.get_database_backend() != DbBackend::Sqlite {
        return;
    }
    let library = seed_library(&app.database, "Large series catalog", true).await;
    let series = seed_item(&app.database, library, "Unwatched series", "Series").await;
    let season = seed_item(&app.database, library, "Season 1", "Season").await;
    app.database
        .execute(Statement::from_string(
            DbBackend::Sqlite,
            format!(
                "UPDATE catalog_items SET parent_id = X'{series}' WHERE id = X'{season}';\
                 WITH RECURSIVE sequence(number) AS (\
                   SELECT 1 UNION ALL SELECT number + 1 FROM sequence WHERE number < 100000\
                 )\
                 INSERT INTO catalog_items (\
                   id, item_type, name, sort_name, sort_key, classification_state, metadata_state,\
                   structure_state, source_state, structure_expansion_revision, source_index_revision,\
                   is_present, parent_id\
                 )\
                 SELECT unhex(printf('%032x', number)), 'Episode',\
                   printf('Episode %d', number), printf('episode %d', number), x'01', 'Matched',\
                   'Ready', 'NotApplicable', 'Indexed', 0, 0, 1, X'{season}'\
                 FROM sequence;\
                 WITH RECURSIVE sequence(number) AS (\
                   SELECT 1 UNION ALL SELECT number + 1 FROM sequence WHERE number < 100000\
                 )\
                 INSERT INTO library_catalog_items (id, library_id, catalog_item_id)\
                 SELECT unhex(printf('20%030x', number)), X'{library}', unhex(printf('%032x', number))\
                 FROM sequence;",
                series = series.as_uuid().simple(),
                season = season.as_uuid().simple(),
                library = library.simple(),
            ),
        ))
        .await
        .unwrap();
    let (_, _, token) = login(&app.router).await;

    let response = tokio::time::timeout(
        std::time::Duration::from_millis(100),
        get(&app.router, "/Users/Me/Insights?range=30d", Some(&token)),
    )
    .await
    .expect("unrelated catalog entries must not delay personal insights");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let insights: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(insights["PlayCount"], 0);
    assert_eq!(insights["Timeline"], json!([]));
}

#[tokio::test]
async fn personal_insights_keep_series_completion_events() {
    let app = test_app().await;
    let library = seed_library(&app.database, "Series", true).await;
    let series = seed_item(&app.database, library, "Completed series", "Series").await;
    let first = seed_item(&app.database, library, "Episode 1", "Episode").await;
    let second = seed_item(&app.database, library, "Episode 2", "Episode").await;
    let backend = app.database.get_database_backend();
    for episode in [first, second] {
        app.database
            .execute(
                backend.build(
                    Query::update()
                        .table(Alias::new("catalog_items"))
                        .value(Alias::new("parent_id"), series.as_uuid())
                        .and_where(Expr::col(Alias::new("id")).eq(episode.as_uuid())),
                ),
            )
            .await
            .unwrap();
    }
    let (user_id, _, token) = login(&app.router).await;
    for episode in [first, second] {
        let response = post_empty(
            &app.router,
            &format!("/Users/{user_id}/PlayedItems/{episode}"),
            Some(&token),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
    }

    let response = get(&app.router, "/Users/Me/Insights?range=today", Some(&token)).await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let insights: Value = serde_json::from_slice(&body).unwrap();
    assert!(
        insights["Timeline"]
            .as_array()
            .unwrap()
            .iter()
            .any(|event| {
                event["Kind"] == "SeriesCompleted" && event["Name"] == "Completed series"
            })
    );
}

async fn assert_visibility_sensitive_routes_hide_item(
    router: &axum::Router,
    token: &str,
    dashboard_path: &str,
) {
    let response = get(router, "/Users/Me/Insights?range=all", Some(token)).await;
    assert_eq!(response.status(), StatusCode::OK);
    let insights: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(insights["PlayCount"], 0);
    assert_eq!(insights["Recent"], json!([]));
    assert_eq!(insights["Timeline"], json!([]));

    for path in [
        "/Discover/Popular?limit=20",
        "/Discover/Server/Top?period=yesterday&limit=20",
    ] {
        let response = get(router, path, Some(token)).await;
        assert_eq!(response.status(), StatusCode::OK);
        let page: Value =
            serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert_eq!(page["Items"], json!([]), "{path}");
    }

    let response = get(router, dashboard_path, Some(token)).await;
    assert_eq!(response.status(), StatusCode::OK);
    let dashboard: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(dashboard["TopItems"], json!([]));
}

#[tokio::test]
async fn administrator_can_manage_complete_system_settings() {
    let app = test_app().await;
    let (_, _, token) = login(&app.router).await;
    let media_browser_root = app.media.path().to_string_lossy().into_owned();

    let response = get(&app.router, "/Admin/System/Settings", Some(&token)).await;
    assert_eq!(response.status(), StatusCode::OK);
    let settings: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(settings["Locale"], "zh-CN");
    assert_eq!(settings["SiteTitle"], "TJXY");
    assert_eq!(settings["ListenHost"], "127.0.0.1");
    assert_eq!(settings["Port"], 8096);
    assert_eq!(settings["MediaBrowserRoots"], json!([]));

    let response = put(
        &app.router,
        "/Admin/System/Settings",
        &token,
        json!({
            "Locale": "en-US",
            "SiteTitle": "Cinema",
            "SiteSubtitle": "Private screenings",
            "LogoUrl": "/brand/tjxy-mark.webp",
            "IconUrl": "/brand/favicon.svg",
            "PublicUrl": "https://media.example.com",
            "ListenHost": "0.0.0.0",
            "Port": 9000,
            "MediaBrowserRoots": [media_browser_root]
        })
        .to_string(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let saved: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(saved["SiteTitle"], "Cinema");
    assert_eq!(saved["Port"], 9000);
    assert_eq!(
        saved["MediaBrowserRoots"],
        json!([app.media.path().to_string_lossy()])
    );
    assert_eq!(saved["RestartRequired"], true);
    assert_eq!(saved["Revision"], 1);

    let response = get(&app.router, "/System/Settings", None).await;
    assert_eq!(response.status(), StatusCode::OK);
    let public: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert!(public.get("MediaBrowserRoots").is_none());
    assert_eq!(
        public["Theme"],
        json!({
            "Id": "classic",
            "SchemaVersion": 1,
            "Options": {},
            "Revision": 0
        })
    );
}

#[tokio::test]
async fn administrator_can_switch_site_themes_and_preserve_each_configuration() {
    let app = test_app().await;
    let (_, _, token) = login(&app.router).await;

    let response = get(&app.router, "/Admin/System/Theme", Some(&token)).await;
    assert_eq!(response.status(), StatusCode::OK);
    let initial: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(initial["ActiveThemeId"], "classic");
    assert_eq!(initial["Revision"], 0);

    let response = put(
        &app.router,
        "/Admin/System/Theme",
        &token,
        json!({
            "ThemeId": "cinema",
            "SchemaVersion": 1,
            "Options": {"Density": "compact", "Accent": "crimson"}
        })
        .to_string(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let cinema: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(cinema["ActiveThemeId"], "cinema");
    assert_eq!(cinema["Revision"], 1);

    let response = put(
        &app.router,
        "/Admin/System/Theme",
        &token,
        json!({
            "ThemeId": "classic",
            "SchemaVersion": 1,
            "Options": {"ContentWidth": "wide"},
            "Revision": 1
        })
        .to_string(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let saved: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(saved["ActiveThemeId"], "classic");
    assert_eq!(saved["Configurations"].as_array().unwrap().len(), 2);

    let response = get(&app.router, "/System/Settings", None).await;
    let public: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(public["Theme"]["Id"], "classic");
    assert_eq!(public["Theme"]["Options"], json!({"ContentWidth": "wide"}));
    assert_eq!(public["Theme"]["Revision"], 2);
}

#[tokio::test]
async fn site_theme_settings_enforce_admin_revision_and_input_validation() {
    let app = test_app().await;
    AuthService::new(
        app.database.clone(),
        SystemClock,
        Some(Duration::days(30)),
        2,
    )
    .await
    .unwrap()
    .create_user("Bob", "ordinary password", false)
    .await
    .unwrap();
    let (_, _, admin_token) = login(&app.router).await;
    let (_, _, user_token) = login_as(&app.router, "bob", "ordinary password").await;

    assert_eq!(
        get(&app.router, "/Admin/System/Theme", None).await.status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        get(&app.router, "/Admin/System/Theme", Some(&user_token))
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    let response = put(
        &app.router,
        "/Admin/System/Theme",
        &admin_token,
        json!({"ThemeId": "cinema", "SchemaVersion": 1, "Options": {}}).to_string(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    for (body, expected) in [
        (
            json!({"ThemeId": "cinema", "SchemaVersion": 1, "Options": {}, "Revision": 1}),
            StatusCode::OK,
        ),
        (
            json!({"ThemeId": "classic", "SchemaVersion": 1, "Options": {}, "Revision": 1}),
            StatusCode::CONFLICT,
        ),
        (
            json!({"ThemeId": "Invalid Theme", "SchemaVersion": 1, "Options": {}, "Revision": 2}),
            StatusCode::BAD_REQUEST,
        ),
        (
            json!({"ThemeId": "classic", "SchemaVersion": 1, "Options": [], "Revision": 2}),
            StatusCode::BAD_REQUEST,
        ),
    ] {
        assert_eq!(
            put(
                &app.router,
                "/Admin/System/Theme",
                &admin_token,
                body.to_string(),
            )
            .await
            .status(),
            expected
        );
    }

    let response = get(&app.router, "/Admin/System/Theme", Some(&admin_token)).await;
    assert_eq!(response.status(), StatusCode::OK);
    let persisted: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(persisted["ActiveThemeId"], "cinema");
    assert_eq!(persisted["Revision"], 2);
}

#[tokio::test]
async fn system_settings_reject_missing_media_browser_roots_without_advancing_revision() {
    let app = test_app().await;
    let (_, _, token) = login(&app.router).await;

    let response = put(
        &app.router,
        "/Admin/System/Settings",
        &token,
        json!({
            "Locale": "zh-CN",
            "SiteTitle": "TJXY",
            "SiteSubtitle": "Your media library",
            "LogoUrl": "/brand/tjxy-mark.webp",
            "IconUrl": "/brand/favicon.svg",
            "PublicUrl": null,
            "ListenHost": "127.0.0.1",
            "Port": 8096,
            "MediaBrowserRoots": [app.media.path().join("missing")]
        })
        .to_string(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let response = get(&app.router, "/Admin/System/Settings", Some(&token)).await;
    let settings: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(settings["Revision"], 0);
    assert_eq!(settings["MediaBrowserRoots"], json!([]));
}

#[tokio::test]
async fn system_settings_report_unavailable_persisted_media_browser_roots() {
    let app = test_app().await;
    let (_, _, token) = login(&app.router).await;
    let missing = app.media.path().join("missing");
    let settings = SystemSettingsInput {
        media_browser_roots: vec![missing.to_string_lossy().into_owned()],
        ..SystemSettingsInput::default()
    };
    SystemSettingsRepository::new(&app.database)
        .put(&settings, None)
        .await
        .unwrap();

    let response = get(&app.router, "/Admin/System/Settings", Some(&token)).await;
    assert_eq!(response.status(), StatusCode::OK);
    let settings: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();

    assert_eq!(settings["InvalidMediaBrowserRootIndexes"], json!([0]));
}

#[tokio::test]
async fn concurrent_system_settings_updates_return_one_conflict() {
    let app = test_app().await;
    let (_, _, token) = login(&app.router).await;
    let initial = json!({
        "Locale": "zh-CN",
        "SiteTitle": "Initial",
        "SiteSubtitle": "Your media library",
        "LogoUrl": "/brand/tjxy-mark.webp",
        "IconUrl": "/brand/favicon.svg",
        "PublicUrl": null,
        "ListenHost": "127.0.0.1",
        "Port": 8096,
        "MediaBrowserRoots": []
    });
    let response = put(
        &app.router,
        "/Admin/System/Settings",
        &token,
        initial.to_string(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let first = json!({
        "Locale": "zh-CN",
        "SiteTitle": "First writer",
        "SiteSubtitle": "Your media library",
        "LogoUrl": "/brand/tjxy-mark.webp",
        "IconUrl": "/brand/favicon.svg",
        "PublicUrl": null,
        "ListenHost": "127.0.0.1",
        "Port": 8096,
        "MediaBrowserRoots": [],
        "Revision": 1
    });
    let mut second = first.clone();
    second["SiteTitle"] = json!("Second writer");
    let (first_response, second_response) = tokio::join!(
        put(
            &app.router,
            "/Admin/System/Settings",
            &token,
            first.to_string(),
        ),
        put(
            &app.router,
            "/Admin/System/Settings",
            &token,
            second.to_string(),
        )
    );
    let statuses = [first_response.status(), second_response.status()];
    assert_eq!(
        statuses
            .iter()
            .filter(|status| **status == StatusCode::OK)
            .count(),
        1
    );
    assert_eq!(
        statuses
            .iter()
            .filter(|status| **status == StatusCode::CONFLICT)
            .count(),
        1
    );
}

#[tokio::test]
async fn concurrent_setup_system_settings_language_updates_map_cas_conflict_to_409() {
    let app = test_app_with_user(false).await;
    tjxy_db::SystemSettingsRepository::new(&app.database)
        .put(&tjxy_db::SystemSettingsInput::default(), None)
        .await
        .unwrap();

    let request = |locale: &str| {
        Request::builder()
            .method("PUT")
            .uri("/System/Language")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({"Locale": locale, "Revision": 1}).to_string(),
            ))
            .unwrap()
    };
    let (first_response, second_response) = tokio::join!(
        app.router.clone().oneshot(request("en-US")),
        app.router.clone().oneshot(request("zh-CN"))
    );
    let statuses = [
        first_response.unwrap().status(),
        second_response.unwrap().status(),
    ];
    assert_eq!(
        statuses
            .iter()
            .filter(|status| **status == StatusCode::OK)
            .count(),
        1
    );
    assert_eq!(
        statuses
            .iter()
            .filter(|status| **status == StatusCode::CONFLICT)
            .count(),
        1
    );
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Keeps missing and real JMP empty identity variants in one lifecycle.
async fn playstate_accepts_optional_jellyfin_identity_fields_and_derives_missing_session_state() {
    let app = test_app().await;
    let library = seed_library(&app.database, "Movies", true).await;
    let item = seed_item(&app.database, library, "Arrival", "Movie").await;
    seed_playable_source(
        &app.database,
        item,
        app.media_account,
        &app.media_object_id,
        10,
        &app.subtitle_object_id,
    )
    .await;
    let (user_id, _, token) = login(&app.router).await;
    let repository = tjxy_db::UserDataRepository::new(&app.database);
    let user = tjxy_common::UserId::from_uuid(user_id);

    for path in [
        "/Sessions/Playing",
        "/Sessions/Playing/Progress",
        "/Sessions/Playing/Stopped",
    ] {
        assert_eq!(
            post(
                &app.router,
                path,
                &token,
                json!({"CanSeek": true}).to_string()
            )
            .await
            .status(),
            StatusCode::NO_CONTENT,
            "{path} accepts schema-valid telemetry without an item identity"
        );
    }
    assert_eq!(repository.revision(user).await.unwrap(), None);

    for (path, position) in [
        ("/Sessions/Playing", 10_i64),
        ("/Sessions/Playing/Progress", 20_i64),
        ("/Sessions/Playing/Stopped", 30_i64),
    ] {
        assert_eq!(
            post(
                &app.router,
                path,
                &token,
                json!({"ItemId": item, "PositionTicks": position}).to_string(),
            )
            .await
            .status(),
            StatusCode::NO_CONTENT,
            "{path} derives the preferred source and a stable fallback session"
        );
    }
    let data = repository.get(user, item).await.unwrap().unwrap();
    assert_eq!(data.play_count, 1);
    assert_eq!(data.playback_position_ticks, 30);
    assert_eq!(repository.revision(user).await.unwrap(), Some(3));

    let second_item = seed_item(&app.database, library, "Contact", "Movie").await;
    let second_source = seed_playable_source(
        &app.database,
        second_item,
        Uuid::new_v4(),
        &format!("jmp-{}", Uuid::new_v4()),
        10,
        &format!("subtitle-{}", Uuid::new_v4()),
    )
    .await;
    let real_jmp_body = |position| {
        json!({
            "VolumeLevel": 100,
            "IsMuted": false,
            "IsPaused": false,
            "RepeatMode": "RepeatNone",
            "ShuffleMode": "Sorted",
            "MaxStreamingBitrate": 2_147_483_647_i64,
            "PositionTicks": position,
            "PlaybackRate": 1,
            "SecondarySubtitleStreamIndex": -1,
            "BufferedRanges": [],
            "PlayMethod": "DirectStream",
            "PlaySessionId": "",
            "PlaylistItemId": "playlistItem0",
            "MediaSourceId": second_source,
            "CanSeek": true,
            "ItemId": second_item,
            "NowPlayingQueue": [{"Id": second_item, "PlaylistItemId": "playlistItem0"}]
        })
        .to_string()
    };
    for (path, position) in [
        ("/Sessions/Playing", 100_i64),
        ("/Sessions/Playing/Progress", 200_i64),
        ("/Sessions/Playing/Stopped", 300_i64),
    ] {
        assert_eq!(
            post(&app.router, path, &token, real_jmp_body(position))
                .await
                .status(),
            StatusCode::NO_CONTENT,
            "{path} accepts JMP's empty optional PlaySessionId"
        );
    }
    let data = repository.get(user, second_item).await.unwrap().unwrap();
    assert_eq!(data.play_count, 1);
    assert_eq!(data.playback_position_ticks, 300);
}
