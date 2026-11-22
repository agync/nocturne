//! The screens' way to the phone's own memory. Every call runs the engine on a blocking worker.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use nocturne_contract::{
    BackupPhase, BackupProgress, EVENT_BACKUP, HomeAnswer, HomeCall, HomeError, PhoneError,
    PhoneMode, RemoteAnswer, RemoteCall,
};
use tauri::ipc::Response;
use tauri::{AppHandle, Emitter, Manager, Runtime, State};
use tauri_plugin_nocturne_speech::{Access, SpeechExt};

use super::voice::{Heard, Reader};
use super::{Home, dispatch};
use crate::state::AppState;

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
    emit_mode(&app);
    Ok(())
}

/// A screen call, answered from this phone's memory.
#[tauri::command]
pub async fn home_call<R: Runtime>(app: AppHandle<R>, call: RemoteCall) -> RemoteAnswer {
    blocking(move || {
        let home = app.state::<Home>();
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
    let changes_mode = matches!(call, HomeCall::RestoreBackup { .. });
    let changes_store = matches!(
        call,
        HomeCall::RestoreBackup { .. } | HomeCall::DeleteEverything { .. }
    );
    let worker = app.clone();
    let answer = blocking(move || {
        let progress = progress(worker.clone());
        worker.state::<Home>().local_call(call, paired, &progress)
    })
    .await
    .unwrap_or(HomeAnswer::Error {
        code: HomeError::Failed,
    });
    let done = matches!(answer, HomeAnswer::Restored | HomeAnswer::Deleted);
    if changes_store && done {
        // What was sent before went into the store that is gone: send it all again.
        app.state::<AppState>().syncer.ledger.forget_sends();
    }
    if changes_mode && done {
        emit_mode(&app);
        hear_soon(&app);
    }
    answer
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
