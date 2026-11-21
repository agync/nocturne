//! What the phone screens ask of the app. Only Rust touches the network: the screens reach the
//! computer through these commands, never by fetch.

use nocturne_channel::client::{self, CallError, Connection, PairFailure, Reach};
use nocturne_channel::phone::PhonePairing;
use nocturne_contract::{AskDelta, AskPlace, PhoneError, RemoteAnswer, RemoteCall};
use serde::Serialize;
use tauri::ipc::{Channel, Response};
use tauri::{AppHandle, Runtime, State};

use crate::state::{AppState, now_ms};
use crate::store::{DeviceInfo, Meta};

fn not_paired() -> CallError {
    CallError::Connection {
        state: Reach::CantConnect,
    }
}

fn invalid() -> CallError {
    CallError::Phone {
        code: PhoneError::Invalid,
    }
}

/// A scanned pairing link: kept until the person names the phone.
#[tauri::command]
pub fn pair_scanned(state: State<'_, AppState>, link: String) -> Result<(), PairFailure> {
    let pairing = PhonePairing::scanned(&link, "").map_err(|_| PairFailure::BadCode)?;
    *state.pairing.lock().unwrap() = Some(pairing);
    Ok(())
}

/// Whether the computer answers at the typed address; answers the address it resolved to.
#[tauri::command]
pub async fn check_address(
    state: State<'_, AppState>,
    address: String,
) -> Result<String, PairFailure> {
    client::check_address(&state.router, &state.http, &address).await
}

/// A typed code at a typed address: the computer's key is asked for now, while the offer is open.
#[tauri::command]
pub async fn pair_typed(
    state: State<'_, AppState>,
    address: String,
    code: String,
) -> Result<(), PairFailure> {
    let mut pairing = PhonePairing::typed(&code, &address, "").map_err(|e| match e {
        nocturne_channel::phone::PairError::BadAddress => PairFailure::BadAddress,
        _ => PairFailure::BadCode,
    })?;
    client::take_hello(&mut pairing, &state.router, &state.http).await?;
    *state.pairing.lock().unwrap() = Some(pairing);
    Ok(())
}

/// Name the phone, show the words, and wait for the person's decision on the computer.
#[tauri::command]
pub async fn pair_connect<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    name: String,
    on_words: Channel<Vec<String>>,
) -> Result<DeviceInfo, PairFailure> {
    let mut pairing = state
        .pairing
        .lock()
        .unwrap()
        .take()
        .ok_or(PairFailure::Spent)?;
    pairing.set_name(&name).map_err(|_| PairFailure::BadName)?;
    let identity = client::pair(pairing, &state.router, &state.http, move |w| {
        let _ = on_words.send(w);
    })
    .await?;
    let meta = Meta {
        paired_at: now_ms(),
        ..Meta::default()
    };
    if state.store.save_identity(&identity).is_err() || state.store.save_meta(&meta).is_err() {
        return Err(PairFailure::Spent);
    }
    state.adopt(&app, identity.clone());
    Ok(DeviceInfo::new(&identity, meta))
}

#[tauri::command]
pub fn device(state: State<'_, AppState>) -> Option<DeviceInfo> {
    state.store.device()
}

/// Open a session now, or find out why not.
#[tauri::command]
pub async fn connect<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
) -> Result<Connection, CallError> {
    Ok(match state.phone(&app) {
        Some(phone) => phone.connect().await,
        None => Connection::CantConnect,
    })
}

/// The state as it stands, without opening anything.
#[tauri::command]
pub fn connection<R: Runtime>(app: AppHandle<R>, state: State<'_, AppState>) -> Connection {
    state
        .phone(&app)
        .map_or(Connection::CantConnect, |p| p.state())
}

#[tauri::command]
pub async fn call<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    call: RemoteCall,
) -> Result<RemoteAnswer, CallError> {
    let phone = state.phone(&app).ok_or_else(not_paired)?;
    phone.call(call).await
}

#[tauri::command]
pub async fn ask<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    question: String,
    place: Option<AskPlace>,
    on_delta: Channel<AskDelta>,
) -> Result<String, CallError> {
    let phone = state.phone(&app).ok_or_else(not_paired)?;
    phone
        .ask(&question, place, move |d| {
            let _ = on_delta.send(d);
        })
        .await
}

/// A keep with bytes, begun: its call. The bytes follow in slices; answers the handle.
#[tauri::command]
pub fn keep_begin(state: State<'_, AppState>, call: RemoteCall) -> Result<u32, CallError> {
    state
        .uploads
        .lock()
        .unwrap()
        .begin(call)
        .map_err(|_| invalid())
}

/// One slice of a keep's bytes, base64url, in order.
#[tauri::command]
pub fn keep_part(state: State<'_, AppState>, handle: u32, slice: String) -> Result<(), CallError> {
    state
        .uploads
        .lock()
        .unwrap()
        .part(handle, &slice)
        .map_err(|_| invalid())
}

/// Every slice is in: send the keep across.
#[tauri::command]
pub async fn keep_end<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    handle: u32,
) -> Result<RemoteAnswer, CallError> {
    let (call, bytes) = state
        .uploads
        .lock()
        .unwrap()
        .end(handle)
        .map_err(|_| invalid())?;
    let phone = state.phone(&app).ok_or_else(not_paired)?;
    phone.upload(call, &bytes).await
}

#[tauri::command]
pub fn keep_cancel(state: State<'_, AppState>, handle: u32) {
    state.uploads.lock().unwrap().cancel(handle);
}

/// An item's original, as `[type length][type][bytes]`.
#[tauri::command]
pub async fn original<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    id: String,
) -> Result<Response, CallError> {
    let phone = state.phone(&app).ok_or_else(not_paired)?;
    let (mime, bytes) = phone.original(&id).await?;
    Ok(Response::new(framed(&mime, &bytes)))
}

pub fn framed(mime: &str, bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(1 + mime.len() + bytes.len());
    out.push(mime.len() as u8);
    out.extend_from_slice(mime.as_bytes());
    out.extend_from_slice(bytes);
    out
}

#[tauri::command]
pub async fn relink<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    address: String,
) -> Result<String, CallError> {
    let phone = state.phone(&app).ok_or_else(not_paired)?;
    phone.relink(&address).await?;
    Ok(phone.identity().map(|i| i.address).unwrap_or_default())
}

#[derive(Serialize)]
pub struct Disconnected {
    reached: bool,
}

/// Tell the computer, then forget this phone.
#[tauri::command]
pub async fn disconnect<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
) -> Result<Disconnected, CallError> {
    let reached = match state.phone(&app) {
        Some(phone) => phone.disconnect().await,
        None => true,
    };
    state.drop_phone();
    state.store.clear();
    Ok(Disconnected { reached })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_the_original() {
        let f = framed("image/jpeg", &[1, 2, 3]);
        assert_eq!(f[0] as usize, "image/jpeg".len());
        assert_eq!(&f[1..11], b"image/jpeg");
        assert_eq!(&f[11..], &[1, 2, 3]);
    }

    #[test]
    fn keep_calls_parse_from_the_screens() {
        let call =
            r#"{"cmd":"keepBytes","mime":"image/jpeg","size":3,"clientId":"c-1","caption":null}"#;
        let parsed: RemoteCall = serde_json::from_str(call).unwrap();
        assert!(matches!(parsed, RemoteCall::KeepBytes { .. }));
    }
}
