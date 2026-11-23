//! A phone that holds its own memory moves it to this computer: the phone app's own client
//! against the real listener, through a relay on loopback, as `phone_client.rs`. The phone's
//! side is its store and the same backup a phone makes; the computer is the app with Away from
//! home on. It offers; an empty computer agrees at once, one that holds a memory asks the person
//! there. The backup goes up in chunks, every count matches, and its things are added with no
//! restart: the phone is one of the computer's phones, and all it kept is here.

#![cfg(unix)]

mod common;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use common::*;
use nocturne_app_lib::remote::moving::{EndedWhy, MoveView};
use nocturne_app_lib::remote::testing;
use nocturne_channel::client::{self, CallError, Connection, OnIdentity, Options, Phone, Reach};
use nocturne_channel::phone::{PhoneIdentity, PhonePairing};
use nocturne_contract::{
    BroughtCounts, CalendarBatch, CalendarChange, CalendarEntry, CheckInInput, Feeling,
    FeelingPart, FeelingPoint, Kept, MemoryCounts, PhoneCalendar, PhoneError, RemoteAnswer,
    RemoteCall, RemoteState, Source,
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
const GATE: &str = "The gate code is 4512.";

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
            account: None,
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
            since: None,
            things: Some(self.counts().items.saturating_sub(1)),
            bring: false,
        }
    }

    /// The offer of what a move put aside, brought in once the phone works with the computer.
    fn bring(&self, file: &Path) -> RemoteCall {
        RemoteCall::OfferMemory {
            counts: self.counts(),
            bytes: std::fs::metadata(file).unwrap().len() as f64,
            schema: nocturne_engine::store::latest_version(),
            since: Some("2026-11-13".into()),
            things: None,
            bring: true,
        }
    }

    /// Also kept on the phone: the gate code the computer holds too, marked private here, and a
    /// check-in.
    fn with_more(self) -> PhoneHome {
        let gate = match self.engine.keep_text(GATE, Source::Phone).unwrap() {
            Kept::New { item } | Kept::Duplicate { item } => item.id,
        };
        self.engine.set_item_private(&gate, true).unwrap();
        self.engine
            .keep_checkin(
                CheckInInput {
                    feeling: Feeling {
                        part: FeelingPart::Settled,
                        point: FeelingPoint {
                            pleasant: 0.5,
                            energy: 0.5,
                        },
                        words: vec!["calm".into()],
                    },
                    line: Some("A slow morning".into()),
                    urge: None,
                    replaces: None,
                },
                Source::Phone,
            )
            .unwrap();
        self
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

/// An empty computer agrees at once: nothing is asked here.
fn offered(phone: &Phone, offer: RemoteCall) -> RemoteAnswer {
    block(phone.call(offer)).unwrap()
}

const BROUGHT: RemoteAnswer = RemoteAnswer::MemoryAccepted {
    here: None,
    bring: true,
};

#[test]
fn an_empty_computer_brings_the_phones_things_in_with_no_restart() {
    let home = phone_home().with_more();
    let c = computer(harness_with_mind(Arc::new(FakeMind::new())));
    let phone = c.phone(c.pair());
    let device = c.device_id();
    let (file, sha) = home.backup();
    let shown = c.h.events("move-changed");

    assert_eq!(offered(&phone, home.offer(&file)), BROUGHT);
    assert!(
        matches!(c.view(), Some(MoveView::Preparing { bring: true, .. })),
        "nothing is asked: it waits for the phone's things"
    );
    assert!(
        shown
            .lock()
            .unwrap()
            .iter()
            .all(|v| v["phase"] != "offered"),
        "the window never asks"
    );

    let seen = Arc::new(AtomicUsize::new(0));
    let heard = seen.clone();
    let brought = block(phone.upload_memory(done(&file, &sha), &file, move |n| {
        heard.store(n as usize, Ordering::SeqCst)
    }))
    .unwrap();
    let RemoteAnswer::MemoryBrought { counts, added } = brought else {
        panic!("{brought:?}")
    };
    assert_eq!(counts, home.counts(), "every count matches");
    assert_eq!(
        seen.load(Ordering::SeqCst) as u64,
        std::fs::metadata(&file).unwrap().len()
    );
    assert_eq!(
        added,
        BroughtCounts {
            things: 2,
            entries: 0,
            checkins: 1,
        }
    );
    assert!(matches!(c.view(), Some(MoveView::Brought { .. })));
    assert!(
        !nocturne_app_lib::restore::marker_path(c.data()).exists(),
        "no restart"
    );
    assert!(!nocturne_app_lib::restore::stage_dir(c.data()).exists());
    assert_eq!(
        std::fs::read_dir(c.hub.core.incoming.dir())
            .unwrap()
            .count(),
        0,
        "the upload is gone once added"
    );

    // Everything is here at once, with no restart: the note with where it was kept, the
    // private mark, the check-in.
    let e = c.h.engine();
    assert_eq!(e.local_device().unwrap(), None, "no own row shows here");
    assert_eq!(e.devices(false).unwrap().len(), 1, "one of its phones");
    let note = e
        .remote_keep(&device, "c-note-1")
        .unwrap()
        .expect("the note came");
    assert_eq!(
        e.get_item(&note).unwrap().from_device.as_deref(),
        Some(NAME)
    );
    let gate = e.find_items(GATE, 10).unwrap().remove(0).receipt.item_id;
    assert!(e.get_item(&gate).unwrap().private, "still private");
    let before_calendar = e.stats().unwrap().item_count;

    // The phone missed the answer: it hears it again, and nothing is doubled.
    assert_eq!(
        block(phone.memory_status()).unwrap(),
        RemoteAnswer::MemoryBrought { counts, added }
    );
    assert_eq!(
        offered(&phone, home.offer(&file)),
        RemoteAnswer::MemoryBrought { counts, added }
    );
    assert_eq!(
        items_like(&c, "The heron feeder is refilled on Thursday."),
        1
    );
    assert_eq!(items_like(&c, GATE), 1);
    assert_eq!(e.stats().unwrap().item_count, before_calendar);

    // The calendar was not copied: the phone sends it again, and only once.
    assert_eq!(send_calendar(&phone, "s2"), (1, 0));
    assert_eq!(send_calendar(&phone, "s3"), (0, 1));

    let shown = c.h.ok("get_move", json!({}));
    assert_eq!(shown["phase"], "brought");
    assert_eq!(shown["added"]["things"], 2);
    c.h.ok("finish_move", json!({}));
    assert!(c.h.ok("get_move", json!({})).is_null());
}

#[test]
fn the_computer_counts_the_things_as_the_phone_does() {
    let home = phone_home();
    let c = computer_with_a_memory();
    let phone = c.phone(c.pair());
    let (file, _) = home.backup();
    let offer = home.offer(&file);
    let RemoteCall::OfferMemory {
        things: Some(things),
        counts,
        ..
    } = offer.clone()
    else {
        panic!("the phone says how many things it kept");
    };
    assert_ne!(things, counts.items, "the check-ins are counted apart");
    let p = phone.clone();
    let asked = tauri::async_runtime::spawn(async move { p.call(offer).await });
    wait_for("the computer asks", || {
        matches!(c.view(), Some(MoveView::Offered { .. }))
    });
    let Some(MoveView::Offered { items, .. }) = c.view() else {
        panic!("the question shows");
    };
    assert_eq!(items, things);
    c.h.ok("answer_move", json!({"choice": "notNow"}));
    let _ = block(asked);
}

#[test]
fn what_was_forgotten_on_the_phone_stays_forgotten_on_the_computer() {
    let home = phone_home();
    let said = || {
        nocturne_engine::Candidate::new(
            "heron feeder",
            nocturne_engine::Predicate::Scheduled,
            "Thursday",
            "The heron feeder is refilled on Thursday.",
            0.8,
        )
    };
    let read = |e: &Engine, item: &str| {
        let lease = e
            .lease_item(item, std::time::Duration::from_secs(60))
            .unwrap()
            .lease;
        e.apply_candidates(item, &lease, vec![said()]).unwrap()
    };
    let note = home.engine.find_items("heron feeder", 1).unwrap()[0]
        .receipt
        .item_id
        .clone();
    let memory = read(&home.engine, &note).created.remove(0);
    let forget = home.engine.preview_forget(&[memory], false).unwrap();
    home.engine.forget(&forget.token).unwrap();

    let c = computer(harness_with_mind(Arc::new(FakeMind::new())));
    let phone = c.phone(c.pair());
    let (file, sha) = home.backup();
    assert_eq!(offered(&phone, home.offer(&file)), BROUGHT);
    let brought = block(phone.upload_memory(done(&file, &sha), &file, |_| {})).unwrap();
    assert!(
        matches!(brought, RemoteAnswer::MemoryBrought { .. }),
        "{brought:?}"
    );
    let e = c.h.engine();
    let here = e
        .remote_keep(&c.device_id(), "c-note-1")
        .unwrap()
        .expect("the thing itself came");
    assert!(
        e.find_items("heron feeder", 10).unwrap().is_empty(),
        "nor is it found by its words, as after a forget here"
    );
    assert!(
        read(&e, &here).created.is_empty(),
        "the computer never reads what was forgotten from it again"
    );
}

#[test]
fn removing_the_phone_while_its_things_come_to_an_empty_computer_adds_nothing() {
    let home = phone_home();
    let c = computer(harness_with_mind(Arc::new(FakeMind::new())));
    let phone = c.phone(c.pair());
    let (file, _) = home.backup();
    let (big, sha, size) = big_backup(&home);
    let offer = RemoteCall::OfferMemory {
        counts: home.counts(),
        bytes: size,
        schema: nocturne_engine::store::latest_version(),
        since: None,
        things: None,
        bring: false,
    };
    assert_eq!(offered(&phone, offer), BROUGHT);
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
    assert_eq!(
        c.view(),
        Some(MoveView::Ended {
            name: NAME.into(),
            why: EndedWhy::Removed
        })
    );
    assert_eq!(
        c.h.engine().stats().unwrap().item_count,
        0,
        "nothing came in"
    );
    assert_eq!(c.hub.core.incoming.in_flight(), 0);
    assert!(!nocturne_app_lib::restore::marker_path(c.data()).exists());
    // Removed, it cannot offer again.
    assert!(block(phone.call(home.offer(&file))).is_err());
}

#[test]
fn keeping_what_is_here_undoes_the_pairing() {
    let home = phone_home();
    let c = computer_with_a_memory();
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
}

#[test]
fn a_phone_that_says_not_now_takes_the_question_away() {
    let home = phone_home();
    let c = computer_with_a_memory();
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
    // Enough to be cut part way.
    let (big, big_sha, size) = big_backup(&home);
    let (file, sha) = home.backup();
    let offer = || RemoteCall::OfferMemory {
        counts: home.counts(),
        bytes: size,
        schema: nocturne_engine::store::latest_version(),
        since: None,
        things: None,
        bring: false,
    };
    assert_eq!(offered(&phone, offer()), BROUGHT);
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
        matches!(c.view(), Some(MoveView::Preparing { .. })) && c.hub.core.incoming.in_flight() == 0
    });
    assert_eq!(c.h.engine().stats().unwrap().item_count, 0);
    // Carrying on after the pause, it is agreed as before.
    assert_eq!(offered(&phone, offer()), BROUGHT);

    // A file that is not what the phone said is refused whole, and nothing changes either.
    let wrong = block(phone.upload_memory(done(&file, &big_sha.replace('a', "b")), &file, |_| {}));
    assert_eq!(
        wrong.err(),
        Some(CallError::Phone {
            code: PhoneError::Invalid
        })
    );
    assert_eq!(c.h.engine().stats().unwrap().item_count, 0);
    assert!(c.view().is_none(), "the phone may offer it again");

    // Offered again, its things are added.
    assert_eq!(offered(&phone, home.offer(&file)), BROUGHT);
    assert!(matches!(
        block(phone.upload_memory(done(&file, &sha), &file, |_| {})).unwrap(),
        RemoteAnswer::MemoryBrought { .. }
    ));
}

#[test]
fn a_memory_is_not_taken_unless_offered_and_fitting() {
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
        since: None,
        things: None,
        bring: false,
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
        since: None,
        things: None,
        bring: false,
    }));
    assert_eq!(
        newer.err(),
        Some(CallError::Phone {
            code: PhoneError::ComputerOutdated
        })
    );
    assert!(c.view().is_none());
}

#[test]
fn short_of_room_nothing_moves_until_there_is_some() {
    let home = phone_home();
    let c = computer(harness_with_mind(Arc::new(FakeMind::new())));
    let phone = c.phone(c.pair());
    let (file, sha) = home.backup();
    nocturne_app_lib::remote::moving::pretend_free(c.data(), Some(1024));

    assert_eq!(
        block(phone.call(home.offer(&file))).err(),
        Some(CallError::Phone {
            code: PhoneError::TooLarge
        })
    );
    assert!(
        c.h.ok("get_move", json!({})).is_null(),
        "nothing shows here: the phone says so"
    );

    // Room freed: the phone's next try is agreed at once.
    nocturne_app_lib::remote::moving::pretend_free(c.data(), Some(u64::MAX / 4));
    assert_eq!(offered(&phone, home.offer(&file)), BROUGHT);
    assert!(matches!(
        block(phone.upload_memory(done(&file, &sha), &file, |_| {})).unwrap(),
        RemoteAnswer::MemoryBrought { .. }
    ));
}

#[test]
fn stopping_on_the_computer_ends_the_move_and_the_phone_hears_so() {
    let home = phone_home();
    let c = computer(harness_with_mind(Arc::new(FakeMind::new())));
    let phone = c.phone(c.pair());
    let (big, sha, size) = big_backup(&home);
    let offer = RemoteCall::OfferMemory {
        counts: home.counts(),
        bytes: size,
        schema: nocturne_engine::store::latest_version(),
        since: None,
        things: None,
        bring: false,
    };
    assert_eq!(offered(&phone, offer), BROUGHT);
    let before = c.relay.sent();
    let p = phone.clone();
    let (call, path) = (done(&big, &sha), big.clone());
    let going =
        tauri::async_runtime::spawn(async move { p.upload_memory(call, &path, |_| {}).await });
    wait_for("part of it went", || {
        c.relay.sent() > before + 2 * 1024 * 1024
    });
    c.h.ok("stop_move", json!({}));
    assert_eq!(
        block(going).unwrap().err(),
        Some(CallError::Phone {
            code: PhoneError::Paused
        })
    );
    assert!(c.view().is_none());
    assert_eq!(c.h.engine().stats().unwrap().item_count, 0);
    assert_eq!(c.hub.core.incoming.in_flight(), 0);
}

#[test]
fn a_stop_before_the_upload_reaches_the_phone_and_not_the_next_move() {
    let home = phone_home();
    let c = computer(harness_with_mind(Arc::new(FakeMind::new())));
    let phone = c.phone(c.pair());
    let (file, sha) = home.backup();
    assert_eq!(offered(&phone, home.offer(&file)), BROUGHT);
    c.h.ok("stop_move", json!({}));
    assert!(c.view().is_none());
    assert_eq!(
        block(phone.upload_memory(done(&file, &sha), &file, |_| {})).err(),
        Some(CallError::Phone {
            code: PhoneError::Paused
        }),
        "the phone hears the move was stopped"
    );
    // Tried again, it is agreed again and the things come.
    assert_eq!(offered(&phone, home.offer(&file)), BROUGHT);
    assert!(matches!(
        block(phone.upload_memory(done(&file, &sha), &file, |_| {})).unwrap(),
        RemoteAnswer::MemoryBrought { .. }
    ));
}

#[test]
fn a_phone_that_asks_where_its_memory_stands_starts_nothing() {
    let c = computer(harness_with_mind(Arc::new(FakeMind::new())));
    let phone = c.phone(c.pair());
    assert_eq!(
        block(phone.memory_status()).err(),
        Some(CallError::Phone {
            code: PhoneError::NotReady
        }),
        "nothing is known of it here"
    );
    assert!(c.view().is_none(), "nothing starts");
}

/// As the phone app does it: an offer that guesses the size low, then, once agreed and the backup
/// is made, an ask of where its memory stands before anything is sent.
#[test]
fn the_phone_asks_where_its_memory_stands_then_sends_it() {
    let home = phone_home();
    let c = computer(harness_with_mind(Arc::new(FakeMind::new())));
    let phone = c.phone(c.pair());
    let (file, sha) = home.backup();
    let offer = RemoteCall::OfferMemory {
        counts: home.counts(),
        bytes: 4096.0,
        schema: nocturne_engine::store::latest_version(),
        since: None,
        things: None,
        bring: false,
    };
    assert_eq!(offered(&phone, offer), BROUGHT);
    assert_eq!(
        block(phone.memory_status()).err(),
        Some(CallError::Phone {
            code: PhoneError::NotReady
        })
    );
    let brought = block(phone.upload_memory(done(&file, &sha), &file, |_| {})).unwrap();
    assert!(
        matches!(brought, RemoteAnswer::MemoryBrought { .. }),
        "{brought:?}"
    );
}

/// A computer that already holds a memory: the gate code, kept and learned.
fn computer_with_a_memory() -> Computer {
    let h = harness_with_mind(Arc::new(FakeMind::new()));
    h.learn(GATE, "The gate code is 4512.", "Gate", "4512");
    computer(h)
}

/// The phone's calendar as the computer takes it: how many came in, and how many it had.
fn send_calendar(phone: &Phone, sync: &str) -> (u32, u32) {
    let RemoteAnswer::Synced { receipt } = block(phone.call(RemoteCall::SendCalendar {
        batch: calendar(sync),
    }))
    .unwrap() else {
        panic!("a receipt")
    };
    (receipt.taken, receipt.unchanged)
}

/// Things found by `words`: one each, however often they were kept.
fn items_like(c: &Computer, words: &str) -> usize {
    c.h.engine().find_items(words, 50).unwrap().len()
}

fn big_backup(home: &PhoneHome) -> (PathBuf, String, f64) {
    let (file, _) = home.backup();
    let big = home.dir.path().join("big.nocturne");
    let mut bytes = std::fs::read(&file).unwrap();
    bytes.resize(bytes.len() + 6 * 1024 * 1024, 7);
    std::fs::write(&big, &bytes).unwrap();
    let sha: String = Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    (big, sha, bytes.len() as f64)
}

#[test]
fn the_hello_says_whether_the_computer_holds_a_memory() {
    let c = computer(harness_with_mind(Arc::new(FakeMind::new())));
    let phone = c.phone(c.pair());
    let Connection::Connected { hello, .. } = block(phone.connect()) else {
        panic!("connected")
    };
    assert!(!hello.memory_here);
    c.h.learn("garden", GATE, "Gate", "4512");
    let phone = c.phone(c.pair());
    let Connection::Connected { hello, .. } = block(phone.connect()) else {
        panic!("connected")
    };
    assert!(hello.memory_here);
}

#[test]
fn a_computer_with_a_memory_brings_the_phones_things_in() {
    let home = phone_home().with_more();
    let c = computer_with_a_memory();
    let before = c.h.engine().stats().unwrap().item_count;
    let phone = c.phone(c.pair());
    let device = c.device_id();
    let (file, sha) = home.backup();

    let p = phone.clone();
    let offer = home.offer(&file);
    let asked = tauri::async_runtime::spawn(async move { p.call(offer).await });
    wait_for("the computer asks", || c.view().is_some());
    let shown = c.h.ok("get_move", json!({}));
    assert_eq!(shown["phase"], "offered");
    assert_eq!(shown["here"], true, "it asks how the phone joins");
    assert_eq!(
        c.h.code("answer_move", json!({"choice": "move"})),
        "notFound",
        "a memory here is never put aside in a move"
    );
    c.h.ok("answer_move", json!({"choice": "bring"}));
    let RemoteAnswer::MemoryAccepted {
        here: Some(_),
        bring: true,
    } = block(asked).unwrap().unwrap()
    else {
        panic!("the phone hears it brings its things in")
    };
    assert!(matches!(
        c.view(),
        Some(MoveView::Preparing { bring: true, .. })
    ));

    let brought = block(phone.upload_memory(done(&file, &sha), &file, |_| {})).unwrap();
    let RemoteAnswer::MemoryBrought { counts, added } = brought else {
        panic!("{brought:?}")
    };
    assert_eq!(counts, home.counts(), "every count matches");
    assert_eq!(
        added,
        BroughtCounts {
            things: 1,
            entries: 0,
            checkins: 1,
        },
        "the gate code was here already"
    );
    assert!(matches!(c.view(), Some(MoveView::Brought { .. })));
    assert!(
        !nocturne_app_lib::restore::marker_path(c.data()).exists(),
        "no restart"
    );

    let e = c.h.engine();
    assert_eq!(e.stats().unwrap().item_count, before + 2);
    assert_eq!(items_like(&c, GATE), 1, "kept once");
    let gate = e.find_items(GATE, 10).unwrap().remove(0).receipt.item_id;
    assert!(e.get_item(&gate).unwrap().private, "private on either side");
    let note = e
        .remote_keep(&device, "c-note-1")
        .unwrap()
        .expect("the note came");
    let note = e.get_item(&note).unwrap();
    assert_eq!(note.from_device.as_deref(), Some(NAME));
    assert_eq!(
        note.status,
        nocturne_contract::ItemStatus::Kept,
        "read as new"
    );
    assert_eq!(
        e.devices(false).unwrap().len(),
        1,
        "the phone stays one of its phones"
    );

    // The phone missed the answer: it hears it again, however often it asks.
    assert_eq!(
        block(phone.memory_status()).unwrap(),
        RemoteAnswer::MemoryBrought { counts, added }
    );
    // Its calendar comes again from the phone, and only once.
    assert_eq!(send_calendar(&phone, "s2"), (1, 0));
    assert_eq!(send_calendar(&phone, "s3"), (0, 1));

    let shown = c.h.ok("get_move", json!({}));
    assert_eq!(shown["phase"], "brought");
    assert_eq!(shown["added"]["checkins"], 1);
    c.h.ok("finish_move", json!({}));
    assert!(c.h.ok("get_move", json!({})).is_null());
}

#[test]
fn using_the_computers_memory_brings_nothing_and_the_phone_stays_paired() {
    let home = phone_home();
    let c = computer_with_a_memory();
    let before = c.h.engine().stats().unwrap().item_count;
    let phone = c.phone(c.pair());
    let (file, _) = home.backup();
    let held = home.counts();

    let answer = offer_and_answer(&c, &phone, home.offer(&file), "useHere");
    assert_eq!(answer, RemoteAnswer::UseComputerMemory);
    assert!(matches!(c.view(), Some(MoveView::Used { .. })));
    assert_eq!(
        c.h.engine().stats().unwrap().item_count,
        before,
        "nothing came in"
    );
    assert_eq!(home.counts(), held, "the phone keeps all it held");
    assert_eq!(c.h.engine().devices(false).unwrap().len(), 1);
    // Its calendar comes here from now on.
    assert_eq!(send_calendar(&phone, "s2"), (1, 0));
    // Offered again, it is not asked about twice.
    assert_eq!(
        block(phone.call(home.offer(&file))).unwrap(),
        RemoteAnswer::UseComputerMemory
    );
    c.h.ok("finish_move", json!({}));
    assert!(c.view().is_none());
}

#[test]
fn what_a_move_put_aside_is_brought_in_later_without_asking() {
    let home = phone_home();
    let c = computer_with_a_memory();
    let phone = c.phone(c.pair());
    let (file, sha) = home.backup();
    offer_and_answer(&c, &phone, home.offer(&file), "useHere");
    c.h.ok("finish_move", json!({}));
    assert_eq!(send_calendar(&phone, "s2"), (1, 0));

    let accepted = block(phone.call(home.bring(&file))).unwrap();
    assert!(
        matches!(accepted, RemoteAnswer::MemoryAccepted { bring: true, .. }),
        "{accepted:?}"
    );
    let shown = c.h.ok("get_move", json!({}));
    assert_eq!(shown["phase"], "preparing", "nothing is asked here");
    assert_eq!(shown["bring"], true);
    let brought = block(phone.upload_memory(done(&file, &sha), &file, |_| {})).unwrap();
    assert!(
        matches!(brought, RemoteAnswer::MemoryBrought { added, .. } if added.things == 1),
        "{brought:?}"
    );
    // The phone sends its calendar again from nothing: what came before counts as unchanged.
    assert_eq!(send_calendar(&phone, "s3"), (0, 1));
    // Brought in twice, nothing is doubled.
    let again = block(phone.call(home.bring(&file))).unwrap();
    assert!(
        matches!(again, RemoteAnswer::MemoryBrought { .. }),
        "{again:?}"
    );
    assert_eq!(
        items_like(&c, "The heron feeder is refilled on Thursday."),
        1
    );
}

#[test]
fn stopping_a_bring_in_before_its_things_are_added_changes_nothing() {
    let home = phone_home();
    let c = computer_with_a_memory();
    let before = c.h.engine().stats().unwrap().item_count;
    let phone = c.phone(c.pair());
    let (big, sha, size) = big_backup(&home);
    let offer = RemoteCall::OfferMemory {
        counts: home.counts(),
        bytes: size,
        schema: nocturne_engine::store::latest_version(),
        since: None,
        things: None,
        bring: false,
    };
    offer_and_answer(&c, &phone, offer, "bring");
    let sent = c.relay.sent();
    let p = phone.clone();
    let (call, path) = (done(&big, &sha), big.clone());
    let going =
        tauri::async_runtime::spawn(async move { p.upload_memory(call, &path, |_| {}).await });
    wait_for("part of it went", || {
        c.relay.sent() > sent + 2 * 1024 * 1024
    });
    assert!(matches!(
        c.view(),
        Some(MoveView::Arriving { bring: true, .. })
    ));
    c.h.ok("stop_move", json!({}));
    assert_eq!(
        block(going).unwrap().err(),
        Some(CallError::Phone {
            code: PhoneError::Paused
        })
    );
    assert!(c.view().is_none());
    assert_eq!(c.h.engine().stats().unwrap().item_count, before);
    assert_eq!(c.hub.core.incoming.in_flight(), 0);
}

#[test]
fn the_phone_stopping_a_bring_in_stops_it_here_and_says_so() {
    let home = phone_home();
    let c = computer_with_a_memory();
    let before = c.h.engine().stats().unwrap().item_count;
    let phone = c.phone(c.pair());
    let (big, sha, size) = big_backup(&home);
    let offer = RemoteCall::OfferMemory {
        counts: home.counts(),
        bytes: size,
        schema: nocturne_engine::store::latest_version(),
        since: None,
        things: None,
        bring: false,
    };
    offer_and_answer(&c, &phone, offer, "bring");
    let sent = c.relay.sent();
    let p = phone.clone();
    let (call, path) = (done(&big, &sha), big.clone());
    let going =
        tauri::async_runtime::spawn(async move { p.upload_memory(call, &path, |_| {}).await });
    wait_for("part of it went", || {
        c.relay.sent() > sent + 2 * 1024 * 1024
    });
    assert_eq!(
        block(phone.call(RemoteCall::StopMemory)).unwrap(),
        RemoteAnswer::Done
    );
    assert!(block(going).unwrap().is_err());
    assert_eq!(
        c.view(),
        Some(MoveView::Ended {
            name: NAME.into(),
            why: EndedWhy::Stopped
        })
    );
    assert_eq!(c.h.engine().stats().unwrap().item_count, before);
    assert_eq!(c.hub.core.incoming.in_flight(), 0);
    c.h.ok("finish_move", json!({}));
    assert!(c.view().is_none());
}

#[test]
fn removing_the_phone_while_it_is_asked_how_it_joins_takes_the_question_away() {
    let home = phone_home();
    let c = computer_with_a_memory();
    let phone = c.phone(c.pair());
    let (file, _) = home.backup();
    let p = phone.clone();
    let offer = home.offer(&file);
    let asked = tauri::async_runtime::spawn(async move { p.call(offer).await });
    wait_for("the computer asks", || c.view().is_some());
    c.h.ok("remove_device", json!({"id": c.device_id()}));
    assert!(c.view().is_none());
    assert!(block(asked).unwrap().is_err());
    assert_eq!(
        c.h.code("answer_move", json!({"choice": "useHere"})),
        "notFound"
    );
}

#[test]
fn removing_the_phone_while_its_things_arrive_adds_nothing() {
    let home = phone_home();
    let c = computer_with_a_memory();
    let before = c.h.engine().stats().unwrap().item_count;
    let phone = c.phone(c.pair());
    let (big, sha, size) = big_backup(&home);
    let offer = RemoteCall::OfferMemory {
        counts: home.counts(),
        bytes: size,
        schema: nocturne_engine::store::latest_version(),
        since: None,
        things: None,
        bring: false,
    };
    offer_and_answer(&c, &phone, offer, "bring");
    let sent = c.relay.sent();
    let p = phone.clone();
    let (call, path) = (done(&big, &sha), big.clone());
    let going =
        tauri::async_runtime::spawn(async move { p.upload_memory(call, &path, |_| {}).await });
    wait_for("part of it went", || {
        c.relay.sent() > sent + 2 * 1024 * 1024
    });
    let shown = c.h.events("move-changed");
    c.h.ok("remove_device", json!({"id": c.device_id()}));
    assert!(block(going).unwrap().is_err());
    assert_eq!(
        c.view(),
        Some(MoveView::Ended {
            name: NAME.into(),
            why: EndedWhy::Removed
        }),
        "the person here hears that nothing came in"
    );
    assert_eq!(
        shown.lock().unwrap().last().map(|v| v["phase"].clone()),
        Some(json!("ended")),
        "and the window is told so"
    );
    assert_eq!(c.h.engine().stats().unwrap().item_count, before);
    c.h.ok("finish_move", json!({}));
    assert!(c.view().is_none());
    assert!(
        block(phone.call(home.bring(&big))).is_err(),
        "removed, it cannot offer again"
    );
}
