use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use nocturne_contract::{RemoteAnswer, RemoteCall};
use nocturne_engine::nightly::{BackupPlace, MemoryPlace, PlaceEntry, PlaceError};
use nocturne_engine::{Clock, ManualClock};

use super::{
    BackupKeys, BackupProblem, BackupState, MemoryBackupKeys, Opening, PhoneBackup, Trouble, lock,
};
use crate::home::{Home, dispatch};

const PHRASE: &str = "harbour violet pencil maple drift oven";

struct Shared(Arc<MemoryBackupKeys>);

impl BackupKeys for Shared {
    fn keep(&self, key: &str) -> bool {
        self.0.keep(key)
    }
    fn key(&self) -> Option<String> {
        self.0.key()
    }
    fn forget(&self) {
        self.0.forget()
    }
}

struct Phone {
    _dir: tempfile::TempDir,
    clock: Arc<ManualClock>,
    home: Home,
    backup: PhoneBackup,
    keys: Arc<MemoryBackupKeys>,
}

fn phone(place: &Arc<MemoryPlace>, alone: bool) -> Phone {
    let dir = tempfile::tempdir().unwrap();
    let clock = Arc::new(ManualClock::at("2026-11-24T22:00:00Z", "Europe/London"));
    let home = Home::new(
        dir.path().to_path_buf(),
        clock.clone(),
        Box::new(|_: &Path| {}),
    );
    if alone {
        home.start_alone("Pixel", false).unwrap();
    }
    let keys = Arc::new(MemoryBackupKeys::default());
    let mut backup = PhoneBackup::new(
        dir.path().to_path_buf(),
        Box::new(Shared(keys.clone())),
        clock.clone(),
    );
    backup.set_work_factor(4);
    backup.set_place(Some(place.clone()));
    Phone {
        _dir: dir,
        clock,
        home,
        backup,
        keys,
    }
}

impl Phone {
    fn keep(&self, text: &str) -> String {
        let call = RemoteCall::KeepText {
            text: text.into(),
            client_id: format!("c-{}", text.len()),
            feeling: None,
            good: None,
            captured_at: None,
            web: false,
        };
        match dispatch::answer(&self.home.open_store().unwrap(), call) {
            RemoteAnswer::Kept { kept } => kept.item.id,
            other => panic!("not kept: {other:?}"),
        }
    }

    fn now(&self) -> Result<super::LastNight, BackupProblem> {
        self.backup.back_up_now(&self.home, false, &|_, _, _| {})
    }

    /// A recovery key made and saved, so it opens the backup.
    fn made_key(&self) -> String {
        let text = self.backup.recovery_key().unwrap();
        self.backup.keep_recovery_key().unwrap();
        text
    }

    fn state(&self) -> BackupState {
        self.backup.status().state
    }

    fn later(&self, hours: i64) {
        let now = self.clock.now();
        self.clock
            .set(now + jiff::SignedDuration::from_hours(hours));
    }
}

fn nights(place: &MemoryPlace) -> usize {
    place
        .names()
        .iter()
        .filter(|n| n.starts_with("night-"))
        .count()
}

#[test]
fn nothing_runs_until_the_phone_can_reach_an_account() {
    let place = Arc::new(MemoryPlace::new());
    let p = phone(&place, true);
    assert_eq!(p.state(), BackupState::Off);
    p.backup.set_place(None);
    assert_eq!(p.backup.turn_on(PHRASE), Err(BackupProblem::Unavailable));
    assert_eq!(p.backup.found(), Err(BackupProblem::Unavailable));
    assert!(!p.backup.pass(&p.home, true));
}

#[test]
fn a_backup_turned_on_runs_its_first_night_and_comes_back_on_a_new_phone() {
    let place = Arc::new(MemoryPlace::new());
    let old = phone(&place, true);
    old.keep("The plumber comes on Tuesday.");
    old.backup.turn_on(PHRASE).unwrap();
    assert!(old.keys.key().is_some(), "the key is in the safe place");
    assert_eq!(old.state(), BackupState::Waiting);
    let last = old.now().unwrap();
    assert!(last.bytes > 0);
    assert_eq!(old.state(), BackupState::UpToDate);
    assert_eq!(old.backup.status().last, Some(last));
    assert_eq!(old.backup.turn_on(PHRASE), Err(BackupProblem::AlreadyThere));
    let recovery = old.made_key();

    let new = phone(&place, false);
    assert!(new.backup.found().unwrap().is_some());
    assert_eq!(
        new.backup
            .restore(
                &new.home,
                false,
                &Opening::Phrase("harbour violet".into()),
                None,
                &|_, _, _| {}
            )
            .unwrap_err(),
        BackupProblem::WrongPhrase
    );
    let restored = new
        .backup
        .restore(
            &new.home,
            false,
            &Opening::RecoveryKey(recovery),
            None,
            &|_, _, _| {},
        )
        .unwrap();
    let super::Restored::Replaced { summary, own, .. } = restored else {
        panic!("an empty phone takes the backup as it is");
    };
    assert!(!own, "another phone's backup");
    assert_eq!(summary.items, 1);
    let open = new.home.open_store().unwrap();
    assert_eq!(open.engine.find_items("plumber", 5).unwrap().len(), 1);
    drop(open);

    // The new phone waits to be the one in use, off meanwhile; the old one keeps going.
    assert_eq!(new.state(), BackupState::Off);
    new.later(24);
    assert!(!new.backup.pass(&new.home, true));
    assert_eq!(nights(&place), 1);

    // Made the one in use, it writes its first night at once.
    new.backup.make_in_use(&new.home).unwrap();
    assert_eq!(new.state(), BackupState::UpToDate);
    assert_eq!(nights(&place), 2);

    // The old phone finds the newer night and stops, overwriting nothing.
    old.later(24);
    assert!(!old.backup.pass(&old.home, true));
    assert!(matches!(old.state(), BackupState::AnotherPhone { .. }));
    old.later(24);
    assert!(!old.backup.pass(&old.home, true));
    assert_eq!(nights(&place), 2);
}

#[test]
fn a_night_runs_while_charging_once_a_day() {
    let place = Arc::new(MemoryPlace::new());
    let p = phone(&place, true);
    p.keep("Something to keep.");
    p.backup.turn_on(PHRASE).unwrap();
    assert!(!p.backup.pass(&p.home, false), "not on battery");
    assert!(p.backup.pass(&p.home, true));
    assert!(!p.backup.pass(&p.home, true), "one a night");
    p.later(21);
    assert_eq!(
        p.state(),
        BackupState::NotCharged,
        "not charged since the last night"
    );
    assert!(p.backup.pass(&p.home, true));
    assert_eq!(nights(&place), 2);
    p.later(21);
    place.fail_with(Some(PlaceError::Offline));
    assert!(!p.backup.pass(&p.home, true));
    assert_eq!(
        p.state(),
        BackupState::Waiting,
        "charged, and waits for tonight"
    );
}

#[test]
fn the_status_says_what_still_waits_for_the_account() {
    let place = Arc::new(MemoryPlace::new());
    let p = phone(&place, true);
    let id = p.keep("Something to forget.");
    p.keep("Something to keep.");
    p.backup.turn_on(PHRASE).unwrap();
    p.now().unwrap();
    assert!(!p.backup.status_with(&p.home).forget_waiting);

    place.fail_with(Some(PlaceError::Offline));
    p.home.open_store().unwrap().engine.unkeep(&id).unwrap();
    assert!(!p.backup.pass(&p.home, false));
    assert!(p.backup.status_with(&p.home).forget_waiting);
    place.fail_with(None);
    p.later(2);
    assert!(p.backup.pass(&p.home, false));
    assert!(!p.backup.status_with(&p.home).forget_waiting);

    place.fail_with(Some(PlaceError::Offline));
    assert_eq!(p.backup.turn_off(true), Err(BackupProblem::Offline));
    assert!(p.backup.status().delete_pending);
    place.fail_with(None);
    assert!(!p.backup.pass(&p.home, false));
    assert!(!p.backup.status().delete_pending);
    assert_eq!(nights(&place), 0);
}

#[test]
fn wifi_only_waits_for_wifi() {
    let place = Arc::new(MemoryPlace::new());
    let p = phone(&place, true);
    p.keep("Something to keep.");
    p.backup.turn_on(PHRASE).unwrap();
    assert!(!p.backup.wifi_only());
    p.backup.set_wifi_only(true);
    assert!(
        p.backup.wifi_only(),
        "the background windows wait for Wi-Fi too"
    );
    assert_eq!(p.state(), BackupState::WaitingForWifi);
    assert!(!p.backup.pass(&p.home, true));
    assert_eq!(p.now(), Err(BackupProblem::WaitingForWifi));
    p.backup.set_unmetered(Some(false));
    assert!(!p.backup.pass(&p.home, true));
    p.backup
        .back_up_now(&p.home, true, &|_, _, _| {})
        .expect("chosen over mobile data this once");
    p.later(21);
    p.backup.set_unmetered(Some(true));
    assert!(p.backup.pass(&p.home, true));
    p.backup.turn_off(true).unwrap();
    assert!(
        !p.backup.wifi_only(),
        "a deletion owed to the account goes on any network"
    );
}

#[test]
fn a_forget_runs_a_night_at_once_even_on_battery() {
    let place = Arc::new(MemoryPlace::new());
    let p = phone(&place, true);
    let id = p.keep("Something to forget.");
    p.keep("Something to keep.");
    p.backup.turn_on(PHRASE).unwrap();
    p.now().unwrap();
    p.now().unwrap();
    assert_eq!(nights(&place), 2);
    p.home.open_store().unwrap().engine.unkeep(&id).unwrap();
    assert!(p.backup.pass(&p.home, false));
    assert_eq!(
        nights(&place),
        1,
        "older nights went with what was forgotten"
    );
}

#[test]
fn the_account_refusing_shows_and_the_next_night_tries_again() {
    let place = Arc::new(MemoryPlace::new());
    let p = phone(&place, true);
    p.keep("Something to keep.");
    p.backup.turn_on(PHRASE).unwrap();
    place.fail_with(Some(PlaceError::NoRoom));
    assert_eq!(p.now(), Err(BackupProblem::NoRoom));
    p.later(2);
    assert!(!p.backup.pass(&p.home, true));
    assert_eq!(p.state(), BackupState::NoRoom);
    place.fail_with(None);
    assert!(
        !p.backup.pass(&p.home, true),
        "a full account is not asked at every look"
    );
    p.later(2);
    assert!(p.backup.pass(&p.home, true));
    assert_eq!(p.state(), BackupState::UpToDate);

    for (error, state, trouble) in [
        (
            PlaceError::SignedOut,
            BackupState::SignedOut,
            Trouble::SignedOut,
        ),
        (
            PlaceError::NoPermission,
            BackupState::NoPermission,
            Trouble::NoPermission,
        ),
    ] {
        place.fail_with(Some(error.clone()));
        p.later(21);
        assert!(!p.backup.pass(&p.home, true));
        assert_eq!(p.state(), state, "{error}");
        place.fail_with(None);
        p.later(21);
        assert!(!p.backup.pass(&p.home, true), "waits for the account again");
        assert_eq!(p.backup.saved().trouble, Some(trouble));
        p.backup.turn_off(false).unwrap();
        p.backup.turn_on_again().unwrap();
    }
}

#[test]
fn turning_off_keeps_the_backup_and_a_forget_still_reaches_it() {
    let place = Arc::new(MemoryPlace::new());
    let p = phone(&place, true);
    let id = p.keep("Something to forget.");
    p.keep("Something to keep.");
    p.backup.turn_on(PHRASE).unwrap();
    p.now().unwrap();
    p.now().unwrap();
    p.backup.turn_off(false).unwrap();
    assert_eq!(p.state(), BackupState::Off);
    assert!(p.keys.key().is_some(), "the key stays for turning on again");
    assert!(!p.backup.pass(&p.home, true), "no nights while off");
    assert_eq!(nights(&place), 2);
    p.home.open_store().unwrap().engine.unkeep(&id).unwrap();
    assert!(p.backup.pass(&p.home, false));
    assert_eq!(nights(&place), 1, "the forget reached the kept backup");
    assert_eq!(p.state(), BackupState::Off);
    p.backup.turn_on_again().unwrap();
    assert_eq!(p.state(), BackupState::UpToDate);
}

#[test]
fn turning_off_with_delete_and_delete_everything_empty_the_account() {
    let place = Arc::new(MemoryPlace::new());
    let p = phone(&place, true);
    p.keep("Something to keep.");
    p.backup.turn_on(PHRASE).unwrap();
    p.now().unwrap();
    p.backup.turn_off(true).unwrap();
    assert!(p.keys.key().is_none());
    assert!(place.names().is_empty());

    p.backup.turn_on(PHRASE).unwrap();
    p.now().unwrap();
    // While the account does not answer, the delete waits, and goes when it does.
    place.fail_with(Some(PlaceError::Offline));
    p.backup.erase();
    assert_eq!(p.state(), BackupState::Off);
    assert!(p.backup.is_on(), "the delete still asks for time");
    place.fail_with(None);
    assert!(!p.backup.pass(&p.home, true));
    assert!(place.names().is_empty());
    assert!(!p.backup.is_on());
}

#[test]
fn a_night_with_no_key_here_waits_instead_of_spinning() {
    let place = Arc::new(MemoryPlace::new());
    let p = phone(&place, true);
    p.keep("Something to keep.");
    p.backup.turn_on(PHRASE).unwrap();
    p.keys.forget();
    assert!(!p.backup.pass(&p.home, true));
    assert!(lock(&p.backup.failed_at).is_some(), "the next look waits");
}

#[test]
fn a_backup_restored_onto_a_phone_with_a_memory_brings_its_things_in() {
    let place = Arc::new(MemoryPlace::new());
    let old = phone(&place, true);
    old.keep("The plumber comes on Tuesday.");
    old.backup.turn_on(PHRASE).unwrap();
    old.now().unwrap();
    let new = phone(&place, true);
    new.keep("Already here.");
    let opening = Opening::Phrase(PHRASE.into());
    assert_eq!(new.backup.nights(&opening).unwrap().len(), 1);
    let restored = new
        .backup
        .restore(&new.home, false, &opening, None, &|_, _, _| {})
        .unwrap();
    assert!(matches!(
        restored,
        super::Restored::BroughtIn {
            things: 1,
            own: false,
            ..
        }
    ));
    assert!(
        new.home.put_aside_names().is_empty(),
        "nothing is put aside"
    );
    let open = new.home.open_store().unwrap();
    assert_eq!(open.engine.find_items("plumber", 5).unwrap().len(), 1);
    assert_eq!(open.engine.find_items("Already", 5).unwrap().len(), 1);
    drop(open);
    assert_eq!(new.state(), BackupState::Off);
}

#[test]
fn the_backup_stops_by_itself_once_the_memory_moves_to_a_computer() {
    let place = Arc::new(MemoryPlace::new());
    let p = phone(&place, true);
    p.keep("The plumber comes on Tuesday.");
    p.backup.turn_on(PHRASE).unwrap();
    p.now().unwrap();
    let written = nights(&place);

    // The person chose the computer's memory: the memory here is put aside.
    p.home.use_computer().unwrap();
    p.later(24);
    assert!(!p.backup.pass(&p.home, true), "no night runs");
    let status = p.backup.status();
    assert!(!status.on && status.moved);
    assert_eq!(p.now(), Err(BackupProblem::Off));
    assert_eq!(p.backup.turn_on_again(), Err(BackupProblem::Off));
    assert_eq!(nights(&place), written, "the backup stays as it was");
    assert!(p.keys.key().is_some(), "kept only to delete it later");

    p.backup.turn_off(true).unwrap();
    assert!(place.names().is_empty(), "deleting it later still works");
}

#[test]
fn a_move_says_so_and_nothing_runs_after_a_forget() {
    let place = Arc::new(MemoryPlace::new());
    let p = phone(&place, true);
    p.keep("Something to keep.");
    p.backup.turn_on(PHRASE).unwrap();
    p.now().unwrap();
    p.backup.memory_moved();
    assert!(p.backup.status().moved);
    assert!(!p.backup.pass(&p.home, true));
    assert_eq!(nights(&place), 1);
}

#[test]
fn choosing_the_account_again_carries_on_and_another_starts_afresh() {
    let old = Arc::new(MemoryPlace::new());
    let p = phone(&old, true);
    p.keep("Something to keep.");
    p.backup.turn_on(PHRASE).unwrap();
    let recovery = p.made_key();
    p.now().unwrap();

    old.fail_with(Some(PlaceError::NoPermission));
    p.later(21);
    assert!(!p.backup.pass(&p.home, true));
    assert_eq!(p.state(), BackupState::NoPermission);
    old.fail_with(None);
    p.backup.reconnected(None, None);
    assert_eq!(p.state(), BackupState::Waiting);
    assert!(p.backup.pass(&p.home, true), "carries on at once");
    assert_eq!(nights(&old), 2);

    let new = Arc::new(MemoryPlace::new());
    p.backup.set_place(Some(new.clone()));
    p.backup.reconnected(None, Some("before@example.org"));
    assert!(p.backup.saved().fresh);
    assert_eq!(p.backup.status().last, None);
    p.now().unwrap();
    assert!(!p.backup.saved().fresh);
    assert_eq!(nights(&new), 1);
    assert_eq!(nights(&old), 2, "the earlier account is left as it was");

    let other = phone(&new, true);
    let opening = Opening::Phrase(PHRASE.into());
    assert_eq!(other.backup.nights(&opening).unwrap().len(), 1);
    let by_key = Opening::RecoveryKey(recovery);
    assert_eq!(other.backup.nights(&by_key).unwrap().len(), 1);
}

#[test]
fn a_new_account_holding_another_backup_is_not_written_over() {
    let place = Arc::new(MemoryPlace::new());
    let p = phone(&place, true);
    p.keep("Something to keep.");
    p.backup.turn_on(PHRASE).unwrap();
    p.now().unwrap();

    let theirs = Arc::new(MemoryPlace::new());
    let someone = phone(&theirs, true);
    someone.keep("Someone else's.");
    someone.backup.turn_on("a phrase of their own").unwrap();
    someone.now().unwrap();

    p.backup.set_place(Some(theirs.clone()));
    p.backup.reconnected(None, Some("before@example.org"));
    assert!(p.now().is_err());
    assert_eq!(p.state(), BackupState::OtherBackup);
    assert_eq!(nights(&theirs), 1);
    assert!(p.backup.saved().fresh, "tries again only as a fresh start");
    p.later(24);
    assert!(
        !p.backup.pass(&p.home, true),
        "waits for the person, not the hour"
    );
    assert_eq!(nights(&theirs), 1);

    // Choosing an account again lets it try; an empty one takes the backup.
    let empty = Arc::new(MemoryPlace::new());
    p.backup.set_place(Some(empty.clone()));
    p.backup.reconnected(None, Some("before@example.org"));
    assert_eq!(p.state(), BackupState::Waiting);
    assert!(p.backup.pass(&p.home, true));
    assert_eq!(nights(&empty), 1);
}

/// An account whose reads fail while `reads_fail` is set, and whose pieces can't be written
/// while `pieces_fail` is; everything else works.
#[derive(Default)]
struct Flaky {
    inner: MemoryPlace,
    reads_fail: AtomicBool,
    pieces_fail: AtomicBool,
}

impl BackupPlace for Flaky {
    fn put(&self, name: &str, bytes: &[u8]) -> Result<(), PlaceError> {
        if name.starts_with("piece-") && self.pieces_fail.load(Ordering::SeqCst) {
            return Err(PlaceError::Offline);
        }
        self.inner.put(name, bytes)
    }
    fn get(&self, name: &str) -> Result<Option<Vec<u8>>, PlaceError> {
        if self.reads_fail.load(Ordering::SeqCst) {
            return Err(PlaceError::Offline);
        }
        self.inner.get(name)
    }
    fn list(&self, prefix: &str) -> Result<Vec<PlaceEntry>, PlaceError> {
        self.inner.list(prefix)
    }
    fn delete(&self, name: &str) -> Result<(), PlaceError> {
        self.inner.delete(name)
    }
}

fn reads_fail(place: &Flaky, on: bool) {
    place.reads_fail.store(on, Ordering::SeqCst);
}

/// A phone backing up to a `Flaky` account, with one night there.
fn flaky_phone() -> (Phone, Arc<Flaky>) {
    let old = Arc::new(Flaky::default());
    let p = phone(&Arc::new(MemoryPlace::new()), true);
    p.backup.set_place(Some(old.clone()));
    p.keep("Something to keep.");
    (p, old)
}

/// Moves `p` to a new, empty account and answers it.
fn new_account(p: &Phone) -> Arc<MemoryPlace> {
    let new = Arc::new(MemoryPlace::new());
    p.backup.set_place(Some(new.clone()));
    p.backup.reconnected(None, Some("before@example.org"));
    new
}

fn opens(place: &Arc<MemoryPlace>, opening: Opening) -> bool {
    phone(place, true)
        .backup
        .nights(&opening)
        .is_ok_and(|n| !n.is_empty())
}

#[test]
fn a_replaced_recovery_key_never_opens_the_backup_on_a_new_account() {
    let (p, old) = flaky_phone();
    p.backup.turn_on(PHRASE).unwrap();
    let recovery = p.made_key();
    reads_fail(&old, true);
    let replaced = p.made_key();
    reads_fail(&old, false);

    let new = new_account(&p);
    p.now().unwrap();
    assert!(!opens(&new, Opening::RecoveryKey(recovery)));
    assert!(opens(&new, Opening::RecoveryKey(replaced)));
    assert!(opens(&new, Opening::Phrase(PHRASE.into())));
}

#[test]
fn a_recovery_key_not_saved_opens_nothing_and_the_earlier_one_still_does() {
    let place = Arc::new(MemoryPlace::new());
    let p = phone(&place, true);
    p.keep("Something to keep.");
    p.backup.turn_on(PHRASE).unwrap();
    let saved = p.made_key();
    let shown = p.backup.recovery_key().unwrap();
    p.backup.drop_recovery_key();
    assert_eq!(p.backup.keep_recovery_key(), Err(BackupProblem::NotReady));
    p.now().unwrap();
    assert!(opens(&place, Opening::RecoveryKey(saved)));
    assert!(!opens(&place, Opening::RecoveryKey(shown)));
}

#[test]
fn a_recovery_key_the_account_did_not_take_can_be_kept_again() {
    let place = Arc::new(MemoryPlace::new());
    let p = phone(&place, true);
    p.keep("Something to keep.");
    p.backup.turn_on(PHRASE).unwrap();
    let shown = p.backup.recovery_key().unwrap();
    place.fail_with(Some(PlaceError::Offline));
    assert_eq!(p.backup.keep_recovery_key(), Err(BackupProblem::Offline));
    place.fail_with(None);
    p.backup.keep_recovery_key().unwrap();
    p.now().unwrap();
    assert!(opens(&place, Opening::RecoveryKey(shown)));
}

#[test]
fn locks_the_phone_could_not_read_back_are_never_carried() {
    let (p, old) = flaky_phone();
    reads_fail(&old, true);
    p.backup.turn_on(PHRASE).unwrap();
    let recovery = p.made_key();
    reads_fail(&old, false);
    assert_eq!(p.backup.saved().locks, None);

    let new = new_account(&p);
    assert_eq!(p.now(), Err(BackupProblem::NoPhrase));
    assert_eq!(p.state(), BackupState::NeedsPhrase);
    assert!(!p.backup.pass(&p.home, true), "waits for the phrase");
    assert!(new.names().is_empty(), "nothing is carried");
    assert_eq!(
        p.backup.recovery_key(),
        Err(BackupProblem::Damaged),
        "no recovery key is made for locks the phone doesn't know"
    );

    // "Change the phrase" makes a new copy, and the backup starts afresh with it.
    p.backup.change_phrase("lantern copper meadow").unwrap();
    assert_ne!(p.state(), BackupState::NeedsPhrase);
    assert!(
        new.names().is_empty(),
        "nothing goes before the first night"
    );
    p.now().unwrap();
    assert!(opens(&new, Opening::Phrase("lantern copper meadow".into())));
    assert!(!opens(&new, Opening::Phrase(PHRASE.into())));
    assert!(!opens(&new, Opening::RecoveryKey(recovery)));
}

#[test]
fn an_old_phrase_never_opens_the_backup_on_a_new_account() {
    let (p, old) = flaky_phone();
    p.backup.turn_on(PHRASE).unwrap();
    reads_fail(&old, true);
    p.backup.change_phrase("lantern copper meadow").unwrap();
    reads_fail(&old, false);

    let new = new_account(&p);
    p.now().unwrap();
    assert!(!opens(&new, Opening::Phrase(PHRASE.into())));
    assert!(opens(&new, Opening::Phrase("lantern copper meadow".into())));
}

#[test]
fn lock_changes_before_the_first_night_on_a_new_account_wait_for_it() {
    let p = phone(&Arc::new(MemoryPlace::new()), true);
    p.keep("Something to keep.");
    p.backup.turn_on(PHRASE).unwrap();
    let kept = p.made_key();
    p.now().unwrap();

    let new = new_account(&p);
    let made = p.made_key();
    p.backup.change_phrase("lantern copper meadow").unwrap();
    assert!(
        new.names().is_empty(),
        "nothing goes before the first night"
    );
    p.now().unwrap();
    assert!(opens(&new, Opening::RecoveryKey(made)), "the key made last");
    assert!(!opens(&new, Opening::RecoveryKey(kept)));
    assert!(opens(&new, Opening::Phrase("lantern copper meadow".into())));
    assert!(!opens(&new, Opening::Phrase(PHRASE.into())));
}

#[test]
fn a_recovery_lock_a_cut_short_start_left_goes_with_the_next_night() {
    let (p, _) = flaky_phone();
    p.backup.turn_on(PHRASE).unwrap();
    let recovery = p.made_key();
    p.now().unwrap();

    let new = Arc::new(Flaky::default());
    p.backup.set_place(Some(new.clone()));
    p.backup.reconnected(None, Some("before@example.org"));
    new.pieces_fail.store(true, Ordering::SeqCst);
    assert!(p.now().is_err());
    assert!(new.inner.names().contains(&"key-recovery".to_string()));

    let replaced = p.made_key();
    p.backup.change_phrase("lantern copper meadow").unwrap();
    new.pieces_fail.store(false, Ordering::SeqCst);
    p.now().unwrap();
    let other = phone(&Arc::new(MemoryPlace::new()), true);
    other.backup.set_place(Some(new.clone()));
    let opens = |opening| other.backup.nights(&opening).is_ok_and(|n| !n.is_empty());
    assert!(opens(Opening::RecoveryKey(replaced)));
    assert!(!opens(Opening::RecoveryKey(recovery)));
    assert!(!opens(Opening::Phrase(PHRASE.into())));
}

#[test]
fn a_night_across_an_account_change_notes_nothing() {
    let p = phone(&Arc::new(MemoryPlace::new()), true);
    p.keep("Something to keep.");
    p.backup.turn_on(PHRASE).unwrap();
    let state = p.backup.saved().state.unwrap();
    let account = p.backup.saved().account;
    new_account(&p);
    let ran = p.backup.note(
        state,
        Err(nocturne_engine::nightly::NightError::Cancelled),
        account,
        None,
    );
    assert!(ran.is_err());
    let saved = p.backup.saved();
    assert!(saved.fresh, "the new account still starts afresh");
    assert_eq!(saved.last, None);
}

#[test]
fn a_forget_with_backup_off_never_starts_a_new_account() {
    let p = phone(&Arc::new(MemoryPlace::new()), true);
    let id = p.keep("Something to forget.");
    p.backup.turn_on(PHRASE).unwrap();
    p.now().unwrap();
    p.backup.turn_off(false).unwrap();
    let new = new_account(&p);
    p.home.open_store().unwrap().engine.unkeep(&id).unwrap();
    assert!(!p.backup.pass(&p.home, true));
    assert!(new.names().is_empty());
}

#[test]
fn a_phrase_changed_during_the_first_night_on_a_new_account_goes_with_the_next() {
    let p = phone(&Arc::new(MemoryPlace::new()), true);
    p.keep("Something to keep.");
    p.backup.turn_on(PHRASE).unwrap();
    p.now().unwrap();
    let new = new_account(&p);

    // The first night carries the locks; the phrase changes before it is noted.
    let saved = p.backup.saved();
    let key = p.backup.key().unwrap();
    let mut state = saved.state.clone().unwrap();
    let carried = p.backup.start_afresh(new.as_ref(), &key, &state).unwrap();
    let night = p.home.open_store().unwrap().engine.back_up_night(
        new.as_ref(),
        &key,
        &mut state,
        &nocturne_engine::nightly::NightRun {
            progress: &|_, _, _| {},
            cancel: &AtomicBool::new(false),
            check: false,
        },
    );
    p.backup.change_phrase("lantern copper meadow").unwrap();
    {
        let _held = lock(&p.backup.locking);
        p.backup.note(state, night, saved.account, carried).unwrap();
    }
    assert!(
        p.backup.saved().fresh,
        "the changed copy is still to be carried"
    );
    assert!(opens(&new, Opening::Phrase(PHRASE.into())));

    p.later(21);
    p.now().unwrap();
    assert!(!p.backup.saved().fresh);
    assert!(opens(&new, Opening::Phrase("lantern copper meadow".into())));
    assert!(!opens(&new, Opening::Phrase(PHRASE.into())));
}

#[test]
fn a_phrase_changed_after_the_first_night_on_a_new_account_goes_there_at_once() {
    let p = phone(&Arc::new(MemoryPlace::new()), true);
    p.keep("Something to keep.");
    p.backup.turn_on(PHRASE).unwrap();
    p.now().unwrap();
    let new = new_account(&p);
    p.now().unwrap();
    p.backup.change_phrase("lantern copper meadow").unwrap();
    assert!(opens(&new, Opening::Phrase("lantern copper meadow".into())));
    assert!(!opens(&new, Opening::Phrase(PHRASE.into())));
}

#[test]
fn a_newer_backup_names_both_versions_to_the_screens() {
    let shown = serde_json::to_value(BackupProblem::Newer {
        theirs: Some("9.1.0".into()),
        ours: "0.7.2".into(),
    })
    .unwrap();
    assert_eq!(
        shown,
        serde_json::json!({ "newer": { "theirs": "9.1.0", "ours": "0.7.2" } })
    );
    assert_eq!(
        serde_json::to_value(BackupProblem::WrongPhrase).unwrap(),
        serde_json::json!("wrongPhrase")
    );
}

#[test]
fn this_phone_s_own_backup_turned_off_comes_back_as_its_own() {
    let place = Arc::new(MemoryPlace::new());
    let p = phone(&place, true);
    p.keep("The plumber comes on Tuesday.");
    p.backup.turn_on(PHRASE).unwrap();
    p.now().unwrap();
    p.backup.turn_off(false).unwrap();
    let restored = p
        .backup
        .restore(
            &p.home,
            false,
            &Opening::Phrase(PHRASE.into()),
            None,
            &|_, _, _| {},
        )
        .unwrap();
    assert!(matches!(
        restored,
        super::Restored::BroughtIn { own: true, .. }
    ));
    p.backup.turn_on_again().unwrap();
    assert!(
        p.backup.status().on,
        "the same backup, on again with no steps"
    );
}

#[test]
fn what_the_phone_notes_of_the_backup_stays_out_of_its_own_device_backup() {
    let dir = tempfile::tempdir().unwrap();
    let clock = Arc::new(ManualClock::at("2026-11-24T22:00:00Z", "Europe/London"));
    let marked = Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen = marked.clone();
    let mut backup = PhoneBackup::new(
        dir.path().to_path_buf(),
        Box::new(MemoryBackupKeys::default()),
        clock,
    )
    .with_exclude(Box::new(move |path: &Path| {
        seen.lock().unwrap().push(path.to_path_buf())
    }));
    backup.set_work_factor(4);
    backup.set_place(Some(Arc::new(MemoryPlace::new())));
    backup.turn_on(PHRASE).unwrap();
    backup.set_wifi_only(true);
    let marked = marked.lock().unwrap();
    let file = dir.path().join("nightly-backup.json");
    assert!(file.exists());
    assert!(marked.len() >= 2, "marked again after each write");
    assert!(marked.iter().all(|p| *p == file));
}

#[test]
fn a_recovery_key_made_for_another_backup_is_never_kept() {
    let place = Arc::new(MemoryPlace::new());
    let p = phone(&place, true);
    p.keep("Something to keep.");
    p.backup.turn_on(PHRASE).unwrap();
    p.backup.recovery_key().unwrap();
    p.backup.reconnected(None, Some("before@example.org"));
    assert_eq!(p.backup.keep_recovery_key(), Err(BackupProblem::NotReady));

    p.backup.recovery_key().unwrap();
    p.backup.turn_off(true).unwrap();
    assert_eq!(p.backup.keep_recovery_key(), Err(BackupProblem::NotReady));
}

#[test]
fn a_forget_waits_only_on_what_keeps_the_account_out_of_reach() {
    let place = Arc::new(MemoryPlace::new());
    let p = phone(&place, true);
    let id = p.keep("Something to forget.");
    p.backup.turn_on(PHRASE).unwrap();
    p.now().unwrap();
    place.fail_with(Some(PlaceError::NoRoom));
    p.home.open_store().unwrap().engine.unkeep(&id).unwrap();
    assert!(!p.backup.pass(&p.home, false));
    *lock(&p.backup.failed_at) = None;
    assert_eq!(p.state(), BackupState::NoRoom);
    assert!(
        !p.backup.status_with(&p.home).forget_waiting,
        "the account answers; it is full"
    );
}

fn connected(account: &str, previous: &str) -> Option<tauri_plugin_nocturne_system::Connected> {
    Some(tauri_plugin_nocturne_system::Connected::Connected {
        account: Some(account.into()),
        previous: Some(previous.into()),
        free: None,
    })
}

#[test]
fn a_deletion_owed_to_one_account_stays_with_it_across_a_change() {
    let old = Arc::new(MemoryPlace::new());
    let p = phone(&old, true);
    p.keep("Something to keep.");
    p.backup.turn_on(PHRASE).unwrap();
    p.now().unwrap();
    let held = old.names();

    // Delete everything while offline, then another account before the phone is back online.
    old.fail_with(Some(PlaceError::Offline));
    p.backup.erase();
    assert!(p.backup.status().delete_pending);
    let new = Arc::new(MemoryPlace::new());
    p.backup.set_place(Some(new.clone()));
    p.backup
        .reconnected(Some("b@example.org"), Some("a@example.org"));
    assert!(p.backup.status().delete_pending, "the change keeps it");
    assert!(!p.backup.is_on(), "nothing is owed to this account");
    assert!(!p.backup.pass(&p.home, true));

    // Backup turned on in the new account: its nights run, and it is never deleted.
    p.backup.turn_on(PHRASE).unwrap();
    p.now().unwrap();
    p.later(21);
    assert!(p.backup.pass(&p.home, true), "the new account's nights run");
    p.backup.turn_off(false).unwrap();
    assert!(!p.backup.pass(&p.home, true));
    let theirs = new.names();
    assert!(theirs.contains(&nocturne_engine::nightly::KEY_PHRASE.to_string()));
    old.fail_with(None);
    assert_eq!(old.names(), held, "the earlier account waits as it was");

    // The earlier account chosen again: the deletion owed to it runs there, and only there.
    p.backup.set_place(Some(old.clone()));
    p.backup
        .reconnected(Some("A@Example.org"), Some("b@example.org"));
    assert!(p.backup.is_on(), "the deletion asks for time");
    assert!(!p.backup.pass(&p.home, false));
    assert!(old.names().is_empty());
    assert!(!p.backup.status().delete_pending);
    assert_eq!(new.names(), theirs);
}

#[test]
fn a_deletion_owed_to_a_named_account_runs_when_that_account_is_back() {
    let old = Arc::new(MemoryPlace::new());
    let p = phone(&old, true);
    p.backup.reconnected(Some("a@example.org"), None);
    p.keep("Something to keep.");
    p.backup.turn_on(PHRASE).unwrap();
    p.now().unwrap();
    old.fail_with(Some(PlaceError::Offline));
    p.backup.turn_off(true).unwrap_err();

    let new = Arc::new(MemoryPlace::new());
    new.put("key-phrase", b"another backup").unwrap();
    p.backup.set_place(Some(new.clone()));
    p.backup
        .reconnected(Some("b@example.org"), Some("a@example.org"));
    // A second Delete everything, owed to the new account: the first stays owed too.
    p.backup.erase();
    assert!(new.names().is_empty(), "the new account's own deletion ran");
    assert!(p.backup.status().delete_pending);

    old.fail_with(None);
    p.backup.set_place(Some(old.clone()));
    p.backup
        .reconnected(Some("a@example.org"), Some("b@example.org"));
    assert!(!p.backup.pass(&p.home, false));
    assert!(old.names().is_empty());
    assert!(!p.backup.status().delete_pending);
}

/// An account whose first deletion waits until it is let go.
#[derive(Default)]
struct Gated {
    inner: MemoryPlace,
    started: AtomicBool,
    go: AtomicBool,
    deletes: Mutex<Vec<String>>,
}

impl BackupPlace for Gated {
    fn put(&self, name: &str, bytes: &[u8]) -> Result<(), PlaceError> {
        self.inner.put(name, bytes)
    }
    fn get(&self, name: &str) -> Result<Option<Vec<u8>>, PlaceError> {
        self.inner.get(name)
    }
    fn list(&self, prefix: &str) -> Result<Vec<PlaceEntry>, PlaceError> {
        self.inner.list(prefix)
    }
    fn delete(&self, name: &str) -> Result<(), PlaceError> {
        self.started.store(true, Ordering::SeqCst);
        while !self.go.load(Ordering::SeqCst) {
            std::thread::yield_now();
        }
        lock(&self.deletes).push(name.to_string());
        self.inner.delete(name)
    }
}

fn wait_for(flag: &AtomicBool) {
    while !flag.load(Ordering::SeqCst) {
        std::thread::yield_now();
    }
}

#[test]
fn a_change_of_account_waits_for_a_deletion_under_way() {
    let old = Arc::new(Gated::default());
    let p = phone(&Arc::new(MemoryPlace::new()), true);
    p.backup.set_place(Some(old.clone()));
    p.keep("Something to keep.");
    old.go.store(true, Ordering::SeqCst);
    p.backup.turn_on(PHRASE).unwrap();
    p.now().unwrap();
    old.go.store(false, Ordering::SeqCst);
    old.started.store(false, Ordering::SeqCst);

    let new = Arc::new(Gated::default());
    new.go.store(true, Ordering::SeqCst);
    let asked = AtomicBool::new(false);
    std::thread::scope(|scope| {
        scope.spawn(|| p.backup.erase());
        wait_for(&old.started);
        let change = scope.spawn(|| {
            p.backup.connect(|| {
                asked.store(true, Ordering::SeqCst);
                p.backup.set_place(Some(new.clone()));
                Ok::<_, ()>(connected("b@example.org", "a@example.org"))
            })
        });
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert!(
            !asked.load(Ordering::SeqCst),
            "the phone is not asked to change while a deletion runs"
        );
        old.go.store(true, Ordering::SeqCst);
        change.join().unwrap().unwrap();
    });
    assert!(old.inner.names().is_empty(), "the deletion finished there");
    assert!(lock(&new.deletes).is_empty(), "nothing was deleted here");
    assert!(!p.backup.status().delete_pending);
}

#[test]
fn a_deletion_waits_for_a_change_of_account_under_way_and_stays_with_the_earlier() {
    let old = Arc::new(MemoryPlace::new());
    let p = phone(&old, true);
    p.keep("Something to keep.");
    p.backup.turn_on(PHRASE).unwrap();
    p.now().unwrap();
    old.fail_with(Some(PlaceError::Offline));
    p.backup.erase();
    old.fail_with(None);

    let new = Arc::new(Gated::default());
    new.go.store(true, Ordering::SeqCst);
    new.put("key-phrase", b"theirs").unwrap();
    let looked = AtomicBool::new(false);
    std::thread::scope(|scope| {
        p.backup
            .connect(|| {
                // The phone has moved to the new account; a look starts before it is noted.
                p.backup.set_place(Some(new.clone()));
                scope.spawn(|| {
                    looked.store(true, Ordering::SeqCst);
                    p.backup.pass(&p.home, false)
                });
                wait_for(&looked);
                std::thread::sleep(std::time::Duration::from_millis(50));
                Ok::<_, ()>(connected("b@example.org", "a@example.org"))
            })
            .unwrap();
    });
    assert!(
        lock(&new.deletes).is_empty(),
        "the new account never gets it"
    );
    assert_eq!(new.inner.names(), vec!["key-phrase".to_string()]);
    assert!(!old.names().is_empty());
    assert!(
        p.backup.status().delete_pending,
        "still owed to the earlier"
    );
}
