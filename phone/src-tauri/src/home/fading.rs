//! Fading and letting go on a phone alone. On a computer they run each night with the tidy; a
//! phone has no night, so they run when the app opens or comes back to the front, at most once
//! a day. Nothing is deleted: a faded or let-go memory comes back when it is used or restored.

use nocturne_engine::DecayReport;
use serde::{Deserialize, Serialize};

use super::{Home, lock};
use crate::sync::ledger::write_private;

const FADED: &str = "last-faded.json";

/// The local day the policy last ran.
#[derive(Debug, Serialize, Deserialize)]
struct Faded {
    day: String,
}

impl Home {
    /// Fade and let go what has gone unused, unless that already ran today (the phone's own
    /// day, in its own time zone). Answers what changed, or none when it was not due or the
    /// memory is not open. Blocks: call it off the async workers.
    pub fn fade_if_due(&self) -> Option<DecayReport> {
        let _once = lock(&self.fading);
        let today = self
            .clock
            .now()
            .to_zoned(self.clock.tz())
            .date()
            .to_string();
        if self.last_faded().as_deref() == Some(today.as_str()) {
            return None;
        }
        let (_use, open) = self.using().ok()?;
        let report = match open.engine.decay() {
            Ok(report) => report,
            Err(e) => {
                tracing::warn!("what has gone unused could not be faded: {e}");
                return None;
            }
        };
        if let Err(e) = open.engine.retry_scrub() {
            tracing::warn!("forgotten words could not be cleared from the log: {e}");
        }
        let json = serde_json::to_vec(&Faded { day: today }).expect("the note serialises");
        if let Err(e) = write_private(&self.path(FADED), &json) {
            tracing::warn!("could not note the day fading ran: {e}");
        }
        Some(report)
    }

    fn last_faded(&self) -> Option<String> {
        let bytes = std::fs::read(self.path(FADED)).ok()?;
        serde_json::from_slice::<Faded>(&bytes).ok().map(|f| f.day)
    }

    /// A store that came back or started over has its own days: run again on the next open.
    pub(crate) fn forget_fading_note(&self) {
        let _ = std::fs::remove_file(self.path(FADED));
    }
}
