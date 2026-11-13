//! The private engine inside the app, against the fake engine the lemonade crate ships for its
//! own tests: launch, setup, the mind in the slot, failure, pause and quit. No real engine, no
//! network, nothing outside the temporary folders.

mod common;

use std::path::{Path, PathBuf};
use std::sync::Arc;
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

/// The fake engine binary, built on demand when the test run did not build it.
fn fake_engine() -> PathBuf {
    let exe = std::env::current_exe().unwrap();
    let debug = exe.parent().and_then(Path::parent).unwrap().to_path_buf();
    let bin = debug.join(if cfg!(windows) {
        "fake-lemond.exe"
    } else {
        "fake-lemond"
    });
    if !bin.exists() {
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
        let built = std::process::Command::new(cargo)
            .args([
                "build",
                "-p",
                "nocturne-lemonade",
                "--features",
                "fake-server",
                "--bin",
                "fake-lemond",
            ])
            .status()
            .expect("cargo runs");
        assert!(built.success() && bin.exists(), "the fake engine is built");
    }
    bin
}

struct Lab {
    h: Harness,
    services: Arc<Services>,
    _bundle: tempfile::TempDir,
}

/// What a lab starts from.
struct Plan {
    mode: &'static str,
    /// A finished setup for this tier is already on disk.
    installed: Option<Tier>,
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
                tier: Some(tier),
                done: true,
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
    opts.quit_budget = Duration::from_secs(5);
    let free = plan.free_memory;
    opts.available_memory = Some(Arc::new(move || free));
    let services = start(&h, opts);
    Lab {
        h,
        services,
        _bundle: bundle,
    }
}

impl Lab {
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
    lab.h.ok("resume_setup", json!({}));
    // A resumed download starts over in the fake; let it be quick.
    lab.fake_mode("download-ms=150");
    wait_for("setup to finish after resuming", || {
        lab.setup_phase() == "done"
    });
    wait_for("the mind to arrive", || lab.has_mind());
}

#[test]
fn an_engine_that_gives_up_empties_the_slot_and_questions_say_unavailable() {
    let lab = lab(Plan::new("downloaded=all,crash-after-ms=900").installed(Tier::Small));
    wait_for("the mind to arrive", || lab.has_mind());
    wait_for("the engine to give up", || !lab.has_mind());
    wait_for("the notice", || lab.has_notice("restarts"));
    assert_eq!(lab.h.ok("get_status", json!({}))["state"], "gettingReady");

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
fn a_failed_engine_is_started_again_later() {
    let lab = lab(Plan {
        retry_after: Duration::from_millis(1_500),
        restart_limit: 1,
        ..Plan::new("downloaded=all,crash-after-ms=700").installed(Tier::Small)
    });
    wait_for("the mind to arrive", || lab.has_mind());
    wait_for("the engine to give up", || !lab.has_mind());
    set_mode(&lab.h.dir.path().join("engine"), "downloaded=all");
    wait_for("the mind to come back", || lab.has_mind());
    wait_for("the notice to go", || !lab.has_notice("restarts"));
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
    assert!(
        notice["bytes"].as_f64().unwrap() > 0.0,
        "it says what the download is"
    );
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
        lab.h.engine().get_item(&note).unwrap().status != ItemStatus::Kept
    });
}
