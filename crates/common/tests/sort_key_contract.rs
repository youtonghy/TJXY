use tjxy_common::SortKey;

#[test]
fn sort_key_is_nfkc_lowercase_utf8_bytes() {
    assert_eq!(
        SortKey::from_text("Ｔhe Éclair").as_bytes(),
        "the éclair".as_bytes()
    );
    assert_eq!(SortKey::from_text("Arrival"), SortKey::from_text("arrival"));
}
