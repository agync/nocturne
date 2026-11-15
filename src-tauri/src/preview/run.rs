//! What the preview and the server share around the router: the data folder they may use, the
//! app on the mock runtime, the address it is reached from, and serving until a stop signal.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use nocturne_engine::SystemClock;
use tauri::test::{MockRuntime, mock_builder, mock_context, noop_assets};
use tauri::{App, Manager, WebviewWindow, WebviewWindowBuilder};

use super::Preview;
use crate::{AppState, configure, models, online};

/// A data folder this program may use: new, empty, or already marked with `marker`, so the
/// desktop app's own folder (or the other program's) is never taken over.
pub fn claim(dir: &Path, marker: &str, text: &str) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("the data folder cannot be made: {e}"))?;
    let mark = dir.join(marker);
    let empty = std::fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .next()
        .is_none();
    if !empty && !mark.is_file() {
        return Err(
            "that folder is not empty and was not made by this program; refusing to use it".into(),
        );
    }
    std::fs::write(&mark, text).map_err(|e| e.to_string())?;
    dunce::canonicalize(dir).map_err(|e| e.to_string())
}

/// The address other machines reach the server from, as an origin. Only `https://`, or plain
/// `http://` on this computer, so a browser never sends the token in the clear over a network.
pub fn public_origin(text: &str) -> Result<String, String> {
    let url = url::Url::parse(text).map_err(|_| format!("not an address: {text}"))?;
    let extra = !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/";
    if extra {
        return Err(format!("give only the scheme, host and port: {text}"));
    }
    let local = matches!(
        url.host(),
        Some(url::Host::Domain("localhost"))
            | Some(url::Host::Ipv4(std::net::Ipv4Addr::LOCALHOST))
            | Some(url::Host::Ipv6(std::net::Ipv6Addr::LOCALHOST))
    );
    match url.scheme() {
        "https" => {}
        "http" if local => {}
        _ => return Err(format!("use https:// behind a TLS proxy: {text}")),
    }
    Ok(url.origin().ascii_serialization())
}

/// Build the app on the mock runtime over `data_dir` and start everything it runs: the store,
/// the reader and the tidy, Online answers and the model choices. The private engine starts
/// when a bundle is found (`NOCTURNE_LEMOND`).
pub fn start_app(
    data_dir: &Path,
) -> Result<(App<MockRuntime>, WebviewWindow<MockRuntime>), String> {
    let state = AppState::open(data_dir, Arc::new(SystemClock));
    let app = configure(mock_builder().manage(state))
        .build(mock_context(noop_assets()))
        .map_err(|e| format!("the app did not build: {e}"))?;
    crate::allow_originals_in_the_interface(app.handle());
    let window = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .map_err(|e| format!("no window: {e}"))?;
    let handle = app.handle().clone();
    // Before the engine starts, which asks for the route: the key is kept in the data folder's
    // owner-only key file, never in a system keychain, and turning on is written to the output
    // instead of asked in a window of the system's own, which a browser does not have.
    if let Some(hub) = online::manage_hub(
        &handle,
        Arc::new(online::route::AppPort {
            app: handle.clone(),
        }),
        Arc::new(online::keys::NoKeychain),
    ) {
        hub.set_confirm(Arc::new(|address: &str| {
            println!("Online answers is turning on, with {address}");
            Box::pin(async { true })
        }));
    }
    if !crate::start_services(&handle) {
        return Err("the store could not be opened".into());
    }
    if let Err(e) = online::install(&handle) {
        eprintln!("Online answers did not start: {e}");
    }
    if let Err(e) = models::install(&handle) {
        eprintln!("the model choices did not start: {e}");
    }
    Ok((app, window))
}

/// Serve `server` on `listener` until SIGTERM or SIGINT, then stop the engine.
pub fn serve(app: App<MockRuntime>, listener: tokio::net::TcpListener, server: Arc<Preview>) {
    let handle = app.handle().clone();
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
        let served = axum::serve(listener, super::router(server))
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
    handle.state::<AppState>().run_exit_hooks();
    drop(app);
}

#[cfg(unix)]
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

#[cfg(not(unix))]
async fn shutdown_signal() {
    if tokio::signal::ctrl_c().await.is_err() {
        std::future::pending::<()>().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_https_or_this_computer_is_a_public_origin() {
        assert_eq!(
            public_origin("https://memory.example.test").unwrap(),
            "https://memory.example.test"
        );
        assert_eq!(
            public_origin("https://memory.example.test:8443/").unwrap(),
            "https://memory.example.test:8443"
        );
        assert_eq!(
            public_origin("http://127.0.0.1:8080").unwrap(),
            "http://127.0.0.1:8080"
        );
        assert_eq!(
            public_origin("http://localhost:8080").unwrap(),
            "http://localhost:8080"
        );
        for bad in [
            "http://memory.example.test",
            "http://192.168.1.5:55066",
            "ftp://memory.example.test",
            "https://user@memory.example.test",
            "https://memory.example.test/nocturne",
            "https://memory.example.test/?a=1",
            "memory.example.test",
        ] {
            assert!(public_origin(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_folder_with_something_else_in_it_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let fresh = dir.path().join("fresh");
        assert!(claim(&fresh, ".mark", "x").is_ok());
        assert!(claim(&fresh, ".mark", "x").is_ok());
        assert!(claim(&fresh, ".other", "x").is_err());
        let used = dir.path().join("used");
        std::fs::create_dir_all(&used).unwrap();
        std::fs::write(used.join("nocturne.db"), b"").unwrap();
        assert!(claim(&used, ".mark", "x").is_err());
    }
}
