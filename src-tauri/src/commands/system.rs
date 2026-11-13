//! Status, setup and the window.

use nocturne_contract::{EngineStatus, SetupView, Tier};
use tauri::{AppHandle, Manager, Runtime};

use super::run;
use crate::error::CommandError;
use crate::runtime;
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

/// Install the mind for a tier. Progress arrives as `setup-progress` events. Choosing a smaller
/// tier than the one in use, after a notice about memory, is how the person agrees to switch.
#[tauri::command]
pub async fn start_setup<R: Runtime>(app: AppHandle<R>, tier: Tier) -> Result<(), CommandError> {
    run(app, move |app| async move {
        runtime::start_setup(&app, tier).await
    })
    .await
}

#[tauri::command]
pub async fn pause_setup<R: Runtime>(app: AppHandle<R>) -> Result<(), CommandError> {
    run(app, |app| async move { runtime::pause_setup(&app).await }).await
}

#[tauri::command]
pub async fn resume_setup<R: Runtime>(app: AppHandle<R>) -> Result<(), CommandError> {
    run(app, |app| async move { runtime::resume_setup(&app).await }).await
}

/// The tier of a change the person asked for that did not finish, while its notice stands. The
/// interface starts setup for it when the person agrees.
#[tauri::command]
pub async fn get_unfinished_tier<R: Runtime>(
    app: AppHandle<R>,
) -> Result<Option<Tier>, CommandError> {
    run(app, |app| async move {
        Ok(app
            .state::<AppState>()
            .services()
            .and_then(|s| s.tier_unfinished()))
    })
    .await
}

/// "Not now" to the notice about an unfinished change.
#[tauri::command]
pub async fn decline_unfinished_tier<R: Runtime>(app: AppHandle<R>) -> Result<(), CommandError> {
    run(app, |app| async move {
        runtime::decline_unfinished_tier(&app).await;
        Ok(())
    })
    .await
}

/// "Try again" on the notice about restarts: the private engine is started afresh now.
#[tauri::command]
pub async fn retry_engine<R: Runtime>(app: AppHandle<R>) -> Result<(), CommandError> {
    run(app, |app| async move { runtime::retry_engine(&app).await }).await
}

/// The smaller mind on offer in the notice about memory, if the offer stands. The interface
/// starts setup for it when the person agrees.
#[tauri::command]
pub async fn get_tier_offer<R: Runtime>(app: AppHandle<R>) -> Result<Option<Tier>, CommandError> {
    run(app, |app| async move {
        Ok(app
            .state::<AppState>()
            .services()
            .and_then(|s| s.tier_offer()))
    })
    .await
}

/// "Not now": the offer goes, and comes back only after memory runs short twice more.
#[tauri::command]
pub async fn decline_tier_offer<R: Runtime>(app: AppHandle<R>) -> Result<(), CommandError> {
    run(app, |app| async move {
        runtime::decline_tier_offer(&app).await;
        Ok(())
    })
    .await
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
