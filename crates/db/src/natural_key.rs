use sha2::{Digest, Sha256};

pub(crate) fn hash(parts: &[&str]) -> String {
    let mut digest = Sha256::new();
    for part in parts {
        digest.update((part.len() as u64).to_be_bytes());
        digest.update(part.as_bytes());
    }
    format!("{:x}", digest.finalize())
}

#[cfg(test)]
mod tests {
    use super::hash;

    #[test]
    fn natural_key_hash_is_stable_and_boundary_safe() {
        assert_eq!(hash(&["drive", "object"]), hash(&["drive", "object"]));
        assert_ne!(hash(&["ab", "c"]), hash(&["a", "bc"]));
        assert_eq!(hash(&["value"]).len(), 64);
    }
}
