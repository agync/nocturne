//! Setup against the fake engine: downloads, pauses, a dropped network, a full disk and restarts.

mod common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use nocturne_contract::{LedgerKind, PauseReason, SetupError, SetupPhase, SetupView, Tier};
use nocturne_lemonade::npu;
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

/// Setup's look at what is installed, failing after twenty seconds.
async fn check(setup: &Setup) -> SetupView {
    tokio::time::timeout(Duration::from_secs(20), setup.check())
        .await
        .expect("setup looked at what is installed within twenty seconds")
        .unwrap_or_else(|e| panic!("setup's look at what is installed failed: {e:?}"))
}

/// Setup's answer on whether a tier is installed, failing after twenty seconds.
async fn installed_ready(setup: &Setup) -> Option<Tier> {
    tokio::time::timeout(Duration::from_secs(20), setup.installed_ready())
        .await
        .expect("setup said within twenty seconds whether a tier is installed")
}

async fn raw(sup: &Supervisor, method: &str, path: &str, body: Value) -> Value {
    let http = nocturne_lemonade::tls::client();
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
        if let Ok(cfg) = sup.admin().config().await
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
        let admin = rig.sup.admin();
        tokio::spawn(async move {
            while rx.changed().await.is_ok() {
                let (phase, bytes) = {
                    let v = rx.borrow_and_update();
                    (v.phase, v.downloaded_bytes)
                };
                if phase == SetupPhase::Downloading
                    && bytes > 0.0
                    && let Ok(cfg) = admin.config().await
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

    let checked = check(&rig.setup).await;
    assert_eq!(checked.phase, SetupPhase::Choosing);
    assert_eq!(checked.detected, Some(Tier::Standard));
    assert_eq!(checked.ram_gib, 32);
    assert_eq!(checked.options.len(), 4);
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
    assert_eq!(rig.sup.admin().config().await.unwrap()["offline"], true);

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
    let view = check(&again).await;
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
    let cfg = rig.sup.admin().config().await.unwrap();
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
    let cfg = rig.sup.admin().config().await.unwrap();
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
    let checked = check(&rig.setup).await;
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
    let rig = rig("", 5).await;
    let checked = check(&rig.setup).await;
    assert_eq!(checked.detected, Some(Tier::Unsupported));
    assert_eq!(checked.error, Some(SetupError::NotEnoughMemory));
    rig.setup.start(Tier::Tiny).await.unwrap();
    let view = rig.setup.view().borrow().clone();
    assert_eq!(view.phase, SetupPhase::Failed);
    assert_eq!(view.error, Some(SetupError::NotEnoughMemory));
    rig.sup.shutdown().await.unwrap();
}

#[tokio::test]
async fn an_8_gb_computer_is_offered_the_tiny_tier() {
    let rig = rig("", 8).await;
    let checked = check(&rig.setup).await;
    assert_eq!(checked.detected, Some(Tier::Tiny));
    assert_eq!(checked.error, None);
    let fitting: Vec<Tier> = checked
        .options
        .iter()
        .filter(|o| o.fits)
        .map(|o| o.tier)
        .collect();
    assert_eq!(fitting, [Tier::Tiny]);
    rig.sup.shutdown().await.unwrap();
}

#[tokio::test]
async fn a_restarted_engine_resumes_the_partial_download() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    // Each download takes half a minute, so the stop below always lands in the middle of one: a
    // download that finished between the look and the stop would leave nothing cut short.
    common::set_mode(&dirs, "download-ms=30000");
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
    let kept: Vec<(String, u64)> = state["partial"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(name, bytes)| (name.clone(), bytes.as_u64().unwrap()))
        .filter(|(_, bytes)| *bytes > 0)
        .collect();
    assert!(!kept.is_empty(), "{state}");

    // Slow at first, so the resumed download is still in the engine's list when it is looked
    // at: a download that finished first leaves the list, and nothing would be found.
    common::set_mode(&dirs, "download-ms=30000");
    let sup = Supervisor::start(dirs.clone(), sup_opts()).await.unwrap();
    let setup = new_setup(&sup, &dirs, 16, &ledger);
    let checked = check(&setup).await;
    assert_eq!(checked.phase, SetupPhase::Paused);
    assert_eq!(checked.chosen, Some(Tier::Small));
    let mut rx = setup.view();
    setup.resume().await.unwrap();
    wait_view(&mut rx, |v| v.phase == SetupPhase::Downloading, 10).await;
    // The download that was cut short, as the engine lists it once it is back.
    let mut listed = json!([]);
    let (name, kept_bytes, resumed) = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let jobs = raw(&sup, "GET", "/v1/downloads", json!({})).await;
            listed = jobs.clone();
            let found = jobs.as_array().unwrap().iter().find_map(|job| {
                let name = job["model_name"].as_str()?;
                let (_, kept_bytes) = kept.iter().find(|(k, _)| k == name)?;
                Some((
                    name.to_string(),
                    *kept_bytes,
                    job["bytes_downloaded"].as_u64()?,
                ))
            });
            if let Some(found) = found {
                return found;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| {
        panic!("no download of {kept:?} in the engine's list, which is {listed}");
    });
    assert!(
        resumed >= kept_bytes,
        "{name} restarted from {resumed}, though {kept_bytes} bytes were kept"
    );
    raw(
        &sup,
        "POST",
        "/fake/mode",
        json!({"mode": "download-ms=200"}),
    )
    .await;
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
    assert_eq!(rig.sup.admin().config().await.unwrap()["offline"], false);
    drop(rig.setup);
    wait_offline(&rig.sup).await;

    let second = new_setup(&rig.sup, &rig.dirs, 16, &rig.ledger);
    let checked = check(&second).await;
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

/// Before the first check, the view already says whether a size was ever chosen, so the window
/// can tell a first run from a later one while the engine is still starting.
#[tokio::test]
async fn the_first_view_carries_the_size_chosen_before() {
    let rig = rig("", 16).await;
    assert_eq!(rig.setup.view().borrow().chosen, None);
    let record = nocturne_lemonade::record::SetupRecord {
        version: 1,
        tier: Some(Tier::Small),
        done: true,
        completed: Some(Tier::Small),
        ..Default::default()
    };
    nocturne_lemonade::record::write(&rig.dirs.config(), &record).unwrap();
    let again = new_setup(&rig.sup, &rig.dirs, 16, &rig.ledger);
    let first = again.view().borrow().clone();
    assert_eq!(
        (first.phase, first.chosen),
        (SetupPhase::NotStarted, Some(Tier::Small))
    );
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
    assert_eq!(installed_ready(&rig.setup).await, Some(Tier::Small));

    // A later launch finds the working tier, not a paused setup.
    let later = new_setup(&rig.sup, &rig.dirs, 64, &rig.ledger);
    let view = check(&later).await;
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

#[tokio::test]
async fn two_starts_for_different_tiers_at_once_let_one_run_and_refuse_the_other() {
    let rig = rig("download-ms=1500", 64).await;
    let (small, standard) = tokio::join!(
        rig.setup.start(Tier::Small),
        rig.setup.start(Tier::Standard)
    );
    let refused = [&small, &standard]
        .into_iter()
        .filter(|r| **r == Err(nocturne_lemonade::RuntimeError::Busy))
        .count();
    assert_eq!(
        refused, 1,
        "one is refused, not silently accepted: {small:?} {standard:?}"
    );
    assert_eq!(
        [&small, &standard]
            .into_iter()
            .filter(|r| r.is_ok())
            .count(),
        1
    );
    rig.sup.shutdown().await.unwrap();
}

#[tokio::test]
async fn an_unfinished_change_of_tier_is_dropped_so_resume_does_not_aim_at_it() {
    let rig = rig("downloaded=all", 64).await;
    nocturne_lemonade::record::write(
        &rig.dirs.config(),
        &nocturne_lemonade::record::SetupRecord {
            version: 1,
            tier: Some(Tier::Standard),
            done: false,
            completed: Some(Tier::Small),
            backends: Default::default(),
        },
    )
    .unwrap();
    let view = check(&rig.setup).await;
    assert_eq!(view.phase, SetupPhase::Done);
    assert_eq!(view.chosen, Some(Tier::Small));
    let record = nocturne_lemonade::record::read(&rig.dirs.config()).unwrap();
    assert_eq!(record.tier, Some(Tier::Small), "nothing is left pending");
    assert!(record.done);
    assert_eq!(record.completed, Some(Tier::Small));
    assert_eq!(
        rig.setup.take_dropped(),
        Some(Tier::Standard),
        "what was dropped is handed over so the person can be told"
    );
    assert_eq!(rig.setup.take_dropped(), None, "and only once");
    rig.sup.shutdown().await.unwrap();
}

#[tokio::test]
async fn a_download_that_stops_on_its_own_is_cleared_and_started_again_through_a_conflict() {
    let rig = rig(
        "download-ms=400,pause-itself-ms=100,conflict-first-remove",
        32,
    )
    .await;
    let rx = rig.setup.view();
    rig.setup.start(Tier::Small).await.unwrap();
    // The engine forgets its log when it restarts at the end, so watch while it runs.
    let mut calls: Vec<String> = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    while rx.borrow().phase != SetupPhase::Done {
        assert!(
            tokio::time::Instant::now() < deadline,
            "setup did not finish: {:?} {calls:?}",
            rx.borrow().phase
        );
        let log = raw(&rig.sup, "GET", "/fake/requests", json!({})).await;
        let seen: Vec<String> = log
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|r| match r["path"].as_str()? {
                "/v1/pull" | "/v1/install" => Some("start".to_string()),
                "/v1/downloads/control" => Some(r["body"]["action"].as_str()?.to_string()),
                _ => None,
            })
            .collect();
        if seen.len() > calls.len() {
            calls = seen;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    // The first download stopped by itself. The refused removal is repeated before the next start.
    let first_start = calls.iter().position(|c| c == "start").unwrap();
    let second_start = calls[first_start + 1..]
        .iter()
        .position(|c| c == "start")
        .map(|i| i + first_start + 1)
        .expect("the download was started again");
    let removals = calls[first_start..second_start]
        .iter()
        .filter(|c| *c == "remove")
        .count();
    assert_eq!(removals, 2, "refused, then done: {calls:?}");
    rig.sup.shutdown().await.unwrap();
}

#[tokio::test]
async fn a_new_standard_setup_fetches_the_reranker_and_counts_its_size() {
    let rig = rig("download-ms=100", 32).await;
    let mut rx = rig.setup.view();
    let offered = check(&rig.setup).await;
    let standard = offered
        .options
        .iter()
        .find(|o| o.tier == Tier::Standard)
        .unwrap();
    assert_eq!(
        standard.download_bytes,
        tier::download_bytes(Tier::Standard) as f64
    );
    let reranker = tier::reranker(Tier::Standard).unwrap();
    let size = tier::extras(Tier::Standard)[0].bytes;
    let components: u64 = tier::components(Tier::Standard)
        .iter()
        .map(|c| c.bytes)
        .sum();
    assert_eq!(tier::download_bytes(Tier::Standard), components + size);

    rig.setup.start(Tier::Standard).await.unwrap();
    let mid = wait_view(
        &mut rx,
        |v| v.phase == SetupPhase::Downloading && v.total_bytes > 0.0,
        30,
    )
    .await;
    assert_eq!(mid.total_bytes, (components + size) as f64);
    let done = wait_view(&mut rx, |v| v.phase == SetupPhase::Done, 60).await;
    assert_eq!(done.total_bytes, (components + size) as f64);

    let ledger = rig.ledger.lock().unwrap().clone();
    assert!(
        ledger.iter().any(|(kind, label, bytes)| label == reranker
            && *kind == LedgerKind::Setup
            && *bytes > 0),
        "{ledger:?}"
    );
    let models = rig.sup.client().models().await.unwrap();
    assert!(models.iter().any(|m| m.id == reranker && m.present()));
    rig.sup.shutdown().await.unwrap();
}

#[tokio::test]
async fn a_standard_install_from_before_the_reranker_is_still_installed() {
    let rig = rig("download-ms=100", 32).await;
    let mut rx = rig.setup.view();
    rig.setup.start(Tier::Standard).await.unwrap();
    wait_view(&mut rx, |v| v.phase == SetupPhase::Done, 60).await;
    // As an install made before the reranker existed: everything but it on disk.
    let reranker = tier::reranker(Tier::Standard).unwrap();
    rig.sup.client().delete(reranker).await.unwrap();
    let models = rig.sup.client().models().await.unwrap();
    assert!(!models.iter().any(|m| m.id == reranker && m.present()));
    let pulls_before = requests_to(&rig.sup, "/v1/pull").await.len();

    let later = new_setup(&rig.sup, &rig.dirs, 32, &rig.ledger);
    let view = check(&later).await;
    assert_eq!(view.phase, SetupPhase::Done);
    assert_eq!(view.chosen, Some(Tier::Standard));
    assert_eq!(later.installed(), Some(Tier::Standard));
    assert_eq!(installed_ready(&later).await, Some(Tier::Standard));
    let pulls_after = requests_to(&rig.sup, "/v1/pull").await.len();
    assert_eq!(pulls_after, pulls_before, "checking fetches nothing");
    assert_eq!(rig.sup.admin().config().await.unwrap()["offline"], true);
    rig.sup.shutdown().await.unwrap();
}

/// What setup fetched, by the names it gave the ledger: `recipe:backend` or the model.
fn fetched(ledger: &Ledger) -> Vec<String> {
    ledger
        .lock()
        .unwrap()
        .iter()
        .map(|(_, label, _)| label.clone())
        .collect()
}

const NPU_RUNTIME: &str = "flm:npu";

#[tokio::test]
async fn a_usable_npu_gets_its_runtime_and_model_and_counts_their_size() {
    let rig = rig("download-ms=100,npu=ready", 16).await;
    let mut rx = rig.setup.view();
    rig.setup.start(Tier::Small).await.unwrap();
    let done = wait_view(&mut rx, |v| v.phase == SetupPhase::Done, 60).await;
    assert_eq!(
        done.total_bytes,
        (tier::download_bytes(Tier::Small) + npu::SPEECH.bytes) as f64
    );
    let got = fetched(&rig.ledger);
    assert!(got.iter().any(|l| l == NPU_RUNTIME), "{got:?}");
    assert!(got.iter().any(|l| l == npu::SPEECH.model), "{got:?}");
    let info = rig.sup.client().system_info().await.unwrap();
    assert_eq!(npu::readiness(&info, false), npu::Readiness::Ready);
    let models = rig.sup.client().models().await.unwrap();
    assert!(
        models
            .iter()
            .any(|m| m.id == npu::SPEECH.model && m.present())
    );
    rig.sup.shutdown().await.unwrap();
}

#[tokio::test]
async fn the_npus_mind_is_fetched_only_where_it_will_work() {
    let reader = npu::MIND.model.to_string();
    for (ram, tier, wanted) in [
        (48, Tier::Standard, true),
        (32, Tier::Standard, true),
        (16, Tier::Small, false),
        (64, Tier::Small, true),
        (64, Tier::Large, false),
    ] {
        let rig = rig("download-ms=50,npu=ready", ram).await;
        let mut rx = rig.setup.view();
        rig.setup.start(tier).await.unwrap();
        let done = wait_view(&mut rx, |v| v.phase == SetupPhase::Done, 60).await;
        let extra = if wanted { npu::MIND.bytes } else { 0 };
        assert_eq!(
            done.total_bytes,
            (tier::download_bytes(tier) + npu::SPEECH.bytes + extra) as f64,
            "{ram} GB {tier:?}"
        );
        assert_eq!(
            fetched(&rig.ledger).contains(&reader),
            wanted,
            "{ram} GB {tier:?}"
        );
        rig.sup.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn a_disk_without_room_for_the_reader_still_gets_the_tier() {
    let backends = 3 * 150_000_000 + npu::RUNTIME_BYTES;
    let free =
        tier::download_bytes(Tier::Standard) + npu::SPEECH.bytes + backends + DISK_HEADROOM + GIB;
    let rig = rig(&format!("download-ms=50,npu=ready,free-bytes={free}"), 48).await;
    let mut rx = rig.setup.view();
    rig.setup.start(Tier::Standard).await.unwrap();
    let done = wait_view(&mut rx, |v| v.phase == SetupPhase::Done, 60).await;
    assert_eq!(
        done.total_bytes,
        (tier::download_bytes(Tier::Standard) + npu::SPEECH.bytes) as f64
    );
    let got = fetched(&rig.ledger);
    assert!(!got.iter().any(|l| l == npu::MIND.model), "{got:?}");
    assert!(got.iter().any(|l| l == npu::SPEECH.model), "{got:?}");
    rig.sup.shutdown().await.unwrap();
}

#[tokio::test]
async fn nothing_for_the_npu_is_fetched_when_it_will_not_be_used() {
    // No NPU, an NPU the engine cannot use, and an 8 GB computer where the model does not fit.
    for (mode, ram, tier) in [
        ("download-ms=50", 32, Tier::Standard),
        ("download-ms=50,npu=unsupported", 32, Tier::Standard),
        ("download-ms=50,npu=driver-missing", 32, Tier::Standard),
        ("download-ms=50,npu=ready", 8, Tier::Tiny),
    ] {
        let rig = rig(mode, ram).await;
        let mut rx = rig.setup.view();
        rig.setup.start(tier).await.unwrap();
        let done = wait_view(&mut rx, |v| v.phase == SetupPhase::Done, 60).await;
        assert_eq!(
            done.total_bytes,
            tier::download_bytes(tier) as f64,
            "{mode}"
        );
        let got = fetched(&rig.ledger);
        assert!(!got.iter().any(|l| l == NPU_RUNTIME), "{mode}: {got:?}");
        assert!(
            !got.iter().any(|l| l == npu::SPEECH.model),
            "{mode}: {got:?}"
        );
        rig.sup.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn a_chosen_speech_model_keeps_the_npu_share_away() {
    // Set up without an NPU, choose a speech model, then set up again once the NPU is usable.
    let rig = rig("download-ms=50", 16).await;
    let mut rx = rig.setup.view();
    rig.setup.start(Tier::Small).await.unwrap();
    wait_view(&mut rx, |v| v.phase == SetupPhase::Done, 60).await;
    nocturne_lemonade::selection::update(&rig.dirs.config(), |s| {
        s.custom.insert(
            nocturne_contract::ComponentRole::Speech,
            "Whisper-Large-v3".to_string(),
        );
    })
    .unwrap();
    common::set_mode(&rig.dirs, "download-ms=50,npu=ready");
    raw(
        &rig.sup,
        "POST",
        "/fake/mode",
        json!({"mode": "download-ms=50,npu=ready"}),
    )
    .await;
    rig.ledger.lock().unwrap().clear();
    let again = new_setup(&rig.sup, &rig.dirs, 16, &rig.ledger);
    let mut rx = again.view();
    again.start(Tier::Small).await.unwrap();
    wait_view(&mut rx, |v| v.phase == SetupPhase::Done, 60).await;
    let info = rig.sup.client().system_info().await.unwrap();
    assert_eq!(npu::readiness(&info, false), npu::Readiness::Fetch);
    let got = fetched(&rig.ledger);
    assert!(!got.iter().any(|l| l == NPU_RUNTIME), "{got:?}");
    assert!(!got.iter().any(|l| l == npu::SPEECH.model), "{got:?}");
    rig.sup.shutdown().await.unwrap();
}

#[tokio::test]
async fn setup_records_the_backend_each_kind_of_graphics_runs_on() {
    // The fake engine lists graphics as the real one does: an NVIDIA card (with CUDA offered, and
    // not wanted), and nothing at all for an Intel GPU, which the app reads from the computer.
    for (mode, llamacpp, whispercpp, kokoro) in [
        ("download-ms=50,gpu=nvidia", "vulkan", "vulkan", "cpu"),
        ("download-ms=50,gpu=nvidia-small", "vulkan", "vulkan", "cpu"),
        ("download-ms=50,gpu=discrete", "rocm", "rocm", "cpu"),
        ("download-ms=50,gpu=none", "cpu", "cpu", "cpu"),
        ("download-ms=50,gpu=intel", "cpu", "cpu", "cpu"),
    ] {
        let rig = rig(mode, 32).await;
        let mut rx = rig.setup.view();
        rig.setup.start(Tier::Small).await.unwrap();
        wait_view(&mut rx, |v| v.phase == SetupPhase::Done, 60).await;
        let record: Value = serde_json::from_slice(
            &std::fs::read(rig.dirs.config().join("nocturne-setup.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(record["backends"]["llamacpp"], llamacpp, "{mode}");
        assert_eq!(record["backends"]["whispercpp"], whispercpp, "{mode}");
        assert_eq!(record["backends"]["kokoro"], kokoro, "{mode}");
        let got = fetched(&rig.ledger);
        assert!(!got.iter().any(|l| l.ends_with(":cuda")), "{mode}: {got:?}");
        rig.sup.shutdown().await.unwrap();
    }
}
