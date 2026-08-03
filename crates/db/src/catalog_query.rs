use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbErr, QueryResult,
    sea_query::{
        Alias, CommonTableExpression, Condition, Expr, Func, JoinType, LikeExpr, NullOrdering,
        Order, Query, SelectStatement, UnionType, WithClause,
    },
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tjxy_common::{
    CatalogItemId, ImageType, PublicationId, StorageObjectRecordId, StorageRootId, UserId,
};
use uuid::Uuid;

use crate::MetadataRequirement;

const MAX_PAGE_SIZE: u64 = 200;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BrowseParent {
    Library(Uuid),
    Item(CatalogItemId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CatalogItemType {
    Movie,
    Audio,
    Series,
    Season,
    Episode,
    Folder,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CatalogSortField {
    SortName,
    DateCreated,
    ProductionYear,
    Runtime,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CatalogSortOrder {
    Ascending,
    Descending,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CatalogSort {
    field: CatalogSortField,
    order: CatalogSortOrder,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CatalogItemsScope {
    AllVisible,
    Parent(BrowseParent),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogItemsQuery {
    scope: CatalogItemsScope,
    page: CatalogPageRequest,
    search_term: Option<String>,
    recursive: bool,
    recursive_for_library: bool,
    sorts: Vec<CatalogSort>,
    genre: Option<String>,
    production_year: Option<i32>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogFilterFacets {
    genres: Vec<String>,
    production_years: Vec<i32>,
}

impl CatalogFilterFacets {
    #[must_use]
    pub fn genres(&self) -> &[String] {
        &self.genres
    }

    #[must_use]
    pub fn production_years(&self) -> &[i32] {
        &self.production_years
    }
}

impl CatalogItemsQuery {
    #[must_use]
    pub const fn new(scope: CatalogItemsScope, page: CatalogPageRequest) -> Self {
        Self {
            scope,
            page,
            search_term: None,
            recursive: false,
            recursive_for_library: false,
            sorts: Vec::new(),
            genre: None,
            production_year: None,
        }
    }

    #[must_use]
    pub fn with_search_term(mut self, search_term: Option<String>) -> Self {
        self.search_term = search_term;
        self
    }

    #[must_use]
    pub const fn with_scope(mut self, scope: CatalogItemsScope) -> Self {
        self.scope = scope;
        self
    }

    #[must_use]
    pub const fn with_recursive(mut self, recursive: bool) -> Self {
        self.recursive = recursive;
        self
    }

    #[must_use]
    pub const fn with_recursive_for_library(mut self, recursive: bool) -> Self {
        self.recursive_for_library = recursive;
        self
    }

    #[must_use]
    pub fn with_sorts(mut self, sorts: Vec<CatalogSort>) -> Self {
        self.sorts = sorts;
        self
    }

    #[must_use]
    pub fn with_genre(mut self, genre: Option<String>) -> Self {
        self.genre = genre;
        self
    }

    #[must_use]
    pub const fn with_production_year(mut self, production_year: Option<i32>) -> Self {
        self.production_year = production_year;
        self
    }

    #[must_use]
    pub const fn scope(&self) -> CatalogItemsScope {
        self.scope
    }

    #[must_use]
    pub const fn page(&self) -> &CatalogPageRequest {
        &self.page
    }

    #[must_use]
    pub fn search_term(&self) -> Option<&str> {
        self.search_term.as_deref()
    }

    #[must_use]
    pub const fn recursive(&self) -> bool {
        self.recursive
    }

    #[must_use]
    pub const fn recursive_for_library(&self) -> bool {
        self.recursive_for_library
    }

    #[must_use]
    pub fn sorts(&self) -> &[CatalogSort] {
        &self.sorts
    }

    #[must_use]
    pub fn genre(&self) -> Option<&str> {
        self.genre.as_deref()
    }

    #[must_use]
    pub const fn production_year(&self) -> Option<i32> {
        self.production_year
    }
}

impl CatalogSort {
    #[must_use]
    pub const fn new(field: CatalogSortField, order: CatalogSortOrder) -> Self {
        Self { field, order }
    }

    #[must_use]
    pub const fn field(self) -> CatalogSortField {
        self.field
    }

    #[must_use]
    pub const fn order(self) -> CatalogSortOrder {
        self.order
    }
}

impl CatalogItemType {
    const fn as_database_value(self) -> &'static str {
        match self {
            Self::Movie => "Movie",
            Self::Audio => "Audio",
            Self::Series => "Series",
            Self::Season => "Season",
            Self::Episode => "Episode",
            Self::Folder => "Folder",
        }
    }

    fn from_database_value(value: &str) -> Result<Self, CatalogQueryError> {
        match value {
            "Movie" => Ok(Self::Movie),
            "Audio" => Ok(Self::Audio),
            "Series" => Ok(Self::Series),
            "Season" => Ok(Self::Season),
            "Episode" => Ok(Self::Episode),
            "Folder" => Ok(Self::Folder),
            _ => Err(CatalogQueryError::InvalidItemType),
        }
    }

    #[must_use]
    pub const fn cache_name(self) -> &'static str {
        match self {
            Self::Movie => "Movie",
            Self::Audio => "Audio",
            Self::Series => "Series",
            Self::Season => "Season",
            Self::Episode => "Episode",
            Self::Folder => "Folder",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogPageRequest {
    start_index: u64,
    limit: u64,
    item_types: Vec<CatalogItemType>,
}

impl CatalogPageRequest {
    /// Creates a bounded offset page.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogQueryError::InvalidPage`] for a zero or oversized limit.
    pub const fn new(start_index: u64, limit: u64) -> Result<Self, CatalogQueryError> {
        if limit == 0 || limit > MAX_PAGE_SIZE {
            return Err(CatalogQueryError::InvalidPage);
        }
        Ok(Self {
            start_index,
            limit,
            item_types: Vec::new(),
        })
    }

    #[must_use]
    pub fn with_item_types(mut self, item_types: Vec<CatalogItemType>) -> Self {
        self.item_types = item_types;
        self
    }

    #[must_use]
    pub const fn start_index(&self) -> u64 {
        self.start_index
    }

    #[must_use]
    pub const fn limit(&self) -> u64 {
        self.limit
    }

    #[must_use]
    pub fn has_item_type_filter(&self) -> bool {
        !self.item_types.is_empty()
    }

    #[must_use]
    pub fn item_types(&self) -> &[CatalogItemType] {
        &self.item_types
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LibraryViewRecord {
    id: Uuid,
    name: String,
    collection_type: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CacheRevisions {
    catalog_generation: i64,
    user_revision: i64,
}

impl CacheRevisions {
    #[must_use]
    pub const fn catalog_generation(self) -> i64 {
        self.catalog_generation
    }

    #[must_use]
    pub const fn user_revision(self) -> i64 {
        self.user_revision
    }
}

impl LibraryViewRecord {
    #[must_use]
    pub const fn id(&self) -> Uuid {
        self.id
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn collection_type(&self) -> &str {
        &self.collection_type
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CatalogItemRecord {
    id: CatalogItemId,
    parent_id: Option<CatalogItemId>,
    item_type: String,
    name: String,
    original_title: Option<String>,
    production_year: Option<i32>,
    overview: Option<String>,
    community_rating: Option<f64>,
    index_number: Option<i32>,
    runtime_ticks: Option<i64>,
    date_created: DateTime<Utc>,
    is_favorite: bool,
    is_played: bool,
    play_count: i32,
    playback_position_ticks: i64,
    image_tags: BTreeMap<String, String>,
    backdrop_image_tags: Vec<String>,
    primary_image_aspect_ratio: Option<f64>,
}

impl CatalogItemRecord {
    #[must_use]
    pub const fn id(&self) -> CatalogItemId {
        self.id
    }

    #[must_use]
    pub const fn parent_id(&self) -> Option<CatalogItemId> {
        self.parent_id
    }

    #[must_use]
    pub fn item_type(&self) -> &str {
        &self.item_type
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn original_title(&self) -> Option<&str> {
        self.original_title.as_deref()
    }

    #[must_use]
    pub const fn production_year(&self) -> Option<i32> {
        self.production_year
    }

    #[must_use]
    pub fn overview(&self) -> Option<&str> {
        self.overview.as_deref()
    }

    #[must_use]
    pub const fn community_rating(&self) -> Option<f64> {
        self.community_rating
    }

    #[must_use]
    pub const fn index_number(&self) -> Option<i32> {
        self.index_number
    }

    #[must_use]
    pub const fn runtime_ticks(&self) -> Option<i64> {
        self.runtime_ticks
    }

    #[must_use]
    pub const fn date_created(&self) -> DateTime<Utc> {
        self.date_created
    }

    #[must_use]
    pub const fn is_favorite(&self) -> bool {
        self.is_favorite
    }

    #[must_use]
    pub const fn is_played(&self) -> bool {
        self.is_played
    }

    #[must_use]
    pub const fn play_count(&self) -> i32 {
        self.play_count
    }

    #[must_use]
    pub const fn playback_position_ticks(&self) -> i64 {
        self.playback_position_ticks
    }

    #[must_use]
    pub const fn image_tags(&self) -> &BTreeMap<String, String> {
        &self.image_tags
    }

    #[must_use]
    pub fn backdrop_image_tags(&self) -> &[String] {
        &self.backdrop_image_tags
    }

    #[must_use]
    pub const fn primary_image_aspect_ratio(&self) -> Option<f64> {
        self.primary_image_aspect_ratio
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CatalogNamedCodeRecord {
    code: String,
    name: String,
}

impl CatalogNamedCodeRecord {
    #[must_use]
    pub fn code(&self) -> &str {
        &self.code
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CatalogCreditRecord {
    person_id: Uuid,
    person_name: String,
    role: String,
    credit_type: Option<String>,
    sort_order: i32,
}

impl CatalogCreditRecord {
    #[must_use]
    pub const fn person_id(&self) -> Uuid {
        self.person_id
    }

    #[must_use]
    pub fn person_name(&self) -> &str {
        &self.person_name
    }

    #[must_use]
    pub fn role(&self) -> &str {
        &self.role
    }

    #[must_use]
    pub fn credit_type(&self) -> Option<&str> {
        self.credit_type.as_deref()
    }

    #[must_use]
    pub const fn sort_order(&self) -> i32 {
        self.sort_order
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CatalogItemDetailRecord {
    item: CatalogItemRecord,
    tagline: Option<String>,
    vote_count: Option<i64>,
    runtime_ticks: Option<i64>,
    premiere_date: Option<DateTime<Utc>>,
    end_date: Option<DateTime<Utc>>,
    release_status: Option<String>,
    official_rating: Option<String>,
    original_language: Option<String>,
    genres: Vec<String>,
    studios: Vec<String>,
    countries: Vec<CatalogNamedCodeRecord>,
    languages: Vec<CatalogNamedCodeRecord>,
    credits: Vec<CatalogCreditRecord>,
    provider_ids: BTreeMap<String, String>,
    has_media_sources: bool,
}

impl CatalogItemDetailRecord {
    #[must_use]
    pub const fn item(&self) -> &CatalogItemRecord {
        &self.item
    }

    #[must_use]
    pub fn tagline(&self) -> Option<&str> {
        self.tagline.as_deref()
    }

    #[must_use]
    pub const fn community_rating(&self) -> Option<f64> {
        self.item.community_rating
    }

    #[must_use]
    pub const fn vote_count(&self) -> Option<i64> {
        self.vote_count
    }

    #[must_use]
    pub const fn runtime_ticks(&self) -> Option<i64> {
        self.runtime_ticks
    }

    #[must_use]
    pub const fn premiere_date(&self) -> Option<DateTime<Utc>> {
        self.premiere_date
    }

    #[must_use]
    pub const fn end_date(&self) -> Option<DateTime<Utc>> {
        self.end_date
    }

    #[must_use]
    pub fn release_status(&self) -> Option<&str> {
        self.release_status.as_deref()
    }

    #[must_use]
    pub fn official_rating(&self) -> Option<&str> {
        self.official_rating.as_deref()
    }

    #[must_use]
    pub fn original_language(&self) -> Option<&str> {
        self.original_language.as_deref()
    }

    #[must_use]
    pub fn genres(&self) -> &[String] {
        &self.genres
    }

    #[must_use]
    pub fn studios(&self) -> &[String] {
        &self.studios
    }

    #[must_use]
    pub fn countries(&self) -> &[CatalogNamedCodeRecord] {
        &self.countries
    }

    #[must_use]
    pub fn languages(&self) -> &[CatalogNamedCodeRecord] {
        &self.languages
    }

    #[must_use]
    pub fn credits(&self) -> &[CatalogCreditRecord] {
        &self.credits
    }

    #[must_use]
    pub const fn provider_ids(&self) -> &BTreeMap<String, String> {
        &self.provider_ids
    }

    #[must_use]
    pub const fn has_media_sources(&self) -> bool {
        self.has_media_sources
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssetRecord {
    sha256: String,
    mime_type: String,
    width: Option<i32>,
    height: Option<i32>,
    byte_size: u64,
    local_relative_path: String,
}

impl AssetRecord {
    #[must_use]
    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    #[must_use]
    pub fn mime_type(&self) -> &str {
        &self.mime_type
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
    pub const fn byte_size(&self) -> u64 {
        self.byte_size
    }

    #[must_use]
    pub fn local_relative_path(&self) -> &str {
        &self.local_relative_path
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CatalogPage {
    items: Vec<CatalogItemRecord>,
    total_record_count: u64,
    start_index: u64,
}

impl CatalogPage {
    #[must_use]
    pub fn items(&self) -> &[CatalogItemRecord] {
        &self.items
    }

    #[must_use]
    pub const fn total_record_count(&self) -> u64 {
        self.total_record_count
    }

    #[must_use]
    pub const fn start_index(&self) -> u64 {
        self.start_index
    }
}

pub struct CatalogQueryRepository<'connection> {
    database: &'connection DatabaseConnection,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LazyCatalogWorkTarget {
    item_type: CatalogItemType,
    metadata_revision: i64,
    metadata_resolved_revision: i64,
    metadata_resolved_requirement: Option<MetadataRequirement>,
    structure_revision: i64,
    source_revision: i64,
    has_current_structure: bool,
    has_current_sources: bool,
    storage_scope: Option<LazyStorageScope>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LazyStorageScope {
    storage_root_id: StorageRootId,
    storage_object_id: StorageObjectRecordId,
    children_indexed: bool,
    children_revision: i64,
    reconciled_revision: i64,
    facts_reconciled: bool,
}

impl LazyStorageScope {
    #[must_use]
    pub const fn storage_root_id(self) -> StorageRootId {
        self.storage_root_id
    }

    #[must_use]
    pub const fn storage_object_id(self) -> StorageObjectRecordId {
        self.storage_object_id
    }

    #[must_use]
    pub const fn children_revision(self) -> i64 {
        self.children_revision
    }

    #[must_use]
    pub const fn reconciled_revision(self) -> i64 {
        self.reconciled_revision
    }

    #[must_use]
    pub const fn metadata_input_revision(self) -> i64 {
        if self.children_indexed {
            self.children_revision
        } else {
            self.reconciled_revision
        }
    }

    #[must_use]
    pub const fn is_ready(self) -> bool {
        self.children_indexed
            && self.reconciled_revision >= self.children_revision
            && self.facts_reconciled
    }

    #[must_use]
    pub const fn is_ready_for_direct_source(self) -> bool {
        self.reconciled_revision >= self.metadata_input_revision() && self.facts_reconciled
    }
}

impl LazyCatalogWorkTarget {
    #[must_use]
    pub const fn item_type(self) -> CatalogItemType {
        self.item_type
    }

    #[must_use]
    pub const fn metadata_revision(self) -> i64 {
        self.metadata_revision
    }

    #[must_use]
    pub const fn needs_metadata_resolution(self, requirement: MetadataRequirement) -> bool {
        self.metadata_resolved_revision < self.metadata_revision
            || match self.metadata_resolved_requirement {
                Some(current) => current.as_i32() < requirement.as_i32(),
                None => true,
            }
    }

    #[must_use]
    pub const fn structure_revision(self) -> i64 {
        self.structure_revision
    }

    #[must_use]
    pub const fn source_revision(self) -> i64 {
        self.source_revision
    }

    #[must_use]
    pub const fn has_current_structure(self) -> bool {
        self.has_current_structure
    }

    #[must_use]
    pub const fn has_current_sources(self) -> bool {
        self.has_current_sources
    }

    #[must_use]
    pub const fn storage_scope(self) -> Option<LazyStorageScope> {
        self.storage_scope
    }
}

impl<'connection> CatalogQueryRepository<'connection> {
    #[must_use]
    pub const fn new(database: &'connection DatabaseConnection) -> Self {
        Self { database }
    }

    /// Returns complete genre and production-year choices for one visible library.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogQueryError`] when either distinct query fails.
    pub async fn filter_facets(
        &self,
        user_id: UserId,
        library_id: Uuid,
    ) -> Result<CatalogFilterFacets, CatalogQueryError> {
        const FILTER_TYPES: [&str; 4] = ["Movie", "Series", "Episode", "Audio"];
        let ci = Alias::new("ci");
        let mut year_query = home_item_query(user_id, &FILTER_TYPES, Some(library_id));
        year_query
            .expr_as(
                Expr::col((ci.clone(), Alias::new("production_year"))),
                Alias::new("production_year"),
            )
            .and_where(Expr::col((ci.clone(), Alias::new("production_year"))).is_not_null())
            .distinct()
            .order_by((ci.clone(), Alias::new("production_year")), Order::Desc);

        let link = Alias::new("facet_item_genre");
        let genre = Alias::new("facet_genre");
        let mut genre_query = home_item_query(user_id, &FILTER_TYPES, Some(library_id));
        genre_query
            .expr_as(
                Expr::col((genre.clone(), Alias::new("name"))),
                Alias::new("genre"),
            )
            .join_as(
                JoinType::InnerJoin,
                Alias::new("item_genres"),
                link.clone(),
                Expr::col((link.clone(), Alias::new("catalog_item_id")))
                    .equals((ci, Alias::new("id"))),
            )
            .join_as(
                JoinType::InnerJoin,
                Alias::new("genres"),
                genre.clone(),
                Expr::col((genre.clone(), Alias::new("id"))).equals((link, Alias::new("genre_id"))),
            )
            .distinct()
            .order_by((genre, Alias::new("name")), Order::Asc);

        let backend = self.database.get_database_backend();
        let production_years = self
            .database
            .query_all(backend.build(&year_query))
            .await?
            .iter()
            .map(|row| row.try_get("", "production_year"))
            .collect::<Result<Vec<_>, _>>()?;
        let genres = self
            .database
            .query_all(backend.build(&genre_query))
            .await?
            .iter()
            .map(|row| row.try_get("", "genre"))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(CatalogFilterFacets {
            genres,
            production_years,
        })
    }

    /// Returns every enabled library in database-independent order.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogQueryError`] when SQL execution or row decoding fails.
    pub async fn user_views(&self) -> Result<Vec<LibraryViewRecord>, CatalogQueryError> {
        let query = Query::select()
            .columns([
                Alias::new("id"),
                Alias::new("name"),
                Alias::new("collection_type"),
            ])
            .from(Alias::new("libraries"))
            .and_where(Expr::col(Alias::new("is_enabled")).eq(true))
            .order_by(Alias::new("sort_key"), Order::Asc)
            .order_by(Alias::new("id"), Order::Asc)
            .to_owned();
        let backend = self.database.get_database_backend();
        self.database
            .query_all(backend.build(&query))
            .await?
            .iter()
            .map(library_from_row)
            .collect()
    }

    /// Reads the SQL revisions that isolate user-scoped cache keys.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogQueryError`] for SQL or row-decoding failures.
    pub async fn cache_revisions(
        &self,
        user_id: UserId,
    ) -> Result<CacheRevisions, CatalogQueryError> {
        let catalog = Alias::new("cache_catalog_state");
        let user = Alias::new("cache_user_state");
        let query = Query::select()
            .expr_as(
                Expr::col((catalog.clone(), Alias::new("generation"))),
                Alias::new("catalog_generation"),
            )
            .expr_as(
                Expr::col((user.clone(), Alias::new("revision"))).if_null(0_i64),
                Alias::new("user_revision"),
            )
            .from_as(Alias::new("catalog_state"), catalog.clone())
            .join_as(
                JoinType::LeftJoin,
                Alias::new("user_catalog_state"),
                user,
                Expr::col((Alias::new("cache_user_state"), Alias::new("user_id")))
                    .eq(user_id.as_uuid()),
            )
            .and_where(Expr::col((catalog, Alias::new("id"))).eq(1_i32))
            .limit(1)
            .to_owned();
        let backend = self.database.get_database_backend();
        let row = self
            .database
            .query_one(backend.build(&query))
            .await?
            .ok_or(CatalogQueryError::MissingCatalogState)?;
        Ok(CacheRevisions {
            catalog_generation: row.try_get("", "catalog_generation")?,
            user_revision: row.try_get("", "user_revision")?,
        })
    }

    /// Resolves a Jellyfin parent UUID without exposing disabled libraries or
    /// catalog items outside an enabled library.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogQueryError::AmbiguousParent`] if corrupt data uses the
    /// same UUID for both namespaces, or a database error if resolution fails.
    pub async fn resolve_parent(
        &self,
        id: Uuid,
    ) -> Result<Option<BrowseParent>, CatalogQueryError> {
        let backend = self.database.get_database_backend();
        let library = Query::select()
            .expr(Expr::val(1_i32))
            .from(Alias::new("libraries"))
            .and_where(Expr::col(Alias::new("id")).eq(id))
            .and_where(Expr::col(Alias::new("is_enabled")).eq(true))
            .limit(1)
            .to_owned();
        let is_library = self
            .database
            .query_one(backend.build(&library))
            .await?
            .is_some();

        let item = Alias::new("item");
        let membership = Alias::new("membership");
        let library = Alias::new("library");
        let visible_item = Query::select()
            .expr(Expr::val(1_i32))
            .from_as(Alias::new("catalog_items"), item.clone())
            .join_as(
                JoinType::InnerJoin,
                Alias::new("library_catalog_items"),
                membership.clone(),
                Expr::col((membership.clone(), Alias::new("catalog_item_id")))
                    .equals((item.clone(), Alias::new("id"))),
            )
            .join_as(
                JoinType::InnerJoin,
                Alias::new("libraries"),
                library.clone(),
                Expr::col((library.clone(), Alias::new("id")))
                    .equals((membership, Alias::new("library_id"))),
            )
            .and_where(Expr::col((item.clone(), Alias::new("id"))).eq(id))
            .and_where(Expr::col((item.clone(), Alias::new("is_present"))).eq(true))
            .and_where(Expr::col((item, Alias::new("classification_state"))).eq("Matched"))
            .and_where(Expr::col((library, Alias::new("is_enabled"))).eq(true))
            .limit(1)
            .to_owned();
        let explicitly_visible = self
            .database
            .query_one(backend.build(&visible_item))
            .await?
            .is_some();
        let projected_publication = active_structure_publication(self.database, id).await?;
        let is_item = explicitly_visible || projected_publication.is_some();

        match (is_library, is_item) {
            (true, true) => Err(CatalogQueryError::AmbiguousParent(id)),
            (true, false) => Ok(Some(BrowseParent::Library(id))),
            (false, true) => Ok(Some(BrowseParent::Item(CatalogItemId::from_uuid(id)))),
            (false, false) => Ok(None),
        }
    }

    /// Returns a membership-filtered, stable page and its pre-pagination count.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogQueryError`] when SQL execution or row decoding fails.
    pub async fn items(
        &self,
        user_id: UserId,
        parent: BrowseParent,
        page: CatalogPageRequest,
    ) -> Result<CatalogPage, CatalogQueryError> {
        self.query_items(
            user_id,
            CatalogItemsQuery::new(CatalogItemsScope::Parent(parent), page),
        )
        .await
    }

    /// Returns a stable page for one complete catalog query.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogQueryError`] when SQL execution or row decoding fails.
    pub async fn query_items(
        &self,
        user_id: UserId,
        query: CatalogItemsQuery,
    ) -> Result<CatalogPage, CatalogQueryError> {
        const DEFAULT_ITEM_TYPES: [CatalogItemType; 6] = [
            CatalogItemType::Movie,
            CatalogItemType::Audio,
            CatalogItemType::Series,
            CatalogItemType::Season,
            CatalogItemType::Episode,
            CatalogItemType::Folder,
        ];
        let requested_types = if query.page.item_types.is_empty() {
            &DEFAULT_ITEM_TYPES[..]
        } else {
            &query.page.item_types
        };
        let item_type_names = requested_types
            .iter()
            .map(|item_type| item_type.as_database_value())
            .collect::<Vec<_>>();

        let recursive = query.recursive
            || (query.recursive_for_library
                && matches!(
                    query.scope,
                    CatalogItemsScope::Parent(BrowseParent::Library(_))
                ));
        let (source, mut statement, natural_index_order, recursive_scope) = match query.scope {
            CatalogItemsScope::AllVisible => (
                ItemQuerySource::Catalog,
                home_item_query(user_id, &item_type_names, None),
                false,
                None,
            ),
            CatalogItemsScope::Parent(BrowseParent::Library(library_id)) if recursive => (
                ItemQuerySource::Catalog,
                home_item_query(user_id, &item_type_names, Some(library_id)),
                false,
                None,
            ),
            CatalogItemsScope::Parent(parent) => {
                let source = match parent {
                    BrowseParent::Library(_) => ItemQuerySource::Catalog,
                    BrowseParent::Item(parent_id) => {
                        active_structure_publication(self.database, parent_id.as_uuid())
                            .await?
                            .map_or(ItemQuerySource::Catalog, ItemQuerySource::Publication)
                    }
                };
                let recursive_scope = recursive.then(|| recursive_descendants(source, parent));
                (
                    source,
                    item_query(
                        user_id,
                        parent,
                        &query.page.item_types,
                        source,
                        recursive_scope.is_some(),
                    ),
                    matches!(source, ItemQuerySource::Catalog)
                        && matches!(parent, BrowseParent::Item(_)),
                    recursive_scope,
                )
            }
        };
        if let Some(search_term) = query.search_term.as_deref() {
            apply_search_term(&mut statement, source, search_term);
        }
        apply_catalog_filters(&mut statement, source, &query);
        self.execute_item_page(
            statement,
            source,
            &query,
            natural_index_order,
            recursive_scope,
        )
        .await
    }

    async fn execute_item_page(
        &self,
        statement: SelectStatement,
        source: ItemQuerySource,
        query: &CatalogItemsQuery,
        natural_index_order: bool,
        recursive_scope: Option<WithClause>,
    ) -> Result<CatalogPage, CatalogQueryError> {
        let page = &query.page;
        let mut count = statement.clone();
        count.expr_as(source.id_expr().count(), Alias::new("count"));
        let backend = self.database.get_database_backend();
        let count_statement = if let Some(scope) = recursive_scope.clone() {
            backend.build(&count.with(scope))
        } else {
            backend.build(&count)
        };
        let total: i64 = self
            .database
            .query_one(count_statement)
            .await?
            .ok_or(CatalogQueryError::MissingCount)?
            .try_get("", "count")?;
        let total_record_count =
            u64::try_from(total).map_err(|_| CatalogQueryError::InvalidCount)?;

        let mut data = statement;
        select_item_columns(&mut data, source);
        apply_item_sort(&mut data, source, query.sorts(), natural_index_order);
        data.offset(page.start_index).limit(page.limit);
        let data_statement = if let Some(scope) = recursive_scope {
            backend.build(&data.with(scope))
        } else {
            backend.build(&data)
        };
        let mut items = self
            .database
            .query_all(data_statement)
            .await?
            .iter()
            .map(item_from_row)
            .collect::<Result<Vec<_>, _>>()?;
        attach_image_tags(self.database, &mut items).await?;
        Ok(CatalogPage {
            items,
            total_record_count,
            start_index: page.start_index,
        })
    }

    /// Returns a stable page of visible catalog items matching a name fragment.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogQueryError`] when SQL execution or row decoding fails.
    pub async fn search_hints(
        &self,
        user_id: UserId,
        search_term: &str,
        page: CatalogPageRequest,
    ) -> Result<CatalogPage, CatalogQueryError> {
        self.query_items(
            user_id,
            CatalogItemsQuery::new(CatalogItemsScope::AllVisible, page)
                .with_search_term(Some(search_term.to_owned()))
                .with_sorts(vec![CatalogSort::new(
                    CatalogSortField::SortName,
                    CatalogSortOrder::Ascending,
                )]),
        )
        .await
    }

    /// Returns visible, unfinished items with a saved playback position.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogQueryError`] when SQL execution or row decoding fails.
    pub async fn resume_items(
        &self,
        user_id: UserId,
        page: CatalogPageRequest,
    ) -> Result<CatalogPage, CatalogQueryError> {
        let ci = Alias::new("ci");
        let ud = Alias::new("ud");
        let base = || {
            let mut query = Query::select();
            query
                .from_as(Alias::new("catalog_items"), ci.clone())
                .join_as(
                    JoinType::InnerJoin,
                    Alias::new("user_data"),
                    ud.clone(),
                    Condition::all()
                        .add(
                            Expr::col((ud.clone(), Alias::new("catalog_item_id")))
                                .equals((ci.clone(), Alias::new("id"))),
                        )
                        .add(Expr::col((ud.clone(), Alias::new("user_id"))).eq(user_id.as_uuid())),
                )
                .and_where(
                    Expr::col((ci.clone(), Alias::new("item_type"))).is_in(["Movie", "Episode"]),
                )
                .and_where(Expr::col((ci.clone(), Alias::new("is_present"))).eq(true))
                .and_where(
                    Expr::col((ci.clone(), Alias::new("classification_state"))).eq("Matched"),
                )
                .and_where(Expr::col((ud.clone(), Alias::new("playback_position_ticks"))).gt(0))
                .and_where(Expr::col((ud.clone(), Alias::new("is_played"))).eq(false))
                .cond_where(
                    Condition::any()
                        .add(Expr::exists(enabled_membership_for_item(&ci)))
                        .add(Expr::exists(projected_enabled_membership(&ci))),
                );
            query
        };
        let mut count = base();
        count.expr_as(
            Expr::col((ci.clone(), Alias::new("id"))).count(),
            Alias::new("count"),
        );
        let backend = self.database.get_database_backend();
        let total: i64 = self
            .database
            .query_one(backend.build(&count))
            .await?
            .ok_or(CatalogQueryError::MissingCount)?
            .try_get("", "count")?;
        let total_record_count =
            u64::try_from(total).map_err(|_| CatalogQueryError::InvalidCount)?;

        let mut query = base();
        select_item_columns(&mut query, ItemQuerySource::Catalog);
        query
            .order_by_expr(
                Expr::cust("CASE WHEN ud.last_played_at IS NULL THEN 1 ELSE 0 END"),
                Order::Asc,
            )
            .order_by((ud, Alias::new("last_played_at")), Order::Desc)
            .order_by((ci, Alias::new("id")), Order::Asc)
            .offset(page.start_index)
            .limit(page.limit);
        let mut items = self
            .database
            .query_all(backend.build(&query))
            .await?
            .iter()
            .map(item_from_row)
            .collect::<Result<Vec<_>, _>>()?;
        attach_image_tags(self.database, &mut items).await?;
        Ok(CatalogPage {
            items,
            total_record_count,
            start_index: page.start_index,
        })
    }

    /// Returns the newest visible media, optionally scoped to one enabled library.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogQueryError`] when SQL execution or row decoding fails.
    pub async fn latest_items(
        &self,
        user_id: UserId,
        library_id: Option<Uuid>,
        item_types: &[CatalogItemType],
        limit: u64,
    ) -> Result<Vec<CatalogItemRecord>, CatalogQueryError> {
        if limit == 0 || limit > MAX_PAGE_SIZE {
            return Err(CatalogQueryError::InvalidPage);
        }
        let ci = Alias::new("ci");
        let default_types = [
            CatalogItemType::Movie,
            CatalogItemType::Audio,
            CatalogItemType::Series,
            CatalogItemType::Episode,
        ];
        let item_types = if item_types.is_empty() {
            default_types.as_slice()
        } else {
            item_types
        };
        let database_types = item_types
            .iter()
            .map(|item_type| item_type.as_database_value())
            .collect::<Vec<_>>();
        let mut query = home_item_query(user_id, &database_types, library_id);
        select_item_columns(&mut query, ItemQuerySource::Catalog);
        query
            .order_by((ci.clone(), Alias::new("date_created")), Order::Desc)
            .order_by((ci, Alias::new("id")), Order::Asc)
            .limit(limit);
        let backend = self.database.get_database_backend();
        let mut items = self
            .database
            .query_all(backend.build(&query))
            .await?
            .iter()
            .map(item_from_row)
            .collect::<Result<Vec<_>, _>>()?;
        attach_image_tags(self.database, &mut items).await?;
        Ok(items)
    }

    /// Returns at most one earliest unplayed episode from each visible Series.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogQueryError`] when SQL execution or row decoding fails.
    pub async fn next_up_items(
        &self,
        user_id: UserId,
        series_id: Option<CatalogItemId>,
        include_resumable: bool,
        page: CatalogPageRequest,
    ) -> Result<CatalogPage, CatalogQueryError> {
        let ci = Alias::new("ci");
        let ud = Alias::new("ud");
        let base = || next_up_query(user_id, series_id, include_resumable);
        let mut count = base();
        count.expr_as(
            Expr::col((ci.clone(), Alias::new("id"))).count(),
            Alias::new("count"),
        );
        let backend = self.database.get_database_backend();
        let total: i64 = self
            .database
            .query_one(backend.build(&count))
            .await?
            .ok_or(CatalogQueryError::MissingCount)?
            .try_get("", "count")?;
        let total_record_count =
            u64::try_from(total).map_err(|_| CatalogQueryError::InvalidCount)?;

        let mut query = base();
        select_item_columns(&mut query, ItemQuerySource::Catalog);
        query
            .order_by_expr(
                Expr::cust("CASE WHEN ud.last_played_at IS NULL THEN 1 ELSE 0 END"),
                Order::Asc,
            )
            .order_by((ud, Alias::new("last_played_at")), Order::Desc)
            .order_by(
                (ci.clone(), Alias::new("structure_owner_item_id")),
                Order::Asc,
            )
            .order_by((ci, Alias::new("id")), Order::Asc)
            .offset(page.start_index)
            .limit(page.limit);
        let mut items = self
            .database
            .query_all(backend.build(&query))
            .await?
            .iter()
            .map(item_from_row)
            .collect::<Result<Vec<_>, _>>()?;
        attach_image_tags(self.database, &mut items).await?;
        Ok(CatalogPage {
            items,
            total_record_count,
            start_index: page.start_index,
        })
    }

    /// Returns one published item only when it is visible through an enabled library.
    ///
    /// Active Structure projection metadata takes precedence over the canonical
    /// identity row. Staging and retired publications are never read.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogQueryError`] when SQL execution or row decoding fails.
    pub async fn item(
        &self,
        user_id: UserId,
        item_id: CatalogItemId,
    ) -> Result<Option<CatalogItemRecord>, CatalogQueryError> {
        let active = active_structure_publication(self.database, item_id.as_uuid()).await?;
        if let Some(publication_id) = active
            && let Some(item) = self
                .projected_item(user_id, item_id, publication_id)
                .await?
        {
            return Ok(Some(item));
        }
        self.canonical_item(user_id, item_id).await
    }

    /// Returns one visible item with normalized rich metadata and bounded credits.
    ///
    /// Provider snapshots are deliberately excluded from this read model.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogQueryError`] when SQL execution or stored-value decoding fails.
    pub async fn item_detail(
        &self,
        user_id: UserId,
        item_id: CatalogItemId,
    ) -> Result<Option<CatalogItemDetailRecord>, CatalogQueryError> {
        let Some(item) = self.item(user_id, item_id).await? else {
            return Ok(None);
        };
        let Some(facts) = rich_item_facts(self.database, item_id).await? else {
            return Ok(None);
        };
        let genres =
            named_associations(self.database, item_id, "item_genres", "genre_id", "genres").await?;
        let studios = named_associations(
            self.database,
            item_id,
            "item_studios",
            "studio_id",
            "studios",
        )
        .await?;
        let countries = coded_associations(
            self.database,
            item_id,
            "item_countries",
            "country_id",
            "countries",
        )
        .await?;
        let languages = coded_associations(
            self.database,
            item_id,
            "item_languages",
            "language_id",
            "languages",
        )
        .await?;
        let credits = item_credits(self.database, item_id).await?;
        let provider_ids = item_provider_ids(self.database, item_id).await?;
        Ok(Some(CatalogItemDetailRecord {
            item,
            tagline: facts.tagline,
            vote_count: facts.vote_count,
            runtime_ticks: facts.runtime_ticks,
            premiere_date: facts.premiere_date,
            end_date: facts.end_date,
            release_status: facts.release_status,
            official_rating: facts.official_rating,
            original_language: facts.original_language,
            genres,
            studios,
            countries,
            languages,
            credits,
            provider_ids,
            has_media_sources: facts.has_media_sources,
        }))
    }

    /// Returns the visible item's durable revisions used to join lazy work.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogQueryError`] for database or stored-value corruption.
    pub async fn lazy_work_target(
        &self,
        user_id: UserId,
        item_id: CatalogItemId,
    ) -> Result<Option<LazyCatalogWorkTarget>, CatalogQueryError> {
        self.lazy_work_target_with_root(user_id, item_id, None)
            .await
    }

    /// Returns one visible lazy-work target restricted to a selected storage root.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogQueryError`] for database, ambiguity, or stored-value corruption.
    pub async fn lazy_work_target_in_storage_root(
        &self,
        user_id: UserId,
        item_id: CatalogItemId,
        storage_root: StorageRootId,
    ) -> Result<Option<LazyCatalogWorkTarget>, CatalogQueryError> {
        self.lazy_work_target_with_root(user_id, item_id, Some(storage_root))
            .await
    }

    async fn lazy_work_target_with_root(
        &self,
        _user_id: UserId,
        item_id: CatalogItemId,
        storage_root: Option<StorageRootId>,
    ) -> Result<Option<LazyCatalogWorkTarget>, CatalogQueryError> {
        let item = Alias::new("lazy_work_item");
        let query = Query::select()
            .columns([
                (item.clone(), Alias::new("item_type")),
                (item.clone(), Alias::new("metadata_revision")),
                (item.clone(), Alias::new("metadata_resolved_revision")),
                (item.clone(), Alias::new("metadata_resolved_requirement")),
                (item.clone(), Alias::new("structure_expansion_revision")),
                (item.clone(), Alias::new("source_index_revision")),
            ])
            .expr_as(
                Expr::exists(current_publication_at_revision(
                    &item,
                    "active_structure_publication_id",
                    "structure_expansion_revision",
                    "Structure",
                )),
                Alias::new("has_current_structure"),
            )
            .expr_as(
                Expr::exists(current_effective_source_publication(&item)),
                Alias::new("has_current_sources"),
            )
            .from_as(Alias::new("catalog_items"), item.clone())
            .and_where(Expr::col((item.clone(), Alias::new("id"))).eq(item_id.as_uuid()))
            .and_where(Expr::col((item.clone(), Alias::new("is_present"))).eq(true))
            .and_where(Expr::col((item.clone(), Alias::new("classification_state"))).eq("Matched"))
            .cond_where(
                Condition::any()
                    .add(Expr::exists(enabled_membership_for_item(&item)))
                    .add(Expr::exists(projected_enabled_membership(&item))),
            )
            .limit(1)
            .to_owned();
        let backend = self.database.get_database_backend();
        let target = self
            .database
            .query_one(backend.build(&query))
            .await?
            .map(|row| {
                Ok::<LazyCatalogWorkTarget, CatalogQueryError>(LazyCatalogWorkTarget {
                    item_type: CatalogItemType::from_database_value(
                        &row.try_get::<String>("", "item_type")?,
                    )?,
                    metadata_revision: row.try_get("", "metadata_revision")?,
                    metadata_resolved_revision: row.try_get("", "metadata_resolved_revision")?,
                    metadata_resolved_requirement: row
                        .try_get::<Option<i32>>("", "metadata_resolved_requirement")?
                        .map(MetadataRequirement::from_database)
                        .transpose()
                        .map_err(|_| CatalogQueryError::InvalidMetadataRequirement)?,
                    structure_revision: row.try_get("", "structure_expansion_revision")?,
                    source_revision: row.try_get("", "source_index_revision")?,
                    has_current_structure: row.try_get("", "has_current_structure")?,
                    has_current_sources: row.try_get("", "has_current_sources")?,
                    storage_scope: None,
                })
            })
            .transpose()?;
        let Some(mut target) = target else {
            return Ok(None);
        };
        target.storage_scope = self
            .lazy_storage_scope(
                item_id,
                storage_root,
                target.item_type != CatalogItemType::Audio,
            )
            .await?;
        Ok(Some(target))
    }

    async fn lazy_storage_scope(
        &self,
        item_id: CatalogItemId,
        storage_root: Option<StorageRootId>,
        include_direct_children: bool,
    ) -> Result<Option<LazyStorageScope>, CatalogQueryError> {
        let scope = crate::catalog_storage_scope::resolve_catalog_storage_scope(
            self.database,
            item_id,
            storage_root,
        )
        .await
        .map_err(|error| match error {
            crate::catalog_storage_scope::CatalogStorageScopeError::Ambiguous => {
                CatalogQueryError::AmbiguousStorageScope(item_id.as_uuid())
            }
            crate::catalog_storage_scope::CatalogStorageScopeError::Database(error) => {
                CatalogQueryError::Database(error)
            }
        })?;
        let facts_reconciled = match scope {
            Some(scope) => {
                crate::catalog_storage_scope::storage_scope_is_reconciled(
                    self.database,
                    scope,
                    include_direct_children,
                )
                .await?
            }
            None => false,
        };
        Ok(scope.map(|scope| LazyStorageScope {
            storage_root_id: scope.storage_root_id(),
            storage_object_id: scope.storage_object_id(),
            children_indexed: scope.children_indexed(),
            children_revision: scope.children_revision(),
            reconciled_revision: scope.reconciled_revision(),
            facts_reconciled,
        }))
    }

    async fn projected_item(
        &self,
        user_id: UserId,
        item_id: CatalogItemId,
        publication_id: PublicationId,
    ) -> Result<Option<CatalogItemRecord>, CatalogQueryError> {
        let pci = Alias::new("pci");
        let ud = Alias::new("ud");
        let mut query = Query::select();
        query
            .from_as(Alias::new("publication_catalog_items"), pci.clone())
            .join_as(
                JoinType::LeftJoin,
                Alias::new("user_data"),
                ud.clone(),
                Condition::all()
                    .add(
                        Expr::col((ud.clone(), Alias::new("catalog_item_id")))
                            .equals((pci.clone(), Alias::new("catalog_item_id"))),
                    )
                    .add(Expr::col((ud, Alias::new("user_id"))).eq(user_id.as_uuid())),
            )
            .and_where(
                Expr::col((pci.clone(), Alias::new("publication_id"))).eq(publication_id.as_uuid()),
            )
            .and_where(Expr::col((pci, Alias::new("catalog_item_id"))).eq(item_id.as_uuid()))
            .and_where(Expr::exists(current_structure_publication_visible(
                publication_id,
            )))
            .limit(1);
        select_item_columns(&mut query, ItemQuerySource::Publication(publication_id));
        self.item_from_query(query).await
    }

    async fn canonical_item(
        &self,
        user_id: UserId,
        item_id: CatalogItemId,
    ) -> Result<Option<CatalogItemRecord>, CatalogQueryError> {
        let ci = Alias::new("ci");
        let ud = Alias::new("ud");
        let mut query = Query::select();
        query
            .from_as(Alias::new("catalog_items"), ci.clone())
            .join_as(
                JoinType::LeftJoin,
                Alias::new("user_data"),
                ud.clone(),
                Condition::all()
                    .add(
                        Expr::col((ud.clone(), Alias::new("catalog_item_id")))
                            .equals((ci.clone(), Alias::new("id"))),
                    )
                    .add(Expr::col((ud, Alias::new("user_id"))).eq(user_id.as_uuid())),
            )
            .and_where(Expr::col((ci.clone(), Alias::new("id"))).eq(item_id.as_uuid()))
            .and_where(Expr::col((ci.clone(), Alias::new("is_present"))).eq(true))
            .and_where(Expr::col((ci.clone(), Alias::new("classification_state"))).eq("Matched"))
            .cond_where(
                Condition::any()
                    .add(Expr::exists(enabled_membership_for_item(&ci)))
                    .add(Expr::exists(projected_enabled_membership(&ci))),
            )
            .limit(1);
        select_item_columns(&mut query, ItemQuerySource::Catalog);
        self.item_from_query(query).await
    }

    async fn item_from_query(
        &self,
        query: SelectStatement,
    ) -> Result<Option<CatalogItemRecord>, CatalogQueryError> {
        let backend = self.database.get_database_backend();
        let Some(row) = self.database.query_one(backend.build(&query)).await? else {
            return Ok(None);
        };
        let mut items = vec![item_from_row(&row)?];
        attach_image_tags(self.database, &mut items).await?;
        Ok(items.pop())
    }

    /// Resolves an image only when its item is visible through an enabled library.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogQueryError`] when SQL execution or row decoding fails.
    pub async fn image(
        &self,
        item_id: CatalogItemId,
        image_type: ImageType,
        priority: u32,
    ) -> Result<Option<AssetRecord>, CatalogQueryError> {
        let priority = i32::try_from(priority).map_err(|_| CatalogQueryError::InvalidPriority)?;
        let item = Alias::new("item");
        let asset = Alias::new("asset");
        let blob = Alias::new("blob");
        let enabled_membership = enabled_membership_for_item(&item);
        let projected_membership = projected_enabled_membership(&item);

        let mut query = Query::select();
        query
            .from_as(Alias::new("catalog_items"), item.clone())
            .join_as(
                JoinType::InnerJoin,
                Alias::new("item_assets"),
                asset.clone(),
                Expr::col((asset.clone(), Alias::new("item_id")))
                    .equals((item.clone(), Alias::new("id"))),
            )
            .join_as(
                JoinType::InnerJoin,
                Alias::new("asset_blobs"),
                blob.clone(),
                Expr::col((blob.clone(), Alias::new("id")))
                    .equals((asset.clone(), Alias::new("asset_blob_id"))),
            )
            .and_where(Expr::col((item.clone(), Alias::new("id"))).eq(item_id.as_uuid()))
            .and_where(Expr::col((item.clone(), Alias::new("is_present"))).eq(true))
            .and_where(Expr::col((item, Alias::new("classification_state"))).eq("Matched"))
            .and_where(Expr::col((asset.clone(), Alias::new("image_type"))).eq(image_type.as_str()))
            .and_where(Expr::col((asset, Alias::new("priority"))).eq(priority))
            .cond_where(
                Condition::any()
                    .add(Expr::exists(enabled_membership))
                    .add(Expr::exists(projected_membership)),
            )
            .limit(1);
        for column in [
            "sha256",
            "mime_type",
            "width",
            "height",
            "byte_size",
            "local_relative_path",
        ] {
            query.expr_as(
                Expr::col((blob.clone(), Alias::new(column))),
                Alias::new(column),
            );
        }
        let backend = self.database.get_database_backend();
        self.database
            .query_one(backend.build(&query))
            .await?
            .as_ref()
            .map(asset_from_row)
            .transpose()
    }
}

fn apply_catalog_filters(
    statement: &mut SelectStatement,
    source: ItemQuerySource,
    query: &CatalogItemsQuery,
) {
    if let Some(year) = query.production_year() {
        statement.and_where(source.field_expr(CatalogSortField::ProductionYear).eq(year));
    }
    if let Some(genre) = query.genre() {
        let link = Alias::new("filter_item_genre");
        let entity = Alias::new("filter_genre");
        let genre_match = Query::select()
            .expr(Expr::val(1))
            .from_as(Alias::new("item_genres"), link.clone())
            .join_as(
                JoinType::InnerJoin,
                Alias::new("genres"),
                entity.clone(),
                Expr::col((entity.clone(), Alias::new("id")))
                    .equals((link.clone(), Alias::new("genre_id"))),
            )
            .and_where(Expr::col((link, Alias::new("catalog_item_id"))).eq(source.id_expr()))
            .and_where(Expr::col((entity, Alias::new("name"))).eq(genre))
            .to_owned();
        statement.and_where(Expr::exists(genre_match));
    }
}

struct RichItemFacts {
    tagline: Option<String>,
    vote_count: Option<i64>,
    runtime_ticks: Option<i64>,
    premiere_date: Option<DateTime<Utc>>,
    end_date: Option<DateTime<Utc>>,
    release_status: Option<String>,
    official_rating: Option<String>,
    original_language: Option<String>,
    has_media_sources: bool,
}

async fn rich_item_facts(
    database: &DatabaseConnection,
    item_id: CatalogItemId,
) -> Result<Option<RichItemFacts>, CatalogQueryError> {
    let item = Alias::new("detail_item");
    let mut query = Query::select();
    query
        .from_as(Alias::new("catalog_items"), item.clone())
        .and_where(Expr::col((item.clone(), Alias::new("id"))).eq(item_id.as_uuid()))
        .expr_as(
            Expr::exists(current_effective_source_publication(&item)),
            Alias::new("has_media_sources"),
        )
        .limit(1);
    for column in [
        "tagline",
        "vote_count",
        "runtime_ticks",
        "premiere_date",
        "end_date",
        "release_status",
        "official_rating",
        "original_language",
    ] {
        query.expr_as(
            Expr::col((item.clone(), Alias::new(column))),
            Alias::new(column),
        );
    }
    database
        .query_one(database.get_database_backend().build(&query))
        .await?
        .map(|row| {
            Ok(RichItemFacts {
                tagline: row.try_get("", "tagline")?,
                vote_count: row.try_get("", "vote_count")?,
                runtime_ticks: row.try_get("", "runtime_ticks")?,
                premiere_date: row.try_get("", "premiere_date")?,
                end_date: row.try_get("", "end_date")?,
                release_status: row.try_get("", "release_status")?,
                official_rating: row.try_get("", "official_rating")?,
                original_language: row.try_get("", "original_language")?,
                has_media_sources: row.try_get("", "has_media_sources")?,
            })
        })
        .transpose()
}

async fn named_associations(
    database: &DatabaseConnection,
    item_id: CatalogItemId,
    link_table: &str,
    foreign_key: &str,
    entity_table: &str,
) -> Result<Vec<String>, CatalogQueryError> {
    let link = Alias::new("detail_named_link");
    let entity = Alias::new("detail_named_entity");
    let query = Query::select()
        .expr_as(
            Expr::col((entity.clone(), Alias::new("name"))),
            Alias::new("name"),
        )
        .from_as(Alias::new(link_table), link.clone())
        .join_as(
            JoinType::InnerJoin,
            Alias::new(entity_table),
            entity.clone(),
            Expr::col((entity.clone(), Alias::new("id")))
                .equals((link.clone(), Alias::new(foreign_key))),
        )
        .and_where(Expr::col((link, Alias::new("catalog_item_id"))).eq(item_id.as_uuid()))
        .order_by((entity, Alias::new("name")), Order::Asc)
        .limit(256)
        .to_owned();
    database
        .query_all(database.get_database_backend().build(&query))
        .await?
        .iter()
        .map(|row| row.try_get("", "name").map_err(Into::into))
        .collect()
}

async fn coded_associations(
    database: &DatabaseConnection,
    item_id: CatalogItemId,
    link_table: &str,
    foreign_key: &str,
    entity_table: &str,
) -> Result<Vec<CatalogNamedCodeRecord>, CatalogQueryError> {
    let link = Alias::new("detail_coded_link");
    let entity = Alias::new("detail_coded_entity");
    let query = Query::select()
        .expr_as(
            Expr::col((entity.clone(), Alias::new("code"))),
            Alias::new("code"),
        )
        .expr_as(
            Expr::col((entity.clone(), Alias::new("name"))),
            Alias::new("name"),
        )
        .from_as(Alias::new(link_table), link.clone())
        .join_as(
            JoinType::InnerJoin,
            Alias::new(entity_table),
            entity,
            Expr::col((Alias::new("detail_coded_entity"), Alias::new("id")))
                .equals((link.clone(), Alias::new(foreign_key))),
        )
        .and_where(Expr::col((link.clone(), Alias::new("catalog_item_id"))).eq(item_id.as_uuid()))
        .order_by((link, Alias::new("sort_order")), Order::Asc)
        .limit(64)
        .to_owned();
    database
        .query_all(database.get_database_backend().build(&query))
        .await?
        .iter()
        .map(|row| {
            Ok(CatalogNamedCodeRecord {
                code: row.try_get("", "code")?,
                name: row.try_get("", "name")?,
            })
        })
        .collect()
}

async fn item_credits(
    database: &DatabaseConnection,
    item_id: CatalogItemId,
) -> Result<Vec<CatalogCreditRecord>, CatalogQueryError> {
    let credit = Alias::new("detail_credit");
    let person = Alias::new("detail_person");
    let query = Query::select()
        .expr_as(
            Expr::col((person.clone(), Alias::new("id"))),
            Alias::new("person_id"),
        )
        .expr_as(
            Expr::col((person.clone(), Alias::new("name"))),
            Alias::new("person_name"),
        )
        .expr_as(
            Expr::col((credit.clone(), Alias::new("role"))),
            Alias::new("role"),
        )
        .expr_as(
            Expr::col((credit.clone(), Alias::new("credit_type"))),
            Alias::new("credit_type"),
        )
        .expr_as(
            Expr::col((credit.clone(), Alias::new("sort_order"))),
            Alias::new("sort_order"),
        )
        .from_as(Alias::new("item_people"), credit.clone())
        .join_as(
            JoinType::InnerJoin,
            Alias::new("people"),
            person,
            Expr::col((Alias::new("detail_person"), Alias::new("id")))
                .equals((credit.clone(), Alias::new("person_id"))),
        )
        .and_where(Expr::col((credit.clone(), Alias::new("catalog_item_id"))).eq(item_id.as_uuid()))
        .order_by((credit.clone(), Alias::new("sort_order")), Order::Asc)
        .order_by((credit, Alias::new("id")), Order::Asc)
        .limit(64)
        .to_owned();
    database
        .query_all(database.get_database_backend().build(&query))
        .await?
        .iter()
        .map(|row| {
            Ok(CatalogCreditRecord {
                person_id: row.try_get("", "person_id")?,
                person_name: row.try_get("", "person_name")?,
                role: row.try_get("", "role")?,
                credit_type: row.try_get("", "credit_type")?,
                sort_order: row.try_get("", "sort_order")?,
            })
        })
        .collect()
}

async fn item_provider_ids(
    database: &DatabaseConnection,
    item_id: CatalogItemId,
) -> Result<BTreeMap<String, String>, CatalogQueryError> {
    let query = Query::select()
        .columns([Alias::new("provider"), Alias::new("provider_item_id")])
        .from(Alias::new("provider_ids"))
        .and_where(Expr::col(Alias::new("catalog_item_id")).eq(item_id.as_uuid()))
        .order_by(Alias::new("provider"), Order::Asc)
        .limit(64)
        .to_owned();
    database
        .query_all(database.get_database_backend().build(&query))
        .await?
        .iter()
        .map(|row| {
            Ok((
                row.try_get("", "provider")?,
                row.try_get("", "provider_item_id")?,
            ))
        })
        .collect()
}

#[derive(Debug, Error)]
pub enum CatalogQueryError {
    #[error("page limit must be between 1 and 200")]
    InvalidPage,
    #[error("catalog count row is missing")]
    MissingCount,
    #[error("catalog generation row is missing")]
    MissingCatalogState,
    #[error("catalog count is outside the supported range")]
    InvalidCount,
    #[error("image priority is outside the supported range")]
    InvalidPriority,
    #[error("asset byte size is outside the supported range")]
    InvalidAssetSize,
    #[error("catalog item type is invalid")]
    InvalidItemType,
    #[error("catalog metadata completion requirement is invalid")]
    InvalidMetadataRequirement,
    #[error("parent UUID exists in both library and catalog item namespaces: {0}")]
    AmbiguousParent(Uuid),
    #[error("catalog item appears in more than one active structure publication: {0}")]
    AmbiguousStructurePublication(Uuid),
    #[error("catalog item resolves to more than one authorized storage scope: {0}")]
    AmbiguousStorageScope(Uuid),
    #[error("catalog source publication query failed: {0}")]
    Publication(#[from] crate::CatalogPublicationError),
    #[error("catalog query failed: {0}")]
    Database(#[from] DbErr),
}

#[derive(Clone, Copy)]
enum ItemQuerySource {
    Catalog,
    Publication(PublicationId),
}

impl ItemQuerySource {
    fn id_expr(self) -> Expr {
        match self {
            Self::Catalog => Expr::col((Alias::new("ci"), Alias::new("id"))),
            Self::Publication(_) => Expr::col((Alias::new("pci"), Alias::new("catalog_item_id"))),
        }
    }

    fn sort_expr(self) -> Expr {
        match self {
            Self::Catalog => Expr::col((Alias::new("ci"), Alias::new("sort_key"))),
            Self::Publication(_) => Expr::col((Alias::new("pci"), Alias::new("sort_key"))),
        }
    }

    fn field_expr(self, field: CatalogSortField) -> Expr {
        match (self, field) {
            (Self::Catalog, CatalogSortField::SortName) => {
                Expr::col((Alias::new("ci"), Alias::new("sort_key")))
            }
            (Self::Catalog, CatalogSortField::DateCreated) => {
                Expr::col((Alias::new("ci"), Alias::new("date_created")))
            }
            (Self::Catalog, CatalogSortField::ProductionYear) => {
                Expr::col((Alias::new("ci"), Alias::new("production_year")))
            }
            (Self::Catalog, CatalogSortField::Runtime) => {
                Expr::col((Alias::new("ci"), Alias::new("runtime_ticks")))
            }
            (Self::Publication(_), CatalogSortField::SortName) => {
                Expr::col((Alias::new("pci"), Alias::new("sort_key")))
            }
            (Self::Publication(_), CatalogSortField::ProductionYear) => {
                Expr::col((Alias::new("pci"), Alias::new("production_year")))
            }
            (Self::Publication(_), CatalogSortField::DateCreated) => {
                Expr::col((Alias::new("publication_item"), Alias::new("date_created")))
            }
            (Self::Publication(_), CatalogSortField::Runtime) => {
                Expr::col((Alias::new("publication_item"), Alias::new("runtime_ticks")))
            }
        }
    }
}

fn recursive_descendants(source: ItemQuerySource, parent: BrowseParent) -> WithClause {
    let BrowseParent::Item(parent_id) = parent else {
        unreachable!("recursive library browse uses the all-visible query")
    };
    let descendants = Alias::new("item_descendants");
    let id = Alias::new("id");
    let (mut anchor, recursive) = match source {
        ItemQuerySource::Catalog => {
            let anchor = Alias::new("descendant_anchor");
            let child = Alias::new("descendant_child");
            let mut base = Query::select();
            base.expr_as(Expr::col((anchor.clone(), Alias::new("id"))), id.clone())
                .from_as(Alias::new("catalog_items"), anchor.clone())
                .and_where(
                    Expr::col((anchor.clone(), Alias::new("parent_id"))).eq(parent_id.as_uuid()),
                )
                .and_where(Expr::col((anchor.clone(), Alias::new("is_present"))).eq(true))
                .and_where(Expr::col((anchor, Alias::new("classification_state"))).eq("Matched"))
                .and_where(Expr::exists(shared_enabled_membership(
                    &Alias::new("descendant_anchor"),
                    parent_id,
                )));
            let mut step = Query::select();
            step.expr_as(Expr::col((child.clone(), Alias::new("id"))), id.clone())
                .from_as(Alias::new("catalog_items"), child.clone())
                .join_as(
                    JoinType::InnerJoin,
                    descendants.clone(),
                    Alias::new("ancestor"),
                    Expr::col((child.clone(), Alias::new("parent_id")))
                        .equals((Alias::new("ancestor"), Alias::new("id"))),
                )
                .and_where(Expr::col((child.clone(), Alias::new("is_present"))).eq(true))
                .and_where(Expr::col((child, Alias::new("classification_state"))).eq("Matched"))
                .and_where(Expr::exists(shared_enabled_membership(
                    &Alias::new("descendant_child"),
                    parent_id,
                )));
            (base, step)
        }
        ItemQuerySource::Publication(publication_id) => {
            let anchor = Alias::new("descendant_anchor");
            let child = Alias::new("descendant_child");
            let mut base = Query::select();
            base.expr_as(
                Expr::col((anchor.clone(), Alias::new("catalog_item_id"))),
                id.clone(),
            )
            .from_as(Alias::new("publication_catalog_items"), anchor.clone())
            .and_where(
                Expr::col((anchor.clone(), Alias::new("publication_id")))
                    .eq(publication_id.as_uuid()),
            )
            .and_where(
                Expr::col((anchor, Alias::new("parent_catalog_item_id"))).eq(parent_id.as_uuid()),
            );
            let mut step = Query::select();
            step.expr_as(
                Expr::col((child.clone(), Alias::new("catalog_item_id"))),
                id.clone(),
            )
            .from_as(Alias::new("publication_catalog_items"), child.clone())
            .join_as(
                JoinType::InnerJoin,
                descendants.clone(),
                Alias::new("ancestor"),
                Expr::col((child.clone(), Alias::new("parent_catalog_item_id")))
                    .equals((Alias::new("ancestor"), Alias::new("id"))),
            )
            .and_where(
                Expr::col((child, Alias::new("publication_id"))).eq(publication_id.as_uuid()),
            );
            (base, step)
        }
    };
    let cte = CommonTableExpression::new()
        .table_name(descendants)
        .column(id)
        .query(anchor.union(UnionType::Distinct, recursive).to_owned())
        .to_owned();
    WithClause::new().recursive(true).cte(cte).to_owned()
}

fn descendant_ids() -> SelectStatement {
    Query::select()
        .column(Alias::new("id"))
        .from(Alias::new("item_descendants"))
        .to_owned()
}

fn apply_search_term(query: &mut SelectStatement, source: ItemQuerySource, search_term: &str) {
    let escaped = search_term
        .to_lowercase()
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    let pattern = LikeExpr::new(format!("%{escaped}%")).escape('\\');
    let (name, original_title) = match source {
        ItemQuerySource::Catalog => (
            Expr::col((Alias::new("ci"), Alias::new("name"))),
            Expr::col((Alias::new("ci"), Alias::new("original_title"))),
        ),
        ItemQuerySource::Publication(_) => (
            Expr::col((Alias::new("pci"), Alias::new("name"))),
            Expr::col((Alias::new("publication_item"), Alias::new("original_title"))),
        ),
    };
    query.cond_where(
        Condition::any()
            .add(Expr::expr(Func::lower(name)).like(pattern.clone()))
            .add(Expr::expr(Func::lower(original_title)).like(pattern)),
    );
}

fn apply_item_sort(
    query: &mut SelectStatement,
    source: ItemQuerySource,
    sorts: &[CatalogSort],
    natural_index_order: bool,
) {
    if sorts.is_empty() {
        if natural_index_order {
            query.order_by_expr_with_nulls(
                Expr::col((Alias::new("ci"), Alias::new("index_number"))).into(),
                Order::Asc,
                NullOrdering::Last,
            );
        }
        query.order_by_expr(source.sort_expr().into(), Order::Asc);
    } else {
        for sort in sorts {
            let order = match sort.order() {
                CatalogSortOrder::Ascending => Order::Asc,
                CatalogSortOrder::Descending => Order::Desc,
            };
            query.order_by_expr_with_nulls(
                source.field_expr(sort.field()).into(),
                order,
                NullOrdering::Last,
            );
        }
    }
    query.order_by_expr(source.id_expr().into(), Order::Asc);
}

fn home_item_query(
    user_id: UserId,
    item_types: &[&str],
    library_id: Option<Uuid>,
) -> SelectStatement {
    let ci = Alias::new("ci");
    let ud = Alias::new("ud");
    let mut query = Query::select();
    query
        .from_as(Alias::new("catalog_items"), ci.clone())
        .join_as(
            JoinType::LeftJoin,
            Alias::new("user_data"),
            ud.clone(),
            Condition::all()
                .add(
                    Expr::col((ud.clone(), Alias::new("catalog_item_id")))
                        .equals((ci.clone(), Alias::new("id"))),
                )
                .add(Expr::col((ud, Alias::new("user_id"))).eq(user_id.as_uuid())),
        )
        .and_where(
            Expr::col((ci.clone(), Alias::new("item_type"))).is_in(item_types.iter().copied()),
        )
        .and_where(Expr::col((ci.clone(), Alias::new("is_present"))).eq(true))
        .and_where(Expr::col((ci.clone(), Alias::new("classification_state"))).eq("Matched"))
        .cond_where(
            Condition::any()
                .add(Expr::exists(enabled_membership_for_item_in_library(
                    &ci, library_id,
                )))
                .add(Expr::exists(projected_enabled_membership_in_library(
                    &ci, library_id,
                ))),
        );
    query
}

fn next_up_query(
    user_id: UserId,
    series_id: Option<CatalogItemId>,
    include_resumable: bool,
) -> SelectStatement {
    let ci = Alias::new("ci");
    let ud = Alias::new("ud");
    let earlier = Alias::new("earlier_episode");
    let earlier_ud = Alias::new("earlier_user_data");
    let mut earlier_query = Query::select();
    earlier_query
        .expr(Expr::val(1_i32))
        .from_as(Alias::new("catalog_items"), earlier.clone())
        .join_as(
            JoinType::LeftJoin,
            Alias::new("user_data"),
            earlier_ud.clone(),
            Condition::all()
                .add(
                    Expr::col((earlier_ud.clone(), Alias::new("catalog_item_id")))
                        .equals((earlier.clone(), Alias::new("id"))),
                )
                .add(Expr::col((earlier_ud.clone(), Alias::new("user_id"))).eq(user_id.as_uuid())),
        )
        .and_where(Expr::col((earlier.clone(), Alias::new("item_type"))).eq("Episode"))
        .and_where(Expr::col((earlier.clone(), Alias::new("is_present"))).eq(true))
        .and_where(Expr::col((earlier.clone(), Alias::new("classification_state"))).eq("Matched"))
        .and_where(
            Expr::col((earlier.clone(), Alias::new("structure_owner_item_id")))
                .equals((ci.clone(), Alias::new("structure_owner_item_id"))),
        )
        .cond_where(
            Condition::any()
                .add(Expr::exists(enabled_membership_for_item(&earlier)))
                .add(Expr::exists(projected_enabled_membership(&earlier))),
        )
        .cond_where(
            Condition::any()
                .add(Expr::col((earlier_ud.clone(), Alias::new("is_played"))).is_null())
                .add(Expr::col((earlier_ud, Alias::new("is_played"))).eq(false)),
        )
        .cond_where(
            Condition::any()
                .add(
                    Expr::col((earlier.clone(), Alias::new("sort_key")))
                        .lt(Expr::col((ci.clone(), Alias::new("sort_key")))),
                )
                .add(
                    Condition::all()
                        .add(
                            Expr::col((earlier.clone(), Alias::new("sort_key")))
                                .equals((ci.clone(), Alias::new("sort_key"))),
                        )
                        .add(
                            Expr::col((earlier, Alias::new("id")))
                                .lt(Expr::col((ci.clone(), Alias::new("id")))),
                        ),
                ),
        );

    let mut query = home_item_query(user_id, &["Episode"], None);
    query
        .and_where(Expr::col((ci.clone(), Alias::new("structure_owner_item_id"))).is_not_null())
        .cond_where(
            Condition::any()
                .add(Expr::col((ud.clone(), Alias::new("is_played"))).is_null())
                .add(Expr::col((ud, Alias::new("is_played"))).eq(false)),
        )
        .and_where(Expr::exists(earlier_query).not());
    if !include_resumable {
        query.cond_where(
            Condition::any()
                .add(Expr::col((Alias::new("ud"), Alias::new("playback_position_ticks"))).is_null())
                .add(
                    Expr::col((Alias::new("ud"), Alias::new("playback_position_ticks"))).eq(0_i64),
                ),
        );
    }
    if let Some(series_id) = series_id {
        query.and_where(
            Expr::col((ci, Alias::new("structure_owner_item_id"))).eq(series_id.as_uuid()),
        );
    }
    query
}

fn item_query(
    user_id: UserId,
    parent: BrowseParent,
    item_types: &[CatalogItemType],
    source: ItemQuerySource,
    recursive: bool,
) -> SelectStatement {
    let ud = Alias::new("ud");
    let mut query = Query::select();
    match source {
        ItemQuerySource::Catalog => {
            let ci = Alias::new("ci");
            query.from_as(Alias::new("catalog_items"), ci.clone());
            query.join_as(
                JoinType::LeftJoin,
                Alias::new("user_data"),
                ud.clone(),
                Condition::all()
                    .add(
                        Expr::col((ud.clone(), Alias::new("catalog_item_id")))
                            .equals((ci.clone(), Alias::new("id"))),
                    )
                    .add(Expr::col((ud.clone(), Alias::new("user_id"))).eq(user_id.as_uuid())),
            );
            query
                .and_where(Expr::col((ci.clone(), Alias::new("is_present"))).eq(true))
                .and_where(
                    Expr::col((ci.clone(), Alias::new("classification_state"))).eq("Matched"),
                );
            if !item_types.is_empty() {
                query.and_where(
                    Expr::col((ci.clone(), Alias::new("item_type"))).is_in(
                        item_types
                            .iter()
                            .map(|item_type| item_type.as_database_value()),
                    ),
                );
            }
            if recursive {
                let BrowseParent::Item(parent_id) = parent else {
                    unreachable!("recursive library browse uses the all-visible query")
                };
                query
                    .and_where(source.id_expr().in_subquery(descendant_ids()))
                    .and_where(source.id_expr().ne(parent_id.as_uuid()))
                    .and_where(Expr::exists(shared_enabled_membership(&ci, parent_id)))
                    .and_where(Expr::exists(visible_item(parent_id)));
            } else {
                apply_parent(&mut query, &ci, parent);
            }
        }
        ItemQuerySource::Publication(publication_id) => {
            let pci = Alias::new("pci");
            let catalog_item = Alias::new("publication_item");
            let BrowseParent::Item(parent_id) = parent else {
                unreachable!("library browse cannot use a structure publication")
            };
            query.from_as(Alias::new("publication_catalog_items"), pci.clone());
            query.join_as(
                JoinType::InnerJoin,
                Alias::new("catalog_items"),
                catalog_item.clone(),
                Expr::col((catalog_item, Alias::new("id")))
                    .equals((pci.clone(), Alias::new("catalog_item_id"))),
            );
            query.join_as(
                JoinType::LeftJoin,
                Alias::new("user_data"),
                ud.clone(),
                Condition::all()
                    .add(
                        Expr::col((ud.clone(), Alias::new("catalog_item_id")))
                            .equals((pci.clone(), Alias::new("catalog_item_id"))),
                    )
                    .add(Expr::col((ud.clone(), Alias::new("user_id"))).eq(user_id.as_uuid())),
            );
            query.and_where(
                Expr::col((pci.clone(), Alias::new("publication_id"))).eq(publication_id.as_uuid()),
            );
            if recursive {
                query
                    .and_where(source.id_expr().in_subquery(descendant_ids()))
                    .and_where(source.id_expr().ne(parent_id.as_uuid()));
            } else {
                query.and_where(
                    Expr::col((pci.clone(), Alias::new("parent_catalog_item_id")))
                        .eq(parent_id.as_uuid()),
                );
            }
            query.and_where(Expr::exists(current_structure_publication_visible(
                publication_id,
            )));
            if !item_types.is_empty() {
                query.and_where(
                    Expr::col((pci, Alias::new("item_type"))).is_in(
                        item_types
                            .iter()
                            .map(|item_type| item_type.as_database_value()),
                    ),
                );
            }
        }
    }
    query.clone()
}

async fn active_structure_publication(
    database: &DatabaseConnection,
    parent_id: Uuid,
) -> Result<Option<PublicationId>, CatalogQueryError> {
    let publication = Alias::new("active_publication");
    let owner = Alias::new("publication_owner");
    let membership = Alias::new("owner_membership");
    let library = Alias::new("owner_library");
    let member = Alias::new("publication_parent");
    let mut contains_parent = Query::select();
    contains_parent
        .expr(Expr::val(1_i32))
        .from_as(Alias::new("publication_catalog_items"), member.clone())
        .and_where(
            Expr::col((member.clone(), Alias::new("publication_id")))
                .equals((publication.clone(), Alias::new("id"))),
        )
        .and_where(Expr::col((member, Alias::new("catalog_item_id"))).eq(parent_id));
    let query = Query::select()
        .distinct()
        .expr_as(
            Expr::col((publication.clone(), Alias::new("id"))),
            Alias::new("publication_id"),
        )
        .from_as(Alias::new("catalog_publications"), publication.clone())
        .join_as(
            JoinType::InnerJoin,
            Alias::new("catalog_items"),
            owner.clone(),
            Expr::col((owner.clone(), Alias::new("active_structure_publication_id")))
                .equals((publication.clone(), Alias::new("id"))),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("library_catalog_items"),
            membership.clone(),
            Expr::col((membership.clone(), Alias::new("catalog_item_id")))
                .equals((owner.clone(), Alias::new("id"))),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("libraries"),
            library.clone(),
            Expr::col((library.clone(), Alias::new("id")))
                .equals((membership, Alias::new("library_id"))),
        )
        .and_where(Expr::col((publication.clone(), Alias::new("publication_kind"))).eq("Structure"))
        .and_where(Expr::col((publication, Alias::new("state"))).eq("Active"))
        .and_where(Expr::col((owner.clone(), Alias::new("is_present"))).eq(true))
        .and_where(Expr::col((owner.clone(), Alias::new("classification_state"))).eq("Matched"))
        .and_where(Expr::col((library, Alias::new("is_enabled"))).eq(true))
        .cond_where(
            Condition::any()
                .add(Expr::col((owner, Alias::new("id"))).eq(parent_id))
                .add(Expr::exists(contains_parent)),
        )
        .limit(2)
        .to_owned();
    let backend = database.get_database_backend();
    let rows = database.query_all(backend.build(&query)).await?;
    match rows.as_slice() {
        [] => Ok(None),
        [row] => Ok(Some(PublicationId::from_uuid(
            row.try_get("", "publication_id")?,
        ))),
        _ => Err(CatalogQueryError::AmbiguousStructurePublication(parent_id)),
    }
}

fn projected_enabled_membership(item: &Alias) -> SelectStatement {
    projected_enabled_membership_in_library(item, None)
}

fn projected_enabled_membership_in_library(
    item: &Alias,
    library_id: Option<Uuid>,
) -> SelectStatement {
    let projection = Alias::new("image_projection");
    let publication = Alias::new("image_publication");
    let owner = Alias::new("image_publication_owner");
    let membership = Alias::new("image_owner_membership");
    let library = Alias::new("image_owner_library");
    let mut query = Query::select();
    query
        .expr(Expr::val(1_i32))
        .from_as(Alias::new("publication_catalog_items"), projection.clone())
        .join_as(
            JoinType::InnerJoin,
            Alias::new("catalog_publications"),
            publication.clone(),
            Expr::col((publication.clone(), Alias::new("id")))
                .equals((projection.clone(), Alias::new("publication_id"))),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("catalog_items"),
            owner.clone(),
            Expr::col((owner.clone(), Alias::new("active_structure_publication_id")))
                .equals((publication.clone(), Alias::new("id"))),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("library_catalog_items"),
            membership.clone(),
            Expr::col((membership.clone(), Alias::new("catalog_item_id")))
                .equals((owner.clone(), Alias::new("id"))),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("libraries"),
            library.clone(),
            Expr::col((library.clone(), Alias::new("id")))
                .equals((membership, Alias::new("library_id"))),
        )
        .and_where(
            Expr::col((projection, Alias::new("catalog_item_id")))
                .equals((item.clone(), Alias::new("id"))),
        )
        .and_where(Expr::col((publication.clone(), Alias::new("publication_kind"))).eq("Structure"))
        .and_where(Expr::col((publication, Alias::new("state"))).eq("Active"))
        .and_where(Expr::col((owner.clone(), Alias::new("is_present"))).eq(true))
        .and_where(Expr::col((owner, Alias::new("classification_state"))).eq("Matched"))
        .and_where(Expr::col((library.clone(), Alias::new("is_enabled"))).eq(true));
    if let Some(library_id) = library_id {
        query.and_where(Expr::col((library, Alias::new("id"))).eq(library_id));
    }
    query.clone()
}

fn enabled_membership_for_item(item: &Alias) -> SelectStatement {
    enabled_membership_for_item_in_library(item, None)
}

fn enabled_membership_for_item_in_library(
    item: &Alias,
    library_id: Option<Uuid>,
) -> SelectStatement {
    let membership = Alias::new("enabled_item_membership");
    let library = Alias::new("enabled_item_library");
    let mut query = Query::select();
    query
        .expr(Expr::val(1_i32))
        .from_as(Alias::new("library_catalog_items"), membership.clone())
        .join_as(
            JoinType::InnerJoin,
            Alias::new("libraries"),
            library.clone(),
            Expr::col((library.clone(), Alias::new("id")))
                .equals((membership.clone(), Alias::new("library_id"))),
        )
        .and_where(
            Expr::col((membership, Alias::new("catalog_item_id")))
                .equals((item.clone(), Alias::new("id"))),
        )
        .and_where(Expr::col((library.clone(), Alias::new("is_enabled"))).eq(true));
    if let Some(library_id) = library_id {
        query.and_where(Expr::col((library, Alias::new("id"))).eq(library_id));
    }
    query.clone()
}

fn current_structure_publication_visible(publication_id: PublicationId) -> SelectStatement {
    let publication = Alias::new("current_structure_publication");
    let owner = Alias::new("current_structure_owner");
    let membership = Alias::new("current_structure_membership");
    let library = Alias::new("current_structure_library");
    Query::select()
        .expr(Expr::val(1_i32))
        .from_as(Alias::new("catalog_publications"), publication.clone())
        .join_as(
            JoinType::InnerJoin,
            Alias::new("catalog_items"),
            owner.clone(),
            Expr::col((owner.clone(), Alias::new("active_structure_publication_id")))
                .equals((publication.clone(), Alias::new("id"))),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("library_catalog_items"),
            membership.clone(),
            Expr::col((membership.clone(), Alias::new("catalog_item_id")))
                .equals((owner.clone(), Alias::new("id"))),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("libraries"),
            library.clone(),
            Expr::col((library.clone(), Alias::new("id")))
                .equals((membership, Alias::new("library_id"))),
        )
        .and_where(Expr::col((publication.clone(), Alias::new("id"))).eq(publication_id.as_uuid()))
        .and_where(Expr::col((publication.clone(), Alias::new("publication_kind"))).eq("Structure"))
        .and_where(Expr::col((publication, Alias::new("state"))).eq("Active"))
        .and_where(Expr::col((owner.clone(), Alias::new("is_present"))).eq(true))
        .and_where(Expr::col((owner, Alias::new("classification_state"))).eq("Matched"))
        .and_where(Expr::col((library, Alias::new("is_enabled"))).eq(true))
        .to_owned()
}

fn current_publication_at_revision(
    item: &Alias,
    pointer_column: &str,
    revision_column: &str,
    publication_kind: &str,
) -> SelectStatement {
    let publication = Alias::new(format!("current_{pointer_column}"));
    Query::select()
        .expr(Expr::val(1_i32))
        .from_as(Alias::new("catalog_publications"), publication.clone())
        .and_where(
            Expr::col((publication.clone(), Alias::new("id")))
                .equals((item.clone(), Alias::new(pointer_column))),
        )
        .and_where(
            Expr::col((publication.clone(), Alias::new("expected_revision")))
                .equals((item.clone(), Alias::new(revision_column))),
        )
        .and_where(
            Expr::col((publication.clone(), Alias::new("publication_kind"))).eq(publication_kind),
        )
        .and_where(Expr::col((publication, Alias::new("state"))).eq("Active"))
        .to_owned()
}

#[allow(clippy::too_many_lines)] // Mirrors generation-aware direct/aggregate source selection in one correlated fence.
fn current_effective_source_publication(item: &Alias) -> SelectStatement {
    let candidate = Alias::new("current_source_item");
    let owner = Alias::new("current_source_structure_owner");
    let direct = Alias::new("current_direct_source_publication");
    let structure = Alias::new("current_structure_source_publication");
    let projection = Alias::new("current_structure_source_projection");
    let direct_selected = Condition::all()
        .add(Expr::col((direct.clone(), Alias::new("id"))).is_not_null())
        .add(
            Expr::col((direct.clone(), Alias::new("expected_revision")))
                .equals((candidate.clone(), Alias::new("source_index_revision"))),
        )
        .add(
            Condition::any()
                .add(Expr::col((structure.clone(), Alias::new("id"))).is_null())
                .add(
                    Expr::col((direct.clone(), Alias::new("activated_generation"))).gt(Expr::col(
                        (structure.clone(), Alias::new("activated_generation")),
                    )),
                ),
        );
    let structure_selected = Condition::all()
        .add(Expr::col((structure.clone(), Alias::new("id"))).is_not_null())
        .add(Expr::col((projection.clone(), Alias::new("source_state"))).eq("Indexed"))
        .add(
            Expr::col((projection.clone(), Alias::new("source_index_revision")))
                .equals((candidate.clone(), Alias::new("source_index_revision"))),
        )
        .add(
            Condition::any()
                .add(Expr::col((direct.clone(), Alias::new("id"))).is_null())
                .add(
                    Expr::col((structure.clone(), Alias::new("activated_generation"))).gte(
                        Expr::col((direct.clone(), Alias::new("activated_generation"))),
                    ),
                ),
        );
    Query::select()
        .expr(Expr::val(1_i32))
        .from_as(Alias::new("catalog_items"), candidate.clone())
        .join_as(
            JoinType::LeftJoin,
            Alias::new("catalog_items"),
            owner.clone(),
            Expr::col((owner.clone(), Alias::new("id")))
                .equals((candidate.clone(), Alias::new("structure_owner_item_id"))),
        )
        .join_as(
            JoinType::LeftJoin,
            Alias::new("catalog_publications"),
            direct.clone(),
            Condition::all()
                .add(Expr::col((direct.clone(), Alias::new("id"))).equals((
                    candidate.clone(),
                    Alias::new("active_source_publication_id"),
                )))
                .add(Expr::col((direct.clone(), Alias::new("publication_kind"))).eq("Sources"))
                .add(Expr::col((direct.clone(), Alias::new("state"))).eq("Active")),
        )
        .join_as(
            JoinType::LeftJoin,
            Alias::new("catalog_publications"),
            structure.clone(),
            Condition::all()
                .add(
                    Expr::col((structure.clone(), Alias::new("id")))
                        .equals((owner, Alias::new("active_structure_publication_id"))),
                )
                .add(Expr::col((structure.clone(), Alias::new("publication_kind"))).eq("Structure"))
                .add(Expr::col((structure.clone(), Alias::new("state"))).eq("Active")),
        )
        .join_as(
            JoinType::LeftJoin,
            Alias::new("publication_catalog_items"),
            projection.clone(),
            Condition::all()
                .add(
                    Expr::col((projection.clone(), Alias::new("publication_id")))
                        .equals((structure.clone(), Alias::new("id"))),
                )
                .add(
                    Expr::col((projection.clone(), Alias::new("catalog_item_id")))
                        .equals((candidate.clone(), Alias::new("id"))),
                ),
        )
        .and_where(
            Expr::col((candidate.clone(), Alias::new("id")))
                .equals((item.clone(), Alias::new("id"))),
        )
        .cond_where(
            Condition::any()
                .add(direct_selected)
                .add(structure_selected),
        )
        .to_owned()
}

fn apply_parent(query: &mut SelectStatement, ci: &Alias, parent: BrowseParent) {
    match parent {
        BrowseParent::Library(library_id) => {
            let membership_table = Alias::new("library_catalog_items");
            let library = Alias::new("library");
            let mut membership = Query::select();
            membership
                .expr(Expr::val(1_i32))
                .from(membership_table.clone())
                .join_as(
                    JoinType::InnerJoin,
                    Alias::new("libraries"),
                    library.clone(),
                    Expr::col((library.clone(), Alias::new("id")))
                        .equals((membership_table.clone(), Alias::new("library_id"))),
                )
                .and_where(Expr::col((library.clone(), Alias::new("id"))).eq(library_id))
                .and_where(Expr::col((library, Alias::new("is_enabled"))).eq(true))
                .and_where(
                    Expr::col((membership_table, Alias::new("catalog_item_id")))
                        .equals((ci.clone(), Alias::new("id"))),
                );
            query
                .and_where(Expr::col((ci.clone(), Alias::new("parent_id"))).is_null())
                .and_where(Expr::exists(membership));
        }
        BrowseParent::Item(parent_id) => {
            query
                .and_where(Expr::col((ci.clone(), Alias::new("parent_id"))).eq(parent_id.as_uuid()))
                .and_where(Expr::exists(shared_enabled_membership(ci, parent_id)))
                .and_where(Expr::exists(visible_item(parent_id)));
        }
    }
}

fn shared_enabled_membership(item: &Alias, parent_id: CatalogItemId) -> SelectStatement {
    let child = Alias::new("child_membership");
    let parent = Alias::new("parent_membership");
    let library = Alias::new("shared_library");
    Query::select()
        .expr(Expr::val(1_i32))
        .from_as(Alias::new("library_catalog_items"), child.clone())
        .join_as(
            JoinType::InnerJoin,
            Alias::new("library_catalog_items"),
            parent.clone(),
            Expr::col((parent.clone(), Alias::new("library_id")))
                .equals((child.clone(), Alias::new("library_id"))),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("libraries"),
            library.clone(),
            Expr::col((library.clone(), Alias::new("id")))
                .equals((child.clone(), Alias::new("library_id"))),
        )
        .and_where(
            Expr::col((child, Alias::new("catalog_item_id")))
                .equals((item.clone(), Alias::new("id"))),
        )
        .and_where(Expr::col((parent, Alias::new("catalog_item_id"))).eq(parent_id.as_uuid()))
        .and_where(Expr::col((library, Alias::new("is_enabled"))).eq(true))
        .to_owned()
}

fn visible_item(item_id: CatalogItemId) -> SelectStatement {
    Query::select()
        .expr(Expr::val(1_i32))
        .from(Alias::new("catalog_items"))
        .and_where(Expr::col(Alias::new("id")).eq(item_id.as_uuid()))
        .and_where(Expr::col(Alias::new("is_present")).eq(true))
        .and_where(Expr::col(Alias::new("classification_state")).eq("Matched"))
        .to_owned()
}

pub(crate) async fn catalog_item_is_visible(
    connection: &impl ConnectionTrait,
    item_id: CatalogItemId,
) -> Result<bool, DbErr> {
    let item = Alias::new("visible_catalog_item");
    let query = Query::select()
        .expr(Expr::val(1_i32))
        .from_as(Alias::new("catalog_items"), item.clone())
        .and_where(Expr::col((item.clone(), Alias::new("id"))).eq(item_id.as_uuid()))
        .and_where(Expr::col((item.clone(), Alias::new("is_present"))).eq(true))
        .and_where(Expr::col((item.clone(), Alias::new("classification_state"))).eq("Matched"))
        .cond_where(
            Condition::any()
                .add(Expr::exists(enabled_membership_for_item(&item)))
                .add(Expr::exists(projected_enabled_membership(&item))),
        )
        .limit(1)
        .to_owned();
    let backend = connection.get_database_backend();
    connection
        .query_one(backend.build(&query))
        .await
        .map(|row| row.is_some())
}

pub(crate) async fn lock_catalog_item_visibility(
    transaction: &DatabaseTransaction,
    item_id: CatalogItemId,
) -> Result<bool, DbErr> {
    let library = Alias::new("libraries");
    let direct = Alias::new("locked_direct_membership");
    let direct_membership = Query::select()
        .expr(Expr::val(1_i32))
        .from_as(Alias::new("library_catalog_items"), direct.clone())
        .and_where(
            Expr::col((direct.clone(), Alias::new("library_id")))
                .equals((library.clone(), Alias::new("id"))),
        )
        .and_where(Expr::col((direct, Alias::new("catalog_item_id"))).eq(item_id.as_uuid()))
        .to_owned();
    let projection = Alias::new("locked_projection");
    let publication = Alias::new("locked_publication");
    let owner = Alias::new("locked_owner");
    let owner_membership = Alias::new("locked_owner_membership");
    let projected_membership = Query::select()
        .expr(Expr::val(1_i32))
        .from_as(Alias::new("publication_catalog_items"), projection.clone())
        .join_as(
            JoinType::InnerJoin,
            Alias::new("catalog_publications"),
            publication.clone(),
            Expr::col((publication.clone(), Alias::new("id")))
                .equals((projection.clone(), Alias::new("publication_id"))),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("catalog_items"),
            owner.clone(),
            Expr::col((owner.clone(), Alias::new("active_structure_publication_id")))
                .equals((publication.clone(), Alias::new("id"))),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("library_catalog_items"),
            owner_membership.clone(),
            Expr::col((owner_membership.clone(), Alias::new("catalog_item_id")))
                .equals((owner.clone(), Alias::new("id"))),
        )
        .and_where(
            Expr::col((owner_membership, Alias::new("library_id")))
                .equals((library.clone(), Alias::new("id"))),
        )
        .and_where(Expr::col((projection, Alias::new("catalog_item_id"))).eq(item_id.as_uuid()))
        .and_where(Expr::col((publication.clone(), Alias::new("publication_kind"))).eq("Structure"))
        .and_where(Expr::col((publication, Alias::new("state"))).eq("Active"))
        .and_where(Expr::col((owner.clone(), Alias::new("is_present"))).eq(true))
        .and_where(Expr::col((owner, Alias::new("classification_state"))).eq("Matched"))
        .to_owned();
    let update = Query::update()
        .table(library.clone())
        .value(Alias::new("is_enabled"), true)
        .and_where(Expr::col((library.clone(), Alias::new("is_enabled"))).eq(true))
        .and_where(Expr::exists(visible_item(item_id)))
        .cond_where(
            Condition::any()
                .add(Expr::exists(direct_membership))
                .add(Expr::exists(projected_membership)),
        )
        .to_owned();
    let backend = transaction.get_database_backend();
    transaction
        .execute(backend.build(&update))
        .await
        .map(|result| result.rows_affected() > 0)
}

fn select_item_columns(query: &mut SelectStatement, source: ItemQuerySource) {
    let ud = Alias::new("ud");
    match source {
        ItemQuerySource::Catalog => {
            let ci = Alias::new("ci");
            for column in [
                "id",
                "parent_id",
                "item_type",
                "name",
                "original_title",
                "production_year",
                "overview",
                "community_rating",
                "index_number",
                "runtime_ticks",
                "date_created",
            ] {
                query.expr_as(
                    Expr::col((ci.clone(), Alias::new(column))),
                    Alias::new(column),
                );
            }
        }
        ItemQuerySource::Publication(_) => {
            let pci = Alias::new("pci");
            for (column, alias) in [
                ("catalog_item_id", "id"),
                ("parent_catalog_item_id", "parent_id"),
                ("item_type", "item_type"),
                ("name", "name"),
                ("production_year", "production_year"),
                ("overview", "overview"),
            ] {
                query.expr_as(
                    Expr::col((pci.clone(), Alias::new(column))),
                    Alias::new(alias),
                );
            }
            let catalog_item = Alias::new("publication_item");
            for column in [
                "original_title",
                "community_rating",
                "index_number",
                "runtime_ticks",
                "date_created",
            ] {
                query.expr_as(
                    Expr::col((catalog_item.clone(), Alias::new(column))),
                    Alias::new(column),
                );
            }
        }
    }
    for column in [
        "is_favorite",
        "is_played",
        "play_count",
        "playback_position_ticks",
    ] {
        query.expr_as(
            Expr::col((ud.clone(), Alias::new(column))),
            Alias::new(column),
        );
    }
}

fn library_from_row(row: &QueryResult) -> Result<LibraryViewRecord, CatalogQueryError> {
    Ok(LibraryViewRecord {
        id: row.try_get("", "id")?,
        name: row.try_get("", "name")?,
        collection_type: row.try_get("", "collection_type")?,
    })
}

fn item_from_row(row: &QueryResult) -> Result<CatalogItemRecord, CatalogQueryError> {
    Ok(CatalogItemRecord {
        id: CatalogItemId::from_uuid(row.try_get("", "id")?),
        parent_id: row
            .try_get::<Option<Uuid>>("", "parent_id")?
            .map(CatalogItemId::from_uuid),
        item_type: row.try_get("", "item_type")?,
        name: row.try_get("", "name")?,
        original_title: row.try_get("", "original_title")?,
        production_year: row.try_get("", "production_year")?,
        overview: row.try_get("", "overview")?,
        community_rating: row.try_get("", "community_rating")?,
        index_number: row.try_get("", "index_number")?,
        runtime_ticks: row.try_get("", "runtime_ticks")?,
        date_created: row.try_get("", "date_created")?,
        is_favorite: row
            .try_get::<Option<bool>>("", "is_favorite")?
            .unwrap_or(false),
        is_played: row
            .try_get::<Option<bool>>("", "is_played")?
            .unwrap_or(false),
        play_count: row.try_get::<Option<i32>>("", "play_count")?.unwrap_or(0),
        playback_position_ticks: row
            .try_get::<Option<i64>>("", "playback_position_ticks")?
            .unwrap_or(0),
        image_tags: BTreeMap::new(),
        backdrop_image_tags: Vec::new(),
        primary_image_aspect_ratio: None,
    })
}

async fn attach_image_tags(
    database: &DatabaseConnection,
    items: &mut [CatalogItemRecord],
) -> Result<(), CatalogQueryError> {
    if items.is_empty() {
        return Ok(());
    }
    let asset = Alias::new("asset");
    let blob = Alias::new("blob");
    let mut query = Query::select();
    query
        .from_as(Alias::new("item_assets"), asset.clone())
        .join_as(
            JoinType::InnerJoin,
            Alias::new("asset_blobs"),
            blob.clone(),
            Expr::col((blob.clone(), Alias::new("id")))
                .equals((asset.clone(), Alias::new("asset_blob_id"))),
        )
        .expr_as(
            Expr::col((asset.clone(), Alias::new("item_id"))),
            Alias::new("item_id"),
        )
        .expr_as(
            Expr::col((asset.clone(), Alias::new("image_type"))),
            Alias::new("image_type"),
        )
        .expr_as(
            Expr::col((blob, Alias::new("sha256"))),
            Alias::new("sha256"),
        )
        .expr_as(
            Expr::col((asset.clone(), Alias::new("priority"))),
            Alias::new("priority"),
        )
        .expr_as(
            Expr::col((Alias::new("blob"), Alias::new("width"))),
            Alias::new("width"),
        )
        .expr_as(
            Expr::col((Alias::new("blob"), Alias::new("height"))),
            Alias::new("height"),
        )
        .and_where(
            Expr::col((asset, Alias::new("item_id")))
                .is_in(items.iter().map(|item| item.id.as_uuid())),
        )
        .order_by((Alias::new("asset"), Alias::new("item_id")), Order::Asc)
        .order_by((Alias::new("asset"), Alias::new("image_type")), Order::Asc)
        .order_by((Alias::new("asset"), Alias::new("priority")), Order::Asc)
        .order_by((Alias::new("asset"), Alias::new("id")), Order::Asc);
    let backend = database.get_database_backend();
    let rows = database.query_all(backend.build(&query)).await?;
    let mut by_item = BTreeMap::<Uuid, BTreeMap<String, String>>::new();
    let mut backdrops = BTreeMap::<Uuid, Vec<String>>::new();
    let mut primary_aspect_ratios = BTreeMap::<Uuid, f64>::new();
    for row in rows {
        let item_id: Uuid = row.try_get("", "item_id")?;
        let image_type: String = row.try_get("", "image_type")?;
        let sha256: String = row.try_get("", "sha256")?;
        let priority: i32 = row.try_get("", "priority")?;
        if image_type == "Backdrop" {
            backdrops.entry(item_id).or_default().push(sha256.clone());
        }
        if priority == 0 {
            if image_type == "Primary" {
                let width: Option<i32> = row.try_get("", "width")?;
                let height: Option<i32> = row.try_get("", "height")?;
                if let Some((width, height)) = width
                    .zip(height)
                    .filter(|(width, height)| *width > 0 && *height > 0)
                {
                    primary_aspect_ratios.insert(item_id, f64::from(width) / f64::from(height));
                }
            }
            by_item
                .entry(item_id)
                .or_default()
                .insert(image_type, sha256);
        }
    }
    for item in items {
        item.image_tags = by_item.remove(&item.id.as_uuid()).unwrap_or_default();
        item.backdrop_image_tags = backdrops.remove(&item.id.as_uuid()).unwrap_or_default();
        item.primary_image_aspect_ratio = primary_aspect_ratios.remove(&item.id.as_uuid());
    }
    Ok(())
}

fn asset_from_row(row: &QueryResult) -> Result<AssetRecord, CatalogQueryError> {
    let byte_size: i64 = row.try_get("", "byte_size")?;
    Ok(AssetRecord {
        sha256: row.try_get("", "sha256")?,
        mime_type: row.try_get("", "mime_type")?,
        width: row.try_get("", "width")?,
        height: row.try_get("", "height")?,
        byte_size: u64::try_from(byte_size).map_err(|_| CatalogQueryError::InvalidAssetSize)?,
        local_relative_path: row.try_get("", "local_relative_path")?,
    })
}
