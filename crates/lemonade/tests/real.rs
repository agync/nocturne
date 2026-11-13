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
    let client = sup.client();

    let health = client.health().await.unwrap();
    assert_eq!(health.version, version);
    let cfg = sup.admin().config().await.unwrap();
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

    // Backends read as unsupported while the engine may not fetch executables, and it reads
    // that switch only at launch. Relaunching online shows what setup would see; nothing is
    // fetched by looking.
    sup.set_online(true).await.unwrap();
    let client = sup.client();
    let info = client.system_info().await.unwrap();
    let relaunched_from = sup.pid().unwrap();
    sup.set_online(false).await.unwrap();
    let pgid = sup.pid().unwrap();
    assert_ne!(pgid, relaunched_from);
    #[cfg(unix)]
    assert_eq!(common::group_members(relaunched_from), Vec::<u32>::new());
    assert_eq!(
        sup.admin().config().await.unwrap()["no_fetch_executables"],
        true
    );
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

    // With nothing to fetch the engine lists only models it can already run, so the names are
    // checked against the catalogue that ships in the bundle.
    let catalogue: serde_json::Value = serde_json::from_slice(
        &std::fs::read(dirs.bundle.join("resources/server_models.json")).unwrap(),
    )
    .unwrap();
    for tier in [
        nocturne_contract::Tier::Large,
        nocturne_contract::Tier::Standard,
        nocturne_contract::Tier::Small,
    ] {
        for c in tier::components(tier) {
            assert!(
                catalogue.get(c.model).is_some(),
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
    #[cfg(unix)]
    {
        assert_eq!(common::group_members(pgid), Vec::<u32>::new());
        assert!(common::wait_dead(pgid, Duration::from_secs(5)).await);
    }
}

#[cfg(unix)]
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

fn files_under(dir: &std::path::Path) -> usize {
    std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| {
                    if e.path().is_dir() {
                        files_under(&e.path())
                    } else {
                        1
                    }
                })
                .sum()
        })
        .unwrap_or(0)
}

/// The engine fetches a missing model on first use even when offline. The mind must not let it.
#[tokio::test]
#[ignore = "needs NOCTURNE_LEMOND"]
async fn the_mind_never_triggers_a_download_for_a_missing_model() {
    use nocturne_engine::{Mind, MindError};
    let tmp = tempfile::tempdir().unwrap();
    let dirs = nocturne_lemonade::Dirs::new(bundle(), tmp.path().join("root"));
    let sup = Supervisor::start(dirs.clone(), SupervisorOptions::default())
        .await
        .unwrap();
    let client = sup.client();
    let small = nocturne_contract::Tier::Small;
    let picks = nocturne_lemonade::selection::picks(
        &nocturne_lemonade::selection::Selection::preset(small),
    );
    let mind = nocturne_lemonade::LemonadeMind::new(client.clone(), picks, small)
        .with_model(nocturne_contract::ComponentRole::Speech, "Whisper-Tiny");
    let wav = tmp.path().join("one-second.wav");
    let mut bytes = b"RIFF".to_vec();
    bytes.extend((36u32 + 32_000).to_le_bytes());
    bytes.extend(b"WAVEfmt ");
    bytes.extend(16u32.to_le_bytes());
    bytes.extend([1, 0, 1, 0]);
    bytes.extend(16_000u32.to_le_bytes());
    bytes.extend(32_000u32.to_le_bytes());
    bytes.extend([2, 0, 16, 0]);
    bytes.extend(b"data");
    bytes.extend(32_000u32.to_le_bytes());
    bytes.extend(vec![0u8; 32_000]);
    std::fs::write(&wav, bytes).unwrap();

    let result = mind.transcribe(&wav).await;

    let started = client.downloads().await.unwrap();
    for job in &started {
        let _ = client
            .control(&job.id, nocturne_lemonade::client::Action::Cancel)
            .await;
    }
    assert!(started.is_empty(), "a download began: {started:?}");
    assert_eq!(result.unwrap_err(), MindError::Unavailable);
    let models = client.models().await.unwrap();
    assert!(
        models
            .iter()
            .filter(|m| m.id == "Whisper-Tiny")
            .all(|m| !m.present() && !m.downloaded),
        "with no executables to fetch the engine may not list it at all"
    );
    assert_eq!(
        files_under(&dirs.models()),
        0,
        "nothing reached the models folder"
    );
    sup.shutdown().await.unwrap();
}

/// Runs setup end to end against the real engine for the smallest model there is. It reaches
/// the network (the model, about 75 MB, and the speech backend), so it runs only with
/// `NOCTURNE_REAL_PULL=1`, and it works in a scratch folder that is removed afterwards.
#[tokio::test]
#[ignore = "needs NOCTURNE_LEMOND and NOCTURNE_REAL_PULL=1"]
async fn setup_pulls_whisper_tiny_through_the_real_engine() {
    use nocturne_contract::{ComponentRole, LedgerKind, SetupPhase, Tier};
    use nocturne_lemonade::{Component, Setup, SetupOptions};
    use std::sync::{Arc, Mutex};

    if std::env::var("NOCTURNE_REAL_PULL").as_deref() != Ok("1") {
        println!("skipped: set NOCTURNE_REAL_PULL=1 to download Whisper-Tiny (75 MB)");
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let dirs = nocturne_lemonade::Dirs::new(bundle(), tmp.path().join("root"));
    let sup = Supervisor::start(dirs.clone(), SupervisorOptions::default())
        .await
        .unwrap();
    let ledger: Arc<Mutex<Vec<(LedgerKind, String, u64)>>> = Arc::default();
    let sink = ledger.clone();
    let setup = Setup::with_options(
        &sup,
        dirs.models(),
        move |k, label, bytes| sink.lock().unwrap().push((k, label, bytes)),
        SetupOptions {
            ram_bytes: Some(16 << 30),
            components: Some(vec![Component {
                role: ComponentRole::Speech,
                model: "Whisper-Tiny",
                bytes: 75_000_000,
            }]),
            ..SetupOptions::default()
        },
    );
    assert_eq!(setup.check().await.unwrap().phase, SetupPhase::Choosing);
    setup.start(Tier::Small).await.unwrap();
    let mut view = setup.view();
    let outcome = tokio::time::timeout(Duration::from_secs(900), async {
        loop {
            let v = view.borrow_and_update().clone();
            println!("{:?} {}/{}", v.phase, v.downloaded_bytes, v.total_bytes);
            if matches!(v.phase, SetupPhase::Done | SetupPhase::Failed) {
                return v;
            }
            view.changed().await.unwrap();
        }
    })
    .await
    .expect("setup did not finish in 15 minutes");
    assert_eq!(outcome.phase, SetupPhase::Done, "{outcome:?}");

    assert_eq!(setup.installed(), Some(Tier::Small));
    let ledger = ledger.lock().unwrap().clone();
    println!("ledger: {ledger:?}");
    assert!(
        ledger
            .iter()
            .any(|(_, l, b)| l == "Whisper-Tiny" && *b > 50_000_000)
    );
    assert!(files_under(&dirs.models()) > 0);
    let cfg = sup.admin().config().await.unwrap();
    assert_eq!(cfg["offline"], true);
    assert_eq!(cfg["no_fetch_executables"], true);
    let models = sup.client().models().await.unwrap();
    assert!(models.iter().any(|m| m.id == "Whisper-Tiny" && m.present()));
    sup.shutdown().await.unwrap();
}
