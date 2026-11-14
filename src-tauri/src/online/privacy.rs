//! Online answers' part of the privacy view: what it sent this month and in all, and whether it
//! is on and to where.

use nocturne_contract::PrivacyView;
use nocturne_engine::Engine;
use nocturne_lemonade::online::host_of;

pub fn fill(engine: &Engine, view: &mut PrivacyView) {
    let Ok(row) = engine.online_state() else {
        return;
    };
    let tz = engine.clock().tz();
    view.sent_online_bytes = row.sent_bytes;
    view.online_questions = engine.questions_this_month(&tz).unwrap_or(0);
    view.online_on = row.wanted_on;
    view.online_host = row
        .base_url
        .as_deref()
        .and_then(|u| url::Url::parse(u).ok())
        .map(|u| host_of(&u));
    view.online_last_on_at = row.on_since.or(row.last_on_at).map(|t| t.to_string());
}
