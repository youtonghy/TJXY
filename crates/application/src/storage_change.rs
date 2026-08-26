use sea_orm::DatabaseConnection;
use thiserror::Error;
use tjxy_common::StorageRootId;
use tjxy_db::{
    ClaimedOutboxEvent, OutboxCompletion, OutboxFailureDisposition, OutboxFailureReason,
    OutboxRepository, OutboxRepositoryError, StorageChangeProjectionError,
    StorageChangeProjectionRepository,
};
use uuid::Uuid;

const MAX_IN_FLIGHT_POLLS: usize = 100;
const IN_FLIGHT_POLL_DELAY: std::time::Duration = std::time::Duration::from_millis(20);
const MAX_ROOTS_PER_PASS: u64 = 100;

pub struct StorageChangeProjector {
    database: DatabaseConnection,
}

impl StorageChangeProjector {
    #[must_use]
    pub const fn new(database: DatabaseConnection) -> Self {
        Self { database }
    }

    /// Applies one claimed storage change and advances its watermark atomically.
    ///
    /// # Errors
    ///
    /// Returns [`StorageChangeProjectionError`] for invalid events, lost leases, or SQL failures.
    pub async fn apply(
        &self,
        claimed: &ClaimedOutboxEvent,
    ) -> Result<OutboxCompletion, StorageChangeProjectorError> {
        StorageChangeProjectionRepository::new(&self.database)
            .apply(claimed)
            .await
            .map_err(Into::into)
    }

    /// Projects a root's contiguous outbox sequence through the expected sync revision.
    ///
    /// # Errors
    ///
    /// Returns [`StorageChangeProjectorError`] for claim/projection failures or a blocked gap.
    pub async fn drain_root(
        &self,
        root_id: StorageRootId,
        expected_revision: i64,
    ) -> Result<i64, StorageChangeProjectorError> {
        let outbox = OutboxRepository::new(&self.database);
        let owner = format!("storage-projector-{}", Uuid::new_v4());
        let mut in_flight_polls = 0_usize;
        loop {
            let observed = outbox.reconciled_revision(root_id).await?;
            if observed >= expected_revision {
                return Ok(observed);
            }
            match self.reconcile_next(&outbox, root_id, &owner).await? {
                ReconcileNext::Processed => in_flight_polls = 0,
                ReconcileNext::Deferred => {
                    in_flight_polls += 1;
                    if in_flight_polls >= MAX_IN_FLIGHT_POLLS {
                        let observed = outbox.reconciled_revision(root_id).await?;
                        return Err(StorageChangeProjectorError::Incomplete {
                            expected: expected_revision,
                            observed,
                        });
                    }
                    tokio::time::sleep(IN_FLIGHT_POLL_DELAY).await;
                }
            }
        }
    }

    async fn reconcile_next(
        &self,
        outbox: &OutboxRepository<'_>,
        root_id: StorageRootId,
        owner: &str,
    ) -> Result<ReconcileNext, StorageChangeProjectorError> {
        let Some(claimed) = outbox
            .claim_next(root_id, owner, chrono::Duration::minutes(5))
            .await?
        else {
            return Ok(ReconcileNext::Deferred);
        };
        match self.apply(&claimed).await {
            Ok(_) => Ok(ReconcileNext::Processed),
            Err(error) => {
                if !projector_error_is_lost_lease(&error) {
                    let disposition = outbox
                        .fail(
                            &claimed,
                            outbox_retry_delay(claimed.attempt_count()),
                            outbox_failure_reason(&error),
                        )
                        .await?;
                    if disposition == OutboxFailureDisposition::DeadLettered {
                        tracing::error!(
                            storage_root_id = %claimed.storage_root_id(),
                            sync_revision = claimed.sync_revision(),
                            event_id = %claimed.id(),
                            "Storage change moved to dead letter after repeated failures"
                        );
                    }
                }
                Err(error)
            }
        }
    }
}

enum ReconcileNext {
    Processed,
    Deferred,
}

fn outbox_retry_delay(attempt_count: i32) -> chrono::Duration {
    let exponent = u32::try_from(attempt_count).unwrap_or_default().min(6);
    chrono::Duration::seconds((5_i64 * (1_i64 << exponent)).min(300))
}

fn outbox_failure_reason(error: &StorageChangeProjectorError) -> OutboxFailureReason {
    match error {
        StorageChangeProjectorError::Projection(StorageChangeProjectionError::InvalidPayload) => {
            OutboxFailureReason::InvalidPayload
        }
        StorageChangeProjectorError::Projection(
            StorageChangeProjectionError::Database(_)
            | StorageChangeProjectionError::RollbackFailed { .. },
        )
        | StorageChangeProjectorError::Outbox(
            OutboxRepositoryError::Database(_) | OutboxRepositoryError::RollbackFailed { .. },
        ) => OutboxFailureReason::DatabaseUnavailable,
        _ => OutboxFailureReason::ProjectionConflict,
    }
}

fn projector_error_is_lost_lease(error: &StorageChangeProjectorError) -> bool {
    matches!(
        error,
        StorageChangeProjectorError::Projection(StorageChangeProjectionError::Outbox(
            OutboxRepositoryError::LostLease
        )) | StorageChangeProjectorError::Outbox(OutboxRepositoryError::LostLease)
    )
}

pub struct StorageChangeReconciler {
    database: DatabaseConnection,
    after: Option<StorageRootId>,
}

impl StorageChangeReconciler {
    #[must_use]
    pub const fn new(database: DatabaseConnection) -> Self {
        Self {
            database,
            after: None,
        }
    }

    /// Processes at most one durable storage change for each root in one bounded page.
    ///
    /// # Errors
    ///
    /// Returns [`StorageChangeReconcilerError`] when the root backlog cannot be enumerated.
    pub async fn run_once(
        &mut self,
    ) -> Result<StorageChangeReconcileReport, StorageChangeReconcilerError> {
        let outbox = OutboxRepository::new(&self.database);
        let mut roots = outbox
            .backlogged_roots(self.after, MAX_ROOTS_PER_PASS)
            .await?;
        if roots.is_empty() && self.after.take().is_some() {
            roots = outbox.backlogged_roots(None, MAX_ROOTS_PER_PASS).await?;
        }
        self.after = roots.last().map(|root| root.root_id());
        let projector = StorageChangeProjector::new(self.database.clone());
        let owner = format!("storage-reconciler-{}", Uuid::new_v4());
        let mut report = StorageChangeReconcileReport::default();
        for root in roots {
            match projector
                .reconcile_next(&outbox, root.root_id(), &owner)
                .await
            {
                Ok(ReconcileNext::Processed) => {
                    report.events_processed += 1;
                    if outbox.reconciled_revision(root.root_id()).await? >= root.expected_revision()
                    {
                        report.roots_reconciled += 1;
                    }
                }
                Ok(ReconcileNext::Deferred) => {
                    if outbox.reconciled_revision(root.root_id()).await? >= root.expected_revision()
                    {
                        report.roots_reconciled += 1;
                    }
                }
                Err(error) => report.failures.push(StorageChangeReconcileFailure {
                    root_id: root.root_id(),
                    error: error.to_string(),
                }),
            }
        }
        Ok(report)
    }
}

#[derive(Debug, Default)]
pub struct StorageChangeReconcileReport {
    events_processed: u64,
    roots_reconciled: u64,
    failures: Vec<StorageChangeReconcileFailure>,
}

impl StorageChangeReconcileReport {
    #[must_use]
    pub const fn events_processed(&self) -> u64 {
        self.events_processed
    }

    #[must_use]
    pub const fn roots_reconciled(&self) -> u64 {
        self.roots_reconciled
    }

    #[must_use]
    pub fn failures(&self) -> &[StorageChangeReconcileFailure] {
        &self.failures
    }
}

#[derive(Debug)]
pub struct StorageChangeReconcileFailure {
    root_id: StorageRootId,
    error: String,
}

impl StorageChangeReconcileFailure {
    #[must_use]
    pub const fn root_id(&self) -> StorageRootId {
        self.root_id
    }

    #[must_use]
    pub fn error(&self) -> &str {
        &self.error
    }
}

#[derive(Debug, Error)]
pub enum StorageChangeReconcilerError {
    #[error("storage outbox operation failed: {0}")]
    Outbox(#[from] OutboxRepositoryError),
}

#[derive(Debug, Error)]
pub enum StorageChangeProjectorError {
    #[error("storage change projection failed: {0}")]
    Projection(#[from] StorageChangeProjectionError),
    #[error("storage outbox operation failed: {0}")]
    Outbox(#[from] OutboxRepositoryError),
    #[error(
        "storage outbox reconciliation stopped at {observed}, before expected revision {expected}"
    )]
    Incomplete { expected: i64, observed: i64 },
}
