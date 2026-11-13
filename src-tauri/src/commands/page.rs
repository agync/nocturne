//! The morning page, the day's question, which-is-right and ticking things off.

use jiff::civil::Date;
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
    page_view(&app, day, Edition::Screen).await
}

/// The body of `get_page` for any edition. Every edition but the screen leaves out the lines
/// about private memories.
pub(crate) async fn page_view<R: Runtime>(
    app: &AppHandle<R>,
    day: Option<Date>,
    edition: Edition,
) -> Result<PageView, CommandError> {
    let services = app.state::<AppState>().services();
    run(app.clone(), move |app| async move {
        let engine = engine(&app)?;
        let (mut page, composed) = blocking(move || {
            let saved = match edition {
                Edition::Screen => engine.page(day)?,
                _ => engine.page_for(day, edition)?,
            };
            if let Some(p) = saved {
                return Ok((p, false));
            }
            let next = engine.next_page_date()?;
            match day {
                Some(d) if d != next => Err(nocturne_engine::Error::NotFound(format!("page {d}"))),
                _ => Ok((engine.compose_page(next, edition)?, true)),
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
    pages_for(&app, Edition::Screen).await
}

/// The body of `list_pages` for any edition. Every edition but the screen gives the template
/// headline when a line is private now and counts public needs-you lines only, the count
/// `nocturne_engine::needs_you_count` gives for that edition of the page (and the one a phone's
/// status carries).
pub(crate) async fn pages_for<R: Runtime>(
    app: &AppHandle<R>,
    edition: Edition,
) -> Result<Vec<PageSummary>, CommandError> {
    read(app.clone(), move |e| e.pages_for(edition)).await
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
