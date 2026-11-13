//! The other editions of the page: spoken, printed.

use nocturne_contract::{Edition, PageView, SpokenPage};
use nocturne_engine::MindError;
use tauri::{AppHandle, Manager, Runtime};

use super::{engine, not_yet, parse_date, read, run};
use crate::error::{CommandError, ErrorCode, blocking, blocking_cmd};
use crate::state::AppState;

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

#[tauri::command]
pub async fn print_page<R: Runtime>(_app: AppHandle<R>) -> Result<(), CommandError> {
    Err(not_yet("printing"))
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
/// the audio file and how many private lines were left out. Sections and duration are filled
/// in by the spoken page work.
#[tauri::command]
pub async fn speak_page<R: Runtime>(
    app: AppHandle<R>,
    date: String,
) -> Result<SpokenPage, CommandError> {
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
            let skipped = page.as_ref().map_or(0, |p| p.private_skipped);
            return Ok(spoken(path, skipped));
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
        Ok(spoken(path, page.private_skipped))
    })
    .await
}

fn spoken(path: std::path::PathBuf, private_skipped: u32) -> SpokenPage {
    SpokenPage {
        path: path.to_string_lossy().into_owned(),
        sections: Vec::new(),
        duration_ms: 0,
        private_skipped,
    }
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
