//! Local filesystem implementation of the provider-neutral storage contract.

use std::{
    collections::HashMap,
    fs::Metadata,
    io::SeekFrom,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use async_trait::async_trait;
use bytes::Bytes;
use chrono::{DateTime, Utc};
use notify::{ErrorKind as NotifyErrorKind, RecommendedWatcher, RecursiveMode, Watcher};
use tjxy_storage::{
    BackendError, ByteRange, ByteStream, ChangeCursor, ChangePage, IdentityQuality, ObjectPage,
    ObjectType, PageToken, StorageBackend, StorageCapabilities, StorageObject, StorageObjectId,
};
use tokio::{
    fs::{self, File},
    io::{AsyncReadExt, AsyncSeekExt},
    sync::{RwLock, Semaphore, mpsc},
};

const RANGE_CHUNK_SIZE: usize = 64 * 1024;
const MAX_DIRECTORY_ENTRIES: usize = 10_000;
const MAX_INDEXED_PATHS: usize = 100_000;
const MAX_BLOCKING_OPENS: usize = 16;
const FILESYSTEM_OPEN_TIMEOUT: Duration = Duration::from_secs(5);
const EVENT_CHANNEL_CAPACITY: usize = 4_096;
const FSEVENT_WATCH_RETRIES: usize = 5;
const FSEVENT_WATCH_RETRY_BASE_DELAY: Duration = Duration::from_millis(50);

#[derive(Debug)]
pub struct FilesystemEventBatch {
    paths: Vec<PathBuf>,
}

pub struct FilesystemEventMonitor {
    _watcher: Option<RecommendedWatcher>,
    receiver: mpsc::Receiver<notify::Result<notify::Event>>,
    overflowed: Arc<AtomicBool>,
    quiet_window: Duration,
}

impl FilesystemEventMonitor {
    /// Waits for one quiet-window batch of filesystem event hints.
    ///
    /// # Errors
    ///
    /// Returns a temporary backend error when the native watcher fails or closes.
    pub async fn next_batch(&mut self) -> Result<FilesystemEventBatch, BackendError> {
        if self.overflowed.swap(false, Ordering::AcqRel) {
            return Err(event_overflow_error());
        }
        let first = self
            .receiver
            .recv()
            .await
            .ok_or_else(|| BackendError::TemporarilyUnavailable {
                message: "filesystem event watcher closed".to_owned(),
            })?
            .map_err(map_notify_error)?;
        let mut paths = first.paths;
        let sleep = tokio::time::sleep(self.quiet_window);
        tokio::pin!(sleep);
        loop {
            tokio::select! {
                () = &mut sleep => break,
                event = self.receiver.recv() => {
                    let event = event
                        .ok_or_else(|| BackendError::TemporarilyUnavailable {
                            message: "filesystem event watcher closed".to_owned(),
                        })?
                        .map_err(map_notify_error)?;
                    paths.extend(event.paths);
                    sleep.as_mut().reset(tokio::time::Instant::now() + self.quiet_window);
                }
            }
        }
        paths.sort();
        paths.dedup();
        if self.overflowed.swap(false, Ordering::AcqRel) {
            return Err(event_overflow_error());
        }
        Ok(FilesystemEventBatch { paths })
    }
}

pub struct FilesystemBackend {
    root: PathBuf,
    #[cfg(unix)]
    root_directory: std::fs::File,
    root_id: StorageObjectId,
    root_identity: String,
    physical_root_identity: String,
    root_identity_changed: bool,
    #[cfg(unix)]
    device_alias: Option<(u64, u64)>,
    paths: RwLock<HashMap<StorageObjectId, PathBuf>>,
    open_permits: Arc<Semaphore>,
}

impl FilesystemBackend {
    /// Opens a canonical directory as an isolated filesystem storage root.
    ///
    /// # Errors
    ///
    /// Returns a [`BackendError`] when the root cannot be resolved, inspected,
    /// or is not a directory.
    pub async fn new(root: impl AsRef<Path>) -> Result<Self, BackendError> {
        Self::open(root.as_ref(), None).await
    }

    /// Opens a canonical directory using its persisted root identity namespace.
    ///
    /// The configured path is authoritative. On Unix, device and inode changes are accepted while
    /// the persisted root namespace is retained. Objects on a renumbered device keep their
    /// persisted device component, so remounting does not replace the complete storage identity
    /// graph.
    ///
    /// # Errors
    ///
    /// Returns a [`BackendError`] when the root cannot be resolved or its persisted identity cannot
    /// be safely matched to the current directory.
    pub async fn new_with_root_id(
        root: impl AsRef<Path>,
        persisted_root_id: StorageObjectId,
    ) -> Result<Self, BackendError> {
        Self::open(root.as_ref(), Some(persisted_root_id)).await
    }

    async fn open(
        root: &Path,
        persisted_root_id: Option<StorageObjectId>,
    ) -> Result<Self, BackendError> {
        let root = fs::canonicalize(root).await.map_err(map_io_error)?;
        let metadata = fs::metadata(&root).await.map_err(map_io_error)?;
        if !metadata.is_dir() {
            return Err(BackendError::InvalidValue {
                message: "filesystem root must be a directory".to_owned(),
            });
        }
        let (current_identity, _) = filesystem_identity(&root, &metadata);
        let (root_id, root_identity) = match persisted_root_id {
            Some(root_id) => {
                let identity = persisted_root_identity(&root_id, &current_identity)?;
                (root_id, identity)
            }
            None => (
                StorageObjectId::new(
                    "filesystem",
                    format!("{current_identity}/{current_identity}"),
                )?,
                current_identity.clone(),
            ),
        };
        let root_identity_changed = root_identity != current_identity;
        #[cfg(unix)]
        let device_alias = unix_device_alias(&metadata, &root_identity)?;
        let mut paths = HashMap::new();
        paths.insert(root_id.clone(), root.clone());
        #[cfg(unix)]
        let root_directory = {
            use rustix::fs::{Mode, OFlags, open};

            std::fs::File::from(
                open(
                    &root,
                    OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                    Mode::empty(),
                )
                .map_err(|error| map_io_error(error.into()))?,
            )
        };
        Ok(Self {
            root,
            #[cfg(unix)]
            root_directory,
            root_id,
            root_identity,
            physical_root_identity: current_identity,
            root_identity_changed,
            #[cfg(unix)]
            device_alias,
            paths: RwLock::new(paths),
            open_permits: Arc::new(Semaphore::new(MAX_BLOCKING_OPENS)),
        })
    }

    #[must_use]
    pub const fn root_id(&self) -> &StorageObjectId {
        &self.root_id
    }

    #[must_use]
    pub fn root_path(&self) -> &Path {
        &self.root
    }

    #[must_use]
    pub const fn root_identity_changed(&self) -> bool {
        self.root_identity_changed
    }

    #[must_use]
    pub fn physical_root_identity(&self) -> &str {
        &self.physical_root_identity
    }

    /// Reads a known object through a persisted root-relative path.
    ///
    /// # Errors
    ///
    /// Returns a backend error when the path is unsafe, unavailable, or no longer identifies the
    /// requested object.
    pub async fn get_object_at(
        &self,
        id: &StorageObjectId,
        relative_path: &Path,
    ) -> Result<StorageObject, BackendError> {
        let path = self.path_from_relative(relative_path)?;
        self.get_object_from_path(id, &path).await
    }

    /// Lists a known directory through a persisted root-relative path.
    ///
    /// # Errors
    ///
    /// Returns a backend error when the path is unsafe, unavailable, or no longer identifies the
    /// requested directory.
    pub async fn list_children_at(
        &self,
        parent: &StorageObjectId,
        relative_path: &Path,
        page: Option<PageToken>,
    ) -> Result<ObjectPage, BackendError> {
        let path = self.path_from_relative(relative_path)?;
        self.list_children_from_path(parent, path, page).await
    }

    /// Opens a known object range through a persisted root-relative path.
    ///
    /// # Errors
    ///
    /// Returns a backend error when the path is unsafe, unavailable, stale, or outside the range.
    pub async fn open_range_at(
        &self,
        id: &StorageObjectId,
        relative_path: &Path,
        range: ByteRange,
    ) -> Result<ByteStream, BackendError> {
        let path = self.path_from_relative(relative_path)?;
        self.open_range_from_path(id, &path, range).await
    }

    /// Resolves a local descriptor reference from a persisted descriptor path.
    ///
    /// # Errors
    ///
    /// Returns a backend error when either path is unsafe or unavailable.
    pub async fn resolve_local_reference_at(
        &self,
        descriptor: &StorageObjectId,
        descriptor_relative_path: &Path,
        reference: &str,
    ) -> Result<StorageObject, BackendError> {
        let descriptor_path = self.path_from_relative(descriptor_relative_path)?;
        self.ensure_path_identity(descriptor, &descriptor_path)
            .await?;
        self.resolve_reference_from_path(&descriptor_path, reference)
            .await
    }

    /// Starts a recursive native event monitor rooted at this backend.
    ///
    /// Events are hints only. Call [`Self::inventory_scopes_for`] to resolve a batch back to
    /// canonical, stable directory identities before scheduling inventory work.
    ///
    /// # Errors
    ///
    /// Returns a temporary backend error when the platform watcher cannot be created or started.
    pub fn watch_events(
        &self,
        quiet_window: Duration,
    ) -> Result<FilesystemEventMonitor, BackendError> {
        if quiet_window.is_zero() {
            return Err(BackendError::InvalidValue {
                message: "filesystem event quiet window must be positive".to_owned(),
            });
        }
        let (sender, receiver) = mpsc::channel(EVENT_CHANNEL_CAPACITY);
        let overflowed = Arc::new(AtomicBool::new(false));
        let callback_overflowed = Arc::clone(&overflowed);
        let mut watcher =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                if event
                    .as_ref()
                    .is_ok_and(|event| !event_requires_inventory(event))
                {
                    return;
                }
                if matches!(
                    sender.try_send(event),
                    Err(mpsc::error::TrySendError::Full(_))
                ) {
                    callback_overflowed.store(true, Ordering::Release);
                }
            })
            .map_err(map_notify_error)?;
        watch_recursively(&mut watcher, &self.root)?;
        Ok(FilesystemEventMonitor {
            _watcher: Some(watcher),
            receiver,
            overflowed,
            quiet_window,
        })
    }

    /// Resolves event hints to canonical directory identities suitable for scoped inventory.
    ///
    /// Missing paths are represented by their surviving parent. Paths outside the configured
    /// root are ignored rather than converted into storage facts.
    ///
    /// # Errors
    ///
    /// Returns a backend error when an in-root directory cannot be inspected safely.
    pub async fn inventory_scopes_for(
        &self,
        batch: &FilesystemEventBatch,
    ) -> Result<Vec<StorageObjectId>, BackendError> {
        let mut scopes = HashMap::<StorageObjectId, PathBuf>::new();
        for path in &batch.paths {
            if !path.starts_with(&self.root) {
                continue;
            }
            let mut candidates = Vec::with_capacity(2);
            if path == &self.root {
                candidates.push(path.clone());
            } else if let Some(parent) = path.parent() {
                candidates.push(parent.to_owned());
            }
            if fs::metadata(path)
                .await
                .is_ok_and(|metadata| metadata.is_dir())
            {
                candidates.push(path.clone());
            }
            for candidate in candidates {
                let canonical = match fs::canonicalize(candidate).await {
                    Ok(path) => path,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                    Err(error) => return Err(map_io_error(error)),
                };
                if !canonical.starts_with(&self.root) {
                    continue;
                }
                let metadata = fs::metadata(&canonical).await.map_err(map_io_error)?;
                if !metadata.is_dir() {
                    continue;
                }
                let object = self.object_from_metadata(&canonical, &metadata)?;
                scopes.insert(object.id().clone(), canonical);
            }
        }
        let mut scopes = scopes.into_iter().collect::<Vec<_>>();
        scopes.sort_by(|(left, _), (right, _)| {
            left.provider_object_id().cmp(right.provider_object_id())
        });
        self.paths.write().await.extend(scopes.iter().cloned());
        Ok(scopes.into_iter().map(|(id, _)| id).collect())
    }

    async fn path_for(&self, id: &StorageObjectId) -> Result<PathBuf, BackendError> {
        if let Some(path) = self.paths.read().await.get(id).cloned() {
            return Ok(path);
        }
        Err(BackendError::BackendNotReady {
            message: "filesystem path is not indexed; storage validation is required".to_owned(),
        })
    }

    fn path_from_relative(&self, relative_path: &Path) -> Result<PathBuf, BackendError> {
        if relative_path.is_absolute()
            || relative_path
                .components()
                .any(|component| !matches!(component, std::path::Component::Normal(_)))
        {
            return Err(BackendError::InvalidValue {
                message: "filesystem relative path contains an unsafe component".to_owned(),
            });
        }
        Ok(self.root.join(relative_path))
    }

    async fn ensure_path_identity(
        &self,
        id: &StorageObjectId,
        path: &Path,
    ) -> Result<StorageObject, BackendError> {
        if id == &self.root_id && path == self.root {
            let metadata = fs::metadata(path).await.map_err(map_io_error)?;
            return self.root_object_from_metadata(&metadata);
        }
        let file = self.open_media_file(path).await?;
        let metadata = file.metadata().await.map_err(map_io_error)?;
        let object = self.object_from_metadata(path, &metadata)?;
        if object.id() != id {
            return Err(BackendError::TemporarilyUnavailable {
                message: "filesystem path index no longer matches the requested object".to_owned(),
            });
        }
        Ok(object)
    }

    async fn get_object_from_path(
        &self,
        id: &StorageObjectId,
        path: &Path,
    ) -> Result<StorageObject, BackendError> {
        self.ensure_path_identity(id, path).await
    }

    async fn list_children_from_path(
        &self,
        parent: &StorageObjectId,
        parent_path: PathBuf,
        page: Option<PageToken>,
    ) -> Result<ObjectPage, BackendError> {
        if page.is_some() {
            return Err(BackendError::unsupported_capability(
                "filesystem pagination",
            ));
        }
        if parent != &self.root_id || parent_path != self.root {
            let object = self.ensure_path_identity(parent, &parent_path).await?;
            if object.object_type() != ObjectType::Directory {
                return Err(BackendError::InvalidValue {
                    message: "filesystem child listing target is not a directory".to_owned(),
                });
            }
        }
        let mut directory = fs::read_dir(parent_path).await.map_err(map_io_error)?;
        let mut objects = Vec::new();
        let mut indexed_paths = Vec::new();
        while let Some(entry) = directory.next_entry().await.map_err(map_io_error)? {
            if objects.len() >= MAX_DIRECTORY_ENTRIES {
                return Err(BackendError::unsupported_capability(
                    "filesystem directory exceeds the 10000-entry limit",
                ));
            }
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
        let mut paths = self.paths.write().await;
        if paths.len().saturating_add(indexed_paths.len()) > MAX_INDEXED_PATHS {
            paths.retain(|id, _| id == &self.root_id);
        }
        paths.extend(indexed_paths);
        Ok(ObjectPage::complete(objects))
    }

    async fn open_range_from_path(
        &self,
        id: &StorageObjectId,
        path: &Path,
        range: ByteRange,
    ) -> Result<ByteStream, BackendError> {
        let mut file = self.open_media_file(path).await?;
        let metadata = file.metadata().await.map_err(map_io_error)?;
        let object = self.object_from_metadata(path, &metadata)?;
        if object.id() != id {
            return Err(BackendError::TemporarilyUnavailable {
                message: "filesystem path index no longer matches the requested object".to_owned(),
            });
        }
        if range.end_exclusive() > metadata.len() {
            return Err(BackendError::RangeNotSatisfiable {
                size: metadata.len(),
            });
        }
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

    fn object_from_metadata(
        &self,
        path: &Path,
        metadata: &Metadata,
    ) -> Result<StorageObject, BackendError> {
        let (identity, quality) = self.object_identity(path, metadata);
        let id = StorageObjectId::new("filesystem", format!("{}/{identity}", self.root_identity))?;
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| BackendError::InvalidValue {
                message: "filesystem object name is not valid UTF-8".to_owned(),
            })?;
        let object = if metadata.is_dir() {
            StorageObject::directory_with_identity(id, name, quality)
        } else {
            StorageObject::file_with_identity(id, name, metadata.len(), quality)
        };
        let modified = metadata.modified().map_err(map_io_error)?;
        Ok(object
            .with_remote_revision(filesystem_revision(metadata, &identity)?)?
            .with_remote_modified_at(DateTime::<Utc>::from(modified)))
    }

    fn root_object_from_metadata(
        &self,
        metadata: &Metadata,
    ) -> Result<StorageObject, BackendError> {
        let name = self
            .root
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("/");
        let modified = metadata.modified().map_err(map_io_error)?;
        Ok(StorageObject::directory_with_identity(
            self.root_id.clone(),
            name,
            IdentityQuality::StableFileId,
        )
        .with_remote_revision(filesystem_revision(metadata, &self.physical_root_identity)?)?
        .with_remote_modified_at(DateTime::<Utc>::from(modified)))
    }

    fn object_identity(&self, path: &Path, metadata: &Metadata) -> (String, IdentityQuality) {
        #[cfg(unix)]
        if let Some((current_device, persisted_device)) = self.device_alias {
            use std::os::unix::fs::MetadataExt;

            if metadata.dev() == current_device {
                return (
                    format!("{persisted_device:x}-{:x}", metadata.ino()),
                    IdentityQuality::StableFileId,
                );
            }
        }
        filesystem_identity(path, metadata)
    }

    #[cfg(unix)]
    async fn open_media_file(&self, path: &Path) -> Result<File, BackendError> {
        let relative = path
            .strip_prefix(&self.root)
            .map_err(|_| BackendError::NotFound)?
            .to_owned();
        let root = self.root_directory.try_clone().map_err(map_io_error)?;
        let permit = tokio::time::timeout(
            FILESYSTEM_OPEN_TIMEOUT,
            Arc::clone(&self.open_permits).acquire_owned(),
        )
        .await
        .map_err(|_| BackendError::TemporarilyUnavailable {
            message: "filesystem open concurrency limit timed out".to_owned(),
        })?
        .map_err(|_| BackendError::TemporarilyUnavailable {
            message: "filesystem open concurrency limiter closed".to_owned(),
        })?;
        let operation = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            open_relative_no_symlinks(&root, &relative)
        });
        let file = tokio::time::timeout(FILESYSTEM_OPEN_TIMEOUT, operation)
            .await
            .map_err(|_| BackendError::TemporarilyUnavailable {
                message: "filesystem open timed out".to_owned(),
            })?
            .map_err(|_| BackendError::TemporarilyUnavailable {
                message: "filesystem open task failed".to_owned(),
            })?
            .map_err(map_io_error)?;
        Ok(File::from_std(file))
    }

    #[cfg(not(unix))]
    async fn open_media_file(&self, _path: &Path) -> Result<File, BackendError> {
        Err(BackendError::unsupported_capability(
            "secure filesystem media reads",
        ))
    }
}

fn watch_recursively(watcher: &mut RecommendedWatcher, root: &Path) -> Result<(), BackendError> {
    for attempt in 0..=FSEVENT_WATCH_RETRIES {
        match watcher.watch(root, RecursiveMode::Recursive) {
            Ok(()) => return Ok(()),
            Err(error)
                if is_transient_fsevent_start_error(&error) && attempt < FSEVENT_WATCH_RETRIES =>
            {
                let _ = watcher.unwatch(root);
                let delay_factor = 1_u32 << attempt;
                std::thread::sleep(FSEVENT_WATCH_RETRY_BASE_DELAY * delay_factor);
            }
            Err(error) => return Err(map_notify_error(error)),
        }
    }
    unreachable!("filesystem watcher retries must return")
}

fn is_transient_fsevent_start_error(error: &notify::Error) -> bool {
    cfg!(target_os = "macos")
        && matches!(
            &error.kind,
            NotifyErrorKind::Generic(message) if message == "unable to start FSEvent stream"
        )
}

#[async_trait]
impl StorageBackend for FilesystemBackend {
    async fn get_object(&self, id: &StorageObjectId) -> Result<StorageObject, BackendError> {
        let path = self.path_for(id).await?;
        self.get_object_from_path(id, &path).await
    }

    async fn list_children(
        &self,
        parent: &StorageObjectId,
        page: Option<PageToken>,
    ) -> Result<ObjectPage, BackendError> {
        let parent_path = self.path_for(parent).await?;
        self.list_children_from_path(parent, parent_path, page)
            .await
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
        self.open_range_from_path(id, &path, range).await
    }

    async fn resolve_local_reference(
        &self,
        descriptor: &StorageObjectId,
        reference: &str,
    ) -> Result<StorageObject, BackendError> {
        let descriptor_path = if Path::new(reference).is_absolute() {
            self.root.clone()
        } else {
            self.path_for(descriptor).await?
        };
        self.resolve_reference_from_path(&descriptor_path, reference)
            .await
    }

    fn capabilities(&self) -> StorageCapabilities {
        StorageCapabilities::new()
            .with_file_events(true)
            .with_range_reads(true)
    }
}

impl FilesystemBackend {
    async fn resolve_reference_from_path(
        &self,
        descriptor_path: &Path,
        reference: &str,
    ) -> Result<StorageObject, BackendError> {
        let reference = Path::new(reference);
        if reference.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir | std::path::Component::CurDir
            )
        }) {
            return Err(BackendError::InvalidValue {
                message: "local reference must not contain dot path components".to_owned(),
            });
        }
        let path = if reference.is_absolute() {
            reference
                .strip_prefix(&self.root)
                .map(|relative| self.root.join(relative))
                .map_err(|_| BackendError::NotFound)?
        } else {
            descriptor_path
                .parent()
                .ok_or_else(|| BackendError::InvalidValue {
                    message: "descriptor has no parent directory".to_owned(),
                })?
                .join(reference)
        };
        let file = self.open_media_file(&path).await?;
        let metadata = file.metadata().await.map_err(map_io_error)?;
        if !metadata.is_file() {
            return Err(BackendError::InvalidValue {
                message: "local reference target is not a regular file".to_owned(),
            });
        }
        let object = self.object_from_metadata(&path, &metadata)?;
        self.paths.write().await.insert(object.id().clone(), path);
        Ok(object)
    }
}

#[cfg(unix)]
fn open_relative_no_symlinks(
    root: &std::fs::File,
    relative: &Path,
) -> Result<std::fs::File, std::io::Error> {
    use std::path::Component;

    use rustix::fs::{Mode, OFlags, openat};

    let mut directory = openat(
        root,
        ".",
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(std::io::Error::from)?;
    let mut components = relative.components().peekable();
    while let Some(Component::Normal(component)) = components.next() {
        let is_file = components.peek().is_none();
        let flags = if is_file {
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC
        } else {
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC
        };
        let opened =
            openat(&directory, component, flags, Mode::empty()).map_err(std::io::Error::from)?;
        if is_file {
            return Ok(std::fs::File::from(opened));
        }
        directory = opened;
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        "media path has no file component",
    ))
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

#[allow(clippy::needless_pass_by_value)] // `Result::map_err` passes the owned error.
fn map_notify_error(error: notify::Error) -> BackendError {
    BackendError::TemporarilyUnavailable {
        message: format!("filesystem event watcher failed: {error}"),
    }
}

fn event_overflow_error() -> BackendError {
    BackendError::TemporarilyUnavailable {
        message: "filesystem event queue overflowed; run storage validation to repair possible missed changes"
            .to_owned(),
    }
}

fn event_requires_inventory(event: &notify::Event) -> bool {
    event.need_rescan() || !matches!(event.kind, notify::EventKind::Access(_))
}

fn filesystem_revision(metadata: &Metadata, identity: &str) -> Result<String, BackendError> {
    let modified = metadata.modified().map_err(map_io_error)?;
    let elapsed = modified
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| BackendError::InvalidValue {
            message: format!("filesystem modification time predates the Unix epoch: {error}"),
        })?;
    Ok(format!(
        "{identity}:{}:{}:{}",
        metadata.len(),
        elapsed.as_secs(),
        elapsed.subsec_nanos()
    ))
}

#[cfg(unix)]
fn persisted_root_identity(
    root_id: &StorageObjectId,
    _current_identity: &str,
) -> Result<String, BackendError> {
    if root_id.provider() != "filesystem" {
        return Err(persisted_root_mismatch());
    }
    let Some((namespace, object)) = root_id.provider_object_id().split_once('/') else {
        return Err(persisted_root_mismatch());
    };
    if namespace.is_empty() || namespace != object || object.contains('/') {
        return Err(persisted_root_mismatch());
    }
    Ok(namespace.to_owned())
}

#[cfg(not(unix))]
fn persisted_root_identity(
    root_id: &StorageObjectId,
    current_identity: &str,
) -> Result<String, BackendError> {
    let current_root_id = StorageObjectId::new(
        "filesystem",
        format!("{current_identity}/{current_identity}"),
    )?;
    if root_id != &current_root_id {
        return Err(persisted_root_mismatch());
    }
    Ok(current_identity.to_owned())
}

#[cfg(unix)]
fn unix_device_alias(
    metadata: &Metadata,
    persisted_identity: &str,
) -> Result<Option<(u64, u64)>, BackendError> {
    use std::os::unix::fs::MetadataExt;

    let Some((device, inode)) = persisted_identity.split_once('-') else {
        return Err(persisted_root_mismatch());
    };
    let persisted_device =
        u64::from_str_radix(device, 16).map_err(|_| persisted_root_mismatch())?;
    u64::from_str_radix(inode, 16).map_err(|_| persisted_root_mismatch())?;
    Ok((persisted_device != metadata.dev()).then_some((metadata.dev(), persisted_device)))
}

fn persisted_root_mismatch() -> BackendError {
    BackendError::InvalidValue {
        message: "persisted filesystem root identity does not match the configured directory"
            .to_owned(),
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

#[cfg(test)]
mod tests {
    use std::{
        path::PathBuf,
        sync::{Arc, atomic::AtomicBool},
        time::Duration,
    };

    use notify::{
        Event, EventKind,
        event::{AccessKind, CreateKind, ModifyKind, RemoveKind},
    };
    use tokio::sync::mpsc;

    use super::{FilesystemEventMonitor, event_requires_inventory};

    #[test]
    fn inventory_events_exclude_non_mutating_access() {
        assert!(!event_requires_inventory(&Event::new(EventKind::Access(
            AccessKind::Any,
        ))));
        assert!(event_requires_inventory(&Event::new(EventKind::Any)));
        assert!(event_requires_inventory(&Event::new(EventKind::Create(
            CreateKind::Any,
        ))));
        assert!(event_requires_inventory(&Event::new(EventKind::Modify(
            ModifyKind::Any,
        ))));
        assert!(event_requires_inventory(&Event::new(EventKind::Remove(
            RemoveKind::Any,
        ))));
    }

    #[tokio::test]
    async fn event_monitor_coalesces_and_deduplicates_one_quiet_window() {
        let (sender, receiver) = mpsc::channel(4);
        let mut monitor = FilesystemEventMonitor {
            _watcher: None,
            receiver,
            overflowed: Arc::new(AtomicBool::new(false)),
            quiet_window: Duration::from_millis(10),
        };
        sender
            .send(Ok(
                Event::new(EventKind::Any).add_path(PathBuf::from("/root/b"))
            ))
            .await
            .unwrap();
        sender
            .send(Ok(
                Event::new(EventKind::Any).add_path(PathBuf::from("/root/a"))
            ))
            .await
            .unwrap();
        sender
            .send(Ok(
                Event::new(EventKind::Any).add_path(PathBuf::from("/root/b"))
            ))
            .await
            .unwrap();

        let batch = monitor.next_batch().await.unwrap();

        assert_eq!(
            batch.paths,
            vec![PathBuf::from("/root/a"), PathBuf::from("/root/b")]
        );
    }
}
