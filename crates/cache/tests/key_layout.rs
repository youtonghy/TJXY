use tjxy_cache::{CacheKeyBuilder, CacheProjection, CacheQueryDigest, PlaybackProbeDigest};

#[test]
fn user_scoped_keys_include_catalog_and_user_revisions() {
    let keys = CacheKeyBuilder::new("tjxy").unwrap();

    let digest = CacheQueryDigest::from_bytes(b"resume:start=0:limit=20");
    let key = keys.user_scoped(42, "user-7", 11, CacheProjection::Resume, &digest);

    assert_eq!(key.len(), "tjxy:v1:g:42:u:user-7:r:11:resume:".len() + 64);
    assert!(key.starts_with("tjxy:v1:g:42:u:user-7:r:11:resume:"));
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
fn generation_registry_is_exact_and_bounded_to_the_prefix() {
    let keys = CacheKeyBuilder::new("tjxy").unwrap();

    assert_eq!(keys.generation_registry(42).unwrap(), "tjxy:v1:g:42:keys");
    assert!(keys.generation_registry(-1).is_err());
    assert!(CacheKeyBuilder::new("tjxy*").is_err());
    assert!(CacheKeyBuilder::new("tjxy?").is_err());
    assert!(CacheKeyBuilder::new("tjxy[old]").is_err());
    assert!(CacheKeyBuilder::new("tjxy\\").is_err());
}

#[test]
fn key_segments_reject_delimiter_injection() {
    assert!(CacheKeyBuilder::new("tjxy:other").is_err());
    assert!(PlaybackProbeDigest::new("source:secret").is_err());
}
