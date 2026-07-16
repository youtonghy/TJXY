//! Local filesystem implementation of the provider-neutral storage contract.

use std::{
    collections::HashMap,
    fs::Metadata,
    io::SeekFrom,
    path::{Path, PathBuf},
};

use async_trait::async_trait;
use bytes::Bytes;
use tjxy_storage::{
    BackendError, ByteRange, ByteStream, ChangeCursor, ChangePage, IdentityQuality, ObjectPage,
    PageToken, StorageBackend, StorageCapabilities, StorageObject, StorageObjectId,
};
use tokio::{
    fs::{self, File},
    io::{AsyncReadExt, AsyncSeekExt},
    sync::RwLock,
};

const RANGE_CHUNK_SIZE: usize = 64 * 1024;

pub struct FilesystemBackend {
    root: PathBuf,
    root_id: StorageObjectId,
    root_identity: String,
    paths: RwLock<HashMap<StorageObjectId, PathBuf>>,
}

impl FilesystemBackend {
    /// Opens a canonical directory as an isolated filesystem storage root.
    ///
    /// # Errors
    ///
    /// Returns a [`BackendError`] when the root cannot be resolved, inspected,
    /// or is not a directory.
    pub async fn new(root: impl AsRef<Path>) -> Result<Self, BackendError> {
        let root = fs::canonicalize(root).await.map_err(map_io_error)?;
        let metadata = fs::metadata(&root).await.map_err(map_io_error)?;
        if !metadata.is_dir() {
            return Err(BackendError::InvalidValue {
                message: "filesystem root must be a directory".to_owned(),
            });
        }
        let (root_identity, _) = filesystem_identity(&root, &metadata);
        let root_id =
            StorageObjectId::new("filesystem", format!("{root_identity}/{root_identity}"))?;
        let mut paths = HashMap::new();
        paths.insert(root_id.clone(), root.clone());
        Ok(Self {
            root,
            root_id,
            root_identity,
            paths: RwLock::new(paths),
        })
    }

    #[must_use]
    pub const fn root_id(&self) -> &StorageObjectId {
        &self.root_id
    }

    async fn path_for(&self, id: &StorageObjectId) -> Result<PathBuf, BackendError> {
        self.paths
            .read()
            .await
            .get(id)
            .cloned()
            .ok_or(BackendError::NotFound)
    }

    fn object_from_metadata(
        &self,
        path: &Path,
        metadata: &Metadata,
    ) -> Result<StorageObject, BackendError> {
        let (identity, quality) = filesystem_identity(path, metadata);
        let id = StorageObjectId::new("filesystem", format!("{}/{identity}", self.root_identity))?;
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| BackendError::InvalidValue {
                message: "filesystem object name is not valid UTF-8".to_owned(),
            })?;
        if metadata.is_dir() {
            Ok(StorageObject::directory_with_identity(id, name, quality))
        } else {
            Ok(StorageObject::file_with_identity(
                id,
                name,
                metadata.len(),
                quality,
            ))
        }
    }
}

#[async_trait]
impl StorageBackend for FilesystemBackend {
    async fn get_object(&self, id: &StorageObjectId) -> Result<StorageObject, BackendError> {
        let path = self.path_for(id).await?;
        let metadata = fs::metadata(&path).await.map_err(map_io_error)?;
        self.object_from_metadata(&path, &metadata)
    }

    async fn list_children(
        &self,
        parent: &StorageObjectId,
        page: Option<PageToken>,
    ) -> Result<ObjectPage, BackendError> {
        if page.is_some() {
            return Err(BackendError::unsupported_capability(
                "filesystem pagination",
            ));
        }
        let parent_path = self.path_for(parent).await?;
        let mut directory = fs::read_dir(parent_path).await.map_err(map_io_error)?;
        let mut objects = Vec::new();
        let mut indexed_paths = Vec::new();
        while let Some(entry) = directory.next_entry().await.map_err(map_io_error)? {
            let file_type = entry.file_type().await.map_err(map_io_error)?;
            if file_type.is_symlink() {
                continue;
            }
            let canonical = fs::canonicalize(entry.path()).await.map_err(map_io_error)?;
            if !canonical.starts_with(&self.root) {
                continue;
            }
            let metadata = entry.metadata().await.map_err(map_io_error)?;
            let object = self.object_from_metadata(&canonical, &metadata)?;
            indexed_paths.push((object.id().clone(), canonical));
            objects.push(object);
        }
        objects.sort_by(|left, right| left.name().cmp(right.name()));
        self.paths.write().await.extend(indexed_paths);
        Ok(ObjectPage::complete(objects))
    }

    async fn list_changes(&self, _cursor: ChangeCursor) -> Result<ChangePage, BackendError> {
        Err(BackendError::unsupported_capability("changes"))
    }

    async fn open_range(
        &self,
        id: &StorageObjectId,
        range: ByteRange,
    ) -> Result<ByteStream, BackendError> {
        let path = self.path_for(id).await?;
        let metadata = fs::metadata(&path).await.map_err(map_io_error)?;
        if range.end_exclusive() > metadata.len() {
            return Err(BackendError::RangeNotSatisfiable {
                size: metadata.len(),
            });
        }
        let mut file = File::open(path).await.map_err(map_io_error)?;
        file.seek(SeekFrom::Start(range.start()))
            .await
            .map_err(map_io_error)?;
        let stream = async_stream::try_stream! {
            let mut remaining = range.end_exclusive() - range.start();
            while remaining > 0 {
                let chunk_len = usize::try_from(remaining)
                    .unwrap_or(RANGE_CHUNK_SIZE)
                    .min(RANGE_CHUNK_SIZE);
                let mut buffer = vec![0; chunk_len];
                file.read_exact(&mut buffer).await.map_err(map_io_error)?;
                remaining -= chunk_len as u64;
                yield Bytes::from(buffer);
            }
        };
        Ok(Box::pin(stream))
    }

    fn capabilities(&self) -> StorageCapabilities {
        StorageCapabilities::new().with_range_reads(true)
    }
}

#[allow(clippy::needless_pass_by_value)] // `Result::map_err` passes the owned error.
fn map_io_error(error: std::io::Error) -> BackendError {
    if error.kind() == std::io::ErrorKind::NotFound {
        BackendError::NotFound
    } else {
        BackendError::TemporarilyUnavailable {
            message: error.to_string(),
        }
    }
}

#[cfg(unix)]
fn filesystem_identity(_path: &Path, metadata: &Metadata) -> (String, IdentityQuality) {
    use std::os::unix::fs::MetadataExt;

    (
        format!("{:x}-{:x}", metadata.dev(), metadata.ino()),
        IdentityQuality::StableFileId,
    )
}

#[cfg(not(unix))]
fn filesystem_identity(path: &Path, _metadata: &Metadata) -> (String, IdentityQuality) {
    (
        path.to_string_lossy().into_owned(),
        IdentityQuality::PathWeak,
    )
}
