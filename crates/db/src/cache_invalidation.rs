use chrono::{DateTime, Duration, Utc};
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbErr, TransactionTrait,
    sea_query::{Alias, Cond, Expr, Query},
};
use thiserror::Error;
use uuid::Uuid;

const MAX_LEASE_OWNER_CHARS: usize = 128;
const MAX_ERROR_CHARS: usize = 256;
const STATE_ROW_ID: i32 = 1;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClaimedCacheInvalidation {
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

/// Advances the catalog generation in the same transaction as the catalog change.
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
    let backend = transaction.get_database_backend();
    let catalog_generation = transaction
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("generation"))
                    .from(Alias::new("catalog_state"))
                    .and_where(Expr::col(Alias::new("id")).eq(STATE_ROW_ID)),
            ),
        )
        .await?
        .ok_or_else(|| DbErr::Custom("catalog generation row is missing".to_owned()))?
        .try_get::<i64>("", "generation")?;
    let row = transaction
        .query_one(
            backend.build(
                Query::select()
                    .columns([
                        Alias::new("processed_generation"),
                        Alias::new("target_generation"),
                        Alias::new("attempt_count"),
                    ])
                    .from(Alias::new("cache_invalidation_state"))
                    .and_where(Expr::col(Alias::new("id")).eq(STATE_ROW_ID)),
            ),
        )
        .await?
        .ok_or_else(|| DbErr::Custom("cache invalidation state row is missing".to_owned()))?;
    let processed_generation = row.try_get::<i64>("", "processed_generation")?;
    let observed_target = row.try_get::<Option<i64>>("", "target_generation")?;
    let Some(generation) = observed_target
        .or_else(|| (catalog_generation > processed_generation).then_some(catalog_generation))
    else {
        return Ok(None);
    };
    let claimed = ClaimedCacheInvalidation {
        generation,
        attempt_count: row.try_get("", "attempt_count")?,
        lease_token: token.to_owned(),
    };
    let update = Query::update()
        .table(Alias::new("cache_invalidation_state"))
        .value(Alias::new("target_generation"), generation)
        .value(Alias::new("lease_owner"), token)
        .value(Alias::new("lease_expires_at"), expires)
        .value(Alias::new("updated_at"), now)
        .and_where(Expr::col(Alias::new("id")).eq(STATE_ROW_ID))
        .and_where(Expr::col(Alias::new("processed_generation")).eq(processed_generation))
        .cond_where(match observed_target {
            Some(target) => Cond::all().add(Expr::col(Alias::new("target_generation")).eq(target)),
            None => Cond::all().add(Expr::col(Alias::new("target_generation")).is_null()),
        })
        .cond_where(claimable(now))
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
        .table(Alias::new("cache_invalidation_state"))
        .value(Alias::new("lease_owner"), Option::<String>::None)
        .value(
            Alias::new("lease_expires_at"),
            Option::<DateTime<Utc>>::None,
        )
        .value(Alias::new("updated_at"), now);
    match update {
        ClaimUpdate::Complete => {
            statement
                .value(Alias::new("processed_generation"), claimed.generation)
                .value(Alias::new("target_generation"), Option::<i64>::None)
                .value(Alias::new("attempt_count"), 0_i32)
                .value(Alias::new("available_at"), Option::<DateTime<Utc>>::None)
                .value(Alias::new("last_error"), Option::<String>::None);
        }
        ClaimUpdate::Release => {
            statement
                .value(Alias::new("available_at"), Option::<DateTime<Utc>>::None)
                .value(Alias::new("last_error"), Option::<String>::None);
        }
        ClaimUpdate::Fail {
            available_at,
            error,
        } => {
            statement
                .value(
                    Alias::new("attempt_count"),
                    Expr::col(Alias::new("attempt_count")).add(1),
                )
                .value(Alias::new("available_at"), available_at)
                .value(Alias::new("last_error"), error);
        }
    }
    statement
        .and_where(Expr::col(Alias::new("id")).eq(STATE_ROW_ID))
        .and_where(Expr::col(Alias::new("target_generation")).eq(claimed.generation))
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
