use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use sea_orm::{
    ConnectionTrait, DatabaseTransaction, DbErr, QueryResult, TransactionTrait,
    sea_query::{Alias, CaseStatement, Cond, Expr, JoinType, OnConflict, Order, Query, SimpleExpr},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tjxy_common::{
    CatalogItemId, MEDIA_NAME_PARSER_VERSION, MediaLocationId, MediaSourceId, PresentationKey,
    PublicationId, StorageObjectRecordId, StorageRootId, SubtitleId,
};
use tjxy_domain::{LocalMetadataAccessMode, MetadataSourceMode};
use uuid::Uuid;

use crate::{
    catalog_publication::{
        CatalogPublicationError, CatalogPublicationRepository, STATE_BUILDING, STATE_READY,
        activate_publication, advance_generation, finish, insert_change_event,
    },
    work_job::{
        ClaimedWorkJob, MetadataRequirement, WorkJobRepository, WorkJobResult, WorkJobSpec,
        WorkScope, WorkTaskKind, ensure_live_claim, fence_live_claim,
    },
};

const PUBLICATION_KIND: &str = "Sources";
const MAX_ROWS: usize = 100_000;
const MAX_BATCH_ROWS: usize = 5_000;
const MAX_SHORT_TEXT_CHARS: usize = 512;
const MAX_IDENTITY_CHARS: usize = 2048;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourcePlaybackPolicy {
    admin_priority: i32,
    is_default: bool,
    is_hidden: bool,
}

impl SourcePlaybackPolicy {
    #[must_use]
    pub const fn new(admin_priority: i32, is_default: bool, is_hidden: bool) -> Self {
        Self {
            admin_priority,
            is_default,
            is_hidden,
        }
    }

    #[must_use]
    pub const fn admin_priority(self) -> i32 {
        self.admin_priority
    }

    #[must_use]
    pub const fn is_default(self) -> bool {
        self.is_default
    }

    #[must_use]
    pub const fn is_hidden(self) -> bool {
        self.is_hidden
    }
}

#[derive(Debug, Error)]
pub enum SourcePlaybackPolicyError {
    #[error("hidden media sources cannot be the default")]
    HiddenDefault,
    #[error("media source is not available for this catalog item")]
    SourceUnavailable,
    #[error("database operation failed: {0}")]
    Database(#[from] DbErr),
    #[error("rollback failed after {original}: {rollback}")]
    RollbackFailed { original: String, rollback: DbErr },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MediaSourcePublicationRow {
    id: MediaSourceId,
    presentation_key: PresentationKey,
    edition: Option<String>,
    container: Option<String>,
    locator_kind: String,
    naming_hints: Option<Value>,
    row_sha256: String,
}

impl MediaSourcePublicationRow {
    /// Defines one stable source identity projected by Source Indexing.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogPublicationError::InvalidSourceRow`] for unbounded text.
    pub fn new(
        id: MediaSourceId,
        presentation_key: PresentationKey,
        edition: Option<String>,
        container: Option<String>,
    ) -> Result<Self, CatalogPublicationError> {
        if !valid_optional_text(edition.as_deref(), MAX_SHORT_TEXT_CHARS)
            || !valid_optional_text(container.as_deref(), 64)
        {
            return Err(CatalogPublicationError::InvalidSourceRow);
        }
        let mut row = Self {
            id,
            presentation_key,
            edition,
            container,
            locator_kind: "storage".to_owned(),
            naming_hints: None,
            row_sha256: String::new(),
        };
        row.row_sha256 = source_hash(&row);
        Ok(row)
    }

    #[must_use]
    pub const fn id(&self) -> MediaSourceId {
        self.id
    }

    #[must_use]
    pub const fn presentation_key(&self) -> PresentationKey {
        self.presentation_key
    }

    /// Marks the source as a validated storage object or a `.strm` descriptor.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogPublicationError::InvalidSourceRow`] for unknown locator kinds.
    pub fn with_locator_kind(
        mut self,
        locator_kind: impl Into<String>,
    ) -> Result<Self, CatalogPublicationError> {
        let locator_kind = locator_kind.into();
        if !matches!(locator_kind.as_str(), "storage" | "strm") {
            return Err(CatalogPublicationError::InvalidSourceRow);
        }
        self.locator_kind = locator_kind;
        self.row_sha256 = source_hash(&self);
        Ok(self)
    }

    /// Adds bounded parser evidence without treating it as probe output.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogPublicationError::InvalidSourceRow`] for non-object or oversized JSON.
    pub fn with_naming_hints(
        mut self,
        naming_hints: Value,
    ) -> Result<Self, CatalogPublicationError> {
        if !naming_hints.is_object()
            || serde_json::to_vec(&naming_hints)
                .map_err(|_| CatalogPublicationError::InvalidSourceRow)?
                .len()
                > 8_192
        {
            return Err(CatalogPublicationError::InvalidSourceRow);
        }
        self.naming_hints = Some(naming_hints);
        self.row_sha256 = source_hash(&self);
        Ok(self)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MediaLocationPublicationRow {
    id: MediaLocationId,
    media_source_id: MediaSourceId,
    storage_object_id: StorageObjectRecordId,
    content_identity: Option<String>,
    content_identity_kind: Option<String>,
    priority: i32,
    row_sha256: String,
}

impl MediaLocationPublicationRow {
    /// Defines one source-to-storage relationship in a publication.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogPublicationError::InvalidSourceRow`] for unbounded identity text.
    pub fn new(
        id: MediaLocationId,
        media_source_id: MediaSourceId,
        storage_object_id: StorageObjectRecordId,
        content_identity: Option<String>,
        content_identity_kind: Option<String>,
        priority: i32,
    ) -> Result<Self, CatalogPublicationError> {
        if !valid_optional_text(content_identity.as_deref(), MAX_IDENTITY_CHARS)
            || !valid_optional_text(content_identity_kind.as_deref(), 64)
            || content_identity.is_some() != content_identity_kind.is_some()
        {
            return Err(CatalogPublicationError::InvalidSourceRow);
        }
        let mut row = Self {
            id,
            media_source_id,
            storage_object_id,
            content_identity,
            content_identity_kind,
            priority,
            row_sha256: String::new(),
        };
        row.row_sha256 = location_hash(&row);
        Ok(row)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SubtitlePublicationRow {
    id: SubtitleId,
    media_source_id: MediaSourceId,
    storage_object_id: StorageObjectRecordId,
    format: String,
    language: Option<String>,
    delivery_index: Option<i32>,
    is_default: bool,
    is_forced: bool,
    row_sha256: String,
}

impl SubtitlePublicationRow {
    /// Defines one external subtitle identity without probing media bytes.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogPublicationError::InvalidSourceRow`] for unsafe format,
    /// language, or delivery index values.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: SubtitleId,
        media_source_id: MediaSourceId,
        storage_object_id: StorageObjectRecordId,
        format: impl Into<String>,
        language: Option<String>,
        delivery_index: Option<i32>,
        is_default: bool,
        is_forced: bool,
    ) -> Result<Self, CatalogPublicationError> {
        let format = format.into();
        if format.is_empty()
            || format.len() > 32
            || !format
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
            || !valid_optional_text(language.as_deref(), 64)
            || delivery_index.is_some_and(|index| index < 0)
        {
            return Err(CatalogPublicationError::InvalidSourceRow);
        }
        let mut row = Self {
            id,
            media_source_id,
            storage_object_id,
            format,
            language,
            delivery_index,
            is_default,
            is_forced,
            row_sha256: String::new(),
        };
        row.row_sha256 = subtitle_hash(&row);
        Ok(row)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourcePublicationManifest {
    expected_row_count: i64,
    sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SeriesSourcePublication {
    catalog_item_id: CatalogItemId,
    source_revision: i64,
    sources: Vec<MediaSourcePublicationRow>,
    locations: Vec<MediaLocationPublicationRow>,
    subtitles: Vec<SubtitlePublicationRow>,
}

impl SeriesSourcePublication {
    /// Groups the complete source projection for one Episode in a Series expansion.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogPublicationError`] when the group is unbounded or malformed.
    pub fn new(
        catalog_item_id: CatalogItemId,
        sources: Vec<MediaSourcePublicationRow>,
        locations: Vec<MediaLocationPublicationRow>,
        subtitles: Vec<SubtitlePublicationRow>,
    ) -> Result<Self, CatalogPublicationError> {
        SourcePublicationManifest::from_rows(&sources, &locations, &subtitles)?;
        if sources.is_empty() {
            return Err(CatalogPublicationError::InvalidSourceGraph);
        }
        Ok(Self {
            catalog_item_id,
            source_revision: 0,
            sources,
            locations,
            subtitles,
        })
    }

    #[must_use]
    pub const fn catalog_item_id(&self) -> CatalogItemId {
        self.catalog_item_id
    }

    /// Records the Episode source revision consumed by this Series expansion.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogPublicationError::InvalidSourceRow`] for a negative revision.
    pub fn with_source_revision(mut self, revision: i64) -> Result<Self, CatalogPublicationError> {
        if revision < 0 {
            return Err(CatalogPublicationError::InvalidSourceRow);
        }
        self.source_revision = revision;
        Ok(self)
    }

    pub(crate) fn row_count(&self) -> usize {
        self.sources.len() + self.locations.len() + self.subtitles.len()
    }
}

impl SourcePublicationManifest {
    /// Builds the complete order-independent Source projection manifest.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogPublicationError::InvalidSourceManifest`] for duplicate
    /// identities or an unbounded publication.
    pub fn from_rows(
        sources: &[MediaSourcePublicationRow],
        locations: &[MediaLocationPublicationRow],
        subtitles: &[SubtitlePublicationRow],
    ) -> Result<Self, CatalogPublicationError> {
        let expected_row_count = sources
            .len()
            .checked_add(locations.len())
            .and_then(|count| count.checked_add(subtitles.len()))
            .filter(|count| *count <= MAX_ROWS)
            .and_then(|count| i64::try_from(count).ok())
            .ok_or(CatalogPublicationError::InvalidSourceManifest)?;
        let sha256 = source_manifest_hash(
            sources
                .iter()
                .map(|row| (0_u8, row.id.as_uuid(), row.row_sha256.as_str()))
                .chain(
                    locations
                        .iter()
                        .map(|row| (1_u8, row.id.as_uuid(), row.row_sha256.as_str())),
                )
                .chain(
                    subtitles
                        .iter()
                        .map(|row| (2_u8, row.id.as_uuid(), row.row_sha256.as_str())),
                ),
        )?;
        Ok(Self {
            expected_row_count,
            sha256,
        })
    }

    pub(crate) const fn expected_row_count(&self) -> i64 {
        self.expected_row_count
    }

    pub(crate) fn sha256(&self) -> &str {
        &self.sha256
    }
}

pub(crate) fn series_source_manifest(
    groups: &[SeriesSourcePublication],
) -> Result<SourcePublicationManifest, CatalogPublicationError> {
    let expected_row_count = groups
        .iter()
        .try_fold(0_usize, |count, group| count.checked_add(group.row_count()))
        .filter(|count| *count <= MAX_ROWS)
        .and_then(|count| i64::try_from(count).ok())
        .ok_or(CatalogPublicationError::InvalidSourceManifest)?;
    let mut owners = groups
        .iter()
        .map(|group| (group.catalog_item_id, group.source_revision))
        .collect::<Vec<_>>();
    owners.sort_unstable_by_key(|entry| entry.0.as_uuid());
    if owners.windows(2).any(|pair| pair[0].0 == pair[1].0) {
        return Err(CatalogPublicationError::InvalidSourceManifest);
    }
    let mut entries =
        groups
            .iter()
            .flat_map(|group| {
                let owner = group.catalog_item_id.as_uuid();
                group
                    .sources
                    .iter()
                    .map(move |row| (0_u8, owner, row.id.as_uuid(), row.row_sha256.as_str()))
                    .chain(
                        group.locations.iter().map(move |row| {
                            (1_u8, owner, row.id.as_uuid(), row.row_sha256.as_str())
                        }),
                    )
                    .chain(
                        group.subtitles.iter().map(move |row| {
                            (2_u8, owner, row.id.as_uuid(), row.row_sha256.as_str())
                        }),
                    )
            })
            .collect::<Vec<_>>();
    entries.sort_unstable_by_key(|entry| (entry.0, entry.1, entry.2));
    if entries
        .windows(2)
        .any(|pair| (pair[0].0, pair[0].2) == (pair[1].0, pair[1].2))
    {
        return Err(CatalogPublicationError::InvalidSourceManifest);
    }
    let mut digest = Sha256::new();
    for (owner, revision) in owners {
        digest.update(owner.as_uuid().as_bytes());
        digest.update(revision.to_be_bytes());
    }
    for (kind, owner, id, hash) in entries {
        digest.update([kind]);
        digest.update(owner.as_bytes());
        digest.update(id.as_bytes());
        digest.update(hash.as_bytes());
    }
    Ok(SourcePublicationManifest {
        expected_row_count,
        sha256: format!("{:x}", digest.finalize()),
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishedMediaLocation {
    id: MediaLocationId,
    storage_object_id: StorageObjectRecordId,
    priority: i32,
    availability_state: String,
    account_status: String,
}

impl PublishedMediaLocation {
    #[must_use]
    pub const fn id(&self) -> MediaLocationId {
        self.id
    }

    #[must_use]
    pub const fn storage_object_id(&self) -> StorageObjectRecordId {
        self.storage_object_id
    }

    #[must_use]
    pub const fn priority(&self) -> i32 {
        self.priority
    }

    #[must_use]
    pub fn availability_state(&self) -> &str {
        &self.availability_state
    }

    #[must_use]
    pub fn account_status(&self) -> &str {
        &self.account_status
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishedSubtitle {
    id: SubtitleId,
    storage_object_id: StorageObjectRecordId,
    format: String,
    language: Option<String>,
    delivery_index: Option<i32>,
    is_default: bool,
    is_forced: bool,
}

impl PublishedSubtitle {
    #[must_use]
    pub const fn id(&self) -> SubtitleId {
        self.id
    }

    #[must_use]
    pub const fn storage_object_id(&self) -> StorageObjectRecordId {
        self.storage_object_id
    }

    #[must_use]
    pub const fn delivery_index(&self) -> Option<i32> {
        self.delivery_index
    }

    #[must_use]
    pub fn format(&self) -> &str {
        &self.format
    }

    #[must_use]
    pub fn language(&self) -> Option<&str> {
        self.language.as_deref()
    }

    #[must_use]
    pub const fn is_default(&self) -> bool {
        self.is_default
    }

    #[must_use]
    pub const fn is_forced(&self) -> bool {
        self.is_forced
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishedMediaSource {
    id: MediaSourceId,
    presentation_key: PresentationKey,
    edition: Option<String>,
    container: Option<String>,
    locator_kind: String,
    probe_state: String,
    probe_revision: i64,
    bitrate: Option<i64>,
    runtime_ticks: Option<i64>,
    admin_priority: i32,
    is_default: bool,
    is_hidden: bool,
    locations: Vec<PublishedMediaLocation>,
    streams: Vec<PublishedMediaStream>,
    subtitles: Vec<PublishedSubtitle>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishedMediaStream {
    stream_type: String,
    codec: Option<String>,
    language: Option<String>,
    delivery_index: i32,
    is_default: bool,
    is_forced: bool,
    width: Option<i32>,
    height: Option<i32>,
    channels: Option<i32>,
    profile: Option<String>,
    level: Option<i32>,
}

impl PublishedMediaStream {
    #[must_use]
    pub fn stream_type(&self) -> &str {
        &self.stream_type
    }

    #[must_use]
    pub fn codec(&self) -> Option<&str> {
        self.codec.as_deref()
    }

    #[must_use]
    pub fn language(&self) -> Option<&str> {
        self.language.as_deref()
    }

    #[must_use]
    pub const fn delivery_index(&self) -> i32 {
        self.delivery_index
    }

    #[must_use]
    pub const fn is_default(&self) -> bool {
        self.is_default
    }

    #[must_use]
    pub const fn is_forced(&self) -> bool {
        self.is_forced
    }

    #[must_use]
    pub const fn width(&self) -> Option<i32> {
        self.width
    }

    #[must_use]
    pub const fn height(&self) -> Option<i32> {
        self.height
    }

    #[must_use]
    pub const fn channels(&self) -> Option<i32> {
        self.channels
    }

    #[must_use]
    pub fn profile(&self) -> Option<&str> {
        self.profile.as_deref()
    }

    #[must_use]
    pub const fn level(&self) -> Option<i32> {
        self.level
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlaybackLocation {
    storage_object_id: StorageObjectRecordId,
    storage_account_id: Uuid,
    provider: String,
    provider_object_id: String,
    size: u64,
    remote_revision: Option<String>,
    container: Option<String>,
    locator_kind: String,
    is_audio: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlaybackSubtitleLocation {
    location: PlaybackLocation,
    format: String,
}

impl PlaybackSubtitleLocation {
    #[must_use]
    pub const fn location(&self) -> &PlaybackLocation {
        &self.location
    }

    #[must_use]
    pub fn format(&self) -> &str {
        &self.format
    }
}

impl PlaybackLocation {
    #[must_use]
    pub const fn storage_object_id(&self) -> StorageObjectRecordId {
        self.storage_object_id
    }

    #[must_use]
    pub const fn storage_account_id(&self) -> Uuid {
        self.storage_account_id
    }

    #[must_use]
    pub fn provider(&self) -> &str {
        &self.provider
    }

    #[must_use]
    pub fn provider_object_id(&self) -> &str {
        &self.provider_object_id
    }

    #[must_use]
    pub const fn size(&self) -> u64 {
        self.size
    }

    #[must_use]
    pub fn remote_revision(&self) -> Option<&str> {
        self.remote_revision.as_deref()
    }

    #[must_use]
    pub fn container(&self) -> Option<&str> {
        self.container.as_deref()
    }

    #[must_use]
    pub fn locator_kind(&self) -> &str {
        &self.locator_kind
    }

    #[must_use]
    pub const fn is_audio(&self) -> bool {
        self.is_audio
    }
}

impl PublishedMediaSource {
    #[must_use]
    pub const fn id(&self) -> MediaSourceId {
        self.id
    }

    #[must_use]
    pub const fn presentation_key(&self) -> PresentationKey {
        self.presentation_key
    }

    #[must_use]
    pub fn probe_state(&self) -> &str {
        &self.probe_state
    }

    #[must_use]
    pub const fn probe_revision(&self) -> i64 {
        self.probe_revision
    }

    #[must_use]
    pub const fn bitrate(&self) -> Option<i64> {
        self.bitrate
    }

    #[must_use]
    pub const fn runtime_ticks(&self) -> Option<i64> {
        self.runtime_ticks
    }

    #[must_use]
    pub const fn admin_priority(&self) -> i32 {
        self.admin_priority
    }

    #[must_use]
    pub const fn is_default(&self) -> bool {
        self.is_default
    }

    #[must_use]
    pub const fn is_hidden(&self) -> bool {
        self.is_hidden
    }

    #[must_use]
    pub fn edition(&self) -> Option<&str> {
        self.edition.as_deref()
    }

    #[must_use]
    pub fn container(&self) -> Option<&str> {
        self.container.as_deref()
    }

    #[must_use]
    pub fn locator_kind(&self) -> &str {
        &self.locator_kind
    }

    #[must_use]
    pub fn locations(&self) -> &[PublishedMediaLocation] {
        &self.locations
    }

    #[must_use]
    pub fn streams(&self) -> &[PublishedMediaStream] {
        &self.streams
    }

    #[must_use]
    pub fn subtitles(&self) -> &[PublishedSubtitle] {
        &self.subtitles
    }
}

impl CatalogPublicationRepository<'_> {
    /// Creates or resumes a Source publication owned by a live Index job.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogPublicationError`] for incompatible work, manifest, or SQL state.
    pub async fn begin_sources(
        &self,
        claimed: &ClaimedWorkJob,
        manifest: &SourcePublicationManifest,
    ) -> Result<PublicationId, CatalogPublicationError> {
        let owner = source_owner(claimed)?;
        let transaction = self.database.begin().await?;
        let result = begin_sources(&transaction, claimed, owner, manifest, Utc::now()).await;
        finish(transaction, result).await
    }

    /// Idempotently writes one bounded Source projection batch under a live lease.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogPublicationError`] for invalid ownership, lease, or rows.
    pub async fn stage_source_batch(
        &self,
        claimed: &ClaimedWorkJob,
        publication_id: PublicationId,
        sources: &[MediaSourcePublicationRow],
        locations: &[MediaLocationPublicationRow],
        subtitles: &[SubtitlePublicationRow],
    ) -> Result<(), CatalogPublicationError> {
        sources
            .len()
            .checked_add(locations.len())
            .and_then(|count| count.checked_add(subtitles.len()))
            .filter(|count| *count <= MAX_BATCH_ROWS)
            .ok_or(CatalogPublicationError::InvalidSourceRow)?;
        let transaction = self.database.begin().await?;
        let result = stage_source_batch(
            &transaction,
            claimed,
            publication_id,
            sources,
            locations,
            subtitles,
            Utc::now(),
        )
        .await;
        let result = match result {
            Ok(()) => fence_live_claim(&transaction, claimed, Utc::now())
                .await
                .map_err(Into::into),
            Err(error) => Err(error),
        };
        finish(transaction, result).await
    }

    /// Stages bounded Episode source groups inside the owning Series publication.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogPublicationError`] for invalid work, ownership, or rows.
    pub async fn stage_structure_source_batch(
        &self,
        claimed: &ClaimedWorkJob,
        publication_id: PublicationId,
        groups: &[SeriesSourcePublication],
    ) -> Result<(), CatalogPublicationError> {
        groups
            .iter()
            .try_fold(0_usize, |count, group| count.checked_add(group.row_count()))
            .filter(|count| *count <= MAX_BATCH_ROWS)
            .ok_or(CatalogPublicationError::InvalidSourceRow)?;
        let transaction = self.database.begin().await?;
        let result =
            stage_structure_source_batch(&transaction, claimed, publication_id, groups, Utc::now())
                .await;
        let result = match result {
            Ok(()) => fence_live_claim(&transaction, claimed, Utc::now())
                .await
                .map_err(Into::into),
            Err(error) => Err(error),
        };
        finish(transaction, result).await
    }

    /// Validates, materializes stable identities, and freezes a Source projection.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogPublicationError`] when the manifest or relationship graph is invalid.
    pub async fn seal_sources(
        &self,
        claimed: &ClaimedWorkJob,
        publication_id: PublicationId,
    ) -> Result<(), CatalogPublicationError> {
        let transaction = self.database.begin().await?;
        let result = seal_sources(&transaction, claimed, publication_id, Utc::now()).await;
        let result = match result {
            Ok(()) => fence_live_claim(&transaction, claimed, Utc::now())
                .await
                .map_err(Into::into),
            Err(error) => Err(error),
        };
        finish(transaction, result).await
    }

    /// Atomically activates a sealed Source publication and completes its work job.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogPublicationError`] and rolls back the whole switch on any failed fence.
    pub async fn publish_sources(
        &self,
        jobs: &WorkJobRepository<'_>,
        claimed: &ClaimedWorkJob,
        publication_id: PublicationId,
    ) -> Result<i64, CatalogPublicationError> {
        let transaction = self.database.begin().await?;
        let result = publish_sources(&transaction, jobs, claimed, publication_id, Utc::now()).await;
        finish(transaction, result).await
    }

    /// Returns only the immutable projection selected by the item's active Source pointer.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogPublicationError`] for database or stored-value corruption.
    pub async fn active_sources(
        &self,
        owner: CatalogItemId,
    ) -> Result<Vec<PublishedMediaSource>, CatalogPublicationError> {
        active_sources(self.database, owner).await
    }

    /// Persists the administrator playback policy for one stable source identity.
    ///
    /// A default source is exclusive within its catalog item. Every successful
    /// update advances the catalog generation so cached `PlaybackInfo` projections
    /// cannot retain an obsolete order or hidden source.
    ///
    /// # Errors
    ///
    /// Returns [`SourcePlaybackPolicyError`] for contradictory policy values,
    /// unknown sources, or transaction failures.
    pub async fn set_source_playback_policy(
        &self,
        owner: CatalogItemId,
        presentation_key: PresentationKey,
        policy: SourcePlaybackPolicy,
    ) -> Result<(), SourcePlaybackPolicyError> {
        if policy.is_hidden() && policy.is_default() {
            return Err(SourcePlaybackPolicyError::HiddenDefault);
        }
        let transaction = self.database.begin().await?;
        let result =
            set_source_playback_policy(&transaction, owner, presentation_key, policy).await;
        match result {
            Ok(()) => transaction.commit().await.map_err(Into::into),
            Err(error) => match transaction.rollback().await {
                Ok(()) => Err(error),
                Err(rollback) => Err(SourcePlaybackPolicyError::RollbackFailed {
                    original: error.to_string(),
                    rollback,
                }),
            },
        }
    }

    /// Resolves the preferred present location for one active presentation key.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogPublicationError`] for database or stored-value corruption.
    pub async fn playback_location(
        &self,
        owner: CatalogItemId,
        presentation_key: PresentationKey,
    ) -> Result<Option<PlaybackLocation>, CatalogPublicationError> {
        playback_location(self.database, owner, presentation_key).await
    }

    /// Lists storage accounts attached to an enabled library containing this item.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogPublicationError`] for database failures.
    pub async fn playback_storage_accounts(
        &self,
        owner: CatalogItemId,
    ) -> Result<Vec<Uuid>, CatalogPublicationError> {
        playback_storage_accounts(self.database, owner).await
    }

    /// Resolves one active external subtitle by stable presentation and delivery index.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogPublicationError`] for database or stored-value corruption.
    pub async fn subtitle_location(
        &self,
        owner: CatalogItemId,
        presentation_key: PresentationKey,
        delivery_index: i32,
    ) -> Result<Option<PlaybackSubtitleLocation>, CatalogPublicationError> {
        if delivery_index < 0 {
            return Ok(None);
        }
        subtitle_location(self.database, owner, presentation_key, delivery_index).await
    }
}

async fn playback_storage_accounts(
    database: &sea_orm::DatabaseConnection,
    owner: CatalogItemId,
) -> Result<Vec<Uuid>, CatalogPublicationError> {
    let item = Alias::new("strm_item");
    let structure_owner = Alias::new("strm_structure_owner");
    let membership = Alias::new("strm_membership");
    let library = Alias::new("strm_library");
    let library_root = Alias::new("strm_library_root");
    let root = Alias::new("strm_root");
    let account = Alias::new("strm_account");
    let query = Query::select()
        .distinct()
        .expr_as(
            Expr::col((account.clone(), Alias::new("id"))),
            Alias::new("storage_account_id"),
        )
        .from_as(Alias::new("catalog_items"), item.clone())
        .join_as(
            JoinType::LeftJoin,
            Alias::new("catalog_items"),
            structure_owner.clone(),
            Expr::col((structure_owner.clone(), Alias::new("id")))
                .equals((item.clone(), Alias::new("structure_owner_item_id"))),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("library_catalog_items"),
            membership.clone(),
            Cond::any()
                .add(
                    Expr::col((membership.clone(), Alias::new("catalog_item_id")))
                        .equals((item.clone(), Alias::new("id"))),
                )
                .add(
                    Expr::col((membership.clone(), Alias::new("catalog_item_id")))
                        .equals((structure_owner.clone(), Alias::new("id"))),
                ),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("libraries"),
            library.clone(),
            Expr::col((library.clone(), Alias::new("id")))
                .equals((membership.clone(), Alias::new("library_id"))),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("library_storage_roots"),
            library_root.clone(),
            Expr::col((library_root.clone(), Alias::new("library_id")))
                .equals((library.clone(), Alias::new("id"))),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("storage_roots"),
            root.clone(),
            Expr::col((root.clone(), Alias::new("id")))
                .equals((library_root.clone(), Alias::new("storage_root_id"))),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("storage_accounts"),
            account.clone(),
            Expr::col((account.clone(), Alias::new("id")))
                .equals((root.clone(), Alias::new("storage_account_id"))),
        )
        .and_where(Expr::col((item, Alias::new("id"))).eq(owner.as_uuid()))
        .and_where(Expr::col((library, Alias::new("is_enabled"))).eq(true))
        .and_where(Expr::col((account, Alias::new("status"))).is_in(["Active", "Ready"]))
        .to_owned();
    let backend = database.get_database_backend();
    database
        .query_all(backend.build(&query))
        .await?
        .iter()
        .map(|row| row.try_get("", "storage_account_id").map_err(Into::into))
        .collect()
}

async fn set_source_playback_policy(
    transaction: &DatabaseTransaction,
    owner: CatalogItemId,
    presentation_key: PresentationKey,
    policy: SourcePlaybackPolicy,
) -> Result<(), SourcePlaybackPolicyError> {
    let backend = transaction.get_database_backend();
    let source = transaction
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("id"))
                    .from(Alias::new("media_sources"))
                    .and_where(Expr::col(Alias::new("catalog_item_id")).eq(owner.as_uuid()))
                    .and_where(
                        Expr::col(Alias::new("presentation_key")).eq(presentation_key.as_uuid()),
                    ),
            ),
        )
        .await?
        .ok_or(SourcePlaybackPolicyError::SourceUnavailable)?;
    let source_id: Uuid = source.try_get("", "id")?;
    if policy.is_default() {
        transaction
            .execute(
                backend.build(
                    Query::update()
                        .table(Alias::new("media_sources"))
                        .value(Alias::new("is_default"), false)
                        .and_where(Expr::col(Alias::new("catalog_item_id")).eq(owner.as_uuid())),
                ),
            )
            .await?;
    }
    let update = Query::update()
        .table(Alias::new("media_sources"))
        .value(Alias::new("admin_priority"), policy.admin_priority())
        .value(Alias::new("is_default"), policy.is_default())
        .value(Alias::new("is_hidden"), policy.is_hidden())
        .and_where(Expr::col(Alias::new("id")).eq(source_id))
        .to_owned();
    if transaction
        .execute(backend.build(&update))
        .await?
        .rows_affected()
        != 1
    {
        return Err(SourcePlaybackPolicyError::SourceUnavailable);
    }
    crate::advance_catalog_generation(transaction).await?;
    Ok(())
}

async fn stage_structure_source_batch(
    transaction: &DatabaseTransaction,
    claimed: &ClaimedWorkJob,
    publication_id: PublicationId,
    groups: &[SeriesSourcePublication],
    now: DateTime<Utc>,
) -> Result<(), CatalogPublicationError> {
    if claimed.job().task_kind() != WorkTaskKind::ExpandItem {
        return Err(CatalogPublicationError::InvalidWorkKind);
    }
    ensure_live_claim(transaction, claimed, now).await?;
    let row = transaction
        .query_one(
            transaction
                .get_database_backend()
                .build(&publication_by_id(publication_id)),
        )
        .await?
        .ok_or(CatalogPublicationError::InvalidPublication)?;
    if row.try_get::<Uuid>("", "job_id")? != claimed.id().as_uuid()
        || row.try_get::<String>("", "publication_kind")? != "Structure"
        || row.try_get::<String>("", "state")? != STATE_BUILDING
    {
        return Err(CatalogPublicationError::InvalidPublication);
    }
    for group in groups {
        let update = Query::update()
            .table(Alias::new("publication_catalog_items"))
            .value(Alias::new("source_state"), "Indexed")
            .value(Alias::new("source_index_revision"), group.source_revision)
            .and_where(Expr::col(Alias::new("publication_id")).eq(publication_id.as_uuid()))
            .and_where(Expr::col(Alias::new("catalog_item_id")).eq(group.catalog_item_id.as_uuid()))
            .and_where(Expr::col(Alias::new("item_type")).eq("Episode"))
            .to_owned();
        if transaction
            .execute(transaction.get_database_backend().build(&update))
            .await?
            .rows_affected()
            != 1
        {
            return Err(CatalogPublicationError::InvalidSourceGraph);
        }
        stage_source_rows(
            transaction,
            publication_id,
            group.catalog_item_id,
            &group.sources,
        )
        .await?;
        stage_location_rows(transaction, publication_id, &group.locations).await?;
        stage_subtitle_rows(transaction, publication_id, &group.subtitles).await?;
    }
    Ok(())
}

async fn begin_sources(
    transaction: &DatabaseTransaction,
    claimed: &ClaimedWorkJob,
    owner: CatalogItemId,
    manifest: &SourcePublicationManifest,
    now: DateTime<Utc>,
) -> Result<PublicationId, CatalogPublicationError> {
    ensure_live_claim(transaction, claimed, now).await?;
    let publication_id = PublicationId::new();
    let backend = transaction.get_database_backend();
    let insert = Query::insert()
        .into_table(Alias::new("catalog_publications"))
        .columns([
            Alias::new("id"),
            Alias::new("job_id"),
            Alias::new("owner_catalog_item_id"),
            Alias::new("publication_kind"),
            Alias::new("expected_revision"),
            Alias::new("input_sync_revision"),
            Alias::new("state"),
            Alias::new("manifest_sha256"),
            Alias::new("expected_row_count"),
            Alias::new("naming_parser_version"),
            Alias::new("created_at"),
        ])
        .values_panic([
            publication_id.as_uuid().into(),
            claimed.id().as_uuid().into(),
            owner.as_uuid().into(),
            PUBLICATION_KIND.into(),
            claimed.job().expected_revision().into(),
            claimed.job().input_sync_revision().into(),
            STATE_BUILDING.into(),
            manifest.sha256.clone().into(),
            manifest.expected_row_count.into(),
            MEDIA_NAME_PARSER_VERSION.into(),
            now.into(),
        ])
        .on_conflict(idempotent_conflict(backend, "job_id"))
        .to_owned();
    transaction.execute(backend.build(&insert)).await?;
    let row = transaction
        .query_one(backend.build(&publication_for_job(claimed.id().as_uuid())))
        .await?
        .ok_or(CatalogPublicationError::InvalidPublication)?;
    validate_source_publication(&row, claimed, owner, manifest)
}

fn validate_source_publication(
    row: &QueryResult,
    claimed: &ClaimedWorkJob,
    owner: CatalogItemId,
    manifest: &SourcePublicationManifest,
) -> Result<PublicationId, CatalogPublicationError> {
    if row.try_get::<Uuid>("", "job_id")? != claimed.id().as_uuid()
        || row.try_get::<Uuid>("", "owner_catalog_item_id")? != owner.as_uuid()
        || row.try_get::<String>("", "publication_kind")? != PUBLICATION_KIND
        || row.try_get::<i64>("", "expected_revision")? != claimed.job().expected_revision()
        || row.try_get::<Option<i64>>("", "input_sync_revision")?
            != claimed.job().input_sync_revision()
        || !matches!(
            row.try_get::<String>("", "state")?.as_str(),
            STATE_BUILDING | STATE_READY
        )
        || row.try_get::<String>("", "manifest_sha256")? != manifest.sha256
        || row.try_get::<i64>("", "expected_row_count")? != manifest.expected_row_count
        || row.try_get::<i32>("", "naming_parser_version")? != MEDIA_NAME_PARSER_VERSION
    {
        return Err(CatalogPublicationError::InvalidPublication);
    }
    Ok(PublicationId::from_uuid(row.try_get("", "id")?))
}

#[allow(clippy::too_many_arguments)]
async fn stage_source_batch(
    transaction: &DatabaseTransaction,
    claimed: &ClaimedWorkJob,
    publication_id: PublicationId,
    sources: &[MediaSourcePublicationRow],
    locations: &[MediaLocationPublicationRow],
    subtitles: &[SubtitlePublicationRow],
    now: DateTime<Utc>,
) -> Result<(), CatalogPublicationError> {
    let owner = source_owner(claimed)?;
    ensure_live_claim(transaction, claimed, now).await?;
    load_source_publication(transaction, claimed, publication_id, STATE_BUILDING).await?;
    stage_source_rows(transaction, publication_id, owner, sources).await?;
    stage_location_rows(transaction, publication_id, locations).await?;
    stage_subtitle_rows(transaction, publication_id, subtitles).await
}

async fn stage_source_rows(
    transaction: &DatabaseTransaction,
    publication_id: PublicationId,
    owner: CatalogItemId,
    rows: &[MediaSourcePublicationRow],
) -> Result<(), CatalogPublicationError> {
    let backend = transaction.get_database_backend();
    for row in rows {
        let insert = Query::insert()
            .into_table(Alias::new("publication_media_sources"))
            .columns([
                Alias::new("id"),
                Alias::new("publication_id"),
                Alias::new("media_source_id"),
                Alias::new("catalog_item_id"),
                Alias::new("presentation_key"),
                Alias::new("edition"),
                Alias::new("container"),
                Alias::new("locator_kind"),
                Alias::new("naming_hints"),
                Alias::new("row_sha256"),
            ])
            .values_panic([
                Uuid::new_v4().into(),
                publication_id.as_uuid().into(),
                row.id.as_uuid().into(),
                owner.as_uuid().into(),
                row.presentation_key.as_uuid().into(),
                row.edition.clone().into(),
                row.container.clone().into(),
                row.locator_kind.clone().into(),
                row.naming_hints.clone().into(),
                row.row_sha256.clone().into(),
            ])
            .on_conflict(
                OnConflict::columns([Alias::new("publication_id"), Alias::new("media_source_id")])
                    .update_columns([
                        Alias::new("presentation_key"),
                        Alias::new("edition"),
                        Alias::new("container"),
                        Alias::new("locator_kind"),
                        Alias::new("naming_hints"),
                        Alias::new("row_sha256"),
                    ])
                    .to_owned(),
            )
            .to_owned();
        transaction.execute(backend.build(&insert)).await?;
    }
    Ok(())
}

async fn stage_location_rows(
    transaction: &DatabaseTransaction,
    publication_id: PublicationId,
    rows: &[MediaLocationPublicationRow],
) -> Result<(), CatalogPublicationError> {
    let backend = transaction.get_database_backend();
    for row in rows {
        let insert = Query::insert()
            .into_table(Alias::new("publication_media_locations"))
            .columns([
                Alias::new("id"),
                Alias::new("publication_id"),
                Alias::new("media_location_id"),
                Alias::new("media_source_id"),
                Alias::new("storage_object_id"),
                Alias::new("content_identity"),
                Alias::new("content_identity_kind"),
                Alias::new("priority"),
                Alias::new("row_sha256"),
            ])
            .values_panic([
                Uuid::new_v4().into(),
                publication_id.as_uuid().into(),
                row.id.as_uuid().into(),
                row.media_source_id.as_uuid().into(),
                row.storage_object_id.as_uuid().into(),
                row.content_identity.clone().into(),
                row.content_identity_kind.clone().into(),
                row.priority.into(),
                row.row_sha256.clone().into(),
            ])
            .on_conflict(
                OnConflict::columns([
                    Alias::new("publication_id"),
                    Alias::new("media_location_id"),
                ])
                .update_columns([
                    Alias::new("media_source_id"),
                    Alias::new("storage_object_id"),
                    Alias::new("content_identity"),
                    Alias::new("content_identity_kind"),
                    Alias::new("priority"),
                    Alias::new("row_sha256"),
                ])
                .to_owned(),
            )
            .to_owned();
        transaction.execute(backend.build(&insert)).await?;
    }
    Ok(())
}

async fn stage_subtitle_rows(
    transaction: &DatabaseTransaction,
    publication_id: PublicationId,
    rows: &[SubtitlePublicationRow],
) -> Result<(), CatalogPublicationError> {
    let backend = transaction.get_database_backend();
    for row in rows {
        let insert = Query::insert()
            .into_table(Alias::new("publication_subtitles"))
            .columns([
                Alias::new("id"),
                Alias::new("publication_id"),
                Alias::new("subtitle_id"),
                Alias::new("media_source_id"),
                Alias::new("storage_object_id"),
                Alias::new("format"),
                Alias::new("language"),
                Alias::new("delivery_index"),
                Alias::new("is_default"),
                Alias::new("is_forced"),
                Alias::new("row_sha256"),
            ])
            .values_panic([
                Uuid::new_v4().into(),
                publication_id.as_uuid().into(),
                row.id.as_uuid().into(),
                row.media_source_id.as_uuid().into(),
                row.storage_object_id.as_uuid().into(),
                row.format.clone().into(),
                row.language.clone().into(),
                row.delivery_index.into(),
                row.is_default.into(),
                row.is_forced.into(),
                row.row_sha256.clone().into(),
            ])
            .on_conflict(
                OnConflict::columns([Alias::new("publication_id"), Alias::new("subtitle_id")])
                    .update_columns([
                        Alias::new("media_source_id"),
                        Alias::new("storage_object_id"),
                        Alias::new("format"),
                        Alias::new("language"),
                        Alias::new("delivery_index"),
                        Alias::new("is_default"),
                        Alias::new("is_forced"),
                        Alias::new("row_sha256"),
                    ])
                    .to_owned(),
            )
            .to_owned();
        transaction.execute(backend.build(&insert)).await?;
    }
    Ok(())
}

async fn seal_sources(
    transaction: &DatabaseTransaction,
    claimed: &ClaimedWorkJob,
    publication_id: PublicationId,
    now: DateTime<Utc>,
) -> Result<(), CatalogPublicationError> {
    let owner = source_owner(claimed)?;
    ensure_live_claim(transaction, claimed, now).await?;
    let publication =
        load_source_publication(transaction, claimed, publication_id, STATE_BUILDING).await?;
    let sources = load_source_rows(transaction, publication_id).await?;
    let locations = load_location_rows(transaction, publication_id).await?;
    let subtitles = load_subtitle_rows(transaction, publication_id).await?;
    validate_source_projection(&publication, &sources, &locations, &subtitles)?;
    ensure_storage_authorized(transaction, owner, &locations, &subtitles).await?;
    let update = Query::update()
        .table(Alias::new("catalog_publications"))
        .value(Alias::new("state"), STATE_READY)
        .value(Alias::new("sealed_at"), now)
        .and_where(Expr::col(Alias::new("id")).eq(publication_id.as_uuid()))
        .and_where(Expr::col(Alias::new("state")).eq(STATE_BUILDING))
        .to_owned();
    let backend = transaction.get_database_backend();
    if transaction
        .execute(backend.build(&update))
        .await?
        .rows_affected()
        != 1
    {
        return Err(CatalogPublicationError::InvalidPublication);
    }
    Ok(())
}

pub(crate) async fn seal_structure_sources(
    transaction: &DatabaseTransaction,
    publication_id: PublicationId,
    series_owner: CatalogItemId,
    episode_ids: &HashSet<CatalogItemId>,
    expected_sha256: Option<&str>,
    expected_row_count: Option<i64>,
) -> Result<(), CatalogPublicationError> {
    let groups = load_series_source_groups(transaction, publication_id).await?;
    if expected_sha256.is_none() && expected_row_count.is_none() {
        return if groups.is_empty() {
            Ok(())
        } else {
            Err(CatalogPublicationError::ManifestMismatch)
        };
    }
    let actual = series_source_manifest(&groups)?;
    if expected_sha256 != Some(actual.sha256())
        || expected_row_count != Some(actual.expected_row_count())
    {
        return Err(CatalogPublicationError::ManifestMismatch);
    }
    let group_owners = groups
        .iter()
        .map(|group| group.catalog_item_id)
        .collect::<HashSet<_>>();
    if &group_owners != episode_ids {
        return Err(CatalogPublicationError::InvalidSourceGraph);
    }
    for group in &groups {
        if !episode_ids.contains(&group.catalog_item_id) {
            return Err(CatalogPublicationError::InvalidSourceGraph);
        }
        let manifest = SourcePublicationManifest::from_rows(
            &group.sources,
            &group.locations,
            &group.subtitles,
        )?;
        let stored = StoredSourcePublication {
            expected_revision: 0,
            expected_row_count: manifest.expected_row_count,
            manifest_sha256: manifest.sha256,
        };
        validate_source_projection(&stored, &group.sources, &group.locations, &group.subtitles)?;
        ensure_storage_authorized(
            transaction,
            series_owner,
            &group.locations,
            &group.subtitles,
        )
        .await?;
    }
    Ok(())
}

pub(crate) async fn ensure_structure_storage_authorized(
    transaction: &DatabaseTransaction,
    publication_id: PublicationId,
    series_owner: CatalogItemId,
) -> Result<(), CatalogPublicationError> {
    ensure_structure_scope_objects_authorized(transaction, publication_id, series_owner).await?;
    let groups = load_series_source_groups(transaction, publication_id).await?;
    let locations = groups
        .iter()
        .flat_map(|group| group.locations.iter().cloned())
        .collect::<Vec<_>>();
    let subtitles = groups
        .iter()
        .flat_map(|group| group.subtitles.iter().cloned())
        .collect::<Vec<_>>();
    if !locations.is_empty() || !subtitles.is_empty() {
        ensure_storage_authorized(transaction, series_owner, &locations, &subtitles).await?;
    }
    Ok(())
}

pub(crate) async fn ensure_structure_storage_reconciled(
    transaction: &DatabaseTransaction,
    publication_id: PublicationId,
    series_owner: CatalogItemId,
    claimed: &ClaimedWorkJob,
) -> Result<(), CatalogPublicationError> {
    let query = Query::select()
        .columns([
            Alias::new("storage_root_id"),
            Alias::new("scope_storage_object_id"),
        ])
        .from(Alias::new("publication_catalog_items"))
        .and_where(Expr::col(Alias::new("publication_id")).eq(publication_id.as_uuid()))
        .distinct()
        .to_owned();
    let backend = transaction.get_database_backend();
    let scope_pairs = transaction
        .query_all(backend.build(&query))
        .await?
        .iter()
        .map(|row| {
            let root = row
                .try_get::<Option<Uuid>>("", "storage_root_id")?
                .ok_or(CatalogPublicationError::UnauthorizedStorageObject)?;
            let object = row
                .try_get::<Option<Uuid>>("", "scope_storage_object_id")?
                .ok_or(CatalogPublicationError::UnauthorizedStorageObject)?;
            Ok((
                StorageRootId::from_uuid(root),
                StorageObjectRecordId::from_uuid(object),
            ))
        })
        .collect::<Result<Vec<_>, CatalogPublicationError>>()?;
    if !crate::catalog_storage_scope::storage_scope_pairs_are_reconciled(transaction, &scope_pairs)
        .await?
    {
        return Err(CatalogPublicationError::StorageInputPending);
    }
    let owner_scope = crate::catalog_storage_scope::resolve_catalog_storage_scope(
        transaction,
        series_owner,
        claimed.job().storage_root_affinity(),
    )
    .await
    .map_err(|error| match error {
        crate::catalog_storage_scope::CatalogStorageScopeError::Ambiguous => {
            CatalogPublicationError::UnauthorizedStorageObject
        }
        crate::catalog_storage_scope::CatalogStorageScopeError::Database(error) => {
            CatalogPublicationError::Database(error)
        }
    })?;
    if let Some(scope) = owner_scope
        && !crate::catalog_storage_scope::storage_scope_is_reconciled(transaction, scope, true)
            .await?
    {
        return Err(CatalogPublicationError::StorageInputPending);
    }
    let groups = load_series_source_groups(transaction, publication_id).await?;
    let locations = groups
        .iter()
        .flat_map(|group| group.locations.iter().cloned())
        .collect::<Vec<_>>();
    let subtitles = groups
        .iter()
        .flat_map(|group| group.subtitles.iter().cloned())
        .collect::<Vec<_>>();
    if !locations.is_empty() || !subtitles.is_empty() {
        ensure_storage_authorized_and_reconciled(
            transaction,
            series_owner,
            claimed.job().storage_root_affinity(),
            &locations,
            &subtitles,
        )
        .await?;
    }
    Ok(())
}

#[allow(clippy::too_many_lines)] // Keeps publication, owner, root, and object authorization fences auditable together.
async fn ensure_structure_scope_objects_authorized(
    transaction: &DatabaseTransaction,
    publication_id: PublicationId,
    owner: CatalogItemId,
) -> Result<(), CatalogPublicationError> {
    let query = Query::select()
        .columns([
            Alias::new("storage_root_id"),
            Alias::new("scope_storage_object_id"),
        ])
        .from(Alias::new("publication_catalog_items"))
        .and_where(Expr::col(Alias::new("publication_id")).eq(publication_id.as_uuid()))
        .distinct()
        .to_owned();
    let backend = transaction.get_database_backend();
    let requested = transaction
        .query_all(backend.build(&query))
        .await?
        .iter()
        .map(|row| {
            let root = row
                .try_get::<Option<Uuid>>("", "storage_root_id")?
                .ok_or(CatalogPublicationError::UnauthorizedStorageObject)?;
            let object = row
                .try_get::<Option<Uuid>>("", "scope_storage_object_id")?
                .ok_or(CatalogPublicationError::UnauthorizedStorageObject)?;
            Ok((root, object))
        })
        .collect::<Result<HashSet<_>, CatalogPublicationError>>()?;
    if requested.is_empty() {
        return Err(CatalogPublicationError::UnauthorizedStorageObject);
    }
    let owner_query = Query::select()
        .column(Alias::new("structure_owner_item_id"))
        .from(Alias::new("catalog_items"))
        .and_where(Expr::col(Alias::new("id")).eq(owner.as_uuid()))
        .to_owned();
    let owner_row = transaction
        .query_one(backend.build(&owner_query))
        .await?
        .ok_or(CatalogPublicationError::UnauthorizedStorageObject)?;
    let mut authorized_items = vec![owner.as_uuid()];
    if let Some(lineage_owner) = owner_row.try_get::<Option<Uuid>>("", "structure_owner_item_id")? {
        authorized_items.push(lineage_owner);
    }
    let requested = requested.into_iter().collect::<Vec<_>>();
    for requested in requested.chunks(MAX_BATCH_ROWS) {
        let relation = Alias::new("authorized_structure_scope");
        let object = Alias::new("authorized_structure_object");
        let library_root = Alias::new("authorized_structure_library_root");
        let membership = Alias::new("authorized_structure_membership");
        let library = Alias::new("authorized_structure_library");
        let roots = requested.iter().map(|(root, _)| *root).collect::<Vec<_>>();
        let objects = requested
            .iter()
            .map(|(_, object)| *object)
            .collect::<Vec<_>>();
        let query = Query::select()
            .expr_as(
                Expr::col((relation.clone(), Alias::new("storage_root_id"))),
                Alias::new("storage_root_id"),
            )
            .expr_as(
                Expr::col((relation.clone(), Alias::new("storage_object_id"))),
                Alias::new("storage_object_id"),
            )
            .from_as(Alias::new("storage_root_objects"), relation.clone())
            .join_as(
                sea_orm::sea_query::JoinType::InnerJoin,
                Alias::new("storage_objects"),
                object.clone(),
                Expr::col((object.clone(), Alias::new("id")))
                    .equals((relation.clone(), Alias::new("storage_object_id"))),
            )
            .join_as(
                sea_orm::sea_query::JoinType::InnerJoin,
                Alias::new("library_storage_roots"),
                library_root.clone(),
                Expr::col((library_root.clone(), Alias::new("storage_root_id")))
                    .equals((relation.clone(), Alias::new("storage_root_id"))),
            )
            .join_as(
                sea_orm::sea_query::JoinType::InnerJoin,
                Alias::new("library_catalog_items"),
                membership.clone(),
                Expr::col((membership.clone(), Alias::new("library_id")))
                    .equals((library_root.clone(), Alias::new("library_id"))),
            )
            .join_as(
                sea_orm::sea_query::JoinType::InnerJoin,
                Alias::new("libraries"),
                library.clone(),
                Expr::col((library.clone(), Alias::new("id")))
                    .equals((library_root, Alias::new("library_id"))),
            )
            .and_where(Expr::col((relation.clone(), Alias::new("storage_root_id"))).is_in(roots))
            .and_where(
                Expr::col((relation.clone(), Alias::new("storage_object_id"))).is_in(objects),
            )
            .and_where(Expr::col((relation, Alias::new("presence_state"))).eq("Present"))
            .and_where(Expr::col((object, Alias::new("presence_state"))).eq("Present"))
            .and_where(
                Expr::col((membership, Alias::new("catalog_item_id")))
                    .is_in(authorized_items.clone()),
            )
            .and_where(Expr::col((library, Alias::new("is_enabled"))).eq(true))
            .distinct()
            .to_owned();
        let authorized = transaction
            .query_all(backend.build(&query))
            .await?
            .iter()
            .map(|row| {
                Ok((
                    row.try_get::<Uuid>("", "storage_root_id")?,
                    row.try_get::<Uuid>("", "storage_object_id")?,
                ))
            })
            .collect::<Result<HashSet<_>, DbErr>>()?;
        if requested.iter().any(|pair| !authorized.contains(pair)) {
            return Err(CatalogPublicationError::UnauthorizedStorageObject);
        }
    }
    Ok(())
}

pub(crate) async fn materialize_structure_sources(
    transaction: &DatabaseTransaction,
    publication_id: PublicationId,
) -> Result<(), CatalogPublicationError> {
    for group in load_series_source_groups(transaction, publication_id).await? {
        materialize_stable_identities(
            transaction,
            group.catalog_item_id,
            &group.sources,
            &group.locations,
            &group.subtitles,
        )
        .await?;
        let current = Query::update()
            .table(Alias::new("catalog_items"))
            .value(Alias::new("source_state"), "Indexed")
            .value(Alias::new("source_index_revision"), group.source_revision)
            .and_where(Expr::col(Alias::new("id")).eq(group.catalog_item_id.as_uuid()))
            .and_where(Expr::col(Alias::new("structure_owner_item_id")).is_not_null())
            .to_owned();
        if transaction
            .execute(transaction.get_database_backend().build(&current))
            .await?
            .rows_affected()
            != 1
        {
            return Err(CatalogPublicationError::StableIdentityConflict);
        }
    }
    Ok(())
}

async fn load_series_source_groups(
    transaction: &DatabaseTransaction,
    publication_id: PublicationId,
) -> Result<Vec<SeriesSourcePublication>, CatalogPublicationError> {
    let query = Query::select()
        .columns([
            Alias::new("catalog_item_id"),
            Alias::new("media_source_id"),
            Alias::new("presentation_key"),
            Alias::new("edition"),
            Alias::new("container"),
            Alias::new("locator_kind"),
            Alias::new("naming_hints"),
            Alias::new("row_sha256"),
        ])
        .from(Alias::new("publication_media_sources"))
        .and_where(Expr::col(Alias::new("publication_id")).eq(publication_id.as_uuid()))
        .to_owned();
    let backend = transaction.get_database_backend();
    let mut groups = HashMap::<CatalogItemId, SeriesSourcePublication>::new();
    let mut source_owners = HashMap::<MediaSourceId, CatalogItemId>::new();
    for row in transaction.query_all(backend.build(&query)).await? {
        let owner = CatalogItemId::from_uuid(row.try_get("", "catalog_item_id")?);
        let source = MediaSourcePublicationRow {
            id: MediaSourceId::from_uuid(row.try_get("", "media_source_id")?),
            presentation_key: PresentationKey::from_uuid(row.try_get("", "presentation_key")?),
            edition: row.try_get("", "edition")?,
            container: row.try_get("", "container")?,
            locator_kind: row.try_get("", "locator_kind")?,
            naming_hints: row.try_get("", "naming_hints")?,
            row_sha256: row.try_get("", "row_sha256")?,
        };
        source_owners.insert(source.id, owner);
        groups
            .entry(owner)
            .or_insert_with(|| SeriesSourcePublication {
                catalog_item_id: owner,
                source_revision: 0,
                sources: Vec::new(),
                locations: Vec::new(),
                subtitles: Vec::new(),
            })
            .sources
            .push(source);
    }
    for location in load_location_rows(transaction, publication_id).await? {
        let owner = source_owners
            .get(&location.media_source_id)
            .copied()
            .ok_or(CatalogPublicationError::InvalidSourceGraph)?;
        groups
            .get_mut(&owner)
            .ok_or(CatalogPublicationError::InvalidSourceGraph)?
            .locations
            .push(location);
    }
    for subtitle in load_subtitle_rows(transaction, publication_id).await? {
        let owner = source_owners
            .get(&subtitle.media_source_id)
            .copied()
            .ok_or(CatalogPublicationError::InvalidSourceGraph)?;
        groups
            .get_mut(&owner)
            .ok_or(CatalogPublicationError::InvalidSourceGraph)?
            .subtitles
            .push(subtitle);
    }
    let revision_query = Query::select()
        .columns([
            Alias::new("catalog_item_id"),
            Alias::new("source_state"),
            Alias::new("source_index_revision"),
        ])
        .from(Alias::new("publication_catalog_items"))
        .and_where(Expr::col(Alias::new("publication_id")).eq(publication_id.as_uuid()))
        .and_where(Expr::col(Alias::new("item_type")).eq("Episode"))
        .to_owned();
    for row in transaction
        .query_all(backend.build(&revision_query))
        .await?
    {
        let owner = CatalogItemId::from_uuid(row.try_get("", "catalog_item_id")?);
        if let Some(group) = groups.get_mut(&owner) {
            if row.try_get::<String>("", "source_state")? != "Indexed" {
                return Err(CatalogPublicationError::InvalidSourceGraph);
            }
            group.source_revision = row.try_get("", "source_index_revision")?;
        }
    }
    Ok(groups.into_values().collect())
}

#[allow(clippy::too_many_lines)] // Keeps source activation, generation, and follow-up metadata enqueueing atomic.
async fn publish_sources(
    transaction: &DatabaseTransaction,
    jobs: &WorkJobRepository<'_>,
    claimed: &ClaimedWorkJob,
    publication_id: PublicationId,
    now: DateTime<Utc>,
) -> Result<i64, CatalogPublicationError> {
    let owner = source_owner(claimed)?;
    ensure_live_claim(transaction, claimed, now).await?;
    let publication =
        load_source_publication(transaction, claimed, publication_id, STATE_READY).await?;
    let locations = load_location_rows(transaction, publication_id).await?;
    let subtitles = load_subtitle_rows(transaction, publication_id).await?;
    ensure_storage_authorized_and_reconciled(
        transaction,
        owner,
        claimed.job().storage_root_affinity(),
        &locations,
        &subtitles,
    )
    .await?;
    let backend = transaction.get_database_backend();
    let owner_row = transaction
        .query_one(backend.build(&source_owner_pointer(owner)))
        .await?
        .ok_or(CatalogPublicationError::StaleExpectedRevision)?;
    if owner_row.try_get::<i64>("", "source_index_revision")? != publication.expected_revision {
        return Err(CatalogPublicationError::StaleExpectedRevision);
    }
    let sources = load_source_rows(transaction, publication_id).await?;
    materialize_stable_identities(transaction, owner, &sources, &locations, &subtitles).await?;
    let previous: Option<Uuid> = owner_row.try_get("", "active_source_publication_id")?;
    let metadata_revision: i64 = owner_row.try_get("", "metadata_revision")?;
    let input_sync_revision = claimed.job().input_sync_revision();
    let metadata_policy = if input_sync_revision.is_some() {
        metadata_policy_for_item(transaction, owner).await?
    } else {
        None
    };
    let mut switch = Query::update();
    switch
        .table(Alias::new("catalog_items"))
        .value(
            Alias::new("active_source_publication_id"),
            publication_id.as_uuid(),
        )
        .value(Alias::new("source_state"), "Indexed")
        .value(Alias::new("last_error"), Option::<String>::None);
    if metadata_policy.is_some() {
        switch
            .value(Alias::new("metadata_state"), "Resolving")
            .value(
                Alias::new("metadata_revision"),
                Expr::col(Alias::new("metadata_revision")).add(1),
            );
    }
    let switch = switch
        .and_where(Expr::col(Alias::new("id")).eq(owner.as_uuid()))
        .and_where(Expr::col(Alias::new("source_index_revision")).eq(publication.expected_revision))
        .to_owned();
    if transaction
        .execute(backend.build(&switch))
        .await?
        .rows_affected()
        != 1
    {
        return Err(CatalogPublicationError::StaleExpectedRevision);
    }
    let generation = advance_generation(transaction).await?;
    activate_publication(transaction, publication_id, previous, generation, now).await?;
    insert_change_event(
        transaction,
        owner,
        publication_id,
        generation,
        "SourcesPublished",
        now,
    )
    .await?;
    if let (Some(input_sync_revision), Some(metadata_policy)) =
        (input_sync_revision, metadata_policy)
    {
        let mut spec = WorkJobSpec::new(
            WorkTaskKind::ResolveMetadata,
            WorkScope::CatalogItem(owner),
            metadata_revision + 1,
            claimed.job().priority(),
        )?
        .with_metadata_requirement(metadata_policy.requirement)?
        .with_metadata_source_mode(metadata_policy.source_mode)?
        .with_local_metadata_access_mode(metadata_policy.access_mode)?
        .with_input_sync_revision(input_sync_revision)?;
        if let Some(root_id) = claimed.job().storage_root_affinity() {
            spec = spec.with_storage_root_affinity(root_id)?;
        }
        crate::work_job::enqueue_in_transaction(transaction, &spec, now).await?;
    }
    jobs.complete_in_transaction(
        transaction,
        claimed,
        WorkJobResult::success(
            json!({"published_rows": publication.expected_row_count, "catalog_generation": generation}),
            Vec::new(),
        ),
    )
    .await?;
    Ok(generation)
}

#[derive(Clone, Copy)]
struct EffectiveMetadataPolicy {
    requirement: MetadataRequirement,
    source_mode: MetadataSourceMode,
    access_mode: LocalMetadataAccessMode,
}

async fn metadata_policy_for_item(
    transaction: &DatabaseTransaction,
    owner: CatalogItemId,
) -> Result<Option<EffectiveMetadataPolicy>, CatalogPublicationError> {
    let item = Alias::new("metadata_policy_item");
    let membership = Alias::new("metadata_policy_membership");
    let library = Alias::new("metadata_policy_library");
    let query = Query::select()
        .expr_as(
            Expr::col((library.clone(), Alias::new("metadata_policy"))),
            Alias::new("metadata_policy"),
        )
        .expr_as(
            Expr::col((library.clone(), Alias::new("metadata_source_mode"))),
            Alias::new("metadata_source_mode"),
        )
        .expr_as(
            Expr::col((library.clone(), Alias::new("local_metadata_access_mode"))),
            Alias::new("local_metadata_access_mode"),
        )
        .from_as(Alias::new("catalog_items"), item.clone())
        .join_as(
            JoinType::InnerJoin,
            Alias::new("library_catalog_items"),
            membership.clone(),
            Cond::any()
                .add(
                    Expr::col((membership.clone(), Alias::new("catalog_item_id")))
                        .equals((item.clone(), Alias::new("id"))),
                )
                .add(
                    Expr::col((membership.clone(), Alias::new("catalog_item_id")))
                        .equals((item.clone(), Alias::new("structure_owner_item_id"))),
                ),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("libraries"),
            library.clone(),
            Expr::col((library.clone(), Alias::new("id")))
                .equals((membership, Alias::new("library_id"))),
        )
        .and_where(Expr::col((item, Alias::new("id"))).eq(owner.as_uuid()))
        .and_where(Expr::col((library, Alias::new("is_enabled"))).eq(true))
        .to_owned();
    let backend = transaction.get_database_backend();
    let mut requirement = None;
    let mut source_mode = MetadataSourceMode::LocalOnly;
    let mut access_mode = LocalMetadataAccessMode::Direct;
    for row in transaction.query_all(backend.build(&query)).await? {
        let current = match row.try_get::<String>("", "metadata_policy")?.as_str() {
            "none" => None,
            "basic" => Some(MetadataRequirement::Basic),
            "full" => Some(MetadataRequirement::Full),
            _ => return Err(CatalogPublicationError::InvalidMetadataPolicy),
        };
        requirement = requirement.max(current);
        if current.is_some() {
            match row.try_get::<String>("", "metadata_source_mode")?.as_str() {
                "automatic_scrape" => source_mode = MetadataSourceMode::AutomaticScrape,
                "local_only" => {}
                _ => return Err(CatalogPublicationError::InvalidMetadataPolicy),
            }
            match row
                .try_get::<String>("", "local_metadata_access_mode")?
                .as_str()
            {
                "import" => access_mode = LocalMetadataAccessMode::Import,
                "direct" => {}
                _ => return Err(CatalogPublicationError::InvalidMetadataPolicy),
            }
        }
    }
    Ok(requirement.map(|requirement| EffectiveMetadataPolicy {
        requirement,
        source_mode,
        access_mode,
    }))
}

struct StoredSourcePublication {
    expected_revision: i64,
    expected_row_count: i64,
    manifest_sha256: String,
}

async fn load_source_publication(
    transaction: &DatabaseTransaction,
    claimed: &ClaimedWorkJob,
    publication_id: PublicationId,
    expected_state: &str,
) -> Result<StoredSourcePublication, CatalogPublicationError> {
    let backend = transaction.get_database_backend();
    let row = transaction
        .query_one(backend.build(&publication_by_id(publication_id)))
        .await?
        .ok_or(CatalogPublicationError::InvalidPublication)?;
    if row.try_get::<Uuid>("", "job_id")? != claimed.id().as_uuid()
        || row.try_get::<String>("", "publication_kind")? != PUBLICATION_KIND
        || row.try_get::<String>("", "state")? != expected_state
    {
        return Err(CatalogPublicationError::InvalidPublication);
    }
    Ok(StoredSourcePublication {
        expected_revision: row.try_get("", "expected_revision")?,
        expected_row_count: row.try_get("", "expected_row_count")?,
        manifest_sha256: row.try_get("", "manifest_sha256")?,
    })
}

fn validate_source_projection(
    publication: &StoredSourcePublication,
    sources: &[MediaSourcePublicationRow],
    locations: &[MediaLocationPublicationRow],
    subtitles: &[SubtitlePublicationRow],
) -> Result<(), CatalogPublicationError> {
    let total = sources.len() + locations.len() + subtitles.len();
    let digest = source_manifest_hash(
        sources
            .iter()
            .map(|row| (0_u8, row.id.as_uuid(), row.row_sha256.as_str()))
            .chain(
                locations
                    .iter()
                    .map(|row| (1_u8, row.id.as_uuid(), row.row_sha256.as_str())),
            )
            .chain(
                subtitles
                    .iter()
                    .map(|row| (2_u8, row.id.as_uuid(), row.row_sha256.as_str())),
            ),
    )?;
    if i64::try_from(total).ok() != Some(publication.expected_row_count)
        || digest != publication.manifest_sha256
    {
        return Err(CatalogPublicationError::ManifestMismatch);
    }
    let source_ids = sources.iter().map(|row| row.id).collect::<HashSet<_>>();
    let presentations = sources
        .iter()
        .map(|row| row.presentation_key)
        .collect::<HashSet<_>>();
    if source_ids.len() != sources.len() || presentations.len() != sources.len() {
        return Err(CatalogPublicationError::InvalidSourceGraph);
    }
    let mut source_locations = HashMap::<MediaSourceId, usize>::new();
    for location in locations {
        if !source_ids.contains(&location.media_source_id) {
            return Err(CatalogPublicationError::InvalidSourceGraph);
        }
        *source_locations
            .entry(location.media_source_id)
            .or_default() += 1;
    }
    if sources.iter().any(|source| {
        source_locations
            .get(&source.id)
            .copied()
            .unwrap_or_default()
            == 0
    }) || subtitles
        .iter()
        .any(|subtitle| !source_ids.contains(&subtitle.media_source_id))
    {
        return Err(CatalogPublicationError::InvalidSourceGraph);
    }
    Ok(())
}

async fn ensure_storage_authorized(
    transaction: &DatabaseTransaction,
    owner: CatalogItemId,
    locations: &[MediaLocationPublicationRow],
    subtitles: &[SubtitlePublicationRow],
) -> Result<(), CatalogPublicationError> {
    let object_ids = storage_object_ids(locations, subtitles);
    let authorized = authorized_storage_object_roots(
        transaction,
        owner,
        None,
        object_ids.iter().copied().collect(),
    )
    .await?;
    let authorized_objects = authorized
        .iter()
        .map(|(object, _)| *object)
        .collect::<HashSet<_>>();
    if authorized_objects == object_ids {
        Ok(())
    } else {
        Err(CatalogPublicationError::UnauthorizedStorageObject)
    }
}

async fn authorized_storage_object_roots(
    transaction: &DatabaseTransaction,
    owner: CatalogItemId,
    required_root: Option<StorageRootId>,
    object_ids: Vec<Uuid>,
) -> Result<HashSet<(Uuid, Uuid)>, CatalogPublicationError> {
    let backend = transaction.get_database_backend();
    let owner_query = Query::select()
        .column(Alias::new("structure_owner_item_id"))
        .from(Alias::new("catalog_items"))
        .and_where(Expr::col(Alias::new("id")).eq(owner.as_uuid()))
        .to_owned();
    let owner_row = transaction
        .query_one(backend.build(&owner_query))
        .await?
        .ok_or(CatalogPublicationError::UnauthorizedStorageObject)?;
    let lineage_owner: Option<Uuid> = owner_row.try_get("", "structure_owner_item_id")?;
    let mut authorized_items = vec![owner.as_uuid()];
    if let Some(lineage_owner) = lineage_owner {
        authorized_items.push(lineage_owner);
    }
    let mut all_authorized = HashSet::new();
    for object_ids in object_ids.chunks(MAX_BATCH_ROWS) {
        let root_object = Alias::new("authorized_root_object");
        let library_root = Alias::new("authorized_library_root");
        let membership = Alias::new("authorized_item_membership");
        let library = Alias::new("authorized_library");
        let query = Query::select()
            .expr_as(
                Expr::col((root_object.clone(), Alias::new("storage_object_id"))),
                Alias::new("storage_object_id"),
            )
            .expr_as(
                Expr::col((root_object.clone(), Alias::new("storage_root_id"))),
                Alias::new("storage_root_id"),
            )
            .from_as(Alias::new("storage_root_objects"), root_object.clone())
            .join_as(
                sea_orm::sea_query::JoinType::InnerJoin,
                Alias::new("library_storage_roots"),
                library_root.clone(),
                Expr::col((library_root.clone(), Alias::new("storage_root_id")))
                    .equals((root_object.clone(), Alias::new("storage_root_id"))),
            )
            .join_as(
                sea_orm::sea_query::JoinType::InnerJoin,
                Alias::new("library_catalog_items"),
                membership.clone(),
                Expr::col((membership.clone(), Alias::new("library_id")))
                    .equals((library_root.clone(), Alias::new("library_id"))),
            )
            .join_as(
                sea_orm::sea_query::JoinType::InnerJoin,
                Alias::new("libraries"),
                library.clone(),
                Expr::col((library.clone(), Alias::new("id")))
                    .equals((library_root, Alias::new("library_id"))),
            )
            .and_where(
                Expr::col((root_object.clone(), Alias::new("storage_object_id")))
                    .is_in(object_ids.iter().copied()),
            )
            .and_where(
                Expr::col((membership, Alias::new("catalog_item_id")))
                    .is_in(authorized_items.clone()),
            )
            .and_where(Expr::col((library, Alias::new("is_enabled"))).eq(true))
            .distinct()
            .to_owned();
        let mut query = query;
        if let Some(required_root) = required_root {
            query.and_where(
                Expr::col((root_object, Alias::new("storage_root_id"))).eq(required_root.as_uuid()),
            );
        }
        let authorized = transaction
            .query_all(backend.build(&query))
            .await?
            .iter()
            .map(|row| {
                Ok((
                    row.try_get::<Uuid>("", "storage_object_id")?,
                    row.try_get::<Uuid>("", "storage_root_id")?,
                ))
            })
            .collect::<Result<HashSet<_>, DbErr>>()?;
        all_authorized.extend(authorized);
    }
    Ok(all_authorized)
}

async fn ensure_storage_authorized_and_reconciled(
    transaction: &DatabaseTransaction,
    owner: CatalogItemId,
    required_root: Option<StorageRootId>,
    locations: &[MediaLocationPublicationRow],
    subtitles: &[SubtitlePublicationRow],
) -> Result<(), CatalogPublicationError> {
    let object_ids = storage_object_ids(locations, subtitles);
    let authorized = authorized_storage_object_roots(
        transaction,
        owner,
        required_root,
        object_ids.iter().copied().collect(),
    )
    .await?;
    let authorized_objects = authorized
        .iter()
        .map(|(object, _)| *object)
        .collect::<HashSet<_>>();
    if authorized_objects != object_ids {
        return Err(CatalogPublicationError::UnauthorizedStorageObject);
    }
    let mut by_root = HashMap::<Uuid, Vec<StorageObjectRecordId>>::new();
    for (object, root) in authorized {
        by_root
            .entry(root)
            .or_default()
            .push(StorageObjectRecordId::from_uuid(object));
    }
    let mut reconciled = HashSet::new();
    for (root, objects) in by_root {
        let root = StorageRootId::from_uuid(root);
        let ready = crate::catalog_storage_scope::reconciled_storage_objects(
            transaction,
            &objects,
            Some(root),
        )
        .await?;
        reconciled.extend(ready);
    }
    if reconciled != object_ids {
        return Err(CatalogPublicationError::StorageInputPending);
    }
    Ok(())
}

fn storage_object_ids(
    locations: &[MediaLocationPublicationRow],
    subtitles: &[SubtitlePublicationRow],
) -> HashSet<Uuid> {
    locations
        .iter()
        .map(|row| row.storage_object_id)
        .chain(subtitles.iter().map(|row| row.storage_object_id))
        .map(StorageObjectRecordId::as_uuid)
        .collect()
}

async fn materialize_stable_identities(
    transaction: &DatabaseTransaction,
    owner: CatalogItemId,
    sources: &[MediaSourcePublicationRow],
    locations: &[MediaLocationPublicationRow],
    subtitles: &[SubtitlePublicationRow],
) -> Result<(), CatalogPublicationError> {
    materialize_sources(transaction, owner, sources).await?;
    materialize_locations(transaction, locations).await?;
    materialize_subtitles(transaction, subtitles).await
}

async fn materialize_sources(
    transaction: &DatabaseTransaction,
    owner: CatalogItemId,
    rows: &[MediaSourcePublicationRow],
) -> Result<(), CatalogPublicationError> {
    let backend = transaction.get_database_backend();
    for rows in rows.chunks(500) {
        let mut insert = Query::insert();
        insert.into_table(Alias::new("media_sources")).columns([
            Alias::new("id"),
            Alias::new("catalog_item_id"),
            Alias::new("presentation_key"),
            Alias::new("edition"),
            Alias::new("container"),
            Alias::new("locator_kind"),
            Alias::new("naming_hints"),
            Alias::new("probe_state"),
            Alias::new("probe_revision"),
        ]);
        for row in rows {
            insert.values_panic([
                row.id.as_uuid().into(),
                owner.as_uuid().into(),
                row.presentation_key.as_uuid().into(),
                row.edition.clone().into(),
                row.container.clone().into(),
                row.locator_kind.clone().into(),
                row.naming_hints.clone().into(),
                "NotProbed".into(),
                0_i64.into(),
            ]);
        }
        insert.on_conflict(
            OnConflict::column(Alias::new("id"))
                .update_columns([
                    Alias::new("edition"),
                    Alias::new("locator_kind"),
                    Alias::new("naming_hints"),
                ])
                .to_owned(),
        );
        transaction.execute(backend.build(&insert)).await?;
        ensure_uuid_pair_identities(
            transaction,
            "media_sources",
            "catalog_item_id",
            "presentation_key",
            rows.iter()
                .map(|row| {
                    (
                        row.id.as_uuid(),
                        (owner.as_uuid(), row.presentation_key.as_uuid()),
                    )
                })
                .collect(),
        )
        .await?;
    }
    Ok(())
}

async fn materialize_locations(
    transaction: &DatabaseTransaction,
    rows: &[MediaLocationPublicationRow],
) -> Result<(), CatalogPublicationError> {
    let backend = transaction.get_database_backend();
    for rows in rows.chunks(500) {
        let availability = storage_availability_map(transaction, rows).await?;
        let mut insert = Query::insert();
        insert.into_table(Alias::new("media_locations")).columns([
            Alias::new("id"),
            Alias::new("media_source_id"),
            Alias::new("storage_object_id"),
            Alias::new("content_identity"),
            Alias::new("content_identity_kind"),
            Alias::new("priority"),
            Alias::new("availability_state"),
        ]);
        for row in rows {
            insert.values_panic([
                row.id.as_uuid().into(),
                row.media_source_id.as_uuid().into(),
                row.storage_object_id.as_uuid().into(),
                row.content_identity.clone().into(),
                row.content_identity_kind.clone().into(),
                row.priority.into(),
                availability
                    .get(&row.storage_object_id.as_uuid())
                    .ok_or(CatalogPublicationError::InvalidSourceGraph)?
                    .clone()
                    .into(),
            ]);
        }
        insert.on_conflict(idempotent_conflict(backend, "id"));
        transaction.execute(backend.build(&insert)).await?;
        ensure_uuid_pair_identities(
            transaction,
            "media_locations",
            "media_source_id",
            "storage_object_id",
            rows.iter()
                .map(|row| {
                    (
                        row.id.as_uuid(),
                        (
                            row.media_source_id.as_uuid(),
                            row.storage_object_id.as_uuid(),
                        ),
                    )
                })
                .collect(),
        )
        .await?;
    }
    Ok(())
}

async fn materialize_subtitles(
    transaction: &DatabaseTransaction,
    rows: &[SubtitlePublicationRow],
) -> Result<(), CatalogPublicationError> {
    let backend = transaction.get_database_backend();
    for rows in rows.chunks(500) {
        let mut insert = Query::insert();
        insert.into_table(Alias::new("subtitles")).columns([
            Alias::new("id"),
            Alias::new("media_source_id"),
            Alias::new("storage_object_id"),
            Alias::new("format"),
            Alias::new("language"),
            Alias::new("delivery_index"),
            Alias::new("is_default"),
            Alias::new("is_forced"),
        ]);
        for row in rows {
            insert.values_panic([
                row.id.as_uuid().into(),
                row.media_source_id.as_uuid().into(),
                row.storage_object_id.as_uuid().into(),
                row.format.clone().into(),
                row.language.clone().into(),
                row.delivery_index.into(),
                row.is_default.into(),
                row.is_forced.into(),
            ]);
        }
        insert.on_conflict(idempotent_conflict(backend, "id"));
        transaction.execute(backend.build(&insert)).await?;
        ensure_uuid_pair_identities(
            transaction,
            "subtitles",
            "media_source_id",
            "storage_object_id",
            rows.iter()
                .map(|row| {
                    (
                        row.id.as_uuid(),
                        (
                            row.media_source_id.as_uuid(),
                            row.storage_object_id.as_uuid(),
                        ),
                    )
                })
                .collect(),
        )
        .await?;
    }
    Ok(())
}

fn idempotent_conflict(backend: sea_orm::DbBackend, column: &'static str) -> OnConflict {
    if backend == sea_orm::DbBackend::MySql {
        OnConflict::column(Alias::new(column))
            .update_column(Alias::new(column))
            .to_owned()
    } else {
        OnConflict::column(Alias::new(column))
            .do_nothing()
            .to_owned()
    }
}

async fn ensure_uuid_pair_identities(
    transaction: &DatabaseTransaction,
    table: &str,
    left_column: &str,
    right_column: &str,
    mut expected: HashMap<Uuid, (Uuid, Uuid)>,
) -> Result<(), CatalogPublicationError> {
    let query = Query::select()
        .columns([
            Alias::new("id"),
            Alias::new(left_column),
            Alias::new(right_column),
        ])
        .from(Alias::new(table))
        .and_where(Expr::col(Alias::new("id")).is_in(expected.keys().copied()))
        .to_owned();
    let backend = transaction.get_database_backend();
    for row in transaction.query_all(backend.build(&query)).await? {
        let id: Uuid = row.try_get("", "id")?;
        let Some((left, right)) = expected.remove(&id) else {
            return Err(CatalogPublicationError::StableIdentityConflict);
        };
        if row.try_get::<Uuid>("", left_column)? != left
            || row.try_get::<Uuid>("", right_column)? != right
        {
            return Err(CatalogPublicationError::StableIdentityConflict);
        }
    }
    if expected.is_empty() {
        Ok(())
    } else {
        Err(CatalogPublicationError::StableIdentityConflict)
    }
}

async fn storage_availability_map(
    transaction: &DatabaseTransaction,
    locations: &[MediaLocationPublicationRow],
) -> Result<HashMap<Uuid, String>, CatalogPublicationError> {
    let query = Query::select()
        .columns([
            Alias::new("storage_object_id"),
            Alias::new("presence_state"),
        ])
        .from(Alias::new("storage_root_objects"))
        .and_where(
            Expr::col(Alias::new("storage_object_id"))
                .is_in(locations.iter().map(|row| row.storage_object_id.as_uuid())),
        )
        .to_owned();
    let backend = transaction.get_database_backend();
    let mut availability = HashMap::new();
    for row in transaction.query_all(backend.build(&query)).await? {
        let state: String = row.try_get("", "presence_state")?;
        let value = match state.as_str() {
            "Present" => "Available",
            "TemporarilyUnavailable" => "TemporarilyUnavailable",
            "ConfirmedAbsent" => "ConfirmedAbsent",
            _ => return Err(CatalogPublicationError::InvalidSourceGraph),
        };
        let id = row.try_get("", "storage_object_id")?;
        let previous = availability.get(&id).map(String::as_str);
        if availability_priority(value) > previous.map_or(0, availability_priority) {
            availability.insert(id, value.to_owned());
        }
    }
    Ok(availability)
}

#[allow(clippy::too_many_lines)] // The source projection query and its attached associations are one read model.
async fn active_sources(
    database: &sea_orm::DatabaseConnection,
    owner: CatalogItemId,
) -> Result<Vec<PublishedMediaSource>, CatalogPublicationError> {
    let backend = database.get_database_backend();
    let Some(publication_id) = effective_source_publication(database, owner).await? else {
        return Ok(Vec::new());
    };
    let source = Alias::new("projected_source");
    let canonical = Alias::new("canonical_source");
    let publication = Alias::new("active_source_publication");
    let query = Query::select()
        .expr_as(
            Expr::col((source.clone(), Alias::new("media_source_id"))),
            Alias::new("media_source_id"),
        )
        .expr_as(
            Expr::col((source.clone(), Alias::new("presentation_key"))),
            Alias::new("presentation_key"),
        )
        .expr_as(
            Expr::col((source.clone(), Alias::new("edition"))),
            Alias::new("edition"),
        )
        .expr_as(
            Expr::col((canonical.clone(), Alias::new("container"))),
            Alias::new("container"),
        )
        .expr_as(
            Expr::col((canonical.clone(), Alias::new("locator_kind"))),
            Alias::new("locator_kind"),
        )
        .expr_as(
            Expr::col((canonical.clone(), Alias::new("probe_state"))),
            Alias::new("probe_state"),
        )
        .expr_as(
            Expr::col((canonical.clone(), Alias::new("probe_revision"))),
            Alias::new("probe_revision"),
        )
        .expr_as(
            Expr::col((canonical.clone(), Alias::new("bitrate"))),
            Alias::new("bitrate"),
        )
        .expr_as(
            Expr::col((canonical.clone(), Alias::new("runtime_ticks"))),
            Alias::new("runtime_ticks"),
        )
        .expr_as(
            Expr::col((canonical.clone(), Alias::new("admin_priority"))),
            Alias::new("admin_priority"),
        )
        .expr_as(
            Expr::col((canonical.clone(), Alias::new("is_default"))),
            Alias::new("is_default"),
        )
        .expr_as(
            Expr::col((canonical.clone(), Alias::new("is_hidden"))),
            Alias::new("is_hidden"),
        )
        .from_as(Alias::new("publication_media_sources"), source.clone())
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("media_sources"),
            canonical.clone(),
            Expr::col((canonical.clone(), Alias::new("id")))
                .equals((source.clone(), Alias::new("media_source_id"))),
        )
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("catalog_publications"),
            publication.clone(),
            Expr::col((publication.clone(), Alias::new("id")))
                .equals((source.clone(), Alias::new("publication_id"))),
        )
        .and_where(Expr::col((source.clone(), Alias::new("publication_id"))).eq(publication_id))
        .and_where(Expr::col((source.clone(), Alias::new("catalog_item_id"))).eq(owner.as_uuid()))
        .and_where(Expr::exists(effective_source_publication_visible(
            owner,
            publication_id,
        )))
        .and_where(Expr::exists(effective_source_publication_current(
            owner,
            publication_id,
        )))
        .and_where(Expr::col((publication, Alias::new("state"))).eq("Active"))
        .order_by((source, Alias::new("presentation_key")), Order::Asc)
        .to_owned();
    let mut sources = database
        .query_all(backend.build(&query))
        .await?
        .iter()
        .map(published_source_from_row)
        .collect::<Result<Vec<_>, _>>()?;
    let indexes = sources
        .iter()
        .enumerate()
        .map(|(index, source)| (source.id, index))
        .collect::<HashMap<_, _>>();
    attach_locations(database, owner, publication_id, &indexes, &mut sources).await?;
    attach_streams(database, owner, publication_id, &indexes, &mut sources).await?;
    attach_subtitles(database, owner, publication_id, &indexes, &mut sources).await?;
    if !effective_publication_exists(database, owner, publication_id).await? {
        return Ok(Vec::new());
    }
    Ok(sources)
}

async fn effective_publication_exists(
    connection: &impl ConnectionTrait,
    owner: CatalogItemId,
    publication_id: Uuid,
) -> Result<bool, CatalogPublicationError> {
    let query = Query::select()
        .expr(Expr::val(1_i32))
        .and_where(Expr::exists(effective_source_publication_visible(
            owner,
            publication_id,
        )))
        .and_where(Expr::exists(effective_source_publication_current(
            owner,
            publication_id,
        )))
        .to_owned();
    let backend = connection.get_database_backend();
    connection
        .query_one(backend.build(&query))
        .await
        .map(|row| row.is_some())
        .map_err(Into::into)
}

fn effective_source_publication_current(
    owner: CatalogItemId,
    publication_id: Uuid,
) -> sea_orm::sea_query::SelectStatement {
    let item = Alias::new("current_effective_source_item");
    let publication = Alias::new("current_effective_source_publication");
    let projection = Alias::new("current_effective_source_projection");
    let direct = sea_orm::sea_query::Cond::all()
        .add(Expr::col((publication.clone(), Alias::new("publication_kind"))).eq("Sources"))
        .add(
            Expr::col((publication.clone(), Alias::new("expected_revision")))
                .equals((item.clone(), Alias::new("source_index_revision"))),
        );
    let structure = sea_orm::sea_query::Cond::all()
        .add(Expr::col((publication.clone(), Alias::new("publication_kind"))).eq("Structure"))
        .add(Expr::col((projection.clone(), Alias::new("source_state"))).eq("Indexed"))
        .add(
            Expr::col((projection.clone(), Alias::new("source_index_revision")))
                .equals((item.clone(), Alias::new("source_index_revision"))),
        );
    Query::select()
        .expr(Expr::val(1_i32))
        .from_as(Alias::new("catalog_items"), item.clone())
        .join_as(
            JoinType::InnerJoin,
            Alias::new("catalog_publications"),
            publication.clone(),
            Expr::col((publication.clone(), Alias::new("id"))).eq(publication_id),
        )
        .join_as(
            JoinType::LeftJoin,
            Alias::new("publication_catalog_items"),
            projection.clone(),
            sea_orm::sea_query::Cond::all()
                .add(
                    Expr::col((projection.clone(), Alias::new("publication_id")))
                        .equals((publication.clone(), Alias::new("id"))),
                )
                .add(
                    Expr::col((projection.clone(), Alias::new("catalog_item_id")))
                        .equals((item.clone(), Alias::new("id"))),
                ),
        )
        .and_where(Expr::col((item, Alias::new("id"))).eq(owner.as_uuid()))
        .cond_where(sea_orm::sea_query::Cond::any().add(direct).add(structure))
        .to_owned()
}

pub(crate) async fn effective_source_publication(
    database: &impl ConnectionTrait,
    owner: CatalogItemId,
) -> Result<Option<Uuid>, CatalogPublicationError> {
    let backend = database.get_database_backend();
    let item = Alias::new("source_item");
    let structure_owner = Alias::new("source_structure_owner");
    let direct_publication = Alias::new("direct_source_publication");
    let structure_publication = Alias::new("aggregate_source_publication");
    let pointer = Query::select()
        .expr_as(
            Expr::col((direct_publication.clone(), Alias::new("id"))),
            Alias::new("direct_publication_id"),
        )
        .expr_as(
            Expr::col((
                direct_publication.clone(),
                Alias::new("activated_generation"),
            )),
            Alias::new("direct_generation"),
        )
        .expr_as(
            Expr::col((structure_publication.clone(), Alias::new("id"))),
            Alias::new("structure_publication_id"),
        )
        .expr_as(
            Expr::col((
                structure_publication.clone(),
                Alias::new("activated_generation"),
            )),
            Alias::new("structure_generation"),
        )
        .from_as(Alias::new("catalog_items"), item.clone())
        .join_as(
            sea_orm::sea_query::JoinType::LeftJoin,
            Alias::new("catalog_items"),
            structure_owner.clone(),
            Expr::col((structure_owner.clone(), Alias::new("id")))
                .equals((item.clone(), Alias::new("structure_owner_item_id"))),
        )
        .join_as(
            sea_orm::sea_query::JoinType::LeftJoin,
            Alias::new("catalog_publications"),
            direct_publication.clone(),
            sea_orm::sea_query::Cond::all()
                .add(
                    Expr::col((direct_publication.clone(), Alias::new("id")))
                        .equals((item.clone(), Alias::new("active_source_publication_id"))),
                )
                .add(Expr::col((direct_publication.clone(), Alias::new("state"))).eq("Active")),
        )
        .join_as(
            sea_orm::sea_query::JoinType::LeftJoin,
            Alias::new("catalog_publications"),
            structure_publication.clone(),
            sea_orm::sea_query::Cond::all()
                .add(
                    Expr::col((structure_publication.clone(), Alias::new("id"))).equals((
                        structure_owner,
                        Alias::new("active_structure_publication_id"),
                    )),
                )
                .add(Expr::col((structure_publication.clone(), Alias::new("state"))).eq("Active")),
        )
        .and_where(Expr::col((item, Alias::new("id"))).eq(owner.as_uuid()))
        .to_owned();
    let Some(pointer) = database.query_one(backend.build(&pointer)).await? else {
        return Ok(None);
    };
    let direct = pointer
        .try_get::<Option<Uuid>>("", "direct_publication_id")?
        .zip(pointer.try_get::<Option<i64>>("", "direct_generation")?);
    let structure = pointer
        .try_get::<Option<Uuid>>("", "structure_publication_id")?
        .zip(pointer.try_get::<Option<i64>>("", "structure_generation")?);
    let Some((publication_id, _)) = [direct, structure]
        .into_iter()
        .flatten()
        .max_by_key(|row| row.1)
    else {
        return Ok(None);
    };
    Ok(Some(publication_id))
}

pub(crate) async fn effective_video_storage_names(
    connection: &impl ConnectionTrait,
    owner: CatalogItemId,
    storage_root: StorageRootId,
    scope_object: StorageObjectRecordId,
) -> Result<Vec<String>, CatalogPublicationError> {
    let Some(publication_id) = effective_source_publication(connection, owner).await? else {
        return Ok(Vec::new());
    };
    let source = Alias::new("metadata_video_source");
    let location = Alias::new("metadata_video_location");
    let relation = Alias::new("metadata_video_relation");
    let object = Alias::new("metadata_video_object");
    let query = Query::select()
        .distinct()
        .expr_as(
            Expr::col((object.clone(), Alias::new("name"))),
            Alias::new("name"),
        )
        .from_as(Alias::new("publication_media_sources"), source.clone())
        .join_as(
            JoinType::InnerJoin,
            Alias::new("publication_media_locations"),
            location.clone(),
            Cond::all()
                .add(
                    Expr::col((location.clone(), Alias::new("publication_id")))
                        .equals((source.clone(), Alias::new("publication_id"))),
                )
                .add(
                    Expr::col((location.clone(), Alias::new("media_source_id")))
                        .equals((source.clone(), Alias::new("media_source_id"))),
                ),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("storage_root_objects"),
            relation.clone(),
            Expr::col((relation.clone(), Alias::new("storage_object_id")))
                .equals((location, Alias::new("storage_object_id"))),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("storage_objects"),
            object.clone(),
            Expr::col((object.clone(), Alias::new("id")))
                .equals((relation.clone(), Alias::new("storage_object_id"))),
        )
        .and_where(Expr::col((source.clone(), Alias::new("publication_id"))).eq(publication_id))
        .and_where(Expr::col((source, Alias::new("catalog_item_id"))).eq(owner.as_uuid()))
        .and_where(
            Expr::col((relation.clone(), Alias::new("storage_root_id"))).eq(storage_root.as_uuid()),
        )
        .and_where(
            Expr::col((relation.clone(), Alias::new("parent_storage_object_id")))
                .eq(scope_object.as_uuid()),
        )
        .and_where(Expr::col((relation, Alias::new("presence_state"))).eq("Present"))
        .and_where(Expr::col((object, Alias::new("presence_state"))).eq("Present"))
        .to_owned();
    let backend = connection.get_database_backend();
    connection
        .query_all(backend.build(&query))
        .await?
        .iter()
        .map(|row| row.try_get("", "name").map_err(Into::into))
        .collect()
}

pub(crate) async fn active_presentation_exists(
    connection: &impl ConnectionTrait,
    owner: CatalogItemId,
    presentation_key: PresentationKey,
) -> Result<bool, CatalogPublicationError> {
    let Some(publication_id) = effective_source_publication(connection, owner).await? else {
        return Ok(false);
    };
    let source = Alias::new("active_presentation_source");
    let canonical = Alias::new("active_presentation_canonical");
    let query = Query::select()
        .expr(Expr::val(1_i32))
        .from_as(Alias::new("publication_media_sources"), source.clone())
        .join_as(
            JoinType::InnerJoin,
            Alias::new("media_sources"),
            canonical.clone(),
            Expr::col((canonical.clone(), Alias::new("id")))
                .equals((source.clone(), Alias::new("media_source_id"))),
        )
        .and_where(Expr::col((source.clone(), Alias::new("publication_id"))).eq(publication_id))
        .and_where(Expr::col((source.clone(), Alias::new("catalog_item_id"))).eq(owner.as_uuid()))
        .and_where(
            Expr::col((source, Alias::new("presentation_key"))).eq(presentation_key.as_uuid()),
        )
        .and_where(Expr::col((canonical, Alias::new("is_hidden"))).eq(false))
        .and_where(Expr::exists(effective_source_publication_visible(
            owner,
            publication_id,
        )))
        .limit(1)
        .to_owned();
    let backend = connection.get_database_backend();
    connection
        .query_one(backend.build(&query))
        .await
        .map(|row| row.is_some())
        .map_err(Into::into)
}

#[allow(clippy::too_many_lines)] // One set-based query keeps authorization and active location selection atomic.
async fn playback_location(
    database: &sea_orm::DatabaseConnection,
    owner: CatalogItemId,
    presentation_key: PresentationKey,
) -> Result<Option<PlaybackLocation>, CatalogPublicationError> {
    let Some(publication_id) = effective_source_publication(database, owner).await? else {
        return Ok(None);
    };
    let source = Alias::new("playback_source");
    let canonical_source = Alias::new("playback_canonical_source");
    let location = Alias::new("playback_location");
    let publication = Alias::new("playback_publication");
    let job = Alias::new("playback_job");
    let canonical_location = Alias::new("playback_canonical_location");
    let object = Alias::new("playback_object");
    let root_relation = Alias::new("playback_root_relation");
    let account = Alias::new("playback_account");
    let item = Alias::new("playback_item");
    let structure_owner = Alias::new("playback_structure_owner");
    let membership = Alias::new("playback_membership");
    let library = Alias::new("playback_library");
    let library_root = Alias::new("playback_library_root");
    let availability_rank: SimpleExpr = CaseStatement::new()
        .case(
            Expr::col((canonical_location.clone(), Alias::new("availability_state")))
                .eq("Available"),
            0,
        )
        .finally(1)
        .into();
    let query = Query::select()
        .distinct()
        .expr_as(
            Expr::col((object.clone(), Alias::new("id"))),
            Alias::new("storage_object_id"),
        )
        .expr_as(
            Expr::col((root_relation.clone(), Alias::new("storage_root_id"))),
            Alias::new("storage_root_id"),
        )
        .expr_as(
            Expr::col((account.clone(), Alias::new("id"))),
            Alias::new("storage_account_id"),
        )
        .expr_as(
            Expr::col((account.clone(), Alias::new("provider"))),
            Alias::new("provider"),
        )
        .expr_as(
            Expr::col((object.clone(), Alias::new("provider_object_id"))),
            Alias::new("provider_object_id"),
        )
        .expr_as(
            Expr::col((object.clone(), Alias::new("size"))),
            Alias::new("size"),
        )
        .expr_as(
            Expr::col((object.clone(), Alias::new("remote_revision"))),
            Alias::new("remote_revision"),
        )
        .expr_as(
            Expr::col((canonical_source.clone(), Alias::new("container"))),
            Alias::new("container"),
        )
        .expr_as(
            Expr::col((canonical_source.clone(), Alias::new("locator_kind"))),
            Alias::new("locator_kind"),
        )
        .expr_as(
            Expr::col((item.clone(), Alias::new("item_type"))),
            Alias::new("item_type"),
        )
        .expr_as(availability_rank.clone(), Alias::new("availability_rank"))
        .expr_as(
            Expr::col((location.clone(), Alias::new("priority"))),
            Alias::new("location_priority"),
        )
        .from_as(Alias::new("publication_media_sources"), source.clone())
        .join_as(
            JoinType::InnerJoin,
            Alias::new("media_sources"),
            canonical_source.clone(),
            Expr::col((canonical_source.clone(), Alias::new("id")))
                .equals((source.clone(), Alias::new("media_source_id"))),
        )
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("publication_media_locations"),
            location.clone(),
            sea_orm::sea_query::Cond::all()
                .add(
                    Expr::col((location.clone(), Alias::new("publication_id")))
                        .equals((source.clone(), Alias::new("publication_id"))),
                )
                .add(
                    Expr::col((location.clone(), Alias::new("media_source_id")))
                        .equals((source.clone(), Alias::new("media_source_id"))),
                ),
        )
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("catalog_publications"),
            publication.clone(),
            sea_orm::sea_query::Cond::all()
                .add(
                    Expr::col((publication.clone(), Alias::new("id")))
                        .equals((source.clone(), Alias::new("publication_id"))),
                )
                .add(Expr::col((publication.clone(), Alias::new("id"))).eq(publication_id)),
        )
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("work_jobs"),
            job.clone(),
            Expr::col((job.clone(), Alias::new("id"))).equals((publication, Alias::new("job_id"))),
        )
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("media_locations"),
            canonical_location.clone(),
            Expr::col((canonical_location.clone(), Alias::new("id")))
                .equals((location.clone(), Alias::new("media_location_id"))),
        )
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("storage_objects"),
            object.clone(),
            Expr::col((object.clone(), Alias::new("id")))
                .equals((location.clone(), Alias::new("storage_object_id"))),
        )
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("storage_accounts"),
            account.clone(),
            Expr::col((account.clone(), Alias::new("id")))
                .equals((object.clone(), Alias::new("storage_account_id"))),
        )
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("catalog_items"),
            item.clone(),
            Expr::col((item.clone(), Alias::new("id"))).eq(owner.as_uuid()),
        )
        .join_as(
            sea_orm::sea_query::JoinType::LeftJoin,
            Alias::new("catalog_items"),
            structure_owner.clone(),
            Expr::col((structure_owner.clone(), Alias::new("id")))
                .equals((item.clone(), Alias::new("structure_owner_item_id"))),
        )
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("library_catalog_items"),
            membership.clone(),
            sea_orm::sea_query::Cond::any()
                .add(
                    Expr::col((membership.clone(), Alias::new("catalog_item_id")))
                        .equals((item.clone(), Alias::new("id"))),
                )
                .add(
                    Expr::col((membership.clone(), Alias::new("catalog_item_id")))
                        .equals((structure_owner, Alias::new("id"))),
                ),
        )
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("libraries"),
            library.clone(),
            Expr::col((library.clone(), Alias::new("id")))
                .equals((membership, Alias::new("library_id"))),
        )
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("storage_root_objects"),
            root_relation.clone(),
            Expr::col((root_relation.clone(), Alias::new("storage_object_id")))
                .equals((object.clone(), Alias::new("id"))),
        )
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("library_storage_roots"),
            library_root.clone(),
            sea_orm::sea_query::Cond::all()
                .add(
                    Expr::col((library_root.clone(), Alias::new("storage_root_id")))
                        .equals((root_relation.clone(), Alias::new("storage_root_id"))),
                )
                .add(
                    Expr::col((library_root.clone(), Alias::new("library_id")))
                        .equals((library.clone(), Alias::new("id"))),
                ),
        )
        .and_where(Expr::col((source.clone(), Alias::new("publication_id"))).eq(publication_id))
        .and_where(Expr::col((source.clone(), Alias::new("catalog_item_id"))).eq(owner.as_uuid()))
        .and_where(
            Expr::col((source, Alias::new("presentation_key"))).eq(presentation_key.as_uuid()),
        )
        .and_where(Expr::col((canonical_source, Alias::new("is_hidden"))).eq(false))
        .and_where(Expr::exists(effective_source_publication_visible(
            owner,
            publication_id,
        )))
        .and_where(Expr::col((account, Alias::new("status"))).is_in(["Active", "Ready"]))
        .and_where(
            Expr::col((canonical_location.clone(), Alias::new("availability_state")))
                .is_in(["Available", "TemporarilyUnavailable"]),
        )
        .and_where(Expr::col((item, Alias::new("is_present"))).eq(true))
        .and_where(Expr::col((library, Alias::new("is_enabled"))).eq(true))
        .cond_where(
            sea_orm::sea_query::Cond::any()
                .add(Expr::col((job.clone(), Alias::new("storage_root_affinity"))).eq(Uuid::nil()))
                .add(
                    Expr::col((job, Alias::new("storage_root_affinity")))
                        .equals((root_relation.clone(), Alias::new("storage_root_id"))),
                ),
        )
        .order_by(Alias::new("availability_rank"), Order::Asc)
        .order_by(Alias::new("location_priority"), Order::Desc)
        .order_by(Alias::new("storage_root_id"), Order::Asc)
        .to_owned();
    let backend = database.get_database_backend();
    let mut seen_candidates = HashSet::new();
    for row in database.query_all(backend.build(&query)).await? {
        let root_id = StorageRootId::from_uuid(row.try_get("", "storage_root_id")?);
        let object_id = StorageObjectRecordId::from_uuid(row.try_get("", "storage_object_id")?);
        if !seen_candidates.insert((root_id, object_id)) {
            continue;
        }
        if crate::storage_path_authorization::storage_path_is_authorized(
            database,
            root_id,
            object_id,
            crate::storage_path_authorization::StoragePathAvailability::Playback,
        )
        .await?
        {
            return Ok(Some(playback_location_from_row(&row)?));
        }
    }
    Ok(None)
}

fn playback_location_from_row(
    row: &QueryResult,
) -> Result<PlaybackLocation, CatalogPublicationError> {
    let size: i64 = row.try_get("", "size")?;
    Ok(PlaybackLocation {
        storage_object_id: StorageObjectRecordId::from_uuid(row.try_get("", "storage_object_id")?),
        storage_account_id: row.try_get("", "storage_account_id")?,
        provider: row.try_get("", "provider")?,
        provider_object_id: row.try_get("", "provider_object_id")?,
        size: u64::try_from(size).map_err(|_| CatalogPublicationError::InvalidSourceGraph)?,
        remote_revision: row.try_get("", "remote_revision")?,
        container: row.try_get("", "container")?,
        locator_kind: row.try_get("", "locator_kind")?,
        is_audio: row.try_get::<String>("", "item_type")? == "Audio",
    })
}

#[allow(clippy::too_many_lines)] // One query atomically checks active source, subtitle identity, and library access.
async fn subtitle_location(
    database: &sea_orm::DatabaseConnection,
    owner: CatalogItemId,
    presentation_key: PresentationKey,
    delivery_index: i32,
) -> Result<Option<PlaybackSubtitleLocation>, CatalogPublicationError> {
    let Some(publication_id) = effective_source_publication(database, owner).await? else {
        return Ok(None);
    };
    let source = Alias::new("subtitle_source");
    let canonical_source = Alias::new("subtitle_canonical_source");
    let subtitle = Alias::new("subtitle_projection");
    let publication = Alias::new("subtitle_publication");
    let job = Alias::new("subtitle_job");
    let canonical_subtitle = Alias::new("subtitle_canonical");
    let object = Alias::new("subtitle_object");
    let root_relation = Alias::new("subtitle_root_relation");
    let account = Alias::new("subtitle_account");
    let item = Alias::new("subtitle_item");
    let structure_owner = Alias::new("subtitle_structure_owner");
    let membership = Alias::new("subtitle_membership");
    let library = Alias::new("subtitle_library");
    let library_root = Alias::new("subtitle_library_root");
    let query = Query::select()
        .expr_as(
            Expr::col((object.clone(), Alias::new("id"))),
            Alias::new("storage_object_id"),
        )
        .expr_as(
            Expr::col((root_relation.clone(), Alias::new("storage_root_id"))),
            Alias::new("storage_root_id"),
        )
        .expr_as(
            Expr::col((account.clone(), Alias::new("id"))),
            Alias::new("storage_account_id"),
        )
        .expr_as(
            Expr::col((account.clone(), Alias::new("provider"))),
            Alias::new("provider"),
        )
        .expr_as(
            Expr::col((object.clone(), Alias::new("provider_object_id"))),
            Alias::new("provider_object_id"),
        )
        .expr_as(
            Expr::col((object.clone(), Alias::new("size"))),
            Alias::new("size"),
        )
        .expr_as(
            Expr::col((object.clone(), Alias::new("remote_revision"))),
            Alias::new("remote_revision"),
        )
        .expr_as(
            Expr::col((canonical_source.clone(), Alias::new("container"))),
            Alias::new("container"),
        )
        .expr_as(Expr::val("storage"), Alias::new("locator_kind"))
        .expr_as(
            Expr::col((item.clone(), Alias::new("item_type"))),
            Alias::new("item_type"),
        )
        .expr_as(
            Expr::col((canonical_subtitle.clone(), Alias::new("format"))),
            Alias::new("subtitle_format"),
        )
        .from_as(Alias::new("publication_media_sources"), source.clone())
        .join_as(
            JoinType::InnerJoin,
            Alias::new("media_sources"),
            canonical_source.clone(),
            Expr::col((canonical_source.clone(), Alias::new("id")))
                .equals((source.clone(), Alias::new("media_source_id"))),
        )
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("publication_subtitles"),
            subtitle.clone(),
            sea_orm::sea_query::Cond::all()
                .add(
                    Expr::col((subtitle.clone(), Alias::new("publication_id")))
                        .equals((source.clone(), Alias::new("publication_id"))),
                )
                .add(
                    Expr::col((subtitle.clone(), Alias::new("media_source_id")))
                        .equals((source.clone(), Alias::new("media_source_id"))),
                ),
        )
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("catalog_publications"),
            publication.clone(),
            sea_orm::sea_query::Cond::all()
                .add(
                    Expr::col((publication.clone(), Alias::new("id")))
                        .equals((source.clone(), Alias::new("publication_id"))),
                )
                .add(Expr::col((publication.clone(), Alias::new("id"))).eq(publication_id)),
        )
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("work_jobs"),
            job.clone(),
            Expr::col((job.clone(), Alias::new("id"))).equals((publication, Alias::new("job_id"))),
        )
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("subtitles"),
            canonical_subtitle.clone(),
            sea_orm::sea_query::Cond::all()
                .add(
                    Expr::col((canonical_subtitle.clone(), Alias::new("id")))
                        .equals((subtitle.clone(), Alias::new("subtitle_id"))),
                )
                .add(
                    Expr::col((canonical_subtitle.clone(), Alias::new("media_source_id")))
                        .equals((subtitle.clone(), Alias::new("media_source_id"))),
                ),
        )
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("storage_objects"),
            object.clone(),
            Expr::col((object.clone(), Alias::new("id")))
                .equals((subtitle.clone(), Alias::new("storage_object_id"))),
        )
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("storage_accounts"),
            account.clone(),
            Expr::col((account.clone(), Alias::new("id")))
                .equals((object.clone(), Alias::new("storage_account_id"))),
        )
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("catalog_items"),
            item.clone(),
            Expr::col((item.clone(), Alias::new("id"))).eq(owner.as_uuid()),
        )
        .join_as(
            sea_orm::sea_query::JoinType::LeftJoin,
            Alias::new("catalog_items"),
            structure_owner.clone(),
            Expr::col((structure_owner.clone(), Alias::new("id")))
                .equals((item.clone(), Alias::new("structure_owner_item_id"))),
        )
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("library_catalog_items"),
            membership.clone(),
            sea_orm::sea_query::Cond::any()
                .add(
                    Expr::col((membership.clone(), Alias::new("catalog_item_id")))
                        .equals((item.clone(), Alias::new("id"))),
                )
                .add(
                    Expr::col((membership.clone(), Alias::new("catalog_item_id")))
                        .equals((structure_owner, Alias::new("id"))),
                ),
        )
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("libraries"),
            library.clone(),
            Expr::col((library.clone(), Alias::new("id")))
                .equals((membership, Alias::new("library_id"))),
        )
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("storage_root_objects"),
            root_relation.clone(),
            Expr::col((root_relation.clone(), Alias::new("storage_object_id")))
                .equals((object.clone(), Alias::new("id"))),
        )
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("library_storage_roots"),
            library_root.clone(),
            sea_orm::sea_query::Cond::all()
                .add(
                    Expr::col((library_root.clone(), Alias::new("storage_root_id")))
                        .equals((root_relation.clone(), Alias::new("storage_root_id"))),
                )
                .add(
                    Expr::col((library_root.clone(), Alias::new("library_id")))
                        .equals((library.clone(), Alias::new("id"))),
                ),
        )
        .and_where(Expr::col((source.clone(), Alias::new("publication_id"))).eq(publication_id))
        .and_where(Expr::col((source.clone(), Alias::new("catalog_item_id"))).eq(owner.as_uuid()))
        .and_where(
            Expr::col((source, Alias::new("presentation_key"))).eq(presentation_key.as_uuid()),
        )
        .and_where(Expr::col((canonical_source, Alias::new("is_hidden"))).eq(false))
        .and_where(Expr::col((canonical_subtitle, Alias::new("delivery_index"))).eq(delivery_index))
        .and_where(Expr::exists(effective_source_publication_visible(
            owner,
            publication_id,
        )))
        .and_where(Expr::col((account, Alias::new("status"))).is_in(["Active", "Ready"]))
        .and_where(Expr::col((object, Alias::new("presence_state"))).eq("Present"))
        .and_where(Expr::col((item, Alias::new("is_present"))).eq(true))
        .and_where(Expr::col((library, Alias::new("is_enabled"))).eq(true))
        .cond_where(
            sea_orm::sea_query::Cond::any()
                .add(Expr::col((job.clone(), Alias::new("storage_root_affinity"))).eq(Uuid::nil()))
                .add(
                    Expr::col((job, Alias::new("storage_root_affinity")))
                        .equals((root_relation.clone(), Alias::new("storage_root_id"))),
                ),
        )
        .order_by((root_relation, Alias::new("storage_root_id")), Order::Asc)
        .to_owned();
    let backend = database.get_database_backend();
    let mut seen_candidates = HashSet::new();
    for row in database.query_all(backend.build(&query)).await? {
        let root_id = StorageRootId::from_uuid(row.try_get("", "storage_root_id")?);
        let object_id = StorageObjectRecordId::from_uuid(row.try_get("", "storage_object_id")?);
        if !seen_candidates.insert((root_id, object_id)) {
            continue;
        }
        if crate::storage_path_authorization::storage_path_is_authorized(
            database,
            root_id,
            object_id,
            crate::storage_path_authorization::StoragePathAvailability::Playback,
        )
        .await?
        {
            return Ok(Some(PlaybackSubtitleLocation {
                location: playback_location_from_row(&row)?,
                format: row.try_get("", "subtitle_format")?,
            }));
        }
    }
    Ok(None)
}

pub(crate) fn effective_source_publication_visible(
    owner_id: CatalogItemId,
    publication_id: Uuid,
) -> sea_orm::sea_query::SelectStatement {
    let item = Alias::new("effective_source_item");
    let structure_owner = Alias::new("effective_structure_owner");
    let direct = Alias::new("effective_direct_publication");
    let structure = Alias::new("effective_structure_publication");
    let direct_selected = sea_orm::sea_query::Cond::all()
        .add(Expr::col((direct.clone(), Alias::new("id"))).eq(publication_id))
        .add(
            sea_orm::sea_query::Cond::any()
                .add(Expr::col((structure.clone(), Alias::new("id"))).is_null())
                .add(
                    Expr::col((direct.clone(), Alias::new("activated_generation"))).gt(Expr::col(
                        (structure.clone(), Alias::new("activated_generation")),
                    )),
                ),
        );
    let structure_selected = sea_orm::sea_query::Cond::all()
        .add(Expr::col((structure.clone(), Alias::new("id"))).eq(publication_id))
        .add(
            sea_orm::sea_query::Cond::any()
                .add(Expr::col((direct.clone(), Alias::new("id"))).is_null())
                .add(
                    Expr::col((structure.clone(), Alias::new("activated_generation"))).gte(
                        Expr::col((direct.clone(), Alias::new("activated_generation"))),
                    ),
                ),
        );
    Query::select()
        .expr(Expr::val(1_i32))
        .from_as(Alias::new("catalog_items"), item.clone())
        .join_as(
            sea_orm::sea_query::JoinType::LeftJoin,
            Alias::new("catalog_items"),
            structure_owner.clone(),
            Expr::col((structure_owner.clone(), Alias::new("id")))
                .equals((item.clone(), Alias::new("structure_owner_item_id"))),
        )
        .join_as(
            sea_orm::sea_query::JoinType::LeftJoin,
            Alias::new("catalog_publications"),
            direct.clone(),
            sea_orm::sea_query::Cond::all()
                .add(
                    Expr::col((direct.clone(), Alias::new("id")))
                        .equals((item.clone(), Alias::new("active_source_publication_id"))),
                )
                .add(Expr::col((direct.clone(), Alias::new("state"))).eq("Active"))
                .add(Expr::col((direct.clone(), Alias::new("publication_kind"))).eq("Sources")),
        )
        .join_as(
            sea_orm::sea_query::JoinType::LeftJoin,
            Alias::new("catalog_publications"),
            structure.clone(),
            sea_orm::sea_query::Cond::all()
                .add(Expr::col((structure.clone(), Alias::new("id"))).equals((
                    structure_owner,
                    Alias::new("active_structure_publication_id"),
                )))
                .add(Expr::col((structure.clone(), Alias::new("state"))).eq("Active"))
                .add(
                    Expr::col((structure.clone(), Alias::new("publication_kind"))).eq("Structure"),
                ),
        )
        .and_where(Expr::col((item, Alias::new("id"))).eq(owner_id.as_uuid()))
        .cond_where(
            sea_orm::sea_query::Cond::any()
                .add(direct_selected)
                .add(structure_selected),
        )
        .to_owned()
}

async fn attach_locations(
    database: &sea_orm::DatabaseConnection,
    owner: CatalogItemId,
    publication_id: Uuid,
    indexes: &HashMap<MediaSourceId, usize>,
    sources: &mut [PublishedMediaSource],
) -> Result<(), CatalogPublicationError> {
    let source_ids = indexes.keys().map(|id| id.as_uuid()).collect::<Vec<_>>();
    let location = Alias::new("projected_location");
    let canonical = Alias::new("canonical_location");
    let object = Alias::new("location_storage_object");
    let account = Alias::new("location_storage_account");
    let query = Query::select()
        .expr_as(
            Expr::col((location.clone(), Alias::new("media_location_id"))),
            Alias::new("media_location_id"),
        )
        .expr_as(
            Expr::col((location.clone(), Alias::new("media_source_id"))),
            Alias::new("media_source_id"),
        )
        .expr_as(
            Expr::col((location.clone(), Alias::new("storage_object_id"))),
            Alias::new("storage_object_id"),
        )
        .expr_as(
            Expr::col((location.clone(), Alias::new("priority"))),
            Alias::new("priority"),
        )
        .expr_as(
            Expr::col((canonical.clone(), Alias::new("availability_state"))),
            Alias::new("availability_state"),
        )
        .expr_as(
            Expr::col((account.clone(), Alias::new("status"))),
            Alias::new("account_status"),
        )
        .from_as(Alias::new("publication_media_locations"), location.clone())
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("media_locations"),
            canonical.clone(),
            Expr::col((canonical.clone(), Alias::new("id")))
                .equals((location.clone(), Alias::new("media_location_id"))),
        )
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("storage_objects"),
            object.clone(),
            Expr::col((object.clone(), Alias::new("id")))
                .equals((canonical.clone(), Alias::new("storage_object_id"))),
        )
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("storage_accounts"),
            account.clone(),
            Expr::col((account.clone(), Alias::new("id")))
                .equals((object, Alias::new("storage_account_id"))),
        )
        .and_where(Expr::col((location.clone(), Alias::new("publication_id"))).eq(publication_id))
        .and_where(Expr::col((location.clone(), Alias::new("media_source_id"))).is_in(source_ids))
        .and_where(Expr::exists(effective_source_publication_visible(
            owner,
            publication_id,
        )))
        .order_by((location, Alias::new("priority")), Order::Desc)
        .to_owned();
    let backend = database.get_database_backend();
    for row in database.query_all(backend.build(&query)).await? {
        let source_id = MediaSourceId::from_uuid(row.try_get("", "media_source_id")?);
        let index = *indexes
            .get(&source_id)
            .ok_or(CatalogPublicationError::InvalidSourceGraph)?;
        sources[index].locations.push(PublishedMediaLocation {
            id: MediaLocationId::from_uuid(row.try_get("", "media_location_id")?),
            storage_object_id: StorageObjectRecordId::from_uuid(
                row.try_get("", "storage_object_id")?,
            ),
            priority: row.try_get("", "priority")?,
            availability_state: row.try_get("", "availability_state")?,
            account_status: row.try_get("", "account_status")?,
        });
    }
    Ok(())
}

async fn attach_subtitles(
    database: &sea_orm::DatabaseConnection,
    owner: CatalogItemId,
    publication_id: Uuid,
    indexes: &HashMap<MediaSourceId, usize>,
    sources: &mut [PublishedMediaSource],
) -> Result<(), CatalogPublicationError> {
    let source_ids = indexes.keys().map(|id| id.as_uuid()).collect::<Vec<_>>();
    let projection = Alias::new("active_subtitle_projection");
    let canonical = Alias::new("active_subtitle_canonical");
    let query = Query::select()
        .expr_as(
            Expr::col((projection.clone(), Alias::new("subtitle_id"))),
            Alias::new("subtitle_id"),
        )
        .expr_as(
            Expr::col((projection.clone(), Alias::new("media_source_id"))),
            Alias::new("media_source_id"),
        )
        .expr_as(
            Expr::col((projection.clone(), Alias::new("storage_object_id"))),
            Alias::new("storage_object_id"),
        )
        .expr_as(
            Expr::col((canonical.clone(), Alias::new("format"))),
            Alias::new("format"),
        )
        .expr_as(
            Expr::col((canonical.clone(), Alias::new("language"))),
            Alias::new("language"),
        )
        .expr_as(
            Expr::col((canonical.clone(), Alias::new("delivery_index"))),
            Alias::new("delivery_index"),
        )
        .expr_as(
            Expr::col((canonical.clone(), Alias::new("is_default"))),
            Alias::new("is_default"),
        )
        .expr_as(
            Expr::col((canonical.clone(), Alias::new("is_forced"))),
            Alias::new("is_forced"),
        )
        .from_as(Alias::new("publication_subtitles"), projection.clone())
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("subtitles"),
            canonical.clone(),
            sea_orm::sea_query::Cond::all()
                .add(
                    Expr::col((canonical.clone(), Alias::new("id")))
                        .equals((projection.clone(), Alias::new("subtitle_id"))),
                )
                .add(
                    Expr::col((canonical.clone(), Alias::new("media_source_id")))
                        .equals((projection.clone(), Alias::new("media_source_id"))),
                ),
        )
        .and_where(Expr::col((projection.clone(), Alias::new("publication_id"))).eq(publication_id))
        .and_where(Expr::col((projection.clone(), Alias::new("media_source_id"))).is_in(source_ids))
        .and_where(Expr::exists(effective_source_publication_visible(
            owner,
            publication_id,
        )))
        .order_by((projection, Alias::new("subtitle_id")), Order::Asc)
        .to_owned();
    let backend = database.get_database_backend();
    for row in database.query_all(backend.build(&query)).await? {
        let source_id = MediaSourceId::from_uuid(row.try_get("", "media_source_id")?);
        let index = *indexes
            .get(&source_id)
            .ok_or(CatalogPublicationError::InvalidSourceGraph)?;
        sources[index].subtitles.push(PublishedSubtitle {
            id: SubtitleId::from_uuid(row.try_get("", "subtitle_id")?),
            storage_object_id: StorageObjectRecordId::from_uuid(
                row.try_get("", "storage_object_id")?,
            ),
            format: row.try_get("", "format")?,
            language: row.try_get("", "language")?,
            delivery_index: row.try_get("", "delivery_index")?,
            is_default: row.try_get("", "is_default")?,
            is_forced: row.try_get("", "is_forced")?,
        });
    }
    Ok(())
}

async fn attach_streams(
    database: &sea_orm::DatabaseConnection,
    owner: CatalogItemId,
    publication_id: Uuid,
    indexes: &HashMap<MediaSourceId, usize>,
    sources: &mut [PublishedMediaSource],
) -> Result<(), CatalogPublicationError> {
    let source_ids = indexes.keys().map(|id| id.as_uuid()).collect::<Vec<_>>();
    let stream = Alias::new("active_media_stream");
    let query = Query::select()
        .expr_as(
            Expr::col((stream.clone(), Alias::new("media_source_id"))),
            Alias::new("media_source_id"),
        )
        .expr_as(
            Expr::col((stream.clone(), Alias::new("stream_type"))),
            Alias::new("stream_type"),
        )
        .expr_as(
            Expr::col((stream.clone(), Alias::new("codec"))),
            Alias::new("codec"),
        )
        .expr_as(
            Expr::col((stream.clone(), Alias::new("language"))),
            Alias::new("language"),
        )
        .expr_as(
            Expr::col((stream.clone(), Alias::new("delivery_index"))),
            Alias::new("delivery_index"),
        )
        .expr_as(
            Expr::col((stream.clone(), Alias::new("is_default"))),
            Alias::new("is_default"),
        )
        .expr_as(
            Expr::col((stream.clone(), Alias::new("is_forced"))),
            Alias::new("is_forced"),
        )
        .expr_as(
            Expr::col((stream.clone(), Alias::new("width"))),
            Alias::new("width"),
        )
        .expr_as(
            Expr::col((stream.clone(), Alias::new("height"))),
            Alias::new("height"),
        )
        .expr_as(
            Expr::col((stream.clone(), Alias::new("channels"))),
            Alias::new("channels"),
        )
        .expr_as(
            Expr::col((stream.clone(), Alias::new("profile"))),
            Alias::new("profile"),
        )
        .expr_as(
            Expr::col((stream.clone(), Alias::new("level"))),
            Alias::new("level"),
        )
        .from_as(Alias::new("media_streams"), stream.clone())
        .and_where(Expr::col((stream.clone(), Alias::new("media_source_id"))).is_in(source_ids))
        .and_where(Expr::col((stream.clone(), Alias::new("delivery_index"))).is_not_null())
        .and_where(Expr::exists(effective_source_publication_visible(
            owner,
            publication_id,
        )))
        .order_by((stream, Alias::new("delivery_index")), Order::Asc)
        .to_owned();
    let backend = database.get_database_backend();
    for row in database.query_all(backend.build(&query)).await? {
        let source_id = MediaSourceId::from_uuid(row.try_get("", "media_source_id")?);
        let index = *indexes
            .get(&source_id)
            .ok_or(CatalogPublicationError::InvalidSourceGraph)?;
        sources[index].streams.push(PublishedMediaStream {
            stream_type: row.try_get("", "stream_type")?,
            codec: row.try_get("", "codec")?,
            language: row.try_get("", "language")?,
            delivery_index: row.try_get("", "delivery_index")?,
            is_default: row
                .try_get::<Option<bool>>("", "is_default")?
                .unwrap_or(false),
            is_forced: row
                .try_get::<Option<bool>>("", "is_forced")?
                .unwrap_or(false),
            width: row.try_get("", "width")?,
            height: row.try_get("", "height")?,
            channels: row.try_get("", "channels")?,
            profile: row.try_get("", "profile")?,
            level: row.try_get("", "level")?,
        });
    }
    Ok(())
}

fn published_source_from_row(
    row: &QueryResult,
) -> Result<PublishedMediaSource, CatalogPublicationError> {
    Ok(PublishedMediaSource {
        id: MediaSourceId::from_uuid(row.try_get("", "media_source_id")?),
        presentation_key: PresentationKey::from_uuid(row.try_get("", "presentation_key")?),
        edition: row.try_get("", "edition")?,
        container: row.try_get("", "container")?,
        locator_kind: row.try_get("", "locator_kind")?,
        probe_state: row.try_get("", "probe_state")?,
        probe_revision: row.try_get("", "probe_revision")?,
        bitrate: row.try_get("", "bitrate")?,
        runtime_ticks: row.try_get("", "runtime_ticks")?,
        admin_priority: row.try_get("", "admin_priority")?,
        is_default: row.try_get("", "is_default")?,
        is_hidden: row.try_get("", "is_hidden")?,
        locations: Vec::new(),
        streams: Vec::new(),
        subtitles: Vec::new(),
    })
}

async fn load_source_rows(
    transaction: &DatabaseTransaction,
    publication_id: PublicationId,
) -> Result<Vec<MediaSourcePublicationRow>, CatalogPublicationError> {
    let query = Query::select()
        .columns([
            Alias::new("media_source_id"),
            Alias::new("presentation_key"),
            Alias::new("edition"),
            Alias::new("container"),
            Alias::new("locator_kind"),
            Alias::new("naming_hints"),
            Alias::new("row_sha256"),
        ])
        .from(Alias::new("publication_media_sources"))
        .and_where(Expr::col(Alias::new("publication_id")).eq(publication_id.as_uuid()))
        .to_owned();
    let backend = transaction.get_database_backend();
    transaction
        .query_all(backend.build(&query))
        .await?
        .iter()
        .map(|row| {
            Ok(MediaSourcePublicationRow {
                id: MediaSourceId::from_uuid(row.try_get("", "media_source_id")?),
                presentation_key: PresentationKey::from_uuid(row.try_get("", "presentation_key")?),
                edition: row.try_get("", "edition")?,
                container: row.try_get("", "container")?,
                locator_kind: row.try_get("", "locator_kind")?,
                naming_hints: row.try_get("", "naming_hints")?,
                row_sha256: row.try_get("", "row_sha256")?,
            })
        })
        .collect()
}

async fn load_location_rows(
    transaction: &DatabaseTransaction,
    publication_id: PublicationId,
) -> Result<Vec<MediaLocationPublicationRow>, CatalogPublicationError> {
    let query = Query::select()
        .columns([
            Alias::new("media_location_id"),
            Alias::new("media_source_id"),
            Alias::new("storage_object_id"),
            Alias::new("content_identity"),
            Alias::new("content_identity_kind"),
            Alias::new("priority"),
            Alias::new("row_sha256"),
        ])
        .from(Alias::new("publication_media_locations"))
        .and_where(Expr::col(Alias::new("publication_id")).eq(publication_id.as_uuid()))
        .to_owned();
    let backend = transaction.get_database_backend();
    transaction
        .query_all(backend.build(&query))
        .await?
        .iter()
        .map(|row| {
            Ok(MediaLocationPublicationRow {
                id: MediaLocationId::from_uuid(row.try_get("", "media_location_id")?),
                media_source_id: MediaSourceId::from_uuid(row.try_get("", "media_source_id")?),
                storage_object_id: StorageObjectRecordId::from_uuid(
                    row.try_get("", "storage_object_id")?,
                ),
                content_identity: row.try_get("", "content_identity")?,
                content_identity_kind: row.try_get("", "content_identity_kind")?,
                priority: row.try_get("", "priority")?,
                row_sha256: row.try_get("", "row_sha256")?,
            })
        })
        .collect()
}

async fn load_subtitle_rows(
    transaction: &DatabaseTransaction,
    publication_id: PublicationId,
) -> Result<Vec<SubtitlePublicationRow>, CatalogPublicationError> {
    let query = Query::select()
        .columns([
            Alias::new("subtitle_id"),
            Alias::new("media_source_id"),
            Alias::new("storage_object_id"),
            Alias::new("format"),
            Alias::new("language"),
            Alias::new("delivery_index"),
            Alias::new("is_default"),
            Alias::new("is_forced"),
            Alias::new("row_sha256"),
        ])
        .from(Alias::new("publication_subtitles"))
        .and_where(Expr::col(Alias::new("publication_id")).eq(publication_id.as_uuid()))
        .to_owned();
    let backend = transaction.get_database_backend();
    transaction
        .query_all(backend.build(&query))
        .await?
        .iter()
        .map(|row| {
            Ok(SubtitlePublicationRow {
                id: SubtitleId::from_uuid(row.try_get("", "subtitle_id")?),
                media_source_id: MediaSourceId::from_uuid(row.try_get("", "media_source_id")?),
                storage_object_id: StorageObjectRecordId::from_uuid(
                    row.try_get("", "storage_object_id")?,
                ),
                format: row.try_get("", "format")?,
                language: row.try_get("", "language")?,
                delivery_index: row.try_get("", "delivery_index")?,
                is_default: row.try_get("", "is_default")?,
                is_forced: row.try_get("", "is_forced")?,
                row_sha256: row.try_get("", "row_sha256")?,
            })
        })
        .collect()
}

fn publication_for_job(job_id: Uuid) -> sea_orm::sea_query::SelectStatement {
    Query::select()
        .columns([
            Alias::new("id"),
            Alias::new("job_id"),
            Alias::new("owner_catalog_item_id"),
            Alias::new("publication_kind"),
            Alias::new("expected_revision"),
            Alias::new("input_sync_revision"),
            Alias::new("state"),
            Alias::new("manifest_sha256"),
            Alias::new("expected_row_count"),
            Alias::new("naming_parser_version"),
        ])
        .from(Alias::new("catalog_publications"))
        .and_where(Expr::col(Alias::new("job_id")).eq(job_id))
        .to_owned()
}

fn publication_by_id(publication_id: PublicationId) -> sea_orm::sea_query::SelectStatement {
    Query::select()
        .columns([
            Alias::new("job_id"),
            Alias::new("publication_kind"),
            Alias::new("state"),
            Alias::new("expected_revision"),
            Alias::new("expected_row_count"),
            Alias::new("manifest_sha256"),
        ])
        .from(Alias::new("catalog_publications"))
        .and_where(Expr::col(Alias::new("id")).eq(publication_id.as_uuid()))
        .to_owned()
}

fn source_owner_pointer(owner: CatalogItemId) -> sea_orm::sea_query::SelectStatement {
    Query::select()
        .columns([
            Alias::new("source_index_revision"),
            Alias::new("metadata_revision"),
            Alias::new("active_source_publication_id"),
        ])
        .from(Alias::new("catalog_items"))
        .and_where(Expr::col(Alias::new("id")).eq(owner.as_uuid()))
        .to_owned()
}

fn source_owner(claimed: &ClaimedWorkJob) -> Result<CatalogItemId, CatalogPublicationError> {
    if claimed.job().task_kind() != WorkTaskKind::IndexMediaSources {
        return Err(CatalogPublicationError::InvalidWorkKind);
    }
    match claimed.job().scope() {
        WorkScope::CatalogItem(owner) => Ok(owner),
        WorkScope::Library(_)
        | WorkScope::LibraryRootBinding(_)
        | WorkScope::MediaSource(_)
        | WorkScope::StorageRoot(_)
        | WorkScope::StorageObject(_) => Err(CatalogPublicationError::InvalidWorkKind),
    }
}

fn source_manifest_hash<'a>(
    entries: impl IntoIterator<Item = (u8, Uuid, &'a str)>,
) -> Result<String, CatalogPublicationError> {
    let mut entries = entries.into_iter().collect::<Vec<_>>();
    entries.sort_unstable_by_key(|(kind, id, _)| (*kind, *id));
    if entries
        .windows(2)
        .any(|pair| pair[0].0 == pair[1].0 && pair[0].1 == pair[1].1)
    {
        return Err(CatalogPublicationError::InvalidSourceManifest);
    }
    let mut hasher = Sha256::new();
    for (kind, id, row_hash) in entries {
        hasher.update([kind]);
        hasher.update(id.as_bytes());
        hasher.update(row_hash.as_bytes());
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn source_hash(row: &MediaSourcePublicationRow) -> String {
    let mut hasher = Sha256::new();
    hasher.update(row.id.as_uuid().as_bytes());
    hasher.update(row.presentation_key.as_uuid().as_bytes());
    hash_optional_text(&mut hasher, row.edition.as_deref());
    hash_optional_text(&mut hasher, row.container.as_deref());
    hasher.update(row.locator_kind.as_bytes());
    let naming_hints = row.naming_hints.as_ref().map(Value::to_string);
    hash_optional_text(&mut hasher, naming_hints.as_deref());
    format!("{:x}", hasher.finalize())
}

fn location_hash(row: &MediaLocationPublicationRow) -> String {
    let mut hasher = Sha256::new();
    hasher.update(row.id.as_uuid().as_bytes());
    hasher.update(row.media_source_id.as_uuid().as_bytes());
    hasher.update(row.storage_object_id.as_uuid().as_bytes());
    hash_optional_text(&mut hasher, row.content_identity.as_deref());
    hash_optional_text(&mut hasher, row.content_identity_kind.as_deref());
    hasher.update(row.priority.to_be_bytes());
    format!("{:x}", hasher.finalize())
}

fn subtitle_hash(row: &SubtitlePublicationRow) -> String {
    let mut hasher = Sha256::new();
    hasher.update(row.id.as_uuid().as_bytes());
    hasher.update(row.media_source_id.as_uuid().as_bytes());
    hasher.update(row.storage_object_id.as_uuid().as_bytes());
    hash_text(&mut hasher, &row.format);
    hash_optional_text(&mut hasher, row.language.as_deref());
    match row.delivery_index {
        Some(index) => {
            hasher.update([1]);
            hasher.update(index.to_be_bytes());
        }
        None => hasher.update([0]),
    }
    hasher.update([u8::from(row.is_default), u8::from(row.is_forced)]);
    format!("{:x}", hasher.finalize())
}

fn hash_optional_text(hasher: &mut Sha256, value: Option<&str>) {
    match value {
        Some(value) => {
            hasher.update([1]);
            hash_text(hasher, value);
        }
        None => hasher.update([0]),
    }
}

fn hash_text(hasher: &mut Sha256, value: &str) {
    hasher.update(value.len().to_be_bytes());
    hasher.update(value.as_bytes());
}

fn valid_optional_text(value: Option<&str>, max_chars: usize) -> bool {
    value.is_none_or(|value| {
        !value.trim().is_empty()
            && value.chars().count() <= max_chars
            && !value.chars().any(char::is_control)
    })
}

fn availability_priority(state: &str) -> u8 {
    match state {
        "Available" => 3,
        "TemporarilyUnavailable" => 2,
        "ConfirmedAbsent" => 1,
        _ => 0,
    }
}
