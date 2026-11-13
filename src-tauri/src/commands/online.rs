//! Online answers: setting it up, turning it on and off, and what it sent. Main window only.
//! The key arrives once, in the connection form, and is moved into a redacting type at once.

use nocturne_contract::{
    CheckOutcome, OnlineForm, OnlineSend, OnlineSettings, OnlineSettingsPatch, OnlineView,
};
use nocturne_lemonade::online::OnlineKey;
use tauri::{AppHandle, Runtime};

use crate::error::CommandError;

fn not_yet() -> CommandError {
    CommandError::unavailable("Online answers is not available yet")
}

#[tauri::command]
pub async fn get_online<R: Runtime>(app: AppHandle<R>) -> Result<OnlineView, CommandError> {
    let _ = app;
    Ok(crate::online::off_view())
}

/// Check the address, key and model the person entered with one short question.
#[tauri::command]
pub async fn check_online<R: Runtime>(
    app: AppHandle<R>,
    form: OnlineForm,
) -> Result<CheckOutcome, CommandError> {
    let OnlineForm {
        base_url,
        key,
        model,
    } = form;
    let key = OnlineKey::new(key);
    let _ = (app, base_url, key, model);
    Err(not_yet())
}

#[tauri::command]
pub async fn cancel_online_check<R: Runtime>(app: AppHandle<R>) -> Result<(), CommandError> {
    let _ = app;
    Err(not_yet())
}

#[tauri::command]
pub async fn turn_on_online<R: Runtime>(
    app: AppHandle<R>,
    settings: OnlineSettings,
) -> Result<OnlineView, CommandError> {
    let _ = (app, settings);
    Err(not_yet())
}

/// Turn off; `forget` also removes the key.
#[tauri::command]
pub async fn turn_off_online<R: Runtime>(
    app: AppHandle<R>,
    forget: bool,
) -> Result<OnlineView, CommandError> {
    let _ = (app, forget);
    Err(not_yet())
}

/// Leave setup before saying yes: the key and the route are dropped.
#[tauri::command]
pub async fn abandon_online_setup<R: Runtime>(app: AppHandle<R>) -> Result<(), CommandError> {
    let _ = app;
    Err(not_yet())
}

#[tauri::command]
pub async fn update_online_settings<R: Runtime>(
    app: AppHandle<R>,
    patch: OnlineSettingsPatch,
) -> Result<OnlineView, CommandError> {
    let _ = (app, patch);
    Err(not_yet())
}

/// What was sent, newest first, `limit` at a time, before the send with id `before`.
#[tauri::command]
pub async fn list_online_sends<R: Runtime>(
    app: AppHandle<R>,
    limit: u32,
    before: Option<f64>,
) -> Result<Vec<OnlineSend>, CommandError> {
    let _ = (app, limit, before);
    Err(not_yet())
}
