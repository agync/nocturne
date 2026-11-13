//! Choosing the model for each job: the choices, a change and its download, a re-read of
//! memories, and the old model. Main window only.

use nocturne_contract::{ChangePreview, Job, ModelChoice, ModelsView};
use tauri::{AppHandle, Manager, Runtime};

use crate::error::CommandError;
use crate::state::AppState;

fn not_yet() -> CommandError {
    CommandError::unavailable("choosing models is not available yet")
}

#[tauri::command]
pub async fn get_models<R: Runtime>(app: AppHandle<R>) -> Result<ModelsView, CommandError> {
    let dir = crate::models::config_dir(&app.state::<AppState>().data_dir);
    Ok(crate::models::preset_view(&dir))
}

#[tauri::command]
pub async fn list_model_choices<R: Runtime>(
    app: AppHandle<R>,
    job: Job,
) -> Result<Vec<ModelChoice>, CommandError> {
    let _ = (app, job);
    Err(not_yet())
}

#[tauri::command]
pub async fn preview_model_change<R: Runtime>(
    app: AppHandle<R>,
    job: Job,
    model: String,
) -> Result<ChangePreview, CommandError> {
    let _ = (app, job, model);
    Err(not_yet())
}

/// Change the model for `job`; `allow_download` is the person's yes to fetching it.
#[tauri::command]
pub async fn change_model<R: Runtime>(
    app: AppHandle<R>,
    job: Job,
    model: String,
    allow_download: bool,
) -> Result<ModelsView, CommandError> {
    let _ = (app, job, model, allow_download);
    Err(not_yet())
}

#[tauri::command]
pub async fn pause_model_download<R: Runtime>(
    app: AppHandle<R>,
) -> Result<ModelsView, CommandError> {
    let _ = app;
    Err(not_yet())
}

#[tauri::command]
pub async fn resume_model_download<R: Runtime>(
    app: AppHandle<R>,
) -> Result<ModelsView, CommandError> {
    let _ = app;
    Err(not_yet())
}

#[tauri::command]
pub async fn cancel_model_change<R: Runtime>(
    app: AppHandle<R>,
) -> Result<ModelsView, CommandError> {
    let _ = app;
    Err(not_yet())
}

#[tauri::command]
pub async fn cancel_reindex<R: Runtime>(app: AppHandle<R>) -> Result<ModelsView, CommandError> {
    let _ = app;
    Err(not_yet())
}

#[tauri::command]
pub async fn keep_old_model<R: Runtime>(app: AppHandle<R>) -> Result<ModelsView, CommandError> {
    let _ = app;
    Err(not_yet())
}

#[tauri::command]
pub async fn remove_old_model<R: Runtime>(app: AppHandle<R>) -> Result<ModelsView, CommandError> {
    let _ = app;
    Err(not_yet())
}

#[tauri::command]
pub async fn reset_models_to_tier<R: Runtime>(
    app: AppHandle<R>,
) -> Result<ModelsView, CommandError> {
    let _ = app;
    Err(not_yet())
}
