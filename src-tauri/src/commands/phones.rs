//! From your phones: what paired phone apps bring (the calendar and health readings), seeing,
//! stopping and deleting it, and the health pages. Main window only.
//!
//! Every body answers `unavailable` until the store keeps what phones bring.

use nocturne_contract::{
    CameInRow, HealthSeries, PhoneDeletePreview, PhoneDeleteScope, PhonePageOptions, PhoneSource,
    PhoneSourcesView, ReadingKind, ReadingRow,
};
use tauri::{AppHandle, Runtime};

use crate::error::CommandError;

pub(crate) fn not_ready() -> CommandError {
    CommandError::unavailable("what phones bring is not kept yet")
}

#[tauri::command]
pub async fn get_phone_sources<R: Runtime>(
    _app: AppHandle<R>,
) -> Result<PhoneSourcesView, CommandError> {
    Err(not_ready())
}

/// Stop or start taking one source from one phone. Stopping deletes nothing.
#[tauri::command]
pub async fn set_phone_source<R: Runtime>(
    _app: AppHandle<R>,
    _device_id: String,
    _source: PhoneSource,
    _taking: bool,
) -> Result<PhoneSourcesView, CommandError> {
    Err(not_ready())
}

#[tauri::command]
pub async fn set_phone_page_options<R: Runtime>(
    _app: AppHandle<R>,
    _options: PhonePageOptions,
) -> Result<PhoneSourcesView, CommandError> {
    Err(not_ready())
}

/// "Notice what goes together".
#[tauri::command]
pub async fn set_noticing<R: Runtime>(_app: AppHandle<R>, _on: bool) -> Result<(), CommandError> {
    Err(not_ready())
}

/// "What came in" from one source of one phone, newest first, before `before` when given.
#[tauri::command]
pub async fn list_came_in<R: Runtime>(
    _app: AppHandle<R>,
    _device_id: String,
    _source: PhoneSource,
    _before: Option<String>,
    _limit: u32,
) -> Result<Vec<CameInRow>, CommandError> {
    Err(not_ready())
}

/// One kind over days, `from` to `to` both included.
#[tauri::command]
pub async fn get_health_series<R: Runtime>(
    _app: AppHandle<R>,
    _kind: ReadingKind,
    _from: String,
    _to: String,
) -> Result<HealthSeries, CommandError> {
    Err(not_ready())
}

/// "Show as a list": every reading of one kind, `from` to `to` both included.
#[tauri::command]
pub async fn list_readings<R: Runtime>(
    _app: AppHandle<R>,
    _kind: ReadingKind,
    _from: String,
    _to: String,
) -> Result<Vec<ReadingRow>, CommandError> {
    Err(not_ready())
}

/// What a delete would remove, and the token that applies it.
#[tauri::command]
pub async fn preview_phone_delete<R: Runtime>(
    _app: AppHandle<R>,
    _scope: PhoneDeleteScope,
) -> Result<PhoneDeletePreview, CommandError> {
    Err(not_ready())
}

/// Apply a previewed delete; `also_stop` stops taking that source from the phone too.
#[tauri::command]
pub async fn delete_phone_data<R: Runtime>(
    _app: AppHandle<R>,
    _token: String,
    _also_stop: bool,
) -> Result<(), CommandError> {
    Err(not_ready())
}
