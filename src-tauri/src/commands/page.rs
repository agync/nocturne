//! The morning page, the day's question, which-is-right and ticking things off.

use nocturne_contract::{
    Edition, NeedsYouEntry, PageSummary, PageView, QuestionChoice, WhichResolved,
};
use tauri::{AppHandle, Runtime};

use super::{change, parse_date, read};
use crate::error::CommandError;

/// A saved page, or the latest. When nothing is saved for the day being written (the next page
/// date), the page is composed on the spot (and not saved: the scheduler owns saving).
#[tauri::command]
pub async fn get_page<R: Runtime>(
    app: AppHandle<R>,
    date: Option<String>,
) -> Result<PageView, CommandError> {
    let day = date.as_deref().map(parse_date).transpose()?;
    read(app, move |e| {
        if let Some(p) = e.page(day)? {
            return Ok(p);
        }
        let next = e.next_page_date()?;
        match day {
            Some(d) if d != next => Err(nocturne_engine::Error::NotFound(format!("page {d}"))),
            _ => e.compose_page(next, Edition::Screen),
        }
    })
    .await
}

#[tauri::command]
pub async fn list_pages<R: Runtime>(app: AppHandle<R>) -> Result<Vec<PageSummary>, CommandError> {
    read(app, |e| e.pages()).await
}

#[tauri::command]
pub async fn answer_question<R: Runtime>(
    app: AppHandle<R>,
    page_date: String,
    choice: QuestionChoice,
) -> Result<(), CommandError> {
    let day = parse_date(&page_date)?;
    change(app, move |e| e.answer_question(day, choice)).await
}

/// Choose which of two memories is right. Returns what was settled, in the person's words.
#[tauri::command]
pub async fn resolve_which<R: Runtime>(
    app: AppHandle<R>,
    conflict_id: String,
    memory_id: String,
) -> Result<WhichResolved, CommandError> {
    change(app, move |e| {
        let memory = e.get_memory(&memory_id)?;
        // The wording the person chose, as the choice offered it.
        let chosen = e
            .needs_you()?
            .into_iter()
            .find_map(|entry| match entry {
                NeedsYouEntry::Which {
                    conflict_id: c,
                    options,
                    ..
                } if c == conflict_id => options
                    .into_iter()
                    .find(|o| o.memory_id == memory_id)
                    .map(|o| o.text),
                _ => None,
            })
            .unwrap_or_else(|| memory.object.clone());
        e.resolve_which(&conflict_id, &memory_id)?;
        Ok(WhichResolved {
            subject: memory.subject,
            chosen,
            memory_id,
        })
    })
    .await
}

#[tauri::command]
pub async fn mark_done<R: Runtime>(
    app: AppHandle<R>,
    memory_id: String,
) -> Result<(), CommandError> {
    change(app, move |e| e.mark_done(&memory_id)).await
}
