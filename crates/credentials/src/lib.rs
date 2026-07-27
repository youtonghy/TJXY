//! Versioned authenticated encryption for storage-provider credentials.

use std::{collections::HashMap, fmt};

use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, AeadCore, KeyInit, OsRng, Payload},
};
use thiserror::Error;
use uuid::Uuid;
use zeroize::{Zeroize, Zeroizing};

const NONCE_BYTES: usize = 12;
const TAG_BYTES: usize = 16;
const MAX_PLAINTEXT_BYTES: usize = 128 * 1024;
const MAX_ENVELOPE_BYTES: usize = MAX_PLAINTEXT_BYTES + NONCE_BYTES + TAG_BYTES;

#[derive(Clone)]
pub struct CredentialKey {
    version: i32,
    bytes: Zeroizing<[u8; 32]>,
}

impl CredentialKey {
    /// Defines one versioned AES-256 key.
    ///
    /// # Errors
    ///
    /// Returns [`CredentialCipherError::InvalidKeyVersion`] for non-positive versions.
    pub fn new(version: i32, mut bytes: [u8; 32]) -> Result<Self, CredentialCipherError> {
        if version <= 0 {
            bytes.zeroize();
            return Err(CredentialCipherError::InvalidKeyVersion);
        }
        Ok(Self {
            version,
            bytes: Zeroizing::new(bytes),
        })
    }
}

impl fmt::Debug for CredentialKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CredentialKey")
            .field("version", &self.version)
            .field("bytes", &"[REDACTED]")
            .finish()
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct CredentialEnvelope {
    key_version: i32,
    payload: Vec<u8>,
}

impl CredentialEnvelope {
    /// Reconstructs a validated envelope read from durable storage.
    ///
    /// # Errors
    ///
    /// Returns [`CredentialCipherError::InvalidEnvelope`] for invalid bounds or versions.
    pub fn from_parts(key_version: i32, payload: Vec<u8>) -> Result<Self, CredentialCipherError> {
        if key_version <= 0
            || payload.len() < NONCE_BYTES + TAG_BYTES
            || payload.len() > MAX_ENVELOPE_BYTES
        {
            return Err(CredentialCipherError::InvalidEnvelope);
        }
        Ok(Self {
            key_version,
            payload,
        })
    }

    #[must_use]
    pub const fn key_version(&self) -> i32 {
        self.key_version
    }

    #[must_use]
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }
}

impl fmt::Debug for CredentialEnvelope {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CredentialEnvelope")
            .field("key_version", &self.key_version)
            .field("payload", &"[REDACTED]")
            .field("payload_len", &self.payload.len())
            .finish()
    }
}

pub struct CredentialCipher {
    active_version: i32,
    keys: HashMap<i32, CredentialKey>,
}

impl CredentialCipher {
    /// Builds a keyring with one active encryption key and optional historical keys.
    ///
    /// # Errors
    ///
    /// Returns [`CredentialCipherError::DuplicateKeyVersion`] for ambiguous versions.
    pub fn new(
        active: CredentialKey,
        historical: Vec<CredentialKey>,
    ) -> Result<Self, CredentialCipherError> {
        let active_version = active.version;
        let mut keys = HashMap::with_capacity(historical.len() + 1);
        keys.insert(active.version, active);
        for key in historical {
            if keys.insert(key.version, key).is_some() {
                return Err(CredentialCipherError::DuplicateKeyVersion);
            }
        }
        Ok(Self {
            active_version,
            keys,
        })
    }

    /// Encrypts one credential with a fresh nonce and identity-bound associated data.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid provider/plaintext bounds or encryption failure.
    pub fn seal(
        &self,
        credential_id: Uuid,
        provider: &str,
        plaintext: &[u8],
    ) -> Result<CredentialEnvelope, CredentialCipherError> {
        validate_input(provider, plaintext)?;
        let key = self
            .keys
            .get(&self.active_version)
            .ok_or(CredentialCipherError::UnknownKeyVersion)?;
        let cipher = Aes256Gcm::new_from_slice(key.bytes.as_slice())
            .map_err(|_| CredentialCipherError::EncryptionFailed)?;
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let aad = associated_data(credential_id, provider);
        let ciphertext = cipher
            .encrypt(
                &nonce,
                Payload {
                    msg: plaintext,
                    aad: &aad,
                },
            )
            .map_err(|_| CredentialCipherError::EncryptionFailed)?;
        let mut payload = Vec::with_capacity(NONCE_BYTES + ciphertext.len());
        payload.extend_from_slice(&nonce);
        payload.extend_from_slice(&ciphertext);
        CredentialEnvelope::from_parts(self.active_version, payload)
    }

    /// Authenticates and decrypts one identity-bound credential envelope.
    ///
    /// # Errors
    ///
    /// Returns a single authentication error for altered ciphertext or associated data.
    pub fn open(
        &self,
        credential_id: Uuid,
        provider: &str,
        envelope: &CredentialEnvelope,
    ) -> Result<Zeroizing<Vec<u8>>, CredentialCipherError> {
        validate_provider(provider)?;
        let key = self
            .keys
            .get(&envelope.key_version)
            .ok_or(CredentialCipherError::UnknownKeyVersion)?;
        let cipher = Aes256Gcm::new_from_slice(key.bytes.as_slice())
            .map_err(|_| CredentialCipherError::AuthenticationFailed)?;
        let (nonce, ciphertext) = envelope.payload.split_at(NONCE_BYTES);
        let aad = associated_data(credential_id, provider);
        cipher
            .decrypt(
                Nonce::from_slice(nonce),
                Payload {
                    msg: ciphertext,
                    aad: &aad,
                },
            )
            .map(Zeroizing::new)
            .map_err(|_| CredentialCipherError::AuthenticationFailed)
    }
}

impl fmt::Debug for CredentialCipher {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CredentialCipher")
            .field("active_version", &self.active_version)
            .field("key_count", &self.keys.len())
            .finish()
    }
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum CredentialCipherError {
    #[error("credential key version must be positive")]
    InvalidKeyVersion,
    #[error("credential key versions must be unique")]
    DuplicateKeyVersion,
    #[error("credential envelope is malformed or outside supported bounds")]
    InvalidEnvelope,
    #[error("credential provider or plaintext is invalid")]
    InvalidInput,
    #[error("credential key version is not available")]
    UnknownKeyVersion,
    #[error("credential encryption failed")]
    EncryptionFailed,
    #[error("credential authentication failed")]
    AuthenticationFailed,
}

fn validate_input(provider: &str, plaintext: &[u8]) -> Result<(), CredentialCipherError> {
    validate_provider(provider)?;
    if plaintext.is_empty() || plaintext.len() > MAX_PLAINTEXT_BYTES {
        return Err(CredentialCipherError::InvalidInput);
    }
    Ok(())
}

fn validate_provider(provider: &str) -> Result<(), CredentialCipherError> {
    if provider.trim().is_empty() || provider.len() > 255 || provider.chars().any(char::is_control)
    {
        return Err(CredentialCipherError::InvalidInput);
    }
    Ok(())
}

fn associated_data(credential_id: Uuid, provider: &str) -> Vec<u8> {
    format!("tjxy:storage-credential:v1:{credential_id}:{provider}").into_bytes()
}
