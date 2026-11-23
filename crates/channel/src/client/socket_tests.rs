//! The phone's client against a real socket on loopback that accepts and never answers: opening a
//! session, and a call that needs one, give up within the open's waits and the phone is asleep.

use std::sync::Arc;
use std::time::{Duration, Instant};

use nocturne_contract::RemoteCall;

use super::{CallError, Connection, Options, Phone, Reach, loopback_router};
use crate::crypto;
use crate::phone::PhoneIdentity;

const ADDRESS: &str = "amber-otter-lamp-quiet.trycloudflare.com";

/// A listener that takes every connection and holds it, saying nothing.
async fn mute() -> u16 {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        let mut held = Vec::new();
        while let Ok((socket, _)) = listener.accept().await {
            held.push(socket);
        }
    });
    port
}

fn identity() -> PhoneIdentity {
    let c = crypto::new_secret();
    PhoneIdentity {
        device_id: crypto::random(),
        secret: crypto::new_secret(),
        cpub: crypto::public_bytes(&c.public_key()).to_vec(),
        home_origin: format!("https://{ADDRESS}"),
        address: ADDRESS.to_string(),
        device_name: "Pixel".to_string(),
    }
}

#[tokio::test]
async fn a_computer_that_accepts_and_never_answers_leaves_the_phone_asleep() {
    let port = mute().await;
    let options = Options {
        socket_timeout: Duration::from_millis(200),
        handshake: Duration::from_millis(500),
        ..Options::default()
    };
    let phone = Phone::new(identity(), loopback_router(port), options, Arc::new(|_| {}));
    let began = Instant::now();
    assert_eq!(phone.connect().await, Connection::Asleep);
    assert_eq!(
        phone.call(RemoteCall::GetStatus).await.err(),
        Some(CallError::Connection {
            state: Reach::Asleep
        })
    );
    assert!(
        began.elapsed() < Duration::from_secs(10),
        "{:?}",
        began.elapsed()
    );
}
