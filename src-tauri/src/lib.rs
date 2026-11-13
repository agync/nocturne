//! The Nocturne desktop shell: Tauri commands and events over the memory engine.
//!
//! `AppState` (engine, mind slot, status hub, live asks) is managed by Tauri. Commands are thin:
//! validate, run the engine on a blocking thread, map the result to a value or a `CommandError`.
//! Events (`status-changed`, `item-changed`, `ask-delta`, ...) are named in `nocturne_contract`.
//!
//! The reader, the tidy and the scheduler live in [`runtime`]; they share the mind slot with
//! `AppState`. Whatever fills the slot is published with [`set_mind`], setup progress with
//! [`publish_setup`], and the engine is stopped in an [`AppState::on_exit`] hook. The OS
//! integration lane sets `tray_available` and `set_start_hidden`, adds its own exit hooks, and
//! gives the quick-capture window its own capability file (`capabilities/` only grants the
//! `main` window).

pub mod ask;
pub mod commands;
pub mod editions;
pub mod error;
pub mod fsutil;
pub mod logging;
pub mod models;
pub mod online;
pub mod os;
#[cfg(feature = "preview")]
pub mod preview;
pub mod remote;
pub mod runtime;
pub mod state;
pub mod status;
pub mod window;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use nocturne_contract::{Notice, NoticeCode, Source};
use nocturne_engine::SystemClock;
use tauri::{AppHandle, Builder, Manager, RunEvent, Runtime};

pub use commands::{COMMAND_NAMES, invoke_handler, keep_each, keep_paths};
pub use state::{
    AppState, DATA_DIR_ENV, FAKE_MIND_ENV, MindSlot, publish_setup, refresh_status, set_mind,
};

/// How often the status is recomputed when nothing else asks: a timed pause ends by itself.
/// Everything else refreshes on the event that changed it.
const TICK: Duration = Duration::from_secs(60);

/// Keeps the log writer alive.
struct LogGuard(#[allow(dead_code)] Option<tracing_appender::non_blocking::WorkerGuard>);

/// Install the commands, the window behaviour and the setup step on a builder. A test manages
/// its own `AppState` first; otherwise setup opens the store (see [`state::resolve_data_dir`]).
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
        let guard = logging::init(&dir.join("logs"));
        app.manage(LogGuard(guard));
        let state = AppState::open(&dir, Arc::new(SystemClock));
        state.mind.install_from_env();
        let args: Vec<String> = std::env::args().collect();
        state.set_start_hidden(window::wants_hidden(&args));
        app.manage(state);
    }
    start_services(app);
    allow_originals_in_the_interface(app);
    window::match_system_theme(app);
    window::show_after_grace(app);
    Ok(())
}

/// Start the reader, the tidy and the scheduler, and the loop that keeps the status and the
/// log files' permissions current. The desktop shell and the development preview both start
/// the app this way. `false` when the store could not be opened.
pub fn start_services<R: Runtime>(app: &AppHandle<R>) -> bool {
    let started = runtime::start(app, runtime::StartOptions::default()).is_some();
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            refresh_status(&handle).await;
            let dir = handle.state::<AppState>().log_dir();
            let _ = tokio::task::spawn_blocking(move || fsutil::restrict_folder_files(&dir)).await;
            tokio::time::sleep(TICK).await;
        }
    });
    started
}

/// The interface may show originals and spoken pages through the asset protocol, and nothing
/// else on disk.
pub fn allow_originals_in_the_interface<R: Runtime>(app: &AppHandle<R>) {
    let state = app.state::<AppState>();
    if !state.has_engine() {
        return;
    }
    let store = state.data_dir.join("store");
    let scope = app.asset_protocol_scope();
    for sub in ["originals", "audio"] {
        let dir = store.join(sub);
        if let Err(e) = fsutil::create_private_dir(&dir).and_then(|()| {
            scope
                .allow_directory(&dir, true)
                .map_err(std::io::Error::other)
        }) {
            tracing::warn!("could not open {sub} to the interface: {e}");
        }
    }
}

/// Decode `%XX` escapes (a `file://` link from a file manager).
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Some(hex) = s.get(i + 1..i + 3)
            && let Ok(v) = u8::from_str_radix(hex, 16)
        {
            out.push(v);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The files named on a second launch's command line: existing files only. `file://` links are
/// decoded, relative paths are taken from the second launch's own working folder, flags and
/// the program name are ignored.
pub fn paths_from_args(args: &[String], cwd: &str) -> Vec<PathBuf> {
    args.iter()
        .skip(1)
        .filter(|a| !a.starts_with('-'))
        .map(|a| match a.strip_prefix("file://") {
            Some(rest) => PathBuf::from(percent_decode(rest)),
            None => PathBuf::from(a),
        })
        .map(|p| {
            if p.is_relative() {
                PathBuf::from(cwd).join(p)
            } else {
                p
            }
        })
        .filter(|p| p.is_file())
        .collect()
}

/// A second launch: bring the window forward first (the person is waiting), then keep any files
/// it was given. If files were named and none could be kept, say so in the status.
pub async fn handle_second_launch<R: Runtime>(app: &AppHandle<R>, args: Vec<String>, cwd: String) {
    if os::on_second_launch(app, &args) {
        return;
    }
    window::show_main(app);
    let named = args.iter().skip(1).filter(|a| !a.starts_with('-')).count();
    let paths = paths_from_args(&args, &cwd);
    if named == 0 {
        return;
    }
    let kept = if paths.is_empty() {
        Err(())
    } else {
        keep_paths(app, paths, Source::Drop).await.map_err(|e| {
            tracing::warn!("files from a second launch could not be kept: {:?}", e.code);
        })
    };
    if let Ok(files) = &kept {
        tracing::info!("second launch: kept {} files", files.len());
    }
    if kept.is_err() {
        app.state::<AppState>().status.add_notice(Notice {
            id: commands::NOT_KEPT_NOTICE.to_string(),
            code: NoticeCode::CouldntRead,
            item_id: None,
            eta_seconds: None,
            bytes: None,
        });
        refresh_status(app).await;
    }
}

/// What the app does at the edges of its life: keep running in the tray when the last window
/// closes (if there is one), and run the exit hooks (stop the private engine, remove the tray).
pub fn handle_run_event<R: Runtime>(app: &AppHandle<R>, event: &RunEvent) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    match event {
        RunEvent::ExitRequested { api, code, .. } => {
            if exit_prevented(code.is_none(), state.tray_available()) {
                api.prevent_exit();
            }
        }
        RunEvent::Exit => {
            // Out of sight at once; stopping the engine can take a moment.
            for window in app.webview_windows().values() {
                let _ = window.hide();
            }
            state.run_exit_hooks();
        }
        _ => {}
    }
}

/// Whether an exit request is refused: only the "last window closed" kind (no exit code), and
/// only while the tray can bring the app back.
pub fn exit_prevented(last_window_closed: bool, tray_available: bool) -> bool {
    last_window_closed && tray_available
}

/// Run the app.
pub fn run() {
    // Before anything reads the Online answers key: no core dump of the app or the engine.
    if let Err(e) = nocturne_lemonade::supervisor::harden_process() {
        tracing::warn!("{e}");
    }
    let builder = configure(tauri::Builder::default())
        .plugin(tauri_plugin_single_instance::init(|app, args, cwd| {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                handle_second_launch(&app, args, cwd).await;
            });
        }))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init());
    #[cfg(windows)]
    let builder = builder.plugin(tauri_plugin_autostart::init(
        tauri_plugin_autostart::MacosLauncher::LaunchAgent,
        Some(vec!["--hidden"]),
    ));
    let app = builder
        .build(tauri::generate_context!())
        .expect("the Nocturne window could not start");
    #[cfg(unix)]
    quit_on_signals(app.handle());
    app.run(|app, event| {
        // The setup hook, which manages `AppState`, runs inside `run` before `Ready`.
        if matches!(event, RunEvent::Ready) {
            install_opener(app);
            if let Err(e) = os::install(app) {
                tracing::warn!("the desktop integrations did not start: {e}");
            }
            if let Err(e) = remote::install(app) {
                tracing::warn!("Away from home did not start: {e}");
            }
            if let Err(e) = online::install(app) {
                tracing::warn!("Online answers did not start: {e}");
            }
            if let Err(e) = models::install(app) {
                tracing::warn!("the model choices did not start: {e}");
            }
        }
        handle_run_event(app, &event);
    });
}

/// A termination signal (a logout, `kill`, Ctrl-C in a development run) quits the way closing
/// the last window does, so the private engine is stopped properly instead of left to its
/// watchdog. A second signal, from someone who will not wait, ends the app at once.
#[cfg(unix)]
fn quit_on_signals(app: &AppHandle) {
    use tokio::signal::unix::{SignalKind, signal};
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let (Ok(mut term), Ok(mut int)) = (
            signal(SignalKind::terminate()),
            signal(SignalKind::interrupt()),
        ) else {
            return;
        };
        let (tx, rx) = tokio::sync::mpsc::channel::<()>(4);
        tauri::async_runtime::spawn(async move {
            loop {
                tokio::select! {
                    _ = term.recv() => {}
                    _ = int.recv() => {}
                }
                if tx.send(()).await.is_err() {
                    return;
                }
            }
        });
        let quit = move || {
            tracing::info!("asked to quit by a signal");
            handle.exit(0);
        };
        let force = || {
            tracing::warn!("asked to quit again; leaving at once");
            std::process::exit(130);
        };
        serve_signals(rx, quit, force).await;
    });
}

/// The first signal quits, the second forces.
#[cfg(unix)]
async fn serve_signals(
    mut signals: tokio::sync::mpsc::Receiver<()>,
    quit: impl FnOnce(),
    force: impl FnOnce(),
) {
    if signals.recv().await.is_none() {
        return;
    }
    quit();
    if signals.recv().await.is_some() {
        force();
    }
}

fn install_opener(app: &AppHandle) {
    use tauri_plugin_opener::OpenerExt;
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let handle = app.clone();
    state.set_opener(Arc::new(move |path| {
        handle
            .opener()
            .open_path(path.to_string_lossy(), None::<&str>)
            .map_err(|e| e.to_string())
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[tokio::test]
    async fn the_first_signal_quits_and_the_second_forces() {
        use std::sync::atomic::{AtomicU32, Ordering};
        let (quits, forces) = (AtomicU32::new(0), AtomicU32::new(0));
        let (tx, rx) = tokio::sync::mpsc::channel::<()>(4);
        tx.send(()).await.unwrap();
        tx.send(()).await.unwrap();
        drop(tx);
        serve_signals(
            rx,
            || {
                quits.fetch_add(1, Ordering::SeqCst);
            },
            || {
                forces.fetch_add(1, Ordering::SeqCst);
            },
        )
        .await;
        assert_eq!(
            (quits.load(Ordering::SeqCst), forces.load(Ordering::SeqCst)),
            (1, 1)
        );
    }

    #[test]
    fn percent_escapes_decode_and_bad_ones_are_left_alone() {
        assert_eq!(percent_decode("/a%20b/c%2Fd"), "/a b/c/d");
        assert_eq!(percent_decode("/100%"), "/100%");
        assert_eq!(percent_decode("/%zz"), "/%zz");
        assert_eq!(percent_decode("/caf%C3%A9"), "/café");
        assert_eq!(
            percent_decode("/é%"),
            "/é%",
            "multi-byte text next to a stray percent"
        );
    }
}
