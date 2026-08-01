use chrono::{DateTime, Duration, Utc};
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbErr, QueryResult, SqlErr,
    TransactionTrait,
    sea_query::{Alias, Cond, Expr, JoinType, Order, Query},
};
use thiserror::Error;
use tjxy_common::{UserId, Username};
use uuid::Uuid;

use crate::api_key::delete_for_user_on;

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
    pub(crate) fn from_database_row(row: &QueryResult) -> Result<Self, DbErr> {
        Ok(Self {
            id: UserId::from_uuid(row.try_get("", "id")?),
            name: row.try_get("", "username")?,
            is_admin: row.try_get("", "is_admin")?,
            has_password: row.try_get("", "has_password")?,
            is_disabled: row
                .try_get::<Option<DateTime<Utc>>>("", "disabled_at")?
                .is_some(),
            auth_revision: row.try_get("", "auth_revision")?,
        })
    }

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
    origin: AuthenticationOrigin,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AuthenticationOrigin {
    Session { id: Uuid, device_id: String },
    ApiKey { id: i64 },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthSessionRecord {
    id: Uuid,
    user_id: UserId,
    user_name: String,
    device_id: String,
    device_name: String,
    client_name: String,
    client_version: String,
    created_at: DateTime<Utc>,
    last_seen_at: Option<DateTime<Utc>>,
    playable_media_types: Vec<String>,
    supported_commands: Vec<String>,
    supports_media_control: bool,
    supports_persistent_identifier: bool,
}

impl AuthSessionRecord {
    #[must_use]
    pub const fn id(&self) -> Uuid {
        self.id
    }

    #[must_use]
    pub const fn user_id(&self) -> UserId {
        self.user_id
    }

    #[must_use]
    pub fn user_name(&self) -> &str {
        &self.user_name
    }

    #[must_use]
    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    #[must_use]
    pub fn device_name(&self) -> &str {
        &self.device_name
    }

    #[must_use]
    pub fn client_name(&self) -> &str {
        &self.client_name
    }

    #[must_use]
    pub fn client_version(&self) -> &str {
        &self.client_version
    }

    #[must_use]
    pub fn last_activity_at(&self) -> DateTime<Utc> {
        self.last_seen_at.unwrap_or(self.created_at)
    }

    #[must_use]
    pub fn playable_media_types(&self) -> &[String] {
        &self.playable_media_types
    }

    #[must_use]
    pub fn supported_commands(&self) -> &[String] {
        &self.supported_commands
    }

    #[must_use]
    pub const fn supports_media_control(&self) -> bool {
        self.supports_media_control
    }

    #[must_use]
    pub const fn supports_persistent_identifier(&self) -> bool {
        self.supports_persistent_identifier
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AuthSessionQuery {
    pub visible_user_id: Option<UserId>,
    pub controllable_by_user_id: Option<UserId>,
    pub device_id: Option<String>,
    pub active_after: Option<DateTime<Utc>>,
}

impl AuthenticatedPrincipal {
    #[allow(dead_code)] // Consumed by the API-key repository introduced in the next task.
    pub(crate) const fn for_api_key(user: AuthUser, id: i64) -> Self {
        Self {
            user,
            origin: AuthenticationOrigin::ApiKey { id },
        }
    }

    #[must_use]
    pub const fn user(&self) -> &AuthUser {
        &self.user
    }

    #[must_use]
    pub const fn session_id(&self) -> Option<Uuid> {
        match &self.origin {
            AuthenticationOrigin::Session { id, .. } => Some(*id),
            AuthenticationOrigin::ApiKey { .. } => None,
        }
    }

    #[must_use]
    pub fn device_id(&self) -> Option<&str> {
        match &self.origin {
            AuthenticationOrigin::Session { device_id, .. } => Some(device_id),
            AuthenticationOrigin::ApiKey { .. } => None,
        }
    }

    #[must_use]
    pub const fn api_key_id(&self) -> Option<i64> {
        match &self.origin {
            AuthenticationOrigin::Session { .. } => None,
            AuthenticationOrigin::ApiKey { id } => Some(*id),
        }
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

    /// Lists local users in stable username order.
    ///
    /// # Errors
    ///
    /// Returns [`AuthRepositoryError`] when SQL or stored user data is invalid.
    pub async fn list_users(&self) -> Result<Vec<AuthUser>, AuthRepositoryError> {
        let query = user_query()
            .order_by(Alias::new("username"), sea_orm::Order::Asc)
            .to_owned();
        let backend = self.database.get_database_backend();
        self.database
            .query_all(backend.build(&query))
            .await?
            .iter()
            .map(auth_user_from_row)
            .collect()
    }

    /// Lists a bounded, stable set of enabled local user ids for background work.
    ///
    /// # Errors
    ///
    /// Returns [`AuthRepositoryError`] when SQL rejects the query or stored ids are invalid.
    pub async fn enabled_user_ids(&self, limit: u64) -> Result<Vec<UserId>, AuthRepositoryError> {
        let query = Query::select()
            .column(Alias::new("id"))
            .from(Alias::new("users"))
            .and_where(Expr::col(Alias::new("disabled_at")).is_null())
            .order_by(Alias::new("username"), sea_orm::Order::Asc)
            .limit(limit)
            .to_owned();
        let backend = self.database.get_database_backend();
        self.database
            .query_all(backend.build(&query))
            .await?
            .iter()
            .map(|row| row.try_get("", "id").map(UserId::from_uuid))
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    /// Reads one local user by stable id.
    ///
    /// # Errors
    ///
    /// Returns [`AuthRepositoryError`] when SQL or stored user data is invalid.
    pub async fn get_user(&self, user_id: UserId) -> Result<Option<AuthUser>, AuthRepositoryError> {
        get_user_on(self.database, user_id).await
    }

    /// Reads the optional user biography used by the client profile.
    ///
    /// # Errors
    ///
    /// Returns [`AuthRepositoryError`] when the user is missing or stored data is invalid.
    pub async fn user_bio(&self, user_id: UserId) -> Result<String, AuthRepositoryError> {
        let query = Query::select()
            .column(Alias::new("bio"))
            .from(Alias::new("users"))
            .and_where(Expr::col(Alias::new("id")).eq(user_id.as_uuid()))
            .limit(1)
            .to_owned();
        self.database
            .query_one(self.database.get_database_backend().build(&query))
            .await?
            .ok_or(AuthRepositoryError::UserNotFound)?
            .try_get::<Option<String>>("", "bio")
            .map(Option::unwrap_or_default)
            .map_err(Into::into)
    }

    /// Updates a biography without changing authentication state.
    ///
    /// # Errors
    ///
    /// Returns [`AuthRepositoryError`] when the user is missing or the update fails.
    pub async fn update_bio(
        &self,
        user_id: UserId,
        bio: &str,
        now: DateTime<Utc>,
    ) -> Result<AuthUser, AuthRepositoryError> {
        let update = Query::update()
            .table(Alias::new("users"))
            .value(Alias::new("bio"), bio)
            .value(Alias::new("updated_at"), now)
            .and_where(Expr::col(Alias::new("id")).eq(user_id.as_uuid()))
            .to_owned();
        if self
            .database
            .execute(self.database.get_database_backend().build(&update))
            .await?
            .rows_affected()
            != 1
        {
            return Err(AuthRepositoryError::UserNotFound);
        }
        self.get_user(user_id)
            .await?
            .ok_or(AuthRepositoryError::UserNotFound)
    }

    /// Updates username and biography atomically and invalidates existing sessions.
    ///
    /// # Errors
    ///
    /// Returns [`AuthRepositoryError`] when validation, locking, or persistence fails.
    pub async fn update_profile(
        &self,
        user_id: UserId,
        username: &Username,
        bio: &str,
        now: DateTime<Utc>,
    ) -> Result<AuthUser, AuthRepositoryError> {
        let transaction = self.database.begin().await?;
        let result = update_user_fields(
            &transaction,
            user_id,
            [
                (Alias::new("username"), username.as_str().into()),
                (Alias::new("username_key"), username.key().to_vec().into()),
                (Alias::new("bio"), bio.into()),
                (Alias::new("updated_at"), now.into()),
            ],
        )
        .await;
        finish(transaction, result).await
    }

    /// Updates username, biography, and an optional password hash in one transaction.
    ///
    /// # Errors
    ///
    /// Returns [`AuthRepositoryError`] for an empty hash, conflict, missing user, or SQL failure.
    pub async fn update_account(
        &self,
        user_id: UserId,
        username: &Username,
        bio: &str,
        password_hash: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<AuthUser, AuthRepositoryError> {
        if password_hash.is_some_and(str::is_empty) {
            return Err(AuthRepositoryError::EmptyPasswordHash);
        }
        let transaction = self.database.begin().await?;
        let mut values = vec![
            (Alias::new("username"), username.as_str().into()),
            (Alias::new("username_key"), username.key().to_vec().into()),
            (Alias::new("bio"), bio.into()),
            (Alias::new("updated_at"), now.into()),
        ];
        if let Some(password_hash) = password_hash {
            values.push((Alias::new("password_hash"), password_hash.into()));
            values.push((Alias::new("has_password"), true.into()));
        }
        let result = update_user_fields(&transaction, user_id, values).await;
        finish(transaction, result).await
    }

    /// Renames a local user and invalidates all existing sessions.
    ///
    /// # Errors
    ///
    /// Returns [`AuthRepositoryError::UserNotFound`] when the user no longer exists.
    pub async fn rename_user(
        &self,
        user_id: UserId,
        username: &Username,
        now: DateTime<Utc>,
    ) -> Result<AuthUser, AuthRepositoryError> {
        let transaction = self.database.begin().await?;
        let result = update_user_fields(
            &transaction,
            user_id,
            [
                (Alias::new("username"), username.as_str().into()),
                (Alias::new("username_key"), username.key().to_vec().into()),
                (Alias::new("updated_at"), now.into()),
            ],
        )
        .await;
        finish(transaction, result).await
    }

    /// Replaces a local credential and invalidates all existing sessions.
    ///
    /// # Errors
    ///
    /// Returns [`AuthRepositoryError`] for an empty hash, missing user, or SQL failure.
    pub async fn update_password(
        &self,
        user_id: UserId,
        password_hash: &str,
        has_password: bool,
        now: DateTime<Utc>,
    ) -> Result<AuthUser, AuthRepositoryError> {
        if password_hash.is_empty() {
            return Err(AuthRepositoryError::EmptyPasswordHash);
        }
        let transaction = self.database.begin().await?;
        let result = update_user_fields(
            &transaction,
            user_id,
            [
                (Alias::new("password_hash"), password_hash.into()),
                (Alias::new("has_password"), has_password.into()),
                (Alias::new("updated_at"), now.into()),
            ],
        )
        .await;
        finish(transaction, result).await
    }

    /// Updates the supported administrator/disabled policy and invalidates existing sessions.
    ///
    /// # Errors
    ///
    /// Refuses to remove the final enabled administrator.
    pub async fn update_policy(
        &self,
        user_id: UserId,
        is_admin: bool,
        is_disabled: bool,
        now: DateTime<Utc>,
    ) -> Result<AuthUser, AuthRepositoryError> {
        let transaction = self.database.begin().await?;
        let result =
            update_policy_in_transaction(&transaction, user_id, is_admin, is_disabled, now).await;
        finish(transaction, result).await
    }

    /// Deletes a local user and user-owned runtime state atomically.
    ///
    /// # Errors
    ///
    /// Refuses to delete the final enabled administrator or a user referenced by an import.
    pub async fn delete_user(&self, user_id: UserId) -> Result<(), AuthRepositoryError> {
        let transaction = self.database.begin().await?;
        let result = delete_user_in_transaction(&transaction, user_id).await;
        finish(transaction, result).await
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
                Expr::col((sessions.clone(), Alias::new("created_at"))),
                Alias::new("session_created_at"),
            )
            .expr_as(
                Expr::col((sessions.clone(), Alias::new("last_seen_at"))),
                Alias::new("session_last_seen_at"),
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
        let Some(row) = self.database.query_one(backend.build(&statement)).await? else {
            return Ok(None);
        };
        let session_id: Uuid = row.try_get("", "session_id")?;
        let principal = principal_from_row(&row, session_id)?;
        let created_at: DateTime<Utc> = row.try_get("", "session_created_at")?;
        let last_seen_at: Option<DateTime<Utc>> = row.try_get("", "session_last_seen_at")?;
        if !principal.user().is_disabled()
            && last_seen_at.unwrap_or(created_at) < now - Duration::minutes(3)
        {
            touch_session_activity(self.database, session_id, now).await?;
        }
        Ok(Some(principal))
    }

    /// Lists a bounded set of active sessions visible to one authenticated caller.
    ///
    /// # Errors
    ///
    /// Returns [`AuthRepositoryError`] when SQL or stored capability data is invalid.
    pub async fn list_active_sessions(
        &self,
        query: AuthSessionQuery,
        now: DateTime<Utc>,
    ) -> Result<Vec<AuthSessionRecord>, AuthRepositoryError> {
        let sessions = Alias::new("auth_sessions");
        let users = Alias::new("users");
        let mut statement = Query::select();
        statement
            .columns([
                (sessions.clone(), Alias::new("id")),
                (sessions.clone(), Alias::new("device_id")),
                (sessions.clone(), Alias::new("device_name")),
                (sessions.clone(), Alias::new("client_name")),
                (sessions.clone(), Alias::new("client_version")),
                (sessions.clone(), Alias::new("created_at")),
                (sessions.clone(), Alias::new("last_seen_at")),
                (sessions.clone(), Alias::new("playable_media_types")),
                (sessions.clone(), Alias::new("supported_commands")),
                (sessions.clone(), Alias::new("supports_media_control")),
                (
                    sessions.clone(),
                    Alias::new("supports_persistent_identifier"),
                ),
                (users.clone(), Alias::new("username")),
            ])
            .expr_as(
                Expr::col((users.clone(), Alias::new("id"))),
                Alias::new("session_user_id"),
            )
            .from(sessions.clone())
            .join(
                JoinType::InnerJoin,
                users.clone(),
                Expr::col((sessions.clone(), Alias::new("user_id")))
                    .equals((users.clone(), Alias::new("id"))),
            )
            .and_where(Expr::col((sessions.clone(), Alias::new("revoked_at"))).is_null())
            .and_where(
                Expr::col((sessions.clone(), Alias::new("expires_at")))
                    .is_null()
                    .or(Expr::col((sessions.clone(), Alias::new("expires_at"))).gt(now)),
            )
            .and_where(
                Expr::col((sessions.clone(), Alias::new("auth_revision")))
                    .equals((users.clone(), Alias::new("auth_revision"))),
            )
            .and_where(Expr::col((users.clone(), Alias::new("disabled_at"))).is_null());
        if let Some(user_id) = query.visible_user_id {
            statement.and_where(
                Expr::col((sessions.clone(), Alias::new("user_id"))).eq(user_id.as_uuid()),
            );
        }
        if let Some(user_id) = query.controllable_by_user_id {
            statement
                .and_where(
                    Expr::col((sessions.clone(), Alias::new("user_id"))).eq(user_id.as_uuid()),
                )
                .and_where(
                    Expr::col((sessions.clone(), Alias::new("supports_media_control"))).eq(true),
                );
        }
        if let Some(device_id) = query.device_id {
            statement
                .and_where(Expr::col((sessions.clone(), Alias::new("device_id"))).eq(device_id));
        }
        if let Some(active_after) = query.active_after {
            statement.cond_where(
                Cond::any()
                    .add(
                        Expr::col((sessions.clone(), Alias::new("last_seen_at"))).gte(active_after),
                    )
                    .add(
                        Cond::all()
                            .add(
                                Expr::col((sessions.clone(), Alias::new("last_seen_at"))).is_null(),
                            )
                            .add(
                                Expr::col((sessions.clone(), Alias::new("created_at")))
                                    .gte(active_after),
                            ),
                    ),
            );
        }
        statement
            .order_by((sessions.clone(), Alias::new("created_at")), Order::Desc)
            .order_by((sessions, Alias::new("id")), Order::Desc)
            .limit(512);
        let backend = self.database.get_database_backend();
        let mut sessions = self
            .database
            .query_all(backend.build(&statement))
            .await?
            .iter()
            .map(auth_session_from_row)
            .collect::<Result<Vec<_>, _>>()?;
        sessions.sort_by(|left, right| {
            right
                .last_activity_at()
                .cmp(&left.last_activity_at())
                .then_with(|| right.id().cmp(&left.id()))
        });
        Ok(sessions)
    }

    /// Revokes one active session owned by the specified user.
    ///
    /// # Errors
    ///
    /// Returns [`AuthRepositoryError`] when SQL rejects the update.
    pub async fn revoke_session(
        &self,
        user_id: UserId,
        session_id: Uuid,
        now: DateTime<Utc>,
        reason: &str,
    ) -> Result<bool, AuthRepositoryError> {
        if reason.is_empty() || reason.chars().count() > 128 || reason.chars().any(char::is_control)
        {
            return Err(AuthRepositoryError::InvalidRevokeReason);
        }
        let statement = Query::update()
            .table(Alias::new("auth_sessions"))
            .value(Alias::new("revoked_at"), now)
            .value(Alias::new("revoke_reason"), reason)
            .and_where(Expr::col(Alias::new("id")).eq(session_id))
            .and_where(Expr::col(Alias::new("user_id")).eq(user_id.as_uuid()))
            .and_where(Expr::col(Alias::new("revoked_at")).is_null())
            .to_owned();
        Ok(self
            .database
            .execute(self.database.get_database_backend().build(&statement))
            .await?
            .rows_affected()
            == 1)
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

    /// Reads the `DeviceProfile` persisted for the authenticated session.
    ///
    /// # Errors
    ///
    /// Returns [`AuthRepositoryError`] for database or stored-value failures.
    pub async fn session_device_profile(
        &self,
        user_id: UserId,
        session_id: Uuid,
    ) -> Result<Option<serde_json::Value>, AuthRepositoryError> {
        let query = Query::select()
            .column(Alias::new("device_profile"))
            .from(Alias::new("auth_sessions"))
            .and_where(Expr::col(Alias::new("id")).eq(session_id))
            .and_where(Expr::col(Alias::new("user_id")).eq(user_id.as_uuid()))
            .and_where(Expr::col(Alias::new("revoked_at")).is_null())
            .limit(1)
            .to_owned();
        let backend = self.database.get_database_backend();
        let Some(row) = self.database.query_one(backend.build(&query)).await? else {
            return Ok(None);
        };
        Ok(row.try_get("", "device_profile")?)
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
    #[error("session revoke reason is invalid")]
    InvalidRevokeReason,
    #[error("stored session capabilities are invalid")]
    InvalidStoredSessionCapabilities,
    #[error("credential changed while the session was being issued")]
    CredentialChanged,
    #[error("local user was not found")]
    UserNotFound,
    #[error("the final enabled administrator cannot be removed or disabled")]
    LastEnabledAdmin,
    #[error("local user is referenced by durable import configuration")]
    UserReferenced,
    #[error("local username already exists")]
    UsernameConflict,
    #[error("authentication bootstrap state is missing")]
    MissingBootstrapState,
    #[error("database operation failed: {0}")]
    Database(#[from] DbErr),
    #[error("rollback failed after {original}: {rollback}")]
    RollbackFailed { original: String, rollback: DbErr },
}

async fn touch_session_activity(
    database: &DatabaseConnection,
    session_id: Uuid,
    now: DateTime<Utc>,
) -> Result<(), AuthRepositoryError> {
    let threshold = now - Duration::minutes(3);
    let statement = Query::update()
        .table(Alias::new("auth_sessions"))
        .value(Alias::new("last_seen_at"), now)
        .and_where(Expr::col(Alias::new("id")).eq(session_id))
        .and_where(Expr::col(Alias::new("revoked_at")).is_null())
        .cond_where(
            Cond::any()
                .add(Expr::col(Alias::new("last_seen_at")).is_null())
                .add(Expr::col(Alias::new("last_seen_at")).lt(threshold)),
        )
        .to_owned();
    database
        .execute(database.get_database_backend().build(&statement))
        .await?;
    Ok(())
}

fn auth_session_from_row(row: &QueryResult) -> Result<AuthSessionRecord, AuthRepositoryError> {
    Ok(AuthSessionRecord {
        id: row.try_get("", "id")?,
        user_id: UserId::from_uuid(row.try_get("", "session_user_id")?),
        user_name: row.try_get("", "username")?,
        device_id: row.try_get("", "device_id")?,
        device_name: row.try_get("", "device_name")?,
        client_name: row.try_get("", "client_name")?,
        client_version: row.try_get("", "client_version")?,
        created_at: row.try_get("", "created_at")?,
        last_seen_at: row.try_get("", "last_seen_at")?,
        playable_media_types: string_array(row, "playable_media_types")?,
        supported_commands: string_array(row, "supported_commands")?,
        supports_media_control: row.try_get("", "supports_media_control")?,
        supports_persistent_identifier: row.try_get("", "supports_persistent_identifier")?,
    })
}

fn string_array(row: &QueryResult, column: &str) -> Result<Vec<String>, AuthRepositoryError> {
    let value: Option<serde_json::Value> = row.try_get("", column)?;
    let Some(serde_json::Value::Array(values)) = value else {
        return if value.is_none() {
            Ok(Vec::new())
        } else {
            Err(AuthRepositoryError::InvalidStoredSessionCapabilities)
        };
    };
    values
        .into_iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .ok_or(AuthRepositoryError::InvalidStoredSessionCapabilities)
        })
        .collect()
}

fn user_query() -> sea_orm::sea_query::SelectStatement {
    Query::select()
        .columns([
            Alias::new("id"),
            Alias::new("username"),
            Alias::new("is_admin"),
            Alias::new("has_password"),
            Alias::new("disabled_at"),
            Alias::new("auth_revision"),
        ])
        .from(Alias::new("users"))
        .to_owned()
}

async fn get_user_on<Connection>(
    connection: &Connection,
    user_id: UserId,
) -> Result<Option<AuthUser>, AuthRepositoryError>
where
    Connection: ConnectionTrait,
{
    let mut query = user_query();
    query
        .and_where(Expr::col(Alias::new("id")).eq(user_id.as_uuid()))
        .limit(1);
    let backend = connection.get_database_backend();
    connection
        .query_one(backend.build(&query))
        .await?
        .as_ref()
        .map(auth_user_from_row)
        .transpose()
}

async fn update_user_fields(
    transaction: &DatabaseTransaction,
    user_id: UserId,
    values: impl IntoIterator<Item = (Alias, sea_orm::sea_query::SimpleExpr)>,
) -> Result<AuthUser, AuthRepositoryError> {
    let current = get_user_on(transaction, user_id)
        .await?
        .ok_or(AuthRepositoryError::UserNotFound)?;
    lock_user_revision(transaction, user_id, current.auth_revision()).await?;
    delete_for_user_on(transaction, user_id).await?;
    let mut update = Query::update();
    update
        .table(Alias::new("users"))
        .values(values)
        .value(
            Alias::new("auth_revision"),
            Expr::col(Alias::new("auth_revision")).add(1_i64),
        )
        .and_where(Expr::col(Alias::new("id")).eq(user_id.as_uuid()))
        .and_where(Expr::col(Alias::new("auth_revision")).eq(current.auth_revision()));
    let backend = transaction.get_database_backend();
    let result = transaction.execute(backend.build(&update)).await;
    if result
        .as_ref()
        .err()
        .and_then(DbErr::sql_err)
        .is_some_and(|error| matches!(error, SqlErr::UniqueConstraintViolation(_)))
    {
        return Err(AuthRepositoryError::UsernameConflict);
    }
    if result?.rows_affected() != 1 {
        return Err(AuthRepositoryError::UserNotFound);
    }
    get_user_on(transaction, user_id)
        .await?
        .ok_or(AuthRepositoryError::UserNotFound)
}

async fn lock_user_revision(
    transaction: &DatabaseTransaction,
    user_id: UserId,
    auth_revision: i64,
) -> Result<(), AuthRepositoryError> {
    let update = Query::update()
        .table(Alias::new("users"))
        .value(
            Alias::new("auth_revision"),
            Expr::col(Alias::new("auth_revision")),
        )
        .and_where(Expr::col(Alias::new("id")).eq(user_id.as_uuid()))
        .and_where(Expr::col(Alias::new("auth_revision")).eq(auth_revision))
        .to_owned();
    let backend = transaction.get_database_backend();
    transaction.execute(backend.build(&update)).await?;
    let current = get_user_on(transaction, user_id).await?;
    if current.as_ref().map(AuthUser::auth_revision) != Some(auth_revision) {
        return Err(AuthRepositoryError::CredentialChanged);
    }
    Ok(())
}

async fn lock_auth_state(transaction: &DatabaseTransaction) -> Result<(), AuthRepositoryError> {
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
    Ok(())
}

async fn has_other_enabled_admin(
    transaction: &DatabaseTransaction,
    excluded: UserId,
) -> Result<bool, AuthRepositoryError> {
    let query = Query::select()
        .expr(Expr::col(Alias::new("id")))
        .from(Alias::new("users"))
        .and_where(Expr::col(Alias::new("id")).ne(excluded.as_uuid()))
        .and_where(Expr::col(Alias::new("is_admin")).eq(true))
        .and_where(Expr::col(Alias::new("disabled_at")).is_null())
        .limit(1)
        .to_owned();
    let backend = transaction.get_database_backend();
    Ok(transaction
        .query_one(backend.build(&query))
        .await?
        .is_some())
}

async fn ensure_admin_remains(
    transaction: &DatabaseTransaction,
    current: &AuthUser,
    remains_enabled_admin: bool,
) -> Result<(), AuthRepositoryError> {
    if current.is_admin()
        && !current.is_disabled()
        && !remains_enabled_admin
        && !has_other_enabled_admin(transaction, current.id()).await?
    {
        return Err(AuthRepositoryError::LastEnabledAdmin);
    }
    Ok(())
}

async fn update_policy_in_transaction(
    transaction: &DatabaseTransaction,
    user_id: UserId,
    is_admin: bool,
    is_disabled: bool,
    now: DateTime<Utc>,
) -> Result<AuthUser, AuthRepositoryError> {
    lock_auth_state(transaction).await?;
    let current = get_user_on(transaction, user_id)
        .await?
        .ok_or(AuthRepositoryError::UserNotFound)?;
    let updated = update_user_fields(
        transaction,
        user_id,
        [
            (Alias::new("is_admin"), is_admin.into()),
            (
                Alias::new("disabled_at"),
                if is_disabled { Some(now) } else { None }.into(),
            ),
            (Alias::new("updated_at"), now.into()),
        ],
    )
    .await?;
    ensure_admin_remains(transaction, &current, is_admin && !is_disabled).await?;
    Ok(updated)
}

async fn delete_user_in_transaction(
    transaction: &DatabaseTransaction,
    user_id: UserId,
) -> Result<(), AuthRepositoryError> {
    lock_auth_state(transaction).await?;
    let current = get_user_on(transaction, user_id)
        .await?
        .ok_or(AuthRepositoryError::UserNotFound)?;
    ensure_admin_remains(transaction, &current, false).await?;
    let import_reference = Query::select()
        .expr(Expr::col(Alias::new("id")))
        .from(Alias::new("import_sources"))
        .and_where(Expr::col(Alias::new("target_user_id")).eq(user_id.as_uuid()))
        .limit(1)
        .to_owned();
    let backend = transaction.get_database_backend();
    if transaction
        .query_one(backend.build(&import_reference))
        .await?
        .is_some()
    {
        return Err(AuthRepositoryError::UserReferenced);
    }
    lock_user_revision(transaction, user_id, current.auth_revision()).await?;
    delete_for_user_on(transaction, user_id).await?;
    for table in [
        "playback_sessions",
        "auth_sessions",
        "user_data",
        "user_catalog_state",
    ] {
        let delete = Query::delete()
            .from_table(Alias::new(table))
            .and_where(Expr::col(Alias::new("user_id")).eq(user_id.as_uuid()))
            .to_owned();
        transaction.execute(backend.build(&delete)).await?;
    }
    let delete = Query::delete()
        .from_table(Alias::new("users"))
        .and_where(Expr::col(Alias::new("id")).eq(user_id.as_uuid()))
        .and_where(Expr::col(Alias::new("auth_revision")).eq(current.auth_revision()))
        .to_owned();
    if transaction
        .execute(backend.build(&delete))
        .await?
        .rows_affected()
        != 1
    {
        return Err(AuthRepositoryError::CredentialChanged);
    }
    Ok(())
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
    let result = connection.execute(backend.build(&statement)).await;
    if result
        .as_ref()
        .err()
        .and_then(DbErr::sql_err)
        .is_some_and(|error| matches!(error, SqlErr::UniqueConstraintViolation(_)))
    {
        return Err(AuthRepositoryError::UsernameConflict);
    }
    result?;
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
    if let Some(expires_at) = session.expires_at
        && expires_at <= session.created_at
    {
        return Err(AuthRepositoryError::InvalidSessionExpiry);
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
            Alias::new("device_key"),
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
            crate::natural_key::hash(&["device", &session.device_id]).into(),
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
        user: auth_user_from_row(row)?,
        password_hash: row.try_get("", "password_hash")?,
    })
}

fn auth_user_from_row(row: &QueryResult) -> Result<AuthUser, AuthRepositoryError> {
    Ok(AuthUser::from_database_row(row)?)
}

fn principal_from_row(
    row: &QueryResult,
    session_id: Uuid,
) -> Result<AuthenticatedPrincipal, AuthRepositoryError> {
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
        origin: AuthenticationOrigin::Session {
            id: session_id,
            device_id: row.try_get("", "device_id")?,
        },
    })
}
