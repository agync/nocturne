//! The phone's client: pairing over the wire, and [`Phone`], which opens a session when one is
//! needed, renews it before it lapses, and keeps a connection state the screens read. A call is
//! tried once more on a new session after a transport error; a keep carries its client id, so
//! the computer keeps it once.
//!
//! Only `https` addresses are reached. The `dev-loopback` feature adds `http://127.0.0.1` for
//! tests and `adb reverse`.

mod link;
mod pair;
#[cfg(test)]
mod pipe;
#[cfg(test)]
mod pipe_tests;
mod poll;
mod session;
#[cfg(all(test, feature = "dev-loopback"))]
mod socket_tests;
mod ws;

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, SystemTime};

use nocturne_contract::{
    AskDelta, AskPhase, AskPlace, PhoneAppIntro, PhoneError, PhoneHello, QuestionKind,
    RemoteAnswer, RemoteCall,
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
use session::{Down, Fault, OpenFailure, OpenInit, Session, Waits};

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
    /// `agreed`: the newest phone build both sides speak; calls and answers keep to it.
    Connected {
        hello: PhoneHello,
        agreed: u32,
    },
    Asleep,
    CantConnect,
    TurnedOff,
    /// This app is older than the oldest build the computer speaks: update the phone.
    /// `computer` is the computer's version, when it said.
    Outdated {
        min_build: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        computer: Option<String>,
    },
    /// The computer is older than the oldest build this app speaks: update the computer.
    ComputerOutdated {
        computer: String,
    },
    Removed,
}

/// What a phone and its computer work out from the computer's hello and the builds the phone
/// speaks: the newest build both speak, or which side must update.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overlap {
    Agreed(u32),
    UpdatePhone,
    UpdateComputer,
}

/// The oldest build that has `call` as it is: a newer call, or a newer field of an older one,
/// is never sent to a computer that would not read it as meant.
pub fn needs_build(call: &RemoteCall) -> u32 {
    match call {
        RemoteCall::Introduce { .. } => INTRODUCE_BUILD,
        RemoteCall::AnswerJournalQuestion { .. } => JOURNAL_ANSWER_BUILD,
        RemoteCall::KeepText { web: true, .. } => WEB_KEEP_BUILD,
        RemoteCall::OfferMemory { .. } | RemoteCall::MemoryDone { .. } => MEMORY_MOVE_BUILD,
        // The sends are offered only to a computer whose hello lists them.
        _ => MIN_PHONE_BUILD,
    }
}

/// An answer as the agreed build has it: below the build that answers the page's question from
/// the journal, the page shows no such question.
pub fn within(answer: RemoteAnswer, agreed: u32) -> RemoteAnswer {
    match answer {
        RemoteAnswer::Page { mut page }
            if agreed < JOURNAL_ANSWER_BUILD
                && page
                    .question
                    .as_ref()
                    .is_some_and(|q| q.kind == QuestionKind::Journal) =>
        {
            page.question = None;
            RemoteAnswer::Page { page }
        }
        other => other,
    }
}

/// The overlap of the builds the computer says it speaks and `min_build..=max_build`. A computer
/// from before the version window speaks its oldest build only.
pub fn overlap(hello: &PhoneHello, min_build: u32, max_build: u32) -> Overlap {
    let agreed = max_build.min(hello.newest_phone_build());
    if max_build < hello.min_phone_build {
        Overlap::UpdatePhone
    } else if agreed < min_build {
        Overlap::UpdateComputer
    } else {
        Overlap::Agreed(agreed)
    }
}

/// Why a path to the computer is not open, for a call that failed on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Reach {
    Asleep,
    CantConnect,
    TurnedOff,
    Outdated,
    ComputerOutdated,
    Removed,
}

impl Reach {
    fn connection(self) -> Connection {
        match self {
            Reach::Asleep => Connection::Asleep,
            Reach::CantConnect => Connection::CantConnect,
            Reach::TurnedOff => Connection::TurnedOff,
            Reach::Outdated => Connection::Outdated {
                min_build: 0,
                computer: None,
            },
            Reach::ComputerOutdated => Connection::ComputerOutdated {
                computer: String::new(),
            },
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
    /// How long an open session may hear nothing from the computer (no frame, no ping, no empty
    /// poll) before it counts as gone: a socket that died half way behind a tunnel says nothing.
    pub quiet: Duration,
    /// How long a call waits for its answer, or for each next part of one, before its session is
    /// left and the call goes again. Calls the computer holds longer on purpose wait longer.
    pub answer_wait: Duration,
    /// How long each frame up (a call, or a chunk of an upload) may take to leave; each frame
    /// that leaves starts it again.
    pub chunk_wait: Duration,
    /// What a phone app says of itself first on each session. None: it says nothing, and speaks
    /// the builds of this tree.
    pub app: Option<PhoneAppIntro>,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            prefer_poll: false,
            socket_timeout: Duration::from_secs(5),
            handshake: Duration::from_secs(HANDSHAKE_TIMEOUT_SECS),
            renew_margin: Duration::from_secs(60),
            // Three of the listener's 30-second pings, or a long poll and a half.
            quiet: Duration::from_secs(90),
            answer_wait: Duration::from_secs(120),
            chunk_wait: Duration::from_secs(60),
            app: None,
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
    /// The newest build this phone and its computer both speak, from the last open.
    agreed: AtomicU32,
}

/// A paired phone's way to its computer.
#[derive(Clone)]
pub struct Phone {
    inner: Arc<Inner>,
}

/// The computer asks its person about an offered memory for up to ten minutes.
const OFFER_WAIT: Duration = Duration::from_secs(11 * 60);
/// A moved memory is checked after any backup running there, then unpacked whole.
const CHECK_WAIT: Duration = Duration::from_secs(30 * 60);
/// A question's next part, or a look back being written, can wait on a slow mind reading a long
/// context.
const ASK_WAIT: Duration = Duration::from_secs(5 * 60);

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
                agreed: AtomicU32::new(0),
            }),
        }
    }

    /// The identity as it stands, or `None` once forgotten.
    pub fn identity(&self) -> Option<PhoneIdentity> {
        self.inner.identity.lock().unwrap().clone()
    }

    /// The newest phone build this phone and its computer both speak; 0 before a session opens.
    pub fn agreed_build(&self) -> u32 {
        self.inner.agreed.load(Ordering::SeqCst)
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
            quiet: self.inner.options.quiet,
            waits: Waits {
                answer: self.inner.options.answer_wait,
                chunk: self.inner.options.chunk_wait,
            },
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
                self.set(reach.connection());
                return Err(gone(reach));
            }
        };
        // Keys stay and nothing is saved while one side must update.
        let agreed = self.agree(&s).await?;
        self.inner.agreed.store(agreed, Ordering::SeqCst);
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
            agreed,
        });
        Ok(s)
    }

    /// Work out the build to speak on a session just opened: from the hello, and from the
    /// computer's answer when this app introduces itself. Closes the session when one side
    /// must update.
    async fn agree(&self, s: &Session) -> Result<u32, CallError> {
        let app = self.inner.options.app.clone();
        let (min, max) = app.as_ref().map_or((MIN_PHONE_BUILD, PHONE_BUILD), |a| {
            (a.min_build, a.max_build)
        });
        let mut found = overlap(&s.hello, min, max);
        // A computer that speaks introductions hears this app's, even when one side must
        // update: it notes it in its phones list or its activity list.
        if let Some(app) = app
            && s.hello.newest_phone_build() >= INTRODUCE_BUILD
        {
            match call_once(s, RemoteCall::Introduce { app }).await {
                Ok(RemoteAnswer::Introduced { agreed }) => found = Overlap::Agreed(agreed),
                Err(Fault::Phone(PhoneError::TooOld)) => found = Overlap::UpdatePhone,
                Err(Fault::Phone(PhoneError::ComputerOutdated)) => found = Overlap::UpdateComputer,
                Err(Fault::Phone(PhoneError::Revoked)) => {
                    self.forget();
                    return Err(gone(Reach::Removed));
                }
                // A session that does not answer its first call is no use either.
                Err(Fault::Closed | Fault::TimedOut) => {
                    s.close();
                    if s.revoked() {
                        self.forget();
                        return Err(gone(Reach::Removed));
                    }
                    self.set(Connection::Asleep);
                    return Err(gone(Reach::Asleep));
                }
                // Any other answer leaves what the hello said.
                Ok(_) | Err(Fault::Phone(_)) => {}
            }
        }
        let computer = s.hello.version.clone();
        match found {
            Overlap::Agreed(agreed) => Ok(agreed),
            Overlap::UpdatePhone => {
                s.close();
                self.set(Connection::Outdated {
                    min_build: s.hello.min_phone_build,
                    computer: Some(computer),
                });
                Err(gone(Reach::Outdated))
            }
            Overlap::UpdateComputer => {
                s.close();
                self.set(Connection::ComputerOutdated { computer });
                Err(gone(Reach::ComputerOutdated))
            }
        }
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
            // The session still hears the computer: nothing is sent again, and it stays open.
            Fault::TimedOut => Err(gone(Reach::Asleep)),
        }
    }

    /// A call answered once: everything but `ask`, `getOriginal`, the keeps with bytes and the
    /// memory's upload.
    pub async fn call(&self, call: RemoteCall) -> Result<RemoteAnswer, CallError> {
        if matches!(
            call,
            RemoteCall::Ask { .. }
                | RemoteCall::GetOriginal { .. }
                | RemoteCall::KeepBytes { .. }
                | RemoteCall::KeepRecording { .. }
                | RemoteCall::MemoryDone { .. }
        ) {
            return Err(invalid());
        }
        for again in [true, false] {
            let s = self.ensure().await?;
            let agreed = self.agreed_build();
            if needs_build(&call) > agreed {
                // It waits on the phone until the computer speaks a build that has it.
                return Err(CallError::Phone {
                    code: PhoneError::ComputerOutdated,
                });
            }
            match call_once(&s, call.clone()).await {
                Ok(a) => return Ok(within(a, agreed)),
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

    /// A memory moving to the computer (`memoryDone`): the call, then the backup file at `file`
    /// in chunks, read as they go and never held whole. `sent` hears the bytes gone so far. It
    /// waits its turn behind any other upload, and is not tried again on a new session: the
    /// computer may be checking what arrived, so the phone offers its memory again instead.
    pub async fn upload_memory(
        &self,
        call: RemoteCall,
        file: &Path,
        sent: impl Fn(u64) + Send + Sync,
    ) -> Result<RemoteAnswer, CallError> {
        let RemoteCall::MemoryDone { size, .. } = &call else {
            return Err(invalid());
        };
        let len = std::fs::metadata(file).map_err(|_| invalid())?.len();
        if len == 0 || *size != len as f64 || len > MAX_MEMORY_BYTES {
            return Err(invalid());
        }
        let source = std::fs::File::open(file).map_err(|_| invalid())?;
        let _one = self.inner.upload.lock().await;
        let s = self.ensure().await?;
        if needs_build(&call) > self.agreed_build() {
            return Err(CallError::Phone {
                code: PhoneError::ComputerOutdated,
            });
        }
        match memory_once(&s, call, source, len, &sent).await {
            Ok(a) => Ok(a),
            Err(f) => {
                self.after(&s, f, false).await?;
                Err(gone(Reach::Asleep))
            }
        }
    }

    /// Where a memory sent up stands on the computer, asked without sending anything again: a
    /// `memoryDone` with nothing to come. Answered `MemoryArrived` once it opened there.
    pub async fn memory_status(&self) -> Result<RemoteAnswer, CallError> {
        let call = RemoteCall::MemoryDone {
            size: 0.0,
            sha256: String::new(),
            passphrase: String::new(),
        };
        for again in [true, false] {
            let s = self.ensure().await?;
            if needs_build(&call) > self.agreed_build() {
                return Err(CallError::Phone {
                    code: PhoneError::ComputerOutdated,
                });
            }
            match call_once(&s, call.clone()).await {
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

/// How long the computer may take over `call` on purpose: the person deciding on an offer, or
/// a moved memory being checked; the usual wait for everything else.
fn answer_wait(call: &RemoteCall, waits: Waits) -> Duration {
    match call {
        RemoteCall::OfferMemory { .. } => waits.answer.max(OFFER_WAIT),
        RemoteCall::MemoryDone { .. } => waits.answer.max(CHECK_WAIT),
        RemoteCall::Ask { .. } | RemoteCall::LookBack { .. } => waits.answer.max(ASK_WAIT),
        _ => waits.answer,
    }
}

async fn call_once(s: &Session, call: RemoteCall) -> Result<RemoteAnswer, Fault> {
    let wait = answer_wait(&call, s.waits);
    let (id, mut rx) = s.request(call).await?;
    let out = s.next_within(id, &mut rx, wait).await;
    s.finish(id);
    match out? {
        Down::Answer(a) => Ok(a),
        Down::Chunk(_) => Err(Fault::Closed),
        Down::Unreadable => Err(Fault::Phone(PhoneError::Unknown)),
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
    let wait = answer_wait(&call, s.waits);
    let (id, mut rx) = s.request(call).await?;
    let mut ask_id = String::new();
    let out = loop {
        match s.next_within(id, &mut rx, wait).await {
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
    let out = s.next_within(id, &mut rx, s.waits.answer).await;
    s.finish(id);
    match out? {
        Down::Answer(a) => Ok(a),
        Down::Chunk(_) => Err(Fault::Closed),
        Down::Unreadable => Err(Fault::Phone(PhoneError::Unknown)),
    }
}

async fn memory_once(
    s: &Session,
    call: RemoteCall,
    mut source: std::fs::File,
    len: u64,
    sent: &(impl Fn(u64) + Send + Sync),
) -> Result<RemoteAnswer, Fault> {
    use std::io::Read;
    let wait = answer_wait(&call, s.waits);
    let (id, mut rx) = s.request(call).await?;
    let step = CHUNK.min(MAX_CHUNK_PAYLOAD);
    let mut buf = vec![0u8; step];
    let (mut done, mut seq) = (0u64, 0u32);
    let mut failed = None;
    while done < len {
        // An answer before the last chunk (a refusal) ends the upload.
        if !rx.is_empty() {
            break;
        }
        let want = step.min((len - done) as usize);
        if source.read_exact(&mut buf[..want]).is_err() {
            // The file changed under the move: the computer drops what it has.
            failed = Some(Fault::Phone(PhoneError::Invalid));
            break;
        }
        done += want as u64;
        if let Err(f) = s.chunk(id, seq, done == len, &buf[..want]).await {
            failed = Some(f);
            break;
        }
        seq += 1;
        sent(done);
    }
    let out = match failed {
        Some(f) => Err(f),
        None => s.next_within(id, &mut rx, wait).await,
    };
    s.finish(id);
    match out? {
        Down::Answer(a) => Ok(a),
        Down::Chunk(_) => Err(Fault::Closed),
        Down::Unreadable => Err(Fault::Phone(PhoneError::Unknown)),
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
        match s.next_within(rid, &mut rx, s.waits.answer).await {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn hello(json: &str) -> PhoneHello {
        serde_json::from_str(&format!(
            r#"{{"computerName":"Laptop","deviceName":"Pixel","connection":"simple",
                "version":"0.7.2",{json}}}"#
        ))
        .unwrap()
    }

    #[test]
    fn both_speak_the_newest_build_they_share() {
        let h = hello(r#""minPhoneBuild":1,"maxPhoneBuild":3"#);
        assert_eq!(overlap(&h, 1, 3), Overlap::Agreed(3));
        assert_eq!(
            overlap(&h, 1, 2),
            Overlap::Agreed(2),
            "a phone a build behind"
        );
        assert_eq!(
            overlap(&h, 3, 4),
            Overlap::Agreed(3),
            "a phone a build ahead"
        );
    }

    #[test]
    fn with_nothing_shared_the_older_side_updates() {
        let h = hello(r#""minPhoneBuild":2,"maxPhoneBuild":3"#);
        assert_eq!(overlap(&h, 1, 1), Overlap::UpdatePhone);
        assert_eq!(overlap(&h, 4, 5), Overlap::UpdateComputer);
    }

    #[test]
    fn a_computer_from_before_the_window_speaks_its_oldest_build_only() {
        let old = hello(r#""minPhoneBuild":1"#);
        assert_eq!(overlap(&old, 1, 2), Overlap::Agreed(1));
        assert_eq!(overlap(&old, 2, 2), Overlap::UpdateComputer);
        assert!(
            old.newest_phone_build() < INTRODUCE_BUILD,
            "it is not sent an introduction it cannot read"
        );
    }

    fn page_asking(kind: &str) -> RemoteAnswer {
        let page = serde_json::json!({
            "date": "2026-11-13", "pageNumber": 42, "edition": "phone", "variant": "normal",
            "headline": "", "dek": "", "needsYou": [], "changed": [], "letGo": [],
            "upcoming": [], "learned": [],
            "question": {"kind": kind, "text": "", "memoryId": null, "receipt": null,
                "lastAsked": null, "answered": null},
            "moreNeedsYou": 0, "moreChanged": 0, "keptCount": 0, "changesCount": 0,
            "letGoCount": 0, "catchUpDays": 0, "privateSkipped": 0, "pending": 0,
            "tidiedFrom": null, "tidiedTo": null, "update": null
        });
        RemoteAnswer::Page {
            page: Box::new(serde_json::from_value(page).unwrap()),
        }
    }

    fn asks(a: &RemoteAnswer) -> bool {
        matches!(a, RemoteAnswer::Page { page } if page.question.is_some())
    }

    #[test]
    fn a_call_newer_than_the_agreed_build_is_not_sent() {
        let keep = |web| RemoteCall::KeepText {
            text: "a page".into(),
            client_id: "c-1".into(),
            feeling: None,
            good: None,
            captured_at: None,
            web,
        };
        assert_eq!(needs_build(&keep(false)), MIN_PHONE_BUILD);
        assert_eq!(needs_build(&keep(true)), WEB_KEEP_BUILD);
        let offer = RemoteCall::OfferMemory {
            counts: Default::default(),
            bytes: 0.0,
            schema: 1,
        };
        let done = RemoteCall::MemoryDone {
            size: 0.0,
            sha256: String::new(),
            passphrase: String::new(),
        };
        assert_eq!(needs_build(&offer), MEMORY_MOVE_BUILD);
        assert_eq!(needs_build(&done), MEMORY_MOVE_BUILD);
        let answer = RemoteCall::AnswerJournalQuestion {
            page_date: "2026-11-13".into(),
            choice: nocturne_contract::QuestionChoice::Fine,
            line: None,
            client_id: "c-2".into(),
        };
        assert_eq!(needs_build(&answer), JOURNAL_ANSWER_BUILD);
        assert_eq!(needs_build(&RemoteCall::GetStatus), MIN_PHONE_BUILD);
    }

    #[test]
    fn below_the_journal_build_the_page_asks_no_journal_question() {
        assert!(!asks(&within(page_asking("journal"), 1)));
        assert!(asks(&within(page_asking("journal"), JOURNAL_ANSWER_BUILD)));
        assert!(
            asks(&within(page_asking("yesNo"), 1)),
            "other questions stay"
        );
    }

    #[test]
    fn the_states_for_the_update_cards_read_as_the_screens_expect() {
        let phone = Connection::Outdated {
            min_build: 2,
            computer: Some("0.9.0".into()),
        };
        assert_eq!(
            serde_json::to_value(&phone).unwrap(),
            serde_json::json!({"kind": "outdated", "minBuild": 2, "computer": "0.9.0"})
        );
        let computer = Connection::ComputerOutdated {
            computer: "0.7.2".into(),
        };
        assert_eq!(
            serde_json::to_value(&computer).unwrap(),
            serde_json::json!({"kind": "computerOutdated", "computer": "0.7.2"})
        );
    }
}
