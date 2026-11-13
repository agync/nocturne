//! The morning page, the day's question, which-is-right, ticking things off, and the spoken
//! edition.

use nocturne_contract::{
    Edition, NeedsYouEntry, PageSummary, PageView, QuestionChoice, WhichResolved,
};
use nocturne_engine::MindError;
use tauri::{AppHandle, Manager, Runtime};

use super::{change, engine, parse_date, read, run};
use crate::error::{CommandError, ErrorCode, blocking, blocking_cmd};
use crate::state::AppState;

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

/// What the spoken edition says: the headline and dek, then the lines that matter, without
/// the interface's emphasis marks. Forgotten lines are left out.
pub fn speech_text(page: &PageView) -> String {
    let mut parts = vec![page.headline.clone(), page.dek.clone()];
    for line in page
        .needs_you
        .iter()
        .chain(&page.changed)
        .chain(&page.upcoming)
        .filter(|l| !l.forgotten)
    {
        parts.push(line.text.replace("**", ""));
    }
    parts.retain(|p| !p.trim().is_empty());
    parts.join("\n")
}

/// The spoken edition of a page, made once and kept (private lines are never spoken). Returns
/// the path of the audio file.
#[tauri::command]
pub async fn speak_page<R: Runtime>(
    app: AppHandle<R>,
    date: String,
) -> Result<String, CommandError> {
    let day = parse_date(&date)?;
    run(app, move |app| async move {
        let state = app.state::<AppState>();
        let engine = engine(&app)?;
        let e = engine.clone();
        let (cached, voice, stamp, page) = blocking(move || {
            let voice = e.settings()?.voice;
            let cached = e.cached_audio(&day.to_string(), &voice)?;
            // The stamp is taken before the page is read: audio made from a page that
            // changed in between is refused rather than kept.
            let stamp = e.audio_stamp(&day.to_string());
            let page = e.page_for(Some(day), Edition::Audio)?;
            Ok((cached, voice, stamp, page))
        })
        .await?;
        if let Some(path) = cached {
            return Ok(path.to_string_lossy().into_owned());
        }
        let page = page.ok_or_else(|| CommandError::not_found(format!("page {day}")))?;
        let mind = state
            .mind
            .get()
            .ok_or_else(|| CommandError::unavailable("the voice is not ready"))?;
        let text = speech_text(&page);
        let bytes = mind.speak(&text, &voice).await.map_err(|e| match e {
            MindError::Busy => CommandError::new(ErrorCode::Busy, e),
            other => CommandError::unavailable(other),
        })?;
        let date = day.to_string();
        let path = blocking_cmd(move || {
            engine
                .cache_audio(&date, &voice, &bytes, stamp)
                .map_err(CommandError::from)
        })
        .await?
        .ok_or_else(|| CommandError::new(ErrorCode::Busy, "the page changed; try again"))?;
        Ok(path.to_string_lossy().into_owned())
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use nocturne_contract::{LineKind, PageLine, PageVariant};

    fn line(text: &str, forgotten: bool) -> PageLine {
        PageLine {
            id: "l".into(),
            kind: LineKind::Due,
            text: text.into(),
            memory_ids: vec![],
            receipts: vec![],
            choices: vec![],
            conflict_id: None,
            subject: None,
            done: false,
            forgotten,
            on: None,
            last_used: None,
        }
    }

    #[test]
    fn speech_drops_emphasis_and_forgotten_lines() {
        let page = PageView {
            date: "2026-11-13".into(),
            page_number: 1,
            edition: Edition::Audio,
            variant: PageVariant::Normal,
            headline: "Good morning.".into(),
            dek: "One thing today.".into(),
            needs_you: vec![line("Dentist at **10:00**.", false), line("gone", true)],
            changed: vec![],
            let_go: vec![line("not spoken", false)],
            upcoming: vec![line("Soup on Friday.", false)],
            learned: vec![],
            question: None,
            more_needs_you: 0,
            more_changed: 0,
            kept_count: 0,
            changes_count: 0,
            let_go_count: 0,
            catch_up_days: 0,
            private_skipped: 0,
            pending: 0,
            tidied_from: None,
            tidied_to: None,
            update: None,
        };
        assert_eq!(
            speech_text(&page),
            "Good morning.\nOne thing today.\nDentist at 10:00.\nSoup on Friday."
        );
    }
}
