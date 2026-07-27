use chrono::{DateTime, Utc};
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbErr, QueryResult, TransactionTrait,
    sea_query::{Alias, Expr, JoinType, Order, Query},
};
use thiserror::Error;
use tjxy_common::{CatalogItemId, LibraryId};

const SELECTED_AT: &str = "hybrid_admin_selected_at";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HybridCandidateRecord {
    catalog_item_id: CatalogItemId,
    name: String,
    production_year: Option<i32>,
    structure_state: String,
    selected_at: DateTime<Utc>,
}

impl HybridCandidateRecord {
    #[must_use]
    pub const fn catalog_item_id(&self) -> CatalogItemId {
        self.catalog_item_id
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
    pub fn structure_state(&self) -> &str {
        &self.structure_state
    }

    #[must_use]
    pub const fn selected_at(&self) -> DateTime<Utc> {
        self.selected_at
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HybridCandidatePage {
    items: Vec<HybridCandidateRecord>,
    total_record_count: u64,
    start_index: u64,
}

impl HybridCandidatePage {
    #[must_use]
    pub fn items(&self) -> &[HybridCandidateRecord] {
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HybridCandidateMutation {
    changed: bool,
}

impl HybridCandidateMutation {
    #[must_use]
    pub const fn changed(self) -> bool {
        self.changed
    }
}

pub struct HybridCandidateRepository<'connection> {
    database: &'connection DatabaseConnection,
}

impl<'connection> HybridCandidateRepository<'connection> {
    #[must_use]
    pub const fn new(database: &'connection DatabaseConnection) -> Self {
        Self { database }
    }

    /// Lists one stable page of administrator-selected Series for a Library.
    ///
    /// # Errors
    ///
    /// Returns [`HybridCandidateError`] for an unknown Library, invalid page, or SQL failure.
    pub async fn selected(
        &self,
        library_id: LibraryId,
        start_index: u64,
        limit: u64,
    ) -> Result<HybridCandidatePage, HybridCandidateError> {
        if !(1..=100).contains(&limit) {
            return Err(HybridCandidateError::InvalidPage);
        }
        ensure_library(self.database, library_id).await?;
        let membership = Alias::new("selected_hybrid_membership");
        let item = Alias::new("selected_hybrid_item");
        let total_query = Query::select()
            .expr_as(
                Expr::col((membership.clone(), Alias::new("id"))).count(),
                Alias::new("count"),
            )
            .from_as(Alias::new("library_catalog_items"), membership.clone())
            .and_where(
                Expr::col((membership.clone(), Alias::new("library_id"))).eq(library_id.as_uuid()),
            )
            .and_where(Expr::col((membership.clone(), Alias::new(SELECTED_AT))).is_not_null())
            .to_owned();
        let backend = self.database.get_database_backend();
        let total = self
            .database
            .query_one(backend.build(&total_query))
            .await?
            .ok_or(HybridCandidateError::DatabaseInvariant)?
            .try_get::<i64>("", "count")?;
        let total_record_count =
            u64::try_from(total).map_err(|_| HybridCandidateError::DatabaseInvariant)?;
        let query = Query::select()
            .expr_as(
                Expr::col((membership.clone(), Alias::new("catalog_item_id"))),
                Alias::new("catalog_item_id"),
            )
            .expr_as(
                Expr::col((membership.clone(), Alias::new(SELECTED_AT))),
                Alias::new("selected_at"),
            )
            .expr_as(
                Expr::col((item.clone(), Alias::new("name"))),
                Alias::new("name"),
            )
            .expr_as(
                Expr::col((item.clone(), Alias::new("production_year"))),
                Alias::new("production_year"),
            )
            .expr_as(
                Expr::col((item.clone(), Alias::new("structure_state"))),
                Alias::new("structure_state"),
            )
            .from_as(Alias::new("library_catalog_items"), membership.clone())
            .join_as(
                JoinType::InnerJoin,
                Alias::new("catalog_items"),
                item.clone(),
                Expr::col((item.clone(), Alias::new("id")))
                    .equals((membership.clone(), Alias::new("catalog_item_id"))),
            )
            .and_where(
                Expr::col((membership.clone(), Alias::new("library_id"))).eq(library_id.as_uuid()),
            )
            .and_where(Expr::col((membership.clone(), Alias::new(SELECTED_AT))).is_not_null())
            .order_by((membership.clone(), Alias::new(SELECTED_AT)), Order::Asc)
            .order_by((membership, Alias::new("catalog_item_id")), Order::Asc)
            .offset(start_index)
            .limit(limit)
            .to_owned();
        let items = self
            .database
            .query_all(backend.build(&query))
            .await?
            .iter()
            .map(record_from_row)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(HybridCandidatePage {
            items,
            total_record_count,
            start_index,
        })
    }

    /// Idempotently marks one visible, matched Series as an administrator Hybrid candidate.
    ///
    /// # Errors
    ///
    /// Returns [`HybridCandidateError`] when the Library is not enabled for background expansion,
    /// the item is not an eligible member, or the SQL transaction fails.
    pub async fn pin(
        &self,
        library_id: LibraryId,
        item_id: CatalogItemId,
    ) -> Result<HybridCandidateMutation, HybridCandidateError> {
        let transaction = self.database.begin().await?;
        let result = pin(&transaction, library_id, item_id).await;
        finish(transaction, result).await
    }

    /// Idempotently removes an administrator Hybrid preference without cancelling durable work.
    ///
    /// # Errors
    ///
    /// Returns [`HybridCandidateError`] when the SQL update cannot be committed.
    pub async fn unpin(
        &self,
        library_id: LibraryId,
        item_id: CatalogItemId,
    ) -> Result<HybridCandidateMutation, HybridCandidateError> {
        let statement = Query::update()
            .table(Alias::new("library_catalog_items"))
            .value(Alias::new(SELECTED_AT), Option::<DateTime<Utc>>::None)
            .and_where(Expr::col(Alias::new("library_id")).eq(library_id.as_uuid()))
            .and_where(Expr::col(Alias::new("catalog_item_id")).eq(item_id.as_uuid()))
            .and_where(Expr::col(Alias::new(SELECTED_AT)).is_not_null())
            .to_owned();
        let changed = self
            .database
            .execute(self.database.get_database_backend().build(&statement))
            .await?
            .rows_affected()
            == 1;
        Ok(HybridCandidateMutation { changed })
    }
}

#[derive(Debug, Error)]
pub enum HybridCandidateError {
    #[error("hybrid candidate page limit must be between 1 and 100")]
    InvalidPage,
    #[error("library is unavailable")]
    LibraryUnavailable,
    #[error("library is not enabled for background expansion")]
    LibraryNotBackground,
    #[error("catalog item is not a visible matched Series in this library")]
    ItemUnavailable,
    #[error("hybrid candidate database result is invalid")]
    DatabaseInvariant,
    #[error("hybrid candidate database operation failed: {0}")]
    Database(#[from] DbErr),
    #[error("hybrid candidate rollback failed after {original}: {rollback}")]
    RollbackFailed { original: String, rollback: DbErr },
}

async fn pin(
    transaction: &DatabaseTransaction,
    library_id: LibraryId,
    item_id: CatalogItemId,
) -> Result<HybridCandidateMutation, HybridCandidateError> {
    ensure_background_library(transaction, library_id).await?;
    let selected = candidate_selected_at(transaction, library_id, item_id).await?;
    if selected.is_some() {
        return Ok(HybridCandidateMutation { changed: false });
    }
    let library = Alias::new("pin_hybrid_library");
    let eligible_library = Query::select()
        .expr(Expr::val(1_i32))
        .from_as(Alias::new("libraries"), library.clone())
        .and_where(Expr::col((library.clone(), Alias::new("id"))).eq(library_id.as_uuid()))
        .and_where(Expr::col((library.clone(), Alias::new("is_enabled"))).eq(true))
        .and_where(Expr::col((library, Alias::new("expansion_policy"))).eq("background"))
        .limit(1)
        .to_owned();
    let item = Alias::new("pin_hybrid_eligible_item");
    let eligible_item = Query::select()
        .expr(Expr::val(1_i32))
        .from_as(Alias::new("catalog_items"), item.clone())
        .and_where(Expr::col((item.clone(), Alias::new("id"))).eq(item_id.as_uuid()))
        .and_where(Expr::col((item.clone(), Alias::new("item_type"))).eq("Series"))
        .and_where(Expr::col((item.clone(), Alias::new("is_present"))).eq(true))
        .and_where(Expr::col((item, Alias::new("classification_state"))).eq("Matched"))
        .limit(1)
        .to_owned();
    let statement = Query::update()
        .table(Alias::new("library_catalog_items"))
        .value(Alias::new(SELECTED_AT), Utc::now())
        .and_where(Expr::col(Alias::new("library_id")).eq(library_id.as_uuid()))
        .and_where(Expr::col(Alias::new("catalog_item_id")).eq(item_id.as_uuid()))
        .and_where(Expr::col(Alias::new(SELECTED_AT)).is_null())
        .and_where(Expr::exists(eligible_library))
        .and_where(Expr::exists(eligible_item))
        .to_owned();
    if transaction
        .execute(transaction.get_database_backend().build(&statement))
        .await?
        .rows_affected()
        == 1
    {
        return Ok(HybridCandidateMutation { changed: true });
    }
    ensure_background_library(transaction, library_id).await?;
    if candidate_selected_at(transaction, library_id, item_id)
        .await?
        .is_some()
    {
        Ok(HybridCandidateMutation { changed: false })
    } else {
        Err(HybridCandidateError::ItemUnavailable)
    }
}

async fn ensure_library(
    database: &impl ConnectionTrait,
    library_id: LibraryId,
) -> Result<(), HybridCandidateError> {
    let query = Query::select()
        .expr(Expr::val(1_i32))
        .from(Alias::new("libraries"))
        .and_where(Expr::col(Alias::new("id")).eq(library_id.as_uuid()))
        .limit(1)
        .to_owned();
    if database
        .query_one(database.get_database_backend().build(&query))
        .await?
        .is_some()
    {
        Ok(())
    } else {
        Err(HybridCandidateError::LibraryUnavailable)
    }
}

async fn ensure_background_library(
    database: &impl ConnectionTrait,
    library_id: LibraryId,
) -> Result<(), HybridCandidateError> {
    let query = Query::select()
        .columns([Alias::new("is_enabled"), Alias::new("expansion_policy")])
        .from(Alias::new("libraries"))
        .and_where(Expr::col(Alias::new("id")).eq(library_id.as_uuid()))
        .limit(1)
        .to_owned();
    let Some(row) = database
        .query_one(database.get_database_backend().build(&query))
        .await?
    else {
        return Err(HybridCandidateError::LibraryUnavailable);
    };
    if row.try_get::<bool>("", "is_enabled")?
        && row.try_get::<String>("", "expansion_policy")? == "background"
    {
        Ok(())
    } else {
        Err(HybridCandidateError::LibraryNotBackground)
    }
}

async fn candidate_selected_at(
    database: &impl ConnectionTrait,
    library_id: LibraryId,
    item_id: CatalogItemId,
) -> Result<Option<DateTime<Utc>>, HybridCandidateError> {
    let membership = Alias::new("pin_hybrid_membership");
    let item = Alias::new("pin_hybrid_item");
    let query = Query::select()
        .expr_as(
            Expr::col((membership.clone(), Alias::new(SELECTED_AT))),
            Alias::new("selected_at"),
        )
        .from_as(Alias::new("library_catalog_items"), membership.clone())
        .join_as(
            JoinType::InnerJoin,
            Alias::new("catalog_items"),
            item.clone(),
            Expr::col((item.clone(), Alias::new("id")))
                .equals((membership.clone(), Alias::new("catalog_item_id"))),
        )
        .and_where(
            Expr::col((membership.clone(), Alias::new("library_id"))).eq(library_id.as_uuid()),
        )
        .and_where(Expr::col((membership, Alias::new("catalog_item_id"))).eq(item_id.as_uuid()))
        .and_where(Expr::col((item.clone(), Alias::new("item_type"))).eq("Series"))
        .and_where(Expr::col((item.clone(), Alias::new("is_present"))).eq(true))
        .and_where(Expr::col((item, Alias::new("classification_state"))).eq("Matched"))
        .limit(1)
        .to_owned();
    database
        .query_one(database.get_database_backend().build(&query))
        .await?
        .map(|row| row.try_get::<Option<DateTime<Utc>>>("", "selected_at"))
        .transpose()?
        .ok_or(HybridCandidateError::ItemUnavailable)
}

fn record_from_row(row: &QueryResult) -> Result<HybridCandidateRecord, HybridCandidateError> {
    Ok(HybridCandidateRecord {
        catalog_item_id: CatalogItemId::from_uuid(row.try_get("", "catalog_item_id")?),
        name: row.try_get("", "name")?,
        production_year: row.try_get("", "production_year")?,
        structure_state: row.try_get("", "structure_state")?,
        selected_at: row.try_get("", "selected_at")?,
    })
}

async fn finish<T>(
    transaction: DatabaseTransaction,
    result: Result<T, HybridCandidateError>,
) -> Result<T, HybridCandidateError> {
    match result {
        Ok(value) => {
            transaction.commit().await?;
            Ok(value)
        }
        Err(original) => match transaction.rollback().await {
            Ok(()) => Err(original),
            Err(rollback) => Err(HybridCandidateError::RollbackFailed {
                original: original.to_string(),
                rollback,
            }),
        },
    }
}
