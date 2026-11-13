//! The listener the helper forwards to. Local only: an owner-only Unix socket in an owner-only
//! folder on Linux and macOS, `127.0.0.1` with a per-launch helper key on Windows. Never another
//! interface, never the `lemond` port, never a proxy to anything.
//!
//! Routes: the phone app's files (see `static_files`) and one sealed path, `/s`:
//! - WebSocket (`GET /s` with an upgrade): each binary message is one frame, at most
//!   `MAX_FRAME`. The unsealed refusal is a close with code 4403 and no reason.
//! - Long-poll, for when WebSockets do not get through. The phone names its conversation with
//!   `c` (an id from `ID_ALPHABET`). `POST /s?c=<id>&d=up` carries up to `MAX_LONG_POLL_BATCH`
//!   frames, each as a 4-byte big-endian length then the frame; it answers 204, or 403 empty
//!   (the unsealed refusal), or 410 when the conversation has ended. `POST /s?c=<id>&d=down` is
//!   held up to `LONG_POLL_HOLD_SECS` and answers 200 with frames in the same layout, 204 when
//!   nothing came, 410 once the conversation has ended. One outstanding POST per direction:
//!   a second answers 409.
//!
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
use nocturne_helpers::{KeyCarrier, Origin};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio_util::sync::CancellationToken;

use super::crypto;
use super::limits::Bucket;
use super::protocol::*;
use super::session::{Conv, Core, Inbound};
use super::static_files::{self, PhoneFiles};

/// The close code of the unsealed refusal.
pub const REFUSED_CLOSE: u16 = 4403;
/// Length prefix of each frame in a long-poll body.
const LEN_PREFIX: usize = 4;

pub enum Bind {
    /// An owner-only socket at this path; its folder is made owner-only.
    Unix(PathBuf),
    /// `127.0.0.1:0`, reachable only with `key` carried as `carrier` says.
    Tcp { key: String, carrier: KeyCarrier },
}

pub struct ListenerConfig {
    pub bind: Bind,
    pub files: Arc<dyn PhoneFiles>,
}

/// A running listener. Stop it with [`ListenerHandle::stop`].
pub struct ListenerHandle {
    pub origin: Origin,
    stop: CancellationToken,
    task: tokio::task::JoinHandle<()>,
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
            })
        }
    }
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
        KeyCarrier::PathPrefix => {
            let rest = path.strip_prefix('/')?;
            let (first, tail) = rest.split_at(rest.find('/').unwrap_or(rest.len()));
            crypto::eq_ct(first.as_bytes(), key.as_bytes()).then(|| {
                if tail.is_empty() {
                    "/".into()
                } else {
                    tail.into()
                }
            })
        }
        KeyCarrier::UnixSocket => Some(path),
    }
}

async fn handle(ctx: Arc<Ctx>, req: Request<Body>) -> Response<Body> {
    let Some(path) = keyed_path(&ctx, &req) else {
        ctx.core.limits.take(Bucket::NoKey, "");
        tracing::debug!("no key");
        return bare(StatusCode::NOT_FOUND);
    };
    if !ctx.core.is_on() {
        return bare(StatusCode::NOT_FOUND);
    }
    let Ok(permit) = ctx.permits.clone().try_acquire_owned() else {
        return bare(StatusCode::SERVICE_UNAVAILABLE);
    };
    if path == PATH_SESSION {
        return session(ctx, req, permit).await;
    }
    if !matches!(*req.method(), Method::GET | Method::HEAD) {
        return bare(StatusCode::NOT_FOUND);
    }
    let Some(name) = static_files::file_for(&path) else {
        return bare(StatusCode::NOT_FOUND);
    };
    if !ctx.core.limits.take(Bucket::Static, "") {
        return bare(StatusCode::TOO_MANY_REQUESTS);
    }
    let Some(bytes) = ctx.files.get(&name) else {
        return bare(StatusCode::NOT_FOUND);
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
        return bare(StatusCode::FORBIDDEN);
    };
    match *req.method() {
        Method::OPTIONS => {
            let mut res = bare(StatusCode::NO_CONTENT);
            let h = res.headers_mut();
            h.insert(
                header::ACCESS_CONTROL_ALLOW_METHODS,
                HeaderValue::from_static("POST"),
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
        Method::GET => {
            let (mut parts, _) = req.into_parts();
            let Ok(upgrade) = WebSocketUpgrade::from_request_parts(&mut parts, &()).await else {
                return bare(StatusCode::NOT_FOUND);
            };
            let core = ctx.core.clone();
            let res = upgrade
                .max_message_size(MAX_FRAME)
                .max_frame_size(MAX_FRAME)
                .on_upgrade(move |socket| websocket(socket, core, permit));
            with_cors(res, &origin)
        }
        Method::POST => with_cors(long_poll(&ctx, req).await, &origin),
        _ => bare(StatusCode::NOT_FOUND),
    }
}

async fn websocket(mut socket: WebSocket, core: Arc<Core>, _permit: OwnedSemaphorePermit) {
    let conv = core.new_conv();
    let mut rx = conv.rx.lock().await;
    let handshake = tokio::time::sleep(Duration::from_secs(HANDSHAKE_TIMEOUT_SECS));
    tokio::pin!(handshake);
    let mut refused = false;
    loop {
        tokio::select! {
            biased;
            out = rx.recv() => match out {
                Some(frame) => {
                    core.count_sent(frame.len());
                    if socket.send(Message::Binary(frame.into())).await.is_err() {
                        break;
                    }
                }
                None => break,
            },
            () = conv.closed.cancelled() => break,
            () = &mut handshake, if conv.is_fresh() => break,
            msg = socket.recv() => match msg {
                Some(Ok(Message::Binary(frame))) => match core.inbound(&conv, &frame) {
                    Inbound::Continue => {}
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

fn query(req: &Request<Body>) -> (Option<String>, Option<String>) {
    let mut c = None;
    let mut d = None;
    for pair in req.uri().query().unwrap_or_default().split('&') {
        match pair.split_once('=') {
            Some(("c", v)) => c = Some(v.to_string()),
            Some(("d", v)) => d = Some(v.to_string()),
            _ => {}
        }
    }
    (c, d)
}

/// Clears a busy flag when the request ends, however it ends.
struct Busy<'a>(&'a std::sync::atomic::AtomicBool);

impl Drop for Busy<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

async fn long_poll(ctx: &Ctx, req: Request<Body>) -> Response<Body> {
    let (Some(name), Some(dir)) = query(&req) else {
        return bare(StatusCode::NOT_FOUND);
    };
    if crypto::check_id(&name).is_err() {
        return bare(StatusCode::NOT_FOUND);
    }
    let core = &ctx.core;
    match dir.as_str() {
        "up" => {
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
            let Ok(body) = axum::body::to_bytes(req.into_body(), limit).await else {
                return bare(StatusCode::PAYLOAD_TOO_LARGE);
            };
            let Some(frames) = split_frames(&body) else {
                return bare(StatusCode::PAYLOAD_TOO_LARGE);
            };
            let Some(conv) = core.poll_conv(&name, true) else {
                return bare(StatusCode::GONE);
            };
            if conv.closed.is_cancelled() {
                return bare(StatusCode::GONE);
            }
            if conv.up_busy.swap(true, Ordering::SeqCst) {
                return bare(StatusCode::CONFLICT);
            }
            let _busy = Busy(&conv.up_busy);
            for frame in frames {
                match core.inbound(&conv, frame) {
                    Inbound::Continue => {}
                    Inbound::Refuse => {
                        core.end_conv(&conv);
                        return bare(StatusCode::FORBIDDEN);
                    }
                    Inbound::Close => {
                        conv.close();
                        return bare(StatusCode::GONE);
                    }
                }
            }
            bare(StatusCode::NO_CONTENT)
        }
        "down" => {
            let Some(conv) = core.poll_conv(&name, false) else {
                return bare(StatusCode::GONE);
            };
            if conv.down_busy.swap(true, Ordering::SeqCst) {
                return bare(StatusCode::CONFLICT);
            }
            let _busy = Busy(&conv.down_busy);
            *conv.last_seen.lock().unwrap_or_else(|e| e.into_inner()) = core.now();
            let frames = hold(&conv).await;
            if frames.is_empty() {
                if conv.closed.is_cancelled() {
                    core.end_conv(&conv);
                    return bare(StatusCode::GONE);
                }
                return bare(StatusCode::NO_CONTENT);
            }
            let mut body = Vec::new();
            for f in &frames {
                body.extend_from_slice(&(f.len() as u32).to_be_bytes());
                body.extend_from_slice(f);
            }
            core.count_sent(body.len());
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "application/octet-stream")
                .header(header::CONTENT_LENGTH, body.len())
                .header(header::CACHE_CONTROL, "no-store")
                .body(Body::from(body))
                .expect("a frames response")
        }
        _ => bare(StatusCode::NOT_FOUND),
    }
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
