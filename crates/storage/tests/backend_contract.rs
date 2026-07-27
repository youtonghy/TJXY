use bytes::Bytes;
use futures_util::{StreamExt, stream};
use tjxy_storage::{
    BackendError, ByteRange, ChangeCursor, ChangePage, ObjectPage, PageToken, StorageBackend,
    StorageCapabilities, StorageChange, StorageObject, StorageObjectId,
};

struct ContractFake;

#[async_trait::async_trait]
impl StorageBackend for ContractFake {
    async fn get_object(&self, id: &StorageObjectId) -> Result<StorageObject, BackendError> {
        Ok(StorageObject::file(id.clone(), "movie.mkv", 8))
    }

    async fn list_children(
        &self,
        _parent: &StorageObjectId,
        _page: Option<PageToken>,
    ) -> Result<ObjectPage, BackendError> {
        Ok(ObjectPage::complete(Vec::new()))
    }

    async fn list_changes(
        &self,
        _cursor: ChangeCursor,
    ) -> Result<tjxy_storage::ChangePage, BackendError> {
        Err(BackendError::unsupported_capability("changes"))
    }

    async fn open_range(
        &self,
        _id: &StorageObjectId,
        range: ByteRange,
    ) -> Result<tjxy_storage::ByteStream, BackendError> {
        let bytes = Bytes::from_static(b"abcdefgh");
        let start = usize::try_from(range.start()).unwrap();
        let end = usize::try_from(range.end_exclusive()).unwrap();
        let selected = bytes.slice(start..end);
        Ok(Box::pin(stream::once(async move { Ok(selected) })))
    }

    fn capabilities(&self) -> StorageCapabilities {
        StorageCapabilities::new().with_range_reads(true)
    }
}

#[tokio::test]
async fn backend_contract_keeps_object_access_separate_from_media_semantics() {
    let backend = ContractFake;
    let id = StorageObjectId::new("filesystem", "stable-object-id").unwrap();

    let object = backend.get_object(&id).await.unwrap();

    assert_eq!(object.id(), &id);
    assert_eq!(object.name(), "movie.mkv");
    assert_eq!(object.size(), Some(8));
}

#[tokio::test]
async fn byte_ranges_are_half_open_and_bounded() {
    let backend = ContractFake;
    let id = StorageObjectId::new("filesystem", "stable-object-id").unwrap();
    let range = ByteRange::new(2, 5).unwrap();

    let chunks = backend
        .open_range(&id, range)
        .await
        .unwrap()
        .collect::<Vec<_>>()
        .await;
    let bytes = chunks.into_iter().collect::<Result<Vec<_>, _>>().unwrap();

    assert_eq!(bytes, vec![Bytes::from_static(b"cde")]);
}

#[tokio::test]
async fn unsupported_change_feeds_are_explicit_capability_errors() {
    let backend = ContractFake;

    let error = backend
        .list_changes(ChangeCursor::new("opaque-cursor").unwrap())
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        BackendError::UnsupportedCapability { capability } if capability == "changes"
    ));
    assert!(!backend.capabilities().changes());
    assert!(!backend.capabilities().file_events());
    assert!(backend.capabilities().range_reads());
}

#[test]
fn filesystem_events_are_a_separate_capability_from_provider_changes() {
    let capabilities = StorageCapabilities::new().with_file_events(true);

    assert!(capabilities.file_events());
    assert!(!capabilities.changes());
}

#[test]
fn pagination_and_change_tokens_remain_opaque_and_non_empty() {
    let page = PageToken::new("provider-next-page").unwrap();
    let cursor = ChangeCursor::new("provider-delta-cursor").unwrap();

    assert_eq!(page.as_str(), "provider-next-page");
    assert_eq!(cursor.as_str(), "provider-delta-cursor");
    assert!(PageToken::new("").is_err());
    assert!(ChangeCursor::new("").is_err());
    assert!(StorageObjectId::new("bad\nprovider", "object").is_err());
    assert!(StorageObjectId::new("provider", "x".repeat(2049)).is_err());
}

#[test]
fn change_pages_distinguish_continuations_from_terminal_cursors() {
    let terminal = ChangePage::new(Vec::new(), ChangeCursor::new("new-start-token").unwrap());
    let continuation =
        ChangePage::continuation(Vec::new(), ChangeCursor::new("next-page-token").unwrap());

    assert!(!terminal.has_more());
    assert!(continuation.has_more());
}

#[test]
fn change_pages_distinguish_upserts_from_confirmed_provider_removals() {
    let present_id = StorageObjectId::new("drive", "present").unwrap();
    let removed_id = StorageObjectId::new("drive", "removed").unwrap();
    let present = StorageObject::file(present_id.clone(), "movie.mkv", 8)
        .with_remote_revision("revision-2")
        .unwrap()
        .with_mime_type("video/x-matroska")
        .unwrap();
    let page = ChangePage::new(
        vec![
            StorageChange::Upsert(present),
            StorageChange::Removed(removed_id.clone()),
        ],
        ChangeCursor::new("cursor-2").unwrap(),
    );

    assert_eq!(page.changes().len(), 2);
    let StorageChange::Upsert(upserted) = &page.changes()[0] else {
        panic!("first change was not an upsert");
    };
    assert_eq!(upserted.id(), &present_id);
    assert_eq!(upserted.remote_revision(), Some("revision-2"));
    assert_eq!(upserted.mime_type(), Some("video/x-matroska"));
    assert_eq!(page.changes()[1], StorageChange::Removed(removed_id));
    assert_eq!(page.next_cursor().as_str(), "cursor-2");
}

#[test]
fn change_objects_carry_stable_provider_parent_identities() {
    let parent = StorageObjectId::new("drive", "folder-id").unwrap();
    let object = StorageObject::file(
        StorageObjectId::new("drive", "movie-id").unwrap(),
        "movie.mkv",
        8,
    )
    .with_parents(vec![parent.clone()])
    .unwrap();

    assert_eq!(object.parents(), &[parent]);
    assert!(
        StorageObject::file(
            StorageObjectId::new("drive", "other").unwrap(),
            "other.mkv",
            8,
        )
        .with_parents(vec![
            StorageObjectId::new("other-provider", "folder").unwrap()
        ])
        .is_err()
    );
}
