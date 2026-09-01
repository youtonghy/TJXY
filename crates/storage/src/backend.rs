use std::{pin::Pin, time::Duration};

use async_trait::async_trait;
use bytes::Bytes;
use futures_core::Stream;
use thiserror::Error;

use crate::{
    ByteRange, ChangeCursor, ChangePage, ObjectPage, PageToken, StorageCapabilities, StorageObject,
    StorageObjectId,
};

pub type ByteStream = Pin<Box<dyn Stream<Item = Result<Bytes, BackendError>> + Send + 'static>>;

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum BackendError {
    #[error("storage backend does not support {capability}")]
    UnsupportedCapability { capability: String },
    #[error("invalid storage value: {message}")]
    InvalidValue { message: String },
    #[error("storage object was not found")]
    NotFound,
    #[error("requested byte range is not satisfiable for an object of size {size}")]
    RangeNotSatisfiable { size: u64 },
    #[error("storage backend is temporarily unavailable: {message}")]
    TemporarilyUnavailable { message: String },
    #[error("storage backend is not ready: {message}")]
    BackendNotReady { message: String },
    #[error("storage backend rate limit was exceeded")]
    RateLimited { retry_after: Option<Duration> },
    #[error("storage change cursor is no longer valid")]
    ChangeCursorInvalid,
}

impl BackendError {
    #[must_use]
    pub fn unsupported_capability(capability: impl Into<String>) -> Self {
        Self::UnsupportedCapability {
            capability: capability.into(),
        }
    }

    pub(crate) fn invalid_value(message: impl Into<String>) -> Self {
        Self::InvalidValue {
            message: message.into(),
        }
    }
}

#[async_trait]
pub trait StorageBackend: Send + Sync {
    async fn get_object(&self, id: &StorageObjectId) -> Result<StorageObject, BackendError>;

    async fn list_children(
        &self,
        parent: &StorageObjectId,
        page: Option<PageToken>,
    ) -> Result<ObjectPage, BackendError>;

    async fn list_changes(&self, cursor: ChangeCursor) -> Result<ChangePage, BackendError>;

    async fn latest_change_cursor(&self) -> Result<ChangeCursor, BackendError> {
        Err(BackendError::unsupported_capability(
            "latest changes cursor",
        ))
    }

    async fn open_range(
        &self,
        id: &StorageObjectId,
        range: ByteRange,
    ) -> Result<ByteStream, BackendError>;

    /// Resolves a local path reference without allowing the target to escape this backend's root.
    async fn resolve_local_reference(
        &self,
        _descriptor: &StorageObjectId,
        _reference: &str,
    ) -> Result<StorageObject, BackendError> {
        Err(BackendError::unsupported_capability(
            "local path references",
        ))
    }

    fn capabilities(&self) -> StorageCapabilities;
}
