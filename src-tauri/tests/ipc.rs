//! The shell over the Tauri mock runtime: every command through `get_ipc_response`, over a temp
//! data dir and a fixed clock. Never the person's home, the network or a real Lemonade.

mod common;

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

use common::*;
use nocturne_app_lib::COMMAND_NAMES;
use nocturne_contract::{EVENT_ITEM, EVENT_NEEDS_YOU, EVENT_STATUS};
use nocturne_engine::{Candidate, Predicate};
use serde_json::{Value, json};

const CSP: &str = "default-src 'self'; img-src 'self' asset: http://asset.localhost blob: data:; media-src 'self' asset: http://asset.localhost blob:; style-src 'self' 'unsafe-inline'; font-src 'self'; connect-src 'self' ipc: http://ipc.localhost";

/// The roadmap's command list (plan 00), plus the additions the UI asked for that the engine
/// can serve today.
const ROADMAP: &[&str] = &[
    "get_status",
    "get_setup",
    "start_setup",
    "pause_setup",
    "resume_setup",
    "keep_text",
    "keep_files",
    "keep_recording",
    "list_items",
    "get_item",
    "retry_item",
    "ask",
    "cancel_ask",
    "get_page",
    "list_pages",
    "answer_question",
    "resolve_which",
    "confirm_memory",
    "mark_done",
    "list_memories",
    "get_entity",
    "correct_memory",
    "set_keep_forever",
    "set_private",
    "restore",
    "preview_forget",
    "forget",
    "list_needs_you",
    "list_fading",
    "get_privacy",
    "export_to",
    "delete_everything",
    "get_settings",
    "update_settings",
    "pause",
    "resume",
    "get_advanced",
    "speak_page",
    "app_ready",
];

fn conf() -> Value {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tauri.conf.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

// ---- Task 1: scaffold and window ----

#[test]
fn csp_is_exactly_the_planned_string_and_nothing_remote_is_configured() {
    let c = conf();
    assert_eq!(c["app"]["security"]["csp"], CSP);
    let mut c = c;
    c.as_object_mut().unwrap().remove("$schema");
    let text = c.to_string();
    for url in text.match_indices("http") {
        let rest = &text[url.0..];
        assert!(
            rest.starts_with("http://asset.localhost")
                || rest.starts_with("http://ipc.localhost")
                || rest.starts_with("http://localhost:5173"),
            "a remote URL in the config: {}",
            &rest[..rest.len().min(40)]
        );
    }
}

#[test]
fn the_window_and_identity_follow_the_plan() {
    let c = conf();
    assert_eq!(c["identifier"], "app.nocturne.desktop");
    assert_eq!(c["productName"], "Nocturne");
    let w = &c["app"]["windows"][0];
    assert_eq!(w["label"], "main");
    assert_eq!(w["width"], 1280);
    assert_eq!(w["height"], 800);
    assert_eq!(w["minWidth"], 400);
    assert_eq!(w["minHeight"], 600);
    assert_eq!(w["title"], "Nocturne");
    assert_eq!(w["dragDropEnabled"], true);
    assert_eq!(w["visible"], false, "hidden until the interface is ready");
    assert_eq!(c["build"]["devUrl"], "http://localhost:5173");
    assert_eq!(c["build"]["frontendDist"], "../ui/dist");
    assert_eq!(c["app"]["security"]["assetProtocol"]["enable"], true);
}

#[test]
fn every_roadmap_command_is_registered() {
    let h = harness();
    for name in ROADMAP.iter().chain(COMMAND_NAMES) {
        // Whatever the arguments do, the command must exist.
        let reply = h.call(name, json!({}));
        if let Err(e) = reply {
            assert!(
                !e.as_str().is_some_and(|s| s.contains("not found")),
                "{name} is not registered: {e}"
            );
        }
    }
    let registered: BTreeSet<_> = COMMAND_NAMES.iter().collect();
    for name in ROADMAP {
        assert!(registered.contains(name), "{name} missing from the list");
    }
    assert_eq!(registered.len(), COMMAND_NAMES.len(), "no duplicates");
}

/// Plan C1: the interface's own client is the source of truth for names.
#[test]
fn every_command_the_ui_client_calls_is_registered() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../ui/src/engine/tauri.ts");
    let Ok(src) = std::fs::read_to_string(path) else {
        eprintln!("SKIPPED: ui/src/engine/tauri.ts is not in this checkout (the UI is not merged)");
        return;
    };
    let mut names = BTreeSet::new();
    for marker in ["this.call", "invoke("] {
        for (at, _) in src.match_indices(marker) {
            let rest = &src[at + marker.len()..];
            // The first quoted word after the marker, within the same call.
            let Some(open) = rest.find('"') else { continue };
            let close = rest[open + 1..].find('"').unwrap_or(0);
            let name = &rest[open + 1..open + 1 + close];
            let between = &rest[..open];
            if name.chars().all(|c| c.is_ascii_lowercase() || c == '_')
                && !name.is_empty()
                && !between.contains(';')
                && !between.contains('\n')
            {
                names.insert(name.to_string());
            }
        }
    }
    assert!(names.len() >= 30, "found too few commands: {names:?}");
    let registered: BTreeSet<_> = COMMAND_NAMES.iter().map(|s| s.to_string()).collect();
    let missing: Vec<_> = names.difference(&registered).collect();
    assert!(
        missing.is_empty(),
        "the UI calls unregistered commands: {missing:?}"
    );
    let h = harness();
    for name in &names {
        if let Err(e) = h.call(name, json!({})) {
            assert!(
                !e.as_str().is_some_and(|s| s.contains("not found")),
                "{name}"
            );
        }
    }
}

// ---- Task 2: errors and the status hub ----

#[test]
fn errors_reach_the_interface_as_a_code() {
    let h = harness();
    assert_eq!(h.code("get_item", json!({"id": "itm_nope"})), "notFound");
    assert_eq!(
        h.code("keep_text", json!({"text": "  ", "source": "paste"})),
        "invalid"
    );
    assert_eq!(
        h.code("forget", json!({"previewToken": "stale"})),
        "tokenExpired"
    );
    let e = h.call("get_item", json!({"id": "itm_nope"})).unwrap_err();
    assert!(e.is_object() && e.get("detail").is_some(), "{e}");
}

/// Review focus 1: a command that panics settles with `internal`, and the app carries on.
#[test]
fn a_panicking_command_settles_with_internal_and_the_app_survives() {
    #[tauri::command]
    async fn boom() -> Result<(), nocturne_app_lib::error::CommandError> {
        nocturne_app_lib::error::guard(async { panic!("boom") }).await
    }
    #[tauri::command]
    async fn boom_blocking() -> Result<(), nocturne_app_lib::error::CommandError> {
        nocturne_app_lib::error::blocking(|| panic!("boom")).await
    }
    let h = build(None, |b| {
        let own: Box<dyn Fn(tauri::ipc::Invoke<tauri::test::MockRuntime>) -> bool + Send + Sync> =
            Box::new(tauri::generate_handler![boom, boom_blocking]);
        let all = nocturne_app_lib::invoke_handler::<tauri::test::MockRuntime>();
        b.invoke_handler(move |invoke| {
            if matches!(invoke.message.command(), "boom" | "boom_blocking") {
                own(invoke)
            } else {
                all(invoke)
            }
        })
    });
    assert_eq!(h.code("boom", json!({})), "internal");
    assert_eq!(h.code("boom_blocking", json!({})), "internal");
    // Still working afterwards.
    h.ok(
        "keep_text",
        json!({"text": "after the panic", "source": "paste"}),
    );
}

#[test]
fn the_status_is_emitted_once_per_change() {
    let h = harness_with_mind(Arc::new(nocturne_engine::FakeMind::new()));
    let status = h.events(EVENT_STATUS);
    let first = h.ok("get_status", json!({}));
    assert_eq!(first["state"], "resting");
    h.ok("get_status", json!({}));
    assert_eq!(
        status.lock().unwrap().len(),
        1,
        "an unchanged status is not re-sent"
    );
    h.ok("keep_text", json!({"text": "a note", "source": "paste"}));
    let all = status.lock().unwrap().clone();
    assert_eq!(all.len(), 2);
    assert_eq!(all[1]["state"], "reading");
    assert_eq!(all[1]["waitingToRead"], 1);
}

#[test]
fn pausing_and_resuming_show_in_the_status() {
    let h = harness_with_mind(Arc::new(nocturne_engine::FakeMind::new()));
    h.ok("pause", json!({"until": "2026-11-13T09:00:00Z"}));
    let s = h.ok("get_status", json!({}));
    assert_eq!(s["state"], "paused");
    assert_eq!(s["pausedUntil"], "2026-11-13T09:00:00Z");
    assert_eq!(
        h.code("pause", json!({"until": "2020-01-01T00:00:00Z"})),
        "invalid"
    );
    assert_eq!(h.code("pause", json!({"until": "soon"})), "invalid");
    h.ok("resume", json!({}));
    assert_eq!(h.ok("get_status", json!({}))["state"], "resting");
    h.ok("pause", json!({"until": null}));
    assert_eq!(h.ok("get_status", json!({}))["pausedUntil"], Value::Null);
}

#[test]
fn a_paused_app_remembers_the_pause_across_a_restart() {
    let h = harness_with_mind(Arc::new(nocturne_engine::FakeMind::new()));
    h.ok("pause", json!({"until": null}));
    assert!(h.dir.path().join("app-state.json").exists());
    let again = nocturne_app_lib::AppState::open(
        h.dir.path(),
        Arc::new(nocturne_engine::ManualClock::at(
            "2026-11-13T08:00:00Z",
            "Europe/London",
        )),
    );
    assert!(
        again
            .status
            .is_paused("2026-11-13T08:00:00Z".parse().unwrap())
    );
}

#[test]
fn the_needs_you_count_is_announced_when_it_changes() {
    let h = harness_with_mind(Arc::new(nocturne_engine::FakeMind::new()));
    let counts = h.events(EVENT_NEEDS_YOU);
    h.ok("get_status", json!({}));
    let mut c = Candidate::new("Rent", Predicate::Due, "rent", "Rent is due.", 0.9);
    c.due = Some("2026-11-14".parse().unwrap());
    h.learn_candidate("rent note", c);
    h.ok("get_status", json!({}));
    assert_eq!(counts.lock().unwrap().as_slice(), &[json!(1)]);
}

/// Review focus 4: the store cannot be opened, so the window still opens and says why.
#[test]
fn an_unwritable_data_folder_shows_the_repair_notice_not_a_blank_window() {
    let h = broken_harness();
    let s = h.ok("get_status", json!({}));
    assert_eq!(s["state"], "stopped");
    assert_eq!(s["notices"][0]["code"], "repairNeeded");
    assert_eq!(h.code("get_settings", json!({})), "unavailable");
    assert_eq!(
        h.code("keep_text", json!({"text": "x", "source": "paste"})),
        "unavailable"
    );
    assert_eq!(h.code("get_page", json!({"date": null})), "unavailable");
    h.ok("app_ready", json!({}));
}

#[test]
fn a_damaged_store_shows_the_repair_notice_too() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("store")).unwrap();
    std::fs::write(dir.path().join("store/nocturne.db"), b"not a database").unwrap();
    let clock = Arc::new(nocturne_engine::ManualClock::at(
        "2026-11-13T07:00:00Z",
        "Europe/London",
    ));
    let state = nocturne_app_lib::AppState::open(dir.path(), clock);
    assert!(!state.has_engine());
    state.status.set_engine_up(false);
    let ev = state
        .status
        .evaluate(&Default::default(), "2026-11-13T07:00:00Z".parse().unwrap());
    assert_eq!(
        ev.status.notices[0].code,
        nocturne_contract::NoticeCode::RepairNeeded
    );
}

// ---- Task 3: commands ----

#[test]
fn keep_text_keeps_announces_and_dedupes() {
    let h = harness();
    let items = h.events(EVENT_ITEM);
    let kept = h.ok(
        "keep_text",
        json!({"text": "Buy oat milk", "source": "quickCapture"}),
    );
    assert_eq!(kept["type"], "new");
    assert_eq!(kept["item"]["status"], "kept");
    assert_eq!(items.lock().unwrap().len(), 1);
    let again = h.ok(
        "keep_text",
        json!({"text": "Buy oat milk", "source": "quickCapture"}),
    );
    assert_eq!(again["type"], "duplicate");
    assert_eq!(id_of(&again), id_of(&kept));
    assert_eq!(items.lock().unwrap().len(), 1, "a duplicate is not news");
    let listed = h.ok(
        "list_items",
        json!({"filter": {"since": null, "status": null, "limit": null}}),
    );
    assert_eq!(listed.as_array().unwrap().len(), 1);
    let item = h.ok("get_item", json!({"id": id_of(&kept)}));
    assert_eq!(item["title"], "Buy oat milk");
}

/// Review focus 3: keeping returns at once, whether the mind slot is empty or stuck.
#[test]
fn keep_text_returns_before_any_mind_work_even_when_the_mind_is_empty_or_stuck() {
    for mind in [
        None,
        Some(TestMind::new(AnswerMode::Never) as Arc<dyn nocturne_engine::Mind>),
    ] {
        let h = build(mind, |b| b);
        let (tx, rx) = std::sync::mpsc::channel();
        let webview = h.webview.clone();
        std::thread::spawn(move || {
            let req = json!({"text": "kept at once", "source": "quickCapture"});
            let r = tauri::test::get_ipc_response(
                &webview,
                tauri::webview::InvokeRequest {
                    cmd: "keep_text".into(),
                    callback: tauri::ipc::CallbackFn(0),
                    error: tauri::ipc::CallbackFn(1),
                    url: "tauri://localhost".parse().unwrap(),
                    body: tauri::ipc::InvokeBody::Json(req),
                    headers: Default::default(),
                    invoke_key: tauri::test::INVOKE_KEY.to_string(),
                },
            );
            let _ = tx.send(r.is_ok());
        });
        let started = std::time::Instant::now();
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)), Ok(true));
        assert!(
            started.elapsed() < Duration::from_millis(500),
            "capture took {:?}",
            started.elapsed()
        );
        let item = h.engine().items(Default::default()).unwrap();
        assert_eq!(item.len(), 1);
        assert_eq!(item[0].status, nocturne_contract::ItemStatus::Kept);
    }
}

#[test]
fn keep_files_keeps_each_file_and_reports_a_missing_one() {
    let h = harness();
    let src = tempfile::tempdir().unwrap();
    let a = write_file(src.path(), "a.txt", b"first file");
    let b = write_file(src.path(), "b.txt", b"second file");
    let kept = h.ok(
        "keep_files",
        json!({"paths": [a.to_str().unwrap(), b.to_str().unwrap()], "source": "drop"}),
    );
    assert_eq!(kept.as_array().unwrap().len(), 2);
    assert_eq!(kept[0]["item"]["kind"], "document");
    // One good, one missing: the good one is kept.
    let c = write_file(src.path(), "c.txt", b"third file");
    let mixed = h.ok(
        "keep_files",
        json!({"paths": [c.to_str().unwrap(), "/no/such/file.txt"], "source": "drop"}),
    );
    assert_eq!(mixed.as_array().unwrap().len(), 1);
    assert_eq!(
        h.code(
            "keep_files",
            json!({"paths": ["/no/such/file.txt"], "source": "drop"})
        ),
        "notFound"
    );
    assert_eq!(
        h.code("keep_files", json!({"paths": [], "source": "drop"})),
        "invalid"
    );
}

#[test]
fn keep_recording_takes_a_raw_body_and_a_mime_header() {
    let h = harness();
    let kept = h
        .call_raw(
            "keep_recording",
            vec![1, 2, 3, 4, 5],
            &[("x-nocturne-mime", "audio/webm;codecs=opus")],
        )
        .unwrap();
    assert_eq!(kept["type"], "new");
    assert_eq!(kept["item"]["kind"], "audio");
    assert_eq!(kept["item"]["source"], "recording");
    // Nothing of the temporary copy is left behind.
    let tmp = h.dir.path().join("tmp");
    assert!(!tmp.exists() || std::fs::read_dir(&tmp).unwrap().next().is_none());
    // Wrong mime, empty body, and a JSON body are refused.
    let code = |r: Result<Value, Value>| r.unwrap_err()["code"].as_str().unwrap().to_string();
    assert_eq!(
        code(h.call_raw(
            "keep_recording",
            vec![1],
            &[("x-nocturne-mime", "video/mp4")]
        )),
        "invalid"
    );
    assert_eq!(
        code(h.call_raw(
            "keep_recording",
            vec![],
            &[("x-nocturne-mime", "audio/webm")]
        )),
        "invalid"
    );
    assert_eq!(code(h.call_raw("keep_recording", vec![1], &[])), "invalid");
    assert_eq!(
        h.code(
            "keep_recording",
            json!({"bytes": [1, 2], "mime": "audio/webm"})
        ),
        "invalid"
    );
}

#[test]
fn keep_bytes_takes_any_file_with_its_source() {
    let h = harness();
    let png = vec![0x89, b'P', b'N', b'G', 1, 2, 3];
    let kept = h
        .call_raw(
            "keep_bytes",
            png,
            &[
                ("x-nocturne-mime", "image/png"),
                ("x-nocturne-source", "screenshot"),
            ],
        )
        .unwrap();
    assert_eq!(kept["item"]["kind"], "image");
    assert_eq!(kept["item"]["source"], "screenshot");
    let r = h.call_raw("keep_bytes", vec![1], &[("x-nocturne-mime", "image/png")]);
    assert_eq!(r.unwrap_err()["code"], "invalid", "the source is required");
    let r = h.call_raw(
        "keep_bytes",
        vec![1],
        &[
            ("x-nocturne-mime", "image/png"),
            ("x-nocturne-source", "nonsense"),
        ],
    );
    assert_eq!(r.unwrap_err()["code"], "invalid");
}

#[test]
fn retry_item_requeues_and_wakes_the_pipeline() {
    let h = harness();
    let kept = h.ok(
        "keep_text",
        json!({"text": "a note to retry", "source": "paste"}),
    );
    let id = id_of(&kept);
    // An item that was read is not retried: the engine leaves it alone but it exists.
    let item = h.ok("retry_item", json!({"id": id}));
    assert_eq!(item["id"], json!(id));
    assert_eq!(h.code("retry_item", json!({"id": "itm_nope"})), "notFound");
    h.ok("read_now", json!({"id": id}));
    assert_eq!(h.code("read_now", json!({"id": "itm_nope"})), "notFound");
}

#[cfg(not(feature = "engine-round2"))]
#[test]
fn pending_engine_work_is_refused_plainly_not_faked() {
    let h = harness();
    assert_eq!(h.code("unkeep", json!({"id": "itm_x"})), "unavailable");
    assert_eq!(
        h.code("set_item_private", json!({"id": "itm_x", "private": true})),
        "unavailable"
    );
    assert_eq!(
        h.code(
            "set_item_keep_forever",
            json!({"id": "itm_x", "keepForever": true})
        ),
        "unavailable"
    );
    assert_eq!(
        h.code("snooze", json!({"memoryId": "m", "days": 3})),
        "unavailable"
    );
}

#[test]
fn setup_commands_wait_for_the_runtime_lane() {
    let h = harness();
    assert_eq!(
        h.code("start_setup", json!({"tier": "standard"})),
        "unavailable"
    );
    assert_eq!(h.code("pause_setup", json!({})), "unavailable");
    assert_eq!(h.code("resume_setup", json!({})), "unavailable");
}

/// Commands that need the second round of engine work (feature `engine-round2`).
#[cfg(feature = "engine-round2")]
mod round2 {
    use super::*;

    #[test]
    fn unkeep_removes_a_fresh_item_and_is_too_late_after_the_window() {
        let h = harness();
        let fresh = h.ok(
            "keep_text",
            json!({"text": "kept by mistake", "source": "paste"}),
        );
        h.ok("unkeep", json!({"id": id_of(&fresh)}));
        assert_eq!(h.code("get_item", json!({"id": id_of(&fresh)})), "notFound");
        let old = h.ok(
            "keep_text",
            json!({"text": "kept long ago", "source": "paste"}),
        );
        h.clock.advance_days(1);
        assert_eq!(h.code("unkeep", json!({"id": id_of(&old)})), "tooLate");
    }

    #[test]
    fn item_flags_and_snooze_reach_the_engine() {
        let h = harness();
        let (item, mem) = h.learn("a note", "You like tea.", "Tea", "tea");
        h.ok("set_item_private", json!({"id": item, "private": true}));
        assert!(h.memory(&mem).private);
        h.ok(
            "set_item_keep_forever",
            json!({"id": item, "keepForever": true}),
        );
        assert!(h.memory(&mem).keep_forever);
        h.ok("snooze", json!({"memoryId": mem, "days": 3}));
        assert_eq!(
            h.code("snooze", json!({"memoryId": "mem_nope", "days": 3})),
            "notFound"
        );
    }

    #[test]
    fn read_now_returns_the_item() {
        let h = harness();
        let kept = h.ok(
            "keep_text",
            json!({"text": "read me first", "source": "paste"}),
        );
        let item = h.ok("read_now", json!({"id": id_of(&kept)}));
        assert_eq!(item["id"], json!(id_of(&kept)));
    }
}

#[test]
fn the_page_commands_serve_pages_and_the_days_question() {
    let h = harness();
    // Nothing saved: the page being written is composed on the spot.
    let page = h.ok("get_page", json!({"date": null}));
    assert_eq!(page["date"], "2026-11-13");
    assert_eq!(h.ok("list_pages", json!({})).as_array().unwrap().len(), 0);
    assert_eq!(
        h.code("get_page", json!({"date": "2026-10-01"})),
        "notFound"
    );
    assert_eq!(h.code("get_page", json!({"date": "last week"})), "invalid");

    let mut c = Candidate::new(
        "Rent",
        Predicate::Due,
        "rent",
        "Rent is due on Saturday.",
        0.9,
    );
    c.due = Some("2026-11-14".parse().unwrap());
    let (_, created) = h.learn_candidate("rent note", c);
    let mem = created[0].clone();
    let saved = h.save_todays_page();
    assert_eq!(saved.needs_you.len(), 1);
    let again = h.ok("get_page", json!({"date": "2026-11-13"}));
    assert!(again.to_string().contains("Rent is due on Saturday"));
    let pages = h.ok("list_pages", json!({}));
    assert_eq!(pages[0]["date"], "2026-11-13");
    assert_eq!(pages[0]["needsYou"], 1);

    h.ok("mark_done", json!({"memoryId": mem}));
    let after = h.ok("get_page", json!({"date": "2026-11-13"}));
    assert_eq!(after["needsYou"][0]["done"], true);
    assert_eq!(
        h.code("mark_done", json!({"memoryId": "mem_nope"})),
        "notFound"
    );
    // Page one asks about the page time: yes does not fit, right does, and only once.
    assert_eq!(
        h.code(
            "answer_question",
            json!({"pageDate": "2026-11-13", "choice": "yes"})
        ),
        "invalid"
    );
    h.ok(
        "answer_question",
        json!({"pageDate": "2026-11-13", "choice": "right"}),
    );
    assert_eq!(
        h.code(
            "answer_question",
            json!({"pageDate": "2026-11-13", "choice": "right"})
        ),
        "invalid"
    );
    assert_eq!(
        h.code(
            "answer_question",
            json!({"pageDate": "2026-10-01", "choice": "right"})
        ),
        "notFound"
    );
    assert_eq!(
        h.code(
            "answer_question",
            json!({"pageDate": "yesterday", "choice": "yes"})
        ),
        "invalid"
    );
}

#[test]
fn resolve_which_says_what_was_settled() {
    let h = harness();
    let mk = |obj: &str, sentence: &str| {
        Candidate::new(
            "Dentist appointment",
            Predicate::Scheduled,
            obj,
            sentence,
            0.9,
        )
    };
    h.learn_candidate("d1", mk("Tuesday", "Dentist is Tuesday."));
    let (_, created) = h.learn_candidate("d2", mk("Thursday", "Dentist is Thursday."));
    let thursday = created[0].clone();
    let needs = h.ok("list_needs_you", json!({}));
    let which = &needs[0];
    assert_eq!(which["type"], "which");
    let conflict = which["conflictId"].as_str().unwrap();
    let option = which["options"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["memoryId"] == json!(thursday))
        .unwrap();
    let resolved = h.ok(
        "resolve_which",
        json!({"conflictId": conflict, "memoryId": thursday}),
    );
    assert_eq!(resolved["subject"], "Dentist appointment");
    assert_eq!(resolved["chosen"], option["text"]);
    assert_eq!(resolved["memoryId"], json!(thursday));
    assert_eq!(
        h.ok("list_needs_you", json!({})).as_array().unwrap().len(),
        0
    );
    assert_eq!(
        h.code(
            "resolve_which",
            json!({"conflictId": conflict, "memoryId": thursday})
        ),
        "invalid"
    );
}

#[test]
fn memory_commands_browse_and_change() {
    let h = harness();
    let mut c = Candidate::new(
        "Coffee",
        Predicate::Prefers,
        "oat flat white",
        "You like oat flat whites.",
        0.9,
    );
    c.subject_kind = Some(nocturne_contract::EntityKind::Thing);
    let (_, created) = h.learn_candidate("coffee note", c);
    let mem = created[0].clone();
    let all = h.ok("list_memories", json!({"filter": {"entityId": null, "itemId": null, "status": null, "query": null, "limit": null}}));
    assert_eq!(all.as_array().unwrap().len(), 1);
    let found = h.ok("list_memories", json!({"filter": {"entityId": null, "itemId": null, "status": null, "query": "flat", "limit": null}}));
    assert_eq!(found.as_array().unwrap().len(), 1);

    h.ok("set_keep_forever", json!({"id": mem, "keepForever": true}));
    assert!(h.memory(&mem).keep_forever);
    h.ok("set_private", json!({"id": mem, "private": true}));
    assert!(h.memory(&mem).private);
    assert_eq!(
        h.code("set_private", json!({"id": "mem_nope", "private": true})),
        "notFound"
    );

    let fixed = h.ok("correct_memory", json!({"id": mem, "text": "oat cortado"}));
    assert_ne!(fixed["id"], json!(mem));
    assert!(fixed["sentence"].as_str().unwrap().contains("oat cortado"));
    assert_eq!(
        h.code("correct_memory", json!({"id": fixed["id"], "text": "  "})),
        "invalid"
    );

    assert_eq!(h.ok("list_fading", json!({})).as_array().unwrap().len(), 0);
    h.ok("restore", json!({"ids": [fixed["id"]]}));
    h.ok(
        "confirm_memory",
        json!({"id": fixed["id"], "confirmed": true}),
    );

    let entities = h.ok("list_entities", json!({"kind": null, "query": null}));
    assert!(!entities.as_array().unwrap().is_empty());
    let id = entities[0]["id"].as_str().unwrap().to_string();
    assert_eq!(h.ok("get_entity", json!({"id": id}))["id"], json!(id));
    assert_eq!(h.code("get_entity", json!({"id": "ent_nope"})), "notFound");
    let none = h.ok("list_entities", json!({"kind": null, "query": "zzz"}));
    assert_eq!(none.as_array().unwrap().len(), 0);
}

/// Review focus 5: forget, then get_page never returns the forgotten text, through IPC alone.
#[test]
fn a_forgotten_memory_never_comes_back_in_a_page_or_the_files() {
    const SECRET: &str = "zebrafish47";
    let h = harness();
    let mut c = Candidate::new(
        "Dr Zebrafish47",
        Predicate::Due,
        "form",
        "Hand the Zebrafish47 form in on Saturday.",
        0.9,
    );
    c.due = Some("2026-11-14".parse().unwrap());
    let (_, created) = h.learn_candidate("the zebrafish47 note", c);
    let mem = created[0].clone();
    h.save_todays_page();
    assert!(
        h.ok("get_page", json!({"date": "2026-11-13"}))
            .to_string()
            .to_lowercase()
            .contains(SECRET)
    );

    let preview = h.ok(
        "preview_forget",
        json!({"ids": [mem], "includeOriginals": true}),
    );
    assert_eq!(preview["memories"].as_array().unwrap().len(), 1);
    assert_eq!(preview["pagesAffected"], 1);
    let report = h.ok("forget", json!({"previewToken": preview["token"]}));
    assert_eq!(report["memoriesRemoved"], 1);

    for date in [json!("2026-11-13"), Value::Null] {
        let page = h.ok("get_page", json!({"date": date}));
        assert!(!page.to_string().to_lowercase().contains(SECRET), "{page}");
    }
    assert!(
        !h.ok("list_needs_you", json!({}))
            .to_string()
            .to_lowercase()
            .contains(SECRET)
    );
    assert!(!h
        .ok("list_memories", json!({"filter": {"entityId": null, "itemId": null, "status": null, "query": SECRET, "limit": null}}))
        .to_string()
        .to_lowercase()
        .contains(SECRET));
    // The token is single use.
    assert_eq!(
        h.code("forget", json!({"previewToken": preview["token"]})),
        "tokenExpired"
    );
}

#[test]
fn settings_read_and_write_with_validation() {
    let h = harness();
    let s = h.ok("get_settings", json!({}));
    assert_eq!(s["pageTime"], "07:00");
    let patch = json!({"patch": {
        "pageTime": "06:30", "delivery": null, "autoPrint": null, "voice": null, "voiceSpeed": null,
        "shortcut": null, "inboxFolder": null, "startAtLogin": null, "lettingGo": "brisk",
        "theme": null, "language": null, "checkUpdates": null, "reminders": null, "lock": null, "agents": null
    }});
    let s = h.ok("update_settings", patch);
    assert_eq!(s["pageTime"], "06:30");
    assert_eq!(s["lettingGo"], "brisk");
    assert_eq!(h.ok("get_settings", json!({}))["pageTime"], "06:30");
    let bad = json!({"patch": {"pageTime": "25:99"}});
    assert_eq!(h.code("update_settings", bad), "invalid");
}

#[test]
fn privacy_and_advanced_report_what_the_shell_knows() {
    let h = harness();
    h.ok(
        "keep_text",
        json!({"text": "something kept", "source": "paste"}),
    );
    let p = h.ok("get_privacy", json!({}));
    assert_eq!(p["sentToInternetBytes"], 0.0);
    assert_eq!(p["sentMeasured"], false);
    assert_eq!(p["dataDir"], json!(h.dir.path().to_string_lossy()));
    assert!(p["keptBytes"].as_f64().unwrap() > 0.0);
    h.state().status.set_sent(1234.0, true);
    assert_eq!(
        h.ok("get_privacy", json!({}))["sentToInternetBytes"],
        1234.0
    );
    let a = h.ok("get_advanced", json!({}));
    assert_eq!(a["itemCount"], 1);
    assert_eq!(a["integrityOk"], true);
    assert!(a["logDir"].as_str().unwrap().ends_with("logs"));
}

#[test]
fn export_writes_a_fresh_folder_and_reports_it() {
    let h = harness();
    h.learn("a note", "You like tea.", "Tea", "tea");
    let out = tempfile::tempdir().unwrap();
    let progress = h.events(nocturne_contract::EVENT_EXPORT);
    let report = h.ok("export_to", json!({"dir": out.path().to_str().unwrap()}));
    assert_eq!(report["memories"], 1);
    let path = std::path::PathBuf::from(report["path"].as_str().unwrap());
    assert!(path.starts_with(out.path()) && path.is_dir());
    assert!(
        !progress.lock().unwrap().is_empty(),
        "export progress is announced"
    );
    assert_eq!(
        h.code("export_to", json!({"dir": "/no/such/folder"})),
        "invalid"
    );
}

#[test]
fn delete_everything_needs_the_exact_phrase() {
    let h = harness_with_mind(Arc::new(nocturne_engine::FakeMind::new()));
    h.learn("a note", "You like tea.", "Tea", "tea");
    h.ok("pause", json!({"until": null}));
    assert_eq!(
        h.code("delete_everything", json!({"confirmPhrase": "please"})),
        "invalid"
    );
    assert_eq!(h.ok("get_advanced", json!({}))["memoryCount"], 1);
    h.ok(
        "delete_everything",
        json!({"confirmPhrase": "delete everything"}),
    );
    let a = h.ok("get_advanced", json!({}));
    assert_eq!(a["memoryCount"], 0);
    assert_eq!(a["itemCount"], 0);
    assert!(
        !h.dir.path().join("app-state.json").exists(),
        "the pause goes too"
    );
    assert_eq!(h.ok("get_status", json!({}))["state"], "resting");
}

#[test]
fn speak_page_makes_the_spoken_edition_once_and_never_speaks_private_lines() {
    let h = harness_with_mind(Arc::new(nocturne_engine::FakeMind::new()));
    let mut c = Candidate::new(
        "Rent",
        Predicate::Due,
        "rent",
        "Rent is due on Saturday.",
        0.9,
    );
    c.due = Some("2026-11-14".parse().unwrap());
    let (_, created) = h.learn_candidate("rent note", c);
    let mut c2 = Candidate::new(
        "Clinic",
        Predicate::Due,
        "x",
        "The clinic wants a visit on Sunday.",
        0.9,
    );
    c2.due = Some("2026-11-15".parse().unwrap());
    let (_, created2) = h.learn_candidate("clinic note", c2);
    h.save_todays_page();
    h.ok("set_private", json!({"id": created2[0], "private": true}));
    let _ = created;
    let path = h.ok("speak_page", json!({"date": "2026-11-13"}));
    let spoken = std::fs::read_to_string(path.as_str().unwrap()).unwrap();
    assert!(spoken.contains("Rent is due on Saturday."), "{spoken}");
    assert!(
        !spoken.contains("clinic"),
        "a private line was spoken: {spoken}"
    );
    let again = h.ok("speak_page", json!({"date": "2026-11-13"}));
    assert_eq!(again, path, "kept, not made again");
    assert_eq!(
        h.code("speak_page", json!({"date": "2026-01-01"})),
        "notFound"
    );
    let quiet = harness();
    assert_eq!(
        quiet.code("speak_page", json!({"date": "2026-11-13"})),
        "notFound"
    );
}

#[test]
fn speak_page_without_a_mind_is_unavailable() {
    let h = harness();
    h.save_todays_page();
    assert_eq!(
        h.code("speak_page", json!({"date": "2026-11-13"})),
        "unavailable"
    );
}

#[test]
fn the_setup_view_reflects_the_mind_slot() {
    let h = harness();
    assert_eq!(h.ok("get_setup", json!({}))["phase"], "notStarted");
    h.state()
        .mind
        .set(Arc::new(nocturne_engine::FakeMind::new()));
    assert_eq!(h.ok("get_setup", json!({}))["phase"], "done");
}

// ---- second launch and tray drops belong to the shell ----

#[test]
fn a_second_launch_keeps_the_files_it_was_given_and_focuses_the_window() {
    let h = harness();
    let items = h.events(EVENT_ITEM);
    let src = tempfile::tempdir().unwrap();
    let f = write_file(src.path(), "forwarded.txt", b"sent by the second launch");
    let args = vec![
        "nocturne".to_string(),
        "--flag".to_string(),
        f.to_string_lossy().into_owned(),
        "/no/such/file".to_string(),
    ];
    assert_eq!(nocturne_app_lib::paths_from_args(&args), vec![f.clone()]);
    let handle = h.app.handle().clone();
    tauri::async_runtime::block_on(nocturne_app_lib::handle_second_launch(&handle, args));
    assert_eq!(items.lock().unwrap().len(), 1);
    assert_eq!(
        h.engine().items(Default::default()).unwrap()[0].source,
        nocturne_contract::Source::Drop
    );
}
