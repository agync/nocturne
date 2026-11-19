//! Forgetting: a preview of exactly what goes, then the forget itself.

use nocturne_contract::{ForgetPreview, ForgetReport};
use tauri::{AppHandle, Manager, Runtime};

use crate::state::AppState;

use super::{change, read};
use crate::error::CommandError;

/// `also` are items deleted with the originals even when no memory came from them; with `pick`,
/// only they are deleted, and every other original listed stays.
#[tauri::command]
pub async fn preview_forget<R: Runtime>(
    app: AppHandle<R>,
    ids: Vec<String>,
    include_originals: bool,
    also: Option<Vec<String>>,
    pick: Option<bool>,
) -> Result<ForgetPreview, CommandError> {
    read(app, move |e| {
        e.preview_delete(
            &ids,
            include_originals,
            &also.unwrap_or_default(),
            pick.unwrap_or(false),
        )
    })
    .await
}

/// Carry out a previewed forget. The engine hides the memories everywhere in one step and
/// redacts the pages that cited them; the status is refreshed because counts moved.
#[tauri::command]
pub async fn forget<R: Runtime>(
    app: AppHandle<R>,
    preview_token: String,
) -> Result<ForgetReport, CommandError> {
    // The forget hides the memories at once; an answer already reading them is stopped first,
    // so no word of it follows the forget.
    app.state::<AppState>().asks.cancel_all();
    let outcome = change(app, move |e| e.forget(&preview_token)).await?;
    if !outcome.fully_scrubbed {
        tracing::warn!("a forget finished but the log could not be truncated yet");
    }
    Ok(outcome.report)
}
