//! The morning page, the day's question, which-is-right and ticking things off.

use nocturne_contract::{
    Edition, NeedsYouEntry, PageSummary, PageVariant, PageView, QuestionChoice, WhichResolved,
};
use tauri::{AppHandle, Manager, Runtime};

use super::{change, engine, parse_date, read, run};
use crate::error::{CommandError, blocking};
use crate::state::AppState;

/// A saved page, or the latest. When nothing is saved for the day being written (the next page
/// date), the page is composed on the spot (and not saved: the scheduler owns saving). While a
/// tidy is writing it, the page says so (`beingWritten`) and counts what is still to be read.
/// Opening a page is not recorded here: the window's focus does that.
#[tauri::command]
pub async fn get_page<R: Runtime>(
    app: AppHandle<R>,
    date: Option<String>,
) -> Result<PageView, CommandError> {
    let day = date.as_deref().map(parse_date).transpose()?;
    let services = app.state::<AppState>().services();
    run(app, move |app| async move {
        let engine = engine(&app)?;
        let (mut page, composed) = blocking(move || {
            if let Some(p) = engine.page(day)? {
                return Ok((p, false));
            }
            let next = engine.next_page_date()?;
            match day {
                Some(d) if d != next => Err(nocturne_engine::Error::NotFound(format!("page {d}"))),
                _ => Ok((engine.compose_page(next, Edition::Screen)?, true)),
            }
        })
        .await?;
        if composed
            && let Some(services) = services
            && services.tidy.running().await
        {
            page.variant = PageVariant::BeingWritten;
            let progress = services.tidy.progress().borrow().clone();
            if let Some(p) = progress {
                page.pending = page.pending.max(p.total.saturating_sub(p.done));
            }
        }
        Ok(page)
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
