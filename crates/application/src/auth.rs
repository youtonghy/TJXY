use std::{fmt, sync::Arc};

use argon2::{
    Argon2,
    password_hash::{
        PasswordHash, PasswordHasher, PasswordVerifier, SaltString,
        rand_core::{OsRng, RngCore},
    },
};
use chrono::{DateTime, Duration, Utc};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tjxy_common::{UserId, Username};
use tjxy_credentials::{CredentialCipher, CredentialCipherError};
use tjxy_db::{
    ApiKeyRepository, ApiKeyRepositoryError, AuthRepository, AuthRepositoryError, AuthSessionQuery,
    AuthSessionRecord, AuthUser, AuthenticatedPrincipal, CredentialSnapshot, DeviceOptionsRecord,
    DeviceRecord, DeviceRepository, DeviceRepositoryError, SessionCapabilitiesDraft, SessionDraft,
};
use tokio::sync::Semaphore;
use uuid::Uuid;
use zeroize::Zeroizing;

const MAX_PASSWORD_BYTES: usize = 1_024;
const MAX_CAPABILITY_VALUES: usize = 128;
const MAX_CAPABILITY_VALUE_CHARS: usize = 128;
const MAX_CAPABILITY_URL_CHARS: usize = 255;
const MAX_DEVICE_PROFILE_BYTES: usize = 64 * 1_024;
const MAX_ACTIVE_WITHIN_SECONDS: u32 = 30 * 24 * 60 * 60;
const MAX_BIO_CHARS: usize = 500;
const MAX_DEVICE_DELETE_BATCH: usize = 128;

pub trait AuthClock: Clone + Send + Sync + 'static {
    fn now(&self) -> DateTime<Utc>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl AuthClock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClientIdentity {
    client_name: String,
    device_name: String,
    device_id: String,
    client_version: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SessionCapabilities {
    pub playable_media_types: Vec<String>,
    pub supported_commands: Vec<String>,
    pub supports_media_control: bool,
    pub supports_persistent_identifier: bool,
    pub device_profile: Option<serde_json::Value>,
    pub app_store_url: Option<String>,
    pub icon_url: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SessionListFilter {
    device_id: Option<String>,
    active_within_seconds: Option<u32>,
    controllable_by_user_id: Option<UserId>,
}

impl SessionListFilter {
    #[must_use]
    pub fn with_device_id(mut self, device_id: impl Into<String>) -> Self {
        self.device_id = Some(device_id.into());
        self
    }

    #[must_use]
    pub const fn with_active_within_seconds(mut self, seconds: u32) -> Self {
        self.active_within_seconds = Some(seconds);
        self
    }

    #[must_use]
    pub const fn with_controllable_by_user_id(mut self, user_id: UserId) -> Self {
        self.controllable_by_user_id = Some(user_id);
        self
    }
}

impl ClientIdentity {
    /// Creates validated client metadata for a durable session.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::InvalidClientIdentity`] when any required value is
    /// empty, contains a control character, or exceeds its schema limit.
    pub fn new(
        client_name: impl Into<String>,
        device_name: impl Into<String>,
        device_id: impl Into<String>,
        client_version: impl Into<String>,
    ) -> Result<Self, AuthError> {
        let identity = Self {
            client_name: client_name.into(),
            device_name: device_name.into(),
            device_id: device_id.into(),
            client_version: client_version.into(),
        };
        for (value, maximum) in [
            (&identity.client_name, 256),
            (&identity.device_name, 256),
            (&identity.device_id, 512),
            (&identity.client_version, 128),
        ] {
            if value.is_empty()
                || value.chars().count() > maximum
                || value.chars().any(char::is_control)
            {
                return Err(AuthError::InvalidClientIdentity);
            }
        }
        Ok(identity)
    }

    #[must_use]
    pub fn client_name(&self) -> &str {
        &self.client_name
    }

    #[must_use]
    pub fn device_name(&self) -> &str {
        &self.device_name
    }

    #[must_use]
    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    #[must_use]
    pub fn client_version(&self) -> &str {
        &self.client_version
    }
}

pub struct SecretSessionToken(String);

impl SecretSessionToken {
    #[must_use]
    pub fn expose_secret(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SecretSessionToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretSessionToken([REDACTED])")
    }
}

pub struct IssuedAuthentication {
    user: AuthUser,
    session_id: Uuid,
    client: ClientIdentity,
    access_token: SecretSessionToken,
    expires_at: Option<DateTime<Utc>>,
}

impl fmt::Debug for IssuedAuthentication {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("IssuedAuthentication")
            .field("user", &self.user)
            .field("session_id", &self.session_id)
            .field("client", &self.client)
            .field("access_token", &self.access_token)
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

impl IssuedAuthentication {
    #[must_use]
    pub const fn user(&self) -> &AuthUser {
        &self.user
    }

    #[must_use]
    pub const fn session_id(&self) -> Uuid {
        self.session_id
    }

    #[must_use]
    pub const fn client(&self) -> &ClientIdentity {
        &self.client
    }

    #[must_use]
    pub const fn access_token(&self) -> &SecretSessionToken {
        &self.access_token
    }

    #[must_use]
    pub const fn expires_at(&self) -> Option<DateTime<Utc>> {
        self.expires_at
    }
}

pub struct AuthService<Clock = SystemClock> {
    database: sea_orm::DatabaseConnection,
    clock: Clock,
    session_lifetime: Option<Duration>,
    password_slots: Arc<Semaphore>,
    dummy_password_hash: String,
    credential_cipher: Option<Arc<CredentialCipher>>,
}

impl<Clock> AuthService<Clock>
where
    Clock: AuthClock,
{
    /// Builds the auth use case and prepares a dummy Argon2 hash used to avoid
    /// obvious username-enumeration timing differences.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError`] for invalid policy or password-engine failures.
    pub async fn new(
        database: sea_orm::DatabaseConnection,
        clock: Clock,
        session_lifetime: Option<Duration>,
        max_concurrent_password_hashes: usize,
    ) -> Result<Self, AuthError> {
        if session_lifetime.is_some_and(|lifetime| lifetime <= Duration::zero()) {
            return Err(AuthError::InvalidSessionLifetime);
        }
        if max_concurrent_password_hashes == 0 {
            return Err(AuthError::InvalidPasswordConcurrency);
        }
        let password_slots = Arc::new(Semaphore::new(max_concurrent_password_hashes));
        let dummy_password_hash =
            hash_password(password_slots.clone(), "tjxy-dummy-password").await?;
        Ok(Self {
            database,
            clock,
            session_lifetime,
            password_slots,
            dummy_password_hash,
            credential_cipher: None,
        })
    }

    #[must_use]
    pub fn with_credential_cipher(mut self, cipher: Arc<CredentialCipher>) -> Self {
        self.credential_cipher = Some(cipher);
        self
    }

    pub(crate) const fn database(&self) -> &sea_orm::DatabaseConnection {
        &self.database
    }

    pub(crate) fn now(&self) -> DateTime<Utc> {
        self.clock.now()
    }

    pub(crate) fn credential_cipher(&self) -> Result<&CredentialCipher, AuthError> {
        self.credential_cipher
            .as_deref()
            .ok_or(AuthError::CredentialCipherUnavailable)
    }

    /// Creates a local account with an Argon2id PHC credential.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError`] for invalid input, hashing failures, or persistence errors.
    pub async fn create_user(
        &self,
        username: &str,
        password: &str,
        is_admin: bool,
    ) -> Result<AuthUser, AuthError> {
        let username = Username::parse(username).map_err(|_| AuthError::InvalidUsername)?;
        validate_password(password)?;
        let password_hash = hash_password(self.password_slots.clone(), password).await?;
        AuthRepository::new(&self.database)
            .create_user(
                &username,
                &password_hash,
                !password.is_empty(),
                is_admin,
                self.clock.now(),
            )
            .await
            .map_err(Into::into)
    }

    /// Atomically creates the first enabled administrator when one is absent.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError`] for invalid input, hashing failures, or persistence errors.
    pub async fn create_initial_admin(
        &self,
        username: &str,
        password: &str,
    ) -> Result<Option<AuthUser>, AuthError> {
        let username = Username::parse(username).map_err(|_| AuthError::InvalidUsername)?;
        if password.is_empty() {
            return Err(AuthError::PasswordRequired);
        }
        validate_password(password)?;
        let password_hash = hash_password(self.password_slots.clone(), password).await?;
        AuthRepository::new(&self.database)
            .create_initial_admin(&username, &password_hash, self.clock.now())
            .await
            .map_err(Into::into)
    }

    /// Lists local users for the administrator API.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError`] when the authentication source of truth is unavailable.
    pub async fn list_users(&self) -> Result<Vec<AuthUser>, AuthError> {
        AuthRepository::new(&self.database)
            .list_users()
            .await
            .map_err(Into::into)
    }

    /// Lists a bounded, stable set of enabled users for background work.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError`] when the authentication source of truth is unavailable.
    pub async fn enabled_user_ids(&self, limit: u64) -> Result<Vec<UserId>, AuthError> {
        AuthRepository::new(&self.database)
            .enabled_user_ids(limit)
            .await
            .map_err(Into::into)
    }

    /// Reads one local user for the administrator API.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError`] when the authentication source of truth is unavailable.
    pub async fn get_user(&self, user_id: UserId) -> Result<Option<AuthUser>, AuthError> {
        AuthRepository::new(&self.database)
            .get_user(user_id)
            .await
            .map_err(Into::into)
    }

    /// Renames one local user and invalidates their existing sessions.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError`] for invalid input, missing users, or persistence failures.
    pub async fn rename_user(
        &self,
        user_id: UserId,
        username: &str,
    ) -> Result<AuthUser, AuthError> {
        let username = Username::parse(username).map_err(|_| AuthError::InvalidUsername)?;
        AuthRepository::new(&self.database)
            .rename_user(user_id, &username, self.clock.now())
            .await
            .map_err(Into::into)
    }

    /// Reads the current client profile.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError`] when the user is missing or profile persistence fails.
    pub async fn user_profile(&self, user_id: UserId) -> Result<(AuthUser, String), AuthError> {
        let repository = AuthRepository::new(&self.database);
        let user = repository
            .get_user(user_id)
            .await?
            .ok_or(AuthRepositoryError::UserNotFound)?;
        let bio = repository.user_bio(user_id).await?;
        Ok((user, bio))
    }

    /// Updates the authenticated user's username and biography after password confirmation.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError`] for invalid input, credentials, conflicts, or persistence failures.
    pub async fn update_self_profile(
        &self,
        user_id: UserId,
        username: &str,
        bio: &str,
        current_password: &str,
    ) -> Result<(AuthUser, String), AuthError> {
        if bio.chars().count() > MAX_BIO_CHARS || bio.chars().any(char::is_control) {
            return Err(AuthError::InvalidProfile);
        }
        let current = self.verify_user_password(user_id, current_password).await?;
        let parsed = Username::parse(username).map_err(|_| AuthError::InvalidUsername)?;
        let repository = AuthRepository::new(&self.database);
        let user = if current.name() == parsed.as_str() {
            repository
                .update_bio(user_id, bio, self.clock.now())
                .await?
        } else {
            repository
                .update_profile(user_id, &parsed, bio, self.clock.now())
                .await?
        };
        Ok((user, bio.to_owned()))
    }

    /// Updates the authenticated user's profile and optional password after one confirmation.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError`] for invalid input, credentials, hashing, or persistence failures.
    pub async fn update_self_account(
        &self,
        user_id: UserId,
        username: &str,
        bio: &str,
        current_password: &str,
        new_password: Option<&str>,
    ) -> Result<(AuthUser, String), AuthError> {
        if bio.chars().count() > MAX_BIO_CHARS || bio.chars().any(char::is_control) {
            return Err(AuthError::InvalidProfile);
        }
        self.verify_user_password(user_id, current_password).await?;
        let parsed = Username::parse(username).map_err(|_| AuthError::InvalidUsername)?;
        let password_hash = if let Some(password) = new_password.filter(|value| !value.is_empty()) {
            validate_password(password)?;
            Some(hash_password(self.password_slots.clone(), password).await?)
        } else {
            None
        };
        let user = AuthRepository::new(&self.database)
            .update_account(
                user_id,
                &parsed,
                bio,
                password_hash.as_deref(),
                self.clock.now(),
            )
            .await?;
        Ok((user, bio.to_owned()))
    }

    /// Changes the authenticated user's password after verifying the current credential.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError`] for invalid credentials, password input, hashing, or persistence.
    pub async fn update_self_password(
        &self,
        user_id: UserId,
        current_password: &str,
        new_password: &str,
    ) -> Result<AuthUser, AuthError> {
        self.verify_user_password(user_id, current_password).await?;
        self.update_user_password(user_id, new_password, false)
            .await
    }

    async fn verify_user_password(
        &self,
        user_id: UserId,
        password: &str,
    ) -> Result<AuthUser, AuthError> {
        validate_password(password).map_err(|_| AuthError::InvalidCredentials)?;
        let current = AuthRepository::new(&self.database)
            .get_user(user_id)
            .await?
            .ok_or(AuthError::InvalidCredentials)?;
        let username =
            Username::parse(current.name()).map_err(|_| AuthError::InvalidCredentials)?;
        let credential = AuthRepository::new(&self.database)
            .find_credential(&username)
            .await?
            .ok_or(AuthError::InvalidCredentials)?;
        let matches = verify_password(
            self.password_slots.clone(),
            password,
            credential.password_hash(),
        )
        .await?;
        if !matches || credential.user().id() != user_id {
            return Err(AuthError::InvalidCredentials);
        }
        Ok(current)
    }

    /// Replaces or resets one local password and invalidates existing sessions.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError`] for invalid input, unavailable password workers, or persistence errors.
    pub async fn update_user_password(
        &self,
        user_id: UserId,
        new_password: &str,
        reset_password: bool,
    ) -> Result<AuthUser, AuthError> {
        if !reset_password && new_password.is_empty() {
            return Err(AuthError::PasswordRequired);
        }
        let password = if reset_password { "" } else { new_password };
        validate_password(password)?;
        let password_hash = hash_password(self.password_slots.clone(), password).await?;
        AuthRepository::new(&self.database)
            .update_password(user_id, &password_hash, !reset_password, self.clock.now())
            .await
            .map_err(Into::into)
    }

    /// Updates the supported local policy and invalidates existing sessions.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError`] when the user is missing, the final administrator would be removed,
    /// or persistence fails.
    pub async fn update_user_policy(
        &self,
        user_id: UserId,
        is_admin: bool,
        is_disabled: bool,
    ) -> Result<AuthUser, AuthError> {
        AuthRepository::new(&self.database)
            .update_policy(user_id, is_admin, is_disabled, self.clock.now())
            .await
            .map_err(Into::into)
    }

    /// Deletes one local user and user-owned runtime state.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError`] when the user is missing, protected, referenced, or cannot be deleted.
    pub async fn delete_user(&self, user_id: UserId) -> Result<(), AuthError> {
        AuthRepository::new(&self.database)
            .delete_user(user_id)
            .await
            .map_err(Into::into)
    }

    /// Verifies a username/password and atomically commits a new durable session.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::InvalidCredentials`] for both unknown usernames and
    /// wrong passwords, [`AuthError::Forbidden`] for disabled users, or an
    /// operational error without exposing credentials.
    pub async fn authenticate(
        &self,
        username: &str,
        password: &str,
        client: ClientIdentity,
    ) -> Result<IssuedAuthentication, AuthError> {
        let credential = self.verified_credential(username, password).await?;

        let now = self.clock.now();
        let expires_at = match self.session_lifetime {
            Some(lifetime) => Some(
                now.checked_add_signed(lifetime)
                    .ok_or(AuthError::TimestampOverflow)?,
            ),
            None => None,
        };
        let access_token = generate_session_token();
        let token_digest = digest_token(access_token.expose_secret());
        let draft = SessionDraft {
            id: Uuid::new_v4(),
            token_digest,
            device_id: client.device_id.clone(),
            device_name: client.device_name.clone(),
            client_name: client.client_name.clone(),
            client_version: client.client_version.clone(),
            created_at: now,
            expires_at,
        };
        let issued = AuthRepository::new(&self.database)
            .issue_session(&credential, draft)
            .await
            .map_err(|error| match error {
                AuthRepositoryError::CredentialChanged => AuthError::Forbidden,
                other => AuthError::Repository(other),
            })?;
        Ok(IssuedAuthentication {
            user: credential.user().clone(),
            session_id: issued.id(),
            client,
            access_token,
            expires_at: issued.expires_at(),
        })
    }

    /// Issues a new session for a user whose existing authenticated session
    /// has explicitly approved an out-of-band login.
    pub async fn issue_approved_session(
        &self,
        user: &AuthUser,
        client: ClientIdentity,
    ) -> Result<IssuedAuthentication, AuthError> {
        let now = self.clock.now();
        let expires_at = match self.session_lifetime {
            Some(lifetime) => Some(
                now.checked_add_signed(lifetime)
                    .ok_or(AuthError::TimestampOverflow)?,
            ),
            None => None,
        };
        let access_token = generate_session_token();
        let draft = SessionDraft {
            id: Uuid::new_v4(),
            token_digest: digest_token(access_token.expose_secret()),
            device_id: client.device_id.clone(),
            device_name: client.device_name.clone(),
            client_name: client.client_name.clone(),
            client_version: client.client_version.clone(),
            created_at: now,
            expires_at,
        };
        let session_id = draft.id;
        AuthRepository::new(&self.database)
            .issue_session_for_user(user.id(), user.auth_revision(), draft)
            .await?;
        Ok(IssuedAuthentication {
            user: user.clone(),
            session_id,
            client,
            access_token,
            expires_at,
        })
    }

    /// Verifies local credentials without creating a durable session.
    ///
    /// # Errors
    ///
    /// Returns the same credential errors as [`Self::authenticate`] while leaving
    /// session state unchanged.
    pub async fn verify_credentials(
        &self,
        username: &str,
        password: &str,
    ) -> Result<AuthUser, AuthError> {
        self.verified_credential(username, password)
            .await
            .map(|credential| credential.user().clone())
    }

    async fn verified_credential(
        &self,
        username: &str,
        password: &str,
    ) -> Result<CredentialSnapshot, AuthError> {
        validate_password(password).map_err(|_| AuthError::InvalidCredentials)?;
        let parsed_username =
            Username::parse(username).map_err(|_| AuthError::InvalidCredentials)?;
        let credential = AuthRepository::new(&self.database)
            .find_credential(&parsed_username)
            .await?;
        let candidate_hash = credential.as_ref().map_or(
            self.dummy_password_hash.as_str(),
            CredentialSnapshot::password_hash,
        );
        let password_matches =
            verify_password(self.password_slots.clone(), password, candidate_hash).await?;
        let Some(credential) = credential else {
            return Err(AuthError::InvalidCredentials);
        };
        if !password_matches {
            return Err(AuthError::InvalidCredentials);
        }
        if credential.user().is_disabled() {
            return Err(AuthError::Forbidden);
        }
        Ok(credential)
    }

    /// Resolves a raw session or API-key token to its current principal.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::InvalidToken`] for unknown, revoked, or expired
    /// credentials and [`AuthError::Forbidden`] for a disabled user.
    pub async fn authenticate_token(
        &self,
        access_token: &str,
    ) -> Result<AuthenticatedPrincipal, AuthError> {
        let digest = digest_token(access_token);
        let now = self.clock.now();
        if let Some(principal) = AuthRepository::new(&self.database)
            .find_principal_by_token_digest(&digest, now)
            .await?
        {
            if principal.user().is_disabled() {
                return Err(AuthError::Forbidden);
            }
            return Ok(principal);
        }
        let principal = ApiKeyRepository::new(&self.database)
            .find_principal_by_token_digest(&digest, now)
            .await?
            .ok_or(AuthError::InvalidToken)?;
        if principal.user().is_disabled() {
            return Err(AuthError::Forbidden);
        }
        Ok(principal)
    }

    /// Persists capabilities for the authenticated session only.
    ///
    /// An explicit session id is treated as an assertion and cannot select a
    /// different session, including another session owned by the same user.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError`] when persistence fails. `Ok(false)` means the
    /// requested session was not the authenticated session.
    pub async fn update_session_capabilities(
        &self,
        principal: &AuthenticatedPrincipal,
        requested_session: Option<Uuid>,
        capabilities: SessionCapabilities,
    ) -> Result<bool, AuthError> {
        let session_id = require_session_id(principal)?;
        validate_capabilities(&capabilities)?;
        if requested_session.is_some_and(|session| session != session_id) {
            return Ok(false);
        }
        AuthRepository::new(&self.database)
            .update_session_capabilities(
                principal.user().id(),
                session_id,
                SessionCapabilitiesDraft {
                    playable_media_types: capabilities.playable_media_types,
                    supported_commands: capabilities.supported_commands,
                    supports_media_control: capabilities.supports_media_control,
                    supports_persistent_identifier: capabilities.supports_persistent_identifier,
                    device_profile: capabilities.device_profile,
                    app_store_url: capabilities.app_store_url,
                    icon_url: capabilities.icon_url,
                },
            )
            .await
            .map_err(Into::into)
    }

    /// Returns the `DeviceProfile` reported by the authenticated session.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError`] when persistence is unavailable.
    pub async fn session_device_profile(
        &self,
        principal: &AuthenticatedPrincipal,
    ) -> Result<Option<serde_json::Value>, AuthError> {
        let Some(session_id) = principal.session_id() else {
            return Ok(None);
        };
        AuthRepository::new(&self.database)
            .session_device_profile(principal.user().id(), session_id)
            .await
            .map_err(Into::into)
    }

    /// Lists active sessions visible to the authenticated user.
    ///
    /// Administrators can observe all active sessions. Other users can only
    /// observe their own sessions.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError`] for invalid filters, unauthorized user selection,
    /// or persistence failures.
    pub async fn sessions(
        &self,
        principal: &AuthenticatedPrincipal,
        filter: SessionListFilter,
    ) -> Result<Vec<AuthSessionRecord>, AuthError> {
        validate_session_filter(&filter)?;
        if filter.controllable_by_user_id.is_some_and(|requested| {
            requested != principal.user().id() && !principal.user().is_admin()
        }) {
            return Err(AuthError::Forbidden);
        }
        let now = self.clock.now();
        let active_after = filter
            .active_within_seconds
            .map(|seconds| now - Duration::seconds(i64::from(seconds)));
        AuthRepository::new(&self.database)
            .list_active_sessions(
                AuthSessionQuery {
                    visible_user_id: (!principal.user().is_admin())
                        .then_some(principal.user().id()),
                    controllable_by_user_id: filter.controllable_by_user_id,
                    device_id: filter.device_id,
                    active_after,
                },
                now,
            )
            .await
            .map_err(Into::into)
    }

    /// Revokes the authenticated session.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError`] when persistence is unavailable.
    pub async fn logout(&self, principal: &AuthenticatedPrincipal) -> Result<(), AuthError> {
        let session_id = require_session_id(principal)?;
        AuthRepository::new(&self.database)
            .revoke_session(
                principal.user().id(),
                session_id,
                self.clock.now(),
                "logout",
            )
            .await?;
        Ok(())
    }

    /// Revokes one active session owned by the authenticated user.
    ///
    /// API-key principals are deliberately rejected because they do not
    /// represent a durable login session.
    pub async fn revoke_user_session(
        &self,
        principal: &AuthenticatedPrincipal,
        session_id: Uuid,
    ) -> Result<bool, AuthError> {
        let current_user_id = principal.user().id();
        require_session_id(principal)?;
        Ok(AuthRepository::new(&self.database)
            .revoke_session(
                current_user_id,
                session_id,
                self.clock.now(),
                "user_revoked",
            )
            .await?)
    }

    /// Lists the active devices visible to an administrator.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError`] for non-administrators or persistence failures.
    pub async fn devices(
        &self,
        principal: &AuthenticatedPrincipal,
        user_id: Option<UserId>,
    ) -> Result<Vec<DeviceRecord>, AuthError> {
        require_administrator(principal)?;
        if let Some(user_id) = user_id
            && AuthRepository::new(&self.database)
                .get_user(user_id)
                .await?
                .is_none()
        {
            return Err(AuthRepositoryError::UserNotFound.into());
        }
        DeviceRepository::new(&self.database)
            .list_active(None, self.clock.now())
            .await
            .map_err(Into::into)
    }

    /// Returns one active device to an administrator.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError`] for invalid input, non-administrators, or persistence failures.
    pub async fn device(
        &self,
        principal: &AuthenticatedPrincipal,
        device_id: &str,
    ) -> Result<Option<DeviceRecord>, AuthError> {
        require_administrator(principal)?;
        validate_device_id(device_id)?;
        DeviceRepository::new(&self.database)
            .device(device_id, self.clock.now())
            .await
            .map_err(Into::into)
    }

    /// Returns custom options for one device to an administrator.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError`] for invalid input, non-administrators, or persistence failures.
    pub async fn device_options(
        &self,
        principal: &AuthenticatedPrincipal,
        device_id: &str,
    ) -> Result<Option<DeviceOptionsRecord>, AuthError> {
        require_administrator(principal)?;
        validate_device_id(device_id)?;
        DeviceRepository::new(&self.database)
            .options(device_id)
            .await
            .map_err(Into::into)
    }

    /// Creates or replaces custom options for one active device.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError`] for invalid input, non-administrators, or persistence failures.
    pub async fn update_device_options(
        &self,
        principal: &AuthenticatedPrincipal,
        device_id: &str,
        custom_name: Option<&str>,
    ) -> Result<bool, AuthError> {
        require_administrator(principal)?;
        validate_device_id(device_id)?;
        validate_device_custom_name(custom_name)?;
        DeviceRepository::new(&self.database)
            .update_options(device_id, custom_name, self.clock.now())
            .await
            .map_err(Into::into)
    }

    /// Atomically revokes every active session associated with selected devices.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError`] for invalid input, non-administrators, or persistence failures.
    pub async fn delete_devices(
        &self,
        principal: &AuthenticatedPrincipal,
        device_ids: &[String],
    ) -> Result<bool, AuthError> {
        require_administrator(principal)?;
        if device_ids.is_empty() || device_ids.len() > MAX_DEVICE_DELETE_BATCH {
            return Err(AuthError::InvalidDeviceRequest);
        }
        for device_id in device_ids {
            validate_device_id(device_id)?;
        }
        let mut device_ids = device_ids.iter().map(String::as_str).collect::<Vec<_>>();
        device_ids.sort_unstable();
        device_ids.dedup();
        DeviceRepository::new(&self.database)
            .delete_active(&device_ids, self.clock.now())
            .await
            .map_err(Into::into)
    }

    /// Reports whether startup has at least one configured local user.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError`] when persistence fails.
    pub async fn has_users(&self) -> Result<bool, AuthError> {
        AuthRepository::new(&self.database)
            .has_users()
            .await
            .map_err(Into::into)
    }

    /// Reports whether an enabled local administrator can manage the service.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError`] when persistence fails.
    pub async fn has_enabled_admin(&self) -> Result<bool, AuthError> {
        AuthRepository::new(&self.database)
            .has_enabled_admin()
            .await
            .map_err(Into::into)
    }

    /// Checks whether the authentication source of truth is reachable.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError`] when the database cannot answer a health probe.
    pub async fn check_health(&self) -> Result<(), AuthError> {
        self.database
            .ping()
            .await
            .map_err(AuthRepositoryError::from)?;
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("username is invalid")]
    InvalidUsername,
    #[error("password exceeds the supported length")]
    InvalidPassword,
    #[error("profile data is invalid")]
    InvalidProfile,
    #[error("password must not be empty")]
    PasswordRequired,
    #[error("client identity is invalid")]
    InvalidClientIdentity,
    #[error("session capabilities are invalid")]
    InvalidCapabilities,
    #[error("session list filter is invalid")]
    InvalidSessionFilter,
    #[error("device request is invalid")]
    InvalidDeviceRequest,
    #[error("API key request is invalid")]
    InvalidApiKeyRequest,
    #[error("invalid username or password")]
    InvalidCredentials,
    #[error("session token is invalid or expired")]
    InvalidToken,
    #[error("session authentication is required")]
    SessionRequired,
    #[error("API key capacity has been reached")]
    ApiKeyCapacity,
    #[error("credential cipher is unavailable")]
    CredentialCipherUnavailable,
    #[error("account is not permitted to authenticate")]
    Forbidden,
    #[error("session lifetime must be positive")]
    InvalidSessionLifetime,
    #[error("password hashing concurrency must be at least one")]
    InvalidPasswordConcurrency,
    #[error("authentication timestamp is outside the supported range")]
    TimestampOverflow,
    #[error("password engine failed")]
    PasswordEngine,
    #[error("password worker failed")]
    PasswordWorker,
    #[error("authentication password workers are busy")]
    Busy,
    #[error("authentication repository failed: {0}")]
    Repository(#[from] AuthRepositoryError),
    #[error("device repository failed: {0}")]
    DeviceRepository(#[from] DeviceRepositoryError),
    #[error("API key repository failed: {0}")]
    ApiKeyRepository(#[from] ApiKeyRepositoryError),
    #[error("credential cipher failed: {0}")]
    CredentialCipher(#[from] CredentialCipherError),
}

pub(crate) fn require_administrator(principal: &AuthenticatedPrincipal) -> Result<(), AuthError> {
    if principal.user().is_admin() {
        Ok(())
    } else {
        Err(AuthError::Forbidden)
    }
}

fn require_session_id(principal: &AuthenticatedPrincipal) -> Result<Uuid, AuthError> {
    principal.session_id().ok_or(AuthError::SessionRequired)
}

fn validate_device_id(device_id: &str) -> Result<(), AuthError> {
    if device_id.is_empty()
        || device_id.chars().count() > 512
        || device_id.chars().any(char::is_control)
    {
        return Err(AuthError::InvalidDeviceRequest);
    }
    Ok(())
}

fn validate_device_custom_name(custom_name: Option<&str>) -> Result<(), AuthError> {
    if custom_name.is_some_and(|custom_name| {
        custom_name.chars().count() > 256 || custom_name.chars().any(char::is_control)
    }) {
        return Err(AuthError::InvalidDeviceRequest);
    }
    Ok(())
}

fn validate_session_filter(filter: &SessionListFilter) -> Result<(), AuthError> {
    if filter.active_within_seconds > Some(MAX_ACTIVE_WITHIN_SECONDS)
        || filter.device_id.as_ref().is_some_and(|device_id| {
            device_id.is_empty()
                || device_id.chars().count() > 512
                || device_id.chars().any(char::is_control)
        })
    {
        return Err(AuthError::InvalidSessionFilter);
    }
    Ok(())
}

impl PartialEq for AuthError {
    fn eq(&self, other: &Self) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }
}

fn validate_password(password: &str) -> Result<(), AuthError> {
    if password.len() > MAX_PASSWORD_BYTES {
        return Err(AuthError::InvalidPassword);
    }
    Ok(())
}

fn validate_capabilities(capabilities: &SessionCapabilities) -> Result<(), AuthError> {
    for values in [
        &capabilities.playable_media_types,
        &capabilities.supported_commands,
    ] {
        if values.len() > MAX_CAPABILITY_VALUES
            || values.iter().any(|value| {
                value.is_empty()
                    || value.chars().count() > MAX_CAPABILITY_VALUE_CHARS
                    || value.chars().any(char::is_control)
            })
        {
            return Err(AuthError::InvalidCapabilities);
        }
    }
    for value in [
        capabilities.app_store_url.as_deref(),
        capabilities.icon_url.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        if value.chars().count() > MAX_CAPABILITY_URL_CHARS || value.chars().any(char::is_control) {
            return Err(AuthError::InvalidCapabilities);
        }
    }
    if capabilities
        .device_profile
        .as_ref()
        .is_some_and(|profile| profile.to_string().len() > MAX_DEVICE_PROFILE_BYTES)
    {
        return Err(AuthError::InvalidCapabilities);
    }
    Ok(())
}

async fn hash_password(slots: Arc<Semaphore>, password: &str) -> Result<String, AuthError> {
    let permit = slots.try_acquire_owned().map_err(|_| AuthError::Busy)?;
    let password = password.as_bytes().to_vec();
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let salt = SaltString::generate(&mut OsRng);
        Argon2::default()
            .hash_password(&password, &salt)
            .map(|hash| hash.to_string())
            .map_err(|_| AuthError::PasswordEngine)
    })
    .await
    .map_err(|_| AuthError::PasswordWorker)?
}

async fn verify_password(
    slots: Arc<Semaphore>,
    password: &str,
    password_hash: &str,
) -> Result<bool, AuthError> {
    let permit = slots.try_acquire_owned().map_err(|_| AuthError::Busy)?;
    let password = password.as_bytes().to_vec();
    let password_hash = password_hash.to_owned();
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let parsed = PasswordHash::new(&password_hash).map_err(|_| AuthError::PasswordEngine)?;
        Ok(Argon2::default()
            .verify_password(&password, &parsed)
            .is_ok())
    })
    .await
    .map_err(|_| AuthError::PasswordWorker)?
}

fn generate_session_token() -> SecretSessionToken {
    SecretSessionToken(generate_token())
}

pub(crate) fn generate_token() -> String {
    const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";

    let mut random = Zeroizing::new([0_u8; 32]);
    OsRng.fill_bytes(&mut *random);
    let mut token = String::with_capacity(64);
    for byte in random.iter().copied() {
        token.push(char::from(HEX_DIGITS[usize::from(byte >> 4)]));
        token.push(char::from(HEX_DIGITS[usize::from(byte & 0x0f)]));
    }
    token
}

pub(crate) fn digest_token(token: &str) -> [u8; 32] {
    Sha256::digest(token.as_bytes()).into()
}
