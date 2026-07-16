use tjxy_common::{Username, UsernameError};

#[test]
fn username_key_is_nfkc_normalized_and_lowercase() {
    let username = Username::parse("Ａlice").unwrap();

    assert_eq!(username.as_str(), "Ａlice");
    assert_eq!(username.key(), b"alice");
}

#[test]
fn username_rejects_ambiguous_or_unsafe_input() {
    for value in ["", " alice", "alice ", "ali\0ce", "ali\nce"] {
        assert!(Username::parse(value).is_err(), "accepted {value:?}");
    }

    assert_eq!(
        Username::parse(&"a".repeat(129)).unwrap_err(),
        UsernameError::TooLong
    );
}
