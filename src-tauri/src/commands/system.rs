//! Status, setup and the window.

use nocturne_contract::{EngineStatus, SetupView, Tier};
use tauri::{AppHandle, Manager, Runtime};

use super::{not_yet, run};
use crate::error::CommandError;
use crate::state::{AppState, refresh_status};

#[tauri::command]
pub async fn get_status<R: Runtime>(app: AppHandle<R>) -> Result<EngineStatus, CommandError> {
    run(app, |app| async move { Ok(refresh_status(&app).await) }).await
}

#[tauri::command]
pub async fn get_setup<R: Runtime>(app: AppHandle<R>) -> Result<SetupView, CommandError> {
    run(
        app,
        |app| async move { Ok(app.state::<AppState>().get_setup()) },
    )
    .await
}

/// Setup belongs to the runtime lane, which fills the mind slot and publishes progress with
/// `publish_setup`. Until it is wired these say so.
#[tauri::command]
pub async fn start_setup<R: Runtime>(app: AppHandle<R>, tier: Tier) -> Result<(), CommandError> {
    let _ = (app, tier);
    Err(not_yet("setup"))
}

#[tauri::command]
pub async fn pause_setup<R: Runtime>(app: AppHandle<R>) -> Result<(), CommandError> {
    let _ = app;
    Err(not_yet("setup"))
}

#[tauri::command]
pub async fn resume_setup<R: Runtime>(app: AppHandle<R>) -> Result<(), CommandError> {
    let _ = app;
    Err(not_yet("setup"))
}

/// "Tidy anyway": the battery rule is set aside until this tidy is done. The scheduler starts the
/// tidy on its next look, which is at most half a minute away.
#[tauri::command]
pub async fn tidy_anyway<R: Runtime>(app: AppHandle<R>) -> Result<(), CommandError> {
    run(app, |app| async move {
        let state = app.state::<AppState>();
        let services = state
            .services()
            .ok_or_else(|| CommandError::unavailable("the tidy is not running"))?;
        services.tidy.tidy_anyway();
        Ok(())
    })
    .await
}

/// The main interface has drawn its first frame: show the window. Another window being ready
/// shows nothing.
#[tauri::command]
pub async fn app_ready<R: Runtime>(
    app: AppHandle<R>,
    webview: tauri::Webview<R>,
) -> Result<(), CommandError> {
    if webview.label() != crate::window::MAIN {
        return Ok(());
    }
    run(app, |app| async move {
        crate::window::show_when_ready(&app, crate::window::ShowReason::InterfaceReady);
        Ok(())
    })
    .await
}
