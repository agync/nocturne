//! The screens' way to the phone's own memory. Every call runs the engine on a blocking worker.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use nocturne_contract::{
    BackupPhase, BackupProgress, EVENT_BACKUP, HomeAnswer, HomeCall, HomeError, PhoneError,
    PhoneMode, PhoneSource, RemoteAnswer, RemoteCall,
};
use tauri::ipc::Response;
use tauri::{AppHandle, Emitter, Manager, Runtime, State};
use tauri_plugin_nocturne_speech::{Access, SpeechExt};
use tauri_plugin_nocturne_system::{Sheet, SystemExt};

use super::files::{ChosenBackup, INBOX, OUTBOX, Sheets};
use super::voice::{Heard, Reader};
use super::{Home, dispatch};
use crate::state::AppState;
use crate::sync::LedgerFile;

/// Sent with the new mode whenever where the memory lives changes.
pub const EVENT_MODE: &str = "mode_changed";

/// Backup progress reaches the screens at most this often; a new phase and the last always do.
const PROGRESS_EVERY: Duration = Duration::from_millis(100);

fn paired(app: &AppHandle<impl Runtime>) -> bool {
    app.state::<AppState>().store.identity().is_some()
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    tauri::async_runtime::spawn_blocking(f).await.ok()
}

fn emit_mode<R: Runtime>(app: &AppHandle<R>) {
    let mode = app.state::<Home>().mode(paired(app));
    if let Err(e) = app.emit(EVENT_MODE, mode) {
        tracing::warn!("could not say where the memory lives: {e}");
    }
    crate::sources::start(app);
}

/// Where the memory lives: `alone`, `companion`, or none before the person has chosen.
#[tauri::command]
pub fn mode<R: Runtime>(app: AppHandle<R>, home: State<'_, Home>) -> Option<PhoneMode> {
    home.mode(paired(&app))
}

/// The folder a damaged memory was put aside in, while the phone waits for a backup or a fresh
/// start; `""` when it could not be moved. None when the memory is fine.
#[tauri::command]
pub fn memory_damaged(home: State<'_, Home>) -> Option<String> {
    home.damaged()
}

/// "Start on this phone", with the phone's name for its own row.
#[tauri::command]
pub async fn start_alone<R: Runtime>(app: AppHandle<R>, name: String) -> Result<(), HomeError> {
    let paired = paired(&app);
    let handle = app.clone();
    blocking(move || handle.state::<Home>().start_alone(&name, paired))
        .await
        .unwrap_or(Err(HomeError::Failed))?;
    // What was sent before went into a store that is not this one.
    app.state::<AppState>().syncer.ledger.forget_sends();
    emit_mode(&app);
    Ok(())
}

/// A screen call, answered from this phone's memory.
#[tauri::command]
pub async fn home_call<R: Runtime>(app: AppHandle<R>, call: RemoteCall) -> RemoteAnswer {
    blocking(move || {
        let home = app.state::<Home>();
        // A question waits for the online mind, without the store held meanwhile.
        if let RemoteCall::Ask { question, .. } = &call {
            let rt = tokio::runtime::Handle::current();
            return app
                .state::<crate::mind::PhoneMind>()
                .answer_call(&home, &rt, question);
        }
        match home.using() {
            Ok((_use, open)) => dispatch::answer(&open, call),
            Err(_) => RemoteAnswer::Error {
                code: PhoneError::NotReady,
            },
        }
    })
    .await
    .unwrap_or(RemoteAnswer::Error {
        code: PhoneError::Busy,
    })
}

/// Every slice of a keep with bytes is in (`keep_begin`, `keep_part`): keep it here.
#[tauri::command]
pub async fn home_keep_end<R: Runtime>(app: AppHandle<R>, handle: u32) -> RemoteAnswer {
    let ended = app.state::<AppState>().uploads.lock().unwrap().end(handle);
    let Ok((call, bytes)) = ended else {
        return RemoteAnswer::Error {
            code: PhoneError::Invalid,
        };
    };
    let worker = app.clone();
    let answer = blocking(move || {
        let home = worker.state::<Home>();
        match home.using() {
            Ok((_use, open)) => dispatch::keep_bytes(&open, call, &bytes, &home.keeping_dir()),
            Err(_) => RemoteAnswer::Error {
                code: PhoneError::NotReady,
            },
        }
    })
    .await
    .unwrap_or(RemoteAnswer::Error {
        code: PhoneError::Busy,
    });
    if dispatch::is_voice_note(&answer) {
        hear_soon(&app);
    }
    answer
}

/// An item's original, as `[type length][type][bytes]`, as the companion's `original` answers.
#[tauri::command]
pub async fn home_original<R: Runtime>(
    app: AppHandle<R>,
    id: String,
) -> Result<Response, PhoneError> {
    let (mime, bytes) = blocking(move || {
        let home = app.state::<Home>();
        let (_use, open) = home.using().map_err(|_| PhoneError::NotReady)?;
        dispatch::original(&open, &id)
    })
    .await
    .unwrap_or(Err(PhoneError::Busy))?;
    Ok(Response::new(crate::commands::framed(&mime, &bytes)))
}

/// A home call. Backups report their progress as `backup-progress` events.
#[tauri::command]
pub async fn home_local<R: Runtime>(app: AppHandle<R>, call: HomeCall) -> HomeAnswer {
    let paired = paired(&app);
    let swaps = matches!(
        call,
        HomeCall::RestoreBackup { .. } | HomeCall::BringBack { .. }
    );
    let restores = matches!(call, HomeCall::RestoreBackup { .. });
    let deletes = matches!(call, HomeCall::DeleteEverything { .. });
    let source = match &call {
        HomeCall::DeleteSource {
            source, also_stop, ..
        } => Some((*source, *also_stop)),
        _ => None,
    };
    let worker = app.clone();
    let answer = blocking(move || {
        let progress = progress(worker.clone());
        let syncer = &worker.state::<AppState>().syncer;
        // What the phone brings and the store change together: no send runs in between.
        let _hold = (deletes || source.is_some()).then(|| syncer.hold());
        if let Some((source, true)) = source {
            // Stopped first, so nothing read meanwhile comes back after the delete.
            syncer.ledger.update(|l| match source {
                PhoneSource::Calendar => l.calendar.settings.bring = false,
                PhoneSource::Health => l.health.settings.bring = false,
            });
        }
        let answer = worker.state::<Home>().local_call(call, paired, &progress);
        match (&answer, source) {
            (HomeAnswer::Deleted, _) if deletes => {
                syncer.ledger.reset();
                // The mind's key, its choice and what it sent go in the same step: a new
                // start chooses again.
                worker.state::<crate::mind::PhoneMind>().erase();
            }
            (HomeAnswer::SourceDeleted, Some((source, also_stop))) => {
                forget_source_sends(&syncer.ledger, source, also_stop)
            }
            _ => {}
        }
        answer
    })
    .await
    .unwrap_or(HomeAnswer::Error {
        code: HomeError::Failed,
    });
    let ledger = &app.state::<AppState>().syncer.ledger;
    if matches!(answer, HomeAnswer::Restored) {
        if restores {
            let_go_backup_file(app.clone());
        }
        if swaps {
            // What was sent before went into the store that is gone: send it all again.
            ledger.forget_sends();
            emit_mode(&app);
            hear_soon(&app);
        }
    }
    if deletes && matches!(answer, HomeAnswer::Deleted) {
        // Nothing comes back until the person starts again and chooses what the phone brings.
        emit_mode(&app);
    }
    answer
}

/// After what a source brought was deleted: stop it, or have the calendar's next two weeks
/// brought again the next time it is read. Health readings are not brought again.
pub(crate) fn forget_source_sends(ledger: &LedgerFile, source: PhoneSource, also_stop: bool) {
    if source != PhoneSource::Calendar {
        if also_stop {
            ledger.update(|l| l.health.settings.bring = false);
        }
        return;
    }
    ledger.update(|l| {
        if also_stop {
            l.calendar.settings.bring = false;
        }
        l.calendar.sent.clear();
        l.calendar.pending.clear();
        l.calendar.calendars_sent = None;
    });
}

/// A progress callback for the engine: at most one event every [`PROGRESS_EVERY`], always when
/// the phase changes, and always the last.
fn progress<R: Runtime>(app: AppHandle<R>) -> impl Fn(BackupPhase, u64, u64) + Send + Sync {
    let last = Mutex::new(None::<(Instant, BackupPhase)>);
    move |phase, done, total| {
        let now = Instant::now();
        let mut last = last.lock().unwrap_or_else(|e| e.into_inner());
        let due = match *last {
            None => true,
            Some((at, was)) => was != phase || now.duration_since(at) >= PROGRESS_EVERY,
        };
        if due || done >= total {
            *last = Some((now, phase));
            let value = BackupProgress {
                phase,
                done: done as f64,
                total: total as f64,
            };
            let _ = app.emit(EVENT_BACKUP, value);
        }
    }
}

/// The phone's speech reader, through its plugin.
struct PhoneReader<R: Runtime>(AppHandle<R>);

impl<R: Runtime> Reader for PhoneReader<R> {
    fn ready(&self, language: &str) -> bool {
        let speech = self.0.speech();
        speech.access() == Access::Granted
            && speech
                .check_support(language)
                .is_ok_and(|outcome| !outcome.waits())
    }

    fn read(&self, pcm: &std::path::Path, language: &str) -> Heard {
        let Some(path) = pcm.to_str() else {
            return Heard::Waits;
        };
        match self.0.speech().transcribe(path, language) {
            Ok(outcome) => match outcome.words() {
                Some(words) => Heard::Words(words.to_string()),
                None if outcome.waits() => Heard::Waits,
                None => Heard::Nothing,
            },
            Err(_) => Heard::Waits,
        }
    }

    fn decode(&self, src: &std::path::Path, dest: &std::path::Path) -> Option<u32> {
        let (src, dest) = (src.to_str()?, dest.to_str()?);
        let ms = self.0.speech().decode(src, dest).ok().flatten()?;
        u32::try_from(ms).ok()
    }
}

/// Read waiting voice notes in the background, if a language is known.
fn hear_soon<R: Runtime>(app: &AppHandle<R>) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let heard = app.state::<Home>().hear_waiting(&PhoneReader(app.clone()));
        if heard > 0 {
            tracing::info!("{heard} voice notes have their words");
        }
    });
}

/// Fade and let go what has gone unused, in the background, when the app opens and each time it
/// comes back to the front. It runs at most once a day.
pub fn fade_soon<R: Runtime>(app: &AppHandle<R>) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        if let Some(done) = app.state::<Home>().fade_if_due()
            && (done.faded, done.let_go) != (0, 0)
        {
            tracing::info!("{} memories faded, {} let go", done.faded, done.let_go);
        }
    });
}

/// The phone's language, as the web view knows it (such as `en-GB`), for reading voice notes;
/// then every note still waiting is read where the phone can. Called when the app opens and
/// each time it comes back to the front. Never asks for the microphone.
#[tauri::command]
pub fn hear_voice_notes<R: Runtime>(app: AppHandle<R>, language: String) -> bool {
    let taken = app.state::<Home>().set_language(&language);
    if taken {
        hear_soon(&app);
    }
    taken
}

/// Whether the phone's speech reader may run: it asks for the microphone permission although it
/// only reads files.
#[tauri::command]
pub async fn voice_access<R: Runtime>(app: AppHandle<R>) -> Access {
    blocking(move || app.speech().access())
        .await
        .unwrap_or(Access::Denied)
}

/// The phone's own prompt for that permission, unless it will no longer show it. The screens
/// say why first.
#[tauri::command]
pub async fn voice_request<R: Runtime>(app: AppHandle<R>) -> Access {
    blocking(move || app.speech().request().unwrap_or(Access::Denied))
        .await
        .unwrap_or(Access::Denied)
}

/// A folder of the app's cache, for copies that never outlive their use.
fn cache_dir<R: Runtime>(app: &AppHandle<R>, name: &str) -> Option<std::path::PathBuf> {
    app.path().app_cache_dir().ok().map(|d| d.join(name))
}

/// Copies an earlier run left behind, when it stopped while a sheet was open.
pub fn clear_backup_copies<R: Runtime>(app: &AppHandle<R>) {
    for name in [OUTBOX, INBOX] {
        if let Some(dir) = cache_dir(app, name) {
            super::calls::clear(&dir);
        }
    }
}

/// The phone's sheets, through the system plugin.
struct PhoneSheets<R: Runtime>(AppHandle<R>);

impl<R: Runtime> Sheets for PhoneSheets<R> {
    fn save(&self, src: &std::path::Path, name: &str) -> Sheet {
        self.0.system().save_file(src, name).unwrap_or_else(|e| {
            tracing::warn!("the save sheet failed: {e:?}");
            Sheet::Failed
        })
    }

    fn choose(&self, dir: &std::path::Path) -> Sheet {
        self.0.system().choose_file(dir).unwrap_or_else(|e| {
            tracing::warn!("the open sheet failed: {e:?}");
            Sheet::Failed
        })
    }
}

/// "Save the backup…": the backup is made, with `backup-progress` events, then the phone's save
/// sheet lets the person choose where it goes. `None` when they closed the sheet: nothing was
/// saved and the last backup stays as it was.
#[tauri::command]
pub async fn save_backup_file<R: Runtime>(
    app: AppHandle<R>,
    passphrase: String,
) -> Option<HomeAnswer> {
    let Some(outbox) = cache_dir(&app, OUTBOX) else {
        return Some(HomeAnswer::Error {
            code: HomeError::Failed,
        });
    };
    blocking(move || {
        let progress = progress(app.clone());
        let sheets = PhoneSheets(app.clone());
        app.state::<Home>()
            .save_backup_file(&outbox, &passphrase, &sheets, &progress)
    })
    .await
    .unwrap_or(Some(HomeAnswer::Error {
        code: HomeError::Failed,
    }))
}

/// "Stop" on a backup, a check or a restore: it ends where it can be ended, and answers `stopped`.
#[tauri::command]
pub fn backup_stop<R: Runtime>(app: AppHandle<R>) {
    app.state::<Home>().stop_job();
}

/// "Choose the file": the phone's open sheet. The file is copied into the app's cache and
/// checked to be a Nocturne backup; its `file` then goes to `checkBackup` and `restoreBackup`.
/// `None` when the sheet was closed. `NotABackup` for any other file.
#[tauri::command]
pub async fn choose_backup_file<R: Runtime>(
    app: AppHandle<R>,
) -> Result<Option<ChosenBackup>, HomeError> {
    let inbox = cache_dir(&app, INBOX).ok_or(HomeError::Failed)?;
    blocking(move || {
        let sheets = PhoneSheets(app.clone());
        app.state::<Home>().choose_backup_file(&inbox, &sheets)
    })
    .await
    .unwrap_or(Err(HomeError::Failed))
}

/// The chosen backup's copy goes, when the person leaves the restore without bringing it back.
/// A restore that worked lets go of it itself.
#[tauri::command]
pub fn let_go_backup_file<R: Runtime>(app: AppHandle<R>) {
    if let Some(dir) = cache_dir(&app, INBOX) {
        super::calls::clear(&dir);
    }
}

/// The bytes free on the phone where the memory is, for the "nearly full" banner. `None` where
/// the phone does not say.
#[tauri::command]
pub async fn free_space<R: Runtime>(app: AppHandle<R>) -> Option<f64> {
    let dir = app.path().app_data_dir().ok()?;
    blocking(move || app.system().free_space(&dir).ok().flatten())
        .await
        .flatten()
        .map(|b| b as f64)
}
