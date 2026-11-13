//! The activity trail: what a phone did and when, never what it contained. Repeats within a
//! minute collapse into one line with a count (the store does that); lines older than
//! `EVENTS_KEPT_DAYS` are pruned.

use nocturne_contract::{RemoteAction, RemoteResult};

use super::protocol::EVENTS_KEPT_DAYS;
use super::session::Core;

pub fn log(
    core: &Core,
    device: Option<&str>,
    action: RemoteAction,
    result: RemoteResult,
    bytes: f64,
) {
    let Some(engine) = core.shell.engine() else {
        return;
    };
    if let Err(e) = engine.log_remote_event(device, action, result, bytes) {
        tracing::warn!("could not note Away from home activity: {e}");
    }
}

/// Drop lines older than the keeping window.
pub fn prune(core: &Core) {
    let Some(engine) = core.shell.engine() else {
        return;
    };
    let days = jiff::SignedDuration::from_hours(24 * i64::from(EVENTS_KEPT_DAYS));
    if let Ok(before) = core.shell.wall_now().checked_sub(days)
        && let Err(e) = engine.prune_remote_events(before)
    {
        tracing::warn!("could not prune Away from home activity: {e}");
    }
}
