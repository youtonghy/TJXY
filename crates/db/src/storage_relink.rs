use std::collections::HashSet;

use sea_orm::{
    ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbErr, QueryResult, TransactionTrait,
    sea_query::{Alias, Expr, OnConflict, Query},
};
use serde_json::json;
use thiserror::Error;
use tjxy_common::CatalogItemId;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageRelinkDecision {
    Confirm,
    Reject,
}

impl StorageRelinkDecision {
    const fn state(self) -> &'static str {
        match self {
            Self::Confirm => "Confirmed",
            Self::Reject => "Rejected",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StorageRelinkDecisionReport {
    changed: bool,
    state: &'static str,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StorageRelinkCandidate {
    id: Uuid,
    root_id: Uuid,
    previous_object_id: Uuid,
    replacement_object_id: Uuid,
    previous_name: String,
    replacement_name: String,
    confidence: f64,
    evidence: serde_json::Value,
    state: String,
}

impl StorageRelinkCandidate {
    #[must_use]
    pub const fn id(&self) -> Uuid {
        self.id
    }

    #[must_use]
    pub const fn root_id(&self) -> Uuid {
        self.root_id
    }

    #[must_use]
    pub const fn previous_object_id(&self) -> Uuid {
        self.previous_object_id
    }

    #[must_use]
    pub const fn replacement_object_id(&self) -> Uuid {
        self.replacement_object_id
    }

    #[must_use]
    pub fn previous_name(&self) -> &str {
        &self.previous_name
    }

    #[must_use]
    pub fn replacement_name(&self) -> &str {
        &self.replacement_name
    }

    #[must_use]
    pub const fn confidence(&self) -> f64 {
        self.confidence
    }

    #[must_use]
    pub const fn evidence(&self) -> &serde_json::Value {
        &self.evidence
    }

    #[must_use]
    pub fn state(&self) -> &str {
        &self.state
    }
}

impl StorageRelinkDecisionReport {
    #[must_use]
    pub const fn changed(self) -> bool {
        self.changed
    }

    #[must_use]
    pub const fn state(self) -> &'static str {
        self.state
    }
}

pub struct StorageRelinkRepository<'connection> {
    database: &'connection DatabaseConnection,
}

impl<'connection> StorageRelinkRepository<'connection> {
    #[must_use]
    pub const fn new(database: &'connection DatabaseConnection) -> Self {
        Self { database }
    }

    /// Lists a bounded oldest-first page of pending weak-path relink candidates.
    ///
    /// Provider object identities are deliberately omitted because a weak filesystem identity
    /// can contain a real local path.
    ///
    /// # Errors
    ///
    /// Returns [`StorageRelinkRepositoryError::InvalidLimit`] outside `1..=200` or on SQL failure.
    pub async fn pending(
        &self,
        limit: u64,
    ) -> Result<Vec<StorageRelinkCandidate>, StorageRelinkRepositoryError> {
        if !(1..=200).contains(&limit) {
            return Err(StorageRelinkRepositoryError::InvalidLimit);
        }
        let candidate = Alias::new("pending_relink_candidate");
        let previous = Alias::new("pending_relink_previous");
        let replacement = Alias::new("pending_relink_replacement");
        let query = Query::select()
            .expr_as(
                Expr::col((candidate.clone(), Alias::new("id"))),
                Alias::new("candidate_id"),
            )
            .expr_as(
                Expr::col((candidate.clone(), Alias::new("storage_root_id"))),
                Alias::new("storage_root_id"),
            )
            .expr_as(
                Expr::col((candidate.clone(), Alias::new("previous_storage_object_id"))),
                Alias::new("previous_storage_object_id"),
            )
            .expr_as(
                Expr::col((
                    candidate.clone(),
                    Alias::new("replacement_storage_object_id"),
                )),
                Alias::new("replacement_storage_object_id"),
            )
            .expr_as(
                Expr::col((previous.clone(), Alias::new("name"))),
                Alias::new("previous_name"),
            )
            .expr_as(
                Expr::col((replacement.clone(), Alias::new("name"))),
                Alias::new("replacement_name"),
            )
            .expr_as(
                Expr::col((candidate.clone(), Alias::new("confidence"))),
                Alias::new("confidence"),
            )
            .expr_as(
                Expr::col((candidate.clone(), Alias::new("evidence"))),
                Alias::new("evidence"),
            )
            .expr_as(
                Expr::col((candidate.clone(), Alias::new("state"))),
                Alias::new("state"),
            )
            .from_as(Alias::new("storage_relink_candidates"), candidate.clone())
            .join_as(
                sea_orm::sea_query::JoinType::InnerJoin,
                Alias::new("storage_objects"),
                previous.clone(),
                Expr::col((previous.clone(), Alias::new("id")))
                    .equals((candidate.clone(), Alias::new("previous_storage_object_id"))),
            )
            .join_as(
                sea_orm::sea_query::JoinType::InnerJoin,
                Alias::new("storage_objects"),
                replacement.clone(),
                Expr::col((replacement.clone(), Alias::new("id"))).equals((
                    candidate.clone(),
                    Alias::new("replacement_storage_object_id"),
                )),
            )
            .and_where(Expr::col((candidate.clone(), Alias::new("state"))).eq("Pending"))
            .order_by(
                (candidate.clone(), Alias::new("created_at")),
                sea_orm::Order::Asc,
            )
            .order_by((candidate, Alias::new("id")), sea_orm::Order::Asc)
            .limit(limit)
            .to_owned();
        self.database
            .query_all(self.database.get_database_backend().build(&query))
            .await?
            .iter()
            .map(candidate_from_row)
            .collect()
    }

    /// Confirms or rejects one pending weak-path relink candidate atomically.
    ///
    /// Repeating the same decision is an idempotent no-op. The opposite decision conflicts.
    ///
    /// # Errors
    ///
    /// Returns an error when the candidate is missing, stale, contradictory, or cannot be
    /// committed without violating stable identity invariants.
    pub async fn decide(
        &self,
        candidate_id: Uuid,
        decision: StorageRelinkDecision,
    ) -> Result<StorageRelinkDecisionReport, StorageRelinkRepositoryError> {
        let transaction = self.database.begin().await?;
        let result = decide(&transaction, candidate_id, decision).await;
        finish(transaction, result).await
    }
}

#[derive(Debug, Error)]
pub enum StorageRelinkRepositoryError {
    #[error("storage relink page limit must be from 1 through 200")]
    InvalidLimit,
    #[error("storage relink candidate was not found")]
    NotFound,
    #[error("storage relink candidate already has a different terminal decision")]
    DecisionConflict,
    #[error("storage relink candidate no longer describes an absent-to-present weak identity")]
    StaleCandidate,
    #[error("replacement object is already matched to a different catalog item")]
    IdentityConflict,
    #[error("storage relink database operation failed: {0}")]
    Database(#[from] DbErr),
    #[error("storage relink rollback failed after {original}: {rollback}")]
    RollbackFailed { original: String, rollback: DbErr },
}

fn candidate_from_row(
    row: &QueryResult,
) -> Result<StorageRelinkCandidate, StorageRelinkRepositoryError> {
    Ok(StorageRelinkCandidate {
        id: row.try_get("", "candidate_id")?,
        root_id: row.try_get("", "storage_root_id")?,
        previous_object_id: row.try_get("", "previous_storage_object_id")?,
        replacement_object_id: row.try_get("", "replacement_storage_object_id")?,
        previous_name: row.try_get("", "previous_name")?,
        replacement_name: row.try_get("", "replacement_name")?,
        confidence: row.try_get("", "confidence")?,
        evidence: row.try_get("", "evidence")?,
        state: row.try_get("", "state")?,
    })
}

struct CandidateContext {
    state: String,
    previous_id: Uuid,
    replacement_id: Uuid,
    previous_presence: String,
    replacement_presence: String,
    previous_quality: String,
    replacement_quality: String,
}

async fn decide(
    transaction: &DatabaseTransaction,
    candidate_id: Uuid,
    decision: StorageRelinkDecision,
) -> Result<StorageRelinkDecisionReport, StorageRelinkRepositoryError> {
    let context = candidate_context(transaction, candidate_id)
        .await?
        .ok_or(StorageRelinkRepositoryError::NotFound)?;
    let target_state = decision.state();
    if context.state == target_state {
        return Ok(StorageRelinkDecisionReport {
            changed: false,
            state: target_state,
        });
    }
    if context.state != "Pending" {
        return Err(StorageRelinkRepositoryError::DecisionConflict);
    }
    if context.previous_presence != "ConfirmedAbsent"
        || context.replacement_presence != "Present"
        || context.previous_quality != "PathWeak"
        || context.replacement_quality != "PathWeak"
    {
        return Err(StorageRelinkRepositoryError::StaleCandidate);
    }
    if decision == StorageRelinkDecision::Confirm {
        confirm_candidate(transaction, &context).await?;
    }
    let update = Query::update()
        .table(Alias::new("storage_relink_candidates"))
        .value(Alias::new("state"), target_state)
        .and_where(Expr::col(Alias::new("id")).eq(candidate_id))
        .and_where(Expr::col(Alias::new("state")).eq("Pending"))
        .to_owned();
    if transaction
        .execute(transaction.get_database_backend().build(&update))
        .await?
        .rows_affected()
        != 1
    {
        return Err(StorageRelinkRepositoryError::DecisionConflict);
    }
    Ok(StorageRelinkDecisionReport {
        changed: true,
        state: target_state,
    })
}

async fn candidate_context(
    transaction: &DatabaseTransaction,
    candidate_id: Uuid,
) -> Result<Option<CandidateContext>, DbErr> {
    let candidate = Alias::new("decision_candidate");
    let previous = Alias::new("decision_previous");
    let replacement = Alias::new("decision_replacement");
    let previous_relation = Alias::new("decision_previous_relation");
    let replacement_relation = Alias::new("decision_replacement_relation");
    let query = Query::select()
        .expr_as(
            Expr::col((candidate.clone(), Alias::new("state"))),
            Alias::new("candidate_state"),
        )
        .expr_as(
            Expr::col((previous.clone(), Alias::new("id"))),
            Alias::new("previous_id"),
        )
        .expr_as(
            Expr::col((replacement.clone(), Alias::new("id"))),
            Alias::new("replacement_id"),
        )
        .expr_as(
            Expr::col((previous_relation.clone(), Alias::new("presence_state"))),
            Alias::new("previous_presence"),
        )
        .expr_as(
            Expr::col((replacement_relation.clone(), Alias::new("presence_state"))),
            Alias::new("replacement_presence"),
        )
        .expr_as(
            Expr::col((previous.clone(), Alias::new("identity_quality"))),
            Alias::new("previous_quality"),
        )
        .expr_as(
            Expr::col((replacement.clone(), Alias::new("identity_quality"))),
            Alias::new("replacement_quality"),
        )
        .from_as(Alias::new("storage_relink_candidates"), candidate.clone())
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("storage_objects"),
            previous.clone(),
            Expr::col((previous.clone(), Alias::new("id")))
                .equals((candidate.clone(), Alias::new("previous_storage_object_id"))),
        )
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("storage_objects"),
            replacement.clone(),
            Expr::col((replacement.clone(), Alias::new("id"))).equals((
                candidate.clone(),
                Alias::new("replacement_storage_object_id"),
            )),
        )
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("storage_root_objects"),
            previous_relation.clone(),
            Expr::col((previous_relation.clone(), Alias::new("storage_object_id")))
                .equals((previous.clone(), Alias::new("id"))),
        )
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("storage_root_objects"),
            replacement_relation.clone(),
            Expr::col((
                replacement_relation.clone(),
                Alias::new("storage_object_id"),
            ))
            .equals((replacement.clone(), Alias::new("id"))),
        )
        .and_where(Expr::col((candidate.clone(), Alias::new("id"))).eq(candidate_id))
        .and_where(
            Expr::col((previous_relation.clone(), Alias::new("storage_root_id")))
                .equals((candidate.clone(), Alias::new("storage_root_id"))),
        )
        .and_where(
            Expr::col((replacement_relation.clone(), Alias::new("storage_root_id")))
                .equals((candidate, Alias::new("storage_root_id"))),
        )
        .limit(1)
        .to_owned();
    transaction
        .query_one(transaction.get_database_backend().build(&query))
        .await?
        .map(|row| {
            Ok(CandidateContext {
                state: row.try_get("", "candidate_state")?,
                previous_id: row.try_get("", "previous_id")?,
                replacement_id: row.try_get("", "replacement_id")?,
                previous_presence: row.try_get("", "previous_presence")?,
                replacement_presence: row.try_get("", "replacement_presence")?,
                previous_quality: row.try_get("", "previous_quality")?,
                replacement_quality: row.try_get("", "replacement_quality")?,
            })
        })
        .transpose()
}

async fn confirm_candidate(
    transaction: &DatabaseTransaction,
    context: &CandidateContext,
) -> Result<(), StorageRelinkRepositoryError> {
    let matched_items = matched_items(transaction, context.previous_id).await?;
    let affected_items = affected_items(transaction, context.previous_id, &matched_items).await?;
    ensure_no_identity_conflict(transaction, context.replacement_id, &matched_items).await?;
    for item_id in matched_items {
        let insert = Query::insert()
            .into_table(Alias::new("identity_matches"))
            .columns([
                Alias::new("id"),
                Alias::new("storage_object_id"),
                Alias::new("candidate_catalog_item_id"),
                Alias::new("confidence"),
                Alias::new("state"),
                Alias::new("evidence"),
            ])
            .values_panic([
                Uuid::new_v4().into(),
                context.replacement_id.into(),
                item_id.as_uuid().into(),
                1.0.into(),
                "Matched".into(),
                json!({"kind":"AdminRelink","previous_storage_object_id":context.previous_id})
                    .into(),
            ])
            .on_conflict(
                OnConflict::columns([
                    Alias::new("storage_object_id"),
                    Alias::new("candidate_catalog_item_id"),
                ])
                .update_columns([
                    Alias::new("confidence"),
                    Alias::new("state"),
                    Alias::new("evidence"),
                ])
                .to_owned(),
            )
            .to_owned();
        transaction
            .execute(transaction.get_database_backend().build(&insert))
            .await?;
    }
    invalidate_items(transaction, &affected_items).await?;
    Ok(())
}

async fn matched_items(
    transaction: &DatabaseTransaction,
    object_id: Uuid,
) -> Result<HashSet<CatalogItemId>, DbErr> {
    let query = Query::select()
        .column(Alias::new("candidate_catalog_item_id"))
        .from(Alias::new("identity_matches"))
        .and_where(Expr::col(Alias::new("storage_object_id")).eq(object_id))
        .and_where(Expr::col(Alias::new("state")).eq("Matched"))
        .to_owned();
    transaction
        .query_all(transaction.get_database_backend().build(&query))
        .await?
        .into_iter()
        .map(|row| {
            row.try_get::<Uuid>("", "candidate_catalog_item_id")
                .map(CatalogItemId::from_uuid)
        })
        .collect()
}

async fn affected_items(
    transaction: &DatabaseTransaction,
    object_id: Uuid,
    matched: &HashSet<CatalogItemId>,
) -> Result<HashSet<CatalogItemId>, DbErr> {
    let mut items = matched.clone();
    let location = Alias::new("relink_location");
    let source = Alias::new("relink_source");
    let query = Query::select()
        .expr_as(
            Expr::col((source.clone(), Alias::new("catalog_item_id"))),
            Alias::new("catalog_item_id"),
        )
        .from_as(Alias::new("media_locations"), location.clone())
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("media_sources"),
            source.clone(),
            Expr::col((source, Alias::new("id")))
                .equals((location.clone(), Alias::new("media_source_id"))),
        )
        .and_where(Expr::col((location, Alias::new("storage_object_id"))).eq(object_id))
        .to_owned();
    for row in transaction
        .query_all(transaction.get_database_backend().build(&query))
        .await?
    {
        items.insert(CatalogItemId::from_uuid(
            row.try_get("", "catalog_item_id")?,
        ));
    }
    Ok(items)
}

async fn ensure_no_identity_conflict(
    transaction: &DatabaseTransaction,
    replacement_id: Uuid,
    allowed: &HashSet<CatalogItemId>,
) -> Result<(), StorageRelinkRepositoryError> {
    let existing = matched_items(transaction, replacement_id).await?;
    if existing.iter().any(|item| !allowed.contains(item)) {
        return Err(StorageRelinkRepositoryError::IdentityConflict);
    }
    Ok(())
}

async fn invalidate_items(
    transaction: &DatabaseTransaction,
    items: &HashSet<CatalogItemId>,
) -> Result<(), DbErr> {
    for item in items {
        let row = Query::select()
            .column(Alias::new("item_type"))
            .from(Alias::new("catalog_items"))
            .and_where(Expr::col(Alias::new("id")).eq(item.as_uuid()))
            .limit(1)
            .to_owned();
        let Some(row) = transaction
            .query_one(transaction.get_database_backend().build(&row))
            .await?
        else {
            continue;
        };
        let item_type: String = row.try_get("", "item_type")?;
        let mut update = Query::update();
        update.table(Alias::new("catalog_items"));
        if item_type == "Series" {
            update
                .value(
                    Alias::new("structure_expansion_revision"),
                    Expr::col(Alias::new("structure_expansion_revision")).add(1),
                )
                .value(Alias::new("structure_state"), "NotExpanded");
        } else {
            update
                .value(
                    Alias::new("source_index_revision"),
                    Expr::col(Alias::new("source_index_revision")).add(1),
                )
                .value(Alias::new("source_state"), "NotIndexed");
        }
        update.and_where(Expr::col(Alias::new("id")).eq(item.as_uuid()));
        transaction
            .execute(transaction.get_database_backend().build(&update))
            .await?;
    }
    Ok(())
}

async fn finish<T>(
    transaction: DatabaseTransaction,
    result: Result<T, StorageRelinkRepositoryError>,
) -> Result<T, StorageRelinkRepositoryError> {
    match result {
        Ok(value) => {
            transaction.commit().await?;
            Ok(value)
        }
        Err(original) => match transaction.rollback().await {
            Ok(()) => Err(original),
            Err(rollback) => Err(StorageRelinkRepositoryError::RollbackFailed {
                original: original.to_string(),
                rollback,
            }),
        },
    }
}
