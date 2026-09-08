use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use futures_util::StreamExt;
use sea_orm::DatabaseConnection;
use thiserror::Error;
use tjxy_common::StorageObjectRecordId;
use tjxy_db::{
    ObjectAvailabilityUpdate, StorageSyncRepository, StorageSyncRepositoryError,
    TemporaryAvailabilityReason,
};
use tjxy_storage::{
    BackendError, ByteRange, ByteStream, StorageBackend, StorageObject, StorageObjectId,
};

use crate::{StorageChangeProjector, StorageChangeProjectorError};

const MAX_AVAILABILITY_WRITE_ATTEMPTS: usize = 3;

const THROTTLE_PRUNE_THRESHOLD: usize = 4096;

/// Rate-limits per-object `Present` availability writes so range-heavy
/// playback does not open a database transaction for every seek.
///
/// Failure observations bypass the throttle: `reset` clears an object's
/// entry so the next successful read immediately persists recovery.
#[derive(Clone)]
pub(crate) struct ReadAvailabilityThrottle {
    window: Duration,
    inner: std::sync::Arc<Mutex<HashMap<StorageObjectRecordId, std::time::Instant>>>,
}

impl ReadAvailabilityThrottle {
    pub(crate) fn new(window: Duration) -> Self {
        Self {
            window,
            inner: std::sync::Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Builds a pass-through instance for low-frequency callers such as probes.
    pub(crate) fn unthrottled() -> Self {
        Self::new(Duration::ZERO)
    }

    /// Returns `true` when a `Present` observation should be persisted now.
    pub(crate) fn should_record_present(&self, object: StorageObjectRecordId) -> bool {
        if self.window.is_zero() {
            return true;
        }
        let now = std::time::Instant::now();
        let mut guard = self.inner.lock().expect("read availability throttle lock");
        if guard.len() >= THROTTLE_PRUNE_THRESHOLD {
            guard.retain(|_, recorded| now.duration_since(*recorded) < self.window);
        }
        match guard.get(&object) {
            Some(recorded) if now.duration_since(*recorded) < self.window => false,
            _ => {
                guard.insert(object, now);
                true
            }
        }
    }

    /// Clears an object's throttle entry after a failure observation so the
    /// next successful read persists immediately.
    pub(crate) fn reset(&self, object: StorageObjectRecordId) {
        if self.window.is_zero() {
            return;
        }
        self.inner
            .lock()
            .expect("read availability throttle lock")
            .remove(&object);
    }
}

pub(crate) async fn get_object(
    database: &DatabaseConnection,
    backend: &dyn StorageBackend,
    record_id: StorageObjectRecordId,
    backend_id: &StorageObjectId,
) -> Result<StorageObject, StorageReadError> {
    match backend.get_object(backend_id).await {
        Ok(object) => {
            record_and_project_availability(database, record_id, ReadAvailability::Present).await?;
            Ok(object)
        }
        Err(error) => {
            record_backend_failure(database, record_id, &error).await?;
            Err(StorageReadError::Backend(error))
        }
    }
}

pub(crate) async fn open_range(
    database: &DatabaseConnection,
    backend: &dyn StorageBackend,
    record_id: StorageObjectRecordId,
    backend_id: &StorageObjectId,
    range: ByteRange,
    throttle: &ReadAvailabilityThrottle,
) -> Result<ByteStream, StorageReadError> {
    match backend.open_range(backend_id, range).await {
        Ok(stream) => {
            if throttle.should_record_present(record_id) {
                record_and_project_availability(database, record_id, ReadAvailability::Present)
                    .await?;
            }
            Ok(availability_observing_stream(
                stream,
                database.clone(),
                record_id,
                throttle.clone(),
            ))
        }
        Err(error) => {
            throttle.reset(record_id);
            record_backend_failure(database, record_id, &error).await?;
            Err(StorageReadError::Backend(error))
        }
    }
}

async fn record_backend_failure(
    database: &DatabaseConnection,
    object_id: StorageObjectRecordId,
    error: &BackendError,
) -> Result<(), StorageReadError> {
    if let Some(reason) = failure_reason(error) {
        record_and_project_availability(database, object_id, ReadAvailability::Unavailable(reason))
            .await?;
    }
    Ok(())
}

fn availability_observing_stream(
    mut stream: ByteStream,
    database: DatabaseConnection,
    object_id: StorageObjectRecordId,
    throttle: ReadAvailabilityThrottle,
) -> ByteStream {
    Box::pin(async_stream::stream! {
        while let Some(item) = stream.next().await {
            match item {
                Ok(bytes) => yield Ok(bytes),
                Err(error) => {
                    throttle.reset(object_id);
                    if let Some(reason) = failure_reason(&error)
                        && let Err(observation_error) = record_and_project_availability(
                            &database,
                            object_id,
                            ReadAvailability::Unavailable(reason),
                        )
                        .await
                    {
                        yield Err(BackendError::TemporarilyUnavailable {
                            message: format!(
                                "failed to persist storage availability observation: \
                                 {observation_error}"
                            ),
                        });
                        break;
                    }
                    yield Err(error);
                }
            }
        }
    })
}

#[derive(Clone, Copy)]
enum ReadAvailability {
    Present,
    Unavailable(TemporaryAvailabilityReason),
}

async fn record_and_project_availability(
    database: &DatabaseConnection,
    object_id: StorageObjectRecordId,
    availability: ReadAvailability,
) -> Result<(), StorageReadError> {
    let mut attempts = 0_usize;
    let updates = loop {
        attempts += 1;
        let repository = StorageSyncRepository::new(database);
        let result = match availability {
            ReadAvailability::Present => repository.record_object_read_present(object_id).await,
            ReadAvailability::Unavailable(reason) => {
                repository
                    .record_object_read_unavailable(object_id, reason)
                    .await
            }
        };
        match result {
            Ok(updates) => break updates,
            Err(StorageSyncRepositoryError::RevisionConflict)
                if attempts < MAX_AVAILABILITY_WRITE_ATTEMPTS =>
            {
                tokio::task::yield_now().await;
            }
            Err(error) => return Err(error.into()),
        }
    };
    project_updates(database, &updates).await
}

async fn project_updates(
    database: &DatabaseConnection,
    updates: &[ObjectAvailabilityUpdate],
) -> Result<(), StorageReadError> {
    let projector = StorageChangeProjector::new(database.clone());
    for update in updates {
        projector
            .drain_root(update.root_id(), update.sync_revision())
            .await?;
    }
    Ok(())
}

const fn failure_reason(error: &BackendError) -> Option<TemporaryAvailabilityReason> {
    match error {
        BackendError::NotFound => {
            Some(TemporaryAvailabilityReason::BackendObjectNotFoundUnconfirmed)
        }
        BackendError::TemporarilyUnavailable { .. } => {
            Some(TemporaryAvailabilityReason::BackendTemporarilyUnavailable)
        }
        BackendError::RateLimited { .. } => Some(TemporaryAvailabilityReason::BackendRateLimited),
        BackendError::UnsupportedCapability { .. }
        | BackendError::InvalidValue { .. }
        | BackendError::BackendNotReady { .. }
        | BackendError::FilesystemIndexRebuilding
        | BackendError::FilesystemIndexFailed
        | BackendError::RangeNotSatisfiable { .. }
        | BackendError::ChangeCursorInvalid => None,
    }
}

#[derive(Debug, Error)]
pub(crate) enum StorageReadError {
    #[error("storage read failed: {0}")]
    Backend(BackendError),
    #[error("storage availability persistence failed: {0}")]
    Availability(#[from] StorageSyncRepositoryError),
    #[error("storage availability projection failed: {0}")]
    Projection(#[from] StorageChangeProjectorError),
}

#[cfg(test)]
mod tests {
    use tjxy_storage::BackendError;
    use uuid::Uuid;

    use super::{ReadAvailabilityThrottle, failure_reason};

    #[test]
    fn backend_readiness_gate_does_not_mutate_object_availability() {
        assert!(
            failure_reason(&BackendError::BackendNotReady {
                message: "rebuilding".to_owned(),
            })
            .is_none()
        );
    }

    #[test]
    fn present_observations_are_throttled_per_object_and_reset_on_failure() {
        let throttle = ReadAvailabilityThrottle::new(std::time::Duration::from_millis(50));
        let object = tjxy_common::StorageObjectRecordId::from_uuid(Uuid::new_v4());

        assert!(throttle.should_record_present(object));
        assert!(!throttle.should_record_present(object));

        throttle.reset(object);
        assert!(throttle.should_record_present(object));

        let other = tjxy_common::StorageObjectRecordId::from_uuid(Uuid::new_v4());
        assert!(throttle.should_record_present(other));
    }

    #[test]
    fn unthrottled_instances_always_allow_present_observations() {
        let throttle = ReadAvailabilityThrottle::unthrottled();
        let object = tjxy_common::StorageObjectRecordId::from_uuid(Uuid::new_v4());

        assert!(throttle.should_record_present(object));
        assert!(throttle.should_record_present(object));
    }
}
