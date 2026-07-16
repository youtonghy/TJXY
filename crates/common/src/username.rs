use thiserror::Error;
use unicode_normalization::UnicodeNormalization;

const MAX_USERNAME_CHARS: usize = 128;
const MAX_USERNAME_KEY_BYTES: usize = 512;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Username {
    display: String,
    key: Vec<u8>,
}

impl Username {
    /// Validates a display username and derives its cross-database identity key.
    ///
    /// The identity key is Unicode NFKC followed by Unicode lowercase mapping.
    /// It is persisted as bytes so `SQLite` and `PostgreSQL` collation cannot change
    /// equality semantics.
    ///
    /// # Errors
    ///
    /// Returns [`UsernameError`] for empty, padded, control-containing, or
    /// oversized input.
    pub fn parse(value: &str) -> Result<Self, UsernameError> {
        if value.is_empty() {
            return Err(UsernameError::Empty);
        }
        if value.starts_with(char::is_whitespace) || value.ends_with(char::is_whitespace) {
            return Err(UsernameError::SurroundingWhitespace);
        }
        if value.chars().any(char::is_control) {
            return Err(UsernameError::ControlCharacter);
        }
        if value.chars().count() > MAX_USERNAME_CHARS {
            return Err(UsernameError::TooLong);
        }

        let normalized = value
            .nfkc()
            .flat_map(char::to_lowercase)
            .collect::<String>();
        if normalized.is_empty() {
            return Err(UsernameError::Empty);
        }
        if normalized.len() > MAX_USERNAME_KEY_BYTES {
            return Err(UsernameError::TooLong);
        }

        Ok(Self {
            display: value.to_owned(),
            key: normalized.into_bytes(),
        })
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.display
    }

    #[must_use]
    pub fn key(&self) -> &[u8] {
        &self.key
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum UsernameError {
    #[error("username must not be empty")]
    Empty,
    #[error("username must not have leading or trailing whitespace")]
    SurroundingWhitespace,
    #[error("username must not contain control characters")]
    ControlCharacter,
    #[error("username exceeds the supported length")]
    TooLong,
}
