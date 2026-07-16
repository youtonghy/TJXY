use unicode_normalization::UnicodeNormalization;

/// Database-independent Unicode ordering key.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SortKey(Vec<u8>);

impl SortKey {
    #[must_use]
    pub fn from_text(value: &str) -> Self {
        let normalized = value
            .nfkc()
            .flat_map(char::to_lowercase)
            .collect::<String>();
        Self(normalized.into_bytes())
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }
}
