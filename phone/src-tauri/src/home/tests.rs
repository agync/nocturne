use std::path::Path;
use std::sync::Arc;

use nocturne_contract::{
    BackupPhase, CheckInInput, Feeling, FeelingPart, FeelingPoint, ForgetScope, HomeAnswer,
    HomeCall, HomeError, ItemKind, ItemStatus, PhoneError, PhoneMode, RemoteAnswer, RemoteCall,
};
use nocturne_engine::ManualClock;

use super::voice::{Heard, Reader};
use super::{Home, dispatch};

const PASS: &str = "a long enough passphrase";
const NOW: &str = "2026-11-22T09:00:00Z";

fn home_at(dir: &Path, clock: Arc<ManualClock>) -> Home {
    let mut home = Home::new(dir.to_path_buf(), clock, Box::new(|_: &Path| {}));
    home.set_work_factor(10);
    home
}

fn clock() -> Arc<ManualClock> {
    Arc::new(ManualClock::at(NOW, "Europe/London"))
}

fn alone(dir: &Path) -> (Home, Arc<ManualClock>) {
    let clock = clock();
    let home = home_at(dir, clock.clone());
    home.start_alone("Pixel", false).unwrap();
    (home, clock)
}

fn call(home: &Home, call: RemoteCall) -> RemoteAnswer {
    dispatch::answer(&home.open_store().unwrap(), call)
}

fn no_progress(_: BackupPhase, _: u64, _: u64) {}

fn local(home: &Home, c: HomeCall) -> HomeAnswer {
    home.local_call(c, false, &no_progress)
}

fn note(text: &str, client_id: &str) -> RemoteCall {
    RemoteCall::KeepText {
        text: text.into(),
        client_id: client_id.into(),
        feeling: None,
        good: None,
        captured_at: None,
        web: false,
    }
}

fn kept_id(answer: &RemoteAnswer) -> String {
    match answer {
        RemoteAnswer::Kept { kept } => kept.item.id.clone(),
        other => panic!("not kept: {other:?}"),
    }
}

fn feeling() -> Feeling {
    Feeling {
        part: FeelingPart::Settled,
        point: FeelingPoint {
            pleasant: 0.5,
            energy: -0.3,
        },
        words: vec!["calm".into()],
    }
}

fn found(home: &Home, query: &str) -> Vec<String> {
    match local(
        home,
        HomeCall::Find {
            query: query.into(),
        },
    ) {
        HomeAnswer::Found { items } => items.into_iter().map(|f| f.receipt.item_id).collect(),
        other => panic!("not found: {other:?}"),
    }
}

fn stats(home: &Home) -> nocturne_contract::HomeStats {
    match local(home, HomeCall::Stats) {
        HomeAnswer::Stats { stats } => stats,
        other => panic!("no stats: {other:?}"),
    }
}

/// A WAV file of plain sound as the phone's reader takes it.
fn wav(samples: usize) -> Vec<u8> {
    let data = vec![0u8; samples * 2];
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&16_000u32.to_le_bytes());
    out.extend_from_slice(&32_000u32.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&data);
    out
}

fn recording(mime: &str, client_id: &str, size: usize) -> RemoteCall {
    RemoteCall::KeepRecording {
        mime: mime.into(),
        size: size as f64,
        client_id: client_id.into(),
        feeling: None,
        good: None,
        captured_at: None,
    }
}

#[test]
fn a_phone_starts_alone() {
    let dir = tempfile::tempdir().unwrap();
    let home = home_at(dir.path(), clock());
    assert_eq!(home.mode(false), None);
    assert_eq!(home.mode(true), Some(PhoneMode::Companion));
    assert!(
        home.with_local(|_| ()).is_none(),
        "nothing opens before the choice"
    );
    assert_eq!(home.start_alone("   ", false), Err(HomeError::Invalid));
    assert_eq!(home.start_alone("Pixel", true), Err(HomeError::NotReady));

    home.start_alone("  Pixel  ", false).unwrap();
    assert_eq!(home.mode(false), Some(PhoneMode::Alone));
    assert_eq!(home.mode(true), Some(PhoneMode::Alone));
    assert!(dir.path().join("store").join("nocturne.db").is_file());
    let (device, row) = home
        .with_local(|o| (o.device.clone(), o.engine.local_device().unwrap().unwrap()))
        .expect("the sync gets the open store");
    assert_eq!(row.id, device);
    assert_eq!(row.name, "Pixel");

    // Renamed in place while open: the same row.
    home.start_alone("Fold", false).unwrap();
    let renamed = home
        .with_local(|o| {
            (
                o.device.clone(),
                o.engine.local_device().unwrap().unwrap().name,
            )
        })
        .unwrap();
    assert_eq!(renamed, (device.clone(), "Fold".to_string()));

    // Again, after a restart: the same row.
    let again = home_at(dir.path(), clock());
    assert_eq!(again.with_local(|o| o.device.clone()), Some(device));
}

#[test]
fn keeps_journal_and_check_ins_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let (home, clock) = alone(dir.path());

    let first = call(&home, note("Bins go out on Thursday", "c-1"));
    let id = kept_id(&first);
    let RemoteAnswer::Kept { kept } = &first else {
        unreachable!()
    };
    assert!(!kept.duplicate);
    assert_eq!(kept.item.text.as_deref(), Some("Bins go out on Thursday"));
    assert_eq!(kept.take_back_until, "2026-11-22T09:10:00.000Z");

    // Sent twice from the offline queue: kept once.
    let again = call(&home, note("Bins go out on Thursday", "c-1"));
    assert_eq!(kept_id(&again), id);
    assert!(matches!(again, RemoteAnswer::Kept { kept } if kept.duplicate));

    let RemoteAnswer::JournalDay { day } = call(&home, RemoteCall::GetJournalDay { date: None })
    else {
        panic!("no journal day")
    };
    assert_eq!(day.day, "2026-11-22");
    assert!(
        day.rows.iter().any(|r| r.item.id == id),
        "on today's journal"
    );
    assert_eq!(
        day.rows
            .iter()
            .find(|r| r.item.id == id)
            .unwrap()
            .item
            .status,
        ItemStatus::Kept
    );

    let check_in = RemoteCall::KeepCheckIn {
        input: CheckInInput {
            feeling: feeling(),
            line: Some("a quiet morning".into()),
            urge: None,
            replaces: None,
        },
        client_id: "c-2".into(),
        captured_at: None,
    };
    let RemoteAnswer::CheckIn { checkin } = call(&home, check_in.clone()) else {
        panic!("no check-in")
    };
    let RemoteAnswer::CheckIn { checkin: twice } = call(&home, check_in) else {
        panic!("no check-in again")
    };
    assert_eq!(twice.id, checkin.id, "kept once");
    let RemoteAnswer::JournalDay { day } = call(
        &home,
        RemoteCall::GetJournalDay {
            date: Some("2026-11-22".into()),
        },
    ) else {
        panic!("no journal day")
    };
    assert_eq!(day.checkins.len(), 1);

    let RemoteAnswer::Item { item } = call(&home, RemoteCall::GetItem { id: id.clone() }) else {
        panic!("no item")
    };
    assert_eq!(item.title, "Bins go out on Thursday");

    // A photo, its bytes put together from the screens' slices.
    let photo = RemoteCall::KeepBytes {
        mime: "image/png".into(),
        size: 4.0,
        client_id: "c-3".into(),
        caption: Some("Receipt / ../x".into()),
        feeling: None,
        good: None,
        captured_at: None,
    };
    let open = home.open_store().unwrap();
    let answer = dispatch::keep_bytes(&open, photo, b"\x89PNG", &home.keeping_dir());
    let photo_id = kept_id(&answer);
    let RemoteAnswer::Kept { kept } = &answer else {
        unreachable!()
    };
    assert_eq!(kept.item.kind, ItemKind::Image);
    assert!(kept.item.has_original);
    let (mime, bytes) = dispatch::original(&open, &photo_id).unwrap();
    assert_eq!(mime, "image/png");
    assert_eq!(bytes, b"\x89PNG");
    assert!(
        std::fs::read_dir(home.keeping_dir())
            .unwrap()
            .next()
            .is_none(),
        "the scratch copy goes"
    );

    let RemoteAnswer::Page { page } = call(&home, RemoteCall::GetPage { date: None }) else {
        panic!("no page")
    };
    assert_eq!(page.date, "2026-11-22");
    assert!(
        matches!(call(&home, RemoteCall::ListPages), RemoteAnswer::Pages { pages } if pages.is_empty())
    );
    assert!(matches!(
        call(
            &home,
            RemoteCall::Ask {
                question: "when are bins?".into(),
                place: None,
            }
        ),
        RemoteAnswer::Error {
            code: PhoneError::NotReady
        }
    ));

    // Taken back within its window; not after it.
    assert_eq!(
        call(&home, RemoteCall::Unkeep { id: photo_id }),
        RemoteAnswer::Done
    );
    clock.set_str("2026-11-22T09:11:00Z");
    assert_eq!(
        call(&home, RemoteCall::Unkeep { id }),
        RemoteAnswer::Error {
            code: PhoneError::TooLate
        }
    );
}

#[test]
fn finds_by_words_but_never_a_check_in() {
    let dir = tempfile::tempdir().unwrap();
    let (home, _) = alone(dir.path());
    assert!(found(&home, "giraffe").is_empty());
    let id = kept_id(&call(
        &home,
        note("The purple giraffes bought lemons", "c-1"),
    ));
    call(
        &home,
        RemoteCall::KeepCheckIn {
            input: CheckInInput {
                feeling: feeling(),
                line: Some("thinking of giraffes".into()),
                urge: None,
                replaces: None,
            },
            client_id: "c-2".into(),
            captured_at: None,
        },
    );
    assert_eq!(found(&home, "giraffe"), vec![id]);
    assert!(found(&home, "   ").is_empty());
}

#[test]
fn forgets_after_a_preview() {
    let dir = tempfile::tempdir().unwrap();
    let (home, _) = alone(dir.path());
    let id = kept_id(&call(&home, note("A secret about the lemons", "c-1")));
    let HomeAnswer::ForgetPreview { preview } = local(
        &home,
        HomeCall::PreviewForget {
            scope: ForgetScope {
                ids: Vec::new(),
                include_originals: true,
                also: Some(vec![id.clone()]),
                pick: None,
            },
        },
    ) else {
        panic!("no preview")
    };
    assert_eq!(preview.originals.len(), 1);
    assert_eq!(preview.originals[0].item_id, id);
    let token = preview.token.clone();
    let HomeAnswer::Forgotten { report } = local(
        &home,
        HomeCall::Forget {
            token: token.clone(),
        },
    ) else {
        panic!("not forgotten")
    };
    assert_eq!(report.originals_removed, 1);
    assert!(found(&home, "lemons").is_empty());
    assert_eq!(
        local(&home, HomeCall::Forget { token }),
        HomeAnswer::Error {
            code: HomeError::TooLate
        }
    );
}

#[test]
fn a_backup_comes_back_on_another_phone() {
    let one = tempfile::tempdir().unwrap();
    let (home, _) = alone(one.path());
    call(&home, note("Bins go out on Thursday", "c-1"));
    call(&home, note("The purple giraffe", "c-2"));
    let open = home.open_store().unwrap();
    kept_id(&dispatch::keep_bytes(
        &open,
        RemoteCall::KeepBytes {
            mime: "image/jpeg".into(),
            size: 3.0,
            client_id: "c-3".into(),
            caption: None,
            feeling: None,
            good: None,
            captured_at: None,
        },
        b"jpg",
        &home.keeping_dir(),
    ));
    assert_eq!(stats(&home).last_backup, None);
    for inside in [
        "store/b.nocturne",
        "b.nocturne",
        "store-put-aside-x/b.nocturne",
    ] {
        let dest = one.path().join(inside);
        std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
        assert_eq!(
            local(
                &home,
                HomeCall::SaveBackup {
                    dest: dest.to_string_lossy().into(),
                    passphrase: PASS.into(),
                }
            ),
            HomeAnswer::Error {
                code: HomeError::Invalid
            },
            "{inside}"
        );
    }
    std::fs::remove_dir(one.path().join("store-put-aside-x")).unwrap();

    let saved = tempfile::tempdir().unwrap();
    let file = saved.path().join("Nocturne backup.nocturne");
    let HomeAnswer::BackupSaved { report } = local(
        &home,
        HomeCall::SaveBackup {
            dest: file.to_string_lossy().into(),
            passphrase: PASS.into(),
        },
    ) else {
        panic!("no backup")
    };
    assert!(file.is_file());
    let before = stats(&home);
    assert_eq!(
        before.last_backup.as_ref().map(|b| b.bytes),
        Some(report.bytes)
    );
    assert_eq!(before.counts.originals, 1);

    // A new phone that has not chosen yet.
    let two = tempfile::tempdir().unwrap();
    let other = home_at(two.path(), clock());
    let wrong = local(
        &other,
        HomeCall::CheckBackup {
            file: file.to_string_lossy().into(),
            passphrase: "not the passphrase at all".into(),
        },
    );
    assert_eq!(
        wrong,
        HomeAnswer::Error {
            code: HomeError::WrongPassphrase
        }
    );
    let HomeAnswer::BackupChecked { check } = local(
        &other,
        HomeCall::CheckBackup {
            file: file.to_string_lossy().into(),
            passphrase: PASS.into(),
        },
    ) else {
        panic!("not checked")
    };
    assert_eq!(check.here, None, "nothing here to replace");
    assert_eq!(check.summary.originals, 1);
    // A companion's memory is on its computer.
    assert_eq!(
        other.local_call(
            HomeCall::RestoreBackup {
                file: file.to_string_lossy().into(),
                passphrase: PASS.into(),
                replace: false,
            },
            true,
            &no_progress
        ),
        HomeAnswer::Error {
            code: HomeError::NotReady
        }
    );
    assert_eq!(
        local(
            &other,
            HomeCall::RestoreBackup {
                file: file.to_string_lossy().into(),
                passphrase: PASS.into(),
                replace: false,
            }
        ),
        HomeAnswer::Restored
    );
    assert_eq!(other.mode(false), Some(PhoneMode::Alone));
    let after = stats(&other);
    assert_eq!(after.counts, before.counts);
    assert_eq!(after.last_kept, before.last_kept);
    assert_eq!(found(&other, "giraffe").len(), 1);
    assert!(!two.path().join("restore-stage").exists());

    // Over a memory that holds something: only when the person said so, and what was here is
    // put aside.
    call(&other, note("Kept after the restore", "c-9"));
    assert_eq!(
        local(
            &other,
            HomeCall::RestoreBackup {
                file: file.to_string_lossy().into(),
                passphrase: PASS.into(),
                replace: false,
            }
        ),
        HomeAnswer::Error {
            code: HomeError::Invalid
        }
    );
    assert_eq!(
        local(
            &other,
            HomeCall::RestoreBackup {
                file: file.to_string_lossy().into(),
                passphrase: PASS.into(),
                replace: true,
            }
        ),
        HomeAnswer::Restored
    );
    assert_eq!(other.put_aside_names().len(), 1);
    assert!(found(&other, "restore").is_empty());
    assert_eq!(stats(&other).counts, before.counts);
}

#[test]
fn a_damaged_store_is_put_aside() {
    let dir = tempfile::tempdir().unwrap();
    let (home, _) = alone(dir.path());
    call(&home, note("Bins go out on Thursday", "c-1"));
    home.close();
    let store = dir.path().join("store");
    for side in ["nocturne.db-wal", "nocturne.db-shm"] {
        let _ = std::fs::remove_file(store.join(side));
    }
    std::fs::write(store.join("nocturne.db"), vec![0x5au8; 8192]).unwrap();

    let again = home_at(dir.path(), clock());
    assert!(again.with_local(|_| ()).is_none());
    let aside = again.damaged().expect("marked as damaged");
    assert!(aside.starts_with("store-put-aside-2026-11-22-"), "{aside}");
    assert_eq!(
        std::fs::read(dir.path().join(&aside).join("nocturne.db")).unwrap(),
        vec![0x5au8; 8192],
        "nothing deleted"
    );
    assert!(!store.exists());
    // Still not opened over it after a restart.
    let later = home_at(dir.path(), clock());
    assert!(later.with_local(|_| ()).is_none());
    assert_eq!(later.damaged().as_deref(), Some(aside.as_str()));

    // Starting over makes a fresh memory; the damaged one stays aside.
    later.start_alone("Pixel", false).unwrap();
    assert!(later.damaged().is_none());
    assert!(found(&later, "bins").is_empty());
    assert!(dir.path().join(&aside).is_dir());
}

#[test]
fn delete_everything_needs_the_phrase_and_takes_copies_too() {
    let dir = tempfile::tempdir().unwrap();
    let (home, _) = alone(dir.path());
    call(&home, note("Bins go out on Thursday", "c-1"));
    std::fs::create_dir_all(dir.path().join("store-put-aside-2026-01-01-000000")).unwrap();
    assert_eq!(
        local(
            &home,
            HomeCall::DeleteEverything {
                phrase: "delete".into()
            }
        ),
        HomeAnswer::Error {
            code: HomeError::WrongPhrase
        }
    );
    assert_eq!(
        local(
            &home,
            HomeCall::DeleteEverything {
                phrase: nocturne_engine::DELETE_PHRASE.into()
            }
        ),
        HomeAnswer::Deleted
    );
    assert_eq!(stats(&home).counts.items, 0);
    assert!(home.put_aside_names().is_empty());
    let (device, row) = home
        .with_local(|o| (o.device.clone(), o.engine.local_device().unwrap()))
        .unwrap();
    assert_eq!(row.map(|r| r.id), Some(device), "its own row, made again");
    kept_id(&call(&home, note("A fresh start", "c-1")));
}

struct Scripted {
    ready: bool,
    heard: Heard,
}

impl Reader for Scripted {
    fn ready(&self, _: &str) -> bool {
        self.ready
    }

    fn read(&self, pcm: &Path, language: &str) -> Heard {
        assert_eq!(language, "en-GB");
        assert_eq!(std::fs::metadata(pcm).unwrap().len(), 32_000, "header off");
        self.heard.clone()
    }
}

#[test]
fn a_voice_note_gets_its_words_and_stays_waiting_for_a_mind() {
    let dir = tempfile::tempdir().unwrap();
    let (home, _) = alone(dir.path());
    let open = home.open_store().unwrap();
    let sound = wav(16_000);
    let id = kept_id(&dispatch::keep_bytes(
        &open,
        recording("audio/wav", "c-1", sound.len()),
        &sound,
        &home.keeping_dir(),
    ));
    let other = kept_id(&dispatch::keep_bytes(
        &open,
        recording("audio/webm", "c-2", 4),
        b"\x1a\x45\xdf\xa3",
        &home.keeping_dir(),
    ));
    let words = Scripted {
        ready: true,
        heard: Heard::Words("the purple giraffe".into()),
    };
    assert_eq!(home.hear_waiting(&words), 0, "no language yet");
    assert!(!home.set_language("en_GB"));
    assert!(home.set_language("en-GB"));
    let not_ready = Scripted {
        ready: false,
        heard: Heard::Words("never".into()),
    };
    assert_eq!(home.hear_waiting(&not_ready), 0);
    assert_eq!(home.hear_waiting(&words), 1);

    let item = open.engine.get_item(&id).unwrap();
    assert_eq!(item.text.as_deref(), Some("the purple giraffe"));
    assert_eq!(item.duration_ms, Some(1000));
    assert_eq!(item.status, ItemStatus::Kept, "still waiting for a mind");
    assert_eq!(found(&home, "giraffe"), vec![id.clone()]);
    assert_eq!(
        open.engine.get_item(&other).unwrap().text,
        None,
        "a webm waits"
    );
    assert_eq!(home.hear_waiting(&words), 0, "read once");
}

#[test]
fn nothing_heard_is_not_read_again_this_run() {
    let dir = tempfile::tempdir().unwrap();
    let (home, _) = alone(dir.path());
    let open = home.open_store().unwrap();
    let sound = wav(16_000);
    let id = kept_id(&dispatch::keep_bytes(
        &open,
        recording("audio/wav", "c-1", sound.len()),
        &sound,
        &home.keeping_dir(),
    ));
    home.set_language("en-GB");
    let silent = Scripted {
        ready: true,
        heard: Heard::Nothing,
    };
    assert_eq!(home.hear_waiting(&silent), 0);
    let item = open.engine.get_item(&id).unwrap();
    assert_eq!(item.text, None);
    assert_eq!(item.status, ItemStatus::Kept);
    let words = Scripted {
        ready: true,
        heard: Heard::Words("late words".into()),
    };
    assert_eq!(home.hear_waiting(&words), 0);
}

#[test]
fn a_restore_waits_for_every_use_to_end() {
    let saved = tempfile::tempdir().unwrap();
    let file = saved.path().join("b.nocturne");
    let one = tempfile::tempdir().unwrap();
    let (home, _) = alone(one.path());
    call(&home, note("Bins go out on Thursday", "c-1"));
    assert!(matches!(
        local(
            &home,
            HomeCall::SaveBackup {
                dest: file.to_string_lossy().into(),
                passphrase: PASS.into(),
            }
        ),
        HomeAnswer::BackupSaved { .. }
    ));
    let done = std::sync::atomic::AtomicBool::new(false);
    std::thread::scope(|s| {
        let (held, open) = home.using().unwrap();
        let restore = s.spawn(|| {
            let answer = local(
                &home,
                HomeCall::RestoreBackup {
                    file: file.to_string_lossy().into(),
                    passphrase: PASS.into(),
                    replace: true,
                },
            );
            done.store(true, std::sync::atomic::Ordering::SeqCst);
            answer
        });
        std::thread::sleep(std::time::Duration::from_millis(1500));
        assert!(
            !done.load(std::sync::atomic::Ordering::SeqCst),
            "waits for the use"
        );
        // The use still works on the store it opened.
        assert!(matches!(
            dispatch::answer(&open, note("Still here", "c-2")),
            RemoteAnswer::Kept { .. }
        ));
        // A use ends with the store it opened as well as its place at the gate: a store still
        // held open cannot be moved on every system.
        drop(open);
        drop(held);
        assert_eq!(restore.join().unwrap(), HomeAnswer::Restored);
    });
    let aside = home.put_aside_names();
    assert_eq!(
        aside.len(),
        1,
        "what was here, with the late keep, is put aside"
    );
    assert!(found(&home, "still").is_empty());
    assert_eq!(found(&home, "bins").len(), 1);
}

#[test]
fn files_from_the_screens_never_land_among_the_apps_own() {
    let dir = tempfile::tempdir().unwrap();
    let (home, _) = alone(dir.path());
    assert_eq!(
        local(
            &home,
            HomeCall::Export {
                dest: dir.path().join("store").to_string_lossy().into()
            }
        ),
        HomeAnswer::Error {
            code: HomeError::Invalid
        }
    );
    let staged = dir.path().join("restore-check");
    std::fs::create_dir_all(&staged).unwrap();
    std::fs::write(staged.join("b.nocturne"), b"x").unwrap();
    assert_eq!(
        local(
            &home,
            HomeCall::CheckBackup {
                file: staged.join("b.nocturne").to_string_lossy().into(),
                passphrase: PASS.into(),
            }
        ),
        HomeAnswer::Error {
            code: HomeError::Invalid
        }
    );
    let out = tempfile::tempdir().unwrap();
    assert!(matches!(
        local(
            &home,
            HomeCall::Export {
                dest: out.path().to_string_lossy().into()
            }
        ),
        HomeAnswer::Exported { .. }
    ));
}
