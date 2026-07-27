use std::time::{Duration, Instant};

use tjxy_cache::{CacheFillPermit, SingleFlight};

#[tokio::test]
async fn one_key_has_one_leader_and_waiters_observe_completion() {
    let flights = SingleFlight::new(8, Duration::from_millis(50)).unwrap();
    let CacheFillPermit::Leader(leader) = flights.enter("key-a") else {
        panic!("first cache miss was not elected leader");
    };
    let CacheFillPermit::Waiter(waiter) = flights.enter("key-a") else {
        panic!("second cache miss did not join the leader");
    };

    drop(leader);

    assert!(waiter.wait().await);
    assert!(matches!(flights.enter("key-a"), CacheFillPermit::Leader(_)));
}

#[tokio::test]
async fn full_or_slow_single_flight_never_blocks_sql_fallback() {
    let flights = SingleFlight::new(1, Duration::from_millis(20)).unwrap();
    let CacheFillPermit::Leader(_leader) = flights.enter("key-a") else {
        panic!("first cache miss was not elected leader");
    };
    assert!(matches!(flights.enter("key-b"), CacheFillPermit::Bypass));
    let CacheFillPermit::Waiter(waiter) = flights.enter("key-a") else {
        panic!("same key did not join the leader");
    };

    let started = Instant::now();
    assert!(!waiter.wait().await);
    assert!(started.elapsed() < Duration::from_millis(100));
}

#[test]
fn single_flight_bounds_must_be_positive() {
    assert!(SingleFlight::new(0, Duration::from_millis(1)).is_err());
    assert!(SingleFlight::new(1, Duration::ZERO).is_err());
}
