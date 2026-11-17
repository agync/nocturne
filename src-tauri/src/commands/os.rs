//! Commands that reach the desktop outside the window.

use tauri::{AppHandle, Manager, Runtime};

use crate::error::{CommandError, blocking_cmd};
use crate::os::OsState;
use crate::state::AppState;

/// Open the system's sound settings. `unavailable` when this desktop has none to open.
#[tauri::command]
pub async fn open_sound_settings<R: Runtime>(app: AppHandle<R>) -> Result<(), CommandError> {
    blocking_cmd(move || {
        let unavailable = || CommandError::unavailable("no sound settings found");
        crate::desktop::get(&app)
            .ok_or_else(unavailable)?
            .open_sound_settings()
            .map_err(|_| unavailable())
    })
    .await
}

/// Open the desktop's own settings for the quick keep shortcut it holds. `unavailable` where the
/// app holds the shortcut, or the desktop cannot open them.
#[tauri::command]
pub async fn configure_shortcut<R: Runtime>(app: AppHandle<R>) -> Result<(), CommandError> {
    blocking_cmd(move || match app.try_state::<OsState>() {
        Some(os) => crate::os::shortcut::configure(&os),
        None => Err(CommandError::unavailable("no shortcut is held")),
    })
    .await
}

/// Ask the desktop again for the quick keep shortcut it did not add. Returns once it has
/// answered.
#[tauri::command]
pub async fn retry_shortcut<R: Runtime>(app: AppHandle<R>) -> Result<(), CommandError> {
    blocking_cmd(move || {
        let (Some(shell), Some(os)) = (app.try_state::<AppState>(), app.try_state::<OsState>())
        else {
            return Ok(());
        };
        let spelling = shell.engine()?.settings()?.shortcut;
        crate::os::shortcut::retry(&os, &spelling);
        Ok(())
    })
    .await
}

/// The command line that opens quick capture, for a desktop shortcut.
#[tauri::command]
pub async fn quick_keep_command<R: Runtime>(_app: AppHandle<R>) -> Result<String, CommandError> {
    Ok(crate::os::shortcut::quick_keep_command_now())
}
