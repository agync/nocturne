//! Passphrase backups: making one, checking one, restoring one, and the copies put aside. A
//! passphrase only travels into the engine call: it is never logged, stored or remembered beside
//! a path.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use nocturne_contract::{
    BackupPhase, BackupProgress, BackupReport, BackupsView, EVENT_BACKUP, Holdings, RestoreCheck,
    RestoredView,
};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use super::{engine, run};
use crate::error::{CommandError, ErrorCode, blocking, blocking_cmd};
use crate::restore::{self, Aside, Source};
use crate::state::AppState;

/// Progress reaches the window at most this often; a new phase, the first and the last always do.
const PROGRESS_EVERY: Duration = Duration::from_millis(100);
/// A restore answer should reach the window before the app restarts.
const RESTART_DELAY: Duration = Duration::from_millis(400);

/// Releases the one backup job however the command ends.
struct Job<'a>(&'a AppState);

impl Drop for Job<'_> {
    fn drop(&mut self) {
        self.0.end_backup();
    }
}

/// A progress callback for the engine: at most one event every [`PROGRESS_EVERY`], always when
/// the phase changes, and always the first and the last call.
fn progress<R: Runtime>(app: AppHandle<R>) -> impl Fn(BackupPhase, u64, u64) + Send + Sync {
    let last = Mutex::new(None::<(Instant, BackupPhase)>);
    move |phase, done, total| {
        let value = BackupProgress {
            phase,
            done: done as f64,
            total: total as f64,
        };
        app.state::<AppState>().set_backup_progress(Some(value));
        let now = Instant::now();
        let mut last = last.lock().unwrap_or_else(|e| e.into_inner());
        let due = match *last {
            None => true,
            Some((at, was)) => was != phase || now.duration_since(at) >= PROGRESS_EVERY,
        };
        if due || done >= total {
            *last = Some((now, phase));
            if let Err(e) = app.emit(EVENT_BACKUP, value) {
                tracing::warn!("could not emit backup progress: {e}");
            }
        }
    }
}

/// The stamp that names a copy put aside: a Windows-legal folder name from the local clock.
fn stamp(state: &AppState) -> String {
    state
        .now()
        .to_zoned(state.clock.tz())
        .strftime("%Y-%m-%d %H%M%S")
        .to_string()
}

fn as_holdings(here: Option<&Holdings>) -> Option<Aside> {
    here.map(|h| Aside {
        at: String::new(),
        memories: h.memories,
        pages: h.pages,
        originals: h.originals,
        bytes: h.bytes,
    })
}

/// Restart the app soon, so the answer reaches the window first.
fn restart_soon<R: Runtime>(app: &AppHandle<R>) {
    let state = app.state::<AppState>();
    if let Some(restart) = state.restarter() {
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(RESTART_DELAY).await;
            restart();
        });
    }
}

fn busy() -> CommandError {
    CommandError::new(ErrorCode::Busy, "a backup is already running")
}

/// What this install holds now, with a store that cannot be opened treated as holding something.
async fn here_now<R: Runtime>(app: &AppHandle<R>) -> Result<Option<Holdings>, CommandError> {
    match app.state::<AppState>().engine() {
        Ok(e) => blocking(move || e.holdings(None)).await,
        Err(_) => Ok(Some(Holdings::default())),
    }
}

/// Write an encrypted backup to the file the save dialog chose.
#[tauri::command]
pub async fn make_backup<R: Runtime>(
    app: AppHandle<R>,
    path: String,
    passphrase: String,
) -> Result<BackupReport, CommandError> {
    run(app, move |app| async move {
        let dest = PathBuf::from(&path);
        let state = app.state::<AppState>();
        if !state.save_target_allowed(&dest) {
            return Err(CommandError::invalid(
                "choose where to save the backup first",
            ));
        }
        let parent = dest
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        if !parent.is_dir() {
            return Err(CommandError::invalid("choose an existing folder"));
        }
        if passphrase.chars().count() < nocturne_engine::MIN_PASSPHRASE_CHARS {
            return Err(CommandError::invalid(format!(
                "use at least {} characters",
                nocturne_engine::MIN_PASSPHRASE_CHARS
            )));
        }
        let parent = parent.to_path_buf();
        let e = engine(&app)?;
        let cancel = state.begin_backup()?;
        let _job = Job(&state);

        let data = state.data_dir.clone();
        let selection = crate::models::config_dir(&data).join("selection.json");
        let started_data = data.clone();
        let started = move |partial: &Path| {
            if let Err(e) = restore::write_job(&started_data, partial) {
                tracing::warn!("could not record the backup in progress: {e}");
            }
        };
        let handle = app.clone();
        let target = dest.clone();
        let work_factor = state.backup_work_factor();
        let result = blocking(move || {
            restore::clear_interrupted_backup(&data);
            let extras: [(&str, &Path); 1] = [("engine/selection.json", selection.as_path())];
            let progress = progress(handle);
            let run = nocturne_engine::BackupRun {
                extras: &extras,
                progress: &progress,
                cancel: &cancel,
                started: &started,
                work_factor,
            };
            e.backup_to(&target, &passphrase, &run)
        })
        .await;

        let data = state.data_dir.clone();
        match &result {
            Ok(report) => {
                // A finished save cannot be replayed without the dialog; a failed one may be
                // retried on the same path.
                state.take_save_target(&dest);
                // "Show in folder" opens the folder the backup was saved in.
                state.allow_export(&parent);
                let at = state.now().to_string();
                let bytes = report.bytes;
                let _ = blocking_cmd(move || {
                    restore::clear_job(&data);
                    let _ = restore::write_last(&data, &at, bytes);
                    Ok(())
                })
                .await;
            }
            Err(_) => {
                let _ = blocking_cmd(move || {
                    restore::clear_job(&data);
                    Ok(())
                })
                .await;
            }
        }
        result
    })
    .await
}

/// Whether a picked file is a Nocturne backup at all, before any passphrase is asked for.
#[tauri::command]
pub async fn probe_backup<R: Runtime>(app: AppHandle<R>, path: String) -> Result<(), CommandError> {
    run(app, move |app| async move {
        let file = PathBuf::from(&path);
        if !app.state::<AppState>().restore_source_allowed(&file) {
            return Err(CommandError::invalid("choose the backup file first"));
        }
        blocking(move || nocturne_engine::probe_backup(&file)).await
    })
    .await
}

/// The bytes a backup would roughly take.
#[tauri::command]
pub async fn estimate_backup<R: Runtime>(app: AppHandle<R>) -> Result<f64, CommandError> {
    run(app, move |app| async move {
        let e = engine(&app)?;
        blocking(move || e.backup_estimate().map(|bytes| bytes as f64)).await
    })
    .await
}

/// Check a backup file in full and say what restoring it would do here.
#[tauri::command]
pub async fn check_backup<R: Runtime>(
    app: AppHandle<R>,
    path: String,
    passphrase: String,
) -> Result<RestoreCheck, CommandError> {
    run(app, move |app| async move {
        let file = PathBuf::from(&path);
        let state = app.state::<AppState>();
        if !state.restore_source_allowed(&file) {
            return Err(CommandError::invalid("choose the backup file first"));
        }
        let cancel = state.begin_backup()?;
        let _job = Job(&state);
        state.set_checked_backup(None);

        let data = state.data_dir.clone();
        let scratch = restore::check_dir(&data);
        let restore_dir = restore::restore_dir(&data);
        blocking_cmd(move || {
            restore::clear_check(&data);
            crate::fsutil::create_private_dir(&restore_dir).map_err(CommandError::internal)?;
            Ok(())
        })
        .await?;

        let handle = app.clone();
        let asked = file.clone();
        let checked = blocking(move || {
            nocturne_engine::check_backup(&asked, &passphrase, &scratch, &progress(handle), &cancel)
        })
        .await;
        let summary = match checked {
            Ok(summary) => summary,
            Err(e) => {
                let data = state.data_dir.clone();
                let _ = blocking_cmd(move || {
                    restore::clear_check(&data);
                    Ok(())
                })
                .await;
                return Err(e);
            }
        };

        let data = state.data_dir.clone();
        let _ = blocking_cmd(move || {
            restore::clear_interrupted_restore(&data);
            Ok(())
        })
        .await;
        let here = match state.engine() {
            Ok(e) => {
                let made_at = summary.made_at.clone();
                blocking(move || e.holdings(Some(&made_at))).await?
            }
            // The safe answer keeps whatever is in a store that could not be opened.
            Err(_) => Some(Holdings::default()),
        };
        let data = state.data_dir.clone();
        let put_aside = blocking_cmd(move || Ok(restore::newest_put_aside(&data))).await?;
        state.set_checked_backup(Some((file, summary.clone())));
        Ok(RestoreCheck {
            summary,
            here,
            put_aside,
        })
    })
    .await
}

/// Unpack the checked backup beside the store and schedule the switch for the next start.
#[tauri::command]
pub async fn restore_backup<R: Runtime>(
    app: AppHandle<R>,
    path: String,
    passphrase: String,
    replace: bool,
) -> Result<(), CommandError> {
    run(app, move |app| async move {
        let file = PathBuf::from(&path);
        let state = app.state::<AppState>();
        let Some((checked, checked_summary)) = state.checked_backup() else {
            return Err(CommandError::invalid("open a backup first"));
        };
        if checked != file {
            return Err(CommandError::invalid(
                "that is not the backup that was checked",
            ));
        }
        if !file.is_file() {
            return Err(CommandError::not_found("the backup file went away"));
        }
        if !state.restore_source_allowed(&file) {
            return Err(CommandError::invalid("choose the backup file first"));
        }
        if !state.backup_idle() {
            return Err(busy());
        }
        let here = here_now(&app).await?;
        if here.is_some() && !replace {
            return Err(CommandError::invalid(
                "this install already holds things of yours",
            ));
        }

        let cancel = state.begin_backup()?;
        let _job = Job(&state);
        let data = state.data_dir.clone();
        let stage = restore::stage_dir(&data);
        let restore_dir = restore::restore_dir(&data);
        blocking_cmd(move || {
            // A new restore takes the place of one scheduled earlier: its marker goes before its
            // stage, so a start never reports a restore whose files are gone.
            match std::fs::remove_file(restore::marker_path(&data)) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(CommandError::internal(e)),
            }
            restore::clear_stage(&data);
            crate::fsutil::create_private_dir(&restore_dir).map_err(CommandError::internal)?;
            Ok(())
        })
        .await?;

        let handle = app.clone();
        let unpacked = blocking(move || {
            nocturne_engine::unpack_backup(&file, &passphrase, &stage, &progress(handle), &cancel)
        })
        .await;
        // The file is read again here: one changed since the check is not what the person saw.
        let unpacked = unpacked.and_then(|summary| {
            if summary == checked_summary {
                Ok(summary)
            } else {
                Err(CommandError::new(
                    ErrorCode::DamagedBackup,
                    "the backup changed after it was checked",
                ))
            }
        });
        let summary = match unpacked {
            Ok(summary) => summary,
            Err(e) => {
                let data = state.data_dir.clone();
                let _ = blocking_cmd(move || {
                    restore::clear_stage(&data);
                    Ok(())
                })
                .await;
                return Err(e);
            }
        };

        // The reading setup must be one this app can use, and must not carry a download from the
        // machine that made the backup.
        let data = state.data_dir.clone();
        let selection = restore::stage_dir(&data)
            .join("engine")
            .join("selection.json");
        let present = blocking_cmd(move || Ok(selection.is_file())).await?;
        if present {
            let data = state.data_dir.clone();
            let read = blocking_cmd(move || {
                let path = restore::stage_dir(&data)
                    .join("engine")
                    .join("selection.json");
                Ok(restore::sanitised_selection_file(&path))
            })
            .await?;
            let Some(bytes) = read else {
                let data = state.data_dir.clone();
                let _ = blocking_cmd(move || {
                    restore::clear_stage(&data);
                    Ok(())
                })
                .await;
                return Err(CommandError::new(
                    ErrorCode::DamagedBackup,
                    "the reading setup in this backup is not valid",
                ));
            };
            let data = state.data_dir.clone();
            blocking_cmd(move || {
                let path = restore::stage_dir(&data)
                    .join("engine")
                    .join("selection.json");
                crate::fsutil::write_private(&path, &bytes).map_err(CommandError::internal)
            })
            .await?;
        }

        // Unpacking can take minutes: what is here now may not be what the check saw.
        let here = here_now(&app).await?;
        let replace = replace || here.is_some();

        // The last step cannot be stopped any more.
        state.commit_backup();
        state.asks.cancel_all();
        let data = state.data_dir.clone();
        let when = stamp(&state);
        let mut aside = as_holdings(here.as_ref());
        if let Some(aside) = aside.as_mut() {
            aside.at = when.clone();
        }
        blocking_cmd(move || {
            restore::schedule(&data, replace, &when, Source::Stage, aside, Some(summary))
                .map_err(CommandError::internal)
        })
        .await?;
        restart_soon(&app);
        Ok(())
    })
    .await
}

/// Bring back one of the copies put aside: what is here now is put aside in turn.
#[tauri::command]
pub async fn restore_put_aside<R: Runtime>(
    app: AppHandle<R>,
    id: String,
) -> Result<(), CommandError> {
    run(app, move |app| async move {
        let state = app.state::<AppState>();
        if !state.backup_idle() {
            return Err(busy());
        }
        let Some(path) = restore::put_aside_path(&state.data_dir, &id) else {
            return Err(CommandError::invalid("that is not a copy's name"));
        };
        let present = blocking_cmd(move || Ok(path.join("store").is_dir())).await?;
        if !present {
            return Err(CommandError::not_found("that copy is not there"));
        }
        state.asks.cancel_all();
        let here = here_now(&app).await?;
        let data = state.data_dir.clone();
        let when = stamp(&state);
        let mut aside = as_holdings(here.as_ref());
        if let Some(aside) = aside.as_mut() {
            aside.at = when.clone();
        }
        blocking_cmd(move || {
            restore::schedule(
                &data,
                here.is_some(),
                &when,
                Source::PutAside(id),
                aside,
                None,
            )
            .map_err(CommandError::internal)
        })
        .await?;
        restart_soon(&app);
        Ok(())
    })
    .await
}

/// Delete one copy put aside, after checking its name.
#[tauri::command]
pub async fn delete_put_aside<R: Runtime>(
    app: AppHandle<R>,
    id: String,
) -> Result<(), CommandError> {
    run(app, move |app| async move {
        let data = app.state::<AppState>().data_dir.clone();
        blocking_cmd(move || {
            restore::delete_put_aside(&data, &id).map_err(|e| match e.kind() {
                std::io::ErrorKind::NotFound => CommandError::not_found(e),
                std::io::ErrorKind::InvalidInput => CommandError::invalid(e),
                _ => CommandError::internal(e),
            })
        })
        .await?;
        Ok(())
    })
    .await
}

/// What the backup screen shows: the last backup, the copies put aside and what is running.
#[tauri::command]
pub async fn get_backups<R: Runtime>(app: AppHandle<R>) -> Result<BackupsView, CommandError> {
    run(app, move |app| async move {
        let state = app.state::<AppState>();
        let running = state.backup_progress();
        let data = state.data_dir.clone();
        let mut view = blocking_cmd(move || {
            Ok(BackupsView {
                last: restore::last_backup(&data),
                put_aside: restore::put_aside(&data),
                running: None,
                interrupted_backup: restore::interrupted_backup(&data),
                interrupted_restore: restore::interrupted_restore(&data),
            })
        })
        .await?;
        view.running = running;
        Ok(view)
    })
    .await
}

/// Clear the two notices about work that stopped unexpectedly.
#[tauri::command]
pub async fn dismiss_backup_notices<R: Runtime>(app: AppHandle<R>) -> Result<(), CommandError> {
    run(app, move |app| async move {
        let data = app.state::<AppState>().data_dir.clone();
        blocking_cmd(move || {
            restore::dismiss_notices(&data);
            Ok(())
        })
        .await?;
        Ok(())
    })
    .await
}

/// What the last restore brought in, until the screen has shown it.
#[tauri::command]
pub async fn get_restored<R: Runtime>(
    app: AppHandle<R>,
) -> Result<Option<RestoredView>, CommandError> {
    run(app, move |app| async move {
        let data = app.state::<AppState>().data_dir.clone();
        blocking_cmd(move || Ok(restore::restored(&data))).await
    })
    .await
}

/// Forget what the last restore brought in.
#[tauri::command]
pub async fn finish_restore<R: Runtime>(app: AppHandle<R>) -> Result<(), CommandError> {
    run(app, move |app| async move {
        let data = app.state::<AppState>().data_dir.clone();
        blocking_cmd(move || {
            restore::finish_restore(&data);
            Ok(())
        })
        .await?;
        Ok(())
    })
    .await
}

/// Stop the running job. Harmless when nothing runs, and nothing once a restore is committed.
#[tauri::command]
pub async fn cancel_backup<R: Runtime>(app: AppHandle<R>) -> Result<(), CommandError> {
    run(app, move |app| async move {
        app.state::<AppState>().cancel_backup();
        Ok(())
    })
    .await
}

/// Throw away a checked backup and anything staged for it.
#[tauri::command]
pub async fn discard_restore<R: Runtime>(app: AppHandle<R>) -> Result<(), CommandError> {
    run(app, move |app| async move {
        let state = app.state::<AppState>();
        if !state.backup_idle() {
            return Err(busy());
        }
        let data = state.data_dir.clone();
        // Once a restore is scheduled its staged files are the next start's to use.
        if restore::marker_path(&data).exists() {
            return Err(busy());
        }
        state.set_checked_backup(None);
        blocking_cmd(move || {
            restore::clear_staging(&data);
            Ok(())
        })
        .await?;
        Ok(())
    })
    .await
}
