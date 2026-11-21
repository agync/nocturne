//! The phone app's client against a computer from before the version window: its hello leaves
//! the newest build out, so the two speak build 1. The app introduces nothing, asks no journal
//! question and keeps a shared web page waiting, sending only what build 1 reads as meant.

#![cfg(unix)]

use std::sync::{Arc, Mutex};

use nocturne_app_lib::remote::listener::{self, Bind, ListenerConfig};
use nocturne_app_lib::remote::session::{BoxFuture, CallCtx, Core, Dispatch};
use nocturne_app_lib::remote::static_files::DirFiles;
use nocturne_app_lib::remote::testing::{self, TEST_HOST};
use nocturne_channel::client::{self, CallError, Connection, OnIdentity, Options, Phone};
use nocturne_channel::protocol::{MIN_PHONE_BUILD, PHONE_BUILD};
use nocturne_contract::{
    AppPlatform, PhoneAppIntro, PhoneError, PhoneReadiness, PhoneStatus, RemoteAnswer, RemoteCall,
};
use nocturne_helpers::Origin;

fn block<T>(f: impl std::future::Future<Output = T>) -> T {
    tauri::async_runtime::block_on(f)
}

/// Notes each call's name, and answers as a build-1 computer would.
struct Old {
    calls: Arc<Mutex<Vec<String>>>,
}

fn page_asking_from_the_journal() -> RemoteAnswer {
    let page = serde_json::json!({
        "date": "2026-11-13", "pageNumber": 42, "edition": "phone", "variant": "normal",
        "headline": "A quiet day.", "dek": "", "needsYou": [], "changed": [], "letGo": [],
        "upcoming": [], "learned": [],
        "question": {"kind": "journal", "text": "Call the plumber.", "memoryId": null,
            "receipt": null, "lastAsked": null, "answered": null},
        "moreNeedsYou": 0, "moreChanged": 0, "keptCount": 0, "changesCount": 0,
        "letGoCount": 0, "catchUpDays": 0, "privateSkipped": 0, "pending": 0,
        "tidiedFrom": null, "tidiedTo": null, "update": null
    });
    RemoteAnswer::Page {
        page: Box::new(serde_json::from_value(page).unwrap()),
    }
}

impl Dispatch for Old {
    fn call(&self, _core: Arc<Core>, ctx: CallCtx) -> BoxFuture<'static, ()> {
        let call = serde_json::to_value(&ctx.req.call).unwrap();
        self.calls
            .lock()
            .unwrap()
            .push(call["cmd"].as_str().unwrap().to_string());
        Box::pin(async move {
            let answer = match ctx.req.call {
                RemoteCall::GetStatus => RemoteAnswer::Status {
                    status: PhoneStatus {
                        readiness: PhoneReadiness::Ready,
                        page_date: None,
                        needs_you: 0,
                    },
                },
                RemoteCall::GetPage { .. } => page_asking_from_the_journal(),
                _ => RemoteAnswer::Done,
            };
            ctx.out.send(answer);
        })
    }
}

/// A relay on loopback in front of the listener's socket, where the client's loopback router
/// points.
fn relay(socket: std::path::PathBuf) -> u16 {
    let listener = block(tokio::net::TcpListener::bind(("127.0.0.1", 0))).unwrap();
    let port = listener.local_addr().unwrap().port();
    tauri::async_runtime::spawn(async move {
        while let Ok((mut phone, _)) = listener.accept().await {
            let socket = socket.clone();
            tokio::spawn(async move {
                if let Ok(mut computer) = tokio::net::UnixStream::connect(socket).await {
                    let _ = tokio::io::copy_bidirectional(&mut phone, &mut computer).await;
                }
            });
        }
    });
    port
}

#[test]
fn a_computer_from_before_the_window_gets_only_what_build_1_reads() {
    let dir = tempfile::tempdir().unwrap();
    let bench = testing::bench(dir.path());
    bench
        .shell
        .before_window
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let calls: Arc<Mutex<Vec<String>>> = Arc::default();
    bench.core.set_dispatch(Arc::new(Old {
        calls: calls.clone(),
    }));
    let files = dir.path().join("phone");
    std::fs::create_dir_all(&files).unwrap();
    let handle = block(listener::serve(
        ListenerConfig {
            bind: Bind::Unix(dir.path().join("remote/run/listener.sock")),
            files: Arc::new(DirFiles(files)),
            keepalive: listener::WS_KEEPALIVE,
        },
        bench.core.clone(),
    ))
    .unwrap();
    let Origin::Unix(socket) = handle.origin.clone() else {
        panic!("a socket listener")
    };
    let port = relay(socket);

    let keys = bench.paired("Pixel");
    let options = Options {
        app: Some(PhoneAppIntro {
            platform: AppPlatform::Android,
            version: "0.8.0".into(),
            min_build: MIN_PHONE_BUILD,
            max_build: PHONE_BUILD,
        }),
        ..Options::default()
    };
    let on: OnIdentity = Arc::new(|_| {});
    let phone = Phone::new(
        keys.identity(TEST_HOST, "Pixel"),
        client::loopback_router(port),
        options,
        on,
    );

    let Connection::Connected { hello, agreed } = block(phone.connect()) else {
        panic!("{:?}", phone.state())
    };
    assert_eq!(hello.max_phone_build, None);
    assert_eq!(agreed, 1);

    let RemoteAnswer::Page { page } =
        block(phone.call(RemoteCall::GetPage { date: None })).unwrap()
    else {
        panic!("a page")
    };
    assert_eq!(page.question, None, "no journal question below build 2");

    let keep = |web| RemoteCall::KeepText {
        text: "https://example.com/a-recipe".into(),
        client_id: if web { "c-web" } else { "c-plain" }.into(),
        feeling: None,
        good: None,
        captured_at: None,
        web,
    };
    assert_eq!(
        block(phone.call(keep(true))).err(),
        Some(CallError::Phone {
            code: PhoneError::ComputerOutdated
        }),
        "a shared web page waits on the phone"
    );
    assert!(block(phone.call(keep(false))).is_ok());
    let answer = RemoteCall::AnswerJournalQuestion {
        page_date: "2026-11-13".into(),
        choice: nocturne_contract::QuestionChoice::Fine,
        line: None,
        client_id: "c-answer".into(),
    };
    assert!(block(phone.call(answer)).is_err());

    assert_eq!(
        *calls.lock().unwrap(),
        ["getPage", "keepText"],
        "no introduction, no web keep, no journal answer went across"
    );
    assert!(matches!(phone.state(), Connection::Connected { .. }));
    block(handle.stop());
}
