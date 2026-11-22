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
use tauri::Manager;

const CSP: &str = "default-src 'self'; img-src 'self' asset: http://asset.localhost blob: data:; media-src 'self' asset: http://asset.localhost blob:; style-src 'self' 'unsafe-inline'; font-src 'self'; connect-src 'self' ipc: http://ipc.localhost asset: http://asset.localhost";

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
    "add_answer_line",
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
    wait_for("the status after the keep", || {
        status.lock().unwrap().len() == 2
    });
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
                    url: APP_URL.parse().unwrap(),
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
    h.state().allow_paths([a.clone(), b.clone()]);
    let kept = h.ok(
        "keep_files",
        json!({"paths": [a.to_str().unwrap(), b.to_str().unwrap()], "source": "drop"}),
    );
    assert_eq!(kept.as_array().unwrap().len(), 2);
    assert_eq!(kept[0]["item"]["kind"], "document");
    // One good, one missing: the good one is kept.
    let c = write_file(src.path(), "c.txt", b"third file");
    h.state()
        .allow_paths([c.clone(), "/no/such/file.txt".into()]);
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
    // A path nobody dropped or picked is refused, whatever it is.
    let stranger = write_file(src.path(), "d.txt", b"never chosen");
    assert_eq!(
        h.code(
            "keep_files",
            json!({"paths": [stranger.to_str().unwrap()], "source": "drop"})
        ),
        "invalid"
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
            &[("x-nocturne-mime", "audio/wav")],
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
    // What a web view records natively is refused up front: the speech model cannot read it.
    for mime in [
        "audio/webm",
        "audio/webm;codecs=opus",
        "audio/mp4",
        "audio/ogg",
    ] {
        assert_eq!(
            code(h.call_raw("keep_recording", vec![1, 2], &[("x-nocturne-mime", mime)])),
            "invalid",
            "{mime}"
        );
    }
    // Nothing was kept by those.
    assert_eq!(
        h.ok(
            "list_items",
            json!({"filter": {"since": null, "status": null, "limit": null}})
        )
        .as_array()
        .unwrap()
        .len(),
        1
    );
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
            &[("x-nocturne-mime", "audio/wav")]
        )),
        "invalid"
    );
    assert_eq!(code(h.call_raw("keep_recording", vec![1], &[])), "invalid");
    assert_eq!(
        h.code(
            "keep_recording",
            json!({"bytes": [1, 2], "mime": "audio/wav"})
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

#[test]
fn setup_commands_wait_for_the_runtime_lane() {
    let h = harness();
    assert_eq!(
        h.code("start_setup", json!({"tier": "standard"})),
        "unavailable"
    );
    assert_eq!(h.code("pause_setup", json!({})), "unavailable");
    assert_eq!(h.code("resume_setup", json!({})), "unavailable");
    assert_eq!(h.code("retry_engine", json!({})), "unavailable");
}

/// Commands built on the second round of engine work.
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
fn a_line_is_added_only_to_an_answered_question_from_the_journal() {
    let h = harness();
    assert_eq!(
        h.code(
            "add_answer_line",
            json!({"pageDate": "2026-11-13", "line": "Went well"})
        ),
        "notFound",
        "no question from the journal that day"
    );
    assert_eq!(
        h.code(
            "add_answer_line",
            json!({"pageDate": "2026-11-13", "line": "   "})
        ),
        "invalid"
    );
    assert_eq!(
        h.code(
            "add_answer_line",
            json!({"pageDate": "yesterday", "line": "Went well"})
        ),
        "invalid"
    );
    // The page-time question of page one is not one from the journal.
    h.save_todays_page();
    assert_eq!(
        h.code(
            "add_answer_line",
            json!({"pageDate": "2026-11-13", "line": "Went well"})
        ),
        "notFound"
    );
    // And a journal choice does not fit it.
    assert_eq!(
        h.code(
            "answer_question",
            json!({"pageDate": "2026-11-13", "choice": "fine"})
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
    assert_eq!(fixed["sentence"], json!("oat cortado"));
    let again = h.ok(
        "correct_memory",
        json!({"id": fixed["id"], "text": "oat flat white"}),
    );
    assert_eq!(again["sentence"], json!("oat flat white"));
    assert_eq!(again["was"], json!("oat cortado"));
    let fixed = again;
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

#[test]
fn entities_are_renamed_merged_and_memories_refiled() {
    let h = harness();
    let claim = |subject: &str, object: &str, sentence: &str| {
        let mut c = Candidate::new(subject, Predicate::Knows, object, sentence, 0.9);
        c.subject_kind = Some(nocturne_contract::EntityKind::Thing);
        c
    };
    let (_, a) = h.learn_candidate(
        "trinks note",
        claim("Trinks", "5 pounds", "Trinks are 5 pounds."),
    );
    let (_, b) = h.learn_candidate(
        "drinks note",
        claim("Drinks", "in the fridge", "Drinks are in the fridge."),
    );
    let (a, b) = (a[0].clone(), b[0].clone());
    let ea = h.memory(&a).entity_id.unwrap();
    let eb = h.memory(&b).entity_id.unwrap();

    let out = h.ok("rename_entity", json!({"id": ea, "name": "Drinks"}));
    assert_eq!(out["type"], json!("exists"));
    assert_eq!(out["entity"]["id"], json!(eb));
    let out = h.ok("rename_entity", json!({"id": ea, "name": "Pop"}));
    assert_eq!(out["type"], json!("renamed"));
    assert_eq!(out["entity"]["name"], json!("Pop"));
    assert_eq!(out["entity"]["formerNames"], json!(["Trinks"]));
    let change = out["changeId"].as_str().unwrap().to_string();
    assert_eq!(h.memory(&a).sentence, "Pop are 5 pounds.");
    h.ok("undo_change", json!({"changeId": change}));
    assert_eq!(h.memory(&a).sentence, "Trinks are 5 pounds.");
    assert_eq!(
        h.code("undo_change", json!({"changeId": change})),
        "invalid"
    );
    assert_eq!(
        h.code("rename_entity", json!({"id": ea, "name": "  "})),
        "invalid"
    );
    assert_eq!(
        h.code("rename_entity", json!({"id": "ent_nope", "name": "X"})),
        "notFound"
    );

    let moved = h.ok(
        "refile_memory",
        json!({"id": a, "target": {"type": "existing", "entityId": eb}}),
    );
    assert_eq!(moved["memory"]["subject"], json!("Drinks"));
    assert_eq!(moved["memory"]["movedFrom"], json!("Trinks"));
    assert_eq!(moved["entity"]["memoryCount"], json!(2));
    h.ok(
        "undo_change",
        json!({"changeId": moved["changeId"].as_str().unwrap()}),
    );
    assert_eq!(h.memory(&a).entity_id.as_deref(), Some(ea.as_str()));
    let fresh = h.ok(
        "refile_memory",
        json!({"id": a, "target": {"type": "new", "name": "Cellar", "kind": "place"}}),
    );
    assert_eq!(fresh["entity"]["kind"], json!("place"));

    h.ok(
        "undo_change",
        json!({"changeId": fresh["changeId"].as_str().unwrap()}),
    );
    let merged = h.ok("merge_entities", json!({"fromId": ea, "intoId": eb}));
    assert_eq!(merged["moved"], json!(1));
    assert_eq!(merged["entity"]["memoryCount"], json!(2));
    assert_eq!(h.code("get_entity", json!({"id": ea})), "notFound");
    assert_eq!(
        h.code("merge_entities", json!({"fromId": eb, "intoId": eb})),
        "invalid"
    );
    h.ok(
        "undo_change",
        json!({"changeId": merged["changeId"].as_str().unwrap()}),
    );
    assert_eq!(
        h.ok("get_entity", json!({"id": ea}))["name"],
        json!("Trinks")
    );
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

/// The interface says which pace is the default and the mock starts on it; both follow the
/// engine's real default, so a change there cannot leave the label telling the wrong story.
#[test]
fn the_settings_screen_names_the_engines_default_pace() {
    let h = harness();
    let real = h.ok("get_settings", json!({}))["lettingGo"]
        .as_str()
        .unwrap()
        .to_string();
    let ui = |rel: &str| {
        std::fs::read_to_string(format!("{}/../ui/{rel}", env!("CARGO_MANIFEST_DIR")))
            .unwrap_or_else(|e| panic!("{rel} must exist: {e}"))
    };
    let copy: Value = serde_json::from_str(&ui("src/copy/en-GB/settings.json")).unwrap();
    let labelled: Vec<&str> = copy
        .as_object()
        .unwrap()
        .iter()
        .filter(|(k, v)| {
            k.starts_with("settings.letting.")
                && v.as_str().is_some_and(|v| v.contains("(default)"))
        })
        .map(|(k, _)| k.trim_start_matches("settings.letting."))
        .collect();
    assert_eq!(labelled, [real.as_str()], "the pace marked as the default");
    assert!(
        ui("src/engine/mock/seed/settings.ts").contains(&format!("lettingGo: \"{real}\"")),
        "the mock starts on the engine's default pace"
    );
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
fn get_edition_serves_a_saved_page_in_the_edition_asked_for() {
    let h = harness();
    h.save_todays_page();
    let print = h.ok(
        "get_edition",
        json!({"date": "2026-11-13", "edition": "print"}),
    );
    assert_eq!(print["edition"], "print");
    assert_eq!(
        h.code(
            "get_edition",
            json!({"date": "2026-01-01", "edition": "audio"})
        ),
        "notFound"
    );
    assert_eq!(
        h.code("get_edition", json!({"date": "soon", "edition": "audio"})),
        "invalid"
    );
}

#[test]
fn print_page_asks_the_window_to_print() {
    let h = harness();
    h.ok("print_page", json!({}));
}

#[test]
fn quick_capture_window_cannot_delete() {
    let h = harness();
    let quick = h.window("quick-capture");
    let phrase = json!({"confirmPhrase": "delete everything"});
    assert_eq!(h.code_from(&quick, "delete_everything", phrase), "invalid");
    assert_eq!(
        h.code_from(&quick, "forget", json!({"token": "x"})),
        "invalid"
    );
    assert_eq!(
        h.code_from(&quick, "update_settings", json!({"patch": {}})),
        "invalid"
    );
    let kept = h
        .call_from(
            &quick,
            "keep_text",
            json!({"text": "milk is on the list", "source": "quickCapture"}),
        )
        .unwrap();
    assert!(kept["item"]["id"].is_string());
}

/// Away from home's commands belong to the main window only.
#[test]
fn remote_commands_refused_from_quick_capture() {
    let h = harness();
    let quick = h.window("quick-capture");
    for cmd in [
        "get_remote",
        "start_remote",
        "stop_remote",
        "pause_remote_download",
        "begin_pairing",
        "decide_pairing",
        "cancel_pairing",
        "rename_device",
        "remove_device",
        "list_remote_events",
        "update_remote_settings",
        "open_remote_sign_in",
        "new_computer_key",
        "get_move",
        "answer_move",
        "finish_move",
    ] {
        assert_eq!(h.code_from(&quick, cmd, json!({})), "invalid", "{cmd}");
    }
}

/// A mistyped phrase deletes nothing and runs no Away from home hook (once it does real work,
/// a hook would stop the listener and sign out); the right phrase runs both, in order.
#[test]
fn a_wrong_phrase_runs_no_remote_hook() {
    use nocturne_app_lib::remote::HookLog;
    let h = build(None, |b| b.manage(HookLog::default()));
    assert_eq!(
        h.code(
            "delete_everything",
            json!({"confirmPhrase": "delete it all"})
        ),
        "invalid"
    );
    assert!(h.app.state::<HookLog>().calls().is_empty());
    h.ok(
        "delete_everything",
        json!({"confirmPhrase": nocturne_engine::DELETE_PHRASE}),
    );
    assert_eq!(
        h.app.state::<HookLog>().calls(),
        ["before_delete_everything", "on_delete_everything"]
    );
}

/// A fresh store has Away from home off: the default connection, nothing paired, nothing sent,
/// and no folder of its own.
#[test]
fn get_remote_is_off_on_a_fresh_store() {
    let h = harness();
    let v = h.ok("get_remote", json!({}));
    assert_eq!(v["state"], json!({"type": "off"}));
    assert_eq!(v["connection"], "simple");
    assert_eq!(v["autoOff"], "unused30Days");
    assert_eq!(v["keepAwake"], false);
    assert_eq!(v["devices"], json!([]));
    assert_eq!(v["sentToPhonesBytes"], 0.0);
    assert_eq!(v["pairing"], Value::Null);
    assert_eq!(v["setUpBefore"], false);
    assert!(!h.dir.path().join("remote").exists());
    let privacy = h.ok("get_privacy", json!({}));
    assert_eq!(privacy["remoteOn"], false);
    assert_eq!(h.ok("get_advanced", json!({}))["remote"], Value::Null);
}

#[test]
fn unknown_label_refused() {
    let h = harness();
    let other = h.window("somewhere-else");
    for cmd in ["get_status", "keep_text", "app_ready"] {
        assert_eq!(
            h.code_from(&other, cmd, json!({"text": "x", "source": "paste"})),
            "invalid",
            "{cmd}"
        );
    }
}

#[test]
fn app_ready_from_quick_capture_keeps_main_hidden() {
    let h = harness();
    let quick = h.window("quick-capture");
    h.call_from(&quick, "app_ready", json!({})).unwrap();
    assert!(!h.state().window_shown(), "main must stay hidden");
    h.ok("app_ready", json!({}));
    assert!(h.state().window_shown());
}

#[test]
fn report_recording_takes_the_four_states_and_refuses_the_rest() {
    let h = harness();
    for state in ["available", "not-secure", "no-devices", "no-recorder"] {
        h.ok("report_recording", json!({ "state": state }));
    }
    assert_eq!(
        h.code("report_recording", json!({ "state": "something else" })),
        "invalid"
    );
}

#[test]
fn quick_capture_may_not_report_recording() {
    let h = harness();
    let quick = h.window("quick-capture");
    assert_eq!(
        h.code_from(&quick, "report_recording", json!({ "state": "available" })),
        "invalid"
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
    let cwd = "/".to_string();
    assert_eq!(
        nocturne_app_lib::paths_from_args(&args, &cwd),
        vec![f.clone()]
    );
    let handle = h.app.handle().clone();
    tauri::async_runtime::block_on(nocturne_app_lib::handle_second_launch(&handle, args, cwd));
    assert_eq!(items.lock().unwrap().len(), 1);
    assert!(h.state().window_shown(), "the window comes first");
    assert_eq!(
        h.engine().items(Default::default()).unwrap()[0].source,
        nocturne_contract::Source::Drop
    );
}

#[test]
fn a_second_launch_reads_relative_paths_and_file_links_properly() {
    let src = tempfile::tempdir().unwrap();
    write_file(src.path(), "my note.txt", b"x");
    write_file(src.path(), "plain.txt", b"y");
    let cwd = src.path().to_string_lossy().into_owned();
    let args: Vec<String> = ["nocturne", "plain.txt", "file:///no/such%20thing.txt"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    assert_eq!(
        nocturne_app_lib::paths_from_args(&args, &cwd),
        vec![src.path().join("plain.txt")],
        "relative to the second launch's own folder"
    );
    let link = format!("file://{}/my%20note.txt", src.path().display());
    let args = vec!["nocturne".to_string(), link];
    assert_eq!(
        nocturne_app_lib::paths_from_args(&args, "/"),
        vec![src.path().join("my note.txt")],
        "a file link is decoded"
    );
}

#[test]
fn a_second_launch_that_keeps_nothing_says_so() {
    let h = harness_with_mind(Arc::new(nocturne_engine::FakeMind::new()));
    let args = vec!["nocturne".to_string(), "/no/such/file".to_string()];
    let handle = h.app.handle().clone();
    tauri::async_runtime::block_on(nocturne_app_lib::handle_second_launch(
        &handle,
        args,
        "/".into(),
    ));
    let status = h.ok("get_status", json!({}));
    assert!(
        status["notices"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n["id"] == "not-kept"),
        "{status}"
    );
    // Keeping something later clears it.
    h.ok("keep_text", json!({"text": "later", "source": "paste"}));
    let status = h.ok("get_status", json!({}));
    assert!(status["notices"].as_array().unwrap().is_empty(), "{status}");
}

// ---- least privilege: what the interface may reach ----

#[test]
fn the_asset_scope_reaches_originals_and_nothing_beside_them() {
    let h = harness();
    let kept = h
        .call_raw(
            "keep_bytes",
            vec![0x89, b'P', b'N', b'G', 9, 9],
            &[
                ("x-nocturne-mime", "image/png"),
                ("x-nocturne-source", "screenshot"),
            ],
        )
        .unwrap();
    let item = h.ok("get_item", json!({"id": id_of(&kept)}));
    let original = std::path::PathBuf::from(item["originalPath"].as_str().unwrap());
    let scope = h.app.asset_protocol_scope();
    assert!(scope.is_allowed(&original), "an original may be shown");
    let store = dunce::canonicalize(h.dir.path()).unwrap().join("store");
    assert!(
        !scope.is_allowed(store.join("nocturne.db")),
        "the database is not an original"
    );
    assert!(
        !scope.is_allowed(store.join("originals").join("..").join("nocturne.db")),
        "a .. path does not climb out"
    );
    assert!(
        !scope.is_allowed(store.join("other").join("a.png")),
        "a sibling folder is out"
    );
    assert!(!scope.is_allowed(h.dir.path().join("app-state.json")));
    assert!(!scope.is_allowed("/etc/passwd"));
}

fn capabilities() -> Value {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/capabilities/default.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn granted() -> BTreeSet<String> {
    capabilities()["permissions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p.as_str().unwrap().to_string())
        .collect()
}

#[test]
fn the_main_window_holds_least_privilege() {
    let c = capabilities();
    assert_eq!(c["windows"], json!(["main"]));
    let granted = granted();
    for wanted in [
        "core:event:default",
        "core:window:default",
        "core:webview:default",
        "core:app:default",
    ] {
        assert!(granted.contains(wanted), "{wanted} is missing");
    }
    assert!(
        !granted.contains("core:default"),
        "core:default is broader than needed"
    );
    for p in &granted {
        assert!(
            p.starts_with("core:"),
            "{p}: no plugin permission is granted; files go through the app's commands"
        );
    }
}

/// Every plugin call and window method in the platform adapter has its permission.
#[test]
fn the_capability_covers_exactly_what_the_platform_adapter_calls() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../ui/src/platform/tauri.ts");
    let src = std::fs::read_to_string(path).expect("ui/src/platform/tauri.ts must exist");
    let granted = granted();
    // Plugin IPC calls: `plugin:name|command` needs `name:allow-command`.
    for (at, _) in src.match_indices("plugin:") {
        let rest = &src[at + "plugin:".len()..];
        let end = rest.find('"').unwrap();
        let (plugin, command) = rest[..end].split_once('|').unwrap();
        let permission = format!("{plugin}:allow-{}", command.replace('_', "-"));
        assert!(
            granted.contains(&permission),
            "the adapter calls {plugin}|{command}: {permission} is not granted"
        );
    }
    // Window and event methods the adapter uses, and the permission each needs.
    let needs: &[(&str, &str)] = &[
        ("emitTo(", "core:event:default"),
        ("listen<", "core:event:default"),
        ("onDragDropEvent(", "core:event:default"),
        ("onThemeChanged(", "core:event:default"),
        (".theme()", "core:window:default"),
        (".show()", "core:window:allow-show"),
        (".setFocus()", "core:window:allow-set-focus"),
        (".close()", "core:window:allow-close"),
    ];
    // `close` and the hand-off belong to the quick-capture window, which has its own
    // capability file; the main window needs the rest.
    let quick_capture_only = [
        "core:window:allow-close",
        "core:window:allow-show",
        "core:window:allow-set-focus",
    ];
    for (call, permission) in needs {
        if src.contains(call) && !quick_capture_only.contains(permission) {
            assert!(granted.contains(*permission), "{call} needs {permission}");
        }
    }
    // The main window itself shows, hides and focuses (tray, second launch, ready).
    for p in [
        "core:window:allow-show",
        "core:window:allow-hide",
        "core:window:allow-set-focus",
    ] {
        assert!(granted.contains(p), "{p}");
    }
}

#[test]
fn the_app_defaults_to_no_scripted_mind() {
    let cargo =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml")).unwrap();
    assert!(
        cargo.contains("default = []"),
        "fake-mind must not be on by default"
    );
    assert!(!cargo.contains("engine-round2"));
}

// ---- keeping things: what is accepted ----

#[test]
fn raw_content_over_the_limit_is_refused_before_it_is_copied() {
    let h = harness();
    let big = vec![0u8; nocturne_app_lib::commands::MAX_RAW_BYTES + 1];
    let r = h.call_raw(
        "keep_bytes",
        big,
        &[
            ("x-nocturne-mime", "image/png"),
            ("x-nocturne-source", "paste"),
        ],
    );
    assert_eq!(r.unwrap_err()["code"], "invalid");
    assert_eq!(h.engine().items(Default::default()).unwrap().len(), 0);
}

#[test]
fn pasted_bytes_leave_no_trace_of_the_data_folder_in_the_item() {
    let h = harness();
    let kept = h
        .call_raw(
            "keep_bytes",
            vec![1, 2, 3],
            &[
                ("x-nocturne-mime", "image/png"),
                ("x-nocturne-source", "paste"),
            ],
        )
        .unwrap();
    let item = h.ok("get_item", json!({"id": id_of(&kept)}));
    let dir = h.dir.path().to_string_lossy().into_owned();
    assert!(!item["title"].as_str().unwrap().contains(&dir));
    assert!(!item["fileName"].as_str().unwrap_or("").contains("tmp"));
    assert!(
        !h.dir.path().join("tmp").exists(),
        "no temporary copy is made"
    );
}

#[test]
fn only_regular_files_of_a_sensible_size_are_kept_by_path() {
    let h = harness();
    let src = tempfile::tempdir().unwrap();
    // A folder.
    let folder = src.path().join("folder");
    std::fs::create_dir(&folder).unwrap();
    // A pipe, where the file system has them (Windows keeps its pipes out of it).
    let pipe = src.path().join("pipe");
    let made = cfg!(unix)
        && std::process::Command::new("mkfifo")
            .arg(&pipe)
            .status()
            .is_ok_and(|s| s.success());
    // A sparse file over the cap.
    let huge = src.path().join("huge.pdf");
    let f = std::fs::File::create(&huge).unwrap();
    f.set_len(nocturne_app_lib::commands::MAX_FILE_BYTES + 1)
        .unwrap();
    let mut chosen = vec![folder.clone(), huge.clone()];
    if made {
        chosen.push(pipe.clone());
    }
    h.state().allow_paths(chosen.clone());
    for p in chosen {
        assert_eq!(
            h.code(
                "keep_files",
                json!({"paths": [p.to_str().unwrap()], "source": "drop"})
            ),
            "invalid",
            "{}",
            p.display()
        );
    }
    assert_eq!(h.engine().items(Default::default()).unwrap().len(), 0);
}

#[test]
fn files_dropped_on_the_window_may_then_be_kept() {
    use tauri::{DragDropEvent, WindowEvent};
    let h = harness();
    let src = tempfile::tempdir().unwrap();
    let f = write_file(src.path(), "dropped.txt", b"dropped on the window");
    let window = h.webview.as_ref().window();
    nocturne_app_lib::window::on_window_event(
        &window,
        &WindowEvent::DragDrop(DragDropEvent::Drop {
            paths: vec![f.clone()],
            position: tauri::PhysicalPosition::new(0.0, 0.0),
        }),
    );
    h.ok(
        "keep_files",
        json!({"paths": [f.to_str().unwrap()], "source": "drop"}),
    );
}

#[test]
fn files_dropped_on_quick_capture_are_held_for_it_to_ask_for() {
    use tauri::{DragDropEvent, WindowEvent};
    let h = harness();
    let src = tempfile::tempdir().unwrap();
    let f = write_file(src.path(), "dropped.txt", b"dropped on quick capture");
    let drop = |window: &tauri::Window<tauri::test::MockRuntime>| {
        nocturne_app_lib::window::on_window_event(
            window,
            &WindowEvent::DragDrop(DragDropEvent::Drop {
                paths: vec![f.clone()],
                position: tauri::PhysicalPosition::new(0.0, 0.0),
            }),
        );
    };
    let quick = h.window("quick-capture");

    drop(&h.webview.as_ref().window());
    assert_eq!(
        h.call_from(&quick, "take_quick_drop", json!({})).unwrap(),
        json!([]),
        "a drop on the main window is not for quick capture"
    );

    drop(&quick.as_ref().window());
    let taken = h.call_from(&quick, "take_quick_drop", json!({})).unwrap();
    assert_eq!(taken, json!([f.to_str().unwrap()]));
    assert_eq!(
        h.call_from(&quick, "take_quick_drop", json!({})).unwrap(),
        json!([]),
        "what was handed over is not handed over twice"
    );

    let kept = h
        .call_from(
            &quick,
            "keep_files",
            json!({"paths": taken, "source": "drop"}),
        )
        .unwrap();
    let id = kept[0]["item"]["id"].as_str().unwrap();
    h.call_from(&quick, "get_item", json!({"id": id}))
        .expect("quick capture may read what it kept from the drop");
}

// ---- opening and choosing: the shell does it, within bounds ----

#[test]
fn open_original_opens_only_the_kept_original() {
    let h = harness();
    let opened = h.recording_desktop();
    let kept = h
        .call_raw(
            "keep_bytes",
            vec![7, 7, 7],
            &[
                ("x-nocturne-mime", "image/png"),
                ("x-nocturne-source", "paste"),
            ],
        )
        .unwrap();
    h.ok("open_original", json!({"itemId": id_of(&kept)}));
    let item = h.ok("get_item", json!({"id": id_of(&kept)}));
    let seen = opened.paths();
    assert_eq!(seen.len(), 1);
    assert_eq!(
        dunce::canonicalize(&seen[0]).unwrap(),
        dunce::canonicalize(item["originalPath"].as_str().unwrap()).unwrap()
    );
    // A note has no original; a missing item is not found.
    let note = h.ok(
        "keep_text",
        json!({"text": "just words", "source": "paste"}),
    );
    assert_eq!(
        h.code("open_original", json!({"itemId": id_of(&note)})),
        "notFound"
    );
    assert_eq!(
        h.code("open_original", json!({"itemId": "itm_nope"})),
        "notFound"
    );
    assert_eq!(opened.paths().len(), 1);
}

#[test]
fn open_export_opens_only_a_folder_an_export_made_in_this_session() {
    let h = harness();
    let opened = h.recording_desktop();
    h.learn("a note", "You like tea.", "Tea", "tea");
    let out = tempfile::tempdir().unwrap();
    // Nothing exported yet: even a real folder is refused.
    assert_eq!(
        h.code("open_export", json!({"path": out.path().to_str().unwrap()})),
        "invalid"
    );
    let report = h.ok("export_to", json!({"dir": out.path().to_str().unwrap()}));
    let made = report["path"].as_str().unwrap();
    h.ok("open_export", json!({"path": made}));
    assert_eq!(opened.paths().len(), 1);
    // Not the parent, not the data folder, not something with `..` in it.
    for bad in [
        out.path().to_string_lossy().into_owned(),
        h.dir.path().to_string_lossy().into_owned(),
        format!("{made}/../.."),
        "/etc".to_string(),
    ] {
        assert_eq!(
            h.code("open_export", json!({"path": bad})),
            "invalid",
            "{bad}"
        );
    }
    assert_eq!(opened.paths().len(), 1);
}

#[test]
fn open_logs_opens_the_log_folder_inside_the_data_folder() {
    let h = harness();
    let opened = h.recording_desktop();
    std::fs::create_dir_all(h.dir.path().join("logs")).unwrap();
    h.ok("open_logs", json!({}));
    assert_eq!(opened.paths().as_slice(), &[h.dir.path().join("logs")]);
}

#[test]
fn open_logs_still_opens_after_the_log_folder_is_deleted() {
    let h = harness();
    let opened = h.recording_desktop();
    let logs = h.dir.path().join("logs");
    assert!(!logs.exists());
    h.ok("open_logs", json!({}));
    assert!(logs.is_dir(), "the folder is made before it is opened");
    std::fs::remove_dir_all(&logs).unwrap();
    h.ok("open_logs", json!({}));
    assert!(logs.is_dir());
    assert_eq!(opened.paths().as_slice(), &[logs.clone(), logs]);
}

#[test]
fn open_data_folder_opens_the_data_folder() {
    let h = harness();
    let opened = h.recording_desktop();
    h.ok("open_data_folder", json!({}));
    assert_eq!(opened.paths().as_slice(), &[h.dir.path().to_path_buf()]);
}

/// Every command that starts something on the person's desktop.
const LAUNCHERS: &[&str] = &[
    "open_sound_settings",
    "open_original",
    "open_export",
    "open_logs",
    "open_data_folder",
    "open_remote_sign_in",
];

#[test]
fn the_command_loop_launches_nothing_on_this_desktop() {
    let h = harness();
    for name in ROADMAP.iter().chain(COMMAND_NAMES) {
        let _ = h.call(name, json!({}));
    }
    assert!(
        nocturne_app_lib::desktop::get(h.app.handle()).is_none(),
        "the harness must never hold the real desktop"
    );
}

#[test]
fn every_launching_command_is_unavailable_without_the_desktop() {
    let h = harness();
    std::fs::create_dir_all(h.dir.path().join("logs")).unwrap();
    assert_eq!(h.code("open_sound_settings", json!({})), "unavailable");
    assert_eq!(h.code("open_logs", json!({})), "unavailable");
    assert_eq!(h.code("open_data_folder", json!({})), "unavailable");
    let kept = h
        .call_raw(
            "keep_bytes",
            vec![7, 7, 7],
            &[
                ("x-nocturne-mime", "image/png"),
                ("x-nocturne-source", "paste"),
            ],
        )
        .unwrap();
    assert_eq!(
        h.code("open_original", json!({"itemId": id_of(&kept)})),
        "unavailable"
    );
    h.learn("a note", "You like tea.", "Tea", "tea");
    let out = tempfile::tempdir().unwrap();
    let report = h.ok("export_to", json!({"dir": out.path().to_str().unwrap()}));
    assert_eq!(
        h.code("open_export", json!({"path": report["path"]})),
        "unavailable"
    );
    for name in LAUNCHERS {
        assert!(COMMAND_NAMES.contains(name), "{name} is not a command");
    }
}

#[test]
fn the_sound_settings_open_through_the_installed_desktop() {
    use common::Launch;
    let h = harness();
    let desktop = h.recording_desktop();
    h.ok("open_sound_settings", json!({}));
    assert_eq!(desktop.launches(), [Launch::SoundSettings]);
}

#[test]
fn nothing_can_be_opened_before_the_app_installs_its_opener() {
    let h = harness();
    std::fs::create_dir_all(h.dir.path().join("logs")).unwrap();
    assert_eq!(h.code("open_logs", json!({})), "unavailable");
    assert_eq!(h.code("open_data_folder", json!({})), "unavailable");
}

// ---- the window ----

#[test]
fn the_window_shows_once_and_not_at_all_when_started_hidden() {
    let h = harness();
    assert!(!h.state().window_shown());
    h.ok("app_ready", json!({}));
    assert!(h.state().window_shown());

    let hidden = harness();
    hidden.state().set_start_hidden(true);
    hidden.ok("app_ready", json!({}));
    assert!(
        !hidden.state().window_shown(),
        "started in the tray: the interface's ready does not show it"
    );
    nocturne_app_lib::window::show_main(hidden.app.handle());
    assert!(
        hidden.state().window_shown(),
        "an explicit show still works"
    );
}

#[test]
fn exit_hooks_run_once_when_the_app_exits() {
    let h = harness();
    let ran = Arc::new(std::sync::atomic::AtomicU32::new(0));
    let r = ran.clone();
    h.state().on_exit(Box::new(move || {
        r.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }));
    nocturne_app_lib::handle_run_event(h.app.handle(), &tauri::RunEvent::Exit);
    nocturne_app_lib::handle_run_event(h.app.handle(), &tauri::RunEvent::Exit);
    assert_eq!(ran.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert!(nocturne_app_lib::exit_prevented(true, true));
    assert!(!nocturne_app_lib::exit_prevented(true, false));
    assert!(
        !nocturne_app_lib::exit_prevented(false, true),
        "an explicit quit is not refused"
    );
}

// ---- the mind slot and the status ----

#[test]
fn putting_a_mind_in_the_slot_refreshes_the_status_and_the_setup_view() {
    let h = harness();
    let status = h.events(EVENT_STATUS);
    let setup = h.events("setup-progress");
    assert_eq!(h.ok("get_status", json!({}))["state"], "gettingReady");
    let handle = h.app.handle().clone();
    tauri::async_runtime::block_on(nocturne_app_lib::set_mind(
        &handle,
        Some(Arc::new(nocturne_engine::FakeMind::new())),
    ));
    let all = status.lock().unwrap().clone();
    assert_eq!(all.last().unwrap()["state"], "resting");
    assert_eq!(setup.lock().unwrap().last().unwrap()["phase"], "done");
    tauri::async_runtime::block_on(nocturne_app_lib::set_mind(&handle, None));
    assert_eq!(
        status.lock().unwrap().last().unwrap()["state"],
        "gettingReady"
    );
}

#[test]
fn refreshes_that_race_publish_once() {
    let h = harness_with_mind(Arc::new(nocturne_engine::FakeMind::new()));
    let status = h.events(EVENT_STATUS);
    let handle = h.app.handle().clone();
    tauri::async_runtime::block_on(async move {
        let mut all = Vec::new();
        for _ in 0..12 {
            let h = handle.clone();
            all.push(tauri::async_runtime::spawn(async move {
                nocturne_app_lib::refresh_status(&h).await;
            }));
        }
        for t in all {
            t.await.unwrap();
        }
    });
    assert_eq!(status.lock().unwrap().len(), 1);
}

#[cfg(unix)]
#[test]
fn what_the_shell_keeps_on_disk_is_owner_only() {
    use std::os::unix::fs::PermissionsExt;
    let h = harness_with_mind(Arc::new(nocturne_engine::FakeMind::new()));
    h.ok("pause", json!({"until": null}));
    let mode = std::fs::metadata(h.dir.path().join("app-state.json"))
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600);
    for sub in ["store", "store/originals", "store/audio"] {
        let mode = std::fs::metadata(h.dir.path().join(sub))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o700, "{sub}");
    }
}

const ONLINE_AND_MODEL_COMMANDS: &[&str] = &[
    "get_online",
    "check_online",
    "cancel_online_check",
    "turn_on_online",
    "turn_off_online",
    "abandon_online_setup",
    "see_online_change",
    "update_online_settings",
    "list_online_sends",
    "get_models",
    "list_model_choices",
    "preview_model_change",
    "change_model",
    "pause_model_download",
    "resume_model_download",
    "cancel_model_change",
    "cancel_reindex",
    "keep_old_model",
    "remove_old_model",
    "reset_models_to_tier",
];

/// Online answers and choosing models belong to the main window only.
#[test]
fn online_and_model_commands_refused_from_quick_capture() {
    let h = harness();
    let quick = h.window("quick-capture");
    for cmd in ONLINE_AND_MODEL_COMMANDS {
        assert!(COMMAND_NAMES.contains(cmd), "{cmd} is registered");
        assert_eq!(h.code_from(&quick, cmd, json!({})), "invalid", "{cmd}");
    }
}

#[test]
fn get_online_is_off_on_a_fresh_store() {
    let h = harness();
    let view = h.ok("get_online", json!({}));
    assert_eq!(view["state"], json!({"type": "off"}));
    assert_eq!(view["setUp"], false);
    assert_eq!(view["phones"], true);
    assert!(
        view.get("sendSensitive").is_none(),
        "no switch for health, money or ID"
    );
    assert_eq!(
        view["limit"],
        json!({"kind": "none", "questions": 200, "usd": null})
    );
    assert_eq!(view["questionsThisMonth"], 0);
    assert_eq!(view["sentBytes"], 0.0);
    assert_eq!(view["keyStorage"], Value::Null);
    // A key handed to the stub check goes nowhere and comes back in nothing.
    let reply = h.call(
        "check_online",
        json!({"form": {"baseUrl": "https://api.example.com/v1", "key": "sk-never-0123456789", "model": "m"}}),
    );
    assert!(!format!("{reply:?}").contains("sk-never-0123456789"));
    assert_eq!(h.code("turn_on_online", json!({"settings": {"phones": true, "limit": {"kind": "questions", "questions": 200, "usd": null}}})), "unavailable");
}

#[test]
fn get_models_is_the_tier_preset() {
    let h = harness();
    let fresh = h.ok("get_models", json!({}));
    assert_eq!(fresh["preset"], "unsupported");
    assert_eq!(fresh["jobs"], json!([]));

    let config = h.dir.path().join("engine").join("config");
    std::fs::create_dir_all(&config).unwrap();
    std::fs::write(
        config.join("nocturne-setup.json"),
        r#"{"version": 1, "tier": "small", "done": true, "backends": {}}"#,
    )
    .unwrap();
    let view = h.ok("get_models", json!({}));
    assert_eq!(view["preset"], "small");
    assert_eq!(view["custom"], false);
    let jobs: Vec<(String, String)> = view["jobs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|j| {
            (
                j["job"].as_str().unwrap().to_string(),
                j["model"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    let want: Vec<(String, String)> =
        nocturne_lemonade::tier::components(nocturne_contract::Tier::Small)
            .into_iter()
            .map(|c| {
                let job = serde_json::to_value(nocturne_app_lib::models::job_of(c.role)).unwrap();
                (job.as_str().unwrap().to_string(), c.model.to_string())
            })
            .collect();
    assert_eq!(jobs, want);
    assert_eq!(
        h.code("list_model_choices", json!({"job": "answering"})),
        "unavailable"
    );
}

#[test]
fn ask_place_defaults_to_auto() {
    let h = harness();
    assert!(
        h.ok("ask", json!({"question": "When are the bins?"}))
            .is_string()
    );
    assert!(
        h.ok(
            "ask",
            json!({"question": "When are the bins?", "place": "thisComputer"})
        )
        .is_string()
    );
    assert!(
        h.call(
            "ask",
            json!({"question": "When are the bins?", "place": "online"})
        )
        .is_err(),
        "only the known places"
    );
    assert_eq!(
        nocturne_contract::AskPlace::default(),
        nocturne_contract::AskPlace::Auto
    );
}

// ---- The journal ----

fn feeling_json(word: &str) -> Value {
    json!({
        "part": "settled",
        "point": {"pleasant": 0.5, "energy": -0.5},
        "words": [word],
    })
}

#[test]
fn keep_note_with_a_feeling_keeps_announces_and_lands_on_the_journal_day() {
    let h = harness();
    let items = h.events(EVENT_ITEM);
    let kept = h.ok(
        "keep_note",
        json!({
            "text": "Walked by the river",
            "source": "quickCapture",
            "feeling": feeling_json("calm"),
            "good": false,
        }),
    );
    assert_eq!(kept["type"], "new");
    assert_eq!(items.lock().unwrap().len(), 1, "announced like keep_text");

    let day = h.ok("get_journal_day", json!({"day": null}));
    assert_eq!(day["day"], "2026-11-13");
    assert_eq!(day["today"], true);
    assert_eq!(day["checkins"].as_array().unwrap().len(), 1);
    assert_eq!(day["checkins"][0]["feeling"]["words"], json!(["calm"]));
    let named = h.ok("get_journal_day", json!({"day": "2026-11-13"}));
    assert_eq!(named["day"], day["day"]);
    assert_eq!(
        h.ok("list_journal_days", json!({})),
        json!(["2026-11-13"]),
        "dates come back as YYYY-MM-DD"
    );
    assert_eq!(
        h.code("get_journal_day", json!({"day": "last week"})),
        "invalid"
    );
}

#[test]
fn check_ins_and_good_things_are_kept_and_announced() {
    let h = harness();
    let items = h.events(EVENT_ITEM);
    let checkin = h.ok(
        "keep_check_in",
        json!({"input": {
            "feeling": feeling_json("calm"),
            "line": null,
            "urge": null,
            "replaces": null,
        }}),
    );
    assert_eq!(checkin["day"], "2026-11-13");
    let thing = h.ok(
        "keep_good_thing",
        json!({"day": "2026-11-13", "text": "Sun on the balcony", "photoItemId": null}),
    );
    assert_eq!(thing["text"], "Sun on the balcony");
    assert_eq!(items.lock().unwrap().len(), 2);
    assert_eq!(
        h.code(
            "keep_good_thing",
            json!({"day": "tomorrow", "text": "x", "photoItemId": null})
        ),
        "invalid"
    );
    let words = h.ok("top_feeling_words", json!({"part": "settled"}));
    assert!(!words.as_array().unwrap().is_empty());
}
