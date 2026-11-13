//! Making the spoken page: one `speak` call per section, joined into one WAV with the section
//! starts inside it, kept in the store so the next listen is instant.
//!
//! One synthesis at a time per page, voice and data folder: a second call waits for the first
//! and then finds the kept file. If the page changes while the voice is working (a memory made
//! private, a line forgotten) the engine refuses to keep the audio; it is made again once, and
//! after that the answer is "busy".

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use jiff::Timestamp;
use jiff::civil::Date;
use jiff::tz::TimeZone;
use nocturne_contract::{EVENT_PAGE, Edition, EngineState, SpokenPage};
use nocturne_engine::{Engine, MindError};
use tauri::{AppHandle, Listener, Manager, Runtime};

use super::script::audio_script;
use super::wav::{self, WavError};
use crate::error::{CommandError, ErrorCode, blocking, blocking_cmd};
use crate::os::OsState;
use crate::state::{AppState, refresh_status};

/// Silence between sections.
pub const GAP_MS: u32 = 600;
/// The spoken page is prepared ahead only if it was used this recently.
const USED_WITHIN_DAYS: i64 = 14;
/// How many times a day the background make is tried.
const MAX_ATTEMPTS: u32 = 3;
const CHECK_EVERY: Duration = Duration::from_secs(60);

type Key = (PathBuf, String, String);

fn flights() -> &'static Mutex<HashMap<Key, Arc<tokio::sync::Mutex<()>>>> {
    static FLIGHTS: OnceLock<Mutex<HashMap<Key, Arc<tokio::sync::Mutex<()>>>>> = OnceLock::new();
    FLIGHTS.get_or_init(Mutex::default)
}

/// The spoken edition of the page for `day`, made if it is not kept yet.
pub async fn spoken_page<R: Runtime>(
    app: &AppHandle<R>,
    day: Date,
) -> Result<SpokenPage, CommandError> {
    let state = app.state::<AppState>();
    let engine = state.engine()?;
    let e = engine.clone();
    let voice = blocking(move || Ok(e.settings()?.voice)).await?;
    let key: Key = (state.data_dir.clone(), day.to_string(), voice.clone());
    let flight = {
        let mut map = flights().lock().unwrap_or_else(|e| e.into_inner());
        map.entry(key.clone()).or_default().clone()
    };
    let result = {
        let _one_at_a_time = flight.lock().await;
        make(app, &engine, day, &voice).await
    };
    let mut map = flights().lock().unwrap_or_else(|e| e.into_inner());
    // The map and this call hold it: nobody else is waiting.
    if Arc::strong_count(&flight) <= 2 {
        map.remove(&key);
    }
    result
}

/// A kept file read back: its sections and length, or `None` if it is not a file this version
/// wrote (an older one, a damaged one), in which case it is made again.
async fn read_kept(path: PathBuf) -> Option<(PathBuf, Vec<nocturne_contract::SpokenSection>, u32)> {
    blocking_cmd(move || {
        let bytes = std::fs::read(&path).map_err(|_| CommandError::internal("read"))?;
        let sections = wav::sections_of(&bytes).map_err(|_| CommandError::internal("wav"))?;
        let duration = wav::duration_ms(&bytes).map_err(|_| CommandError::internal("wav"))?;
        if sections.is_empty() {
            return Err(CommandError::internal("no sections"));
        }
        Ok((path, sections, duration))
    })
    .await
    .ok()
}

async fn make<R: Runtime>(
    app: &AppHandle<R>,
    engine: &Engine,
    day: Date,
    voice: &str,
) -> Result<SpokenPage, CommandError> {
    let state = app.state::<AppState>();
    let date = day.to_string();
    for _ in 0..2 {
        let e = engine.clone();
        let (d, v) = (date.clone(), voice.to_string());
        let (cached, stamp, page) = blocking(move || {
            let cached = e.cached_audio(&d, &v)?;
            // The stamp is taken before the page is read: audio made from a page that
            // changed in between is refused rather than kept.
            let stamp = e.audio_stamp(&d);
            Ok((cached, stamp, e.page_for(Some(day), Edition::Audio)?))
        })
        .await?;
        let skipped = page.as_ref().map_or(0, |p| p.private_skipped);
        if let Some(path) = cached
            && let Some((path, sections, duration_ms)) = read_kept(path).await
        {
            return Ok(SpokenPage {
                path: path.to_string_lossy().into_owned(),
                sections,
                duration_ms,
                private_skipped: skipped,
            });
        }
        let page = page.ok_or_else(|| CommandError::not_found(format!("page {day}")))?;
        let script = audio_script(&page);
        if script.is_empty() {
            return Err(CommandError::not_found("nothing to read"));
        }
        let mind = state
            .mind
            .get()
            .ok_or_else(|| CommandError::unavailable("the voice is not ready"))?;
        let mut parts = Vec::with_capacity(script.len());
        for section in &script {
            let bytes = mind
                .speak(&section.text, voice)
                .await
                .map_err(|e| match e {
                    MindError::Busy => CommandError::new(ErrorCode::Busy, e),
                    other => CommandError::unavailable(other),
                })?;
            parts.push((section.kind, bytes));
        }
        let (audio, sections) = wav::join(&parts, GAP_MS).map_err(unusable)?;
        let duration_ms = wav::duration_ms(&audio).map_err(unusable)?;
        let e = engine.clone();
        let (d, v) = (date.clone(), voice.to_string());
        let kept = blocking_cmd(move || {
            e.cache_audio(&d, &v, &audio, stamp)
                .map_err(CommandError::from)
        })
        .await?;
        if let Some(path) = kept {
            return Ok(SpokenPage {
                path: path.to_string_lossy().into_owned(),
                sections,
                duration_ms,
                private_skipped: page.private_skipped,
            });
        }
    }
    Err(CommandError::new(
        ErrorCode::Busy,
        "the page changed; try again",
    ))
}

fn unusable(e: WavError) -> CommandError {
    tracing::warn!("the voice returned audio that could not be used: {e}");
    CommandError::unavailable("the voice returned audio that could not be used")
}

/// Whether the spoken page should be made now, ahead of the person asking: it was used in the
/// last two weeks, today's page is saved and its time has come, and nothing else is going on
/// (not paused, not tidying), and it has not been tried too often today.
#[allow(clippy::too_many_arguments)]
pub fn should_prepare(
    now: Timestamp,
    tz: &TimeZone,
    page_time: &str,
    audio_used: Option<Date>,
    saved_today: bool,
    paused: bool,
    tidying: bool,
    attempts: u32,
) -> Option<Date> {
    if !saved_today || paused || tidying || attempts >= MAX_ATTEMPTS {
        return None;
    }
    let today = now.to_zoned(tz.clone()).date();
    let used = audio_used?;
    let age = today.since(used).ok()?.get_days();
    if !(0..=USED_WITHIN_DAYS as i32).contains(&age) {
        return None;
    }
    let (h, m) = page_time.split_once(':')?;
    let start = today
        .at(h.parse().ok()?, m.parse().ok()?, 0, 0)
        .to_zoned(tz.clone())
        .ok()?
        .timestamp();
    (now >= start).then_some(today)
}

/// Make today's spoken page in the background if it is due. Returns whether it was tried.
pub async fn prepare_if_due<R: Runtime>(app: &AppHandle<R>) -> bool {
    let (Some(shell), Some(os)) = (app.try_state::<AppState>(), app.try_state::<OsState>()) else {
        return false;
    };
    let Ok(engine) = shell.engine() else {
        return false;
    };
    if shell.mind.get().is_none() {
        return false;
    }
    let now = shell.now();
    let tz = shell.clock.tz();
    let today = now.to_zoned(tz.clone()).date();
    let status = refresh_status(app).await;
    let tidying = status.state == EngineState::Tidying || status.tidy.is_some();
    let paused = shell.status.is_paused(now);
    let used = os
        .file()
        .audio_used
        .as_deref()
        .and_then(|d| d.parse::<Date>().ok());
    let e = engine.clone();
    let read = blocking(move || {
        let settings = e.settings()?;
        let saved = e.page(Some(today))?.is_some();
        let kept = e
            .cached_audio(&today.to_string(), &settings.voice)?
            .is_some();
        Ok((settings.page_time, saved, kept))
    })
    .await;
    let Ok((page_time, saved, kept)) = read else {
        return false;
    };
    if kept {
        return false;
    }
    let attempts = os.audio_attempts(today);
    if should_prepare(now, &tz, &page_time, used, saved, paused, tidying, attempts).is_none() {
        return false;
    }
    os.note_audio_attempt(today);
    match spoken_page(app, today).await {
        Ok(_) => tracing::info!("the spoken page is ready"),
        Err(e) => tracing::info!("the spoken page was not made ahead: {:?}", e.code),
    }
    true
}

/// Prepare the spoken page when a page is saved and once a minute. The event is a trigger only.
pub fn install<R: Runtime>(app: &AppHandle<R>) {
    let handle = app.clone();
    app.listen_any(EVENT_PAGE, move |_| {
        let handle = handle.clone();
        tauri::async_runtime::spawn(async move {
            prepare_if_due(&handle).await;
        });
    });
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            prepare_if_due(&handle).await;
            tokio::time::sleep(CHECK_EVERY).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: &str) -> Timestamp {
        s.parse().unwrap()
    }

    fn due(
        now: &str,
        used: Option<&str>,
        saved: bool,
        paused: bool,
        tidying: bool,
        n: u32,
    ) -> bool {
        should_prepare(
            at(now),
            &TimeZone::get("Europe/London").unwrap(),
            "07:00",
            used.map(|d| d.parse().unwrap()),
            saved,
            paused,
            tidying,
            n,
        )
        .is_some()
    }

    #[test]
    fn ready_by_morning_only_when_it_is_used_saved_and_quiet() {
        let now = "2026-11-13T07:00:00Z";
        assert!(due(now, Some("2026-11-12"), true, false, false, 0));
        assert!(
            due(now, Some("2026-10-30"), true, false, false, 2),
            "14 days ago"
        );
        assert!(
            !due(now, Some("2026-10-29"), true, false, false, 0),
            "15 days ago"
        );
        assert!(!due(now, None, true, false, false, 0), "never used");
        assert!(!due(
            "2026-11-13T06:59:00Z",
            Some("2026-11-12"),
            true,
            false,
            false,
            0
        ));
        assert!(
            !due(now, Some("2026-11-12"), false, false, false, 0),
            "no page"
        );
        assert!(
            !due(now, Some("2026-11-12"), true, true, false, 0),
            "paused"
        );
        assert!(
            !due(now, Some("2026-11-12"), true, false, true, 0),
            "tidying"
        );
        assert!(
            !due(now, Some("2026-11-12"), true, false, false, 3),
            "tried enough"
        );
    }
}
