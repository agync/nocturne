//! The private engine inside the app, against the fake engine the lemonade crate ships for its
//! own tests: launch, setup, the mind in the slot, failure, pause and quit. No real engine, no
//! network, nothing outside the temporary folders.

mod common;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use common::*;
use nocturne_app_lib::runtime::{self, Services, StartOptions};
use nocturne_contract::{ItemStatus, Tier};
use nocturne_engine::FakeMind;
use nocturne_intelligence::FakeHost;
use nocturne_lemonade::record::{self, SetupRecord};
use nocturne_lemonade::{Dirs, SetupOptions, SupervisorOptions};
use serde_json::{Value, json};
use tauri::async_runtime::block_on;

const GIB: u64 = 1 << 30;

/// The fake engine binary, brought up to date once per test run. A test run of this package alone
/// does not build another package's binaries, so one left by an earlier checkout would otherwise
/// be used as it is. `NOCTURNE_FAKE_LEMOND` names a prebuilt one instead, for a layout this does
/// not follow.
fn fake_engine() -> PathBuf {
    static BUILT: OnceLock<PathBuf> = OnceLock::new();
    BUILT.get_or_init(build_fake_engine).clone()
}

fn build_fake_engine() -> PathBuf {
    if let Some(path) = std::env::var_os("NOCTURNE_FAKE_LEMOND") {
        let path = PathBuf::from(path);
        assert!(path.is_file(), "NOCTURNE_FAKE_LEMOND is a file");
        return path;
    }
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let mut build = std::process::Command::new(cargo);
    build
        .args(["build", "--locked", "-p", "nocturne-lemonade"])
        .args(["--features", "fake-server", "--bin", "fake-lemond"])
        .args([
            "--profile",
            if cfg!(debug_assertions) {
                "dev"
            } else {
                "release"
            },
        ])
        .arg("--message-format=json-render-diagnostics");
    let out = build
        .stderr(std::process::Stdio::inherit())
        .output()
        .expect("cargo runs");
    assert!(out.status.success(), "the fake engine is built");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|m| m["reason"] == "compiler-artifact" && m["target"]["name"] == "fake-lemond")
        .find_map(|m| m["executable"].as_str().map(PathBuf::from))
        .expect("cargo says where the fake engine is")
}

struct Lab {
    /// Memory the machine reports as free, which a test may change.
    free: Arc<AtomicU64>,
    h: Harness,
    /// Every status the interface was told of, from before the engine started.
    statuses: Arc<std::sync::Mutex<Vec<Value>>>,
    services: Arc<Services>,
    _bundle: tempfile::TempDir,
}

/// What a lab starts from.
struct Plan {
    mode: &'static str,
    /// A finished setup for this tier is already on disk.
    installed: Option<Tier>,
    /// A change to this tier was started and never finished.
    unfinished: Option<Tier>,
    restart_limit: u32,
    retry_after: Duration,
    /// Memory the machine reports as free.
    free_memory: u64,
}

impl Plan {
    fn new(mode: &'static str) -> Plan {
        Plan {
            mode,
            installed: None,
            unfinished: None,
            restart_limit: 3,
            retry_after: Duration::from_secs(3600),
            free_memory: 64 * GIB,
        }
    }

    fn installed(mut self, tier: Tier) -> Plan {
        self.installed = Some(tier);
        self
    }
}

fn set_mode(root: &Path, mode: &str) {
    let path = root.join("config/config.json");
    let mut v: Value = std::fs::read(&path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_else(|| json!({}));
    v["fake_mode"] = json!(mode);
    std::fs::write(path, v.to_string()).unwrap();
}

fn lab(plan: Plan) -> Lab {
    let h = harness();
    let bundle = tempfile::tempdir().unwrap();
    let link = bundle.path().join(if cfg!(windows) {
        "lemond.exe"
    } else {
        "lemond"
    });
    #[cfg(unix)]
    std::os::unix::fs::symlink(fake_engine(), &link).unwrap();
    #[cfg(not(unix))]
    std::fs::copy(fake_engine(), &link).unwrap();

    let root = h.dir.path().join("engine");
    let dirs = Dirs::new(bundle.path(), &root);
    dirs.ensure().unwrap();
    set_mode(&root, plan.mode);
    if let Some(tier) = plan.installed {
        record::write(
            &dirs.config(),
            &SetupRecord {
                version: 1,
                tier: Some(plan.unfinished.unwrap_or(tier)),
                done: plan.unfinished.is_none(),
                completed: Some(tier),
                backends: Default::default(),
            },
        )
        .unwrap();
    }
    let mut opts = StartOptions::with_engine(FakeHost::new(), bundle.path());
    opts.supervisor = SupervisorOptions {
        restart_limit: plan.restart_limit,
        health_interval: Duration::from_millis(200),
        start_timeout: Duration::from_secs(20),
        backoff_base: Duration::from_millis(20),
    };
    opts.setup = SetupOptions {
        poll: Duration::from_millis(50),
        ram_bytes: Some(32 * GIB),
        ..SetupOptions::default()
    };
    opts.retry_after = plan.retry_after;
    let free = Arc::new(AtomicU64::new(plan.free_memory));
    let probe = free.clone();
    opts.available_memory = Some(Arc::new(move || probe.load(Ordering::SeqCst)));
    opts.memory_poll = Duration::from_millis(20);
    let statuses = h.events(nocturne_contract::EVENT_STATUS);
    let services = start(&h, opts);
    Lab {
        free,
        statuses,
        h,
        services,
        _bundle: bundle,
    }
}

impl Drop for Lab {
    /// Stops the engine even when a test fails half way.
    fn drop(&mut self) {
        self.h.state().run_exit_hooks();
    }
}

impl Lab {
    /// Kill the running fake engine now, as a crash would.
    fn crash(&self) {
        block_on(async {
            // Between crashes the engine is relaunching and has no address for a moment.
            let deadline = std::time::Instant::now() + Duration::from_secs(20);
            let endpoint = loop {
                if let Some(client) = self.services.client().await
                    && let Some(endpoint) = client.endpoint()
                {
                    break endpoint;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "the engine came back with an address"
                );
                tokio::time::sleep(Duration::from_millis(20)).await;
            };
            let _ = reqwest::Client::new()
                .post(format!("http://127.0.0.1:{}/fake/crash", endpoint.port))
                .bearer_auth(&endpoint.key)
                .send()
                .await;
        });
    }

    /// The engine's address changes at every launch and stop; true once a new one answers.
    fn engine_generation(&self) -> u64 {
        block_on(async { self.services.client().await.map_or(0, |c| c.generation()) })
    }

    fn engine_answers(&self) -> bool {
        block_on(async {
            match self.services.client().await {
                Some(client) => client.live().await.is_ok(),
                None => false,
            }
        })
    }

    /// How many times the fake engine has been started in this lab.
    fn launches(&self) -> u64 {
        let dirs = Dirs::new(self._bundle.path(), self.h.dir.path().join("engine"));
        std::fs::read_to_string(dirs.cache().join("fake-launches"))
            .ok()
            .and_then(|t| t.trim().parse().ok())
            .unwrap_or(0)
    }

    /// How many statuses the interface was told of that carried the engine's "keeps failing"
    /// notice. It only grows, so a test sees the engine give up even when a short retry timer has
    /// already started it again.
    fn gave_up_count(&self) -> usize {
        self.statuses
            .lock()
            .unwrap()
            .iter()
            .filter(|s| {
                s["notices"]
                    .as_array()
                    .is_some_and(|n| n.iter().any(|n| n["code"] == "restarts"))
            })
            .count()
    }

    fn has_mind(&self) -> bool {
        self.h.state().mind.get().is_some()
    }

    fn setup_phase(&self) -> String {
        self.h.ok("get_setup", json!({}))["phase"]
            .as_str()
            .unwrap()
            .to_string()
    }

    fn has_notice(&self, code: &str) -> bool {
        self.h.ok("get_status", json!({}))["notices"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n["code"] == code)
    }

    /// Whether the running engine is set to stay off the internet.
    fn engine_offline(&self) -> Option<bool> {
        block_on(async {
            let admin = self.services.admin().await?;
            admin.config().await.ok()?["offline"].as_bool()
        })
    }

    /// Change how the running fake engine behaves.
    fn fake_mode(&self, mode: &str) {
        block_on(async {
            let client = self.services.client().await.expect("an engine");
            let endpoint = client.endpoint().expect("an address");
            reqwest::Client::new()
                .post(format!("http://127.0.0.1:{}/fake/mode", endpoint.port))
                .bearer_auth(&endpoint.key)
                .json(&json!({ "mode": mode }))
                .send()
                .await
                .unwrap();
        });
    }

    /// What the fake engine has been asked, newest last.
    fn requests(&self) -> Vec<Value> {
        block_on(async {
            let client = self.services.client().await.expect("an engine");
            let endpoint = client.endpoint().expect("an address");
            reqwest::Client::new()
                .get(format!("http://127.0.0.1:{}/fake/requests", endpoint.port))
                .bearer_auth(&endpoint.key)
                .send()
                .await
                .unwrap()
                .json::<Vec<Value>>()
                .await
                .unwrap()
        })
    }
}

#[test]
fn launch_with_everything_on_disk_fills_the_slot() {
    let lab = lab(Plan::new("downloaded=all").installed(Tier::Small));
    wait_for("the mind to arrive", || lab.has_mind());
    wait_for("setup to read done", || lab.setup_phase() == "done");
    assert_eq!(lab.services.tier(), Some(Tier::Small));
    let status = lab.h.ok("get_status", json!({}));
    assert_ne!(status["state"], "gettingReady", "{status}");
    assert!(
        block_on(lab.services.engine_pid()).is_some(),
        "the engine runs"
    );
}

#[test]
fn a_fresh_install_offers_a_choice_and_setup_fills_the_slot() {
    let lab = lab(Plan::new(""));
    wait_for("the choice", || lab.setup_phase() == "choosing");
    let setup = lab.h.ok("get_setup", json!({}));
    assert!(!setup["options"].as_array().unwrap().is_empty(), "{setup}");
    assert!(!lab.has_mind(), "nothing to read with before setup");
    assert_eq!(lab.h.ok("get_status", json!({}))["state"], "gettingReady");

    lab.h.ok("start_setup", json!({"tier": "small"}));
    wait_for("setup to finish", || lab.setup_phase() == "done");
    wait_for("the mind to arrive", || lab.has_mind());
    assert_eq!(lab.services.tier(), Some(Tier::Small));

    let privacy = lab.h.ok("get_privacy", json!({}));
    assert!(
        !privacy["ledger"].as_array().unwrap().is_empty(),
        "downloads are written to the ledger: {privacy}"
    );
    assert!(lab.h.engine().ledger().unwrap().len() >= 4);
}

#[test]
fn setup_can_be_paused_and_resumed() {
    let lab = lab(Plan::new("download-ms=1500"));
    wait_for("the choice", || lab.setup_phase() == "choosing");
    lab.h.ok("start_setup", json!({"tier": "small"}));
    wait_for("bytes to arrive", || {
        let view = lab.h.ok("get_setup", json!({}));
        view["phase"] == "downloading" && view["downloadedBytes"].as_f64().unwrap_or(0.0) > 0.0
    });
    lab.h.ok("pause_setup", json!({}));
    wait_for("paused", || lab.setup_phase() == "paused");
    wait_for("the engine to go offline", || {
        lab.engine_offline() == Some(true)
    });
    // Pausing takes the engine offline by restarting it, which reads its mode from disk; a
    // resumed download starts over in the fake, so let it be quick.
    set_mode(&lab.h.dir.path().join("engine"), "download-ms=150");
    lab.h.ok("resume_setup", json!({}));
    wait_for("setup to finish after resuming", || {
        lab.setup_phase() == "done"
    });
    wait_for("the mind to arrive", || lab.has_mind());
}

/// Crash the engine until the supervisor gives up (it allows `restart_limit` restarts). Giving
/// up is read from the statuses the interface was told of, not from the empty slot, which a short
/// retry timer may refill before a loaded machine looks.
fn crash_until_it_gives_up(lab: &Lab) {
    let before_all = lab.gave_up_count();
    let gave_up = || lab.gave_up_count() > before_all;
    for _ in 0..4 {
        if gave_up() {
            return;
        }
        let before = lab.engine_generation();
        lab.crash();
        wait_for("the crash to be noticed", || {
            lab.engine_generation() != before || !lab.has_mind() || gave_up()
        });
        if gave_up() {
            return;
        }
        wait_for("the engine to answer again", || {
            lab.engine_answers() || gave_up()
        });
    }
    wait_for("the engine to give up", gave_up);
}

#[test]
fn an_engine_that_gives_up_empties_the_slot_and_questions_say_unavailable() {
    let lab = lab(Plan {
        restart_limit: 1,
        ..Plan::new("downloaded=all").installed(Tier::Small)
    });
    wait_for("the mind to arrive", || lab.has_mind());
    wait_for("setup to read done", || lab.setup_phase() == "done");
    crash_until_it_gives_up(&lab);
    wait_for("the notice", || lab.has_notice("restarts"));
    assert_eq!(lab.h.ok("get_status", json!({}))["state"], "gettingReady");
    assert_eq!(
        lab.setup_phase(),
        "failed",
        "setup is not done while there is nothing to read with"
    );

    let seen = lab.h.ask_deltas();
    let id = lab
        .h
        .ok("ask", json!({"question": "When is the dentist?"}))
        .as_str()
        .unwrap()
        .to_string();
    wait_for("the answer", || {
        deltas_for(&seen, &id).iter().any(|d| is_terminal(d.phase))
    });
    assert_eq!(
        deltas_for(&seen, &id).last().unwrap().phase,
        nocturne_contract::AskPhase::Unavailable
    );
}

#[test]
fn the_advanced_details_say_what_the_engine_and_the_tier_are() {
    let lab = lab(Plan::new("downloaded=all").installed(Tier::Small));
    wait_for("the mind to arrive", || lab.has_mind());
    let details = lab.h.ok("get_advanced", json!({}));
    assert_eq!(details["tier"], "small");
    assert_eq!(details["engineVersion"], "fake-1.0");
    let port = block_on(lab.services.client())
        .and_then(|c| c.endpoint())
        .map(|e| e.port);
    assert_eq!(details["enginePort"].as_u64().map(|p| p as u16), port);
    assert!(details["engineUptimeSeconds"].is_u64(), "{details}");
    assert_eq!(details["restarts"], 0);

    let before = lab.engine_generation();
    lab.crash();
    wait_for("the engine to come back", || {
        lab.engine_generation() != before && lab.engine_answers()
    });
    wait_for("the restart to be counted", || {
        lab.h.ok("get_advanced", json!({}))["restarts"] == 1
    });
}

#[test]
fn try_again_starts_the_engine_now_instead_of_waiting_for_the_timer() {
    let lab = lab(Plan {
        restart_limit: 1,
        ..Plan::new("downloaded=all").installed(Tier::Small)
    });
    wait_for("the mind to arrive", || lab.has_mind());
    crash_until_it_gives_up(&lab);
    wait_for("the notice", || lab.has_notice("restarts"));

    lab.h.ok("retry_engine", json!({}));
    wait_for("the mind to come back", || lab.has_mind());
    wait_for("the notice to go", || !lab.has_notice("restarts"));
    wait_for("setup to read done again", || lab.setup_phase() == "done");
}

#[test]
fn an_engine_that_gave_up_reports_no_version_port_or_uptime() {
    let lab = lab(Plan {
        restart_limit: 1,
        ..Plan::new("downloaded=all").installed(Tier::Small)
    });
    wait_for("the mind to arrive", || lab.has_mind());
    crash_until_it_gives_up(&lab);
    wait_for("the notice", || lab.has_notice("restarts"));
    let details = lab.h.ok("get_advanced", json!({}));
    assert!(details["engineVersion"].is_null(), "{details}");
    assert!(details["enginePort"].is_null(), "{details}");
    assert!(details["engineUptimeSeconds"].is_null(), "{details}");
    assert_eq!(details["restarts"], 1);
}

#[test]
fn try_again_while_the_engine_is_healthy_leaves_it_alone() {
    let lab = lab(Plan::new("downloaded=all").installed(Tier::Small));
    wait_for("the mind to arrive", || lab.has_mind());
    let pid = block_on(lab.services.engine_pid());
    let launches = lab.launches();
    lab.h.ok("retry_engine", json!({}));
    assert_eq!(block_on(lab.services.engine_pid()), pid, "the same engine");
    assert_eq!(lab.launches(), launches, "nothing was started");
    assert!(lab.has_mind() && lab.engine_answers());
}

#[test]
fn two_presses_of_try_again_start_one_engine() {
    let lab = lab(Plan {
        restart_limit: 1,
        ..Plan::new("downloaded=all").installed(Tier::Small)
    });
    wait_for("the mind to arrive", || lab.has_mind());
    crash_until_it_gives_up(&lab);
    wait_for("the notice", || lab.has_notice("restarts"));
    let launches = lab.launches();
    let app = lab.h.app.handle().clone();
    let (first, second) =
        block_on(async { tokio::join!(runtime::retry_engine(&app), runtime::retry_engine(&app)) });
    assert!(first.is_ok() && second.is_ok());
    wait_for("the mind to come back", || lab.has_mind());
    assert!(lab.engine_answers());
    assert_eq!(lab.launches(), launches + 1, "one engine, not two");
}

#[test]
fn a_failed_engine_is_started_again_later() {
    let lab = lab(Plan {
        retry_after: Duration::from_millis(1_500),
        restart_limit: 1,
        ..Plan::new("downloaded=all").installed(Tier::Small)
    });
    wait_for("the mind to arrive", || lab.has_mind());
    crash_until_it_gives_up(&lab);
    wait_for("the mind to come back", || lab.has_mind());
    wait_for("the notice to go", || !lab.has_notice("restarts"));
    wait_for("setup to read done again", || lab.setup_phase() == "done");
}

/// The engine coming back at the same tier is not a change of mind: a standing offer of a
/// smaller one, made because memory is short, is still true.
#[test]
fn the_same_mind_coming_back_keeps_a_standing_offer() {
    let lab = lab(Plan {
        retry_after: Duration::from_millis(1_000),
        restart_limit: 1,
        free_memory: 2 * GIB,
        ..Plan::new("downloaded=all").installed(Tier::Standard)
    });
    wait_for("the mind to arrive", || lab.has_mind());
    runtime::out_of_memory(lab.h.app.handle());
    runtime::out_of_memory(lab.h.app.handle());
    wait_for("the offer", || lab.has_notice("tierDropped"));

    crash_until_it_gives_up(&lab);
    wait_for("the mind to come back", || lab.has_mind());
    assert!(
        lab.has_notice("tierDropped"),
        "memory is as short as it was"
    );
    assert_eq!(lab.services.tier_offer(), Some(Tier::Small));
}

#[test]
fn pausing_lets_go_of_the_loaded_models() {
    let lab = lab(Plan::new("downloaded=all").installed(Tier::Small));
    wait_for("the mind to arrive", || lab.has_mind());
    let unloads = |lab: &Lab| {
        lab.requests()
            .iter()
            .filter(|r| {
                r["path"]
                    .as_str()
                    .is_some_and(|p| p.ends_with("/v1/unload"))
            })
            .count()
    };
    assert_eq!(unloads(&lab), 0);
    lab.h.ok("pause", json!({"until": "2026-11-13T18:00:00Z"}));
    wait_for("the models to be unloaded", || unloads(&lab) > 0);
}

#[cfg(target_os = "linux")]
#[test]
fn quitting_leaves_no_engine_behind() {
    let lab = lab(Plan::new("downloaded=all").installed(Tier::Small));
    wait_for("the mind to arrive", || lab.has_mind());
    let pid = block_on(lab.services.engine_pid()).expect("the engine runs");
    let alive = |pid: u32| {
        std::fs::read_to_string(format!("/proc/{pid}/stat"))
            .ok()
            .and_then(|s| {
                s.rsplit_once(')')
                    .and_then(|(_, rest)| rest.split_whitespace().next().map(|c| c != "Z"))
            })
            .unwrap_or(false)
    };
    assert!(alive(pid));
    let started = std::time::Instant::now();
    lab.h.state().run_exit_hooks();
    assert!(
        started.elapsed() < Duration::from_secs(6),
        "quitting took {:?}",
        started.elapsed()
    );
    assert!(!alive(pid), "the engine is gone");
    assert!(lab.services.token().is_cancelled());
}

#[test]
fn an_upgrade_reads_recent_items_again_and_a_drop_retries_unread_ones() {
    let h = harness_with_mind(Arc::new(FakeMind::new()));
    let (note, _) = h.learn(
        "dentist note",
        "The dentist is on Thursday.",
        "Dentist",
        "Thursday",
    );
    block_on(runtime::tier_changed(
        h.app.handle(),
        Tier::Small,
        Tier::Standard,
    ));
    assert_eq!(h.engine().rereads(10).unwrap(), vec![note]);

    let stuck = id_of(&h.ok("keep_text", json!({"text": "stuck", "source": "paste"})));
    let lease = h
        .engine()
        .lease_item(&stuck, Duration::from_secs(60))
        .unwrap()
        .lease;
    h.engine()
        .fail_item(
            &stuck,
            &lease,
            nocturne_contract::CouldntReadReason::MindUnavailable,
        )
        .unwrap();
    assert_eq!(
        h.engine().get_item(&stuck).unwrap().status,
        ItemStatus::CouldntRead
    );
    block_on(runtime::tier_changed(
        h.app.handle(),
        Tier::Standard,
        Tier::Small,
    ));
    assert_eq!(
        h.engine().get_item(&stuck).unwrap().status,
        ItemStatus::Kept
    );
}

#[test]
fn two_failed_loads_with_little_memory_offer_the_smaller_mind() {
    let lab = lab(Plan {
        free_memory: 2 * GIB,
        ..Plan::new("downloaded=all").installed(Tier::Standard)
    });
    wait_for("the mind to arrive", || lab.has_mind());
    let app = lab.h.app.handle();

    runtime::out_of_memory(app);
    assert!(!lab.has_notice("tierDropped"), "one failure is not enough");
    // The second is too long after the first.
    lab.h.clock.set_str("2026-11-13T07:20:00Z");
    runtime::out_of_memory(app);
    assert!(
        !lab.has_notice("tierDropped"),
        "the first has been forgotten"
    );
    lab.h.clock.set_str("2026-11-13T07:25:00Z");
    runtime::out_of_memory(app);
    wait_for("the offer", || lab.has_notice("tierDropped"));
    let notice = lab.h.ok("get_status", json!({}))["notices"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["code"] == "tierDropped")
        .cloned()
        .unwrap();
    assert_eq!(
        notice["bytes"].as_f64().unwrap() as u64,
        3_808_000_000,
        "it counts only what is not already on disk"
    );
}

#[test]
fn the_small_mind_short_of_memory_is_offered_the_tiny_one_and_tiny_offers_nothing() {
    let small = lab(Plan {
        free_memory: GIB,
        ..Plan::new("downloaded=all").installed(Tier::Small)
    });
    wait_for("the mind to arrive", || small.has_mind());
    let app = small.h.app.handle();
    runtime::out_of_memory(app);
    runtime::out_of_memory(app);
    wait_for("the offer", || small.has_notice("tierDropped"));
    assert_eq!(small.services.tier_offer(), Some(Tier::Tiny));
    let notice = small.h.ok("get_status", json!({}))["notices"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["code"] == "tierDropped")
        .cloned()
        .unwrap();
    assert_eq!(
        notice["bytes"].as_f64().unwrap() as u64,
        nocturne_lemonade::tier::missing_bytes(Tier::Tiny, Tier::Small),
        "only the tiny mind is new"
    );
    drop(small);

    let tiny = lab(Plan {
        free_memory: GIB,
        ..Plan::new("downloaded=all").installed(Tier::Tiny)
    });
    wait_for("the mind to arrive", || tiny.has_mind());
    let app = tiny.h.app.handle();
    runtime::out_of_memory(app);
    runtime::out_of_memory(app);
    std::thread::sleep(Duration::from_millis(500));
    assert!(!tiny.has_notice("tierDropped"), "there is nothing smaller");
    assert_eq!(tiny.services.tier_offer(), None);
}

#[test]
fn the_offer_is_withdrawn_once_memory_has_stayed_back_for_ten_minutes() {
    let lab = lab(Plan {
        free_memory: 2 * GIB,
        ..Plan::new("downloaded=all").installed(Tier::Standard)
    });
    wait_for("the mind to arrive", || lab.has_mind());
    let app = lab.h.app.handle();
    runtime::out_of_memory(app);
    runtime::out_of_memory(app);
    wait_for("the offer", || lab.has_notice("tierDropped"));
    // Looks the watcher has made since this call, so a "still there" means it looked.
    let looked = |more: u64| {
        let from = lab.services.memory_checks();
        wait_for("the watcher to look", || {
            lab.services.memory_checks() >= from + more
        });
    };

    // Enough for the mind to load, though less than the whole computer needs to run it.
    lab.free.store(20 * GIB, Ordering::SeqCst);
    looked(3);
    lab.h.clock.set_str("2026-11-13T07:05:00Z");
    looked(3);
    assert!(lab.has_notice("tierDropped"), "five minutes is not enough");

    lab.free.store(2 * GIB, Ordering::SeqCst);
    looked(3);
    lab.free.store(20 * GIB, Ordering::SeqCst);
    looked(3);
    lab.h.clock.set_str("2026-11-13T07:12:00Z");
    looked(3);
    assert!(
        lab.has_notice("tierDropped"),
        "a dip starts the ten minutes again"
    );

    lab.h.clock.set_str("2026-11-13T07:20:00Z");
    wait_for("the offer to be withdrawn", || {
        !lab.has_notice("tierDropped")
    });
    assert_eq!(lab.services.tier_offer(), None);
}

#[test]
fn plenty_of_memory_means_no_offer() {
    let lab = lab(Plan::new("downloaded=all").installed(Tier::Standard));
    wait_for("the mind to arrive", || lab.has_mind());
    for _ in 0..3 {
        runtime::out_of_memory(lab.h.app.handle());
    }
    assert!(!lab.has_notice("tierDropped"));
}

#[test]
fn not_now_withdraws_the_offer_until_memory_runs_short_twice_more() {
    let lab = lab(Plan {
        free_memory: 2 * GIB,
        ..Plan::new("downloaded=all").installed(Tier::Standard)
    });
    wait_for("the mind to arrive", || lab.has_mind());
    let app = lab.h.app.handle();
    runtime::out_of_memory(app);
    runtime::out_of_memory(app);
    wait_for("the offer", || lab.has_notice("tierDropped"));
    assert_eq!(lab.h.ok("get_tier_offer", json!({})), "small");

    lab.h.ok("decline_tier_offer", json!({}));
    assert!(!lab.has_notice("tierDropped"));
    assert_eq!(lab.h.ok("get_tier_offer", json!({})), Value::Null);

    runtime::out_of_memory(app);
    assert!(
        !lab.has_notice("tierDropped"),
        "one failure after saying not now is not a pair"
    );
    runtime::out_of_memory(app);
    wait_for("the offer again", || lab.has_notice("tierDropped"));
}

/// The whole path: the engine cannot load the model, the reader says so, the person agrees to
/// the smaller mind, it is swapped in and the waiting note is read with it.
#[test]
fn agreeing_to_the_smaller_mind_swaps_it_in_and_the_note_is_read() {
    let lab = lab(Plan {
        free_memory: 2 * GIB,
        ..Plan::new("downloaded=all,load-oom").installed(Tier::Standard)
    });
    wait_for("the mind to arrive", || lab.has_mind());
    let note = id_of(&lab.h.ok(
        "keep_text",
        json!({"text": "Gran lives in Leeds", "source": "paste"}),
    ));
    wait_for("the offer", || {
        lab.services.reader.retry_now();
        lab.has_notice("tierDropped")
    });
    let status = lab.h.engine().get_item(&note).unwrap().status;
    assert!(
        matches!(status, ItemStatus::Kept | ItemStatus::Reading),
        "nothing is read while the model will not load: {status:?}"
    );

    // Choosing the smaller mind relaunches the engine, which reads its mode from disk again.
    lab.fake_mode("downloaded=all");
    set_mode(&lab.h.dir.path().join("engine"), "downloaded=all");
    lab.h.ok("start_setup", json!({"tier": "small"}));
    wait_for("the smaller mind", || {
        lab.services.tier() == Some(Tier::Small)
    });
    wait_for("the offer to go", || !lab.has_notice("tierDropped"));
    wait_for("the note to be read", || {
        lab.services.reader.retry_now();
        lab.h.engine().get_item(&note).unwrap().status == ItemStatus::Understood
    });
}

/// A change of tier that never finished leaves the tier that works in charge.
#[test]
fn an_unfinished_change_of_tier_does_not_leave_the_next_launch_without_a_mind() {
    let lab = lab(Plan {
        unfinished: Some(Tier::Small),
        ..Plan::new("downloaded=all").installed(Tier::Standard)
    });
    wait_for("the mind to arrive", || lab.has_mind());
    assert_eq!(lab.services.tier(), Some(Tier::Standard));
    wait_for("setup to read done", || lab.setup_phase() == "done");
    assert_ne!(lab.h.ok("get_status", json!({}))["state"], "gettingReady");
}

#[test]
fn a_change_that_did_not_finish_is_dropped_with_a_notice_and_can_be_started_again() {
    let lab = lab(Plan {
        unfinished: Some(Tier::Small),
        ..Plan::new("downloaded=all").installed(Tier::Standard)
    });
    wait_for("the mind to arrive", || lab.has_mind());
    wait_for("the notice", || lab.has_notice("tierUnfinished"));
    assert_eq!(lab.h.ok("get_unfinished_tier", json!({})), "small");
    assert_eq!(
        lab.services.tier(),
        Some(Tier::Standard),
        "nothing swapped by itself"
    );

    lab.h.ok("start_setup", json!({"tier": "small"}));
    wait_for("the notice to go", || !lab.has_notice("tierUnfinished"));
    assert!(lab.h.ok("get_unfinished_tier", json!({})).is_null());
    wait_for("the smaller mind", || {
        lab.services.tier() == Some(Tier::Small)
    });
}

#[test]
fn not_now_forgets_the_change_that_did_not_finish() {
    let lab = lab(Plan {
        unfinished: Some(Tier::Small),
        ..Plan::new("downloaded=all").installed(Tier::Standard)
    });
    wait_for("the notice", || lab.has_notice("tierUnfinished"));
    lab.h.ok("decline_unfinished_tier", json!({}));
    assert!(!lab.has_notice("tierUnfinished"));
    assert!(lab.h.ok("get_unfinished_tier", json!({})).is_null());
    assert_eq!(lab.services.tier(), Some(Tier::Standard));
}

/// Nothing leaves the machine unless the person started it: an unfinished setup is not resumed
/// by launching, and a finished one leaves the engine offline.
#[test]
fn launching_with_an_unfinished_setup_downloads_nothing_and_a_finished_one_goes_offline() {
    let lab = lab(Plan {
        unfinished: Some(Tier::Small),
        ..Plan::new("").installed(Tier::Standard)
    });
    wait_for("the setup to be looked at", || {
        lab.setup_phase() != "notStarted"
    });
    std::thread::sleep(Duration::from_millis(400));
    let touched: Vec<String> = lab
        .requests()
        .iter()
        .filter_map(|r| r["path"].as_str().map(str::to_string))
        .filter(|p| {
            ["/pull", "/install", "/load"]
                .iter()
                .any(|bad| p.ends_with(bad))
        })
        .collect();
    assert!(
        touched.is_empty(),
        "launching fetched or loaded: {touched:?}"
    );
    assert_eq!(lab.engine_offline(), Some(true));
}

#[test]
fn a_finished_setup_leaves_the_engine_offline() {
    let lab = lab(Plan::new(""));
    wait_for("the choice", || lab.setup_phase() == "choosing");
    lab.h.ok("start_setup", json!({"tier": "small"}));
    wait_for("setup to finish", || lab.setup_phase() == "done");
    wait_for("the engine to go offline", || {
        lab.engine_offline() == Some(true)
    });
}

/// Quitting does not wait on an engine that will not stop, and does not leave it behind
/// either: past the budget its whole group is ended, and none of it is left 3 s after quit.
#[cfg(target_os = "linux")]
#[test]
fn quitting_does_not_wait_for_an_engine_that_hangs_on_shutdown() {
    let lab = lab(Plan::new("downloaded=all,hang-shutdown,leak-sleeper").installed(Tier::Small));
    wait_for("the mind to arrive", || lab.has_mind());
    let engine_dir = lab.h.dir.path().join("engine");
    assert!(!engines_using(&engine_dir).is_empty());
    let started = std::time::Instant::now();
    lab.h.state().run_exit_hooks();
    assert!(
        started.elapsed() < Duration::from_millis(2_500),
        "quitting took {:?}",
        started.elapsed()
    );
    while !engines_using(&engine_dir).is_empty() {
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "engine processes outlived quit: {:?}",
            engines_using(&engine_dir)
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// The worst engine: it ignores the request to stop, ignores SIGTERM and has no watchdog.
/// Quitting still ends within its budget (the request's second, half a second after SIGTERM,
/// then SIGKILL), not seconds later. The documented bound is two and a half seconds; the limit
/// adds slack for a loaded machine and stays well under the twelve seconds of the polite stop.
#[cfg(target_os = "linux")]
#[test]
fn quitting_ends_an_engine_that_ignores_everything_but_sigkill() {
    let lab = lab(
        Plan::new("downloaded=all,hang-shutdown,ignore-term,no-watchdog,leak-sleeper")
            .installed(Tier::Small),
    );
    wait_for("the mind to arrive", || lab.has_mind());
    let engine_dir = lab.h.dir.path().join("engine");
    let started = std::time::Instant::now();
    lab.h.state().run_exit_hooks();
    let took = started.elapsed();
    assert!(took < Duration::from_secs(4), "quitting took {took:?}");
    assert!(
        engines_using(&engine_dir).is_empty(),
        "{:?}",
        engines_using(&engine_dir)
    );
}

/// Quitting while the engine is still starting must not leave it running.
#[cfg(target_os = "linux")]
#[test]
fn quitting_while_the_engine_starts_leaves_none_behind() {
    let lab = lab(Plan::new("slow-start-ms=2500").installed(Tier::Small));
    std::thread::sleep(Duration::from_millis(500));
    assert!(!lab.has_mind(), "still starting");
    let engine_dir = lab.h.dir.path().join("engine");
    let running = || engines_using(&engine_dir);
    wait_for("the engine to be spawned", || !running().is_empty());
    let started = std::time::Instant::now();
    lab.h.state().run_exit_hooks();
    assert!(started.elapsed() < Duration::from_secs(3));
    wait_for("the engine to be stopped", || running().is_empty());
}

/// Process ids whose command line names this engine folder.
#[cfg(target_os = "linux")]
fn engines_using(dir: &Path) -> Vec<u32> {
    let needle = dir.to_string_lossy().into_owned();
    std::fs::read_dir("/proc")
        .unwrap()
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().to_string_lossy().parse::<u32>().ok())
        .filter(|pid| {
            std::fs::read(format!("/proc/{pid}/cmdline"))
                .map(|c| String::from_utf8_lossy(&c).contains(&needle))
                .unwrap_or(false)
                && std::fs::read_to_string(format!("/proc/{pid}/stat"))
                    .ok()
                    .and_then(|s| {
                        s.rsplit_once(')')
                            .and_then(|(_, r)| r.split_whitespace().next().map(|c| c != "Z"))
                    })
                    .unwrap_or(false)
        })
        .collect()
}

#[test]
fn choosing_another_tier_while_setup_runs_is_refused_not_ignored() {
    let lab = lab(Plan::new("download-ms=3000"));
    wait_for("the choice", || lab.setup_phase() == "choosing");
    lab.h.ok("start_setup", json!({"tier": "small"}));
    wait_for("downloading", || lab.setup_phase() == "downloading");
    assert_eq!(
        lab.h.code("start_setup", json!({"tier": "standard"})),
        "busy"
    );
    lab.h.ok("start_setup", json!({"tier": "small"}));
}
