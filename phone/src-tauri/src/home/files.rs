//! Backups through the phone's own sheets. The engine writes and reads plain files, so a backup
//! is made in a folder of the app's cache and handed to the save sheet, and a chosen backup is
//! copied into another before it is checked or brought back. Neither copy outlives its use.

use std::path::Path;

use nocturne_contract::{HomeAnswer, HomeError};
use serde::Serialize;
use tauri_plugin_nocturne_system::Sheet;

use super::calls::{Progress, clear};
use super::{Home, Job};

/// The folder of the app's cache a backup is made in before the save sheet takes it.
pub const OUTBOX: &str = "backup-out";
/// The folder of the app's cache a chosen backup is copied into.
pub const INBOX: &str = "backup-in";

/// The phone's save and open sheets, behind a seam so tests need no phone.
pub trait Sheets {
    /// Offer the file at `src` under `name`; blocks until it is written where the person chose.
    fn save(&self, src: &Path, name: &str) -> Sheet;
    /// Let the person choose a file; it is copied into `dir`.
    fn choose(&self, dir: &Path) -> Sheet;
}

/// A backup file the person chose, ready to check and bring back.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChosenBackup {
    /// The copy to pass as `file` to `checkBackup` and `restoreBackup`.
    pub file: String,
    /// Its own name.
    pub name: String,
    pub size: f64,
    /// When the file last changed where it was, when that place says. The backup's own date is
    /// inside the lock and shows only after the passphrase.
    pub modified: Option<String>,
}

fn from_millis(ms: i64) -> Option<String> {
    jiff::Timestamp::from_millisecond(ms)
        .ok()
        .map(|t| t.to_string())
}

impl Home {
    /// One sheet and its copy at a time: a second ask while one is up is `NotReady`.
    fn begin_sheet(&self) -> Result<Job<'_>, HomeError> {
        if self.sheet.swap(true, std::sync::atomic::Ordering::SeqCst) {
            return Err(HomeError::NotReady);
        }
        Ok(Job(&self.sheet, &self.stop))
    }

    /// Make a backup in `outbox` and offer it on the save sheet. `None` when the person closed the
    /// sheet: nothing was saved, and the last backup stays as it was.
    pub fn save_backup_file(
        &self,
        outbox: &Path,
        passphrase: &str,
        sheets: &dyn Sheets,
        progress: Progress<'_>,
    ) -> Option<HomeAnswer> {
        let fail = |code| Some(HomeAnswer::Error { code });
        let _sheet = match self.begin_sheet() {
            Ok(sheet) => sheet,
            Err(code) => return fail(code),
        };
        let name = match self.using() {
            Ok((_use, open)) => open.engine.suggested_backup_name(),
            Err(code) => return fail(code),
        };
        clear(outbox);
        if let Err(e) = std::fs::create_dir_all(outbox) {
            return fail(if super::io_full(&e) {
                HomeError::Full
            } else {
                HomeError::Failed
            });
        }
        let dest = outbox.join(&name);
        let made = self.make_backup(&dest, passphrase, progress);
        let sheet = made.as_ref().ok().map(|_| sheets.save(&dest, &name));
        clear(outbox);
        let report = match made {
            Ok(report) => report,
            Err(code) => return fail(code),
        };
        match sheet? {
            Sheet::Saved => {
                self.note_backup(report.bytes);
                Some(HomeAnswer::BackupSaved { report })
            }
            Sheet::Cancelled => None,
            Sheet::Full => fail(HomeError::Full),
            Sheet::Chosen(_) | Sheet::Failed => fail(HomeError::Failed),
        }
    }

    /// Let the person choose a backup file; it is copied into `inbox` and checked to be a
    /// Nocturne backup before anything else. `None` when the sheet was closed.
    pub fn choose_backup_file(
        &self,
        inbox: &Path,
        sheets: &dyn Sheets,
    ) -> Result<Option<ChosenBackup>, HomeError> {
        let _sheet = self.begin_sheet()?;
        clear(inbox);
        let chosen = match sheets.choose(inbox) {
            Sheet::Chosen(chosen) => chosen,
            Sheet::Cancelled => return Ok(None),
            Sheet::Full => return Err(HomeError::Full),
            Sheet::Saved | Sheet::Failed => return Err(HomeError::Failed),
        };
        let file = Path::new(&chosen.path);
        let inside = file
            .parent()
            .zip(dunce::canonicalize(inbox).ok())
            .is_some_and(|(parent, inbox)| {
                dunce::canonicalize(parent).is_ok_and(|parent| parent == inbox)
            });
        if !inside || !file.is_file() {
            clear(inbox);
            return Err(HomeError::Failed);
        }
        if nocturne_engine::probe_backup(file).is_err() {
            clear(inbox);
            return Err(HomeError::NotABackup);
        }
        Ok(Some(ChosenBackup {
            file: chosen.path,
            name: chosen.name,
            size: chosen.size as f64,
            modified: chosen.modified.and_then(from_millis),
        }))
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use nocturne_contract::{BackupPhase, HomeCall};
    use nocturne_engine::ManualClock;
    use tauri_plugin_nocturne_system::Chosen;

    use super::*;

    const PASS: &str = "a long enough passphrase";

    fn no_progress(_: BackupPhase, _: u64, _: u64) {}

    fn alone(dir: &Path) -> Home {
        let clock = Arc::new(ManualClock::at("2026-11-22T09:00:00Z", "Europe/London"));
        let mut home = Home::new(dir.join("app"), clock, Box::new(|_: &Path| {}));
        home.set_work_factor(10);
        home.start_alone("Pixel", false).unwrap();
        home
    }

    /// Sheets that answer as told; a save copies the file to `place`, a choice copies `pick`.
    struct Fake {
        save: Sheet,
        place: PathBuf,
        pick: Option<PathBuf>,
        choose: Sheet,
    }

    impl Fake {
        fn new(dir: &Path) -> Fake {
            Fake {
                save: Sheet::Saved,
                place: dir.join("drive"),
                pick: None,
                choose: Sheet::Cancelled,
            }
        }
    }

    impl Sheets for Fake {
        fn save(&self, src: &Path, name: &str) -> Sheet {
            if self.save == Sheet::Saved {
                std::fs::create_dir_all(&self.place).unwrap();
                std::fs::copy(src, self.place.join(name)).unwrap();
            }
            self.save.clone()
        }

        fn choose(&self, dir: &Path) -> Sheet {
            let Some(pick) = &self.pick else {
                return self.choose.clone();
            };
            std::fs::create_dir_all(dir).unwrap();
            let path = dir.join("chosen");
            std::fs::copy(pick, &path).unwrap();
            Sheet::Chosen(Chosen {
                path: path.to_string_lossy().into_owned(),
                name: "Nocturne backup 2026-11-22.nocturne".into(),
                size: std::fs::metadata(&path).unwrap().len(),
                modified: Some(1_763_800_000_000),
            })
        }
    }

    fn saved_backup(home: &Home, tmp: &Path) -> PathBuf {
        let fake = Fake::new(tmp);
        let answer = home.save_backup_file(&tmp.join(OUTBOX), PASS, &fake, &no_progress);
        assert!(
            matches!(answer, Some(HomeAnswer::BackupSaved { .. })),
            "{answer:?}"
        );
        fake.place.join("Nocturne backup 2026-11-22.nocturne")
    }

    #[test]
    fn a_saved_backup_lands_where_the_person_chose_and_its_copy_goes() {
        let tmp = tempfile::tempdir().unwrap();
        let home = alone(tmp.path());
        let file = saved_backup(&home, tmp.path());
        assert!(file.is_file());
        assert!(!tmp.path().join(OUTBOX).exists());
        assert!(home.last_backup().is_some());
        let checked = home.local_call(
            HomeCall::CheckBackup {
                file: file.to_string_lossy().into_owned(),
                passphrase: PASS.into(),
            },
            false,
            &no_progress,
        );
        assert!(
            matches!(checked, HomeAnswer::BackupChecked { .. }),
            "{checked:?}"
        );
    }

    #[test]
    fn a_closed_save_sheet_saves_nothing_and_notes_no_backup() {
        let tmp = tempfile::tempdir().unwrap();
        let home = alone(tmp.path());
        let mut fake = Fake::new(tmp.path());
        fake.save = Sheet::Cancelled;
        let answer = home.save_backup_file(&tmp.path().join(OUTBOX), PASS, &fake, &no_progress);
        assert_eq!(answer, None);
        assert!(!tmp.path().join(OUTBOX).exists());
        assert!(home.last_backup().is_none());
    }

    #[test]
    fn a_stopped_backup_saves_nothing_and_says_it_was_stopped() {
        let tmp = tempfile::tempdir().unwrap();
        let home = alone(tmp.path());
        let fake = Fake::new(tmp.path());
        let stop = |_: BackupPhase, _: u64, _: u64| home.stop_job();
        let answer = home.save_backup_file(&tmp.path().join(OUTBOX), PASS, &fake, &stop);
        assert_eq!(
            answer,
            Some(HomeAnswer::Error {
                code: HomeError::Stopped
            })
        );
        assert!(!tmp.path().join("drive").exists());
        assert!(home.last_backup().is_none());
        // A stop with nothing running does not wait for the next job.
        home.stop_job();
        // The next job starts without the stop.
        let again = home.save_backup_file(&tmp.path().join(OUTBOX), PASS, &fake, &no_progress);
        assert!(
            matches!(again, Some(HomeAnswer::BackupSaved { .. })),
            "{again:?}"
        );
    }

    #[test]
    fn a_place_with_no_room_says_so() {
        let tmp = tempfile::tempdir().unwrap();
        let home = alone(tmp.path());
        let mut fake = Fake::new(tmp.path());
        fake.save = Sheet::Full;
        let answer = home.save_backup_file(&tmp.path().join(OUTBOX), PASS, &fake, &no_progress);
        assert_eq!(
            answer,
            Some(HomeAnswer::Error {
                code: HomeError::Full
            })
        );
        assert!(home.last_backup().is_none());
    }

    #[test]
    fn a_short_passphrase_never_reaches_the_sheet() {
        let tmp = tempfile::tempdir().unwrap();
        let home = alone(tmp.path());
        let mut fake = Fake::new(tmp.path());
        fake.save = Sheet::Failed;
        let answer = home.save_backup_file(&tmp.path().join(OUTBOX), "short", &fake, &no_progress);
        assert!(
            matches!(answer, Some(HomeAnswer::Error { .. })),
            "{answer:?}"
        );
        assert!(!fake.place.exists());
        assert!(!tmp.path().join(OUTBOX).exists());
    }

    #[test]
    fn a_chosen_backup_is_copied_dated_and_can_be_checked() {
        let tmp = tempfile::tempdir().unwrap();
        let home = alone(tmp.path());
        let mut fake = Fake::new(tmp.path());
        fake.pick = Some(saved_backup(&home, tmp.path()));
        let inbox = tmp.path().join(INBOX);
        let chosen = home.choose_backup_file(&inbox, &fake).unwrap().unwrap();
        assert_eq!(chosen.modified.as_deref(), Some("2025-11-22T08:26:40Z"));
        assert!(chosen.size > 0.0);
        assert!(Path::new(&chosen.file).starts_with(&inbox));
        let checked = home.local_call(
            HomeCall::CheckBackup {
                file: chosen.file,
                passphrase: PASS.into(),
            },
            false,
            &no_progress,
        );
        assert!(
            matches!(checked, HomeAnswer::BackupChecked { .. }),
            "{checked:?}"
        );
    }

    #[test]
    fn a_file_that_is_no_backup_is_told_at_once_and_let_go() {
        let tmp = tempfile::tempdir().unwrap();
        let home = alone(tmp.path());
        let other = tmp.path().join("letter.txt");
        std::fs::write(&other, "dear diary").unwrap();
        let mut fake = Fake::new(tmp.path());
        fake.pick = Some(other);
        let inbox = tmp.path().join(INBOX);
        assert_eq!(
            home.choose_backup_file(&inbox, &fake),
            Err(HomeError::NotABackup)
        );
        assert!(!inbox.exists());
    }

    #[test]
    fn a_closed_open_sheet_chooses_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let home = alone(tmp.path());
        let mut fake = Fake::new(tmp.path());
        let inbox = tmp.path().join(INBOX);
        assert_eq!(home.choose_backup_file(&inbox, &fake), Ok(None));
        fake.choose = Sheet::Full;
        assert_eq!(home.choose_backup_file(&inbox, &fake), Err(HomeError::Full));
    }

    #[test]
    fn a_copy_said_to_be_outside_the_folder_is_refused() {
        struct Elsewhere(PathBuf);
        impl Sheets for Elsewhere {
            fn save(&self, _: &Path, _: &str) -> Sheet {
                Sheet::Failed
            }
            fn choose(&self, _: &Path) -> Sheet {
                Sheet::Chosen(Chosen {
                    path: self.0.to_string_lossy().into_owned(),
                    name: String::new(),
                    size: 1,
                    modified: None,
                })
            }
        }
        let tmp = tempfile::tempdir().unwrap();
        let home = alone(tmp.path());
        let file = saved_backup(&home, tmp.path());
        let inbox = tmp.path().join(INBOX);
        assert_eq!(
            home.choose_backup_file(&inbox, &Elsewhere(file)),
            Err(HomeError::Failed)
        );
    }

    #[test]
    fn a_second_sheet_waits_for_the_first() {
        let tmp = tempfile::tempdir().unwrap();
        let home = alone(tmp.path());
        let _up = home.begin_sheet().unwrap();
        let fake = Fake::new(tmp.path());
        let answer = home.save_backup_file(&tmp.path().join(OUTBOX), PASS, &fake, &no_progress);
        assert_eq!(
            answer,
            Some(HomeAnswer::Error {
                code: HomeError::NotReady
            })
        );
        assert_eq!(
            home.choose_backup_file(&tmp.path().join(INBOX), &fake),
            Err(HomeError::NotReady)
        );
        assert!(!fake.place.exists());
    }

    #[test]
    fn a_date_in_milliseconds_reads_as_a_timestamp() {
        assert_eq!(
            from_millis(1_763_800_000_000).as_deref(),
            Some("2025-11-22T08:26:40Z")
        );
    }
}
