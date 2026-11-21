//! A link to the computer: a WebSocket when one opens quickly, otherwise long-poll. Frames come
//! in through an [`Inbox`] in arrival order, then the end. A link never sends a frame twice.

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use tokio::sync::mpsc;

use super::{poll, ws};
use crate::protocol::*;

/// Where frames go: `https://<host>`, or loopback for tests with `dev-loopback`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route {
    base: String,
}

impl Route {
    /// `https://<address>` for a good host name.
    pub fn https(address: &str) -> Option<Route> {
        crate::phone::check_host(address).map(|h| Route {
            base: format!("https://{h}"),
        })
    }

    /// `http://127.0.0.1:<port>`, whatever the address says.
    #[cfg(feature = "dev-loopback")]
    pub fn loopback(port: u16) -> Route {
        Route {
            base: format!("http://127.0.0.1:{port}"),
        }
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    pub(crate) fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }

    pub(crate) fn ws_url(&self) -> String {
        let url = self.url(PATH_SESSION);
        match url.strip_prefix("https://") {
            Some(rest) => format!("wss://{rest}"),
            None => format!("ws://{}", url.trim_start_matches("http://")),
        }
    }

    pub(crate) fn secure(&self) -> bool {
        self.base.starts_with("https://")
    }
}

/// Picks the route for an address. Only `https` unless `dev-loopback` is on.
pub type Router = Arc<dyn Fn(&str) -> Option<Route> + Send + Sync>;

pub fn https_router() -> Router {
    Arc::new(Route::https)
}

/// Present only in builds with `dev-loopback`, so a release check can refuse them.
#[cfg(feature = "dev-loopback")]
pub const DEV_LOOPBACK_MARKER: &str = "nocturne-dev-loopback-marker";

/// Every address reached at `http://127.0.0.1:<port>`: a computer behind `adb reverse`.
#[cfg(feature = "dev-loopback")]
pub fn loopback_router(port: u16) -> Router {
    tracing::warn!("{}", std::hint::black_box(DEV_LOOPBACK_MARKER));
    Arc::new(move |address| crate::phone::check_host(address).map(|_| Route::loopback(port)))
}

/// TLS with the public roots: the computer's addresses all use public certificates.
pub(crate) fn tls() -> Arc<rustls::ClientConfig> {
    static TLS: OnceLock<Arc<rustls::ClientConfig>> = OnceLock::new();
    TLS.get_or_init(|| {
        let roots = rustls::RootCertStore {
            roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
        };
        let mut cfg = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .expect("the default protocol versions")
        .with_root_certificates(roots)
        .with_no_client_auth();
        cfg.alpn_protocols = vec![b"http/1.1".to_vec()];
        Arc::new(cfg)
    })
    .clone()
}

/// The HTTP client for long-poll and the address check: no cookies, no redirects, no proxy.
pub fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .tls_backend_preconfigured((*tls()).clone())
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .connect_timeout(Duration::from_secs(10))
        .build()
        .expect("an HTTP client")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkEnd {
    /// The WebSocket closed with this code.
    Closed {
        code: u16,
    },
    Network,
    Http {
        status: u16,
    },
    Protocol,
}

pub(crate) enum Incoming {
    Frame(Vec<u8>),
    End(LinkEnd),
}

pub(crate) enum Got {
    Frame(Vec<u8>),
    End(LinkEnd),
    Timeout,
}

/// Frames in, in arrival order, then the end, which repeats.
pub(crate) struct Inbox {
    rx: mpsc::UnboundedReceiver<Incoming>,
    ended: Option<LinkEnd>,
}

pub(crate) type InboxTx = mpsc::UnboundedSender<Incoming>;

impl Inbox {
    pub(crate) fn new() -> (InboxTx, Inbox) {
        let (tx, rx) = mpsc::unbounded_channel();
        (tx, Inbox { rx, ended: None })
    }

    pub(crate) async fn next(&mut self, wait: Option<Duration>) -> Got {
        if let Some(end) = self.ended {
            return Got::End(end);
        }
        let got = match wait {
            Some(w) => match tokio::time::timeout(w, self.rx.recv()).await {
                Ok(g) => g,
                Err(_) => return Got::Timeout,
            },
            None => self.rx.recv().await,
        };
        match got {
            Some(Incoming::Frame(f)) => Got::Frame(f),
            Some(Incoming::End(e)) => {
                self.ended = Some(e);
                Got::End(e)
            }
            None => {
                self.ended = Some(LinkEnd::Network);
                Got::End(LinkEnd::Network)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkKind {
    WebSocket,
    Poll,
}

pub(crate) enum Link {
    Ws(ws::WsLink),
    Poll(poll::PollLink),
}

impl Link {
    pub(crate) fn kind(&self) -> LinkKind {
        match self {
            Link::Ws(_) => LinkKind::WebSocket,
            Link::Poll(_) => LinkKind::Poll,
        }
    }

    /// Resolves once the frame has left; fails if the link ended first.
    pub(crate) async fn send(&self, frame: Vec<u8>) -> Result<(), ()> {
        match self {
            Link::Ws(l) => l.send(frame).await,
            Link::Poll(l) => l.send(frame).await,
        }
    }

    /// Long-poll only: the session or pairing id to ask the computer's frames for.
    pub(crate) fn set_sid(&self, sid: &[u8]) {
        if let Link::Poll(l) = self {
            l.set_sid(sid);
        }
    }

    pub(crate) fn close(&self) {
        match self {
            Link::Ws(l) => l.close(),
            Link::Poll(l) => l.close(),
        }
    }
}

impl Drop for Link {
    fn drop(&mut self) {
        self.close();
    }
}

/// A WebSocket when one opens within `socket_timeout`, otherwise long-poll.
pub(crate) async fn open(
    route: &Route,
    http: &reqwest::Client,
    prefer_poll: bool,
    socket_timeout: Duration,
) -> (Link, Inbox) {
    let (tx, inbox) = Inbox::new();
    if !prefer_poll
        && let Ok(Ok(l)) =
            tokio::time::timeout(socket_timeout, ws::WsLink::open(route, tx.clone())).await
    {
        return (Link::Ws(l), inbox);
    }
    (
        Link::Poll(poll::PollLink::new(route, http.clone(), tx)),
        inbox,
    )
}

/// A run of frames, each as a 4-byte big-endian length then the frame.
pub(crate) fn frame_run(frames: &[&[u8]]) -> Vec<u8> {
    let mut out = Vec::new();
    for f in frames {
        out.extend_from_slice(&(f.len() as u32).to_be_bytes());
        out.extend_from_slice(f);
    }
    out
}

pub(crate) fn parse_run(body: &[u8]) -> Option<Vec<Vec<u8>>> {
    let mut frames = Vec::new();
    let mut at = 0;
    while at < body.len() {
        let len: [u8; 4] = body.get(at..at + 4)?.try_into().ok()?;
        let n = u32::from_be_bytes(len) as usize;
        at += 4;
        if n > MAX_FRAME {
            return None;
        }
        frames.push(body.get(at..at + n)?.to_vec());
        at += n;
    }
    Some(frames)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_round_trip() {
        let a = vec![1u8; 10];
        let b = vec![2u8; 300];
        let run = frame_run(&[&a, &b]);
        assert_eq!(parse_run(&run).unwrap(), vec![a, b]);
        assert!(parse_run(&run[..run.len() - 1]).is_none());
        assert_eq!(parse_run(&[]).unwrap(), Vec::<Vec<u8>>::new());
    }

    #[test]
    fn routes_are_https() {
        let r = Route::https("Amber-Otter-Lamp-Quiet.trycloudflare.com").unwrap();
        assert_eq!(r.base(), "https://amber-otter-lamp-quiet.trycloudflare.com");
        assert_eq!(
            r.ws_url(),
            "wss://amber-otter-lamp-quiet.trycloudflare.com/s"
        );
        assert!(Route::https("127.0.0.1:8080").is_none());
        assert!(Route::https("localhost").is_none());
        assert!(https_router()("http://x.y").is_none());
    }
}
