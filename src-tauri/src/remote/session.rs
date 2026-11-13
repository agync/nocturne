//! Conversations and sessions: what arrives on `/s`, frame by frame.
//!
//! A conversation is one WebSocket, or one long-poll pair named by the phone. Its first frame
//! decides what it is: a pairing (HELLO, PAIR_COMMIT, PAIR_REVEAL) or a session (SESSION_OPEN).
//! A session is pending until its first good up frame: pending sessions expire after ten
//! seconds, count toward nothing and touch nothing. Live sessions last fifteen minutes; a phone
//! renews with a fresh SESSION_OPEN, which checks revocation again. Keys and counters live here
//! in memory only.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, RwLock};
use std::time::{Duration, Instant};

use jiff::Timestamp;
use nocturne_contract::{
    PairingRequest, PhoneError, PhoneHello, RemoteAction, RemoteAnswer, RemoteConnection,
    RemoteReply, RemoteRequest, RemoteResult,
};
use nocturne_engine::{DeviceRow, Engine};
use p256::SecretKey;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::crypto::{self, Envelope};
use super::frame::{Channel, Side};
use super::incoming::Incoming;
use super::limits::{Bucket, Limits, Mono};
use super::pairing::{ConvId, Pairings};
use super::protocol::*;

/// What the core needs from the app around it.
pub trait Shell: Send + Sync + 'static {
    fn engine(&self) -> Option<Engine>;
    fn wall_now(&self) -> Timestamp;
    /// Away from home's state changed: refresh the view (`remote-changed`).
    fn changed(&self);
    /// The person must decide on a phone (`pairing-request`); the main window comes forward.
    fn pairing_request(&self, request: &PairingRequest);
    /// A phone started a live session (`device-active`).
    fn device_active(&self, device: &DeviceRow);
    fn hello(&self, device: &DeviceRow, connection: RemoteConnection) -> PhoneHello;
    /// Stop everything a phone has running (its answers).
    fn stop_phone(&self, device_id: &str);
}

pub type BoxFuture<'a, T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

/// One call from a phone, with where its answers go and, for a keep of bytes, its upload.
pub struct CallCtx {
    pub device: DeviceRow,
    pub req: RemoteRequest,
    pub out: ReplySink,
    pub upload: Option<super::incoming::UploadDone>,
}

/// Answers a phone's calls: a typed match, never a forwarder (see `dispatch`).
pub trait Dispatch: Send + Sync + 'static {
    fn call(&self, core: Arc<Core>, ctx: CallCtx) -> BoxFuture<'static, ()>;
}

enum Role {
    Fresh,
    Pairing,
    Session(Box<Session>),
    Closed,
}

struct Session {
    device: DeviceRow,
    channel: Channel,
    live: bool,
    opened: Instant,
}

/// How a conversation reaches the phone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Carrier {
    /// A socket: it ends when the socket does, however quiet it is.
    WebSocket,
    /// Posts and held polls: it ends when the phone stops polling.
    LongPoll,
}

/// Bytes that may wait for the transport before a chunk going down waits for room: at most
/// about two frames of an original are in memory at once.
pub const OUT_BUDGET: usize = MAX_FRAME;

/// One conversation: the frames going out wait in `out` for the transport.
pub struct Conv {
    pub id: ConvId,
    pub carrier: Carrier,
    queued: AtomicUsize,
    drained: tokio::sync::Notify,
    out: mpsc::UnboundedSender<Vec<u8>>,
    pub rx: tokio::sync::Mutex<mpsc::UnboundedReceiver<Vec<u8>>>,
    pub closed: CancellationToken,
    role: Mutex<Role>,
    created: Instant,
    pub last_seen: Mutex<Instant>,
    pub down_busy: AtomicBool,
    /// Set when the conversation ended with the unsealed refusal.
    pub refused: AtomicBool,
}

impl Conv {
    fn role(&self) -> MutexGuard<'_, Role> {
        self.role.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn push(&self, frame: Vec<u8>) {
        let len = frame.len();
        if self.out.send(frame).is_ok() {
            self.queued.fetch_add(len, Ordering::SeqCst);
        }
    }

    /// The transport took a frame of `len` bytes.
    pub fn took(&self, len: usize) {
        let _ = self
            .queued
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |q| {
                Some(q.saturating_sub(len))
            });
        self.drained.notify_waiters();
    }

    /// Bytes waiting for the transport.
    pub fn queued(&self) -> usize {
        self.queued.load(Ordering::SeqCst)
    }

    /// Wait until fewer than `budget` bytes wait for the transport. False once closed.
    pub async fn room(&self, budget: usize) -> bool {
        loop {
            let drained = self.drained.notified();
            tokio::pin!(drained);
            drained.as_mut().enable();
            if self.closed.is_cancelled() {
                return false;
            }
            if self.queued() < budget {
                return true;
            }
            tokio::select! {
                () = drained => {}
                () = self.closed.cancelled() => return false,
            }
        }
    }

    pub fn close(&self) {
        *self.role() = Role::Closed;
        self.closed.cancel();
    }

    /// The device of this conversation's session, live or pending.
    pub fn device(&self) -> Option<String> {
        match &*self.role() {
            Role::Session(s) => Some(s.device.id.clone()),
            _ => None,
        }
    }

    pub fn is_live(&self) -> bool {
        matches!(&*self.role(), Role::Session(s) if s.live)
    }

    /// Still in a handshake: fresh, or pairing.
    pub fn in_handshake(&self) -> bool {
        matches!(&*self.role(), Role::Fresh | Role::Pairing)
    }

    pub fn is_fresh(&self) -> bool {
        matches!(&*self.role(), Role::Fresh)
    }

    pub fn age(&self, now: Instant) -> Duration {
        now.duration_since(self.created)
    }
}

/// What the transport does after a frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Inbound {
    Continue,
    /// The unsealed refusal (unknown device, bad proof, too many tries): close.
    Refuse,
    Close,
}

/// Where Away from home is listening, and with which key, while it is on.
pub struct OnState {
    pub computer: Arc<SecretKey>,
    pub connection: RemoteConnection,
    /// The address phones reach the computer at (a host name), once the helper has one.
    pub host: Option<String>,
}

/// What the computer tracks per phone across its sessions.
#[derive(Default)]
pub struct PhoneState {
    /// Item ids sent to this phone, and when: only these may be opened from it.
    pub cited: HashMap<String, Instant>,
    /// Bytes kept from this phone today, by local date.
    pub day: (String, u64),
    /// An original is on its way down to this phone.
    pub sending_original: bool,
}

pub struct Core {
    pub shell: Arc<dyn Shell>,
    pub mono: Arc<dyn Mono>,
    pub limits: Limits,
    dispatch: RwLock<Option<Arc<dyn Dispatch>>>,
    on: RwLock<Option<OnState>>,
    pub pairings: Mutex<Pairings>,
    convs: Mutex<HashMap<ConvId, Arc<Conv>>>,
    /// Session ids and pairing ids to the conversation they belong to (long-poll finds its
    /// conversation this way).
    sids: Mutex<HashMap<[u8; SID_LEN], ConvId>>,
    pub phones: Mutex<HashMap<String, PhoneState>>,
    /// Phones removed while on: checked again once a session is set up and on every frame,
    /// so an open racing a removal never carries on.
    revoked: Mutex<HashSet<String>>,
    pub incoming: Incoming,
    next_conv: AtomicU64,
    sent: AtomicU64,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Core {
    pub fn new(shell: Arc<dyn Shell>, mono: Arc<dyn Mono>, remote_dir: PathBuf) -> Arc<Core> {
        Arc::new(Core {
            shell,
            limits: Limits::new(mono.clone()),
            incoming: Incoming::new(remote_dir.join("incoming")),
            mono,
            dispatch: RwLock::new(None),
            on: RwLock::new(None),
            pairings: Mutex::new(Pairings::default()),
            convs: Mutex::new(HashMap::new()),
            sids: Mutex::new(HashMap::new()),
            phones: Mutex::new(HashMap::new()),
            revoked: Mutex::new(HashSet::new()),
            next_conv: AtomicU64::new(1),
            sent: AtomicU64::new(0),
        })
    }

    pub fn set_dispatch(&self, d: Arc<dyn Dispatch>) {
        *self.dispatch.write().unwrap_or_else(|e| e.into_inner()) = Some(d);
    }

    pub fn now(&self) -> Instant {
        self.mono.now()
    }

    /// Turn the channel on with the computer's key; nothing is reachable until a listener
    /// serves this core.
    pub fn set_on(&self, on: Option<OnState>) {
        let was_on = self.is_on();
        *self.on.write().unwrap_or_else(|e| e.into_inner()) = on;
        if was_on && !self.is_on() {
            lock(&self.pairings).cancel();
            self.close_all();
        }
    }

    pub fn is_on(&self) -> bool {
        self.on.read().unwrap_or_else(|e| e.into_inner()).is_some()
    }

    pub fn set_host(&self, host: Option<String>) {
        if let Some(on) = self.on.write().unwrap_or_else(|e| e.into_inner()).as_mut() {
            on.host = host;
        }
    }

    pub fn host(&self) -> Option<String> {
        self.on
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .and_then(|o| o.host.clone())
    }

    pub fn connection(&self) -> Option<RemoteConnection> {
        self.on
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(|o| o.connection)
    }

    pub fn computer(&self) -> Option<Arc<SecretKey>> {
        self.on
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(|o| o.computer.clone())
    }

    /// Bytes sent to phones since the last flush.
    pub fn count_sent(&self, bytes: usize) {
        self.sent.fetch_add(bytes as u64, Ordering::Relaxed);
    }

    /// Move the bytes counted so far into the store, once.
    pub fn flush_sent(&self) {
        let n = self.sent.swap(0, Ordering::Relaxed);
        if n == 0 {
            return;
        }
        match self.shell.engine().map(|e| e.add_sent_to_phones(n)) {
            Some(Ok(())) => {}
            _ => {
                self.sent.fetch_add(n, Ordering::Relaxed);
            }
        }
    }

    pub fn audit(&self, device: Option<&str>, action: RemoteAction, result: RemoteResult) {
        super::audit::log(self, device, action, result, 0.0);
    }

    // Conversations ------------------------------------------------------------------------

    pub fn new_conv(&self, carrier: Carrier) -> Arc<Conv> {
        let (tx, rx) = mpsc::unbounded_channel();
        let now = self.now();
        let conv = Arc::new(Conv {
            id: self.next_conv.fetch_add(1, Ordering::Relaxed),
            carrier,
            queued: AtomicUsize::new(0),
            drained: tokio::sync::Notify::new(),
            out: tx,
            rx: tokio::sync::Mutex::new(rx),
            closed: CancellationToken::new(),
            role: Mutex::new(Role::Fresh),
            created: now,
            last_seen: Mutex::new(now),
            down_busy: AtomicBool::new(false),
            refused: AtomicBool::new(false),
        });
        lock(&self.convs).insert(conv.id, conv.clone());
        conv
    }

    /// The conversation a session id or pairing id belongs to.
    pub fn conv_for_sid(&self, sid: &[u8; SID_LEN]) -> Option<Arc<Conv>> {
        let id = *lock(&self.sids).get(sid)?;
        lock(&self.convs).get(&id).cloned()
    }

    /// Whether a session id or pairing id leads to this conversation.
    pub fn has_sid(&self, conv: ConvId) -> bool {
        lock(&self.sids).values().any(|c| *c == conv)
    }

    fn register_sid(&self, sid: [u8; SID_LEN], conv: ConvId) {
        lock(&self.sids).insert(sid, conv);
    }

    pub fn end_conv(&self, conv: &Conv) {
        conv.close();
        lock(&self.convs).remove(&conv.id);
        lock(&self.sids).retain(|_, id| *id != conv.id);
    }

    fn convs(&self) -> Vec<Arc<Conv>> {
        lock(&self.convs).values().cloned().collect()
    }

    pub fn conv_alive(&self, id: ConvId) -> bool {
        lock(&self.convs)
            .get(&id)
            .is_some_and(|c| !c.closed.is_cancelled())
    }

    pub fn conv_count(&self) -> usize {
        lock(&self.convs).len()
    }

    /// Live sessions, and which phones they belong to.
    pub fn live_devices(&self) -> Vec<String> {
        self.convs()
            .iter()
            .filter(|c| c.is_live())
            .filter_map(|c| c.device())
            .collect()
    }

    pub fn close_all(&self) {
        for c in self.convs() {
            self.end_conv(&c);
        }
        self.incoming.clear_all();
    }

    /// A removed phone: close its sessions (their sockets and held polls end now) and stop
    /// its answers and its upload.
    pub fn drop_device(&self, device_id: &str) {
        lock(&self.revoked).insert(device_id.to_string());
        for c in self.convs() {
            if c.device().as_deref() == Some(device_id) {
                self.end_conv(&c);
            }
        }
        self.shell.stop_phone(device_id);
        self.incoming.cancel(device_id);
        lock(&self.phones).remove(device_id);
        self.limits.forget(device_id);
    }

    /// Housekeeping: pending sessions past ten seconds, long-poll conversations nobody polls,
    /// expired pairing deadlines.
    pub fn sweep(&self) {
        let now = self.now();
        for c in self.convs() {
            let stale = {
                let role = c.role();
                match &*role {
                    Role::Session(s) if !s.live => {
                        now.duration_since(s.opened)
                            >= Duration::from_secs(PENDING_SESSION_TTL_SECS)
                    }
                    Role::Fresh => c.age(now) >= Duration::from_secs(HANDSHAKE_TIMEOUT_SECS),
                    Role::Closed => true,
                    _ => false,
                }
            };
            let idle = c.carrier == Carrier::LongPoll
                && now.duration_since(*lock(&c.last_seen))
                    >= Duration::from_secs(LONG_POLL_HOLD_SECS * 2 + HANDSHAKE_TIMEOUT_SECS)
                && !c.down_busy.load(Ordering::SeqCst);
            if stale || idle {
                self.end_conv(&c);
            }
        }
        self.pairing_tick();
        let cutoff = Duration::from_secs(CITED_TTL_SECS);
        for p in lock(&self.phones).values_mut() {
            p.cited.retain(|_, at| now.duration_since(*at) < cutoff);
        }
    }

    pub fn pairing_tick(&self) {
        let (changed, send) = lock(&self.pairings).tick(self.now());
        if let Some((conv, frame)) = send {
            self.send_to(conv, frame);
        }
        if changed {
            self.shell.changed();
        }
    }

    pub fn send_to(&self, conv: ConvId, frame: Vec<u8>) {
        if let Some(c) = lock(&self.convs).get(&conv) {
            c.push(frame);
        }
    }

    // Frames -------------------------------------------------------------------------------

    /// Handle one frame that arrived on `conv`.
    pub fn inbound(self: &Arc<Self>, conv: &Arc<Conv>, frame: &[u8]) -> Inbound {
        if !self.is_on() {
            return Inbound::Close;
        }
        *lock(&conv.last_seen) = self.now();
        let kind = frame.get(1).copied().unwrap_or(0);
        let role = match &*conv.role() {
            Role::Fresh => 0,
            Role::Pairing => 1,
            Role::Session(_) => 2,
            Role::Closed => return Inbound::Close,
        };
        match (role, kind) {
            (0 | 1, KIND_HELLO | KIND_PAIR_COMMIT | KIND_PAIR_REVEAL) => {
                *conv.role() = Role::Pairing;
                self.pairing_frame(conv, frame)
            }
            (0, KIND_SESSION_OPEN) => self.open_session(conv, frame),
            (2, _) => self.session_frame(conv, frame),
            // Another kind on a pairing: dropped, the pairing goes on.
            (1, _) => Inbound::Continue,
            _ => Inbound::Close,
        }
    }

    fn pairing_frame(&self, conv: &Arc<Conv>, frame: &[u8]) -> Inbound {
        if !self.limits.take(Bucket::PairingFrames, "") {
            return Inbound::Close;
        }
        let step = lock(&self.pairings).handle(frame, conv.id, self.now(), self.shell.wall_now());
        if (!step.reply.is_empty() || step.request.is_some())
            && let Ok(h) = crypto::parse_header(frame)
        {
            self.register_sid(h.sid, conv.id);
        }
        for f in step.reply {
            conv.push(f);
        }
        if step.wrong {
            self.audit(None, RemoteAction::WrongCode, RemoteResult::Refused);
        }
        if let Some(req) = &step.request {
            self.shell.pairing_request(req);
        }
        if step.changed {
            self.shell.changed();
        }
        Inbound::Continue
    }

    fn open_session(self: &Arc<Self>, conv: &Arc<Conv>, frame: &[u8]) -> Inbound {
        let (Some(computer), Some(engine)) = (self.computer(), self.shell.engine()) else {
            return Inbound::Close;
        };
        // A paired phone is limited by its own bucket and never by the shared one, so failing
        // opens from outside cannot lock it out. Only an open naming no paired phone meets the
        // shared gate, before any work is done on it.
        let named = frame
            .get(HEADER_LEN..HEADER_LEN + DEVICE_ID_LEN)
            .map(crypto::b64)
            .and_then(|id| engine.device(&id).ok().flatten())
            .filter(|d| d.revoked_at.is_none() && !self.removed(&d.id));
        match &named {
            Some(d) if !self.limits.take(Bucket::SessionOpens, &d.id) => {
                return self.refuse(conv);
            }
            Some(_) => {}
            None if !self.limits.has(Bucket::FailedOpen, "") => return self.refuse(conv),
            None => {}
        }
        let lookup = |id: &crypto::DeviceId| {
            let row = engine.device(&crypto::b64(id)).ok().flatten()?;
            Some((row.public_key.clone(), row))
        };
        let open = match crypto::verify_open(frame, lookup, &computer) {
            Ok(o) => o,
            Err(_) => {
                self.limits.take(Bucket::FailedOpen, "");
                return self.refuse(conv);
            }
        };
        let device = open.device;
        if device.revoked_at.is_some() || self.removed(&device.id) {
            conv.push(crypto::revoked(&open.kd, &open.device_id, &open.on));
            *conv.role() = Role::Closed;
            return Inbound::Close;
        }
        let connection = self.connection().unwrap_or(RemoteConnection::Simple);
        let f = crypto::new_secret();
        let sid = crypto::random::<SID_LEN>();
        let (epub, fpub) = (
            crypto::public_bytes(&open.epub),
            crypto::public_bytes(&f.public_key()),
        );
        let cpub = crypto::public_bytes(&computer.public_key());
        let Ok(dkey) = crypto::check_public_key(&device.public_key) else {
            return self.refuse(conv);
        };
        let keys = crypto::session_keys_computer(
            &f,
            &computer,
            &open.epub,
            &dkey,
            &crypto::SessionInput {
                device_id: &open.device_id,
                epub: &epub,
                on: &open.on,
                sid: &sid,
                fpub: &fpub,
                cpub: &cpub,
                dpub: &device.public_key,
            },
        );
        drop(f);
        let mut channel = Channel::new(Side::Computer, sid, keys);
        let hello = RemoteReply {
            id: 0,
            answer: RemoteAnswer::Hello {
                hello: self.shell.hello(&device, connection),
            },
        };
        let body = serde_json::to_vec(&hello).expect("serialises");
        let first = channel
            .seal(&crypto::envelope_json(&body))
            .expect("a new channel is open");
        self.register_sid(sid, conv.id);
        conv.push(crypto::session_accept(&sid, &fpub));
        conv.push(first);
        let id = device.id.clone();
        *conv.role() = Role::Session(Box::new(Session {
            device,
            channel,
            live: false,
            opened: self.now(),
        }));
        // Removed while this open was being answered: end it.
        if self.removed(&id) {
            self.end_conv(conv);
            return Inbound::Close;
        }
        Inbound::Continue
    }

    /// Removed while Away from home is on.
    pub fn removed(&self, device: &str) -> bool {
        lock(&self.revoked).contains(device)
    }

    fn refuse(&self, conv: &Arc<Conv>) -> Inbound {
        conv.refused.store(true, Ordering::SeqCst);
        *conv.role() = Role::Closed;
        Inbound::Refuse
    }

    fn session_frame(self: &Arc<Self>, conv: &Arc<Conv>, frame: &[u8]) -> Inbound {
        let now = self.now();
        let (envelope, device, went_live) = {
            let mut role = conv.role();
            let Role::Session(s) = &mut *role else {
                return Inbound::Close;
            };
            let ttl = if s.live {
                SESSION_TTL_SECS
            } else {
                PENDING_SESSION_TTL_SECS
            };
            if now.duration_since(s.opened) >= Duration::from_secs(ttl) {
                *role = Role::Closed;
                return Inbound::Close;
            }
            // Until the session is established a frame that does not open is dropped, so junk
            // naming a sid cannot end it; once live, any refusal closes it.
            let opened = if s.live {
                s.channel.open(frame)
            } else {
                s.channel.try_open(frame)
            };
            let Ok(env) = opened else {
                if !s.live {
                    return Inbound::Continue;
                }
                *role = Role::Closed;
                return Inbound::Close;
            };
            if self.removed(&s.device.id) {
                *role = Role::Closed;
                return Inbound::Close;
            }
            let went_live = !s.live;
            s.live = true;
            (env, s.device.clone(), went_live)
        };
        if went_live {
            self.went_live(conv, &device);
        }
        match envelope {
            Envelope::Json(body) => self.request(conv, device, &body),
            Envelope::Chunk(chunk) => {
                if let Err(code) = self.incoming.chunk(&device.id, conv.id, chunk) {
                    self.reply(conv, 0, RemoteAnswer::Error { code });
                }
                Inbound::Continue
            }
        }
    }

    /// The first good up frame: the session counts now. A phone keeps at most three live
    /// sessions; the oldest gives way.
    fn went_live(&self, conv: &Arc<Conv>, device: &DeviceRow) {
        let mut mine: Vec<Arc<Conv>> = self
            .convs()
            .into_iter()
            .filter(|c| c.id != conv.id && c.is_live() && c.device().as_ref() == Some(&device.id))
            .collect();
        mine.sort_by_key(|c| c.created);
        while mine.len() >= MAX_SESSIONS_PER_DEVICE {
            let oldest = mine.remove(0);
            self.end_conv(&oldest);
        }
        let address = self.host().unwrap_or_default();
        if let Some(e) = self.shell.engine()
            && let Err(e) = e.touch_device(&device.id, &address)
        {
            tracing::warn!("could not note a phone's use: {e}");
        }
        self.shell.device_active(device);
    }

    fn request(self: &Arc<Self>, conv: &Arc<Conv>, device: DeviceRow, body: &[u8]) -> Inbound {
        let req: RemoteRequest = match serde_json::from_slice(body) {
            Ok(r) => r,
            Err(_) => {
                let id = serde_json::from_slice::<serde_json::Value>(body)
                    .ok()
                    .and_then(|v| v["id"].as_u64())
                    .and_then(|id| u32::try_from(id).ok())
                    .unwrap_or(0);
                self.reply(
                    conv,
                    id,
                    RemoteAnswer::Error {
                        code: PhoneError::Invalid,
                    },
                );
                return Inbound::Continue;
            }
        };
        if !self.limits.take(Bucket::Requests, &device.id) {
            self.audit(
                Some(&device.id),
                RemoteAction::Limited,
                RemoteResult::Limited,
            );
            self.reply(
                conv,
                req.id,
                RemoteAnswer::Error {
                    code: PhoneError::TooMany,
                },
            );
            return Inbound::Continue;
        }
        let upload = match super::incoming::declared(&req.call) {
            Some(decl) => match self.incoming.begin(self, &device.id, conv.id, req.id, decl) {
                Ok(done) => Some(done),
                Err(code) => {
                    self.reply(conv, req.id, RemoteAnswer::Error { code });
                    return Inbound::Continue;
                }
            },
            None => None,
        };
        let Some(dispatch) = self
            .dispatch
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
        else {
            return Inbound::Close;
        };
        let ctx = CallCtx {
            out: ReplySink {
                core: self.clone(),
                conv: conv.clone(),
                id: req.id,
                device: device.id.clone(),
            },
            device,
            req,
            upload,
        };
        tokio::spawn(dispatch.call(self.clone(), ctx));
        Inbound::Continue
    }

    /// Seal and queue one answer on `conv`. False when the session has closed.
    fn reply(&self, conv: &Conv, id: u32, answer: RemoteAnswer) -> bool {
        let body = serde_json::to_vec(&RemoteReply { id, answer }).expect("serialises");
        self.seal_out(conv, &crypto::envelope_json(&body))
    }

    fn seal_out(&self, conv: &Conv, envelope: &[u8]) -> bool {
        let mut role = conv.role();
        let Role::Session(s) = &mut *role else {
            return false;
        };
        match s.channel.seal(envelope) {
            Some(frame) => {
                conv.push(frame);
                true
            }
            None => false,
        }
    }

    /// Note that ids were sent to a phone: it may open these for the next hour.
    pub fn cite(&self, device: &str, ids: impl IntoIterator<Item = String>) {
        let now = self.now();
        let mut phones = lock(&self.phones);
        let p = phones.entry(device.to_string()).or_default();
        for id in ids {
            p.cited.insert(id, now);
        }
    }

    /// Claim the one original a phone may have on its way down. None while another is.
    pub fn original_slot(self: &Arc<Self>, device: &str) -> Option<OriginalSlot> {
        let mut phones = lock(&self.phones);
        let p = phones.entry(device.to_string()).or_default();
        if p.sending_original {
            return None;
        }
        p.sending_original = true;
        Some(OriginalSlot {
            core: self.clone(),
            device: device.to_string(),
        })
    }

    pub fn was_cited(&self, device: &str, id: &str) -> bool {
        let now = self.now();
        lock(&self.phones)
            .get(device)
            .and_then(|p| p.cited.get(id))
            .is_some_and(|at| now.duration_since(*at) < Duration::from_secs(CITED_TTL_SECS))
    }
}

/// The one original a phone has on its way down; released when dropped.
pub struct OriginalSlot {
    core: Arc<Core>,
    device: String,
}

impl Drop for OriginalSlot {
    fn drop(&mut self) {
        if let Some(p) = lock(&self.core.phones).get_mut(&self.device) {
            p.sending_original = false;
        }
    }
}

/// Where one call's answers go: sealed on its session, in order.
#[derive(Clone)]
pub struct ReplySink {
    core: Arc<Core>,
    conv: Arc<Conv>,
    pub id: u32,
    device: String,
}

impl ReplySink {
    /// Send one answer. False once the session has closed: stop working on it.
    pub fn send(&self, answer: RemoteAnswer) -> bool {
        if self.conv.closed.is_cancelled() {
            return false;
        }
        self.core.reply(&self.conv, self.id, answer)
    }

    pub fn error(&self, code: PhoneError) -> bool {
        self.send(RemoteAnswer::Error { code })
    }

    /// One piece of an original going down, once the transport has taken enough of what
    /// went before. False once the session has closed.
    pub async fn chunk(&self, seq: u32, last: bool, bytes: &[u8]) -> bool {
        if !self.conv.room(OUT_BUDGET).await {
            return false;
        }
        self.core.seal_out(
            &self.conv,
            &crypto::envelope_chunk(self.id, seq, last, bytes),
        )
    }

    pub fn closed(&self) -> tokio_util::sync::WaitForCancellationFuture<'_> {
        self.conv.closed.cancelled()
    }

    pub fn is_closed(&self) -> bool {
        self.conv.closed.is_cancelled()
    }

    pub fn device(&self) -> &str {
        &self.device
    }

    /// End this phone's conversation (it asked to disconnect).
    pub fn end(&self) {
        self.core.end_conv(&self.conv);
    }
}
