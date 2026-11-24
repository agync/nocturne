mod common;

use std::sync::atomic::AtomicBool;

use common::*;
use nocturne_contract::{
    BackupPhase, CalendarBatch, CalendarChange, CalendarEntry, Edition, EntityKind, Kept,
    PeriodKind, PhoneCalendar, Reading, ReadingBatch, ReadingChange, ReadingValue, ReflectionView,
    SameNameAnswer, Source, Span, TimeSpan, WrittenWhen,
};
use nocturne_engine::nightly::{
    self, BackupPlace, FolderPlace, KEY_PHRASE, KEY_RECOVERY, MemoryPlace, NightError, NightKey,
    NightRun, NightState, PlaceError,
};
use nocturne_engine::{Candidate, Engine, Predicate, WrittenSentence, hash_embedding};

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

/// A place that keeps every earlier version of a file until told to drop them, as an account
/// with file versions does.
#[derive(Default)]
struct VersionedPlace {
    now: MemoryPlace,
    earlier: std::sync::Mutex<Vec<(String, Vec<u8>)>>,
}

impl VersionedPlace {
    fn drop_earlier(&self) {
        self.earlier.lock().unwrap().clear();
    }

    /// Whether `phrase` opens the key in any version of the phrase lock someone with the account
    /// could still read.
    fn phrase_opens_a_version(&self, phrase: &str) -> bool {
        let mut versions: Vec<Vec<u8>> = self
            .earlier
            .lock()
            .unwrap()
            .iter()
            .filter(|(n, _)| n == KEY_PHRASE)
            .map(|(_, b)| b.clone())
            .collect();
        versions.extend(self.now.get(KEY_PHRASE).unwrap());
        versions.into_iter().any(|bytes| {
            let one = MemoryPlace::new();
            one.put(KEY_PHRASE, &bytes).unwrap();
            nightly::open_with_phrase(&one, phrase).is_ok()
        })
    }
}

impl BackupPlace for VersionedPlace {
    fn put(&self, name: &str, bytes: &[u8]) -> Result<(), PlaceError> {
        if let Some(old) = self.now.get(name)? {
            self.earlier.lock().unwrap().push((name.to_string(), old));
        }
        self.now.put(name, bytes)
    }
    fn get(&self, name: &str) -> Result<Option<Vec<u8>>, PlaceError> {
        self.now.get(name)
    }
    fn list(&self, prefix: &str) -> Result<Vec<nightly::PlaceEntry>, PlaceError> {
        self.now.list(prefix)
    }
    fn delete(&self, name: &str) -> Result<(), PlaceError> {
        self.earlier.lock().unwrap().retain(|(n, _)| n != name);
        self.now.delete(name)
    }
}

/// "Change the phrase" locks the same key again, so the old phrase stops opening the backup only
/// once no earlier version of the phrase lock can be read: `BackupPlace::put` promises that.
#[test]
fn the_old_phrase_stops_opening_only_once_the_place_drops_earlier_versions() {
    let f = fixture();
    keep(&f.engine, "Something to keep.");
    let place = VersionedPlace::default();
    let key = nightly::turn_on(&place, PHRASE, WORK).unwrap();
    night(&f.engine, &place, &key, &mut NightState::new()).unwrap();
    nightly::change_phrase(&place, &key, "lantern copper meadow", WORK).unwrap();
    assert!(matches!(
        nightly::open_with_phrase(&place, PHRASE),
        Err(NightError::WrongPhrase)
    ));
    assert!(
        place.phrase_opens_a_version(PHRASE),
        "a kept earlier version still opens with the old phrase"
    );
    place.drop_earlier();
    assert!(!place.phrase_opens_a_version(PHRASE));
    assert!(place.phrase_opens_a_version("lantern copper meadow"));
}

/// A place that keeps a damaged copy of each piece it is given while `spoil` is set.
#[derive(Default)]
struct Spoiling {
    inner: MemoryPlace,
    spoil: AtomicBool,
}

impl BackupPlace for Spoiling {
    fn put(&self, name: &str, bytes: &[u8]) -> Result<(), PlaceError> {
        let mut bytes = bytes.to_vec();
        if name.starts_with("piece-") && self.spoil.load(std::sync::atomic::Ordering::SeqCst) {
            let last = bytes.len() - 1;
            bytes[last] ^= 1;
        }
        self.inner.put(name, &bytes)
    }
    fn get(&self, name: &str) -> Result<Option<Vec<u8>>, PlaceError> {
        self.inner.get(name)
    }
    fn list(&self, prefix: &str) -> Result<Vec<nightly::PlaceEntry>, PlaceError> {
        self.inner.list(prefix)
    }
    fn delete(&self, name: &str) -> Result<(), PlaceError> {
        self.inner.delete(name)
    }
}

#[test]
fn a_later_night_is_not_written_when_a_piece_it_sent_does_not_read_back() {
    let f = fixture();
    keep(&f.engine, "Something to keep.");
    let place = Spoiling::default();
    let key = nightly::turn_on(&place, PHRASE, WORK).unwrap();
    let mut state = NightState::new();
    night(&f.engine, &place, &key, &mut state).unwrap();
    keep(&f.engine, "Something new tonight.");
    place.spoil.store(true, std::sync::atomic::Ordering::SeqCst);
    assert!(matches!(
        night(&f.engine, &place, &key, &mut state),
        Err(NightError::Damaged)
    ));
    assert_eq!(names(&place.inner, "night-").len(), 1, "no night lists it");
    assert_eq!(state.count, 1);
    place
        .spoil
        .store(false, std::sync::atomic::Ordering::SeqCst);
    night(&f.engine, &place, &key, &mut state).unwrap();
    assert_eq!(nightly::nights(&place, &key).unwrap().len(), 2);
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
fn the_nights_are_listed_before_the_phrase_and_counted_after_it() {
    let f = fixture();
    keep(&f.engine, "The first thing kept.");
    let place = MemoryPlace::new();
    assert!(nightly::listed_nights(&place).unwrap().is_empty());
    let key = nightly::turn_on(&place, PHRASE, WORK).unwrap();
    let mut state = NightState::new();
    night(&f.engine, &place, &key, &mut state).unwrap();
    keep(&f.engine, "The second thing kept.");
    night(&f.engine, &place, &key, &mut state).unwrap();

    let listed = nightly::listed_nights(&place).unwrap();
    let opened = nightly::nights(&place, &key).unwrap();
    assert_eq!(listed.len(), 2);
    for (l, o) in listed.iter().zip(&opened) {
        assert_eq!(l.count, o.count, "newest first, the same nights");
        assert_eq!(l.bytes, Some(o.bytes), "the size the night takes");
        let made: jiff::Timestamp = o.made_at.parse().unwrap();
        assert_eq!(l.made_at.map(|t| t.as_second()), Some(made.as_second()));
    }
    assert_eq!(opened[0].items, 2);
    assert_eq!(opened[1].items, 1);
    assert!(opened[0].first.is_some());
    assert_eq!(
        opened[0].first, opened[1].first,
        "the first thing is the same"
    );
}

#[test]
fn a_restore_says_it_gets_then_checks_the_night() {
    let f = fixture();
    keep(&f.engine, "Something to keep.");
    let place = MemoryPlace::new();
    let key = nightly::turn_on(&place, PHRASE, WORK).unwrap();
    night(&f.engine, &place, &key, &mut NightState::new()).unwrap();
    let phases = std::sync::Mutex::new(Vec::new());
    let note = |phase, done, total| phases.lock().unwrap().push((phase, done == total));
    let cancel = AtomicBool::new(false);
    let out = tempfile::tempdir().unwrap();
    nightly::restore_night(
        &place,
        &key,
        None,
        &out.path().join("stage"),
        &NightRun {
            progress: &note,
            cancel: &cancel,
            check: false,
        },
    )
    .unwrap();
    let phases = phases.into_inner().unwrap();
    let first_check = phases
        .iter()
        .position(|(p, _)| *p == BackupPhase::Checking)
        .unwrap();
    assert!(
        phases[..first_check]
            .iter()
            .all(|(p, _)| *p == BackupPhase::Restoring)
    );
    assert_eq!(phases.last(), Some(&(BackupPhase::Checking, true)));
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
    let left_nights = names(&place, "night-");
    assert_eq!(left_nights.len(), 1, "every older night went");
    assert!(left_nights[0].starts_with(&format!("night-000000000004-{}-", state.device)));
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
    // No night is written beside one that doesn't open: what it lists would be pruned.
    assert!(matches!(
        night(&f.engine, &place, &key, &mut state),
        Err(NightError::Damaged)
    ));
    place.delete(&moved).unwrap();
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

fn forget_for_good(engine: &Engine, memory: &str) {
    let preview = engine.preview_forget(&[memory.to_string()], true).unwrap();
    engine.forget(&preview.token).unwrap();
}

#[test]
fn what_this_phone_forgot_for_good_does_not_come_back_from_an_earlier_night() {
    let f = fixture();
    f.engine.ensure_local_device("Pixel").unwrap();
    let (item, memory) = learn(
        &f.engine,
        "Zed knows x.",
        cand("Zed", Predicate::Knows, "x", "Zed knows x."),
    );
    keep(&f.engine, "Something to keep.");
    let place = MemoryPlace::new();
    let key = nightly::turn_on(&place, PHRASE, WORK).unwrap();
    night(&f.engine, &place, &key, &mut NightState::new()).unwrap();
    // Forgotten while the account cannot be reached: the earlier night still holds it.
    forget_for_good(&f.engine, &memory);
    assert!(f.engine.get_item(&item).is_err());

    let brought = bring_newest(&f.engine, &place, &key);
    assert_eq!((brought.things, brought.already), (0, 1));
    assert!(
        f.engine.get_item(&item).is_err(),
        "the thing stays forgotten"
    );
    assert!(
        f.engine.get_memory(&memory).is_err(),
        "and what was read from it"
    );
    assert_eq!(sql_count(f.dir.path(), "SELECT COUNT(*) FROM memories"), 0);
}

#[test]
fn a_thing_kept_again_after_it_was_forgotten_comes() {
    let f = fixture();
    f.engine.ensure_local_device("Old phone").unwrap();
    let (item, memory) = learn(
        &f.engine,
        "Zed knows x.",
        cand("Zed", Predicate::Knows, "x", "Zed knows x."),
    );
    forget_for_good(&f.engine, &memory);
    assert!(f.engine.get_item(&item).is_err());
    keep(&f.engine, "Zed knows x.");
    let place = MemoryPlace::new();
    let key = nightly::turn_on(&place, PHRASE, WORK).unwrap();
    night(&f.engine, &place, &key, &mut NightState::new()).unwrap();

    let here = fixture();
    here.engine.ensure_local_device("Pixel").unwrap();
    let brought = bring_newest(&here.engine, &place, &key);
    assert_eq!(brought.things, 1, "kept again, it is the person's again");
    assert_eq!(
        sql_count(
            here.dir.path(),
            "SELECT COUNT(*) FROM forgotten_for_good WHERE key LIKE 'sha:%' \
             AND substr(key, 5) IN (SELECT sha256 FROM items)"
        ),
        0,
        "no note here names what is here"
    );
}

#[test]
fn what_another_phone_forgot_for_good_is_noted_and_nothing_here_goes() {
    let first = fixture();
    first.engine.ensure_local_device("Old phone").unwrap();
    let (item, memory) = learn(
        &first.engine,
        "Zed knows x.",
        cand("Zed", Predicate::Knows, "x", "Zed knows x."),
    );
    let place = MemoryPlace::new();
    let key = nightly::turn_on(&place, PHRASE, WORK).unwrap();
    let mut state = NightState::new();
    night(&first.engine, &place, &key, &mut state).unwrap();

    // A second phone brings it in, forgets it for good and backs up.
    let second = fixture();
    second.engine.ensure_local_device("New phone").unwrap();
    bring_newest(&second.engine, &place, &key);
    forget_for_good(&second.engine, &memory);
    let other = MemoryPlace::new();
    let other_key = nightly::turn_on(&other, PHRASE, WORK).unwrap();
    night(&second.engine, &other, &other_key, &mut NightState::new()).unwrap();

    // The first phone brings that in: what it holds stays, and it keeps the note.
    bring_newest(&first.engine, &other, &other_key);
    assert!(first.engine.get_item(&item).is_ok(), "nothing here goes");
    assert_eq!(
        sql_count(
            first.dir.path(),
            &format!("SELECT COUNT(*) FROM forgotten_for_good WHERE key = 'item:{item}'")
        ),
        1
    );
    // The first phone's page says it, and goes into its backup.
    first.clock.set_str("2026-11-14T07:00:00Z");
    let day = common::date("2026-11-14");
    let page = first.engine.compose_page(day, Edition::Screen).unwrap();
    first.engine.save_page(&page).unwrap();
    let cites = format!(
        "SELECT COUNT(*) FROM pages WHERE instr(body, '{memory}') > 0 \
         OR instr(body, '{item}') > 0"
    );
    assert!(sql_count(first.dir.path(), &cites) > 0, "the page cites it");
    night(&first.engine, &place, &key, &mut state).unwrap();

    // A third phone holding the second phone's notes does not get it back from the first
    // phone's backup, and the page comes with that line blank.
    let third = fixture();
    third.engine.ensure_local_device("Third phone").unwrap();
    bring_newest(&third.engine, &other, &other_key);
    bring_newest(&third.engine, &place, &key);
    assert!(third.engine.get_item(&item).is_err());
    assert!(third.engine.get_memory(&memory).is_err());
    assert!(
        third.engine.page(Some(day)).unwrap().is_some(),
        "the page came"
    );
    assert_eq!(
        sql_count(third.dir.path(), &cites),
        0,
        "what was forgotten is not on it"
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

#[test]
fn the_backup_starts_again_in_another_account_with_the_same_phrase_and_recovery_key() {
    let f = fixture();
    keep(&f.engine, "The plumber comes on Tuesday.");
    let old = MemoryPlace::new();
    let key = nightly::turn_on(&old, PHRASE, WORK).unwrap();
    let recovery = nightly::add_recovery_key(&old, &key, WORK).unwrap();
    let mut state = NightState::new();
    night(&f.engine, &old, &key, &mut state).unwrap();
    let locks = nightly::key_locks(&old).unwrap();
    assert!(locks.recovery.is_some());

    let new = MemoryPlace::new();
    nightly::carry_locks(&new, &key, &locks, &[], &state).unwrap();
    night(&f.engine, &new, &key, &mut state).unwrap();
    run_with(true, |run| {
        nightly::check_newest(&new, &key, run).expect("every piece is in the new account");
    });
    assert!(
        nightly::open_with_phrase(&new, PHRASE)
            .unwrap()
            .same_as(&key)
    );
    assert!(
        nightly::open_with_recovery_key(&new, &recovery)
            .unwrap()
            .same_as(&key)
    );
    let (out, _, _) = restore(&new, &key).unwrap();
    assert!(out.path().join("stage/store").is_dir());

    // Chosen again, the old account keeps its nights and takes the locks.
    nightly::carry_locks(&old, &key, &locks, &[], &state).unwrap();
    assert_eq!(names(&old, "night-").len(), 1);

    // An account holding only this backup's phrase lock takes the rest, and loses a recovery
    // lock `locks` does not hold.
    let started = MemoryPlace::new();
    started.put(KEY_PHRASE, &locks.phrase).unwrap();
    started.put(KEY_RECOVERY, b"another lock").unwrap();
    let no_recovery = nightly::KeyLocks {
        recovery: None,
        ..locks.clone()
    };
    nightly::carry_locks(&started, &key, &no_recovery, &[], &state).unwrap();
    assert_eq!(names(&started, "key-"), vec![KEY_PHRASE.to_string()]);

    // An account with another backup is never written over.
    let other = MemoryPlace::new();
    nightly::turn_on(&other, "another phrase of someone else", WORK).unwrap();
    assert!(matches!(
        nightly::carry_locks(&other, &key, &locks, &[], &state),
        Err(NightError::AlreadyThere)
    ));
    assert!(nightly::open_with_phrase(&other, PHRASE).is_err());
}

#[test]
fn another_backup_in_the_account_is_never_written_over_or_pruned() {
    let theirs = fixture();
    keep(&theirs.engine, "Someone else's thing.");
    let place = MemoryPlace::new();
    let their_key = nightly::turn_on(&place, "a phrase of their own", WORK).unwrap();
    night(&theirs.engine, &place, &their_key, &mut NightState::new()).unwrap();
    let their_pieces = names(&place, "piece-");

    let mine = fixture();
    keep(&mine.engine, "My thing.");
    let key = NightKey::generate();
    // Even with this backup's phrase lock written over theirs, their nights mark it as theirs.
    let locks = nightly::KeyLocks {
        phrase: nightly::phrase_lock(&key, PHRASE, WORK).unwrap(),
        recovery: None,
    };
    place.put(KEY_PHRASE, &locks.phrase).unwrap();
    let mut state = NightState::new();
    assert!(matches!(
        nightly::carry_locks(&place, &key, &locks, &[], &state),
        Err(NightError::AlreadyThere)
    ));
    assert!(matches!(
        night(&mine.engine, &place, &key, &mut state),
        Err(NightError::Damaged)
    ));
    assert_eq!(names(&place, "piece-"), their_pieces, "their pieces stay");
}

#[test]
fn a_newer_night_from_another_phone_stops_the_locks_being_carried() {
    let f = fixture();
    keep(&f.engine, "The plumber comes on Tuesday.");
    let place = MemoryPlace::new();
    let key = nightly::turn_on(&place, PHRASE, WORK).unwrap();
    let mut other = NightState::new();
    night(&f.engine, &place, &key, &mut other).unwrap();
    let theirs = place.get(KEY_PHRASE).unwrap();

    let locks = nightly::KeyLocks {
        phrase: nightly::phrase_lock(&key, "an old phrase", WORK).unwrap(),
        recovery: None,
    };
    let mine = NightState::new();
    assert!(matches!(
        nightly::carry_locks(&place, &key, &locks, &[], &mine),
        Err(NightError::AnotherPhone { .. })
    ));
    assert_eq!(place.get(KEY_PHRASE).unwrap(), theirs, "their lock stays");
}

fn raw_db(f: &Fixture) -> rusqlite::Connection {
    rusqlite::Connection::open(f.dir.path().join("nocturne.db")).unwrap()
}

/// A phone that holds a memory of its own, for a backup to be brought into.
fn phone_with_memory() -> Fixture {
    let here = fixture();
    here.engine.ensure_local_device("Pixel").unwrap();
    keep(&here.engine, "Already on this phone.");
    here
}

/// Back up one night of `old` into a place of its own.
fn back_up_night(old: &Fixture) -> (MemoryPlace, NightKey) {
    old.engine.ensure_local_device("Old phone").unwrap();
    let place = MemoryPlace::new();
    let key = nightly::turn_on(&place, PHRASE, WORK).unwrap();
    night(&old.engine, &place, &key, &mut NightState::new()).unwrap();
    (place, key)
}

fn read_all(f: &Fixture) {
    let ids: Vec<String> = raw_db(f)
        .prepare("SELECT id FROM items WHERE status = 'kept'")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    for id in ids {
        f.engine.apply(&id, vec![]).unwrap();
    }
}

/// Write an entry for `day` and a reflection on it that says `line`; returns the entry's id.
fn reflect_on(f: &Fixture, day: &str, entry: &str, line: &str) -> String {
    let block = f
        .engine
        .save_entry_text(date(day), None, 0, entry)
        .unwrap()
        .unwrap();
    read_all(f);
    let period = f.engine.period_of(PeriodKind::Day, date(day)).unwrap();
    let input = f.engine.reflection_input(&period).unwrap();
    let at = input
        .items
        .iter()
        .position(|i| i.id == block.item.id)
        .unwrap();
    f.engine
        .save_day_reflection(
            &period,
            &input,
            &[WrittenSentence {
                text: line.to_string(),
                cites: vec![at],
            }],
            WrittenWhen::Overnight,
        )
        .unwrap();
    block.item.id
}

fn day_reflection(f: &Fixture, day: &str) -> Option<nocturne_contract::DayReflection> {
    let period = f.engine.period_of(PeriodKind::Day, date(day)).unwrap();
    match f.engine.saved_reflection(&period).unwrap() {
        Some(ReflectionView::Written { reflection }) => Some(reflection),
        Some(other) => panic!("a day is Written, not {other:?}"),
        None => None,
    }
}

#[test]
fn a_reflection_comes_for_a_day_this_phone_has_none_for_and_never_replaces_or_revives_one() {
    let old = fixture();
    let (d1, d2, d3, d4) = ("2026-11-13", "2026-11-12", "2026-11-11", "2026-11-10");
    let cited = reflect_on(&old, d1, "Gran came for tea", "Gran came for **tea**.");
    reflect_on(&old, d2, "Walked by the river", "Old phone: a river walk.");
    reflect_on(&old, d3, "Fixed the shelf", "Old phone: the shelf.");
    reflect_on(&old, d4, "Slept badly", "Old phone: a bad night.");
    let period4 = old.engine.period_of(PeriodKind::Day, date(d4)).unwrap();
    old.engine.delete_reflection(&period4).unwrap();
    let (place, key) = back_up_night(&old);

    let here = phone_with_memory();
    reflect_on(&here, d2, "Cooked supper", "This phone: supper.");
    reflect_on(&here, d3, "Painted the door", "This phone: the door.");
    let period3 = here.engine.period_of(PeriodKind::Day, date(d3)).unwrap();
    here.engine.delete_reflection(&period3).unwrap();
    bring_newest(&here.engine, &place, &key);

    let came = day_reflection(&here, d1).expect("a day with none here gets the backup's");
    assert_eq!(came.the_day[0].text, "Gran came for **tea**.");
    assert_eq!(came.the_day[0].receipts[0].item_id, cited);
    assert!(here.engine.get_item(&cited).is_ok(), "its cite came too");
    assert_eq!(
        day_reflection(&here, d2).unwrap().the_day[0].text,
        "This phone: supper.",
        "the one here is not replaced"
    );
    assert!(
        day_reflection(&here, d3).is_none(),
        "a day deleted here stays so"
    );
    assert!(
        day_reflection(&here, d4).is_none(),
        "a day deleted there stays so"
    );
    assert_eq!(
        sql_count(here.dir.path(), "SELECT COUNT(*) FROM reflections_declined"),
        2
    );
    assert_eq!(
        sql_count(here.dir.path(), "SELECT COUNT(*) FROM reflections"),
        2
    );
}

#[test]
fn a_page_for_a_day_both_have_stays_this_phones_and_the_backups_does_not_come() {
    let day = common::date("2026-11-14");
    let page_body = |f: &Fixture| -> String {
        raw_db(f)
            .query_row(
                "SELECT body FROM pages WHERE date = '2026-11-14'",
                [],
                |r| r.get(0),
            )
            .unwrap()
    };
    let old = fixture();
    let (_, theirs) = learn(
        &old.engine,
        "Zed knows x.",
        cand("Zed", Predicate::Knows, "x", "Zed knows x."),
    );
    old.clock.set_str("2026-11-14T07:00:00Z");
    let page = old.engine.compose_page(day, Edition::Screen).unwrap();
    old.engine.save_page(&page).unwrap();
    assert!(
        page_body(&old).contains(&theirs),
        "the backup's page cites it"
    );
    let (place, key) = back_up_night(&old);

    let here = phone_with_memory();
    let (_, mine) = learn(
        &here.engine,
        "Amy prefers tea.",
        cand("Amy", Predicate::Prefers, "tea", "Amy prefers tea."),
    );
    here.clock.set_str("2026-11-14T07:00:00Z");
    let page = here.engine.compose_page(day, Edition::Screen).unwrap();
    here.engine.save_page(&page).unwrap();
    let before = page_body(&here);
    assert!(before.contains(&mine));

    let brought = bring_newest(&here.engine, &place, &key);
    assert_eq!(brought.things, 1, "the backup's things still come");
    assert!(here.engine.get_memory(&theirs).is_ok());
    assert_eq!(
        page_body(&here),
        before,
        "this phone's page is the one kept"
    );
    assert!(!page_body(&here).contains(&theirs));
    assert_eq!(
        sql_count(
            here.dir.path(),
            "SELECT COUNT(*) FROM pages WHERE date = '2026-11-14'"
        ),
        1,
        "never two pages for one day"
    );
}

fn add_mention(f: &Fixture, item: &str, entity: &str, name: &str, ord: u32) {
    let conn = raw_db(f);
    conn.execute(
        "INSERT OR IGNORE INTO entities(id, kind, name, key, role) \
         VALUES (?1, 'person', ?2, ?3, NULL)",
        rusqlite::params![entity, name, name.to_lowercase()],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO mentions(id, item_id, entity_id, name, start, end, ord, state, new, \
         created_at) VALUES (?1, ?2, ?3, ?4, 0, ?5, ?6, 'found', 0, '2026-11-13T07:00:00.000Z')",
        rusqlite::params![
            format!("men_{item}_{entity}"),
            item,
            entity,
            name,
            name.len() as u32,
            ord
        ],
    )
    .unwrap();
}

fn mentioned(f: &Fixture, item: &str) -> Vec<(String, String)> {
    raw_db(f)
        .prepare(
            "SELECT e.id, e.name FROM mentions n JOIN entities e ON e.id = n.entity_id \
             WHERE n.item_id = ?1 ORDER BY n.ord",
        )
        .unwrap()
        .query_map([item], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

#[test]
fn names_found_in_a_brought_thing_come_with_it_under_the_same_entity() {
    let old = fixture();
    let note = keep(&old.engine, "Priya came round with Marcus.");
    add_mention(&old, &note, "ent_old_priya", "Priya", 0);
    add_mention(&old, &note, "ent_old_marcus", "Marcus", 1);
    let (place, key) = back_up_night(&old);

    let here = phone_with_memory();
    let own = keep(&here.engine, "Priya sent the form.");
    add_mention(&here, &own, "ent_here_priya", "Priya", 0);
    bring_newest(&here.engine, &place, &key);

    assert_eq!(
        mentioned(&here, &note),
        [
            ("ent_here_priya".to_string(), "Priya".to_string()),
            ("ent_old_marcus".to_string(), "Marcus".to_string())
        ],
        "Priya is the one here; Marcus is new"
    );
    assert_eq!(
        sql_count(
            here.dir.path(),
            "SELECT COUNT(*) FROM entities WHERE key = 'priya'"
        ),
        1,
        "the same key is the same entity"
    );
    assert_eq!(
        sql_count(
            here.dir.path(),
            "SELECT COUNT(*) FROM mentions WHERE entity_id = 'ent_here_priya'"
        ),
        2
    );
    assert_eq!(
        mentioned(&here, &own).len(),
        1,
        "what was here is as it was"
    );
}

fn person(subject: &str, predicate: Predicate, object: &str, sentence: &str) -> Candidate {
    Candidate {
        subject_kind: Some(EntityKind::Person),
        ..cand(subject, predicate, object, sentence)
    }
}

fn entity_of(f: &Fixture, memory: &str) -> String {
    f.engine.get_memory(memory).unwrap().entity_id.unwrap()
}

#[test]
fn a_which_name_question_on_a_brought_thing_comes_with_its_options_remapped() {
    let old = fixture();
    let mut sams = Vec::new();
    for (full, role) in [
        ("Sam Jones", "your neighbour"),
        ("Sam Patel", "Leo\u{2019}s teacher"),
    ] {
        let (_, m) = learn(
            &old.engine,
            &format!("{full} is {role}."),
            person(full, Predicate::Knows, "you", &format!("{full} is {role}.")),
        );
        let entity = entity_of(&old, &m);
        raw_db(&old)
            .execute(
                "UPDATE entities SET role = ?2 WHERE id = ?1",
                rusqlite::params![entity, role],
            )
            .unwrap();
        sams.push(entity);
    }
    let entry = keep(
        &old.engine,
        "Priya made tea. Sam came round to fix the sink.",
    );
    raw_db(&old)
        .execute(
            "UPDATE items SET metadata = json_set(metadata, '$.journal', 'entry', \
             '$.journal_day', '2026-11-12') WHERE id = ?1",
            [&entry],
        )
        .unwrap();
    old.engine
        .apply(
            &entry,
            vec![person(
                "Sam",
                Predicate::Promised,
                "the sink",
                "Sam will fix the sink.",
            )],
        )
        .unwrap();
    assert_eq!(old.engine.same_name_questions(None).unwrap().len(), 1);
    let (place, key) = back_up_night(&old);

    let here = phone_with_memory();
    let (_, mine) = learn(
        &here.engine,
        "Sam Jones lives next door.",
        person(
            "Sam Jones",
            Predicate::Knows,
            "the lane",
            "Sam Jones lives next door.",
        ),
    );
    let jones_here = entity_of(&here, &mine);
    assert_ne!(jones_here, sams[0]);
    bring_newest(&here.engine, &place, &key);

    let questions = here.engine.same_name_questions(None).unwrap();
    assert_eq!(questions.len(), 1, "asked once, not twice");
    let q = &questions[0];
    assert_eq!(q.name, "Sam");
    assert_eq!(q.receipt.item_id, entry);
    assert_eq!(q.quote, "Sam came round to fix the sink.");
    let ids: Vec<&str> = q.options.iter().map(|o| o.entity_id.as_str()).collect();
    assert_eq!(ids[0], jones_here, "the Sam Jones here, not the old id");
    assert_eq!(
        ids[1], sams[1],
        "the Sam Patel that came keeps the id it had"
    );
    assert_eq!(q.options[1].role.as_deref(), Some("Leo\u{2019}s teacher"));
    for id in ids {
        assert_eq!(
            sql_count(
                here.dir.path(),
                &format!("SELECT COUNT(*) FROM entities WHERE id = '{id}'")
            ),
            1
        );
    }

    here.engine
        .answer_same_name(
            &q.id,
            SameNameAnswer::Entity {
                entity_id: jones_here.clone(),
            },
        )
        .unwrap();
    assert!(here.engine.same_name_questions(None).unwrap().is_empty());
    assert_eq!(
        mentioned(&here, &entry),
        [(jones_here, "Sam Jones".to_string())]
    );
}

/// A dentist appointment that a later note moves while filed under another name, so it asks.
fn dentist_conflict(f: &Fixture) -> (String, String) {
    let dentist = |subject: &str, object: &str, on: &str, quote: &str| {
        let mut c = cand(
            subject,
            Predicate::Scheduled,
            object,
            &format!("The dentist appointment with Dr Okafor is on {object}."),
        );
        c.due = Some(date(on));
        c.span = Some(Span::Quote {
            text: quote.to_string(),
        });
        c
    };
    const FIRST: &str = "Dentist appointment with Dr Okafor on Wednesday 18 November at 10:00.";
    const LINE: &str = "Dr Okafor's receptionist called: it is now Thursday 19 November at 14:30, not Wednesday 18 November.";
    learn(
        &f.engine,
        FIRST,
        dentist(
            "Dr Okafor",
            "Wednesday 18 November at 10:00",
            "2026-11-18",
            FIRST,
        ),
    );
    let moved = keep(
        &f.engine,
        &format!("The dentist appointment has moved. {LINE}"),
    );
    let report = f
        .engine
        .apply(
            &moved,
            vec![dentist(
                "Dentist appointment",
                "Thursday 19 November at 14:30",
                "2026-11-19",
                LINE,
            )],
        )
        .unwrap();
    assert_eq!(report.conflicts.len(), 1, "{report:?}");
    raw_db(f)
        .query_row("SELECT a, b FROM conflicts", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap()
}

#[test]
fn a_which_is_right_between_two_brought_memories_comes_in_once() {
    let old = fixture();
    let (a, b) = dentist_conflict(&old);
    let (place, key) = back_up_night(&old);

    let here = phone_with_memory();
    bring_newest(&here.engine, &place, &key);
    assert_eq!(
        here.engine.open_disputes(std::slice::from_ref(&a)).unwrap(),
        [(a.clone(), b.clone())]
    );
    assert_eq!(
        sql_count(here.dir.path(), "SELECT COUNT(*) FROM conflicts"),
        1
    );
    bring_newest(&here.engine, &place, &key);
    assert_eq!(
        sql_count(here.dir.path(), "SELECT COUNT(*) FROM conflicts"),
        1,
        "brought again, it does not double"
    );
}

#[test]
fn a_settled_which_is_right_comes_in_settled() {
    let old = fixture();
    let (a, b) = dentist_conflict(&old);
    let conflict: String = raw_db(&old)
        .query_row("SELECT id FROM conflicts", [], |r| r.get(0))
        .unwrap();
    old.engine.resolve_which(&conflict, &b).unwrap();
    let (place, key) = back_up_night(&old);

    let here = phone_with_memory();
    bring_newest(&here.engine, &place, &key);
    assert!(here.engine.open_disputes(&[a]).unwrap().is_empty());
    let (settled, kept): (bool, Option<String>) = raw_db(&here)
        .query_row(
            "SELECT resolved_at IS NOT NULL, kept FROM conflicts WHERE id = ?1",
            [&conflict],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert!(settled);
    assert_eq!(kept.as_deref(), Some(b.as_str()));
}

fn thing(subject: &str, object: &str, sentence: &str) -> Candidate {
    Candidate {
        subject_kind: Some(EntityKind::Thing),
        ..cand(subject, Predicate::Knows, object, sentence)
    }
}

#[test]
fn an_entity_merged_on_the_old_phone_arrives_merged_into_the_one_here_with_its_names() {
    let old = fixture();
    let (_, a) = learn(
        &old.engine,
        "Trinks are 5 pounds on Friday",
        thing(
            "Trinks",
            "5 pounds on Friday",
            "Trinks are 5 pounds on Friday.",
        ),
    );
    let (_, b) = learn(
        &old.engine,
        "Drinks are in the fridge",
        thing("Drinks", "in the fridge", "Drinks are in the fridge."),
    );
    let (trinks, drinks) = (entity_of(&old, &a), entity_of(&old, &b));
    old.engine.merge_entities(&trinks, &drinks).unwrap();
    let said = keep(&old.engine, "Trinks again on Friday.");
    add_mention(&old, &said, &trinks, "Trinks", 0);
    let (place, key) = back_up_night(&old);

    let here = phone_with_memory();
    let (_, mine) = learn(
        &here.engine,
        "Drinks are cold",
        thing("Drinks", "cold", "Drinks are cold."),
    );
    let drinks_here = entity_of(&here, &mine);
    assert_ne!(drinks_here, drinks);
    bring_newest(&here.engine, &place, &key);

    let merged_into: Option<String> = raw_db(&here)
        .query_row(
            "SELECT merged_into FROM entities WHERE id = ?1",
            [&trinks],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(merged_into.as_deref(), Some(drinks_here.as_str()));
    assert_eq!(
        sql_count(
            here.dir.path(),
            "SELECT COUNT(*) FROM entities WHERE name = 'Drinks' AND merged_into IS NULL"
        ),
        1,
        "the Drinks that came is the one here"
    );
    assert_eq!(entity_of(&here, &a), drinks_here);
    assert_eq!(here.engine.get_memory(&a).unwrap().subject, "Drinks");
    let found = here.engine.entities_matching(None, Some("trinks")).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(
        found[0].id, drinks_here,
        "the name the person taught finds it"
    );
}

#[test]
fn a_brought_memory_is_found_by_meaning_without_being_embedded_again() {
    let old = fixture();
    let (_, swim) = learn(
        &old.engine,
        "swimming lessons on Wednesday",
        cand(
            "Leo",
            Predicate::Prefers,
            "swimming",
            "Leo has swimming lessons on Wednesday.",
        ),
    );
    old.engine
        .store_embeddings(vec![(
            swim.clone(),
            hash_embedding("swimming lessons on Wednesday"),
        )])
        .unwrap();
    let (place, key) = back_up_night(&old);

    let here = phone_with_memory();
    let (_, own) = learn(
        &here.engine,
        "dentist on Tuesday",
        cand(
            "Dentist",
            Predicate::Scheduled,
            "Tuesday",
            "The dentist is on Tuesday.",
        ),
    );
    here.engine
        .store_embeddings(vec![(own, hash_embedding("dentist on Tuesday"))])
        .unwrap();
    bring_newest(&here.engine, &place, &key);

    let waiting = here.engine.memories_without_embedding(10).unwrap();
    assert!(
        waiting.iter().all(|(id, _)| *id != swim),
        "its vector came with it"
    );
    let query = "pool classes midweek";
    let by_words = here.engine.retrieve(query, None, 5).unwrap();
    assert!(by_words.iter().all(|h| h.memory.id != swim));
    let hits = here
        .engine
        .retrieve(
            query,
            Some(&hash_embedding("swimming lessons on Wednesday")),
            5,
        )
        .unwrap();
    assert_eq!(hits[0].memory.id, swim);
}
