//! Where the app meets the desktop: tray, global shortcut, notification, start with the
//! computer, the watched folder and the quick capture window. The shell calls these hooks at
//! the edges of its life; each is safe when the part it serves is not running.

use std::error::Error;

use nocturne_contract::{SettingsPatch, SettingsView};
use tauri::{AppHandle, Runtime};

use crate::error::CommandError;

pub mod quick_capture;
pub mod shortcut;
pub mod state;

use std::sync::Arc;

pub use state::OsState;
use tauri::Manager;

use crate::state::AppState;

/// The commands the quick capture window may call. Everything else is refused for it.
const QUICK_CAPTURE_COMMANDS: &[&str] = &[
    "get_status",
    "get_settings",
    "keep_text",
    "keep_files",
    "keep_bytes",
    "keep_recording",
    "unkeep",
    "get_item",
    "app_ready",
];

/// Start the desktop integrations. Runs once, from `RunEvent::Ready`, never from setup: tests
/// run setup, and the integrations touch the person's desktop.
pub fn install<R: Runtime>(app: &AppHandle<R>) -> Result<(), Box<dyn Error>> {
    let Some(shell) = app.try_state::<AppState>() else {
        return Ok(());
    };
    if app.try_state::<OsState>().is_some() {
        return Ok(());
    }
    app.manage(OsState::new(&shell.data_dir));
    let os = app.state::<OsState>();

    os.set_registrar(Arc::new(shortcut::PluginRegistrar(app.clone())));
    if let Ok(settings) = shell.engine().and_then(|e| Ok(e.settings()?)) {
        shortcut::install(&os, &settings.shortcut, shortcut::session_allows_grab_now());
    }
    let handle = app.clone();
    shell.on_exit(Box::new(move || {
        if let Some(os) = handle.try_state::<OsState>() {
            shortcut::release(&os);
        }
    }));
    Ok(())
}

/// Handle a second launch that is for the OS lane (`--quick-capture`). `true` means it was
/// handled and the main window should stay as it is.
pub fn on_second_launch<R: Runtime>(app: &AppHandle<R>, args: &[String]) -> bool {
    if quick_capture::wants_quick_capture(args) {
        quick_capture::open(app);
        return true;
    }
    false
}

/// Whether the window with this label may call this command.
pub fn window_may_call(label: &str, command: &str) -> bool {
    match label {
        crate::window::MAIN => true,
        quick_capture::LABEL => QUICK_CAPTURE_COMMANDS.contains(&command),
        _ => false,
    }
}

/// Fill in what only the desktop knows (the shortcut is registered, the folder is watched).
pub fn adjust_settings<R: Runtime>(app: &AppHandle<R>, mut view: SettingsView) -> SettingsView {
    if let Some(os) = app.try_state::<OsState>() {
        view.shortcut_available = os.shortcut_registered();
    }
    view
}

/// Apply a settings change to the desktop before it is saved, so a refusal keeps the old value.
pub async fn apply_settings<R: Runtime>(
    app: &AppHandle<R>,
    before: &SettingsView,
    patch: &SettingsPatch,
) -> Result<(), CommandError> {
    let Some(os) = app.try_state::<OsState>() else {
        return Ok(());
    };
    if let Some(new) = &patch.shortcut
        && *new != before.shortcut
    {
        shortcut::change(&os, new)?;
    }
    Ok(())
}

/// Everything was deleted: drop what the desktop side remembers.
pub fn on_delete_everything<R: Runtime>(_app: &AppHandle<R>) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_main_window_may_call_anything() {
        assert!(window_may_call("main", "delete_everything"));
    }

    #[test]
    fn quick_capture_may_keep_but_not_forget_or_delete() {
        for ok in ["keep_text", "keep_files", "app_ready", "get_status"] {
            assert!(window_may_call("quick-capture", ok), "{ok}");
        }
        for no in [
            "delete_everything",
            "forget",
            "export_to",
            "update_settings",
        ] {
            assert!(!window_may_call("quick-capture", no), "{no}");
        }
    }

    #[test]
    fn an_unknown_window_may_call_nothing() {
        assert!(!window_may_call("other", "get_status"));
        assert!(!window_may_call("", "get_status"));
    }
}
