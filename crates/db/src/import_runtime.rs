use chrono::Utc;
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbErr, TransactionTrait,
    sea_query::{Alias, Expr, Query},
};
use serde_json::json;
use thiserror::Error;
use tjxy_credentials::{CredentialCipherError, CredentialEnvelope};
use uuid::Uuid;

const MAX_IDENTITY_CHARS: usize = 2048;

#[derive(Clone, Debug)]
pub struct ImportRuntimeDraft {
    source_id: Uuid,
    source_instance_id: String,
    dry_run: bool,
    envelope: CredentialEnvelope,
    target_library_id: Uuid,
    target_user_id: Uuid,
}

impl ImportRuntimeDraft {
    /// Defines one encrypted Emby import source and its publication target.
    ///
    /// # Errors
    ///
    /// Returns [`ImportRuntimeRepositoryError::InvalidDraft`] for an invalid source identity.
    pub fn new(
        source_id: Uuid,
        source_instance_id: impl Into<String>,
        dry_run: bool,
        envelope: CredentialEnvelope,
        target_library_id: Uuid,
        target_user_id: Uuid,
    ) -> Result<Self, ImportRuntimeRepositoryError> {
        let source_instance_id = source_instance_id.into();
        if !valid_identity(&source_instance_id) {
            return Err(ImportRuntimeRepositoryError::InvalidDraft);
        }
        Ok(Self {
            source_id,
            source_instance_id,
            dry_run,
            envelope,
            target_library_id,
            target_user_id,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CreatedImportRuntime {
    job: Uuid,
    source: Uuid,
}

impl CreatedImportRuntime {
    #[must_use]
    pub const fn job_id(self) -> Uuid {
        self.job
    }

    #[must_use]
    pub const fn source_id(self) -> Uuid {
        self.source
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImportSourceRecord {
    source: Uuid,
    job: Uuid,
    envelope: CredentialEnvelope,
    target_library: Uuid,
    target_user: Uuid,
}

impl ImportSourceRecord {
    #[must_use]
    pub const fn source_id(&self) -> Uuid {
        self.source
    }

    #[must_use]
    pub const fn job_id(&self) -> Uuid {
        self.job
    }

    #[must_use]
    pub const fn envelope(&self) -> &CredentialEnvelope {
        &self.envelope
    }

    #[must_use]
    pub const fn target_library_id(&self) -> Uuid {
        self.target_library
    }

    #[must_use]
    pub const fn target_user_id(&self) -> Uuid {
        self.target_user
    }
}

pub struct ImportRuntimeRepository<'connection> {
    database: &'connection DatabaseConnection,
}

impl<'connection> ImportRuntimeRepository<'connection> {
    #[must_use]
    pub const fn new(database: &'connection DatabaseConnection) -> Self {
        Self { database }
    }

    /// Atomically creates a pending Emby job and its encrypted restart configuration.
    ///
    /// # Errors
    ///
    /// Returns foreign-key, uniqueness, commit, or rollback failures.
    pub async fn create_emby(
        &self,
        draft: &ImportRuntimeDraft,
    ) -> Result<CreatedImportRuntime, ImportRuntimeRepositoryError> {
        let transaction = self.database.begin().await?;
        let result = create_emby(&transaction, draft).await;
        finish(transaction, result).await
    }

    /// Loads the encrypted source and target for a durable import job.
    ///
    /// # Errors
    ///
    /// Returns malformed envelope or database failures.
    pub async fn source_for_job(
        &self,
        job_id: Uuid,
    ) -> Result<Option<ImportSourceRecord>, ImportRuntimeRepositoryError> {
        let query = Query::select()
            .columns([
                Alias::new("id"),
                Alias::new("import_job_id"),
                Alias::new("encrypted_payload"),
                Alias::new("key_version"),
                Alias::new("target_library_id"),
                Alias::new("target_user_id"),
            ])
            .from(Alias::new("import_sources"))
            .and_where(Expr::col(Alias::new("import_job_id")).eq(job_id))
            .limit(1)
            .to_owned();
        let backend = self.database.get_database_backend();
        self.database
            .query_one(backend.build(&query))
            .await?
            .map(|row| {
                Ok(ImportSourceRecord {
                    source: row.try_get("", "id")?,
                    job: row.try_get("", "import_job_id")?,
                    envelope: CredentialEnvelope::from_parts(
                        row.try_get("", "key_version")?,
                        row.try_get("", "encrypted_payload")?,
                    )?,
                    target_library: row.try_get("", "target_library_id")?,
                    target_user: row.try_get("", "target_user_id")?,
                })
            })
            .transpose()
    }
}

#[derive(Debug, Error)]
pub enum ImportRuntimeRepositoryError {
    #[error("import runtime draft is invalid")]
    InvalidDraft,
    #[error("stored import credential envelope is invalid")]
    InvalidEnvelope(#[from] CredentialCipherError),
    #[error("import runtime database operation failed: {0}")]
    Database(#[from] DbErr),
    #[error("import runtime rollback failed after {original}: {rollback}")]
    RollbackFailed { original: String, rollback: DbErr },
}

async fn create_emby(
    transaction: &DatabaseTransaction,
    draft: &ImportRuntimeDraft,
) -> Result<CreatedImportRuntime, ImportRuntimeRepositoryError> {
    let job_id = Uuid::new_v4();
    let now = Utc::now();
    let backend = transaction.get_database_backend();
    let job = Query::insert()
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
            job_id.into(),
            "EmbyApi".into(),
            draft.source_instance_id.clone().into(),
            "Pending".into(),
            draft.dry_run.into(),
            json!({}).into(),
            json!({}).into(),
            0_i32.into(),
            now.into(),
            now.into(),
        ])
        .to_owned();
    transaction.execute(backend.build(&job)).await?;
    let source = Query::insert()
        .into_table(Alias::new("import_sources"))
        .columns([
            Alias::new("id"),
            Alias::new("import_job_id"),
            Alias::new("encrypted_payload"),
            Alias::new("key_version"),
            Alias::new("target_library_id"),
            Alias::new("target_user_id"),
            Alias::new("created_at"),
            Alias::new("updated_at"),
        ])
        .values_panic([
            draft.source_id.into(),
            job_id.into(),
            draft.envelope.payload().to_vec().into(),
            draft.envelope.key_version().into(),
            draft.target_library_id.into(),
            draft.target_user_id.into(),
            now.into(),
            now.into(),
        ])
        .to_owned();
    transaction.execute(backend.build(&source)).await?;
    Ok(CreatedImportRuntime {
        job: job_id,
        source: draft.source_id,
    })
}

fn valid_identity(value: &str) -> bool {
    !value.trim().is_empty()
        && value.chars().count() <= MAX_IDENTITY_CHARS
        && !value.chars().any(char::is_control)
}

async fn finish<T>(
    transaction: DatabaseTransaction,
    result: Result<T, ImportRuntimeRepositoryError>,
) -> Result<T, ImportRuntimeRepositoryError> {
    match result {
        Ok(value) => {
            transaction.commit().await?;
            Ok(value)
        }
        Err(original) => match transaction.rollback().await {
            Ok(()) => Err(original),
            Err(rollback) => Err(ImportRuntimeRepositoryError::RollbackFailed {
                original: original.to_string(),
                rollback,
            }),
        },
    }
}
