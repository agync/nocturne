//! The other editions of the page: spoken, printed.

use nocturne_contract::{Edition, PageView, SpokenPage};
use tauri::{AppHandle, Runtime};

use super::{parse_date, read, run};
use crate::error::CommandError;

/// A saved page in one edition (private lines are already removed from the spoken and printed
/// ones).
#[tauri::command]
pub async fn get_edition<R: Runtime>(
    app: AppHandle<R>,
    date: String,
    edition: Edition,
) -> Result<PageView, CommandError> {
    let day = parse_date(&date)?;
    read(app, move |e| {
        e.page_for(Some(day), edition)?
            .ok_or_else(|| nocturne_engine::Error::NotFound(format!("page {day}")))
    })
    .await
}

/// Open the system's print dialog for the window that asked (the print screen).
#[tauri::command]
pub async fn print_page<R: Runtime>(webview: tauri::WebviewWindow<R>) -> Result<(), CommandError> {
    webview
        .print()
        .map_err(|_| CommandError::unavailable("printing is not available"))
}

/// The spoken edition of a page, made once and kept (private lines are never spoken). Returns
/// the audio file, where each section starts, how long it plays and how many private lines were
/// left out.
#[tauri::command]
pub async fn speak_page<R: Runtime>(
    app: AppHandle<R>,
    date: String,
) -> Result<SpokenPage, CommandError> {
    let day = parse_date(&date)?;
    run(app, move |app| async move {
        let page = crate::editions::speech::spoken_page(&app, day).await?;
        crate::os::note_audio_used(&app);
        Ok(page)
    })
    .await
}
