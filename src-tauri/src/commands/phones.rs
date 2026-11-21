//! From your phones: what paired phone apps bring (the calendar and health readings), seeing,
//! stopping and deleting it, and the health pages. Main window only.

use jiff::civil::Date;
use nocturne_contract::{
    CameInRow, HealthSeries, PhoneDeletePreview, PhoneDeleteScope, PhonePageOptions, PhoneSource,
    PhoneSourcesView, ReadingKind, ReadingRow,
};
use nocturne_engine::PhoneBuilds;
use tauri::{AppHandle, Manager, Runtime};

use super::{change, read};
use crate::error::CommandError;
use crate::remote::protocol::{MIN_PHONE_BUILD, PHONE_BUILD};
use crate::state::AppState;

/// The phone builds this computer speaks, for the phones list's update lines.
const BUILDS: PhoneBuilds = PhoneBuilds {
    oldest: MIN_PHONE_BUILD,
    newest: PHONE_BUILD,
};

/// The most rows "What came in" lists at once.
const MAX_CAME_IN: u32 = 200;

fn day(s: &str) -> Result<Date, CommandError> {
    s.parse()
        .map_err(|_| CommandError::invalid("a day is YYYY-MM-DD"))
}

#[tauri::command]
pub async fn get_phone_sources<R: Runtime>(
    app: AppHandle<R>,
) -> Result<PhoneSourcesView, CommandError> {
    read(app, |e| e.phone_sources_for(BUILDS)).await
}

/// Stop or start taking one source from one phone. Stopping deletes nothing.
#[tauri::command]
pub async fn set_phone_source<R: Runtime>(
    app: AppHandle<R>,
    device_id: String,
    source: PhoneSource,
    taking: bool,
) -> Result<PhoneSourcesView, CommandError> {
    change(app, move |e| {
        e.set_source_taking(&device_id, source, taking)?;
        e.phone_sources_for(BUILDS)
    })
    .await
}

#[tauri::command]
pub async fn set_phone_page_options<R: Runtime>(
    app: AppHandle<R>,
    options: PhonePageOptions,
) -> Result<PhoneSourcesView, CommandError> {
    change(app, move |e| {
        e.set_phone_page_options(options)?;
        e.phone_sources_for(BUILDS)
    })
    .await
}

/// "Notice what goes together".
#[tauri::command]
pub async fn set_noticing<R: Runtime>(app: AppHandle<R>, on: bool) -> Result<(), CommandError> {
    change(app, move |e| e.set_noticing(on)).await
}

/// "What came in" from one source of one phone, newest first. Load more passes the last row's
/// `cameInAt` as `before` and its `itemId` as `beforeId`; `before` alone still works.
#[tauri::command]
pub async fn list_came_in<R: Runtime>(
    app: AppHandle<R>,
    device_id: String,
    source: PhoneSource,
    before: Option<String>,
    before_id: Option<String>,
    limit: u32,
) -> Result<Vec<CameInRow>, CommandError> {
    read(app, move |e| {
        e.came_in(
            &device_id,
            source,
            before.as_deref(),
            before_id.as_deref(),
            limit.min(MAX_CAME_IN),
        )
    })
    .await
}

/// One kind over days, `from` to `to` both included.
#[tauri::command]
pub async fn get_health_series<R: Runtime>(
    app: AppHandle<R>,
    kind: ReadingKind,
    from: String,
    to: String,
) -> Result<HealthSeries, CommandError> {
    let (from, to) = (day(&from)?, day(&to)?);
    read(app, move |e| e.health_series(kind, from, to)).await
}

/// "Show as a list": every reading of one kind, `from` to `to` both included.
#[tauri::command]
pub async fn list_readings<R: Runtime>(
    app: AppHandle<R>,
    kind: ReadingKind,
    from: String,
    to: String,
) -> Result<Vec<ReadingRow>, CommandError> {
    let (from, to) = (day(&from)?, day(&to)?);
    read(app, move |e| e.readings(kind, from, to)).await
}

/// What a delete would remove, and the token that applies it.
#[tauri::command]
pub async fn preview_phone_delete<R: Runtime>(
    app: AppHandle<R>,
    scope: PhoneDeleteScope,
) -> Result<PhoneDeletePreview, CommandError> {
    read(app, move |e| e.preview_phone_delete(scope)).await
}

/// Apply a previewed delete; `also_stop` stops taking that source from the phone too.
#[tauri::command]
pub async fn delete_phone_data<R: Runtime>(
    app: AppHandle<R>,
    token: String,
    also_stop: bool,
) -> Result<(), CommandError> {
    // An answer already reading an appointment is stopped first, as for every forget.
    app.state::<AppState>().asks.cancel_all();
    let outcome = change(app, move |e| e.delete_phone_data(&token, also_stop)).await?;
    if !outcome.fully_scrubbed {
        tracing::warn!("a phone delete finished but the log could not be truncated yet");
    }
    Ok(())
}
