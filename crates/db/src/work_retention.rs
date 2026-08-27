use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Duration, Utc};
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbErr, TransactionTrait,
    sea_query::{Alias, Cond, Expr, JoinType, Order, Query},
};
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

const MAX_LEASE_OWNER_CHARS: usize = 128;
const RETENTION_BATCH_SIZE: u64 = 100;
const LEGACY_ENROLL_LIMIT: u64 = 1_000;
const LEGACY_ENROLL_INSERT_BATCH_SIZE: usize = 200;
const TERMINAL_STATES: [&str; 2] = ["Completed", "Failed"];
const ACTIVE_STATES: [&str; 2] = ["Pending", "Running"];
const DEFERRED_REASON: &str = "dependency active";

pub struct WorkRetentionRepository<'connection> {
    database: &'connection DatabaseConnection,
}

impl<'connection> WorkRetentionRepository<'connection> {
    #[must_use]
    pub const fn new(database: &'connection DatabaseConnection) -> Self {
        Self { database }
    }

    /// Processes one bounded batch whose forward retention period has elapsed.
    ///
    /// # Errors
    ///
    /// Returns validation, timestamp, database, or rollback failures.
    pub async fn run_once(
        &self,
        lease_owner: &str,
        retention: Duration,
        lease_duration: Duration,
    ) -> Result<WorkRetentionRun, WorkRetentionError> {
        validate(lease_owner, retention, lease_duration)?;
        let now = Utc::now();
        let cutoff = now
            .checked_sub_signed(retention)
            .ok_or(WorkRetentionError::TimestampOverflow)?;
        let lease_expires_at = now
            .checked_add_signed(lease_duration)
            .ok_or(WorkRetentionError::TimestampOverflow)?;
        let transaction = self.database.begin().await?;
        let claim =
            claim_next_batch(&transaction, lease_owner, now, cutoff, lease_expires_at).await;
        let claimed = finish(transaction, claim).await?;
        let Some(claimed) = claimed else {
            let transaction = self.database.begin().await?;
            let enrollment = enroll_legacy(&transaction, now, cutoff).await;
            let count = finish(transaction, enrollment).await?;
            if count != 0 {
                return Ok(WorkRetentionRun::EnrolledLegacy { count });
            }
            return Ok(WorkRetentionRun::Idle);
        };

        let transaction = self.database.begin().await?;
        let result = process_batch(&transaction, &claimed, now).await;
        finish(transaction, result).await
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkRetentionRun {
    Idle,
    EnrolledLegacy {
        count: u64,
    },
    Processed {
        deleted: u64,
        compacted: u64,
        deferred: u64,
    },
}

struct RetentionClaimBatch {
    job_ids: Vec<Uuid>,
    lease_token: String,
}

struct RetentionClassification {
    compacted: Vec<Uuid>,
    deferred: Vec<Uuid>,
    deleted: Vec<Uuid>,
    missing: Vec<Uuid>,
}

async fn enroll_legacy(
    transaction: &DatabaseTransaction,
    now: DateTime<Utc>,
    cutoff: DateTime<Utc>,
) -> Result<u64, WorkRetentionError> {
    let backend = transaction.get_database_backend();
    let entries = legacy_entries(transaction, now, cutoff).await?;
    let conflict = if backend == sea_orm::DbBackend::MySql {
        sea_orm::sea_query::OnConflict::new()
            .update_column(Alias::new("job_id"))
            .to_owned()
    } else {
        sea_orm::sea_query::OnConflict::new()
            .do_nothing()
            .to_owned()
    };
    let mut enrolled = 0_u64;
    for entries in entries.chunks(LEGACY_ENROLL_INSERT_BATCH_SIZE) {
        let mut insert = Query::insert();
        insert
            .into_table(Alias::new("work_job_retention_queue"))
            .columns([
                Alias::new("job_id"),
                Alias::new("terminal_at"),
                Alias::new("attempt_count"),
            ])
            .on_conflict(conflict.clone());
        for (job_id, terminal_at) in entries {
            insert.values_panic([
                job_id.to_owned().into(),
                terminal_at.to_owned().into(),
                0_i32.into(),
            ]);
        }
        enrolled += transaction
            .execute(backend.build(&insert))
            .await?
            .rows_affected();
    }
    Ok(enrolled)
}

async fn legacy_entries(
    transaction: &DatabaseTransaction,
    now: DateTime<Utc>,
    cutoff: DateTime<Utc>,
) -> Result<Vec<(Uuid, DateTime<Utc>)>, WorkRetentionError> {
    let backend = transaction.get_database_backend();
    let job = Alias::new("legacy_job");
    let queue = Alias::new("legacy_queue");
    let publication = Alias::new("legacy_publication");
    let rows = transaction
        .query_all(
            backend.build(
                Query::select()
                    .columns([
                        (job.clone(), Alias::new("id")),
                        (job.clone(), Alias::new("completed_at")),
                        (job.clone(), Alias::new("created_at")),
                    ])
                    .from_as(Alias::new("work_jobs"), job.clone())
                    .join_as(
                        JoinType::LeftJoin,
                        Alias::new("work_job_retention_queue"),
                        queue.clone(),
                        Expr::col((queue.clone(), Alias::new("job_id")))
                            .equals((job.clone(), Alias::new("id"))),
                    )
                    .join_as(
                        JoinType::LeftJoin,
                        Alias::new("catalog_publications"),
                        publication.clone(),
                        Expr::col((publication.clone(), Alias::new("job_id")))
                            .equals((job.clone(), Alias::new("id"))),
                    )
                    .and_where(Expr::col((job.clone(), Alias::new("state"))).is_in(TERMINAL_STATES))
                    .cond_where(
                        Cond::any()
                            .add(Expr::col((job.clone(), Alias::new("completed_at"))).lte(cutoff))
                            .add(
                                Cond::all()
                                    .add(
                                        Expr::col((job.clone(), Alias::new("completed_at")))
                                            .is_null(),
                                    )
                                    .add(
                                        Cond::any()
                                            .add(
                                                Expr::col((job.clone(), Alias::new("created_at")))
                                                    .lte(cutoff),
                                            )
                                            .add(
                                                Expr::col((job.clone(), Alias::new("created_at")))
                                                    .is_null(),
                                            ),
                                    ),
                            ),
                    )
                    .and_where(Expr::col((queue, Alias::new("job_id"))).is_null())
                    .and_where(Expr::col((publication, Alias::new("job_id"))).is_null())
                    .order_by((job.clone(), Alias::new("completed_at")), Order::Asc)
                    .order_by((job, Alias::new("id")), Order::Asc)
                    .limit(LEGACY_ENROLL_LIMIT),
            ),
        )
        .await?;
    let entries = rows
        .iter()
        .map(|row| {
            Ok((
                row.try_get::<Uuid>("", "id")?,
                row.try_get::<Option<DateTime<Utc>>>("", "completed_at")?
                    .or(row.try_get::<Option<DateTime<Utc>>>("", "created_at")?)
                    .unwrap_or(now),
            ))
        })
        .collect::<Result<Vec<_>, DbErr>>()?;
    Ok(entries)
}

fn validate(
    lease_owner: &str,
    retention: Duration,
    lease_duration: Duration,
) -> Result<(), WorkRetentionError> {
    if lease_owner.trim().is_empty() {
        return Err(WorkRetentionError::EmptyLeaseOwner);
    }
    if lease_owner.chars().count() > MAX_LEASE_OWNER_CHARS {
        return Err(WorkRetentionError::LeaseOwnerTooLong);
    }
    if retention <= Duration::zero() {
        return Err(WorkRetentionError::InvalidRetention);
    }
    if lease_duration <= Duration::zero() {
        return Err(WorkRetentionError::InvalidLeaseDuration);
    }
    Ok(())
}

async fn claim_next_batch(
    transaction: &DatabaseTransaction,
    lease_owner: &str,
    now: DateTime<Utc>,
    cutoff: DateTime<Utc>,
    lease_expires_at: DateTime<Utc>,
) -> Result<Option<RetentionClaimBatch>, WorkRetentionError> {
    let backend = transaction.get_database_backend();
    let condition = claimable(now);
    let rows = transaction
        .query_all(
            backend.build(
                Query::select()
                    .column(Alias::new("job_id"))
                    .from(Alias::new("work_job_retention_queue"))
                    .and_where(Expr::col(Alias::new("terminal_at")).lte(cutoff))
                    .cond_where(condition.clone())
                    .order_by(Alias::new("terminal_at"), Order::Asc)
                    .order_by(Alias::new("job_id"), Order::Asc)
                    .limit(RETENTION_BATCH_SIZE),
            ),
        )
        .await?;
    if rows.is_empty() {
        return Ok(None);
    }
    let lease_token = format!("{lease_owner}:{}", Uuid::new_v4());
    let candidate_ids = rows
        .iter()
        .map(|row| row.try_get("", "job_id"))
        .collect::<Result<Vec<Uuid>, DbErr>>()?;
    let update = Query::update()
        .table(Alias::new("work_job_retention_queue"))
        .value(Alias::new("lease_owner"), &lease_token)
        .value(Alias::new("lease_expires_at"), lease_expires_at)
        .and_where(Expr::col(Alias::new("job_id")).is_in(candidate_ids.iter().copied()))
        .and_where(Expr::col(Alias::new("terminal_at")).lte(cutoff))
        .cond_where(condition)
        .to_owned();
    transaction.execute(backend.build(&update)).await?;
    let job_ids = transaction
        .query_all(
            backend.build(
                Query::select()
                    .column(Alias::new("job_id"))
                    .from(Alias::new("work_job_retention_queue"))
                    .and_where(Expr::col(Alias::new("job_id")).is_in(candidate_ids))
                    .and_where(Expr::col(Alias::new("lease_owner")).eq(&lease_token))
                    .and_where(Expr::col(Alias::new("lease_expires_at")).gt(now)),
            ),
        )
        .await?
        .iter()
        .map(|row| row.try_get("", "job_id"))
        .collect::<Result<Vec<Uuid>, DbErr>>()?;
    Ok((!job_ids.is_empty()).then_some(RetentionClaimBatch {
        job_ids: sorted_ids(job_ids),
        lease_token,
    }))
}

fn claimable(now: DateTime<Utc>) -> Cond {
    Cond::all()
        .add(
            Cond::any()
                .add(Expr::col(Alias::new("available_at")).is_null())
                .add(Expr::col(Alias::new("available_at")).lte(now)),
        )
        .add(
            Cond::any()
                .add(Expr::col(Alias::new("lease_owner")).is_null())
                .add(Expr::col(Alias::new("lease_expires_at")).lte(now)),
        )
}

async fn process_batch(
    transaction: &DatabaseTransaction,
    claimed: &RetentionClaimBatch,
    now: DateTime<Utc>,
) -> Result<WorkRetentionRun, WorkRetentionError> {
    ensure_live_claims(transaction, claimed, now).await?;
    let classification = classify_claims(transaction, claimed).await?;
    let cleanup_ids = sorted_ids(
        classification
            .deleted
            .iter()
            .chain(&classification.compacted)
            .copied()
            .collect(),
    );
    if !classification.deferred.is_empty() {
        defer_claims(transaction, claimed, &classification.deferred, now).await?;
    }
    if !cleanup_ids.is_empty() {
        clear_terminal_dependencies(transaction, &cleanup_ids).await?;
        delete_child_rows(transaction, &cleanup_ids).await?;
    }
    if !classification.deleted.is_empty() {
        delete_jobs(transaction, &classification.deleted).await?;
    }
    let queue_ids = sorted_ids(
        classification
            .compacted
            .iter()
            .chain(&classification.missing)
            .copied()
            .collect(),
    );
    if !queue_ids.is_empty() {
        delete_queue_claims(transaction, claimed, &queue_ids, now).await?;
    }
    Ok(WorkRetentionRun::Processed {
        deleted: u64::try_from(classification.deleted.len() + classification.missing.len())
            .expect("retention batch size fits u64"),
        compacted: u64::try_from(classification.compacted.len())
            .expect("retention batch size fits u64"),
        deferred: u64::try_from(classification.deferred.len())
            .expect("retention batch size fits u64"),
    })
}

async fn ensure_live_claims(
    transaction: &DatabaseTransaction,
    claimed: &RetentionClaimBatch,
    now: DateTime<Utc>,
) -> Result<(), WorkRetentionError> {
    let backend = transaction.get_database_backend();
    let rows = transaction
        .query_all(
            backend.build(
                Query::select()
                    .column(Alias::new("job_id"))
                    .from(Alias::new("work_job_retention_queue"))
                    .and_where(
                        Expr::col(Alias::new("job_id")).is_in(claimed.job_ids.iter().copied()),
                    )
                    .and_where(Expr::col(Alias::new("lease_owner")).eq(&claimed.lease_token))
                    .and_where(Expr::col(Alias::new("lease_expires_at")).gt(now)),
            ),
        )
        .await?;
    if rows.len() != claimed.job_ids.len() {
        return Err(WorkRetentionError::LostLease);
    }
    Ok(())
}

async fn classify_claims(
    transaction: &DatabaseTransaction,
    claimed: &RetentionClaimBatch,
) -> Result<RetentionClassification, WorkRetentionError> {
    let backend = transaction.get_database_backend();
    let rows = transaction
        .query_all(
            backend.build(
                Query::select()
                    .columns([Alias::new("id"), Alias::new("state")])
                    .from(Alias::new("work_jobs"))
                    .and_where(Expr::col(Alias::new("id")).is_in(claimed.job_ids.iter().copied())),
            ),
        )
        .await?;
    let mut states = HashMap::with_capacity(rows.len());
    for row in rows {
        states.insert(
            row.try_get::<Uuid>("", "id")?,
            row.try_get::<String>("", "state")?,
        );
    }
    let existing_ids = states.keys().copied().collect::<Vec<_>>();
    let mut protected_ids = active_dependency_ids(transaction, &existing_ids).await?;
    protected_ids.extend(recovery_cursor_ids(transaction, &existing_ids).await?);
    protected_ids.extend(active_full_scan_child_ids(transaction, &existing_ids).await?);
    let publication_ids = catalog_publication_ids(transaction, &existing_ids).await?;
    let mut compacted_ids = Vec::new();
    let mut deferred_ids = Vec::new();
    let mut deleted_ids = Vec::new();
    let mut missing_ids = Vec::new();
    for job_id in &claimed.job_ids {
        let Some(state) = states.get(job_id) else {
            missing_ids.push(*job_id);
            continue;
        };
        if !TERMINAL_STATES.contains(&state.as_str()) || protected_ids.contains(job_id) {
            deferred_ids.push(*job_id);
        } else if publication_ids.contains(job_id) {
            compacted_ids.push(*job_id);
        } else {
            deleted_ids.push(*job_id);
        }
    }
    Ok(RetentionClassification {
        compacted: sorted_ids(compacted_ids),
        deferred: sorted_ids(deferred_ids),
        deleted: sorted_ids(deleted_ids),
        missing: sorted_ids(missing_ids),
    })
}

async fn active_dependency_ids(
    transaction: &DatabaseTransaction,
    job_ids: &[Uuid],
) -> Result<HashSet<Uuid>, DbErr> {
    selected_ids(
        transaction,
        Query::select()
            .column(Alias::new("required_sync_job_id"))
            .from(Alias::new("work_jobs"))
            .and_where(Expr::col(Alias::new("required_sync_job_id")).is_in(job_ids.iter().copied()))
            .and_where(Expr::col(Alias::new("state")).is_in(ACTIVE_STATES))
            .to_owned(),
        "required_sync_job_id",
    )
    .await
}

async fn recovery_cursor_ids(
    transaction: &DatabaseTransaction,
    job_ids: &[Uuid],
) -> Result<HashSet<Uuid>, DbErr> {
    selected_ids(
        transaction,
        Query::select()
            .column(Alias::new("recovery_job_id"))
            .from(Alias::new("storage_sync_cursors"))
            .and_where(Expr::col(Alias::new("recovery_job_id")).is_in(job_ids.iter().copied()))
            .to_owned(),
        "recovery_job_id",
    )
    .await
}

async fn catalog_publication_ids(
    transaction: &DatabaseTransaction,
    job_ids: &[Uuid],
) -> Result<HashSet<Uuid>, DbErr> {
    selected_ids(
        transaction,
        Query::select()
            .column(Alias::new("job_id"))
            .from(Alias::new("catalog_publications"))
            .and_where(Expr::col(Alias::new("job_id")).is_in(job_ids.iter().copied()))
            .to_owned(),
        "job_id",
    )
    .await
}

async fn active_full_scan_child_ids(
    transaction: &DatabaseTransaction,
    job_ids: &[Uuid],
) -> Result<HashSet<Uuid>, DbErr> {
    if job_ids.is_empty() {
        return Ok(HashSet::new());
    }
    let staging = Alias::new("retention_active_staging");
    let parent = Alias::new("retention_active_parent");
    let query = Query::select()
        .column((staging.clone(), Alias::new("payload")))
        .from_as(Alias::new("work_staging_rows"), staging.clone())
        .join_as(
            JoinType::InnerJoin,
            Alias::new("work_jobs"),
            parent.clone(),
            Expr::col((parent.clone(), Alias::new("id")))
                .equals((staging.clone(), Alias::new("job_id"))),
        )
        .and_where(Expr::col((parent.clone(), Alias::new("state"))).is_in(ACTIVE_STATES))
        .and_where(
            Expr::col((parent, Alias::new("task_kind")))
                .is_in(["FullMediaScan", "FullLibraryRootScan"]),
        )
        .to_owned();
    let requested = job_ids.iter().copied().collect::<HashSet<_>>();
    let backend = transaction.get_database_backend();
    let mut protected = HashSet::new();
    for row in transaction.query_all(backend.build(&query)).await? {
        let payload: Value = row.try_get("", "payload")?;
        if let Some(job_id) = payload
            .get("job_id")
            .and_then(Value::as_str)
            .and_then(|value| Uuid::parse_str(value).ok())
            .filter(|job_id| requested.contains(job_id))
        {
            protected.insert(job_id);
        }
    }
    Ok(protected)
}

async fn selected_ids(
    transaction: &DatabaseTransaction,
    query: sea_orm::sea_query::SelectStatement,
    column: &str,
) -> Result<HashSet<Uuid>, DbErr> {
    let backend = transaction.get_database_backend();
    transaction
        .query_all(backend.build(&query))
        .await?
        .iter()
        .map(|row| row.try_get("", column))
        .collect()
}

async fn defer_claims(
    transaction: &DatabaseTransaction,
    claimed: &RetentionClaimBatch,
    job_ids: &[Uuid],
    now: DateTime<Utc>,
) -> Result<(), WorkRetentionError> {
    let available_at = now
        .checked_add_signed(Duration::hours(1))
        .ok_or(WorkRetentionError::TimestampOverflow)?;
    let backend = transaction.get_database_backend();
    let updated = transaction
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("work_job_retention_queue"))
                    .value(Alias::new("lease_owner"), Option::<String>::None)
                    .value(
                        Alias::new("lease_expires_at"),
                        Option::<DateTime<Utc>>::None,
                    )
                    .value(Alias::new("available_at"), available_at)
                    .value(Alias::new("last_error"), DEFERRED_REASON)
                    .and_where(Expr::col(Alias::new("job_id")).is_in(job_ids.iter().copied()))
                    .and_where(Expr::col(Alias::new("lease_owner")).eq(&claimed.lease_token))
                    .and_where(Expr::col(Alias::new("lease_expires_at")).gt(now)),
            ),
        )
        .await?
        .rows_affected();
    ensure_affected(updated, job_ids.len())
}

async fn clear_terminal_dependencies(
    transaction: &DatabaseTransaction,
    job_ids: &[Uuid],
) -> Result<(), DbErr> {
    let backend = transaction.get_database_backend();
    transaction
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("work_jobs"))
                    .value(Alias::new("required_sync_job_id"), Option::<Uuid>::None)
                    .and_where(
                        Expr::col(Alias::new("required_sync_job_id"))
                            .is_in(job_ids.iter().copied()),
                    )
                    .and_where(Expr::col(Alias::new("state")).is_in(TERMINAL_STATES)),
            ),
        )
        .await?;
    Ok(())
}

async fn delete_child_rows(
    transaction: &DatabaseTransaction,
    job_ids: &[Uuid],
) -> Result<(), DbErr> {
    let backend = transaction.get_database_backend();
    for table in ["work_staging_rows", "storage_sync_pages", "work_results"] {
        transaction
            .execute(
                backend.build(
                    Query::delete()
                        .from_table(Alias::new(table))
                        .and_where(Expr::col(Alias::new("job_id")).is_in(job_ids.iter().copied())),
                ),
            )
            .await?;
    }
    Ok(())
}

async fn delete_jobs(
    transaction: &DatabaseTransaction,
    job_ids: &[Uuid],
) -> Result<(), WorkRetentionError> {
    let backend = transaction.get_database_backend();
    let deleted = transaction
        .execute(
            backend.build(
                Query::delete()
                    .from_table(Alias::new("work_jobs"))
                    .and_where(Expr::col(Alias::new("id")).is_in(job_ids.iter().copied()))
                    .and_where(Expr::col(Alias::new("state")).is_in(TERMINAL_STATES)),
            ),
        )
        .await?
        .rows_affected();
    ensure_affected(deleted, job_ids.len())
}

async fn delete_queue_claims(
    transaction: &DatabaseTransaction,
    claimed: &RetentionClaimBatch,
    job_ids: &[Uuid],
    now: DateTime<Utc>,
) -> Result<(), WorkRetentionError> {
    let backend = transaction.get_database_backend();
    let deleted = transaction
        .execute(
            backend.build(
                Query::delete()
                    .from_table(Alias::new("work_job_retention_queue"))
                    .and_where(Expr::col(Alias::new("job_id")).is_in(job_ids.iter().copied()))
                    .and_where(Expr::col(Alias::new("lease_owner")).eq(&claimed.lease_token))
                    .and_where(Expr::col(Alias::new("lease_expires_at")).gt(now)),
            ),
        )
        .await?
        .rows_affected();
    ensure_affected(deleted, job_ids.len())
}

fn ensure_affected(actual: u64, expected: usize) -> Result<(), WorkRetentionError> {
    (actual == u64::try_from(expected).expect("retention batch size fits u64"))
        .then_some(())
        .ok_or(WorkRetentionError::LostLease)
}

fn sorted_ids(mut job_ids: Vec<Uuid>) -> Vec<Uuid> {
    job_ids.sort_unstable();
    job_ids.dedup();
    job_ids
}

async fn finish<T>(
    transaction: DatabaseTransaction,
    result: Result<T, WorkRetentionError>,
) -> Result<T, WorkRetentionError> {
    match result {
        Ok(value) => {
            transaction.commit().await?;
            Ok(value)
        }
        Err(original) => match transaction.rollback().await {
            Ok(()) => Err(original),
            Err(rollback) => Err(WorkRetentionError::RollbackFailed {
                original: original.to_string(),
                rollback,
            }),
        },
    }
}

#[derive(Debug, Error)]
pub enum WorkRetentionError {
    #[error("lease owner must not be empty")]
    EmptyLeaseOwner,
    #[error("lease owner must not exceed 128 characters")]
    LeaseOwnerTooLong,
    #[error("work retention duration must be positive")]
    InvalidRetention,
    #[error("lease duration must be positive")]
    InvalidLeaseDuration,
    #[error("retention timestamp is outside the supported range")]
    TimestampOverflow,
    #[error("work retention lease is expired or no longer owned")]
    LostLease,
    #[error("database operation failed: {0}")]
    Database(#[from] DbErr),
    #[error("rollback failed after {original}: {rollback}")]
    RollbackFailed { original: String, rollback: DbErr },
}
