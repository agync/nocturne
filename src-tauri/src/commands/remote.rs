//! Away from home: turning it on and off, adding and removing phones, and its activity. Main
//! window only (the capability file grants these to no other window). Every body reads the
//! store and the hub; none takes an address, a path or a command from the interface.

use nocturne_contract::{
    DeviceView, PairingOffer, RemoteConnection, RemoteEvent, RemoteSettingsPatch, RemoteView,
};
use tauri::{AppHandle, Manager, Runtime};

use crate::error::CommandError;
use crate::remote::controller::RemoteHub;

fn hub<R: Runtime>(app: &AppHandle<R>) -> Result<std::sync::Arc<RemoteHub>, CommandError> {
    crate::remote::hub(app).ok_or_else(|| CommandError::unavailable("Nocturne's store is not open"))
}

#[tauri::command]
pub async fn get_remote<R: Runtime>(app: AppHandle<R>) -> Result<RemoteView, CommandError> {
    Ok(hub(&app)?.view().await)
}

/// Turn on with a connection; `allow_download` is the person's yes to fetching its helper.
#[tauri::command]
pub async fn start_remote<R: Runtime>(
    app: AppHandle<R>,
    connection: RemoteConnection,
    allow_download: bool,
) -> Result<RemoteView, CommandError> {
    hub(&app)?.start(connection, allow_download).await
}

/// Turn off; `remove_all` also unpairs every phone and removes the helper's sign-in.
#[tauri::command]
pub async fn stop_remote<R: Runtime>(
    app: AppHandle<R>,
    remove_all: bool,
) -> Result<RemoteView, CommandError> {
    hub(&app)?.stop(remove_all).await
}

#[tauri::command]
pub async fn pause_remote_download<R: Runtime>(
    app: AppHandle<R>,
) -> Result<RemoteView, CommandError> {
    hub(&app)?.pause_download().await
}

#[tauri::command]
pub async fn begin_pairing<R: Runtime>(app: AppHandle<R>) -> Result<PairingOffer, CommandError> {
    hub(&app)?.begin_pairing().await
}

#[tauri::command]
pub async fn decide_pairing<R: Runtime>(
    app: AppHandle<R>,
    pairing_id: String,
    allow: bool,
) -> Result<(), CommandError> {
    hub(&app)?.decide(&pairing_id, allow).await
}

#[tauri::command]
pub async fn cancel_pairing<R: Runtime>(app: AppHandle<R>) -> Result<(), CommandError> {
    hub(&app)?.cancel_pairing();
    Ok(())
}

#[tauri::command]
pub async fn rename_device<R: Runtime>(
    app: AppHandle<R>,
    id: String,
    name: String,
) -> Result<DeviceView, CommandError> {
    hub(&app)?.rename_device(&id, &name).await
}

/// Remove a phone. What it brought stays unless `also_delete_brought`, false when left out.
#[tauri::command]
pub async fn remove_device<R: Runtime>(
    app: AppHandle<R>,
    id: String,
    also_delete_brought: Option<bool>,
) -> Result<(), CommandError> {
    let hub = hub(&app)?;
    if also_delete_brought == Some(true) {
        // What it brought goes first: a phone is never removed with its delete left undone.
        let device = id.clone();
        app.state::<crate::state::AppState>().asks.cancel_all();
        super::change(app.clone(), move |e| e.delete_brought(&device)).await?;
    }
    hub.remove_device(&id).await
}

#[tauri::command]
pub async fn list_remote_events<R: Runtime>(
    app: AppHandle<R>,
    limit: u32,
) -> Result<Vec<RemoteEvent>, CommandError> {
    let engine = hub(&app)?
        .core
        .shell
        .engine()
        .ok_or_else(|| CommandError::unavailable("Nocturne's store is not open"))?;
    let limit = limit.clamp(1, 500);
    crate::error::blocking(move || engine.remote_events(limit)).await
}

/// A new `connection` unpairs every phone: their addresses belong to the other one.
#[tauri::command]
pub async fn update_remote_settings<R: Runtime>(
    app: AppHandle<R>,
    patch: RemoteSettingsPatch,
) -> Result<RemoteView, CommandError> {
    hub(&app)?.update_settings(patch).await
}

/// Opens the sign-in address held in Away from home's state, never one from the interface.
#[tauri::command]
pub async fn open_remote_sign_in<R: Runtime>(app: AppHandle<R>) -> Result<(), CommandError> {
    let url = hub(&app)?
        .sign_in_url()
        .ok_or_else(|| CommandError::invalid("there is nothing to sign in to"))?;
    if !url.starts_with("https://") {
        return Err(CommandError::invalid("there is nothing to sign in to"));
    }
    let unavailable = || CommandError::unavailable("the sign-in page could not be opened");
    crate::desktop::get(&app)
        .ok_or_else(unavailable)?
        .open_url(&url)
        .map_err(|_| unavailable())
}

/// A new key for this computer; every phone has to be added again.
#[tauri::command]
pub async fn new_computer_key<R: Runtime>(app: AppHandle<R>) -> Result<RemoteView, CommandError> {
    hub(&app)?.new_computer_key().await
}
