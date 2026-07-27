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
) -> Result<ByteStream, StorageReadError> {
    match backend.open_range(backend_id, range).await {
        Ok(stream) => {
            record_and_project_availability(database, record_id, ReadAvailability::Present).await?;
            Ok(availability_observing_stream(
                stream,
                database.clone(),
                record_id,
            ))
        }
        Err(error) => {
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
) -> ByteStream {
    Box::pin(async_stream::stream! {
        while let Some(item) = stream.next().await {
            match item {
                Ok(bytes) => yield Ok(bytes),
                Err(error) => {
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
