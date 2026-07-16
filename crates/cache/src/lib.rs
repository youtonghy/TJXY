//! Redis cache-aside key contracts. SQL revisions remain authoritative.

use std::fmt;

use thiserror::Error;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CacheKeyBuilder {
    prefix: String,
}

impl CacheKeyBuilder {
    /// Creates a versioned cache key builder.
    ///
    /// # Errors
    ///
    /// Returns [`CacheKeyError`] when the prefix is empty or contains the key
    /// segment delimiter.
    pub fn new(prefix: impl Into<String>) -> Result<Self, CacheKeyError> {
        Ok(Self {
            prefix: validate_segment(prefix.into())?,
        })
    }

    #[must_use]
    pub fn user_scoped(
        &self,
        catalog_generation: i64,
        user_id: &str,
        user_revision: i64,
        projection: CacheProjection,
        query_hash: &str,
    ) -> String {
        format!(
            "{}:v1:g:{catalog_generation}:u:{user_id}:r:{user_revision}:{projection}:{query_hash}",
            self.prefix
        )
    }

    #[must_use]
    pub fn catalog_item(&self, catalog_generation: i64, item_id: &str) -> String {
        format!("{}:v1:g:{catalog_generation}:item:{item_id}", self.prefix)
    }

    #[must_use]
    pub fn playback_info(
        &self,
        catalog_generation: i64,
        user_id: &str,
        user_revision: i64,
        item_id: &str,
        probe_digest: &PlaybackProbeDigest,
    ) -> String {
        format!(
            "{}:v1:g:{catalog_generation}:u:{user_id}:r:{user_revision}:playback:{item_id}:p:{}",
            self.prefix, probe_digest.0
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CacheProjection {
    UserViews,
    Latest,
    Resume,
    NextUp,
    Items,
    Search,
}

impl fmt::Display for CacheProjection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::UserViews => "user-views",
            Self::Latest => "latest",
            Self::Resume => "resume",
            Self::NextUp => "next-up",
            Self::Items => "items",
            Self::Search => "search",
        };
        formatter.write_str(value)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlaybackProbeDigest(String);

impl PlaybackProbeDigest {
    /// Creates a digest segment from persisted `MediaSource` probe revisions.
    ///
    /// # Errors
    ///
    /// Returns [`CacheKeyError`] when the digest is empty or contains the key
    /// segment delimiter.
    pub fn new(value: impl Into<String>) -> Result<Self, CacheKeyError> {
        Ok(Self(validate_segment(value.into())?))
    }
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error("cache key segment must be non-empty and cannot contain ':'")]
pub struct CacheKeyError;

fn validate_segment(value: String) -> Result<String, CacheKeyError> {
    if value.is_empty() || value.contains(':') {
        return Err(CacheKeyError);
    }
    Ok(value)
}
