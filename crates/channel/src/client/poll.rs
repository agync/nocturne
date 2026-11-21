//! The long-poll link, as the listener serves it: `POST /s` carries frames up and answers with
//! any frames ready; `GET /s?sid=<32 hex>` is held up to `LONG_POLL_HOLD_SECS` and answers 200
//! with frames or 204 for none. At most one of each is in flight.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::link::{InboxTx, Incoming, LinkEnd, Route, frame_run, parse_run};
use crate::protocol::*;

/// How long to wait before trying again after a busy answer.
const BUSY_WAIT: Duration = Duration::from_secs(1);
/// How many busy answers to one POST before the link gives up.
const POST_BUSY_TRIES: u32 = 5;

struct Inner {
    url: String,
    http: reqwest::Client,
    inbox: InboxTx,
    closed: AtomicBool,
    posting: tokio::sync::Mutex<()>,
    sid: Mutex<Option<String>>,
    down: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

pub(crate) struct PollLink {
    inner: Arc<Inner>,
}

enum Exchange {
    Frames(Vec<Vec<u8>>),
    Busy,
    Ended,
}

impl Inner {
    fn end(&self, end: LinkEnd) {
        if !self.closed.swap(true, Ordering::SeqCst) {
            let _ = self.inbox.send(Incoming::End(end));
        }
    }

    async fn exchange(&self, req: reqwest::RequestBuilder) -> Exchange {
        let res = match req.send().await {
            Ok(r) => r,
            Err(_) => {
                self.end(LinkEnd::Network);
                return Exchange::Ended;
            }
        };
        if self.closed.load(Ordering::SeqCst) {
            return Exchange::Ended;
        }
        match res.status().as_u16() {
            200 => match res.bytes().await.ok().and_then(|b| parse_run(&b)) {
                Some(frames) => Exchange::Frames(frames),
                None => {
                    self.end(LinkEnd::Protocol);
                    Exchange::Ended
                }
            },
            204 => Exchange::Frames(Vec::new()),
            HTTP_POLL_BUSY => Exchange::Busy,
            status => {
                self.end(LinkEnd::Http { status });
                Exchange::Ended
            }
        }
    }

    fn take(&self, frames: Vec<Vec<u8>>) {
        for f in frames {
            let _ = self.inbox.send(Incoming::Frame(f));
        }
    }
}

impl PollLink {
    pub(crate) fn new(route: &Route, http: reqwest::Client, inbox: InboxTx) -> PollLink {
        PollLink {
            inner: Arc::new(Inner {
                url: route.url(PATH_SESSION),
                http,
                inbox,
                closed: AtomicBool::new(false),
                posting: tokio::sync::Mutex::new(()),
                sid: Mutex::new(None),
                down: Mutex::new(None),
            }),
        }
    }

    pub(crate) async fn send(&self, frame: Vec<u8>) -> Result<(), ()> {
        let inner = &self.inner;
        let _one = inner.posting.lock().await;
        let body = frame_run(&[&frame]);
        for _ in 0..=POST_BUSY_TRIES {
            if inner.closed.load(Ordering::SeqCst) {
                return Err(());
            }
            let req = inner
                .http
                .post(&inner.url)
                .header("content-type", "text/plain")
                .header("cache-control", "no-store")
                .timeout(Duration::from_secs(LONG_POLL_HOLD_SECS + 10))
                .body(body.clone());
            match inner.exchange(req).await {
                Exchange::Frames(frames) => {
                    inner.take(frames);
                    return Ok(());
                }
                // The computer had no room to read this body: nothing was taken, so send again.
                Exchange::Busy => tokio::time::sleep(BUSY_WAIT).await,
                Exchange::Ended => return Err(()),
            }
        }
        inner.end(LinkEnd::Http {
            status: HTTP_POLL_BUSY,
        });
        Err(())
    }

    pub(crate) fn set_sid(&self, sid: &[u8]) {
        let hex: String = sid.iter().map(|b| format!("{b:02x}")).collect();
        *self.inner.sid.lock().unwrap() = Some(hex);
        let mut down = self.inner.down.lock().unwrap();
        if down.is_some() || self.inner.closed.load(Ordering::SeqCst) {
            return;
        }
        let inner = self.inner.clone();
        *down = Some(tokio::spawn(async move {
            while !inner.closed.load(Ordering::SeqCst) {
                let Some(sid) = inner.sid.lock().unwrap().clone() else {
                    break;
                };
                let req = inner
                    .http
                    .get(format!("{}?sid={sid}", inner.url))
                    .header("cache-control", "no-store")
                    .timeout(Duration::from_secs(LONG_POLL_HOLD_SECS + 10));
                match inner.exchange(req).await {
                    Exchange::Frames(frames) => inner.take(frames),
                    // Another poll for this id is still held: wait, then poll again.
                    Exchange::Busy => tokio::time::sleep(BUSY_WAIT).await,
                    Exchange::Ended => break,
                }
            }
        }));
    }

    pub(crate) fn close(&self) {
        self.inner.closed.store(true, Ordering::SeqCst);
        if let Some(h) = self.inner.down.lock().unwrap().take() {
            h.abort();
        }
    }
}
