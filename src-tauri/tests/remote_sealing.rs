//! The relay sees no content: a recording relay sits where the helper would, and a scripted
//! phone pairs, keeps a marked text and a photo, asks, reads the page, opens the photo and
//! renews its session, once with the scanned code and once with the typed code.

mod common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use common::*;
use nocturne_app_lib::remote::crypto;
use nocturne_app_lib::remote::testing::client::{Addr, Down, Mode, Opened, Pairing, PhoneClient};
use nocturne_app_lib::remote::testing::{self, PhoneKeys, PhonePair};
use nocturne_contract::{RemoteAnswer, RemoteCall, RemoteState};
use nocturne_engine::FakeMind;
use nocturne_helpers::{HelperKind, Origin};
use nocturne_intelligence::FakeHost;
use serde_json::json;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};

fn block<T>(f: impl std::future::Future<Output = T>) -> T {
    tauri::async_runtime::block_on(f)
}

type Tape = Arc<Mutex<Vec<u8>>>;

/// Copy one way, writing everything that passes onto the tape.
async fn pipe(mut from: impl AsyncRead + Unpin, mut to: impl AsyncWrite + Unpin, tape: Tape) {
    use tokio::io::AsyncReadExt;
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = match from.read(&mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        tape.lock().unwrap().extend_from_slice(&buf[..n]);
        if to.write_all(&buf[..n]).await.is_err() {
            break;
        }
    }
    let _ = to.shutdown().await;
}

/// A relay on loopback in front of the listener; returns its port and the tape.
fn relay(target: Addr) -> (u16, Tape) {
    let tape: Tape = Arc::new(Mutex::new(Vec::new()));
    let listener = block(tokio::net::TcpListener::bind("127.0.0.1:0")).unwrap();
    let port = listener.local_addr().unwrap().port();
    let rec = tape.clone();
    tauri::async_runtime::spawn(async move {
        while let Ok((phone, _)) = listener.accept().await {
            let (tape, target) = (rec.clone(), target.clone());
            tokio::spawn(async move {
                let (pr, pw) = tokio::io::split(phone);
                match target {
                    #[cfg(unix)]
                    Addr::Unix(p) => {
                        let Ok(up) = tokio::net::UnixStream::connect(p).await else {
                            return;
                        };
                        let (ur, uw) = tokio::io::split(up);
                        tokio::join!(pipe(pr, uw, tape.clone()), pipe(ur, pw, tape));
                    }
                    #[cfg(not(unix))]
                    Addr::Unix(_) => {}
                    Addr::Tcp(port) | Addr::Keyed { port, .. } => {
                        let Ok(up) = tokio::net::TcpStream::connect(("127.0.0.1", port)).await
                        else {
                            return;
                        };
                        let (ur, uw) = tokio::io::split(up);
                        tokio::join!(pipe(pr, uw, tape.clone()), pipe(ur, pw, tape));
                    }
                }
            });
        }
    });
    (port, tape)
}

const MARKER: &str = "MARKER-a81f3c tell nobody about the blue door";
const SENTENCE: &str = "The heron feeder is refilled on Thursday.";
const NAME: &str = "Ana\u{2019}s Pixel 9";

fn photo() -> Vec<u8> {
    let mut b = vec![0xff, 0xd8, 0xff, 0xe0, 0, 0x10, b'J', b'F', b'I', b'F'];
    while b.len() < 300 * 1024 {
        b.extend_from_slice(&crypto::random::<32>());
    }
    b
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && hay.windows(needle.len()).any(|w| w == needle)
}

fn run(path: u8) {
    let h = harness_with_mind(Arc::new(FakeMind::new()));
    h.learn("heron note", SENTENCE, "Heron feeder", "Thursday");
    let page = h.save_todays_page();
    let files = tempfile::tempdir().unwrap();
    std::fs::write(files.path().join("index.html"), b"<!doctype html>").unwrap();
    let (hub, helpers, _mono) =
        testing::install_fakes(h.app.handle(), FakeHost::new(), files.path());
    helpers.install_now(HelperKind::Cloudflared);
    h.ok(
        "start_remote",
        json!({"connection": "simple", "allowDownload": false}),
    );
    wait_for("on", || matches!(hub.state(), RemoteState::On { .. }));
    // On Windows the phone's requests carry the listener's key, as cloudflared's would.
    let (target, key) = match hub.listener_origin().unwrap() {
        Origin::Unix(p) => (Addr::Unix(p), None),
        Origin::Tcp { port, key } => (Addr::Tcp(port), Some(key)),
    };
    let (port, tape) = relay(target);
    let addr = match key {
        Some(host) => Addr::Keyed { port, host },
        None => Addr::Tcp(port),
    };

    // Pair through the relay.
    let offer = h.ok("begin_pairing", json!({}));
    let secret = {
        let link = offer["link"].as_str().unwrap();
        let fragment = link.split_once('#').unwrap().1;
        crypto::unb64(fragment.split('.').nth(1).unwrap()).unwrap()
    };
    let phone = if path == nocturne_app_lib::remote::protocol::PATH_SCANNED {
        PhonePair::scanned(offer["link"].as_str().unwrap(), NAME)
    } else {
        PhonePair::typed(
            offer["typedCode"].as_str().unwrap(),
            testing::TEST_HOST,
            NAME,
        )
    };
    let d_scalar = phone.d.to_bytes().to_vec();
    let run = block(Pairing::start(&addr, phone, Mode::WebSocket)).unwrap();
    wait_for("asked", || {
        matches!(
            block(hub.view()).pairing.map(|p| p.phase),
            Some(nocturne_contract::PairingPhase::WaitingForYou { .. })
        )
    });
    h.ok(
        "decide_pairing",
        json!({"pairingId": offer["pairingId"], "allow": true}),
    );
    let keys: PhoneKeys = block(run.outcome()).unwrap().unwrap();

    // Use it, over each transport.
    let photo = photo();
    for (i, mode) in [Mode::WebSocket, Mode::LongPoll].into_iter().enumerate() {
        let Opened::Session(mut c, hello) = block(PhoneClient::open(&addr, &keys, mode)) else {
            panic!("a session")
        };
        assert_eq!(hello.device_name, NAME);
        let kept = block(c.call(RemoteCall::KeepText {
            text: MARKER.into(),
            client_id: format!("c-text-{i}"),
            feeling: None,
            good: None,
            captured_at: None,
        }))
        .unwrap();
        assert!(matches!(kept, RemoteAnswer::Kept { .. }));
        let id = block(c.request(RemoteCall::KeepBytes {
            mime: "image/jpeg".into(),
            size: photo.len() as f64,
            client_id: format!("c-photo-{i}"),
            caption: Some("Blue door".into()),
            feeling: None,
            good: None,
            captured_at: None,
        }))
        .unwrap();
        let parts: Vec<&[u8]> = photo.chunks(100 * 1024).collect();
        for (seq, p) in parts.iter().enumerate() {
            block(c.chunk(id, seq as u32, seq + 1 == parts.len(), p)).unwrap();
        }
        let RemoteAnswer::Kept { kept } = block(c.answer(id)).unwrap() else {
            panic!("the photo is kept")
        };
        let ask = block(c.request(RemoteCall::Ask {
            question: "When is the heron feeder refilled?".into(),
        }))
        .unwrap();
        let mut answer = String::new();
        loop {
            match block(c.answer(ask)).unwrap() {
                RemoteAnswer::AskDelta { delta } => {
                    answer.push_str(delta.text_delta.as_deref().unwrap_or_default());
                    if is_terminal(delta.phase) {
                        break;
                    }
                }
                RemoteAnswer::AskStarted { .. } => {}
                other => panic!("{other:?}"),
            }
        }
        assert!(answer.contains(SENTENCE), "{answer}");
        let got = block(c.call(RemoteCall::GetPage { date: None })).unwrap();
        assert!(matches!(got, RemoteAnswer::Page { .. }));
        let rid = block(c.request(RemoteCall::GetOriginal {
            id: kept.item.id.clone(),
        }))
        .unwrap();
        assert!(matches!(
            block(c.answer(rid)).unwrap(),
            RemoteAnswer::OriginalStart { .. }
        ));
        let mut back = Vec::new();
        while let Ok(Down::Chunk(ch)) = block(c.next_within(Duration::from_secs(10))) {
            back.extend_from_slice(&ch.bytes);
            if ch.last {
                break;
            }
        }
        assert_eq!(back, photo);
    }

    std::thread::sleep(Duration::from_millis(200));
    let tape = tape.lock().unwrap().clone();
    assert!(
        tape.len() > photo.len() * 2,
        "the relay carried the traffic"
    );
    let page_text = page
        .learned
        .iter()
        .chain(page.upcoming.iter())
        .map(|l| l.text.clone())
        .find(|t| t.contains("heron") || t.contains("Heron"));
    let mut needles: Vec<(String, Vec<u8>)> = vec![
        ("marker".into(), MARKER.as_bytes().to_vec()),
        ("marker start".into(), b"MARKER-a81f3c".to_vec()),
        ("answer".into(), SENTENCE.as_bytes().to_vec()),
        ("device name".into(), NAME.as_bytes().to_vec()),
        (
            "device name start".into(),
            b"Ana\xe2\x80\x99s Pixel".to_vec(),
        ),
        ("caption".into(), b"Blue door".to_vec()),
        ("secret".into(), secret.clone()),
        ("secret b64".into(), crypto::b64(&secret).into_bytes()),
        (
            "secret code".into(),
            crypto::code_encode(&secret.clone().try_into().unwrap()).into_bytes(),
        ),
        ("device key".into(), d_scalar),
        ("device kd".into(), keys.kd().to_vec()),
    ];
    if let Some(t) = page_text {
        needles.push(("page".into(), t.into_bytes()));
    }
    for (what, n) in &needles {
        assert!(!contains(&tape, n), "the relay saw the {what}");
    }
    let starts: std::collections::HashSet<[u8; 8]> =
        tape.windows(8).map(|w| w.try_into().unwrap()).collect();
    for (at, w) in photo.windows(32).enumerate() {
        let head: [u8; 8] = w[..8].try_into().unwrap();
        if starts.contains(&head) {
            assert!(
                !contains(&tape, w),
                "the relay saw the photo's bytes at {at}"
            );
        }
    }
}

#[test]
fn relay_sees_no_plaintext() {
    run(nocturne_app_lib::remote::protocol::PATH_SCANNED);
    run(nocturne_app_lib::remote::protocol::PATH_TYPED);
}
