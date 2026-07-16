use tjxy_cache::{CacheKeyBuilder, CacheProjection, PlaybackProbeDigest};

#[test]
fn user_scoped_keys_include_catalog_and_user_revisions() {
    let keys = CacheKeyBuilder::new("tjxy").unwrap();

    let key = keys.user_scoped(42, "user-7", 11, CacheProjection::Resume, "query-a1");

    assert_eq!(key, "tjxy:v1:g:42:u:user-7:r:11:resume:query-a1");
}

#[test]
fn non_user_and_playback_keys_keep_generation_and_probe_digest() {
    let keys = CacheKeyBuilder::new("tjxy").unwrap();

    assert_eq!(keys.catalog_item(9, "item-2"), "tjxy:v1:g:9:item:item-2");
    assert_eq!(
        keys.playback_info(
            9,
            "user-7",
            11,
            "item-2",
            &PlaybackProbeDigest::new("source-a=3,source-b=8").unwrap(),
        ),
        "tjxy:v1:g:9:u:user-7:r:11:playback:item-2:p:source-a=3,source-b=8"
    );
}

#[test]
fn key_segments_reject_delimiter_injection() {
    assert!(CacheKeyBuilder::new("tjxy:other").is_err());
    assert!(PlaybackProbeDigest::new("source:secret").is_err());
}
