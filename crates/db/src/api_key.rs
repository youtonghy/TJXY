use chrono::{DateTime, Duration, Utc};
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbErr, QueryResult, SqlErr,
    TransactionTrait,
    sea_query::{Alias, Cond, Expr, JoinType, LockType, Order, Query, SelectStatement},
};
use thiserror::Error;
use tjxy_common::UserId;
use tjxy_credentials::{CredentialCipherError, CredentialEnvelope};
use uuid::Uuid;

use crate::auth::{AuthUser, AuthenticatedPrincipal};

const MAX_API_KEYS: u16 = 256;
const STARTUP_API_KEY_QUERY_LIMIT: u64 = MAX_API_KEYS as u64 + 1;
const MAX_APP_NAME_CHARS: usize = 256;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApiKeyDraft {
    pub envelope_id: Uuid,
    pub creator_user_id: UserId,
    pub creator_auth_revision: i64,
    pub token_digest: [u8; 32],
    pub envelope: CredentialEnvelope,
    pub app_name: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredApiKey {
    id: i64,
    envelope_id: Uuid,
    creator_user_id: UserId,
    creator_user_name: String,
    creator_auth_revision: i64,
    token_digest: [u8; 32],
    envelope: CredentialEnvelope,
    app_name: String,
    created_at: DateTime<Utc>,
    last_used_at: Option<DateTime<Utc>>,
}

impl StoredApiKey {
    #[must_use]
    pub const fn id(&self) -> i64 {
        self.id
    }

    #[must_use]
    pub const fn envelope_id(&self) -> Uuid {
        self.envelope_id
    }

    #[must_use]
    pub const fn creator_user_id(&self) -> UserId {
        self.creator_user_id
    }

    #[must_use]
    pub fn creator_user_name(&self) -> &str {
        &self.creator_user_name
    }

    #[must_use]
    pub const fn creator_auth_revision(&self) -> i64 {
        self.creator_auth_revision
    }

    #[must_use]
    pub const fn token_digest(&self) -> &[u8; 32] {
        &self.token_digest
    }

    #[must_use]
    pub const fn envelope(&self) -> &CredentialEnvelope {
        &self.envelope
    }

    #[must_use]
    pub fn app_name(&self) -> &str {
        &self.app_name
    }

    #[must_use]
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    #[must_use]
    pub const fn last_used_at(&self) -> Option<DateTime<Utc>> {
        self.last_used_at
    }
}

pub struct ApiKeyRepository<'connection> {
    database: &'connection DatabaseConnection,
}

impl<'connection> ApiKeyRepository<'connection> {
    #[must_use]
    pub const fn new(database: &'connection DatabaseConnection) -> Self {
        Self { database }
    }

    /// Persists one encrypted API-key credential for the current administrator snapshot.
    ///
    /// # Errors
    ///
    /// Returns an actor fence, capacity, uniqueness, validation, or database error.
    pub async fn create(
        &self,
        actor: &AuthUser,
        draft: ApiKeyDraft,
    ) -> Result<(), ApiKeyRepositoryError> {
        let transaction = self.database.begin().await?;
        let result = create_on(&transaction, actor, draft).await;
        finish(transaction, result).await
    }

    /// Lists all durable API keys in deterministic newest-first order.
    ///
    /// # Errors
    ///
    /// Returns an explicit actor error or a stored-row/database error.
    pub async fn list(&self, actor: &AuthUser) -> Result<Vec<StoredApiKey>, ApiKeyRepositoryError> {
        let transaction = self.database.begin().await?;
        let result = list_on(&transaction, actor).await;
        finish(transaction, result).await
    }

    /// Deletes the key with the supplied digest. Missing keys are successful no-ops.
    ///
    /// # Errors
    ///
    /// Returns an explicit actor error or a database error.
    pub async fn delete_by_digest(
        &self,
        actor: &AuthUser,
        digest: &[u8; 32],
    ) -> Result<(), ApiKeyRepositoryError> {
        let transaction = self.database.begin().await?;
        let result = delete_by_digest_on(&transaction, actor, digest).await;
        finish(transaction, result).await
    }

    /// Finds the enabled administrator authenticated by a binary API-key digest.
    ///
    /// # Errors
    ///
    /// Returns a database or stored-user decoding error. Encrypted payloads are not read.
    pub async fn find_principal_by_token_digest(
        &self,
        digest: &[u8; 32],
        now: DateTime<Utc>,
    ) -> Result<Option<AuthenticatedPrincipal>, ApiKeyRepositoryError> {
        let Some(creator_user_id) = candidate_creator_user_id(self.database, digest).await? else {
            return Ok(None);
        };
        let transaction = self.database.begin().await?;
        let result = find_principal_on(&transaction, digest, now, creator_user_id).await;
        finish(transaction, result).await
    }

    /// Loads the bounded encrypted key set required during startup.
    ///
    /// # Errors
    ///
    /// Returns a stored-capacity, stored-row, or database error. This method never decrypts
    /// envelopes.
    pub async fn list_for_startup(&self) -> Result<Vec<StoredApiKey>, ApiKeyRepositoryError> {
        let stored = query_stored_keys(self.database, STARTUP_API_KEY_QUERY_LIMIT).await?;
        if stored.len() > usize::from(MAX_API_KEYS) {
            return Err(ApiKeyRepositoryError::StoredCapacityExceeded);
        }
        Ok(stored)
    }
}

async fn candidate_creator_user_id(
    database: &DatabaseConnection,
    digest: &[u8; 32],
) -> Result<Option<UserId>, ApiKeyRepositoryError> {
    let query = Query::select()
        .column(Alias::new("creator_user_id"))
        .from(Alias::new("api_keys"))
        .and_where(Expr::col(Alias::new("token_digest")).eq(digest.to_vec()))
        .limit(1)
        .to_owned();
    database
        .query_one(database.get_database_backend().build(&query))
        .await?
        .map(|row| {
            row.try_get("", "creator_user_id")
                .map(UserId::from_uuid)
                .map_err(Into::into)
        })
        .transpose()
}

async fn find_principal_on(
    transaction: &DatabaseTransaction,
    digest: &[u8; 32],
    now: DateTime<Utc>,
    creator_user_id: UserId,
) -> Result<Option<AuthenticatedPrincipal>, ApiKeyRepositoryError> {
    lock_lookup_user(transaction, creator_user_id).await?;
    let query = principal_query(digest);
    let backend = transaction.get_database_backend();
    let Some(row) = transaction.query_one(backend.build(&query)).await? else {
        return Ok(None);
    };
    let id = row.try_get("", "api_key_id")?;
    let principal = AuthenticatedPrincipal::for_api_key(AuthUser::from_database_row(&row)?, id);
    touch_activity(transaction, id, now).await?;
    Ok(Some(principal))
}

async fn lock_lookup_user(
    transaction: &DatabaseTransaction,
    creator_user_id: UserId,
) -> Result<(), ApiKeyRepositoryError> {
    let update = Query::update()
        .table(Alias::new("users"))
        .value(
            Alias::new("auth_revision"),
            Expr::col(Alias::new("auth_revision")),
        )
        .and_where(Expr::col(Alias::new("id")).eq(creator_user_id.as_uuid()))
        .to_owned();
    transaction
        .execute(transaction.get_database_backend().build(&update))
        .await?;
    Ok(())
}

fn principal_query(digest: &[u8; 32]) -> SelectStatement {
    let keys = Alias::new("api_keys");
    let users = Alias::new("users");
    Query::select()
        .expr_as(
            Expr::col((keys.clone(), Alias::new("id"))),
            Alias::new("api_key_id"),
        )
        .expr_as(
            Expr::col((users.clone(), Alias::new("id"))),
            Alias::new("id"),
        )
        .expr_as(
            Expr::col((users.clone(), Alias::new("username"))),
            Alias::new("username"),
        )
        .expr_as(
            Expr::col((users.clone(), Alias::new("is_admin"))),
            Alias::new("is_admin"),
        )
        .expr_as(
            Expr::col((users.clone(), Alias::new("has_password"))),
            Alias::new("has_password"),
        )
        .expr_as(
            Expr::col((users.clone(), Alias::new("disabled_at"))),
            Alias::new("disabled_at"),
        )
        .expr_as(
            Expr::col((users.clone(), Alias::new("auth_revision"))),
            Alias::new("auth_revision"),
        )
        .from(keys.clone())
        .join(
            JoinType::InnerJoin,
            users.clone(),
            Expr::col((users.clone(), Alias::new("id")))
                .equals((keys.clone(), Alias::new("creator_user_id"))),
        )
        .and_where(Expr::col((keys.clone(), Alias::new("token_digest"))).eq(digest.to_vec()))
        .and_where(Expr::col((users.clone(), Alias::new("is_admin"))).eq(true))
        .and_where(Expr::col((users.clone(), Alias::new("disabled_at"))).is_null())
        .and_where(
            Expr::col((users.clone(), Alias::new("auth_revision")))
                .equals((keys, Alias::new("creator_auth_revision"))),
        )
        .lock(LockType::Update)
        .limit(1)
        .to_owned()
}

#[derive(Debug, Error)]
pub enum ApiKeyRepositoryError {
    #[error("administrator snapshot was rejected")]
    ActorRejected,
    #[error("API key application name is invalid")]
    InvalidAppName,
    #[error("stored API key application name is invalid")]
    InvalidStoredAppName,
    #[error("stored API key digest is invalid")]
    InvalidStoredDigest,
    #[error("stored API key row could not be decoded")]
    InvalidStoredRow(#[source] DbErr),
    #[error("stored API key envelope is malformed")]
    InvalidStoredEnvelope(#[from] CredentialCipherError),
    #[error("API key capacity has been reached")]
    CapacityReached,
    #[error("stored API key capacity exceeds the supported limit")]
    StoredCapacityExceeded,
    #[error("API key capacity lock state is missing")]
    MissingCapacityState,
    #[error("API key envelope or digest already exists")]
    DuplicateCredential,
    #[error("API key database operation failed: {0}")]
    Database(#[from] DbErr),
    #[error("rollback failed after {original}: {rollback}")]
    RollbackFailed { original: String, rollback: DbErr },
}

async fn create_on(
    transaction: &DatabaseTransaction,
    actor: &AuthUser,
    draft: ApiKeyDraft,
) -> Result<(), ApiKeyRepositoryError> {
    lock_capacity(transaction).await?;
    fence_actor(transaction, actor).await?;
    if draft.creator_user_id != actor.id() || draft.creator_auth_revision != actor.auth_revision() {
        return Err(ApiKeyRepositoryError::ActorRejected);
    }
    if !valid_app_name(&draft.app_name) {
        return Err(ApiKeyRepositoryError::InvalidAppName);
    }

    let count = Query::select()
        .expr_as(Expr::col(Alias::new("id")).count(), Alias::new("count"))
        .from(Alias::new("api_keys"))
        .to_owned();
    let backend = transaction.get_database_backend();
    let count = transaction
        .query_one(backend.build(&count))
        .await?
        .ok_or_else(|| DbErr::Custom("API key capacity query returned no row".to_owned()))?
        .try_get::<i64>("", "count")?;
    if count >= i64::from(MAX_API_KEYS) {
        return Err(ApiKeyRepositoryError::CapacityReached);
    }

    let insert = Query::insert()
        .into_table(Alias::new("api_keys"))
        .columns([
            Alias::new("envelope_id"),
            Alias::new("creator_user_id"),
            Alias::new("creator_auth_revision"),
            Alias::new("token_digest"),
            Alias::new("encrypted_payload"),
            Alias::new("key_version"),
            Alias::new("app_name"),
            Alias::new("created_at"),
        ])
        .values_panic([
            draft.envelope_id.into(),
            draft.creator_user_id.as_uuid().into(),
            draft.creator_auth_revision.into(),
            draft.token_digest.to_vec().into(),
            draft.envelope.payload().to_vec().into(),
            draft.envelope.key_version().into(),
            draft.app_name.into(),
            draft.created_at.into(),
        ])
        .to_owned();
    let result = transaction.execute(backend.build(&insert)).await;
    if result
        .as_ref()
        .err()
        .and_then(DbErr::sql_err)
        .is_some_and(|error| matches!(error, SqlErr::UniqueConstraintViolation(_)))
    {
        return Err(ApiKeyRepositoryError::DuplicateCredential);
    }
    result?;
    Ok(())
}

async fn list_on(
    transaction: &DatabaseTransaction,
    actor: &AuthUser,
) -> Result<Vec<StoredApiKey>, ApiKeyRepositoryError> {
    fence_actor(transaction, actor).await?;
    query_stored_keys(transaction, u64::from(MAX_API_KEYS)).await
}

async fn delete_by_digest_on(
    transaction: &DatabaseTransaction,
    actor: &AuthUser,
    digest: &[u8; 32],
) -> Result<(), ApiKeyRepositoryError> {
    fence_actor(transaction, actor).await?;
    let delete = Query::delete()
        .from_table(Alias::new("api_keys"))
        .and_where(Expr::col(Alias::new("token_digest")).eq(digest.to_vec()))
        .to_owned();
    transaction
        .execute(transaction.get_database_backend().build(&delete))
        .await?;
    Ok(())
}

async fn fence_actor(
    transaction: &DatabaseTransaction,
    actor: &AuthUser,
) -> Result<(), ApiKeyRepositoryError> {
    let update = Query::update()
        .table(Alias::new("users"))
        .value(
            Alias::new("auth_revision"),
            Expr::col(Alias::new("auth_revision")),
        )
        .and_where(Expr::col(Alias::new("id")).eq(actor.id().as_uuid()))
        .and_where(Expr::col(Alias::new("auth_revision")).eq(actor.auth_revision()))
        .and_where(Expr::col(Alias::new("is_admin")).eq(true))
        .and_where(Expr::col(Alias::new("disabled_at")).is_null())
        .to_owned();
    let backend = transaction.get_database_backend();
    transaction.execute(backend.build(&update)).await?;

    let verify = Query::select()
        .column(Alias::new("id"))
        .from(Alias::new("users"))
        .and_where(Expr::col(Alias::new("id")).eq(actor.id().as_uuid()))
        .and_where(Expr::col(Alias::new("auth_revision")).eq(actor.auth_revision()))
        .and_where(Expr::col(Alias::new("is_admin")).eq(true))
        .and_where(Expr::col(Alias::new("disabled_at")).is_null())
        .limit(1)
        .to_owned();
    if transaction
        .query_one(backend.build(&verify))
        .await?
        .is_none()
    {
        return Err(ApiKeyRepositoryError::ActorRejected);
    }
    Ok(())
}

async fn lock_capacity(transaction: &DatabaseTransaction) -> Result<(), ApiKeyRepositoryError> {
    let update = Query::update()
        .table(Alias::new("auth_state"))
        .value(
            Alias::new("bootstrap_revision"),
            Expr::col(Alias::new("bootstrap_revision")),
        )
        .and_where(Expr::col(Alias::new("id")).eq(1_i32))
        .to_owned();
    let backend = transaction.get_database_backend();
    transaction.execute(backend.build(&update)).await?;
    let verify = Query::select()
        .column(Alias::new("id"))
        .from(Alias::new("auth_state"))
        .and_where(Expr::col(Alias::new("id")).eq(1_i32))
        .limit(1)
        .to_owned();
    if transaction
        .query_one(backend.build(&verify))
        .await?
        .is_none()
    {
        return Err(ApiKeyRepositoryError::MissingCapacityState);
    }
    Ok(())
}

fn stored_key_query(limit: u64) -> SelectStatement {
    let keys = Alias::new("api_keys");
    let users = Alias::new("users");
    Query::select()
        .expr_as(
            Expr::col((keys.clone(), Alias::new("id"))),
            Alias::new("id"),
        )
        .expr_as(
            Expr::col((keys.clone(), Alias::new("envelope_id"))),
            Alias::new("envelope_id"),
        )
        .expr_as(
            Expr::col((keys.clone(), Alias::new("creator_user_id"))),
            Alias::new("creator_user_id"),
        )
        .expr_as(
            Expr::col((users.clone(), Alias::new("username"))),
            Alias::new("creator_user_name"),
        )
        .expr_as(
            Expr::col((keys.clone(), Alias::new("creator_auth_revision"))),
            Alias::new("creator_auth_revision"),
        )
        .expr_as(
            Expr::col((keys.clone(), Alias::new("token_digest"))),
            Alias::new("token_digest"),
        )
        .expr_as(
            Expr::col((keys.clone(), Alias::new("encrypted_payload"))),
            Alias::new("encrypted_payload"),
        )
        .expr_as(
            Expr::col((keys.clone(), Alias::new("key_version"))),
            Alias::new("key_version"),
        )
        .expr_as(
            Expr::col((keys.clone(), Alias::new("app_name"))),
            Alias::new("app_name"),
        )
        .expr_as(
            Expr::col((keys.clone(), Alias::new("created_at"))),
            Alias::new("created_at"),
        )
        .expr_as(
            Expr::col((keys.clone(), Alias::new("last_used_at"))),
            Alias::new("last_used_at"),
        )
        .from(keys.clone())
        .join(
            JoinType::InnerJoin,
            users.clone(),
            Expr::col((users, Alias::new("id")))
                .equals((keys.clone(), Alias::new("creator_user_id"))),
        )
        .order_by((keys.clone(), Alias::new("created_at")), Order::Desc)
        .order_by((keys, Alias::new("id")), Order::Desc)
        .limit(limit)
        .to_owned()
}

async fn query_stored_keys<Connection>(
    connection: &Connection,
    limit: u64,
) -> Result<Vec<StoredApiKey>, ApiKeyRepositoryError>
where
    Connection: ConnectionTrait,
{
    let query = stored_key_query(limit);
    connection
        .query_all(connection.get_database_backend().build(&query))
        .await?
        .iter()
        .map(stored_key_from_row)
        .collect()
}

fn stored_key_from_row(row: &QueryResult) -> Result<StoredApiKey, ApiKeyRepositoryError> {
    let digest: Vec<u8> = row
        .try_get("", "token_digest")
        .map_err(ApiKeyRepositoryError::InvalidStoredRow)?;
    let token_digest = digest
        .try_into()
        .map_err(|_| ApiKeyRepositoryError::InvalidStoredDigest)?;
    let app_name: String = row
        .try_get("", "app_name")
        .map_err(ApiKeyRepositoryError::InvalidStoredRow)?;
    if !valid_app_name(&app_name) {
        return Err(ApiKeyRepositoryError::InvalidStoredAppName);
    }
    let key_version = row
        .try_get("", "key_version")
        .map_err(ApiKeyRepositoryError::InvalidStoredRow)?;
    let encrypted_payload = row
        .try_get("", "encrypted_payload")
        .map_err(ApiKeyRepositoryError::InvalidStoredRow)?;
    Ok(StoredApiKey {
        id: row
            .try_get("", "id")
            .map_err(ApiKeyRepositoryError::InvalidStoredRow)?,
        envelope_id: row
            .try_get("", "envelope_id")
            .map_err(ApiKeyRepositoryError::InvalidStoredRow)?,
        creator_user_id: UserId::from_uuid(
            row.try_get("", "creator_user_id")
                .map_err(ApiKeyRepositoryError::InvalidStoredRow)?,
        ),
        creator_user_name: row
            .try_get("", "creator_user_name")
            .map_err(ApiKeyRepositoryError::InvalidStoredRow)?,
        creator_auth_revision: row
            .try_get("", "creator_auth_revision")
            .map_err(ApiKeyRepositoryError::InvalidStoredRow)?,
        token_digest,
        envelope: CredentialEnvelope::from_parts(key_version, encrypted_payload)?,
        app_name,
        created_at: row
            .try_get("", "created_at")
            .map_err(ApiKeyRepositoryError::InvalidStoredRow)?,
        last_used_at: row
            .try_get("", "last_used_at")
            .map_err(ApiKeyRepositoryError::InvalidStoredRow)?,
    })
}

async fn touch_activity<Connection>(
    connection: &Connection,
    id: i64,
    now: DateTime<Utc>,
) -> Result<(), ApiKeyRepositoryError>
where
    Connection: ConnectionTrait,
{
    let threshold = now - Duration::minutes(3);
    let update = Query::update()
        .table(Alias::new("api_keys"))
        .value(Alias::new("last_used_at"), now)
        .and_where(Expr::col(Alias::new("id")).eq(id))
        .cond_where(
            Cond::any()
                .add(Expr::col(Alias::new("last_used_at")).is_null())
                .add(Expr::col(Alias::new("last_used_at")).lt(threshold)),
        )
        .to_owned();
    connection
        .execute(connection.get_database_backend().build(&update))
        .await?;
    Ok(())
}

fn valid_app_name(value: &str) -> bool {
    !value.trim().is_empty()
        && value.chars().count() <= MAX_APP_NAME_CHARS
        && !value.chars().any(char::is_control)
}

pub(crate) async fn delete_for_user_on(
    transaction: &DatabaseTransaction,
    user_id: UserId,
) -> Result<(), DbErr> {
    let delete = Query::delete()
        .from_table(Alias::new("api_keys"))
        .and_where(Expr::col(Alias::new("creator_user_id")).eq(user_id.as_uuid()))
        .to_owned();
    transaction
        .execute(transaction.get_database_backend().build(&delete))
        .await?;
    Ok(())
}

async fn finish<T>(
    transaction: DatabaseTransaction,
    result: Result<T, ApiKeyRepositoryError>,
) -> Result<T, ApiKeyRepositoryError> {
    match result {
        Ok(value) => {
            transaction.commit().await?;
            Ok(value)
        }
        Err(original) => match transaction.rollback().await {
            Ok(()) => Err(original),
            Err(rollback) => Err(ApiKeyRepositoryError::RollbackFailed {
                original: original.to_string(),
                rollback,
            }),
        },
    }
}

#[cfg(test)]
mod tests {
    use sea_orm::sea_query::{MysqlQueryBuilder, PostgresQueryBuilder, SqliteQueryBuilder};

    use super::principal_query;

    #[test]
    fn principal_lookup_uses_current_row_locks_on_locking_backends() {
        let query = principal_query(&[7; 32]);
        assert!(
            query
                .to_string(PostgresQueryBuilder)
                .ends_with("FOR UPDATE")
        );
        assert!(query.to_string(MysqlQueryBuilder).ends_with("FOR UPDATE"));
        assert!(!query.to_string(SqliteQueryBuilder).contains("FOR UPDATE"));
    }
}
