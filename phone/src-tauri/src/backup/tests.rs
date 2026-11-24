use std::path::Path;
use std::sync::Arc;

use nocturne_contract::{RemoteAnswer, RemoteCall};
use nocturne_engine::nightly::{MemoryPlace, PlaceError};
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
    let recovery = old.backup.recovery_key().unwrap();

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
    let super::Restored::Replaced { summary, .. } = restored else {
        panic!("an empty phone takes the backup as it is");
    };
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
    assert_eq!(p.state(), BackupState::Waiting);
    assert!(p.backup.pass(&p.home, true));
    assert_eq!(nights(&place), 2);
}

#[test]
fn wifi_only_waits_for_wifi() {
    let place = Arc::new(MemoryPlace::new());
    let p = phone(&place, true);
    p.keep("Something to keep.");
    p.backup.turn_on(PHRASE).unwrap();
    p.backup.set_wifi_only(true);
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

    place.fail_with(Some(PlaceError::NoAccess));
    p.later(21);
    assert!(!p.backup.pass(&p.home, true));
    assert_eq!(p.state(), BackupState::NoAccess);
    place.fail_with(None);
    p.later(21);
    assert!(!p.backup.pass(&p.home, true), "waits for the account again");
    let saved = p.backup.saved();
    assert_eq!(saved.trouble, Some(Trouble::NoAccess));
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
        super::Restored::BroughtIn { things: 1, .. }
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
