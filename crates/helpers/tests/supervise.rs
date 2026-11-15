//! The supervisor against the stand-in helper.
#![cfg(feature = "fake")]

mod common;

use std::ffi::OsString;
use std::time::Duration;

use common::{Sandbox, argv, env_keys, sha256_of, until};
use nocturne_helpers::{HelperCommand, HelperKind, Phase, SuperviseOptions, Supervisor};
use serde_json::json;

const FAKE: &str = env!("CARGO_BIN_EXE_fake-cloudflared");

fn command(sb: &Sandbox) -> (HelperCommand, SuperviseOptions) {
    let installed = sb.install("c", FAKE, "cloudflared", HelperKind::Cloudflared);
    let cmd = HelperCommand {
        name: "cloudflared",
        sha256: installed.sha256,
        program: installed.path,
        args: ["tunnel", "--no-autoupdate"].map(OsString::from).to_vec(),
        env: vec![
            ("PATH".into(), "".into()),
            ("HOME".into(), sb.root.clone().into()),
        ],
        secret_env: Vec::new(),
        cwd: Some(sb.root.clone()),
    };
    let mut opts = SuperviseOptions::new(sb.root.join("logs"));
    opts.backoff = [100, 200, 400].map(Duration::from_millis).to_vec();
    opts.max_restarts = 3;
    opts.grace = Duration::from_secs(2);
    (cmd, opts)
}

async fn gave_up(sup: &Supervisor) {
    let mut rx = sup.subscribe();
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if matches!(rx.borrow_and_update().phase, Phase::GaveUp(_)) {
                return;
            }
            rx.changed().await.unwrap();
        }
    })
    .await
    .expect("the supervisor gives up");
}

#[tokio::test]
async fn environment_is_scrubbed() {
    let sb = Sandbox::new("scrub");
    let (cmd, opts) = command(&sb);
    let sup = Supervisor::start(cmd, opts).await.unwrap();
    until("a record", 20, || !sb.records("c").is_empty()).await;
    let mut keys = env_keys(&sb.records("c")[0]);
    keys.sort();
    assert_eq!(keys, ["HOME", "PATH"]);
    sup.stop().await.unwrap();
}

#[tokio::test]
async fn restarts_with_backoff() {
    let sb = Sandbox::new("backoff");
    let (cmd, opts) = command(&sb);
    sb.control("c", json!({"mode": "crash"}));
    let sup = Supervisor::start(cmd, opts).await.unwrap();
    gave_up(&sup).await;
    let at: Vec<u64> = sb
        .records("c")
        .iter()
        .map(|r| r["at_ms"].as_u64().unwrap())
        .collect();
    assert_eq!(at.len(), 4, "the first start and three restarts");
    for (i, want) in [100u64, 200, 400].iter().enumerate() {
        let gap = at[i + 1] - at[i];
        assert!(
            gap >= *want - 10,
            "gap {i} was {gap} ms, wanted at least {want}"
        );
    }
    sup.stop().await.unwrap();
}

#[tokio::test]
async fn gives_up_after_limit() {
    let sb = Sandbox::new("giveup");
    let (cmd, opts) = command(&sb);
    sb.control("c", json!({"mode": "crash"}));
    let sup = Supervisor::start(cmd, opts).await.unwrap();
    gave_up(&sup).await;
    let runs = sb.records("c").len();
    tokio::time::sleep(Duration::from_millis(800)).await;
    assert_eq!(sb.records("c").len(), runs, "no start after giving up");
    let state = sup.state();
    assert_eq!(state.restarts, 3);
    assert!(matches!(state.phase, Phase::GaveUp(_)));
    sup.stop().await.unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn stop_leaves_no_process_in_group() {
    let sb = Sandbox::new("group");
    let (cmd, opts) = command(&sb);
    sb.control("c", json!({"spawn_child": true}));
    let sup = Supervisor::start(cmd, opts).await.unwrap();
    let child_file = sb.bin_file("c", "fake-child-pid");
    until("the child", 20, || {
        child_file.exists() && !std::fs::read_to_string(&child_file).unwrap().is_empty()
    })
    .await;
    let child: u32 = std::fs::read_to_string(&child_file)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    let leader = sup.state().pid.expect("a pid");
    assert!(nocturne_helpers::group_members(leader).contains(&child));
    sup.stop().await.unwrap();
    assert!(nocturne_helpers::group_members(leader).is_empty());
    assert!(
        !std::path::Path::new(&format!("/proc/{child}")).exists()
            || std::fs::read_to_string(format!("/proc/{child}/stat"))
                .is_ok_and(|s| s.contains(") Z")),
        "the child outlived the stop"
    );
}

#[tokio::test]
async fn binary_unchanged_after_runs() {
    let sb = Sandbox::new("unchanged");
    let (cmd, opts) = command(&sb);
    let before = sha256_of(&cmd.program);
    sb.control("c", json!({"mode": "crash"}));
    let sup = Supervisor::start(cmd.clone(), opts).await.unwrap();
    gave_up(&sup).await;
    sup.stop().await.unwrap();
    assert_eq!(sha256_of(&cmd.program), before);
}

#[tokio::test]
async fn fake_rejects_unknown_flags() {
    let sb = Sandbox::new("flags");
    let installed = sb.install("c", FAKE, "cloudflared", HelperKind::Cloudflared);
    let run = |args: &[&str]| {
        std::process::Command::new(&installed.path)
            .args(args)
            .env_clear()
            .output()
            .unwrap()
    };
    let bad = run(&["tunnel", "--debug-me"]);
    assert_eq!(bad.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&bad.stderr).contains("flag provided but not defined"));
    let worse = run(&["tunnel", "--no-autoupdate", "--shell-out=1"]);
    assert_eq!(worse.status.code(), Some(2));
    let fine = std::process::Command::new(&installed.path)
        .args(["tunnel", "--no-autoupdate", "--hello-world"])
        .env_clear()
        .spawn()
        .unwrap();
    let mut fine = fine;
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        fine.try_wait().unwrap().is_none(),
        "known flags are accepted"
    );
    fine.kill().unwrap();
    let _ = fine.wait();
    assert!(argv(&sb.records("c")[0]).contains(&"--debug-me".to_string()));
}

#[tokio::test]
async fn a_changed_binary_is_not_restarted() {
    let sb = Sandbox::new("swap");
    let (cmd, opts) = command(&sb);
    sb.control("c", json!({"mode": "crash"}));
    let sup = Supervisor::start(cmd.clone(), opts).await.unwrap();
    until("the first run", 20, || !sb.records("c").is_empty()).await;
    let mut bytes = std::fs::read(&cmd.program).unwrap();
    bytes.push(0);
    std::fs::write(&cmd.program, bytes).unwrap();
    gave_up(&sup).await;
    assert!(
        sb.records("c").len() < 4,
        "no run of a changed helper after the swap"
    );
    sup.stop().await.unwrap();
}

#[tokio::test]
async fn a_changed_binary_is_not_started() {
    let sb = Sandbox::new("swap2");
    let (mut cmd, opts) = command(&sb);
    cmd.sha256 = "0".repeat(64);
    assert_eq!(
        Supervisor::start(cmd, opts).await.err(),
        Some(nocturne_helpers::HelperError::Mismatch)
    );
    assert!(sb.records("c").is_empty());
}

#[cfg(unix)]
#[tokio::test]
async fn stop_goes_straight_to_sigterm_when_the_stop_line_cannot_be_written() {
    let sb = Sandbox::new("closed-input");
    let sh = std::path::Path::new("/bin/sh");
    let cmd = HelperCommand {
        name: "sh",
        sha256: sha256_of(sh),
        program: sh.into(),
        args: ["-c", "exec 0<&-; : > ready; exec sleep 60"]
            .map(OsString::from)
            .to_vec(),
        env: vec![("PATH".into(), "/usr/bin:/bin".into())],
        secret_env: Vec::new(),
        cwd: Some(sb.root.clone()),
    };
    let mut opts = SuperviseOptions::new(sb.root.join("logs"));
    opts.lines = true;
    opts.stop_line = Some("stop".into());
    opts.stop_wait = Duration::from_secs(30);
    opts.grace = Duration::from_secs(2);
    let sup = Supervisor::start(cmd, opts).await.unwrap();
    let ready = sb.root.join("ready");
    until("the input is closed", 30, || ready.exists()).await;
    let asked = std::time::Instant::now();
    sup.stop().await.unwrap();
    assert!(
        asked.elapsed() < Duration::from_secs(15),
        "no stop wait: {:?}",
        asked.elapsed()
    );
}

/// What the helper prints on either output reaches its log with every secret it was given
/// hidden, and so does a command's `Debug`.
#[cfg(unix)]
#[tokio::test]
async fn the_helper_log_never_shows_its_secrets() {
    let sb = Sandbox::new("secret-log");
    let sh = std::path::Path::new("/bin/sh");
    let key = "k3y-4b5c6d7e8f9a0b1c2d3e4f5a";
    let cmd = HelperCommand {
        name: "sh",
        sha256: sha256_of(sh),
        program: sh.into(),
        args: [
            "-c",
            "echo \"err $HOST_KEY\" >&2; echo \"out $HOST_KEY\"; : > done; exec sleep 60",
        ]
        .map(OsString::from)
        .to_vec(),
        env: vec![("PATH".into(), "/usr/bin:/bin".into())],
        secret_env: vec![("HOST_KEY".into(), key.into())],
        cwd: Some(sb.root.clone()),
    };
    assert!(!format!("{cmd:?}").contains(key));
    let opts = SuperviseOptions::new(sb.root.join("logs"));
    let sup = Supervisor::start(cmd, opts).await.unwrap();
    let log = sup.log_path().to_path_buf();
    until("both lines are in the log", 30, || {
        std::fs::read_to_string(&log).is_ok_and(|t| t.contains("err ") && t.contains("out "))
    })
    .await;
    sup.stop().await.unwrap();
    let text = std::fs::read_to_string(&log).unwrap();
    assert!(
        !text.contains(&key[..8]) && !text.contains(&key[key.len() - 8..]),
        "{text}"
    );
    assert!(
        text.contains("err [key]") && text.contains("out [key]"),
        "{text}"
    );
}

/// A line longer than a log piece loses its tail, and a key that starts just before the cut is
/// never written in part.
#[cfg(unix)]
#[tokio::test]
async fn a_key_at_the_cut_of_a_long_line_is_not_written_in_part() {
    let sb = Sandbox::new("secret-cut");
    let sh = std::path::Path::new("/bin/sh");
    let key = "k3y-4b5c6d7e8f9a0b1c2d3e4f5a";
    let cmd = HelperCommand {
        name: "sh",
        sha256: sha256_of(sh),
        program: sh.into(),
        args: [
            "-c",
            "{ head -c 65533 /dev/zero | tr '\\0' a; echo \"$HOST_KEY\"; echo next; } >&2; : > done; exec sleep 60",
        ]
        .map(OsString::from)
        .to_vec(),
        env: vec![("PATH".into(), "/usr/bin:/bin".into())],
        secret_env: vec![("HOST_KEY".into(), key.into())],
        cwd: Some(sb.root.clone()),
    };
    let opts = SuperviseOptions::new(sb.root.join("logs"));
    let sup = Supervisor::start(cmd, opts).await.unwrap();
    let log = sup.log_path().to_path_buf();
    until("the next line is in the log", 30, || {
        std::fs::read_to_string(&log).is_ok_and(|t| t.contains("next"))
    })
    .await;
    sup.stop().await.unwrap();
    let text = std::fs::read_to_string(&log).unwrap();
    assert!(
        !text.contains("k3y"),
        "{}",
        &text[text.len().saturating_sub(100)..]
    );
}

/// The first line a helper is sent can carry its key; the helper's log hides it too.
#[cfg(unix)]
#[tokio::test]
async fn the_helper_log_never_shows_its_first_line() {
    let sb = Sandbox::new("secret-line");
    let sh = std::path::Path::new("/bin/sh");
    let key = "k3y-9f8e7d6c5b4a39281706f5e4";
    let cmd = HelperCommand {
        name: "sh",
        sha256: sha256_of(sh),
        program: sh.into(),
        args: [
            "-c",
            "read line; echo \"got $line\" >&2; : > done; exec sleep 60",
        ]
        .map(OsString::from)
        .to_vec(),
        env: vec![("PATH".into(), "/usr/bin:/bin".into())],
        secret_env: Vec::new(),
        cwd: Some(sb.root.clone()),
    };
    let mut opts = SuperviseOptions::new(sb.root.join("logs"));
    opts.lines = true;
    opts.first_line = Some(nocturne_helpers::Secret(format!("{{\"key\":\"{key}\"}}")));
    let sup = Supervisor::start(cmd, opts).await.unwrap();
    let log = sup.log_path().to_path_buf();
    until("the line is in the log", 30, || {
        std::fs::read_to_string(&log).is_ok_and(|t| t.contains("got "))
    })
    .await;
    sup.stop().await.unwrap();
    let text = std::fs::read_to_string(&log).unwrap();
    assert!(!text.contains(&key[..8]), "{text}");
}
