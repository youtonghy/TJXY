use std::time::Duration;

use tjxy_cache::{
    CacheHealth, CacheInvalidationOutcome, CacheInvalidator, CacheRuntime, CacheStartupError,
    RedisCacheConfig, RedisMode,
};

#[tokio::test]
async fn disabled_mode_never_opens_or_validates_a_redis_endpoint() {
    let config = RedisCacheConfig::new(
        RedisMode::Disabled,
        "not a redis url",
        "tjxy",
        Duration::from_millis(20),
    )
    .unwrap();

    let runtime = CacheRuntime::connect(config).await.unwrap();

    assert!(!runtime.is_enabled());
    assert_eq!(runtime.health(), CacheHealth::Disabled);
}

#[tokio::test]
async fn disabled_cache_explicitly_completes_generation_invalidation() {
    let runtime = CacheRuntime::Disabled;

    assert_eq!(
        runtime.invalidate_generation(0).await.unwrap(),
        CacheInvalidationOutcome::SkippedDisabled
    );
}

#[tokio::test]
async fn auto_only_probes_local_endpoints_and_degrades_when_unavailable() {
    let remote = RedisCacheConfig::new(
        RedisMode::Auto,
        "redis://cache.example.com:6379",
        "tjxy",
        Duration::from_millis(20),
    )
    .unwrap();
    assert!(matches!(
        CacheRuntime::connect(remote).await.unwrap_err(),
        CacheStartupError::NonLocalAutoEndpoint
    ));

    let local = RedisCacheConfig::new(
        RedisMode::Auto,
        "redis://127.0.0.1:1",
        "tjxy",
        Duration::from_millis(20),
    )
    .unwrap();
    assert!(!CacheRuntime::connect(local).await.unwrap().is_enabled());
}

#[tokio::test]
async fn enabled_mode_fails_startup_when_redis_is_unavailable() {
    let config = RedisCacheConfig::new(
        RedisMode::Enabled,
        "redis://127.0.0.1:1",
        "tjxy",
        Duration::from_millis(20),
    )
    .unwrap();

    assert!(matches!(
        CacheRuntime::connect(config).await.unwrap_err(),
        CacheStartupError::Connection(_)
    ));
}

#[test]
fn cache_configuration_rejects_unbounded_or_unsafe_values() {
    assert!(
        RedisCacheConfig::new(
            RedisMode::Auto,
            "redis://127.0.0.1",
            "bad:prefix",
            Duration::from_millis(20),
        )
        .is_err()
    );
    assert!(
        RedisCacheConfig::new(RedisMode::Auto, "redis://127.0.0.1", "tjxy", Duration::ZERO,)
            .is_err()
    );
    let secret = RedisCacheConfig::new(
        RedisMode::Enabled,
        "redis://user:super-secret@127.0.0.1:6379",
        "tjxy",
        Duration::from_millis(20),
    )
    .unwrap();
    assert!(!format!("{secret:?}").contains("super-secret"));
}
