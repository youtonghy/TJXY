use chrono::{DateTime, Duration, Utc};
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbErr, QueryResult, TransactionTrait,
    sea_query::{Alias, Cond, Expr, Order, Query},
};
use thiserror::Error;
use uuid::Uuid;

const STATE_PENDING: &str = "Pending";
const STATE_PROCESSING: &str = "Processing";
const STATE_PROCESSED: &str = "Processed";
const MAX_LEASE_OWNER_CHARS: usize = 128;
const MAX_ERROR_CHARS: usize = 256;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClaimedCacheInvalidation {
    id: Uuid,
    generation: i64,
    attempt_count: i32,
    lease_token: String,
}

impl ClaimedCacheInvalidation {
    #[must_use]
    pub const fn generation(&self) -> i64 {
        self.generation
    }

    #[must_use]
    pub const fn stale_generation(&self) -> i64 {
        self.generation - 1
    }

    #[must_use]
    pub const fn attempt_count(&self) -> i32 {
        self.attempt_count
    }
}

pub trait CacheInvalidationClock: Clone + Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CacheInvalidationSystemClock;

impl CacheInvalidationClock for CacheInvalidationSystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// Advances the catalog generation and records its invalidation in the same transaction.
///
/// # Errors
///
/// Returns a database or invariant failure without committing the caller's transaction.
pub async fn advance_catalog_generation(transaction: &DatabaseTransaction) -> Result<i64, DbErr> {
    CacheInvalidationRepository::<CacheInvalidationSystemClock>::advance_generation(transaction)
        .await
}

pub struct CacheInvalidationRepository<'connection, Clock = CacheInvalidationSystemClock> {
    database: &'connection DatabaseConnection,
    clock: Clock,
}

impl<'connection> CacheInvalidationRepository<'connection, CacheInvalidationSystemClock> {
    #[must_use]
    pub const fn new(database: &'connection DatabaseConnection) -> Self {
        Self {
            database,
            clock: CacheInvalidationSystemClock,
        }
    }
}

impl<'connection, Clock> CacheInvalidationRepository<'connection, Clock>
where
    Clock: CacheInvalidationClock,
{
    #[must_use]
    pub const fn with_clock(database: &'connection DatabaseConnection, clock: Clock) -> Self {
        Self { database, clock }
    }

    async fn advance_generation(transaction: &DatabaseTransaction) -> Result<i64, DbErr> {
        let backend = transaction.get_database_backend();
        let update = Query::update()
            .table(Alias::new("catalog_state"))
            .value(
                Alias::new("generation"),
                Expr::col(Alias::new("generation")).add(1),
            )
            .and_where(Expr::col(Alias::new("id")).eq(1_i32))
            .to_owned();
        if transaction
            .execute(backend.build(&update))
            .await?
            .rows_affected()
            != 1
        {
            return Err(DbErr::Custom(
                "catalog generation row is missing".to_owned(),
            ));
        }
        let generation = transaction
            .query_one(
                backend.build(
                    Query::select()
                        .column(Alias::new("generation"))
                        .from(Alias::new("catalog_state"))
                        .and_where(Expr::col(Alias::new("id")).eq(1_i32)),
                ),
            )
            .await?
            .ok_or_else(|| DbErr::Custom("catalog generation row is missing".to_owned()))?
            .try_get::<i64>("", "generation")?;
        transaction
            .execute(
                backend.build(
                    Query::insert()
                        .into_table(Alias::new("cache_invalidation_outbox"))
                        .columns([
                            Alias::new("id"),
                            Alias::new("generation"),
                            Alias::new("state"),
                            Alias::new("attempt_count"),
                            Alias::new("created_at"),
                        ])
                        .values_panic([
                            Uuid::new_v4().into(),
                            generation.into(),
                            STATE_PENDING.into(),
                            0_i32.into(),
                            Utc::now().into(),
                        ]),
                ),
            )
            .await?;
        Ok(generation)
    }

    /// Claims the oldest available invalidation with a fenced lease.
    ///
    /// # Errors
    ///
    /// Returns validation, timestamp, database, or rollback errors.
    pub async fn claim_next(
        &self,
        lease_owner: &str,
        lease_duration: Duration,
    ) -> Result<Option<ClaimedCacheInvalidation>, CacheInvalidationRepositoryError> {
        validate_lease(lease_owner, lease_duration)?;
        let now = self.clock.now();
        let expires = now
            .checked_add_signed(lease_duration)
            .ok_or(CacheInvalidationRepositoryError::TimestampOverflow)?;
        let transaction = self.database.begin().await?;
        let token = format!("{lease_owner}:{}", Uuid::new_v4());
        let result = claim(&transaction, &token, now, expires).await;
        finish(transaction, result).await
    }

    /// Completes an invalidation while its lease is current.
    ///
    /// # Errors
    ///
    /// Returns [`CacheInvalidationRepositoryError::LostLease`] or a database failure.
    pub async fn complete(
        &self,
        claimed: &ClaimedCacheInvalidation,
    ) -> Result<(), CacheInvalidationRepositoryError> {
        let transaction = self.database.begin().await?;
        let result = update_claim(
            &transaction,
            claimed,
            self.clock.now(),
            ClaimUpdate::Complete,
        )
        .await;
        finish(transaction, result).await
    }

    /// Releases an incomplete bounded batch without recording a failed attempt.
    ///
    /// # Errors
    ///
    /// Returns [`CacheInvalidationRepositoryError::LostLease`] or a database failure.
    pub async fn release(
        &self,
        claimed: &ClaimedCacheInvalidation,
    ) -> Result<(), CacheInvalidationRepositoryError> {
        let transaction = self.database.begin().await?;
        let result = update_claim(
            &transaction,
            claimed,
            self.clock.now(),
            ClaimUpdate::Release,
        )
        .await;
        finish(transaction, result).await
    }

    /// Releases a failed invalidation for retry after a bounded delay.
    ///
    /// # Errors
    ///
    /// Returns validation, lost lease, timestamp, database, or rollback errors.
    pub async fn fail(
        &self,
        claimed: &ClaimedCacheInvalidation,
        backoff: Duration,
        error: &str,
    ) -> Result<(), CacheInvalidationRepositoryError> {
        if backoff < Duration::zero() {
            return Err(CacheInvalidationRepositoryError::InvalidBackoff);
        }
        if error.trim().is_empty() || error.chars().count() > MAX_ERROR_CHARS {
            return Err(CacheInvalidationRepositoryError::InvalidError);
        }
        let now = self.clock.now();
        let available_at = now
            .checked_add_signed(backoff)
            .ok_or(CacheInvalidationRepositoryError::TimestampOverflow)?;
        let transaction = self.database.begin().await?;
        let result = update_claim(
            &transaction,
            claimed,
            now,
            ClaimUpdate::Fail {
                available_at,
                error,
            },
        )
        .await;
        finish(transaction, result).await
    }
}

fn validate_lease(owner: &str, duration: Duration) -> Result<(), CacheInvalidationRepositoryError> {
    if owner.trim().is_empty() {
        return Err(CacheInvalidationRepositoryError::EmptyLeaseOwner);
    }
    if owner.chars().count() > MAX_LEASE_OWNER_CHARS {
        return Err(CacheInvalidationRepositoryError::LeaseOwnerTooLong);
    }
    if duration <= Duration::zero() {
        return Err(CacheInvalidationRepositoryError::InvalidLeaseDuration);
    }
    Ok(())
}

async fn claim(
    transaction: &DatabaseTransaction,
    token: &str,
    now: DateTime<Utc>,
    expires: DateTime<Utc>,
) -> Result<Option<ClaimedCacheInvalidation>, CacheInvalidationRepositoryError> {
    let condition = claimable(now);
    let backend = transaction.get_database_backend();
    let Some(row) = transaction
        .query_one(
            backend.build(
                Query::select()
                    .columns([
                        Alias::new("id"),
                        Alias::new("generation"),
                        Alias::new("attempt_count"),
                    ])
                    .from(Alias::new("cache_invalidation_outbox"))
                    .cond_where(condition.clone())
                    .order_by(Alias::new("generation"), Order::Asc)
                    .limit(1),
            ),
        )
        .await?
    else {
        return Ok(None);
    };
    let claimed = claimed_from_row(&row, token.to_owned())?;
    let update = Query::update()
        .table(Alias::new("cache_invalidation_outbox"))
        .value(Alias::new("state"), STATE_PROCESSING)
        .value(Alias::new("lease_owner"), token)
        .value(Alias::new("lease_expires_at"), expires)
        .and_where(Expr::col(Alias::new("id")).eq(claimed.id))
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
    Ok(Some(claimed))
}

fn claimable(now: DateTime<Utc>) -> Cond {
    Cond::any()
        .add(
            Cond::all()
                .add(Expr::col(Alias::new("state")).eq(STATE_PENDING))
                .add(
                    Cond::any()
                        .add(Expr::col(Alias::new("available_at")).is_null())
                        .add(Expr::col(Alias::new("available_at")).lte(now)),
                ),
        )
        .add(
            Cond::all()
                .add(Expr::col(Alias::new("state")).eq(STATE_PROCESSING))
                .add(Expr::col(Alias::new("lease_expires_at")).lte(now)),
        )
}

enum ClaimUpdate<'a> {
    Complete,
    Release,
    Fail {
        available_at: DateTime<Utc>,
        error: &'a str,
    },
}

async fn update_claim(
    transaction: &DatabaseTransaction,
    claimed: &ClaimedCacheInvalidation,
    now: DateTime<Utc>,
    update: ClaimUpdate<'_>,
) -> Result<(), CacheInvalidationRepositoryError> {
    let mut statement = Query::update();
    statement
        .table(Alias::new("cache_invalidation_outbox"))
        .value(Alias::new("lease_owner"), Option::<String>::None)
        .value(
            Alias::new("lease_expires_at"),
            Option::<DateTime<Utc>>::None,
        );
    match update {
        ClaimUpdate::Complete => {
            statement
                .value(Alias::new("state"), STATE_PROCESSED)
                .value(Alias::new("processed_at"), now);
        }
        ClaimUpdate::Release => {
            statement
                .value(Alias::new("state"), STATE_PENDING)
                .value(Alias::new("available_at"), Option::<DateTime<Utc>>::None)
                .value(Alias::new("last_error"), Option::<String>::None);
        }
        ClaimUpdate::Fail {
            available_at,
            error,
        } => {
            statement
                .value(Alias::new("state"), STATE_PENDING)
                .value(
                    Alias::new("attempt_count"),
                    Expr::col(Alias::new("attempt_count")).add(1),
                )
                .value(Alias::new("available_at"), available_at)
                .value(Alias::new("last_error"), error);
        }
    }
    statement
        .and_where(Expr::col(Alias::new("id")).eq(claimed.id))
        .and_where(Expr::col(Alias::new("state")).eq(STATE_PROCESSING))
        .and_where(Expr::col(Alias::new("lease_owner")).eq(&claimed.lease_token))
        .and_where(Expr::col(Alias::new("lease_expires_at")).gt(now));
    let backend = transaction.get_database_backend();
    if transaction
        .execute(backend.build(&statement))
        .await?
        .rows_affected()
        != 1
    {
        return Err(CacheInvalidationRepositoryError::LostLease);
    }
    Ok(())
}

fn claimed_from_row(
    row: &QueryResult,
    lease_token: String,
) -> Result<ClaimedCacheInvalidation, DbErr> {
    Ok(ClaimedCacheInvalidation {
        id: row.try_get("", "id")?,
        generation: row.try_get("", "generation")?,
        attempt_count: row.try_get("", "attempt_count")?,
        lease_token,
    })
}

async fn finish<T>(
    transaction: sea_orm::DatabaseTransaction,
    result: Result<T, CacheInvalidationRepositoryError>,
) -> Result<T, CacheInvalidationRepositoryError> {
    match result {
        Ok(value) => {
            transaction.commit().await?;
            Ok(value)
        }
        Err(original) => match transaction.rollback().await {
            Ok(()) => Err(original),
            Err(rollback) => Err(CacheInvalidationRepositoryError::RollbackFailed {
                original: original.to_string(),
                rollback,
            }),
        },
    }
}

#[derive(Debug, Error)]
pub enum CacheInvalidationRepositoryError {
    #[error("lease owner must not be empty")]
    EmptyLeaseOwner,
    #[error("lease owner must not exceed 128 characters")]
    LeaseOwnerTooLong,
    #[error("lease duration must be positive")]
    InvalidLeaseDuration,
    #[error("retry backoff must not be negative")]
    InvalidBackoff,
    #[error("cache invalidation error must contain 1 to 256 characters")]
    InvalidError,
    #[error("lease or retry timestamp is outside the supported range")]
    TimestampOverflow,
    #[error("cache invalidation lease is expired or no longer owned by this claim")]
    LostLease,
    #[error("database operation failed: {0}")]
    Database(#[from] DbErr),
    #[error("rollback failed after {original}: {rollback}")]
    RollbackFailed { original: String, rollback: DbErr },
}
