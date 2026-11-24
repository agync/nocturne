//! The person's own account as the backup's place (ADR 0019): a "Nocturne backup" folder in their
//! Google Drive on Android, the app's own iCloud space on an iPhone. The phone's side of the
//! system plugin does the calls; this checks names, moves the bytes through a private staging
//! folder, and turns the phone's answers into [`PlaceError`]s.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use nocturne_engine::nightly::{BackupPlace, PlaceEntry, PlaceError};
use serde::Serialize;
use tauri::{AppHandle, Runtime};
use tauri_plugin_nocturne_system::{Connected, PlaceAnswer, PlaceOutcome, SystemExt};

use super::{BackupProblem, PhoneBackup};

/// The staging folder's name in the app's cache.
const STAGING: &str = "backup-place";

/// The phone's side of the place, as [`AccountPlace`] needs it. An `Err` is a call that did not
/// reach the phone's side at all.
pub trait PlaceCalls: Send + Sync {
    fn put(&self, name: &str, path: &Path) -> Result<PlaceAnswer, String>;
    fn get(&self, name: &str, path: &Path) -> Result<PlaceAnswer, String>;
    fn list(&self, prefix: &str) -> Result<PlaceAnswer, String>;
    fn delete(&self, name: &str) -> Result<PlaceAnswer, String>;
}

/// The calls through the system plugin.
pub struct SystemCalls<R: Runtime>(pub AppHandle<R>);

impl<R: Runtime> PlaceCalls for SystemCalls<R> {
    fn put(&self, name: &str, path: &Path) -> Result<PlaceAnswer, String> {
        self.0
            .system()
            .place_put(name, path)
            .map_err(|e| format!("{e:?}"))
    }

    fn get(&self, name: &str, path: &Path) -> Result<PlaceAnswer, String> {
        self.0
            .system()
            .place_get(name, path)
            .map_err(|e| format!("{e:?}"))
    }

    fn list(&self, prefix: &str) -> Result<PlaceAnswer, String> {
        self.0
            .system()
            .place_list(prefix)
            .map_err(|e| format!("{e:?}"))
    }

    fn delete(&self, name: &str) -> Result<PlaceAnswer, String> {
        self.0
            .system()
            .place_delete(name)
            .map_err(|e| format!("{e:?}"))
    }
}

/// The backup's folder in the person's account.
pub struct AccountPlace<C: PlaceCalls> {
    calls: C,
    staging: PathBuf,
    next: AtomicU64,
}

/// Whether `name` is one the folder takes: `[a-z0-9-]`, 1 to 128 bytes.
fn valid_name(name: &str) -> bool {
    !name.is_empty() && valid_prefix(name)
}

fn valid_prefix(prefix: &str) -> bool {
    prefix.len() <= 128
        && prefix
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn checked(name: &str) -> Result<(), PlaceError> {
    if valid_name(name) {
        Ok(())
    } else {
        Err(PlaceError::Failed(format!(
            "{name:?} is not a name a place takes"
        )))
    }
}

/// What the phone's answer means for the backup.
fn answered(answer: Result<PlaceAnswer, String>) -> Result<PlaceAnswer, PlaceError> {
    let answer = answer.map_err(PlaceError::Failed)?;
    let why = || answer.message.clone().unwrap_or_default();
    match answer.outcome {
        PlaceOutcome::Ok => Ok(answer),
        PlaceOutcome::NoRoom => Err(PlaceError::NoRoom),
        PlaceOutcome::SignedOut => Err(PlaceError::SignedOut),
        PlaceOutcome::NoPermission => Err(PlaceError::NoPermission),
        PlaceOutcome::Offline => Err(PlaceError::Offline),
        PlaceOutcome::Failed => Err(PlaceError::Failed(why())),
    }
}

fn io(e: std::io::Error) -> PlaceError {
    if e.kind() == std::io::ErrorKind::StorageFull {
        PlaceError::Failed("this phone is full".into())
    } else {
        PlaceError::Failed(e.to_string())
    }
}

/// A staged file, removed when dropped.
struct Staged(PathBuf);

impl Drop for Staged {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

impl<C: PlaceCalls> AccountPlace<C> {
    /// A place whose bytes pass through `staging`, a folder of the app's own that no backup takes.
    /// What an earlier run left there goes.
    pub fn new(calls: C, staging: PathBuf) -> Self {
        let _ = std::fs::remove_dir_all(&staging);
        AccountPlace {
            calls,
            staging,
            next: AtomicU64::new(0),
        }
    }

    fn stage(&self, name: &str) -> Result<Staged, PlaceError> {
        std::fs::create_dir_all(&self.staging).map_err(io)?;
        let n = self.next.fetch_add(1, Ordering::Relaxed);
        Ok(Staged(self.staging.join(format!("{name}.{n}"))))
    }
}

impl<C: PlaceCalls> BackupPlace for AccountPlace<C> {
    fn put(&self, name: &str, bytes: &[u8]) -> Result<(), PlaceError> {
        checked(name)?;
        let staged = self.stage(name)?;
        crate::sync::ledger::write_private(&staged.0, bytes).map_err(io)?;
        answered(self.calls.put(name, &staged.0)).map(drop)
    }

    fn get(&self, name: &str) -> Result<Option<Vec<u8>>, PlaceError> {
        checked(name)?;
        let staged = self.stage(name)?;
        if !answered(self.calls.get(name, &staged.0))?.found {
            return Ok(None);
        }
        std::fs::read(&staged.0).map(Some).map_err(io)
    }

    fn list(&self, prefix: &str) -> Result<Vec<PlaceEntry>, PlaceError> {
        if !valid_prefix(prefix) {
            return Err(PlaceError::Failed(format!(
                "{prefix:?} is not a prefix a place takes"
            )));
        }
        let answer = answered(self.calls.list(prefix))?;
        Ok(answer
            .entries
            .into_iter()
            .filter(|f| valid_name(&f.name) && f.name.starts_with(prefix))
            .map(|f| PlaceEntry {
                name: f.name,
                size: f.size,
                modified: f
                    .modified
                    .and_then(|ms| jiff::Timestamp::from_millisecond(ms).ok()),
            })
            .collect())
    }

    fn delete(&self, name: &str) -> Result<(), PlaceError> {
        checked(name)?;
        answered(self.calls.delete(name)).map(drop)
    }
}

/// The account for the backup on this phone, set at start. A computer has none.
pub fn attach<R: Runtime>(app: &AppHandle<R>, backup: &PhoneBackup) {
    #[cfg(mobile)]
    match tauri::Manager::path(app).app_cache_dir() {
        Ok(cache) => backup.set_place(Some(std::sync::Arc::new(AccountPlace::new(
            SystemCalls(app.clone()),
            cache.join(STAGING),
        )))),
        Err(e) => tracing::warn!("the backup has no staging folder: {e}"),
    }
    // An iPhone can say which iCloud it is signed in to at any time: another one signed in is
    // another account, as choosing another Google account is on Android.
    #[cfg(target_os = "ios")]
    {
        let handle = app.clone();
        backup.set_account_probe(Some(std::sync::Arc::new(move || {
            handle.system().place_account().ok().flatten()
        })));
    }
    #[cfg(not(mobile))]
    let _ = (app, backup, STAGING);
}

/// What came of asking for the account, as the screens show it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "camelCase")]
pub enum Connection {
    /// The account can be reached. On Android, `previous` is the account the backup used
    /// before, when the person picked another, and `free` the room left in the Drive.
    Connected {
        account: Option<String>,
        previous: Option<String>,
        free: Option<u64>,
    },
    Refused,
    NoAccount,
    TurnedOff,
    Offline,
    Failed,
}

impl From<Connected> for Connection {
    fn from(c: Connected) -> Self {
        match c {
            Connected::Connected {
                account,
                previous,
                free,
            } => Connection::Connected {
                account,
                previous,
                free,
            },
            Connected::Refused => Connection::Refused,
            Connected::NoAccount => Connection::NoAccount,
            Connected::TurnedOff => Connection::TurnedOff,
            Connected::Offline => Connection::Offline,
            Connected::Failed => Connection::Failed,
        }
    }
}

/// Step 1's "Continue", "Choose account" and "Try another account": on Android, Google's account
/// picker and its one-time permission (with `choose`, the picker always); on an iPhone, whether
/// its iCloud can be reached. `unavailable` on a computer. Once connected, a backup stopped for
/// being signed out or for permission carries on, and one moved to another account starts
/// afresh there ([`PhoneBackup::connect`]).
#[tauri::command]
pub async fn backup_connect<R: Runtime>(
    app: AppHandle<R>,
    choose: Option<bool>,
) -> Result<Connection, BackupProblem> {
    let choose = choose.unwrap_or(false);
    tauri::async_runtime::spawn_blocking(move || {
        let backup = tauri::Manager::state::<PhoneBackup>(&app);
        match backup.connect(|| app.system().place_connect(choose)) {
            Ok(Some(c)) => Ok(c.into()),
            Ok(None) => Err(BackupProblem::Unavailable),
            Err(e) => {
                tracing::warn!("the account could not be asked for: {e:?}");
                Ok(Connection::Failed)
            }
        }
    })
    .await
    .unwrap_or(Err(BackupProblem::Failed))
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use tauri_plugin_nocturne_system::PlaceFile;

    use super::*;

    /// The phone's side, answering what it is told to and keeping files as the account would.
    #[derive(Default)]
    struct Fake {
        files: Mutex<std::collections::BTreeMap<String, Vec<u8>>>,
        answer: Mutex<Option<Result<PlaceAnswer, String>>>,
        calls: Mutex<Vec<String>>,
        staged: Mutex<Vec<PathBuf>>,
    }

    fn ok() -> PlaceAnswer {
        PlaceAnswer {
            outcome: PlaceOutcome::Ok,
            message: None,
            found: false,
            entries: vec![],
        }
    }

    impl Fake {
        fn told(&self, call: String) -> Option<Result<PlaceAnswer, String>> {
            self.calls.lock().unwrap().push(call);
            self.answer.lock().unwrap().take()
        }
    }

    impl PlaceCalls for &Fake {
        fn put(&self, name: &str, path: &Path) -> Result<PlaceAnswer, String> {
            self.staged.lock().unwrap().push(path.to_path_buf());
            if let Some(a) = self.told(format!("put {name}")) {
                return a;
            }
            let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
            self.files.lock().unwrap().insert(name.into(), bytes);
            Ok(ok())
        }

        fn get(&self, name: &str, path: &Path) -> Result<PlaceAnswer, String> {
            self.staged.lock().unwrap().push(path.to_path_buf());
            if let Some(a) = self.told(format!("get {name}")) {
                return a;
            }
            let found = match self.files.lock().unwrap().get(name) {
                Some(b) => {
                    std::fs::write(path, b).unwrap();
                    true
                }
                None => false,
            };
            Ok(PlaceAnswer { found, ..ok() })
        }

        fn list(&self, prefix: &str) -> Result<PlaceAnswer, String> {
            if let Some(a) = self.told(format!("list {prefix}")) {
                return a;
            }
            let entries = self
                .files
                .lock()
                .unwrap()
                .iter()
                .filter(|(n, _)| n.starts_with(prefix))
                .map(|(n, b)| PlaceFile {
                    name: n.clone(),
                    size: b.len() as u64,
                    modified: Some(1_763_800_000_000),
                })
                .collect();
            Ok(PlaceAnswer { entries, ..ok() })
        }

        fn delete(&self, name: &str) -> Result<PlaceAnswer, String> {
            if let Some(a) = self.told(format!("delete {name}")) {
                return a;
            }
            self.files.lock().unwrap().remove(name);
            Ok(ok())
        }
    }

    fn place(fake: &Fake) -> (tempfile::TempDir, AccountPlace<&Fake>) {
        let dir = tempfile::tempdir().unwrap();
        let place = AccountPlace::new(fake, dir.path().join(STAGING));
        (dir, place)
    }

    #[test]
    fn bytes_go_through_the_phone_and_leave_no_staged_copy() {
        let fake = Fake::default();
        let (_dir, place) = place(&fake);
        place.put("piece-a1", b"locked").unwrap();
        assert_eq!(
            place.get("piece-a1").unwrap().as_deref(),
            Some(&b"locked"[..])
        );
        assert_eq!(place.get("piece-none").unwrap(), None);
        place.delete("piece-a1").unwrap();
        assert_eq!(place.get("piece-a1").unwrap(), None);
        let staged = fake.staged.lock().unwrap();
        assert!(staged.len() >= 3);
        assert!(staged.iter().all(|p| !p.exists()), "{staged:?}");
        let names: std::collections::HashSet<_> = staged.iter().collect();
        assert_eq!(
            names.len(),
            staged.len(),
            "each call has its own staged file"
        );
    }

    #[test]
    fn a_listing_keeps_the_prefix_sizes_and_times() {
        let fake = Fake::default();
        let (_dir, place) = place(&fake);
        place.put("night-1-ab", b"12345").unwrap();
        place.put("piece-ff", b"1").unwrap();
        let nights = place.list("night-").unwrap();
        assert_eq!(nights.len(), 1);
        assert_eq!(nights[0].name, "night-1-ab");
        assert_eq!(nights[0].size, 5);
        assert_eq!(
            nights[0].modified,
            Some(jiff::Timestamp::from_millisecond(1_763_800_000_000).unwrap())
        );
        assert_eq!(place.list("").unwrap().len(), 2);
    }

    #[test]
    fn a_listing_drops_what_the_phone_should_not_have_sent() {
        let fake = Fake::default();
        let (_dir, place) = place(&fake);
        let file = |name: &str| PlaceFile {
            name: name.into(),
            size: 1,
            modified: None,
        };
        *fake.answer.lock().unwrap() = Some(Ok(PlaceAnswer {
            entries: vec![
                file("night-2-ab"),
                file("piece-aa"),
                file("Night-3"),
                file(""),
            ],
            ..ok()
        }));
        let listed = place.list("night-").unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].modified, None);
    }

    #[test]
    fn names_a_place_does_not_take_never_reach_the_phone() {
        let fake = Fake::default();
        let (_dir, place) = place(&fake);
        let long = "a".repeat(129);
        for bad in [
            "",
            "Piece",
            "../key",
            "a/b",
            "a b",
            "piece_1",
            long.as_str(),
        ] {
            assert!(
                matches!(place.put(bad, b"x"), Err(PlaceError::Failed(_))),
                "{bad}"
            );
            assert!(
                matches!(place.get(bad), Err(PlaceError::Failed(_))),
                "{bad}"
            );
            assert!(
                matches!(place.delete(bad), Err(PlaceError::Failed(_))),
                "{bad}"
            );
        }
        assert!(matches!(place.list("night_"), Err(PlaceError::Failed(_))));
        assert!(fake.calls.lock().unwrap().is_empty());
        assert!(place.put(&"a".repeat(128), b"x").is_ok());
    }

    #[test]
    fn each_answer_from_the_phone_is_the_error_the_backup_shows() {
        let fake = Fake::default();
        let (_dir, place) = place(&fake);
        let failing = |outcome: PlaceOutcome, message: Option<&str>| {
            *fake.answer.lock().unwrap() = Some(Ok(PlaceAnswer {
                outcome,
                message: message.map(String::from),
                ..ok()
            }));
        };
        failing(PlaceOutcome::NoRoom, None);
        assert_eq!(place.put("piece-aa", b"x"), Err(PlaceError::NoRoom));
        failing(PlaceOutcome::SignedOut, None);
        assert_eq!(place.get("key-phrase"), Err(PlaceError::SignedOut));
        failing(PlaceOutcome::NoPermission, None);
        assert_eq!(place.list("piece-"), Err(PlaceError::NoPermission));
        failing(PlaceOutcome::Offline, None);
        assert_eq!(place.list("night-"), Err(PlaceError::Offline));
        failing(PlaceOutcome::Failed, Some("refused"));
        assert_eq!(
            place.delete("piece-aa"),
            Err(PlaceError::Failed("refused".into()))
        );
        *fake.answer.lock().unwrap() = Some(Err("no plugin".into()));
        assert_eq!(
            place.put("piece-aa", b"x"),
            Err(PlaceError::Failed("no plugin".into()))
        );
    }

    #[test]
    fn what_an_earlier_run_staged_goes_at_start() {
        let fake = Fake::default();
        let dir = tempfile::tempdir().unwrap();
        let staging = dir.path().join(STAGING);
        std::fs::create_dir_all(&staging).unwrap();
        std::fs::write(staging.join("piece-aa.0"), b"left").unwrap();
        let _place = AccountPlace::new(&fake, staging.clone());
        assert!(!staging.join("piece-aa.0").exists());
    }

    #[test]
    fn a_connection_reads_as_the_screens_name_it() {
        let shown = |c: Connected| serde_json::to_value(Connection::from(c)).unwrap();
        assert_eq!(
            shown(Connected::Connected {
                account: Some("a@example.org".into()),
                previous: Some("b@example.org".into()),
                free: Some(600),
            }),
            serde_json::json!({
                "outcome": "connected",
                "account": "a@example.org",
                "previous": "b@example.org",
                "free": 600
            })
        );
        assert_eq!(
            shown(Connected::NoAccount),
            serde_json::json!({ "outcome": "noAccount" })
        );
        assert_eq!(
            shown(Connected::TurnedOff),
            serde_json::json!({ "outcome": "turnedOff" })
        );
        assert_eq!(
            shown(Connected::Refused),
            serde_json::json!({ "outcome": "refused" })
        );
    }
}
