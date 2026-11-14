//! Turning Online answers on and off over IPC: the route the engine is given, the key's
//! homes, the pauses, the limit, and what the views say. The engine route is the stand-in
//! provider; the keychain is in memory.

mod common;

use std::sync::Arc;

use common::*;
use nocturne_app_lib::online::testing::{
    MemoryKeychain, RouteSeen, StandIn, StandInPort, agree, confirm_with, fake_provider,
};
use nocturne_contract::EVENT_ONLINE;
use nocturne_engine::FakeMind;
use serde_json::{Value, json};
use tauri::Emitter;
use tauri::async_runtime::block_on;

const KEY: &str = "sk-hub-test-0123456789abcdefghijklmnop";
const URL: &str = "https://api.example.com/v1";

struct Lab {
    h: Harness,
    s: StandIn,
    port: Arc<StandInPort>,
    keychain: Arc<MemoryKeychain>,
}

fn lab_with(keychain: MemoryKeychain) -> Lab {
    let h = harness_with_mind(Arc::new(FakeMind::new()));
    let s = StandIn::start(&fake_provider());
    let port = Arc::new(StandInPort::new(Some(s.port)));
    let keychain = Arc::new(keychain);
    nocturne_app_lib::online::manage_hub(h.app.handle(), port.clone(), keychain.clone())
        .unwrap()
        .set_confirm(agree(Default::default()));
    Lab {
        h,
        s,
        port,
        keychain,
    }
}

fn lab() -> Lab {
    lab_with(MemoryKeychain::default())
}

impl Lab {
    fn script(&self, body: Value) {
        block_on(async {
            reqwest::Client::new()
                .post(self.s.url("/fake/script"))
                .json(&body)
                .send()
                .await
                .unwrap();
        });
    }

    fn requests(&self) -> Vec<Value> {
        self.port.own_with(
            &[KEY],
            block_on(async {
                reqwest::Client::new()
                    .get(self.s.url("/fake/requests"))
                    .send()
                    .await
                    .unwrap()
                    .json()
                    .await
                    .unwrap()
            }),
        )
    }

    fn check(&self, key: &str) -> Value {
        self.h.ok(
            "check_online",
            json!({"form": {"baseUrl": URL, "key": key, "model": "stand-in-model"}}),
        )
    }

    fn turn_on(&self, questions: u32) -> Value {
        assert_eq!(self.check(KEY)["type"], "works");
        self.h.ok(
            "turn_on_online",
            json!({"settings": {"phones": true, "sendSensitive": false,
                                "limit": {"kind": "questions", "questions": questions, "usd": null}}}),
        )
    }

    fn view(&self) -> Value {
        self.h.ok("get_online", json!({}))
    }

    fn ask(&self, question: &str) -> nocturne_contract::AskDelta {
        let seen = self.h.ask_deltas();
        let id = self
            .h
            .ok("ask", json!({"question": question}))
            .as_str()
            .unwrap()
            .to_string();
        wait_for("the answer", || {
            deltas_for(&seen, &id).iter().any(|d| is_terminal(d.phase))
        });
        wait_for("the ask to end", || self.h.state().asks.in_flight() == 0);
        deltas_for(&seen, &id).pop().unwrap()
    }

    fn key_file(&self) -> std::path::PathBuf {
        self.h.dir.path().join("online").join("key")
    }

    fn route_at_launch(&self) -> bool {
        nocturne_app_lib::online::route_at_launch(self.h.app.handle()).is_some()
    }

    fn ledger(&self) -> Vec<(String, String)> {
        self.h
            .engine()
            .ledger()
            .unwrap()
            .into_iter()
            .map(|e| {
                (
                    serde_json::to_value(e.kind)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_string(),
                    e.label,
                )
            })
            .collect()
    }

    fn notices(&self) -> Vec<String> {
        let status = self.h.ok("get_status", json!({}));
        status["notices"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n["code"].as_str().unwrap().to_string())
            .collect()
    }
}

fn stand_in_route() -> Option<RouteSeen> {
    Some(RouteSeen {
        base_url: URL.into(),
        model: "stand-in-model".into(),
        with_key: true,
    })
}

#[test]
fn fresh_store_sets_no_route() {
    let l = lab();
    let v = l.view();
    assert_eq!(v["state"]["type"], "off");
    assert_eq!(v["setUp"], false);
    assert!(!l.route_at_launch());
    nocturne_app_lib::online::install(l.h.app.handle()).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(200));
    assert!(l.port.routes().iter().all(Option::is_none));
    {
        let r = l.requests();
        assert!(r.is_empty(), "stray requests: {r:#?}");
    }
}

#[test]
fn another_ports_engine_client_is_not_this_labs_request() {
    let l = lab();
    let other = StandInPort::new(Some(l.s.port));
    let get = |key: &str| {
        block_on(async {
            reqwest::Client::new()
                .get(l.s.url("/v1/models"))
                .bearer_auth(key)
                .send()
                .await
                .unwrap();
        })
    };
    get(other.engine_key());
    block_on(async {
        reqwest::Client::new()
            .get(l.s.url("/v1/models"))
            .header("user-agent", "Go-http-client/1.1")
            .send()
            .await
            .unwrap();
    });
    assert!(l.requests().is_empty(), "a stray client is ignored");
    get(l.port.engine_key());
    assert_eq!(l.requests().len(), 1, "this lab's own engine client counts");
}

#[test]
fn off_with_key_stored_sets_no_route() {
    let l = lab();
    l.turn_on(200);
    l.h.ok("turn_off_online", json!({"forget": false}));
    assert!(l.keychain.holds_a_key(), "turning off keeps the key");
    assert_eq!(l.port.current(), None);
    assert!(!l.route_at_launch());
    l.port.set_has_cloud(Some(false));
    nocturne_app_lib::online::install(l.h.app.handle()).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(200));
    assert_eq!(l.port.current(), None);
}

#[test]
fn check_sets_provisional_route_and_logs_a_check() {
    let l = lab();
    let out = l.check(KEY);
    assert_eq!(
        out,
        json!({"type": "works", "host": "api.example.com", "model": "stand-in-model"})
    );
    assert_eq!(l.port.current(), stand_in_route());
    assert!(
        !l.route_at_launch(),
        "a route being set up is never launched with"
    );
    assert!(
        !l.keychain.holds_a_key(),
        "nothing is kept before the person agrees"
    );
    assert!(!l.key_file().exists());
    let v = l.view();
    assert_eq!(v["state"]["type"], "off");
    assert_eq!(v["setUp"], false);
    let sends =
        l.h.ok("list_online_sends", json!({"limit": 10, "before": null}));
    assert_eq!(sends.as_array().unwrap().len(), 1);
    assert_eq!(sends[0]["kind"], "check");
    assert_eq!(sends[0]["question"], "Reply with the word ready.");
    assert!(
        l.ledger()
            .contains(&("onlineConnected".into(), "api.example.com".into()))
    );
    assert_eq!(v["questionsThisMonth"], 0, "a check is not a question");
}

#[test]
fn check_refuses_addresses_before_anything_is_sent() {
    let l = lab();
    for (url, want) in [
        ("http://api.example.com/v1", "notHttps"),
        ("http://8.8.8.8/v1", "notHttps"),
        ("https://169.254.169.254/latest", "addressNotAllowed"),
        ("http://169.254.169.254/latest", "addressNotAllowed"),
        ("http://0.0.0.0:11434/v1", "addressNotAllowed"),
        ("http://[fe80::1]:11434/v1", "addressNotAllowed"),
        ("http://metadata.google.internal/v1", "addressNotAllowed"),
        ("https://api.example.com/v1?key=1", "addressNotAllowed"),
        ("http://192.168.1.10:8000/v1?key=1", "addressNotAllowed"),
    ] {
        let out = l.h.ok(
            "check_online",
            json!({"form": {"baseUrl": url, "key": KEY, "model": "m"}}),
        );
        assert_eq!(out["type"], want, "{url}");
    }
    assert!(l.port.routes().is_empty());
    {
        let r = l.requests();
        assert!(r.is_empty(), "stray requests: {r:#?}");
    }
}

#[test]
fn leaving_before_consent_forgets_key_and_route() {
    let l = lab();
    l.check(KEY);
    l.h.ok("abandon_online_setup", json!({}));
    assert_eq!(l.port.current(), None, "relaunched without the route");
    assert!(!l.route_at_launch());
    assert!(!l.keychain.holds_a_key());
    assert!(!l.key_file().exists());
    assert_eq!(
        l.h.code(
            "turn_on_online",
            json!({"settings": {"phones": true, "sendSensitive": false,
            "limit": {"kind": "questions", "questions": 200, "usd": null}}})
        ),
        "unavailable",
        "nothing is left to turn on"
    );
}

#[test]
fn turn_on_saves_key_and_ledger() {
    let l = lab();
    let v = l.turn_on(200);
    assert_eq!(v["state"]["type"], "on");
    assert_eq!(v["setUp"], true);
    assert_eq!(v["host"], "api.example.com");
    assert_eq!(v["keyStorage"], "keychain");
    assert!(l.keychain.holds_a_key());
    assert!(!l.key_file().exists());
    assert_eq!(l.port.current(), stand_in_route());
    assert_eq!(
        l.port.routes().len(),
        1,
        "turning on after a check of the same route does not relaunch"
    );
    assert!(l.ledger().contains(&("onlineOnOff".into(), "on".into())));
}

#[test]
fn turn_off_keeps_key_forget_removes_it() {
    let l = lab();
    l.turn_on(200);
    let v = l.h.ok("turn_off_online", json!({"forget": false}));
    assert_eq!(v["state"]["type"], "off");
    assert_eq!(v["setUp"], true);
    assert!(l.keychain.holds_a_key());
    assert_eq!(l.port.current(), None);
    assert!(l.ledger().contains(&("onlineOnOff".into(), "off".into())));
    // On again without a new check: the kept key is used.
    let v = l.h.ok(
        "turn_on_online",
        json!({"settings": {"phones": true, "sendSensitive": false,
                            "limit": {"kind": "questions", "questions": 200, "usd": null}}}),
    );
    assert_eq!(v["state"]["type"], "on");
    assert_eq!(l.port.current(), stand_in_route());
    let v = l.h.ok("turn_off_online", json!({"forget": true}));
    assert_eq!(v["setUp"], false);
    assert!(!l.keychain.holds_a_key());
    assert_eq!(l.port.current(), None);
}

#[test]
fn forged_event_changes_nothing() {
    let l = lab();
    let mut forged = l.view();
    forged["state"] = json!({"type": "on"});
    forged["setUp"] = json!(true);
    forged["baseUrl"] = json!("https://evil.example.com/v1");
    l.h.app.emit(EVENT_ONLINE, &forged).unwrap();
    l.h.webview.emit(EVENT_ONLINE, &forged).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert_eq!(l.view()["state"]["type"], "off");
    assert!(!l.route_at_launch());
    assert!(l.port.routes().is_empty());
    dentist_ask(&l);
    {
        let r = l.requests();
        assert!(r.is_empty(), "stray requests: {r:#?}");
    }
}

fn dentist_ask(l: &Lab) -> nocturne_contract::AskDelta {
    if l.h.engine().stats().unwrap().memory_count == 0 {
        l.h.learn("note", "The dentist is on Thursday.", "Dentist", "Thursday");
    }
    l.ask("When is the dentist?")
}

#[test]
fn key_unreadable_at_launch_is_a_problem() {
    let l = lab();
    l.h.engine()
        .update_online_state(|s| {
            s.wanted_on = true;
            s.set_up = true;
            s.base_url = Some(URL.into());
            s.model = Some("stand-in-model".into());
        })
        .unwrap();
    assert!(!l.route_at_launch(), "no key, no route");
    let v = l.view();
    assert_eq!(v["state"]["type"], "problem");
    assert_eq!(v["state"]["code"], "keyUnavailable");
    let d = dentist_ask(&l);
    assert_eq!(
        serde_json::to_value(&d.answered_by).unwrap(),
        json!({"type": "computer", "reason": "problem"})
    );
    {
        let r = l.requests();
        assert!(r.is_empty(), "stray requests: {r:#?}");
    }
}

#[test]
fn limit_pauses_and_lifts_on_the_first() {
    let l = lab();
    l.turn_on(2);
    for _ in 0..2 {
        let d = dentist_ask(&l);
        assert_eq!(
            serde_json::to_value(&d.answered_by).unwrap()["type"],
            "online"
        );
    }
    let v = l.view();
    assert_eq!(v["state"]["type"], "paused");
    assert_eq!(v["state"]["pause"]["type"], "limit");
    assert_eq!(v["questionsThisMonth"], 2);
    assert!(l.notices().contains(&"onlineLimit".to_string()));
    let d = dentist_ask(&l);
    assert_eq!(
        serde_json::to_value(&d.answered_by).unwrap(),
        json!({"type": "computer", "reason": "limit"})
    );
    // The 1st of next month, local time.
    l.h.clock.set_str("2026-12-01T00:00:01Z");
    let v = l.view();
    assert_eq!(v["state"]["type"], "on");
    assert_eq!(v["questionsThisMonth"], 0);
    assert_eq!(v["resetsOn"], "2027-01-01");
    let d = dentist_ask(&l);
    assert_eq!(
        serde_json::to_value(&d.answered_by).unwrap()["type"],
        "online"
    );
}

#[test]
fn a_higher_limit_lifts_the_pause() {
    let l = lab();
    l.turn_on(1);
    dentist_ask(&l);
    assert_eq!(l.view()["state"]["type"], "paused");
    let v = l.h.ok(
        "update_online_settings",
        json!({"patch": {"limit": {"kind": "questions", "questions": 5, "usd": null}}}),
    );
    assert_eq!(v["state"]["type"], "on");
}

fn pauses_until_check(status: u16, pause: &str, notice: &str) {
    let l = lab();
    l.turn_on(200);
    l.script(json!({"status": status}));
    dentist_ask(&l);
    let v = l.view();
    assert_eq!(v["state"]["type"], "paused", "{status}");
    assert_eq!(v["state"]["pause"]["type"], pause);
    assert!(l.notices().contains(&notice.to_string()));
    let before = l.requests().len();
    let d = dentist_ask(&l);
    assert_eq!(
        serde_json::to_value(&d.answered_by).unwrap()["reason"],
        pause,
        "paused: answered here without trying"
    );
    assert_eq!(l.requests().len(), before);
    // "Check again" with the key kept: it works now, and the pause lifts.
    l.script(json!({"status": null}));
    let out = l.check("");
    assert_eq!(out["type"], "works");
    assert_eq!(l.view()["state"]["type"], "on");
    assert!(!l.notices().contains(&notice.to_string()));
    let d = dentist_ask(&l);
    assert_eq!(
        serde_json::to_value(&d.answered_by).unwrap()["type"],
        "online"
    );
}

#[test]
fn key_refused_pauses_until_check() {
    pauses_until_check(401, "keyRefused", "onlineKeyRefused");
}

#[test]
fn no_credit_pauses_until_check() {
    pauses_until_check(402, "noCredit", "onlineNoCredit");
}

#[test]
fn a_failed_check_while_on_keeps_the_route_in_use() {
    let l = lab();
    l.turn_on(200);
    l.script(json!({"status": 401}));
    let out = l.h.ok(
        "check_online",
        json!({"form": {"baseUrl": "https://other.example.com/v1", "key": "sk-other-key-000",
                        "model": "m2"}}),
    );
    assert_eq!(out["type"], "keyRefused");
    assert_eq!(
        l.port.current(),
        stand_in_route(),
        "the route in use is back"
    );
    assert_eq!(l.view()["host"], "api.example.com");
}

#[test]
fn delete_everything_forgets_key_log_route() {
    let l = lab_with(MemoryKeychain::default());
    l.turn_on(200);
    dentist_ask(&l);
    l.h.ok(
        "delete_everything",
        json!({"confirmPhrase": nocturne_engine::DELETE_PHRASE}),
    );
    assert!(!l.keychain.holds_a_key());
    assert!(!l.key_file().exists());
    assert!(!l.h.dir.path().join("online").exists());
    assert_eq!(l.port.current(), None, "relaunched without the route");
    assert!(!l.route_at_launch());
    let v = l.view();
    assert_eq!(v["state"]["type"], "off");
    assert_eq!(v["setUp"], false);
    let sends =
        l.h.ok("list_online_sends", json!({"limit": 10, "before": null}));
    assert!(sends.as_array().unwrap().is_empty());
}

#[test]
fn delete_everything_removes_the_key_file_too() {
    let l = lab_with(MemoryKeychain::unavailable());
    l.turn_on(200);
    assert!(l.key_file().exists());
    l.h.ok(
        "delete_everything",
        json!({"confirmPhrase": nocturne_engine::DELETE_PHRASE}),
    );
    assert!(!l.key_file().exists());
}

#[test]
fn privacy_view_counts_online() {
    let l = lab();
    l.turn_on(200);
    dentist_ask(&l);
    let p = l.h.ok("get_privacy", json!({}));
    assert_eq!(p["onlineOn"], true);
    assert_eq!(p["onlineQuestions"], 1);
    assert_eq!(p["onlineHost"], "api.example.com");
    assert!(p["sentOnlineBytes"].as_f64().unwrap() > 0.0);
    assert!(p["onlineLastOnAt"].is_string());
}

#[test]
fn advanced_view_names_route_without_key() {
    let l = lab();
    l.turn_on(200);
    dentist_ask(&l);
    let a = l.h.ok("get_advanced", json!({}));
    let online = &a["online"];
    assert_eq!(online["state"], "on");
    assert!(online["route"].as_str().unwrap().contains(URL));
    assert_eq!(online["maxTokens"], 600);
    assert_eq!(online["keyStorage"], "keychain");
    assert_eq!(online["lastStatus"], 200);
    assert!(!a.to_string().contains(&KEY[..20]));
}

#[test]
fn no_request_at_idle() {
    let l = lab();
    l.turn_on(200);
    let bearer = format!("Bearer {}", l.port.engine_key());
    let mine = || {
        l.requests()
            .iter()
            .filter(|r| r["headers"]["authorization"] == bearer.as_str())
            .count()
    };
    let after_check = mine();
    assert_eq!(after_check, 1, "the check");
    nocturne_app_lib::online::install(l.h.app.handle()).unwrap();
    for _ in 0..5 {
        l.view();
        l.h.ok("get_privacy", json!({}));
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    assert_eq!(
        mine(),
        after_check,
        "no polling, no balance, nothing at idle"
    );
}

#[test]
fn the_sent_log_shows_what_was_sent() {
    let l = lab();
    l.turn_on(200);
    dentist_ask(&l);
    let sends =
        l.h.ok("list_online_sends", json!({"limit": 1, "before": null}));
    let s = &sends[0];
    assert_eq!(s["kind"], "question");
    assert_eq!(s["question"], "When is the dentist?");
    assert!(
        s["instructions"]
            .as_str()
            .unwrap()
            .contains("You answer a person's question")
    );
    let lines = s["lines"].as_array().unwrap();
    let memory = lines.iter().find(|x| x["type"] == "memory").unwrap();
    assert_eq!(memory["state"], "same");
    assert!(
        memory["text"]
            .as_str()
            .unwrap()
            .contains("The dentist is on Thursday.")
    );
    assert_eq!(s["result"]["type"], "answered");
}

/// A blank key means the key kept, and only for the address it was kept for.
#[test]
fn a_blank_key_is_never_sent_to_a_changed_address() {
    let l = lab();
    l.turn_on(200);
    let before = l.requests().len();
    let routes = l.port.routes().len();
    for url in [
        "https://other.example.com/v1",
        "https://api.example.com:8443/v1",
        "https://api.example.com/v2",
    ] {
        let out = l.h.ok(
            "check_online",
            json!({"form": {"baseUrl": url, "key": "", "model": "stand-in-model"}}),
        );
        assert_eq!(out["type"], "keyRefused", "{url}: enter the key again");
    }
    assert_eq!(
        l.port.routes().len(),
        routes,
        "the engine was given no new route"
    );
    assert_eq!(l.requests().len(), before, "nothing was sent");
    // The same address, written another way, is the same address.
    let out = l.h.ok(
        "check_online",
        json!({"form": {"baseUrl": "HTTPS://API.example.com./v1", "key": "",
                        "model": "stand-in-model"}}),
    );
    assert_eq!(out["type"], "works");
}

#[test]
fn a_blank_key_before_setup_is_refused() {
    let l = lab();
    let out = l.h.ok(
        "check_online",
        json!({"form": {"baseUrl": URL, "key": "  ", "model": "stand-in-model"}}),
    );
    assert_eq!(out["type"], "keyRefused");
    assert!(l.port.routes().is_empty());
    {
        let r = l.requests();
        assert!(r.is_empty(), "stray requests: {r:#?}");
    }
}

/// A server on this computer is a provider over plain HTTP in every build, with no warning:
/// the check works and the route the engine is given allows plain HTTP.
#[test]
fn a_server_on_this_computer_works_over_plain_http() {
    let l = lab();
    let url = format!("http://localhost:{}/v1", l.s.port);
    let out = l.h.ok(
        "check_online",
        json!({"form": {"baseUrl": url, "key": KEY, "model": "stand-in-model"}}),
    );
    assert_eq!(out["type"], "works", "{out}");
    assert_eq!(out["host"], format!("localhost:{}", l.s.port));
    l.h.ok(
        "turn_on_online",
        json!({"settings": {"phones": true, "sendSensitive": false,
                            "limit": {"kind": "questions", "questions": 200, "usd": null}}}),
    );
    let route = nocturne_app_lib::online::route_at_launch(l.h.app.handle()).unwrap();
    assert_eq!(route.base_url().as_str(), url);
    assert!(route.allow_insecure_http());
}

/// A name on the person's own network is judged by what it resolves to at the check: private
/// addresses take plain HTTP, a public one does not, and one that is never a provider is
/// refused. Nothing is sent for a refused name.
#[test]
fn an_own_name_is_judged_by_what_it_resolves_to() {
    let l = lab();
    let hub = nocturne_app_lib::online::hub(l.h.app.handle()).unwrap();
    let to = |addr: &'static str| -> nocturne_lemonade::online::Lookup {
        Arc::new(move |_: String, _: u16| Box::pin(async move { Ok(vec![addr.parse().unwrap()]) }))
    };
    let check = |url: &str| -> String {
        l.h.ok(
            "check_online",
            json!({"form": {"baseUrl": url, "key": KEY, "model": "stand-in-model"}}),
        )["type"]
            .as_str()
            .unwrap()
            .to_string()
    };
    hub.set_lookup(to("8.8.8.8"));
    assert_eq!(check("http://gpubox:8000/v1"), "addressNotAllowed");
    hub.set_lookup(to("169.254.169.254"));
    assert_eq!(check("http://gpubox:8000/v1"), "addressNotAllowed");
    assert_eq!(check("https://gpubox.local/v1"), "addressNotAllowed");
    assert!(
        l.port.routes().is_empty(),
        "nothing was sent for a refused name"
    );
    hub.set_lookup(to("192.168.1.10"));
    assert_eq!(check("http://gpubox:8000/v1"), "works");
}

/// Two stand-ins on loopback: the key kept for the first never reaches the second.
#[test]
fn a_blank_key_never_reaches_a_second_stand_in() {
    let l = lab();
    let first = format!("http://127.0.0.1:{}/v1", l.s.port);
    let out = l.h.ok(
        "check_online",
        json!({"form": {"baseUrl": first, "key": KEY, "model": "stand-in-model"}}),
    );
    assert_eq!(out["type"], "works");
    l.h.ok(
        "turn_on_online",
        json!({"settings": {"phones": true, "sendSensitive": false,
                            "limit": {"kind": "questions", "questions": 200, "usd": null}}}),
    );
    let other = StandIn::start(&fake_provider());
    let second = format!("http://127.0.0.1:{}/v1", other.port);
    let out = l.h.ok(
        "check_online",
        json!({"form": {"baseUrl": second, "key": "", "model": "stand-in-model"}}),
    );
    assert_eq!(out["type"], "keyRefused");
    let log = |other: &StandIn| -> Vec<Value> {
        block_on(async {
            reqwest::Client::new()
                .get(other.url("/fake/requests"))
                .send()
                .await
                .unwrap()
                .json()
                .await
                .unwrap()
        })
    };
    // The whole log, whatever key a request carried: a leak of the first key must show here.
    let all = log(&other);
    assert!(
        !serde_json::to_string(&all).unwrap().contains(&KEY[..20]),
        "{all:?}"
    );
    let seen = l.port.own(all);
    assert!(seen.is_empty(), "{seen:?}");
    // With a key typed for it, the second address gets that key and only that one.
    let out = l.h.ok(
        "check_online",
        json!({"form": {"baseUrl": second, "key": "sk-second-key-000111222",
                        "model": "stand-in-model"}}),
    );
    assert_eq!(out["type"], "works");
    let all = log(&other);
    assert!(
        !serde_json::to_string(&all).unwrap().contains(&KEY[..20]),
        "{all:?}"
    );
    let seen = l.port.own_with(&["sk-second-key-000111222"], all);
    assert_eq!(seen.len(), 1, "{seen:?}");
}

fn settings() -> Value {
    json!({"settings": {"phones": true, "sendSensitive": false,
                        "limit": {"kind": "questions", "questions": 200, "usd": null}}})
}

/// While on, "Change…" runs a check and the person leaves: only the check goes; the route in
/// use and the key kept stay, and questions keep going where they went.
#[test]
fn leaving_a_change_while_on_keeps_the_live_route_and_key() {
    let l = lab();
    l.turn_on(200);
    let out = l.h.ok(
        "check_online",
        json!({"form": {"baseUrl": "https://new.example.com/v1", "key": "sk-new-key-0000111122",
                        "model": "other-model"}}),
    );
    assert_eq!(out["type"], "works");
    assert_eq!(
        l.port.current(),
        stand_in_route(),
        "a check while on never changes what questions use"
    );
    assert_eq!(l.view()["host"], "api.example.com");
    l.h.ok("abandon_online_setup", json!({}));
    assert_eq!(l.port.current(), stand_in_route());
    assert!(l.keychain.holds_a_key());
    let v = l.view();
    assert_eq!(v["state"]["type"], "on");
    assert_eq!(v["host"], "api.example.com");
    assert_eq!(v["model"], "stand-in-model");
    let d = dentist_ask(&l);
    assert_eq!(
        serde_json::to_value(&d.answered_by).unwrap(),
        json!({"type": "online", "host": "api.example.com"})
    );
    // Turning on now keeps the route agreed before, not the abandoned one.
    l.h.ok("turn_on_online", settings());
    assert_eq!(l.view()["host"], "api.example.com");
}

/// A change agreed to while on takes effect only when turned on after its check passed.
#[test]
fn turn_on_takes_only_the_route_that_passed_its_check() {
    let l = lab();
    l.turn_on(200);
    l.h.ok(
        "check_online",
        json!({"form": {"baseUrl": "https://new.example.com/v1", "key": "sk-new-key-0000111122",
                        "model": "other-model"}}),
    );
    let v = l.h.ok("turn_on_online", settings());
    assert_eq!(v["host"], "new.example.com");
    assert_eq!(v["model"], "other-model");
    assert_eq!(
        l.port.current(),
        Some(RouteSeen {
            base_url: "https://new.example.com/v1".into(),
            model: "other-model".into(),
            with_key: true,
        })
    );
}

#[test]
fn turn_on_refuses_a_route_whose_check_failed() {
    let l = lab();
    l.script(json!({"status": 401}));
    assert_eq!(l.check(KEY)["type"], "keyRefused");
    assert_eq!(
        l.port.current(),
        None,
        "a failed check hands the engine back"
    );
    assert_eq!(l.h.code("turn_on_online", settings()), "unavailable");
    assert_eq!(l.view()["state"]["type"], "off");
    assert!(!l.keychain.holds_a_key());
    // While on: a failed check of another address leaves nothing to turn on but the route in use.
    l.script(json!({"status": null}));
    l.turn_on(200);
    l.script(json!({"status": 401}));
    l.h.ok(
        "check_online",
        json!({"form": {"baseUrl": "https://new.example.com/v1", "key": "sk-new-key-0000111122",
                        "model": "other-model"}}),
    );
    let v = l.h.ok("turn_on_online", settings());
    assert_eq!(v["host"], "api.example.com");
    assert_eq!(l.port.current(), stand_in_route());
}

/// A relaunch that failed leaves the engine's route unknown: leaving, turning off and deleting
/// everything each still reach the engine, so no later launch has the key.
#[test]
fn a_failed_relaunch_never_strands_the_key() {
    for step in ["leave", "off", "delete"] {
        let l = lab();
        l.port.fail.store(true, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(l.check(KEY)["type"], "unreachable", "{step}");
        l.port
            .fail
            .store(false, std::sync::atomic::Ordering::SeqCst);
        let before = l.port.calls();
        match step {
            "leave" => {
                l.h.ok("abandon_online_setup", json!({}));
            }
            "off" => {
                l.h.ok("turn_off_online", json!({"forget": false}));
            }
            _ => {
                l.h.ok(
                    "delete_everything",
                    json!({"confirmPhrase": nocturne_engine::DELETE_PHRASE}),
                );
            }
        }
        assert!(l.port.calls() > before, "{step}: the engine was told");
        assert_eq!(l.port.current(), None, "{step}");
        assert!(!l.route_at_launch(), "{step}");
    }
}

#[test]
fn a_key_that_could_not_be_removed_is_reported() {
    let l = lab();
    l.turn_on(200);
    l.keychain
        .unavailable
        .store(true, std::sync::atomic::Ordering::SeqCst);
    assert_eq!(
        l.h.code("turn_off_online", json!({"forget": true})),
        "unavailable",
        "the person is told"
    );
    let v = l.view();
    assert_eq!(v["state"]["type"], "off");
    assert_eq!(
        v["keyStorage"], "keychain",
        "the view says a key may still be kept"
    );
    assert_eq!(l.port.current(), None);
    l.h.ok(
        "delete_everything",
        json!({"confirmPhrase": nocturne_engine::DELETE_PHRASE}),
    );
    assert_eq!(l.view()["keyStorage"], "keychain");
}

/// A keychain that is not available at launch is asked again later, not given up on.
#[test]
fn a_key_read_that_failed_is_tried_again() {
    let l = lab_with(MemoryKeychain::unavailable());
    l.h.engine()
        .update_online_state(|s| {
            s.wanted_on = true;
            s.set_up = true;
            s.base_url = Some(URL.into());
            s.model = Some("stand-in-model".into());
            s.key_storage = Some(nocturne_contract::KeyStorage::Keychain);
        })
        .unwrap();
    let v = l.view();
    assert_eq!(v["state"]["code"], "keyUnavailable");
    use nocturne_app_lib::online::keys::Keychain;
    l.keychain
        .unavailable
        .store(false, std::sync::atomic::Ordering::SeqCst);
    l.keychain.set(KEY).unwrap();
    let v = l.view();
    assert_eq!(v["state"]["type"], "on", "{v}");
    let d = dentist_ask(&l);
    assert_eq!(
        serde_json::to_value(&d.answered_by).unwrap()["type"],
        "online"
    );
}

#[test]
fn turning_off_stops_a_check_under_way() {
    let l = lab();
    l.turn_on(200);
    l.script(json!({"delay_ms": 8000}));
    let hub = nocturne_app_lib::online::hub(l.h.app.handle()).unwrap();
    let form: nocturne_contract::OnlineForm = serde_json::from_value(
        json!({"baseUrl": "https://new.example.com/v1", "key": "sk-new-0001112223", "model": "m"}),
    )
    .unwrap();
    let began = std::time::Instant::now();
    let checking = tauri::async_runtime::spawn(async move { hub.check(form).await });
    wait_for("the check to begin", || {
        l.view()["state"]["type"] == "checking"
    });
    l.h.ok("turn_off_online", json!({"forget": false}));
    let out = block_on(checking).unwrap().unwrap();
    assert_eq!(out, nocturne_contract::CheckOutcome::Cancelled);
    assert!(began.elapsed() < std::time::Duration::from_secs(7));
    assert_eq!(l.port.current(), None);
    assert_eq!(l.view()["state"]["type"], "off");
}

#[test]
fn a_money_limit_without_reported_cost_falls_back_to_the_count() {
    let l = lab();
    l.turn_on(200);
    l.h.ok(
        "update_online_settings",
        json!({"patch": {"limit": {"kind": "usd", "questions": 1, "usd": 5.0}}}),
    );
    let d = dentist_ask(&l);
    assert_eq!(
        serde_json::to_value(&d.answered_by).unwrap()["type"],
        "online"
    );
    let v = l.view();
    assert_eq!(v["costReported"], false);
    assert_eq!(
        v["state"]["type"], "paused",
        "limiting never silently stops"
    );
    let d = dentist_ask(&l);
    assert_eq!(
        serde_json::to_value(&d.answered_by).unwrap(),
        json!({"type": "computer", "reason": "limit"})
    );
}

/// Only the route in use is launched with: never one being set up, never one whose check
/// failed.
#[test]
fn only_the_route_in_use_is_launched_with() {
    let l = lab();
    let at_launch = || {
        nocturne_app_lib::online::route_at_launch(l.h.app.handle())
            .map(|r| (r.base_url().to_string(), r.model.clone()))
    };
    assert_eq!(l.check(KEY)["type"], "works");
    assert_eq!(at_launch(), None, "a route that passed its check");
    l.script(json!({"status": 401}));
    assert_eq!(l.check(KEY)["type"], "keyRefused");
    assert_eq!(at_launch(), None, "a route whose check failed");
    l.script(json!({"status": null}));
    l.turn_on(200);
    let in_use = Some((URL.to_string(), "stand-in-model".to_string()));
    assert_eq!(at_launch(), in_use);
    let out = l.h.ok(
        "check_online",
        json!({"form": {"baseUrl": "https://new.example.com/v1", "key": "sk-new-key-0000111122",
                        "model": "other-model"}}),
    );
    assert_eq!(out["type"], "works");
    assert_eq!(at_launch(), in_use, "a change not yet agreed to");
    l.script(json!({"status": 401}));
    l.h.ok(
        "check_online",
        json!({"form": {"baseUrl": "https://new.example.com/v1", "key": "sk-new-key-0000111122",
                        "model": "other-model"}}),
    );
    assert_eq!(at_launch(), in_use, "a change whose check failed");
}

/// A check cancelled while the engine relaunches with the route being checked: the engine has
/// taken that route already, so it is always handed back, to the route in use while on and to
/// none while off. Questions go only where the mark says.
#[test]
fn a_check_cancelled_mid_relaunch_hands_the_engine_back() {
    for on in [false, true] {
        let l = lab();
        if on {
            l.turn_on(200);
        }
        assert_eq!(l.route_at_launch(), on, "on={on}");
        let hub = nocturne_app_lib::online::hub(l.h.app.handle()).unwrap();
        let hold = l.port.hold_next_relaunch();
        let form: nocturne_contract::OnlineForm = serde_json::from_value(json!({
            "baseUrl": "https://new.example.com/v1", "key": "sk-new-0001112223", "model": "m"
        }))
        .unwrap();
        let checking = {
            let hub = hub.clone();
            tauri::async_runtime::spawn(async move { hub.check(form).await })
        };
        block_on(hold.reached.notified());
        assert_eq!(
            l.port.current().map(|r| r.base_url),
            Some("https://new.example.com/v1".to_string()),
            "on={on}: the engine took the route being checked"
        );
        l.h.ok("cancel_online_check", json!({}));
        let out = block_on(checking).unwrap().unwrap();
        assert_eq!(out, nocturne_contract::CheckOutcome::Cancelled);
        if on {
            assert_eq!(
                l.port.current(),
                stand_in_route(),
                "back to the route in use"
            );
            assert!(hub.still_on());
            let d = dentist_ask(&l);
            assert_eq!(
                serde_json::to_value(&d.answered_by).unwrap(),
                json!({"type": "online", "host": "api.example.com"})
            );
            assert_eq!(l.port.current(), stand_in_route());
        } else {
            assert_eq!(l.port.current(), None, "no route and no key while off");
            assert!(!l.route_at_launch());
            assert_eq!(l.view()["state"]["type"], "off");
        }
    }
}

/// Turning on with a route just checked is asked about in the app's own window, with its
/// full address: a no turns nothing on, and that route is then refused without asking again
/// until the person leaves setup. A changed key or path is asked about again.
#[test]
fn turning_on_is_confirmed_outside_the_web_view() {
    let l = lab();
    let hub = nocturne_app_lib::online::hub(l.h.app.handle()).unwrap();
    let asked = Arc::new(std::sync::Mutex::new(Vec::new()));
    hub.set_confirm(confirm_with(asked.clone(), false));
    assert_eq!(l.check(KEY)["type"], "works");
    assert_eq!(l.h.code("turn_on_online", settings()), "declined");
    assert_eq!(*asked.lock().unwrap(), [URL]);
    assert_eq!(l.view()["state"]["type"], "off");
    assert!(!l.keychain.holds_a_key(), "nothing kept after a no");

    hub.set_confirm(agree(asked.clone()));
    assert_eq!(l.h.code("turn_on_online", settings()), "declined");
    assert_eq!(l.check(KEY)["type"], "works");
    assert_eq!(l.h.code("turn_on_online", settings()), "declined");
    assert_eq!(
        asked.lock().unwrap().len(),
        1,
        "a refused route is not asked about again"
    );

    l.h.ok("abandon_online_setup", json!({}));
    assert_eq!(l.check(KEY)["type"], "works");
    assert_eq!(l.h.ok("turn_on_online", settings())["state"]["type"], "on");
    assert_eq!(*asked.lock().unwrap(), [URL, URL]);

    l.h.ok("turn_off_online", json!({"forget": false}));
    assert_eq!(l.h.ok("turn_on_online", settings())["state"]["type"], "on");
    assert_eq!(
        asked.lock().unwrap().len(),
        2,
        "the route agreed to, from where it is kept"
    );

    let other_key = json!({"form": {"baseUrl": URL, "key": "sk-other-key-000111222333",
                                    "model": "stand-in-model"}});
    assert_eq!(l.h.ok("check_online", other_key)["type"], "works");
    l.h.ok("turn_on_online", settings());
    assert_eq!(asked.lock().unwrap().len(), 3, "a new key is asked about");

    let other_path = "https://api.example.com/other/v1";
    let form = json!({"form": {"baseUrl": other_path, "key": KEY, "model": "stand-in-model"}});
    assert_eq!(l.h.ok("check_online", form)["type"], "works");
    l.h.ok("turn_on_online", settings());
    assert_eq!(asked.lock().unwrap().last().unwrap(), other_path);
}

/// One question in the app's window at a time: turning on again while it is open is refused.
#[test]
fn only_one_confirmation_at_a_time() {
    let l = lab();
    let hub = nocturne_app_lib::online::hub(l.h.app.handle()).unwrap();
    let reached = Arc::new(tokio::sync::Notify::new());
    let go = Arc::new(tokio::sync::Notify::new());
    let (r, g) = (reached.clone(), go.clone());
    hub.set_confirm(Arc::new(move |_: &str| {
        let (r, g) = (r.clone(), g.clone());
        Box::pin(async move {
            r.notify_one();
            g.notified().await;
            true
        })
    }));
    assert_eq!(l.check(KEY)["type"], "works");
    let first = {
        let hub = hub.clone();
        let settings: nocturne_contract::OnlineSettings =
            serde_json::from_value(settings()["settings"].clone()).unwrap();
        tauri::async_runtime::spawn(async move { hub.turn_on(settings).await })
    };
    block_on(reached.notified());
    assert_eq!(l.h.code("turn_on_online", settings()), "unavailable");
    go.notify_one();
    let view = block_on(first).unwrap().unwrap();
    assert_eq!(view.state, nocturne_contract::OnlineState::On);
}

/// Leaving setup while the confirmation is open, then agreeing: the route shown is gone, and
/// the route kept before is not turned on in its place.
#[test]
fn agreeing_after_leaving_setup_turns_nothing_on() {
    let l = lab();
    let hub = nocturne_app_lib::online::hub(l.h.app.handle()).unwrap();
    assert_eq!(l.check(KEY)["type"], "works");
    assert_eq!(l.h.ok("turn_on_online", settings())["state"]["type"], "on");
    l.h.ok("turn_off_online", json!({"forget": false}));
    assert!(l.keychain.holds_a_key());

    let reached = Arc::new(tokio::sync::Notify::new());
    let go = Arc::new(tokio::sync::Notify::new());
    let (r, g) = (reached.clone(), go.clone());
    hub.set_confirm(Arc::new(move |_: &str| {
        let (r, g) = (r.clone(), g.clone());
        Box::pin(async move {
            r.notify_one();
            g.notified().await;
            true
        })
    }));
    let other_key = json!({"form": {"baseUrl": URL, "key": "sk-other-key-000111222333",
                                    "model": "stand-in-model"}});
    assert_eq!(l.h.ok("check_online", other_key)["type"], "works");
    let turning = {
        let hub = hub.clone();
        let settings: nocturne_contract::OnlineSettings =
            serde_json::from_value(settings()["settings"].clone()).unwrap();
        tauri::async_runtime::spawn(async move { hub.turn_on(settings).await })
    };
    block_on(reached.notified());
    l.h.ok("abandon_online_setup", json!({}));
    go.notify_one();
    let err = block_on(turning).unwrap().unwrap_err();
    let code = format!("{:?}", err.code);
    assert!(code == "Declined" || code == "Unavailable", "{code}");
    assert_eq!(l.view()["state"]["type"], "off");
    assert_eq!(l.port.current(), None, "no route given to the engine");
}

/// With no way to ask in the app's own window, nothing is turned on for a new host.
#[test]
fn without_a_way_to_ask_nothing_is_turned_on() {
    let h = harness_with_mind(Arc::new(FakeMind::new()));
    let s = StandIn::start(&fake_provider());
    let port = Arc::new(StandInPort::new(Some(s.port)));
    nocturne_app_lib::online::manage_hub(
        h.app.handle(),
        port.clone(),
        Arc::new(MemoryKeychain::default()),
    )
    .unwrap();
    let out = h.ok(
        "check_online",
        json!({"form": {"baseUrl": URL, "key": KEY, "model": "stand-in-model"}}),
    );
    assert_eq!(out["type"], "works");
    assert_eq!(h.code("turn_on_online", settings()), "declined");
    assert_eq!(h.ok("get_online", json!({}))["state"]["type"], "off");
}
