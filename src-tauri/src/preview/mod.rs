//! The HTTP front that puts the real app behind a browser, so the real product (the private
//! engine, the real models, every command) can be used without the desktop shell. Two programs
//! use it: the development preview (`nocturne-preview`, the `preview` feature) and the
//! experimental server (`nocturne-server`, the `server` feature, for the container image; see
//! docs/adr/0012-server-mode.md).
//!
//! It builds the app on Tauri's mock runtime, the way the real-model end-to-end test does, and
//! forwards what a browser sends to the app's own commands through the same window allowlist as
//! the main window. Neither feature is in a default or release desktop build, and the package
//! checks fail if either program shows up in a desktop package.
//!
//! A bearer token guards everything that carries data: made at every launch for the preview,
//! kept owner-only in the data folder for the server. The interface is served without it, since
//! a browser cannot send a header for a page load.

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
mod run;
mod server;
mod transcode;

pub use auth::{Token, ensure_token_file, new_token, rotate_token_file};
pub use run::{claim, public_origin, serve, start_app};
pub use server::router;

/// Text the server prints and the package checks look for in anything that ships.
pub const MARKER: &str = "nocturne-preview-dev-only";

/// The file a preview data folder is marked with, so a folder that is not one is never taken
/// over (the real app's own data folder in particular).
pub const DATA_MARKER: &str = ".nocturne-preview";

/// How long a ticket for the event stream lives, and that it works once.
const EVENTS_TICKET_TTL: Duration = Duration::from_secs(30);
/// How long a ticket for showing originals lives after it was made or last used. Media elements
/// cannot send a header, so the ticket rides in the address and is kept short.
const MEDIA_TICKET_TTL: Duration = Duration::from_secs(3600);
/// The longest a media ticket lives from when it was made, however often it is used.
const MEDIA_TICKET_MAX: Duration = Duration::from_secs(12 * 3600);
/// How often an open event stream checks that its token is still the current one.
const STREAM_TOKEN_CHECK: Duration = Duration::from_secs(1);
/// Raw uploads (recordings, files) that may be read and converted at once.
const RAW_UPLOADS: usize = 2;

/// Where converted recordings are worked on, under the data folder. Emptied at every start.
pub(crate) const WORK_DIR: &str = "preview-tmp";

/// What the server is told at start.
#[derive(Clone, Debug)]
pub struct Options {
    /// The bearer token every request carrying data must present.
    pub token: Token,
    /// The port the server listens on; `127.0.0.1` on it is always an accepted host.
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
    issued: Instant,
    expires: Instant,
    /// The token in force when it was made: a new token ends it.
    token: String,
}

/// The tickets in use. Expired ones are dropped whenever the set is touched.
#[derive(Default)]
struct Tickets(HashMap<String, Ticket>);

impl Tickets {
    fn issue(&mut self, kind: TicketKind, now: Instant, token: String) -> String {
        let ticket = auth::random_hex(16);
        let ttl = match kind {
            TicketKind::Events => EVENTS_TICKET_TTL,
            TicketKind::Media => MEDIA_TICKET_TTL,
        };
        self.0.retain(|_, t| t.expires > now);
        self.0.insert(
            ticket.clone(),
            Ticket {
                kind,
                issued: now,
                expires: now + ttl,
                token,
            },
        );
        ticket
    }

    /// An events ticket is spent by the check; a media ticket is renewed by it, up to its
    /// maximum. A ticket made under another token than `current` is dropped.
    fn redeem(
        &mut self,
        ticket: &str,
        kind: TicketKind,
        now: Instant,
        current: Option<&str>,
    ) -> bool {
        self.0.retain(|_, t| t.expires > now);
        let Some(found) = self.0.get_mut(ticket).filter(|t| t.kind == kind) else {
            return false;
        };
        if !current.is_some_and(|c| auth::same(c, &found.token)) {
            self.0.remove(ticket);
            return false;
        }
        match kind {
            TicketKind::Events => {
                self.0.remove(ticket);
            }
            TicketKind::Media => {
                found.expires = (now + MEDIA_TICKET_TTL).min(found.issued + MEDIA_TICKET_MAX);
            }
        }
        true
    }
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
    /// The places on this machine that never reach the browser as text.
    pub(crate) hidden: Vec<String>,
    /// The converter, resolved once at start.
    pub(crate) ffmpeg: Option<PathBuf>,
    /// One slot per raw upload being read or converted.
    pub(crate) raw_slots: tokio::sync::Semaphore,
    tickets: Mutex<Tickets>,
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
        let hidden = files::hidden_roots(data_dir);
        let _ = std::fs::remove_dir_all(data_dir.join(WORK_DIR));
        for name in nocturne_contract::ALL_EVENTS {
            let tx = events.clone();
            let hidden = hidden.clone();
            app.listen(name, move |ev| {
                if let Some(payload) = files::hide_paths_in_json(ev.payload(), &hidden) {
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
            hidden,
            ffmpeg: transcode::find_ffmpeg(),
            raw_slots: tokio::sync::Semaphore::new(RAW_UPLOADS),
            tickets: Mutex::new(Tickets::default()),
            events,
        })
    }

    /// A ticket for the event stream (once, soon) or for media (for an hour after its last use,
    /// twelve at most), tied to the token in force. `None` when there is no token.
    pub(crate) fn issue(&self, kind: TicketKind) -> Option<String> {
        let token = self.opts.token.current()?;
        let mut all = self.tickets.lock().unwrap_or_else(|e| e.into_inner());
        Some(all.issue(kind, Instant::now(), token))
    }

    /// Whether `ticket` is a live one of this kind, made under the token in force.
    pub(crate) fn redeem(&self, ticket: &str, kind: TicketKind) -> bool {
        let current = self.opts.token.current();
        let mut all = self.tickets.lock().unwrap_or_else(|e| e.into_inner());
        all.redeem(ticket, kind, Instant::now(), current.as_deref())
    }

    pub(crate) fn engine(&self) -> Option<nocturne_engine::Engine> {
        self.app.state::<crate::AppState>().engine().ok()
    }

    pub(crate) fn first_status(&self) -> Option<Value> {
        invoke::call(&self.window, "get_status", serde_json::json!({}))
            .ok()
            .map(|mut v| {
                files::hide_paths(&mut v, &self.hidden);
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

#[cfg(test)]
mod tests {
    use super::*;

    const T: &str = "token-a";

    fn issue(tickets: &mut Tickets, kind: TicketKind, now: Instant) -> String {
        tickets.issue(kind, now, T.into())
    }

    #[test]
    fn a_media_ticket_is_renewed_by_use_and_dropped_when_it_lapses() {
        let t0 = Instant::now();
        let mut tickets = Tickets::default();
        let ticket = issue(&mut tickets, TicketKind::Media, t0);
        let later = t0 + MEDIA_TICKET_TTL - Duration::from_secs(1);
        assert!(tickets.redeem(&ticket, TicketKind::Media, later, Some(T)));
        let again = later + MEDIA_TICKET_TTL - Duration::from_secs(1);
        assert!(tickets.redeem(&ticket, TicketKind::Media, again, Some(T)));
        assert!(!tickets.redeem(&ticket, TicketKind::Events, again, Some(T)));
        let lapsed = again + MEDIA_TICKET_TTL;
        assert!(!tickets.redeem(&ticket, TicketKind::Media, lapsed, Some(T)));
        assert!(tickets.0.is_empty());
    }

    #[test]
    fn a_media_ticket_ends_at_its_maximum_however_often_it_is_used() {
        let t0 = Instant::now();
        let mut tickets = Tickets::default();
        let ticket = issue(&mut tickets, TicketKind::Media, t0);
        let step = MEDIA_TICKET_TTL / 2;
        let mut now = t0;
        while now + step < t0 + MEDIA_TICKET_MAX {
            now += step;
            assert!(tickets.redeem(&ticket, TicketKind::Media, now, Some(T)));
        }
        let end = t0 + MEDIA_TICKET_MAX;
        assert!(!tickets.redeem(&ticket, TicketKind::Media, end, Some(T)));
    }

    #[test]
    fn a_ticket_made_under_another_token_is_refused_and_dropped() {
        let t0 = Instant::now();
        let mut tickets = Tickets::default();
        let media = issue(&mut tickets, TicketKind::Media, t0);
        let events = issue(&mut tickets, TicketKind::Events, t0);
        assert!(!tickets.redeem(&media, TicketKind::Media, t0, Some("token-b")));
        assert!(!tickets.redeem(&events, TicketKind::Events, t0, None));
        assert!(tickets.0.is_empty());
        assert!(!tickets.redeem(&media, TicketKind::Media, t0, Some(T)));
    }

    #[test]
    fn an_events_ticket_works_once_and_lapses_quickly() {
        let t0 = Instant::now();
        let mut tickets = Tickets::default();
        let used = issue(&mut tickets, TicketKind::Events, t0);
        assert!(tickets.redeem(&used, TicketKind::Events, t0, Some(T)));
        assert!(!tickets.redeem(&used, TicketKind::Events, t0, Some(T)));
        let old = issue(&mut tickets, TicketKind::Events, t0);
        let late = t0 + EVENTS_TICKET_TTL;
        assert!(!tickets.redeem(&old, TicketKind::Events, late, Some(T)));
    }

    #[test]
    fn expired_tickets_are_purged_when_a_new_one_is_made() {
        let t0 = Instant::now();
        let mut tickets = Tickets::default();
        issue(&mut tickets, TicketKind::Media, t0);
        issue(&mut tickets, TicketKind::Events, t0);
        let later = t0 + MEDIA_TICKET_TTL + Duration::from_secs(1);
        issue(&mut tickets, TicketKind::Media, later);
        assert_eq!(tickets.0.len(), 1);
    }
}
