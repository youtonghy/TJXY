//! Process-wide admission leaves eight storage slots available to foreground reads.
use async_trait::async_trait;
use futures_util::StreamExt;
use std::{
    future::Future,
    sync::{Arc, OnceLock},
    time::Duration,
};
use tjxy_storage::{
    BackendError, ByteRange, ByteStream, ChangeCursor, ChangePage, ObjectPage, PageToken,
    StorageBackend, StorageCapabilities, StorageObject, StorageObjectId,
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

tokio::task_local! { static BACKGROUND: bool; }

/// Longest a foreground read waits for a storage slot before the caller is told to retry.
const FOREGROUND_ADMISSION_WAIT: Duration = Duration::from_secs(5);

/// Runs durable work with background admission unless promoted by a foreground request.
/// Priority 100 is the existing manual-media / Lazy foreground priority.
pub async fn with_work_io_priority<T>(priority: i32, work: impl Future<Output = T>) -> T {
    BACKGROUND.scope(priority < 100, work).await
}

struct Admission {
    total: Arc<Semaphore>,
    background: Arc<Semaphore>,
    foreground_wait: Duration,
}
struct Permit {
    _total: OwnedSemaphorePermit,
    _background: Option<OwnedSemaphorePermit>,
}
impl Admission {
    fn new(total: usize, background: usize) -> Self {
        Self {
            total: Arc::new(Semaphore::new(total)),
            background: Arc::new(Semaphore::new(background)),
            foreground_wait: FOREGROUND_ADMISSION_WAIT,
        }
    }
    /// Foreground reads give up after [`FOREGROUND_ADMISSION_WAIT`] so a saturated pool answers
    /// with a retryable error instead of hanging the request. Durable background work waits.
    async fn acquire_bounded(&self, background: bool) -> Result<Permit, BackendError> {
        if background {
            return Ok(self.acquire(true).await);
        }
        tokio::time::timeout(self.foreground_wait, self.acquire(false))
            .await
            .map_err(|_| BackendError::RateLimited {
                retry_after: Some(self.foreground_wait),
            })
    }
    async fn acquire(&self, background: bool) -> Permit {
        let background = if background {
            Some(
                Arc::clone(&self.background)
                    .acquire_owned()
                    .await
                    .expect("admission is never closed"),
            )
        } else {
            None
        };
        let total = Arc::clone(&self.total)
            .acquire_owned()
            .await
            .expect("admission is never closed");
        Permit {
            _total: total,
            _background: background,
        }
    }
}
fn storage_admission() -> Arc<Admission> {
    static VALUE: OnceLock<Arc<Admission>> = OnceLock::new();
    Arc::clone(VALUE.get_or_init(|| Arc::new(Admission::new(32, 24))))
}

pub(crate) async fn parser_permit() -> impl Send {
    static VALUE: OnceLock<Admission> = OnceLock::new();
    VALUE
        .get_or_init(|| {
            let background = std::thread::available_parallelism()
                .map_or(1, |cores| cores.get().saturating_sub(1).clamp(1, 4));
            Admission::new(background + 1, background)
        })
        .acquire(BACKGROUND.try_with(|value| *value).unwrap_or(false))
        .await
}

pub(crate) fn governed(backend: Arc<dyn StorageBackend>) -> Arc<dyn StorageBackend> {
    Arc::new(GovernedBackend {
        backend,
        admission: storage_admission(),
    })
}
struct GovernedBackend {
    backend: Arc<dyn StorageBackend>,
    admission: Arc<Admission>,
}
impl GovernedBackend {
    async fn permit(&self) -> Result<Permit, BackendError> {
        self.admission.acquire_bounded(is_background()).await
    }
}
fn is_background() -> bool {
    BACKGROUND.try_with(|value| *value).unwrap_or(false)
}
#[async_trait]
impl StorageBackend for GovernedBackend {
    async fn get_object(&self, id: &StorageObjectId) -> Result<StorageObject, BackendError> {
        let _permit = self.permit().await?;
        self.backend.get_object(id).await
    }
    async fn list_children(
        &self,
        parent: &StorageObjectId,
        page: Option<PageToken>,
    ) -> Result<ObjectPage, BackendError> {
        let _permit = self.permit().await?;
        self.backend.list_children(parent, page).await
    }
    async fn list_changes(&self, cursor: ChangeCursor) -> Result<ChangePage, BackendError> {
        let _permit = self.permit().await?;
        self.backend.list_changes(cursor).await
    }
    async fn latest_change_cursor(&self) -> Result<ChangeCursor, BackendError> {
        let _permit = self.permit().await?;
        self.backend.latest_change_cursor().await
    }
    async fn resolve_local_reference(
        &self,
        descriptor: &StorageObjectId,
        reference: &str,
    ) -> Result<StorageObject, BackendError> {
        let _permit = self.permit().await?;
        self.backend
            .resolve_local_reference(descriptor, reference)
            .await
    }
    async fn open_range(
        &self,
        id: &StorageObjectId,
        range: ByteRange,
    ) -> Result<ByteStream, BackendError> {
        let permit = self.permit().await?;
        let mut stream = self.backend.open_range(id, range).await?;
        if !is_background() {
            // A foreground stream can stay open for hours while a viewer pauses or buffers.
            // Holding a slot for that whole time would let 32 idle players starve every other
            // storage read, so the slot only covers establishing the upstream connection.
            return Ok(stream);
        }
        Ok(Box::pin(async_stream::stream! {
            let _permit = permit;
            while let Some(chunk) = stream.next().await { yield chunk; }
        }))
    }
    fn capabilities(&self) -> StorageCapabilities {
        self.backend.capabilities()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn saturated_background_preserves_foreground_capacity_and_resumes() {
        let admission = Admission::new(4, 2);
        let first = admission.acquire(true).await;
        let _second = admission.acquire(true).await;
        let pending = admission.acquire(true);
        tokio::pin!(pending);
        assert!(futures_util::poll!(&mut pending).is_pending());
        let _foreground = admission.acquire(false).await;
        let _other_foreground = admission.acquire(false).await;
        drop(first);
        assert!(futures_util::poll!(&mut pending).is_ready());
    }

    /// Opens a never-ending stream so only admission, not the backend, limits concurrency.
    struct OpenBackend;

    #[async_trait]
    impl StorageBackend for OpenBackend {
        async fn get_object(&self, _id: &StorageObjectId) -> Result<StorageObject, BackendError> {
            Err(BackendError::NotFound)
        }
        async fn list_children(
            &self,
            _parent: &StorageObjectId,
            _page: Option<PageToken>,
        ) -> Result<ObjectPage, BackendError> {
            Err(BackendError::NotFound)
        }
        async fn list_changes(&self, _cursor: ChangeCursor) -> Result<ChangePage, BackendError> {
            Err(BackendError::NotFound)
        }
        async fn open_range(
            &self,
            _id: &StorageObjectId,
            _range: ByteRange,
        ) -> Result<ByteStream, BackendError> {
            Ok(Box::pin(futures_util::stream::pending()))
        }
        fn capabilities(&self) -> StorageCapabilities {
            StorageCapabilities::new()
        }
    }

    fn governed_with(admission: Arc<Admission>) -> GovernedBackend {
        GovernedBackend {
            backend: Arc::new(OpenBackend),
            admission,
        }
    }

    fn object_id() -> StorageObjectId {
        StorageObjectId::new("filesystem", "movie.mkv").unwrap()
    }

    #[tokio::test]
    async fn open_foreground_streams_do_not_hold_storage_slots() {
        let backend = governed_with(Arc::new(Admission::new(1, 1)));
        let range = ByteRange::new(0, 1024).unwrap();
        let id = object_id();
        let _paused_viewers: Vec<_> =
            futures_util::future::join_all((0..4).map(|_| backend.open_range(&id, range)))
                .await
                .into_iter()
                .map(|stream| stream.expect("a paused viewer must not block the next one"))
                .collect();
        backend
            .get_object(&object_id())
            .await
            .expect_err("the fake object is missing, but admission must not time out");
    }

    #[tokio::test]
    async fn background_streams_keep_their_slot_until_dropped() {
        let backend = governed_with(Arc::new(Admission::new(1, 1)));
        let stream = BACKGROUND
            .scope(
                true,
                backend.open_range(&object_id(), ByteRange::new(0, 1024).unwrap()),
            )
            .await
            .unwrap();
        let id = object_id();
        let blocked = BACKGROUND.scope(true, backend.get_object(&id));
        tokio::pin!(blocked);
        assert!(futures_util::poll!(&mut blocked).is_pending());
        drop(stream);
        assert!(matches!(blocked.await, Err(BackendError::NotFound)));
    }

    #[tokio::test]
    async fn saturated_foreground_reads_fail_fast_with_a_retry_hint() {
        let wait = Duration::from_millis(20);
        let mut admission = Admission::new(1, 1);
        admission.foreground_wait = wait;
        let admission = Arc::new(admission);
        let _held = admission.acquire(false).await;
        let backend = governed_with(admission);
        let id = object_id();
        let error = backend.get_object(&id).await.unwrap_err();
        assert!(matches!(
            error,
            BackendError::RateLimited { retry_after: Some(hint) } if hint == wait
        ));
    }
}
