//! Online answers in the advanced details: how the route runs and what the last question
//! did. Never the key, only where it is kept.

use nocturne_contract::{OnlineAdvanced, OnlineState};
use nocturne_engine::Engine;
use nocturne_lemonade::online::{MAX_TOKENS, host_of};

pub fn view(engine: &Engine, state: &OnlineState) -> Option<OnlineAdvanced> {
    let row = engine.online_state().ok()?;
    let (last, kept) = engine.last_online_send().ok()?;
    if !row.set_up && kept == 0 {
        return None;
    }
    let tz = engine.clock().tz();
    let host = row
        .base_url
        .as_deref()
        .and_then(|u| url::Url::parse(u).ok())
        .map(|u| host_of(&u));
    let state = match state {
        OnlineState::Off => "off",
        OnlineState::Checking => "checking",
        OnlineState::On => "on",
        OnlineState::Paused { .. } => "paused",
        OnlineState::Problem { .. } => "problem",
    };
    let route = match &row.base_url {
        Some(url) if row.wanted_on => {
            format!("engine cloud providers (experimental) · 127.0.0.1 → {url}")
        }
        _ => "none".to_string(),
    };
    Some(OnlineAdvanced {
        state: state.to_string(),
        route,
        base_url: row.base_url.clone(),
        host,
        model: row.model.clone(),
        max_tokens: MAX_TOKENS,
        key_storage: row.key_storage,
        last_at: last.as_ref().map(|l| l.at.to_string()),
        last_status: last.as_ref().and_then(|l| l.status),
        last_seconds: last.as_ref().and_then(|l| l.seconds),
        last_bytes_up: last.as_ref().map(|l| l.bytes_up),
        last_bytes_down: last.as_ref().map(|l| l.bytes_down),
        cost_reported: engine.online_cost_reported().unwrap_or(false),
        sends_kept: kept,
        questions_this_month: engine.questions_this_month(&tz).unwrap_or(0),
    })
}
