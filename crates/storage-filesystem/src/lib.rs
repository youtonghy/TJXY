//! Local filesystem implementation of the provider-neutral storage contract.

use std::{
    collections::{HashMap, VecDeque},
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
    PageToken, StorageBackend, StorageCapabilities, StorageObject, StorageObjectId,
};
use tokio::{
    fs::{self, File},
    io::{AsyncReadExt, AsyncSeekExt},
    sync::{RwLock, mpsc},
};

const RANGE_CHUNK_SIZE: usize = 64 * 1024;
const MAX_RECOVERY_ENTRIES: usize = 1_000_000;
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
            paths: RwLock::new(paths),
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
        let mut watcher = notify::recommended_watcher(move |event| {
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
        self.recover_path(id).await
    }

    async fn recover_path(&self, target: &StorageObjectId) -> Result<PathBuf, BackendError> {
        if target.provider() != "filesystem"
            || !target
                .provider_object_id()
                .starts_with(&format!("{}/", self.root_identity))
        {
            return Err(BackendError::NotFound);
        }
        let mut directories = VecDeque::from([self.root.clone()]);
        let mut recovered = Vec::new();
        let mut visited = 0_usize;
        while let Some(parent) = directories.pop_front() {
            let mut directory = fs::read_dir(parent).await.map_err(map_io_error)?;
            while let Some(entry) = directory.next_entry().await.map_err(map_io_error)? {
                visited = visited
                    .checked_add(1)
                    .ok_or_else(|| BackendError::InvalidValue {
                        message: "filesystem recovery entry count overflowed".to_owned(),
                    })?;
                if visited > MAX_RECOVERY_ENTRIES {
                    return Err(BackendError::TemporarilyUnavailable {
                        message: "filesystem recovery exceeded its bounded entry count".to_owned(),
                    });
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
                recovered.push((object.id().clone(), canonical.clone()));
                if object.id() == target {
                    self.paths.write().await.extend(recovered);
                    return Ok(canonical);
                }
                if metadata.is_dir() {
                    directories.push_back(canonical);
                }
            }
        }
        self.paths.write().await.extend(recovered);
        Err(BackendError::NotFound)
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

    #[cfg(unix)]
    async fn open_media_file(&self, path: &Path) -> Result<File, BackendError> {
        let relative = path
            .strip_prefix(&self.root)
            .map_err(|_| BackendError::NotFound)?
            .to_owned();
        let root = self.root_directory.try_clone().map_err(map_io_error)?;
        let file = tokio::task::spawn_blocking(move || open_relative_no_symlinks(&root, &relative))
            .await
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
        let file = self.open_media_file(&path).await?;
        let metadata = file.metadata().await.map_err(map_io_error)?;
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
        let mut file = self.open_media_file(&path).await?;
        let metadata = file.metadata().await.map_err(map_io_error)?;
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

    async fn resolve_local_reference(
        &self,
        descriptor: &StorageObjectId,
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
            let descriptor_path = self.path_for(descriptor).await?;
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

    fn capabilities(&self) -> StorageCapabilities {
        StorageCapabilities::new()
            .with_file_events(true)
            .with_range_reads(true)
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

    use notify::{Event, EventKind};
    use tokio::sync::mpsc;

    use super::FilesystemEventMonitor;

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
