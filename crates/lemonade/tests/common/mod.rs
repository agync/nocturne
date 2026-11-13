//! Helpers shared by the integration tests.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use nocturne_lemonade::Dirs;

pub const FAKE: &str = env!("CARGO_BIN_EXE_fake-lemond");

/// A bundle directory holding the fake engine under the name the supervisor looks for.
pub fn bundle(parent: &Path) -> PathBuf {
    let dir = parent.join("bundle");
    std::fs::create_dir_all(&dir).unwrap();
    let target = dir.join(if cfg!(windows) {
        "lemond.exe"
    } else {
        "lemond"
    });
    #[cfg(unix)]
    std::os::unix::fs::symlink(FAKE, &target).unwrap();
    #[cfg(not(unix))]
    std::fs::copy(FAKE, &target).unwrap();
    dir
}

pub fn dirs(parent: &Path) -> Dirs {
    let dirs = Dirs::new(bundle(parent), parent.join("root"));
    dirs.ensure().unwrap();
    dirs
}

/// Puts a mode for the fake engine in `config.json`, where it survives the scrubbed environment.
pub fn set_mode(dirs: &Dirs, mode: &str) {
    let path = dirs.config().join("config.json");
    let mut v: serde_json::Value = std::fs::read(&path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    v["fake_mode"] = serde_json::json!(mode);
    std::fs::write(path, v.to_string()).unwrap();
}

pub fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// True while a process exists and is not a zombie.
#[cfg(target_os = "linux")]
pub fn alive(pid: u32) -> bool {
    match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(stat) => stat
            .rsplit_once(')')
            .and_then(|(_, rest)| rest.split_whitespace().next().map(|s| s != "Z"))
            .unwrap_or(false),
        Err(_) => false,
    }
}

#[cfg(target_os = "linux")]
pub async fn wait_dead(pid: u32, within: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < within {
        if !alive(pid) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    !alive(pid)
}

/// Processes whose group is `pgid`.
#[cfg(target_os = "linux")]
pub fn group_members(pgid: u32) -> Vec<u32> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir("/proc").unwrap().flatten() {
        let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
            continue;
        };
        if let Ok(stat) = std::fs::read_to_string(entry.path().join("stat"))
            && let Some((_, rest)) = stat.rsplit_once(')')
        {
            let fields: Vec<&str> = rest.split_whitespace().collect();
            if fields.get(2).and_then(|g| g.parse::<u32>().ok()) == Some(pgid)
                && fields.first() != Some(&"Z")
            {
                out.push(pid);
            }
        }
    }
    out
}

/// Live processes whose command line mentions `needle`, for proving nothing was left behind.
#[cfg(target_os = "linux")]
pub fn processes_mentioning(needle: &str) -> Vec<u32> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir("/proc").unwrap().flatten() {
        let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
            continue;
        };
        if let Ok(cmd) = std::fs::read(entry.path().join("cmdline"))
            && String::from_utf8_lossy(&cmd).contains(needle)
            && alive(pid)
        {
            out.push(pid);
        }
    }
    out
}

/// A fake engine run directly, without a supervisor, with the key `k3y`.
pub struct Fake {
    pub child: std::process::Child,
    pub port: u16,
    pub http: reqwest::Client,
    pub dirs: Dirs,
    pub client: nocturne_lemonade::Client,
    _tmp: tempfile::TempDir,
}

impl Fake {
    pub fn start(mode: Option<&str>) -> Fake {
        let tmp = tempfile::tempdir().unwrap();
        let dirs = dirs(tmp.path());
        if let Some(m) = mode {
            set_mode(&dirs, m);
        }
        let port = free_port();
        let child = std::process::Command::new(FAKE)
            .arg(dirs.cache())
            .arg(dirs.config())
            .args(["--port", &port.to_string(), "--host", "127.0.0.1"])
            .args(["--no-broadcast", "--watchdog-fd", "0"])
            .env_clear()
            .env("LEMONADE_API_KEY", "k3y")
            .stdin(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        Fake {
            child,
            port,
            http: reqwest::Client::new(),
            dirs,
            client: nocturne_lemonade::Client::new(port, "k3y"),
            _tmp: tmp,
        }
    }

    pub fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{path}", self.port)
    }

    pub async fn wait_live(&self) {
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

    pub async fn started(mode: Option<&str>) -> Fake {
        let fake = Fake::start(mode);
        fake.wait_live().await;
        fake
    }

    pub fn wait_exit(&mut self, within: Duration) -> Option<std::process::ExitStatus> {
        let start = Instant::now();
        while start.elapsed() < within {
            if let Some(s) = self.child.try_wait().unwrap() {
                return Some(s);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        None
    }

    /// Authenticated GET of a `/fake/*` path, as JSON.
    pub async fn peek(&self, path: &str) -> serde_json::Value {
        self.http
            .get(self.url(path))
            .bearer_auth("k3y")
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap()
    }

    /// Queues what the fake's next chat calls answer.
    pub async fn script(&self, chat: serde_json::Value) {
        self.http
            .post(self.url("/fake/script"))
            .bearer_auth("k3y")
            .json(&serde_json::json!({ "chat": chat }))
            .send()
            .await
            .unwrap();
    }

    /// Requests the fake has seen on `path`, oldest first.
    pub async fn seen(&self, path: &str) -> Vec<serde_json::Value> {
        self.peek("/fake/requests")
            .await
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["path"] == path)
            .cloned()
            .collect()
    }
}

impl Drop for Fake {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
