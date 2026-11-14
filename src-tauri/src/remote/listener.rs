//! The listener the helper forwards to. Local only: an owner-only Unix socket in an owner-only
//! folder on Linux and macOS, `127.0.0.1` with a per-launch helper key on Windows. Never another
//! interface, never the `lemond` port, never a proxy to anything.
//!
//! Routes: the phone app's files (see `static_files`) and one sealed path, `/s`:
//! - WebSocket (`GET /s` with an upgrade): each binary message is one frame, at most
//!   `MAX_FRAME`. The unsealed refusal is a close with code 4403 and no reason.
//! - Long-poll, for when WebSockets do not get through. Bodies are a run of frames, each a
//!   4-byte big-endian length then the frame. `POST /s` carries up to `MAX_LONG_POLL_BATCH`
//!   frames (any content type, so a browser sends it without a preflight); it answers 200 with
//!   the replies to handshake frames (HELLO, the pairing nonce, SESSION_ACCEPT with the first
//!   `Hello`, or a REVOKED), 204 when there are none, 403 empty for the unsealed refusal, 410
//!   when the session it names has ended. `GET /s?sid=<32 hex>` names a session id or a pairing
//!   id and is held up to `LONG_POLL_HOLD_SECS`: 200 with what came down (`PAIR_OUTCOME` comes
//!   this way under the pairing id), 204 when nothing did, 410 once it has ended, 409 while
//!   another poll for it is held.
//!
//! A body that stops arriving for `BODY_IDLE_SECS` is answered 408. A `POST` is read from a
//! small pool of its own (`POST_SLOTS`), not from the connection permits that paired phones
//! need, because nothing is known about who sent it until the body is read.
//!
//! While Away from home is off every keyed request gets a bare 503 (the phone reads it as
//! "turned off").
//! Everything else is a bare 404. Without the helper key (Windows) every request is a bare
//! 404 before routing. The key is never echoed, redirected or logged; logs say only "no key".

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use axum::body::Body;
use axum::extract::FromRequestParts;
use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::http::{HeaderMap, HeaderValue, Method, Request, Response, StatusCode, header};
use nocturne_helpers::{HELPER_KEY_HEADER, KeyCarrier, Origin};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio_util::sync::CancellationToken;

use super::crypto;
use super::limits::Bucket;
use super::protocol::*;
use super::session::{Carrier, Conv, Core, Inbound};
use super::static_files::{self, PhoneFiles};

/// The close code of the unsealed refusal.
pub const REFUSED_CLOSE: u16 = 4403;
/// Length prefix of each frame in a long-poll body.
const LEN_PREFIX: usize = 4;
/// How long a long-poll body may go without a byte before it is given up on.
const BODY_IDLE_SECS: u64 = 5;
/// Long-poll bodies read at once. A body is read before its sender is known, so these are
/// separate from the connection permits: a stalled POST cannot take a paired phone's place.
const POST_SLOTS: usize = MAX_CONNECTIONS / 2;
/// How often a quiet WebSocket that carries a session is pinged. Cloudflare ends a socket that
/// carries nothing for about two minutes, and the phone then takes seconds to open a new one at
/// its next request; a ping keeps it open. Strangers are never pinged.
pub const WS_KEEPALIVE: Duration = Duration::from_secs(30);

pub enum Bind {
    /// An owner-only socket at this path; its folder is made owner-only.
    Unix(PathBuf),
    /// `127.0.0.1:0`, reachable only with `key` carried as `carrier` says.
    Tcp { key: String, carrier: KeyCarrier },
}

pub struct ListenerConfig {
    pub bind: Bind,
    pub files: Arc<dyn PhoneFiles>,
    /// [`WS_KEEPALIVE`], shorter in tests.
    pub keepalive: Duration,
}

/// A running listener. Stop it with [`ListenerHandle::stop`].
pub struct ListenerHandle {
    pub origin: Origin,
    stop: CancellationToken,
    task: tokio::task::JoinHandle<()>,
    /// A port on loopback, which no address for Online answers may name.
    _port: Option<nocturne_lemonade::online::OwnPort>,
}

impl ListenerHandle {
    /// For the advanced view: how the listener is reached, never the key.
    pub fn describe(&self) -> String {
        match &self.origin {
            Origin::Unix(_) => "unix socket, owner only".to_string(),
            Origin::Tcp { port, .. } => format!("127.0.0.1:{port}, helper key only"),
        }
    }

    /// Stop listening; returns once the socket is closed.
    pub async fn stop(self) {
        self.stop.cancel();
        let abort = self.task.abort_handle();
        if tokio::time::timeout(Duration::from_secs(2), self.task)
            .await
            .is_err()
        {
            abort.abort();
        }
        if let Origin::Unix(path) = &self.origin {
            let _ = std::fs::remove_file(path);
        }
    }
}

struct Ctx {
    core: Arc<Core>,
    files: Arc<dyn PhoneFiles>,
    key: Option<(String, KeyCarrier)>,
    permits: Arc<Semaphore>,
    posts: Arc<Semaphore>,
    keepalive: Duration,
}

/// Start listening and serving `core`.
pub async fn serve(cfg: ListenerConfig, core: Arc<Core>) -> std::io::Result<ListenerHandle> {
    let stop = CancellationToken::new();
    let key = match &cfg.bind {
        Bind::Unix(_) => None,
        Bind::Tcp { key, carrier } => Some((key.clone(), *carrier)),
    };
    let ctx = Arc::new(Ctx {
        core,
        files: cfg.files,
        key,
        permits: Arc::new(Semaphore::new(MAX_CONNECTIONS)),
        posts: Arc::new(Semaphore::new(POST_SLOTS)),
        keepalive: cfg.keepalive,
    });
    let app = axum::Router::new().fallback(move |req: Request<Body>| {
        let ctx = ctx.clone();
        async move { handle(ctx, req).await }
    });
    let shutdown = stop.clone().cancelled_owned();
    match cfg.bind {
        #[cfg(unix)]
        Bind::Unix(path) => {
            let dir = path.parent().map(PathBuf::from).unwrap_or_default();
            crate::fsutil::create_private_dir(&dir)?;
            let _ = std::fs::remove_file(&path);
            let listener = tokio::net::UnixListener::bind(&path)?;
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
            }
            let task = tokio::spawn(async move {
                let _ = axum::serve(listener, app)
                    .with_graceful_shutdown(shutdown)
                    .await;
            });
            Ok(ListenerHandle {
                origin: Origin::Unix(path),
                stop,
                task,
                _port: None,
            })
        }
        #[cfg(not(unix))]
        Bind::Unix(_) => Err(std::io::Error::other("no unix sockets here")),
        Bind::Tcp { key, .. } => {
            let listener =
                tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
            let port = listener.local_addr()?.port();
            let task = tokio::spawn(async move {
                let _ = axum::serve(listener, app)
                    .with_graceful_shutdown(shutdown)
                    .await;
            });
            Ok(ListenerHandle {
                origin: Origin::Tcp { port, key },
                stop,
                task,
                _port: Some(nocturne_lemonade::online::OwnPort::hold(port)),
            })
        }
    }
}

fn code(n: u16) -> StatusCode {
    StatusCode::from_u16(n).expect("a valid status")
}

fn bare(status: StatusCode) -> Response<Body> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_LENGTH, "0")
        .body(Body::empty())
        .expect("a plain response")
}

/// Strip and check the helper key (TCP only). None: refuse with a bare 404.
fn keyed_path(ctx: &Ctx, req: &Request<Body>) -> Option<String> {
    let path = req.uri().path().to_string();
    let Some((key, carrier)) = &ctx.key else {
        return Some(path);
    };
    match carrier {
        KeyCarrier::HostHeader => {
            let host = req
                .headers()
                .get(header::HOST)
                .map(|h| h.as_bytes())
                .unwrap_or_default();
            crypto::eq_ct(host, key.as_bytes()).then_some(path)
        }
        KeyCarrier::Header => {
            let mut values = req.headers().get_all(HELPER_KEY_HEADER).iter();
            let (Some(value), None) = (values.next(), values.next()) else {
                return None;
            };
            crypto::eq_ct(value.as_bytes(), key.as_bytes()).then_some(path)
        }
        KeyCarrier::UnixSocket => Some(path),
    }
}

async fn handle(ctx: Arc<Ctx>, req: Request<Body>) -> Response<Body> {
    let Some(path) = keyed_path(&ctx, &req) else {
        ctx.core.limits.take(Bucket::NoKey, "");
        tracing::debug!("no key");
        return bare(code(HTTP_NOT_FOUND));
    };
    if !ctx.core.is_on() {
        return bare(code(HTTP_OFF));
    }
    // Full is not off: 503 means only "turned off" to the phone.
    let pool = if path == PATH_SESSION && *req.method() == Method::POST {
        &ctx.posts
    } else {
        &ctx.permits
    };
    let Ok(permit) = pool.clone().try_acquire_owned() else {
        return bare(code(HTTP_POLL_BUSY));
    };
    if path == PATH_SESSION {
        return session(ctx, req, permit).await;
    }
    if !matches!(*req.method(), Method::GET | Method::HEAD) {
        return bare(code(HTTP_NOT_FOUND));
    }
    let Some(name) = static_files::file_for(&path) else {
        return bare(code(HTTP_NOT_FOUND));
    };
    if !ctx.core.limits.take(Bucket::Static, "") {
        return bare(StatusCode::TOO_MANY_REQUESTS);
    }
    let Some(bytes) = ctx.files.get(&name) else {
        return bare(code(HTTP_NOT_FOUND));
    };
    let connection = ctx
        .core
        .connection()
        .unwrap_or(nocturne_contract::RemoteConnection::Simple);
    let mut res = Response::builder()
        .status(StatusCode::OK)
        .header(
            header::CONTENT_TYPE,
            static_files::mime_for(&name).unwrap_or("application/octet-stream"),
        )
        .header(header::CONTENT_LENGTH, bytes.len());
    for (k, v) in static_files::phone_headers(connection) {
        res = res.header(k, v);
    }
    ctx.core.count_sent(bytes.len());
    let body = if *req.method() == Method::HEAD {
        Body::empty()
    } else {
        Body::from(bytes)
    };
    res.body(body).expect("a file response")
}

/// The exact origin to echo, when the request carries one we accept. `Err` when it carries
/// one we do not.
fn allowed_origin(core: &Core, headers: &HeaderMap) -> Result<Option<HeaderValue>, ()> {
    let Some(origin) = headers.get(header::ORIGIN) else {
        return Ok(None);
    };
    let text = origin.to_str().map_err(|_| ())?;
    let current = core.host().map(|h| format!("https://{h}"));
    let homes = core
        .shell
        .engine()
        .and_then(|e| e.home_origins().ok())
        .unwrap_or_default();
    if current.as_deref() == Some(text) || homes.iter().any(|h| h == text) {
        Ok(Some(origin.clone()))
    } else {
        Err(())
    }
}

fn with_cors(mut res: Response<Body>, origin: &Option<HeaderValue>) -> Response<Body> {
    if let Some(o) = origin {
        let h = res.headers_mut();
        h.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, o.clone());
        h.insert(header::VARY, HeaderValue::from_static("Origin"));
    }
    res
}

async fn session(
    ctx: Arc<Ctx>,
    req: Request<Body>,
    permit: OwnedSemaphorePermit,
) -> Response<Body> {
    let Ok(origin) = allowed_origin(&ctx.core, req.headers()) else {
        return bare(code(HTTP_REFUSED));
    };
    match *req.method() {
        Method::OPTIONS => {
            let mut res = bare(StatusCode::NO_CONTENT);
            let h = res.headers_mut();
            h.insert(
                header::ACCESS_CONTROL_ALLOW_METHODS,
                HeaderValue::from_static("GET, POST"),
            );
            h.insert(
                header::ACCESS_CONTROL_ALLOW_HEADERS,
                HeaderValue::from_static("content-type"),
            );
            h.insert(
                header::ACCESS_CONTROL_MAX_AGE,
                HeaderValue::from_static("600"),
            );
            with_cors(res, &origin)
        }
        Method::GET if req.uri().query().is_some() => {
            with_cors(poll_down(&ctx, req).await, &origin)
        }
        Method::GET => {
            let (mut parts, _) = req.into_parts();
            let Ok(upgrade) = WebSocketUpgrade::from_request_parts(&mut parts, &()).await else {
                return bare(code(HTTP_NOT_FOUND));
            };
            let (core, permits, keepalive) = (ctx.core.clone(), ctx.permits.clone(), ctx.keepalive);
            let res = upgrade
                .max_message_size(MAX_FRAME)
                .max_frame_size(MAX_FRAME)
                .on_upgrade(move |socket| websocket(socket, core, permits, keepalive));
            drop(permit);
            with_cors(res, &origin)
        }
        Method::POST => with_cors(poll_up(&ctx, req).await, &origin),
        _ => bare(code(HTTP_NOT_FOUND)),
    }
}

/// A socket holds a connection permit only once it carries a session: until then it is one of
/// the few strangers `Core` keeps, and the permits stay for paired phones.
async fn websocket(
    mut socket: WebSocket,
    core: Arc<Core>,
    permits: Arc<Semaphore>,
    keepalive: Duration,
) {
    let conv = core.new_conv(Carrier::WebSocket);
    let mut permit: Option<OwnedSemaphorePermit> = None;
    let mut rx = conv.rx.lock().await;
    let handshake = tokio::time::sleep(Duration::from_secs(HANDSHAKE_TIMEOUT_SECS));
    tokio::pin!(handshake);
    let mut ping = tokio::time::interval_at(tokio::time::Instant::now() + keepalive, keepalive);
    ping.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut refused = false;
    loop {
        tokio::select! {
            biased;
            out = rx.recv() => match out {
                Some(frame) => {
                    conv.took(frame.len());
                    core.count_sent(frame.len());
                    if socket.send(Message::Binary(frame.into())).await.is_err() {
                        break;
                    }
                }
                None => break,
            },
            () = conv.closed.cancelled() => break,
            () = &mut handshake, if conv.is_fresh() => break,
            _ = ping.tick(), if permit.is_some() => {
                if socket.send(Message::Ping(Default::default())).await.is_err() {
                    break;
                }
            }
            msg = socket.recv() => match msg {
                Some(Ok(Message::Binary(frame))) => match core.inbound(&conv, &frame) {
                    Inbound::Continue => {
                        if permit.is_none() && conv.is_verified() {
                            match permits.clone().try_acquire_owned() {
                                Ok(p) => permit = Some(p),
                                Err(_) => break,
                            }
                        }
                    }
                    Inbound::Refuse => {
                        refused = true;
                        break;
                    }
                    Inbound::Close => break,
                },
                Some(Ok(Message::Ping(_) | Message::Pong(_))) => {}
                _ => break,
            },
        }
    }
    // What was queued before the end (a REVOKED, a last answer) still goes out.
    while let Ok(frame) = rx.try_recv() {
        conv.took(frame.len());
        core.count_sent(frame.len());
        if socket.send(Message::Binary(frame.into())).await.is_err() {
            break;
        }
    }
    let code = if refused { REFUSED_CLOSE } else { 1000 };
    let _ = socket
        .send(Message::Close(Some(CloseFrame {
            code,
            reason: "".into(),
        })))
        .await;
    drop(rx);
    core.end_conv(&conv);
}

/// Clears a busy flag when the request ends, however it ends.
struct Busy<'a>(&'a std::sync::atomic::AtomicBool);

impl Drop for Busy<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

fn frames_body(frames: &[Vec<u8>]) -> Vec<u8> {
    let mut body = Vec::new();
    for f in frames {
        body.extend_from_slice(&(f.len() as u32).to_be_bytes());
        body.extend_from_slice(f);
    }
    body
}

fn frames_response(core: &Core, frames: &[Vec<u8>]) -> Response<Body> {
    let body = frames_body(frames);
    core.count_sent(body.len());
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::CONTENT_LENGTH, body.len())
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::from(body))
        .expect("a frames response")
}

/// The body, at most `limit` bytes, given up on when it goes `BODY_IDLE_SECS` without a byte.
async fn read_body(mut body: Body, limit: usize) -> Result<Vec<u8>, StatusCode> {
    use axum::body::HttpBody;
    let mut out = Vec::new();
    loop {
        let next = tokio::time::timeout(
            Duration::from_secs(BODY_IDLE_SECS),
            std::future::poll_fn(|cx| std::pin::Pin::new(&mut body).poll_frame(cx)),
        )
        .await;
        match next {
            Err(_) => return Err(StatusCode::REQUEST_TIMEOUT),
            Ok(None) => return Ok(out),
            Ok(Some(Err(_))) => return Err(StatusCode::BAD_REQUEST),
            Ok(Some(Ok(frame))) => {
                if let Ok(data) = frame.into_data() {
                    if out.len() + data.len() > limit {
                        return Err(StatusCode::PAYLOAD_TOO_LARGE);
                    }
                    out.extend_from_slice(&data);
                }
            }
        }
    }
}

fn is_pairing_kind(kind: u8) -> bool {
    matches!(kind, KIND_HELLO | KIND_PAIR_COMMIT | KIND_PAIR_REVEAL)
}

/// Frames up. Each finds its conversation by its sid: zeros (a SESSION_OPEN) start one, a
/// pairing frame starts one under its pairing id, anything else must name a live one.
async fn poll_up(ctx: &Ctx, req: Request<Body>) -> Response<Body> {
    let core = &ctx.core;
    let limit = MAX_LONG_POLL_BATCH * (LEN_PREFIX + MAX_FRAME);
    let declared = req
        .headers()
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<usize>().ok());
    match declared {
        Some(n) if n <= limit => {}
        _ => return bare(StatusCode::PAYLOAD_TOO_LARGE),
    }
    let body = match read_body(req.into_body(), limit).await {
        Ok(body) => body,
        Err(status) => return bare(status),
    };
    let Some(frames) = split_frames(&body) else {
        return bare(StatusCode::PAYLOAD_TOO_LARGE);
    };
    let mut handshakes: Vec<std::sync::Arc<Conv>> = Vec::new();
    for frame in frames {
        let Ok(h) = crypto::parse_header(frame) else {
            return bare(code(HTTP_NOT_FOUND));
        };
        let conv = match core.conv_for_sid(&h.sid) {
            Some(c) => c,
            None if h.sid == [0u8; SID_LEN] || is_pairing_kind(h.kind) => {
                core.new_conv(Carrier::LongPoll)
            }
            None => return bare(code(HTTP_ENDED)),
        };
        let handshake = conv.in_handshake();
        match core.inbound(&conv, frame) {
            Inbound::Continue => {}
            Inbound::Refuse => {
                core.end_conv(&conv);
                return bare(code(HTTP_REFUSED));
            }
            Inbound::Close => conv.close(),
        }
        if handshake && !handshakes.iter().any(|c| c.id == conv.id) {
            handshakes.push(conv);
        }
    }
    let mut out = Vec::new();
    for conv in &handshakes {
        if let Ok(mut rx) = conv.rx.try_lock() {
            while out.len() < MAX_LONG_POLL_BATCH
                && let Ok(f) = rx.try_recv()
            {
                conv.took(f.len());
                out.push(f);
            }
        }
        // Ended, or nothing answered (no offer open, a wrong code): nothing to keep.
        if conv.closed.is_cancelled() || !core.has_sid(conv.id) {
            core.end_conv(conv);
        }
    }
    if out.is_empty() {
        bare(StatusCode::NO_CONTENT)
    } else {
        frames_response(core, &out)
    }
}

/// A held poll for what comes down under a session id or a pairing id.
async fn poll_down(ctx: &Ctx, req: Request<Body>) -> Response<Body> {
    let core = &ctx.core;
    let sid = req
        .uri()
        .query()
        .unwrap_or_default()
        .split('&')
        .find_map(|p| p.strip_prefix("sid="))
        .and_then(parse_sid);
    let Some(sid) = sid else {
        return bare(code(HTTP_NOT_FOUND));
    };
    let Some(conv) = core.conv_for_sid(&sid) else {
        return bare(code(HTTP_ENDED));
    };
    if conv.down_busy.swap(true, Ordering::SeqCst) {
        return bare(code(HTTP_POLL_BUSY));
    }
    let _busy = Busy(&conv.down_busy);
    *conv.last_seen.lock().unwrap_or_else(|e| e.into_inner()) = core.now();
    let frames = hold(&conv).await;
    if frames.is_empty() {
        if conv.closed.is_cancelled() {
            core.end_conv(&conv);
            return bare(code(HTTP_ENDED));
        }
        return bare(StatusCode::NO_CONTENT);
    }
    frames_response(core, &frames)
}

fn parse_sid(hex: &str) -> Option<[u8; SID_LEN]> {
    if hex.len() != SID_LEN * 2 {
        return None;
    }
    let mut out = [0u8; SID_LEN];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(hex.get(2 * i..2 * i + 2)?, 16).ok()?;
    }
    Some(out)
}

/// Wait for frames (or the end, or the hold), then take up to a batch.
async fn hold(conv: &Conv) -> Vec<Vec<u8>> {
    let mut rx = conv.rx.lock().await;
    let mut out = Vec::new();
    if let Ok(f) = rx.try_recv() {
        out.push(f);
    } else {
        tokio::select! {
            f = rx.recv() => out.extend(f),
            () = conv.closed.cancelled() => {}
            () = tokio::time::sleep(Duration::from_secs(LONG_POLL_HOLD_SECS)) => {}
        }
    }
    while out.len() < MAX_LONG_POLL_BATCH
        && let Ok(f) = rx.try_recv()
    {
        out.push(f);
    }
    for f in &out {
        conv.took(f.len());
    }
    out
}

/// A long-poll body: frames, each with a 4-byte big-endian length, at most a batch.
pub fn split_frames(body: &[u8]) -> Option<Vec<&[u8]>> {
    let mut out = Vec::new();
    let mut rest = body;
    while !rest.is_empty() {
        let len = u32::from_be_bytes(rest.get(..LEN_PREFIX)?.try_into().ok()?) as usize;
        if len > MAX_FRAME || out.len() == MAX_LONG_POLL_BATCH {
            return None;
        }
        let frame = rest.get(LEN_PREFIX..LEN_PREFIX + len)?;
        out.push(frame);
        rest = &rest[LEN_PREFIX + len..];
    }
    (!out.is_empty()).then_some(out)
}
