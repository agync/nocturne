//! Helpers shared by the integration tests.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use nocturne_lemonade::Dirs;

pub const FAKE: &str = env!("CARGO_BIN_EXE_fake-lemond");

/// Whether a variable in a child's environment is there for the platform's sake rather than
/// passed on purpose: on macOS, CoreFoundation sets `__CF_USER_TEXT_ENCODING` in every process
/// that loads it, and on Windows the child gets `SystemRoot` (see [`clear_env`]).
pub fn set_by_the_platform(name: &str) -> bool {
    (cfg!(target_os = "macos") && name == "__CF_USER_TEXT_ENCODING")
        || (cfg!(windows) && name == "SystemRoot")
}

/// Clears a child's environment as the supervisor does. Winsock cannot start in a Windows process
/// without `SystemRoot`, so there it is passed on, as the supervisor passes it.
pub fn clear_env(cmd: &mut std::process::Command) -> &mut std::process::Command {
    cmd.env_clear();
    if cfg!(windows)
        && let Some(root) = std::env::var_os("SystemRoot")
    {
        cmd.env("SystemRoot", root);
    }
    cmd
}

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

/// The port a fake started with `--port 0` bound, which it writes to its cache directory once it
/// listens. The fake chooses and holds the port itself, so there is no moment between a test
/// picking a port and the fake taking it for something else to take it in. Panics if the fake
/// exits first or says nothing within `within`.
pub fn bound_port(dirs: &Dirs, child: &mut std::process::Child, within: Duration) -> u16 {
    let start = Instant::now();
    loop {
        if let Ok(text) = std::fs::read_to_string(dirs.cache().join("fake-port"))
            && let Ok(port) = text.trim().parse()
        {
            return port;
        }
        if let Some(status) = child.try_wait().unwrap() {
            panic!("fake-lemond exited before it listened ({status:?})");
        }
        if start.elapsed() >= within {
            let _ = child.kill();
            let _ = child.wait();
            panic!("fake-lemond did not say which port it bound within {within:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// A port nothing listens on, for "no engine" tests: connecting is refused, always.
pub const CLOSED_PORT: u16 = 1;

/// One row of `ps`: process id, group id, state and command line.
#[cfg(all(unix, not(target_os = "linux")))]
fn ps_rows() -> Vec<(u32, u32, String, String)> {
    let out = std::process::Command::new("/bin/ps")
        .args(["-axww", "-o", "pid=,pgid=,stat=,command="])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| {
            let mut cols = line.split_whitespace();
            let pid = cols.next()?.parse().ok()?;
            let pgid = cols.next()?.parse().ok()?;
            let state = cols.next()?.to_string();
            let command = cols.collect::<Vec<_>>().join(" ");
            Some((pid, pgid, state, command))
        })
        .collect()
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

#[cfg(all(unix, not(target_os = "linux")))]
pub fn alive(pid: u32) -> bool {
    ps_rows()
        .iter()
        .any(|(p, _, state, _)| *p == pid && !state.starts_with('Z'))
}

#[cfg(unix)]
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

#[cfg(all(unix, not(target_os = "linux")))]
pub fn group_members(pgid: u32) -> Vec<u32> {
    ps_rows()
        .into_iter()
        .filter(|(_, group, state, _)| *group == pgid && !state.starts_with('Z'))
        .map(|(pid, ..)| pid)
        .collect()
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

#[cfg(all(unix, not(target_os = "linux")))]
pub fn processes_mentioning(needle: &str) -> Vec<u32> {
    ps_rows()
        .into_iter()
        .filter(|(_, _, state, command)| command.contains(needle) && !state.starts_with('Z'))
        .map(|(pid, ..)| pid)
        .collect()
}

/// A fake engine run directly, without a supervisor, with the key `k3y`.
pub struct Fake {
    pub child: std::process::Child,
    pub port: u16,
    pub http: reqwest::Client,
    pub dirs: Dirs,
    pub client: nocturne_lemonade::Client,
    pub admin: nocturne_lemonade::Admin,
    _tmp: tempfile::TempDir,
}

impl Fake {
    pub fn start(mode: Option<&str>) -> Fake {
        Fake::start_with_admin_key(mode, "adm1n")
    }

    /// A fake given `admin` as its admin key, which may be empty.
    pub fn start_with_admin_key(mode: Option<&str>, admin: &str) -> Fake {
        let tmp = tempfile::tempdir().unwrap();
        let dirs = dirs(tmp.path());
        if let Some(m) = mode {
            set_mode(&dirs, m);
        }
        let mut child = clear_env(&mut std::process::Command::new(FAKE))
            .arg(dirs.cache())
            .arg(dirs.config())
            .args(["--port", "0", "--host", "127.0.0.1"])
            .args(["--no-broadcast", "--watchdog-fd", "0"])
            .env("LEMONADE_API_KEY", "k3y")
            .env("LEMONADE_ADMIN_API_KEY", admin)
            .stdin(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let port = bound_port(&dirs, &mut child, Duration::from_secs(20));
        Fake {
            child,
            port,
            http: nocturne_lemonade::tls::client(),
            dirs,
            client: nocturne_lemonade::Client::new(port, "k3y"),
            admin: nocturne_lemonade::Admin::new(
                port,
                if admin.is_empty() { "k3y" } else { admin },
            )
            .unwrap(),
            _tmp: tmp,
        }
    }

    pub fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{path}", self.port)
    }

    /// Whether the fake answers `/live` within `limit`. Each probe has its own short limit: on
    /// Windows a connection to a port nobody listens on takes about two seconds to be refused.
    async fn live_within(&self, limit: Duration) -> bool {
        let deadline = Instant::now() + limit;
        while Instant::now() < deadline {
            let probe = self
                .http
                .get(self.url("/live"))
                .timeout(Duration::from_millis(500));
            if let Ok(r) = probe.send().await
                && r.status().is_success()
            {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        false
    }

    /// What a test that waited in vain reports: the port and whether the fake is still running.
    fn not_live(&mut self, limit: Duration) -> String {
        format!(
            "fake-lemond did not answer /live on 127.0.0.1:{} within {limit:?} (exit status: {:?})",
            self.port,
            self.child.try_wait().ok().flatten()
        )
    }

    pub async fn wait_live(&mut self) {
        let limit = Duration::from_secs(5);
        if !self.live_within(limit).await {
            panic!("{}", self.not_live(limit));
        }
    }

    /// Start a fake and wait for it to answer.
    pub async fn started(mode: Option<&str>) -> Fake {
        let mut fake = Fake::start(mode);
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

/// How many times the fake has been launched in this root. Retries a moment in case the count
/// is being replaced as it is read.
pub fn launch_count(dirs: &Dirs) -> u64 {
    for _ in 0..40 {
        if let Ok(text) = std::fs::read_to_string(dirs.cache().join("fake-launches"))
            && let Ok(n) = text.trim().parse()
        {
            return n;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    panic!("no launch count");
}

/// When the fake launched and crashed, in order, as wall-clock milliseconds. Written by the engine
/// itself, so how long a restart took does not depend on when a test got to look.
pub fn timeline(dirs: &Dirs) -> Vec<(String, u128)> {
    std::fs::read_to_string(dirs.cache().join("fake-timeline"))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            let (what, ms) = l.split_once(' ')?;
            Some((what.to_string(), ms.parse().ok()?))
        })
        .collect()
}

/// The launch count once it has held still for a while.
pub fn settled_launch_count(dirs: &Dirs) -> u64 {
    let mut last = launch_count(dirs);
    let mut steady = 0;
    while steady < 10 {
        std::thread::sleep(Duration::from_millis(50));
        let now = launch_count(dirs);
        if now == last {
            steady += 1;
        } else {
            last = now;
            steady = 0;
        }
    }
    last
}
