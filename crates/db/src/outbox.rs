use chrono::{DateTime, Duration, Utc};
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbErr, QueryResult, TransactionTrait,
    sea_query::{Alias, Cond, Expr, Order, Query},
};
use serde_json::Value;
use thiserror::Error;
use tjxy_common::{StorageObjectRecordId, StorageRootId};
use uuid::Uuid;

const STATE_PENDING: &str = "Pending";
const STATE_PROCESSING: &str = "Processing";
const STATE_PROCESSED: &str = "Processed";
const MAX_LEASE_OWNER_CHARS: usize = 128;

#[derive(Clone, Debug, PartialEq)]
pub struct ClaimedOutboxEvent {
    id: Uuid,
    storage_root_id: StorageRootId,
    sync_revision: i64,
    event_type: String,
    storage_object_id: StorageObjectRecordId,
    payload_version: i32,
    payload: Value,
    attempt_count: i32,
    lease_token: String,
}

impl ClaimedOutboxEvent {
    #[must_use]
    pub const fn id(&self) -> Uuid {
        self.id
    }

    #[must_use]
    pub const fn storage_root_id(&self) -> StorageRootId {
        self.storage_root_id
    }

    #[must_use]
    pub const fn sync_revision(&self) -> i64 {
        self.sync_revision
    }

    #[must_use]
    pub fn event_type(&self) -> &str {
        &self.event_type
    }

    #[must_use]
    pub const fn storage_object_id(&self) -> StorageObjectRecordId {
        self.storage_object_id
    }

    #[must_use]
    pub const fn payload_version(&self) -> i32 {
        self.payload_version
    }

    #[must_use]
    pub const fn payload(&self) -> &Value {
        &self.payload
    }

    #[must_use]
    pub const fn attempt_count(&self) -> i32 {
        self.attempt_count
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OutboxCompletion {
    pub reconciled_sync_revision: i64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BackloggedStorageRoot {
    root_id: StorageRootId,
    expected_revision: i64,
}

impl BackloggedStorageRoot {
    #[must_use]
    pub const fn root_id(self) -> StorageRootId {
        self.root_id
    }

    #[must_use]
    pub const fn expected_revision(self) -> i64 {
        self.expected_revision
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutboxFailureReason {
    TransientProvider,
    ProjectionConflict,
    InvalidPayload,
    DatabaseUnavailable,
}

impl OutboxFailureReason {
    const fn as_str(self) -> &'static str {
        match self {
            Self::TransientProvider => "TransientProvider",
            Self::ProjectionConflict => "ProjectionConflict",
            Self::InvalidPayload => "InvalidPayload",
            Self::DatabaseUnavailable => "DatabaseUnavailable",
        }
    }
}

pub trait OutboxClock: Clone + Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl OutboxClock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

pub struct OutboxRepository<'connection, Clock = SystemClock> {
    database: &'connection DatabaseConnection,
    clock: Clock,
}

impl<'connection> OutboxRepository<'connection, SystemClock> {
    #[must_use]
    pub const fn new(database: &'connection DatabaseConnection) -> Self {
        Self {
            database,
            clock: SystemClock,
        }
    }
}

impl<'connection, Clock> OutboxRepository<'connection, Clock>
where
    Clock: OutboxClock,
{
    #[must_use]
    pub const fn with_clock(database: &'connection DatabaseConnection, clock: Clock) -> Self {
        Self { database, clock }
    }

    /// Reads the durable reconciled watermark for one storage root.
    ///
    /// # Errors
    ///
    /// Returns [`OutboxRepositoryError::MissingRoot`] or a database failure.
    pub async fn reconciled_revision(
        &self,
        storage_root_id: StorageRootId,
    ) -> Result<i64, OutboxRepositoryError> {
        read_root_revisions(self.database, storage_root_id)
            .await?
            .map(|(_, reconciled)| reconciled)
            .ok_or(OutboxRepositoryError::MissingRoot)
    }

    /// Lists storage roots whose catalog projection watermark trails storage sync.
    ///
    /// # Errors
    ///
    /// Returns [`OutboxRepositoryError::InvalidRootLimit`] for an unbounded request or a
    /// database error when the backlog cannot be read.
    pub async fn backlogged_roots(
        &self,
        after: Option<StorageRootId>,
        limit: u64,
    ) -> Result<Vec<BackloggedStorageRoot>, OutboxRepositoryError> {
        if !(1..=1_000).contains(&limit) {
            return Err(OutboxRepositoryError::InvalidRootLimit);
        }
        let mut query = Query::select();
        query
            .columns([Alias::new("id"), Alias::new("sync_revision")])
            .from(Alias::new("storage_roots"))
            .and_where(
                Expr::col(Alias::new("sync_revision"))
                    .gt(Expr::col(Alias::new("reconciled_sync_revision"))),
            )
            .order_by(Alias::new("id"), Order::Asc)
            .limit(limit);
        if let Some(after) = after {
            query.and_where(Expr::col(Alias::new("id")).gt(after.as_uuid()));
        }
        let query = query.clone();
        let backend = self.database.get_database_backend();
        self.database
            .query_all(backend.build(&query))
            .await?
            .iter()
            .map(|row| {
                Ok(BackloggedStorageRoot {
                    root_id: StorageRootId::from_uuid(row.try_get("", "id")?),
                    expected_revision: row.try_get("", "sync_revision")?,
                })
            })
            .collect()
    }

    /// Claims the next event in the root's lowest unreconciled revision.
    ///
    /// Expired claims may be reclaimed. The stored owner value includes a fresh
    /// fencing token so a previous worker instance cannot finish a reclaimed event.
    ///
    /// # Errors
    ///
    /// Returns [`OutboxRepositoryError`] for invalid lease arguments, database
    /// failures, rollback failures, or a missing root.
    pub async fn claim_next(
        &self,
        storage_root_id: StorageRootId,
        lease_owner: &str,
        lease_duration: Duration,
    ) -> Result<Option<ClaimedOutboxEvent>, OutboxRepositoryError> {
        if lease_owner.trim().is_empty() {
            return Err(OutboxRepositoryError::EmptyLeaseOwner);
        }
        if lease_owner.chars().count() > MAX_LEASE_OWNER_CHARS {
            return Err(OutboxRepositoryError::LeaseOwnerTooLong);
        }
        if lease_duration <= Duration::zero() {
            return Err(OutboxRepositoryError::InvalidLeaseDuration);
        }
        let now = self.clock.now();
        let lease_expires_at = now
            .checked_add_signed(lease_duration)
            .ok_or(OutboxRepositoryError::TimestampOverflow)?;
        let transaction = self.database.begin().await?;
        let lease_token = format!("{lease_owner}:{}", Uuid::new_v4());
        let result = claim_in_transaction(
            &transaction,
            storage_root_id,
            &lease_token,
            now,
            lease_expires_at,
        )
        .await;
        finish(transaction, result).await
    }

    /// Completes a claim inside a caller-owned transaction.
    ///
    /// The application reconciler uses this after applying its catalog
    /// projection to `transaction`, then commits the caller-owned transaction.
    /// That keeps projection writes, outbox state, and the watermark atomic.
    ///
    /// # Errors
    ///
    /// Returns [`OutboxRepositoryError::LostLease`] when the claim was expired
    /// or replaced, plus database and invariant errors.
    pub async fn complete_in_transaction(
        &self,
        transaction: &DatabaseTransaction,
        claimed: &ClaimedOutboxEvent,
    ) -> Result<OutboxCompletion, OutboxRepositoryError> {
        complete_in_transaction(transaction, claimed, self.clock.now()).await
    }

    /// Releases a claim for retry after a bounded delay.
    ///
    /// # Errors
    ///
    /// Returns [`OutboxRepositoryError::LostLease`] when the claim is no longer
    /// current, plus database or rollback errors.
    pub async fn fail(
        &self,
        claimed: &ClaimedOutboxEvent,
        backoff: Duration,
        reason: OutboxFailureReason,
    ) -> Result<(), OutboxRepositoryError> {
        if backoff < Duration::zero() {
            return Err(OutboxRepositoryError::InvalidBackoff);
        }
        let now = self.clock.now();
        let available_at = now
            .checked_add_signed(backoff)
            .ok_or(OutboxRepositoryError::TimestampOverflow)?;
        let transaction = self.database.begin().await?;
        let result = fail_in_transaction(&transaction, claimed, now, available_at, reason).await;
        finish(transaction, result).await
    }
}

#[derive(Debug, Error)]
pub enum OutboxRepositoryError {
    #[error("lease owner must not be empty")]
    EmptyLeaseOwner,
    #[error("lease owner must not exceed 128 characters")]
    LeaseOwnerTooLong,
    #[error("outbox backlog root limit must be between 1 and 1000")]
    InvalidRootLimit,
    #[error("lease duration must be positive")]
    InvalidLeaseDuration,
    #[error("retry backoff must not be negative")]
    InvalidBackoff,
    #[error("lease or retry timestamp is outside the supported range")]
    TimestampOverflow,
    #[error("storage root does not exist")]
    MissingRoot,
    #[error("outbox lease is expired or no longer owned by this claim")]
    LostLease,
    #[error("database did not return the expected aggregate row")]
    MissingAggregateRow,
    #[error("reconciled watermark changed unexpectedly during compare-and-swap")]
    WatermarkConflict,
    #[error("database operation failed: {0}")]
    Database(#[from] DbErr),
    #[error("rollback failed after {original}: {rollback}")]
    RollbackFailed { original: String, rollback: DbErr },
}

async fn finish<T>(
    transaction: DatabaseTransaction,
    result: Result<T, OutboxRepositoryError>,
) -> Result<T, OutboxRepositoryError> {
    match result {
        Ok(value) => {
            transaction.commit().await?;
            Ok(value)
        }
        Err(original) => match transaction.rollback().await {
            Ok(()) => Err(original),
            Err(rollback) => Err(OutboxRepositoryError::RollbackFailed {
                original: original.to_string(),
                rollback,
            }),
        },
    }
}

async fn claim_in_transaction(
    transaction: &DatabaseTransaction,
    storage_root_id: StorageRootId,
    lease_token: &str,
    now: DateTime<Utc>,
    lease_expires_at: DateTime<Utc>,
) -> Result<Option<ClaimedOutboxEvent>, OutboxRepositoryError> {
    let reconciled_revision = advance_contiguous_watermark(transaction, storage_root_id).await?;
    let (sync_revision, _) = read_root_revisions(transaction, storage_root_id)
        .await?
        .ok_or(OutboxRepositoryError::MissingRoot)?;
    if reconciled_revision >= sync_revision {
        return Ok(None);
    }
    let target_revision = reconciled_revision + 1;
    let claimable = claimable_condition(now);
    let statement = Query::select()
        .columns([
            Alias::new("id"),
            Alias::new("storage_root_id"),
            Alias::new("sync_revision"),
            Alias::new("event_type"),
            Alias::new("storage_object_id"),
            Alias::new("payload_version"),
            Alias::new("payload"),
            Alias::new("attempt_count"),
        ])
        .from(Alias::new("storage_change_outbox"))
        .and_where(Expr::col(Alias::new("storage_root_id")).eq(storage_root_id.as_uuid()))
        .and_where(Expr::col(Alias::new("sync_revision")).eq(target_revision))
        .cond_where(claimable.clone())
        .order_by(Alias::new("id"), Order::Asc)
        .limit(1)
        .to_owned();
    let backend = transaction.get_database_backend();
    let Some(row) = transaction.query_one(backend.build(&statement)).await? else {
        return Ok(None);
    };
    let claimed = event_from_row(&row, lease_token.to_owned())?;
    let update = Query::update()
        .table(Alias::new("storage_change_outbox"))
        .value(Alias::new("state"), STATE_PROCESSING)
        .value(Alias::new("lease_owner"), lease_token)
        .value(Alias::new("lease_expires_at"), lease_expires_at)
        .and_where(Expr::col(Alias::new("id")).eq(claimed.id))
        .cond_where(claimable)
        .to_owned();
    let result = transaction.execute(backend.build(&update)).await?;
    if result.rows_affected() != 1 {
        return Ok(None);
    }
    Ok(Some(claimed))
}

fn claimable_condition(now: DateTime<Utc>) -> Cond {
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

async fn complete_in_transaction(
    transaction: &DatabaseTransaction,
    claimed: &ClaimedOutboxEvent,
    now: DateTime<Utc>,
) -> Result<OutboxCompletion, OutboxRepositoryError> {
    let backend = transaction.get_database_backend();
    let statement = Query::delete()
        .from_table(Alias::new("storage_change_outbox"))
        .and_where(Expr::col(Alias::new("id")).eq(claimed.id))
        .and_where(Expr::col(Alias::new("state")).eq(STATE_PROCESSING))
        .and_where(Expr::col(Alias::new("lease_owner")).eq(&claimed.lease_token))
        .and_where(Expr::col(Alias::new("lease_expires_at")).gt(now))
        .to_owned();
    if transaction
        .execute(backend.build(&statement))
        .await?
        .rows_affected()
        != 1
    {
        return Err(OutboxRepositoryError::LostLease);
    }
    let reconciled_sync_revision =
        advance_contiguous_watermark(transaction, claimed.storage_root_id).await?;
    Ok(OutboxCompletion {
        reconciled_sync_revision,
    })
}

async fn fail_in_transaction(
    transaction: &DatabaseTransaction,
    claimed: &ClaimedOutboxEvent,
    now: DateTime<Utc>,
    available_at: DateTime<Utc>,
    reason: OutboxFailureReason,
) -> Result<(), OutboxRepositoryError> {
    let backend = transaction.get_database_backend();
    let statement = Query::update()
        .table(Alias::new("storage_change_outbox"))
        .value(Alias::new("state"), STATE_PENDING)
        .value(
            Alias::new("attempt_count"),
            Expr::col(Alias::new("attempt_count")).add(1),
        )
        .value(Alias::new("available_at"), available_at)
        .value(Alias::new("last_error"), reason.as_str())
        .value(Alias::new("lease_owner"), Option::<String>::None)
        .value(
            Alias::new("lease_expires_at"),
            Option::<DateTime<Utc>>::None,
        )
        .and_where(Expr::col(Alias::new("id")).eq(claimed.id))
        .and_where(Expr::col(Alias::new("state")).eq(STATE_PROCESSING))
        .and_where(Expr::col(Alias::new("lease_owner")).eq(&claimed.lease_token))
        .and_where(Expr::col(Alias::new("lease_expires_at")).gt(now))
        .to_owned();
    if transaction
        .execute(backend.build(&statement))
        .await?
        .rows_affected()
        != 1
    {
        return Err(OutboxRepositoryError::LostLease);
    }
    Ok(())
}

async fn advance_contiguous_watermark(
    transaction: &DatabaseTransaction,
    storage_root_id: StorageRootId,
) -> Result<i64, OutboxRepositoryError> {
    let (sync_revision, mut reconciled_revision) =
        read_root_revisions(transaction, storage_root_id)
            .await?
            .ok_or(OutboxRepositoryError::MissingRoot)?;
    let backend = transaction.get_database_backend();
    while reconciled_revision < sync_revision {
        let next = reconciled_revision + 1;
        let statement = Query::select()
            .expr_as(Expr::col(Alias::new("id")).count(), Alias::new("remaining"))
            .from(Alias::new("storage_change_outbox"))
            .and_where(Expr::col(Alias::new("storage_root_id")).eq(storage_root_id.as_uuid()))
            .and_where(Expr::col(Alias::new("sync_revision")).eq(next))
            .and_where(Expr::col(Alias::new("state")).ne(STATE_PROCESSED))
            .to_owned();
        let remaining: i64 = transaction
            .query_one(backend.build(&statement))
            .await?
            .ok_or(OutboxRepositoryError::MissingAggregateRow)?
            .try_get("", "remaining")?;
        if remaining != 0 {
            break;
        }
        let update = Query::update()
            .table(Alias::new("storage_roots"))
            .value(Alias::new("reconciled_sync_revision"), next)
            .and_where(Expr::col(Alias::new("id")).eq(storage_root_id.as_uuid()))
            .and_where(Expr::col(Alias::new("reconciled_sync_revision")).eq(reconciled_revision))
            .to_owned();
        if transaction
            .execute(backend.build(&update))
            .await?
            .rows_affected()
            != 1
        {
            let Some((_, observed_revision)) =
                read_root_revisions(transaction, storage_root_id).await?
            else {
                return Err(OutboxRepositoryError::MissingRoot);
            };
            if observed_revision >= next {
                reconciled_revision = observed_revision;
                continue;
            }
            return Err(OutboxRepositoryError::WatermarkConflict);
        }
        reconciled_revision = next;
    }
    Ok(reconciled_revision)
}

async fn read_root_revisions(
    connection: &impl ConnectionTrait,
    storage_root_id: StorageRootId,
) -> Result<Option<(i64, i64)>, DbErr> {
    let statement = Query::select()
        .columns([
            Alias::new("sync_revision"),
            Alias::new("reconciled_sync_revision"),
        ])
        .from(Alias::new("storage_roots"))
        .and_where(Expr::col(Alias::new("id")).eq(storage_root_id.as_uuid()))
        .to_owned();
    let backend = connection.get_database_backend();
    connection
        .query_one(backend.build(&statement))
        .await?
        .map(|row| {
            Ok((
                row.try_get("", "sync_revision")?,
                row.try_get("", "reconciled_sync_revision")?,
            ))
        })
        .transpose()
}

fn event_from_row(row: &QueryResult, lease_token: String) -> Result<ClaimedOutboxEvent, DbErr> {
    Ok(ClaimedOutboxEvent {
        id: row.try_get("", "id")?,
        storage_root_id: StorageRootId::from_uuid(row.try_get("", "storage_root_id")?),
        sync_revision: row.try_get("", "sync_revision")?,
        event_type: row.try_get("", "event_type")?,
        storage_object_id: StorageObjectRecordId::from_uuid(row.try_get("", "storage_object_id")?),
        payload_version: row.try_get("", "payload_version")?,
        payload: row.try_get("", "payload")?,
        attempt_count: row.try_get("", "attempt_count")?,
        lease_token,
    })
}
