//! The screens' way to the phone's own memory. Every call runs the engine on a blocking worker.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use nocturne_contract::{
    BackupPhase, BackupProgress, EVENT_BACKUP, HomeAnswer, HomeCall, HomeError, PhoneError,
    PhoneMode, RemoteAnswer, RemoteCall,
};
use tauri::ipc::Response;
use tauri::{AppHandle, Emitter, Manager, Runtime, State};

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
        match home.open_store() {
            Ok(open) => dispatch::answer(&open, call),
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
    blocking(move || {
        let home = worker.state::<Home>();
        match home.open_store() {
            Ok(open) => dispatch::keep_bytes(&open, call, &bytes, &home.keeping_dir()),
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

/// An item's original, as `[type length][type][bytes]`, as the companion's `original` answers.
#[tauri::command]
pub async fn home_original<R: Runtime>(
    app: AppHandle<R>,
    id: String,
) -> Result<Response, PhoneError> {
    let (mime, bytes) = blocking(move || {
        let home = app.state::<Home>();
        let open = home.open_store().map_err(|_| PhoneError::NotReady)?;
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
