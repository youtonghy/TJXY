use chrono::{DateTime, Utc};
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbErr, QueryResult, TransactionTrait,
    sea_query::{Alias, Expr, JoinType, Query},
};
use thiserror::Error;
use tjxy_common::{UserId, Username};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq)]
pub struct SessionCapabilitiesDraft {
    pub playable_media_types: Vec<String>,
    pub supported_commands: Vec<String>,
    pub supports_media_control: bool,
    pub supports_persistent_identifier: bool,
    pub device_profile: Option<serde_json::Value>,
    pub app_store_url: Option<String>,
    pub icon_url: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthUser {
    id: UserId,
    name: String,
    is_admin: bool,
    has_password: bool,
    is_disabled: bool,
    auth_revision: i64,
}

impl AuthUser {
    #[must_use]
    pub const fn id(&self) -> UserId {
        self.id
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub const fn is_admin(&self) -> bool {
        self.is_admin
    }

    #[must_use]
    pub const fn has_password(&self) -> bool {
        self.has_password
    }

    #[must_use]
    pub const fn is_disabled(&self) -> bool {
        self.is_disabled
    }

    #[must_use]
    pub const fn auth_revision(&self) -> i64 {
        self.auth_revision
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CredentialSnapshot {
    user: AuthUser,
    password_hash: String,
}

impl CredentialSnapshot {
    #[must_use]
    pub const fn user(&self) -> &AuthUser {
        &self.user
    }

    #[must_use]
    pub fn password_hash(&self) -> &str {
        &self.password_hash
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionDraft {
    pub id: Uuid,
    pub token_digest: [u8; 32],
    pub device_id: String,
    pub device_name: String,
    pub client_name: String,
    pub client_version: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IssuedSession {
    id: Uuid,
    user_id: UserId,
    expires_at: Option<DateTime<Utc>>,
}

impl IssuedSession {
    #[must_use]
    pub const fn id(&self) -> Uuid {
        self.id
    }

    #[must_use]
    pub const fn user_id(&self) -> UserId {
        self.user_id
    }

    #[must_use]
    pub const fn expires_at(&self) -> Option<DateTime<Utc>> {
        self.expires_at
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthenticatedPrincipal {
    user: AuthUser,
    session_id: Uuid,
    device_id: String,
}

impl AuthenticatedPrincipal {
    #[must_use]
    pub const fn user(&self) -> &AuthUser {
        &self.user
    }

    #[must_use]
    pub const fn session_id(&self) -> Uuid {
        self.session_id
    }

    #[must_use]
    pub fn device_id(&self) -> &str {
        &self.device_id
    }
}

pub struct AuthRepository<'connection> {
    database: &'connection DatabaseConnection,
}

impl<'connection> AuthRepository<'connection> {
    #[must_use]
    pub const fn new(database: &'connection DatabaseConnection) -> Self {
        Self { database }
    }

    /// Creates a local user whose password is already encoded as an Argon2 PHC string.
    ///
    /// # Errors
    ///
    /// Returns [`AuthRepositoryError`] if the hash is empty or SQL rejects the user.
    pub async fn create_user(
        &self,
        username: &Username,
        password_hash: &str,
        has_password: bool,
        is_admin: bool,
        now: DateTime<Utc>,
    ) -> Result<AuthUser, AuthRepositoryError> {
        if password_hash.is_empty() {
            return Err(AuthRepositoryError::EmptyPasswordHash);
        }
        let user = new_user(username, has_password, is_admin);
        insert_user(self.database, username, password_hash, &user, now).await?;
        Ok(user)
    }

    /// Atomically creates the first enabled administrator, if one is still absent.
    ///
    /// # Errors
    ///
    /// Returns [`AuthRepositoryError`] when the bootstrap lock, insert, commit,
    /// or rollback fails.
    pub async fn create_initial_admin(
        &self,
        username: &Username,
        password_hash: &str,
        now: DateTime<Utc>,
    ) -> Result<Option<AuthUser>, AuthRepositoryError> {
        if password_hash.is_empty() {
            return Err(AuthRepositoryError::EmptyPasswordHash);
        }
        let transaction = self.database.begin().await?;
        let result =
            create_initial_admin_in_transaction(&transaction, username, password_hash, now).await;
        finish(transaction, result).await
    }

    /// Finds the current credential snapshot for username/password verification.
    ///
    /// # Errors
    ///
    /// Returns [`AuthRepositoryError`] when SQL or row decoding fails.
    pub async fn find_credential(
        &self,
        username: &Username,
    ) -> Result<Option<CredentialSnapshot>, AuthRepositoryError> {
        let statement = Query::select()
            .columns([
                Alias::new("id"),
                Alias::new("username"),
                Alias::new("password_hash"),
                Alias::new("is_admin"),
                Alias::new("has_password"),
                Alias::new("disabled_at"),
                Alias::new("auth_revision"),
            ])
            .from(Alias::new("users"))
            .and_where(Expr::col(Alias::new("username_key")).eq(username.key().to_vec()))
            .to_owned();
        let backend = self.database.get_database_backend();
        self.database
            .query_one(backend.build(&statement))
            .await?
            .map(|row| credential_from_row(&row))
            .transpose()
    }

    /// Atomically rechecks a verified credential snapshot and inserts a session.
    ///
    /// # Errors
    ///
    /// Returns [`AuthRepositoryError::CredentialChanged`] if the account was
    /// disabled or changed after password verification, plus database/rollback errors.
    pub async fn issue_session(
        &self,
        credential: &CredentialSnapshot,
        session: SessionDraft,
    ) -> Result<IssuedSession, AuthRepositoryError> {
        validate_session(&session)?;
        let transaction = self.database.begin().await?;
        let result = issue_session_in_transaction(&transaction, credential, &session).await;
        finish(transaction, result).await
    }

    /// Resolves a non-revoked, unexpired session whose auth revision still
    /// matches the enabled user.
    ///
    /// # Errors
    ///
    /// Returns [`AuthRepositoryError`] when SQL or row decoding fails.
    pub async fn find_principal_by_token_digest(
        &self,
        token_digest: &[u8; 32],
        now: DateTime<Utc>,
    ) -> Result<Option<AuthenticatedPrincipal>, AuthRepositoryError> {
        let sessions = Alias::new("auth_sessions");
        let users = Alias::new("users");
        let statement = Query::select()
            .expr_as(
                Expr::col((sessions.clone(), Alias::new("id"))),
                Alias::new("session_id"),
            )
            .expr_as(
                Expr::col((sessions.clone(), Alias::new("device_id"))),
                Alias::new("device_id"),
            )
            .expr_as(
                Expr::col((users.clone(), Alias::new("id"))),
                Alias::new("user_id"),
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
            .from(sessions.clone())
            .join(
                JoinType::InnerJoin,
                users.clone(),
                Expr::col((sessions.clone(), Alias::new("user_id")))
                    .equals((users.clone(), Alias::new("id"))),
            )
            .and_where(
                Expr::col((sessions.clone(), Alias::new("token_digest"))).eq(token_digest.to_vec()),
            )
            .and_where(Expr::col((sessions.clone(), Alias::new("revoked_at"))).is_null())
            .and_where(
                Expr::col((sessions.clone(), Alias::new("expires_at")))
                    .is_null()
                    .or(Expr::col((sessions.clone(), Alias::new("expires_at"))).gt(now)),
            )
            .and_where(
                Expr::col((sessions, Alias::new("auth_revision")))
                    .equals((users, Alias::new("auth_revision"))),
            )
            .to_owned();
        let backend = self.database.get_database_backend();
        self.database
            .query_one(backend.build(&statement))
            .await?
            .map(|row| principal_from_row(&row))
            .transpose()
    }

    /// Reports whether at least one local user exists.
    ///
    /// # Errors
    ///
    /// Returns [`AuthRepositoryError`] when the query fails.
    pub async fn has_users(&self) -> Result<bool, AuthRepositoryError> {
        let statement = Query::select()
            .expr(Expr::col(Alias::new("id")))
            .from(Alias::new("users"))
            .limit(1)
            .to_owned();
        let backend = self.database.get_database_backend();
        Ok(self
            .database
            .query_one(backend.build(&statement))
            .await?
            .is_some())
    }

    /// Reports whether an enabled local administrator exists.
    ///
    /// # Errors
    ///
    /// Returns [`AuthRepositoryError`] when the query fails.
    pub async fn has_enabled_admin(&self) -> Result<bool, AuthRepositoryError> {
        has_enabled_admin_on(self.database).await
    }

    /// Replaces the capabilities reported by one active session owned by the user.
    ///
    /// # Errors
    ///
    /// Returns [`AuthRepositoryError`] when the atomic update fails.
    pub async fn update_session_capabilities(
        &self,
        user_id: UserId,
        session_id: Uuid,
        capabilities: SessionCapabilitiesDraft,
    ) -> Result<bool, AuthRepositoryError> {
        let playable_media_types = serde_json::Value::Array(
            capabilities
                .playable_media_types
                .into_iter()
                .map(serde_json::Value::String)
                .collect(),
        );
        let supported_commands = serde_json::Value::Array(
            capabilities
                .supported_commands
                .into_iter()
                .map(serde_json::Value::String)
                .collect(),
        );
        let statement = Query::update()
            .table(Alias::new("auth_sessions"))
            .values([
                (
                    Alias::new("playable_media_types"),
                    playable_media_types.into(),
                ),
                (Alias::new("supported_commands"), supported_commands.into()),
                (
                    Alias::new("supports_media_control"),
                    capabilities.supports_media_control.into(),
                ),
                (
                    Alias::new("supports_persistent_identifier"),
                    capabilities.supports_persistent_identifier.into(),
                ),
                (
                    Alias::new("device_profile"),
                    capabilities.device_profile.into(),
                ),
                (
                    Alias::new("app_store_url"),
                    capabilities.app_store_url.into(),
                ),
                (Alias::new("icon_url"), capabilities.icon_url.into()),
            ])
            .and_where(Expr::col(Alias::new("id")).eq(session_id))
            .and_where(Expr::col(Alias::new("user_id")).eq(user_id.as_uuid()))
            .and_where(Expr::col(Alias::new("revoked_at")).is_null())
            .to_owned();
        let backend = self.database.get_database_backend();
        Ok(self
            .database
            .execute(backend.build(&statement))
            .await?
            .rows_affected()
            == 1)
    }
}

#[derive(Debug, Error)]
pub enum AuthRepositoryError {
    #[error("password hash must not be empty")]
    EmptyPasswordHash,
    #[error("session metadata contains an empty or oversized value")]
    InvalidSessionMetadata,
    #[error("session expiry must be after its creation time")]
    InvalidSessionExpiry,
    #[error("credential changed while the session was being issued")]
    CredentialChanged,
    #[error("authentication bootstrap state is missing")]
    MissingBootstrapState,
    #[error("database operation failed: {0}")]
    Database(#[from] DbErr),
    #[error("rollback failed after {original}: {rollback}")]
    RollbackFailed { original: String, rollback: DbErr },
}

fn new_user(username: &Username, has_password: bool, is_admin: bool) -> AuthUser {
    AuthUser {
        id: UserId::new(),
        name: username.as_str().to_owned(),
        is_admin,
        has_password,
        is_disabled: false,
        auth_revision: 0,
    }
}

async fn insert_user<Connection>(
    connection: &Connection,
    username: &Username,
    password_hash: &str,
    user: &AuthUser,
    now: DateTime<Utc>,
) -> Result<(), AuthRepositoryError>
where
    Connection: ConnectionTrait,
{
    let statement = Query::insert()
        .into_table(Alias::new("users"))
        .columns([
            Alias::new("id"),
            Alias::new("username"),
            Alias::new("username_key"),
            Alias::new("password_hash"),
            Alias::new("is_admin"),
            Alias::new("has_password"),
            Alias::new("disabled_at"),
            Alias::new("auth_revision"),
            Alias::new("created_at"),
            Alias::new("updated_at"),
        ])
        .values_panic([
            user.id.as_uuid().into(),
            user.name.clone().into(),
            username.key().to_vec().into(),
            password_hash.into(),
            user.is_admin.into(),
            user.has_password.into(),
            Option::<DateTime<Utc>>::None.into(),
            0_i64.into(),
            now.into(),
            now.into(),
        ])
        .to_owned();
    let backend = connection.get_database_backend();
    connection.execute(backend.build(&statement)).await?;
    Ok(())
}

async fn has_enabled_admin_on<Connection>(
    connection: &Connection,
) -> Result<bool, AuthRepositoryError>
where
    Connection: ConnectionTrait,
{
    let statement = Query::select()
        .expr(Expr::col(Alias::new("id")))
        .from(Alias::new("users"))
        .and_where(Expr::col(Alias::new("is_admin")).eq(true))
        .and_where(Expr::col(Alias::new("disabled_at")).is_null())
        .limit(1)
        .to_owned();
    let backend = connection.get_database_backend();
    Ok(connection
        .query_one(backend.build(&statement))
        .await?
        .is_some())
}

async fn create_initial_admin_in_transaction(
    transaction: &DatabaseTransaction,
    username: &Username,
    password_hash: &str,
    now: DateTime<Utc>,
) -> Result<Option<AuthUser>, AuthRepositoryError> {
    let update = Query::update()
        .table(Alias::new("auth_state"))
        .value(
            Alias::new("bootstrap_revision"),
            Expr::col(Alias::new("bootstrap_revision")).add(1_i64),
        )
        .and_where(Expr::col(Alias::new("id")).eq(1_i32))
        .to_owned();
    let backend = transaction.get_database_backend();
    if transaction
        .execute(backend.build(&update))
        .await?
        .rows_affected()
        != 1
    {
        return Err(AuthRepositoryError::MissingBootstrapState);
    }
    if has_enabled_admin_on(transaction).await? {
        return Ok(None);
    }
    let user = new_user(username, true, true);
    insert_user(transaction, username, password_hash, &user, now).await?;
    Ok(Some(user))
}

fn validate_session(session: &SessionDraft) -> Result<(), AuthRepositoryError> {
    for (value, maximum) in [
        (&session.device_id, 512),
        (&session.device_name, 256),
        (&session.client_name, 256),
        (&session.client_version, 128),
    ] {
        if value.is_empty()
            || value.chars().count() > maximum
            || value.chars().any(char::is_control)
        {
            return Err(AuthRepositoryError::InvalidSessionMetadata);
        }
    }
    if let Some(expires_at) = session.expires_at {
        if expires_at <= session.created_at {
            return Err(AuthRepositoryError::InvalidSessionExpiry);
        }
    }
    Ok(())
}

async fn issue_session_in_transaction(
    transaction: &DatabaseTransaction,
    credential: &CredentialSnapshot,
    session: &SessionDraft,
) -> Result<IssuedSession, AuthRepositoryError> {
    let user = credential.user();
    let update = Query::update()
        .table(Alias::new("users"))
        .value(Alias::new("last_login_at"), session.created_at)
        .value(Alias::new("last_activity_at"), session.created_at)
        .value(Alias::new("updated_at"), session.created_at)
        .and_where(Expr::col(Alias::new("id")).eq(user.id.as_uuid()))
        .and_where(Expr::col(Alias::new("auth_revision")).eq(user.auth_revision))
        .and_where(Expr::col(Alias::new("password_hash")).eq(&credential.password_hash))
        .and_where(Expr::col(Alias::new("disabled_at")).is_null())
        .to_owned();
    let backend = transaction.get_database_backend();
    if transaction
        .execute(backend.build(&update))
        .await?
        .rows_affected()
        != 1
    {
        return Err(AuthRepositoryError::CredentialChanged);
    }

    let insert = Query::insert()
        .into_table(Alias::new("auth_sessions"))
        .columns([
            Alias::new("id"),
            Alias::new("user_id"),
            Alias::new("token_digest"),
            Alias::new("auth_revision"),
            Alias::new("device_id"),
            Alias::new("device_name"),
            Alias::new("client_name"),
            Alias::new("client_version"),
            Alias::new("created_at"),
            Alias::new("expires_at"),
        ])
        .values_panic([
            session.id.into(),
            user.id.as_uuid().into(),
            session.token_digest.to_vec().into(),
            user.auth_revision.into(),
            session.device_id.clone().into(),
            session.device_name.clone().into(),
            session.client_name.clone().into(),
            session.client_version.clone().into(),
            session.created_at.into(),
            session.expires_at.into(),
        ])
        .to_owned();
    transaction.execute(backend.build(&insert)).await?;
    Ok(IssuedSession {
        id: session.id,
        user_id: user.id,
        expires_at: session.expires_at,
    })
}

async fn finish<T>(
    transaction: DatabaseTransaction,
    result: Result<T, AuthRepositoryError>,
) -> Result<T, AuthRepositoryError> {
    match result {
        Ok(value) => {
            transaction.commit().await?;
            Ok(value)
        }
        Err(original) => match transaction.rollback().await {
            Ok(()) => Err(original),
            Err(rollback) => Err(AuthRepositoryError::RollbackFailed {
                original: original.to_string(),
                rollback,
            }),
        },
    }
}

fn credential_from_row(row: &QueryResult) -> Result<CredentialSnapshot, AuthRepositoryError> {
    Ok(CredentialSnapshot {
        user: AuthUser {
            id: UserId::from_uuid(row.try_get("", "id")?),
            name: row.try_get("", "username")?,
            is_admin: row.try_get("", "is_admin")?,
            has_password: row.try_get("", "has_password")?,
            is_disabled: row
                .try_get::<Option<DateTime<Utc>>>("", "disabled_at")?
                .is_some(),
            auth_revision: row.try_get("", "auth_revision")?,
        },
        password_hash: row.try_get("", "password_hash")?,
    })
}

fn principal_from_row(row: &QueryResult) -> Result<AuthenticatedPrincipal, AuthRepositoryError> {
    Ok(AuthenticatedPrincipal {
        user: AuthUser {
            id: UserId::from_uuid(row.try_get("", "user_id")?),
            name: row.try_get("", "username")?,
            is_admin: row.try_get("", "is_admin")?,
            has_password: row.try_get("", "has_password")?,
            is_disabled: row
                .try_get::<Option<DateTime<Utc>>>("", "disabled_at")?
                .is_some(),
            auth_revision: row.try_get("", "auth_revision")?,
        },
        session_id: row.try_get("", "session_id")?,
        device_id: row.try_get("", "device_id")?,
    })
}
