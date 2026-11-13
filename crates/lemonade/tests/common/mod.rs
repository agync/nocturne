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
