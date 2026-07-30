use std::collections::BTreeMap;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::Deserialize;
use thiserror::Error;
use tjxy_credentials::{CredentialCipher, CredentialKey};
use zeroize::Zeroizing;

#[derive(Debug, Error)]
pub enum CredentialKeyringError {
    #[error("credential keyring must contain an active version and Base64 32-byte keys")]
    Invalid,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SerializedCredentialKeyring {
    active_version: i32,
    keys: BTreeMap<i32, Zeroizing<String>>,
}

/// Parses the shared environment representation of the credential keyring.
///
/// # Errors
///
/// Returns [`CredentialKeyringError::Invalid`] without exposing key material when the
/// JSON, active version, Base64 payload, or key length is invalid.
pub fn parse_credential_keyring(value: &str) -> Result<CredentialCipher, CredentialKeyringError> {
    let serialized: SerializedCredentialKeyring =
        serde_json::from_str(value).map_err(|_| CredentialKeyringError::Invalid)?;
    let mut active = None;
    let mut historical = Vec::with_capacity(serialized.keys.len().saturating_sub(1));
    for (version, encoded) in serialized.keys {
        let decoded = Zeroizing::new(
            STANDARD
                .decode(encoded.as_bytes())
                .map_err(|_| CredentialKeyringError::Invalid)?,
        );
        let bytes: [u8; 32] = decoded
            .as_slice()
            .try_into()
            .map_err(|_| CredentialKeyringError::Invalid)?;
        let key =
            CredentialKey::new(version, bytes).map_err(|_| CredentialKeyringError::Invalid)?;
        if version == serialized.active_version {
            active = Some(key);
        } else {
            historical.push(key);
        }
    }
    CredentialCipher::new(active.ok_or(CredentialKeyringError::Invalid)?, historical)
        .map_err(|_| CredentialKeyringError::Invalid)
}
