//! The notices Online answers raises: the key refused, no credit, and the month's limit (once a
//! month). Each opens Online answers' settings; a successful check or a new month clears them.

use nocturne_contract::{Notice, NoticeCode};
use tauri::{AppHandle, Manager, Runtime};

use crate::state::AppState;

fn id(code: NoticeCode, month: Option<&str>) -> String {
    let base = match code {
        NoticeCode::OnlineKeyRefused => "onlineKeyRefused",
        NoticeCode::OnlineNoCredit => "onlineNoCredit",
        _ => "onlineLimit",
    };
    match month {
        Some(m) => format!("{base}:{m}"),
        None => base.to_string(),
    }
}

/// Raises one notice; the limit's carries its month so it is raised once a month.
pub async fn raise<R: Runtime>(app: &AppHandle<R>, code: NoticeCode, month: Option<&str>) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    state.status.add_notice(Notice {
        id: id(code, month),
        code,
        item_id: None,
        eta_seconds: None,
        bytes: None,
    });
    crate::state::refresh_status(app).await;
}

/// Clears every notice of these kinds.
pub async fn clear<R: Runtime>(app: &AppHandle<R>, codes: &[NoticeCode]) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    for code in codes {
        state.status.clear_notices_of(*code);
    }
    crate::state::refresh_status(app).await;
}
