use std::{
    collections::HashSet,
    fs::{self, File},
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::Duration,
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use futures_util::StreamExt;
use reqwest::{Client, Response, StatusCode, Url, header::CACHE_CONTROL};
use sea_orm::{ConnectionTrait, Database, DbBackend, Statement};
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::time::{Instant, sleep};
use uuid::Uuid;

const IDENTITY: &str = r#"MediaBrowser Client="TJXY TCP Smoke", Device="Test", DeviceId="tcp-smoke-1", Version="0.1.0""#;
const SERVER_ID: &str = "018f17ac-4e99-7ec5-b4fd-8f15ca9f4f11";
const USERNAME: &str = "Admin";
const PASSWORD: &str = "admin-password";
const FIXTURE_MEDIA: &[u8] =
    include_bytes!("fixtures/jellyfin-smoke/Smoke Show/Season 01/Smoke Show S01E01.mp4");
const FIXTURE_SUBTITLE: &str =
    include_str!("fixtures/jellyfin-smoke/Smoke Show/Season 01/Smoke Show S01E01.srt");
const FILESYSTEM_PLAYBACK_REQUEST: &str =
    include_str!("golden/playback/filesystem-playback-info.request.json");
const FILESYSTEM_PLAYBACK_RESPONSE: &str =
    include_str!("golden/playback/filesystem-playback-info.response.json");

fn credential_keyring(active_version: i32, keys: &[(i32, u8)]) -> String {
    let keys = keys
        .iter()
        .map(|(version, byte)| (version.to_string(), json!(STANDARD.encode([*byte; 32]))))
        .collect::<serde_json::Map<_, _>>();
    json!({"active_version": active_version, "keys": keys}).to_string()
}

struct TestServer {
    child: Child,
    base_url: String,
    database_url: String,
    stdout_log: PathBuf,
    stderr_log: PathBuf,
}

#[derive(Debug)]
struct PlaybackContractSnapshot {
    source_id: String,
    direct_stream_url: String,
    external_subtitles: Vec<(i64, String)>,
}

fn normalize_filesystem_playback(playback: &mut Value, item_id: Uuid, source_id: Uuid) {
    let _play_session = playback["PlaySessionId"]
        .as_str()
        .and_then(|value| Uuid::parse_str(value).ok())
        .expect("PlaybackInfo PlaySessionId is a UUID");
    assert_eq!(
        Uuid::parse_str(
            playback["MediaSources"][0]["Id"]
                .as_str()
                .expect("source ID")
        )
        .expect("source ID is a UUID"),
        source_id,
    );
    let expected_direct = format!("/Videos/{item_id}/stream?static=true&mediaSourceId={source_id}");
    assert_eq!(
        playback["MediaSources"][0]["DirectStreamUrl"].as_str(),
        Some(expected_direct.as_str()),
    );
    playback["PlaySessionId"] = json!("{{play_session_id}}");
    playback["MediaSources"][0]["Id"] = json!("{{source_id}}");
    playback["MediaSources"][0]["DirectStreamUrl"] =
        json!("/Videos/{{item_id}}/stream?static=true&mediaSourceId={{source_id}}");
    for stream in playback["MediaSources"][0]["MediaStreams"]
        .as_array_mut()
        .expect("media stream list")
    {
        if stream["IsExternal"] == true {
            let index = stream["Index"].as_i64().expect("subtitle index");
            let expected_subtitle =
                format!("/Videos/{item_id}/{source_id}/Subtitles/{index}/Stream.srt");
            assert_eq!(
                stream["DeliveryUrl"].as_str(),
                Some(expected_subtitle.as_str())
            );
            stream["DeliveryUrl"] = json!(format!(
                "/Videos/{{{{item_id}}}}/{{{{source_id}}}}/Subtitles/{index}/Stream.srt"
            ));
        }
    }
}

impl TestServer {
    fn spawn(root: &Path) -> Self {
        let database_url = format!("sqlite://{}?mode=rwc", root.join("tjxy.db").display());
        Self::spawn_with_database(root, &database_url, None)
    }

    fn spawn_with_database(
        root: &Path,
        database_url: &str,
        credential_keyring: Option<&str>,
    ) -> Self {
        let port = available_port();
        let admin_dist = root.join("admin");
        fs::create_dir_all(&admin_dist).expect("create temporary admin directory");
        fs::write(
            admin_dist.join("index.html"),
            "<!doctype html><title>TCP smoke</title>",
        )
        .expect("write temporary admin index");
        let stdout_log = root.join(format!("server-{port}.stdout.log"));
        let stderr_log = root.join(format!("server-{port}.stderr.log"));
        let mut command = Command::new(env!("CARGO_BIN_EXE_tjxy-server"));
        command
            .env("TJXY_SERVER_ID", SERVER_ID)
            .env("TJXY_SERVER_NAME", "TJXY TCP Smoke")
            .env("TJXY_BIND", format!("127.0.0.1:{port}"))
            .env("TJXY_DATABASE_URL", database_url)
            .env("TJXY_ASSETS_DIR", root.join("assets"))
            .env("TJXY_REDIS_MODE", "disabled")
            .env("TJXY_ENABLE_REMOTE_PROVIDERS", "false")
            .env("TJXY_FILESYSTEM_REALTIME", "false")
            .env("TJXY_MEDIA_REFRESH_INTERVAL_SECONDS", "0")
            .env("TJXY_LAZY_WAIT_MS", "5000")
            .env("TJXY_BOOTSTRAP_ADMIN_USERNAME", USERNAME)
            .env("TJXY_BOOTSTRAP_ADMIN_PASSWORD", PASSWORD)
            .env("TJXY_ADMIN_DIST_DIR", admin_dist)
            .stdout(Stdio::from(
                File::create(&stdout_log).expect("create stdout log"),
            ))
            .stderr(Stdio::from(
                File::create(&stderr_log).expect("create stderr log"),
            ));
        if let Some(keyring) = credential_keyring {
            command.env("TJXY_CREDENTIAL_KEYRING", keyring);
        } else {
            command.env_remove("TJXY_CREDENTIAL_KEYRING");
        }
        let child = command.spawn().expect("start tjxy-server process");
        Self {
            child,
            base_url: format!("http://127.0.0.1:{port}"),
            database_url: database_url.to_owned(),
            stdout_log,
            stderr_log,
        }
    }

    async fn wait_ready(&mut self, client: &Client) {
        let deadline = Instant::now() + Duration::from_secs(30);
        while Instant::now() < deadline {
            if let Ok(response) = client
                .get(format!("{}/health/ready", self.base_url))
                .send()
                .await
                && response.status() == StatusCode::OK
            {
                return;
            }
            if self
                .child
                .try_wait()
                .expect("inspect server process")
                .is_some()
            {
                panic!("tjxy-server exited before becoming ready:\n{}", self.logs());
            }
            sleep(Duration::from_millis(100)).await;
        }
        panic!("tjxy-server did not become ready:\n{}", self.logs());
    }

    fn logs(&self) -> String {
        format!(
            "stdout:\n{}\nstderr:\n{}",
            fs::read_to_string(&self.stdout_log).unwrap_or_default(),
            fs::read_to_string(&self.stderr_log).unwrap_or_default(),
        )
    }

    async fn job_error(&self, job_id: &str) -> Option<String> {
        let job_id = Uuid::parse_str(job_id).expect("administrator job ID is a UUID");
        let database = Database::connect(&self.database_url)
            .await
            .expect("open smoke database for failure diagnostics");
        database
            .query_one(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                "SELECT last_error FROM work_jobs WHERE id = ?",
                [job_id.into()],
            ))
            .await
            .expect("read smoke job failure diagnostics")
            .and_then(|row| row.try_get::<Option<String>>("", "last_error").ok())
            .flatten()
    }

    async fn effective_source_publication(&self, item_id: &str) -> (Uuid, i64) {
        let item_id = Uuid::parse_str(item_id).expect("catalog item ID is a UUID");
        let database = Database::connect(&self.database_url)
            .await
            .expect("open smoke database for publication diagnostics");
        let row = database
            .query_one(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                "SELECT publication_id, activated_generation FROM (\
                     SELECT p.id AS publication_id, p.activated_generation \
                     FROM catalog_items c \
                     INNER JOIN catalog_publications p ON p.id = c.active_source_publication_id \
                     WHERE c.id = ? AND p.state = 'Active' AND p.publication_kind = 'Sources' \
                     UNION ALL \
                     SELECT p.id AS publication_id, p.activated_generation \
                     FROM catalog_items c \
                     INNER JOIN catalog_items owner ON owner.id = c.structure_owner_item_id \
                     INNER JOIN catalog_publications p ON p.id = owner.active_structure_publication_id \
                     WHERE c.id = ? AND p.state = 'Active' AND p.publication_kind = 'Structure'\
                 ) candidates ORDER BY activated_generation DESC LIMIT 1",
                [item_id.into(), item_id.into()],
            ))
            .await
            .expect("read effective source publication")
            .expect("catalog item has an effective source publication");
        (
            row.try_get("", "publication_id")
                .expect("effective source publication ID"),
            row.try_get("", "activated_generation")
                .expect("effective source publication generation"),
        )
    }

    fn stop(mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

async fn create_filesystem_library(
    client: &Client,
    server: &TestServer,
    token: &str,
    name: &str,
    media_root: &Path,
    scan_profile: &str,
) {
    let mut create_folder = Url::parse(&format!("{}/Library/VirtualFolders", server.base_url))
        .expect("build virtual-folder URL");
    create_folder
        .query_pairs_mut()
        .append_pair("name", name)
        .append_pair("collectionType", "tvshows")
        .append_pair("paths", media_root.to_str().expect("fixture path is UTF-8"))
        .append_pair("refreshLibrary", "false");
    assert_status(
        client
            .post(create_folder)
            .header("Authorization", token_header(token))
            .json(&json!({"LibraryOptions": {"Enabled": true, "ScanProfile": scan_profile}}))
            .send()
            .await
            .expect("create filesystem virtual folder request"),
        StatusCode::NO_CONTENT,
        "create filesystem virtual folder",
    )
    .await;
}

async fn library_id_by_name(
    client: &Client,
    server: &TestServer,
    token: &str,
    user_id: &str,
    name: &str,
) -> String {
    let views = json_response(
        client
            .get(format!("{}/UserViews?userId={user_id}", server.base_url))
            .header("Authorization", token_header(token))
            .send()
            .await
            .expect("read user views request"),
        StatusCode::OK,
        "read persisted virtual folder after restart",
    )
    .await;
    views["Items"]
        .as_array()
        .and_then(|items| items.iter().find(|item| item["Name"] == name))
        .and_then(|item| item["Id"].as_str())
        .expect("persisted filesystem library is visible")
        .to_owned()
}

async fn storage_root_id_by_library_name(
    client: &Client,
    server: &TestServer,
    token: &str,
    name: &str,
) -> String {
    let folders = json_response(
        client
            .get(format!("{}/Library/VirtualFolders", server.base_url))
            .header("Authorization", token_header(token))
            .send()
            .await
            .expect("read virtual folders request"),
        StatusCode::OK,
        "read persisted virtual folders",
    )
    .await;
    folders
        .as_array()
        .and_then(|folders| folders.iter().find(|folder| folder["Name"] == name))
        .and_then(|folder| folder["Locations"].as_array())
        .and_then(|locations| locations.first())
        .and_then(Value::as_str)
        .and_then(|location| location.strip_prefix("tjxy://storage-root/"))
        .expect("persisted filesystem root uses an opaque storage-root location")
        .to_owned()
}

#[tokio::test]
async fn tcp_new_filesystem_binding_completes_initial_sync_without_restart() {
    let temp = tempfile::tempdir().expect("create temporary test directory");
    let media_root = copy_media_fixture(&temp);
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .expect("build HTTP client");
    let mut server = TestServer::spawn(temp.path());
    server.wait_ready(&client).await;
    let (_, token) = authenticate(&client, &server).await;

    create_filesystem_library(
        &client,
        &server,
        &token,
        "Hot Storage Smoke TV",
        &media_root,
        "Manual",
    )
    .await;
    wait_for_job(
        &client,
        &server,
        &token,
        None,
        "ScopedStorageSync",
        None,
        50,
        "hot-activated filesystem initial sync",
    )
    .await;
}

#[tokio::test]
async fn tcp_catalog_generation_notifies_authenticated_websocket() {
    let temp = tempfile::tempdir().expect("create temporary test directory");
    let media_root = copy_media_fixture(&temp);
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .expect("build HTTP client");
    let mut server = TestServer::spawn(temp.path());
    server.wait_ready(&client).await;
    let (_, token) = authenticate(&client, &server).await;
    let mut socket_url = Url::parse(&server.base_url).expect("build websocket URL");
    socket_url.set_scheme("ws").expect("set websocket scheme");
    socket_url.set_path("/socket");
    socket_url.query_pairs_mut().append_pair("api_key", &token);
    let (mut socket, _) = tokio_tungstenite::connect_async(socket_url.as_str())
        .await
        .expect("connect authenticated websocket");

    create_filesystem_library(
        &client,
        &server,
        &token,
        "Realtime Library",
        &media_root,
        "Manual",
    )
    .await;

    let received = tokio::time::timeout(Duration::from_secs(10), socket.next()).await;
    server.stop();
    let message = received
        .expect("catalog generation must notify the active socket")
        .expect("socket must remain open")
        .expect("socket message must be valid");
    let tokio_tungstenite::tungstenite::Message::Text(payload) = message else {
        panic!("expected a text event");
    };
    let event: Value = serde_json::from_str(&payload).expect("parse websocket event");
    assert_eq!(event["MessageType"], "LibraryChanged");
    assert!(
        event["Data"]["CatalogRevision"]
            .as_i64()
            .is_some_and(|value| value > 0)
    );
}

#[tokio::test]
async fn tcp_system_endpoint_reports_loopback_as_local_network() {
    let temp = tempfile::tempdir().expect("create temporary test directory");
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .expect("build HTTP client");
    let mut server = TestServer::spawn(temp.path());
    server.wait_ready(&client).await;
    let (_, token) = authenticate(&client, &server).await;

    let endpoint = json_response(
        client
            .get(format!("{}/System/Endpoint", server.base_url))
            .header("Authorization", token_header(&token))
            .send()
            .await
            .expect("request endpoint information"),
        StatusCode::OK,
        "read loopback endpoint information",
    )
    .await;
    server.stop();
    assert_eq!(endpoint, json!({"IsLocal": true, "IsInNetwork": true}));
}

#[tokio::test]
async fn tcp_device_options_and_revoke_lifecycle_are_durable() {
    let temp = tempfile::tempdir().expect("create temporary test directory");
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .expect("build HTTP client");
    let mut server = TestServer::spawn(temp.path());
    server.wait_ready(&client).await;
    let (_, token) = authenticate(&client, &server).await;

    let devices = json_response(
        client
            .get(format!("{}/Devices", server.base_url))
            .header("Authorization", token_header(&token))
            .send()
            .await
            .expect("list devices request"),
        StatusCode::OK,
        "list devices",
    )
    .await;
    assert_eq!(devices["TotalRecordCount"], 1);
    assert_eq!(devices["Items"][0]["Id"], "tcp-smoke-1");
    assert!(devices["Items"][0].get("AccessToken").is_none());

    assert_status(
        client
            .post(format!("{}/Devices/Options", server.base_url))
            .query(&[("id", "tcp-smoke-1")])
            .header("Authorization", token_header(&token))
            .json(&json!({
                "Id": 0,
                "DeviceId": "tcp-smoke-1",
                "CustomName": "TCP smoke device"
            }))
            .send()
            .await
            .expect("update device options request"),
        StatusCode::NO_CONTENT,
        "update device options",
    )
    .await;
    let options = json_response(
        client
            .get(format!("{}/Devices/Options", server.base_url))
            .query(&[("id", "tcp-smoke-1")])
            .header("Authorization", token_header(&token))
            .send()
            .await
            .expect("read device options request"),
        StatusCode::OK,
        "read device options",
    )
    .await;
    assert_eq!(options["DeviceId"], "tcp-smoke-1");
    assert_eq!(options["CustomName"], "TCP smoke device");

    assert_status(
        client
            .delete(format!("{}/Devices", server.base_url))
            .query(&[("id", "tcp-smoke-1")])
            .header("Authorization", token_header(&token))
            .send()
            .await
            .expect("delete device request"),
        StatusCode::NO_CONTENT,
        "delete device",
    )
    .await;
    assert_status(
        client
            .get(format!("{}/Users/Me", server.base_url))
            .header("Authorization", token_header(&token))
            .send()
            .await
            .expect("verify revoked device token request"),
        StatusCode::UNAUTHORIZED,
        "verify revoked device token",
    )
    .await;
    server.stop();
}

#[tokio::test]
async fn tcp_api_key_lifecycle_is_durable() {
    let temp = tempfile::tempdir().expect("create temporary test directory");
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .expect("build HTTP client");
    let database_url = format!(
        "sqlite://{}?mode=rwc",
        temp.path().join("tjxy.db").display()
    );
    let first_keyring = credential_keyring(1, &[(1, 1)]);
    let mut first =
        TestServer::spawn_with_database(temp.path(), &database_url, Some(&first_keyring));
    first.wait_ready(&client).await;
    let (_, session_token) = authenticate(&client, &first).await;

    assert_status(
        client
            .post(format!("{}/Auth/Keys", first.base_url))
            .query(&[("app", "Smoke")])
            .header("Authorization", token_header(&session_token))
            .send()
            .await
            .expect("create API key request"),
        StatusCode::NO_CONTENT,
        "create API key",
    )
    .await;
    let raw_key = listed_api_key(&client, &first, &session_token, "list API keys").await;
    assert_status(
        client
            .get(format!("{}/Users/Me", first.base_url))
            .header("Authorization", token_header(&raw_key))
            .send()
            .await
            .expect("authenticate with API key before restart"),
        StatusCode::OK,
        "authenticate with API key before restart",
    )
    .await;
    first.stop();

    let rotated_keyring = credential_keyring(2, &[(1, 1), (2, 2)]);
    let mut second =
        TestServer::spawn_with_database(temp.path(), &database_url, Some(&rotated_keyring));
    second.wait_ready(&client).await;
    assert_status(
        client
            .get(format!("{}/Users/Me", second.base_url))
            .header("Authorization", token_header(&raw_key))
            .send()
            .await
            .expect("authenticate with historical API key after restart"),
        StatusCode::OK,
        "authenticate with historical API key after restart",
    )
    .await;
    let restarted_key =
        listed_api_key(&client, &second, &raw_key, "list API keys after restart").await;
    assert_eq!(restarted_key, raw_key);

    let encoded_key = format!("%{:02X}{}", raw_key.as_bytes()[0], &raw_key[1..]);
    let delete_request = client
        .delete(format!("{}/Auth/Keys/{encoded_key}", second.base_url))
        .header("Authorization", token_header(&raw_key))
        .build()
        .expect("build encoded API key delete request");
    assert!(delete_request.url().as_str().contains("/Auth/Keys/%"));
    assert_status(
        client
            .execute(delete_request)
            .await
            .expect("delete API key after restart"),
        StatusCode::NO_CONTENT,
        "delete API key after restart",
    )
    .await;
    let rejected = client
        .get(format!("{}/Users/Me", second.base_url))
        .header("Authorization", token_header(&raw_key))
        .send()
        .await
        .expect("verify deleted API key request");
    let rejected_status = rejected.status();
    let rejected_body = rejected.text().await.expect("read deleted-key response");
    assert_eq!(rejected_status, StatusCode::UNAUTHORIZED);
    assert!(!rejected_body.contains(&raw_key));
    second.stop();
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Keeps the restart, playback, stream, subtitle, and playstate lifecycle together.
async fn tcp_filesystem_library_survives_restart_and_supports_jellyfin_playback_contract() {
    let temp = tempfile::tempdir().expect("create temporary test directory");
    let media_root = copy_media_fixture(&temp);
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .expect("build HTTP client");

    let mut first = TestServer::spawn(temp.path());
    first.wait_ready(&client).await;
    let (_, first_token) = authenticate(&client, &first).await;
    create_filesystem_library(
        &client,
        &first,
        &first_token,
        "Smoke TV",
        &media_root,
        "Lazy",
    )
    .await;
    first.stop();

    let mut second = TestServer::spawn(temp.path());
    second.wait_ready(&client).await;
    let (user_id, token) = authenticate(&client, &second).await;
    let library_id = library_id_by_name(&client, &second, &token, &user_id, "Smoke TV").await;

    assert_status(
        client
            .post(format!("{}/Library/Refresh", second.base_url))
            .header("Authorization", token_header(&token))
            .send()
            .await
            .expect("refresh library request"),
        StatusCode::NO_CONTENT,
        "start filesystem library refresh",
    )
    .await;

    let series = wait_for_child(
        &client,
        &second,
        &token,
        &user_id,
        &library_id,
        "Series",
        "series discovered from lazy root",
    )
    .await;
    let series_id = series["Id"].as_str().expect("series id");
    let season = wait_for_child(
        &client,
        &second,
        &token,
        &user_id,
        series_id,
        "Season",
        "series expansion publishes season",
    )
    .await;
    let season_id = season["Id"].as_str().expect("season id");
    let episode = wait_for_child(
        &client,
        &second,
        &token,
        &user_id,
        season_id,
        "Episode",
        "series expansion publishes episode",
    )
    .await;
    let episode_id = episode["Id"].as_str().expect("episode id").to_owned();

    let initial_delivery =
        assert_playback_delivery_contract(&client, &second, &token, &user_id, &episode_id).await;
    let initial_publication = second.effective_source_publication(&episode_id).await;

    let reindex_job = submit_manual_task(
        &client,
        &second,
        &token,
        &format!("/Admin/Tasks/IndexMediaSources/{episode_id}"),
        "submit playback-contract source re-index",
    )
    .await;
    wait_for_job(
        &client,
        &second,
        &token,
        Some(&reindex_job),
        "IndexMediaSources",
        Some(&episode_id),
        100,
        "playback-contract source re-index",
    )
    .await;
    let reindexed_delivery =
        assert_playback_delivery_contract(&client, &second, &token, &user_id, &episode_id).await;
    let reindexed_publication = second.effective_source_publication(&episode_id).await;
    assert_ne!(reindexed_publication.0, initial_publication.0);
    assert!(reindexed_publication.1 > initial_publication.1);
    assert_eq!(reindexed_delivery.source_id, initial_delivery.source_id);
    assert_eq!(
        reindexed_delivery.direct_stream_url,
        initial_delivery.direct_stream_url
    );
    assert_eq!(
        reindexed_delivery.external_subtitles,
        initial_delivery.external_subtitles
    );
    let source_id = reindexed_delivery.source_id;

    for path in [
        "/Sessions/Playing",
        "/Sessions/Playing/Progress",
        "/Sessions/Playing/Stopped",
    ] {
        assert_status(
            client
                .post(format!("{}{}", second.base_url, path))
                .header("Authorization", token_header(&token))
                .json(&json!({"CanSeek": true}))
                .send()
                .await
                .expect("schema-valid optional playstate request"),
            StatusCode::NO_CONTENT,
            "accept optional playstate telemetry",
        )
        .await;
    }

    let play_session = Uuid::new_v4();
    for (path, position) in [
        ("/Sessions/Playing", 1_000_000_i64),
        ("/Sessions/Playing/Progress", 2_000_000_i64),
        ("/Sessions/Playing/Stopped", 3_000_000_i64),
    ] {
        assert_status(
            client
                .post(format!("{}{}", second.base_url, path))
                .header("Authorization", token_header(&token))
                .json(&json!({
                    "ItemId": episode_id,
                    "MediaSourceId": source_id,
                    "PlaySessionId": play_session,
                    "PositionTicks": position,
                    "UserId": user_id,
                    "CanSeek": true,
                    "PlayMethod": "DirectPlay"
                }))
                .send()
                .await
                .expect("playstate request"),
            StatusCode::NO_CONTENT,
            path,
        )
        .await;
    }
    let resume = json_response(
        client
            .get(format!(
                "{}/UserItems/Resume?userId={user_id}&mediaTypes=Video&limit=10&enableUserData=true",
                second.base_url
            ))
            .header("Authorization", token_header(&token))
            .send()
            .await
            .expect("resume request"),
        StatusCode::OK,
        "read persisted resume position",
    )
    .await;
    assert!(
        resume["Items"]
            .as_array()
            .is_some_and(|items| items.iter().any(|item| item["Id"] == episode_id))
    );
    assert_eq!(
        resume["Items"]
            .as_array()
            .and_then(|items| items.iter().find(|item| item["Id"] == episode_id))
            .and_then(|item| item["UserData"]["PlaybackPositionTicks"].as_i64()),
        Some(3_000_000)
    );
}

#[allow(clippy::too_many_lines)] // Keeps the externally observable direct-play contract together.
async fn assert_playback_delivery_contract(
    client: &Client,
    server: &TestServer,
    token: &str,
    user_id: &str,
    episode_id: &str,
) -> PlaybackContractSnapshot {
    let detail = json_response(
        client
            .get(format!(
                "{}/Items/{episode_id}?userId={user_id}",
                server.base_url
            ))
            .header("Authorization", token_header(token))
            .send()
            .await
            .expect("episode detail request"),
        StatusCode::OK,
        "read episode detail",
    )
    .await;
    assert_eq!(detail["Id"], episode_id);
    assert_eq!(detail["Type"], "Episode");

    let request: Value = serde_json::from_str(FILESYSTEM_PLAYBACK_REQUEST)
        .expect("filesystem PlaybackInfo request golden is valid JSON");
    let playback = json_response(
        client
            .post(format!(
                "{}/Items/{episode_id}/PlaybackInfo?userId={user_id}",
                server.base_url
            ))
            .header("Authorization", token_header(token))
            .json(&request)
            .send()
            .await
            .expect("playback info request"),
        StatusCode::OK,
        "resolve direct-play playback info",
    )
    .await;
    let sources = playback["MediaSources"]
        .as_array()
        .unwrap_or_else(|| panic!("PlaybackInfo returned invalid media sources: {playback}"));
    assert_eq!(
        sources.len(),
        1,
        "fixture must expose its complete source list"
    );
    let source = &sources[0];
    let source_id = source["Id"].as_str().expect("media source id").to_owned();
    let mut normalized = playback.clone();
    normalize_filesystem_playback(
        &mut normalized,
        Uuid::parse_str(episode_id).expect("episode ID is a UUID"),
        Uuid::parse_str(&source_id).expect("source ID is a UUID"),
    );
    let expected: Value = serde_json::from_str(FILESYSTEM_PLAYBACK_RESPONSE)
        .expect("filesystem PlaybackInfo golden is valid JSON");
    assert_eq!(normalized, expected);
    assert_eq!(source["Protocol"], "Http");
    assert_eq!(source["Path"], Value::Null);
    assert_eq!(source["IsRemote"], false);
    assert_eq!(source["SupportsTranscoding"], false);
    assert_eq!(source["SupportsDirectStream"], false);
    assert_eq!(source["SupportsDirectPlay"], true);
    assert_eq!(source["TranscodingUrl"], Value::Null);
    let direct_stream_url = source["DirectStreamUrl"]
        .as_str()
        .expect("direct stream URL")
        .to_owned();
    assert_eq!(
        direct_stream_url,
        format!("/Videos/{episode_id}/stream?static=true&mediaSourceId={source_id}")
    );
    let external_subtitles = source["MediaStreams"]
        .as_array()
        .expect("media stream list")
        .iter()
        .filter(|stream| stream["IsExternal"] == true)
        .map(|stream| {
            assert_eq!(stream["Type"], "Subtitle");
            assert_eq!(stream["DeliveryMethod"], "External");
            assert_eq!(stream["IsExternalUrl"], false);
            (
                stream["Index"].as_i64().expect("subtitle delivery index"),
                stream["DeliveryUrl"]
                    .as_str()
                    .expect("external subtitle delivery URL")
                    .to_owned(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        external_subtitles.len(),
        1,
        "fixture must expose exactly one external subtitle"
    );
    let (subtitle_index, subtitle_url) = &external_subtitles[0];
    assert_eq!(
        subtitle_url,
        &format!("/Videos/{episode_id}/{source_id}/Subtitles/{subtitle_index}/Stream.srt")
    );
    assert!(direct_stream_url.starts_with('/'));
    assert!(subtitle_url.starts_with('/'));

    let stream = client
        .get(format!("{}{}", server.base_url, direct_stream_url))
        .header("Authorization", token_header(token))
        .send()
        .await
        .expect("full media request");
    assert_eq!(stream.status(), StatusCode::OK);
    assert_eq!(
        stream
            .bytes()
            .await
            .expect("full media response body")
            .as_ref(),
        FIXTURE_MEDIA
    );
    let head = client
        .head(format!("{}{}", server.base_url, direct_stream_url))
        .header("Authorization", token_header(token))
        .send()
        .await
        .expect("full media HEAD request");
    assert_eq!(head.status(), StatusCode::OK);
    assert_eq!(
        head.headers()["content-length"],
        FIXTURE_MEDIA.len().to_string()
    );
    assert!(
        head.bytes()
            .await
            .expect("full HEAD response body")
            .is_empty()
    );
    let range = client
        .get(format!("{}{}", server.base_url, direct_stream_url))
        .header("Authorization", token_header(token))
        .header("Range", "bytes=0-15")
        .send()
        .await
        .expect("range media request");
    assert_eq!(range.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(
        range.bytes().await.expect("range response body").as_ref(),
        &FIXTURE_MEDIA[..16]
    );
    let range_head = client
        .head(format!("{}{}", server.base_url, direct_stream_url))
        .header("Authorization", token_header(token))
        .header("Range", "bytes=0-15")
        .send()
        .await
        .expect("range media HEAD request");
    assert_eq!(range_head.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(range_head.headers()["content-length"], "16");
    assert_eq!(
        range_head.headers()["content-range"],
        format!("bytes 0-15/{}", FIXTURE_MEDIA.len())
    );
    assert!(
        range_head
            .bytes()
            .await
            .expect("range HEAD response body")
            .is_empty()
    );
    let subtitle_response = client
        .get(format!("{}{}", server.base_url, subtitle_url))
        .header("Authorization", token_header(token))
        .send()
        .await
        .expect("subtitle request");
    assert_eq!(subtitle_response.status(), StatusCode::OK);
    assert_eq!(
        subtitle_response
            .text()
            .await
            .expect("subtitle response body"),
        FIXTURE_SUBTITLE
    );

    PlaybackContractSnapshot {
        source_id,
        direct_stream_url,
        external_subtitles,
    }
}

#[tokio::test]
async fn tcp_hybrid_refresh_completes_background_series_expansion_before_browse() {
    let temp = tempfile::tempdir().expect("create temporary test directory");
    let media_root = copy_media_fixture(&temp);
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .expect("build HTTP client");

    let mut first = TestServer::spawn(temp.path());
    first.wait_ready(&client).await;
    let (_, first_token) = authenticate(&client, &first).await;
    create_filesystem_library(
        &client,
        &first,
        &first_token,
        "Hybrid Smoke TV",
        &media_root,
        "Hybrid",
    )
    .await;
    first.stop();

    let mut second = TestServer::spawn(temp.path());
    second.wait_ready(&client).await;
    let (user_id, token) = authenticate(&client, &second).await;
    let library_id =
        library_id_by_name(&client, &second, &token, &user_id, "Hybrid Smoke TV").await;

    assert_status(
        client
            .post(format!("{}/Library/Refresh", second.base_url))
            .header("Authorization", token_header(&token))
            .send()
            .await
            .expect("hybrid refresh request"),
        StatusCode::NO_CONTENT,
        "start hybrid filesystem refresh",
    )
    .await;

    let expansion = wait_for_job(
        &client,
        &second,
        &token,
        None,
        "ExpandItem",
        None,
        5,
        "low-priority Hybrid Series expansion",
    )
    .await;
    assert_eq!(expansion["ScopeType"], "CatalogItem");
    let series_id = expansion["ScopeId"]
        .as_str()
        .expect("Hybrid expansion Series scope")
        .to_owned();
    wait_for_job(
        &client,
        &second,
        &token,
        None,
        "FullMediaScan",
        Some(&library_id),
        20,
        "Hybrid parent refresh",
    )
    .await;

    let series = wait_for_child(
        &client,
        &second,
        &token,
        &user_id,
        &library_id,
        "Series",
        "Series published by Hybrid refresh",
    )
    .await;
    assert_eq!(series["Id"], series_id);
    let season = wait_for_child(
        &client,
        &second,
        &token,
        &user_id,
        &series_id,
        "Season",
        "Season published by background Hybrid expansion",
    )
    .await;
    let season_id = season["Id"].as_str().expect("Hybrid Season ID");
    let episode = wait_for_child(
        &client,
        &second,
        &token,
        &user_id,
        season_id,
        "Episode",
        "Episode published by background Hybrid expansion",
    )
    .await;
    assert_eq!(episode["Name"], "Smoke Show S01E01");
}

async fn assert_root_full_job_graph(client: &Client, server: &TestServer, token: &str) {
    let jobs = json_response(
        client
            .get(format!("{}/Admin/Tasks/Jobs?Limit=100", server.base_url))
            .header("Authorization", token_header(token))
            .send()
            .await
            .expect("read root Full task graph"),
        StatusCode::OK,
        "read root Full task graph",
    )
    .await;
    let task_kinds = jobs
        .as_array()
        .expect("root Full jobs")
        .iter()
        .filter_map(|job| job["TaskKind"].as_str())
        .collect::<HashSet<_>>();
    for expected in [
        "FullLibraryRootScan",
        "ValidateStorageRoot",
        "DiscoverTitles",
        "ResolveMetadata",
        "ExpandItem",
        "ProbeMedia",
    ] {
        assert!(
            task_kinds.contains(expected),
            "root Full did not execute {expected}; observed {task_kinds:?}"
        );
    }
    assert!(
        !task_kinds.contains("IndexMediaSources"),
        "Series expansion must publish Episode sources without a redundant IndexMediaSources job"
    );
}

#[tokio::test]
async fn tcp_manual_library_runs_only_explicit_root_and_item_stages() {
    let temp = tempfile::tempdir().expect("create temporary test directory");
    let media_root = copy_media_fixture(&temp);
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .expect("build HTTP client");

    let mut first = TestServer::spawn(temp.path());
    first.wait_ready(&client).await;
    let (_, first_token) = authenticate(&client, &first).await;
    create_filesystem_library(
        &client,
        &first,
        &first_token,
        "Manual Smoke TV",
        &media_root,
        "Manual",
    )
    .await;
    first.stop();

    let mut second = TestServer::spawn(temp.path());
    second.wait_ready(&client).await;
    let (user_id, token) = authenticate(&client, &second).await;
    let library_id =
        library_id_by_name(&client, &second, &token, &user_id, "Manual Smoke TV").await;
    let root_id =
        storage_root_id_by_library_name(&client, &second, &token, "Manual Smoke TV").await;
    wait_for_job(
        &client,
        &second,
        &token,
        None,
        "ScopedStorageSync",
        None,
        50,
        "initial Manual root registration sync",
    )
    .await;
    assert_manual_jobs_are_limited_to(&client, &second, &token, &["ScopedStorageSync"]).await;

    assert_status(
        client
            .post(format!("{}/Library/Refresh", second.base_url))
            .header("Authorization", token_header(&token))
            .send()
            .await
            .expect("manual refresh request"),
        StatusCode::NO_CONTENT,
        "Manual library refresh remains explicit",
    )
    .await;
    assert_manual_jobs_are_limited_to(&client, &second, &token, &["ScopedStorageSync"]).await;

    let series_id =
        run_manual_root_stages(&client, &second, &token, &user_id, &library_id, &root_id).await;
    run_manual_item_stages(&client, &second, &token, &user_id, &series_id).await;
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Exercises one complete root-scoped production worker pipeline.
async fn tcp_manual_root_full_scan_completes_the_series_pipeline() {
    let temp = tempfile::tempdir().expect("create temporary test directory");
    let media_root = copy_media_fixture(&temp);
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .expect("build HTTP client");

    let mut first = TestServer::spawn(temp.path());
    first.wait_ready(&client).await;
    let (_, first_token) = authenticate(&client, &first).await;
    create_filesystem_library(
        &client,
        &first,
        &first_token,
        "Root Full Smoke TV",
        &media_root,
        "Manual",
    )
    .await;
    first.stop();

    let mut second = TestServer::spawn(temp.path());
    second.wait_ready(&client).await;
    let (user_id, token) = authenticate(&client, &second).await;
    let library_id =
        library_id_by_name(&client, &second, &token, &user_id, "Root Full Smoke TV").await;
    let root_id =
        storage_root_id_by_library_name(&client, &second, &token, "Root Full Smoke TV").await;
    wait_for_job(
        &client,
        &second,
        &token,
        None,
        "ScopedStorageSync",
        None,
        50,
        "initial root registration sync",
    )
    .await;

    let full_scan = submit_manual_task(
        &client,
        &second,
        &token,
        &format!("/Admin/Tasks/FullScan/{library_id}/{root_id}"),
        "submit explicit root Full scan",
    )
    .await;
    wait_for_job(
        &client,
        &second,
        &token,
        Some(&full_scan),
        "FullLibraryRootScan",
        None,
        20,
        "explicit root Full Series pipeline",
    )
    .await;
    assert_root_full_job_graph(&client, &second, &token).await;

    let series = wait_for_child(
        &client,
        &second,
        &token,
        &user_id,
        &library_id,
        "Series",
        "Series published by root Full",
    )
    .await;
    let series_id = series["Id"].as_str().expect("root Full Series ID");
    let season = wait_for_child(
        &client,
        &second,
        &token,
        &user_id,
        series_id,
        "Season",
        "Season published by root Full",
    )
    .await;
    let season_id = season["Id"].as_str().expect("root Full Season ID");
    let episode = wait_for_child(
        &client,
        &second,
        &token,
        &user_id,
        season_id,
        "Episode",
        "Episode published by root Full",
    )
    .await;
    let episode_id = episode["Id"].as_str().expect("root Full Episode ID");
    let playback = json_response(
        client
            .post(format!(
                "{}/Items/{episode_id}/PlaybackInfo?userId={user_id}",
                second.base_url
            ))
            .header("Authorization", token_header(&token))
            .json(&json!({}))
            .send()
            .await
            .expect("root Full PlaybackInfo request"),
        StatusCode::OK,
        "root Full PlaybackInfo",
    )
    .await;
    assert_eq!(
        playback["MediaSources"]
            .as_array()
            .map_or(0, std::vec::Vec::len),
        1
    );
}

async fn run_manual_root_stages(
    client: &Client,
    server: &TestServer,
    token: &str,
    user_id: &str,
    library_id: &str,
    root_id: &str,
) -> String {
    let validation_job = submit_manual_task(
        client,
        server,
        token,
        &format!("/Admin/Tasks/ValidateStorage/{root_id}"),
        "submit explicit Manual root validation",
    )
    .await;
    wait_for_job(
        client,
        server,
        token,
        Some(&validation_job),
        "ValidateStorageRoot",
        Some(root_id),
        20,
        "explicit Manual root validation",
    )
    .await;
    assert_manual_jobs_are_limited_to(
        client,
        server,
        token,
        &["ScopedStorageSync", "ValidateStorageRoot"],
    )
    .await;

    let discovery_job = submit_manual_task(
        client,
        server,
        token,
        &format!("/Admin/Tasks/DiscoverTitles/{root_id}"),
        "submit explicit Manual title discovery",
    )
    .await;
    wait_for_job(
        client,
        server,
        token,
        Some(&discovery_job),
        "DiscoverTitles",
        None,
        20,
        "explicit Manual title discovery",
    )
    .await;
    wait_for_child(
        client,
        server,
        token,
        user_id,
        library_id,
        "Series",
        "Series published by explicit Manual discovery",
    )
    .await["Id"]
        .as_str()
        .expect("Manual Series ID")
        .to_owned()
}

#[allow(clippy::too_many_lines)] // Keeps the ordered explicit Manual item workflow in one helper.
async fn run_manual_item_stages(
    client: &Client,
    server: &TestServer,
    token: &str,
    user_id: &str,
    series_id: &str,
) {
    for (path, task_kind, priority, context) in [
        (
            format!("/Admin/Tasks/ResolveMetadata/{series_id}"),
            "ResolveMetadata",
            20,
            "explicit Manual metadata resolution",
        ),
        (
            format!("/Admin/Tasks/ExpandItem/{series_id}"),
            "ExpandItem",
            100,
            "explicit Manual Series expansion",
        ),
    ] {
        let job_id = submit_manual_task(client, server, token, &path, context).await;
        wait_for_job(
            client,
            server,
            token,
            Some(&job_id),
            task_kind,
            Some(series_id),
            priority,
            context,
        )
        .await;
    }

    let season = wait_for_child(
        client,
        server,
        token,
        user_id,
        series_id,
        "Season",
        "Season published by explicit Manual expansion",
    )
    .await;
    let season_id = season["Id"].as_str().expect("Manual Season ID");
    let episode = wait_for_child(
        client,
        server,
        token,
        user_id,
        season_id,
        "Episode",
        "Episode published by explicit Manual expansion",
    )
    .await;
    let episode_id = episode["Id"].as_str().expect("Manual Episode ID");
    let index_job = submit_manual_task(
        client,
        server,
        token,
        &format!("/Admin/Tasks/IndexMediaSources/{episode_id}"),
        "submit explicit Manual Episode source index",
    )
    .await;
    wait_for_job(
        client,
        server,
        token,
        Some(&index_job),
        "IndexMediaSources",
        Some(episode_id),
        100,
        "explicit Manual Episode source index",
    )
    .await;

    let probe = json_response(
        client
            .post(format!(
                "{}/Admin/Tasks/ProbeMedia/{episode_id}",
                server.base_url
            ))
            .header("Authorization", token_header(token))
            .send()
            .await
            .expect("submit explicit Manual Probe request"),
        StatusCode::ACCEPTED,
        "submit explicit Manual Probe",
    )
    .await;
    let probe_jobs = probe["Jobs"]
        .as_array()
        .expect("Manual Probe submission jobs");
    assert!(
        !probe_jobs.is_empty(),
        "Manual Probe submits at least one job"
    );
    for job in probe_jobs {
        let job_id = job["JobId"].as_str().expect("Manual Probe job ID");
        wait_for_job(
            client,
            server,
            token,
            Some(job_id),
            "ProbeMedia",
            None,
            100,
            "explicit Manual Probe",
        )
        .await;
    }
}

async fn submit_manual_task(
    client: &Client,
    server: &TestServer,
    token: &str,
    path: &str,
    context: &str,
) -> String {
    let submission = json_response(
        client
            .post(format!("{}{path}", server.base_url))
            .header("Authorization", token_header(token))
            .send()
            .await
            .expect("submit explicit Manual task request"),
        StatusCode::ACCEPTED,
        context,
    )
    .await;
    submission["JobId"]
        .as_str()
        .expect("Manual task submission JobId")
        .to_owned()
}

async fn assert_manual_jobs_are_limited_to(
    client: &Client,
    server: &TestServer,
    token: &str,
    allowed_task_kinds: &[&str],
) {
    let jobs = json_response(
        client
            .get(format!("{}/Admin/Tasks/Jobs?Limit=100", server.base_url))
            .header("Authorization", token_header(token))
            .send()
            .await
            .expect("read Manual task isolation jobs"),
        StatusCode::OK,
        "read Manual task isolation jobs",
    )
    .await;
    assert!(jobs.as_array().is_some_and(|jobs| {
        jobs.iter().all(|job| {
            job["TaskKind"]
                .as_str()
                .is_some_and(|task| allowed_task_kinds.contains(&task))
        })
    }));
}

async fn authenticate(client: &Client, server: &TestServer) -> (String, String) {
    let login = json_response(
        client
            .post(format!("{}/Users/AuthenticateByName", server.base_url))
            .header("Authorization", IDENTITY)
            .json(&json!({"Username": USERNAME, "Pw": PASSWORD}))
            .send()
            .await
            .expect("authenticate request"),
        StatusCode::OK,
        "authenticate bootstrap administrator",
    )
    .await;
    (
        login["User"]["Id"]
            .as_str()
            .expect("authenticated user id")
            .to_owned(),
        login["AccessToken"]
            .as_str()
            .expect("authenticated access token")
            .to_owned(),
    )
}

async fn listed_api_key(
    client: &Client,
    server: &TestServer,
    token: &str,
    context: &str,
) -> String {
    let response = client
        .get(format!("{}/Auth/Keys", server.base_url))
        .header("Authorization", token_header(token))
        .send()
        .await
        .expect("list API keys request");
    assert_eq!(response.headers()[CACHE_CONTROL], "no-store");
    let payload = json_response(response, StatusCode::OK, context).await;
    payload["Items"][0]["AccessToken"]
        .as_str()
        .expect("listed API key access token")
        .to_owned()
}

async fn wait_for_child(
    client: &Client,
    server: &TestServer,
    token: &str,
    user_id: &str,
    parent_id: &str,
    item_type: &str,
    context: &str,
) -> Value {
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        let response = client
            .get(format!(
                "{}/Items?userId={user_id}&parentId={parent_id}&includeItemTypes={item_type}&recursive=false",
                server.base_url
            ))
            .header("Authorization", token_header(token))
            .send()
            .await
            .expect("poll catalog child request");
        if response.status() == StatusCode::OK {
            let payload: Value = response.json().await.expect("catalog child JSON");
            if let Some(item) = payload["Items"].as_array().and_then(|items| items.first()) {
                return item.clone();
            }
        }
        sleep(Duration::from_millis(200)).await;
    }
    panic!("timed out waiting for {context}:\n{}", server.logs());
}

#[allow(clippy::too_many_arguments)]
async fn wait_for_job(
    client: &Client,
    server: &TestServer,
    token: &str,
    job_id: Option<&str>,
    task_kind: &str,
    scope_id: Option<&str>,
    priority: i64,
    context: &str,
) -> Value {
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut last_jobs = Value::Null;
    while Instant::now() < deadline {
        let response = client
            .get(format!("{}/Admin/Tasks/Jobs?Limit=100", server.base_url))
            .header("Authorization", token_header(token))
            .send()
            .await
            .expect("poll administrator jobs request");
        if response.status() == StatusCode::OK {
            let jobs: Value = response.json().await.expect("administrator jobs JSON");
            if let Some(job) = jobs.as_array().and_then(|jobs| {
                jobs.iter().find(|job| {
                    job_id.is_none_or(|job_id| job["Id"] == job_id)
                        && job["TaskKind"] == task_kind
                        && job["Priority"] == priority
                        && scope_id.is_none_or(|scope_id| job["ScopeId"] == scope_id)
                })
            }) {
                match job["Status"].as_str() {
                    Some("Completed") => return job.clone(),
                    Some("Failed" | "Cancelled") => {
                        let job_id = job["Id"].as_str().expect("administrator job ID");
                        let error = server.job_error(job_id).await;
                        panic!(
                            "{context} terminated unexpectedly: {job}; persisted error: {error:?}\n{}",
                            server.logs()
                        );
                    }
                    _ => {}
                }
            }
            last_jobs = jobs;
        }
        sleep(Duration::from_millis(200)).await;
    }
    let persisted_error = match job_id {
        Some(job_id) => server.job_error(job_id).await,
        None => None,
    };
    let mut failed_errors = Vec::new();
    if let Some(jobs) = last_jobs.as_array() {
        for failed in jobs.iter().filter(|job| job["Status"] == "Failed").take(10) {
            if let Some(failed_id) = failed["Id"].as_str() {
                failed_errors.push((
                    failed_id.to_owned(),
                    failed["TaskKind"].clone(),
                    server.job_error(failed_id).await,
                ));
            }
        }
    }
    panic!(
        "timed out waiting for {context}; persisted error: {persisted_error:?}; failed jobs: {failed_errors:?}; recent jobs: {last_jobs}\n{}",
        server.logs()
    );
}

async fn assert_status(response: Response, expected: StatusCode, context: &str) {
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    assert_eq!(status, expected, "{context}: {body}");
}

async fn json_response(response: Response, expected: StatusCode, context: &str) -> Value {
    let status = response.status();
    let body = response.text().await.expect("read response body");
    assert_eq!(status, expected, "{context}: {body}");
    serde_json::from_str(&body).expect("parse JSON response")
}

fn copy_media_fixture(temp: &TempDir) -> PathBuf {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/jellyfin-smoke/Smoke Show/Season 01");
    let destination = temp.path().join("media/Smoke Show/Season 01");
    fs::create_dir_all(&destination).expect("create temporary media directory");
    for name in ["Smoke Show S01E01.mp4", "Smoke Show S01E01.srt"] {
        fs::copy(source.join(name), destination.join(name)).expect("copy media fixture");
    }
    temp.path().join("media")
}

fn available_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("reserve local port")
        .local_addr()
        .expect("read local port")
        .port()
}

fn token_header(token: &str) -> String {
    format!(r#"MediaBrowser Token="{token}""#)
}
