//! Whether the chosen provider takes look-ups, remembered with its address and the name to use
//! (docs/design/ask-with-tools.md#when-the-mind-cannot-look-things-up): found out by the look-up
//! check after the connection check, or learned on the first refusal. Kept in a file of the
//! app's beside the choice; a new address or name starts again from nothing known.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use nocturne_provider::Provider;
use nocturne_provider::lookup::{self, Tools};
use serde::{Deserialize, Serialize};

use super::PhoneMind;
use crate::sync::ledger::write_private;

const FILE: &str = "tools.json";

#[derive(Debug, Serialize, Deserialize)]
struct Known {
    url: String,
    model: String,
    tools: Tools,
}

fn path(dir: &Path) -> PathBuf {
    dir.join(FILE)
}

/// What is known of `url` and `model`.
pub(super) fn read(dir: &Path, url: &str, model: &str) -> Tools {
    std::fs::read(path(dir))
        .ok()
        .and_then(|b| serde_json::from_slice::<Known>(&b).ok())
        .filter(|k| k.url == url.trim() && k.model == model.trim())
        .map(|k| k.tools)
        .unwrap_or_default()
}

/// Keeps what is known of `url` and `model`, in place of anything known before.
pub(super) fn write(dir: &Path, url: &str, model: &str, tools: Tools) {
    let known = Known {
        url: url.trim().to_string(),
        model: model.trim().to_string(),
        tools,
    };
    let bytes = serde_json::to_vec(&known).expect("what is known serialises");
    if let Err(e) = write_private(&path(dir), &bytes) {
        tracing::warn!("whether look-ups are taken was not kept: {e}");
    }
}

/// Who a mind tells when it learns whether look-ups are taken.
pub(super) fn keeper(dir: PathBuf, url: String, model: String) -> Arc<dyn Fn(Tools) + Send + Sync> {
    Arc::new(move |tools| write(&dir, &url, &model, tools))
}

impl PhoneMind {
    /// What is known now of whether the chosen provider takes look-ups.
    pub fn tools(&self) -> Tools {
        if let Some(on) = super::lock(&self.on).as_ref() {
            return on.mind.tools();
        }
        self.saved()
            .map(|s| read(&self.dir, &s.url, &s.model))
            .unwrap_or_default()
    }

    /// The look-up check, after a connection check that worked: listed with the check, never
    /// counted, and remembered with the address and the name to use. A request that got no
    /// answer leaves nothing known.
    pub(super) async fn check_tools(&self, provider: &Provider, url: &str) {
        let (passed, exchange) = lookup::check(provider).await;
        self.sent.note(
            nocturne_provider::Job::Check,
            provider.host(),
            provider.model(),
            &exchange,
        );
        let today = self
            .clock
            .now()
            .to_zoned_datetime(self.clock.tz())
            .date()
            .to_string();
        let tools = match passed {
            Some(true) => Tools::Takes { on: today },
            Some(false) => Tools::Refuses { on: today },
            None => Tools::Unknown,
        };
        write(&self.dir, url, provider.model(), tools);
    }
}
