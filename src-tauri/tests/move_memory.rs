//! A phone that holds its own memory moves it to this computer: the phone app's own client
//! against the real listener, through a relay on loopback, as `phone_client.rs`. The phone's
//! side is its store and the same backup a phone makes; the computer is the app with Away from
//! home on. It offers, the person agrees, the backup goes up in chunks, every count matches, and
//! after the next start the phone is one of the computer's phones with all it kept.

#![cfg(unix)]

mod common;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use common::*;
use nocturne_app_lib::remote::moving::MoveView;
use nocturne_app_lib::remote::testing;
use nocturne_channel::client::{self, CallError, Connection, OnIdentity, Options, Phone, Reach};
use nocturne_channel::phone::{PhoneIdentity, PhonePairing};
use nocturne_contract::{
    CalendarBatch, CalendarChange, CalendarEntry, MemoryCounts, PhoneCalendar, PhoneError,
    RemoteAnswer, RemoteCall, RemoteState, Source,
};
use nocturne_engine::{BackupRun, Engine, FakeMind, KeepPlace, ManualClock};
use nocturne_helpers::{HelperKind, Origin};
use nocturne_intelligence::FakeHost;
use serde_json::json;
use sha2::{Digest, Sha256};

fn block<T>(f: impl std::future::Future<Output = T>) -> T {
    tauri::async_runtime::block_on(f)
}

const NAME: &str = "Ana\u{2019}s Pixel 9";
const PASSPHRASE: &str = "otter lamp quiet amber river";

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
    fn start(socket: PathBuf) -> Relay {
        let listener = block(tokio::net::TcpListener::bind(("127.0.0.1", 0))).unwrap();
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

    fn cut(&self) {
        for t in self.live.lock().unwrap().drain(..) {
            t.abort();
        }
    }
}

struct Computer {
    h: Harness,
    hub: Arc<nocturne_app_lib::remote::controller::RemoteHub>,
    relay: Relay,
    _files: tempfile::TempDir,
}

/// The computer with Away from home on, behind a relay.
fn computer(h: Harness) -> Computer {
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
    let Some(Origin::Unix(socket)) = hub.listener_origin() else {
        panic!("a socket listener")
    };
    Computer {
        h,
        hub,
        relay: Relay::start(socket),
        _files: files,
    }
}

impl Computer {
    fn router(&self) -> client::Router {
        client::loopback_router(self.relay.port)
    }

    /// Pair through the client; the person allows it once the words are on screen.
    fn pair(&self) -> PhoneIdentity {
        let offer = self.h.ok("begin_pairing", json!({}));
        let pairing = PhonePairing::scanned(offer["link"].as_str().unwrap(), NAME).unwrap();
        let router = self.router();
        let run = tauri::async_runtime::spawn(async move {
            client::pair(pairing, &router, &client::http_client(), |_| {}).await
        });
        wait_for("asked", || {
            matches!(
                block(self.hub.view()).pairing.map(|p| p.phase),
                Some(nocturne_contract::PairingPhase::WaitingForYou { .. })
            )
        });
        self.h.ok(
            "decide_pairing",
            json!({"pairingId": offer["pairingId"], "allow": true}),
        );
        block(run).unwrap().unwrap()
    }

    fn phone(&self, identity: PhoneIdentity) -> Phone {
        let on: OnIdentity = Arc::new(|_| {});
        Phone::new(identity, self.router(), Options::default(), on)
    }

    fn view(&self) -> Option<MoveView> {
        self.hub.core.moves.view()
    }

    fn device_id(&self) -> String {
        self.h
            .engine()
            .devices(false)
            .unwrap()
            .into_iter()
            .find(|d| d.name == NAME)
            .unwrap()
            .id
    }

    fn data(&self) -> &Path {
        self.h.dir.path()
    }

    /// Stop as the app does when it quits, and start again on the same folder.
    fn restart(self) -> Computer {
        block(self.hub.shutdown());
        self.relay.cut();
        computer(restart(self.h))
    }
}

/// A phone's own memory: a note it kept and an appointment from its calendar.
struct PhoneHome {
    dir: tempfile::TempDir,
    engine: Engine,
}

fn entry(phone_id: &str, title: &str) -> CalendarEntry {
    CalendarEntry {
        phone_id: phone_id.into(),
        calendar_id: "c1".into(),
        title: title.into(),
        start: "2026-11-13T10:00:00Z".into(),
        end: "2026-11-13T10:30:00Z".into(),
        time_zone: None,
        all_day: false,
        place: None,
        people: Vec::new(),
        repeats: false,
        notes: None,
        repeat_rule: None,
    }
}

fn calendar(sync: &str) -> CalendarBatch {
    CalendarBatch {
        sync_id: sync.into(),
        seq: 0,
        last: true,
        window_from: "2026-11-11".into(),
        window_to: "2026-11-27".into(),
        calendars: vec![PhoneCalendar {
            id: "c1".into(),
            name: "Home".into(),
            colour: None,
        }],
        changes: vec![CalendarChange::Upsert {
            entry: entry("e1", "Dentist"),
        }],
    }
}

fn phone_home() -> PhoneHome {
    let dir = tempfile::tempdir().unwrap();
    let clock = Arc::new(ManualClock::at("2026-11-13T07:00:00Z", "Europe/London"));
    let engine = Engine::open(&dir.path().join("store"), clock).unwrap();
    let local = engine.ensure_local_device("Phone").unwrap();
    let day = engine.capture_day(None).unwrap();
    let place = KeepPlace {
        day: Some(day),
        phone: Some((&local, "c-note-1")),
    };
    engine
        .keep_note_item(
            "The heron feeder is refilled on Thursday.",
            Source::Phone,
            false,
            &place,
        )
        .unwrap();
    let taken = engine.take_calendar(&local, calendar("s1")).unwrap();
    assert_eq!(taken.taken, 1);
    PhoneHome { dir, engine }
}

impl PhoneHome {
    fn counts(&self) -> MemoryCounts {
        let holdings = self.engine.holdings(None).unwrap().unwrap();
        MemoryCounts {
            items: self.engine.stats().unwrap().item_count,
            memories: holdings.memories,
            pages: holdings.pages,
            originals: holdings.originals,
        }
    }

    /// The backup a phone makes for the move, and its digest.
    fn backup(&self) -> (PathBuf, String) {
        let dest = self.dir.path().join("move.nocturne");
        let run = BackupRun {
            extras: &[],
            progress: &|_, _, _| {},
            cancel: &AtomicBool::new(false),
            started: &|_| {},
            work_factor: Some(10),
        };
        self.engine.backup_to(&dest, PASSPHRASE, &run).unwrap();
        let digest = Sha256::digest(std::fs::read(&dest).unwrap());
        let hex = digest.iter().map(|b| format!("{b:02x}")).collect();
        (dest, hex)
    }

    fn offer(&self, file: &Path) -> RemoteCall {
        RemoteCall::OfferMemory {
            counts: self.counts(),
            bytes: std::fs::metadata(file).unwrap().len() as f64,
            schema: nocturne_engine::store::latest_version(),
        }
    }
}

fn done(file: &Path, sha256: &str) -> RemoteCall {
    RemoteCall::MemoryDone {
        size: std::fs::metadata(file).unwrap().len() as f64,
        sha256: sha256.into(),
        passphrase: PASSPHRASE.into(),
    }
}

/// Offer the memory and answer on the computer once it asks.
fn offer_and_answer(c: &Computer, phone: &Phone, offer: RemoteCall, choice: &str) -> RemoteAnswer {
    let p = phone.clone();
    let asked = tauri::async_runtime::spawn(async move { p.call(offer).await });
    wait_for("the computer asks", || {
        matches!(c.view(), Some(MoveView::Offered { .. }))
    });
    c.h.ok("answer_move", json!({"choice": choice}));
    match block(asked).unwrap() {
        Ok(answer) => answer,
        Err(e) => RemoteAnswer::Error {
            code: match e {
                CallError::Phone { code } => code,
                CallError::Connection { .. } => PhoneError::Revoked,
            },
        },
    }
}

#[test]
fn a_phones_memory_moves_here_and_the_phone_becomes_one_of_its_phones() {
    let home = phone_home();
    let c = computer(harness_with_mind(Arc::new(FakeMind::new())));
    let phone = c.phone(c.pair());
    let device = c.device_id();
    let (file, sha) = home.backup();

    let accepted = offer_and_answer(&c, &phone, home.offer(&file), "move");
    assert_eq!(accepted, RemoteAnswer::MemoryAccepted { here: None });
    assert!(matches!(c.view(), Some(MoveView::Arriving { .. })));

    let seen = Arc::new(AtomicUsize::new(0));
    let heard = seen.clone();
    let arrived = block(phone.upload_memory(done(&file, &sha), &file, move |n| {
        heard.store(n as usize, Ordering::SeqCst)
    }))
    .unwrap();
    let RemoteAnswer::MemoryArrived { counts } = arrived else {
        panic!("{arrived:?}")
    };
    assert_eq!(counts, home.counts(), "every count matches");
    assert_eq!(
        seen.load(Ordering::SeqCst) as u64,
        std::fs::metadata(&file).unwrap().len()
    );
    assert!(matches!(c.view(), Some(MoveView::Arrived { .. })));

    // Until the next start, what a phone sends would land in the store about to be put aside.
    let waits = block(phone.call(RemoteCall::SendCalendar {
        batch: calendar("s2"),
    }));
    assert_eq!(
        waits.err(),
        Some(CallError::Phone {
            code: PhoneError::Busy
        })
    );
    // A phone that missed the answer is told again, without sending anything.
    assert_eq!(
        block(phone.call(home.offer(&file))).unwrap(),
        RemoteAnswer::MemoryArrived { counts }
    );
    let parts = std::fs::read_dir(c.hub.core.incoming.dir())
        .unwrap()
        .count();
    assert_eq!(parts, 0, "the upload is gone once checked");
    // Nothing replaces the memory waiting for the next start.
    assert_eq!(
        c.h.code("restore_put_aside", json!({"id": "2026-11-13 070000"})),
        "busy"
    );

    let c = c.restart();
    let e = c.h.engine();
    assert_eq!(e.local_device().unwrap(), None, "no own row shows here");
    let phones: Vec<String> = e
        .devices(false)
        .unwrap()
        .into_iter()
        .map(|d| d.id)
        .collect();
    assert_eq!(phones, vec![device.clone()]);
    let note = e
        .remote_keep(&device, "c-note-1")
        .unwrap()
        .expect("the note came");
    assert_eq!(
        e.get_item(&note).unwrap().from_device.as_deref(),
        Some(NAME)
    );
    assert_eq!(e.stats().unwrap().item_count, counts.items);
    assert!(c.view().is_none());
    // The window says it is here once, after the start that put it in place.
    let shown = c.h.ok("get_move", json!({}));
    assert_eq!(shown["phase"], "arrived");
    assert_eq!(shown["name"], NAME);
    c.h.ok("finish_move", json!({}));
    assert!(c.h.ok("get_move", json!({})).is_null());

    // The same phone, with the keys it paired with, reaches the memory that moved here, and a
    // calendar sent again is not taken twice.
    let phone = c.phone(phone.identity().unwrap());
    assert!(matches!(
        block(phone.connect()),
        Connection::Connected { .. }
    ));
    let RemoteAnswer::Synced { receipt } = block(phone.call(RemoteCall::SendCalendar {
        batch: calendar("s3"),
    }))
    .unwrap() else {
        panic!("a receipt")
    };
    assert_eq!((receipt.taken, receipt.unchanged), (0, 1), "{receipt:?}");
}

/// Offer, agree and send: the memory waits for the next start.
fn moved(c: &Computer, home: &PhoneHome, phone: &Phone) {
    let (file, sha) = home.backup();
    offer_and_answer(c, phone, home.offer(&file), "move");
    assert!(matches!(
        block(phone.upload_memory(done(&file, &sha), &file, |_| {})).unwrap(),
        RemoteAnswer::MemoryArrived { .. }
    ));
}

#[test]
fn removing_the_phone_before_the_next_start_lets_its_memory_go() {
    let home = phone_home();
    let c = computer(harness_with_mind(Arc::new(FakeMind::new())));
    let phone = c.phone(c.pair());
    moved(&c, &home, &phone);
    let device = c.device_id();
    c.h.ok("remove_device", json!({"id": device}));
    assert!(c.view().is_none());
    assert!(!nocturne_app_lib::restore::marker_path(c.data()).exists());
    assert!(!nocturne_app_lib::restore::stage_dir(c.data()).exists());
    let c = c.restart();
    let e = c.h.engine();
    assert_eq!(e.stats().unwrap().item_count, 0, "nothing of it came in");
    assert!(e.devices(false).unwrap().is_empty());
}

#[test]
fn removing_the_phone_while_its_memory_comes_stages_nothing() {
    let home = phone_home();
    let c = computer(harness_with_mind(Arc::new(FakeMind::new())));
    let phone = c.phone(c.pair());
    let (file, _) = home.backup();
    let big = home.dir.path().join("big.nocturne");
    let mut bytes = std::fs::read(&file).unwrap();
    bytes.resize(bytes.len() + 6 * 1024 * 1024, 7);
    std::fs::write(&big, &bytes).unwrap();
    let sha: String = Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let offer = RemoteCall::OfferMemory {
        counts: home.counts(),
        bytes: bytes.len() as f64,
        schema: nocturne_engine::store::latest_version(),
    };
    offer_and_answer(&c, &phone, offer, "move");
    let device = c.device_id();
    let before = c.relay.sent();
    let p = phone.clone();
    let (call, path) = (done(&big, &sha), big.clone());
    let going =
        tauri::async_runtime::spawn(async move { p.upload_memory(call, &path, |_| {}).await });
    wait_for("part of it went", || {
        c.relay.sent() > before + 2 * 1024 * 1024
    });
    c.h.ok("remove_device", json!({"id": device}));
    assert!(block(going).unwrap().is_err());
    assert!(c.view().is_none());
    assert!(!nocturne_app_lib::restore::marker_path(c.data()).exists());
    assert!(!nocturne_app_lib::restore::stage_dir(c.data()).exists());
    assert_eq!(c.hub.core.incoming.in_flight(), 0);
    // Removed, it cannot offer again.
    assert!(block(phone.call(home.offer(&file))).is_err());
}

#[test]
fn the_computers_phones_as_they_are_at_the_next_start_hold() {
    let home = phone_home();
    let c = computer(harness_with_mind(Arc::new(FakeMind::new())));
    let phone = c.phone(c.pair());
    moved(&c, &home, &phone);
    // Paired after the memory arrived and before the next start.
    let tablet = c.pair();
    let c = c.restart();
    let names: Vec<String> =
        c.h.engine()
            .devices(false)
            .unwrap()
            .into_iter()
            .map(|d| d.id)
            .collect();
    assert_eq!(names.len(), 2, "{names:?}");
    let tablet = c.phone(tablet);
    assert!(matches!(
        block(tablet.connect()),
        Connection::Connected { .. }
    ));
}

#[test]
fn a_computer_that_holds_a_memory_offers_the_choice() {
    let home = phone_home();
    let h = harness_with_mind(Arc::new(FakeMind::new()));
    h.learn("garden", "The gate code is 4512.", "Gate", "4512");
    let c = computer(h);
    let phone = c.phone(c.pair());
    let (file, sha) = home.backup();

    let p = phone.clone();
    let offer = home.offer(&file);
    let asked = tauri::async_runtime::spawn(async move { p.call(offer).await });
    wait_for("the computer asks", || c.view().is_some());
    let Some(MoveView::Offered { here, items, .. }) = c.view() else {
        panic!("{:?}", c.view())
    };
    assert!(here, "it says a memory is here");
    assert_eq!(items, home.counts().items);
    c.h.ok("answer_move", json!({"choice": "move"}));
    let RemoteAnswer::MemoryAccepted { here: Some(here) } = block(asked).unwrap().unwrap() else {
        panic!("the phone hears what is here")
    };
    assert!(here.items > 0);
    assert!(matches!(
        block(phone.upload_memory(done(&file, &sha), &file, |_| {})).unwrap(),
        RemoteAnswer::MemoryArrived { .. }
    ));
    let data = c.data().to_path_buf();
    let c = c.restart();
    let aside = nocturne_app_lib::restore::put_aside(&data);
    assert_eq!(aside.len(), 1, "what was here is put aside");
    assert!(aside[0].memories > 0);
    assert!(c.h.engine().local_device().unwrap().is_none());
}

#[test]
fn keeping_what_is_here_undoes_the_pairing() {
    let home = phone_home();
    let c = computer(harness_with_mind(Arc::new(FakeMind::new())));
    let phone = c.phone(c.pair());
    let (file, _) = home.backup();

    let answer = offer_and_answer(&c, &phone, home.offer(&file), "notNow");
    assert_eq!(
        answer,
        RemoteAnswer::Error {
            code: PhoneError::Revoked
        }
    );
    assert!(phone.identity().is_none(), "the phone forgot the pairing");
    assert!(c.h.engine().devices(false).unwrap().is_empty());
    assert!(c.view().is_none());
    assert!(!nocturne_app_lib::restore::marker_path(c.data()).exists());
}

#[test]
fn a_phone_that_says_not_now_takes_the_question_away() {
    let home = phone_home();
    let c = computer(harness_with_mind(Arc::new(FakeMind::new())));
    let phone = c.phone(c.pair());
    let (file, _) = home.backup();
    let p = phone.clone();
    let offer = home.offer(&file);
    let asked = tauri::async_runtime::spawn(async move { p.call(offer).await });
    wait_for("the computer asks", || c.view().is_some());
    // "Not now" on the phone undoes the pairing on both.
    let other = c.phone(phone.identity().unwrap());
    assert!(block(other.disconnect()));
    wait_for("the question goes", || c.view().is_none());
    assert!(block(asked).unwrap().is_err());
    assert!(c.h.engine().devices(false).unwrap().is_empty());
}

#[test]
fn an_interrupted_upload_changes_nothing() {
    let home = phone_home();
    let c = computer(harness_with_mind(Arc::new(FakeMind::new())));
    let phone = c.phone(c.pair());
    let (file, sha) = home.backup();
    // Enough to be cut part way.
    let big = home.dir.path().join("big.nocturne");
    let mut bytes = std::fs::read(&file).unwrap();
    bytes.resize(bytes.len() + 6 * 1024 * 1024, 7);
    std::fs::write(&big, &bytes).unwrap();
    let big_sha: String = Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();

    let offer = RemoteCall::OfferMemory {
        counts: home.counts(),
        bytes: bytes.len() as f64,
        schema: nocturne_engine::store::latest_version(),
    };
    offer_and_answer(&c, &phone, offer, "move");
    let before = c.relay.sent();
    let p = phone.clone();
    let (call, path) = (done(&big, &big_sha), big.clone());
    let going =
        tauri::async_runtime::spawn(async move { p.upload_memory(call, &path, |_| {}).await });
    wait_for("part of it went", || {
        c.relay.sent() > before + 2 * 1024 * 1024
    });
    c.relay.cut();
    let out = block(going).unwrap();
    assert!(
        matches!(
            out,
            Err(CallError::Connection {
                state: Reach::Asleep
            })
        ),
        "{out:?}"
    );
    wait_for("the computer lets go", || {
        matches!(c.view(), Some(MoveView::Arriving { done, .. }) if done == 0.0)
            && c.hub.core.incoming.in_flight() == 0
    });
    assert!(!nocturne_app_lib::restore::marker_path(c.data()).exists());
    assert!(!nocturne_app_lib::restore::stage_dir(c.data()).exists());
    // Carrying on after the pause, the phone is not asked about twice.
    let again = RemoteCall::OfferMemory {
        counts: home.counts(),
        bytes: bytes.len() as f64,
        schema: nocturne_engine::store::latest_version(),
    };
    assert_eq!(
        block(phone.call(again)).unwrap(),
        RemoteAnswer::MemoryAccepted { here: None }
    );

    // A file that is not what the phone said is refused whole, and nothing changes either.
    let wrong = block(phone.upload_memory(done(&file, &big_sha.replace('a', "b")), &file, |_| {}));
    assert_eq!(
        wrong.err(),
        Some(CallError::Phone {
            code: PhoneError::Invalid
        })
    );
    assert!(!nocturne_app_lib::restore::marker_path(c.data()).exists());
    assert!(c.view().is_none(), "the phone may offer it again");

    // Offered again, it arrives.
    offer_and_answer(&c, &phone, home.offer(&file), "move");
    assert!(matches!(
        block(phone.upload_memory(done(&file, &sha), &file, |_| {})).unwrap(),
        RemoteAnswer::MemoryArrived { .. }
    ));
}

#[test]
fn a_memory_is_not_taken_without_the_persons_word() {
    let home = phone_home();
    let c = computer(harness_with_mind(Arc::new(FakeMind::new())));
    let phone = c.phone(c.pair());
    let (file, sha) = home.backup();
    let early = block(phone.upload_memory(done(&file, &sha), &file, |_| {}));
    assert_eq!(
        early.err(),
        Some(CallError::Phone {
            code: PhoneError::NotReady
        })
    );
    let too_big = block(phone.call(RemoteCall::OfferMemory {
        counts: home.counts(),
        bytes: 5e9,
        schema: 1,
    }));
    assert_eq!(
        too_big.err(),
        Some(CallError::Phone {
            code: PhoneError::TooLarge
        })
    );
    let newer = block(phone.call(RemoteCall::OfferMemory {
        counts: home.counts(),
        bytes: 10.0,
        schema: nocturne_engine::store::latest_version() + 1,
    }));
    assert_eq!(
        newer.err(),
        Some(CallError::Phone {
            code: PhoneError::ComputerOutdated
        })
    );
    assert!(c.view().is_none());
}
