//! Looking back on a day or a period: what the journal shows, writing it on request, stopping
//! that, and deleting what was written. Argument key `period`.

use nocturne_contract::{Period, ReflectionView};
use tauri::{AppHandle, Manager, Runtime};

use super::run;
use crate::error::CommandError;
use crate::state::AppState;

/// What the journal shows for the period: what is written, or why it is not and what can be done.
#[tauri::command]
pub async fn get_reflection<R: Runtime>(
    app: AppHandle<R>,
    period: Period,
) -> Result<ReflectionView, CommandError> {
    run(app, move |app| async move {
        crate::journal::view(&app.state::<AppState>(), &period).await
    })
    .await
}

/// Write the reflection or recap for the period now. A period still going is written as it is so
/// far.
#[tauri::command]
pub async fn reflect<R: Runtime>(
    app: AppHandle<R>,
    period: Period,
) -> Result<ReflectionView, CommandError> {
    run(app, move |app| async move {
        crate::journal::reflect(&app.state::<AppState>(), &period).await
    })
    .await
}

/// Stop the reflection being written for the period.
#[tauri::command]
pub async fn stop_reflecting<R: Runtime>(
    app: AppHandle<R>,
    period: Period,
) -> Result<(), CommandError> {
    run(app, move |app| async move {
        crate::journal::stop(&app.state::<AppState>(), &period);
        Ok(())
    })
    .await
}

/// Delete what was written for the period. Nothing the person kept is touched.
#[tauri::command]
pub async fn delete_reflection<R: Runtime>(
    app: AppHandle<R>,
    period: Period,
) -> Result<(), CommandError> {
    run(app, move |app| async move {
        crate::journal::delete(&app.state::<AppState>(), &period).await
    })
    .await
}
