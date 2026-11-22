//! Online answers in the app: whether a question's answer may be written by the person's
//! provider through the private engine, and the hooks the rest of the app calls. The hub
//! ([`controller::OnlineHub`]) owns the route and the key; off (the default) means the engine
//! has no provider, no row and no key, and no code builds an online request.

pub mod advanced;
pub mod controller;
pub mod keys;
pub mod limit;
pub mod notices;
pub mod privacy;
pub mod route;
#[doc(hidden)]
pub mod testing;

use std::error::Error;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use nocturne_contract::{OnlineAdvanced, OnlineLimit, OnlineState, OnlineView, PrivacyView};
use nocturne_lemonade::online::CloudRoute;
use tauri::{AppHandle, Manager, Runtime};

use crate::state::AppState;
use controller::OnlineHub;
use keys::{KeyStore, Keychain, SystemKeychain};
use route::{AppPort, EnginePort};

/// How often the sent log is pruned while the app runs.
const PRUNE_EVERY: Duration = Duration::from_secs(24 * 3600);

struct Hub<R: Runtime>(Arc<OnlineHub<R>>);

static MAKING: Mutex<()> = Mutex::new(());

/// Makes and manages the hub from these parts (tests pass fakes before first use).
pub fn manage_hub<R: Runtime>(
    app: &AppHandle<R>,
    port: Arc<dyn EnginePort>,
    keychain: Arc<dyn Keychain>,
) -> Option<Arc<OnlineHub<R>>> {
    let data_dir = app.try_state::<AppState>()?.data_dir.clone();
    let hub = OnlineHub::new(app.clone(), port, KeyStore::new(keychain, &data_dir));
    app.manage(Hub(Arc::new(hub)));
    app.try_state::<Hub<R>>().map(|h| h.0.clone())
}

/// The hub, made on first use with the system keychain and the app's engine. Making it reads
/// nothing: the key is read only when Online answers is on.
pub fn hub<R: Runtime>(app: &AppHandle<R>) -> Option<Arc<OnlineHub<R>>> {
    if let Some(h) = app.try_state::<Hub<R>>() {
        return Some(h.0.clone());
    }
    let _making = MAKING.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(h) = app.try_state::<Hub<R>>() {
        return Some(h.0.clone());
    }
    manage_hub(
        app,
        Arc::new(AppPort { app: app.clone() }),
        Arc::new(SystemKeychain),
    )
}

/// Called once the app is ready: prunes the sent log (and again each day), and relaunches an
/// engine that started without the route Online answers needs.
pub fn install<R: Runtime>(app: &AppHandle<R>) -> Result<(), Box<dyn Error>> {
    let Some(hub) = hub(app) else {
        return Ok(());
    };
    hub.prune();
    let daily = hub.clone();
    tauri::async_runtime::spawn(async move {
        daily.settle_after_launch().await;
        loop {
            tokio::time::sleep(PRUNE_EVERY).await;
            daily.prune();
        }
    });
    Ok(())
}

/// Asks in a window of the system's own, modal to the main window, before Online answers is
/// turned on with a route just checked, naming its full address, so script in the web view
/// cannot turn on an address of its own.
pub fn install_confirm(app: &AppHandle) {
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
    let Some(hub) = hub(app) else {
        return;
    };
    let handle = app.clone();
    hub.set_confirm(Arc::new(move |address: &str| {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let mut ask = handle
            .dialog()
            .message(format!(
                "While this is on, your questions and the few lines that answer them leave \
                 this computer and go to this address, which writes the answers:\n\n\
                 {address}\n\nTurn on online answers with it?"
            ))
            .title("Turn on online answers?")
            .kind(MessageDialogKind::Warning)
            .buttons(MessageDialogButtons::OkCancelCustom(
                "Turn on".into(),
                "Cancel".into(),
            ));
        if let Some(main) = handle.get_webview_window("main") {
            ask = ask.parent(&main);
        }
        ask.show(move |yes| {
            let _ = tx.send(yes);
        });
        Box::pin(async move { rx.await.unwrap_or(false) })
    }));
}

/// The route the engine's first launch uses. `None` while Online answers is off.
pub fn route_at_launch<R: Runtime>(app: &AppHandle<R>) -> Option<CloudRoute> {
    hub(app)?.route_at_launch()
}

/// Before the store is wiped, once the phrase is right: questions stop, the key goes from the
/// keychain and the file, and the engine is relaunched without the route.
pub async fn before_delete_everything<R: Runtime>(app: &AppHandle<R>) {
    if let Some(h) = hub(app) {
        h.before_delete().await;
    }
}

/// After the store is wiped: Online answers' own folder goes.
pub fn on_delete_everything<R: Runtime>(app: &AppHandle<R>) {
    match hub(app) {
        Some(h) => h.after_delete(),
        None => {
            if let Some(state) = app.try_state::<AppState>() {
                let _ = std::fs::remove_dir_all(state.data_dir.join(keys::FOLDER));
            }
        }
    }
}

/// Online answers' part of the privacy view.
pub fn privacy<R: Runtime>(app: &AppHandle<R>, view: &mut PrivacyView) {
    if let Some(engine) = app.try_state::<AppState>().and_then(|s| s.engine().ok()) {
        privacy::fill(&engine, view);
    }
}

/// Online answers in the advanced details, never with the key.
pub fn advanced<R: Runtime>(app: &AppHandle<R>) -> Option<OnlineAdvanced> {
    let engine = app.try_state::<AppState>()?.engine().ok()?;
    let state = match engine.online_state().ok()? {
        r if !r.wanted_on => OnlineState::Off,
        r => match (r.problem, r.pause) {
            (Some(code), _) => OnlineState::Problem {
                code,
                since: r.problem_since.map(|t| t.to_string()).unwrap_or_default(),
            },
            (None, Some(pause)) => OnlineState::Paused { pause },
            (None, None) => OnlineState::On,
        },
    };
    advanced::view(&engine, &state)
}

/// Online answers' files in an export, written into `dir`. The sent log is written by the
/// store's own export (`online/sent.md`); the key and its file are never exported.
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
        change_notice: false,
        limit: OnlineLimit::default(),
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
