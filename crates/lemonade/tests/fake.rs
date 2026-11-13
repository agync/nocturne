//! The fake engine itself: it must behave like the real one on the points the tests lean on.

mod common;

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

struct Fake {
    child: std::process::Child,
    port: u16,
    http: reqwest::Client,
    _tmp: tempfile::TempDir,
}

impl Fake {
    fn start(mode: Option<&str>) -> Fake {
        let tmp = tempfile::tempdir().unwrap();
        let dirs = common::dirs(tmp.path());
        if let Some(m) = mode {
            common::set_mode(&dirs, m);
        }
        let port = common::free_port();
        let child = Command::new(common::FAKE)
            .arg(dirs.cache())
            .arg(dirs.config())
            .args(["--port", &port.to_string(), "--host", "127.0.0.1"])
            .args(["--no-broadcast", "--watchdog-fd", "0"])
            .env_clear()
            .env("LEMONADE_API_KEY", "k3y")
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        Fake {
            child,
            port,
            http: reqwest::Client::new(),
            _tmp: tmp,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{path}", self.port)
    }

    async fn wait_live(&self) {
        for _ in 0..100 {
            if let Ok(r) = self.http.get(self.url("/live")).send().await
                && r.status().is_success()
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("fake did not come up");
    }

    fn wait_exit(&mut self, within: Duration) -> Option<std::process::ExitStatus> {
        let start = Instant::now();
        while start.elapsed() < within {
            if let Some(s) = self.child.try_wait().unwrap() {
                return Some(s);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        None
    }
}

impl Drop for Fake {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[tokio::test]
async fn live_needs_no_key_and_everything_else_does() {
    let fake = Fake::start(None);
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
    let mut fake = Fake::start(None);
    fake.wait_live().await;
    drop(fake.child.stdin.take());
    let status = fake.wait_exit(Duration::from_secs(5));
    assert!(status.is_some_and(|s| s.success()), "{status:?}");
}

#[tokio::test]
async fn crash_mode_comes_from_config_and_exits_nonzero() {
    let mut fake = Fake::start(Some("crash-after-ms=200"));
    let status = fake.wait_exit(Duration::from_secs(5));
    assert!(status.is_some_and(|s| !s.success()), "{status:?}");
}

#[tokio::test]
async fn echoes_its_environment() {
    let fake = Fake::start(None);
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
    let keys: Vec<&String> = env.as_object().unwrap().keys().collect();
    assert_eq!(keys, vec!["LEMONADE_API_KEY"]);
}
