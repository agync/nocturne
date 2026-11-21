//! The phone's client: pairing over the wire, and [`Phone`], which opens a session when one is
//! needed, renews it before it lapses, and keeps a connection state the screens read. A call is
//! tried once more on a new session after a transport error; a keep carries its client id, so
//! the computer keeps it once.
//!
//! Only `https` addresses are reached. The `dev-loopback` feature adds `http://127.0.0.1` for
//! tests and `adb reverse`.

mod link;
mod pair;
mod poll;
mod session;
mod ws;

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, SystemTime};

use nocturne_contract::{
    AskDelta, AskPhase, AskPlace, PhoneError, PhoneHello, RemoteAnswer, RemoteCall,
};
use serde::{Deserialize, Serialize};
use tokio::sync::watch;

#[cfg(feature = "dev-loopback")]
pub use link::{DEV_LOOPBACK_MARKER, loopback_router};
pub use link::{LinkKind, Route, Router, http_client, https_router};
pub use pair::{PairFailure, check_address, pair, take_hello};

/// The HTTP client the link and the address check use.
pub type HttpClient = reqwest::Client;

use crate::frame::MAX_CHUNK_PAYLOAD;
use crate::phone::PhoneIdentity;
use crate::protocol::*;
use session::{Down, Fault, OpenFailure, OpenInit, Session};

/// Where the phone stands with its computer, as the screens show it. `Removed` is only ever the
/// result of a sealed revocation; every unsealed failure leaves the keys alone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Connection {
    Connecting,
    Connected { hello: PhoneHello },
    Asleep,
    CantConnect,
    TurnedOff,
    Outdated { min_build: u32 },
    Removed,
}

/// Why a path to the computer is not open, for a call that failed on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Reach {
    Asleep,
    CantConnect,
    TurnedOff,
    Outdated,
    Removed,
}

impl Reach {
    fn connection(self, min_build: u32) -> Connection {
        match self {
            Reach::Asleep => Connection::Asleep,
            Reach::CantConnect => Connection::CantConnect,
            Reach::TurnedOff => Connection::TurnedOff,
            Reach::Outdated => Connection::Outdated { min_build },
            Reach::Removed => Connection::Removed,
        }
    }
}

/// A call that did not get its answer: the computer said no, or it could not be reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum CallError {
    Phone { code: PhoneError },
    Connection { state: Reach },
}

impl std::fmt::Display for CallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for CallError {}

fn invalid() -> CallError {
    CallError::Phone {
        code: PhoneError::Invalid,
    }
}

fn gone(state: Reach) -> CallError {
    CallError::Connection { state }
}

#[derive(Debug, Clone)]
pub struct Options {
    /// Skip the WebSocket and use long-poll from the start.
    pub prefer_poll: bool,
    pub socket_timeout: Duration,
    /// The wait for the computer's answers while opening.
    pub handshake: Duration,
    /// Open a replacement session this long before the old one lapses.
    pub renew_margin: Duration,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            prefer_poll: false,
            socket_timeout: Duration::from_secs(5),
            handshake: Duration::from_secs(HANDSHAKE_TIMEOUT_SECS),
            renew_margin: Duration::from_secs(60),
        }
    }
}

/// Told each time the identity changes (a new address) and with `None` once it is forgotten (the
/// phone was removed, or disconnected). The app keeps it.
pub type OnIdentity = Arc<dyn Fn(Option<&PhoneIdentity>) + Send + Sync>;

struct Inner {
    identity: Mutex<Option<PhoneIdentity>>,
    router: Router,
    http: reqwest::Client,
    options: Options,
    on_identity: OnIdentity,
    state: watch::Sender<Connection>,
    session: tokio::sync::Mutex<Option<Arc<Session>>>,
    sticky_poll: AtomicBool,
    closed: AtomicBool,
    upload: tokio::sync::Mutex<()>,
    /// Counts opens; a session that ends says connecting only while it is the latest.
    opens: AtomicU64,
}

/// A paired phone's way to its computer.
#[derive(Clone)]
pub struct Phone {
    inner: Arc<Inner>,
}

const TERMINAL: [AskPhase; 4] = [
    AskPhase::Done,
    AskPhase::NotSure,
    AskPhase::Nothing,
    AskPhase::Unavailable,
];

impl Phone {
    pub fn new(
        identity: PhoneIdentity,
        router: Router,
        options: Options,
        on_identity: OnIdentity,
    ) -> Phone {
        let (state, _) = watch::channel(Connection::Connecting);
        Phone {
            inner: Arc::new(Inner {
                identity: Mutex::new(Some(identity)),
                router,
                http: http_client(),
                sticky_poll: AtomicBool::new(options.prefer_poll),
                options,
                on_identity,
                state,
                session: tokio::sync::Mutex::new(None),
                closed: AtomicBool::new(false),
                upload: tokio::sync::Mutex::new(()),
                opens: AtomicU64::new(0),
            }),
        }
    }

    /// The identity as it stands, or `None` once forgotten.
    pub fn identity(&self) -> Option<PhoneIdentity> {
        self.inner.identity.lock().unwrap().clone()
    }

    /// The connection state, as it changes.
    pub fn connection(&self) -> watch::Receiver<Connection> {
        self.inner.state.subscribe()
    }

    pub fn state(&self) -> Connection {
        self.inner.state.borrow().clone()
    }

    fn set(&self, next: Connection) {
        self.inner.state.send_if_modified(|cur| {
            let same = *cur == next && !matches!(next, Connection::Connected { .. });
            if !same {
                *cur = next;
            }
            !same
        });
    }

    fn set_if_connected(&self, next: Connection) {
        self.inner.state.send_if_modified(|cur| {
            let was = matches!(cur, Connection::Connected { .. });
            if was {
                *cur = next;
            }
            was
        });
    }

    fn forget(&self) {
        *self.inner.identity.lock().unwrap() = None;
        (self.inner.on_identity)(None);
        self.set(Connection::Removed);
    }

    /// Open a session now, or find out why not.
    pub async fn connect(&self) -> Connection {
        let _ = self.ensure().await;
        self.state()
    }

    /// Close the session. Keys stay; nothing opens again.
    pub async fn close(&self) {
        self.inner.closed.store(true, Ordering::SeqCst);
        if let Some(s) = self.inner.session.lock().await.take() {
            s.close();
        }
    }

    async fn ensure(&self) -> Result<Arc<Session>, CallError> {
        if self.inner.closed.load(Ordering::SeqCst) {
            return Err(gone(Reach::Asleep));
        }
        let mut slot = self.inner.session.lock().await;
        if let Some(s) = slot.as_ref()
            && s.is_open()
            && SystemTime::now() + self.inner.options.renew_margin < s.expires_at()
        {
            return Ok(s.clone());
        }
        let opened = Arc::new(self.open_new().await?);
        // The old session is retired, not closed: calls still on it finish there, and it closes
        // when the last of them lets it go.
        drop(slot.replace(opened.clone()));
        Ok(opened)
    }

    async fn open_at(&self, address: &str) -> Result<Session, CallError> {
        let Some(identity) = self.identity() else {
            self.set(Connection::CantConnect);
            return Err(gone(Reach::CantConnect));
        };
        let Some(route) = (self.inner.router)(address) else {
            self.set(Connection::CantConnect);
            return Err(gone(Reach::CantConnect));
        };
        if !matches!(self.state(), Connection::Connected { .. }) {
            self.set(Connection::Connecting);
        }
        let init = OpenInit {
            identity: &identity,
            route: &route,
            http: &self.inner.http,
            prefer_poll: self.inner.sticky_poll.load(Ordering::SeqCst),
            socket_timeout: self.inner.options.socket_timeout,
            handshake: self.inner.options.handshake,
        };
        let open_no = self.inner.opens.fetch_add(1, Ordering::SeqCst) + 1;
        let weak: Weak<Inner> = Arc::downgrade(&self.inner);
        let on_end: session::OnEnd = Arc::new(move || {
            let Some(inner) = weak.upgrade() else { return };
            let latest = inner.opens.load(Ordering::SeqCst) == open_no;
            if latest && !inner.closed.load(Ordering::SeqCst) {
                Phone { inner }.set_if_connected(Connection::Connecting);
            }
        });
        // A session that ended under the open (410) is opened again, once.
        let opened = match Session::open(&init, on_end.clone()).await {
            Err(e) if e.failure == OpenFailure::Ended => Session::open(&init, on_end).await,
            other => other,
        };
        let s = match opened {
            Ok(s) => s,
            Err(e) => {
                let reach = match e.failure {
                    OpenFailure::Revoked => {
                        self.forget();
                        return Err(gone(Reach::Removed));
                    }
                    OpenFailure::Refused | OpenFailure::Protocol => Reach::CantConnect,
                    OpenFailure::Off => Reach::TurnedOff,
                    OpenFailure::Network | OpenFailure::Ended => Reach::Asleep,
                };
                self.set(reach.connection(0));
                return Err(gone(reach));
            }
        };
        if s.hello.min_phone_build > PHONE_BUILD {
            // This app is older than the computer wants: keys stay, nothing is saved.
            s.close();
            self.set(Connection::Outdated {
                min_build: s.hello.min_phone_build,
            });
            return Err(gone(Reach::Outdated));
        }
        if s.kind() == LinkKind::Poll {
            self.inner.sticky_poll.store(true, Ordering::SeqCst);
        }
        // The address is kept only now: a session to it has opened and Hello has been read.
        let changed = {
            let mut id = self.inner.identity.lock().unwrap();
            match id.as_mut() {
                Some(i) if i.address != address || i.device_name != s.hello.device_name => {
                    i.address = address.to_string();
                    i.device_name = s.hello.device_name.clone();
                    Some(i.clone())
                }
                _ => None,
            }
        };
        if self.identity().is_none() {
            s.close();
            self.set(Connection::CantConnect);
            return Err(gone(Reach::CantConnect));
        }
        if let Some(i) = changed {
            (self.inner.on_identity)(Some(&i));
        }
        self.set(Connection::Connected {
            hello: s.hello.clone(),
        });
        Ok(s)
    }

    async fn open_new(&self) -> Result<Session, CallError> {
        let address = self
            .identity()
            .map(|i| i.address)
            .ok_or(gone(Reach::CantConnect))?;
        self.open_at(&address).await
    }

    /// What a failed attempt means: `Ok` to try once more on a new session, or the error.
    async fn after(&self, s: &Arc<Session>, fault: Fault, again: bool) -> Result<(), CallError> {
        match fault {
            Fault::Phone(PhoneError::Revoked) => {
                self.forget();
                Err(gone(Reach::Removed))
            }
            _ if s.revoked() => {
                self.forget();
                Err(gone(Reach::Removed))
            }
            Fault::Phone(PhoneError::SessionExpired) | Fault::Closed => {
                s.close();
                let mut slot = self.inner.session.lock().await;
                if slot.as_ref().is_some_and(|c| Arc::ptr_eq(c, s)) {
                    *slot = None;
                }
                drop(slot);
                if again {
                    return Ok(());
                }
                if !self.inner.closed.load(Ordering::SeqCst) {
                    self.set(Connection::Connecting);
                }
                Err(gone(Reach::Asleep))
            }
            Fault::Phone(code) => Err(CallError::Phone { code }),
        }
    }

    /// A call answered once: everything but `ask`, `getOriginal` and the keeps with bytes.
    pub async fn call(&self, call: RemoteCall) -> Result<RemoteAnswer, CallError> {
        if matches!(
            call,
            RemoteCall::Ask { .. }
                | RemoteCall::GetOriginal { .. }
                | RemoteCall::KeepBytes { .. }
                | RemoteCall::KeepRecording { .. }
        ) {
            return Err(invalid());
        }
        for again in [true, false] {
            let s = self.ensure().await?;
            match call_once(&s, call.clone()).await {
                Ok(a) => return Ok(a),
                Err(f) => self.after(&s, f, again).await?,
            }
        }
        Err(gone(Reach::Asleep))
    }

    /// Asks a question; each delta goes to `on_delta`. Never sent twice. Answers the ask id.
    /// `place` is `ThisComputer` when the person chose to have it answered on the computer.
    pub async fn ask(
        &self,
        question: &str,
        place: Option<AskPlace>,
        mut on_delta: impl FnMut(AskDelta) + Send,
    ) -> Result<String, CallError> {
        if question.trim().is_empty() || question.len() > MAX_QUESTION {
            return Err(invalid());
        }
        let s = self.ensure().await?;
        match ask_once(&s, question, place, &mut on_delta).await {
            Ok(id) => Ok(id),
            Err(f) => {
                self.after(&s, f, false).await?;
                Err(gone(Reach::Asleep))
            }
        }
    }

    /// A keep with bytes (`keepBytes`, `keepRecording`): the call, then the bytes in chunks. One
    /// upload at a time.
    pub async fn upload(&self, call: RemoteCall, bytes: &[u8]) -> Result<RemoteAnswer, CallError> {
        let size = match &call {
            RemoteCall::KeepBytes { size, .. } | RemoteCall::KeepRecording { size, .. } => *size,
            _ => return Err(invalid()),
        };
        if bytes.is_empty() || size != bytes.len() as f64 || bytes.len() as u64 > MAX_ITEM {
            return Err(invalid());
        }
        let _one = self.inner.upload.lock().await;
        for again in [true, false] {
            let s = self.ensure().await?;
            match upload_once(&s, call.clone(), bytes).await {
                Ok(a) => return Ok(a),
                Err(f) => self.after(&s, f, again).await?,
            }
        }
        Err(gone(Reach::Asleep))
    }

    /// An item's original: its type, from the allowlist, and its bytes.
    pub async fn original(&self, id: &str) -> Result<(String, Vec<u8>), CallError> {
        if crate::crypto::check_id(id).is_err() {
            return Err(invalid());
        }
        for again in [true, false] {
            let s = self.ensure().await?;
            match original_once(&s, id).await {
                Ok(v) => return Ok(v),
                Err(f) => self.after(&s, f, again).await?,
            }
        }
        Err(gone(Reach::Asleep))
    }

    /// The computer has a new address: open a session there with the keys this phone has. The
    /// address is kept only once Hello opens; nothing is approved.
    pub async fn relink(&self, address: &str) -> Result<(), CallError> {
        let address = crate::phone::check_host(address).ok_or(gone(Reach::CantConnect))?;
        let mut slot = self.inner.session.lock().await;
        let s = Arc::new(self.open_at(&address).await?);
        drop(slot.replace(s));
        Ok(())
    }

    /// Tells the computer, then forgets this phone. `false` when the computer could not be told.
    pub async fn disconnect(&self) -> bool {
        let reached = match self.call(RemoteCall::Disconnect).await {
            Ok(_) => true,
            // A phone the computer already removed has nothing left to tell it.
            Err(CallError::Connection {
                state: Reach::Removed,
            }) => true,
            Err(_) => false,
        };
        self.close().await;
        *self.inner.identity.lock().unwrap() = None;
        (self.inner.on_identity)(None);
        self.set(Connection::CantConnect);
        reached
    }
}

async fn call_once(s: &Session, call: RemoteCall) -> Result<RemoteAnswer, Fault> {
    let (id, mut rx) = s.request(call).await?;
    let out = session::next(&mut rx).await;
    s.finish(id);
    match out? {
        Down::Answer(a) => Ok(a),
        Down::Chunk(_) => Err(Fault::Closed),
    }
}

async fn ask_once(
    s: &Session,
    question: &str,
    place: Option<AskPlace>,
    on_delta: &mut (impl FnMut(AskDelta) + Send),
) -> Result<String, Fault> {
    let call = RemoteCall::Ask {
        question: question.to_string(),
        place,
    };
    let (id, mut rx) = s.request(call).await?;
    let mut ask_id = String::new();
    let out = loop {
        match session::next(&mut rx).await {
            Ok(Down::Answer(RemoteAnswer::AskStarted { ask_id: a })) => ask_id = a,
            Ok(Down::Answer(RemoteAnswer::AskDelta { delta })) => {
                let done = TERMINAL.contains(&delta.phase);
                let from_delta = delta.ask_id.clone();
                on_delta(delta);
                if done {
                    break Ok(if ask_id.is_empty() {
                        from_delta
                    } else {
                        ask_id
                    });
                }
            }
            Ok(_) => {}
            Err(e) => break Err(e),
        }
    };
    s.finish(id);
    out
}

async fn upload_once(s: &Session, call: RemoteCall, bytes: &[u8]) -> Result<RemoteAnswer, Fault> {
    let (id, mut rx) = s.request(call).await?;
    let parts: Vec<&[u8]> = bytes.chunks(CHUNK.min(MAX_CHUNK_PAYLOAD)).collect();
    for (seq, part) in parts.iter().enumerate() {
        // An answer before the last chunk (a refusal) ends the upload.
        if !rx.is_empty() {
            break;
        }
        s.chunk(id, seq as u32, seq + 1 == parts.len(), part)
            .await?;
    }
    let out = session::next(&mut rx).await;
    s.finish(id);
    match out? {
        Down::Answer(a) => Ok(a),
        Down::Chunk(_) => Err(Fault::Closed),
    }
}

async fn original_once(s: &Session, id: &str) -> Result<(String, Vec<u8>), Fault> {
    let bad = Fault::Phone(PhoneError::Invalid);
    let (rid, mut rx) = s
        .request(RemoteCall::GetOriginal { id: id.to_string() })
        .await?;
    let mut meta: Option<(String, usize)> = None;
    let mut bytes = Vec::new();
    let mut seq = 0;
    let out = loop {
        match session::next(&mut rx).await {
            Ok(Down::Answer(RemoteAnswer::OriginalStart { mime, size })) => {
                let fine = meta.is_none()
                    && (1.0..=MAX_ITEM as f64).contains(&size)
                    && size.fract() == 0.0;
                if !fine {
                    break Err(bad);
                }
                meta = Some((mime, size as usize));
            }
            Ok(Down::Chunk(c)) => {
                let Some((mime, size)) = meta.as_ref() else {
                    break Err(bad);
                };
                if c.seq != seq {
                    break Err(bad);
                }
                seq += 1;
                if bytes.is_empty() && crate::crypto::magic_matches(mime, &c.bytes).is_err() {
                    break Err(bad);
                }
                bytes.extend_from_slice(&c.bytes);
                if bytes.len() > *size {
                    break Err(bad);
                }
                if c.last {
                    break if bytes.len() == *size {
                        Ok((mime.clone(), std::mem::take(&mut bytes)))
                    } else {
                        Err(bad)
                    };
                }
            }
            Ok(_) => {}
            Err(e) => break Err(e),
        }
    };
    s.finish(rid);
    // A bad original leaves chunks in flight on this session: it is not used again.
    if out.is_err() && meta.is_some() {
        s.close();
    }
    out
}
