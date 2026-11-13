//! Forgetting: a preview of exactly what goes, then the forget itself.

use nocturne_contract::{ForgetPreview, ForgetReport};
use tauri::{AppHandle, Runtime};

use super::{change, read};
use crate::error::CommandError;

#[tauri::command]
pub async fn preview_forget<R: Runtime>(
    app: AppHandle<R>,
    ids: Vec<String>,
    include_originals: bool,
) -> Result<ForgetPreview, CommandError> {
    read(app, move |e| e.preview_forget(&ids, include_originals)).await
}

/// Carry out a previewed forget. The engine hides the memories everywhere in one step and
/// redacts the pages that cited them; the status is refreshed because counts moved.
#[tauri::command]
pub async fn forget<R: Runtime>(
    app: AppHandle<R>,
    preview_token: String,
) -> Result<ForgetReport, CommandError> {
    let outcome = change(app, move |e| e.forget(&preview_token)).await?;
    if !outcome.fully_scrubbed {
        tracing::warn!("a forget finished but the log could not be truncated yet");
    }
    Ok(outcome.report)
}
