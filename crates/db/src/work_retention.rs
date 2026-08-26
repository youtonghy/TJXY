use chrono::{DateTime, Duration, Utc};
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbErr, TransactionTrait,
    sea_query::{Alias, Cond, Expr, JoinType, Order, Query},
};
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

const MAX_LEASE_OWNER_CHARS: usize = 128;
const TERMINAL_STATES: [&str; 2] = ["Completed", "Failed"];
const ACTIVE_STATES: [&str; 2] = ["Pending", "Running"];

pub struct WorkRetentionRepository<'connection> {
    database: &'connection DatabaseConnection,
}

impl<'connection> WorkRetentionRepository<'connection> {
    #[must_use]
    pub const fn new(database: &'connection DatabaseConnection) -> Self {
        Self { database }
    }

    /// Processes at most one task whose forward retention period has elapsed.
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
        let claim = claim_next(&transaction, lease_owner, now, cutoff, lease_expires_at).await;
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
        let result = process_claim(&transaction, &claimed, now).await;
        finish(transaction, result).await
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkRetentionRun {
    Idle,
    EnrolledLegacy { count: u64 },
    Deleted { job_id: Uuid },
    CompactedPublication { job_id: Uuid },
    Deferred { job_id: Uuid },
}

async fn enroll_legacy(
    transaction: &DatabaseTransaction,
    now: DateTime<Utc>,
    cutoff: DateTime<Utc>,
) -> Result<u64, WorkRetentionError> {
    const ENROLL_LIMIT: u64 = 1_000;
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
                    .limit(ENROLL_LIMIT),
            ),
        )
        .await?;
    let mut enrolled = 0_u64;
    for row in rows {
        let job_id: Uuid = row.try_get("", "id")?;
        let terminal_at = row
            .try_get::<Option<DateTime<Utc>>>("", "completed_at")?
            .or(row.try_get::<Option<DateTime<Utc>>>("", "created_at")?)
            .unwrap_or(now);
        let conflict = if backend == sea_orm::DbBackend::MySql {
            sea_orm::sea_query::OnConflict::new()
                .update_column(Alias::new("job_id"))
                .to_owned()
        } else {
            sea_orm::sea_query::OnConflict::new()
                .do_nothing()
                .to_owned()
        };
        enrolled += transaction
            .execute(
                backend.build(
                    Query::insert()
                        .into_table(Alias::new("work_job_retention_queue"))
                        .columns([
                            Alias::new("job_id"),
                            Alias::new("terminal_at"),
                            Alias::new("attempt_count"),
                        ])
                        .values_panic([job_id.into(), terminal_at.into(), 0_i32.into()])
                        .on_conflict(conflict),
                ),
            )
            .await?
            .rows_affected();
    }
    Ok(enrolled)
}

struct RetentionClaim {
    job_id: Uuid,
    lease_token: String,
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

async fn claim_next(
    transaction: &DatabaseTransaction,
    lease_owner: &str,
    now: DateTime<Utc>,
    cutoff: DateTime<Utc>,
    lease_expires_at: DateTime<Utc>,
) -> Result<Option<RetentionClaim>, WorkRetentionError> {
    let backend = transaction.get_database_backend();
    let condition = claimable(now);
    let Some(row) = transaction
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("job_id"))
                    .from(Alias::new("work_job_retention_queue"))
                    .and_where(Expr::col(Alias::new("terminal_at")).lte(cutoff))
                    .cond_where(condition.clone())
                    .order_by(Alias::new("terminal_at"), Order::Asc)
                    .limit(1),
            ),
        )
        .await?
    else {
        return Ok(None);
    };
    let job_id: Uuid = row.try_get("", "job_id")?;
    let lease_token = format!("{lease_owner}:{}", Uuid::new_v4());
    let update = Query::update()
        .table(Alias::new("work_job_retention_queue"))
        .value(Alias::new("lease_owner"), &lease_token)
        .value(Alias::new("lease_expires_at"), lease_expires_at)
        .and_where(Expr::col(Alias::new("job_id")).eq(job_id))
        .and_where(Expr::col(Alias::new("terminal_at")).lte(cutoff))
        .cond_where(condition)
        .to_owned();
    if transaction
        .execute(backend.build(&update))
        .await?
        .rows_affected()
        != 1
    {
        return Ok(None);
    }
    Ok(Some(RetentionClaim {
        job_id,
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

async fn process_claim(
    transaction: &DatabaseTransaction,
    claimed: &RetentionClaim,
    now: DateTime<Utc>,
) -> Result<WorkRetentionRun, WorkRetentionError> {
    ensure_live_claim(transaction, claimed, now).await?;
    let backend = transaction.get_database_backend();
    let Some(job) = transaction
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("state"))
                    .from(Alias::new("work_jobs"))
                    .and_where(Expr::col(Alias::new("id")).eq(claimed.job_id)),
            ),
        )
        .await?
    else {
        delete_queue_claim(transaction, claimed, now).await?;
        return Ok(WorkRetentionRun::Deleted {
            job_id: claimed.job_id,
        });
    };
    let state: String = job.try_get("", "state")?;
    if !TERMINAL_STATES.contains(&state.as_str())
        || has_active_dependency(transaction, claimed.job_id).await?
        || has_recovery_cursor(transaction, claimed.job_id).await?
        || has_active_full_scan_parent(transaction, claimed.job_id).await?
    {
        defer_claim(transaction, claimed, now).await?;
        return Ok(WorkRetentionRun::Deferred {
            job_id: claimed.job_id,
        });
    }

    transaction
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("work_jobs"))
                    .value(Alias::new("required_sync_job_id"), Option::<Uuid>::None)
                    .and_where(Expr::col(Alias::new("required_sync_job_id")).eq(claimed.job_id))
                    .and_where(Expr::col(Alias::new("state")).is_in(TERMINAL_STATES)),
            ),
        )
        .await?;
    for table in ["work_staging_rows", "storage_sync_pages", "work_results"] {
        transaction
            .execute(
                backend.build(
                    Query::delete()
                        .from_table(Alias::new(table))
                        .and_where(Expr::col(Alias::new("job_id")).eq(claimed.job_id)),
                ),
            )
            .await?;
    }

    if has_catalog_publication(transaction, claimed.job_id).await? {
        delete_queue_claim(transaction, claimed, now).await?;
        return Ok(WorkRetentionRun::CompactedPublication {
            job_id: claimed.job_id,
        });
    }
    let deleted = transaction
        .execute(
            backend.build(
                Query::delete()
                    .from_table(Alias::new("work_jobs"))
                    .and_where(Expr::col(Alias::new("id")).eq(claimed.job_id))
                    .and_where(Expr::col(Alias::new("state")).is_in(TERMINAL_STATES)),
            ),
        )
        .await?
        .rows_affected();
    if deleted != 1 {
        return Err(WorkRetentionError::LostLease);
    }
    Ok(WorkRetentionRun::Deleted {
        job_id: claimed.job_id,
    })
}

async fn ensure_live_claim(
    transaction: &DatabaseTransaction,
    claimed: &RetentionClaim,
    now: DateTime<Utc>,
) -> Result<(), WorkRetentionError> {
    let backend = transaction.get_database_backend();
    let live = Query::select()
        .expr(Expr::val(1_i32))
        .from(Alias::new("work_job_retention_queue"))
        .and_where(Expr::col(Alias::new("job_id")).eq(claimed.job_id))
        .and_where(Expr::col(Alias::new("lease_owner")).eq(&claimed.lease_token))
        .and_where(Expr::col(Alias::new("lease_expires_at")).gt(now))
        .limit(1)
        .to_owned();
    if transaction.query_one(backend.build(&live)).await?.is_none() {
        return Err(WorkRetentionError::LostLease);
    }
    Ok(())
}

async fn has_active_dependency(
    transaction: &DatabaseTransaction,
    job_id: Uuid,
) -> Result<bool, DbErr> {
    exists(
        transaction,
        Query::select()
            .expr(Expr::val(1_i32))
            .from(Alias::new("work_jobs"))
            .and_where(Expr::col(Alias::new("required_sync_job_id")).eq(job_id))
            .and_where(Expr::col(Alias::new("state")).is_in(ACTIVE_STATES))
            .limit(1)
            .to_owned(),
    )
    .await
}

async fn has_recovery_cursor(
    transaction: &DatabaseTransaction,
    job_id: Uuid,
) -> Result<bool, DbErr> {
    exists(
        transaction,
        Query::select()
            .expr(Expr::val(1_i32))
            .from(Alias::new("storage_sync_cursors"))
            .and_where(Expr::col(Alias::new("recovery_job_id")).eq(job_id))
            .limit(1)
            .to_owned(),
    )
    .await
}

async fn has_catalog_publication(
    transaction: &DatabaseTransaction,
    job_id: Uuid,
) -> Result<bool, DbErr> {
    exists(
        transaction,
        Query::select()
            .expr(Expr::val(1_i32))
            .from(Alias::new("catalog_publications"))
            .and_where(Expr::col(Alias::new("job_id")).eq(job_id))
            .limit(1)
            .to_owned(),
    )
    .await
}

async fn has_active_full_scan_parent(
    transaction: &DatabaseTransaction,
    job_id: Uuid,
) -> Result<bool, DbErr> {
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
    let backend = transaction.get_database_backend();
    for row in transaction.query_all(backend.build(&query)).await? {
        let payload: Value = row.try_get("", "payload")?;
        if payload
            .get("job_id")
            .and_then(Value::as_str)
            .and_then(|value| Uuid::parse_str(value).ok())
            == Some(job_id)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

async fn exists(
    transaction: &DatabaseTransaction,
    query: sea_orm::sea_query::SelectStatement,
) -> Result<bool, DbErr> {
    let backend = transaction.get_database_backend();
    Ok(transaction
        .query_one(backend.build(&query))
        .await?
        .is_some())
}

async fn defer_claim(
    transaction: &DatabaseTransaction,
    claimed: &RetentionClaim,
    now: DateTime<Utc>,
) -> Result<(), WorkRetentionError> {
    let available_at = now
        .checked_add_signed(Duration::hours(1))
        .ok_or(WorkRetentionError::TimestampOverflow)?;
    update_queue_claim(
        transaction,
        claimed,
        now,
        Some(available_at),
        "dependency active",
    )
    .await
}

async fn delete_queue_claim(
    transaction: &DatabaseTransaction,
    claimed: &RetentionClaim,
    now: DateTime<Utc>,
) -> Result<(), WorkRetentionError> {
    let backend = transaction.get_database_backend();
    let deleted = transaction
        .execute(
            backend.build(
                Query::delete()
                    .from_table(Alias::new("work_job_retention_queue"))
                    .and_where(Expr::col(Alias::new("job_id")).eq(claimed.job_id))
                    .and_where(Expr::col(Alias::new("lease_owner")).eq(&claimed.lease_token))
                    .and_where(Expr::col(Alias::new("lease_expires_at")).gt(now)),
            ),
        )
        .await?
        .rows_affected();
    if deleted != 1 {
        return Err(WorkRetentionError::LostLease);
    }
    Ok(())
}

async fn update_queue_claim(
    transaction: &DatabaseTransaction,
    claimed: &RetentionClaim,
    now: DateTime<Utc>,
    available_at: Option<DateTime<Utc>>,
    last_error: &str,
) -> Result<(), WorkRetentionError> {
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
                    .value(Alias::new("last_error"), last_error)
                    .and_where(Expr::col(Alias::new("job_id")).eq(claimed.job_id))
                    .and_where(Expr::col(Alias::new("lease_owner")).eq(&claimed.lease_token))
                    .and_where(Expr::col(Alias::new("lease_expires_at")).gt(now)),
            ),
        )
        .await?
        .rows_affected();
    if updated != 1 {
        return Err(WorkRetentionError::LostLease);
    }
    Ok(())
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
