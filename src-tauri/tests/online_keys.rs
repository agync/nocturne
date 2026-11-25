//! The key for Online answers: where it is kept, that the export never holds it, and that it
//! never appears in a reply, an event, a notice, a view or a log line, even at trace level.
//! This binary installs one trace-level subscriber that captures every line it writes.

mod common;

use std::io::Write;
use std::sync::{Arc, Mutex, OnceLock};

use common::*;
use nocturne_app_lib::online::testing::{MemoryKeychain, StandIn, StandInPort, fake_provider};
use nocturne_engine::FakeMind;
use serde_json::{Value, json};
use tauri::Listener;
use tauri::async_runtime::block_on;

const URL: &str = "https://api.example.com/v1";

#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Every log line any test in this binary writes, at trace level.
fn logs() -> &'static Captured {
    static LOGS: OnceLock<Captured> = OnceLock::new();
    LOGS.get_or_init(|| {
        let captured = Captured::default();
        let writer = captured.clone();
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_ansi(false)
            .with_writer(move || writer.clone())
            .try_init()
            .expect("one subscriber for this binary");
        captured
    })
}

fn logged() -> String {
    String::from_utf8_lossy(&logs().0.lock().unwrap()).into_owned()
}

struct Lab {
    h: Harness,
    s: StandIn,
    keychain: Arc<MemoryKeychain>,
}

fn lab_with(keychain: MemoryKeychain) -> Lab {
    logs();
    let h = harness_with_mind(Arc::new(FakeMind::new()));
    let s = StandIn::start(&fake_provider());
    let port = Arc::new(StandInPort::new(Some(s.port)));
    let keychain = Arc::new(keychain);
    nocturne_app_lib::online::manage_hub(h.app.handle(), port, keychain.clone())
        .unwrap()
        .set_confirm(nocturne_app_lib::online::testing::agree(Default::default()));
    // As on the consent step: the person has seen that only what is marked private stays here.
    h.ok("see_online_change", json!({}));
    Lab { h, s, keychain }
}

impl Lab {
    fn script(&self, body: Value) {
        block_on(async {
            nocturne_lemonade::tls::client()
                .post(self.s.url("/fake/script"))
                .json(&body)
                .send()
                .await
                .unwrap();
        });
    }

    fn turn_on(&self, key: &str) -> Value {
        let out = self.h.ok(
            "check_online",
            json!({"form": {"baseUrl": URL, "key": key, "model": "stand-in-model"}}),
        );
        assert_eq!(out["type"], "works");
        self.h.ok(
            "turn_on_online",
            json!({"settings": {"phones": true,
                                "limit": {"kind": "questions", "questions": 200, "usd": null}}}),
        )
    }

    fn ask(&self, question: &str) -> Vec<nocturne_contract::AskDelta> {
        let seen = self.h.ask_deltas();
        let id = self
            .h
            .ok("ask", json!({"question": question}))
            .as_str()
            .unwrap()
            .to_string();
        wait_for("the answer", || {
            deltas_for(&seen, &id).iter().any(|d| is_terminal(d.phase))
        });
        wait_for("the ask to end", || self.h.state().asks.in_flight() == 0);
        deltas_for(&seen, &id)
    }
}

#[test]
fn keychain_unavailable_uses_owner_only_file() {
    let key = "sk-file-test-0123456789abcdefghijk";
    let l = lab_with(MemoryKeychain::unavailable());
    let v = l.turn_on(key);
    assert_eq!(v["keyStorage"], "file");
    let file = l.h.dir.path().join("online").join("key");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), key);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&file), 0o600);
        assert_eq!(mode(file.parent().unwrap()), 0o700);
    }
    // The keychain comes back: the next save moves the key there and the file goes.
    l.keychain
        .unavailable
        .store(false, std::sync::atomic::Ordering::SeqCst);
    l.h.ok("turn_off_online", json!({"forget": false}));
    let v = l.turn_on(key);
    assert_eq!(v["keyStorage"], "keychain");
    assert!(!file.exists());
    l.h.ok("turn_off_online", json!({"forget": true}));
    assert!(!l.keychain.holds_a_key());
}

#[test]
fn key_file_excluded_from_export() {
    let key = "sk-export-test-0123456789abcdefghij";
    let l = lab_with(MemoryKeychain::unavailable());
    l.h.learn("note", "The dentist is on Thursday.", "Dentist", "Thursday");
    l.turn_on(key);
    l.ask("When is the dentist?");
    let out = tempfile::tempdir().unwrap();
    let report =
        l.h.ok("export_to", json!({"dir": out.path().to_str().unwrap()}));
    let folder = std::path::PathBuf::from(report["path"].as_str().unwrap());
    assert!(folder.join("online").join("sent.md").is_file());
    let mut stack = vec![folder];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            assert_ne!(path.file_name().unwrap(), "key", "{}", path.display());
            let bytes = std::fs::read(&path).unwrap();
            let text = String::from_utf8_lossy(&bytes);
            assert!(!text.contains(&key[..20]), "{}", path.display());
        }
    }
}

#[test]
fn key_never_in_views_events_notices_or_logs() {
    let key = "sk-never-shown-0123456789abcdefghijklmn";
    let l = lab_with(MemoryKeychain::default());
    let events = Arc::new(Mutex::new(Vec::<String>::new()));
    for name in nocturne_contract::ALL_EVENTS {
        let sink = events.clone();
        l.h.app.listen(name.to_string(), move |ev| {
            sink.lock().unwrap().push(ev.payload().to_string());
        });
    }
    l.h.learn("note", "The dentist is on Thursday.", "Dentist", "Thursday");
    let mut replies = Vec::new();
    replies.push(l.turn_on(key));
    let d = l.ask("When is the dentist?");
    replies.push(serde_json::to_value(&d).unwrap());
    // A provider that echoes the key in its error body: nothing of it comes back.
    l.script(json!({"status": 401, "marker": "ECHO-MARKER"}));
    let d = l.ask("When is the dentist?");
    replies.push(serde_json::to_value(&d).unwrap());
    l.script(json!({"status": null}));
    replies.push(l.h.ok(
        "check_online",
        json!({"form": {"baseUrl": URL, "key": key, "model": "stand-in-model"}}),
    ));
    for cmd in ["get_online", "get_status", "get_privacy", "get_advanced"] {
        replies.push(l.h.ok(cmd, json!({})));
    }
    replies.push(l.h.ok("list_online_sends", json!({"limit": 50, "before": null})));
    replies.push(l.h.ok(
        "update_online_settings",
        json!({"patch": {"phones": false}}),
    ));
    let shown = format!(
        "{}\n{}\n{}",
        serde_json::to_string(&replies).unwrap(),
        events.lock().unwrap().join("\n"),
        logged()
    );
    assert!(!logged().is_empty(), "the subscriber captured the run");
    for needle in [key, &key[..20], &key[key.len() - 12..], "ECHO-MARKER"] {
        assert!(!shown.contains(needle), "{needle} was shown or logged");
    }
    // The form's own Debug says nothing of the key.
    let form: nocturne_contract::OnlineForm =
        serde_json::from_value(json!({"baseUrl": URL, "key": key, "model": "m"})).unwrap();
    assert!(!format!("{form:?}").contains(&key[..20]));
}

#[test]
fn trace_level_ipc_never_logs_the_key() {
    let key = "sk-trace-ipc-0123456789abcdefghijklmn";
    let l = lab_with(MemoryKeychain::default());
    l.h.ok(
        "check_online",
        json!({"form": {"baseUrl": URL, "key": key, "model": "stand-in-model"}}),
    );
    l.h.ok("abandon_online_setup", json!({}));
    let text = logged();
    assert!(!text.contains(&key[..20]), "the IPC layer logged the form");
}
