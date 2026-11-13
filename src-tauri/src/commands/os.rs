//! Commands that reach the desktop outside the window.

use tauri::{AppHandle, Runtime};

use super::not_yet;
use crate::error::CommandError;

#[tauri::command]
pub async fn open_sound_settings<R: Runtime>(_app: AppHandle<R>) -> Result<(), CommandError> {
    Err(not_yet("sound settings"))
}

/// The command line that opens quick capture, for a desktop shortcut.
#[tauri::command]
pub async fn quick_keep_command<R: Runtime>(_app: AppHandle<R>) -> Result<String, CommandError> {
    Err(not_yet("the quick keep command"))
}
