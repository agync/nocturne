//! What the reader and the scheduler tell the screens on a phone alone: a thing being read or
//! read (as the screens read an item), and a page saved. The same event names as the computer's.

use nocturne_contract::{EVENT_ITEM, EVENT_PAGE, ItemView, PageSummary};
use nocturne_intelligence::Events;
use tauri::{AppHandle, Emitter, Runtime};

use crate::home::dispatch::phone_item;

/// How the mind is doing changed: the screens look at its status again.
pub const EVENT_STATUS: &str = "mind-status";

/// Tells the screens that how the mind is doing changed.
pub fn status_changed<R: Runtime>(app: &AppHandle<R>) {
    if let Err(e) = app.emit(EVENT_STATUS, ()) {
        tracing::warn!("could not tell the screens of the mind: {e}");
    }
}

pub struct PhoneEvents<R: Runtime>(pub AppHandle<R>);

impl<R: Runtime> Events for PhoneEvents<R> {
    fn item_changed(&self, item: &ItemView) {
        if let Err(e) = self.0.emit(EVENT_ITEM, phone_item(item)) {
            tracing::warn!("could not tell the screens of an item: {e}");
        }
    }

    fn page_ready(&self, page: PageSummary) {
        if let Err(e) = self.0.emit(EVENT_PAGE, page) {
            tracing::warn!("could not tell the screens of the page: {e}");
        }
    }
}
