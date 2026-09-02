use std::{sync::Arc, time::Duration};

use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tjxy_cache::{
    CacheFillLeader, CacheFillPermit, CacheKeyBuilder, CacheProjection, CacheQueryDigest,
    CacheStore, PlaybackProbeDigest, SingleFlight,
};
use tjxy_common::{CatalogItemId, UserId, WorkJobId};
use tjxy_db::{
    BrowseParent, CatalogFilterFacets, CatalogItemDetailRecord, CatalogItemRecord, CatalogItemType,
    CatalogItemsQuery, CatalogItemsScope, CatalogPage, CatalogPageRequest, CatalogPublicationError,
    CatalogPublicationRepository, CatalogQueryError, CatalogQueryRepository, CatalogSortField,
    CatalogSortOrder, LatestItemRecord, LazyCatalogWorkTarget, LibraryViewRecord,
    PlaystateRepository, PlaystateRepositoryError, SourcePlaybackPolicy, SourcePlaybackPolicyError,
    WorkJobRepository, WorkJobRepositoryError, WorkJobSpec, WorkJobState, WorkScope, WorkTaskKind,
};
use tjxy_domain::MetadataSourceMode;
use tokio::time::Instant;
use uuid::Uuid;

const MAX_HOME_LATEST_LIBRARIES: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LazyWaitOutcome {
    Completed,
    Failed,
    TimedOut,
    Missing,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PlaybackSource {
    id: tjxy_common::MediaSourceId,
    presentation_key: tjxy_common::PresentationKey,
    container: String,
    edition: Option<String>,
    bitrate: Option<i64>,
    runtime_ticks: Option<i64>,
    #[serde(default)]
    is_audio: bool,
    last_used: bool,
    admin_priority: i32,
    is_default: bool,
    resolution_pixels: i64,
    account_health: i32,
    location_priority: i32,
    streams: Vec<PlaybackStream>,
    subtitles: Vec<PlaybackSubtitle>,
}

impl PlaybackSource {
    #[must_use]
    pub const fn id(&self) -> tjxy_common::MediaSourceId {
        self.id
    }

    #[must_use]
    pub const fn presentation_key(&self) -> tjxy_common::PresentationKey {
        self.presentation_key
    }

    #[must_use]
    pub fn container(&self) -> &str {
        &self.container
    }

    #[must_use]
    pub fn edition(&self) -> Option<&str> {
        self.edition.as_deref()
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
    pub const fn is_audio(&self) -> bool {
        self.is_audio
    }

    #[must_use]
    pub fn streams(&self) -> &[PlaybackStream] {
        &self.streams
    }

    #[must_use]
    pub fn subtitles(&self) -> &[PlaybackSubtitle] {
        &self.subtitles
    }

    #[must_use]
    pub const fn is_last_used(&self) -> bool {
        self.last_used
    }

    #[must_use]
    pub const fn is_default(&self) -> bool {
        self.is_default
    }

    #[must_use]
    pub const fn admin_priority(&self) -> i32 {
        self.admin_priority
    }

    #[must_use]
    pub const fn resolution_pixels(&self) -> i64 {
        self.resolution_pixels
    }

    #[must_use]
    pub const fn account_health(&self) -> i32 {
        self.account_health
    }

    #[must_use]
    pub const fn location_priority(&self) -> i32 {
        self.location_priority
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PlaybackStream {
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

impl PlaybackStream {
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

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PlaybackSubtitle {
    format: String,
    language: Option<String>,
    delivery_index: i32,
    is_default: bool,
    is_forced: bool,
}

impl PlaybackSubtitle {
    #[must_use]
    pub fn format(&self) -> &str {
        &self.format
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
}

/// Authenticated read boundary for the published catalog.
#[derive(Clone)]
pub struct CatalogQueryService {
    database: DatabaseConnection,
    lazy_wait_timeout: Duration,
    cache: Option<CatalogCache>,
    direct_metadata: Option<Arc<crate::DirectMetadataReadService>>,
}

#[derive(Clone)]
struct CatalogCache {
    store: Arc<dyn CacheStore>,
    keys: CacheKeyBuilder,
    home_ttl: Duration,
    item_ttl: Duration,
    empty_ttl: Duration,
    single_flight: SingleFlight,
}

enum CacheLookup<T> {
    Hit(T),
    Leader(CacheFillLeader),
    Fallback,
}

impl CatalogQueryService {
    #[must_use]
    pub const fn new(database: DatabaseConnection) -> Self {
        Self {
            database,
            lazy_wait_timeout: Duration::ZERO,
            cache: None,
            direct_metadata: None,
        }
    }

    #[must_use]
    pub fn with_direct_metadata(mut self, service: Arc<crate::DirectMetadataReadService>) -> Self {
        self.direct_metadata = Some(service);
        self
    }

    #[must_use]
    pub const fn with_lazy_wait_timeout(mut self, timeout: Duration) -> Self {
        self.lazy_wait_timeout = timeout;
        self
    }

    #[must_use]
    pub fn with_cache(
        self,
        store: Arc<dyn CacheStore>,
        keys: CacheKeyBuilder,
        home_ttl: Duration,
    ) -> Self {
        self.with_cache_ttls(store, keys, home_ttl, home_ttl, Duration::from_secs(3))
    }

    #[must_use]
    pub fn with_cache_ttls(
        mut self,
        store: Arc<dyn CacheStore>,
        keys: CacheKeyBuilder,
        home_ttl: Duration,
        item_ttl: Duration,
        empty_ttl: Duration,
    ) -> Self {
        self.cache = Some(CatalogCache {
            store,
            keys,
            home_ttl,
            item_ttl,
            empty_ttl,
            single_flight: SingleFlight::default(),
        });
        self
    }

    /// Returns enabled library views visible under the current v1 policy.
    ///
    /// The v1 schema has no per-user library grants, so every authenticated,
    /// enabled user sees every enabled library. A supplied Jellyfin `UserId`
    /// remains an assertion about the principal, never an authority source.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogServiceError::ForbiddenUser`] for a mismatched user or
    /// propagates a catalog query failure.
    pub async fn user_views(
        &self,
        principal: UserId,
        requested_user: Option<UserId>,
    ) -> Result<Vec<LibraryViewRecord>, CatalogServiceError> {
        authorize_user(principal, requested_user)?;
        let repository = CatalogQueryRepository::new(&self.database);
        let Some(cache) = &self.cache else {
            return repository.user_views().await.map_err(Into::into);
        };
        let revisions = repository.cache_revisions(principal).await?;
        let key = cache.keys.user_scoped(
            revisions.catalog_generation(),
            &principal.to_string(),
            revisions.user_revision(),
            CacheProjection::UserViews,
            &CacheQueryDigest::from_bytes(b"all"),
        );
        match cache_lookup(cache, &key).await {
            CacheLookup::Hit(views) => Ok(views),
            CacheLookup::Fallback => repository.user_views().await.map_err(Into::into),
            CacheLookup::Leader(_leader) => {
                let views = repository.user_views().await?;
                cache_put(cache, &key, &views, cache.home_ttl).await;
                Ok(views)
            }
        }
    }

    /// Returns complete filter choices for one enabled library.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogServiceError::ForbiddenUser`] for impersonation or propagates a catalog
    /// query failure.
    pub async fn filter_facets(
        &self,
        principal: UserId,
        requested_user: Option<UserId>,
        library_id: Uuid,
    ) -> Result<CatalogFilterFacets, CatalogServiceError> {
        authorize_user(principal, requested_user)?;
        CatalogQueryRepository::new(&self.database)
            .filter_facets(principal, library_id)
            .await
            .map_err(Into::into)
    }

    /// Fills the default home cache entries for the supplied users.
    ///
    /// This delegates to the public catalog query boundaries so warmup shares their
    /// authorization, revision, cache-aside, and TTL behavior.
    ///
    /// # Errors
    ///
    /// Returns the first catalog query failure encountered while warming the supplied users.
    pub async fn warm_home(&self, users: &[UserId]) -> Result<(), CatalogServiceError> {
        for &user in users {
            let views = self.user_views(user, None).await?;
            self.latest_items(user, None, None, Vec::new(), 20, true, None)
                .await?;
            for view in views.into_iter().take(MAX_HOME_LATEST_LIBRARIES) {
                self.latest_items(user, None, Some(view.id()), Vec::new(), 20, true, None)
                    .await?;
            }
            let page = CatalogPageRequest::new(0, 100)?;
            self.resume_items(user, None, page.clone()).await?;
            self.next_up_items(user, None, None, false, page).await?;
        }
        Ok(())
    }

    /// Returns a bounded, membership-filtered catalog page for the principal.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogServiceError::ForbiddenUser`] for a mismatched user or
    /// propagates a catalog query failure.
    pub async fn items(
        &self,
        principal: UserId,
        requested_user: Option<UserId>,
        parent: BrowseParent,
        page: CatalogPageRequest,
    ) -> Result<CatalogPage, CatalogServiceError> {
        self.query_items(
            principal,
            requested_user,
            CatalogItemsQuery::new(CatalogItemsScope::Parent(parent), page),
        )
        .await
    }

    /// Returns a bounded page for one complete catalog query.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogServiceError::ForbiddenUser`] for a mismatched user or
    /// propagates a catalog query failure.
    pub async fn query_items(
        &self,
        principal: UserId,
        requested_user: Option<UserId>,
        query: CatalogItemsQuery,
    ) -> Result<CatalogPage, CatalogServiceError> {
        authorize_user(principal, requested_user)?;
        let repository = CatalogQueryRepository::new(&self.database);
        let Some(cache) = &self.cache else {
            return repository
                .query_items(principal, query)
                .await
                .map_err(Into::into);
        };
        let revisions = repository.cache_revisions(principal).await?;
        let descriptor = items_cache_descriptor(&query);
        let key = cache.keys.user_scoped(
            revisions.catalog_generation(),
            &principal.to_string(),
            revisions.user_revision(),
            CacheProjection::Items,
            &CacheQueryDigest::from_bytes(descriptor.as_bytes()),
        );
        match cache_lookup(cache, &key).await {
            CacheLookup::Hit(page) => Ok(page),
            CacheLookup::Fallback => repository
                .query_items(principal, query)
                .await
                .map_err(Into::into),
            CacheLookup::Leader(_leader) => {
                let page = repository.query_items(principal, query).await?;
                let ttl = if page.items().is_empty() {
                    cache.empty_ttl
                } else {
                    cache.home_ttl
                };
                cache_put(cache, &key, &page, ttl).await;
                Ok(page)
            }
        }
    }

    /// Returns a bounded page of catalog items matching a name fragment.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogServiceError::ForbiddenUser`] for a mismatched user or
    /// propagates a catalog query failure.
    pub async fn search_hints(
        &self,
        principal: UserId,
        requested_user: Option<UserId>,
        search_term: &str,
        page: CatalogPageRequest,
    ) -> Result<CatalogPage, CatalogServiceError> {
        authorize_user(principal, requested_user)?;
        let repository = CatalogQueryRepository::new(&self.database);
        let Some(cache) = &self.cache else {
            return repository
                .search_hints(principal, search_term, page)
                .await
                .map_err(Into::into);
        };
        let revisions = repository.cache_revisions(principal).await?;
        let descriptor = search_cache_descriptor(search_term, &page);
        let key = cache.keys.user_scoped(
            revisions.catalog_generation(),
            &principal.to_string(),
            revisions.user_revision(),
            CacheProjection::Search,
            &CacheQueryDigest::from_bytes(descriptor.as_bytes()),
        );
        match cache_lookup(cache, &key).await {
            CacheLookup::Hit(page) => Ok(page),
            CacheLookup::Fallback => repository
                .search_hints(principal, search_term, page)
                .await
                .map_err(Into::into),
            CacheLookup::Leader(_leader) => {
                let page = repository
                    .search_hints(principal, search_term, page)
                    .await?;
                let ttl = if page.items().is_empty() {
                    cache.empty_ttl
                } else {
                    cache.item_ttl
                };
                cache_put(cache, &key, &page, ttl).await;
                Ok(page)
            }
        }
    }

    /// Returns the principal's visible unfinished playback items.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogServiceError`] for impersonation or query failures.
    pub async fn resume_items(
        &self,
        principal: UserId,
        requested_user: Option<UserId>,
        page: CatalogPageRequest,
    ) -> Result<CatalogPage, CatalogServiceError> {
        authorize_user(principal, requested_user)?;
        let repository = CatalogQueryRepository::new(&self.database);
        let Some(cache) = &self.cache else {
            return repository
                .resume_items(principal, page)
                .await
                .map_err(Into::into);
        };
        let revisions = repository.cache_revisions(principal).await?;
        let descriptor = page_cache_descriptor(&page);
        let key = cache.keys.user_scoped(
            revisions.catalog_generation(),
            &principal.to_string(),
            revisions.user_revision(),
            CacheProjection::Resume,
            &CacheQueryDigest::from_bytes(descriptor.as_bytes()),
        );
        match cache_lookup(cache, &key).await {
            CacheLookup::Hit(page) => Ok(page),
            CacheLookup::Fallback => repository
                .resume_items(principal, page)
                .await
                .map_err(Into::into),
            CacheLookup::Leader(_leader) => {
                let page = repository.resume_items(principal, page).await?;
                let ttl = if page.items().is_empty() {
                    cache.empty_ttl
                } else {
                    cache.home_ttl
                };
                cache_put(cache, &key, &page, ttl).await;
                Ok(page)
            }
        }
    }

    /// Returns newest visible media for the principal and optional library.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogServiceError`] for impersonation or query failures.
    #[allow(clippy::too_many_arguments)] // Mirrors the Jellyfin Latest query boundary.
    pub async fn latest_items(
        &self,
        principal: UserId,
        requested_user: Option<UserId>,
        library_id: Option<Uuid>,
        item_types: Vec<CatalogItemType>,
        limit: u64,
        group_items: bool,
        is_played: Option<bool>,
    ) -> Result<Vec<LatestItemRecord>, CatalogServiceError> {
        authorize_user(principal, requested_user)?;
        let repository = CatalogQueryRepository::new(&self.database);
        let Some(cache) = &self.cache else {
            return repository
                .latest_items(
                    principal,
                    library_id,
                    &item_types,
                    limit,
                    group_items,
                    is_played,
                )
                .await
                .map_err(Into::into);
        };
        let revisions = repository.cache_revisions(principal).await?;
        let mut type_names = item_types
            .iter()
            .map(|item_type| item_type.cache_name())
            .collect::<Vec<_>>();
        type_names.sort_unstable();
        let descriptor = format!(
            "library={library_id:?};limit={limit};group={group_items};played={is_played:?};types={}",
            type_names.join(",")
        );
        let key = cache.keys.user_scoped(
            revisions.catalog_generation(),
            &principal.to_string(),
            revisions.user_revision(),
            CacheProjection::Latest,
            &CacheQueryDigest::from_bytes(descriptor.as_bytes()),
        );
        match cache_lookup(cache, &key).await {
            CacheLookup::Hit(items) => Ok(items),
            CacheLookup::Fallback => repository
                .latest_items(
                    principal,
                    library_id,
                    &item_types,
                    limit,
                    group_items,
                    is_played,
                )
                .await
                .map_err(Into::into),
            CacheLookup::Leader(_leader) => {
                let items = repository
                    .latest_items(
                        principal,
                        library_id,
                        &item_types,
                        limit,
                        group_items,
                        is_played,
                    )
                    .await?;
                let ttl = if items.is_empty() {
                    cache.empty_ttl
                } else {
                    cache.home_ttl
                };
                cache_put(cache, &key, &items, ttl).await;
                Ok(items)
            }
        }
    }

    /// Returns the principal's next unplayed episode from each visible Series.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogServiceError`] for impersonation or query failures.
    pub async fn next_up_items(
        &self,
        principal: UserId,
        requested_user: Option<UserId>,
        series_id: Option<CatalogItemId>,
        include_resumable: bool,
        page: CatalogPageRequest,
    ) -> Result<CatalogPage, CatalogServiceError> {
        authorize_user(principal, requested_user)?;
        let repository = CatalogQueryRepository::new(&self.database);
        let Some(cache) = &self.cache else {
            return repository
                .next_up_items(principal, series_id, include_resumable, page)
                .await
                .map_err(Into::into);
        };
        let revisions = repository.cache_revisions(principal).await?;
        let descriptor = format!(
            "series={series_id:?};resumable={include_resumable};{}",
            page_cache_descriptor(&page)
        );
        let key = cache.keys.user_scoped(
            revisions.catalog_generation(),
            &principal.to_string(),
            revisions.user_revision(),
            CacheProjection::NextUp,
            &CacheQueryDigest::from_bytes(descriptor.as_bytes()),
        );
        match cache_lookup(cache, &key).await {
            CacheLookup::Hit(page) => Ok(page),
            CacheLookup::Fallback => repository
                .next_up_items(principal, series_id, include_resumable, page)
                .await
                .map_err(Into::into),
            CacheLookup::Leader(_leader) => {
                let page = repository
                    .next_up_items(principal, series_id, include_resumable, page)
                    .await?;
                let ttl = if page.items().is_empty() {
                    cache.empty_ttl
                } else {
                    cache.home_ttl
                };
                cache_put(cache, &key, &page, ttl).await;
                Ok(page)
            }
        }
    }

    /// Resolves a wire-level parent UUID and returns its catalog page.
    ///
    /// `None` deliberately combines unknown and inaccessible parents so callers
    /// cannot use this boundary to enumerate disabled catalog data.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogServiceError::ForbiddenUser`] for a mismatched user or
    /// propagates a catalog query failure.
    pub async fn items_by_parent_id(
        &self,
        principal: UserId,
        requested_user: Option<UserId>,
        parent_id: Uuid,
        page: CatalogPageRequest,
    ) -> Result<Option<CatalogPage>, CatalogServiceError> {
        self.query_items_by_parent_id(
            principal,
            requested_user,
            parent_id,
            CatalogItemsQuery::new(CatalogItemsScope::AllVisible, page),
        )
        .await
    }

    /// Resolves a wire-level parent UUID and executes a complete catalog query.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogServiceError::ForbiddenUser`] for a mismatched user or
    /// propagates a catalog query failure.
    pub async fn query_items_by_parent_id(
        &self,
        principal: UserId,
        requested_user: Option<UserId>,
        parent_id: Uuid,
        query: CatalogItemsQuery,
    ) -> Result<Option<CatalogPage>, CatalogServiceError> {
        authorize_user(principal, requested_user)?;
        let repository = CatalogQueryRepository::new(&self.database);
        let Some(parent) = repository.resolve_parent(parent_id).await? else {
            return Ok(None);
        };
        if let BrowseParent::Item(parent_item) = parent
            && let Some(target) = repository.lazy_work_target(principal, parent_item).await?
            && target.item_type() == CatalogItemType::Series
            && !target.has_current_structure()
        {
            self.enqueue_and_wait(target, parent_item, WorkTaskKind::ExpandItem)
                .await?;
        }
        self.query_items(
            principal,
            requested_user,
            query.with_scope(CatalogItemsScope::Parent(parent)),
        )
        .await
        .map(Some)
    }

    /// Returns one visible published item for the authenticated principal.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogServiceError::ForbiddenUser`] for user impersonation or
    /// propagates a catalog query failure.
    pub async fn item(
        &self,
        principal: UserId,
        requested_user: Option<UserId>,
        item_id: CatalogItemId,
    ) -> Result<Option<CatalogItemRecord>, CatalogServiceError> {
        authorize_user(principal, requested_user)?;
        let repository = CatalogQueryRepository::new(&self.database);
        let Some(cache) = &self.cache else {
            return self
                .read_item_with_lazy(&repository, principal, item_id)
                .await;
        };
        let revisions = repository.cache_revisions(principal).await?;
        let descriptor = format!("detail/{item_id}");
        let key = cache.keys.user_scoped(
            revisions.catalog_generation(),
            &principal.to_string(),
            revisions.user_revision(),
            CacheProjection::Items,
            &CacheQueryDigest::from_bytes(descriptor.as_bytes()),
        );
        match cache_lookup(cache, &key).await {
            CacheLookup::Hit(item) => Ok(item),
            CacheLookup::Fallback => {
                self.read_item_with_lazy(&repository, principal, item_id)
                    .await
            }
            CacheLookup::Leader(_leader) => {
                let item = self
                    .read_item_with_lazy(&repository, principal, item_id)
                    .await?;
                let ttl = if item.is_some() {
                    cache.item_ttl
                } else {
                    cache.empty_ttl
                };
                cache_put(cache, &key, &item, ttl).await;
                Ok(item)
            }
        }
    }

    /// Returns one visible item with normalized detail metadata for the principal.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogServiceError::ForbiddenUser`] for user impersonation or
    /// propagates catalog, lazy-work, and cache failures.
    pub async fn item_detail(
        &self,
        principal: UserId,
        requested_user: Option<UserId>,
        item_id: CatalogItemId,
    ) -> Result<Option<CatalogItemDetailRecord>, CatalogServiceError> {
        authorize_user(principal, requested_user)?;
        let repository = CatalogQueryRepository::new(&self.database);
        if let Some(target) = repository.lazy_work_target(principal, item_id).await?
            && matches!(
                target.item_type(),
                CatalogItemType::Movie | CatalogItemType::Series | CatalogItemType::Episode
            )
            && target.should_retry_metadata()
        {
            tracing::debug!(
                trigger = "lazy_click",
                task_kind = WorkTaskKind::ResolveMetadata.as_str(),
                scope_type = "CatalogItem",
                scope_id = %item_id,
                expected_revision = target.metadata_revision(),
                "lazy detail requested metadata resolution"
            );
            self.retry_metadata_and_wait(target, item_id).await?;
        }
        let Some(cache) = &self.cache else {
            let mut item = self
                .read_item_detail_with_lazy(&repository, principal, item_id)
                .await?;
            self.apply_direct_metadata(item_id, &mut item).await;
            return Ok(item);
        };
        let revisions = repository.cache_revisions(principal).await?;
        let descriptor = format!("rich-detail/{item_id}");
        let key = cache.keys.user_scoped(
            revisions.catalog_generation(),
            &principal.to_string(),
            revisions.user_revision(),
            CacheProjection::Items,
            &CacheQueryDigest::from_bytes(descriptor.as_bytes()),
        );
        match cache_lookup(cache, &key).await {
            CacheLookup::Hit(mut item) => {
                self.apply_direct_metadata(item_id, &mut item).await;
                Ok(item)
            }
            CacheLookup::Fallback => {
                let mut item = self
                    .read_item_detail_with_lazy(&repository, principal, item_id)
                    .await?;
                self.apply_direct_metadata(item_id, &mut item).await;
                Ok(item)
            }
            CacheLookup::Leader(_leader) => {
                let item = self
                    .read_item_detail_with_lazy(&repository, principal, item_id)
                    .await?;
                let ttl = if item.is_some() {
                    cache.item_ttl
                } else {
                    cache.empty_ttl
                };
                cache_put(cache, &key, &item, ttl).await;
                let mut item = item;
                self.apply_direct_metadata(item_id, &mut item).await;
                Ok(item)
            }
        }
    }

    async fn apply_direct_metadata(
        &self,
        item_id: CatalogItemId,
        item: &mut Option<CatalogItemDetailRecord>,
    ) {
        let (Some(service), Some(item)) = (&self.direct_metadata, item.as_mut()) else {
            return;
        };
        match service.nfo(item_id).await {
            Ok(Some(document)) => item.apply_direct_nfo(&document),
            Ok(None) => {}
            Err(error) => tracing::debug!(%item_id, %error, "direct NFO overlay unavailable"),
        }
    }

    /// Returns visible same-type recommendations for one source item and principal.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogServiceError::ForbiddenUser`] for user impersonation or propagates a
    /// catalog query failure.
    pub async fn similar_items(
        &self,
        principal: UserId,
        requested_user: Option<UserId>,
        item_id: CatalogItemId,
        limit: u64,
    ) -> Result<Option<Vec<CatalogItemRecord>>, CatalogServiceError> {
        authorize_user(principal, requested_user)?;
        CatalogQueryRepository::new(&self.database)
            .similar_items(principal, item_id, limit)
            .await
            .map_err(Into::into)
    }

    async fn read_item_with_lazy(
        &self,
        repository: &CatalogQueryRepository<'_>,
        principal: UserId,
        item_id: CatalogItemId,
    ) -> Result<Option<CatalogItemRecord>, CatalogServiceError> {
        let item = repository.item(principal, item_id).await?;
        if item.is_some()
            && let Some(target) = repository.lazy_work_target(principal, item_id).await?
            && matches!(
                target.item_type(),
                CatalogItemType::Movie | CatalogItemType::Audio
            )
            && !target.has_current_sources()
        {
            self.enqueue_and_wait(target, item_id, WorkTaskKind::IndexMediaSources)
                .await?;
        }
        Ok(item)
    }

    async fn read_item_detail_with_lazy(
        &self,
        repository: &CatalogQueryRepository<'_>,
        principal: UserId,
        item_id: CatalogItemId,
    ) -> Result<Option<CatalogItemDetailRecord>, CatalogServiceError> {
        if self
            .read_item_with_lazy(repository, principal, item_id)
            .await?
            .is_none()
        {
            return Ok(None);
        }
        repository
            .item_detail(principal, item_id)
            .await
            .map_err(Into::into)
    }

    /// Returns only probed, currently available sources suitable for direct playback.
    ///
    /// Missing source indexes and probes are joined through durable high-priority
    /// work. Timeout or worker failure returns the current safe subset, never an
    /// invented Direct Play source.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogServiceError`] for authorization, query, or work failures.
    pub async fn playback_sources(
        &self,
        principal: UserId,
        requested_user: Option<UserId>,
        item_id: CatalogItemId,
    ) -> Result<Option<Vec<PlaybackSource>>, CatalogServiceError> {
        authorize_user(principal, requested_user)?;
        let query = CatalogQueryRepository::new(&self.database);
        let Some(item) = query.item(principal, item_id).await? else {
            return Ok(None);
        };
        let is_audio = item.item_type() == "Audio";
        let cache_revisions = if self.cache.is_some() {
            Some(query.cache_revisions(principal).await?)
        } else {
            None
        };
        let publications = CatalogPublicationRepository::new(&self.database);
        let mut sources = publications.playable_sources(item_id).await?;
        tracing::debug!(item_id = %item_id, source_count = sources.len(), "playback source resolution started");
        if sources.is_empty()
            && let Some(target) = query.lazy_work_target(principal, item_id).await?
            && matches!(
                target.item_type(),
                CatalogItemType::Movie | CatalogItemType::Episode | CatalogItemType::Audio
            )
        {
            self.enqueue_and_wait(target, item_id, WorkTaskKind::IndexMediaSources)
                .await?;
            sources = publications.playable_sources(item_id).await?;
            tracing::debug!(item_id = %item_id, source_count = sources.len(), "playback source index completed");
        }
        let jobs = WorkJobRepository::new(&self.database);
        let deadline = Instant::now() + self.lazy_wait_timeout;
        let mut probe_jobs = Vec::new();
        for source in &sources {
            if source.probe_state() != "Probed"
                && source
                    .locations()
                    .iter()
                    .any(|location| location.availability_state() == "Available")
            {
                let submission = jobs
                    .enqueue_or_join(&WorkJobSpec::new(
                        WorkTaskKind::ProbeMedia,
                        WorkScope::MediaSource(source.id()),
                        source.probe_revision(),
                        200,
                    )?)
                    .await?;
                tracing::debug!(
                    item_id = %item_id,
                    media_source_id = %source.id().as_uuid(),
                    job_id = %submission.job().id().as_uuid(),
                    probe_revision = source.probe_revision(),
                    created = submission.created(),
                    "playback probe enqueued or joined"
                );
                probe_jobs.push(submission.job().id());
            }
        }
        for job_id in probe_jobs {
            let _ = self.wait_for_job(&jobs, job_id, deadline).await?;
        }
        if !sources.is_empty() {
            sources = publications.playable_sources(item_id).await?;
        }
        tracing::debug!(item_id = %item_id, source_count = sources.len(), "playback sources refreshed after probes");
        let last_used = PlaystateRepository::new(&self.database)
            .last_presentation_key(principal, item_id)
            .await?;
        let Some(cache) = &self.cache else {
            return Ok(Some(playable_sources(sources, last_used, is_audio)));
        };
        let revisions = query.cache_revisions(principal).await?;
        if Some(revisions) != cache_revisions {
            return Ok(Some(playable_sources(sources, last_used, is_audio)));
        }
        let probe_digest = playback_probe_digest(&sources);
        let key = cache.keys.playback_info(
            revisions.catalog_generation(),
            &principal.to_string(),
            revisions.user_revision(),
            &item_id.to_string(),
            &probe_digest,
        );
        match cache_lookup(cache, &key).await {
            CacheLookup::Hit(sources) => Ok(Some(sources)),
            CacheLookup::Fallback => Ok(Some(playable_sources(sources, last_used, is_audio))),
            CacheLookup::Leader(_leader) => {
                let playable = playable_sources(sources, last_used, is_audio);
                let ttl = if playable.is_empty() {
                    cache.empty_ttl
                } else {
                    cache.item_ttl
                };
                cache_put(cache, &key, &playable, ttl).await;
                Ok(Some(playable))
            }
        }
    }

    /// Returns the currently available, already-probed playback sources without
    /// scheduling indexing or probe work.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogServiceError`] for authorization or query failures.
    pub async fn available_playback_sources(
        &self,
        principal: UserId,
        requested_user: Option<UserId>,
        item_id: CatalogItemId,
    ) -> Result<Option<Vec<PlaybackSource>>, CatalogServiceError> {
        authorize_user(principal, requested_user)?;
        let query = CatalogQueryRepository::new(&self.database);
        let Some(item) = query.item(principal, item_id).await? else {
            return Ok(None);
        };
        let sources = CatalogPublicationRepository::new(&self.database)
            .playable_sources(item_id)
            .await?;
        let last_used = PlaystateRepository::new(&self.database)
            .last_presentation_key(principal, item_id)
            .await?;
        Ok(Some(playable_sources(
            sources,
            last_used,
            item.item_type() == "Audio",
        )))
    }

    /// Updates the administrator policy for a stable media source identity.
    ///
    /// This mutates only canonical source policy and advances the catalog
    /// generation inside the same transaction.
    ///
    /// # Errors
    ///
    /// Returns [`SourcePlaybackPolicyError`] when the policy is contradictory,
    /// the source is unavailable, or its transaction fails.
    pub async fn set_source_playback_policy(
        &self,
        item_id: CatalogItemId,
        presentation_key: tjxy_common::PresentationKey,
        policy: SourcePlaybackPolicy,
    ) -> Result<(), SourcePlaybackPolicyError> {
        CatalogPublicationRepository::new(&self.database)
            .set_source_playback_policy(item_id, presentation_key, policy)
            .await
    }

    async fn enqueue_and_wait(
        &self,
        target: LazyCatalogWorkTarget,
        item_id: CatalogItemId,
        task_kind: WorkTaskKind,
    ) -> Result<(), CatalogServiceError> {
        let revision = match task_kind {
            WorkTaskKind::ExpandItem => target.structure_revision(),
            WorkTaskKind::IndexMediaSources => target.source_revision(),
            WorkTaskKind::ScopedStorageSync
            | WorkTaskKind::RecoverStorageCursor
            | WorkTaskKind::ValidateStorageRoot
            | WorkTaskKind::DiscoverTitles
            | WorkTaskKind::ResolveMetadata
            | WorkTaskKind::ProbeMedia
            | WorkTaskKind::FullMediaScan
            | WorkTaskKind::FullLibraryRootScan => {
                return Err(CatalogServiceError::InvalidLazyTask);
            }
        };
        let Some(scope) = target.storage_scope() else {
            return Ok(());
        };
        let jobs = WorkJobRepository::new(&self.database);
        let deadline = Instant::now() + self.lazy_wait_timeout;
        let direct_audio = task_kind == WorkTaskKind::IndexMediaSources
            && target.item_type() == CatalogItemType::Audio;
        let media_spec =
            if scope.is_ready() || (direct_audio && scope.is_ready_for_direct_source()) {
                WorkJobSpec::new(task_kind, WorkScope::CatalogItem(item_id), revision, 100)?
                    .with_input_sync_revision(if direct_audio {
                        scope.metadata_input_revision()
                    } else {
                        scope.children_revision()
                    })?
            } else {
                let sync = jobs
                    .enqueue_or_join(
                        &WorkJobSpec::new(
                            WorkTaskKind::ScopedStorageSync,
                            WorkScope::StorageObject(scope.storage_object_id()),
                            scope.children_revision(),
                            100,
                        )?
                        .with_storage_root_affinity(scope.storage_root_id())?,
                    )
                    .await?;
                if self.wait_for_job(&jobs, sync.job().id(), deadline).await?
                    != LazyWaitOutcome::Completed
                {
                    return Ok(());
                }
                let Some(sync_revision) = jobs.completed_sync_revision(sync.job().id()).await?
                else {
                    return Ok(());
                };
                WorkJobSpec::new(task_kind, WorkScope::CatalogItem(item_id), revision, 100)?
                    .with_required_sync(sync.job().id(), sync_revision)
            }
            .with_storage_root_affinity(scope.storage_root_id())?;
        let Some(submission) = jobs.enqueue_lazy_or_join(&media_spec).await? else {
            return Ok(());
        };
        tracing::debug!(
            trigger = "lazy_click",
            job_id = %submission.job().id().as_uuid(),
            task_kind = task_kind.as_str(),
            scope_type = "CatalogItem",
            scope_id = %item_id,
            created = submission.created(),
            "lazy media work enqueued or joined"
        );
        let _ = self
            .wait_for_job(&jobs, submission.job().id(), deadline)
            .await?;
        Ok(())
    }

    async fn retry_metadata_and_wait(
        &self,
        target: LazyCatalogWorkTarget,
        item_id: CatalogItemId,
    ) -> Result<(), CatalogServiceError> {
        let Some(requirement) = target.metadata_requirement() else {
            return Ok(());
        };
        let Some(scope) = target.storage_scope() else {
            return Ok(());
        };
        let jobs = WorkJobRepository::new(&self.database);
        let deadline = Instant::now() + self.lazy_wait_timeout;
        let mut spec = if scope.is_ready() {
            WorkJobSpec::new(
                WorkTaskKind::ResolveMetadata,
                WorkScope::CatalogItem(item_id),
                target.metadata_revision(),
                100,
            )?
            .with_input_sync_revision(scope.metadata_input_revision())?
        } else {
            let sync = jobs
                .enqueue_or_join(
                    &WorkJobSpec::new(
                        WorkTaskKind::ScopedStorageSync,
                        WorkScope::StorageObject(scope.storage_object_id()),
                        scope.children_revision(),
                        100,
                    )?
                    .with_storage_root_affinity(scope.storage_root_id())?,
                )
                .await?;
            if self.wait_for_job(&jobs, sync.job().id(), deadline).await?
                != LazyWaitOutcome::Completed
            {
                return Ok(());
            }
            let Some(sync_revision) = jobs.completed_sync_revision(sync.job().id()).await? else {
                return Ok(());
            };
            WorkJobSpec::new(
                WorkTaskKind::ResolveMetadata,
                WorkScope::CatalogItem(item_id),
                target.metadata_revision(),
                100,
            )?
            .with_required_sync(sync.job().id(), sync_revision)
        };
        spec = spec
            .with_metadata_requirement(requirement)?
            .with_metadata_source_mode(target.metadata_source_mode())?
            .with_local_metadata_access_mode(target.local_metadata_access_mode())?
            .with_storage_root_affinity(scope.storage_root_id())?;
        let submission = if !target.needs_metadata_resolution(requirement)
            && target.local_metadata_access_mode().imports_metadata()
            && matches!(
                target.metadata_source_mode(),
                MetadataSourceMode::AutomaticScrape
            ) {
            jobs.enqueue_metadata_retry_or_join(&spec).await
        } else {
            jobs.enqueue_or_join(&spec).await.map(Some)
        };
        let submission = match submission {
            Ok(submission) => submission,
            Err(WorkJobRepositoryError::IncompatibleActiveJob) => {
                tracing::debug!(
                    %item_id,
                    expected_revision = target.metadata_revision(),
                    "lazy metadata joined an active job with a different storage prerequisite"
                );
                None
            }
            Err(error) => return Err(error.into()),
        };
        let Some(submission) = submission else {
            return Ok(());
        };
        tracing::debug!(
            trigger = "lazy_click",
            job_id = %submission.job().id().as_uuid(),
            task_kind = WorkTaskKind::ResolveMetadata.as_str(),
            scope_type = "CatalogItem",
            scope_id = %item_id,
            created = submission.created(),
            "lazy metadata work enqueued or joined"
        );
        let _ = self
            .wait_for_job(&jobs, submission.job().id(), deadline)
            .await?;
        Ok(())
    }

    async fn wait_for_job(
        &self,
        jobs: &WorkJobRepository<'_>,
        job_id: WorkJobId,
        deadline: Instant,
    ) -> Result<LazyWaitOutcome, CatalogServiceError> {
        let mut delay = Duration::from_millis(50);
        let outcome = loop {
            if Instant::now() >= deadline {
                break LazyWaitOutcome::TimedOut;
            }
            match jobs.get(job_id).await?.map(|job| job.state()) {
                Some(WorkJobState::Completed) => break LazyWaitOutcome::Completed,
                Some(WorkJobState::Failed) => break LazyWaitOutcome::Failed,
                None => break LazyWaitOutcome::Missing,
                Some(WorkJobState::Pending | WorkJobState::Running) => {
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    if remaining.is_zero() {
                        break LazyWaitOutcome::TimedOut;
                    }
                    tokio::time::sleep(delay.min(remaining)).await;
                    delay = delay.saturating_mul(2).min(Duration::from_millis(250));
                }
            }
        };
        tracing::debug!(
            job_id = %job_id.as_uuid(),
            ?outcome,
            "playback probe wait finished"
        );
        Ok(outcome)
    }
}

#[derive(Debug, Error)]
pub enum CatalogServiceError {
    #[error("requested user does not match the authenticated principal")]
    ForbiddenUser,
    #[error("catalog query failed: {0}")]
    Query(#[from] CatalogQueryError),
    #[error("lazy catalog work failed: {0}")]
    Work(#[from] WorkJobRepositoryError),
    #[error("catalog publication failed: {0}")]
    Publication(#[from] CatalogPublicationError),
    #[error("playstate query failed: {0}")]
    Playstate(#[from] PlaystateRepositoryError),
    #[error("unsupported lazy catalog task")]
    InvalidLazyTask,
}

async fn cache_lookup<T>(cache: &CatalogCache, key: &str) -> CacheLookup<T>
where
    T: DeserializeOwned,
{
    if let Some(bytes) = cache.store.get(key).await {
        if let Ok(value) = serde_json::from_slice(&bytes) {
            return CacheLookup::Hit(value);
        }
        cache.store.delete(key).await;
    }
    match cache.single_flight.enter(key) {
        CacheFillPermit::Leader(leader) => CacheLookup::Leader(leader),
        CacheFillPermit::Waiter(waiter) => {
            if waiter.wait().await
                && let Some(bytes) = cache.store.get(key).await
            {
                if let Ok(value) = serde_json::from_slice(&bytes) {
                    return CacheLookup::Hit(value);
                }
                cache.store.delete(key).await;
            }
            CacheLookup::Fallback
        }
        CacheFillPermit::Bypass => CacheLookup::Fallback,
    }
}

async fn cache_put<T>(cache: &CatalogCache, key: &str, value: &T, ttl: Duration)
where
    T: Serialize,
{
    if let Ok(bytes) = serde_json::to_vec(value) {
        cache.store.put(key, &bytes, ttl).await;
    }
}

fn playback_probe_digest(sources: &[tjxy_db::PublishedMediaSource]) -> PlaybackProbeDigest {
    let mut revisions = sources
        .iter()
        .map(|source| (source.id().as_uuid(), source.probe_revision()))
        .collect::<Vec<_>>();
    revisions.sort_unstable();
    let mut digest = Sha256::new();
    for (source_id, probe_revision) in revisions {
        digest.update(source_id.as_bytes());
        digest.update(probe_revision.to_be_bytes());
    }
    PlaybackProbeDigest::new(format!("{:x}", digest.finalize()))
        .expect("SHA-256 digest is a valid Redis key segment")
}

fn playable_sources(
    sources: Vec<tjxy_db::PublishedMediaSource>,
    last_used: Option<tjxy_common::PresentationKey>,
    is_audio: bool,
) -> Vec<PlaybackSource> {
    let mut playable = sources
        .into_iter()
        .filter_map(|source| {
            if source.probe_state() != "Probed"
                || source.is_hidden()
                || !source
                    .locations()
                    .iter()
                    .any(|location| location.availability_state() == "Available")
            {
                return None;
            }
            let container = source.container()?.to_owned();
            let streams = source
                .streams()
                .iter()
                .map(|stream| PlaybackStream {
                    stream_type: stream.stream_type().to_owned(),
                    codec: stream.codec().map(str::to_owned),
                    language: stream.language().map(str::to_owned),
                    delivery_index: stream.delivery_index(),
                    is_default: stream.is_default(),
                    is_forced: stream.is_forced(),
                    width: stream.width(),
                    height: stream.height(),
                    channels: stream.channels(),
                    profile: stream.profile().map(str::to_owned),
                    level: stream.level(),
                })
                .collect::<Vec<_>>();
            let subtitles = source
                .subtitles()
                .iter()
                .filter_map(|subtitle| {
                    Some(PlaybackSubtitle {
                        format: subtitle.format().to_owned(),
                        language: subtitle.language().map(str::to_owned),
                        delivery_index: subtitle.delivery_index()?,
                        is_default: subtitle.is_default(),
                        is_forced: subtitle.is_forced(),
                    })
                })
                .collect();
            let priority = source
                .locations()
                .iter()
                .filter(|location| location.availability_state() == "Available")
                .map(tjxy_db::PublishedMediaLocation::priority)
                .max()
                .unwrap_or(i32::MIN);
            let resolution_pixels = streams
                .iter()
                .filter(|stream| stream.stream_type() == "Video")
                .filter_map(|stream| {
                    stream
                        .width()
                        .zip(stream.height())
                        .and_then(|(width, height)| i64::from(width).checked_mul(i64::from(height)))
                })
                .max()
                .unwrap_or_default();
            let account_health = source
                .locations()
                .iter()
                .filter(|location| location.availability_state() == "Available")
                .map(|location| match location.account_status() {
                    "Active" => 2,
                    "Ready" => 1,
                    _ => 0,
                })
                .max()
                .unwrap_or_default();
            Some((
                priority,
                PlaybackSource {
                    id: source.id(),
                    presentation_key: source.presentation_key(),
                    container,
                    edition: source.edition().map(str::to_owned),
                    bitrate: source.bitrate(),
                    runtime_ticks: source.runtime_ticks(),
                    is_audio,
                    last_used: last_used == Some(source.presentation_key()),
                    admin_priority: source.admin_priority(),
                    is_default: source.is_default(),
                    resolution_pixels,
                    account_health,
                    location_priority: priority,
                    streams,
                    subtitles,
                },
            ))
        })
        .collect::<Vec<_>>();
    sort_playable_sources(&mut playable);
    playable.into_iter().map(|(_, source)| source).collect()
}

fn sort_playable_sources(playable: &mut [(i32, PlaybackSource)]) {
    playable.sort_by(|left, right| {
        right
            .1
            .last_used
            .cmp(&left.1.last_used)
            .then_with(|| right.1.is_default.cmp(&left.1.is_default))
            .then_with(|| right.1.admin_priority.cmp(&left.1.admin_priority))
            .then_with(|| right.1.resolution_pixels.cmp(&left.1.resolution_pixels))
            .then_with(|| right.1.account_health.cmp(&left.1.account_health))
            .then_with(|| right.0.cmp(&left.0))
            .then_with(|| {
                left.1
                    .presentation_key
                    .as_uuid()
                    .cmp(&right.1.presentation_key.as_uuid())
            })
    });
}

fn items_cache_descriptor(query: &CatalogItemsQuery) -> String {
    let scope = match query.scope() {
        CatalogItemsScope::AllVisible => "all-visible".to_owned(),
        CatalogItemsScope::Parent(BrowseParent::Library(id)) => format!("library/{id}"),
        CatalogItemsScope::Parent(BrowseParent::Item(id)) => format!("item/{id}"),
    };
    let page = query.page();
    let mut item_types = page
        .item_types()
        .iter()
        .map(|item_type| item_type.cache_name())
        .collect::<Vec<_>>();
    item_types.sort_unstable();
    let sorts = query
        .sorts()
        .iter()
        .map(|sort| {
            let field = match sort.field() {
                CatalogSortField::SortName => "sort-name",
                CatalogSortField::DateCreated => "date-created",
                CatalogSortField::ProductionYear => "production-year",
                CatalogSortField::Runtime => "runtime",
            };
            let order = match sort.order() {
                CatalogSortOrder::Ascending => "asc",
                CatalogSortOrder::Descending => "desc",
            };
            format!("{field}:{order}")
        })
        .collect::<Vec<_>>()
        .join(",");
    let search = query.search_term().unwrap_or_default();
    let genre = query.genre().unwrap_or_default();
    let production_year = query
        .production_year()
        .map_or_else(String::new, |year| year.to_string());
    let recursive = query.recursive()
        || (query.recursive_for_library()
            && matches!(
                query.scope(),
                CatalogItemsScope::Parent(BrowseParent::Library(_))
            ));
    format!(
        "query-items/v2;scope={scope};recursive={};favorite-only={};search-length={};search={search};genre-length={};genre={genre};production-year={production_year};start={};limit={};types={};sorts={sorts}",
        recursive,
        query.favorite_only(),
        search.len(),
        genre.len(),
        page.start_index(),
        page.limit(),
        item_types.join(",")
    )
}

fn page_cache_descriptor(page: &CatalogPageRequest) -> String {
    format!("start={};limit={}", page.start_index(), page.limit())
}

fn search_cache_descriptor(search_term: &str, page: &CatalogPageRequest) -> String {
    let mut item_types = page
        .item_types()
        .iter()
        .map(|item_type| item_type.cache_name())
        .collect::<Vec<_>>();
    item_types.sort_unstable();
    format!(
        "term-length={};term={search_term};start={};limit={};types={}",
        search_term.len(),
        page.start_index(),
        page.limit(),
        item_types.join(",")
    )
}

fn authorize_user(
    principal: UserId,
    requested_user: Option<UserId>,
) -> Result<(), CatalogServiceError> {
    if requested_user.is_some_and(|requested| requested != principal) {
        return Err(CatalogServiceError::ForbiddenUser);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use tjxy_common::{MediaSourceId, PresentationKey};
    use tjxy_db::{
        BrowseParent, CatalogItemsQuery, CatalogItemsScope, CatalogPageRequest, CatalogSort,
        CatalogSortField, CatalogSortOrder,
    };

    use super::{PlaybackSource, items_cache_descriptor, sort_playable_sources};

    #[test]
    fn last_used_source_precedes_location_priority_with_stable_key_fallback() {
        let stable = PlaybackSource {
            id: MediaSourceId::new(),
            presentation_key: PresentationKey::new(),
            container: "mkv".to_owned(),
            edition: None,
            bitrate: None,
            runtime_ticks: None,
            is_audio: false,
            last_used: false,
            admin_priority: 0,
            is_default: false,
            resolution_pixels: 0,
            account_health: 0,
            location_priority: 100,
            streams: Vec::new(),
            subtitles: Vec::new(),
        };
        let last_used = PlaybackSource {
            id: MediaSourceId::new(),
            presentation_key: PresentationKey::new(),
            container: "mkv".to_owned(),
            edition: None,
            bitrate: None,
            runtime_ticks: None,
            is_audio: false,
            last_used: true,
            admin_priority: 0,
            is_default: false,
            resolution_pixels: 0,
            account_health: 0,
            location_priority: 1,
            streams: Vec::new(),
            subtitles: Vec::new(),
        };
        let mut sources = vec![(100, stable.clone()), (1, last_used.clone())];

        sort_playable_sources(&mut sources);

        assert_eq!(
            sources[0].1.presentation_key(),
            last_used.presentation_key()
        );
        assert_eq!(sources[1].1.presentation_key(), stable.presentation_key());
    }

    #[test]
    fn item_query_cache_descriptor_covers_every_result_dimension() {
        let parent = tjxy_common::CatalogItemId::new();
        let base = CatalogItemsQuery::new(
            CatalogItemsScope::Parent(BrowseParent::Item(parent)),
            CatalogPageRequest::new(0, 20).unwrap(),
        );
        let search = base.clone().with_search_term(Some("Pilot".to_owned()));
        let recursive = search.clone().with_recursive(true);
        let sorted = recursive.clone().with_sorts(vec![CatalogSort::new(
            CatalogSortField::DateCreated,
            CatalogSortOrder::Descending,
        )]);
        let favorite = sorted.clone().with_favorite_only(true);
        let library = CatalogItemsQuery::new(
            CatalogItemsScope::Parent(BrowseParent::Library(uuid::Uuid::new_v4())),
            CatalogPageRequest::new(0, 20).unwrap(),
        );
        let library_default_recursive = library.clone().with_recursive_for_library(true);

        let descriptors = [
            base,
            search,
            recursive,
            sorted,
            favorite,
            library,
            library_default_recursive,
        ]
        .iter()
        .map(items_cache_descriptor)
        .collect::<std::collections::BTreeSet<_>>();

        assert_eq!(descriptors.len(), 7);
    }
}
