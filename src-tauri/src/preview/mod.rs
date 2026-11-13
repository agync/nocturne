//! A development-only server that puts the real app behind HTTP, so the real product (the
//! private engine, the real models, every command) can be used from a browser, a phone on the
//! owner's network included.
//!
//! It builds the app on Tauri's mock runtime, the way the real-model end-to-end test does, and
//! forwards what a browser sends to the app's own commands through the same window allowlist as
//! the main window. It binds loopback only and is never packaged: it sits behind the `preview`
//! feature, which no default or release build enables, and the package checks fail if it shows
//! up in one.
//!
//! A bearer token, made at every launch, guards everything that carries data. The interface is
//! served without it, since a browser cannot send a header for a page load.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::Value;
use tauri::test::MockRuntime;
use tauri::{AppHandle, Listener, Manager, WebviewWindow};
use tokio::sync::broadcast;

mod auth;
mod files;
mod invoke;
mod server;
mod transcode;

pub use auth::new_token;
pub use server::router;

/// Text the server prints and the package checks look for in anything that ships.
pub const MARKER: &str = "nocturne-preview-dev-only";

/// The file a preview data folder is marked with, so a folder that is not one is never taken
/// over (the real app's own data folder in particular).
pub const DATA_MARKER: &str = ".nocturne-preview";

/// How long a ticket for the event stream lives, and that it works once.
const EVENTS_TICKET_TTL: Duration = Duration::from_secs(30);
/// How long a ticket for showing originals lives. Media elements cannot send a header.
const MEDIA_TICKET_TTL: Duration = Duration::from_secs(12 * 3600);

/// What the server is told at start.
#[derive(Clone, Debug)]
pub struct Options {
    /// The bearer token every request carrying data must present.
    pub token: String,
    /// The one port the server listens on, on loopback.
    pub port: u16,
    /// Where the interface is reached from other machines, such as `https://host.example:8443`.
    pub public_origin: Option<String>,
    /// The built interface to serve, if any.
    pub ui_dir: Option<PathBuf>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum TicketKind {
    Events,
    Media,
}

struct Ticket {
    kind: TicketKind,
    expires: Instant,
}

/// One event the app emitted, as the browser hears it.
pub(crate) type Event = (String, String);

pub struct Preview {
    pub(crate) app: AppHandle<MockRuntime>,
    pub(crate) window: WebviewWindow<MockRuntime>,
    pub(crate) opts: Options,
    pub(crate) hosts: Vec<String>,
    pub(crate) origins: Vec<String>,
    pub(crate) data_dir: PathBuf,
    tickets: Mutex<HashMap<String, Ticket>>,
    pub(crate) events: broadcast::Sender<Event>,
}

impl Preview {
    /// Serve `window`'s app. Starts forwarding the app's events at once.
    pub fn new(
        app: AppHandle<MockRuntime>,
        window: WebviewWindow<MockRuntime>,
        data_dir: &Path,
        opts: Options,
    ) -> Arc<Preview> {
        let (events, _) = broadcast::channel(4096);
        let (hosts, origins) = allowed(&opts);
        for name in nocturne_contract::ALL_EVENTS {
            let tx = events.clone();
            app.listen(name, move |ev| {
                if let Some(payload) = files::hide_paths_in_json(ev.payload()) {
                    let _ = tx.send((name.to_string(), payload));
                }
            });
        }
        Arc::new(Preview {
            app,
            window,
            opts,
            hosts,
            origins,
            data_dir: data_dir.to_path_buf(),
            tickets: Mutex::new(HashMap::new()),
            events,
        })
    }

    /// A ticket for the event stream (once, soon) or for media (many times, for hours).
    pub(crate) fn issue(&self, kind: TicketKind) -> String {
        let ticket = auth::random_hex(16);
        let ttl = match kind {
            TicketKind::Events => EVENTS_TICKET_TTL,
            TicketKind::Media => MEDIA_TICKET_TTL,
        };
        let now = Instant::now();
        let mut all = self.tickets.lock().unwrap_or_else(|e| e.into_inner());
        all.retain(|_, t| t.expires > now);
        all.insert(
            ticket.clone(),
            Ticket {
                kind,
                expires: now + ttl,
            },
        );
        ticket
    }

    /// Whether `ticket` is a live one of this kind. An events ticket is spent by the check.
    pub(crate) fn redeem(&self, ticket: &str, kind: TicketKind) -> bool {
        let mut all = self.tickets.lock().unwrap_or_else(|e| e.into_inner());
        let live = all
            .get(ticket)
            .is_some_and(|t| t.kind == kind && t.expires > Instant::now());
        if live && kind == TicketKind::Events {
            all.remove(ticket);
        }
        live
    }

    pub(crate) fn engine(&self) -> Option<nocturne_engine::Engine> {
        self.app.state::<crate::AppState>().engine().ok()
    }

    pub(crate) fn first_status(&self) -> Option<Value> {
        invoke::call(&self.window, "get_status", serde_json::json!({}))
            .ok()
            .map(|mut v| {
                files::hide_paths(&mut v);
                v
            })
    }
}

/// The `Host` values and `Origin`s a request may carry: loopback on the port, and the public
/// origin when there is one.
fn allowed(opts: &Options) -> (Vec<String>, Vec<String>) {
    let mut hosts = vec![format!("127.0.0.1:{}", opts.port)];
    let mut origins = vec![format!("http://127.0.0.1:{}", opts.port)];
    if let Some(public) = &opts.public_origin {
        let public = public.trim_end_matches('/');
        origins.push(public.to_string());
        let host = public.split_once("://").map_or(public, |(_, h)| h);
        hosts.push(host.to_string());
        let default_port = if public.starts_with("https://") {
            ":443"
        } else {
            ":80"
        };
        if let Some(bare) = host.strip_suffix(default_port) {
            hosts.push(bare.to_string());
        }
    }
    (hosts, origins)
}
