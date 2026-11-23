//! One session: the handshake (SESSION_OPEN, then SESSION_ACCEPT or a sealed REVOKED, then
//! Hello), then requests by id over sealed frames. Session keys and counters live here only. A
//! frame is never sent twice: after a transport error the session is dead, and the caller opens
//! a new one and tries the call again.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use nocturne_contract::{
    PhoneError, PhoneHello, RemoteAnswer, RemoteCall, RemoteReply, RemoteRequest,
};
use tokio::sync::mpsc;

use super::link::{self, Got, Inbox, Link, LinkEnd, LinkKind, Route};
use crate::crypto::{self, Chunk, Envelope};
use crate::frame::Channel;
use crate::phone::PhoneIdentity;
use crate::protocol::*;

/// The listener's close code for the unsealed refusal.
pub(crate) const WS_REFUSED: u16 = 4403;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OpenFailure {
    Revoked,
    Refused,
    Network,
    Ended,
    Off,
    Protocol,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct OpenError {
    pub failure: OpenFailure,
    /// An opened WebSocket closed before the session was established.
    pub socket_lost: bool,
}

fn failed(failure: OpenFailure) -> OpenError {
    OpenError {
        failure,
        socket_lost: false,
    }
}

/// Off (503) and the unsealed refusal (403, or a close with 4403) are told apart; a session that
/// ended (410) is tried again; everything else is one state: asleep, or at a new address.
fn failure_for(end: LinkEnd) -> OpenError {
    match end {
        LinkEnd::Http { status: HTTP_OFF } => failed(OpenFailure::Off),
        LinkEnd::Http { status: HTTP_ENDED } => failed(OpenFailure::Ended),
        LinkEnd::Http {
            status: HTTP_REFUSED,
        } => failed(OpenFailure::Refused),
        LinkEnd::Http { .. } | LinkEnd::Network => failed(OpenFailure::Network),
        LinkEnd::Protocol => failed(OpenFailure::Protocol),
        LinkEnd::Closed { code: WS_REFUSED } => failed(OpenFailure::Refused),
        LinkEnd::Closed { .. } => OpenError {
            failure: OpenFailure::Network,
            socket_lost: true,
        },
    }
}

/// What came down for one request.
#[derive(Debug)]
pub(crate) enum Down {
    Answer(RemoteAnswer),
    Chunk(Chunk),
    /// A reply to the request that this build cannot read.
    Unreadable,
}

/// Why a request on a session did not get its answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Fault {
    /// The session ended; the call can be tried on a new one.
    Closed,
    /// The computer answered with an error.
    Phone(PhoneError),
    /// Nothing came for this request in its wait while the session still hears the computer:
    /// this call alone gives up, and the calls beside it carry on.
    TimedOut,
}

pub(crate) struct OpenInit<'a> {
    pub identity: &'a PhoneIdentity,
    pub route: &'a Route,
    pub http: &'a reqwest::Client,
    pub prefer_poll: bool,
    pub socket_timeout: Duration,
    pub handshake: Duration,
    /// How long an open session may hear nothing at all from the computer before it is left.
    pub quiet: Duration,
    pub waits: Waits,
}

/// How long a call on an open session waits before the session is left and the call goes again.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Waits {
    /// For each answer, or each next part of one.
    pub answer: Duration,
    /// For each frame up, a call or a chunk of an upload, to leave.
    pub chunk: Duration,
}

/// Called once when an open session ends on its own (the link closed, a frame did not open).
pub(crate) type OnEnd = Arc<dyn Fn() + Send + Sync>;

type Pending = Arc<Mutex<HashMap<u32, mpsc::UnboundedSender<Down>>>>;

struct Shared {
    closed: AtomicBool,
    revoked: AtomicBool,
    pending: Pending,
}

impl Shared {
    fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
        self.pending.lock().unwrap().clear();
    }
}

pub(crate) struct Session {
    link: Link,
    channel: Arc<Mutex<Channel>>,
    send_lock: tokio::sync::Mutex<()>,
    shared: Arc<Shared>,
    next_id: AtomicU32,
    pub hello: PhoneHello,
    pub waits: Waits,
    /// Wall-clock time: a phone asleep for an hour must see its session as lapsed, which the
    /// monotonic clock (paused in sleep on Android) would not.
    pub opened_at: SystemTime,
    pump: tokio::task::JoinHandle<()>,
}

impl Drop for Session {
    fn drop(&mut self) {
        self.shared.close();
        self.link.close();
        self.pump.abort();
    }
}

impl Session {
    /// A WebSocket that opens and then closes before the session is established is tried again
    /// once, over long-poll.
    pub(crate) async fn open(init: &OpenInit<'_>, on_end: OnEnd) -> Result<Session, OpenError> {
        match Session::open_once(init, init.prefer_poll, on_end.clone()).await {
            Err(e) if e.socket_lost && !init.prefer_poll => {
                Session::open_once(init, true, on_end).await
            }
            other => other,
        }
    }

    async fn open_once(
        init: &OpenInit<'_>,
        prefer_poll: bool,
        on_end: OnEnd,
    ) -> Result<Session, OpenError> {
        let (link, mut inbox) =
            link::open(init.route, init.http, prefer_poll, init.socket_timeout).await;
        let identity = init.identity;
        let (frame, opening) = identity
            .session_open()
            .map_err(|_| failed(OpenFailure::Protocol))?;
        let deadline = Instant::now() + init.handshake;
        let sent = tokio::time::timeout(init.handshake, link.send(frame)).await;
        if !matches!(sent, Ok(Ok(()))) {
            return Err(match inbox.next(Some(Duration::ZERO)).await {
                Got::End(e) => failure_for(e),
                _ => failed(OpenFailure::Network),
            });
        }
        let left = || deadline.saturating_duration_since(Instant::now());
        let mut channel = loop {
            match inbox.next(Some(left())).await {
                Got::Timeout => return Err(failed(OpenFailure::Network)),
                Got::End(e) => return Err(failure_for(e)),
                Got::Frame(f) => match f.get(1).copied() {
                    Some(KIND_SESSION_ACCEPT) => {
                        break Session::accept(opening, &f, identity)?;
                    }
                    Some(KIND_REVOKED) if opening.revoked(&f, identity) => {
                        return Err(failed(OpenFailure::Revoked));
                    }
                    // A REVOKED that is not an answer to this open is ignored.
                    Some(KIND_REVOKED) => continue,
                    _ => return Err(failed(OpenFailure::Refused)),
                },
            }
        };
        link.set_sid(channel.sid());
        let hello = match inbox.next(Some(left())).await {
            Got::Timeout => return Err(failed(OpenFailure::Network)),
            Got::End(e) => return Err(failure_for(e)),
            Got::Frame(f) => match channel.open(&f) {
                Ok(Envelope::Json(body)) => match serde_json::from_slice::<RemoteReply>(&body) {
                    Ok(RemoteReply {
                        id: 0,
                        answer: RemoteAnswer::Hello { hello },
                    }) => hello,
                    _ => return Err(failed(OpenFailure::Protocol)),
                },
                _ => return Err(failed(OpenFailure::Protocol)),
            },
        };
        let shared = Arc::new(Shared {
            closed: AtomicBool::new(false),
            revoked: AtomicBool::new(false),
            pending: Arc::new(Mutex::new(HashMap::new())),
        });
        let channel = Arc::new(Mutex::new(channel));
        let pump = tokio::spawn(pump(
            inbox,
            channel.clone(),
            shared.clone(),
            init.quiet,
            on_end,
        ));
        Ok(Session {
            link,
            channel,
            send_lock: tokio::sync::Mutex::new(()),
            shared,
            next_id: AtomicU32::new(1),
            hello,
            waits: init.waits,
            opened_at: SystemTime::now(),
            pump,
        })
    }

    fn accept(
        opening: crate::phone::Opening,
        frame: &[u8],
        identity: &PhoneIdentity,
    ) -> Result<Channel, OpenError> {
        opening
            .accept(frame, identity)
            .map_err(|_| failed(OpenFailure::Refused))
    }

    pub(crate) fn kind(&self) -> LinkKind {
        self.link.kind()
    }

    pub(crate) fn is_open(&self) -> bool {
        !self.shared.closed.load(Ordering::SeqCst)
    }

    pub(crate) fn revoked(&self) -> bool {
        self.shared.revoked.load(Ordering::SeqCst)
    }

    pub(crate) fn expires_at(&self) -> SystemTime {
        self.opened_at + Duration::from_secs(SESSION_TTL_SECS)
    }

    pub(crate) fn close(&self) {
        self.shared.close();
        self.link.close();
    }

    async fn send_envelope(&self, env: &[u8]) -> Result<(), Fault> {
        let _order = self.send_lock.lock().await;
        if !self.is_open() {
            return Err(Fault::Closed);
        }
        let frame = self
            .channel
            .lock()
            .unwrap()
            .seal(env)
            .ok_or(Fault::Closed)?;
        // A frame that has not left within the wait (a socket nobody reads any more) ends the
        // session; each frame that leaves starts the wait again.
        let sent = tokio::time::timeout(self.waits.chunk, self.link.send(frame)).await;
        if !matches!(sent, Ok(Ok(()))) {
            self.close();
            return Err(Fault::Closed);
        }
        Ok(())
    }

    /// Sends one call; its answers and chunks come to the receiver, in order.
    pub(crate) async fn request(
        &self,
        call: RemoteCall,
    ) -> Result<(u32, mpsc::UnboundedReceiver<Down>), Fault> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = mpsc::unbounded_channel();
        if !self.is_open() {
            return Err(Fault::Closed);
        }
        self.shared.pending.lock().unwrap().insert(id, tx);
        let body = serde_json::to_vec(&RemoteRequest { id, call }).expect("a call serialises");
        self.send_envelope(&crypto::envelope_json(&body)).await?;
        Ok((id, rx))
    }

    /// One chunk of an upload, as a DATA frame.
    pub(crate) async fn chunk(
        &self,
        id: u32,
        seq: u32,
        last: bool,
        bytes: &[u8],
    ) -> Result<(), Fault> {
        self.send_envelope(&crypto::envelope_chunk(id, seq, last, bytes))
            .await
    }

    /// The next thing down for request `id`, or it is given up when nothing came for it within
    /// `wait`. A socket that died half way is the pump's to find: it hears nothing at all.
    pub(crate) async fn next_within(
        &self,
        id: u32,
        rx: &mut mpsc::UnboundedReceiver<Down>,
        wait: Duration,
    ) -> Result<Down, Fault> {
        match tokio::time::timeout(wait, next(rx)).await {
            Ok(down) => down,
            Err(_) => {
                self.finish(id);
                Err(Fault::TimedOut)
            }
        }
    }

    /// The request is answered; nothing more is routed to it.
    pub(crate) fn finish(&self, id: u32) {
        self.shared.pending.lock().unwrap().remove(&id);
    }
}

/// The next thing down for a request; an error answer, or one this build cannot read, becomes
/// a fault.
pub(crate) async fn next(rx: &mut mpsc::UnboundedReceiver<Down>) -> Result<Down, Fault> {
    match rx.recv().await {
        None => Err(Fault::Closed),
        Some(Down::Answer(RemoteAnswer::Error { code })) => Err(Fault::Phone(code)),
        Some(Down::Unreadable) => Err(Fault::Phone(PhoneError::Unknown)),
        Some(d) => Ok(d),
    }
}

/// A reply this build cannot read: its request fails, and the session stays open. Without an
/// id it names no request, and is dropped.
fn unreadable(body: &[u8]) -> Option<(u32, Down)> {
    let id = serde_json::from_slice::<serde_json::Value>(body).ok()?["id"].as_u64()?;
    Some((u32::try_from(id).ok()?, Down::Unreadable))
}

async fn pump(
    mut inbox: Inbox,
    channel: Arc<Mutex<Channel>>,
    shared: Arc<Shared>,
    quiet: Duration,
    on_end: OnEnd,
) {
    loop {
        // Silence for this long (no frame, ping or empty poll) is a socket gone half way.
        let frame = match inbox.next_idle(quiet).await {
            Got::Frame(f) => f,
            Got::End(_) | Got::Timeout => break,
        };
        let opened = channel.lock().unwrap().open(&frame);
        let (id, down) = match opened {
            Ok(Envelope::Json(body)) => match serde_json::from_slice::<RemoteReply>(&body) {
                Ok(RemoteReply {
                    answer: RemoteAnswer::Revoked,
                    ..
                }) => {
                    shared.revoked.store(true, Ordering::SeqCst);
                    break;
                }
                Ok(r) => (r.id, Down::Answer(r.answer)),
                Err(_) => match unreadable(&body) {
                    Some(got) => got,
                    None => continue,
                },
            },
            Ok(Envelope::Chunk(c)) => (c.call_id, Down::Chunk(c)),
            Err(_) => break,
        };
        if let Some(tx) = shared.pending.lock().unwrap().get(&id) {
            let _ = tx.send(down);
        }
    }
    shared.close();
    on_end();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_answer_or_error_from_a_newer_computer_is_unreadable_but_has_its_id() {
        for body in [
            br#"{"id":4,"answer":{"type":"somethingNew","extra":1}}"#.as_slice(),
            br#"{"id":4,"answer":{"type":"error","code":"somethingNew"}}"#,
            br#"{"id":4,"answer":{"type":"status","status":1}}"#,
        ] {
            assert!(serde_json::from_slice::<RemoteReply>(body).is_err());
            let (id, down) = unreadable(body).unwrap();
            assert_eq!(id, 4);
            assert!(matches!(down, Down::Unreadable));
        }
        assert!(unreadable(b"not json").is_none());
        assert!(unreadable(br#"{"answer":{}}"#).is_none());
    }

    #[tokio::test]
    async fn an_unreadable_reply_is_a_failed_call_and_the_next_one_reads() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        tx.send(Down::Unreadable).unwrap();
        tx.send(Down::Answer(RemoteAnswer::Done)).unwrap();
        assert!(matches!(
            next(&mut rx).await,
            Err(Fault::Phone(PhoneError::Unknown))
        ));
        assert!(matches!(
            next(&mut rx).await,
            Ok(Down::Answer(RemoteAnswer::Done))
        ));
    }
}
