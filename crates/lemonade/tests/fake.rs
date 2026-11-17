//! The fake engine itself: it must behave like the real one on the points the tests lean on.

mod common;

use std::time::Duration;

use serde_json::Value;

#[tokio::test]
async fn live_needs_no_key_and_everything_else_does() {
    let mut fake = common::Fake::start(None);
    fake.wait_live().await;
    let no_key = fake.http.get(fake.url("/v1/health")).send().await.unwrap();
    assert_eq!(no_key.status(), 401);
    let wrong = fake
        .http
        .get(fake.url("/v1/health"))
        .bearer_auth("nope")
        .send()
        .await
        .unwrap();
    assert_eq!(wrong.status(), 401);
    let ok: Value = fake
        .http
        .get(fake.url("/v1/health"))
        .bearer_auth("k3y")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(ok["status"], "ok");
    assert_eq!(ok["version"], "fake-1.0");
}

#[tokio::test]
async fn exits_when_stdin_closes() {
    let mut fake = common::Fake::start(None);
    fake.wait_live().await;
    drop(fake.child.stdin.take());
    let status = fake.wait_exit(Duration::from_secs(5));
    assert!(status.is_some_and(|s| s.success()), "{status:?}");
}

/// A fake asked for port 0 holds the port the system gave it and says which, so tests that run
/// several at once never choose a port and release it before a fake binds it.
#[tokio::test]
async fn port_zero_binds_a_port_of_its_own_and_reports_it() {
    let mut fakes: Vec<common::Fake> = std::thread::scope(|scope| {
        let starting: Vec<_> = (0..8)
            .map(|_| scope.spawn(|| common::Fake::start(None)))
            .collect();
        starting.into_iter().map(|t| t.join().unwrap()).collect()
    });
    for fake in &mut fakes {
        fake.wait_live().await;
        assert_ne!(fake.port, 0);
        let stats = fake.peek("/fake/stats").await;
        assert_eq!(stats["port"], fake.port);
    }
    let mut ports: Vec<u16> = fakes.iter().map(|f| f.port).collect();
    ports.sort_unstable();
    ports.dedup();
    assert_eq!(ports.len(), fakes.len(), "two fakes shared a port");
}

#[tokio::test]
async fn crash_mode_comes_from_config_and_exits_nonzero() {
    let mut fake = common::Fake::start(Some("crash-after-ms=200"));
    fake.wait_live().await;
    std::thread::sleep(Duration::from_millis(400));
    assert!(
        fake.child.try_wait().unwrap().is_none(),
        "the crash clock waits for the first health check"
    );
    fake.client.health().await.unwrap();
    let status = fake.wait_exit(Duration::from_secs(5));
    assert!(status.is_some_and(|s| !s.success()), "{status:?}");
}

#[tokio::test]
async fn echoes_its_environment() {
    let mut fake = common::Fake::start(None);
    fake.wait_live().await;
    let env: Value = fake
        .http
        .get(fake.url("/fake/env"))
        .bearer_auth("k3y")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let keys: Vec<&String> = env
        .as_object()
        .unwrap()
        .keys()
        .filter(|k| !common::set_by_the_platform(k))
        .collect();
    assert_eq!(keys, vec!["LEMONADE_ADMIN_API_KEY", "LEMONADE_API_KEY"]);
}
