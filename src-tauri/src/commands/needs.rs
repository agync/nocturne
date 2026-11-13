//! What waits on the person, and what is fading.

use nocturne_contract::{MemoryFilter, MemoryStatus, MemoryView, NeedsYouEntry};
use tauri::{AppHandle, Runtime};

use super::read;
use crate::error::CommandError;

#[tauri::command]
pub async fn list_needs_you<R: Runtime>(
    app: AppHandle<R>,
) -> Result<Vec<NeedsYouEntry>, CommandError> {
    read(app, |e| e.needs_you()).await
}

#[tauri::command]
pub async fn list_fading<R: Runtime>(app: AppHandle<R>) -> Result<Vec<MemoryView>, CommandError> {
    read(app, |e| {
        e.memories(MemoryFilter {
            status: Some(MemoryStatus::Fading),
            ..Default::default()
        })
    })
    .await
}
