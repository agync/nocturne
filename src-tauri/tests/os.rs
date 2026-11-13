//! The OS lane through the mock runtime: nothing here touches the real desktop (no tray,
//! notification, shortcut or autostart entry).

mod common;

use std::collections::BTreeSet;

use common::harness;
use nocturne_app_lib::COMMAND_NAMES;
use std::sync::{Arc, Mutex};

use nocturne_app_lib::os::shortcut::{self, Registrar};
use nocturne_app_lib::os::{OsState, on_second_launch, quick_capture, window_may_call};
use serde_json::{Value, json};
use tauri::Manager;
use tauri_plugin_global_shortcut::Shortcut;

const CAPTURE_ALLOWED: &[&str] = &[
    "get_status",
    "get_settings",
    "keep_text",
    "keep_files",
    "keep_bytes",
    "keep_recording",
    "unkeep",
    "get_item",
    "app_ready",
];

#[test]
fn window_may_call_allows_only_capture() {
    for cmd in COMMAND_NAMES {
        assert!(window_may_call("main", cmd), "main: {cmd}");
        assert_eq!(
            window_may_call("quick-capture", cmd),
            CAPTURE_ALLOWED.contains(cmd),
            "quick-capture: {cmd}"
        );
        assert!(!window_may_call("other", cmd), "other: {cmd}");
    }
    for allowed in CAPTURE_ALLOWED {
        assert!(
            COMMAND_NAMES.contains(allowed),
            "{allowed} is not a command"
        );
    }
}

fn capability(name: &str) -> Value {
    let path = format!("{}/capabilities/{name}.json", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn quick_capture_capability_is_exact() {
    let c = capability("quick-capture");
    assert_eq!(c["windows"], json!(["quick-capture"]));
    let granted: BTreeSet<&str> = c["permissions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p.as_str().unwrap())
        .collect();
    let exact: BTreeSet<&str> = [
        "core:event:default",
        "core:window:allow-close",
        "core:window:allow-show",
        "core:window:allow-hide",
        "core:window:allow-set-focus",
        "core:window:allow-get-all-windows",
        "core:webview:default",
        "core:app:default",
    ]
    .into();
    assert_eq!(granted, exact);
    assert!(c.get("remote").is_none(), "no remote origins");
}

#[test]
fn the_main_capability_does_not_cover_quick_capture() {
    assert_eq!(capability("default")["windows"], json!(["main"]));
}

#[test]
fn second_launch_flag_opens_capture() {
    let h = harness();
    let file = h.dir.path().join("note.txt");
    std::fs::write(&file, "do not keep me").unwrap();
    let args = vec![
        "nocturne".to_string(),
        quick_capture::FLAG.to_string(),
        file.to_string_lossy().into_owned(),
    ];
    assert!(on_second_launch(h.app.handle(), &args));
    let plain = vec!["nocturne".to_string(), "--hidden".to_string()];
    assert!(!on_second_launch(h.app.handle(), &plain));

    let app = h.app.handle().clone();
    tauri::async_runtime::block_on(nocturne_app_lib::handle_second_launch(
        &app,
        args,
        "/".into(),
    ));
    let items = h.ok("list_items", json!({"filter": {}}));
    assert_eq!(items.as_array().map(Vec::len), Some(0), "{items}");
}

#[test]
fn the_window_is_made_once_and_a_second_open_focuses_it() {
    let h = harness();
    quick_capture::create(h.app.handle()).unwrap();
    quick_capture::create(h.app.handle()).unwrap();
    assert_eq!(
        h.app
            .webview_windows()
            .keys()
            .filter(|l| l.as_str() == quick_capture::LABEL)
            .count(),
        1
    );
}

#[derive(Default)]
struct Fake {
    held: Mutex<Vec<Shortcut>>,
    refuse: Mutex<Vec<Shortcut>>,
}

impl Registrar for Fake {
    fn register(&self, s: &Shortcut) -> Result<(), String> {
        if self.refuse.lock().unwrap().contains(s) {
            return Err("taken".into());
        }
        self.held.lock().unwrap().push(*s);
        Ok(())
    }

    fn unregister(&self, s: &Shortcut) -> Result<(), String> {
        self.held.lock().unwrap().retain(|h| h != s);
        Ok(())
    }
}

#[test]
fn failed_change_keeps_old() {
    let h = harness();
    let fake = Arc::new(Fake::default());
    h.app.manage(OsState::new(h.dir.path()));
    let os = h.app.state::<OsState>();
    os.set_registrar(fake.clone());
    shortcut::install(&os, "CommandOrControl+Shift+Space", true);
    let old = shortcut::parse("Ctrl+Shift+Space").unwrap();
    assert_eq!(*fake.held.lock().unwrap(), vec![old]);
    assert_eq!(h.ok("get_settings", json!({}))["shortcutAvailable"], true);

    let taken = shortcut::parse("Ctrl+Alt+K").unwrap();
    fake.refuse.lock().unwrap().push(taken);
    assert_eq!(
        h.code(
            "update_settings",
            json!({"patch": {"shortcut": "Ctrl+Alt+K"}})
        ),
        "invalid"
    );
    assert_eq!(
        h.code(
            "update_settings",
            json!({"patch": {"shortcut": "Ctrl+Shift"}})
        ),
        "invalid"
    );
    let settings = h.ok("get_settings", json!({}));
    assert_eq!(
        settings["shortcut"], "CommandOrControl+Shift+Space",
        "not saved"
    );
    assert_eq!(settings["shortcutAvailable"], true);
    assert_eq!(*fake.held.lock().unwrap(), vec![old]);

    fake.refuse.lock().unwrap().clear();
    let saved = h.ok(
        "update_settings",
        json!({"patch": {"shortcut": "Ctrl+Alt+K"}}),
    );
    assert_eq!(saved["shortcut"], "Ctrl+Alt+K");
    assert_eq!(*fake.held.lock().unwrap(), vec![taken]);
}

#[test]
fn a_session_that_cannot_grab_says_the_shortcut_is_not_available() {
    let h = harness();
    let fake = Arc::new(Fake::default());
    h.app.manage(OsState::new(h.dir.path()));
    let os = h.app.state::<OsState>();
    os.set_registrar(fake.clone());
    shortcut::install(&os, "CommandOrControl+Shift+Space", false);
    assert!(fake.held.lock().unwrap().is_empty());
    assert_eq!(h.ok("get_settings", json!({}))["shortcutAvailable"], false);
    let saved = h.ok(
        "update_settings",
        json!({"patch": {"shortcut": "Ctrl+Alt+K"}}),
    );
    assert_eq!(
        saved["shortcut"], "Ctrl+Alt+K",
        "saved for when it can be used"
    );
    assert_eq!(saved["shortcutAvailable"], false);
    assert!(fake.held.lock().unwrap().is_empty());
}

#[test]
fn the_quick_keep_command_is_a_flagged_launch() {
    let h = harness();
    let cmd = h.ok("quick_keep_command", json!({}));
    assert!(cmd.as_str().unwrap().ends_with(" --quick-capture"), "{cmd}");
}
