use bytes::Bytes;
use futures_util::{StreamExt, stream};
use tjxy_storage::{
    BackendError, ByteRange, ChangeCursor, ObjectPage, PageToken, StorageBackend,
    StorageCapabilities, StorageObject, StorageObjectId,
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
    assert!(backend.capabilities().range_reads());
}
