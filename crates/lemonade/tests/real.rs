//! Smoke tests against the real `lemond`. Ignored unless `NOCTURNE_LEMOND` names a bundle:
//!
//! ```text
//! NOCTURNE_LEMOND=vendor/lemonade/linux-x86_64 cargo test -p nocturne-lemonade -- --ignored
//! ```
//!
//! They bind loopback only, use a scratch directory, never touch the network and never pull a model.

mod common;

use std::path::PathBuf;
use std::time::Duration;

use nocturne_lemonade::client::BackendState;
use nocturne_lemonade::{RuntimeState, Supervisor, SupervisorOptions, tier};
use tokio::io::{AsyncBufReadExt, BufReader};

fn bundle() -> PathBuf {
    let given = PathBuf::from(
        std::env::var("NOCTURNE_LEMOND").expect("set NOCTURNE_LEMOND to the bundle directory"),
    );
    if given.is_absolute() || given.exists() {
        return given;
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(given)
}

#[tokio::test]
#[ignore = "needs NOCTURNE_LEMOND"]
async fn the_real_engine_starts_answers_and_stops_cleanly() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = nocturne_lemonade::Dirs::new(bundle(), tmp.path().join("root"));
    let sup = Supervisor::start(dirs.clone(), SupervisorOptions::default())
        .await
        .unwrap();
    let RuntimeState::Ready { port, version } = sup.state().borrow().clone() else {
        panic!("not ready");
    };
    assert!(!version.is_empty());
    let pgid = sup.pid().unwrap();
    let client = sup.client();

    let health = client.health().await.unwrap();
    assert_eq!(health.version, version);
    let cfg = client.config().await.unwrap();
    assert_eq!(cfg["host"], "127.0.0.1");
    assert_eq!(cfg["offline"], true);
    assert_eq!(cfg["broadcast"], false);
    assert_eq!(cfg["telemetry"]["enabled"], false);
    assert_eq!(cfg["models_dir"], dirs.models().to_string_lossy().as_ref());
    let ws = cfg["websocket_port"].as_u64().unwrap() as u16;
    assert_eq!(health.websocket_port, Some(ws));
    assert_ne!(ws, 9000);
    assert_ne!(ws, port);

    let raw = reqwest::get(format!("http://127.0.0.1:{port}/v1/health"))
        .await
        .unwrap();
    assert_eq!(raw.status(), 401, "everything but /live wants the key");

    let info = client.system_info().await.unwrap();
    let ram = {
        let mut sys = sysinfo::System::new();
        sys.refresh_memory();
        sys.total_memory()
    };
    let free = info.free_bytes().expect("system-info reports free space");
    let detected = tier::detect(ram, free);
    let llama = tier::choose_backend(&info, "llamacpp");
    println!("tier={detected:?} llamacpp={llama} free_gib={}", free >> 30);
    assert_ne!(
        info.backend_state("llamacpp", llama),
        BackendState::Unsupported
    );

    let models = client.models().await.unwrap();
    for tier in [
        nocturne_contract::Tier::Large,
        nocturne_contract::Tier::Standard,
        nocturne_contract::Tier::Small,
    ] {
        for c in tier::components(tier) {
            assert!(
                models.iter().any(|m| m.id == c.model),
                "{} is not in the catalogue",
                c.model
            );
        }
    }

    assert!(
        std::process::Command::new("kill")
            .args(["-0", &pgid.to_string()])
            .status()
            .unwrap()
            .success()
    );
    sup.shutdown().await.unwrap();
    #[cfg(target_os = "linux")]
    {
        assert_eq!(common::group_members(pgid), Vec::<u32>::new());
        assert!(common::wait_dead(pgid, Duration::from_secs(5)).await);
    }
}

#[cfg(target_os = "linux")]
#[tokio::test]
#[ignore = "needs NOCTURNE_LEMOND"]
async fn the_real_engine_exits_when_its_owner_is_killed() {
    let tmp = tempfile::tempdir().unwrap();
    let mut host = tokio::process::Command::new(env!("CARGO_BIN_EXE_supervisor-host"))
        .arg(bundle())
        .arg(tmp.path().join("root"))
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(host.stdout.take().unwrap()).lines();
    let line = tokio::time::timeout(Duration::from_secs(30), lines.next_line())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let pid: u32 = line.split_whitespace().nth(2).unwrap().parse().unwrap();
    assert!(common::alive(pid));
    host.kill().await.unwrap();
    let killed = std::time::Instant::now();
    let gone = common::wait_dead(pid, Duration::from_secs(10)).await;
    println!(
        "engine exited {:?} after its owner was killed",
        killed.elapsed()
    );
    assert!(gone, "the real engine outlived its owner");
    assert_eq!(common::group_members(pid), Vec::<u32>::new());
}
