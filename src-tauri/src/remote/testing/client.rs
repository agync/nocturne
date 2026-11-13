//! A phone on the wire: plain HTTP/1.1 and a minimal WebSocket client over the listener's Unix
//! socket or loopback port, and [`PhoneClient`], which pairs and opens sessions through them.

use std::path::PathBuf;
use std::time::Duration;

use nocturne_contract::{PhoneHello, RemoteAnswer, RemoteCall, RemoteReply, RemoteRequest};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use super::super::crypto::{self, Chunk, Envelope};
use super::super::frame::{Channel, Side};
use super::super::protocol::*;
use super::{PhoneKeys, PhonePair};

/// Where the listener (or a relay in front of it) is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Addr {
    Unix(PathBuf),
    Tcp(u16),
}

trait Io: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Io for T {}

async fn connect(addr: &Addr) -> std::io::Result<Box<dyn Io>> {
    Ok(match addr {
        #[cfg(unix)]
        Addr::Unix(p) => Box::new(tokio::net::UnixStream::connect(p).await?),
        #[cfg(not(unix))]
        Addr::Unix(_) => return Err(std::io::Error::other("no unix sockets here")),
        Addr::Tcp(port) => Box::new(tokio::net::TcpStream::connect(("127.0.0.1", *port)).await?),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Http {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    /// Everything that came back, as sent.
    pub raw: Vec<u8>,
}

impl Http {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    pub fn headers_named(&self, name: &str) -> Vec<&str> {
        self.headers
            .iter()
            .filter(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
            .collect()
    }
}

type Head = (u16, Vec<(String, String)>, usize);

fn parse_head(raw: &[u8]) -> Option<Head> {
    let end = raw.windows(4).position(|w| w == b"\r\n\r\n")?;
    let head = std::str::from_utf8(&raw[..end]).ok()?;
    let mut lines = head.split("\r\n");
    let status = lines.next()?.split(' ').nth(1)?.parse().ok()?;
    let headers = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect();
    Some((status, headers, end + 4))
}

/// One request with `Connection: close`; reads to the end.
pub async fn http(
    addr: &Addr,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &[u8],
) -> std::io::Result<Http> {
    let mut s = connect(addr).await?;
    let mut req = format!("{method} {path} HTTP/1.1\r\nconnection: close\r\n");
    if !headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("host")) {
        req.push_str("host: phone.test\r\n");
    }
    for (k, v) in headers {
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    if !headers
        .iter()
        .any(|(k, _)| k.eq_ignore_ascii_case("content-length"))
    {
        req.push_str(&format!("content-length: {}\r\n", body.len()));
    }
    req.push_str("\r\n");
    s.write_all(req.as_bytes()).await?;
    s.write_all(body).await?;
    let mut raw = Vec::new();
    s.read_to_end(&mut raw).await?;
    let (status, headers, at) =
        parse_head(&raw).ok_or_else(|| std::io::Error::other("not an HTTP response"))?;
    Ok(Http {
        status,
        headers,
        body: raw[at..].to_vec(),
        raw,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WsMsg {
    Binary(Vec<u8>),
    Close(u16),
    Eof,
}

pub struct Ws {
    s: Box<dyn Io>,
    buf: Vec<u8>,
}

impl Ws {
    /// Open a WebSocket on `path`. `Err` carries the HTTP answer when it was not an upgrade.
    pub async fn connect(addr: &Addr, path: &str, headers: &[(&str, &str)]) -> Result<Ws, Http> {
        let fail = |e: std::io::Error| Http {
            status: 0,
            headers: Vec::new(),
            body: e.to_string().into_bytes(),
            raw: Vec::new(),
        };
        let mut s = connect(addr).await.map_err(fail)?;
        let key = crypto::b64(&crypto::random::<16>()) + "==";
        let mut req = format!(
            "GET {path} HTTP/1.1\r\nconnection: Upgrade\r\nupgrade: websocket\r\n\
             sec-websocket-version: 13\r\nsec-websocket-key: {key}\r\n"
        );
        if !headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("host")) {
            req.push_str("host: phone.test\r\n");
        }
        for (k, v) in headers {
            req.push_str(&format!("{k}: {v}\r\n"));
        }
        req.push_str("\r\n");
        s.write_all(req.as_bytes()).await.map_err(fail)?;
        let mut raw = Vec::new();
        loop {
            let mut b = [0u8; 1024];
            let n = s.read(&mut b).await.map_err(fail)?;
            if n == 0 {
                break;
            }
            raw.extend_from_slice(&b[..n]);
            if raw.windows(4).any(|w| w == b"\r\n\r\n") {
                break;
            }
        }
        let (status, hdrs, at) =
            parse_head(&raw).ok_or_else(|| fail(std::io::Error::other("bad")))?;
        if status != 101 {
            let mut rest = raw[at..].to_vec();
            let _ = tokio::time::timeout(Duration::from_secs(2), s.read_to_end(&mut rest)).await;
            return Err(Http {
                status,
                headers: hdrs,
                body: rest,
                raw,
            });
        }
        Ok(Ws {
            s,
            buf: raw[at..].to_vec(),
        })
    }

    async fn frame(&mut self, opcode: u8, data: &[u8]) -> std::io::Result<()> {
        let mut out = vec![0x80 | opcode];
        match data.len() {
            n if n < 126 => out.push(0x80 | n as u8),
            n if n < 65536 => {
                out.push(0x80 | 126);
                out.extend_from_slice(&(n as u16).to_be_bytes());
            }
            n => {
                out.push(0x80 | 127);
                out.extend_from_slice(&(n as u64).to_be_bytes());
            }
        }
        let mask = crypto::random::<4>();
        out.extend_from_slice(&mask);
        out.extend(data.iter().enumerate().map(|(i, b)| b ^ mask[i % 4]));
        self.s.write_all(&out).await
    }

    pub async fn send(&mut self, data: &[u8]) -> std::io::Result<()> {
        self.frame(2, data).await
    }

    pub async fn send_text(&mut self, data: &str) -> std::io::Result<()> {
        self.frame(1, data.as_bytes()).await
    }

    async fn fill(&mut self, n: usize) -> bool {
        while self.buf.len() < n {
            let mut b = vec![0u8; 65536];
            match self.s.read(&mut b).await {
                Ok(0) | Err(_) => return false,
                Ok(k) => self.buf.extend_from_slice(&b[..k]),
            }
        }
        true
    }

    pub async fn recv(&mut self) -> WsMsg {
        loop {
            if !self.fill(2).await {
                return WsMsg::Eof;
            }
            let opcode = self.buf[0] & 0x0f;
            let (len, at) = match self.buf[1] & 0x7f {
                126 => {
                    if !self.fill(4).await {
                        return WsMsg::Eof;
                    }
                    (u16::from_be_bytes([self.buf[2], self.buf[3]]) as usize, 4)
                }
                127 => {
                    if !self.fill(10).await {
                        return WsMsg::Eof;
                    }
                    (
                        u64::from_be_bytes(self.buf[2..10].try_into().unwrap()) as usize,
                        10,
                    )
                }
                n => (n as usize, 2),
            };
            if !self.fill(at + len).await {
                return WsMsg::Eof;
            }
            let payload = self.buf[at..at + len].to_vec();
            self.buf.drain(..at + len);
            match opcode {
                2 => return WsMsg::Binary(payload),
                8 => {
                    let code = payload
                        .get(..2)
                        .map(|c| u16::from_be_bytes([c[0], c[1]]))
                        .unwrap_or(1005);
                    return WsMsg::Close(code);
                }
                _ => continue,
            }
        }
    }
}

/// How a phone reaches the listener.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    WebSocket,
    LongPoll,
}

/// One conversation, over either transport. Every frame up and down is recorded.
pub struct Wire {
    addr: Addr,
    ws: Option<Ws>,
    /// Long-poll: whether this wire polls, and the session or pairing id it polls under.
    poll: Option<Option<[u8; SID_LEN]>>,
    inbox: std::collections::VecDeque<Vec<u8>>,
    pub sent: Vec<Vec<u8>>,
    pub received: Vec<Vec<u8>>,
    /// The listener's last answer status for long-poll (403: the unsealed refusal).
    pub last_status: u16,
    pub headers: Vec<(String, String)>,
}

/// Why a conversation ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum End {
    /// The unsealed refusal ("can't connect").
    Refused,
    Closed,
}

impl Wire {
    pub async fn open(addr: &Addr, mode: Mode) -> Wire {
        Wire::open_with(addr, mode, &[]).await
    }

    pub async fn open_with(addr: &Addr, mode: Mode, headers: &[(&str, &str)]) -> Wire {
        let mut w = Wire {
            addr: addr.clone(),
            ws: None,
            poll: None,
            inbox: Default::default(),
            sent: Vec::new(),
            received: Vec::new(),
            last_status: 0,
            headers: headers
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        };
        match mode {
            Mode::WebSocket => {
                w.ws = Some(
                    Ws::connect(addr, PATH_SESSION, headers)
                        .await
                        .unwrap_or_else(|h| panic!("no WebSocket: {}", h.status)),
                )
            }
            Mode::LongPoll => w.poll = Some(None),
        }
        w
    }

    fn hdrs(&self) -> Vec<(&str, &str)> {
        self.headers
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect()
    }

    pub async fn send(&mut self, frame: &[u8]) -> Result<(), End> {
        self.sent.push(frame.to_vec());
        if let Some(ws) = self.ws.as_mut() {
            return ws.send(frame).await.map_err(|_| End::Closed);
        }
        if let Ok(h) = crypto::parse_header(frame)
            && h.sid != [0u8; SID_LEN]
        {
            self.poll = Some(Some(h.sid));
        }
        let mut body = (frame.len() as u32).to_be_bytes().to_vec();
        body.extend_from_slice(frame);
        let mut hdrs = self.hdrs();
        hdrs.push(("content-type", "text/plain"));
        let res = http(&self.addr, "POST", PATH_SESSION, &hdrs, &body)
            .await
            .map_err(|_| End::Closed)?;
        self.last_status = res.status;
        match res.status {
            200 => {
                let frames =
                    super::super::listener::split_frames(&res.body).expect("a batch of frames");
                for f in frames {
                    self.take_sid(f);
                    self.inbox.push_back(f.to_vec());
                }
                Ok(())
            }
            204 => Ok(()),
            403 => Err(End::Refused),
            _ => Err(End::Closed),
        }
    }

    /// A SESSION_ACCEPT names the session to poll under from then on.
    fn take_sid(&mut self, frame: &[u8]) {
        if frame.get(1) == Some(&KIND_SESSION_ACCEPT)
            && let Ok(h) = crypto::parse_header(frame)
        {
            self.poll = Some(Some(h.sid));
        }
    }

    /// The next frame down, waiting at most `wait`.
    pub async fn recv_within(&mut self, wait: Duration) -> Result<Vec<u8>, End> {
        match tokio::time::timeout(wait, self.recv()).await {
            Ok(r) => r,
            Err(_) => Err(End::Closed),
        }
    }

    pub async fn recv(&mut self) -> Result<Vec<u8>, End> {
        if let Some(f) = self.inbox.pop_front() {
            self.received.push(f.clone());
            return Ok(f);
        }
        if let Some(ws) = self.ws.as_mut() {
            return match ws.recv().await {
                WsMsg::Binary(f) => {
                    self.received.push(f.clone());
                    Ok(f)
                }
                WsMsg::Close(REFUSED) => Err(End::Refused),
                _ => Err(End::Closed),
            };
        }
        let Some(Some(sid)) = self.poll else {
            return Err(End::Closed);
        };
        let hex: String = sid.iter().map(|b| format!("{b:02x}")).collect();
        loop {
            let hdrs = self.hdrs();
            let res = http(
                &self.addr,
                "GET",
                &format!("{PATH_SESSION}?sid={hex}"),
                &hdrs,
                &[],
            )
            .await
            .map_err(|_| End::Closed)?;
            self.last_status = res.status;
            match res.status {
                200 => {
                    let frames =
                        super::super::listener::split_frames(&res.body).expect("a batch of frames");
                    self.inbox.extend(frames.into_iter().map(<[u8]>::to_vec));
                    let f = self.inbox.pop_front().unwrap();
                    self.received.push(f.clone());
                    return Ok(f);
                }
                204 => continue,
                403 => return Err(End::Refused),
                _ => return Err(End::Closed),
            }
        }
    }

    /// Close the WebSocket, or stop polling.
    pub async fn close(mut self) {
        self.close_ref().await;
    }

    /// Close without giving the wire up (the phone's tab went away).
    pub async fn close_ref(&mut self) {
        if let Some(mut ws) = self.ws.take() {
            let _ = ws.frame(8, &1000u16.to_be_bytes()).await;
            let _ = ws.s.shutdown().await;
        }
        self.poll = None;
    }
}

const REFUSED: u16 = super::super::listener::REFUSED_CLOSE;

/// A pairing in progress over the wire.
pub struct Pairing {
    pub phone: PhonePair,
    pub wire: Wire,
}

impl Pairing {
    /// Commit and reveal; returns once the computer has the request (the words are on S6).
    pub async fn start(addr: &Addr, mut phone: PhonePair, mode: Mode) -> Result<Pairing, End> {
        let mut wire = Wire::open(addr, mode).await;
        if phone.path == PATH_TYPED {
            wire.send(&phone.hello()).await?;
            let answer = wire.recv_within(Duration::from_secs(5)).await?;
            phone.take_hello(&answer).map_err(|_| End::Closed)?;
        }
        wire.send(&phone.commit()).await?;
        let nonce = wire.recv_within(Duration::from_secs(5)).await?;
        phone.take_nonce(&nonce).map_err(|_| End::Closed)?;
        wire.send(&phone.reveal()).await?;
        Ok(Pairing { phone, wire })
    }

    /// Wait for the person's decision.
    pub async fn outcome(mut self) -> Result<Option<PhoneKeys>, End> {
        let frame = self.wire.recv_within(Duration::from_secs(30)).await?;
        let out = self.phone.take_outcome(&frame).map_err(|_| End::Closed)?;
        Ok(out.allowed.then(|| self.phone.paired(&out)))
    }
}

/// What came down for a session.
#[derive(Debug, Clone, PartialEq)]
pub enum Down {
    Reply(RemoteReply),
    Chunk(Chunk),
}

/// How a session open ended.
#[derive(Debug)]
pub enum Opened {
    Session(Box<PhoneClient>, PhoneHello),
    /// A sealed REVOKED for this open: the phone forgets its key.
    Revoked,
    /// The unsealed refusal: the phone keeps its key and shows "can't connect".
    Refused,
    Closed,
}

/// A paired phone with an open session.
pub struct PhoneClient {
    pub wire: Wire,
    channel: Channel,
    next_id: u32,
    pending: std::collections::VecDeque<Down>,
}

impl std::fmt::Debug for PhoneClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PhoneClient")
    }
}

impl PhoneClient {
    /// Open a session: SESSION_OPEN, then SESSION_ACCEPT and the first down frame (`Hello`).
    pub async fn open(addr: &Addr, keys: &PhoneKeys, mode: Mode) -> Opened {
        PhoneClient::open_on(Wire::open(addr, mode).await, keys).await
    }

    pub async fn open_on(mut wire: Wire, keys: &PhoneKeys) -> Opened {
        let e = crypto::new_secret();
        let epub = crypto::public_bytes(&e.public_key());
        let on = crypto::random::<OPEN_NONCE_LEN>();
        let kd = keys.kd();
        match wire
            .send(&crypto::session_open(&keys.device_id, &epub, &on, &kd))
            .await
        {
            Ok(()) => {}
            Err(End::Refused) => return Opened::Refused,
            Err(End::Closed) => return Opened::Closed,
        }
        let first = match wire.recv_within(Duration::from_secs(5)).await {
            Ok(f) => f,
            Err(End::Refused) => return Opened::Refused,
            Err(End::Closed) => return Opened::Closed,
        };
        if first.get(1) == Some(&KIND_REVOKED) {
            return match crypto::revoked_open(&first, &kd, &keys.device_id, &on) {
                Ok(()) => Opened::Revoked,
                Err(_) => Opened::Closed,
            };
        }
        let Ok((sid, fkey)) = crypto::session_accept_open(&first) else {
            return Opened::Closed;
        };
        let ckey = crypto::check_public_key(&keys.cpub).unwrap();
        let fpub = crypto::public_bytes(&fkey);
        let dpub = crypto::public_bytes(&keys.d.public_key());
        let session = crypto::session_keys_phone(
            &e,
            &keys.d,
            &fkey,
            &ckey,
            &crypto::SessionInput {
                device_id: &keys.device_id,
                epub: &epub,
                on: &on,
                sid: &sid,
                fpub: &fpub,
                cpub: &keys.cpub,
                dpub: &dpub,
            },
        );
        let mut client = PhoneClient {
            wire,
            channel: Channel::new(Side::Phone, sid, session),
            next_id: 1,
            pending: Default::default(),
        };
        match client.next().await {
            Ok(Down::Reply(RemoteReply {
                id: 0,
                answer: RemoteAnswer::Hello { hello },
            })) => Opened::Session(Box::new(client), hello),
            _ => Opened::Closed,
        }
    }

    pub fn sid(&self) -> [u8; SID_LEN] {
        *self.channel.sid()
    }

    pub async fn send_envelope(&mut self, env: &[u8]) -> Result<(), End> {
        let frame = self.channel.seal(env).ok_or(End::Closed)?;
        self.wire.send(&frame).await
    }

    /// Send a raw frame (tests that tamper).
    pub async fn send_raw(&mut self, frame: &[u8]) -> Result<(), End> {
        self.wire.send(frame).await
    }

    /// Seal a frame without sending it.
    pub fn seal(&mut self, env: &[u8]) -> Vec<u8> {
        self.channel.seal(env).expect("open")
    }

    /// Send a call; returns its request id.
    pub async fn request(&mut self, call: RemoteCall) -> Result<u32, End> {
        let id = self.next_id;
        self.next_id += 1;
        let body = serde_json::to_vec(&RemoteRequest { id, call }).unwrap();
        self.send_envelope(&crypto::envelope_json(&body)).await?;
        Ok(id)
    }

    pub async fn chunk(&mut self, id: u32, seq: u32, last: bool, bytes: &[u8]) -> Result<(), End> {
        self.send_envelope(&crypto::envelope_chunk(id, seq, last, bytes))
            .await
    }

    /// The next thing down.
    pub async fn next(&mut self) -> Result<Down, End> {
        if let Some(d) = self.pending.pop_front() {
            return Ok(d);
        }
        let frame = self.wire.recv().await?;
        match self.channel.open(&frame).map_err(|_| End::Closed)? {
            Envelope::Json(body) => Ok(Down::Reply(
                serde_json::from_slice(&body).map_err(|_| End::Closed)?,
            )),
            Envelope::Chunk(c) => Ok(Down::Chunk(c)),
        }
    }

    pub async fn next_within(&mut self, wait: Duration) -> Result<Down, End> {
        tokio::time::timeout(wait, self.next())
            .await
            .unwrap_or(Err(End::Closed))
    }

    /// The next answer to request `id`, keeping others for later.
    pub async fn answer(&mut self, id: u32) -> Result<RemoteAnswer, End> {
        let mut skipped = Vec::new();
        let out = loop {
            match self.next_within(Duration::from_secs(20)).await {
                Ok(Down::Reply(r)) if r.id == id => break Ok(r.answer),
                Ok(other) => skipped.push(other),
                Err(e) => break Err(e),
            }
        };
        for s in skipped.into_iter().rev() {
            self.pending.push_front(s);
        }
        out
    }

    /// Send a call and wait for its first answer.
    pub async fn call(&mut self, call: RemoteCall) -> Result<RemoteAnswer, End> {
        let id = self.request(call).await?;
        self.answer(id).await
    }
}
