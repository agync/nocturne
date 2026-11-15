//! Shared by the tests that run the stand-in helpers.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use nocturne_helpers::{HelperDirs, HelperKind, Installed, TransportState};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio::sync::watch;

pub struct Sandbox {
    pub root: PathBuf,
}

impl Sandbox {
    pub fn new(label: &str) -> Self {
        static N: AtomicUsize = AtomicUsize::new(0);
        let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
            "{label}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        Sandbox { root }
    }

    pub fn dirs(&self) -> HelperDirs {
        HelperDirs {
            root: self.root.join("remote"),
        }
    }

    /// A private copy of a stand-in, named like the real helper, with its pin.
    pub fn install(&self, folder: &str, from: &str, name: &str, kind: HelperKind) -> Installed {
        let dir = self.root.join("bin").join(folder);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::copy(from, &path).unwrap();
        Installed {
            kind,
            version: "fake".into(),
            sha256: sha256_of(&path),
            path,
        }
    }

    pub fn control(&self, folder: &str, value: Value) {
        let dir = self.root.join("bin").join(folder);
        std::fs::write(dir.join("fake-control.json"), value.to_string()).unwrap();
    }

    pub fn records(&self, folder: &str) -> Vec<Value> {
        let path = self.root.join("bin").join(folder).join("fake-record.jsonl");
        std::fs::read_to_string(path)
            .unwrap_or_default()
            .lines()
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect()
    }

    pub fn bin_file(&self, folder: &str, name: &str) -> PathBuf {
        self.root.join("bin").join(folder).join(name)
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        if std::env::var_os("KEEP_SANDBOX").is_some() {
            return;
        }
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

pub fn sha256_of(path: &Path) -> String {
    Sha256::digest(std::fs::read(path).unwrap())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub async fn until(what: &str, secs: u64, mut ready: impl FnMut() -> bool) {
    let deadline = std::time::Instant::now() + Duration::from_secs(secs);
    while !ready() {
        assert!(
            std::time::Instant::now() < deadline,
            "timed out waiting for {what}"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

pub async fn wait_state(
    rx: &mut watch::Receiver<TransportState>,
    what: &str,
    secs: u64,
    ready: impl Fn(&TransportState) -> bool,
) -> TransportState {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(secs);
    loop {
        let now = rx.borrow_and_update().clone();
        if ready(&now) {
            return now;
        }
        let waited = tokio::time::timeout_at(deadline, rx.changed()).await;
        assert!(
            matches!(waited, Ok(Ok(()))),
            "timed out waiting for {what}; last state {:?}",
            rx.borrow()
        );
    }
}

pub fn env_keys(record: &Value) -> Vec<String> {
    record["env"]
        .as_object()
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default()
}

pub fn argv(record: &Value) -> Vec<String> {
    record["argv"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

/// An origin on `listener` that answers every request with `reply` and hands back the head of the
/// first one for `path`. Other programs on this computer may connect to a loopback port too, so
/// a request for another path is answered and passed over, never taken for the test's own. A
/// connection that goes quiet or sends too much is dropped after two seconds or 64 KiB.
pub fn origin_for(
    listener: std::net::TcpListener,
    path: &'static str,
    reply: &'static [u8],
) -> std::thread::JoinHandle<String> {
    use std::io::{Read, Write};
    std::thread::spawn(move || {
        let wanted = format!("GET {path} ");
        loop {
            let Ok((mut conn, _)) = listener.accept() else {
                return String::new();
            };
            let _ = conn.set_read_timeout(Some(std::time::Duration::from_secs(2)));
            let mut head = Vec::new();
            let mut byte = [0u8; 1];
            while head.len() < 65536
                && !head.ends_with(b"\r\n\r\n")
                && conn.read(&mut byte).unwrap_or(0) == 1
            {
                head.push(byte[0]);
            }
            let _ = conn.write_all(reply);
            let head = String::from_utf8_lossy(&head).into_owned();
            if head.starts_with(&wanted) {
                return head;
            }
        }
    })
}
