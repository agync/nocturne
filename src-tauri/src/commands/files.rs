//! Choosing and opening files and folders. The interface has no dialog or opener permission of
//! its own: it asks here, and these commands only reach what the person chose or what Nocturne
//! itself made.

use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_dialog::DialogExt;

use super::{read, run};
use crate::error::{CommandError, blocking_cmd};
use crate::state::AppState;

fn text(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

/// Ask the person for a folder; `null` when they cancel.
#[tauri::command]
pub async fn pick_folder<R: Runtime>(app: AppHandle<R>) -> Result<Option<String>, CommandError> {
    run(app, |app| async move {
        let dialog = app.clone();
        let picked =
            blocking_cmd(move || Ok(dialog.dialog().file().blocking_pick_folder())).await?;
        Ok(picked.and_then(|p| p.into_path().ok()).map(|p| text(&p)))
    })
    .await
}

/// Ask the person for files. What they pick may then be kept by path.
#[tauri::command]
pub async fn pick_files<R: Runtime>(app: AppHandle<R>) -> Result<Vec<String>, CommandError> {
    run(app, |app| async move {
        let dialog = app.clone();
        let picked = blocking_cmd(move || Ok(dialog.dialog().file().blocking_pick_files())).await?;
        let paths: Vec<PathBuf> = picked
            .unwrap_or_default()
            .into_iter()
            .filter_map(|p| p.into_path().ok())
            .collect();
        app.state::<AppState>().allow_paths(paths.iter().cloned());
        Ok(paths.iter().map(|p| text(p)).collect())
    })
    .await
}

/// Open a path with the system's own app, through the hook the app installed.
fn open(state: &AppState, path: &Path) -> Result<(), CommandError> {
    match state.opener() {
        Some(open) => open(path).map_err(|e| {
            tracing::warn!("a path could not be opened");
            CommandError::unavailable(e)
        }),
        None => Err(CommandError::unavailable("nothing can be opened here")),
    }
}

/// Open the kept original of an item.
#[tauri::command]
pub async fn open_original<R: Runtime>(
    app: AppHandle<R>,
    item_id: String,
) -> Result<(), CommandError> {
    let item = read(app.clone(), move |e| e.get_item(&item_id)).await?;
    run(app, move |app| async move {
        let state = app.state::<AppState>();
        let path = item
            .original_path
            .ok_or_else(|| CommandError::not_found("this item has no original"))?;
        let originals = state.data_dir.join("store").join("originals");
        let inside = blocking_cmd(move || {
            Ok(dunce::canonicalize(&path)
                .ok()
                .filter(|p| dunce::canonicalize(&originals).is_ok_and(|o| p.starts_with(o))))
        })
        .await?
        .ok_or_else(|| CommandError::not_found("the original is not there"))?;
        open(&state, &inside)
    })
    .await
}

/// Open a folder an export made in this session.
#[tauri::command]
pub async fn open_export<R: Runtime>(app: AppHandle<R>, path: String) -> Result<(), CommandError> {
    run(app, move |app| async move {
        let state = app.state::<AppState>();
        let folder = PathBuf::from(path);
        if !state.export_allowed(&folder) {
            return Err(CommandError::invalid(
                "that folder was not made by an export",
            ));
        }
        open(&state, &folder)
    })
    .await
}

/// Open the folder of log files.
#[tauri::command]
pub async fn open_logs<R: Runtime>(app: AppHandle<R>) -> Result<(), CommandError> {
    run(app, move |app| async move {
        let state = app.state::<AppState>();
        let dir = state.log_dir();
        let exists = blocking_cmd(move || Ok(dir.is_dir())).await?;
        if !exists {
            return Err(CommandError::not_found("there are no logs yet"));
        }
        open(&state, &state.log_dir())
    })
    .await
}
