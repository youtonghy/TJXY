use std::{fmt, sync::Arc};

use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use chrono::{DateTime, Duration, Utc};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tjxy_common::Username;
use tjxy_db::{
    AuthRepository, AuthRepositoryError, AuthUser, AuthenticatedPrincipal, CredentialSnapshot,
    SessionDraft,
};
use tokio::sync::Semaphore;
use uuid::Uuid;

const MAX_PASSWORD_BYTES: usize = 1_024;

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
        })
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

    /// Resolves a raw session token to its current principal.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::InvalidToken`] for unknown, revoked, or expired
    /// sessions and [`AuthError::Forbidden`] for a disabled user.
    pub async fn authenticate_token(
        &self,
        access_token: &str,
    ) -> Result<AuthenticatedPrincipal, AuthError> {
        let digest = digest_token(access_token);
        let principal = AuthRepository::new(&self.database)
            .find_principal_by_token_digest(&digest, self.clock.now())
            .await?
            .ok_or(AuthError::InvalidToken)?;
        if principal.user().is_disabled() {
            return Err(AuthError::Forbidden);
        }
        Ok(principal)
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
    #[error("password must not be empty")]
    PasswordRequired,
    #[error("client identity is invalid")]
    InvalidClientIdentity,
    #[error("invalid username or password")]
    InvalidCredentials,
    #[error("session token is invalid or expired")]
    InvalidToken,
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
    SecretSessionToken(format!(
        "{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    ))
}

fn digest_token(token: &str) -> [u8; 32] {
    Sha256::digest(token.as_bytes()).into()
}
