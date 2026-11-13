//! The Nocturne desktop shell: Tauri commands and events over the memory engine.
//!
//! `AppState` (engine, mind slot, status hub, live asks) is managed by Tauri. Commands are thin:
//! validate, run the engine on a blocking thread, map the result to a value or a `CommandError`.
//! Events (`status-changed`, `item-changed`, `ask-delta`, ...) are named in `nocturne_contract`.

pub mod ask;
pub mod commands;
pub mod error;
pub mod logging;
pub mod state;
pub mod status;
pub mod window;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use nocturne_contract::Source;
use nocturne_engine::SystemClock;
use tauri::{AppHandle, Builder, Manager, Runtime};

pub use commands::{COMMAND_NAMES, invoke_handler};
pub use state::{AppState, DATA_DIR_ENV, FAKE_MIND_ENV, MindSlot, publish_setup, refresh_status};

/// How often the status is recomputed when nothing else asks (a timed pause ends by itself).
const TICK: Duration = Duration::from_secs(15);

/// Keeps the log writer alive.
struct LogGuard(#[allow(dead_code)] Option<tracing_appender::non_blocking::WorkerGuard>);

/// Install the commands, the window behaviour and the setup step on a builder. A test manages
/// its own `AppState` first; otherwise setup opens the store (see [`resolve_state`]).
pub fn configure<R: Runtime>(builder: Builder<R>) -> Builder<R> {
    builder
        .invoke_handler(invoke_handler())
        .on_window_event(|window, event| window::on_window_event(window, event))
        .setup(|app| {
            setup(app.handle())?;
            Ok(())
        })
}

fn setup<R: Runtime>(app: &AppHandle<R>) -> Result<(), Box<dyn std::error::Error>> {
    if app.try_state::<AppState>().is_none() {
        let platform = app.path().app_data_dir()?;
        let dir = state::resolve_data_dir(std::env::var(DATA_DIR_ENV).ok(), platform);
        let _ = std::fs::create_dir_all(dir.join("logs"));
        let guard = logging::init(&dir.join("logs"));
        app.manage(LogGuard(guard));
        let state = AppState::open(&dir, Arc::new(SystemClock));
        state.mind.install_from_env();
        app.manage(state);
    }
    let state = app.state::<AppState>();
    allow_originals_in_the_interface(app, &state);
    window::match_system_theme(app);
    window::show_after_grace(app);

    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            refresh_status(&handle).await;
            tokio::time::sleep(TICK).await;
        }
    });
    Ok(())
}

/// The interface may show originals and spoken pages through the asset protocol, and nothing
/// else on disk.
fn allow_originals_in_the_interface<R: Runtime>(app: &AppHandle<R>, state: &AppState) {
    if !state.has_engine() {
        return;
    }
    let store = state.data_dir.join("store");
    let scope = app.asset_protocol_scope();
    for sub in ["originals", "audio"] {
        let dir = store.join(sub);
        if let Err(e) = commands::create_private_dir(&dir).and_then(|()| {
            scope
                .allow_directory(&dir, true)
                .map_err(std::io::Error::other)
        }) {
            tracing::warn!("could not open {sub} to the interface: {e}");
        }
    }
}

/// The files named on a second launch's command line: existing files only, `file://` links
/// accepted, flags and the program name ignored.
pub fn paths_from_args(args: &[String]) -> Vec<PathBuf> {
    args.iter()
        .skip(1)
        .filter(|a| !a.starts_with('-'))
        .map(|a| PathBuf::from(a.strip_prefix("file://").unwrap_or(a)))
        .filter(|p| p.is_file())
        .collect()
}

/// A second launch: keep any files it was given, then bring the window forward.
pub async fn handle_second_launch<R: Runtime>(app: &AppHandle<R>, args: Vec<String>) {
    let paths = paths_from_args(&args);
    if !paths.is_empty()
        && let Err(e) = commands::keep_paths(app, paths, Source::Drop).await
    {
        tracing::warn!("files from a second launch could not be kept: {e}");
    }
    window::show_main(app);
}

/// Run the app.
pub fn run() {
    configure(tauri::Builder::default())
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                handle_second_launch(&app, args).await;
            });
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .run(tauri::generate_context!())
        .expect("the Nocturne window could not start");
}
