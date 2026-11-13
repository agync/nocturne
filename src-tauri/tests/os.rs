//! The OS lane through the mock runtime: nothing here touches the real desktop (no tray,
//! notification, shortcut or autostart entry).

mod common;

use std::collections::BTreeSet;

use common::harness;
use nocturne_app_lib::COMMAND_NAMES;
use nocturne_app_lib::os::{on_second_launch, quick_capture, window_may_call};
use serde_json::{Value, json};
use tauri::Manager;

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
