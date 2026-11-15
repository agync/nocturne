//! Passphrase backups through the shell: the file the save dialog chose, what checking it says,
//! the copies put aside, and the restore that starts at the next launch.

mod common;

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;

use common::*;
use nocturne_app_lib::AppState;
use nocturne_app_lib::restore::{self, ApplyOutcome, Aside, Source};
use nocturne_contract::EVENT_BACKUP;
use nocturne_engine::{Engine, ManualClock};
use serde_json::{Value, json};
use tauri::{Listener, Manager};

const PASS: &str = "a long enough passphrase";

fn payload(file: &Path) -> Vec<u8> {
    let input = std::fs::File::open(file).unwrap();
    let decryptor = age::Decryptor::new(input).unwrap();
    let mut identity =
        age::scrypt::Identity::new(age::secrecy::SecretString::from(PASS.to_string()));
    identity.set_max_work_factor(19);
    let mut stream = decryptor
        .decrypt(std::iter::once(&identity as &dyn age::Identity))
        .unwrap();
    let mut all = Vec::new();
    stream.read_to_end(&mut all).unwrap();
    all
}

fn entries(plain: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut archive = tar::Archive::new(plain);
    let mut out = Vec::new();
    for entry in archive.entries().unwrap() {
        let mut entry = entry.unwrap();
        let name = entry.path().unwrap().to_str().unwrap().to_string();
        let mut data = Vec::new();
        entry.read_to_end(&mut data).unwrap();
        out.push((name, data));
    }
    out
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

fn marker(data: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(restore::marker_path(data)).unwrap()).unwrap()
}

/// A harness with a kept original, a learned memory and a saved page.
fn lived_in(h: &Harness) -> (String, Vec<u8>) {
    let original = b"JPEG bytes of the card".to_vec();
    let src = tempfile::tempdir().unwrap();
    let card = write_file(src.path(), "card.jpg", &original);
    h.state().allow_paths([card.clone()]);
    let kept = h.ok(
        "keep_files",
        json!({"paths": [card.to_str().unwrap()], "source": "drop"}),
    );
    let photo = id_of(&kept[0]);
    h.learn("a note about Gran", "Gran lives in Leeds.", "Gran", "Leeds");
    h.save_todays_page();
    (photo, original)
}

fn save_to(h: &Harness, dest: &Path) -> Value {
    h.state().allow_save_target(dest);
    h.ok(
        "make_backup",
        json!({"path": dest.to_str().unwrap(), "passphrase": PASS}),
    )
}

fn check(h: &Harness, file: &Path) -> Value {
    h.state().allow_restore_source(file);
    h.ok(
        "check_backup",
        json!({"path": file.to_str().unwrap(), "passphrase": PASS}),
    )
}

#[test]
fn a_backup_is_saved_under_the_chosen_name_and_holds_only_the_store() {
    let h = harness();
    let (_photo, original) = lived_in(&h);
    let data = h.dir.path();
    std::fs::create_dir_all(data.join("online")).unwrap();
    std::fs::write(data.join("online/key"), b"ONLINEKEYSECRET9").unwrap();
    std::fs::create_dir_all(data.join("remote")).unwrap();
    std::fs::write(data.join("remote/computer.key"), b"REMOTESECRET9").unwrap();
    std::fs::create_dir_all(data.join("engine")).unwrap();
    std::fs::write(data.join("engine/config.json"), b"ENGINECFGSECRET9").unwrap();
    std::fs::write(data.join("server-token"), b"SERVERTOKENSECRET9").unwrap();
    let selection = b"{\"choices\":[\"small\"]}";
    std::fs::create_dir_all(data.join("engine/config")).unwrap();
    std::fs::write(data.join("engine/config/selection.json"), selection).unwrap();

    let out = tempfile::tempdir().unwrap();
    let dest = out.path().join("Nocturne backup 2026-11-13.nocturne");
    // Nobody chose that path yet.
    assert_eq!(
        h.code(
            "make_backup",
            json!({"path": dest.to_str().unwrap(), "passphrase": PASS}),
        ),
        "invalid"
    );
    assert!(!dest.exists());

    let events = h.events(EVENT_BACKUP);
    let report = save_to(&h, &dest);
    assert_eq!(report["path"], dest.to_str().unwrap());
    assert_eq!(report["originals"], 1);
    assert!(dest.is_file(), "the exact path the dialog returned");
    assert!(
        h.state().export_allowed(out.path()),
        "the folder can be shown afterwards"
    );
    assert!(!dest.with_extension("nocturne.partial").exists());

    let seen = events.lock().unwrap().clone();
    assert!(!seen.is_empty());
    assert_eq!(seen.first().unwrap()["phase"], "saving");
    assert!(seen.iter().any(|e| e["phase"] == "checking"));
    let last = seen.last().unwrap();
    assert_eq!(last["phase"], "checking");
    assert_eq!(last["done"], last["total"], "{last}");

    let view = h.ok("get_backups", json!({}));
    assert_eq!(view["last"]["bytes"], report["bytes"]);
    assert!(view["running"].is_null());
    assert_eq!(view["interruptedBackup"], false);
    assert_eq!(view["interruptedRestore"], false);

    let plain = payload(&dest);
    let all = entries(&plain);
    let names: Vec<&str> = all.iter().map(|(n, _)| n.as_str()).collect();
    assert!(names.contains(&"manifest.json"), "{names:?}");
    assert!(names.contains(&"store/nocturne.db"), "{names:?}");
    assert!(names.contains(&"engine/selection.json"), "{names:?}");
    let stored = all
        .iter()
        .find(|(n, _)| n.starts_with("store/originals/"))
        .expect("the kept original");
    assert_eq!(stored.1, original);
    let chosen = all
        .iter()
        .find(|(n, _)| n == "engine/selection.json")
        .unwrap();
    assert_eq!(chosen.1, selection);

    for secret in [
        b"ONLINEKEYSECRET9".as_slice(),
        b"REMOTESECRET9".as_slice(),
        b"ENGINECFGSECRET9".as_slice(),
        b"SERVERTOKENSECRET9".as_slice(),
    ] {
        assert!(!contains(&plain, secret), "{:?} travels", secret);
    }
}

#[test]
fn the_estimate_covers_what_is_kept_and_probe_reads_only_the_start() {
    let h = harness();
    let original = vec![9u8; 65_536];
    let src = tempfile::tempdir().unwrap();
    let card = write_file(src.path(), "card.jpg", &original);
    h.state().allow_paths([card.clone()]);
    h.ok(
        "keep_files",
        json!({"paths": [card.to_str().unwrap()], "source": "drop"}),
    );
    let estimate = h.ok("estimate_backup", json!({}));
    assert!(estimate.as_f64().unwrap() >= 65_536.0, "{estimate}");

    let out = tempfile::tempdir().unwrap();
    let dest = out.path().join("chosen.nocturne");
    save_to(&h, &dest);
    h.state().allow_restore_source(&dest);
    h.ok("probe_backup", json!({"path": dest.to_str().unwrap()}));

    let words = write_file(out.path(), "notes.txt", b"the quick brown fox");
    assert_eq!(
        h.code("probe_backup", json!({"path": words.to_str().unwrap()})),
        "invalid"
    );
    h.state().allow_restore_source(&words);
    assert_eq!(
        h.code("probe_backup", json!({"path": words.to_str().unwrap()})),
        "notABackup"
    );
}

#[test]
fn checking_a_backup_says_what_restoring_it_would_do() {
    let h = harness();
    let (_photo, _original) = lived_in(&h);
    let out = tempfile::tempdir().unwrap();
    let dest = out.path().join("chosen.nocturne");
    save_to(&h, &dest);
    h.state().allow_restore_source(&dest);

    let not_picked = out.path().join("not-picked.nocturne");
    std::fs::copy(&dest, &not_picked).unwrap();
    assert_eq!(
        h.code(
            "check_backup",
            json!({"path": not_picked.to_str().unwrap(), "passphrase": PASS}),
        ),
        "invalid",
        "the file must be picked first"
    );
    assert_eq!(
        h.code(
            "check_backup",
            json!({"path": dest.to_str().unwrap(), "passphrase": "the wrong one"}),
        ),
        "wrongPassphrase"
    );
    assert!(!restore::check_dir(h.dir.path()).exists());

    let checked = check(&h, &dest);
    assert_eq!(checked["summary"]["originals"], 1);
    assert!(checked["summary"]["items"].as_u64().unwrap() >= 2);
    assert_eq!(checked["here"]["originals"], 1);
    assert!(checked["putAside"].is_null());
    assert!(!restore::check_dir(h.dir.path()).exists());

    let words = write_file(out.path(), "notes.txt", b"not a backup");
    h.state().allow_restore_source(&words);
    assert_eq!(
        h.code(
            "check_backup",
            json!({"path": words.to_str().unwrap(), "passphrase": PASS}),
        ),
        "notABackup"
    );
}

#[test]
fn restoring_over_things_needs_the_yes_and_then_restarts_once() {
    let h = harness();
    lived_in(&h);
    let out = tempfile::tempdir().unwrap();
    let dest = out.path().join("chosen.nocturne");
    save_to(&h, &dest);
    check(&h, &dest);

    assert_eq!(
        h.code(
            "restore_backup",
            json!({"path": dest.to_str().unwrap(), "passphrase": PASS, "replace": false}),
        ),
        "invalid"
    );
    assert!(
        !restore::marker_path(h.dir.path()).exists(),
        "nothing scheduled"
    );

    let other = out.path().join("other.nocturne");
    save_to(&h, &other);
    assert_eq!(
        h.code(
            "restore_backup",
            json!({"path": other.to_str().unwrap(), "passphrase": PASS, "replace": true}),
        ),
        "invalid",
        "that is not the file that was checked"
    );

    let (tx, rx) = std::sync::mpsc::channel::<()>();
    let calls = Arc::new(AtomicU32::new(0));
    let counter = calls.clone();
    h.state().set_restarter(Arc::new(move || {
        counter.fetch_add(1, Ordering::SeqCst);
        let _ = tx.send(());
    }));
    h.ok(
        "restore_backup",
        json!({"path": dest.to_str().unwrap(), "passphrase": PASS, "replace": true}),
    );
    let scheduled = marker(h.dir.path());
    assert_eq!(scheduled["source"], "stage");
    assert_eq!(scheduled["replace"], true);
    assert!(restore::stage_dir(h.dir.path()).join("store").is_dir());
    rx.recv_timeout(Duration::from_secs(10))
        .expect("the app is asked to restart");
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    // The switch is committed: Stop no longer changes anything.
    h.ok("cancel_backup", json!({}));
    assert!(restore::marker_path(h.dir.path()).exists());
    assert!(restore::stage_dir(h.dir.path()).join("store").is_dir());
}

#[test]
fn data_kept_while_the_backup_unpacks_is_scheduled_to_be_put_aside() {
    let source = harness();
    lived_in(&source);
    let out = tempfile::tempdir().unwrap();
    let dest = out.path().join("chosen.nocturne");
    save_to(&source, &dest);

    let target = harness();
    check(&target, &dest);
    let handle = target.app.handle().clone();
    let kept = Arc::new(AtomicBool::new(false));
    let flag = kept.clone();
    target.app.listen(EVENT_BACKUP, move |ev| {
        let value: Value = serde_json::from_str(ev.payload()).unwrap();
        if value["phase"] == "restoring" && !flag.swap(true, Ordering::SeqCst) {
            handle
                .state::<AppState>()
                .engine()
                .unwrap()
                .keep_text(
                    "kept while the backup was unpacked",
                    nocturne_contract::Source::QuickCapture,
                )
                .unwrap();
        }
    });
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    target.state().set_restarter(Arc::new(move || {
        let _ = tx.send(());
    }));
    target.ok(
        "restore_backup",
        json!({"path": dest.to_str().unwrap(), "passphrase": PASS, "replace": false}),
    );
    assert!(
        kept.load(Ordering::SeqCst),
        "something was kept during unpacking"
    );
    rx.recv_timeout(Duration::from_secs(10))
        .expect("the app is asked to restart");
    let scheduled = marker(target.dir.path());
    assert_eq!(
        scheduled["replace"], true,
        "what appeared during the unpack is put aside, not deleted"
    );
    assert!(restore::stage_dir(target.dir.path()).join("store").is_dir());
}

#[test]
fn cancel_after_commit_is_a_no_op() {
    let h = harness();
    let cancel = h.state().begin_backup().unwrap();
    h.state().commit_backup();
    h.state().cancel_backup();
    assert!(!cancel.load(Ordering::SeqCst));
    h.state().end_backup();
    assert!(h.state().backup_idle());
}

#[test]
fn cancelling_while_restoring_leaves_no_stage() {
    let h = harness();
    let big: Vec<u8> = (0..12 * 1024 * 1024u32).map(|i| (i % 251) as u8).collect();
    h.engine()
        .keep_bytes(&big, "txt", nocturne_contract::Source::Paste)
        .unwrap();
    let out = tempfile::tempdir().unwrap();
    let dest = out.path().join("chosen.nocturne");
    save_to(&h, &dest);
    check(&h, &dest);

    let handle = h.app.handle().clone();
    h.app.listen(EVENT_BACKUP, move |ev| {
        let value: Value = serde_json::from_str(ev.payload()).unwrap();
        if value["phase"] == "restoring" {
            handle.state::<AppState>().cancel_backup();
        }
    });
    assert_eq!(
        h.code(
            "restore_backup",
            json!({"path": dest.to_str().unwrap(), "passphrase": PASS, "replace": true}),
        ),
        "cancelled"
    );
    assert!(!restore::stage_dir(h.dir.path()).exists());
    assert!(!restore::marker_path(h.dir.path()).exists());
}

#[test]
fn a_backup_changed_after_its_check_is_not_restored() {
    let h = harness();
    lived_in(&h);
    let out = tempfile::tempdir().unwrap();
    let dest = out.path().join("chosen.nocturne");
    save_to(&h, &dest);
    check(&h, &dest);

    h.engine()
        .keep_text("kept after the check", nocturne_contract::Source::Paste)
        .unwrap();
    let other = out.path().join("other.nocturne");
    save_to(&h, &other);
    std::fs::copy(&other, &dest).unwrap();

    assert_eq!(
        h.code(
            "restore_backup",
            json!({"path": dest.to_str().unwrap(), "passphrase": PASS, "replace": true}),
        ),
        "damagedBackup"
    );
    assert!(!restore::stage_dir(h.dir.path()).exists());
    assert!(!restore::marker_path(h.dir.path()).exists());
}

#[test]
fn a_new_restore_takes_the_place_of_a_scheduled_one() {
    let h = harness();
    let big: Vec<u8> = (0..12 * 1024 * 1024u32).map(|i| (i % 251) as u8).collect();
    h.engine()
        .keep_bytes(&big, "txt", nocturne_contract::Source::Paste)
        .unwrap();
    let out = tempfile::tempdir().unwrap();
    let dest = out.path().join("chosen.nocturne");
    save_to(&h, &dest);
    check(&h, &dest);
    h.ok(
        "restore_backup",
        json!({"path": dest.to_str().unwrap(), "passphrase": PASS, "replace": true}),
    );
    assert!(restore::marker_path(h.dir.path()).exists());

    let handle = h.app.handle().clone();
    h.app.listen(EVENT_BACKUP, move |ev| {
        let value: Value = serde_json::from_str(ev.payload()).unwrap();
        if value["phase"] == "restoring" {
            handle.state::<AppState>().cancel_backup();
        }
    });
    assert_eq!(
        h.code(
            "restore_backup",
            json!({"path": dest.to_str().unwrap(), "passphrase": PASS, "replace": true}),
        ),
        "cancelled"
    );
    assert!(!restore::stage_dir(h.dir.path()).exists());
    assert!(
        !restore::marker_path(h.dir.path()).exists(),
        "no marker is left pointing at a stage that is gone"
    );
    assert_eq!(
        restore::apply_pending(h.dir.path()),
        ApplyOutcome::NothingPending,
        "the next start does not report a restore that did not happen"
    );
}

#[test]
fn crash_leftovers_are_cleaned_up_and_flagged() {
    let h = harness();
    let data = h.dir.path();
    let partial = data.join("chosen.nocturne.partial");
    std::fs::write(&partial, b"half a file").unwrap();
    restore::write_job(data, &partial).unwrap();
    let stage = restore::stage_dir(data);
    std::fs::create_dir_all(stage.join("store")).unwrap();
    std::fs::write(stage.join("store/nocturne.db"), b"staged").unwrap();

    assert_eq!(restore::apply_pending(data), ApplyOutcome::NothingPending);
    assert!(!partial.exists());
    assert!(!stage.exists());
    let view = h.ok("get_backups", json!({}));
    assert_eq!(view["interruptedBackup"], true);
    assert_eq!(view["interruptedRestore"], true);

    h.ok("dismiss_backup_notices", json!({}));
    let view = h.ok("get_backups", json!({}));
    assert_eq!(view["interruptedBackup"], false);
    assert_eq!(view["interruptedRestore"], false);
}

#[test]
fn a_check_then_a_restore_reaches_a_second_install() {
    let h = harness();
    let (photo, original) = lived_in(&h);
    let out = tempfile::tempdir().unwrap();
    let dest = out.path().join("chosen.nocturne");
    save_to(&h, &dest);
    let checked = check(&h, &dest);
    assert_eq!(checked["summary"]["onlineUsed"], false);
    assert_eq!(checked["summary"]["awayUsed"], false);

    // A second install, as a new computer has.
    let b = tempfile::tempdir().unwrap();
    let stage = restore::stage_dir(b.path());
    std::fs::create_dir_all(stage.parent().unwrap()).unwrap();
    let summary =
        nocturne_engine::unpack_backup(&dest, PASS, &stage, &|_, _, _| {}, &AtomicBool::new(false))
            .unwrap();
    restore::schedule(
        b.path(),
        false,
        "2026-11-15 120000",
        Source::Stage,
        None,
        Some(summary),
    )
    .unwrap();
    assert_eq!(
        restore::apply_pending(b.path()),
        ApplyOutcome::Restored { kept: None }
    );
    assert!(!restore::marker_path(b.path()).exists());

    let clock = Arc::new(ManualClock::at("2026-11-15T09:00:00Z", "Europe/London"));
    let engine = Engine::open(&b.path().join("store"), clock).unwrap();
    let memories = engine.memories(Default::default()).unwrap();
    assert_eq!(memories.len(), 1);
    assert_eq!(memories[0].sentence, "Gran lives in Leeds.");
    let items = engine.items(Default::default()).unwrap();
    assert!(items.iter().any(|i| i.id == photo && i.has_original));
    let stored: Vec<_> = std::fs::read_dir(b.path().join("store/originals"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert_eq!(stored.len(), 1);
    assert_eq!(std::fs::read(&stored[0]).unwrap(), original);

    let restored = restore::restored(b.path()).unwrap();
    assert_eq!(restored.memories, 1);
    assert_eq!(restored.originals, 1);
    assert!(!restored.online_used);
    assert!(!restored.away_used);
    assert_eq!(restored.put_aside_at, None);
}

#[test]
fn a_put_aside_copy_can_be_brought_back_and_is_listed() {
    let h = harness();
    lived_in(&h);
    let data = h.dir.path();
    let id = "2026-11-14 090000";
    let copy = data.join("put-aside").join(id);
    std::fs::create_dir_all(copy.join("store")).unwrap();
    std::fs::write(copy.join("store/nocturne.db"), b"older store").unwrap();
    std::fs::write(
        copy.join("info.json"),
        serde_json::to_vec(&Aside {
            at: id.into(),
            memories: 4,
            pages: 2,
            originals: 1,
            bytes: 10.0,
        })
        .unwrap(),
    )
    .unwrap();

    let view = h.ok("get_backups", json!({}));
    let listed = view["putAside"].as_array().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0]["id"], id);
    assert_eq!(listed[0]["memories"], 4);

    for bad in ["../x", "2026-11-14 090000/../x", "not a stamp"] {
        assert_eq!(
            h.code("restore_put_aside", json!({"id": bad})),
            "invalid",
            "{bad}"
        );
        assert_eq!(
            h.code("delete_put_aside", json!({"id": bad})),
            "invalid",
            "{bad}"
        );
    }
    assert_eq!(
        h.code("restore_put_aside", json!({"id": "2026-11-15 000000"})),
        "notFound"
    );

    let (tx, rx) = std::sync::mpsc::channel::<()>();
    h.state().set_restarter(Arc::new(move || {
        let _ = tx.send(());
    }));
    h.ok("restore_put_aside", json!({"id": id}));
    let scheduled = marker(data);
    assert_eq!(scheduled["source"]["putAside"], id);
    assert_eq!(scheduled["replace"], true);
    rx.recv_timeout(Duration::from_secs(10))
        .expect("the app is asked to restart");
}

#[test]
fn a_start_swaps_a_copy_put_aside_back_in_and_puts_the_current_store_aside() {
    let dir = tempfile::tempdir().unwrap();
    let old = dir.path().join("store");
    std::fs::create_dir_all(&old).unwrap();
    std::fs::write(old.join("nocturne.db"), b"LIVEDB").unwrap();
    let copy = dir.path().join("put-aside/2026-11-14 090000");
    std::fs::create_dir_all(copy.join("store")).unwrap();
    std::fs::write(copy.join("store/nocturne.db"), b"OLDER").unwrap();

    restore::schedule(
        dir.path(),
        true,
        "2026-11-15 120000",
        Source::PutAside("2026-11-14 090000".into()),
        Some(Aside {
            at: "2026-11-15 120000".into(),
            ..Aside::default()
        }),
        None,
    )
    .unwrap();
    match restore::apply_pending(dir.path()) {
        ApplyOutcome::Restored { kept: Some(kept) } => {
            assert_eq!(
                std::fs::read(kept.join("store/nocturne.db")).unwrap(),
                b"LIVEDB"
            );
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        std::fs::read(dir.path().join("store/nocturne.db")).unwrap(),
        b"OLDER"
    );
    assert!(
        !copy.exists(),
        "the copy is the live store now, not a second one"
    );
    assert!(restore::restored(dir.path()).is_none());
}

#[test]
fn data_kept_since_the_check_is_put_aside_when_the_marker_said_otherwise() {
    let dir = tempfile::tempdir().unwrap();
    let old = dir.path().join("store");
    std::fs::create_dir_all(old.join("originals")).unwrap();
    std::fs::write(old.join("nocturne.db"), b"KEPT SINCE THE CHECK").unwrap();
    std::fs::write(old.join("originals/one"), b"A KEPT ORIGINAL").unwrap();
    let stage = restore::stage_dir(dir.path());
    std::fs::create_dir_all(stage.join("store")).unwrap();
    std::fs::write(stage.join("store/nocturne.db"), b"THE BACKUP").unwrap();
    restore::schedule(
        dir.path(),
        false,
        "2026-11-15 120000",
        Source::Stage,
        None,
        None,
    )
    .unwrap();
    match restore::apply_pending(dir.path()) {
        ApplyOutcome::Restored { kept: Some(kept) } => {
            assert_eq!(
                std::fs::read(kept.join("store/nocturne.db")).unwrap(),
                b"KEPT SINCE THE CHECK"
            );
            assert_eq!(
                std::fs::read(kept.join("store/originals/one")).unwrap(),
                b"A KEPT ORIGINAL"
            );
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        std::fs::read(dir.path().join("store/nocturne.db")).unwrap(),
        b"THE BACKUP"
    );
}

#[test]
fn deleting_a_copy_put_aside_takes_only_that_one() {
    let h = harness();
    let data = h.dir.path();
    for id in ["2026-11-14 090000", "2026-11-15 100000"] {
        let copy = data.join("put-aside").join(id);
        std::fs::create_dir_all(copy.join("store")).unwrap();
        std::fs::write(copy.join("store/nocturne.db"), b"x").unwrap();
    }
    h.ok("delete_put_aside", json!({"id": "2026-11-14 090000"}));
    let view = h.ok("get_backups", json!({}));
    let left = view["putAside"].as_array().unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0]["id"], "2026-11-15 100000");
}

#[test]
fn deleting_everything_takes_the_backup_restore_and_put_aside_folders() {
    let h = harness();
    let data = h.dir.path();
    std::fs::create_dir_all(data.join("restore/stage/store")).unwrap();
    std::fs::write(data.join("restore/stage/store/nocturne.db"), b"staged").unwrap();
    std::fs::create_dir_all(data.join("put-aside/S/store")).unwrap();
    std::fs::write(data.join("put-aside/S/store/nocturne.db"), b"kept").unwrap();
    std::fs::create_dir_all(data.join("backup")).unwrap();
    std::fs::write(data.join("backup/last.json"), b"{}").unwrap();

    h.ok(
        "delete_everything",
        json!({"confirmPhrase": "delete everything"}),
    );
    assert!(!data.join("restore").exists());
    assert!(!data.join("put-aside").exists());
    assert!(!data.join("backup").exists());
}

#[test]
fn get_restored_and_finish_restore() {
    let h = harness();
    let data = h.dir.path();
    let view = nocturne_contract::RestoredView {
        made_at: "2026-11-14T06:00:00.000Z".into(),
        memories: 2,
        pages: 1,
        originals: 1,
        online_used: true,
        away_used: true,
        put_aside_at: Some("2026-11-15 120000".into()),
    };
    std::fs::create_dir_all(data.join("backup")).unwrap();
    std::fs::write(
        data.join("backup/restored.json"),
        serde_json::to_vec(&view).unwrap(),
    )
    .unwrap();
    let got = h.ok("get_restored", json!({}));
    assert_eq!(got["memories"], 2);
    assert_eq!(got["onlineUsed"], true);
    h.ok("finish_restore", json!({}));
    assert!(h.ok("get_restored", json!({})).is_null());
}

#[test]
fn a_save_path_the_dialog_did_not_give_is_refused() {
    let h = harness();
    let out = tempfile::tempdir().unwrap();
    let dest = out.path().join("chosen.nocturne");
    // No `pick_backup_save`: the path is not allowed, so nothing may be written.
    assert_eq!(
        h.code(
            "make_backup",
            json!({"path": dest.to_str().unwrap(), "passphrase": PASS}),
        ),
        "invalid"
    );
    assert_eq!(std::fs::read_dir(out.path()).unwrap().count(), 0);
    let src = tempfile::tempdir().unwrap();
    let file = write_file(src.path(), "x.nocturne", b"x");
    assert_eq!(
        h.code("probe_backup", json!({"path": file.to_str().unwrap()})),
        "invalid"
    );
}

#[test]
fn checking_and_discarding_removes_the_staged_check() {
    let h = harness();
    lived_in(&h);
    let out = tempfile::tempdir().unwrap();
    let dest = out.path().join("chosen.nocturne");
    save_to(&h, &dest);
    check(&h, &dest);
    assert!(h.state().checked_backup().is_some());
    h.ok("discard_restore", json!({}));
    assert!(h.state().checked_backup().is_none());
    assert!(!restore::check_dir(h.dir.path()).exists());
    assert!(!restore::stage_dir(h.dir.path()).exists());
    assert_eq!(
        h.code(
            "restore_backup",
            json!({"path": dest.to_str().unwrap(), "passphrase": PASS, "replace": true}),
        ),
        "invalid"
    );
}

#[test]
fn a_scheduled_restore_cannot_be_discarded() {
    let h = harness();
    let data = h.dir.path();
    let stage = restore::stage_dir(data);
    std::fs::create_dir_all(stage.join("store")).unwrap();
    std::fs::write(stage.join("store/nocturne.db"), b"x").unwrap();
    restore::schedule(data, false, "2026-11-15 120000", Source::Stage, None, None).unwrap();
    assert_eq!(h.code("discard_restore", json!({})), "busy");
    assert!(stage.join("store/nocturne.db").is_file());
}

#[test]
fn a_full_drive_and_a_read_only_folder_have_their_own_codes() {
    use nocturne_app_lib::error::{CommandError, ErrorCode};
    let full = CommandError::from(nocturne_engine::Error::Io(
        std::io::ErrorKind::StorageFull.into(),
    ));
    assert_eq!(full.code, ErrorCode::NoRoom);
    let readonly = CommandError::from(nocturne_engine::Error::Io(
        std::io::ErrorKind::ReadOnlyFilesystem.into(),
    ));
    assert_eq!(readonly.code, ErrorCode::CantWrite);
    assert!(full.detail.is_none() && readonly.detail.is_none());
}

#[test]
fn the_engine_suggests_the_name_the_dialog_offers() {
    let h = harness();
    let name = h.engine().suggested_backup_name();
    assert_eq!(name, "Nocturne backup 2026-11-13.nocturne");
    assert!(PathBuf::from(name).extension().unwrap() == "nocturne");
}

#[test]
fn a_file_picked_for_keeping_is_no_backup_path() {
    let h = harness();
    let src = tempfile::tempdir().unwrap();
    let shared = write_file(src.path(), "shared.txt", b"keep these bytes");
    h.state().allow_paths([shared.clone()]);

    assert_eq!(
        h.code(
            "make_backup",
            json!({"path": shared.to_str().unwrap(), "passphrase": PASS}),
        ),
        "invalid"
    );
    assert_eq!(
        h.code("probe_backup", json!({"path": shared.to_str().unwrap()})),
        "invalid"
    );
    assert_eq!(
        h.code(
            "check_backup",
            json!({"path": shared.to_str().unwrap(), "passphrase": PASS}),
        ),
        "invalid"
    );
    assert_eq!(std::fs::read(&shared).unwrap(), b"keep these bytes");
}

#[test]
fn a_save_target_is_not_a_file_to_keep_or_to_read() {
    let h = harness();
    let out = tempfile::tempdir().unwrap();
    let target = out.path().join("chosen.nocturne");
    h.state().allow_save_target(&target);

    assert_eq!(
        h.code("probe_backup", json!({"path": target.to_str().unwrap()})),
        "invalid"
    );
    assert_eq!(
        h.code(
            "check_backup",
            json!({"path": target.to_str().unwrap(), "passphrase": PASS}),
        ),
        "invalid"
    );
    assert_eq!(
        h.code(
            "keep_files",
            json!({"paths": [target.to_str().unwrap()], "source": "drop"}),
        ),
        "invalid"
    );
}

#[test]
fn a_restore_source_is_not_a_save_target_or_a_file_to_keep() {
    let h = harness();
    let src = tempfile::tempdir().unwrap();
    let file = write_file(src.path(), "backup.nocturne", b"not really a backup");
    h.state().allow_restore_source(&file);

    assert_eq!(
        h.code(
            "make_backup",
            json!({"path": file.to_str().unwrap(), "passphrase": PASS}),
        ),
        "invalid"
    );
    assert_eq!(
        h.code(
            "keep_files",
            json!({"paths": [file.to_str().unwrap()], "source": "drop"}),
        ),
        "invalid"
    );
    assert_eq!(std::fs::read(&file).unwrap(), b"not really a backup");
}

#[test]
fn a_save_target_works_once_and_a_cancelled_run_keeps_it() {
    let h = harness();
    lived_in(&h);
    let out = tempfile::tempdir().unwrap();
    let dest = out.path().join("once.nocturne");
    h.state().allow_save_target(&dest);
    h.ok(
        "make_backup",
        json!({"path": dest.to_str().unwrap(), "passphrase": PASS}),
    );
    let first = std::fs::read(&dest).unwrap();
    assert_eq!(
        h.code(
            "make_backup",
            json!({"path": dest.to_str().unwrap(), "passphrase": PASS}),
        ),
        "invalid",
        "the grant was used up"
    );
    assert_eq!(
        std::fs::read(&dest).unwrap(),
        first,
        "the first file is untouched"
    );

    let retry = out.path().join("retry.nocturne");
    h.state().allow_save_target(&retry);
    let handle = h.app.handle().clone();
    let cancelled = Arc::new(AtomicBool::new(false));
    let flag = cancelled.clone();
    h.app.listen(EVENT_BACKUP, move |_| {
        if !flag.swap(true, Ordering::SeqCst) {
            handle.state::<AppState>().cancel_backup();
        }
    });
    assert_eq!(
        h.code(
            "make_backup",
            json!({"path": retry.to_str().unwrap(), "passphrase": PASS}),
        ),
        "cancelled"
    );
    assert!(!retry.exists());
    h.ok(
        "make_backup",
        json!({"path": retry.to_str().unwrap(), "passphrase": PASS}),
    );
    assert!(retry.is_file(), "the failed run kept the grant");
    assert_eq!(
        h.code(
            "make_backup",
            json!({"path": retry.to_str().unwrap(), "passphrase": PASS}),
        ),
        "invalid"
    );
}

#[test]
fn a_malformed_reading_setup_is_refused_only_when_restoring() {
    let h = harness();
    let (_photo, _original) = lived_in(&h);
    let out = tempfile::tempdir().unwrap();
    let bad = write_file(out.path(), "selection.json", b"not a reading setup");
    let dest = out.path().join("bad.nocturne");
    let cancel = AtomicBool::new(false);
    let extras = [("engine/selection.json", bad.as_path())];
    let run = nocturne_engine::BackupRun {
        extras: &extras,
        progress: &|_, _, _| {},
        cancel: &cancel,
        started: &|_| {},
        work_factor: Some(10),
    };
    h.engine().backup_to(&dest, PASS, &run).unwrap();

    h.state().allow_restore_source(&dest);
    h.ok(
        "check_backup",
        json!({"path": dest.to_str().unwrap(), "passphrase": PASS}),
    );
    let items = h.engine().items(Default::default()).unwrap();
    assert_eq!(
        h.code(
            "restore_backup",
            json!({"path": dest.to_str().unwrap(), "passphrase": PASS, "replace": true}),
        ),
        "damagedBackup"
    );
    assert!(!restore::stage_dir(h.dir.path()).exists());
    assert!(!restore::marker_path(h.dir.path()).exists());
    assert_eq!(h.engine().items(Default::default()).unwrap(), items);
}

#[test]
fn a_reading_setup_from_a_backup_loses_its_pending_download() {
    use nocturne_contract::{ComponentRole, Tier};
    use nocturne_lemonade::selection::{OldModel, Pending, Selection};

    let h = harness();
    let data = h.dir.path();
    lived_in(&h);
    let mut selection = Selection::preset(Tier::Standard);
    selection
        .custom
        .insert(ComponentRole::Mind, "a-model.gguf".into());
    selection.pending = Some(Pending {
        role: ComponentRole::Mind,
        model: "pending.gguf".into(),
        from: "current.gguf".into(),
        paused: false,
        failed: false,
    });
    selection.old = Some(OldModel {
        role: ComponentRole::Mind,
        model: "old.gguf".into(),
    });
    std::fs::create_dir_all(data.join("engine/config")).unwrap();
    std::fs::write(
        data.join("engine/config/selection.json"),
        serde_json::to_vec(&selection).unwrap(),
    )
    .unwrap();

    let out = tempfile::tempdir().unwrap();
    let dest = out.path().join("chosen.nocturne");
    save_to(&h, &dest);
    check(&h, &dest);
    h.ok(
        "restore_backup",
        json!({"path": dest.to_str().unwrap(), "passphrase": PASS, "replace": true}),
    );
    let staged: Selection = serde_json::from_slice(
        &std::fs::read(restore::stage_dir(data).join("engine/selection.json")).unwrap(),
    )
    .unwrap();
    assert!(staged.pending.is_none() && staged.old.is_none());
    assert_eq!(
        staged.custom.get(&ComponentRole::Mind).map(String::as_str),
        Some("a-model.gguf")
    );

    // A start writes the same clean setup into place, even from a raw unpack.
    let b = tempfile::tempdir().unwrap();
    let stage = restore::stage_dir(b.path());
    std::fs::create_dir_all(stage.parent().unwrap()).unwrap();
    let summary =
        nocturne_engine::unpack_backup(&dest, PASS, &stage, &|_, _, _| {}, &AtomicBool::new(false))
            .unwrap();
    let raw: Selection =
        serde_json::from_slice(&std::fs::read(stage.join("engine/selection.json")).unwrap())
            .unwrap();
    assert!(raw.pending.is_some(), "the file itself still carries it");
    restore::schedule(
        b.path(),
        false,
        "2026-11-15 120000",
        Source::Stage,
        None,
        Some(summary),
    )
    .unwrap();
    assert_eq!(
        restore::apply_pending(b.path()),
        ApplyOutcome::Restored { kept: None }
    );
    let installed: Selection = serde_json::from_slice(
        &std::fs::read(b.path().join("engine/config/selection.json")).unwrap(),
    )
    .unwrap();
    assert!(
        installed.pending.is_none(),
        "the start drops the old download"
    );
    assert_eq!(
        installed
            .custom
            .get(&ComponentRole::Mind)
            .map(String::as_str),
        Some("a-model.gguf")
    );
}
