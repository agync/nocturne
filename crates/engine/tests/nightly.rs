mod common;

use std::sync::atomic::AtomicBool;

use common::*;
use nocturne_contract::{
    BackupPhase, CalendarBatch, CalendarChange, CalendarEntry, Edition, Kept, PhoneCalendar,
    Reading, ReadingBatch, ReadingChange, ReadingValue, Source, TimeSpan,
};
use nocturne_engine::nightly::{
    self, BackupPlace, FolderPlace, KEY_PHRASE, KEY_RECOVERY, MemoryPlace, NightError, NightKey,
    NightRun, NightState, PlaceError,
};
use nocturne_engine::{Engine, Predicate};

const PHRASE: &str = "harbour violet pencil maple drift oven";
/// Passphrase work for test keys, so they open quickly.
const WORK: Option<u8> = Some(4);

fn noop(_: BackupPhase, _: u64, _: u64) {}

fn run_with(check: bool, f: impl FnOnce(&NightRun)) {
    let cancel = AtomicBool::new(false);
    f(&NightRun {
        progress: &noop,
        cancel: &cancel,
        check,
    });
}

fn night(
    engine: &Engine,
    place: &dyn BackupPlace,
    key: &NightKey,
    state: &mut NightState,
) -> Result<nightly::NightReport, NightError> {
    let mut out = None;
    run_with(false, |run| {
        out = Some(engine.back_up_night(place, key, state, run));
    });
    out.unwrap()
}

fn keep_photo(engine: &Engine, bytes: &[u8], name: &str) -> String {
    let src = tempfile::tempdir().unwrap();
    match engine
        .keep_file(&write_file(src.path(), name, bytes), Source::Screenshot)
        .unwrap()
    {
        Kept::New { item } | Kept::Duplicate { item } => item.id,
    }
}

fn noise(len: usize, mut x: u64) -> Vec<u8> {
    (0..len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x as u8
        })
        .collect()
}

fn names(place: &MemoryPlace, prefix: &str) -> Vec<String> {
    place
        .names()
        .into_iter()
        .filter(|n| n.starts_with(prefix))
        .collect()
}

fn restore(
    place: &dyn BackupPlace,
    key: &NightKey,
) -> Result<
    (
        tempfile::TempDir,
        nocturne_contract::BackupSummary,
        NightState,
    ),
    NightError,
> {
    let out = tempfile::tempdir().unwrap();
    let stage = out.path().join("stage");
    let mut result = None;
    run_with(false, |run| {
        result = Some(nightly::restore_night(place, key, None, &stage, run));
    });
    let unpacked = result.unwrap()?;
    Ok((out, unpacked.summary, unpacked.state))
}

#[test]
fn a_night_comes_back_on_a_new_phone_whole() {
    let f = fixture();
    keep(&f.engine, "The plumber comes on Tuesday.");
    let photo = noise(700_000, 3);
    keep_photo(&f.engine, &photo, "receipt.jpg");
    let place = MemoryPlace::new();
    let key = nightly::turn_on(&place, PHRASE, WORK).unwrap();
    let mut state = NightState::new();
    let mut report = None;
    run_with(true, |run| {
        report = Some(
            f.engine
                .back_up_night(&place, &key, &mut state, run)
                .unwrap(),
        );
    });
    let report = report.unwrap();
    assert_eq!((report.items, report.originals, report.count), (2, 1, 1));
    assert_eq!(state.count, 1);
    for n in place.names() {
        assert!(
            !n.contains("plumber") && !n.contains("receipt"),
            "names say nothing: {n}"
        );
    }

    let opened = nightly::open_with_phrase(&place, PHRASE).unwrap();
    let (out, summary, restored) = restore(&place, &opened).unwrap();
    assert_eq!((summary.items, summary.originals), (2, 1));
    assert!(
        !restored.in_use,
        "a restored phone waits to be the one in use"
    );
    assert_ne!(restored.device, state.device);
    let again = Engine::open(&out.path().join("stage/store"), f.clock.clone()).unwrap();
    assert_eq!(again.find_items("plumber", 5).unwrap().len(), 1);
    let originals: Vec<_> = std::fs::read_dir(out.path().join("stage/store/originals"))
        .unwrap()
        .flatten()
        .collect();
    assert_eq!(originals.len(), 1);
    assert_eq!(std::fs::read(originals[0].path()).unwrap(), photo);
}

#[test]
fn the_next_night_sends_only_what_changed() {
    let f = fixture();
    keep_photo(&f.engine, &noise(3_000_000, 5), "trip.jpg");
    let place = MemoryPlace::new();
    let key = nightly::turn_on(&place, PHRASE, WORK).unwrap();
    let mut state = NightState::new();
    let first = night(&f.engine, &place, &key, &mut state).unwrap();
    keep(&f.engine, "Bring the blue folder.");
    let second = night(&f.engine, &place, &key, &mut state).unwrap();
    assert!(
        second.sent * 4 < first.sent,
        "the photo is not sent again: {} then {}",
        first.sent,
        second.sent
    );
    let third = night(&f.engine, &place, &key, &mut state).unwrap();
    assert_eq!(third.count, 3);
    assert!(third.sent <= second.sent);
}

#[test]
fn a_wrong_phrase_is_refused_and_a_recovery_key_opens_it() {
    let f = fixture();
    keep(&f.engine, "Something to keep.");
    let place = MemoryPlace::new();
    let key = nightly::turn_on(&place, PHRASE, WORK).unwrap();
    night(&f.engine, &place, &key, &mut NightState::new()).unwrap();

    assert!(matches!(
        nightly::open_with_phrase(&place, "harbour violet pencil maple drift"),
        Err(NightError::WrongPhrase)
    ));
    assert!(matches!(
        nightly::open_with_phrase(&place, ""),
        Err(NightError::WrongPhrase)
    ));
    assert!(
        nightly::open_with_phrase(&place, " Harbour violet pencil maple drift OVEN ")
            .unwrap()
            .same_as(&key),
        "a capital a keyboard added still opens it"
    );
    assert!(matches!(
        nightly::open_with_recovery_key(&place, "2345-6789-ABCD-EFGH-JKMN-PQRS"),
        Err(NightError::WrongPhrase)
    ));

    let recovery = nightly::add_recovery_key(&place, &key, WORK).unwrap();
    assert!(nightly::has_recovery_key(&place).unwrap());
    let opened = nightly::open_with_recovery_key(&place, &recovery.to_lowercase()).unwrap();
    assert!(opened.same_as(&key));
    assert!(matches!(
        nightly::open_with_recovery_key(&place, "2345-6789-ABCD-EFGH-JKMN-PQRS"),
        Err(NightError::WrongPhrase)
    ));
    assert!(matches!(
        nightly::open_with_recovery_key(&place, "not a key"),
        Err(NightError::WrongPhrase)
    ));
    restore(&place, &opened).unwrap();

    nightly::remove_recovery_key(&place).unwrap();
    assert!(matches!(
        nightly::open_with_recovery_key(&place, &recovery),
        Err(NightError::WrongPhrase)
    ));
}

#[test]
fn a_new_phrase_replaces_the_old_and_the_recovery_key_still_opens() {
    let place = MemoryPlace::new();
    let key = nightly::turn_on(&place, PHRASE, WORK).unwrap();
    let recovery = nightly::add_recovery_key(&place, &key, WORK).unwrap();
    nightly::change_phrase(&place, &key, "lantern copper meadow", WORK).unwrap();
    assert!(matches!(
        nightly::open_with_phrase(&place, PHRASE),
        Err(NightError::WrongPhrase)
    ));
    assert!(
        nightly::open_with_phrase(&place, "lantern copper meadow")
            .unwrap()
            .same_as(&key)
    );
    assert!(
        nightly::open_with_recovery_key(&place, &recovery)
            .unwrap()
            .same_as(&key)
    );
    assert!(matches!(
        nightly::change_phrase(&place, &key, "  ", WORK),
        Err(NightError::NoPhrase)
    ));
}

#[test]
fn another_key_cannot_take_over_the_phrase() {
    let f = fixture();
    keep(&f.engine, "Something to keep.");
    let place = MemoryPlace::new();
    let key = nightly::turn_on(&place, PHRASE, WORK).unwrap();
    night(&f.engine, &place, &key, &mut NightState::new()).unwrap();
    assert!(matches!(
        nightly::change_phrase(&place, &NightKey::generate(), "new words", WORK),
        Err(NightError::Damaged)
    ));
}

#[test]
fn turning_on_over_a_backup_is_refused_and_an_empty_account_has_none() {
    let place = MemoryPlace::new();
    assert!(nightly::found(&place).unwrap().is_none());
    assert!(matches!(
        nightly::open_with_phrase(&place, PHRASE),
        Err(NightError::NoBackup)
    ));
    assert!(matches!(
        nightly::open_with_recovery_key(&place, "2345-6789-ABCD-EFGH-JKMN-PQRS"),
        Err(NightError::NoBackup)
    ));
    assert!(matches!(
        nightly::turn_on(&place, "", WORK),
        Err(NightError::NoPhrase)
    ));
    nightly::turn_on(&place, PHRASE, WORK).unwrap();
    assert!(matches!(
        nightly::turn_on(&place, PHRASE, WORK),
        Err(NightError::AlreadyThere)
    ));
}

#[test]
fn what_an_account_shows_before_the_phrase() {
    let f = fixture();
    keep(&f.engine, "Something to keep.");
    let place = MemoryPlace::new();
    let key = nightly::turn_on(&place, PHRASE, WORK).unwrap();
    let mut state = NightState::new();
    let report = night(&f.engine, &place, &key, &mut state).unwrap();
    let found = nightly::found(&place).unwrap().unwrap();
    assert!(found.modified.is_some());
    assert!(found.bytes >= report.bytes);
    let newest = nightly::newest(&place, &key).unwrap().unwrap();
    assert_eq!(newest.made_at, report.made_at);
    assert_eq!(newest.bytes, report.bytes);
    assert_eq!(newest.device, state.device);
}

#[test]
fn seven_nights_are_kept_and_pieces_no_night_lists_go() {
    let f = fixture();
    let place = MemoryPlace::new();
    let key = nightly::turn_on(&place, PHRASE, WORK).unwrap();
    let mut state = NightState::new();
    for i in 0..10 {
        keep(&f.engine, &format!("Note number {i} for the week."));
        night(&f.engine, &place, &key, &mut state).unwrap();
    }
    let nights = names(&place, "night-");
    assert_eq!(nights.len(), nightly::NIGHTS_KEPT);
    assert!(nights[0].starts_with("night-000000000004-"));
    // Every piece is listed by a kept night: a restore of the oldest kept one would find them.
    let newest = nightly::newest(&place, &key).unwrap().unwrap();
    assert_eq!(newest.count, 10);
    let pieces = names(&place, "piece-").len();
    assert!(pieces > 0);
    restore(&place, &key).unwrap();
}

#[test]
fn a_forget_reaches_the_backup_at_once() {
    let f = fixture();
    let secret = noise(600_000, 9);
    let photo = keep_photo(&f.engine, &secret, "letter.jpg");
    f.engine
        .apply(
            &photo,
            vec![cand("Zed", Predicate::Knows, "x", "Zed knows x.")],
        )
        .unwrap();
    keep(&f.engine, "Something else to keep.");
    let place = MemoryPlace::new();
    let key = nightly::turn_on(&place, PHRASE, WORK).unwrap();
    let mut state = NightState::new();
    for _ in 0..3 {
        night(&f.engine, &place, &key, &mut state).unwrap();
    }
    let pieces_before = names(&place, "piece-");

    assert!(!f.engine.forgot_since_night().unwrap());
    let memory = f.engine.memories(Default::default()).unwrap()[0].id.clone();
    let preview = f
        .engine
        .preview_forget(std::slice::from_ref(&memory), true)
        .unwrap();
    f.engine.forget(&preview.token).unwrap();
    assert!(f.engine.forgot_since_night().unwrap());

    let report = night(&f.engine, &place, &key, &mut state).unwrap();
    assert!(report.carried_forget);
    assert_eq!(report.originals, 0);
    assert_eq!(
        names(&place, "night-"),
        vec![format!("night-000000000004-{}", state.device)],
        "every older night went"
    );
    let left = names(&place, "piece-");
    assert!(left.len() < pieces_before.len());
    assert!(!f.engine.forgot_since_night().unwrap());

    let (out, summary, _) = restore(&place, &key).unwrap();
    assert_eq!(summary.originals, 0);
    let store = out.path().join("stage/store");
    assert_eq!(
        std::fs::read_dir(store.join("originals")).unwrap().count(),
        0
    );

    // The next night keeps history again.
    night(&f.engine, &place, &key, &mut state).unwrap();
    assert_eq!(names(&place, "night-").len(), 2);
}

#[test]
fn a_store_holding_only_the_mark_holds_nothing() {
    let f = fixture();
    let id = keep(&f.engine, "Gone soon.");
    f.engine.unkeep(&id).unwrap();
    assert!(f.engine.forgot_since_night().unwrap());
    assert!(!f.engine.has_data().unwrap());
}

#[test]
fn a_phone_stops_when_another_made_a_newer_night() {
    let f = fixture();
    keep(&f.engine, "Something to keep.");
    let place = MemoryPlace::new();
    let key = nightly::turn_on(&place, PHRASE, WORK).unwrap();
    let mut old = NightState::new();
    night(&f.engine, &place, &key, &mut old).unwrap();

    let (_out, _, mut new) = restore(&place, &key).unwrap();
    assert!(matches!(
        night(&f.engine, &place, &key, &mut new),
        Err(NightError::NotInUse)
    ));
    nightly::take_over(&place, &key, &mut new).unwrap();
    let made = night(&f.engine, &place, &key, &mut new).unwrap();
    assert_eq!(made.count, 2);

    match night(&f.engine, &place, &key, &mut old) {
        Err(NightError::AnotherPhone { made_at }) => assert_eq!(made_at, made.made_at),
        other => panic!("the old phone should stop: {other:?}"),
    }
    assert_eq!(names(&place, "night-").len(), 2, "nothing was overwritten");
}

#[test]
fn a_changed_piece_is_found_and_nothing_comes_back() {
    let f = fixture();
    keep_photo(&f.engine, &noise(400_000, 21), "card.jpg");
    let place = MemoryPlace::new();
    let key = nightly::turn_on(&place, PHRASE, WORK).unwrap();
    night(&f.engine, &place, &key, &mut NightState::new()).unwrap();
    let piece = names(&place, "piece-").pop().unwrap();
    place.tamper(&piece, |b| {
        let n = b.len();
        b[n - 1] ^= 1;
    });
    let out = tempfile::tempdir().unwrap();
    let stage = out.path().join("stage");
    let mut result = None;
    run_with(false, |run| {
        result = Some(nightly::restore_night(&place, &key, None, &stage, run));
    });
    assert!(matches!(result.unwrap(), Err(NightError::Damaged)));
    assert!(!stage.exists(), "nothing half-restored stays");
    let mut checked = None;
    run_with(false, |run| {
        checked = Some(nightly::check_newest(&place, &key, run));
    });
    assert!(matches!(checked.unwrap(), Err(NightError::Damaged)));
}

#[test]
fn a_night_that_was_moved_or_forged_does_not_open() {
    let f = fixture();
    keep(&f.engine, "Something to keep.");
    let place = MemoryPlace::new();
    let key = nightly::turn_on(&place, PHRASE, WORK).unwrap();
    let mut state = NightState::new();
    night(&f.engine, &place, &key, &mut state).unwrap();
    let name = names(&place, "night-").pop().unwrap();
    let bytes = place.get(&name).unwrap().unwrap();

    // The same bytes under a newer count, as if from another phone.
    let moved = format!("night-000000000009-{}", "ab".repeat(8));
    place.put(&moved, &bytes).unwrap();
    assert_eq!(nightly::newest(&place, &key).unwrap().unwrap().count, 1);
    night(&f.engine, &place, &key, &mut state).unwrap();
    assert_eq!(state.count, 2);

    // Another key's night is passed over too.
    let other = MemoryPlace::new();
    let other_key = nightly::turn_on(&other, PHRASE, WORK).unwrap();
    night(&f.engine, &other, &other_key, &mut NightState::new()).unwrap();
    let foreign = names(&other, "night-").pop().unwrap();
    place
        .put(
            &format!("night-000000000010-{}", "cd".repeat(8)),
            &other.get(&foreign).unwrap().unwrap(),
        )
        .unwrap();
    assert_eq!(nightly::newest(&place, &key).unwrap().unwrap().count, 2);
}

#[test]
fn a_stopped_night_leaves_the_last_one_whole() {
    let f = fixture();
    keep_photo(&f.engine, &noise(2_000_000, 31), "a.jpg");
    let place = MemoryPlace::new();
    let key = nightly::turn_on(&place, PHRASE, WORK).unwrap();
    let mut state = NightState::new();
    night(&f.engine, &place, &key, &mut state).unwrap();
    keep_photo(&f.engine, &noise(2_000_000, 32), "b.jpg");

    let cancel = AtomicBool::new(false);
    let stop_soon = |_: BackupPhase, done: u64, _: u64| {
        if done > 0 {
            cancel.store(true, std::sync::atomic::Ordering::SeqCst);
        }
    };
    let run = NightRun {
        progress: &stop_soon,
        cancel: &cancel,
        check: false,
    };
    assert!(matches!(
        f.engine.back_up_night(&place, &key, &mut state, &run),
        Err(NightError::Cancelled)
    ));
    assert_eq!(state.count, 1);
    assert_eq!(names(&place, "night-").len(), 1);
    let (_, summary, _) = restore(&place, &key).unwrap();
    assert_eq!(summary.originals, 1);
    assert!(
        !f.engine.dir().join("night-tmp").exists(),
        "the snapshot does not stay"
    );

    // The account refusing is said as it is, and the next night carries on.
    place.fail_with(Some(PlaceError::NoRoom));
    assert!(matches!(
        night(&f.engine, &place, &key, &mut state),
        Err(NightError::Place(PlaceError::NoRoom))
    ));
    place.fail_with(None);
    let report = night(&f.engine, &place, &key, &mut state).unwrap();
    assert_eq!(report.originals, 2);
}

#[test]
fn turning_off_with_delete_leaves_nothing() {
    let f = fixture();
    keep(&f.engine, "Something to keep.");
    let place = MemoryPlace::new();
    let key = nightly::turn_on(&place, PHRASE, WORK).unwrap();
    nightly::add_recovery_key(&place, &key, WORK).unwrap();
    night(&f.engine, &place, &key, &mut NightState::new()).unwrap();
    nightly::delete_all(&place).unwrap();
    assert!(place.names().is_empty());
    assert!(nightly::found(&place).unwrap().is_none());
}

#[test]
fn a_folder_place_holds_a_backup_across_runs() {
    let f = fixture();
    keep(&f.engine, "Something to keep.");
    let dir = tempfile::tempdir().unwrap();
    let place = FolderPlace::new(dir.path());
    let key = nightly::turn_on(&place, PHRASE, WORK).unwrap();
    let mut state = NightState::new();
    night(&f.engine, &place, &key, &mut state).unwrap();
    let place = FolderPlace::new(dir.path());
    let key = nightly::open_with_phrase(&place, PHRASE).unwrap();
    assert!(place.get(KEY_PHRASE).unwrap().is_some());
    assert!(place.get(KEY_RECOVERY).unwrap().is_none());
    assert!(matches!(
        place.put("../escape", b"x"),
        Err(PlaceError::Failed(_))
    ));
    restore(&place, &key).unwrap();
}

#[test]
fn nocturnes_phrase_is_six_words() {
    let words: Vec<&str> = ["harbour", "violet", "pencil", "maple", "drift", "oven"].to_vec();
    let phrase = nightly::make_phrase(&words);
    assert_eq!(phrase.split(' ').count(), nightly::PHRASE_WORDS);
}

#[test]
fn an_earlier_night_can_be_chosen() {
    let f = fixture();
    keep(&f.engine, "First note.");
    let place = MemoryPlace::new();
    let key = nightly::turn_on(&place, PHRASE, WORK).unwrap();
    let mut state = NightState::new();
    night(&f.engine, &place, &key, &mut state).unwrap();
    keep(&f.engine, "Second note.");
    night(&f.engine, &place, &key, &mut state).unwrap();
    let listed = nightly::nights(&place, &key).unwrap();
    assert_eq!(
        listed.iter().map(|n| n.count).collect::<Vec<_>>(),
        vec![2, 1]
    );
    let out = tempfile::tempdir().unwrap();
    let mut result = None;
    run_with(false, |run| {
        result = Some(nightly::restore_night(
            &place,
            &key,
            Some(1),
            &out.path().join("stage"),
            run,
        ));
    });
    let unpacked = result.unwrap().unwrap();
    assert_eq!(unpacked.summary.items, 1);
    assert_eq!(unpacked.night.count, 1);
    assert_eq!(unpacked.state.count, 2, "the next night follows the newest");
    let mut missing = None;
    run_with(false, |run| {
        missing = Some(nightly::restore_night(
            &place,
            &key,
            Some(9),
            &out.path().join("other"),
            run,
        ));
    });
    assert!(matches!(missing.unwrap(), Err(NightError::NoBackup)));
}

#[test]
fn a_backup_brought_into_a_memory_adds_its_things_and_keeps_shared_ones_once() {
    let old = fixture();
    keep(&old.engine, "The plumber comes on Tuesday.");
    keep(&old.engine, "Both phones kept this.");
    let place = MemoryPlace::new();
    let key = nightly::turn_on(&place, PHRASE, WORK).unwrap();
    night(&old.engine, &place, &key, &mut NightState::new()).unwrap();

    let here = fixture();
    here.engine.ensure_local_device("Pixel").unwrap();
    keep(&here.engine, "Already on this phone.");
    keep(&here.engine, "Both phones kept this.");
    let (out, _, _) = restore(&place, &key).unwrap();
    let brought = here
        .engine
        .bring_in_backup(&out.path().join("stage/store"))
        .unwrap();
    assert_eq!(brought.same, 1);
    assert_eq!(here.engine.find_items("plumber", 5).unwrap().len(), 1);
    assert_eq!(here.engine.find_items("Already", 5).unwrap().len(), 1);
    assert_eq!(here.engine.find_items("Both", 5).unwrap().len(), 1);
}

/// Restore the newest night and bring it into `engine`.
fn bring_newest(
    engine: &Engine,
    place: &dyn BackupPlace,
    key: &NightKey,
) -> nocturne_engine::BroughtIn {
    let (out, _, _) = restore(place, key).unwrap();
    engine
        .bring_in_backup(&out.path().join("stage/store"))
        .unwrap()
}

#[test]
fn a_backup_brought_in_comes_as_it_was_and_nothing_is_read_again() {
    let old = fixture();
    old.engine.ensure_local_device("Old phone").unwrap();
    let (item, memory) = learn(
        &old.engine,
        "Zed knows x.",
        cand("Zed", Predicate::Knows, "x", "Zed knows x."),
    );
    old.engine.set_keep_forever(&memory, true).unwrap();
    old.engine.set_private(&memory, true).unwrap();
    let page = old
        .engine
        .compose_page(common::date("2026-11-13"), Edition::Screen)
        .unwrap();
    old.engine.save_page(&page).unwrap();
    let place = MemoryPlace::new();
    let key = nightly::turn_on(&place, PHRASE, WORK).unwrap();
    night(&old.engine, &place, &key, &mut NightState::new()).unwrap();

    let here = fixture();
    here.engine.ensure_local_device("Pixel").unwrap();
    keep(&here.engine, "Already on this phone.");
    let waiting = here.engine.pending_items(10).unwrap().len();
    let brought = bring_newest(&here.engine, &place, &key);
    assert_eq!(brought.things, 1);

    assert_eq!(
        here.engine.pending_items(10).unwrap().len(),
        waiting,
        "nothing that came waits to be read"
    );
    assert_eq!(
        here.engine.get_item(&item).unwrap().status,
        old.engine.get_item(&item).unwrap().status
    );
    let came = here.engine.get_memory(&memory).unwrap();
    assert!(came.private);
    assert_eq!(came.sentence, "Zed knows x.");
    assert_eq!(
        sql_count(
            here.dir.path(),
            &format!("SELECT keep_forever FROM memories WHERE id = '{memory}'")
        ),
        1
    );
    assert!(
        here.engine
            .page(Some(common::date("2026-11-13")))
            .unwrap()
            .is_some(),
        "the day's page came with it"
    );

    // Brought in twice, nothing doubles.
    let again = bring_newest(&here.engine, &place, &key);
    assert_eq!((again.things, again.already), (0, 1));
    assert_eq!(
        sql_count(here.dir.path(), "SELECT COUNT(*) FROM memories"),
        1
    );
}

#[test]
fn a_memory_read_here_from_a_thing_on_both_is_kept_once_with_both_marks() {
    let old = fixture();
    old.engine.ensure_local_device("Old phone").unwrap();
    let (_, memory) = learn(
        &old.engine,
        "Zed knows x.",
        cand("Zed", Predicate::Knows, "x", "Zed knows x."),
    );
    old.engine.set_private(&memory, true).unwrap();
    let place = MemoryPlace::new();
    let key = nightly::turn_on(&place, PHRASE, WORK).unwrap();
    night(&old.engine, &place, &key, &mut NightState::new()).unwrap();

    let here = fixture();
    here.engine.ensure_local_device("Pixel").unwrap();
    let (_, mine) = learn(
        &here.engine,
        "Zed knows x.",
        cand("Zed", Predicate::Knows, "x", "Zed knows x."),
    );
    here.engine.set_keep_forever(&mine, true).unwrap();
    let brought = bring_newest(&here.engine, &place, &key);
    assert_eq!(brought.same, 1);
    assert_eq!(
        sql_count(here.dir.path(), "SELECT COUNT(*) FROM memories"),
        1
    );
    let kept = here.engine.get_memory(&mine).unwrap();
    assert!(kept.private, "the backup's mark holds");
    assert_eq!(
        sql_count(
            here.dir.path(),
            &format!("SELECT keep_forever FROM memories WHERE id = '{mine}'")
        ),
        1
    );
}

fn calendar_batch(sync: &str, entries: &[(&str, &str, &str)]) -> CalendarBatch {
    CalendarBatch {
        sync_id: sync.into(),
        seq: 0,
        last: true,
        window_from: "2026-11-18".into(),
        window_to: "2026-12-04".into(),
        calendars: vec![PhoneCalendar {
            id: "c1".into(),
            name: "Home".into(),
            colour: None,
            account: None,
        }],
        changes: entries
            .iter()
            .map(|(id, title, start)| CalendarChange::Upsert {
                entry: CalendarEntry {
                    phone_id: (*id).into(),
                    calendar_id: "c1".into(),
                    title: (*title).into(),
                    start: (*start).into(),
                    end: start.replace(":00:00Z", ":30:00Z"),
                    time_zone: Some("Europe/London".into()),
                    all_day: false,
                    place: None,
                    people: Vec::new(),
                    repeats: false,
                    notes: None,
                    repeat_rule: None,
                },
            })
            .collect(),
    }
}

fn sleep_batch(sync: &str, id: &str) -> ReadingBatch {
    ReadingBatch {
        sync_id: sync.into(),
        seq: 0,
        last: true,
        changes: vec![ReadingChange::Upsert {
            reading: Reading {
                phone_id: id.into(),
                day: "2026-11-19".into(),
                start: "2026-11-18T23:00:00Z".into(),
                end: None,
                source_device: Some("Watch".into()),
                value: ReadingValue::Sleep {
                    asleep: vec![TimeSpan {
                        from: "2026-11-18T23:00:00Z".into(),
                        to: "2026-11-19T06:30:00Z".into(),
                    }],
                    in_bed_only: false,
                },
            },
        }],
    }
}

fn titles_by_entry(f: &Fixture) -> Vec<(String, String)> {
    let conn = rusqlite::Connection::open(f.dir.path().join("nocturne.db")).unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT r.phone_id, i.title FROM phone_records r JOIN items i ON i.id = r.item_id \
             JOIN remote_devices d ON d.id = r.device_id WHERE d.connection = 'local' \
             ORDER BY r.phone_id",
        )
        .unwrap();
    stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

#[test]
fn calendar_entries_and_health_readings_come_once_and_the_newer_version_stays() {
    let old = fixture_at("2026-11-20T05:00:00Z", "Europe/London");
    let phone = old.engine.ensure_local_device("Old phone").unwrap();
    old.engine
        .take_calendar(
            &phone,
            calendar_batch(
                "s1",
                &[
                    ("ev1", "Dentist", "2026-11-25T09:00:00Z"),
                    ("ev2", "Gym", "2026-11-26T09:00:00Z"),
                    ("ev3", "Haircut", "2026-11-27T10:00:00Z"),
                ],
            ),
        )
        .unwrap();
    old.engine
        .take_readings(&phone, sleep_batch("r1", "sleep1"))
        .unwrap();
    let place = MemoryPlace::new();
    let key = nightly::turn_on(&place, PHRASE, WORK).unwrap();
    night(&old.engine, &place, &key, &mut NightState::new()).unwrap();

    // This phone saw the haircut before the old one did, and the gym after it had moved.
    let here = fixture_at("2026-11-20T04:00:00Z", "Europe/London");
    let mine = here.engine.ensure_local_device("Pixel").unwrap();
    here.engine
        .take_calendar(
            &mine,
            calendar_batch("h1", &[("ev3", "Haircut", "2026-11-27T09:00:00Z")]),
        )
        .unwrap();
    here.clock.set("2026-11-20T06:00:00Z".parse().unwrap());
    here.engine
        .take_calendar(
            &mine,
            calendar_batch("h2", &[("ev2", "Gym moved", "2026-11-28T09:00:00Z")]),
        )
        .unwrap();
    here.engine
        .take_readings(&mine, sleep_batch("h3", "sleep1"))
        .unwrap();
    let brought = bring_newest(&here.engine, &place, &key);
    assert_eq!((brought.things, brought.same, brought.already), (0, 0, 0));
    assert_eq!(
        titles_by_entry(&here),
        vec![
            ("ev1".to_string(), "Dentist".to_string()),
            ("ev2".to_string(), "Gym moved".to_string()),
            ("ev3".to_string(), "Haircut".to_string()),
            ("sleep/sleep1".to_string(), "Sleep".to_string()),
        ]
    );
    let haircut: String = rusqlite::Connection::open(here.dir.path().join("nocturne.db"))
        .unwrap()
        .query_row(
            "SELECT json_extract(i.metadata, '$.entry.start') FROM phone_records r \
             JOIN items i ON i.id = r.item_id WHERE r.phone_id = 'ev3'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(haircut, "2026-11-27T10:00:00Z", "the newer version stays");
    assert_eq!(
        sql_count(
            here.dir.path(),
            "SELECT COUNT(*) FROM items WHERE kind = 'reading'"
        ),
        1,
        "a reading on both is kept once"
    );
    let status = |f: &Fixture, id: &str| -> String {
        rusqlite::Connection::open(f.dir.path().join("nocturne.db"))
            .unwrap()
            .query_row(
                "SELECT i.status FROM phone_records r JOIN items i ON i.id = r.item_id \
                 WHERE r.phone_id = ?1",
                [id],
                |r| r.get(0),
            )
            .unwrap()
    };
    assert_eq!(
        status(&here, "ev1"),
        status(&old, "ev1"),
        "it comes as it was"
    );

    let items = sql_count(here.dir.path(), "SELECT COUNT(*) FROM items");
    bring_newest(&here.engine, &place, &key);
    assert_eq!(
        sql_count(here.dir.path(), "SELECT COUNT(*) FROM items"),
        items,
        "brought in twice, nothing doubles"
    );
}

#[test]
fn a_night_is_refused_when_the_phrase_lock_has_gone() {
    let f = fixture();
    keep(&f.engine, "Something to keep.");
    let place = MemoryPlace::new();
    let key = nightly::turn_on(&place, PHRASE, WORK).unwrap();
    let mut state = NightState::new();
    night(&f.engine, &place, &key, &mut state).unwrap();
    place.delete(KEY_PHRASE).unwrap();
    assert!(matches!(
        night(&f.engine, &place, &key, &mut state),
        Err(NightError::Damaged)
    ));
}

#[test]
fn no_night_is_made_while_a_forget_is_finishing() {
    let f = fixture();
    let (_, memory) = learn(
        &f.engine,
        "Zed knows x.",
        cand("Zed", Predicate::Knows, "x", "Zed knows x."),
    );
    let place = MemoryPlace::new();
    let key = nightly::turn_on(&place, PHRASE, WORK).unwrap();
    let mut state = NightState::new();
    night(&f.engine, &place, &key, &mut state).unwrap();
    let preview = f
        .engine
        .preview_forget(std::slice::from_ref(&memory), false)
        .unwrap();
    f.engine.forget_begin_only(&preview.token).unwrap();
    assert!(matches!(
        night(&f.engine, &place, &key, &mut state),
        Err(NightError::Busy)
    ));
    f.engine.resume_forgets_for_test().unwrap();
    let report = night(&f.engine, &place, &key, &mut state).unwrap();
    assert!(report.carried_forget);
    assert_eq!(report.memories, 0);
}
