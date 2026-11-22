//! The scripted driver for `scripts/net-check.sh online`: the real app over the mock runtime, the
//! real private engine, and Online answers already on for an address that cannot be reached from
//! the check's loopback-only namespace. It asks questions, idles, turns Online answers off and
//! asks again, writing a mark for each step so `scripts/netlog.py --mode online` can tie every
//! connection the engine makes to the question that caused it.
//!
//! Not part of a normal test run: it needs the engine bundle and the check's environment.
//! Environment:
//! - `NOCTURNE_DATA_DIR`: the scratch data folder (the check's own, never the person's).
//! - `NET_CHECK_MARKS`: the file the marks are appended to, one `<epoch seconds> <kind>` line each.
//! - `NET_CHECK_PROVIDER_URL`: the address Online answers is on for (default `https://198.51.100.7:8443/v1`).
//! - `NET_CHECK_ASKS`: questions asked while on, and again after turning off (default 3).
//! - `NET_SECONDS`: how long to idle while on (default 600).

mod common;

use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use common::*;
use nocturne_app_lib::online::keys::KeyStore;
use nocturne_app_lib::online::route::AppPort;
use nocturne_app_lib::online::testing::MemoryKeychain;
use nocturne_app_lib::runtime::{self, StartOptions};
use nocturne_app_lib::{AppState, configure};
use nocturne_contract::{LimitKind, OnlineLimit};
use nocturne_engine::{FakeMind, ManualClock};
use nocturne_intelligence::FakeHost;
use nocturne_lemonade::online::OnlineKey;
use serde_json::json;
use tauri::WebviewWindowBuilder;
use tauri::async_runtime::block_on;
use tauri::test::{mock_builder, mock_context, noop_assets};

const KEY: &str = "sk-net-check-0123456789abcdefghijklmn";
const MODEL: &str = "net-check-model";

fn mark(path: &PathBuf, kind: &str) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs_f64();
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .expect("the marks file");
    writeln!(file, "{now:.6} {kind}").expect("a mark");
    file.flush().expect("a mark is written");
    println!("mark {kind}");
}

fn ask(h: &Harness, seen: &Mutex<Vec<nocturne_contract::AskDelta>>, question: &str) -> String {
    let id = h
        .ok("ask", json!({"question": question, "place": null}))
        .as_str()
        .unwrap()
        .to_string();
    let start = Instant::now();
    loop {
        if deltas_for(seen, &id).iter().any(|d| is_terminal(d.phase)) {
            break;
        }
        assert!(
            start.elapsed() < Duration::from_secs(90),
            "no answer to {question}"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    wait_for("the ask to end", || h.state().asks.in_flight() == 0);
    let last = deltas_for(seen, &id).pop().unwrap();
    format!("{:?} answered by {:?}", last.phase, last.answered_by)
}

#[test]
#[ignore = "driven by scripts/net-check.sh online"]
fn online_answers_in_the_network_check() {
    let data_dir = PathBuf::from(std::env::var_os("NOCTURNE_DATA_DIR").expect("NOCTURNE_DATA_DIR"));
    let marks = PathBuf::from(std::env::var_os("NET_CHECK_MARKS").expect("NET_CHECK_MARKS"));
    let url = std::env::var("NET_CHECK_PROVIDER_URL")
        .unwrap_or_else(|_| "https://198.51.100.7:8443/v1".to_string());
    let asks: usize = std::env::var("NET_CHECK_ASKS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3);
    let idle: u64 = std::env::var("NET_SECONDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(600);

    std::fs::create_dir_all(data_dir.join("logs")).expect("the logs folder");
    if let Ok(file) = std::fs::File::create(data_dir.join("logs").join("driver.log")) {
        let _ = tracing_subscriber::fmt()
            .with_writer(Mutex::new(file))
            .with_ansi(false)
            .with_env_filter(tracing_subscriber::EnvFilter::new("info"))
            .try_init();
    }

    let clock = Arc::new(ManualClock::at("2026-11-13T07:00:00Z", "Europe/London"));
    let state = AppState::open(&data_dir, clock.clone());
    state.mind.set(Arc::new(FakeMind::new()));
    let app = configure(mock_builder().manage(state))
        .build(mock_context(noop_assets()))
        .expect("the app builds");
    let webview = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("a window");
    let h = Harness {
        app,
        webview,
        dir: tempfile::tempdir().expect("a scratch folder"),
        clock,
    };

    // Online answers is already on, as after any earlier setup: the row says so and the key is
    // in the owner-only file, because no keychain answers here.
    let keychain = Arc::new(MemoryKeychain::unavailable());
    KeyStore::new(keychain.clone(), &data_dir)
        .save(&OnlineKey::new(KEY.to_string()))
        .expect("the key file");
    h.engine()
        .update_online_state(|s| {
            s.wanted_on = true;
            s.set_up = true;
            s.base_url = Some(url.clone());
            s.model = Some(MODEL.to_string());
            s.phones = false;
            s.limit = OnlineLimit {
                kind: LimitKind::Questions,
                questions: 200,
                usd: None,
            };
            s.key_storage = Some(nocturne_contract::KeyStorage::File);
        })
        .expect("the online row");
    h.engine()
        .see_online_change()
        .expect("the person has seen what changed");
    nocturne_app_lib::online::manage_hub(
        h.app.handle(),
        Arc::new(AppPort {
            app: h.app.handle().clone(),
        }),
        keychain,
    )
    .expect("the hub");
    for (note, sentence, subject, object) in [
        (
            "NOTEMARK dentist",
            "The dentist is on Thursday.",
            "Dentist",
            "Thursday",
        ),
        (
            "NOTEMARK boiler",
            "The boiler service is on Monday.",
            "Boiler",
            "Monday",
        ),
    ] {
        h.learn(note, sentence, subject, object);
    }

    let host = FakeHost::new();
    let bundle = runtime::lemonade::find_bundle(h.app.handle())
        .expect("no engine bundle: run scripts/fetch-lemonade.sh");
    mark(&marks, "launch");
    let services = start(&h, StartOptions::with_engine(host, bundle));
    let ready = wait_until(Duration::from_secs(180), || {
        block_on(async { services.client().await.is_some() })
    });
    assert!(ready, "the engine never started");
    println!("engine ready");

    let seen = h.ask_deltas();
    let questions = [
        "When is the dentist?",
        "When is the boiler service?",
        "Who is the dentist?",
        "What day is the boiler service?",
        "Is the dentist on Thursday?",
    ];
    for i in 0..asks {
        mark(&marks, "ask");
        let outcome = ask(&h, &seen, questions[i % questions.len()]);
        println!("ask {}: {outcome}", i + 1);
        std::thread::sleep(Duration::from_secs(3));
    }

    mark(&marks, "idle-start");
    std::thread::sleep(Duration::from_secs(idle));
    mark(&marks, "idle-end");

    mark(&marks, "off");
    h.ok("turn_off_online", json!({"forget": false}));
    println!("turned off");
    for i in 0..asks {
        let outcome = ask(&h, &seen, questions[i % questions.len()]);
        println!("ask after off {}: {outcome}", i + 1);
    }
    std::thread::sleep(Duration::from_secs(3));
    h.state().run_exit_hooks();
}

fn wait_until(limit: Duration, mut done: impl FnMut() -> bool) -> bool {
    let start = Instant::now();
    while start.elapsed() < limit {
        if done() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    false
}
