//! The real host on whatever machine runs the tests (often a CI container with no bus, no
//! battery and no session): every answer is a value or `None`, and none of them hangs.

use std::sync::Arc;
use std::time::{Duration, Instant};

use nocturne_intelligence::{Host, SystemHost, probe};

#[tokio::test]
async fn the_real_host_answers_or_says_it_does_not_know_without_hanging() {
    let host: Arc<dyn Host> = Arc::new(SystemHost::new());
    let started = Instant::now();
    let ac = probe(&host, |h| h.on_ac_power()).await;
    let percent = probe(&host, |h| h.battery_percent()).await;
    let idle = probe(&host, |h| h.idle_for()).await;
    let guard = probe(&host, |h| h.inhibit_sleep()).await;
    assert!(percent.is_none_or(|p| p <= 100), "{percent:?}");
    assert!(
        idle.is_none_or(|d| d < Duration::from_secs(60 * 60 * 24 * 365)),
        "{idle:?}"
    );
    drop(guard);
    let _ = ac;
    assert!(
        started.elapsed() < Duration::from_secs(15),
        "took {:?}",
        started.elapsed()
    );
}

#[tokio::test]
async fn answers_are_cached_for_a_while() {
    let host = SystemHost::new();
    let first = host.on_ac_power();
    let started = Instant::now();
    for _ in 0..1000 {
        assert_eq!(host.on_ac_power(), first);
    }
    assert!(started.elapsed() < Duration::from_secs(1));
}
