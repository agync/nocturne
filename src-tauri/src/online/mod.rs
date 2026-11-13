//! Online answers in the app: whether a question's answer may be written by the person's
//! provider through the private engine, and the hooks the rest of the app calls. Off is the only
//! state for now: no route, no key, nothing sent.

use std::error::Error;
use std::path::Path;

use nocturne_contract::{
    LimitKind, OnlineAdvanced, OnlineLimit, OnlineState, OnlineView, PrivacyView,
};
use nocturne_lemonade::online::{CloudRoute, DEFAULT_LIMIT};
use tauri::{AppHandle, Runtime};

/// Called once the app is ready.
pub fn install<R: Runtime>(app: &AppHandle<R>) -> Result<(), Box<dyn Error>> {
    let _ = app;
    Ok(())
}

/// The route the engine's first launch uses. `None` while Online answers is off.
pub fn route_at_launch<R: Runtime>(app: &AppHandle<R>) -> Option<CloudRoute> {
    let _ = app;
    None
}

/// Before the store is wiped, once the phrase is right.
pub async fn before_delete_everything<R: Runtime>(app: &AppHandle<R>) {
    let _ = app;
}

/// After the store is wiped.
pub fn on_delete_everything<R: Runtime>(app: &AppHandle<R>) {
    let _ = app;
}

/// Online answers' part of the privacy view.
pub fn privacy<R: Runtime>(app: &AppHandle<R>, view: &mut PrivacyView) {
    let _ = (app, view);
}

/// Online answers in the advanced details, never with the key.
pub fn advanced<R: Runtime>(app: &AppHandle<R>) -> Option<OnlineAdvanced> {
    let _ = app;
    None
}

/// Online answers' files in an export, written into `dir`. Never the key.
pub fn export_files<R: Runtime>(app: &AppHandle<R>, dir: &Path) -> std::io::Result<()> {
    let _ = (app, dir);
    Ok(())
}

/// The view while Online answers has never been set up.
pub fn off_view() -> OnlineView {
    OnlineView {
        state: OnlineState::Off,
        set_up: false,
        host: None,
        base_url: None,
        model: None,
        phones: true,
        phones_available: false,
        send_sensitive: false,
        limit: OnlineLimit {
            kind: LimitKind::Questions,
            questions: DEFAULT_LIMIT,
            usd: None,
        },
        questions_this_month: 0,
        spent_this_month_usd: None,
        cost_reported: false,
        resets_on: String::new(),
        last_sent_at: None,
        on_since: None,
        last_on_at: None,
        sent_bytes: 0.0,
        key_storage: None,
    }
}
