use chrono::{DateTime, Duration, Timelike, Utc};
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbErr, QueryResult, TransactionTrait,
    sea_query::{Alias, Cond, Expr, Query},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

use crate::natural_key;

const MAX_IDENTITY_CHARS: usize = 2048;
const MAX_PAYLOAD_BYTES: usize = 4 * 1024 * 1024;
const MAX_ERROR_CHARS: usize = 4096;

fn database_now(database: &DatabaseConnection) -> DateTime<Utc> {
    let now = Utc::now();
    if database.get_database_backend() == sea_orm::DbBackend::MySql {
        now.with_nanosecond(0).unwrap_or(now)
    } else {
        now
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImportJobState {
    Pending,
    Running,
    Paused,
    ReadyToPublish,
    Completed,
    Failed,
}

impl ImportJobState {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "Pending",
            Self::Running => "Running",
            Self::Paused => "Paused",
            Self::ReadyToPublish => "ReadyToPublish",
            Self::Completed => "Completed",
            Self::Failed => "Failed",
        }
    }

    fn from_database(value: &str) -> Result<Self, ImportStagingRepositoryError> {
        match value {
            "Pending" => Ok(Self::Pending),
            "Running" => Ok(Self::Running),
            "Paused" => Ok(Self::Paused),
            "ReadyToPublish" => Ok(Self::ReadyToPublish),
            "Completed" => Ok(Self::Completed),
            "Failed" => Ok(Self::Failed),
            _ => Err(ImportStagingRepositoryError::InvalidStoredState),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ImportJobRecord {
    id: Uuid,
    adapter_kind: String,
    source_instance_id: String,
    state: ImportJobState,
    dry_run: bool,
    checkpoint: Value,
    counters: Value,
    attempt_count: i32,
}

impl ImportJobRecord {
    #[must_use]
    pub const fn id(&self) -> Uuid {
        self.id
    }

    #[must_use]
    pub const fn state(&self) -> ImportJobState {
        self.state
    }

    #[must_use]
    pub fn adapter_kind(&self) -> &str {
        &self.adapter_kind
    }

    #[must_use]
    pub fn source_instance_id(&self) -> &str {
        &self.source_instance_id
    }

    #[must_use]
    pub const fn dry_run(&self) -> bool {
        self.dry_run
    }

    #[must_use]
    pub const fn checkpoint(&self) -> &Value {
        &self.checkpoint
    }

    #[must_use]
    pub const fn counters(&self) -> &Value {
        &self.counters
    }

    #[must_use]
    pub const fn attempt_count(&self) -> i32 {
        self.attempt_count
    }
}

#[derive(Clone, Debug)]
pub struct ClaimedImportJob {
    job: ImportJobRecord,
    lease_token: String,
}

impl ClaimedImportJob {
    #[must_use]
    pub const fn id(&self) -> Uuid {
        self.job.id
    }

    #[must_use]
    pub const fn job(&self) -> &ImportJobRecord {
        &self.job
    }
}

#[derive(Clone, Debug)]
pub struct ImportStagingItem {
    entity_kind: String,
    legacy_item_id: String,
    parent_legacy_item_id: Option<String>,
    payload: Value,
    payload_sha256: String,
}

impl ImportStagingItem {
    /// Defines one bounded, replay-identifiable import staging item.
    ///
    /// # Errors
    ///
    /// Returns [`ImportStagingRepositoryError::InvalidStagingItem`] for invalid bounds.
    pub fn new(
        entity_kind: impl Into<String>,
        legacy_item_id: impl Into<String>,
        parent_legacy_item_id: Option<String>,
        payload: Value,
    ) -> Result<Self, ImportStagingRepositoryError> {
        let entity_kind = entity_kind.into();
        let legacy_item_id = legacy_item_id.into();
        if !valid_identity(&entity_kind)
            || !valid_identity(&legacy_item_id)
            || parent_legacy_item_id
                .as_deref()
                .is_some_and(|value| !valid_identity(value))
        {
            return Err(ImportStagingRepositoryError::InvalidStagingItem);
        }
        let encoded = serde_json::to_vec(&(
            &entity_kind,
            &legacy_item_id,
            &parent_legacy_item_id,
            &payload,
        ))
        .map_err(|_| ImportStagingRepositoryError::InvalidStagingItem)?;
        if encoded.len() > MAX_PAYLOAD_BYTES {
            return Err(ImportStagingRepositoryError::InvalidStagingItem);
        }
        Ok(Self {
            entity_kind,
            legacy_item_id,
            parent_legacy_item_id,
            payload,
            payload_sha256: format!("{:x}", Sha256::digest(encoded)),
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImportStagingCommit {
    replayed: bool,
}

impl ImportStagingCommit {
    #[must_use]
    pub const fn replayed(self) -> bool {
        self.replayed
    }
}

pub struct ImportJobRepository<'connection> {
    database: &'connection DatabaseConnection,
}

impl<'connection> ImportJobRepository<'connection> {
    #[must_use]
    pub const fn new(database: &'connection DatabaseConnection) -> Self {
        Self { database }
    }

    /// Creates a durable import job in Pending state.
    ///
    /// # Errors
    ///
    /// Returns validation or database failures.
    pub async fn create(
        &self,
        adapter_kind: impl Into<String>,
        source_instance_id: impl Into<String>,
        dry_run: bool,
    ) -> Result<ImportJobRecord, ImportStagingRepositoryError> {
        let adapter_kind = adapter_kind.into();
        let source_instance_id = source_instance_id.into();
        if !valid_identity(&adapter_kind) || !valid_identity(&source_instance_id) {
            return Err(ImportStagingRepositoryError::InvalidJob);
        }
        let id = Uuid::new_v4();
        let now = database_now(self.database);
        let insert = Query::insert()
            .into_table(Alias::new("import_jobs"))
            .columns([
                Alias::new("id"),
                Alias::new("adapter_kind"),
                Alias::new("source_instance_id"),
                Alias::new("state"),
                Alias::new("dry_run"),
                Alias::new("checkpoint"),
                Alias::new("counters"),
                Alias::new("attempt_count"),
                Alias::new("created_at"),
                Alias::new("updated_at"),
            ])
            .values_panic([
                id.into(),
                adapter_kind.clone().into(),
                source_instance_id.clone().into(),
                ImportJobState::Pending.as_str().into(),
                dry_run.into(),
                json!({}).into(),
                json!({}).into(),
                0_i32.into(),
                now.into(),
                now.into(),
            ])
            .to_owned();
        let backend = self.database.get_database_backend();
        self.database.execute(backend.build(&insert)).await?;
        Ok(ImportJobRecord {
            id,
            adapter_kind,
            source_instance_id,
            state: ImportJobState::Pending,
            dry_run,
            checkpoint: json!({}),
            counters: json!({}),
            attempt_count: 0,
        })
    }

    /// Claims the next pending or expired job for one adapter kind.
    ///
    /// # Errors
    ///
    /// Returns validation, timestamp, rollback, or database failures.
    pub async fn claim_next(
        &self,
        adapter_kind: &str,
        owner: &str,
        lease_duration: Duration,
    ) -> Result<Option<ClaimedImportJob>, ImportStagingRepositoryError> {
        if !valid_identity(adapter_kind)
            || !valid_identity(owner)
            || lease_duration <= Duration::zero()
        {
            return Err(ImportStagingRepositoryError::InvalidClaim);
        }
        let now = database_now(self.database);
        let lease_expires_at = now
            .checked_add_signed(lease_duration)
            .ok_or(ImportStagingRepositoryError::TimestampOverflow)?;
        let transaction = self.database.begin().await?;
        let result = claim_next(&transaction, adapter_kind, owner, now, lease_expires_at).await;
        finish(transaction, result).await
    }

    /// Idempotently stages one item under a live import claim.
    ///
    /// # Errors
    ///
    /// Returns replay conflicts, lost leases, or database failures.
    pub async fn stage_item(
        &self,
        claimed: &ClaimedImportJob,
        item: &ImportStagingItem,
    ) -> Result<ImportStagingCommit, ImportStagingRepositoryError> {
        let transaction = self.database.begin().await?;
        let result = stage_item(&transaction, claimed, item, database_now(self.database)).await;
        finish(transaction, result).await
    }

    /// Saves an opaque adapter checkpoint under a live claim.
    ///
    /// # Errors
    ///
    /// Returns invalid bounds, lost leases, or database failures.
    pub async fn save_checkpoint(
        &self,
        claimed: &ClaimedImportJob,
        checkpoint: Value,
    ) -> Result<(), ImportStagingRepositoryError> {
        validate_json_bound(&checkpoint)?;
        let now = database_now(self.database);
        let backend = self.database.get_database_backend();
        let update = Query::update()
            .table(Alias::new("import_jobs"))
            .value(Alias::new("checkpoint"), checkpoint)
            .value(Alias::new("updated_at"), now)
            .cond_where(live_claim(claimed, now))
            .to_owned();
        if self
            .database
            .execute(backend.build(&update))
            .await?
            .rows_affected()
            != 1
        {
            return Err(ImportStagingRepositoryError::LostLease);
        }
        Ok(())
    }

    /// Pauses pending or running work and revokes its current lease.
    ///
    /// # Errors
    ///
    /// Returns an invalid transition or database failure.
    pub async fn pause(&self, id: Uuid) -> Result<(), ImportStagingRepositoryError> {
        transition_to_idle(
            self.database,
            id,
            ImportJobState::Paused,
            ["Pending", "Running"],
        )
        .await
    }

    /// Resumes a paused job as pending work.
    ///
    /// # Errors
    ///
    /// Returns an invalid transition or database failure.
    pub async fn resume(&self, id: Uuid) -> Result<(), ImportStagingRepositoryError> {
        transition_to_idle(
            self.database,
            id,
            ImportJobState::Pending,
            ["Paused", "Failed"],
        )
        .await
    }

    /// Extends a live import lease without changing its claim token.
    ///
    /// # Errors
    ///
    /// Returns invalid duration, timestamp overflow, lost leases, or database failures.
    pub async fn renew(
        &self,
        claimed: &ClaimedImportJob,
        lease_duration: Duration,
    ) -> Result<(), ImportStagingRepositoryError> {
        if lease_duration <= Duration::zero() || lease_duration > Duration::days(1) {
            return Err(ImportStagingRepositoryError::InvalidClaim);
        }
        let now = database_now(self.database);
        let lease_expires_at = now
            .checked_add_signed(lease_duration)
            .ok_or(ImportStagingRepositoryError::TimestampOverflow)?;
        let update = Query::update()
            .table(Alias::new("import_jobs"))
            .value(Alias::new("lease_expires_at"), lease_expires_at)
            .value(Alias::new("updated_at"), now)
            .cond_where(live_claim(claimed, now))
            .to_owned();
        let backend = self.database.get_database_backend();
        if self
            .database
            .execute(backend.build(&update))
            .await?
            .rows_affected()
            != 1
        {
            return Err(ImportStagingRepositoryError::LostLease);
        }
        Ok(())
    }

    /// Completes a dry-run claim with durable counters and no catalog publication.
    ///
    /// # Errors
    ///
    /// Returns invalid counters, non-dry-run jobs, lost leases, or database failures.
    pub async fn complete_dry_run(
        &self,
        claimed: &ClaimedImportJob,
        counters: Value,
    ) -> Result<(), ImportStagingRepositoryError> {
        validate_json_bound(&counters)?;
        if !claimed.job.dry_run {
            return Err(ImportStagingRepositoryError::NotDryRun);
        }
        let now = database_now(self.database);
        let backend = self.database.get_database_backend();
        let update = Query::update()
            .table(Alias::new("import_jobs"))
            .value(Alias::new("state"), ImportJobState::Completed.as_str())
            .value(Alias::new("counters"), counters)
            .value(Alias::new("lease_owner"), Option::<String>::None)
            .value(
                Alias::new("lease_expires_at"),
                Option::<DateTime<Utc>>::None,
            )
            .value(Alias::new("updated_at"), now)
            .cond_where(live_claim(claimed, now))
            .to_owned();
        if self
            .database
            .execute(backend.build(&update))
            .await?
            .rows_affected()
            != 1
        {
            return Err(ImportStagingRepositoryError::LostLease);
        }
        Ok(())
    }

    /// Seals a non-dry-run staging generation for later identity resolution and publication.
    ///
    /// The live claim is fenced and released atomically, so no item can be added after the
    /// generation becomes publishable.
    ///
    /// # Errors
    ///
    /// Returns invalid counters, dry-run jobs, lost leases, or database failures.
    pub async fn seal_for_publication(
        &self,
        claimed: &ClaimedImportJob,
        counters: Value,
    ) -> Result<(), ImportStagingRepositoryError> {
        validate_json_bound(&counters)?;
        if claimed.job.dry_run {
            return Err(ImportStagingRepositoryError::DryRunCannotPublish);
        }
        let now = database_now(self.database);
        let backend = self.database.get_database_backend();
        let update = Query::update()
            .table(Alias::new("import_jobs"))
            .value(Alias::new("state"), ImportJobState::ReadyToPublish.as_str())
            .value(Alias::new("counters"), counters)
            .value(Alias::new("lease_owner"), Option::<String>::None)
            .value(
                Alias::new("lease_expires_at"),
                Option::<DateTime<Utc>>::None,
            )
            .value(Alias::new("updated_at"), now)
            .cond_where(live_claim(claimed, now))
            .to_owned();
        if self
            .database
            .execute(backend.build(&update))
            .await?
            .rows_affected()
            != 1
        {
            return Err(ImportStagingRepositoryError::LostLease);
        }
        Ok(())
    }

    /// Requeues a transiently failed claim after a bounded delay.
    ///
    /// # Errors
    ///
    /// Returns invalid failure details, timestamp overflow, lost leases, or database failures.
    pub async fn retry(
        &self,
        claimed: &ClaimedImportJob,
        delay: Duration,
        error: &str,
    ) -> Result<(), ImportStagingRepositoryError> {
        if delay <= Duration::zero() || delay > Duration::days(7) || !valid_error(error) {
            return Err(ImportStagingRepositoryError::InvalidFailure);
        }
        let now = database_now(self.database);
        let available_at = now
            .checked_add_signed(delay)
            .ok_or(ImportStagingRepositoryError::TimestampOverflow)?;
        finish_claim_failure(
            self.database,
            claimed,
            ImportJobState::Pending,
            Some(available_at),
            error,
            now,
        )
        .await
    }

    /// Marks a non-retryable claimed import as failed.
    ///
    /// # Errors
    ///
    /// Returns invalid failure details, lost leases, or database failures.
    pub async fn fail_terminal(
        &self,
        claimed: &ClaimedImportJob,
        error: &str,
    ) -> Result<(), ImportStagingRepositoryError> {
        if !valid_error(error) {
            return Err(ImportStagingRepositoryError::InvalidFailure);
        }
        finish_claim_failure(
            self.database,
            claimed,
            ImportJobState::Failed,
            None,
            error,
            database_now(self.database),
        )
        .await
    }

    /// Reads one durable import job.
    ///
    /// # Errors
    ///
    /// Returns stored invariant or database failures.
    pub async fn get(
        &self,
        id: Uuid,
    ) -> Result<Option<ImportJobRecord>, ImportStagingRepositoryError> {
        let query = Query::select()
            .columns(job_columns())
            .from(Alias::new("import_jobs"))
            .and_where(Expr::col(Alias::new("id")).eq(id))
            .limit(1)
            .to_owned();
        let backend = self.database.get_database_backend();
        self.database
            .query_one(backend.build(&query))
            .await?
            .as_ref()
            .map(job_from_row)
            .transpose()
    }
}

#[derive(Debug, Error)]
pub enum ImportStagingRepositoryError {
    #[error("import job identity is invalid")]
    InvalidJob,
    #[error("import job claim arguments are invalid")]
    InvalidClaim,
    #[error("import staging item is invalid or too large")]
    InvalidStagingItem,
    #[error("import staging natural key was replayed with different content")]
    ReplayConflict,
    #[error("import job lease is expired or no longer owned")]
    LostLease,
    #[error("import job state transition is invalid")]
    InvalidTransition,
    #[error("only dry-run imports can use dry-run completion")]
    NotDryRun,
    #[error("dry-run imports cannot be published")]
    DryRunCannotPublish,
    #[error("import failure details or retry delay are invalid")]
    InvalidFailure,
    #[error("stored import job state is invalid")]
    InvalidStoredState,
    #[error("stored import attempt count is invalid")]
    InvalidStoredAttemptCount,
    #[error("import timestamp is outside supported range")]
    TimestampOverflow,
    #[error("import database operation failed: {0}")]
    Database(#[from] DbErr),
    #[error("import rollback failed after {original}: {rollback}")]
    RollbackFailed { original: String, rollback: DbErr },
}

async fn claim_next(
    transaction: &DatabaseTransaction,
    adapter_kind: &str,
    owner: &str,
    now: DateTime<Utc>,
    lease_expires_at: DateTime<Utc>,
) -> Result<Option<ClaimedImportJob>, ImportStagingRepositoryError> {
    let claimable = Cond::any()
        .add(
            Cond::all()
                .add(Expr::col(Alias::new("state")).eq("Pending"))
                .add(
                    Cond::any()
                        .add(Expr::col(Alias::new("available_at")).is_null())
                        .add(Expr::col(Alias::new("available_at")).lte(now)),
                ),
        )
        .add(
            Cond::all()
                .add(Expr::col(Alias::new("state")).eq("Running"))
                .add(Expr::col(Alias::new("lease_expires_at")).lte(now)),
        );
    let query = Query::select()
        .columns(job_columns())
        .from(Alias::new("import_jobs"))
        .and_where(Expr::col(Alias::new("adapter_kind")).eq(adapter_kind))
        .cond_where(claimable.clone())
        .order_by(Alias::new("created_at"), sea_orm::sea_query::Order::Asc)
        .limit(1)
        .to_owned();
    let backend = transaction.get_database_backend();
    let Some(row) = transaction.query_one(backend.build(&query)).await? else {
        return Ok(None);
    };
    let mut job = job_from_row(&row)?;
    let lease_token = format!("{owner}:{}", Uuid::new_v4());
    let update = Query::update()
        .table(Alias::new("import_jobs"))
        .value(Alias::new("state"), "Running")
        .value(Alias::new("lease_owner"), &lease_token)
        .value(Alias::new("lease_expires_at"), lease_expires_at)
        .value(
            Alias::new("attempt_count"),
            Expr::col(Alias::new("attempt_count")).add(1),
        )
        .value(Alias::new("updated_at"), now)
        .and_where(Expr::col(Alias::new("id")).eq(job.id))
        .cond_where(claimable)
        .to_owned();
    if transaction
        .execute(backend.build(&update))
        .await?
        .rows_affected()
        != 1
    {
        return Ok(None);
    }
    job.state = ImportJobState::Running;
    job.attempt_count = job
        .attempt_count
        .checked_add(1)
        .ok_or(ImportStagingRepositoryError::InvalidStoredAttemptCount)?;
    Ok(Some(ClaimedImportJob { job, lease_token }))
}

async fn stage_item(
    transaction: &DatabaseTransaction,
    claimed: &ClaimedImportJob,
    item: &ImportStagingItem,
    now: DateTime<Utc>,
) -> Result<ImportStagingCommit, ImportStagingRepositoryError> {
    ensure_claim(transaction, claimed, now).await?;
    let identity_key = natural_key::hash(&[&item.entity_kind, &item.legacy_item_id]);
    let query = Query::select()
        .columns([
            Alias::new("payload_sha256"),
            Alias::new("parent_legacy_item_id"),
        ])
        .from(Alias::new("import_staging_items"))
        .and_where(Expr::col(Alias::new("import_job_id")).eq(claimed.id()))
        .and_where(Expr::col(Alias::new("identity_key")).eq(&identity_key))
        .limit(1)
        .to_owned();
    let backend = transaction.get_database_backend();
    if let Some(row) = transaction.query_one(backend.build(&query)).await? {
        let hash: String = row.try_get("", "payload_sha256")?;
        let parent: Option<String> = row.try_get("", "parent_legacy_item_id")?;
        if hash != item.payload_sha256 || parent != item.parent_legacy_item_id {
            return Err(ImportStagingRepositoryError::ReplayConflict);
        }
        return Ok(ImportStagingCommit { replayed: true });
    }
    let insert = Query::insert()
        .into_table(Alias::new("import_staging_items"))
        .columns([
            Alias::new("id"),
            Alias::new("import_job_id"),
            Alias::new("entity_kind"),
            Alias::new("legacy_item_id"),
            Alias::new("identity_key"),
            Alias::new("parent_legacy_item_id"),
            Alias::new("payload"),
            Alias::new("payload_sha256"),
            Alias::new("validation_state"),
            Alias::new("publication_state"),
            Alias::new("created_at"),
            Alias::new("updated_at"),
        ])
        .values_panic([
            Uuid::new_v4().into(),
            claimed.id().into(),
            item.entity_kind.clone().into(),
            item.legacy_item_id.clone().into(),
            identity_key.into(),
            item.parent_legacy_item_id.clone().into(),
            item.payload.clone().into(),
            item.payload_sha256.clone().into(),
            "Pending".into(),
            "NotPublished".into(),
            now.into(),
            now.into(),
        ])
        .to_owned();
    transaction.execute(backend.build(&insert)).await?;
    ensure_claim(transaction, claimed, now).await?;
    Ok(ImportStagingCommit { replayed: false })
}

async fn transition_to_idle(
    database: &DatabaseConnection,
    id: Uuid,
    target: ImportJobState,
    sources: [&str; 2],
) -> Result<(), ImportStagingRepositoryError> {
    let now = database_now(database);
    let update = Query::update()
        .table(Alias::new("import_jobs"))
        .value(Alias::new("state"), target.as_str())
        .value(Alias::new("lease_owner"), Option::<String>::None)
        .value(
            Alias::new("lease_expires_at"),
            Option::<DateTime<Utc>>::None,
        )
        .value(Alias::new("available_at"), Option::<DateTime<Utc>>::None)
        .value(Alias::new("updated_at"), now)
        .and_where(Expr::col(Alias::new("id")).eq(id))
        .and_where(Expr::col(Alias::new("state")).is_in(sources))
        .to_owned();
    let backend = database.get_database_backend();
    if database
        .execute(backend.build(&update))
        .await?
        .rows_affected()
        != 1
    {
        return Err(ImportStagingRepositoryError::InvalidTransition);
    }
    Ok(())
}

async fn ensure_claim(
    transaction: &DatabaseTransaction,
    claimed: &ClaimedImportJob,
    now: DateTime<Utc>,
) -> Result<(), ImportStagingRepositoryError> {
    let query = Query::select()
        .column(Alias::new("id"))
        .from(Alias::new("import_jobs"))
        .cond_where(live_claim(claimed, now))
        .limit(1)
        .to_owned();
    let backend = transaction.get_database_backend();
    if transaction
        .query_one(backend.build(&query))
        .await?
        .is_none()
    {
        return Err(ImportStagingRepositoryError::LostLease);
    }
    Ok(())
}

fn live_claim(claimed: &ClaimedImportJob, now: DateTime<Utc>) -> Cond {
    Cond::all()
        .add(Expr::col(Alias::new("id")).eq(claimed.id()))
        .add(Expr::col(Alias::new("state")).eq("Running"))
        .add(Expr::col(Alias::new("lease_owner")).eq(&claimed.lease_token))
        .add(Expr::col(Alias::new("lease_expires_at")).gt(now))
}

fn job_columns() -> [Alias; 8] {
    [
        Alias::new("id"),
        Alias::new("adapter_kind"),
        Alias::new("source_instance_id"),
        Alias::new("state"),
        Alias::new("dry_run"),
        Alias::new("checkpoint"),
        Alias::new("counters"),
        Alias::new("attempt_count"),
    ]
}

fn job_from_row(row: &QueryResult) -> Result<ImportJobRecord, ImportStagingRepositoryError> {
    let attempt_count: i32 = row.try_get("", "attempt_count")?;
    if attempt_count < 0 {
        return Err(ImportStagingRepositoryError::InvalidStoredAttemptCount);
    }
    Ok(ImportJobRecord {
        id: row.try_get("", "id")?,
        adapter_kind: row.try_get("", "adapter_kind")?,
        source_instance_id: row.try_get("", "source_instance_id")?,
        state: ImportJobState::from_database(&row.try_get::<String>("", "state")?)?,
        dry_run: row.try_get("", "dry_run")?,
        checkpoint: row.try_get("", "checkpoint")?,
        counters: row.try_get("", "counters")?,
        attempt_count,
    })
}

fn validate_json_bound(value: &Value) -> Result<(), ImportStagingRepositoryError> {
    if serde_json::to_vec(value)
        .map_err(|_| ImportStagingRepositoryError::InvalidStagingItem)?
        .len()
        > MAX_PAYLOAD_BYTES
    {
        return Err(ImportStagingRepositoryError::InvalidStagingItem);
    }
    Ok(())
}

fn valid_identity(value: &str) -> bool {
    !value.trim().is_empty()
        && value.chars().count() <= MAX_IDENTITY_CHARS
        && !value.chars().any(char::is_control)
}

fn valid_error(value: &str) -> bool {
    !value.trim().is_empty()
        && value.chars().count() <= MAX_ERROR_CHARS
        && !value.chars().any(|character| character == '\0')
}

async fn finish_claim_failure(
    database: &DatabaseConnection,
    claimed: &ClaimedImportJob,
    state: ImportJobState,
    available_at: Option<DateTime<Utc>>,
    error: &str,
    now: DateTime<Utc>,
) -> Result<(), ImportStagingRepositoryError> {
    let update = Query::update()
        .table(Alias::new("import_jobs"))
        .value(Alias::new("state"), state.as_str())
        .value(Alias::new("available_at"), available_at)
        .value(Alias::new("last_error"), error)
        .value(Alias::new("lease_owner"), Option::<String>::None)
        .value(
            Alias::new("lease_expires_at"),
            Option::<DateTime<Utc>>::None,
        )
        .value(Alias::new("updated_at"), now)
        .cond_where(live_claim(claimed, now))
        .to_owned();
    let backend = database.get_database_backend();
    if database
        .execute(backend.build(&update))
        .await?
        .rows_affected()
        != 1
    {
        return Err(ImportStagingRepositoryError::LostLease);
    }
    Ok(())
}

async fn finish<T>(
    transaction: DatabaseTransaction,
    result: Result<T, ImportStagingRepositoryError>,
) -> Result<T, ImportStagingRepositoryError> {
    match result {
        Ok(value) => {
            transaction.commit().await?;
            Ok(value)
        }
        Err(original) => match transaction.rollback().await {
            Ok(()) => Err(original),
            Err(rollback) => Err(ImportStagingRepositoryError::RollbackFailed {
                original: original.to_string(),
                rollback,
            }),
        },
    }
}
