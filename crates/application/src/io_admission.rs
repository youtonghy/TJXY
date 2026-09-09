//! Process-wide admission leaves eight storage slots available to foreground reads.
use async_trait::async_trait;
use futures_util::StreamExt;
use std::{
    future::Future,
    sync::{Arc, OnceLock},
};
use tjxy_storage::{
    BackendError, ByteRange, ByteStream, ChangeCursor, ChangePage, ObjectPage, PageToken,
    StorageBackend, StorageCapabilities, StorageObject, StorageObjectId,
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

tokio::task_local! { static BACKGROUND: bool; }

/// Runs durable work with background admission unless promoted by a foreground request.
/// Priority 100 is the existing manual-media / Lazy foreground priority.
pub async fn with_work_io_priority<T>(priority: i32, work: impl Future<Output = T>) -> T {
    BACKGROUND.scope(priority < 100, work).await
}

struct Admission {
    total: Arc<Semaphore>,
    background: Arc<Semaphore>,
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
        }
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
fn storage_admission() -> &'static Admission {
    static VALUE: OnceLock<Admission> = OnceLock::new();
    VALUE.get_or_init(|| Admission::new(32, 24))
}

pub(crate) async fn parser_permit() -> impl Send {
    static VALUE: OnceLock<Admission> = OnceLock::new();
    VALUE
        .get_or_init(|| Admission::new(2, 1))
        .acquire(BACKGROUND.try_with(|value| *value).unwrap_or(false))
        .await
}

pub(crate) fn governed(backend: Arc<dyn StorageBackend>) -> Arc<dyn StorageBackend> {
    Arc::new(GovernedBackend(backend))
}
struct GovernedBackend(Arc<dyn StorageBackend>);
async fn permit() -> Permit {
    storage_admission()
        .acquire(BACKGROUND.try_with(|value| *value).unwrap_or(false))
        .await
}
#[async_trait]
impl StorageBackend for GovernedBackend {
    async fn get_object(&self, id: &StorageObjectId) -> Result<StorageObject, BackendError> {
        let _permit = permit().await;
        self.0.get_object(id).await
    }
    async fn list_children(
        &self,
        parent: &StorageObjectId,
        page: Option<PageToken>,
    ) -> Result<ObjectPage, BackendError> {
        let _permit = permit().await;
        self.0.list_children(parent, page).await
    }
    async fn list_changes(&self, cursor: ChangeCursor) -> Result<ChangePage, BackendError> {
        let _permit = permit().await;
        self.0.list_changes(cursor).await
    }
    async fn latest_change_cursor(&self) -> Result<ChangeCursor, BackendError> {
        let _permit = permit().await;
        self.0.latest_change_cursor().await
    }
    async fn resolve_local_reference(
        &self,
        descriptor: &StorageObjectId,
        reference: &str,
    ) -> Result<StorageObject, BackendError> {
        let _permit = permit().await;
        self.0.resolve_local_reference(descriptor, reference).await
    }
    async fn open_range(
        &self,
        id: &StorageObjectId,
        range: ByteRange,
    ) -> Result<ByteStream, BackendError> {
        let permit = permit().await;
        let mut stream = self.0.open_range(id, range).await?;
        Ok(Box::pin(async_stream::stream! {
            let _permit = permit;
            while let Some(chunk) = stream.next().await { yield chunk; }
        }))
    }
    fn capabilities(&self) -> StorageCapabilities {
        self.0.capabilities()
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
}
