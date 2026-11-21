//! The version window, end to end: a phone app introduces itself and learns the build both
//! sides speak, or which side must update; the phones list and the activity list say so; and
//! every call of every build the computer still speaks is dispatched as that build sent it.

mod common;

use std::sync::Arc;

use common::*;
use nocturne_app_lib::remote::controller::RemoteHub;
use nocturne_app_lib::remote::crypto;
use nocturne_app_lib::remote::protocol::*;
use nocturne_app_lib::remote::testing::client::{Addr, Mode, Opened, Pairing, PhoneClient};
use nocturne_app_lib::remote::testing::{self, FakeHelpers, PhonePair};
use nocturne_contract::{
    AppPlatform, PhoneAppIntro, PhoneError, PhoneHello, RemoteAction, RemoteAnswer, RemoteCall,
    RemoteState,
};
use nocturne_engine::FakeMind;
use nocturne_helpers::{HelperKind, Origin};
use nocturne_intelligence::FakeHost;
use serde_json::{Value, json};

struct Computer {
    h: Harness,
    hub: Arc<RemoteHub>,
    #[allow(dead_code)]
    helpers: Arc<FakeHelpers>,
    _files: tempfile::TempDir,
}

fn block<T>(f: impl std::future::Future<Output = T>) -> T {
    tauri::async_runtime::block_on(f)
}

fn on() -> Computer {
    let h = harness_with_mind(Arc::new(FakeMind::new()));
    let files = tempfile::tempdir().unwrap();
    std::fs::write(files.path().join("index.html"), b"<!doctype html>").unwrap();
    let (hub, helpers, _) = testing::install_fakes(h.app.handle(), FakeHost::new(), files.path());
    helpers.install_now(HelperKind::Cloudflared);
    h.ok(
        "start_remote",
        json!({"connection": "simple", "allowDownload": false}),
    );
    wait_for("on", || matches!(hub.state(), RemoteState::On { .. }));
    Computer {
        h,
        hub,
        helpers,
        _files: files,
    }
}

impl Computer {
    fn addr(&self) -> Addr {
        match self.hub.listener_origin().unwrap() {
            Origin::Unix(p) => Addr::Unix(p),
            Origin::Tcp { port, key } => Addr::Keyed { port, host: key },
        }
    }

    /// A phone paired and allowed, and its session with the computer's hello.
    fn session(&self, name: &str) -> (Box<PhoneClient>, PhoneHello) {
        let offer = self.h.ok("begin_pairing", json!({}));
        let run = block(Pairing::start(
            &self.addr(),
            PhonePair::scanned(offer["link"].as_str().unwrap(), name),
            Mode::WebSocket,
        ))
        .unwrap();
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
        let keys = block(run.outcome()).unwrap().unwrap();
        match block(PhoneClient::open(&self.addr(), &keys, Mode::WebSocket)) {
            Opened::Session(c, hello) => (c, hello),
            other => panic!("no session: {other:?}"),
        }
    }

    /// The phones list's line for `name`, as the window reads it.
    fn listed(&self, name: &str) -> Value {
        let view = self.h.ok("get_phone_sources", json!({}));
        view["phones"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["name"] == name)
            .cloned()
            .unwrap_or_else(|| panic!("{name} is listed"))
    }

    fn activity(&self) -> Vec<RemoteAction> {
        let events = self.h.engine().remote_events(50).unwrap();
        events.into_iter().map(|e| e.action).collect()
    }
}

fn introduce(platform: AppPlatform, version: &str, min: u32, max: u32) -> RemoteCall {
    RemoteCall::Introduce {
        app: PhoneAppIntro {
            platform,
            version: version.into(),
            min_build: min,
            max_build: max,
        },
    }
}

fn call(c: &mut PhoneClient, call: RemoteCall) -> RemoteAnswer {
    block(c.call(call)).unwrap()
}

#[test]
fn the_computer_says_the_builds_it_speaks() {
    let c = on();
    let (_, hello) = c.session("Pixel");
    assert_eq!(hello.min_phone_build, MIN_PHONE_BUILD);
    assert_eq!(hello.max_phone_build, Some(PHONE_BUILD));
}

#[test]
fn a_current_app_agrees_on_the_newest_build() {
    let c = on();
    let (mut s, _) = c.session("Pixel");
    let a = call(
        &mut s,
        introduce(AppPlatform::Android, "0.8.0", MIN_PHONE_BUILD, PHONE_BUILD),
    );
    assert_eq!(
        a,
        RemoteAnswer::Introduced {
            agreed: PHONE_BUILD
        }
    );
    let line = c.listed("Pixel");
    assert_eq!(
        line["app"],
        json!({"platform": "android", "version": "0.8.0"})
    );
    assert_eq!(line["inBrowser"], false);
}

#[test]
fn an_app_one_build_behind_still_connects_and_an_update_is_ready() {
    let c = on();
    let (mut s, _) = c.session("Pixel");
    let older = PHONE_BUILD - 1;
    let a = call(&mut s, introduce(AppPlatform::Ios, "0.7.9", 1, older));
    assert_eq!(a, RemoteAnswer::Introduced { agreed: older });
    assert_eq!(
        c.listed("Pixel")["app"],
        json!({"platform": "ios", "version": "0.7.9", "update": "ready"})
    );
    assert!(
        matches!(
            call(&mut s, RemoteCall::GetStatus),
            RemoteAnswer::Status { .. }
        ),
        "it goes on working"
    );
}

#[test]
fn an_app_older_than_the_window_must_update() {
    let c = on();
    let (mut s, _) = c.session("Pixel");
    let a = call(
        &mut s,
        introduce(AppPlatform::Android, "0.0.1", 0, MIN_PHONE_BUILD - 1),
    );
    assert_eq!(
        a,
        RemoteAnswer::Error {
            code: PhoneError::TooOld
        }
    );
    assert_eq!(c.listed("Pixel")["app"]["update"], "needed");
    assert!(!c.activity().contains(&RemoteAction::NeedsNewerHere));
}

#[test]
fn an_app_newer_than_this_computer_is_noted_in_the_activity() {
    let c = on();
    let (mut s, _) = c.session("Pixel");
    let a = call(
        &mut s,
        introduce(
            AppPlatform::Android,
            "9.0.0",
            PHONE_BUILD + 1,
            PHONE_BUILD + 2,
        ),
    );
    assert_eq!(
        a,
        RemoteAnswer::Error {
            code: PhoneError::Outdated
        }
    );
    assert!(c.activity().contains(&RemoteAction::NeedsNewerHere));
    assert_eq!(
        c.listed("Pixel")["app"],
        json!({"platform": "android", "version": "9.0.0"}),
        "no update line for the phone"
    );
}

#[test]
fn the_phone_page_in_a_browser_is_listed_as_such() {
    let c = on();
    let (mut s, _) = c.session("Pixel");
    let a = call(
        &mut s,
        introduce(AppPlatform::Browser, "0.8.0", MIN_PHONE_BUILD, PHONE_BUILD),
    );
    assert_eq!(
        a,
        RemoteAnswer::Introduced {
            agreed: PHONE_BUILD
        }
    );
    let line = c.listed("Pixel");
    assert_eq!(line["inBrowser"], true);
    assert_eq!(line["app"]["platform"], "browser");
}

#[test]
fn a_bad_introduction_is_refused() {
    let c = on();
    let (mut s, _) = c.session("Pixel");
    for bad in [
        introduce(AppPlatform::Android, "", 1, 2),
        introduce(AppPlatform::Android, "0.8.0 <b>", 1, 2),
        introduce(AppPlatform::Android, &"9".repeat(33), 1, 2),
        introduce(AppPlatform::Android, "0.8.0", 2, 1),
    ] {
        assert_eq!(
            call(&mut s, bad),
            RemoteAnswer::Error {
                code: PhoneError::Invalid
            }
        );
    }
    assert!(c.listed("Pixel").get("app").is_none(), "nothing noted");
}

/// Bytes that pass the magic check for `mime`, `len` long.
fn bytes_for(mime: &str, len: usize) -> Vec<u8> {
    let head: &[u8] = match mime {
        "image/jpeg" => &[0xff, 0xd8, 0xff, 0xe0],
        "audio/wav" => b"RIFF\0\0\0\0WAVE",
        other => panic!("no bytes for {other}"),
    };
    let mut b = head.to_vec();
    b.resize(len, 0);
    b
}

/// Send one call exactly as the fixture holds it, and wait for its first answer.
fn send_as_sent(s: &mut PhoneClient, id: u32, sent: &Value) -> RemoteAnswer {
    let body = json!({"id": id, "call": sent}).to_string();
    block(s.send_envelope(&crypto::envelope_json(body.as_bytes()))).unwrap();
    if let Some(mime) = sent["mime"].as_str() {
        let len = usize::try_from(sent["size"].as_u64().unwrap()).unwrap();
        block(s.chunk(id, 0, true, &bytes_for(mime, len))).unwrap();
    }
    block(s.answer(id)).unwrap()
}

#[test]
fn every_call_of_every_build_spoken_here_is_dispatched() {
    let c = on();
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../crates/contract/tests/fixtures");
    let mut id = 100;
    for build in MIN_PHONE_BUILD..=PHONE_BUILD {
        let dir = root.join(format!("phone-v{build}"));
        let mut files: Vec<_> = std::fs::read_dir(&dir)
            .unwrap_or_else(|_| panic!("fixtures for build {build}"))
            .map(|f| f.unwrap().path())
            .collect();
        files.sort();
        // Each build on a phone of its own: `disconnect` ends that phone, so it goes last.
        files.sort_by_key(|f| f.file_stem().unwrap() == "disconnect");
        let (mut s, _) = c.session(&format!("Phone v{build}"));
        for file in files {
            let examples: Vec<Value> =
                serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
            for sent in &examples {
                id += 1;
                let answer = send_as_sent(&mut s, id, sent);
                assert!(
                    !matches!(
                        answer,
                        RemoteAnswer::Error {
                            code: PhoneError::Invalid
                        }
                    ),
                    "build {build}: {} was refused as not understood",
                    sent["cmd"]
                );
            }
        }
    }
}
