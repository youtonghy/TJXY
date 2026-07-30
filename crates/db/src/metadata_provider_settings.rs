use chrono::{DateTime, Utc};
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbErr, QueryResult, SqlErr,
    TransactionTrait,
    sea_query::{Alias, Expr, Query},
};
use thiserror::Error;
use tjxy_credentials::{CredentialEnvelope, SealedCredential};
use uuid::Uuid;

const PROVIDER_MAX_CHARS: usize = 64;
const LANGUAGE_MAX_CHARS: usize = 32;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetadataProviderSettingRecord {
    provider: String,
    enabled: bool,
    language: String,
    credential_id: Uuid,
    envelope: CredentialEnvelope,
    revision: i64,
    updated_at: DateTime<Utc>,
}

impl MetadataProviderSettingRecord {
    #[must_use]
    pub fn provider(&self) -> &str {
        &self.provider
    }

    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    #[must_use]
    pub fn language(&self) -> &str {
        &self.language
    }

    #[must_use]
    pub const fn credential_id(&self) -> Uuid {
        self.credential_id
    }

    #[must_use]
    pub const fn envelope(&self) -> &CredentialEnvelope {
        &self.envelope
    }

    #[must_use]
    pub const fn revision(&self) -> i64 {
        self.revision
    }

    #[must_use]
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

pub struct MetadataProviderSettingsRepository<'connection> {
    database: &'connection DatabaseConnection,
}

impl<'connection> MetadataProviderSettingsRepository<'connection> {
    #[must_use]
    pub const fn new(database: &'connection DatabaseConnection) -> Self {
        Self { database }
    }

    /// Loads one provider's settings without decrypting its credential.
    ///
    /// # Errors
    ///
    /// Returns validation, malformed durable envelope, or database errors.
    pub async fn get(
        &self,
        provider: &str,
    ) -> Result<Option<MetadataProviderSettingRecord>, MetadataProviderSettingsRepositoryError>
    {
        validate_provider(provider)?;
        get_on(self.database, provider).await
    }

    /// Creates or rotates one encrypted provider setting using an optimistic revision fence.
    ///
    /// Provider and credential identity are taken from the cipher-produced sealed value so an
    /// opaque envelope cannot be paired with unrelated associated-data fields.
    ///
    /// Passing `None` creates revision 1 and conflicts with an existing setting. Passing
    /// `Some(revision)` updates only that revision. This boundary never accepts plaintext.
    ///
    /// # Errors
    ///
    /// Returns input validation, revision conflict, commit, rollback, or database errors.
    pub async fn put(
        &self,
        sealed: &SealedCredential,
        enabled: bool,
        language: &str,
        expected_revision: Option<i64>,
    ) -> Result<MetadataProviderSettingRecord, MetadataProviderSettingsRepositoryError> {
        validate_provider(sealed.provider())?;
        validate_language(language)?;
        if expected_revision.is_some_and(|revision| revision <= 0) {
            return Err(MetadataProviderSettingsRepositoryError::InvalidRevision);
        }

        let transaction = self.database.begin().await?;
        let result = put_on(&transaction, sealed, enabled, language, expected_revision).await;
        finish(transaction, result).await
    }

    /// Deletes one provider setting, optionally guarded by its current revision.
    ///
    /// An unguarded missing delete is a successful no-op and returns `false`.
    ///
    /// # Errors
    ///
    /// Returns validation, revision conflict, commit, rollback, or database errors.
    pub async fn delete(
        &self,
        provider: &str,
        expected_revision: Option<i64>,
    ) -> Result<bool, MetadataProviderSettingsRepositoryError> {
        validate_provider(provider)?;
        if expected_revision.is_some_and(|revision| revision <= 0) {
            return Err(MetadataProviderSettingsRepositoryError::InvalidRevision);
        }

        let transaction = self.database.begin().await?;
        let result = delete_on(&transaction, provider, expected_revision).await;
        finish(transaction, result).await
    }
}

#[derive(Debug, Error)]
pub enum MetadataProviderSettingsRepositoryError {
    #[error("metadata provider key is invalid")]
    InvalidProvider,
    #[error("metadata provider language is invalid")]
    InvalidLanguage,
    #[error("metadata provider settings revision is invalid")]
    InvalidRevision,
    #[error("metadata provider settings changed since they were read")]
    RevisionConflict,
    #[error("metadata provider credential identity changed during rotation")]
    CredentialIdentityConflict,
    #[error("stored metadata provider credential envelope is malformed")]
    InvalidStoredEnvelope,
    #[error("metadata provider settings database operation failed: {0}")]
    Database(#[from] DbErr),
    #[error("metadata provider settings rollback failed after {original}: {rollback}")]
    RollbackFailed { original: String, rollback: DbErr },
}

async fn put_on(
    transaction: &DatabaseTransaction,
    sealed: &SealedCredential,
    enabled: bool,
    language: &str,
    expected_revision: Option<i64>,
) -> Result<MetadataProviderSettingRecord, MetadataProviderSettingsRepositoryError> {
    let provider = sealed.provider();
    let backend = transaction.get_database_backend();
    let now = Utc::now();
    match expected_revision {
        None => {
            let insert = Query::insert()
                .into_table(Alias::new("metadata_provider_settings"))
                .columns([
                    Alias::new("provider"),
                    Alias::new("enabled"),
                    Alias::new("language"),
                    Alias::new("credential_id"),
                    Alias::new("encrypted_payload"),
                    Alias::new("key_version"),
                    Alias::new("revision"),
                    Alias::new("created_at"),
                    Alias::new("updated_at"),
                ])
                .values_panic([
                    provider.into(),
                    enabled.into(),
                    language.into(),
                    sealed.credential_id().into(),
                    sealed.envelope().payload().to_vec().into(),
                    sealed.envelope().key_version().into(),
                    1_i64.into(),
                    now.into(),
                    now.into(),
                ])
                .to_owned();
            let result = transaction.execute(backend.build(&insert)).await;
            if result
                .as_ref()
                .err()
                .and_then(DbErr::sql_err)
                .is_some_and(|error| matches!(error, SqlErr::UniqueConstraintViolation(_)))
            {
                return Err(MetadataProviderSettingsRepositoryError::RevisionConflict);
            }
            result?;
        }
        Some(expected_revision) => {
            let current = get_on(transaction, provider)
                .await?
                .ok_or(MetadataProviderSettingsRepositoryError::RevisionConflict)?;
            if current.revision() != expected_revision {
                return Err(MetadataProviderSettingsRepositoryError::RevisionConflict);
            }
            if current.credential_id() != sealed.credential_id() {
                return Err(MetadataProviderSettingsRepositoryError::CredentialIdentityConflict);
            }
            let next_revision = expected_revision
                .checked_add(1)
                .ok_or(MetadataProviderSettingsRepositoryError::InvalidRevision)?;
            let update = Query::update()
                .table(Alias::new("metadata_provider_settings"))
                .value(Alias::new("enabled"), enabled)
                .value(Alias::new("language"), language)
                .value(
                    Alias::new("encrypted_payload"),
                    sealed.envelope().payload().to_vec(),
                )
                .value(Alias::new("key_version"), sealed.envelope().key_version())
                .value(Alias::new("revision"), next_revision)
                .value(Alias::new("updated_at"), now)
                .and_where(Expr::col(Alias::new("provider")).eq(provider))
                .and_where(Expr::col(Alias::new("revision")).eq(expected_revision))
                .and_where(Expr::col(Alias::new("credential_id")).eq(sealed.credential_id()))
                .to_owned();
            if transaction
                .execute(backend.build(&update))
                .await?
                .rows_affected()
                != 1
            {
                return Err(MetadataProviderSettingsRepositoryError::RevisionConflict);
            }
        }
    }

    get_on(transaction, provider)
        .await?
        .ok_or(MetadataProviderSettingsRepositoryError::RevisionConflict)
}

async fn delete_on(
    transaction: &DatabaseTransaction,
    provider: &str,
    expected_revision: Option<i64>,
) -> Result<bool, MetadataProviderSettingsRepositoryError> {
    let mut delete = Query::delete();
    delete
        .from_table(Alias::new("metadata_provider_settings"))
        .and_where(Expr::col(Alias::new("provider")).eq(provider));
    if let Some(expected_revision) = expected_revision {
        delete.and_where(Expr::col(Alias::new("revision")).eq(expected_revision));
    }
    let backend = transaction.get_database_backend();
    let deleted = transaction
        .execute(backend.build(&delete))
        .await?
        .rows_affected();
    if deleted == 1 {
        return Ok(true);
    }
    if expected_revision.is_none() {
        return Ok(false);
    }

    let exists = Query::select()
        .expr(Expr::val(1_i32))
        .from(Alias::new("metadata_provider_settings"))
        .and_where(Expr::col(Alias::new("provider")).eq(provider))
        .limit(1)
        .to_owned();
    if transaction
        .query_one(backend.build(&exists))
        .await?
        .is_some()
    {
        Err(MetadataProviderSettingsRepositoryError::RevisionConflict)
    } else {
        Ok(false)
    }
}

async fn get_on<Connection>(
    connection: &Connection,
    provider: &str,
) -> Result<Option<MetadataProviderSettingRecord>, MetadataProviderSettingsRepositoryError>
where
    Connection: ConnectionTrait,
{
    let query = Query::select()
        .columns([
            Alias::new("provider"),
            Alias::new("enabled"),
            Alias::new("language"),
            Alias::new("credential_id"),
            Alias::new("encrypted_payload"),
            Alias::new("key_version"),
            Alias::new("revision"),
            Alias::new("updated_at"),
        ])
        .from(Alias::new("metadata_provider_settings"))
        .and_where(Expr::col(Alias::new("provider")).eq(provider))
        .limit(1)
        .to_owned();
    connection
        .query_one(connection.get_database_backend().build(&query))
        .await?
        .as_ref()
        .map(record_from_row)
        .transpose()
}

fn record_from_row(
    row: &QueryResult,
) -> Result<MetadataProviderSettingRecord, MetadataProviderSettingsRepositoryError> {
    let key_version = row.try_get("", "key_version")?;
    let encrypted_payload = row.try_get("", "encrypted_payload")?;
    let envelope = CredentialEnvelope::from_parts(key_version, encrypted_payload)
        .map_err(|_| MetadataProviderSettingsRepositoryError::InvalidStoredEnvelope)?;
    Ok(MetadataProviderSettingRecord {
        provider: row.try_get("", "provider")?,
        enabled: row.try_get("", "enabled")?,
        language: row.try_get("", "language")?,
        credential_id: row.try_get("", "credential_id")?,
        envelope,
        revision: row.try_get("", "revision")?,
        updated_at: row.try_get("", "updated_at")?,
    })
}

fn validate_provider(provider: &str) -> Result<(), MetadataProviderSettingsRepositoryError> {
    if provider.is_empty()
        || provider.chars().count() > PROVIDER_MAX_CHARS
        || !provider.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
    {
        return Err(MetadataProviderSettingsRepositoryError::InvalidProvider);
    }
    Ok(())
}

fn validate_language(language: &str) -> Result<(), MetadataProviderSettingsRepositoryError> {
    if language.trim() != language
        || language.is_empty()
        || language.chars().count() > LANGUAGE_MAX_CHARS
        || language.chars().any(char::is_control)
    {
        return Err(MetadataProviderSettingsRepositoryError::InvalidLanguage);
    }
    Ok(())
}

async fn finish<T>(
    transaction: DatabaseTransaction,
    result: Result<T, MetadataProviderSettingsRepositoryError>,
) -> Result<T, MetadataProviderSettingsRepositoryError> {
    match result {
        Ok(value) => {
            transaction.commit().await?;
            Ok(value)
        }
        Err(original) => match transaction.rollback().await {
            Ok(()) => Err(original),
            Err(rollback) => Err(MetadataProviderSettingsRepositoryError::RollbackFailed {
                original: original.to_string(),
                rollback,
            }),
        },
    }
}
