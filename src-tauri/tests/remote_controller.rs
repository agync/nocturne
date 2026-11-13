//! Turning Away from home on and off through the commands, over fake helpers: off means off,
//! consent before download, the helper checked before each start, re-linking, problems,
//! auto-off, keep-awake, removing everything and deleting everything.

mod common;

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use common::*;
use nocturne_app_lib::remote::controller::{IDLE_PERIOD, RemoteHub};
use nocturne_app_lib::remote::limits::ManualMono;
use nocturne_app_lib::remote::protocol::*;
use nocturne_app_lib::remote::testing::client::{Addr, Mode, Opened, PhoneClient, Wire};
use nocturne_app_lib::remote::testing::{self, FakeHelpers, PhonePair, TEST_HOST};
use nocturne_contract::{
    LedgerKind, RemoteAction, RemoteConnection, RemoteProblem, RemoteState, RemoteView, SignInStep,
};
use nocturne_helpers::{HelperError, HelperKind, InstallProgress, Origin, TransportState};
use nocturne_intelligence::FakeHost;
use serde_json::json;
use tauri::{Emitter, Manager};

struct Away {
    h: Harness,
    hub: Arc<RemoteHub>,
    helpers: Arc<FakeHelpers>,
    mono: Arc<ManualMono>,
    host: Arc<FakeHost>,
    _files: tempfile::TempDir,
}

fn away() -> Away {
    let h = harness();
    let files = tempfile::tempdir().unwrap();
    std::fs::write(files.path().join("index.html"), b"<!doctype html>").unwrap();
    let host = FakeHost::new();
    let (hub, helpers, mono) = testing::install_fakes(h.app.handle(), host.clone(), files.path());
    Away {
        h,
        hub,
        helpers,
        mono,
        host,
        _files: files,
    }
}

fn block<T>(f: impl std::future::Future<Output = T>) -> T {
    tauri::async_runtime::block_on(f)
}

impl Away {
    fn view(&self) -> RemoteView {
        serde_json::from_value(self.h.ok("get_remote", json!({}))).unwrap()
    }

    fn start(&self, connection: &str, allow: bool) -> Result<RemoteView, String> {
        self.h
            .call(
                "start_remote",
                json!({"connection": connection, "allowDownload": allow}),
            )
            .map(|v| serde_json::from_value(v).unwrap())
            .map_err(|e| e["code"].as_str().unwrap_or_default().to_string())
    }

    fn on(&self) -> RemoteView {
        self.helpers.install_now(HelperKind::Cloudflared);
        self.helpers.install_now(HelperKind::Tailscaled);
        self.start("simple", false).unwrap();
        self.settle();
        self.view()
    }

    fn settle(&self) {
        wait_for("Away from home to settle", || {
            !matches!(self.hub.state(), RemoteState::Starting)
        });
    }

    fn addr(&self) -> Addr {
        match self.hub.listener_origin().expect("listening") {
            Origin::Unix(p) => Addr::Unix(p),
            Origin::Tcp { port, .. } => Addr::Tcp(port),
        }
    }

    fn remote_dir(&self) -> std::path::PathBuf {
        self.h.dir.path().join("remote")
    }

    /// Pair a phone through the real pairing path.
    fn pair(&self, name: &str) -> testing::PhoneKeys {
        let offer: serde_json::Value = self.h.ok("begin_pairing", json!({}));
        let link = offer["link"].as_str().unwrap().to_string();
        let addr = self.addr();
        let run = block(testing::client::Pairing::start(
            &addr,
            PhonePair::scanned(&link, name),
            Mode::WebSocket,
        ))
        .unwrap();
        wait_for("the person to be asked", || {
            matches!(
                self.view().pairing.map(|p| p.phase),
                Some(nocturne_contract::PairingPhase::WaitingForYou { .. })
            )
        });
        self.h.ok(
            "decide_pairing",
            json!({"pairingId": offer["pairingId"], "allow": true}),
        );
        block(run.outcome()).unwrap().expect("allowed")
    }

    fn ledger(&self) -> Vec<(LedgerKind, String)> {
        self.h
            .engine()
            .ledger()
            .unwrap()
            .into_iter()
            .map(|e| (e.kind, e.label))
            .collect()
    }
}

fn nothing_runs(a: &Away) {
    let t = &a.helpers.transport;
    assert_eq!(t.starts.load(Ordering::SeqCst), 0, "no helper started");
    assert_eq!(
        a.helpers.installs.load(Ordering::SeqCst),
        0,
        "nothing downloaded"
    );
    assert!(a.hub.listener_origin().is_none(), "nothing listening");
    assert!(!a.remote_dir().join("run").exists(), "no socket folder");
    assert!(!a.hub.core.is_on());
}

#[test]
fn fresh_store_spawns_and_binds_nothing() {
    let a = away();
    let v = a.view();
    assert_eq!(v.state, RemoteState::Off);
    assert!(!v.set_up_before);
    assert!(v.devices.is_empty());
    nocturne_app_lib::remote::install(a.h.app.handle()).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    nothing_runs(&a);
    assert!(!a.remote_dir().join("computer.key").exists());
}

#[test]
fn off_never_spawns_or_binds() {
    let a = away();
    let forged = json!({"type": "on", "address": "evil.example", "machineName": null});
    a.h.app
        .emit("remote-changed", json!({"state": forged}))
        .unwrap();
    for (cmd, args) in [
        ("get_remote", json!({})),
        ("stop_remote", json!({"removeAll": false})),
        ("stop_remote", json!({"removeAll": true})),
        ("pause_remote_download", json!({})),
        ("begin_pairing", json!({})),
        ("decide_pairing", json!({"pairingId": "x", "allow": true})),
        ("cancel_pairing", json!({})),
        ("rename_device", json!({"id": "x", "name": "Pixel"})),
        ("remove_device", json!({"id": "x"})),
        ("list_remote_events", json!({"limit": 10})),
        (
            "update_remote_settings",
            json!({"patch": {"keepAwake": true, "autoOff": "afterDay"}}),
        ),
        (
            "update_remote_settings",
            json!({"patch": {"connection": "mostPrivate"}}),
        ),
        ("open_remote_sign_in", json!({})),
        ("get_privacy", json!({})),
        ("get_advanced", json!({})),
    ] {
        let _ = a.h.call(cmd, args);
    }
    std::thread::sleep(Duration::from_millis(100));
    nothing_runs(&a);
    assert_eq!(a.host.inhibitors(), 0);
    assert!(!a.h.engine().remote_state().unwrap().wanted_on);
}

#[test]
fn pairing_needs_on() {
    let a = away();
    assert_eq!(a.h.code("begin_pairing", json!({})), "unavailable");
}

#[test]
fn start_without_consent_and_no_helper_is_refused() {
    let a = away();
    assert_eq!(a.start("simple", false).unwrap_err(), "invalid");
    nothing_runs(&a);
    assert!(!a.h.engine().remote_state().unwrap().wanted_on);
}

#[test]
fn download_progress_pause_resume_and_ledger() {
    let a = away();
    *a.helpers.progress.lock().unwrap() = vec![InstallProgress {
        received: 40,
        total: 100,
    }];
    a.start("simple", true).unwrap();
    wait_for(
        "progress",
        || matches!(a.hub.state(), RemoteState::Downloading { received_bytes, .. } if received_bytes == 40.0),
    );
    let v: RemoteView = serde_json::from_value(a.h.ok("pause_remote_download", json!({}))).unwrap();
    assert!(
        matches!(v.state, RemoteState::Downloading { paused: true, received_bytes, .. } if received_bytes == 40.0)
    );
    assert_eq!(a.helpers.transport.starts.load(Ordering::SeqCst), 0);
    a.start("simple", true).unwrap();
    assert_eq!(a.helpers.installs.load(Ordering::SeqCst), 2);
    a.helpers.finish.notify_one();
    wait_for("on", || matches!(a.hub.state(), RemoteState::On { .. }));
    assert!(
        a.ledger()
            .contains(&(LedgerKind::RemoteHelper, "Cloudflare".into()))
    );
}

#[test]
fn changed_helper_binary_refused_at_start() {
    let a = away();
    a.helpers.install_now(HelperKind::Cloudflared);
    a.helpers.changed.store(true, Ordering::SeqCst);
    let v = a.start("simple", false).unwrap();
    assert!(matches!(
        v.state,
        RemoteState::Problem {
            code: RemoteProblem::CheckFailed,
            ..
        }
    ));
    assert_eq!(a.helpers.transport.starts.load(Ordering::SeqCst), 0);
    assert!(a.hub.listener_origin().is_none());
}

#[test]
fn turn_on_and_off_written_to_ledger() {
    let a = away();
    let v = a.on();
    assert!(matches!(v.state, RemoteState::On { .. }));
    a.h.ok("stop_remote", json!({"removeAll": false}));
    let rows: Vec<String> = a
        .ledger()
        .into_iter()
        .filter(|(k, _)| *k == LedgerKind::RemoteOnOff)
        .map(|(_, l)| l)
        .collect();
    assert_eq!(rows, vec!["on", "off"]);
    let events: Vec<RemoteAction> =
        a.h.engine()
            .remote_events(10)
            .unwrap()
            .into_iter()
            .map(|e| e.action)
            .collect();
    assert!(events.contains(&RemoteAction::TurnedOn) && events.contains(&RemoteAction::TurnedOff));
}

#[test]
fn leaving_setup_before_pairing_turns_off() {
    let a = away();
    a.on();
    assert!(a.h.engine().remote_state().unwrap().wanted_on);
    let v: RemoteView =
        serde_json::from_value(a.h.ok("stop_remote", json!({"removeAll": false}))).unwrap();
    assert_eq!(v.state, RemoteState::Off);
    assert!(!a.h.engine().remote_state().unwrap().wanted_on);
    assert!(!a.helpers.transport.running());
    assert!(a.hub.listener_origin().is_none());
}

const NEW_HOST: &str = "river-maple-stone-quiet.trycloudflare.com";

fn restarted_with_new_address(a: &Away) -> testing::PhoneKeys {
    *a.helpers.transport.addresses.lock().unwrap() = vec![
        format!("https://{TEST_HOST}"),
        format!("https://{NEW_HOST}"),
    ];
    a.on();
    let keys = a.pair("Pixel");
    a.h.ok("stop_remote", json!({"removeAll": false}));
    a.start("simple", false).unwrap();
    a.settle();
    keys
}

#[test]
fn simple_restart_needs_relink() {
    let a = away();
    restarted_with_new_address(&a);
    let v = a.view();
    let RemoteState::NeedsRelink {
        address,
        pairing_link,
        words,
        ..
    } = v.state
    else {
        panic!("needs re-link: {:?}", v.state)
    };
    assert_eq!(address, NEW_HOST);
    assert_eq!(pairing_link, format!("https://{NEW_HOST}/r#{NEW_HOST}"));
    assert_eq!(words, "river maple stone quiet");
    assert!(v.devices[0].needs_address);
}

#[test]
fn paired_phone_relinks_without_approval() {
    let a = away();
    let keys = restarted_with_new_address(&a);
    let Opened::Session(mut phone, _) = block(PhoneClient::open(&a.addr(), &keys, Mode::WebSocket))
    else {
        panic!("the same keys open a session at the new address")
    };
    block(phone.call(nocturne_contract::RemoteCall::GetStatus)).unwrap();
    wait_for("on again", || {
        matches!(a.hub.state(), RemoteState::On { .. })
    });
    let d = &a.h.engine().devices(false).unwrap()[0];
    assert_eq!(d.address.as_deref(), Some(NEW_HOST));
    assert!(!a.view().devices[0].needs_address);
}

#[test]
fn relink_code_cannot_pair() {
    let a = away();
    restarted_with_new_address(&a);
    let RemoteState::NeedsRelink { pairing_link, .. } = a.view().state else {
        panic!("needs re-link")
    };
    let fragment = pairing_link.split_once('#').unwrap().1;
    assert_eq!(fragment, NEW_HOST, "only the address, no secret");
    // A phone that treats the address as a code gets nothing.
    let s: [u8; S_LEN] = std::array::from_fn(|i| fragment.as_bytes()[i]);
    let phone = PhonePair::typed(
        &nocturne_app_lib::remote::crypto::code_encode(&s),
        NEW_HOST,
        "Stranger",
    );
    let mut wire = block(Wire::open(&a.addr(), Mode::WebSocket));
    block(wire.send(&phone.hello())).unwrap();
    assert!(block(wire.recv_within(Duration::from_millis(500))).is_err());
    assert_eq!(a.h.engine().devices(false).unwrap().len(), 1);
}

#[test]
fn most_private_keeps_its_address() {
    let a = away();
    a.helpers.install_now(HelperKind::Tailscaled);
    *a.helpers.transport.addresses.lock().unwrap() =
        vec!["https://nocturne-ab12.tail1234.ts.net".into()];
    a.start("mostPrivate", false).unwrap();
    a.settle();
    a.pair("Pixel");
    a.h.ok("stop_remote", json!({"removeAll": false}));
    a.start("mostPrivate", false).unwrap();
    a.settle();
    let RemoteState::On {
        address,
        machine_name,
    } = a.view().state
    else {
        panic!("on")
    };
    assert_eq!(address, "nocturne-ab12.tail1234.ts.net");
    assert!(machine_name.unwrap().starts_with("nocturne-"));
}

#[test]
fn switching_connection_unpairs_phones() {
    let a = away();
    a.on();
    a.pair("Pixel");
    a.pair("iPad");
    a.h.ok(
        "update_remote_settings",
        json!({"patch": {"connection": "mostPrivate"}}),
    );
    let e = a.h.engine();
    assert!(e.devices(false).unwrap().is_empty());
    assert_eq!(
        e.devices(true).unwrap().len(),
        2,
        "kept for receipts, revoked"
    );
    assert_eq!(
        e.remote_state().unwrap().connection,
        RemoteConnection::MostPrivate
    );
    assert!(
        e.remote_events(20)
            .unwrap()
            .iter()
            .any(|ev| ev.action == RemoteAction::SwitchedConnection)
    );
    a.settle();
    assert!(
        matches!(a.view().state, RemoteState::On { .. }),
        "still on, now Most private"
    );
}

#[test]
fn sign_in_then_allow_links_then_on() {
    let a = away();
    a.helpers.install_now(HelperKind::Tailscaled);
    *a.helpers.transport.on_start.lock().unwrap() = Some(TransportState::NeedsSignIn {
        url: "https://login.tailscale.com/a/abc".into(),
        allow_links: false,
    });
    a.start("mostPrivate", false).unwrap();
    wait_for("sign in", || {
        matches!(
            a.hub.state(),
            RemoteState::NeedsSignIn {
                step: SignInStep::SignIn,
                ..
            }
        )
    });
    assert_eq!(
        a.hub.sign_in_url().as_deref(),
        Some("https://login.tailscale.com/a/abc")
    );
    a.helpers.transport.set(TransportState::NeedsSignIn {
        url: "https://login.tailscale.com/f/serve".into(),
        allow_links: true,
    });
    wait_for("allow links", || {
        matches!(
            a.hub.state(),
            RemoteState::NeedsSignIn {
                step: SignInStep::AllowLinks,
                ..
            }
        )
    });
    a.helpers.transport.set(TransportState::Up {
        address: "https://nocturne-ab12.tail1234.ts.net".into(),
        route: None,
    });
    wait_for("on", || matches!(a.hub.state(), RemoteState::On { .. }));
    assert_eq!(a.hub.sign_in_url(), None);
}

#[test]
fn account_blocked_is_a_problem() {
    let a = away();
    a.on();
    a.helpers.transport.set(TransportState::Failed {
        error: HelperError::AccountBlocked,
    });
    wait_for("problem", || {
        matches!(
            a.hub.state(),
            RemoteState::Problem {
                code: RemoteProblem::AccountBlocked,
                ..
            }
        )
    });
    // "Try again" starts it from the problem.
    a.start("simple", false).unwrap();
    a.settle();
    assert!(matches!(a.view().state, RemoteState::On { .. }));
}

#[test]
fn short_helper_blip_not_shown() {
    let a = away();
    a.on();
    a.helpers
        .transport
        .set(TransportState::Restarting { attempt: 1 });
    std::thread::sleep(Duration::from_millis(100));
    a.mono.advance(Duration::from_secs(29));
    assert!(matches!(a.view().state, RemoteState::On { .. }));
    a.helpers.transport.set(TransportState::Up {
        address: format!("https://{TEST_HOST}"),
        route: None,
    });
    std::thread::sleep(Duration::from_millis(100));
    a.mono.advance(Duration::from_secs(60));
    assert!(matches!(a.view().state, RemoteState::On { .. }));
}

#[test]
fn down_30_seconds_is_a_problem() {
    let a = away();
    a.on();
    a.helpers
        .transport
        .set(TransportState::Restarting { attempt: 1 });
    std::thread::sleep(Duration::from_millis(100));
    a.mono.advance(Duration::from_secs(30));
    assert!(matches!(
        a.view().state,
        RemoteState::Problem {
            code: RemoteProblem::HelperFailed,
            ..
        }
    ));
}

#[test]
fn auto_off_stops_when_due() {
    let a = away();
    a.on();
    a.h.ok(
        "update_remote_settings",
        json!({"patch": {"autoOff": "afterDay"}}),
    );
    a.hub.tick();
    std::thread::sleep(Duration::from_millis(100));
    assert!(a.helpers.transport.running(), "not yet");
    a.h.clock.advance_days(1);
    a.hub.tick();
    wait_for("off by itself", || a.hub.state() == RemoteState::Off);
    assert!(!a.helpers.transport.running());
}

#[test]
fn keep_awake_only_on_mains_and_while_on() {
    let a = away();
    a.h.ok(
        "update_remote_settings",
        json!({"patch": {"keepAwake": true}}),
    );
    assert_eq!(a.host.inhibitors(), 0, "not while off");
    a.on();
    a.hub.tick();
    assert_eq!(a.host.inhibitors(), 1);
    a.host.set_ac(Some(false));
    a.hub.tick();
    assert_eq!(a.host.inhibitors(), 0, "not on battery");
    a.host.set_ac(Some(true));
    a.hub.tick();
    assert_eq!(a.host.inhibitors(), 1);
    a.h.ok("stop_remote", json!({"removeAll": false}));
    assert_eq!(a.host.inhibitors(), 0);
}

#[test]
fn remove_all_revokes_and_signs_out() {
    let a = away();
    a.helpers.install_now(HelperKind::Tailscaled);
    *a.helpers.transport.addresses.lock().unwrap() =
        vec!["https://nocturne-ab12.tail1234.ts.net".into()];
    a.start("mostPrivate", false).unwrap();
    a.settle();
    a.pair("Pixel");
    a.h.ok("stop_remote", json!({"removeAll": true}));
    assert!(a.h.engine().devices(false).unwrap().is_empty());
    assert_eq!(a.helpers.transport.sign_outs.load(Ordering::SeqCst), 1);
    assert!(
        a.helpers
            .transport
            .signed_out_running
            .load(Ordering::SeqCst)
    );
}

#[test]
fn delete_everything_stops_signs_out_and_removes_remote_folder() {
    let a = away();
    a.helpers.install_now(HelperKind::Tailscaled);
    *a.helpers.transport.addresses.lock().unwrap() =
        vec!["https://nocturne-ab12.tail1234.ts.net".into()];
    a.start("mostPrivate", false).unwrap();
    a.settle();
    a.pair("Pixel");
    assert!(a.remote_dir().join("computer.key").exists());
    a.h.ok(
        "delete_everything",
        json!({"confirmPhrase": nocturne_engine::DELETE_PHRASE}),
    );
    assert!(!a.helpers.transport.running());
    assert_eq!(a.helpers.transport.sign_outs.load(Ordering::SeqCst), 1);
    assert!(
        a.helpers
            .transport
            .signed_out_running
            .load(Ordering::SeqCst),
        "signed out before the helper stopped"
    );
    assert!(!a.remote_dir().exists());
    assert_eq!(a.view().state, RemoteState::Off);
}

#[test]
fn wanted_on_restarts_at_launch() {
    let a = away();
    a.helpers.install_now(HelperKind::Cloudflared);
    a.h.engine()
        .update_remote_state(|r| {
            r.wanted_on = true;
            r.on_since = Some(a.h.state().now());
        })
        .unwrap();
    nocturne_app_lib::remote::install(a.h.app.handle()).unwrap();
    wait_for("started at launch", || {
        matches!(a.hub.state(), RemoteState::On { .. })
    });
    assert_eq!(a.helpers.transport.starts.load(Ordering::SeqCst), 1);
}

#[test]
fn idle_timers_no_faster_than_30_s() {
    let a = away();
    a.on();
    assert_eq!(a.hub.period(), IDLE_PERIOD);
    assert!(IDLE_PERIOD >= Duration::from_secs(30));
    a.h.ok("begin_pairing", json!({}));
    assert!(a.hub.period() < IDLE_PERIOD, "while a pairing is open");
    a.h.ok("cancel_pairing", json!({}));
    assert_eq!(a.hub.period(), IDLE_PERIOD);
}

#[test]
fn privacy_view_counts_phones() {
    let a = away();
    let p = a.h.ok("get_privacy", json!({}));
    assert_eq!(p["remoteOn"], false);
    assert!(p["remoteConnection"].is_null());
    a.on();
    let keys = a.pair("Pixel");
    let Opened::Session(mut phone, _) = block(PhoneClient::open(&a.addr(), &keys, Mode::WebSocket))
    else {
        panic!("a session")
    };
    block(phone.call(nocturne_contract::RemoteCall::GetStatus)).unwrap();
    a.hub.core.flush_sent();
    let p = a.h.ok("get_privacy", json!({}));
    assert_eq!(p["remoteOn"], true);
    assert_eq!(p["remoteConnection"], "simple");
    assert!(p["sentToPhonesBytes"].as_f64().unwrap() > 0.0);
    a.h.ok("stop_remote", json!({"removeAll": false}));
    let p = a.h.ok("get_privacy", json!({}));
    assert_eq!(p["remoteOn"], false);
    assert!(p["remoteLastOnAt"].is_string());
}

#[test]
fn advanced_view_names_helper_and_listener() {
    let a = away();
    assert!(a.h.ok("get_advanced", json!({}))["remote"].is_null());
    a.on();
    let adv = a.h.ok("get_advanced", json!({}));
    let r = &adv["remote"];
    assert_eq!(r["helperName"], "cloudflared");
    assert_eq!(r["helperChecked"], true);
    assert_eq!(r["address"], TEST_HOST);
    if cfg!(unix) {
        assert_eq!(r["listener"], "unix socket, owner only");
    }
    let _ = a.h.app.state::<nocturne_app_lib::AppState>();
}

#[test]
fn tray_turn_off_reads_state() {
    let a = away();
    // A forged event claims Away from home is on: the tray draws from the hub, not the event.
    let forged = json!({"state": {"type": "on", "address": "evil.example", "machineName": null}});
    a.h.app.emit("remote-changed", forged).unwrap();
    let (_, menu) = block(nocturne_app_lib::os::tray::current(a.h.app.handle()));
    let text = format!("{menu:?}");
    assert!(!text.contains("Away from home"), "{text}");
    nothing_runs(&a);
    a.on();
    let (_, menu) = block(nocturne_app_lib::os::tray::current(a.h.app.handle()));
    assert!(format!("{menu:?}").contains(nocturne_app_lib::os::tray::AWAY_ON));
}

#[test]
fn delete_everything_while_off_starts_nothing() {
    let a = away();
    a.helpers.install_now(HelperKind::Tailscaled);
    a.h.ok(
        "delete_everything",
        json!({"confirmPhrase": nocturne_engine::DELETE_PHRASE}),
    );
    assert_eq!(a.helpers.transport.starts.load(Ordering::SeqCst), 0);
    assert_eq!(
        a.helpers.transport.sign_outs.load(Ordering::SeqCst),
        1,
        "local state forgotten"
    );
    assert!(
        !a.helpers
            .transport
            .signed_out_running
            .load(Ordering::SeqCst)
    );
    assert!(a.hub.listener_origin().is_none());
}

#[test]
fn leaving_most_private_signs_out_first() {
    let a = away();
    a.helpers.install_now(HelperKind::Tailscaled);
    a.helpers.install_now(HelperKind::Cloudflared);
    a.start("mostPrivate", false).unwrap();
    a.settle();
    a.h.ok(
        "update_remote_settings",
        json!({"patch": {"connection": "simple"}}),
    );
    assert_eq!(a.helpers.transport.sign_outs.load(Ordering::SeqCst), 1);
    assert!(
        a.helpers
            .transport
            .signed_out_running
            .load(Ordering::SeqCst)
    );
}

#[test]
fn switching_during_download_starts_the_right_helper() {
    let a = away();
    a.helpers.install_now(HelperKind::Tailscaled);
    *a.helpers.transport.addresses.lock().unwrap() =
        vec!["https://nocturne-ab12.tail1234.ts.net".into()];
    a.start("simple", true).unwrap();
    wait_for("downloading", || {
        matches!(a.hub.state(), RemoteState::Downloading { .. })
    });
    a.h.ok(
        "update_remote_settings",
        json!({"patch": {"connection": "mostPrivate"}}),
    );
    a.settle();
    assert!(
        matches!(a.view().state, RemoteState::On { .. }),
        "{:?}",
        a.view().state
    );
    assert_eq!(
        *a.helpers.transport.kind.lock().unwrap(),
        HelperKind::Tailscaled
    );
    assert_eq!(
        a.helpers.installs.load(Ordering::SeqCst),
        1,
        "the first download stopped"
    );
    // The cancelled download does not come back to start anything.
    a.helpers.finish.notify_one();
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(a.helpers.transport.starts.load(Ordering::SeqCst), 1);
}
