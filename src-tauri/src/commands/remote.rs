//! Away from home: turning it on and off, adding and removing phones, and its activity. Main
//! window only. Until the feature lands, `get_remote` reports it off and the rest are
//! unavailable.

use nocturne_contract::{
    AutoOff, DeviceView, PairingOffer, RemoteConnection, RemoteEvent, RemoteSettingsPatch,
    RemoteState, RemoteView,
};
use tauri::{AppHandle, Runtime};

use crate::error::CommandError;

fn not_yet() -> CommandError {
    CommandError::unavailable("Away from home is not available yet")
}

/// The view of a computer that has never turned Away from home on.
fn off_view() -> RemoteView {
    RemoteView {
        state: RemoteState::Off,
        connection: RemoteConnection::Simple,
        auto_off: AutoOff::Unused30Days,
        keep_awake: false,
        devices: Vec::new(),
        sent_to_phones_bytes: 0.0,
        last_on_at: None,
        on_since: None,
        set_up_before: false,
        installed: Vec::new(),
        trial: true,
        pairing: None,
        computer_name: sysinfo::System::host_name().unwrap_or_default(),
    }
}

#[tauri::command]
pub async fn get_remote<R: Runtime>(_app: AppHandle<R>) -> Result<RemoteView, CommandError> {
    Ok(off_view())
}

/// Turn on with a connection; `allow_download` is the person's yes to fetching its helper.
#[tauri::command]
pub async fn start_remote<R: Runtime>(
    _app: AppHandle<R>,
    connection: RemoteConnection,
    allow_download: bool,
) -> Result<RemoteView, CommandError> {
    let _ = (connection, allow_download);
    Err(not_yet())
}

/// Turn off; `remove_all` also unpairs every phone and removes the helper's sign-in.
#[tauri::command]
pub async fn stop_remote<R: Runtime>(
    _app: AppHandle<R>,
    remove_all: bool,
) -> Result<RemoteView, CommandError> {
    let _ = remove_all;
    Err(not_yet())
}

#[tauri::command]
pub async fn pause_remote_download<R: Runtime>(
    _app: AppHandle<R>,
) -> Result<RemoteView, CommandError> {
    Err(not_yet())
}

#[tauri::command]
pub async fn begin_pairing<R: Runtime>(_app: AppHandle<R>) -> Result<PairingOffer, CommandError> {
    Err(not_yet())
}

#[tauri::command]
pub async fn decide_pairing<R: Runtime>(
    _app: AppHandle<R>,
    pairing_id: String,
    allow: bool,
) -> Result<(), CommandError> {
    let _ = (pairing_id, allow);
    Err(not_yet())
}

#[tauri::command]
pub async fn cancel_pairing<R: Runtime>(_app: AppHandle<R>) -> Result<(), CommandError> {
    Err(not_yet())
}

#[tauri::command]
pub async fn rename_device<R: Runtime>(
    _app: AppHandle<R>,
    id: String,
    name: String,
) -> Result<DeviceView, CommandError> {
    let _ = (id, name);
    Err(not_yet())
}

#[tauri::command]
pub async fn remove_device<R: Runtime>(_app: AppHandle<R>, id: String) -> Result<(), CommandError> {
    let _ = id;
    Err(not_yet())
}

#[tauri::command]
pub async fn list_remote_events<R: Runtime>(
    _app: AppHandle<R>,
    limit: u32,
) -> Result<Vec<RemoteEvent>, CommandError> {
    let _ = limit;
    Err(not_yet())
}

/// A new `connection` unpairs every phone: their addresses belong to the other one.
#[tauri::command]
pub async fn update_remote_settings<R: Runtime>(
    _app: AppHandle<R>,
    patch: RemoteSettingsPatch,
) -> Result<RemoteView, CommandError> {
    let _ = patch;
    Err(not_yet())
}

/// Opens the sign-in address held in Away from home's state, never one from the interface.
#[tauri::command]
pub async fn open_remote_sign_in<R: Runtime>(_app: AppHandle<R>) -> Result<(), CommandError> {
    Err(not_yet())
}

/// A new key for this computer; every phone has to be added again.
#[tauri::command]
pub async fn new_computer_key<R: Runtime>(_app: AppHandle<R>) -> Result<RemoteView, CommandError> {
    Err(not_yet())
}
