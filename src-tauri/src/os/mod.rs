//! Where the app meets the desktop: tray, global shortcut, notification, start with the
//! computer, the watched folder and the quick capture window. The shell calls these hooks at
//! the edges of its life; each is safe when the part it serves is not running.

use std::error::Error;

use nocturne_contract::{SettingsPatch, SettingsView};
use tauri::{AppHandle, Runtime};

use crate::error::CommandError;

pub mod autostart;
pub mod copy;
pub mod inbox;
pub mod notify;
pub mod quick_capture;
pub mod sandbox;
pub mod shortcut;
pub mod sound;
pub mod state;
pub mod tray;
pub mod webview;

use std::sync::Arc;

pub use state::OsState;
use tauri::Manager;

use crate::state::AppState;

/// Sent to the main window when the tray asks for a screen (the payload is its route). It is
/// the event the interface already listens to for screens the quick capture window opens
/// (`onOpenInMain`), so the tray needs no receiver of its own.
pub const EVENT_NAVIGATE: &str = "open-in-main";

/// The commands the quick capture window may call. Everything else is refused for it.
const QUICK_CAPTURE_COMMANDS: &[&str] = &[
    "get_status",
    "get_settings",
    "keep_text",
    "keep_files",
    "take_quick_drop",
    "keep_bytes",
    "keep_recording",
    "unkeep",
    "get_item",
    "open_sound_settings",
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

    os.set_webview_dirs(webview::default_dirs(&app.config().identifier));
    os.set_registrar(Arc::new(shortcut::PluginRegistrar(app.clone())));
    if let Ok(settings) = shell.engine().and_then(|e| Ok(e.settings()?)) {
        shortcut::install(&os, &settings.shortcut, shortcut::session_allows_grab_now());
    }
    #[cfg(not(windows))]
    if let Some(target) = autostart::Target::from_env() {
        os.set_autostart(target);
    }
    if let Ok(settings) = shell.engine().and_then(|e| Ok(e.settings()?)) {
        match set_start_at_login(app, &os, settings.start_at_login) {
            Ok(()) => tracing::info!("start with this computer: {}", settings.start_at_login),
            Err(e) => tracing::warn!("start with this computer could not be reconciled: {e}"),
        }
    }
    if let Ok(settings) = shell.engine().and_then(|e| Ok(e.settings()?)) {
        let handle = app.clone();
        let data_dir = shell.data_dir.clone();
        tauri::async_runtime::spawn(async move {
            resume_inbox(&handle, &data_dir, settings.inbox_folder.as_deref()).await;
        });
    }
    crate::editions::speech::install(app);
    tray::install(app);
    notify::install(app, &os);
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

/// Watch the saved folder again. A folder that is not there yet is watched anyway: it is looked
/// for quietly until it appears (a drive that is not mounted).
pub async fn resume_inbox<R: Runtime>(
    app: &AppHandle<R>,
    data_dir: &std::path::Path,
    folder: Option<&str>,
) {
    let Some(folder) = folder.filter(|f| !f.trim().is_empty()) else {
        return;
    };
    match inbox::validate(folder, data_dir, inbox::home_dir().as_deref()) {
        Ok(root) => inbox::start(app, root).await,
        Err(inbox::InboxError::Missing) => inbox::start(app, folder.into()).await,
        Err(e) => tracing::warn!("the watched folder is not used: {e}"),
    }
}

/// Make the desktop start (or not start) the app at login.
fn set_start_at_login<R: Runtime>(
    app: &AppHandle<R>,
    os: &OsState,
    want: bool,
) -> Result<(), String> {
    #[cfg(windows)]
    {
        let _ = os;
        let Some(launcher) = app.try_state::<tauri_plugin_autostart::AutoLaunchManager>() else {
            return Err("the autostart plugin is not running".into());
        };
        let result = if want {
            launcher.enable()
        } else {
            launcher.disable()
        };
        result.map_err(|e| e.to_string())
    }
    #[cfg(not(windows))]
    {
        let _ = app;
        match os.autostart() {
            Some(target) => autostart::apply(want, &target)
                .map(|_| ())
                .map_err(|e| e.to_string()),
            // The launch agent needs the program's real path: without it nothing is written, and
            // the person is told rather than shown a switch that does nothing.
            None if want && cfg!(target_os = "macos") => {
                Err("the path of this program could not be resolved".into())
            }
            None => Ok(()),
        }
    }
}

/// Whether the desktop will start the app at login, or `None` when it cannot tell.
fn start_at_login_now<R: Runtime>(app: &AppHandle<R>, os: &OsState) -> Option<bool> {
    #[cfg(windows)]
    {
        let _ = os;
        app.try_state::<tauri_plugin_autostart::AutoLaunchManager>()?
            .is_enabled()
            .ok()
    }
    #[cfg(not(windows))]
    {
        let _ = app;
        os.autostart().map(|t| autostart::enabled(&t.dir))
    }
}

/// Fill in what only the desktop knows (the shortcut is registered, the folder is watched).
pub fn adjust_settings<R: Runtime>(app: &AppHandle<R>, mut view: SettingsView) -> SettingsView {
    view.inbox_folder = view.inbox_folder.filter(|f| !f.is_empty());
    if let Some(os) = app.try_state::<OsState>() {
        view.shortcut_available = os.shortcut_registered();
        if let Some(on) = start_at_login_now(app, &os) {
            view.start_at_login = on;
        }
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
    if let Some(folder) = &patch.inbox_folder
        && Some(folder) != before.inbox_folder.as_ref()
    {
        change_inbox(app, &os, folder).await?;
    }
    if let Some(want) = patch.start_at_login {
        set_start_at_login(app, &os, want).map_err(|e| {
            tracing::warn!("start with this computer was not changed: {e}");
            // The one refusal the person can act on keeps its reason, for the settings screen.
            if e == autostart::MOVE_FIRST {
                CommandError::unavailable(autostart::MOVE_FIRST)
            } else {
                CommandError::unavailable("start with this computer could not be changed")
            }
        })?;
    }
    Ok(())
}

/// Start, change or stop the watched folder (`""` stops it). Refuses a folder that is not
/// allowed before anything changes.
async fn change_inbox<R: Runtime>(
    app: &AppHandle<R>,
    os: &OsState,
    folder: &str,
) -> Result<(), CommandError> {
    if folder.trim().is_empty() {
        inbox::stop(app).await;
        os.update(|f| f.inbox = None);
        return Ok(());
    }
    let data_dir = os.data_dir().to_path_buf();
    let chosen = folder.to_string();
    let root = tokio::task::spawn_blocking(move || {
        let root = inbox::validate(&chosen, &data_dir, inbox::home_dir().as_deref())?;
        if inbox::too_many(&root, inbox::LEDGER_CAP) {
            return Err(inbox::InboxError::TooMany);
        }
        Ok(root)
    })
    .await
    .map_err(CommandError::internal)?
    .map_err(CommandError::invalid)?;
    inbox::stop(app).await;
    os.update(|f| f.inbox = None);
    inbox::start(app, root).await;
    Ok(())
}

/// Everything is about to be deleted: stop keeping files from the watched folder, and wait
/// until nothing is being kept, so nothing lands in the store after the wipe.
pub async fn before_delete_everything<R: Runtime>(app: &AppHandle<R>) {
    inbox::stop(app).await;
}

/// The deletion did not happen (a wrong phrase): watch the folder again.
pub async fn after_failed_delete<R: Runtime>(app: &AppHandle<R>) {
    let Some(shell) = app.try_state::<AppState>() else {
        return;
    };
    if let Ok(settings) = shell.engine().and_then(|e| Ok(e.settings()?)) {
        resume_inbox(app, &shell.data_dir, settings.inbox_folder.as_deref()).await;
    }
}

/// Everything was deleted: drop what the desktop side remembers, and undo what it set up. The
/// settings are back to their defaults, so the start-at-login entry goes and the shortcut is the
/// default one again. The web view's own storage and cache go too.
pub fn on_delete_everything<R: Runtime>(app: &AppHandle<R>) {
    let Some(os) = app.try_state::<OsState>() else {
        return;
    };
    os.forget_all();
    webview::remove(&os.webview_dirs());
    if let Err(e) = set_start_at_login(app, &os, false) {
        tracing::warn!("start with this computer could not be switched off: {e}");
    }
    if let Err(e) = shortcut::change(&os, shortcut::DEFAULT) {
        tracing::warn!(
            "the shortcut could not be put back to its default: {:?}",
            e.code
        );
    }
}

/// The person listened to the spoken page today: keep making it ahead for a while.
pub fn note_audio_used<R: Runtime>(app: &AppHandle<R>) {
    let (Some(shell), Some(os)) = (app.try_state::<AppState>(), app.try_state::<OsState>()) else {
        return;
    };
    let today = shell.now().to_zoned(shell.clock.tz()).date();
    os.update(|f| f.audio_used = Some(today.to_string()));
}

/// The main window came forward. If today's page is saved, the person is looking at it: record
/// the open, so the page notification stays quiet. (`get_page` never records an open: a
/// hidden start still renders the page.)
pub fn on_main_focused<R: Runtime>(app: &AppHandle<R>) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let (Some(shell), Some(os)) = (app.try_state::<AppState>(), app.try_state::<OsState>())
        else {
            return;
        };
        let Ok(engine) = shell.engine() else { return };
        let today = shell.now().to_zoned(shell.clock.tz()).date();
        let opened = crate::error::blocking(move || {
            if engine.page(Some(today))?.is_none() {
                return Ok(false);
            }
            engine.page_opened(today)?;
            Ok(true)
        })
        .await;
        if opened.unwrap_or(false) {
            os.update(|f| f.seen = Some(today.to_string()));
        }
    });
}

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
