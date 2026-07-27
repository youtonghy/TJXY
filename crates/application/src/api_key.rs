use std::fmt;

use chrono::{DateTime, Utc};
use tjxy_common::UserId;
use tjxy_credentials::{CredentialCipher, CredentialCipherError};
use tjxy_db::{
    ApiKeyDraft, ApiKeyRepository, ApiKeyRepositoryError, AuthenticatedPrincipal, StoredApiKey,
};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::auth::{
    AuthClock, AuthError, AuthService, digest_token, generate_token, require_administrator,
};

const API_KEY_PROVIDER: &str = "tjxy-api-key/access-token/v1";
const API_KEY_TOKEN_CHARS: usize = 64;
const MAX_APP_NAME_CHARS: usize = 256;

pub struct SecretApiKey(Zeroizing<String>);

impl SecretApiKey {
    fn new(secret: String) -> Self {
        Self(Zeroizing::new(secret))
    }

    #[must_use]
    pub fn expose_secret(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Debug for SecretApiKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretApiKey([REDACTED])")
    }
}

#[derive(Debug)]
pub struct ApiKeyInfo {
    id: i64,
    access_token: SecretApiKey,
    app_name: String,
    creator_user_id: UserId,
    creator_user_name: String,
    created_at: DateTime<Utc>,
    last_used_at: Option<DateTime<Utc>>,
}

impl ApiKeyInfo {
    #[must_use]
    pub const fn id(&self) -> i64 {
        self.id
    }

    #[must_use]
    pub const fn access_token(&self) -> &SecretApiKey {
        &self.access_token
    }

    #[must_use]
    pub fn app_name(&self) -> &str {
        &self.app_name
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
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    #[must_use]
    pub const fn last_used_at(&self) -> Option<DateTime<Utc>> {
        self.last_used_at
    }
}

impl<Clock> AuthService<Clock>
where
    Clock: AuthClock,
{
    /// Lists and decrypts every bounded API key visible to an administrator.
    ///
    /// # Errors
    ///
    /// Returns an authorization, keyring, authenticated-decryption, or repository error.
    pub async fn list_api_keys(
        &self,
        actor: &AuthenticatedPrincipal,
    ) -> Result<Vec<ApiKeyInfo>, AuthError> {
        require_administrator(actor)?;
        let cipher = self.credential_cipher()?;
        ApiKeyRepository::new(self.database())
            .list(actor.user())
            .await
            .map_err(map_repository_error)?
            .iter()
            .map(|stored| api_key_info(cipher, stored))
            .collect()
    }

    /// Creates one encrypted API key for the current administrator snapshot.
    ///
    /// # Errors
    ///
    /// Returns an input, authorization, keyring, capacity, encryption, or repository error.
    pub async fn create_api_key(
        &self,
        actor: &AuthenticatedPrincipal,
        app_name: &str,
    ) -> Result<(), AuthError> {
        require_administrator(actor)?;
        if !valid_api_key_app_name(app_name) {
            return Err(AuthError::InvalidApiKeyRequest);
        }
        let cipher = self.credential_cipher()?;
        let access_token = SecretApiKey::new(generate_token());
        let token_digest = digest_token(access_token.expose_secret());
        let envelope_id = Uuid::new_v4();
        let envelope = cipher.seal(
            envelope_id,
            API_KEY_PROVIDER,
            access_token.expose_secret().as_bytes(),
        )?;
        ApiKeyRepository::new(self.database())
            .create(
                actor.user(),
                ApiKeyDraft {
                    envelope_id,
                    creator_user_id: actor.user().id(),
                    creator_auth_revision: actor.user().auth_revision(),
                    token_digest,
                    envelope,
                    app_name: app_name.to_owned(),
                    created_at: self.now(),
                },
            )
            .await
            .map_err(map_repository_error)
    }

    /// Deletes an API key by its raw token digest. Unknown tokens are successful no-ops.
    ///
    /// # Errors
    ///
    /// Returns an input, authorization, or repository error.
    pub async fn delete_api_key(
        &self,
        actor: &AuthenticatedPrincipal,
        raw_token: &str,
    ) -> Result<(), AuthError> {
        require_administrator(actor)?;
        validate_raw_token(raw_token)?;
        ApiKeyRepository::new(self.database())
            .delete_by_digest(actor.user(), &digest_token(raw_token))
            .await
            .map_err(map_repository_error)
    }

    /// Authenticates every stored API-key envelope before startup readiness.
    ///
    /// An empty key set does not require a configured credential cipher.
    ///
    /// # Errors
    ///
    /// Returns a keyring, authenticated-decryption, stored-record, or repository error.
    pub async fn validate_api_key_envelopes(&self) -> Result<(), AuthError> {
        let stored = ApiKeyRepository::new(self.database())
            .list_for_startup()
            .await
            .map_err(map_repository_error)?;
        if stored.is_empty() {
            return Ok(());
        }
        let cipher = self.credential_cipher()?;
        for key in &stored {
            validate_stored_token(cipher, key)?;
        }
        Ok(())
    }
}

fn api_key_info(cipher: &CredentialCipher, stored: &StoredApiKey) -> Result<ApiKeyInfo, AuthError> {
    let plaintext = validate_stored_token(cipher, stored)?;
    let access_token = std::str::from_utf8(plaintext.as_slice())
        .map_err(|_| CredentialCipherError::AuthenticationFailed)?
        .to_owned();
    Ok(ApiKeyInfo {
        id: stored.id(),
        access_token: SecretApiKey::new(access_token),
        app_name: stored.app_name().to_owned(),
        creator_user_id: stored.creator_user_id(),
        creator_user_name: stored.creator_user_name().to_owned(),
        created_at: stored.created_at(),
        last_used_at: stored.last_used_at(),
    })
}

fn validate_stored_token(
    cipher: &CredentialCipher,
    stored: &StoredApiKey,
) -> Result<Zeroizing<Vec<u8>>, AuthError> {
    let plaintext = cipher.open(stored.envelope_id(), API_KEY_PROVIDER, stored.envelope())?;
    let token = std::str::from_utf8(plaintext.as_slice())
        .map_err(|_| CredentialCipherError::AuthenticationFailed)?;
    if !valid_raw_token(token) || digest_token(token) != *stored.token_digest() {
        return Err(CredentialCipherError::AuthenticationFailed.into());
    }
    Ok(plaintext)
}

/// Returns whether an API-key application name satisfies the shared request boundary.
#[must_use]
pub fn valid_api_key_app_name(value: &str) -> bool {
    !value.trim().is_empty()
        && value.chars().count() <= MAX_APP_NAME_CHARS
        && !value.chars().any(char::is_control)
}

fn validate_raw_token(value: &str) -> Result<(), AuthError> {
    if !valid_raw_token(value) {
        return Err(AuthError::InvalidApiKeyRequest);
    }
    Ok(())
}

fn valid_raw_token(value: &str) -> bool {
    value.len() == API_KEY_TOKEN_CHARS && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn map_repository_error(error: ApiKeyRepositoryError) -> AuthError {
    match error {
        ApiKeyRepositoryError::ActorRejected => AuthError::Forbidden,
        ApiKeyRepositoryError::InvalidAppName => AuthError::InvalidApiKeyRequest,
        ApiKeyRepositoryError::CapacityReached => AuthError::ApiKeyCapacity,
        other => AuthError::ApiKeyRepository(other),
    }
}
