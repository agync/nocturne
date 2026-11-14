//! The real app in a browser, for development only. See `nocturne_app_lib::preview`.
//!
//! ```text
//! nocturne-preview --data-dir DIR [--port 55066] [--public-origin https://host:8443] [--ui DIR]
//! ```
//!
//! The data folder must be empty or one this tool made, never the app's own. The engine is found
//! the way the app finds it (`NOCTURNE_LEMOND`, then the checkout's `vendor/lemonade`).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use nocturne_app_lib::preview::{self, DATA_MARKER, MARKER, Options, Preview};
use nocturne_app_lib::{AppState, configure, logging, models, online};
use nocturne_engine::SystemClock;
use nocturne_lemonade::online::OwnPort;
use tauri::test::{mock_builder, mock_context, noop_assets};
use tauri::{Manager, WebviewWindowBuilder};

struct Args {
    data_dir: PathBuf,
    port: u16,
    public_origin: Option<String>,
    ui_dir: Option<PathBuf>,
}

fn usage() -> ! {
    eprintln!("usage: nocturne-preview --data-dir DIR [--port N] [--public-origin URL] [--ui DIR]");
    std::process::exit(2)
}

fn parse() -> Args {
    let mut args = std::env::args().skip(1);
    let (mut data_dir, mut port, mut public_origin, mut ui_dir) = (None, 55066u16, None, None);
    while let Some(flag) = args.next() {
        let mut value = || args.next().unwrap_or_else(|| usage());
        match flag.as_str() {
            "--data-dir" => data_dir = Some(PathBuf::from(value())),
            "--port" => port = value().parse().unwrap_or_else(|_| usage()),
            "--public-origin" => public_origin = Some(value()),
            "--ui" => ui_dir = Some(PathBuf::from(value())),
            _ => usage(),
        }
    }
    Args {
        data_dir: data_dir.unwrap_or_else(|| usage()),
        port,
        public_origin,
        ui_dir,
    }
}

/// A folder this tool may use: new, empty, or already marked as a preview folder.
fn claim(dir: &std::path::Path) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("the data folder cannot be made: {e}"))?;
    let marker = dir.join(DATA_MARKER);
    let empty = std::fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .next()
        .is_none();
    if !empty && !marker.is_file() {
        return Err(
            "that folder is not empty and was not made by this tool; refusing to use it".into(),
        );
    }
    std::fs::write(&marker, MARKER).map_err(|e| e.to_string())?;
    dir.canonicalize().map_err(|e| e.to_string())
}

fn main() {
    let args = parse();
    let data_dir = claim(&args.data_dir).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(2)
    });
    let _log = logging::init(&data_dir.join("logs"));

    let state = AppState::open(&data_dir, Arc::new(SystemClock));
    let app = configure(mock_builder().manage(state))
        .build(mock_context(noop_assets()))
        .expect("the app builds");
    nocturne_app_lib::allow_originals_in_the_interface(app.handle());
    let window = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("a window");
    let handle = app.handle().clone();
    // Before the engine starts, which asks for the route: the key is kept in the preview's own
    // folder, never in the system keychain the app uses, and turning on is noted here instead
    // of asked in a window of the system's own, which a browser does not have.
    if let Some(hub) = online::manage_hub(
        &handle,
        Arc::new(online::route::AppPort {
            app: handle.clone(),
        }),
        Arc::new(online::keys::NoKeychain),
    ) {
        hub.set_confirm(Arc::new(|address: &str| {
            println!("Online answers is turning on from the preview, with {address}");
            Box::pin(async { true })
        }));
    }
    if !nocturne_app_lib::start_services(&handle) {
        eprintln!("the store could not be opened");
        std::process::exit(1);
    }
    if let Err(e) = online::install(&handle) {
        eprintln!("Online answers did not start: {e}");
    }
    if let Err(e) = models::install(&handle) {
        eprintln!("the model choices did not start: {e}");
    }

    let token = preview::new_token();
    let opts = Options {
        token: token.clone(),
        port: args.port,
        public_origin: args.public_origin.clone(),
        ui_dir: args.ui_dir.clone(),
    };
    let server = Preview::new(handle.clone(), window, &data_dir, opts);

    let _own = OwnPort::hold(args.port);
    let listener =
        tauri::async_runtime::block_on(tokio::net::TcpListener::bind(("127.0.0.1", args.port)))
            .unwrap_or_else(|e| {
                eprintln!("cannot listen on 127.0.0.1:{}: {e}", args.port);
                std::process::exit(1)
            });
    println!("{MARKER}: development only, never shipped");
    println!("listening on http://127.0.0.1:{}", args.port);
    println!("open http://127.0.0.1:{}/#token={token}", args.port);
    if let Some(public) = &args.public_origin {
        println!("or {}/#token={token}", public.trim_end_matches('/'));
    }

    let stop = handle.clone();
    tauri::async_runtime::block_on(async move {
        let quit = tokio_util::sync::CancellationToken::new();
        tauri::async_runtime::spawn({
            let quit = quit.clone();
            async move {
                shutdown_signal().await;
                quit.cancel();
            }
        });
        let graceful = quit.clone();
        let served = axum::serve(listener, preview::router(server))
            .with_graceful_shutdown(async move { graceful.cancelled().await });
        // Open event streams would hold a graceful stop forever: give them a moment, then go.
        let patience = async {
            quit.cancelled().await;
            tokio::time::sleep(Duration::from_secs(3)).await;
        };
        tokio::select! {
            r = served => if let Err(e) = r { eprintln!("the server stopped: {e}") },
            () = patience => {}
        }
    });
    // The hooks wait on the engine themselves, so they cannot run inside the runtime.
    eprintln!("stopping the engine");
    stop.state::<AppState>().run_exit_hooks();
    drop(app);
}

async fn shutdown_signal() {
    use tokio::signal::unix::{SignalKind, signal};
    let (Ok(mut term), Ok(mut int)) = (
        signal(SignalKind::terminate()),
        signal(SignalKind::interrupt()),
    ) else {
        return std::future::pending().await;
    };
    tokio::select! {
        _ = term.recv() => {}
        _ = int.recv() => {}
    }
}
