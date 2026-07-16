use sea_orm::{
    ConnectionTrait, DatabaseConnection, DbErr, QueryResult,
    sea_query::{Alias, Condition, Expr, JoinType, Order, Query, SelectStatement},
};
use thiserror::Error;
use tjxy_common::{CatalogItemId, UserId};
use uuid::Uuid;

const MAX_PAGE_SIZE: u64 = 200;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BrowseParent {
    Library(Uuid),
    Item(CatalogItemId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CatalogItemType {
    Movie,
    Series,
    Season,
    Episode,
    Folder,
}

impl CatalogItemType {
    const fn as_database_value(self) -> &'static str {
        match self {
            Self::Movie => "Movie",
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
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LibraryViewRecord {
    id: Uuid,
    name: String,
    collection_type: String,
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogItemRecord {
    id: CatalogItemId,
    parent_id: Option<CatalogItemId>,
    item_type: String,
    name: String,
    production_year: Option<i32>,
    overview: Option<String>,
    is_favorite: bool,
    is_played: bool,
    play_count: i32,
    playback_position_ticks: i64,
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
    pub const fn production_year(&self) -> Option<i32> {
        self.production_year
    }

    #[must_use]
    pub fn overview(&self) -> Option<&str> {
        self.overview.as_deref()
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
}

#[derive(Clone, Debug, Eq, PartialEq)]
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

impl<'connection> CatalogQueryRepository<'connection> {
    #[must_use]
    pub const fn new(database: &'connection DatabaseConnection) -> Self {
        Self { database }
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
        let is_item = self
            .database
            .query_one(backend.build(&visible_item))
            .await?
            .is_some();

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
        let mut count = item_query(user_id, parent, &page.item_types);
        count.expr_as(
            Expr::col((Alias::new("ci"), Alias::new("id"))).count(),
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

        let mut query = item_query(user_id, parent, &page.item_types);
        select_item_columns(&mut query);
        query
            .order_by((Alias::new("ci"), Alias::new("sort_key")), Order::Asc)
            .order_by((Alias::new("ci"), Alias::new("id")), Order::Asc)
            .offset(page.start_index)
            .limit(page.limit);
        let items = self
            .database
            .query_all(backend.build(&query))
            .await?
            .iter()
            .map(item_from_row)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(CatalogPage {
            items,
            total_record_count,
            start_index: page.start_index,
        })
    }
}

#[derive(Debug, Error)]
pub enum CatalogQueryError {
    #[error("page limit must be between 1 and 200")]
    InvalidPage,
    #[error("catalog count row is missing")]
    MissingCount,
    #[error("catalog count is outside the supported range")]
    InvalidCount,
    #[error("parent UUID exists in both library and catalog item namespaces: {0}")]
    AmbiguousParent(Uuid),
    #[error("catalog query failed: {0}")]
    Database(#[from] DbErr),
}

fn item_query(
    user_id: UserId,
    parent: BrowseParent,
    item_types: &[CatalogItemType],
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
        .and_where(Expr::col((ci.clone(), Alias::new("is_present"))).eq(true))
        .and_where(Expr::col((ci.clone(), Alias::new("classification_state"))).eq("Matched"));
    if !item_types.is_empty() {
        query.and_where(
            Expr::col((ci.clone(), Alias::new("item_type"))).is_in(
                item_types
                    .iter()
                    .map(|item_type| item_type.as_database_value()),
            ),
        );
    }
    apply_parent(&mut query, &ci, parent);
    query.clone()
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
            let child = Alias::new("child_membership");
            let parent = Alias::new("parent_membership");
            let library = Alias::new("shared_library");
            let mut shared = Query::select();
            shared
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
                        .equals((ci.clone(), Alias::new("id"))),
                )
                .and_where(
                    Expr::col((parent, Alias::new("catalog_item_id"))).eq(parent_id.as_uuid()),
                )
                .and_where(Expr::col((library, Alias::new("is_enabled"))).eq(true));
            query
                .and_where(Expr::col((ci.clone(), Alias::new("parent_id"))).eq(parent_id.as_uuid()))
                .and_where(Expr::exists(shared))
                .and_where(Expr::exists(visible_item(parent_id)));
        }
    }
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

fn select_item_columns(query: &mut SelectStatement) {
    let ci = Alias::new("ci");
    let ud = Alias::new("ud");
    for column in [
        "id",
        "parent_id",
        "item_type",
        "name",
        "production_year",
        "overview",
    ] {
        query.expr_as(
            Expr::col((ci.clone(), Alias::new(column))),
            Alias::new(column),
        );
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
        production_year: row.try_get("", "production_year")?,
        overview: row.try_get("", "overview")?,
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
    })
}
