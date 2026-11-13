//! Commands that reach the desktop outside the window.

use tauri::{AppHandle, Runtime};

use crate::error::{CommandError, blocking_cmd};

/// Open the system's sound settings. `unavailable` when this desktop has none to open.
#[tauri::command]
pub async fn open_sound_settings<R: Runtime>(_app: AppHandle<R>) -> Result<(), CommandError> {
    blocking_cmd(|| {
        crate::os::sound::open()
            .map(|_| ())
            .map_err(|_| CommandError::unavailable("no sound settings found"))
    })
    .await
}

/// The command line that opens quick capture, for a desktop shortcut.
#[tauri::command]
pub async fn quick_keep_command<R: Runtime>(_app: AppHandle<R>) -> Result<String, CommandError> {
    Ok(crate::os::shortcut::quick_keep_command_now())
}
