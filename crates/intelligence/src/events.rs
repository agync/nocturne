//! What the layer tells the app. Every method has an empty default, so a listener implements
//! only what it needs. Methods are called from the layer's tasks and must not block.

use nocturne_contract::{ItemView, Notice, NoticeCode, PageSummary, TidyProgress};

pub trait Events: Send + Sync {
    /// An item's state changed: reading started, it was understood, it could not be read.
    fn item_changed(&self, _item: &ItemView) {}
    /// A problem or event the interface explains in a sentence.
    fn notice(&self, _notice: Notice) {}
    /// The notice with this code (for any item) no longer applies.
    fn notice_cleared(&self, _code: NoticeCode) {}
    /// The overnight tidy's progress; `None` when none is running.
    fn tidy_progress(&self, _progress: Option<TidyProgress>) {}
    /// A page was saved (or saved again once the backlog was read).
    fn page_ready(&self, _page: PageSummary) {}
    /// The mind could not load a model because memory is short. The app counts these (two in
    /// ten minutes with little free memory means a smaller tier).
    fn out_of_memory(&self) {}
}

/// Listens to nothing.
#[derive(Debug, Default, Clone, Copy)]
pub struct NullEvents;

impl Events for NullEvents {}

pub(crate) fn notice(code: NoticeCode, item: Option<&str>) -> Notice {
    Notice {
        id: match item {
            Some(item) => format!("{code:?}:{item}"),
            None => format!("{code:?}"),
        },
        code,
        item_id: item.map(str::to_string),
        eta_seconds: None,
        bytes: None,
    }
}

#[cfg(any(test, feature = "fake"))]
pub use recording::{Event, RecordingEvents};

#[cfg(any(test, feature = "fake"))]
mod recording {
    use std::sync::Mutex;

    use nocturne_contract::{ItemStatus, Notice, NoticeCode, PageSummary, TidyProgress};

    use super::Events;

    #[derive(Debug, Clone, PartialEq)]
    pub enum Event {
        Item { id: String, status: ItemStatus },
        Notice(Notice),
        NoticeCleared(NoticeCode),
        Progress(Option<TidyProgress>),
        PageReady(PageSummary),
        OutOfMemory,
    }

    /// Remembers everything it is told, in order.
    #[derive(Debug, Default)]
    pub struct RecordingEvents {
        events: Mutex<Vec<Event>>,
    }

    impl RecordingEvents {
        pub fn new() -> std::sync::Arc<RecordingEvents> {
            std::sync::Arc::new(RecordingEvents::default())
        }

        fn push(&self, e: Event) {
            self.events
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(e);
        }

        pub fn all(&self) -> Vec<Event> {
            self.events
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone()
        }

        pub fn clear(&self) {
            self.events
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clear();
        }

        /// The statuses an item went through, in order, without repeats.
        pub fn statuses(&self, item: &str) -> Vec<ItemStatus> {
            let mut out: Vec<ItemStatus> = Vec::new();
            for e in self.all() {
                if let Event::Item { id, status } = e
                    && id == item
                    && out.last() != Some(&status)
                {
                    out.push(status);
                }
            }
            out
        }

        pub fn notices(&self) -> Vec<Notice> {
            self.all()
                .into_iter()
                .filter_map(|e| match e {
                    Event::Notice(n) => Some(n),
                    _ => None,
                })
                .collect()
        }

        pub fn pages(&self) -> Vec<PageSummary> {
            self.all()
                .into_iter()
                .filter_map(|e| match e {
                    Event::PageReady(p) => Some(p),
                    _ => None,
                })
                .collect()
        }
    }

    impl Events for RecordingEvents {
        fn item_changed(&self, item: &nocturne_contract::ItemView) {
            self.push(Event::Item {
                id: item.id.clone(),
                status: item.status,
            });
        }
        fn notice(&self, notice: Notice) {
            self.push(Event::Notice(notice));
        }
        fn notice_cleared(&self, code: NoticeCode) {
            self.push(Event::NoticeCleared(code));
        }
        fn tidy_progress(&self, progress: Option<TidyProgress>) {
            self.push(Event::Progress(progress));
        }
        fn page_ready(&self, page: PageSummary) {
            self.push(Event::PageReady(page));
        }
        fn out_of_memory(&self) {
            self.push(Event::OutOfMemory);
        }
    }
}
