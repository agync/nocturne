//! The main window: shown when the interface is ready (or after a grace period, so a broken
//! interface never leaves an invisible app), hidden instead of closed when the tray can bring it
//! back, and brought forward by a second launch.

use std::time::Duration;

use tauri::{AppHandle, Manager, Runtime, Window, WindowEvent};

use crate::state::AppState;

pub const MAIN: &str = "main";
/// How long the window stays hidden waiting for the interface before it is shown anyway.
pub const READY_GRACE: Duration = Duration::from_secs(5);

/// The window's background while the interface loads, to match the dark theme.
const DARK_BACKGROUND: tauri::window::Color = tauri::window::Color(14, 16, 20, 255);

/// Show the window now (a second launch, the tray). Always acts.
pub fn show_main<R: Runtime>(app: &AppHandle<R>) {
    if let Some(w) = app.get_webview_window(MAIN) {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        if let Some(state) = app.try_state::<AppState>() {
            state.mark_window_shown();
        }
    }
}

/// Why the main window is being shown at startup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShowReason {
    InterfaceReady,
    Grace,
    /// A hidden start with nowhere to hide: the tray is not available.
    NoTray,
}

/// Show the window because the interface is ready (or the grace period ran out): unless
/// something showed it already, or the app was asked to start in the tray.
pub fn show_when_ready<R: Runtime>(app: &AppHandle<R>, reason: ShowReason) {
    let skip = app
        .try_state::<AppState>()
        .is_some_and(|s| s.window_shown() || s.start_hidden());
    if !skip {
        match reason {
            ShowReason::InterfaceReady => tracing::info!("window shown: interface ready"),
            ShowReason::Grace => tracing::info!("window shown: grace"),
            ShowReason::NoTray => tracing::info!("window shown: no tray"),
        }
        show_main(app);
    }
}

/// `--hidden` on the command line starts in the tray (autostart passes it).
pub fn wants_hidden(args: &[String]) -> bool {
    args.iter().skip(1).any(|a| a == "--hidden")
}

/// Dark systems get the dark background before the interface has painted.
pub fn match_system_theme<R: Runtime>(app: &AppHandle<R>) {
    if let Some(w) = app.get_webview_window(MAIN)
        && matches!(w.theme(), Ok(tauri::Theme::Dark))
    {
        let _ = w.set_background_color(Some(DARK_BACKGROUND));
    }
}

/// Show the window after the grace period if the interface has not asked yet.
pub fn show_after_grace<R: Runtime>(app: &AppHandle<R>) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(READY_GRACE).await;
        show_when_ready(&app, ShowReason::Grace);
    });
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseAction {
    /// Keep running in the tray.
    Hide,
    /// Let the window close (and the app exit with the last one).
    Close,
}

pub fn close_action(tray_available: bool) -> CloseAction {
    if tray_available {
        CloseAction::Hide
    } else {
        CloseAction::Close
    }
}

pub fn on_window_event<R: Runtime>(window: &Window<R>, event: &WindowEvent) {
    // Files dropped on the window may be kept by path; nothing else may.
    if let WindowEvent::DragDrop(tauri::DragDropEvent::Drop { paths, .. }) = event
        && let Some(state) = window.app_handle().try_state::<AppState>()
    {
        state.allow_paths(paths.iter().cloned());
    }
    if let WindowEvent::Focused(true) = event
        && window.label() == MAIN
    {
        crate::os::on_main_focused(window.app_handle());
    }
    if let WindowEvent::CloseRequested { api, .. } = event
        && window.label() == MAIN
    {
        let tray = window
            .app_handle()
            .try_state::<AppState>()
            .is_some_and(|s| s.tray_available());
        if close_action(tray) == CloseAction::Hide {
            api.prevent_close();
            let _ = window.hide();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_is_asked_for_by_an_argument() {
        let args = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(wants_hidden(&args(&["nocturne", "--hidden"])));
        assert!(!wants_hidden(&args(&["nocturne"])));
        assert!(
            !wants_hidden(&args(&["--hidden"])),
            "the program name is not an argument"
        );
    }

    #[test]
    fn closing_hides_only_when_the_tray_can_bring_it_back() {
        assert_eq!(close_action(true), CloseAction::Hide);
        assert_eq!(close_action(false), CloseAction::Close);
    }
}
