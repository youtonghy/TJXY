use serde::{Deserialize, Serialize};

use crate::BackendError;

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct StorageObjectId {
    provider: String,
    provider_object_id: String,
}

impl StorageObjectId {
    /// Creates a provider-scoped stable object identifier.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError::InvalidValue`] when either segment is empty.
    pub fn new(
        provider: impl Into<String>,
        provider_object_id: impl Into<String>,
    ) -> Result<Self, BackendError> {
        let provider = provider.into();
        let provider_object_id = provider_object_id.into();
        if provider.trim().is_empty() {
            return Err(BackendError::invalid_value("provider cannot be empty"));
        }
        if provider_object_id.trim().is_empty() {
            return Err(BackendError::invalid_value(
                "provider object id cannot be empty",
            ));
        }
        Ok(Self {
            provider,
            provider_object_id,
        })
    }

    #[must_use]
    pub fn provider(&self) -> &str {
        &self.provider
    }

    #[must_use]
    pub fn provider_object_id(&self) -> &str {
        &self.provider_object_id
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ObjectType {
    File,
    Directory,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum IdentityQuality {
    StableFileId,
    PathWeak,
    ProviderStableId,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct StorageObject {
    id: StorageObjectId,
    name: String,
    object_type: ObjectType,
    size: Option<u64>,
    identity_quality: IdentityQuality,
}

impl StorageObject {
    #[must_use]
    pub fn file(id: StorageObjectId, name: impl Into<String>, size: u64) -> Self {
        Self::file_with_identity(id, name, size, IdentityQuality::ProviderStableId)
    }

    #[must_use]
    pub fn file_with_identity(
        id: StorageObjectId,
        name: impl Into<String>,
        size: u64,
        identity_quality: IdentityQuality,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            object_type: ObjectType::File,
            size: Some(size),
            identity_quality,
        }
    }

    #[must_use]
    pub fn directory(id: StorageObjectId, name: impl Into<String>) -> Self {
        Self::directory_with_identity(id, name, IdentityQuality::ProviderStableId)
    }

    #[must_use]
    pub fn directory_with_identity(
        id: StorageObjectId,
        name: impl Into<String>,
        identity_quality: IdentityQuality,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            object_type: ObjectType::Directory,
            size: None,
            identity_quality,
        }
    }

    #[must_use]
    pub const fn id(&self) -> &StorageObjectId {
        &self.id
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub const fn object_type(&self) -> ObjectType {
        self.object_type
    }

    #[must_use]
    pub const fn size(&self) -> Option<u64> {
        self.size
    }

    #[must_use]
    pub const fn identity_quality(&self) -> IdentityQuality {
        self.identity_quality
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct PageToken(String);

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ChangeCursor(String);

impl ChangeCursor {
    /// Wraps an opaque provider cursor without interpreting its contents.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError::InvalidValue`] when the cursor is empty.
    pub fn new(value: impl Into<String>) -> Result<Self, BackendError> {
        let value = value.into();
        if value.is_empty() {
            return Err(BackendError::invalid_value("change cursor cannot be empty"));
        }
        Ok(Self(value))
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ObjectPage {
    pub objects: Vec<StorageObject>,
    pub next_page: Option<PageToken>,
}

impl ObjectPage {
    #[must_use]
    pub fn complete(objects: Vec<StorageObject>) -> Self {
        Self {
            objects,
            next_page: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ChangePage {
    pub objects: Vec<StorageObject>,
    pub next_cursor: ChangeCursor,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ByteRange {
    start: u64,
    end_exclusive: u64,
}

impl ByteRange {
    /// Creates a half-open byte range `[start, end_exclusive)`.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError::InvalidValue`] unless the end is greater than
    /// the start.
    pub fn new(start: u64, end_exclusive: u64) -> Result<Self, BackendError> {
        if start >= end_exclusive {
            return Err(BackendError::invalid_value(
                "byte range end must be greater than start",
            ));
        }
        Ok(Self {
            start,
            end_exclusive,
        })
    }

    #[must_use]
    pub const fn start(self) -> u64 {
        self.start
    }

    #[must_use]
    pub const fn end_exclusive(self) -> u64 {
        self.end_exclusive
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct StorageCapabilities {
    changes: bool,
    range_reads: bool,
}

impl StorageCapabilities {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            changes: false,
            range_reads: false,
        }
    }

    #[must_use]
    pub const fn with_changes(mut self, supported: bool) -> Self {
        self.changes = supported;
        self
    }

    #[must_use]
    pub const fn with_range_reads(mut self, supported: bool) -> Self {
        self.range_reads = supported;
        self
    }

    #[must_use]
    pub const fn changes(self) -> bool {
        self.changes
    }

    #[must_use]
    pub const fn range_reads(self) -> bool {
        self.range_reads
    }
}
