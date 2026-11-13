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
        program: installed.path,
        args: ["tunnel", "--no-autoupdate"].map(OsString::from).to_vec(),
        env: vec![
            ("PATH".into(), "".into()),
            ("HOME".into(), sb.root.clone().into()),
        ],
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
