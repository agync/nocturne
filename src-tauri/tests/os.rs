//! The OS lane through the mock runtime: nothing here touches the real desktop (no tray,
//! notification, shortcut or autostart entry).

mod common;

use std::collections::BTreeSet;

use common::{harness, wait_for};
use nocturne_app_lib::COMMAND_NAMES;
use std::sync::{Arc, Mutex};

use nocturne_app_lib::os::notify::{self, Notifier};
use nocturne_app_lib::os::shortcut::{self, Registrar};
use nocturne_app_lib::os::tray::{self, Icon, MenuEntry};
use nocturne_app_lib::os::{OsState, on_second_launch, quick_capture, window_may_call};
use serde_json::{Value, json};
use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::Shortcut;

const CAPTURE_ALLOWED: &[&str] = &[
    "get_status",
    "get_settings",
    "keep_text",
    "keep_note",
    "top_feeling_words",
    "keep_files",
    "take_quick_drop",
    "keep_bytes",
    "keep_recording",
    "unkeep",
    "get_item",
    "open_sound_settings",
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
        "core:event:allow-emit-to",
        "core:window:allow-close",
        "core:window:allow-show",
        "core:window:allow-set-focus",
        // The window follows its card's height, so it can set its own size and nothing more.
        "core:window:allow-set-size",
        "core:window:allow-get-all-windows",
    ]
    .into();
    assert_eq!(granted, exact);
    // Answers and kept items travel on events, and this window needs neither.
    for p in &granted {
        assert!(!p.contains("listen"), "{p}");
        assert_ne!(*p, "core:event:default", "it includes listening");
    }
    assert!(c.get("remote").is_none(), "no remote origins");
}

fn adapter() -> String {
    std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../ui/src/platform/tauri.ts"
    ))
    .expect("ui/src/platform/tauri.ts must exist")
}

/// The window methods the adapter uses that the quick capture window can reach, each with the
/// permission it needs. Anything the adapter calls beyond these is for the main window.
#[test]
fn quick_capture_capability_covers_what_the_adapter_calls_for_it() {
    let src = adapter();
    let c = capability("quick-capture");
    let granted: BTreeSet<&str> = c["permissions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p.as_str().unwrap())
        .collect();
    for (call, permission) in [
        (".close()", "core:window:allow-close"),
        ("Window.getByLabel(", "core:window:allow-get-all-windows"),
        (".show()", "core:window:allow-show"),
        (".setFocus()", "core:window:allow-set-focus"),
        (".setSize(", "core:window:allow-set-size"),
        ("emitTo(", "core:event:allow-emit-to"),
    ] {
        assert!(src.contains(call), "the adapter no longer uses {call}");
        assert!(granted.contains(permission), "{call} needs {permission}");
    }
    // Nothing is granted that the adapter does not use for this window, beyond the basics.
    for p in &granted {
        assert!(p.starts_with("core:"), "{p}");
    }
}

#[test]
fn the_tray_sends_screens_on_the_event_the_interface_listens_to() {
    let src = adapter();
    let line = src
        .lines()
        .find(|l| l.contains("const OPEN_IN_MAIN"))
        .expect("the adapter names the open-in-main event");
    assert!(
        line.contains(&format!("\"{}\"", nocturne_app_lib::os::EVENT_NAVIGATE)),
        "{line}"
    );
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
    // CommandOrControl is Command on a Mac and Control elsewhere.
    let old = shortcut::parse(if cfg!(target_os = "macos") {
        "Command+Shift+Space"
    } else {
        "Ctrl+Shift+Space"
    })
    .unwrap();
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

type Drawn = Arc<Mutex<Vec<(Icon, Vec<MenuEntry>)>>>;

#[test]
fn a_forged_status_event_does_not_change_the_tray() {
    let h = harness();
    let drawn: Drawn = Arc::default();
    let sink = drawn.clone();
    tray::follow(
        h.app.handle(),
        Arc::new(move |icon, model| sink.lock().unwrap().push((icon, model))),
    );
    wait_for("the first draw", || !drawn.lock().unwrap().is_empty());
    let honest = drawn.lock().unwrap()[0].clone();

    let forged = json!({
        "state": "needsYou", "needsYou": 99, "waitingToRead": 0,
        "sentToInternetBytes": 0.0, "sentMeasured": false, "pausedUntil": null,
        "tidy": null, "nextTidy": null, "notices": []
    });
    h.app.emit("status-changed", forged).unwrap();
    wait_for("a draw after the event", || {
        drawn.lock().unwrap().len() >= 2
    });
    let after = drawn.lock().unwrap()[1].clone();
    assert_eq!(after, honest, "the payload must not reach the tray");
    assert_ne!(after.0, Icon::NeedsYou);
    assert!(
        !format!("{:?}", after.1).contains("99"),
        "a forged count reached the menu"
    );
}

#[derive(Default)]
struct Sent(Mutex<Vec<(String, String)>>);

impl Notifier for Sent {
    fn notify(&self, title: &str, body: &str) -> Result<(), String> {
        self.0.lock().unwrap().push((title.into(), body.into()));
        Ok(())
    }
}

fn with_notifier(h: &common::Harness) -> Arc<Sent> {
    let sent = Arc::new(Sent::default());
    h.app.manage(OsState::new(h.dir.path()));
    h.app.state::<OsState>().set_notifier(sent.clone());
    notify::listen(h.app.handle());
    sent
}

#[test]
fn a_forged_page_ready_with_no_page_saved_sends_nothing() {
    let h = harness();
    let sent = with_notifier(&h);
    let forged =
        json!({"date": "2026-11-13", "pageNumber": 1, "headline": "A secret", "needsYou": 5});
    h.app.emit("page-ready", forged).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(400));
    assert!(sent.0.lock().unwrap().is_empty());
}

#[test]
fn page_ready_with_a_saved_page_sends_one_private_free_notification() {
    let h = harness();
    let sent = with_notifier(&h);
    let page = h.save_todays_page();
    h.app
        .emit("page-ready", json!({"headline": "A secret"}))
        .unwrap();
    wait_for("the notification", || !sent.0.lock().unwrap().is_empty());
    h.app.emit("page-ready", json!({})).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(400));
    let sent = sent.0.lock().unwrap();
    assert_eq!(sent.len(), 1, "once per page date");
    let (title, body) = &sent[0];
    assert_eq!(title, "Your page is ready");
    assert!(
        !body.contains(&page.headline) && !body.contains("secret"),
        "{body}"
    );
    assert!(
        body.ends_with("It\u{2019}s there when you open Nocturne."),
        "{body}"
    );
}

fn run_evening(h: &common::Harness) -> bool {
    let app = h.app.handle().clone();
    tauri::async_runtime::block_on(async move { notify::check_evening(&app).await })
}

#[test]
fn the_evening_reminder_is_off_by_default_and_sends_nothing() {
    let h = harness();
    let sent = with_notifier(&h);
    assert_eq!(
        h.ok("get_settings", json!({}))["eveningReminder"],
        json!({"on": false, "time": "21:00"})
    );
    h.clock.set("2026-11-13T21:00:00Z".parse().unwrap());
    assert!(!run_evening(&h));
    assert!(sent.0.lock().unwrap().is_empty());
}

#[test]
fn the_evening_reminder_sends_once_at_its_time_with_fixed_words() {
    let h = harness();
    let sent = with_notifier(&h);
    h.ok(
        "update_settings",
        json!({"patch": {"eveningReminder": {"on": true, "time": "20:30"}}}),
    );
    h.ok(
        "keep_note",
        json!({"text": "A secret about Gran", "source": "quickCapture", "feeling": null, "good": false}),
    );
    h.clock.set("2026-11-13T20:29:00Z".parse().unwrap());
    assert!(!run_evening(&h), "not before its time");
    h.clock.set("2026-11-13T20:30:00Z".parse().unwrap());
    assert!(run_evening(&h));
    assert!(!run_evening(&h), "once a day");
    h.clock.set("2026-11-13T20:45:00Z".parse().unwrap());
    assert!(!run_evening(&h), "still once");
    {
        let sent = sent.0.lock().unwrap();
        assert_eq!(sent.len(), 1);
        assert_eq!(
            sent[0],
            (
                "Nocturne".to_string(),
                "Anything about today you want to keep?".to_string()
            )
        );
    }
    h.clock.set("2026-11-14T20:30:00Z".parse().unwrap());
    assert!(run_evening(&h), "the next day it is due again");
    let sent = sent.0.lock().unwrap();
    assert_eq!(sent.len(), 2);
    assert_eq!(sent[0], sent[1], "the same words every day");
}

#[test]
fn checks_that_arrive_together_send_one_evening_reminder() {
    let h = harness();
    let sent = with_notifier(&h);
    h.ok(
        "update_settings",
        json!({"patch": {"eveningReminder": {"on": true, "time": "21:00"}}}),
    );
    h.clock.set("2026-11-13T21:00:00Z".parse().unwrap());
    let handles: Vec<_> = (0..8)
        .map(|_| {
            let app = h.app.handle().clone();
            tauri::async_runtime::spawn(async move { notify::check_evening(&app).await })
        })
        .collect();
    let shown = tauri::async_runtime::block_on(async {
        let mut n = 0;
        for handle in handles {
            n += usize::from(handle.await.unwrap());
        }
        n
    });
    assert_eq!(shown, 1);
    assert_eq!(sent.0.lock().unwrap().len(), 1);
}

// Windows starts at login through the autostart plugin, not an entry file.
#[cfg(not(windows))]
#[test]
fn start_at_login_writes_the_entry_in_the_given_folder_and_reports_the_truth() {
    use nocturne_app_lib::os::autostart::{ENTRY, Target};
    let h = harness();
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("autostart");
    h.app.manage(OsState::new(h.dir.path()));
    h.app.state::<OsState>().set_autostart(Target {
        dir: dir.clone(),
        exec: "/usr/bin/nocturne".into(),
    });
    assert_eq!(h.ok("get_settings", json!({}))["startAtLogin"], false);
    let on = h.ok("update_settings", json!({"patch": {"startAtLogin": true}}));
    assert_eq!(on["startAtLogin"], true);
    let entry = std::fs::read_to_string(dir.join(ENTRY)).unwrap();
    if cfg!(target_os = "macos") {
        assert!(
            entry.contains("<string>/usr/bin/nocturne</string>"),
            "{entry}"
        );
        assert!(entry.contains("<string>--hidden</string>"), "{entry}");
    } else {
        assert!(entry.contains("Exec=/usr/bin/nocturne --hidden"), "{entry}");
    }
    // The person removes the entry by hand: the settings say what is true.
    std::fs::remove_file(dir.join(ENTRY)).unwrap();
    assert_eq!(h.ok("get_settings", json!({}))["startAtLogin"], false);
    h.ok("update_settings", json!({"patch": {"startAtLogin": true}}));
    let off = h.ok("update_settings", json!({"patch": {"startAtLogin": false}}));
    assert_eq!(off["startAtLogin"], false);
    assert!(!dir.join(ENTRY).exists());
}

#[test]
fn a_start_at_login_that_cannot_be_written_is_refused_and_not_saved() {
    use nocturne_app_lib::os::autostart::Target;
    let h = harness();
    let root = tempfile::tempdir().unwrap();
    let blocked = root.path().join("a-file");
    std::fs::write(&blocked, b"x").unwrap();
    h.app.manage(OsState::new(h.dir.path()));
    h.app.state::<OsState>().set_autostart(Target {
        dir: blocked.join("autostart"),
        exec: "/usr/bin/nocturne".into(),
    });
    assert_eq!(
        h.code("update_settings", json!({"patch": {"startAtLogin": true}})),
        "unavailable"
    );
    assert!(!h.engine().settings().unwrap().start_at_login);
}

#[cfg(target_os = "macos")]
#[test]
fn from_a_mounted_disk_start_at_login_is_refused_and_says_why() {
    use nocturne_app_lib::os::autostart::{MOVE_FIRST, Target};
    let h = harness();
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("LaunchAgents");
    h.app.manage(OsState::new(h.dir.path()));
    h.app.state::<OsState>().set_autostart(Target {
        dir: dir.clone(),
        exec: "/Volumes/Nocturne/Nocturne.app/Contents/MacOS/Nocturne".into(),
    });
    let err = h
        .call("update_settings", json!({"patch": {"startAtLogin": true}}))
        .unwrap_err();
    assert_eq!(err["code"], "unavailable");
    assert_eq!(err["detail"], MOVE_FIRST);
    assert!(!dir.exists(), "nothing was written");
    assert!(!h.engine().settings().unwrap().start_at_login);
}

#[test]
fn delete_everything_removes_the_web_views_own_folders_and_nothing_else() {
    use nocturne_app_lib::os::webview;
    let h = harness();
    let root = tempfile::tempdir().unwrap();
    let dirs = webview::macos_dirs(root.path(), "app.nocturne.desktop");
    for d in &dirs {
        std::fs::create_dir_all(d.join("data")).unwrap();
    }
    let other = root.path().join("Library/WebKit/some.other.app");
    std::fs::create_dir_all(&other).unwrap();
    h.app.manage(OsState::new(h.dir.path()));
    h.app.state::<OsState>().set_webview_dirs(dirs.clone());
    h.ok(
        "delete_everything",
        json!({"confirmPhrase": "delete everything"}),
    );
    assert!(dirs.iter().all(|d| !d.exists()), "{dirs:?}");
    assert!(other.exists(), "another app's folder stays");
}

mod inbox {
    use std::path::Path;
    use std::time::{Duration, Instant};

    use super::*;
    use nocturne_app_lib::os::inbox::Timing;

    fn fast() -> Timing {
        Timing {
            stable_after: Duration::from_millis(120),
            rescan: Duration::from_millis(150),
            debounce: Duration::from_millis(20),
        }
    }

    fn watching(h: &common::Harness) -> tempfile::TempDir {
        h.app.manage(OsState::new(h.dir.path()));
        h.app.state::<OsState>().set_inbox_timing(fast());
        tempfile::tempdir().unwrap()
    }

    fn kept(h: &common::Harness) -> Vec<Value> {
        h.ok("list_items", json!({"filter": {}}))
            .as_array()
            .unwrap()
            .clone()
    }

    fn listing(dir: &Path) -> Vec<(String, Vec<u8>)> {
        let mut v: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| {
                let e = e.unwrap();
                (
                    e.file_name().to_string_lossy().into_owned(),
                    std::fs::read(e.path()).unwrap(),
                )
            })
            .collect();
        v.sort();
        v
    }

    fn ledger_folder(h: &common::Harness) -> Option<String> {
        h.app.state::<OsState>().file().inbox.map(|l| l.folder)
    }

    #[test]
    fn existing_files_are_not_kept_and_new_ones_are_kept_once_without_touching_the_folder() {
        let h = harness();
        let folder = watching(&h);
        std::fs::write(folder.path().join("old.txt"), "already here").unwrap();
        let saved = h.ok(
            "update_settings",
            json!({"patch": {"inboxFolder": folder.path().to_string_lossy()}}),
        );
        assert!(saved["inboxFolder"].is_string());
        wait_for("the baseline", || ledger_folder(&h).is_some());
        std::thread::sleep(Duration::from_millis(400));
        assert!(kept(&h).is_empty(), "what was there is not kept");

        std::fs::write(folder.path().join("new.txt"), "arrived later").unwrap();
        std::fs::write(folder.path().join("half.pdf.part"), "partial").unwrap();
        std::fs::write(folder.path().join(".hidden.txt"), "hidden").unwrap();
        let before = listing(folder.path());
        wait_for("the new file", || !kept(&h).is_empty());
        std::thread::sleep(Duration::from_millis(500));
        let items = kept(&h);
        assert_eq!(items.len(), 1, "kept once: {items:?}");
        assert_eq!(items[0]["source"], "watchedFolder");
        assert_eq!(
            listing(folder.path()),
            before,
            "nothing in the folder changed"
        );
        let stored = h.dir.path().join("store").join("originals");
        assert!(
            std::fs::read_dir(stored).unwrap().count() >= 1,
            "the original is copied into the store"
        );
    }

    #[test]
    fn a_file_still_growing_is_not_kept_until_it_stops() {
        let h = harness();
        let folder = watching(&h);
        // A write can stall for over a hundred milliseconds on a Mac, which would leave the
        // file still for as long as the shorter settle time of the other tests.
        h.app.state::<OsState>().set_inbox_timing(Timing {
            stable_after: Duration::from_millis(500),
            ..fast()
        });
        h.ok(
            "update_settings",
            json!({"patch": {"inboxFolder": folder.path().to_string_lossy()}}),
        );
        wait_for("the baseline", || ledger_folder(&h).is_some());
        let path = folder.path().join("growing.txt");
        let end = Instant::now() + Duration::from_millis(1200);
        let mut n = 0;
        while Instant::now() < end {
            n += 1;
            std::fs::write(&path, "x".repeat(n)).unwrap();
            std::thread::sleep(Duration::from_millis(40));
            assert!(kept(&h).is_empty(), "kept while it was still changing");
        }
        wait_for("it stops growing", || !kept(&h).is_empty());
    }

    #[test]
    fn empty_string_clears() {
        let h = harness();
        let folder = watching(&h);
        h.ok(
            "update_settings",
            json!({"patch": {"inboxFolder": folder.path().to_string_lossy()}}),
        );
        assert!(h.state().engine().is_ok());
        assert!(h.app.state::<OsState>().inbox_running());
        let cleared = h.ok("update_settings", json!({"patch": {"inboxFolder": ""}}));
        assert!(cleared["inboxFolder"].is_null(), "{cleared}");
        assert!(h.ok("get_settings", json!({}))["inboxFolder"].is_null());
        assert!(
            !h.app.state::<OsState>().inbox_running(),
            "the watcher stopped"
        );
        assert!(h.app.state::<OsState>().file().inbox.is_none());
        std::fs::write(folder.path().join("late.txt"), "late").unwrap();
        std::thread::sleep(Duration::from_millis(500));
        assert!(kept(&h).is_empty());
    }

    #[test]
    fn a_folder_that_is_not_allowed_is_refused_and_not_saved() {
        let h = harness();
        let _folder = watching(&h);
        for bad in [
            "/".to_string(),
            "relative".to_string(),
            "/no/such/folder/here".to_string(),
            h.dir.path().to_string_lossy().into_owned(),
            h.dir.path().join("store").to_string_lossy().into_owned(),
        ] {
            assert_eq!(
                h.code("update_settings", json!({"patch": {"inboxFolder": bad}})),
                "invalid",
                "{bad}"
            );
        }
        assert!(h.ok("get_settings", json!({}))["inboxFolder"].is_null());
        assert!(!h.app.state::<OsState>().inbox_running());
    }

    #[test]
    fn burst_of_500_does_not_block_keep_text() {
        let h = harness();
        let folder = watching(&h);
        h.ok(
            "update_settings",
            json!({"patch": {"inboxFolder": folder.path().to_string_lossy()}}),
        );
        wait_for("the baseline", || ledger_folder(&h).is_some());
        for i in 0..500 {
            std::fs::write(folder.path().join(format!("f{i}.txt")), format!("file {i}")).unwrap();
        }
        let mut took = Vec::new();
        let end = Instant::now() + Duration::from_secs(20);
        while kept(&h).len() < 500 && Instant::now() < end {
            let start = Instant::now();
            h.ok(
                "keep_text",
                json!({"text": format!("note {}", took.len()), "source": "quickCapture"}),
            );
            took.push(start.elapsed());
            std::thread::sleep(Duration::from_millis(20));
        }
        let kept_text = took.len();
        assert!(kept_text > 0);
        // An import that makes capture wait does so on every call while it runs, so the typical
        // call shows it. One slow call on a shared runner is a busy machine, not the import: the
        // test below proves the status recount is off the path, and the store's write lock is
        // held for one file's insert at a time. A call that hangs is still refused.
        took.sort();
        let typical = took[took.len() / 2];
        assert!(
            typical < Duration::from_millis(100),
            "the typical keep_text took {typical:?} during the burst"
        );
        let slowest = *took.last().unwrap();
        assert!(
            slowest < Duration::from_secs(2),
            "a keep_text took {slowest:?} during the burst"
        );
        wait_for("all 500", || kept(&h).len() >= 500 + kept_text);
    }

    #[test]
    fn keep_text_does_not_wait_for_a_status_recount_that_an_import_is_running() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let h = harness();
        let released = Arc::new(AtomicBool::new(false));
        let (held_tx, held_rx) = std::sync::mpsc::channel();
        let flag = released.clone();
        let app = h.app.handle().clone();
        let holder = std::thread::spawn(move || {
            let state = app.state::<nocturne_app_lib::AppState>();
            let _recount = state.status_refresh_gate().blocking_lock();
            held_tx.send(()).unwrap();
            std::thread::sleep(Duration::from_secs(3));
            flag.store(true, Ordering::SeqCst);
        });
        held_rx.recv().unwrap();
        h.ok(
            "keep_text",
            json!({"text": "a quick note", "source": "quickCapture"}),
        );
        assert!(
            !released.load(Ordering::SeqCst),
            "keep_text waited for the status recount"
        );
        holder.join().unwrap();
        assert_eq!(kept(&h).len(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn a_file_swapped_for_a_link_between_the_look_and_the_keep_is_not_kept() {
        let h = harness();
        let folder = watching(&h);
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret.txt"), "not yours to keep").unwrap();
        let target = outside.path().join("secret.txt");
        let swapped = folder.path().to_path_buf();
        let ran = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counted = ran.clone();
        h.app
            .state::<OsState>()
            .set_before_keep(Arc::new(move |ready| {
                counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                for rel in ready {
                    let path = swapped.join(rel);
                    std::fs::remove_file(&path).unwrap();
                    std::os::unix::fs::symlink(&target, &path).unwrap();
                }
            }));
        h.ok(
            "update_settings",
            json!({"patch": {"inboxFolder": folder.path().to_string_lossy()}}),
        );
        wait_for("the baseline", || ledger_folder(&h).is_some());
        std::fs::write(folder.path().join("new.txt"), "arrived later").unwrap();
        wait_for("the swap", || {
            ran.load(std::sync::atomic::Ordering::SeqCst) >= 1
        });
        wait_for("the file settled", || {
            h.app
                .state::<OsState>()
                .file()
                .inbox
                .is_some_and(|l| !l.entries.is_empty())
        });
        assert!(kept(&h).is_empty(), "a file outside the folder was kept");
        let stored = h.dir.path().join("store").join("originals");
        for entry in std::fs::read_dir(stored).unwrap().flatten() {
            let bytes = std::fs::read(entry.path()).unwrap_or_default();
            assert!(!String::from_utf8_lossy(&bytes).contains("not yours to keep"));
        }
    }

    #[test]
    fn each_file_of_a_batch_is_confirmed_right_before_its_own_keep() {
        use nocturne_contract::{ItemFilter, Source};
        let h = harness();
        let src = tempfile::tempdir().unwrap();
        let files: Vec<_> = (0..3)
            .map(|i| {
                common::write_file(
                    src.path(),
                    &format!("f{i}.txt"),
                    format!("file {i}").as_bytes(),
                )
            })
            .collect();
        let engine = h.engine();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = seen.clone();
        let results = tauri::async_runtime::block_on(nocturne_app_lib::keep_each(
            h.app.handle(),
            files,
            Source::Drop,
            move |p| {
                log.lock()
                    .unwrap()
                    .push(engine.items(ItemFilter::default()).unwrap().len());
                Ok(p.clone())
            },
        ))
        .unwrap();
        assert!(results.iter().all(|r| r.is_ok()));
        assert_eq!(
            *seen.lock().unwrap(),
            [0, 1, 2],
            "a file is confirmed after the one before it was kept, not before all of them"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_file_that_cannot_be_read_yet_is_tried_again() {
        use std::os::unix::fs::PermissionsExt;
        let h = harness();
        let folder = watching(&h);
        h.ok(
            "update_settings",
            json!({"patch": {"inboxFolder": folder.path().to_string_lossy()}}),
        );
        wait_for("the baseline", || ledger_folder(&h).is_some());
        let path = folder.path().join("locked.txt");
        std::fs::write(&path, "readable soon").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
        if std::fs::read(&path).is_ok() {
            return; // running with privileges that ignore permissions
        }
        std::thread::sleep(Duration::from_millis(500));
        assert!(kept(&h).is_empty());
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        wait_for("the second try", || kept(&h).len() == 1);
    }

    #[test]
    fn the_saved_ledger_has_no_file_or_folder_names() {
        let h = harness();
        let folder = watching(&h);
        std::fs::write(folder.path().join("secret-diagnosis.pdf"), "x").unwrap();
        h.ok(
            "update_settings",
            json!({"patch": {"inboxFolder": folder.path().to_string_lossy()}}),
        );
        wait_for("the baseline", || ledger_folder(&h).is_some());
        let saved = std::fs::read_to_string(h.dir.path().join("os-state.json")).unwrap();
        assert!(!saved.contains("secret-diagnosis"), "{saved}");
        let name = folder.path().file_name().unwrap().to_string_lossy();
        assert!(!saved.contains(name.as_ref()), "{saved}");
    }

    #[test]
    fn a_folder_that_appears_inside_the_data_folder_later_is_not_watched() {
        let h = harness();
        h.app.manage(OsState::new(h.dir.path()));
        h.app.state::<OsState>().set_inbox_timing(fast());
        let later = h.dir.path().join("appears-later");
        let app = h.app.handle().clone();
        let data = h.dir.path().to_path_buf();
        let shown = later.to_string_lossy().into_owned();
        tauri::async_runtime::block_on(nocturne_app_lib::os::resume_inbox(
            &app,
            &data,
            Some(shown.as_str()),
        ));
        std::thread::sleep(Duration::from_millis(300));
        std::fs::create_dir(&later).unwrap();
        std::thread::sleep(Duration::from_millis(500));
        std::fs::write(later.join("note.txt"), "inside the data folder").unwrap();
        std::thread::sleep(Duration::from_millis(900));
        assert!(kept(&h).is_empty());
    }

    const PHRASE: &str = "delete everything";

    #[test]
    fn delete_everything_stops_the_watcher_the_login_entry_and_the_shortcut() {
        use nocturne_app_lib::os::autostart::{ENTRY, Target};
        let h = harness();
        let folder = watching(&h);
        let fake = Arc::new(Fake::default());
        let os = h.app.state::<OsState>();
        os.set_registrar(fake.clone());
        shortcut::install(&os, "Ctrl+Alt+K", true);
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("autostart");
        os.set_autostart(Target {
            dir: dir.clone(),
            exec: "/usr/bin/nocturne".into(),
        });
        // Windows starts at login through the autostart plugin, not an entry file.
        let entry = !cfg!(windows);
        if entry {
            h.ok("update_settings", json!({"patch": {"startAtLogin": true}}));
            assert!(dir.join(ENTRY).exists());
        }
        h.ok(
            "update_settings",
            json!({"patch": {"inboxFolder": folder.path().to_string_lossy()}}),
        );
        wait_for("the baseline", || ledger_folder(&h).is_some());

        h.ok("delete_everything", json!({"confirmPhrase": PHRASE}));
        assert!(!os.inbox_running(), "the watcher is stopped");
        assert!(!dir.join(ENTRY).exists(), "the login entry is gone");
        assert_eq!(
            *fake.held.lock().unwrap(),
            vec![shortcut::parse(shortcut::DEFAULT).unwrap()],
            "the shortcut is the default again"
        );
        std::fs::write(folder.path().join("late.txt"), "after the wipe").unwrap();
        std::thread::sleep(Duration::from_millis(700));
        assert!(
            kept(&h).is_empty(),
            "nothing lands in the store after the wipe"
        );
        assert!(os.file().inbox.is_none());
    }

    #[test]
    fn a_wrong_phrase_leaves_the_watcher_running() {
        let h = harness();
        let folder = watching(&h);
        h.ok(
            "update_settings",
            json!({"patch": {"inboxFolder": folder.path().to_string_lossy()}}),
        );
        wait_for("the baseline", || ledger_folder(&h).is_some());
        assert_eq!(
            h.code("delete_everything", json!({"confirmPhrase": "please"})),
            "invalid"
        );
        std::fs::write(folder.path().join("still.txt"), "still watched").unwrap();
        wait_for("it keeps watching", || kept(&h).len() == 1);
    }
}

#[test]
fn quick_capture_can_only_read_and_undo_what_it_kept_itself() {
    let h = harness();
    let other = match h
        .engine()
        .keep_text(
            "a note from the main window",
            nocturne_contract::Source::Paste,
        )
        .unwrap()
    {
        nocturne_contract::Kept::New { item } | nocturne_contract::Kept::Duplicate { item } => {
            item.id
        }
    };
    let quick = h.window("quick-capture");
    assert_eq!(
        h.code_from(&quick, "get_item", json!({"id": other})),
        "notFound"
    );
    assert_eq!(
        h.code_from(&quick, "unkeep", json!({"id": other})),
        "notFound"
    );
    let mine = h
        .call_from(
            &quick,
            "keep_text",
            json!({"text": "a note from quick capture", "source": "quickCapture"}),
        )
        .unwrap()["item"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    h.call_from(&quick, "get_item", json!({"id": mine}))
        .unwrap();
    h.call_from(&quick, "unkeep", json!({"id": mine})).unwrap();
    assert!(
        h.call("get_item", json!({"id": other})).is_ok(),
        "the main window still reads anything"
    );
}

#[test]
fn quick_capture_may_undo_a_recording_and_a_picture_it_kept() {
    let h = harness();
    let quick = h.window("quick-capture");
    let image = h
        .call_raw_from(
            &quick,
            "keep_bytes",
            vec![137, 80, 78, 71, 1, 2, 3],
            &[
                ("x-nocturne-mime", "image/png"),
                ("x-nocturne-source", "paste"),
            ],
        )
        .unwrap();
    let id = image["item"]["id"].as_str().unwrap();
    h.call_from(&quick, "get_item", json!({"id": id})).unwrap();
}

#[test]
fn checks_that_arrive_together_send_one_notification() {
    let h = harness();
    let sent = with_notifier(&h);
    h.save_todays_page();
    let handles: Vec<_> = (0..8)
        .map(|_| {
            let app = h.app.handle().clone();
            tauri::async_runtime::spawn(async move { notify::check(&app).await })
        })
        .collect();
    let results: Vec<bool> = tauri::async_runtime::block_on(async {
        let mut out = Vec::new();
        for handle in handles {
            out.push(handle.await.unwrap());
        }
        out
    });
    assert_eq!(results.iter().filter(|sent| **sent).count(), 1);
    assert_eq!(sent.0.lock().unwrap().len(), 1);
}

#[test]
fn the_notification_count_leaves_out_a_private_item_that_needs_you() {
    use nocturne_engine::{Candidate, Predicate};
    let h = harness();
    let sent = with_notifier(&h);
    let due = |subject: &str, sentence: &str| {
        let mut c = Candidate::new(subject, Predicate::Due, "x", sentence, 0.9);
        c.due = Some("2026-11-13".parse().unwrap());
        c
    };
    let (_, public) = h.learn_candidate("rent note", due("Rent", "Rent is due today."));
    let (_, secret) =
        h.learn_candidate("clinic note", due("Clinic", "A clinic visit is due today."));
    let _ = public;
    h.ok("set_private", json!({"id": secret[0], "private": true}));
    assert_eq!(
        h.engine().needs_you().unwrap().len(),
        2,
        "both need the person"
    );
    h.save_todays_page();
    h.app.emit("page-ready", json!({})).unwrap();
    wait_for("the notification", || !sent.0.lock().unwrap().is_empty());
    let body = sent.0.lock().unwrap()[0].1.clone();
    assert!(body.starts_with("One thing needs you today."), "{body}");
    assert!(!body.to_lowercase().contains("clinic"), "{body}");
}

#[test]
fn a_duplicate_kept_from_quick_capture_is_not_the_windows_to_read_or_undo() {
    let h = harness();
    let older = match h
        .engine()
        .keep_text("Gran's number ends 4471", nocturne_contract::Source::Paste)
        .unwrap()
    {
        nocturne_contract::Kept::New { item } | nocturne_contract::Kept::Duplicate { item } => {
            item.id
        }
    };
    let quick = h.window("quick-capture");
    let again = h
        .call_from(
            &quick,
            "keep_text",
            json!({"text": "Gran's number ends 4471", "source": "quickCapture"}),
        )
        .unwrap();
    assert_eq!(again["type"], "duplicate", "{again}");
    assert_eq!(again["item"]["id"], older.as_str());
    assert_eq!(
        h.code_from(&quick, "get_item", json!({"id": older})),
        "notFound"
    );
    assert_eq!(
        h.code_from(&quick, "unkeep", json!({"id": older})),
        "notFound"
    );
    assert!(h.call("get_item", json!({"id": older})).is_ok());
    assert!(h.call("unkeep", json!({"id": older})).is_ok());
}

#[test]
fn the_tray_and_the_command_pause_and_resume_the_same_way() {
    use nocturne_app_lib::state::{pause_until, resume_now};
    let h = harness();
    let app = h.app.handle().clone();
    let run = |f: std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), _>>>>| {
        tauri::async_runtime::block_on(f)
    };
    let past = run(Box::pin(pause_until(
        &app,
        Some("2020-01-01T00:00:00Z".into()),
    )));
    assert_eq!(
        past.unwrap_err().code,
        nocturne_app_lib::error::ErrorCode::Invalid
    );
    let junk = run(Box::pin(pause_until(&app, Some("soon".into()))));
    assert_eq!(
        junk.unwrap_err().code,
        nocturne_app_lib::error::ErrorCode::Invalid
    );
    run(Box::pin(pause_until(&app, None))).unwrap();
    assert_eq!(h.ok("get_status", json!({}))["state"], "paused");
    run(Box::pin(resume_now(&app))).unwrap();
    assert_ne!(h.ok("get_status", json!({}))["state"], "paused");
}
