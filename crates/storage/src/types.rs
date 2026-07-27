use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::BackendError;

const MAX_IDENTITY_CHARS: usize = 2048;
const MAX_CURSOR_CHARS: usize = 16 * 1024;

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
        if !valid_opaque_value(&provider, MAX_IDENTITY_CHARS) {
            return Err(BackendError::invalid_value("provider cannot be empty"));
        }
        if !valid_opaque_value(&provider_object_id, MAX_IDENTITY_CHARS) {
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
    parents: Vec<StorageObjectId>,
    name: String,
    object_type: ObjectType,
    size: Option<u64>,
    identity_quality: IdentityQuality,
    mime_type: Option<String>,
    checksum: Option<String>,
    etag: Option<String>,
    remote_revision: Option<String>,
    remote_modified_at: Option<DateTime<Utc>>,
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
            parents: Vec::new(),
            name: name.into(),
            object_type: ObjectType::File,
            size: Some(size),
            identity_quality,
            mime_type: None,
            checksum: None,
            etag: None,
            remote_revision: None,
            remote_modified_at: None,
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
            parents: Vec::new(),
            name: name.into(),
            object_type: ObjectType::Directory,
            size: None,
            identity_quality,
            mime_type: None,
            checksum: None,
            etag: None,
            remote_revision: None,
            remote_modified_at: None,
        }
    }

    #[must_use]
    pub const fn id(&self) -> &StorageObjectId {
        &self.id
    }

    /// Attaches stable provider parent identities reported by a change feed.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError::InvalidValue`] for duplicate, excessive, or
    /// cross-provider parent identities.
    pub fn with_parents(mut self, parents: Vec<StorageObjectId>) -> Result<Self, BackendError> {
        let mut unique = std::collections::HashSet::with_capacity(parents.len());
        if parents.len() > 100
            || parents.iter().any(|parent| {
                parent.provider() != self.id.provider() || !unique.insert(parent.clone())
            })
        {
            return Err(BackendError::invalid_value(
                "object parents must be unique, bounded, and use the same provider",
            ));
        }
        self.parents = parents;
        Ok(self)
    }

    #[must_use]
    pub fn parents(&self) -> &[StorageObjectId] {
        &self.parents
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

    /// Attaches a provider MIME type without interpreting it as media classification.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError::InvalidValue`] for an empty or unsafe value.
    pub fn with_mime_type(mut self, value: impl Into<String>) -> Result<Self, BackendError> {
        self.mime_type = Some(validate_metadata_value("mime type", value.into(), 255)?);
        Ok(self)
    }

    /// Attaches an opaque provider checksum.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError::InvalidValue`] for an empty or unsafe value.
    pub fn with_checksum(mut self, value: impl Into<String>) -> Result<Self, BackendError> {
        self.checksum = Some(validate_metadata_value("checksum", value.into(), 2048)?);
        Ok(self)
    }

    /// Attaches an opaque provider `ETag`.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError::InvalidValue`] for an empty or unsafe value.
    pub fn with_etag(mut self, value: impl Into<String>) -> Result<Self, BackendError> {
        self.etag = Some(validate_metadata_value("etag", value.into(), 2048)?);
        Ok(self)
    }

    /// Attaches the provider's opaque object revision.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError::InvalidValue`] for an empty or unsafe value.
    pub fn with_remote_revision(mut self, value: impl Into<String>) -> Result<Self, BackendError> {
        self.remote_revision = Some(validate_metadata_value(
            "remote revision",
            value.into(),
            2048,
        )?);
        Ok(self)
    }

    #[must_use]
    pub const fn with_remote_modified_at(mut self, value: DateTime<Utc>) -> Self {
        self.remote_modified_at = Some(value);
        self
    }

    #[must_use]
    pub fn mime_type(&self) -> Option<&str> {
        self.mime_type.as_deref()
    }

    #[must_use]
    pub fn checksum(&self) -> Option<&str> {
        self.checksum.as_deref()
    }

    #[must_use]
    pub fn etag(&self) -> Option<&str> {
        self.etag.as_deref()
    }

    #[must_use]
    pub fn remote_revision(&self) -> Option<&str> {
        self.remote_revision.as_deref()
    }

    #[must_use]
    pub const fn remote_modified_at(&self) -> Option<DateTime<Utc>> {
        self.remote_modified_at
    }
}

fn validate_metadata_value(
    name: &str,
    value: String,
    max_chars: usize,
) -> Result<String, BackendError> {
    if value.trim().is_empty()
        || value.chars().count() > max_chars
        || value.chars().any(char::is_control)
    {
        return Err(BackendError::invalid_value(format!(
            "{name} must be non-empty, bounded, and contain no control characters"
        )));
    }
    Ok(value)
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct PageToken(String);

impl PageToken {
    /// Wraps an opaque provider page token without interpreting it.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError::InvalidValue`] when the token is empty.
    pub fn new(value: impl Into<String>) -> Result<Self, BackendError> {
        let value = value.into();
        if !valid_opaque_value(&value, MAX_CURSOR_CHARS) {
            return Err(BackendError::invalid_value("page token cannot be empty"));
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

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
        if !valid_opaque_value(&value, MAX_CURSOR_CHARS) {
            return Err(BackendError::invalid_value("change cursor cannot be empty"));
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn valid_opaque_value(value: &str, max_chars: usize) -> bool {
    !value.trim().is_empty()
        && value.chars().count() <= max_chars
        && !value.chars().any(char::is_control)
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
pub enum StorageChange {
    Upsert(StorageObject),
    Removed(StorageObjectId),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ChangePage {
    changes: Vec<StorageChange>,
    next_cursor: ChangeCursor,
    #[serde(default)]
    has_more: bool,
}

impl ChangePage {
    #[must_use]
    pub fn new(changes: Vec<StorageChange>, next_cursor: ChangeCursor) -> Self {
        Self {
            changes,
            next_cursor,
            has_more: false,
        }
    }

    #[must_use]
    pub fn continuation(changes: Vec<StorageChange>, next_cursor: ChangeCursor) -> Self {
        Self {
            changes,
            next_cursor,
            has_more: true,
        }
    }

    #[must_use]
    pub fn changes(&self) -> &[StorageChange] {
        &self.changes
    }

    #[must_use]
    pub const fn next_cursor(&self) -> &ChangeCursor {
        &self.next_cursor
    }

    #[must_use]
    pub const fn has_more(&self) -> bool {
        self.has_more
    }
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
    file_events: bool,
    range_reads: bool,
}

impl StorageCapabilities {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            changes: false,
            file_events: false,
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
    pub const fn with_file_events(mut self, supported: bool) -> Self {
        self.file_events = supported;
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

    #[must_use]
    pub const fn file_events(self) -> bool {
        self.file_events
    }
}
