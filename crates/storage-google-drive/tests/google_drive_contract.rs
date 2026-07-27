use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

use futures_util::StreamExt;
use serde_json::{Value, json};
use tjxy_storage::{
    ByteRange, ChangeCursor, PageToken, StorageBackend, StorageChange, StorageObjectId,
};
use tjxy_storage_google_drive::{
    AccessTokenProvider, GoogleDriveBackend, GoogleDriveScope, GoogleDriveTransport,
    GoogleOAuthCredentials, OAuthAccessToken, OAuthRefreshClient, OAuthRefreshRequest,
    RefreshingAccessTokenProvider,
};

#[derive(Clone)]
struct Token;

#[async_trait::async_trait]
impl AccessTokenProvider for Token {
    async fn access_token(&self) -> Result<String, tjxy_storage::BackendError> {
        Ok("test-token".to_owned())
    }
}

struct RefreshClient {
    calls: AtomicUsize,
}

#[async_trait::async_trait]
impl OAuthRefreshClient for RefreshClient {
    async fn refresh(
        &self,
        request: &OAuthRefreshRequest<'_>,
    ) -> Result<OAuthAccessToken, tjxy_storage::BackendError> {
        assert_eq!(request.client_id(), "client-id");
        assert_eq!(request.client_secret(), "client-secret");
        assert_eq!(request.refresh_token(), "refresh-secret");
        self.calls.fetch_add(1, Ordering::SeqCst);
        tokio::task::yield_now().await;
        OAuthAccessToken::new("fresh-access", std::time::Duration::from_secs(3600))
    }
}

#[tokio::test]
async fn oauth_refresh_is_single_flight_and_credentials_are_redacted() {
    let client = Arc::new(RefreshClient {
        calls: AtomicUsize::new(0),
    });
    let credentials =
        GoogleOAuthCredentials::new("client-id", "client-secret", "refresh-secret").unwrap();
    let debug = format!("{credentials:?}");
    assert!(!debug.contains("client-secret"));
    assert!(!debug.contains("refresh-secret"));
    let provider = RefreshingAccessTokenProvider::new(credentials, client.clone());

    let (first, second) = tokio::join!(provider.access_token(), provider.access_token());

    assert_eq!(first.unwrap(), "fresh-access");
    assert_eq!(second.unwrap(), "fresh-access");
    assert_eq!(client.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn oauth_credentials_have_a_versioned_bounded_payload_contract() {
    let credentials =
        GoogleOAuthCredentials::new("client-id", "client-secret", "refresh-secret").unwrap();
    let payload = credentials.to_payload_json().unwrap();
    let restored = GoogleOAuthCredentials::from_payload_json(&payload).unwrap();
    let client = Arc::new(RefreshClient {
        calls: AtomicUsize::new(0),
    });

    let provider = RefreshingAccessTokenProvider::new(restored, client.clone());

    assert_eq!(provider.access_token().await.unwrap(), "fresh-access");
    assert_eq!(client.calls.load(Ordering::SeqCst), 1);
    assert!(
        GoogleOAuthCredentials::from_payload_json(
            br#"{"version":2,"client_id":"a","client_secret":"b","refresh_token":"c"}"#
        )
        .is_err()
    );
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Request {
    path: String,
    query: Vec<(String, String)>,
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
impl GoogleDriveTransport for Captured {
    async fn get_json(
        &self,
        _api_base: &reqwest::Url,
        path: &str,
        query: &[(String, String)],
        access_token: &str,
    ) -> Result<Value, tjxy_storage::BackendError> {
        self.requests.lock().unwrap().push(Request {
            path: path.to_owned(),
            query: query.to_vec(),
            token: access_token.to_owned(),
            range: None,
        });
        Ok(self.response.clone())
    }

    async fn get_range(
        &self,
        _api_base: &reqwest::Url,
        path: &str,
        query: &[(String, String)],
        access_token: &str,
        range: ByteRange,
    ) -> Result<tjxy_storage::ByteStream, tjxy_storage::BackendError> {
        self.requests.lock().unwrap().push(Request {
            path: path.to_owned(),
            query: query.to_vec(),
            token: access_token.to_owned(),
            range: Some(range),
        });
        Ok(Box::pin(futures_util::stream::once(async {
            Ok(bytes::Bytes::from_static(b"cde"))
        })))
    }
}

#[tokio::test]
async fn shared_drive_children_use_exact_scope_pagination_and_fields() {
    let captured = Arc::new(Captured::new(json!({
        "nextPageToken": "opaque-next",
        "files": [
            {"id":"folder-1","name":"Season 1","mimeType":"application/vnd.google-apps.folder","version":"7","modifiedTime":"2026-07-17T01:02:03Z"},
            {"id":"video-1","name":"Episode.mkv","mimeType":"video/x-matroska","size":"8","md5Checksum":"abc","version":"9","modifiedTime":"2026-07-17T02:03:04Z"}
        ]
    })));
    let backend = GoogleDriveBackend::new(
        Token,
        GoogleDriveScope::SharedDrive("shared-drive-id".to_owned()),
    )
    .unwrap()
    .with_transport(captured.clone());
    let parent = StorageObjectId::new("google-drive", "parent-id").unwrap();

    let page = backend
        .list_children(&parent, Some(PageToken::new("page-one").unwrap()))
        .await
        .unwrap();

    assert_eq!(page.next_page.unwrap().as_str(), "opaque-next");
    assert_eq!(page.objects.len(), 2);
    assert_eq!(page.objects[0].id().provider_object_id(), "folder-1");
    assert_eq!(page.objects[1].size(), Some(8));
    assert_eq!(page.objects[1].checksum(), Some("abc"));
    assert_eq!(page.objects[1].remote_revision(), Some("9"));
    let requests = captured.requests.lock().unwrap();
    let request = &requests[0];
    let query = request
        .query
        .iter()
        .cloned()
        .collect::<std::collections::HashMap<_, _>>();
    assert_eq!(request.path, "files");
    assert_eq!(request.token, "test-token");
    assert_eq!(query["q"], "'parent-id' in parents and trashed = false");
    assert_eq!(query["pageToken"], "page-one");
    assert_eq!(query["driveId"], "shared-drive-id");
    assert_eq!(query["corpora"], "drive");
    assert_eq!(query["includeItemsFromAllDrives"], "true");
    assert_eq!(query["supportsAllDrives"], "true");
    assert!(query["fields"].contains("nextPageToken"));
}

#[tokio::test]
async fn shared_drive_listing_preserves_identity_and_pagination() {
    let captured = Arc::new(Captured::new(json!({
        "nextPageToken":"shared-page-two",
        "drives":[
            {"id":"team-drive","name":"Team Media"},
            {"id":"archive-drive","name":"Archive"}
        ]
    })));
    let backend = GoogleDriveBackend::new(Token, GoogleDriveScope::MyDrive)
        .unwrap()
        .with_transport(captured.clone());

    let page = backend
        .list_shared_drives(Some(PageToken::new("shared-page-one").unwrap()))
        .await
        .unwrap();

    assert_eq!(page.next_page().unwrap().as_str(), "shared-page-two");
    assert_eq!(page.drives()[0].id(), "team-drive");
    assert_eq!(page.drives()[0].name(), "Team Media");
    let requests = captured.requests.lock().unwrap();
    let request = &requests[0];
    let query = request
        .query
        .iter()
        .cloned()
        .collect::<std::collections::HashMap<_, _>>();
    assert_eq!(request.path, "drives");
    assert_eq!(query["pageSize"], "100");
    assert_eq!(query["pageToken"], "shared-page-one");
    assert_eq!(query["fields"], "nextPageToken,drives(id,name)");
}

#[tokio::test]
async fn changes_preserve_removed_events_and_terminal_start_cursor() {
    let captured = Arc::new(Captured::new(json!({
        "newStartPageToken": "cursor-two",
        "changes": [
            {"fileId":"removed-id","removed":true},
            {"fileId":"video-id","removed":false,"file":{"id":"video-id","parents":["new-parent"],"name":"Movie.mp4","mimeType":"video/mp4","size":"12","version":"4","modifiedTime":"2026-07-17T02:03:04Z"}}
        ]
    })));
    let backend = GoogleDriveBackend::new(Token, GoogleDriveScope::MyDrive)
        .unwrap()
        .with_transport(captured.clone());

    let page = backend
        .list_changes(ChangeCursor::new("cursor-one").unwrap())
        .await
        .unwrap();

    assert_eq!(page.next_cursor().as_str(), "cursor-two");
    assert!(
        matches!(&page.changes()[0], StorageChange::Removed(id) if id.provider_object_id() == "removed-id")
    );
    assert!(
        matches!(&page.changes()[1], StorageChange::Upsert(object) if object.size() == Some(12))
    );
    let StorageChange::Upsert(object) = &page.changes()[1] else {
        unreachable!();
    };
    assert_eq!(object.parents()[0].provider_object_id(), "new-parent");
    let requests = captured.requests.lock().unwrap();
    let query = requests[0]
        .query
        .iter()
        .cloned()
        .collect::<std::collections::HashMap<_, _>>();
    assert_eq!(requests[0].path, "changes");
    assert_eq!(query["pageToken"], "cursor-one");
    assert_eq!(query["supportsAllDrives"], "true");
    assert!(query["fields"].contains("parents"));
}

#[tokio::test]
async fn changes_next_page_token_is_marked_as_a_continuation() {
    let captured = Arc::new(Captured::new(json!({
        "nextPageToken": "page-two",
        "changes": []
    })));
    let backend = GoogleDriveBackend::new(Token, GoogleDriveScope::MyDrive)
        .unwrap()
        .with_transport(captured);

    let page = backend
        .list_changes(ChangeCursor::new("page-one").unwrap())
        .await
        .unwrap();

    assert!(page.has_more());
    assert_eq!(page.next_cursor().as_str(), "page-two");
}

#[tokio::test]
async fn shared_drive_start_page_token_is_requested_with_drive_scope() {
    let captured = Arc::new(Captured::new(json!({"startPageToken":"start-token"})));
    let backend = GoogleDriveBackend::new(
        Token,
        GoogleDriveScope::SharedDrive("shared-drive-id".to_owned()),
    )
    .unwrap()
    .with_transport(captured.clone());

    let cursor = backend.start_page_token().await.unwrap();

    assert_eq!(cursor.as_str(), "start-token");
    let requests = captured.requests.lock().unwrap();
    let query = requests[0]
        .query
        .iter()
        .cloned()
        .collect::<std::collections::HashMap<_, _>>();
    assert_eq!(requests[0].path, "changes/startPageToken");
    assert_eq!(query["driveId"], "shared-drive-id");
    assert_eq!(query["supportsAllDrives"], "true");
}

#[tokio::test]
async fn range_reads_stream_only_the_requested_half_open_interval() {
    let captured = Arc::new(Captured::new(json!({})));
    let backend = GoogleDriveBackend::new(Token, GoogleDriveScope::MyDrive)
        .unwrap()
        .with_transport(captured.clone());
    let id = StorageObjectId::new("google-drive", "video-id").unwrap();

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
    assert!(backend.capabilities().changes());
    assert!(backend.capabilities().range_reads());
    let requests = captured.requests.lock().unwrap();
    assert_eq!(requests[0].path, "files/video-id");
    assert_eq!(requests[0].range, Some(ByteRange::new(2, 5).unwrap()));
    assert_eq!(
        requests[0].query,
        vec![("alt".to_owned(), "media".to_owned())]
    );
}
