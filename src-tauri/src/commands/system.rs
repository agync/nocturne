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

/// The interface has drawn its first frame: show the window.
#[tauri::command]
pub async fn app_ready<R: Runtime>(app: AppHandle<R>) -> Result<(), CommandError> {
    run(app, |app| async move {
        crate::window::show_main(&app);
        Ok(())
    })
    .await
}
