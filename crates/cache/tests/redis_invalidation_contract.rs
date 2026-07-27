use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tjxy_cache::{
    CacheInvalidationOutcome, CacheInvalidator, CacheKeyBuilder, CacheRuntime, CacheStore,
    RedisCacheConfig, RedisMode,
};

#[tokio::test]
#[ignore = "requires TJXY_TEST_REDIS_URL pointing to a disposable Redis database"]
async fn generation_registry_deletes_only_bounded_obsolete_batches() {
    let url = std::env::var("TJXY_TEST_REDIS_URL")
        .expect("TJXY_TEST_REDIS_URL must point to a disposable Redis database");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let prefix = format!("tjxy-test-{nonce}");
    let keys = CacheKeyBuilder::new(&prefix).unwrap();
    let runtime = CacheRuntime::connect(
        RedisCacheConfig::new(RedisMode::Enabled, url, prefix, Duration::from_secs(1)).unwrap(),
    )
    .await
    .unwrap();

    let obsolete = (0..205)
        .map(|index| keys.catalog_item(7, &format!("item-{index}")))
        .collect::<Vec<_>>();
    for key in &obsolete {
        runtime.put(key, b"obsolete", Duration::from_secs(60)).await;
    }
    let current = keys.catalog_item(8, "current");
    runtime
        .put(&current, b"current", Duration::from_secs(60))
        .await;

    assert_eq!(
        runtime.invalidate_generation(7).await.unwrap(),
        CacheInvalidationOutcome::Deleted {
            count: 100,
            remaining: 105,
        }
    );
    assert_eq!(
        runtime.invalidate_generation(7).await.unwrap(),
        CacheInvalidationOutcome::Deleted {
            count: 100,
            remaining: 5,
        }
    );
    assert_eq!(
        runtime.invalidate_generation(7).await.unwrap(),
        CacheInvalidationOutcome::Deleted {
            count: 5,
            remaining: 0,
        }
    );
    for key in obsolete {
        assert!(runtime.get(&key).await.is_none());
    }
    assert_eq!(
        runtime.get(&current).await.as_deref(),
        Some(b"current".as_slice())
    );
    runtime.delete(&current).await;
}
