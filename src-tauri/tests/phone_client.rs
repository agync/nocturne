//! The phone app's own client against the real listener: a relay on loopback stands where the
//! helper would, and the client reaches it through `dev-loopback`. It pairs scanned and typed,
//! opens a session, calls, uploads 5 MB in chunks, reads it back, asks, survives a dropped
//! socket, works over long-poll, renews an expired session and learns it was removed.
//!
//! `device_bench` (ignored) keeps such a computer running for a debug phone app behind
//! `adb reverse`; see docs/contribution/building.md.

#![cfg(unix)]

mod common;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use common::*;
use nocturne_app_lib::remote::testing;
use nocturne_channel::client::{
    self, CallError, Connection, OnIdentity, Options, PairFailure, Phone, Reach, Router,
};
use nocturne_channel::phone::{PhoneIdentity, PhonePairing};
use nocturne_contract::{PhoneReadiness, RemoteAnswer, RemoteCall, RemoteState};
use nocturne_engine::FakeMind;
use nocturne_helpers::{HelperKind, Origin};
use nocturne_intelligence::FakeHost;
use serde_json::{Value, json};

fn block<T>(f: impl std::future::Future<Output = T>) -> T {
    tauri::async_runtime::block_on(f)
}

const NAME: &str = "Ana\u{2019}s Pixel 9";
const SENTENCE: &str = "The heron feeder is refilled on Thursday.";

/// A relay on loopback in front of the listener's socket, whose connections a test can cut. It
/// counts the bytes the phone sends.
struct Relay {
    port: u16,
    live: Arc<Mutex<Vec<tokio::task::AbortHandle>>>,
    up: Arc<AtomicUsize>,
}

async fn pipe(
    mut from: impl tokio::io::AsyncRead + Unpin,
    mut to: impl tokio::io::AsyncWrite + Unpin,
    count: Option<Arc<AtomicUsize>>,
) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut buf = vec![0u8; 64 * 1024];
    while let Ok(n) = from.read(&mut buf).await {
        if n == 0 || to.write_all(&buf[..n]).await.is_err() {
            break;
        }
        if let Some(c) = &count {
            c.fetch_add(n, Ordering::SeqCst);
        }
    }
    let _ = to.shutdown().await;
}

impl Relay {
    fn start(socket: std::path::PathBuf, port: u16) -> Relay {
        let listener = block(tokio::net::TcpListener::bind(("127.0.0.1", port))).unwrap();
        let port = listener.local_addr().unwrap().port();
        let live: Arc<Mutex<Vec<tokio::task::AbortHandle>>> = Arc::default();
        let up: Arc<AtomicUsize> = Arc::default();
        let (tasks, counted) = (live.clone(), up.clone());
        tauri::async_runtime::spawn(async move {
            while let Ok((phone, _)) = listener.accept().await {
                let (socket, counted) = (socket.clone(), counted.clone());
                let task = tokio::spawn(async move {
                    if let Ok(computer) = tokio::net::UnixStream::connect(socket).await {
                        let (pr, pw) = tokio::io::split(phone);
                        let (cr, cw) = tokio::io::split(computer);
                        tokio::join!(pipe(pr, cw, Some(counted)), pipe(cr, pw, None));
                    }
                });
                tasks.lock().unwrap().push(task.abort_handle());
            }
        });
        Relay { port, live, up }
    }

    fn sent(&self) -> usize {
        self.up.load(Ordering::SeqCst)
    }

    /// Drop every connection through the relay, as a phone losing its signal would.
    fn cut(&self) {
        for t in self.live.lock().unwrap().drain(..) {
            t.abort();
        }
    }
}

struct Computer {
    h: Harness,
    hub: Arc<nocturne_app_lib::remote::controller::RemoteHub>,
    mono: Arc<nocturne_app_lib::remote::limits::ManualMono>,
    relay: Relay,
    router: Router,
    _files: tempfile::TempDir,
}

fn computer(port: u16) -> Computer {
    let h = harness_with_mind(Arc::new(FakeMind::new()));
    h.learn("heron note", SENTENCE, "Heron feeder", "Thursday");
    h.save_todays_page();
    let files = tempfile::tempdir().unwrap();
    std::fs::write(files.path().join("index.html"), b"<!doctype html>").unwrap();
    let (hub, helpers, mono) =
        testing::install_fakes(h.app.handle(), FakeHost::new(), files.path());
    helpers.install_now(HelperKind::Cloudflared);
    h.ok(
        "start_remote",
        json!({"connection": "simple", "allowDownload": false}),
    );
    wait_for("on", || matches!(hub.state(), RemoteState::On { .. }));
    let Some(Origin::Unix(socket)) = hub.listener_origin() else {
        panic!("a socket listener")
    };
    let relay = Relay::start(socket, port);
    let router = client::loopback_router(relay.port);
    Computer {
        h,
        hub,
        mono,
        relay,
        router,
        _files: files,
    }
}

impl Computer {
    fn waiting_for_you(&self) -> bool {
        matches!(
            block(self.hub.view()).pairing.map(|p| p.phase),
            Some(nocturne_contract::PairingPhase::WaitingForYou { .. })
        )
    }

    /// Pair through the client; the person allows it once the words are on screen.
    fn pair(
        &self,
        pairing: PhonePairing,
        allow: bool,
        offer: &Value,
    ) -> (Vec<String>, Result<PhoneIdentity, PairFailure>) {
        let words: Arc<Mutex<Vec<String>>> = Arc::default();
        let seen = words.clone();
        let router = self.router.clone();
        let run = tauri::async_runtime::spawn(async move {
            let http = client::http_client();
            client::pair(pairing, &router, &http, move |w| *seen.lock().unwrap() = w).await
        });
        wait_for("asked", || self.waiting_for_you());
        let asked = block(self.hub.view()).pairing.unwrap();
        let nocturne_contract::PairingPhase::WaitingForYou { request } = asked.phase else {
            unreachable!()
        };
        self.h.ok(
            "decide_pairing",
            json!({"pairingId": offer["pairingId"], "allow": allow}),
        );
        let out = block(run).unwrap();
        let shown = words.lock().unwrap().clone();
        assert_eq!(
            shown, request.check_phrase,
            "both screens show the same words"
        );
        (shown, out)
    }

    fn phone(
        &self,
        identity: PhoneIdentity,
        options: Options,
    ) -> (Phone, Arc<Mutex<Vec<Option<String>>>>) {
        let saved: Arc<Mutex<Vec<Option<String>>>> = Arc::default();
        let log = saved.clone();
        let on: OnIdentity = Arc::new(move |i: Option<&PhoneIdentity>| {
            log.lock().unwrap().push(i.map(|i| i.address.clone()));
        });
        (
            Phone::new(identity, self.router.clone(), options, on),
            saved,
        )
    }
}

fn scanned(c: &Computer) -> PhoneIdentity {
    let offer = c.h.ok("begin_pairing", json!({}));
    let pairing = PhonePairing::scanned(offer["link"].as_str().unwrap(), NAME).unwrap();
    let (_, out) = c.pair(pairing, true, &offer);
    out.unwrap()
}

fn photo(len: usize) -> Vec<u8> {
    let mut b = vec![0xff, 0xd8, 0xff, 0xe0, 0, 0x10, b'J', b'F', b'I', b'F'];
    while b.len() < len {
        b.extend_from_slice(&nocturne_channel::crypto::random::<32>());
    }
    b.truncate(len);
    b
}

fn keep_text(text: &str, id: &str) -> RemoteCall {
    RemoteCall::KeepText {
        text: text.into(),
        client_id: id.into(),
        feeling: None,
        good: None,
        captured_at: None,
    }
}

#[test]
fn pairs_scanned_and_uses_the_computer() {
    let c = computer(0);
    let identity = scanned(&c);
    assert_eq!(identity.address, testing::TEST_HOST);
    assert_eq!(identity.device_name, NAME);
    let back = PhoneIdentity::from_bytes(&identity.to_bytes()).unwrap();
    let (phone, _) = c.phone(back, Options::default());

    match block(phone.connect()) {
        Connection::Connected { hello } => assert_eq!(hello.device_name, NAME),
        other => panic!("{other:?}"),
    }
    let RemoteAnswer::Status { status } = block(phone.call(RemoteCall::GetStatus)).unwrap() else {
        panic!("a status")
    };
    assert_eq!(status.readiness, PhoneReadiness::Ready);
    assert!(matches!(
        block(phone.call(keep_text("Bins out on Tuesday.", "c-text-1"))).unwrap(),
        RemoteAnswer::Kept { .. }
    ));

    let big = photo(5 * 1024 * 1024);
    let call = RemoteCall::KeepBytes {
        mime: "image/jpeg".into(),
        size: big.len() as f64,
        client_id: "c-photo-1".into(),
        caption: Some("Blue door".into()),
        feeling: None,
        good: None,
        captured_at: None,
    };
    let RemoteAnswer::Kept { kept } = block(phone.upload(call, &big)).unwrap() else {
        panic!("the photo is kept")
    };
    let (mime, bytes) = block(phone.original(&kept.item.id)).unwrap();
    assert_eq!(mime, "image/jpeg");
    assert!(bytes == big, "the original comes back whole");

    let mut answer = String::new();
    let ask_id = block(phone.ask("When is the heron feeder refilled?", None, |d| {
        answer.push_str(d.text_delta.as_deref().unwrap_or_default());
    }))
    .unwrap();
    assert!(!ask_id.is_empty());
    assert!(answer.contains(SENTENCE), "{answer}");
    assert!(matches!(
        block(phone.call(RemoteCall::GetPage { date: None })).unwrap(),
        RemoteAnswer::Page { .. }
    ));
}

#[test]
fn pairs_typed_with_the_four_words() {
    let c = computer(0);
    let offer = c.h.ok("begin_pairing", json!({}));
    let code = offer["typedCode"].as_str().unwrap();
    let http = client::http_client();
    let address = block(client::check_address(
        &c.router,
        &http,
        "Amber Otter Lamp Quiet",
    ))
    .unwrap();
    assert_eq!(address, testing::TEST_HOST);
    let mut pairing =
        PhonePairing::typed(&code.to_lowercase(), "amber otter lamp quiet", NAME).unwrap();
    block(client::take_hello(&mut pairing, &c.router, &http)).unwrap();
    let (words, out) = c.pair(pairing, true, &offer);
    assert_eq!(words.len(), 3);
    let identity = out.unwrap();
    assert_eq!(identity.address, testing::TEST_HOST);
    let (phone, _) = c.phone(identity, Options::default());
    assert!(matches!(
        block(phone.connect()),
        Connection::Connected { .. }
    ));

    // A wrong code finds no offer; it never gets as far as the words.
    let wrong = nocturne_channel::crypto::code_encode(&nocturne_channel::crypto::random());
    let mut stray = PhonePairing::typed(&wrong, "amber otter lamp quiet", NAME).unwrap();
    assert_eq!(
        block(client::take_hello(&mut stray, &c.router, &http)),
        Err(PairFailure::Expired)
    );
}

#[test]
fn a_refusal_on_the_computer_pairs_nothing() {
    let c = computer(0);
    let offer = c.h.ok("begin_pairing", json!({}));
    let pairing = PhonePairing::scanned(offer["link"].as_str().unwrap(), NAME).unwrap();
    let (_, out) = c.pair(pairing, false, &offer);
    assert_eq!(out.err(), Some(PairFailure::Refused));
}

#[test]
fn a_dropped_socket_is_retried_on_a_new_session() {
    let c = computer(0);
    let (phone, _) = c.phone(scanned(&c), Options::default());
    assert!(matches!(
        block(phone.connect()),
        Connection::Connected { .. }
    ));
    let mut states = phone.connection();
    c.relay.cut();
    block(async {
        tokio::time::timeout(Duration::from_secs(5), states.changed())
            .await
            .unwrap()
            .unwrap()
    });
    assert!(matches!(
        block(phone.call(keep_text("After the tunnel.", "c-cut-1"))).unwrap(),
        RemoteAnswer::Kept { .. }
    ));
    assert!(matches!(phone.state(), Connection::Connected { .. }));
}

#[test]
fn an_upload_cut_mid_way_is_kept_once() {
    let c = computer(0);
    let (phone, _) = c.phone(scanned(&c), Options::default());
    assert!(matches!(
        block(phone.connect()),
        Connection::Connected { .. }
    ));
    let big = photo(8 * 1024 * 1024);
    let call = RemoteCall::KeepBytes {
        mime: "image/jpeg".into(),
        size: big.len() as f64,
        client_id: "c-cut-photo".into(),
        caption: None,
        feeling: None,
        good: None,
        captured_at: None,
    };
    let before = c.relay.sent();
    let (p, call2, big2) = (phone.clone(), call.clone(), big.clone());
    let run = tauri::async_runtime::spawn(async move { p.upload(call2, &big2).await });
    // The request and the first chunks have left; the answer has not come.
    wait_for("part of the upload sent", || {
        c.relay.sent() > before + 2 * 1024 * 1024
    });
    c.relay.cut();
    let RemoteAnswer::Kept { kept } = block(run).unwrap().unwrap() else {
        panic!("kept on the retry")
    };
    assert!(
        c.relay.sent() - before > big.len() + 2 * 1024 * 1024,
        "the bytes went twice: the cut landed mid-way and the call was sent again"
    );
    let RemoteAnswer::Kept { kept: again } = block(phone.upload(call, &big)).unwrap() else {
        panic!("kept again")
    };
    assert_eq!(kept.item.id, again.item.id, "one item for the client id");
    let (_, back) = block(phone.original(&kept.item.id)).unwrap();
    assert!(back == big, "the kept bytes are whole");
}

#[test]
fn long_poll_works_the_same() {
    let c = computer(0);
    let options = Options {
        prefer_poll: true,
        ..Options::default()
    };
    let (phone, _) = c.phone(scanned(&c), options);
    assert!(matches!(
        block(phone.call(RemoteCall::GetStatus)).unwrap(),
        RemoteAnswer::Status { .. }
    ));
    let small = photo(600 * 1024);
    let call = RemoteCall::KeepBytes {
        mime: "image/jpeg".into(),
        size: small.len() as f64,
        client_id: "c-poll-photo".into(),
        caption: None,
        feeling: None,
        good: None,
        captured_at: None,
    };
    let RemoteAnswer::Kept { kept } = block(phone.upload(call, &small)).unwrap() else {
        panic!("kept over long-poll")
    };
    assert!(block(phone.original(&kept.item.id)).unwrap().1 == small);
}

#[test]
fn an_expired_session_is_renewed() {
    let c = computer(0);
    let (phone, _) = c.phone(scanned(&c), Options::default());
    assert!(matches!(
        block(phone.connect()),
        Connection::Connected { .. }
    ));
    c.mono.advance(Duration::from_secs(
        nocturne_channel::protocol::SESSION_TTL_SECS + 5,
    ));
    assert!(matches!(
        block(phone.call(keep_text("Later on.", "c-late-1"))).unwrap(),
        RemoteAnswer::Kept { .. }
    ));
}

#[test]
fn removed_on_the_computer_forgets_the_phone() {
    let c = computer(0);
    let identity = scanned(&c);
    let id = identity.id();
    let (phone, saved) = c.phone(identity, Options::default());
    assert!(matches!(
        block(phone.connect()),
        Connection::Connected { .. }
    ));
    c.h.ok("remove_device", json!({"id": id}));
    let got = block(phone.call(RemoteCall::GetStatus));
    assert_eq!(
        got.err(),
        Some(CallError::Connection {
            state: Reach::Removed
        })
    );
    assert_eq!(phone.state(), Connection::Removed);
    assert!(phone.identity().is_none());
    assert_eq!(saved.lock().unwrap().last(), Some(&None));
}

#[test]
fn only_https_without_loopback() {
    assert!(client::https_router()("127.0.0.1").is_none());
    assert!(client::https_router()(testing::TEST_HOST).is_some());
}

/// A computer for a debug phone app on a real phone or emulator: it serves the listener on
/// loopback at `NOCTURNE_PHONE_BENCH_PORT`, prints the pairing link and code, allows the pairing
/// a few seconds after the words are shown, and keeps running for `NOCTURNE_PHONE_BENCH_SECS`.
#[test]
#[ignore = "manual: run with --ignored beside `adb reverse`"]
fn device_bench() {
    let port: u16 = std::env::var("NOCTURNE_PHONE_BENCH_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8799);
    let secs: u64 = std::env::var("NOCTURNE_PHONE_BENCH_SECS")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(600);
    let c = computer(port);
    let offer = c.h.ok("begin_pairing", json!({}));
    println!("bench on 127.0.0.1:{}", c.relay.port);
    println!("link: {}", offer["link"].as_str().unwrap());
    println!("words: amber otter lamp quiet");
    println!("code: {}", offer["typedCode"].as_str().unwrap());
    let until = std::time::Instant::now() + Duration::from_secs(secs);
    let mut allowed = false;
    while std::time::Instant::now() < until {
        if !allowed && c.waiting_for_you() {
            let asked = block(c.hub.view()).pairing.unwrap();
            if let nocturne_contract::PairingPhase::WaitingForYou { request } = asked.phase {
                println!("words on the computer: {}", request.check_phrase.join(" "));
            }
            // Long enough to see the words on the phone first, as a person would.
            std::thread::sleep(Duration::from_secs(8));
            c.h.ok(
                "decide_pairing",
                json!({"pairingId": offer["pairingId"], "allow": true}),
            );
            println!("allowed");
            allowed = true;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}
