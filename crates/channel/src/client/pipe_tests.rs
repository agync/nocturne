//! The phone's client against a scripted computer over in-memory pipes: pairing, the session
//! handshake, a removed phone's REVOKED, and a session that ended under an open (410). The
//! computer is the test itself, using the crate's own computer-side steps, so nothing here
//! touches a socket.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use nocturne_contract::{
    PhoneError, PhoneHello, RemoteAnswer, RemoteCall, RemoteConnection, RemoteReply, RemoteRequest,
};
use p256::SecretKey;

use super::link::{LinkEnd, Route};
use super::pipe::{Conn, Listener};
use super::{
    CallError, Connection, Options, PairFailure, Phone, Reach, Router, http_client, pair,
    take_hello,
};
use crate::crypto::{self, DeviceId, Envelope, PairKeys, PairingId};
use crate::frame::{Channel, Side};
use crate::phone::{PhoneIdentity, PhonePairing};
use crate::protocol::*;

const ADDRESS: &str = "amber-otter-lamp-quiet.trycloudflare.com";

/// The computer: its key, the phones it knows, and the pipe the phone reaches it on.
struct Computer {
    listener: Listener,
    c: SecretKey,
    phones: Vec<(DeviceId, Vec<u8>)>,
}

impl Computer {
    fn new() -> Computer {
        Computer {
            listener: Listener::new(),
            c: crypto::new_secret(),
            phones: Vec::new(),
        }
    }

    fn cpub(&self) -> Vec<u8> {
        crypto::public_bytes(&self.c.public_key()).to_vec()
    }

    /// Every address reaches this computer's pipe.
    fn router(&self) -> Router {
        let name = self.listener.name.clone();
        Arc::new(move |address| crate::phone::check_host(address).map(|_| Route::pipe(&name)))
    }

    /// A paired phone this computer knows.
    fn pair_phone(&mut self) -> PhoneIdentity {
        let identity = PhoneIdentity {
            device_id: crypto::random(),
            secret: crypto::new_secret(),
            cpub: self.cpub(),
            home_origin: format!("https://{ADDRESS}"),
            address: ADDRESS.to_string(),
            device_name: "Pixel".to_string(),
        };
        self.phones.push((identity.device_id, identity.dpub()));
        identity
    }

    fn hello() -> PhoneHello {
        PhoneHello {
            computer_name: "Desk".into(),
            device_name: "Pixel".into(),
            connection: RemoteConnection::Simple,
            version: "0.7.2".into(),
            min_phone_build: MIN_PHONE_BUILD,
            max_phone_build: Some(PHONE_BUILD),
            features: Vec::new(),
        }
    }

    /// Reads a SESSION_OPEN the way the computer does: a proof from a phone it knows.
    async fn read_open(&self, conn: &mut Conn) -> crypto::OpenRequest<Vec<u8>> {
        let frame = conn.recv().await.expect("a session open");
        crypto::verify_open(
            &frame,
            |id| {
                self.phones
                    .iter()
                    .find(|(known, _)| known == id)
                    .map(|(_, dpub)| (dpub.clone(), dpub.clone()))
            },
            &self.c,
        )
        .expect("an open the computer accepts")
    }

    /// Accepts an open and says Hello.
    async fn accept(&self, conn: &mut Conn, hello: PhoneHello) -> Channel {
        let mut channel = self.accept_without_hello(conn).await;
        reply(conn, &mut channel, 0, RemoteAnswer::Hello { hello });
        channel
    }

    /// Accepts an open and says nothing more.
    async fn accept_without_hello(&self, conn: &mut Conn) -> Channel {
        let open = self.read_open(conn).await;
        let f = crypto::new_secret();
        let sid = crypto::random::<SID_LEN>();
        let (fpub, cpub) = (
            crypto::public_bytes(&f.public_key()),
            crypto::public_bytes(&self.c.public_key()),
        );
        let dkey = crypto::check_public_key(&open.device).unwrap();
        let keys = crypto::session_keys_computer(
            &f,
            &self.c,
            &open.epub,
            &dkey,
            &crypto::SessionInput {
                device_id: &open.device_id,
                epub: &crypto::public_bytes(&open.epub),
                on: &open.on,
                sid: &sid,
                fpub: &fpub,
                cpub: &cpub,
                dpub: &open.device,
            },
        );
        conn.send(crypto::session_accept(&sid, &fpub));
        Channel::new(Side::Computer, sid, keys)
    }

    /// Answers an open with a sealed REVOKED, as for a phone that was removed.
    async fn revoke(&self, conn: &mut Conn) {
        let open = self.read_open(conn).await;
        conn.send(crypto::revoked(&open.kd, &open.device_id, &open.on));
    }
}

fn reply(conn: &Conn, channel: &mut Channel, id: u32, answer: RemoteAnswer) {
    let body = serde_json::to_vec(&RemoteReply { id, answer }).unwrap();
    conn.send(channel.seal(&crypto::envelope_json(&body)).unwrap());
}

/// The next call on a session.
async fn request(conn: &mut Conn, channel: &mut Channel) -> RemoteRequest {
    let frame = conn.recv().await.expect("a call");
    match channel.open(&frame).expect("a sealed call") {
        Envelope::Json(body) => serde_json::from_slice(&body).expect("a call"),
        Envelope::Chunk(_) => panic!("a chunk where a call was expected"),
    }
}

type IdentityLog = Arc<Mutex<Vec<Option<String>>>>;

fn phone(computer: &Computer, identity: PhoneIdentity, options: Options) -> (Phone, IdentityLog) {
    let log: IdentityLog = Arc::default();
    let seen = log.clone();
    let phone = Phone::new(
        identity,
        computer.router(),
        options,
        Arc::new(move |i| seen.lock().unwrap().push(i.map(PhoneIdentity::id))),
    );
    (phone, log)
}

fn quick() -> Options {
    Options {
        handshake: Duration::from_millis(200),
        ..Options::default()
    }
}

/// Lets what the phone has already queued run, then says whether it opened another connection.
async fn another_connection(computer: &mut Computer) -> bool {
    tokio::time::sleep(Duration::from_millis(50)).await;
    computer.listener.pending()
}

// Session handshake -----------------------------------------------------------------------------

#[tokio::test]
async fn a_session_opens_and_a_call_is_answered() {
    let mut computer = Computer::new();
    let identity = computer.pair_phone();
    let (phone, log) = phone(&computer, identity, Options::default());
    let task = tokio::spawn({
        let phone = phone.clone();
        async move { phone.connect().await }
    });
    let mut conn = computer.listener.accept().await;
    let mut channel = computer.accept(&mut conn, Computer::hello()).await;
    assert!(matches!(
        task.await.unwrap(),
        Connection::Connected {
            agreed: PHONE_BUILD,
            ..
        }
    ));
    assert_eq!(phone.agreed_build(), PHONE_BUILD);

    let call = tokio::spawn({
        let phone = phone.clone();
        async move { phone.call(RemoteCall::GetStatus).await }
    });
    let asked = request(&mut conn, &mut channel).await;
    assert!(matches!(asked.call, RemoteCall::GetStatus));
    reply(&conn, &mut channel, asked.id, RemoteAnswer::Done);
    assert_eq!(call.await.unwrap().unwrap(), RemoteAnswer::Done);
    assert!(
        log.lock().unwrap().is_empty(),
        "nothing about the phone changed"
    );
    assert!(!another_connection(&mut computer).await);
}

#[tokio::test]
async fn a_first_reply_that_is_not_hello_ends_the_open_and_keeps_the_keys() {
    let mut computer = Computer::new();
    let identity = computer.pair_phone();
    let (phone, log) = phone(&computer, identity, Options::default());
    let task = tokio::spawn({
        let phone = phone.clone();
        async move { phone.connect().await }
    });
    let mut conn = computer.listener.accept().await;
    let mut channel = computer.accept_without_hello(&mut conn).await;
    reply(&conn, &mut channel, 0, RemoteAnswer::Done);
    assert_eq!(task.await.unwrap(), Connection::CantConnect);
    assert!(
        phone.identity().is_some(),
        "an unsealed failure leaves the keys alone"
    );
    assert!(log.lock().unwrap().is_empty());
}

#[tokio::test]
async fn an_answer_that_is_not_an_accept_is_a_refusal_and_keeps_the_keys() {
    let mut computer = Computer::new();
    let identity = computer.pair_phone();
    let (phone, _) = phone(&computer, identity, Options::default());
    let task = tokio::spawn({
        let phone = phone.clone();
        async move { phone.connect().await }
    });
    let mut conn = computer.listener.accept().await;
    let _ = conn.recv().await;
    conn.send(crypto::hello_request(&[1; PAIRING_ID_LEN]));
    assert_eq!(task.await.unwrap(), Connection::CantConnect);
    assert!(phone.identity().is_some());
}

#[tokio::test]
async fn an_off_computer_a_refusal_and_a_lost_socket_are_told_apart() {
    let mut computer = Computer::new();
    let identity = computer.pair_phone();

    let (phone_a, _) = phone(&computer, identity.clone(), Options::default());
    let task = tokio::spawn(async move { phone_a.connect().await });
    let mut conn = computer.listener.accept().await;
    let _ = conn.recv().await;
    conn.end(LinkEnd::Http { status: HTTP_OFF });
    assert_eq!(task.await.unwrap(), Connection::TurnedOff);

    let (phone_b, _) = phone(&computer, identity.clone(), Options::default());
    let task = tokio::spawn(async move { phone_b.connect().await });
    let mut conn = computer.listener.accept().await;
    let _ = conn.recv().await;
    conn.end(LinkEnd::Closed { code: 4403 });
    assert_eq!(task.await.unwrap(), Connection::CantConnect);
    assert!(
        !another_connection(&mut computer).await,
        "a refusal is not tried again"
    );

    // A socket that closes before the session exists is tried once more.
    let (phone_c, _) = phone(&computer, identity, Options::default());
    let task = tokio::spawn(async move { phone_c.connect().await });
    for _ in 0..2 {
        let mut conn = computer.listener.accept().await;
        let _ = conn.recv().await;
        conn.end(LinkEnd::Closed { code: 1006 });
    }
    assert_eq!(task.await.unwrap(), Connection::Asleep);
    assert!(!another_connection(&mut computer).await);
}

#[tokio::test]
async fn a_computer_that_never_answers_is_asleep() {
    let mut computer = Computer::new();
    let identity = computer.pair_phone();
    let (phone, _) = phone(&computer, identity, quick());
    let task = tokio::spawn({
        let phone = phone.clone();
        async move { phone.connect().await }
    });
    let mut conn = computer.listener.accept().await;
    let _ = conn.recv().await;
    assert_eq!(task.await.unwrap(), Connection::Asleep);
    assert!(phone.identity().is_some());
}

// REVOKED ---------------------------------------------------------------------------------------

#[tokio::test]
async fn a_sealed_revoked_to_an_open_removes_the_phone() {
    let mut computer = Computer::new();
    let identity = computer.pair_phone();
    let id = identity.id();
    let (phone, log) = phone(&computer, identity, Options::default());
    let task = tokio::spawn({
        let phone = phone.clone();
        async move { phone.connect().await }
    });
    let mut conn = computer.listener.accept().await;
    computer.revoke(&mut conn).await;
    assert_eq!(task.await.unwrap(), Connection::Removed);
    assert!(phone.identity().is_none());
    assert_eq!(
        *log.lock().unwrap(),
        vec![None::<String>],
        "told once, for {id}"
    );
    assert!(matches!(
        phone.call(RemoteCall::GetStatus).await,
        Err(CallError::Connection {
            state: Reach::CantConnect
        })
    ));
}

#[tokio::test]
async fn a_revoked_that_is_not_this_phones_own_answer_is_ignored() {
    let mut computer = Computer::new();
    let identity = computer.pair_phone();
    let (phone, log) = phone(&computer, identity, Options::default());
    let task = tokio::spawn({
        let phone = phone.clone();
        async move { phone.connect().await }
    });
    let mut conn = computer.listener.accept().await;
    let open = computer.read_open(&mut conn).await;
    // Sealed for another open, sealed under another key, and cut short: none of them removes
    // the phone.
    conn.send(crypto::revoked(
        &open.kd,
        &open.device_id,
        &[0; OPEN_NONCE_LEN],
    ));
    conn.send(crypto::revoked(
        &crypto::random(),
        &open.device_id,
        &open.on,
    ));
    let mut short = crypto::revoked(&open.kd, &open.device_id, &open.on);
    short.pop();
    conn.send(short);

    // The same open is then accepted by hand.
    let f = crypto::new_secret();
    let sid = crypto::random::<SID_LEN>();
    let (fpub, cpub) = (
        crypto::public_bytes(&f.public_key()),
        crypto::public_bytes(&computer.c.public_key()),
    );
    let keys = crypto::session_keys_computer(
        &f,
        &computer.c,
        &open.epub,
        &crypto::check_public_key(&open.device).unwrap(),
        &crypto::SessionInput {
            device_id: &open.device_id,
            epub: &crypto::public_bytes(&open.epub),
            on: &open.on,
            sid: &sid,
            fpub: &fpub,
            cpub: &cpub,
            dpub: &open.device,
        },
    );
    conn.send(crypto::session_accept(&sid, &fpub));
    let mut channel = Channel::new(Side::Computer, sid, keys);
    reply(
        &conn,
        &mut channel,
        0,
        RemoteAnswer::Hello {
            hello: Computer::hello(),
        },
    );
    assert!(matches!(task.await.unwrap(), Connection::Connected { .. }));
    assert!(phone.identity().is_some());
    assert!(log.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_revoked_answer_on_an_open_session_removes_the_phone_at_the_call() {
    let mut computer = Computer::new();
    let identity = computer.pair_phone();
    let (phone, log) = phone(&computer, identity, Options::default());
    let task = tokio::spawn({
        let phone = phone.clone();
        async move { phone.connect().await }
    });
    let mut conn = computer.listener.accept().await;
    let mut channel = computer.accept(&mut conn, Computer::hello()).await;
    assert!(matches!(task.await.unwrap(), Connection::Connected { .. }));

    let call = tokio::spawn({
        let phone = phone.clone();
        async move { phone.call(RemoteCall::GetStatus).await }
    });
    let asked = request(&mut conn, &mut channel).await;
    reply(&conn, &mut channel, asked.id, RemoteAnswer::Revoked);
    assert_eq!(
        call.await.unwrap(),
        Err(CallError::Connection {
            state: Reach::Removed
        })
    );
    assert!(phone.identity().is_none());
    assert_eq!(phone.state(), Connection::Removed);
    assert_eq!(log.lock().unwrap().len(), 1);
    assert!(
        !another_connection(&mut computer).await,
        "a removed phone does not open again"
    );
}

#[tokio::test]
async fn an_error_answer_of_revoked_removes_the_phone_too() {
    let mut computer = Computer::new();
    let identity = computer.pair_phone();
    let (phone, _) = phone(&computer, identity, Options::default());
    let task = tokio::spawn({
        let phone = phone.clone();
        async move { phone.connect().await }
    });
    let mut conn = computer.listener.accept().await;
    let mut channel = computer.accept(&mut conn, Computer::hello()).await;
    let _ = task.await.unwrap();
    let call = tokio::spawn({
        let phone = phone.clone();
        async move { phone.call(RemoteCall::GetStatus).await }
    });
    let asked = request(&mut conn, &mut channel).await;
    reply(
        &conn,
        &mut channel,
        asked.id,
        RemoteAnswer::Error {
            code: PhoneError::Revoked,
        },
    );
    assert_eq!(
        call.await.unwrap(),
        Err(CallError::Connection {
            state: Reach::Removed
        })
    );
    assert_eq!(phone.state(), Connection::Removed);
}

// 410 -------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_session_that_ended_under_the_open_is_opened_again_once() {
    let mut computer = Computer::new();
    let identity = computer.pair_phone();
    let (phone, log) = phone(&computer, identity, Options::default());
    let task = tokio::spawn({
        let phone = phone.clone();
        async move { phone.connect().await }
    });
    let mut first = computer.listener.accept().await;
    let _ = first.recv().await;
    first.end(LinkEnd::Http { status: HTTP_ENDED });
    let mut second = computer.listener.accept().await;
    let _channel = computer.accept(&mut second, Computer::hello()).await;
    assert!(matches!(task.await.unwrap(), Connection::Connected { .. }));
    assert!(log.lock().unwrap().is_empty());
    assert!(!another_connection(&mut computer).await);
}

#[tokio::test]
async fn a_second_410_leaves_the_phone_asleep_and_keeps_its_keys() {
    let mut computer = Computer::new();
    let identity = computer.pair_phone();
    let (phone, log) = phone(&computer, identity, Options::default());
    let task = tokio::spawn({
        let phone = phone.clone();
        async move { phone.connect().await }
    });
    for _ in 0..2 {
        let mut conn = computer.listener.accept().await;
        let _ = conn.recv().await;
        conn.end(LinkEnd::Http { status: HTTP_ENDED });
    }
    assert_eq!(task.await.unwrap(), Connection::Asleep);
    assert!(phone.identity().is_some());
    assert!(log.lock().unwrap().is_empty());
    assert!(
        !another_connection(&mut computer).await,
        "tried twice, no more"
    );
}

#[tokio::test]
async fn a_call_on_an_expired_session_goes_again_on_a_new_one() {
    let mut computer = Computer::new();
    let identity = computer.pair_phone();
    let (phone, _) = phone(&computer, identity, Options::default());
    let call = tokio::spawn({
        let phone = phone.clone();
        async move { phone.call(RemoteCall::GetStatus).await }
    });
    let mut first = computer.listener.accept().await;
    let mut channel = computer.accept(&mut first, Computer::hello()).await;
    let asked = request(&mut first, &mut channel).await;
    reply(
        &first,
        &mut channel,
        asked.id,
        RemoteAnswer::Error {
            code: PhoneError::SessionExpired,
        },
    );
    let mut second = computer.listener.accept().await;
    let mut channel = computer.accept(&mut second, Computer::hello()).await;
    let again = request(&mut second, &mut channel).await;
    assert!(matches!(again.call, RemoteCall::GetStatus));
    reply(&second, &mut channel, again.id, RemoteAnswer::Done);
    assert_eq!(call.await.unwrap().unwrap(), RemoteAnswer::Done);
}

// Pairing ---------------------------------------------------------------------------------------

/// The computer's side of one pairing offer.
struct Offer {
    s: [u8; S_LEN],
    pid: PairingId,
    keys: PairKeys,
    nc: [u8; NONCE_PAIR_LEN],
    name: String,
    committed: Vec<u8>,
}

impl Offer {
    fn new(cpub: &[u8]) -> Offer {
        let s = crypto::random::<S_LEN>();
        let pid = crypto::pairing_id(&s);
        Offer {
            keys: crypto::pair_keys(&s, cpub, &pid),
            s,
            pid,
            nc: crypto::random(),
            name: String::new(),
            committed: Vec::new(),
        }
    }

    fn open_up(&self, frame: &[u8], kind: u8) -> Vec<u8> {
        match crypto::pair_open(frame, kind, &self.pid, &self.keys.up).expect("a pairing frame") {
            Envelope::Json(body) => body,
            Envelope::Chunk(_) => panic!("a chunk in pairing"),
        }
    }

    fn seal_down(&self, kind: u8, body: &[u8]) -> Vec<u8> {
        crypto::pair_seal(
            kind,
            &self.pid,
            &self.keys.down,
            &crypto::envelope_json(body),
        )
    }

    async fn take_commit(&mut self, conn: &mut Conn) {
        let frame = conn.recv().await.expect("a commit");
        let commit: crypto::PairCommit =
            serde_json::from_slice(&self.open_up(&frame, KIND_PAIR_COMMIT)).unwrap();
        self.name = commit.device_name;
        self.committed = crypto::unb64(&commit.commit).unwrap();
    }

    fn send_nonce(&self, conn: &Conn) {
        let body = serde_json::to_vec(&crypto::PairNonce {
            nc: crypto::b64(&self.nc),
        })
        .unwrap();
        conn.send(self.seal_down(KIND_PAIR_NONCE, &body));
    }

    /// Checks the reveal against the commit and answers the words the person would compare, and
    /// the key the phone revealed.
    async fn take_reveal(
        &self,
        conn: &mut Conn,
        computer: &Computer,
        host: &str,
        path: u8,
    ) -> (Vec<u8>, Vec<String>) {
        let frame = conn.recv().await.expect("a reveal");
        let reveal: crypto::PairReveal =
            serde_json::from_slice(&self.open_up(&frame, KIND_PAIR_REVEAL)).unwrap();
        let dpub = crypto::unb64(&reveal.dpub).unwrap();
        let np = crypto::unb64(&reveal.np).unwrap();
        let cpub = computer.cpub();
        let dkey = crypto::check_reveal(&cpub, &dpub, &np, self.name.as_bytes(), &self.committed)
            .expect("a reveal that matches its commit");
        let shared = crypto::ecdh(&computer.c, &dkey);
        let words = crypto::words(
            shared.as_ref(),
            &crypto::WordsInput {
                path,
                pid: &self.pid,
                host,
                cpub: &cpub,
                dpub: &dpub,
                name: self.name.as_bytes(),
                np: &np,
                nc: &self.nc,
            },
        );
        (dpub, words.iter().map(|w| w.to_string()).collect())
    }

    fn send_outcome(&self, conn: &Conn, allowed: bool, device_id: Option<DeviceId>) {
        let body = serde_json::to_vec(&crypto::PairOutcome {
            allowed,
            device_id: device_id.map(|d| crypto::b64(&d)),
        })
        .unwrap();
        conn.send(self.seal_down(KIND_PAIR_OUTCOME, &body));
    }
}

type Words = Arc<Mutex<Vec<String>>>;

fn run_pair(
    pairing: PhonePairing,
    computer: &Computer,
    words: &Words,
) -> tokio::task::JoinHandle<Result<PhoneIdentity, PairFailure>> {
    let router = computer.router();
    let words = words.clone();
    tokio::spawn(async move {
        pair(pairing, &router, &http_client(), move |w| {
            *words.lock().unwrap() = w;
        })
        .await
    })
}

#[tokio::test]
async fn a_scanned_pairing_pairs_when_the_person_allows_it() {
    let mut computer = Computer::new();
    let mut offer = Offer::new(&computer.cpub());
    let link = crypto::pairing_link(ADDRESS, &offer.s, &computer.cpub());
    let pairing = PhonePairing::scanned(&link, "Pixel").unwrap();
    let words = Words::default();
    let task = run_pair(pairing, &computer, &words);

    let mut conn = computer.listener.accept().await;
    offer.take_commit(&mut conn).await;
    assert_eq!(offer.name, "Pixel");
    offer.send_nonce(&conn);
    let (dpub, ours) = offer
        .take_reveal(&mut conn, &computer, ADDRESS, PATH_SCANNED)
        .await;
    let device_id = crypto::random::<DEVICE_ID_LEN>();
    offer.send_outcome(&conn, true, Some(device_id));

    let identity = task.await.unwrap().expect("paired");
    assert_eq!(identity.device_id, device_id);
    assert_eq!(identity.cpub, computer.cpub());
    assert_eq!(identity.dpub(), dpub);
    assert_eq!(identity.address, ADDRESS);
    assert_eq!(
        *words.lock().unwrap(),
        ours,
        "both sides show the same three words"
    );
    assert_eq!(ours.len(), WORD_COUNT);
}

#[tokio::test]
async fn a_typed_pairing_asks_for_the_key_first_then_pairs() {
    let mut computer = Computer::new();
    let mut offer = Offer::new(&computer.cpub());
    let mut pairing =
        PhonePairing::typed(&crypto::code_encode(&offer.s), ADDRESS, "Pixel").unwrap();
    assert!(!pairing.knows_computer());

    let router = computer.router();
    let hello = tokio::spawn(async move {
        let out = take_hello(&mut pairing, &router, &http_client()).await;
        (pairing, out)
    });
    let mut conn = computer.listener.accept().await;
    let asked = conn.recv().await.expect("a hello");
    assert_eq!(crypto::hello_request_open(&asked).unwrap(), offer.pid);
    conn.send(crypto::hello_answer(&offer.pid, &computer.cpub()));
    let (pairing, out) = hello.await.unwrap();
    assert_eq!(out, Ok(()));
    assert!(pairing.knows_computer());

    // Asking again, now that the key is known, opens nothing.
    let mut pairing = pairing;
    let router = computer.router();
    assert_eq!(
        take_hello(&mut pairing, &router, &http_client()).await,
        Ok(())
    );
    assert!(!another_connection(&mut computer).await);

    let words = Words::default();
    let task = run_pair(pairing, &computer, &words);
    let mut conn = computer.listener.accept().await;
    offer.take_commit(&mut conn).await;
    offer.send_nonce(&conn);
    let (_, ours) = offer
        .take_reveal(&mut conn, &computer, ADDRESS, PATH_TYPED)
        .await;
    offer.send_outcome(&conn, true, Some(crypto::random()));
    let identity = task.await.unwrap().expect("paired");
    assert_eq!(identity.cpub, computer.cpub());
    assert_eq!(*words.lock().unwrap(), ours);
}

#[tokio::test]
async fn a_typed_pairing_with_no_open_offer_expires() {
    let mut computer = Computer::new();
    let mut pairing =
        PhonePairing::typed(&crypto::code_encode(&crypto::random()), ADDRESS, "Pixel").unwrap();
    let router = computer.router();
    let task = tokio::spawn(async move { take_hello(&mut pairing, &router, &http_client()).await });
    let conn = computer.listener.accept().await;
    conn.end(LinkEnd::Closed { code: 4404 });
    assert_eq!(task.await.unwrap(), Err(PairFailure::Expired));
}

#[tokio::test]
async fn a_pairing_the_person_refuses_is_refused() {
    let mut computer = Computer::new();
    let mut offer = Offer::new(&computer.cpub());
    let link = crypto::pairing_link(ADDRESS, &offer.s, &computer.cpub());
    let task = run_pair(
        PhonePairing::scanned(&link, "Pixel").unwrap(),
        &computer,
        &Words::default(),
    );
    let mut conn = computer.listener.accept().await;
    offer.take_commit(&mut conn).await;
    offer.send_nonce(&conn);
    let _ = offer
        .take_reveal(&mut conn, &computer, ADDRESS, PATH_SCANNED)
        .await;
    offer.send_outcome(&conn, false, None);
    assert_eq!(task.await.unwrap().err(), Some(PairFailure::Refused));
}

#[tokio::test]
async fn frames_that_do_not_open_or_come_out_of_turn_end_nothing() {
    let mut computer = Computer::new();
    let mut offer = Offer::new(&computer.cpub());
    let link = crypto::pairing_link(ADDRESS, &offer.s, &computer.cpub());
    let words = Words::default();
    let task = run_pair(
        PhonePairing::scanned(&link, "Pixel").unwrap(),
        &computer,
        &words,
    );
    let mut conn = computer.listener.accept().await;
    offer.take_commit(&mut conn).await;

    // An outcome before the nonce, a nonce under the wrong key and plain junk come first.
    offer.send_outcome(&conn, true, Some(crypto::random()));
    conn.send(crypto::pair_seal(
        KIND_PAIR_NONCE,
        &offer.pid,
        &crypto::random(),
        &crypto::envelope_json(b"{}"),
    ));
    conn.send(vec![0; 3]);
    offer.send_nonce(&conn);
    // A second nonce after the first is out of turn too.
    offer.send_nonce(&conn);
    let (_, ours) = offer
        .take_reveal(&mut conn, &computer, ADDRESS, PATH_SCANNED)
        .await;
    offer.send_outcome(&conn, true, Some(crypto::random()));
    assert!(task.await.unwrap().is_ok());
    assert_eq!(*words.lock().unwrap(), ours);
}

#[tokio::test]
async fn a_pairing_whose_link_ends_is_expired_before_the_nonce_and_spent_after() {
    let mut computer = Computer::new();
    for after_nonce in [false, true] {
        let mut offer = Offer::new(&computer.cpub());
        let link = crypto::pairing_link(ADDRESS, &offer.s, &computer.cpub());
        let task = run_pair(
            PhonePairing::scanned(&link, "Pixel").unwrap(),
            &computer,
            &Words::default(),
        );
        let mut conn = computer.listener.accept().await;
        offer.take_commit(&mut conn).await;
        if after_nonce {
            offer.send_nonce(&conn);
            let _ = offer
                .take_reveal(&mut conn, &computer, ADDRESS, PATH_SCANNED)
                .await;
        }
        conn.end(LinkEnd::Closed { code: 1006 });
        let want = if after_nonce {
            PairFailure::Spent
        } else {
            PairFailure::Expired
        };
        assert_eq!(
            task.await.unwrap().err(),
            Some(want),
            "after nonce: {after_nonce}"
        );
        assert!(
            !another_connection(&mut computer).await,
            "a pairing commits once and never retries"
        );
    }
}

#[tokio::test]
async fn a_pairing_needs_a_name_and_the_computers_key_before_anything_is_sent() {
    let mut computer = Computer::new();
    let offer = Offer::new(&computer.cpub());
    let link = crypto::pairing_link(ADDRESS, &offer.s, &computer.cpub());
    let mut nameless = PhonePairing::scanned(&link, "Pixel").unwrap();
    nameless.set_name("").unwrap();
    let task = run_pair(nameless, &computer, &Words::default());
    assert_eq!(task.await.unwrap().err(), Some(PairFailure::BadName));

    let typed = PhonePairing::typed(&crypto::code_encode(&offer.s), ADDRESS, "Pixel").unwrap();
    let task = run_pair(typed, &computer, &Words::default());
    assert_eq!(task.await.unwrap().err(), Some(PairFailure::Expired));
    assert!(!another_connection(&mut computer).await);
}
