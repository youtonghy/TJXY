use chrono::{Duration, Utc};
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbErr, TransactionTrait,
    sea_query::{Alias, Cond, Expr, JoinType, Order, Query},
};
use thiserror::Error;
use uuid::Uuid;

const BATCH_LIMIT: u64 = 500;

pub struct QueueMaintenanceRepository<'connection> {
    database: &'connection DatabaseConnection,
}

impl<'connection> QueueMaintenanceRepository<'connection> {
    #[must_use]
    pub const fn new(database: &'connection DatabaseConnection) -> Self {
        Self { database }
    }

    /// Removes at most one bounded batch of obsolete internal queue rows.
    ///
    /// Processed storage events are legacy rows: current completion deletes them atomically.
    /// Dead letters remain available for diagnosis for the configured retention period.
    /// The cache outbox is also legacy because cache invalidation now uses one coalesced state row.
    ///
    /// # Errors
    ///
    /// Returns validation, timestamp, database, or rollback failures.
    pub async fn run_once(
        &self,
        dead_letter_retention: Duration,
    ) -> Result<QueueMaintenanceRun, QueueMaintenanceError> {
        if dead_letter_retention <= Duration::zero() {
            return Err(QueueMaintenanceError::InvalidRetention);
        }
        let dead_letter_cutoff = Utc::now()
            .checked_sub_signed(dead_letter_retention)
            .ok_or(QueueMaintenanceError::TimestampOverflow)?;
        let transaction = self.database.begin().await?;
        let result = cleanup_storage_outbox(&transaction, dead_letter_cutoff).await;
        let storage = finish(transaction, result).await?;
        if storage != 0 {
            return Ok(QueueMaintenanceRun::StorageOutbox { deleted: storage });
        }

        let transaction = self.database.begin().await?;
        let result = cleanup_legacy_cache_outbox(&transaction).await;
        let cache = finish(transaction, result).await?;
        if cache != 0 {
            return Ok(QueueMaintenanceRun::LegacyCacheOutbox { deleted: cache });
        }
        Ok(QueueMaintenanceRun::Idle)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueueMaintenanceRun {
    Idle,
    StorageOutbox { deleted: u64 },
    LegacyCacheOutbox { deleted: u64 },
}

async fn cleanup_storage_outbox(
    transaction: &DatabaseTransaction,
    dead_letter_cutoff: chrono::DateTime<Utc>,
) -> Result<u64, QueueMaintenanceError> {
    let backend = transaction.get_database_backend();
    let outbox = Alias::new("cleanup_outbox");
    let root = Alias::new("cleanup_root");
    let rows = transaction
        .query_all(
            backend.build(
                Query::select()
                    .column((outbox.clone(), Alias::new("id")))
                    .from_as(Alias::new("storage_change_outbox"), outbox.clone())
                    .join_as(
                        JoinType::InnerJoin,
                        Alias::new("storage_roots"),
                        root.clone(),
                        Expr::col((root.clone(), Alias::new("id")))
                            .equals((outbox.clone(), Alias::new("storage_root_id"))),
                    )
                    .cond_where(
                        Cond::any()
                            .add(
                                Cond::all()
                                    .add(
                                        Expr::col((outbox.clone(), Alias::new("state")))
                                            .eq("Processed"),
                                    )
                                    .add(
                                        Expr::col((outbox.clone(), Alias::new("sync_revision")))
                                            .lte(Expr::col((
                                                root.clone(),
                                                Alias::new("reconciled_sync_revision"),
                                            ))),
                                    ),
                            )
                            .add(
                                Cond::all()
                                    .add(
                                        Expr::col((outbox.clone(), Alias::new("state")))
                                            .eq("DeadLetter"),
                                    )
                                    .add(
                                        Expr::col((outbox.clone(), Alias::new("dead_lettered_at")))
                                            .lte(dead_letter_cutoff),
                                    ),
                            ),
                    )
                    .order_by((outbox.clone(), Alias::new("processed_at")), Order::Asc)
                    .order_by((outbox, Alias::new("id")), Order::Asc)
                    .limit(BATCH_LIMIT),
            ),
        )
        .await?;
    delete_ids(transaction, "storage_change_outbox", rows).await
}

async fn cleanup_legacy_cache_outbox(
    transaction: &DatabaseTransaction,
) -> Result<u64, QueueMaintenanceError> {
    let backend = transaction.get_database_backend();
    let rows = transaction
        .query_all(
            backend.build(
                Query::select()
                    .column(Alias::new("id"))
                    .from(Alias::new("cache_invalidation_outbox"))
                    .order_by(Alias::new("generation"), Order::Asc)
                    .limit(BATCH_LIMIT),
            ),
        )
        .await?;
    delete_ids(transaction, "cache_invalidation_outbox", rows).await
}

async fn delete_ids(
    transaction: &DatabaseTransaction,
    table: &str,
    rows: Vec<sea_orm::QueryResult>,
) -> Result<u64, QueueMaintenanceError> {
    let ids = rows
        .iter()
        .map(|row| row.try_get::<Uuid>("", "id"))
        .collect::<Result<Vec<_>, _>>()?;
    if ids.is_empty() {
        return Ok(0);
    }
    let backend = transaction.get_database_backend();
    Ok(transaction
        .execute(
            backend.build(
                Query::delete()
                    .from_table(Alias::new(table))
                    .and_where(Expr::col(Alias::new("id")).is_in(ids)),
            ),
        )
        .await?
        .rows_affected())
}

async fn finish<T>(
    transaction: DatabaseTransaction,
    result: Result<T, QueueMaintenanceError>,
) -> Result<T, QueueMaintenanceError> {
    match result {
        Ok(value) => {
            transaction.commit().await?;
            Ok(value)
        }
        Err(original) => match transaction.rollback().await {
            Ok(()) => Err(original),
            Err(rollback) => Err(QueueMaintenanceError::RollbackFailed {
                original: original.to_string(),
                rollback,
            }),
        },
    }
}

#[derive(Debug, Error)]
pub enum QueueMaintenanceError {
    #[error("queue retention duration must be positive")]
    InvalidRetention,
    #[error("queue retention timestamp is outside the supported range")]
    TimestampOverflow,
    #[error("database operation failed: {0}")]
    Database(#[from] DbErr),
    #[error("rollback failed after {original}: {rollback}")]
    RollbackFailed { original: String, rollback: DbErr },
}
