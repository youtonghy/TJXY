use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

use futures_util::StreamExt;
use serde_json::{Value, json};
use tjxy_storage::{
    ByteRange, ChangeCursor, PageToken, StorageBackend, StorageChange, StorageObjectId,
};
use tjxy_storage_onedrive::{
    MicrosoftAccessToken, MicrosoftAccessTokenProvider, MicrosoftCredentialStore,
    MicrosoftOAuthCredentials, MicrosoftOAuthRefreshClient, MicrosoftOAuthRefreshRequest,
    OneDriveBackend, OneDriveScope, OneDriveTransport, RefreshingMicrosoftAccessTokenProvider,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

struct Token;

#[async_trait::async_trait]
impl MicrosoftAccessTokenProvider for Token {
    async fn access_token(&self) -> Result<String, tjxy_storage::BackendError> {
        Ok("access-token".into())
    }
}

struct RefreshClient {
    calls: AtomicUsize,
    expected_refresh_token: &'static str,
}

#[async_trait::async_trait]
impl MicrosoftOAuthRefreshClient for RefreshClient {
    async fn refresh(
        &self,
        request: &MicrosoftOAuthRefreshRequest<'_>,
    ) -> Result<MicrosoftAccessToken, tjxy_storage::BackendError> {
        assert_eq!(request.client_id(), "client-id");
        assert_eq!(request.client_secret(), Some("client-secret"));
        assert_eq!(request.refresh_token(), self.expected_refresh_token);
        self.calls.fetch_add(1, Ordering::SeqCst);
        tokio::task::yield_now().await;
        MicrosoftAccessToken::new(
            "access-token",
            std::time::Duration::from_secs(3600),
            Some("refresh-two".into()),
        )
    }
}

#[derive(Default)]
struct CredentialStore {
    payloads: Mutex<Vec<Vec<u8>>>,
}

#[async_trait::async_trait]
impl MicrosoftCredentialStore for CredentialStore {
    async fn persist(
        &self,
        credentials: &MicrosoftOAuthCredentials,
    ) -> Result<(), tjxy_storage::BackendError> {
        self.payloads
            .lock()
            .unwrap()
            .push(credentials.to_payload_json()?.to_vec());
        Ok(())
    }
}

#[tokio::test]
async fn oauth_refresh_is_single_flight_and_persists_rotated_refresh_token_first() {
    let credentials =
        MicrosoftOAuthCredentials::new("client-id", Some("client-secret".into()), "refresh-one")
            .unwrap();
    let payload = credentials.to_payload_json().unwrap();
    let credentials = MicrosoftOAuthCredentials::from_payload_json(&payload).unwrap();
    let client = Arc::new(RefreshClient {
        calls: AtomicUsize::new(0),
        expected_refresh_token: "refresh-one",
    });
    let store = Arc::new(CredentialStore::default());
    let provider = RefreshingMicrosoftAccessTokenProvider::new(credentials, client.clone())
        .with_credential_store(store.clone());

    let (first, second) = tokio::join!(provider.access_token(), provider.access_token());

    assert_eq!(first.unwrap(), "access-token");
    assert_eq!(second.unwrap(), "access-token");
    assert_eq!(client.calls.load(Ordering::SeqCst), 1);
    let rotated_payload = {
        let payloads = store.payloads.lock().unwrap();
        assert_eq!(payloads.len(), 1);
        payloads[0].clone()
    };
    let rotated = MicrosoftOAuthCredentials::from_payload_json(&rotated_payload).unwrap();
    assert!(format!("{rotated:?}").contains("[REDACTED]"));
    assert!(!format!("{rotated:?}").contains("refresh-two"));
    let verification_client = Arc::new(RefreshClient {
        calls: AtomicUsize::new(0),
        expected_refresh_token: "refresh-two",
    });
    RefreshingMicrosoftAccessTokenProvider::new(rotated, verification_client)
        .access_token()
        .await
        .unwrap();
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Request {
    target: String,
    token: String,
    range: Option<ByteRange>,
}

struct Captured {
    requests: Mutex<Vec<Request>>,
    response: Value,
}

impl Captured {
    fn new(response: Value) -> Self {
        Self {
            requests: Mutex::new(Vec::new()),
            response,
        }
    }
}

#[async_trait::async_trait]
impl OneDriveTransport for Captured {
    async fn get_json(
        &self,
        _api_base: &reqwest::Url,
        target: &str,
        access_token: &str,
    ) -> Result<Value, tjxy_storage::BackendError> {
        self.requests.lock().unwrap().push(Request {
            target: target.into(),
            token: access_token.into(),
            range: None,
        });
        Ok(self.response.clone())
    }

    async fn get_range(
        &self,
        _api_base: &reqwest::Url,
        target: &str,
        access_token: &str,
        range: ByteRange,
    ) -> Result<tjxy_storage::ByteStream, tjxy_storage::BackendError> {
        self.requests.lock().unwrap().push(Request {
            target: target.into(),
            token: access_token.into(),
            range: Some(range),
        });
        Ok(Box::pin(futures_util::stream::once(async {
            Ok(bytes::Bytes::from_static(b"cde"))
        })))
    }
}

#[tokio::test]
async fn personal_children_use_drive_item_identity_and_opaque_next_link() {
    let captured = Arc::new(Captured::new(json!({
        "@odata.nextLink": "https://graph.microsoft.com/v1.0/drives/drive-id/items/folder/children?$skiptoken=opaque",
        "value": [
            {"id":"folder-2","name":"Season 2","size":0,"folder":{},"parentReference":{"id":"folder","driveId":"drive-id"},"lastModifiedDateTime":"2026-07-17T01:02:03Z","eTag":"folder-etag"},
            {"id":"video-1","name":"Episode.mkv","size":8,"file":{"mimeType":"video/x-matroska","hashes":{"quickXorHash":"checksum"}},"parentReference":{"id":"folder","driveId":"drive-id"},"lastModifiedDateTime":"2026-07-17T02:03:04Z","eTag":"video-etag"}
        ]
    })));
    let backend = OneDriveBackend::new(Token, OneDriveScope::Personal, "drive-id")
        .unwrap()
        .with_transport(captured.clone());
    let parent = StorageObjectId::new("onedrive", "folder").unwrap();

    let page = backend.list_children(&parent, None).await.unwrap();

    assert_eq!(page.objects.len(), 2);
    assert_eq!(page.objects[1].checksum(), Some("checksum"));
    assert_eq!(page.objects[1].remote_revision(), Some("video-etag"));
    assert_eq!(
        page.next_page.unwrap().as_str(),
        "https://graph.microsoft.com/v1.0/drives/drive-id/items/folder/children?$skiptoken=opaque"
    );
    let request = &captured.requests.lock().unwrap()[0];
    assert!(
        request
            .target
            .starts_with("drives/drive-id/items/folder/children?")
    );
    assert_eq!(request.token, "access-token");
}

#[tokio::test]
async fn single_item_request_uses_select_without_collection_top() {
    let captured = Arc::new(Captured::new(json!({
        "id":"video-1","name":"Movie.mkv","size":8,"file":{"mimeType":"video/x-matroska"},"eTag":"etag"
    })));
    let backend = OneDriveBackend::new(Token, OneDriveScope::Personal, "drive-id")
        .unwrap()
        .with_transport(captured.clone());

    backend
        .get_object(&StorageObjectId::new("onedrive", "video-1").unwrap())
        .await
        .unwrap();

    let target = &captured.requests.lock().unwrap()[0].target;
    assert!(target.contains("?$select="));
    assert!(!target.contains("$top"));
}

#[tokio::test]
async fn delta_maps_parented_upserts_deletions_and_terminal_delta_link() {
    let captured = Arc::new(Captured::new(json!({
        "@odata.deltaLink": "https://graph.microsoft.com/v1.0/drives/drive-id/root/delta?token=terminal",
        "value": [
            {"id":"removed","name":"Removed","deleted":{}},
            {"id":"video","name":"Movie.mp4","size":12,"file":{"mimeType":"video/mp4"},"parentReference":{"id":"new-parent","driveId":"drive-id"},"eTag":"etag-2"}
        ]
    })));
    let backend = OneDriveBackend::new(Token, OneDriveScope::Personal, "drive-id")
        .unwrap()
        .with_transport(captured);

    let page = backend
        .list_changes(
            ChangeCursor::new(
                "https://graph.microsoft.com/v1.0/drives/drive-id/root/delta?token=one",
            )
            .unwrap(),
        )
        .await
        .unwrap();

    assert!(!page.has_more());
    assert!(
        matches!(&page.changes()[0], StorageChange::Removed(id) if id.provider_object_id() == "removed")
    );
    let StorageChange::Upsert(object) = &page.changes()[1] else {
        panic!("second delta item was not an upsert");
    };
    assert_eq!(object.parents()[0].provider_object_id(), "new-parent");
    assert_eq!(object.remote_revision(), Some("etag-2"));
}

#[tokio::test]
async fn delta_keeps_only_the_last_occurrence_of_each_drive_item() {
    let captured = Arc::new(Captured::new(json!({
        "@odata.deltaLink": "https://graph.microsoft.com/v1.0/drives/drive-id/root/delta?token=terminal",
        "value": [
            {"id":"removed-last","name":"Old.mp4","size":12,"file":{"mimeType":"video/mp4"},"eTag":"etag-old"},
            {"id":"upsert-last","name":"Deleted first","deleted":{}},
            {"id":"other","name":"Other.mp4","size":8,"file":{"mimeType":"video/mp4"},"eTag":"etag-other"},
            {"id":"removed-last","name":"Removed","deleted":{}},
            {"id":"upsert-last","name":"Current.mp4","size":16,"file":{"mimeType":"video/mp4"},"eTag":"etag-current"}
        ]
    })));
    let backend = OneDriveBackend::new(Token, OneDriveScope::Personal, "drive-id")
        .unwrap()
        .with_transport(captured);

    let page = backend
        .list_changes(
            ChangeCursor::new(
                "https://graph.microsoft.com/v1.0/drives/drive-id/root/delta?token=one",
            )
            .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(page.changes().len(), 3);
    assert!(
        matches!(&page.changes()[0], StorageChange::Upsert(object) if object.id().provider_object_id() == "other")
    );
    assert!(
        matches!(&page.changes()[1], StorageChange::Removed(id) if id.provider_object_id() == "removed-last")
    );
    assert!(
        matches!(&page.changes()[2], StorageChange::Upsert(object) if object.id().provider_object_id() == "upsert-last" && object.remote_revision() == Some("etag-current"))
    );
}

#[test]
fn business_and_sharepoint_are_explicitly_rejected_in_v1() {
    assert!(OneDriveBackend::new(Token, OneDriveScope::Business, "drive-id").is_err());
    assert!(OneDriveBackend::new(Token, OneDriveScope::SharePoint, "drive-id").is_err());
}

#[tokio::test]
async fn range_reads_stay_inside_the_server_backend() {
    let captured = Arc::new(Captured::new(json!({})));
    let backend = OneDriveBackend::new(Token, OneDriveScope::Personal, "drive-id")
        .unwrap()
        .with_transport(captured.clone());
    let id = StorageObjectId::new("onedrive", "video").unwrap();

    let chunks = backend
        .open_range(&id, ByteRange::new(2, 5).unwrap())
        .await
        .unwrap()
        .collect::<Vec<_>>()
        .await;

    assert_eq!(
        chunks.into_iter().collect::<Result<Vec<_>, _>>().unwrap(),
        vec![bytes::Bytes::from_static(b"cde")]
    );
    assert_eq!(
        captured.requests.lock().unwrap()[0].target,
        "drives/drive-id/items/video/content"
    );
}

#[tokio::test]
async fn graph_continuation_urls_reject_cross_origin_bearer_leaks() {
    let captured = Arc::new(Captured::new(json!({"value": []})));
    let backend = OneDriveBackend::new(Token, OneDriveScope::Personal, "drive-id")
        .unwrap()
        .with_transport(captured);
    let parent = StorageObjectId::new("onedrive", "folder").unwrap();

    assert!(
        backend
            .list_children(
                &parent,
                Some(PageToken::new("https://attacker.invalid/steal").unwrap()),
            )
            .await
            .is_err()
    );
}

#[tokio::test]
async fn graph_continuation_urls_accept_the_configured_safe_origin_unchanged() {
    let continuation =
        "http://127.0.0.1:43117/graph/v1.0/drives/drive-id/items/folder/children?$skiptoken=opaque";
    let captured = Arc::new(Captured::new(json!({"value": []})));
    let backend = OneDriveBackend::new(Token, OneDriveScope::Personal, "drive-id")
        .unwrap()
        .with_api_base("http://127.0.0.1:43117/graph/v1.0/")
        .unwrap()
        .with_transport(captured.clone());
    let parent = StorageObjectId::new("onedrive", "folder").unwrap();

    backend
        .list_children(&parent, Some(PageToken::new(continuation).unwrap()))
        .await
        .unwrap();

    assert_eq!(captured.requests.lock().unwrap()[0].target, continuation);
}

#[tokio::test]
async fn content_redirect_forwards_range_but_never_forwards_bearer_token() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut first, _) = listener.accept().await.unwrap();
        let first_request = read_request(&mut first).await;
        assert!(first_request.contains("GET /v1.0/drives/drive-id/items/video/content"));
        assert!(first_request.contains("authorization: Bearer access-token"));
        assert!(first_request.contains("range: bytes=2-4"));
        first
            .write_all(
                format!(
                    "HTTP/1.1 302 Found\r\nLocation: http://{address}/download\r\nConnection: close\r\nContent-Length: 0\r\n\r\n"
                )
                .as_bytes(),
            )
            .await
            .unwrap();
        let (mut second, _) = listener.accept().await.unwrap();
        let second_request = read_request(&mut second).await;
        assert!(second_request.contains("GET /download"));
        assert!(!second_request.contains("authorization:"));
        assert!(second_request.contains("range: bytes=2-4"));
        second
            .write_all(
                b"HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 2-4/8\r\nContent-Length: 3\r\n\r\ncde",
            )
            .await
            .unwrap();
    });
    let backend = OneDriveBackend::new(Token, OneDriveScope::Personal, "drive-id")
        .unwrap()
        .with_api_base(format!("http://{address}/v1.0/"))
        .unwrap();

    let chunks = backend
        .open_range(
            &StorageObjectId::new("onedrive", "video").unwrap(),
            ByteRange::new(2, 5).unwrap(),
        )
        .await
        .unwrap()
        .collect::<Vec<_>>()
        .await;

    assert_eq!(
        chunks.into_iter().collect::<Result<Vec<_>, _>>().unwrap(),
        vec![bytes::Bytes::from_static(b"cde")]
    );
    server.await.unwrap();
}

async fn read_request(stream: &mut tokio::net::TcpStream) -> String {
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 1024];
    loop {
        let read = stream.read(&mut chunk).await.unwrap();
        bytes.extend_from_slice(&chunk[..read]);
        if bytes.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
    }
    String::from_utf8(bytes).unwrap()
}
