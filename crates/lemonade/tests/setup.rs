//! Setup against the fake engine: downloads, pauses, a dropped network, a full disk and restarts.

mod common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use nocturne_contract::{LedgerKind, PauseReason, SetupError, SetupPhase, SetupView, Tier};
use nocturne_lemonade::tier::{self, DISK_HEADROOM};
use nocturne_lemonade::{Dirs, Setup, SetupOptions, Supervisor, SupervisorOptions};
use serde_json::{Value, json};
use tokio::sync::watch;

const GIB: u64 = 1 << 30;

type Ledger = Arc<Mutex<Vec<(LedgerKind, String, u64)>>>;

struct Rig {
    sup: Supervisor,
    setup: Setup,
    dirs: Dirs,
    ledger: Ledger,
    _tmp: tempfile::TempDir,
}

fn setup_opts(ram_gib: u64) -> SetupOptions {
    SetupOptions {
        poll: Duration::from_millis(20),
        offline_after: Duration::from_millis(500),
        retry_delay: Duration::from_millis(100),
        ram_bytes: Some(ram_gib * GIB),
        ..SetupOptions::default()
    }
}

fn sup_opts() -> SupervisorOptions {
    SupervisorOptions {
        health_interval: Duration::from_millis(200),
        backoff_base: Duration::from_millis(100),
        ..SupervisorOptions::default()
    }
}

fn new_setup(sup: &Supervisor, dirs: &Dirs, ram_gib: u64, ledger: &Ledger) -> Setup {
    let sink = ledger.clone();
    Setup::with_options(
        sup,
        dirs.models(),
        move |kind, label, bytes| sink.lock().unwrap().push((kind, label, bytes)),
        setup_opts(ram_gib),
    )
}

async fn rig(mode: &str, ram_gib: u64) -> Rig {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    common::set_mode(&dirs, mode);
    let sup = Supervisor::start(dirs.clone(), sup_opts()).await.unwrap();
    let ledger: Ledger = Arc::default();
    let setup = new_setup(&sup, &dirs, ram_gib, &ledger);
    Rig {
        sup,
        setup,
        dirs,
        ledger,
        _tmp: tmp,
    }
}

async fn wait_view(
    rx: &mut watch::Receiver<SetupView>,
    want: impl Fn(&SetupView) -> bool,
    secs: u64,
) -> SetupView {
    tokio::time::timeout(Duration::from_secs(secs), async {
        loop {
            let current = rx.borrow_and_update().clone();
            if want(&current) {
                return current;
            }
            rx.changed().await.unwrap();
        }
    })
    .await
    .unwrap_or_else(|_| panic!("view never matched; last: {:?}", rx.borrow().clone()))
}

async fn raw(sup: &Supervisor, method: &str, path: &str, body: Value) -> Value {
    let http = reqwest::Client::new();
    for _ in 0..100 {
        if let Some(e) = sup.client().endpoint() {
            let url = format!("http://127.0.0.1:{}{path}", e.port);
            let rb = if method == "POST" {
                http.post(url).json(&body)
            } else {
                http.get(url)
            };
            if let Ok(r) = rb.bearer_auth(e.key).send().await
                && let Ok(v) = r.json().await
            {
                return v;
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the engine never answered {path}");
}

async fn wait_offline(sup: &Supervisor) {
    for _ in 0..200 {
        if let Ok(cfg) = sup.client().config().await
            && cfg["offline"] == true
            && cfg["no_fetch_executables"] == true
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the engine was left online");
}

async fn requests_to(sup: &Supervisor, path: &str) -> Vec<Value> {
    raw(sup, "GET", "/fake/requests", json!({}))
        .await
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["path"] == path)
        .cloned()
        .collect()
}

#[tokio::test]
async fn a_whole_setup_goes_checking_choosing_downloading_installing_done() {
    let rig = rig("download-ms=150", 32).await;
    let mut rx = rig.setup.view();
    let offline_while_downloading = {
        let mut rx = rig.setup.view();
        let client = rig.sup.client();
        tokio::spawn(async move {
            while rx.changed().await.is_ok() {
                let (phase, bytes) = {
                    let v = rx.borrow_and_update();
                    (v.phase, v.downloaded_bytes)
                };
                if phase == SetupPhase::Downloading
                    && bytes > 0.0
                    && let Ok(cfg) = client.config().await
                {
                    return cfg["offline"] == false;
                }
                if phase == SetupPhase::Done {
                    break;
                }
            }
            false
        })
    };

    let checked = rig.setup.check().await.unwrap();
    assert_eq!(checked.phase, SetupPhase::Choosing);
    assert_eq!(checked.detected, Some(Tier::Standard));
    assert_eq!(checked.ram_gib, 32);
    assert_eq!(checked.options.len(), 3);
    assert!(
        checked
            .options
            .iter()
            .find(|o| o.tier == Tier::Standard)
            .unwrap()
            .fits
    );
    assert!(
        !checked
            .options
            .iter()
            .find(|o| o.tier == Tier::Large)
            .unwrap()
            .fits
    );
    assert_eq!(rig.setup.installed(), None);

    rig.setup.start(Tier::Standard).await.unwrap();
    let mid = wait_view(
        &mut rx,
        |v| v.phase == SetupPhase::Downloading && v.downloaded_bytes > 0.0,
        20,
    )
    .await;
    assert!(mid.total_bytes > mid.downloaded_bytes);
    let done = wait_view(&mut rx, |v| v.phase == SetupPhase::Done, 30).await;
    assert_eq!(done.chosen, Some(Tier::Standard));
    assert!(done.downloaded_bytes >= done.total_bytes && done.total_bytes > 0.0);
    assert!(
        offline_while_downloading.await.unwrap(),
        "downloads run with the network switched on"
    );

    let phases = rig.setup.history();
    let order: Vec<SetupPhase> = [
        SetupPhase::Checking,
        SetupPhase::Choosing,
        SetupPhase::Downloading,
        SetupPhase::Installing,
        SetupPhase::Done,
    ]
    .into_iter()
    .collect();
    let mut cursor = phases.iter();
    for want in order {
        assert!(
            cursor.any(|p| *p == want),
            "{want:?} missing from {phases:?}"
        );
    }

    let record: Value = serde_json::from_slice(
        &std::fs::read(rig.dirs.config().join("nocturne-setup.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(record["tier"], "standard");
    assert_eq!(record["done"], true);
    assert_eq!(record["backends"]["llamacpp"], "vulkan");
    assert_eq!(rig.setup.installed(), Some(Tier::Standard));
    assert_eq!(rig.sup.client().config().await.unwrap()["offline"], true);

    let ledger = rig.ledger.lock().unwrap().clone();
    for component in tier::components(Tier::Standard) {
        let hits: Vec<_> = ledger
            .iter()
            .filter(|(_, l, _)| l == component.model)
            .collect();
        assert_eq!(hits.len(), 1, "{} reported {hits:?}", component.model);
        assert!(hits[0].2 > 0);
        assert_eq!(hits[0].0, LedgerKind::Setup);
    }

    let again = new_setup(&rig.sup, &rig.dirs, 32, &rig.ledger);
    let view = again.check().await.unwrap();
    assert_eq!(view.phase, SetupPhase::Done);
    assert_eq!(view.chosen, Some(Tier::Standard));
    rig.sup.shutdown().await.unwrap();
}

#[tokio::test]
async fn pausing_takes_the_engine_offline_and_resuming_brings_it_back_online() {
    let rig = rig("download-ms=1200", 32).await;
    let mut rx = rig.setup.view();
    rig.setup.start(Tier::Small).await.unwrap();
    wait_view(
        &mut rx,
        |v| v.phase == SetupPhase::Downloading && v.downloaded_bytes > 0.0,
        20,
    )
    .await;
    let cfg = rig.sup.client().config().await.unwrap();
    assert_eq!(cfg["offline"], false, "online while it downloads");

    rig.setup.pause().await.unwrap();
    let paused = wait_view(&mut rx, |v| v.phase == SetupPhase::Paused, 5).await;
    assert_eq!(paused.paused, Some(PauseReason::Person));
    wait_offline(&rig.sup).await;
    let frozen = rx.borrow().downloaded_bytes;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        rx.borrow().downloaded_bytes,
        frozen,
        "no progress while paused"
    );
    assert_eq!(rx.borrow().phase, SetupPhase::Paused);

    // The engine reads its mode from disk when it starts again.
    common::set_mode(&rig.dirs, "download-ms=800");
    rig.setup.resume().await.unwrap();
    wait_view(
        &mut rx,
        |v| v.phase == SetupPhase::Downloading && v.paused.is_none(),
        10,
    )
    .await;
    let cfg = rig.sup.client().config().await.unwrap();
    assert_eq!(cfg["offline"], false, "online again after resuming");
    let done = wait_view(&mut rx, |v| v.phase == SetupPhase::Done, 40).await;
    assert!(done.paused.is_none());
    wait_offline(&rig.sup).await;
    rig.sup.shutdown().await.unwrap();
}

#[tokio::test]
async fn a_pause_that_lasts_does_not_lose_the_setup() {
    let rig = rig("download-ms=1500", 32).await;
    let mut rx = rig.setup.view();
    rig.setup.start(Tier::Small).await.unwrap();
    wait_view(
        &mut rx,
        |v| v.phase == SetupPhase::Downloading && v.downloaded_bytes > 0.0,
        20,
    )
    .await;
    rig.setup.pause().await.unwrap();
    rig.setup.resume().await.unwrap();
    common::set_mode(&rig.dirs, "download-ms=100");
    wait_view(&mut rx, |v| v.phase == SetupPhase::Done, 60).await;
    rig.sup.shutdown().await.unwrap();
}

#[tokio::test]
async fn a_stalled_download_reads_as_offline_and_recovers_when_bytes_move() {
    let rig = rig("download-ms=300,no-progress", 16).await;
    let mut rx = rig.setup.view();
    rig.setup.start(Tier::Small).await.unwrap();
    let stalled = wait_view(&mut rx, |v| v.paused == Some(PauseReason::Offline), 20).await;
    assert_eq!(stalled.phase, SetupPhase::Paused);
    assert!(stalled.error.is_none());
    raw(
        &rig.sup,
        "POST",
        "/fake/mode",
        json!({"mode": "download-ms=300"}),
    )
    .await;
    wait_view(
        &mut rx,
        |v| v.phase == SetupPhase::Downloading && v.paused.is_none(),
        10,
    )
    .await;
    wait_view(&mut rx, |v| v.phase == SetupPhase::Done, 40).await;
    rig.sup.shutdown().await.unwrap();
}

#[tokio::test]
async fn a_full_disk_stops_with_how_much_more_is_needed() {
    let rig = rig("download-ms=300,disk-full", 16).await;
    let mut rx = rig.setup.view();
    rig.setup.start(Tier::Small).await.unwrap();
    let failed = wait_view(&mut rx, |v| v.phase == SetupPhase::Failed, 20).await;
    assert_eq!(failed.error, Some(SetupError::DiskFull));
    assert!(failed.paused.is_none());
    // The engine said "requires 2.4 GB but only 0.3 GB": 2.1 GiB short, plus the 2 GiB margin.
    let expected = (2.1 + 2.0) * GIB as f64;
    assert!(
        (failed.needed_bytes.unwrap() - expected).abs() < 1e6,
        "{:?}",
        failed.needed_bytes
    );
    assert_eq!(rig.setup.installed(), None);
    wait_offline(&rig.sup).await;
    rig.sup.shutdown().await.unwrap();
}
#[tokio::test]
async fn not_enough_free_space_is_reported_before_anything_downloads() {
    let free = tier::download_bytes(Tier::Small) - GIB;
    let rig = rig(&format!("free-bytes={free}"), 16).await;
    let checked = rig.setup.check().await.unwrap();
    assert_eq!(checked.error, Some(SetupError::DiskFull));
    assert_eq!(checked.detected, Some(Tier::Unsupported));
    assert!(checked.options.iter().all(|o| !o.fits));
    rig.setup.start(Tier::Small).await.unwrap();
    let view = rig.setup.view().borrow().clone();
    assert_eq!(view.phase, SetupPhase::Failed);
    assert_eq!(view.error, Some(SetupError::DiskFull));
    let backends = 3 * 150_000_000;
    let expected = (tier::download_bytes(Tier::Small) + backends + DISK_HEADROOM - free) as f64;
    assert_eq!(view.needed_bytes, Some(expected));
    assert!(requests_to(&rig.sup, "/v1/pull").await.is_empty());
    assert!(requests_to(&rig.sup, "/v1/install").await.is_empty());
    rig.sup.shutdown().await.unwrap();
}

#[tokio::test]
async fn too_little_memory_is_reported() {
    let rig = rig("", 8).await;
    let checked = rig.setup.check().await.unwrap();
    assert_eq!(checked.detected, Some(Tier::Unsupported));
    assert_eq!(checked.error, Some(SetupError::NotEnoughMemory));
    rig.setup.start(Tier::Standard).await.unwrap();
    let view = rig.setup.view().borrow().clone();
    assert_eq!(view.phase, SetupPhase::Failed);
    assert_eq!(view.error, Some(SetupError::NotEnoughMemory));
    rig.sup.shutdown().await.unwrap();
}

#[tokio::test]
async fn a_restarted_engine_resumes_the_partial_download() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    common::set_mode(&dirs, "download-ms=3000");
    let ledger: Ledger = Arc::default();

    let sup = Supervisor::start(dirs.clone(), sup_opts()).await.unwrap();
    let setup = new_setup(&sup, &dirs, 16, &ledger);
    let mut rx = setup.view();
    setup.start(Tier::Small).await.unwrap();
    wait_view(
        &mut rx,
        |v| v.phase == SetupPhase::Downloading && v.downloaded_bytes > 20_000.0,
        20,
    )
    .await;
    drop(setup);
    sup.shutdown().await.unwrap();
    let state: Value =
        serde_json::from_slice(&std::fs::read(dirs.cache().join("fake-state.json")).unwrap())
            .unwrap();
    let kept: u64 = state["partial"]
        .as_object()
        .unwrap()
        .values()
        .map(|v| v.as_u64().unwrap())
        .sum();
    assert!(kept > 0, "{state}");

    common::set_mode(&dirs, "download-ms=200");
    let sup = Supervisor::start(dirs.clone(), sup_opts()).await.unwrap();
    let setup = new_setup(&sup, &dirs, 16, &ledger);
    let checked = setup.check().await.unwrap();
    assert_eq!(checked.phase, SetupPhase::Paused);
    assert_eq!(checked.chosen, Some(Tier::Small));
    let mut rx = setup.view();
    setup.resume().await.unwrap();
    wait_view(&mut rx, |v| v.phase == SetupPhase::Downloading, 10).await;
    let jobs = loop {
        let jobs = raw(&sup, "GET", "/v1/downloads", json!({})).await;
        if !jobs.as_array().unwrap().is_empty() {
            break jobs;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    };
    let resumed = jobs[0]["bytes_downloaded"].as_u64().unwrap();
    assert!(
        resumed >= kept.min(20_000),
        "restarted from {resumed}, kept {kept}"
    );
    wait_view(&mut rx, |v| v.phase == SetupPhase::Done, 60).await;
    sup.shutdown().await.unwrap();
}

#[tokio::test]
async fn a_dropped_setup_leaves_the_engine_offline_and_a_new_one_carries_on() {
    let rig = rig("download-ms=1500", 16).await;
    rig.setup.start(Tier::Small).await.unwrap();
    let mut rx = rig.setup.view();
    wait_view(
        &mut rx,
        |v| v.phase == SetupPhase::Downloading && v.downloaded_bytes > 0.0,
        20,
    )
    .await;
    assert_eq!(rig.sup.client().config().await.unwrap()["offline"], false);
    drop(rig.setup);
    wait_offline(&rig.sup).await;

    let second = new_setup(&rig.sup, &rig.dirs, 16, &rig.ledger);
    let checked = second.check().await.unwrap();
    assert_eq!(checked.phase, SetupPhase::Paused);
    let mut rx = second.view();
    second.resume().await.unwrap();
    raw(
        &rig.sup,
        "POST",
        "/fake/mode",
        json!({"mode": "download-ms=100"}),
    )
    .await;
    wait_view(&mut rx, |v| v.phase == SetupPhase::Done, 60).await;
    wait_offline(&rig.sup).await;
    rig.sup.shutdown().await.unwrap();
}
#[cfg(unix)]
#[tokio::test]
async fn setup_rides_out_an_engine_crash() {
    let rig = rig("download-ms=700", 16).await;
    let mut rx = rig.setup.view();
    rig.setup.start(Tier::Small).await.unwrap();
    wait_view(
        &mut rx,
        |v| v.phase == SetupPhase::Downloading && v.downloaded_bytes > 0.0,
        20,
    )
    .await;
    let pid = rig.sup.pid().unwrap();
    let _ = std::process::Command::new("kill")
        .args(["-9", &pid.to_string()])
        .status();
    wait_view(&mut rx, |v| v.phase == SetupPhase::Done, 60).await;
    assert!(rig.sup.restarts() >= 1);
    assert_eq!(rig.setup.installed(), Some(Tier::Small));
    wait_offline(&rig.sup).await;
    rig.sup.shutdown().await.unwrap();
}

#[tokio::test]
async fn an_unfinished_change_of_tier_keeps_the_tier_that_works() {
    let rig = rig("download-ms=100", 64).await;
    let mut rx = rig.setup.view();
    rig.setup.start(Tier::Small).await.unwrap();
    wait_view(&mut rx, |v| v.phase == SetupPhase::Done, 60).await;
    assert_eq!(rig.setup.installed(), Some(Tier::Small));

    // The change to a bigger tier starts and is cut short.
    common::set_mode(&rig.dirs, "download-ms=4000");
    rig.setup.start(Tier::Standard).await.unwrap();
    wait_view(
        &mut rx,
        |v| v.phase == SetupPhase::Downloading && v.chosen == Some(Tier::Standard),
        20,
    )
    .await;
    let mut record = nocturne_lemonade::record::read(&rig.dirs.config()).unwrap();
    for _ in 0..100 {
        if record.tier == Some(Tier::Standard) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
        record = nocturne_lemonade::record::read(&rig.dirs.config()).unwrap();
    }
    assert_eq!(record.tier, Some(Tier::Standard));
    assert!(!record.done);
    assert_eq!(record.completed, Some(Tier::Small));
    assert_eq!(
        rig.setup.installed(),
        Some(Tier::Small),
        "still the working one"
    );
    assert_eq!(rig.setup.installed_ready().await, Some(Tier::Small));

    // A later launch finds the working tier, not a paused setup.
    let later = new_setup(&rig.sup, &rig.dirs, 64, &rig.ledger);
    let view = later.check().await.unwrap();
    assert_eq!(view.phase, SetupPhase::Done);
    assert_eq!(view.chosen, Some(Tier::Small));
    rig.sup.shutdown().await.unwrap();
}

#[tokio::test]
async fn setup_for_another_tier_while_one_runs_is_refused() {
    let rig = rig("download-ms=3000", 64).await;
    let mut rx = rig.setup.view();
    rig.setup.start(Tier::Small).await.unwrap();
    wait_view(&mut rx, |v| v.phase == SetupPhase::Downloading, 20).await;
    assert_eq!(
        rig.setup.start(Tier::Standard).await,
        Err(nocturne_lemonade::RuntimeError::Busy)
    );
    assert_eq!(
        rig.setup.start(Tier::Small).await,
        Ok(()),
        "asking again for the same tier is fine"
    );
    rig.sup.shutdown().await.unwrap();
}
