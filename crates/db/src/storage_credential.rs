use chrono::Utc;
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DbErr,
    sea_query::{Alias, Expr, OnConflict, Query},
};
use thiserror::Error;
use tjxy_credentials::{CredentialCipherError, CredentialEnvelope};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CredentialRefreshState {
    Ready,
    RefreshFailed,
    ReauthenticationRequired,
}

impl CredentialRefreshState {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "Ready",
            Self::RefreshFailed => "RefreshFailed",
            Self::ReauthenticationRequired => "ReauthenticationRequired",
        }
    }

    fn from_database(value: &str) -> Result<Self, StorageCredentialRepositoryError> {
        match value {
            "Ready" => Ok(Self::Ready),
            "RefreshFailed" => Ok(Self::RefreshFailed),
            "ReauthenticationRequired" => Ok(Self::ReauthenticationRequired),
            _ => Err(StorageCredentialRepositoryError::InvalidStoredRefreshState),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StorageCredentialRecord {
    id: Uuid,
    envelope: CredentialEnvelope,
    refresh_state: CredentialRefreshState,
}

impl StorageCredentialRecord {
    #[must_use]
    pub const fn id(&self) -> Uuid {
        self.id
    }

    #[must_use]
    pub const fn envelope(&self) -> &CredentialEnvelope {
        &self.envelope
    }

    #[must_use]
    pub const fn refresh_state(&self) -> CredentialRefreshState {
        self.refresh_state
    }
}

pub struct StorageCredentialRepository<'connection> {
    database: &'connection DatabaseConnection,
}

impl<'connection> StorageCredentialRepository<'connection> {
    #[must_use]
    pub const fn new(database: &'connection DatabaseConnection) -> Self {
        Self { database }
    }

    /// Inserts or atomically rotates one encrypted credential envelope.
    ///
    /// # Errors
    ///
    /// Returns database failures. This boundary never accepts plaintext credentials.
    pub async fn put(
        &self,
        id: Uuid,
        envelope: &CredentialEnvelope,
        refresh_state: CredentialRefreshState,
    ) -> Result<(), StorageCredentialRepositoryError> {
        let now = Utc::now();
        let statement = Query::insert()
            .into_table(Alias::new("storage_credentials"))
            .columns([
                Alias::new("id"),
                Alias::new("encrypted_payload"),
                Alias::new("key_version"),
                Alias::new("refresh_state"),
                Alias::new("created_at"),
                Alias::new("updated_at"),
            ])
            .values_panic([
                id.into(),
                envelope.payload().to_vec().into(),
                envelope.key_version().into(),
                refresh_state.as_str().into(),
                now.into(),
                now.into(),
            ])
            .on_conflict(
                OnConflict::column(Alias::new("id"))
                    .update_columns([
                        Alias::new("encrypted_payload"),
                        Alias::new("key_version"),
                        Alias::new("refresh_state"),
                        Alias::new("updated_at"),
                    ])
                    .to_owned(),
            )
            .to_owned();
        let backend = self.database.get_database_backend();
        self.database.execute(backend.build(&statement)).await?;
        Ok(())
    }

    /// Reads one encrypted credential without decrypting it in the persistence layer.
    ///
    /// # Errors
    ///
    /// Returns errors for malformed durable envelopes/states or database failures.
    pub async fn get(
        &self,
        id: Uuid,
    ) -> Result<Option<StorageCredentialRecord>, StorageCredentialRepositoryError> {
        let query = Query::select()
            .columns([
                Alias::new("id"),
                Alias::new("encrypted_payload"),
                Alias::new("key_version"),
                Alias::new("refresh_state"),
            ])
            .from(Alias::new("storage_credentials"))
            .and_where(Expr::col(Alias::new("id")).eq(id))
            .limit(1)
            .to_owned();
        let backend = self.database.get_database_backend();
        self.database
            .query_one(backend.build(&query))
            .await?
            .as_ref()
            .map(|row| {
                let envelope = CredentialEnvelope::from_parts(
                    row.try_get("", "key_version")?,
                    row.try_get("", "encrypted_payload")?,
                )?;
                Ok(StorageCredentialRecord {
                    id: row.try_get("", "id")?,
                    envelope,
                    refresh_state: CredentialRefreshState::from_database(
                        &row.try_get::<String>("", "refresh_state")?,
                    )?,
                })
            })
            .transpose()
    }
}

#[derive(Debug, Error)]
pub enum StorageCredentialRepositoryError {
    #[error("stored credential envelope is malformed")]
    InvalidStoredEnvelope(#[from] CredentialCipherError),
    #[error("stored credential refresh state is invalid")]
    InvalidStoredRefreshState,
    #[error("credential database operation failed: {0}")]
    Database(#[from] DbErr),
}
