//! Online answers: setting it up, turning it on and off, and what it sent. Main window only.
//! The key arrives once, in the connection form, and is moved into a redacting type at once;
//! no reply carries it.

use std::sync::Arc;

use nocturne_contract::{
    CheckOutcome, OnlineForm, OnlineSend, OnlineSettings, OnlineSettingsPatch, OnlineView,
};
use tauri::{AppHandle, Manager, Runtime};

use crate::error::{CommandError, blocking};
use crate::online::controller::OnlineHub;
use crate::state::AppState;

fn hub<R: Runtime>(app: &AppHandle<R>) -> Result<Arc<OnlineHub<R>>, CommandError> {
    crate::online::hub(app).ok_or_else(|| CommandError::unavailable("Nocturne's store is not open"))
}

#[tauri::command]
pub async fn get_online<R: Runtime>(app: AppHandle<R>) -> Result<OnlineView, CommandError> {
    Ok(hub(&app)?.view().await)
}

/// Check the address, key and model the person entered with one short question.
#[tauri::command]
pub async fn check_online<R: Runtime>(
    app: AppHandle<R>,
    form: OnlineForm,
) -> Result<CheckOutcome, CommandError> {
    hub(&app)?.check(form).await
}

#[tauri::command]
pub async fn cancel_online_check<R: Runtime>(app: AppHandle<R>) -> Result<(), CommandError> {
    hub(&app)?.cancel_check();
    Ok(())
}

#[tauri::command]
pub async fn turn_on_online<R: Runtime>(
    app: AppHandle<R>,
    settings: OnlineSettings,
) -> Result<OnlineView, CommandError> {
    hub(&app)?.turn_on(settings).await
}

/// Turn off; `forget` also removes the key.
#[tauri::command]
pub async fn turn_off_online<R: Runtime>(
    app: AppHandle<R>,
    forget: bool,
) -> Result<OnlineView, CommandError> {
    hub(&app)?.turn_off(forget).await
}

/// The person has seen that only what they mark private is held back: on the consent step, or in
/// the one-time notice before their next online question.
#[tauri::command]
pub async fn see_online_change<R: Runtime>(app: AppHandle<R>) -> Result<OnlineView, CommandError> {
    hub(&app)?.see_change().await
}

/// Leave setup before saying yes: the key and the route are dropped.
#[tauri::command]
pub async fn abandon_online_setup<R: Runtime>(app: AppHandle<R>) -> Result<(), CommandError> {
    hub(&app)?.abandon_setup().await
}

#[tauri::command]
pub async fn update_online_settings<R: Runtime>(
    app: AppHandle<R>,
    patch: OnlineSettingsPatch,
) -> Result<OnlineView, CommandError> {
    hub(&app)?.update(patch).await
}

/// What was sent, newest first, `limit` at a time, before the send with id `before`.
#[tauri::command]
pub async fn list_online_sends<R: Runtime>(
    app: AppHandle<R>,
    limit: u32,
    before: Option<f64>,
) -> Result<Vec<OnlineSend>, CommandError> {
    let engine = app.state::<AppState>().engine()?;
    let limit = limit.clamp(1, 200);
    let before = before.map(|b| b as i64);
    blocking(move || engine.online_sends(limit, before)).await
}
