//! Asking with Online answers: where each question goes, what leaves, the fallbacks and their
//! marks. The local mind is a scripted one; the engine route is the stand-in provider, which
//! records every request it gets.

mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use nocturne_app_lib::ask::{self, AskLane, AskOptions, AskSink};
use nocturne_app_lib::online::testing::{MemoryKeychain, StandIn, StandInPort, fake_provider};
use nocturne_contract::{AnsweredBy, AskDelta, AskPhase, AskPlace, HelpKind, LeftOut, LocalReason};
use nocturne_engine::{FakeMind, Mind};
use serde_json::{Value, json};
use tauri::async_runtime::block_on;

const KEY: &str = "sk-ask-test-0123456789abcdefghijklmn";

struct Lab {
    h: Harness,
    s: StandIn,
    port: Arc<StandInPort>,
}

fn lab_with(mind: Arc<dyn Mind>) -> Lab {
    let l = lab_agreed_before(mind);
    // As on the consent step: the person has seen that only what is marked private stays here.
    l.h.ok("see_online_change", json!({}));
    l
}

/// An install whose person turned Online answers on before only what is marked private was held
/// back, and has not seen the notice.
fn lab_agreed_before(mind: Arc<dyn Mind>) -> Lab {
    let h = harness_with_mind(mind);
    let s = StandIn::start(&fake_provider());
    let port = Arc::new(StandInPort::new(Some(s.port)));
    nocturne_app_lib::online::manage_hub(
        h.app.handle(),
        port.clone(),
        Arc::new(MemoryKeychain::default()),
    )
    .unwrap()
    .set_confirm(nocturne_app_lib::online::testing::agree(Default::default()));
    Lab { h, s, port }
}

fn lab() -> Lab {
    lab_with(Arc::new(FakeMind::new()))
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

    fn chats(&self) -> Vec<Value> {
        let all: Vec<Value> = self.port.own(block_on(async {
            reqwest::Client::new()
                .get(self.s.url("/fake/requests"))
                .send()
                .await
                .unwrap()
                .json()
                .await
                .unwrap()
        }));
        all.into_iter()
            .filter(|r| r["path"] == "/v1/chat/completions")
            .collect()
    }

    /// Questions sent, without the check's.
    fn questions(&self) -> Vec<Value> {
        self.chats()
            .into_iter()
            .filter(|c| {
                c["body"]["messages"]
                    .as_array()
                    .is_some_and(|m| m.len() == 2)
            })
            .collect()
    }

    fn turn_on(&self, phones: bool, send_sensitive: bool, questions: u32) {
        let out = self.h.ok(
            "check_online",
            json!({"form": {"baseUrl": "https://api.example.com/v1", "key": KEY,
                            "model": "stand-in-model"}}),
        );
        assert_eq!(out["type"], "works", "{out}");
        let view = self.h.ok(
            "turn_on_online",
            json!({"settings": {"phones": phones, "sendSensitive": send_sensitive,
                                "limit": {"kind": "questions", "questions": questions, "usd": null}}}),
        );
        assert_eq!(view["state"]["type"], "on", "{view}");
    }

    fn ask(&self, question: &str, place: Option<&str>) -> Vec<AskDelta> {
        let seen = self.h.ask_deltas();
        let id = self
            .h
            .ok("ask", json!({"question": question, "place": place}))
            .as_str()
            .unwrap()
            .to_string();
        wait_for("the answer", || {
            deltas_for(&seen, &id).iter().any(|d| is_terminal(d.phase))
        });
        wait_for("the ask to end", || self.h.state().asks.in_flight() == 0);
        deltas_for(&seen, &id)
    }
}

fn dentist(l: &Lab) -> String {
    l.h.learn(
        "NOTEMARK the dentist note",
        "The dentist is on Thursday.",
        "Dentist",
        "Thursday",
    )
    .1
}

/// A memory the store suggests keeping private: a medicine.
fn sensitive(l: &Lab, note: &str, sentence: &str) -> String {
    let c = nocturne_engine::Candidate::new(
        "Dentist",
        nocturne_engine::Predicate::Takes,
        "pills",
        sentence,
        0.9,
    );
    let id = l.h.learn_candidate(note, c).1.remove(0);
    assert!(l.h.memory(&id).suggest_private);
    id
}

fn last(d: &[AskDelta]) -> &AskDelta {
    d.last().unwrap()
}

fn phases(d: &[AskDelta]) -> Vec<AskPhase> {
    d.iter().map(|x| x.phase).collect()
}

fn here(reason: LocalReason) -> Option<AnsweredBy> {
    Some(AnsweredBy::Computer {
        reason: Some(reason),
    })
}

fn online() -> Option<AnsweredBy> {
    Some(AnsweredBy::Online {
        host: "api.example.com".into(),
    })
}

#[test]
fn off_never_builds_an_online_request() {
    let l = lab();
    dentist(&l);
    let d = l.ask("When is the dentist?", None);
    assert_eq!(last(&d).phase, AskPhase::Done);
    assert_eq!(last(&d).answered_by, None, "nothing is said while off");
    assert!(l.chats().is_empty());
    assert!(
        l.port.routes().is_empty(),
        "the engine was never given a route"
    );
}

#[test]
fn an_online_answer_streams_whole_and_cites_what_was_sent() {
    let l = lab();
    let mem = dentist(&l);
    l.turn_on(true, false, 200);
    l.script(json!({"reply": "The dentist is on **Thursday** [m1]."}));
    let d = l.ask("When is the dentist?", None);
    assert_eq!(
        phases(&d),
        [
            AskPhase::Searching,
            AskPhase::Reading,
            AskPhase::AskingOnline,
            AskPhase::Streaming,
            AskPhase::Done
        ]
    );
    let done = last(&d);
    assert_eq!(done.answered_by, online());
    assert_eq!(done.memory_ids, [mem]);
    assert_eq!(done.receipts.len(), 1);
    assert!(done.send_id.is_some());
    assert_eq!(
        d[3].text_delta.as_deref(),
        Some("The dentist is on **Thursday**.")
    );
    let sends =
        l.h.ok("list_online_sends", json!({"limit": 10, "before": null}));
    assert_eq!(sends[0]["id"].as_f64(), done.send_id);
}

/// Help is always written on this computer: with Online answers on, a question the guide answers
/// sends nothing to the provider, adds nothing to Sent online, and says so in its mark. A memory
/// question after it still goes online.
#[test]
fn a_help_answer_is_always_written_on_this_computer() {
    let l = lab();
    dentist(&l);
    l.turn_on(true, false, 200);
    l.h.state().help.set_writes(Some(true));
    let sent = |l: &Lab| {
        l.h.ok("list_online_sends", json!({"limit": 50, "before": null}))
            .as_array()
            .unwrap()
            .len()
    };
    let (asked, logged) = (l.questions().len(), sent(&l));
    for q in [
        "How do I get Nocturne on my phone?",
        "How do I make the dentist note private on my phone?",
    ] {
        let d = l.ask(q, None);
        let end = last(&d);
        assert_eq!(end.phase, AskPhase::Done, "{q}: {end:?}");
        let help = end.help.as_ref().expect("a help answer");
        assert_eq!(help.kind, HelpKind::Written, "{q}");
        assert!(help.online_on, "the mark says help is always written here");
        assert!(end.send_id.is_none());
        assert_ne!(end.answered_by, online());
        assert!(!phases(&d).contains(&AskPhase::AskingOnline));
    }
    assert_eq!(l.questions().len(), asked, "nothing went to the provider");
    assert_eq!(sent(&l), logged, "nothing was added to Sent online");
    l.script(json!({"reply": "The dentist is on **Thursday** [m1]."}));
    let d = l.ask("When is the dentist?", None);
    assert_eq!(last(&d).answered_by, online());
    assert!(
        last(&d)
            .help
            .as_ref()
            .is_none_or(|h| h.kind == HelpKind::Unsure),
        "{:?}",
        last(&d).help
    );
}

/// "Show the guide's answer" asks again on this computer: the question already had its turn
/// online, and is not sent a second time.
#[test]
fn asking_for_the_guides_answer_stays_on_this_computer() {
    let l = lab();
    dentist(&l);
    l.turn_on(true, false, 200);
    let asked = l.questions().len();
    let seen = l.h.ask_deltas();
    let id =
        l.h.ok(
            "ask",
            json!({"question": "When is the dentist?", "place": null, "help": true}),
        )
        .as_str()
        .unwrap()
        .to_string();
    wait_for("the answer", || {
        deltas_for(&seen, &id).iter().any(|d| is_terminal(d.phase))
    });
    wait_for("the ask to end", || l.h.state().asks.in_flight() == 0);
    let d = deltas_for(&seen, &id);
    assert!(last(&d).help.is_some(), "{:?}", last(&d));
    assert!(!phases(&d).contains(&AskPhase::AskingOnline));
    assert_eq!(l.questions().len(), asked, "nothing went to the provider");
}

#[test]
fn this_computer_override_once() {
    let l = lab();
    dentist(&l);
    l.turn_on(true, false, 200);
    let d = l.ask("When is the dentist?", Some("thisComputer"));
    assert_eq!(last(&d).answered_by, here(LocalReason::Chosen));
    assert!(l.questions().is_empty());
    let d = l.ask("When is the dentist?", None);
    assert_eq!(
        last(&d).answered_by,
        online(),
        "the override was for one question"
    );
    assert_eq!(l.questions().len(), 1);
}

fn phone_ask(l: &Lab, question: &str) -> Vec<AskDelta> {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut out = Vec::new();
    block_on(async {
        ask::start_with(
            l.h.app.handle(),
            question.to_string(),
            AskOptions {
                public_only: true,
                lane: AskLane::Phone("phone-1".into()),
                sink: AskSink::Channel(tx),
                place: AskPlace::Auto,
                help: false,
            },
        );
        while let Some(d) = tokio::time::timeout(Duration::from_secs(60), rx.recv())
            .await
            .ok()
            .flatten()
        {
            let end = is_terminal(d.phase);
            out.push(d);
            if end {
                break;
            }
        }
    });
    out
}

#[test]
fn phones_follow_the_setting() {
    let l = lab();
    dentist(&l);
    l.turn_on(true, false, 200);
    let d = phone_ask(&l, "When is the dentist?");
    assert_eq!(last(&d).answered_by, online(), "the phone carries the mark");
    assert_eq!(l.questions().len(), 1);
    l.h.ok(
        "update_online_settings",
        json!({"patch": {"phones": false}}),
    );
    let d = phone_ask(&l, "When is the dentist?");
    assert_eq!(last(&d).phase, AskPhase::Done);
    assert_eq!(last(&d).answered_by, None);
    assert_eq!(l.questions().len(), 1, "a phone question stays here");
}

#[test]
fn private_and_sensitive_never_in_payload() {
    let l = lab();
    let e = l.h.engine();
    let open = dentist(&l);
    let (_, private) = l.h.learn(
        "PRIVNOTE the dentist code",
        "The dentist door code PRIVMARK is 4411.",
        "Dentist",
        "4411",
    );
    e.set_private(&private, true).unwrap();
    let health = sensitive(&l, "health note", "The dentist said HEALTHMARK take pills.");
    l.turn_on(true, false, 200);
    let d = l.ask("What about the dentist?", None);
    let recorded = serde_json::to_string(&l.chats()).unwrap();
    for marker in [
        "PRIVMARK",
        "HEALTHMARK",
        "NOTEMARK",
        "PRIVNOTE",
        &private,
        &health,
        &open,
    ] {
        assert!(!recorded.contains(marker), "{marker} left the computer");
    }
    assert!(recorded.contains("The dentist is on Thursday."));
    assert_eq!(last(&d).left_out, Some(LeftOut::Both));
}

/// A bank detail and a passport number found in what was kept stay here with the switch off,
/// though neither was suggested private when it was kept.
#[test]
fn bank_and_passport_details_stay_here() {
    let l = lab();
    dentist(&l);
    let (_, bank) = l.h.learn(
        "BANKNOTE the dentist bill",
        "The dentist is paid from GB82 WEST 1234 5698 7654 32 BANKMARK.",
        "Dentist",
        "GB82 WEST 1234 5698 7654 32",
    );
    let (_, passport) = l.h.learn(
        "PASSNOTE the dentist form",
        "The dentist form needs passport AB1234567 PASSMARK.",
        "Dentist",
        "AB1234567",
    );
    assert!(!l.h.memory(&bank).suggest_private);
    assert!(!l.h.memory(&passport).suggest_private);
    l.turn_on(true, false, 200);
    let d = l.ask("What about the dentist?", None);
    let recorded = serde_json::to_string(&l.chats()).unwrap();
    for marker in [
        "BANKMARK",
        "PASSMARK",
        "GB82",
        "AB1234567",
        "BANKNOTE",
        "PASSNOTE",
    ] {
        assert!(!recorded.contains(marker), "{marker} left the computer");
    }
    assert!(recorded.contains("The dentist is on Thursday."));
    assert_eq!(last(&d).left_out, Some(LeftOut::Sensitive));
    assert_eq!(last(&d).answered_by, online());
}

#[test]
fn left_out_flag_private_sensitive_both() {
    for (private, sensitive, want) in [
        (true, false, Some(LeftOut::Private)),
        (false, true, Some(LeftOut::Sensitive)),
        (true, true, Some(LeftOut::Both)),
        (false, false, None),
    ] {
        let l = lab();
        dentist(&l);
        let e = l.h.engine();
        if private {
            let (_, p) =
                l.h.learn("p", "The dentist bill is high.", "Dentist", "bill");
            e.set_private(&p, true).unwrap();
        }
        if sensitive {
            self::sensitive(&l, "s", "The dentist gave pills.");
        }
        l.turn_on(true, false, 200);
        let d = l.ask("What about the dentist?", None);
        assert_eq!(last(&d).left_out, want, "{private} {sensitive}");
        assert_eq!(last(&d).answered_by, online());
    }
}

#[test]
fn sensitive_goes_with_the_switch_on() {
    let l = lab();
    sensitive(&l, "s", "The dentist gave SENSMARK pills.");
    l.turn_on(true, true, 200);
    let d = l.ask("What did the dentist give?", None);
    assert_eq!(last(&d).left_out, None);
    assert!(
        serde_json::to_string(&l.questions())
            .unwrap()
            .contains("SENSMARK")
    );
}

#[test]
fn nothing_sendable_answers_here() {
    let l = lab();
    let (_, p) = l.h.learn("p", "The safe code is 9911.", "Safe", "9911");
    l.h.engine().set_private(&p, true).unwrap();
    l.turn_on(true, false, 200);
    let d = l.ask("What is the safe code?", None);
    assert_eq!(last(&d).answered_by, here(LocalReason::KeptHere));
    assert_eq!(
        last(&d).left_out,
        None,
        "said only of an answer written online"
    );
    assert!(!phases(&d).contains(&AskPhase::AskingOnline));
    assert!(l.questions().is_empty());
    let sends =
        l.h.ok("list_online_sends", json!({"limit": 10, "before": null}));
    let sends = sends.as_array().unwrap();
    assert_eq!(
        sends.len(),
        2,
        "the check, and the question that never left"
    );
    assert_eq!(
        sends[0]["result"],
        json!({"type": "notSent", "reason": "keptHere"})
    );
    assert_eq!(sends[0]["reached"], false);
    assert_eq!(sends[0]["bytesUp"], 0.0);
    assert!(sends[0]["lines"].as_array().unwrap().is_empty());
    assert_eq!(
        l.h.ok("get_online", json!({}))["questionsThisMonth"],
        0,
        "not counted"
    );
}

/// One fallback: the stand-in fails as scripted, this computer answers, and the mark says why.
fn falls_back(script: Value, want: LocalReason) {
    let l = lab();
    dentist(&l);
    l.turn_on(true, false, 200);
    l.script(script.clone());
    let d = l.ask("When is the dentist?", None);
    assert_eq!(last(&d).phase, AskPhase::Done, "{script}");
    assert_eq!(last(&d).answered_by, here(want), "{script}");
    assert!(
        phases(&d).contains(&AskPhase::SwitchingToComputer),
        "{script}"
    );
    assert_eq!(l.questions().len(), 1, "one request, no retry: {script}");
}

#[test]
fn every_fallback_row() {
    falls_back(
        json!({"status": 500, "marker": "Network error: CURL error: Couldn't resolve host name"}),
        LocalReason::NoInternet,
    );
    falls_back(
        json!({"status": 500, "marker": "Network error: CURL error: Couldn't connect to server"}),
        LocalReason::CouldntReach,
    );
    falls_back(json!({"status": 500}), LocalReason::CouldntReach);
    falls_back(json!({"status": 503}), LocalReason::CouldntReach);
    falls_back(json!({"status": 429}), LocalReason::CouldntReach);
    falls_back(json!({"status": 401}), LocalReason::KeyRefused);
    falls_back(json!({"status": 402}), LocalReason::NoCredit);
    falls_back(json!({"status": 404}), LocalReason::Problem);
}

#[test]
fn waiting_after_six_seconds_then_the_online_answer() {
    let l = lab();
    dentist(&l);
    l.turn_on(true, false, 200);
    l.script(json!({"delay_ms": 7000}));
    let d = l.ask("When is the dentist?", None);
    let p = phases(&d);
    assert!(p.contains(&AskPhase::WaitingOnline), "{p:?}");
    assert_eq!(last(&d).answered_by, online());
}

#[test]
fn switched_to_this_computer_after_twenty_seconds() {
    let l = lab();
    dentist(&l);
    l.turn_on(true, false, 200);
    l.script(json!({"delay_ms": 25000}));
    let d = l.ask("When is the dentist?", None);
    let p = phases(&d);
    assert!(p.contains(&AskPhase::WaitingOnline), "{p:?}");
    assert!(p.contains(&AskPhase::SwitchingToComputer), "{p:?}");
    assert_eq!(last(&d).answered_by, here(LocalReason::Slow));
    assert_eq!(last(&d).phase, AskPhase::Done);
}

#[test]
fn limit_reached_answers_here() {
    let l = lab();
    dentist(&l);
    l.turn_on(true, false, 1);
    assert_eq!(
        l.ask("When is the dentist?", None)
            .last()
            .unwrap()
            .answered_by,
        online()
    );
    let d = l.ask("When is the dentist?", None);
    assert_eq!(last(&d).answered_by, here(LocalReason::Limit));
    assert_eq!(l.questions().len(), 1);
}

#[test]
fn not_sure_never_resends() {
    let mind = TestMind::new(AnswerMode::Normal);
    let l = lab_with(mind.clone());
    // Nothing is kept back: the note says only what its memory says, so no line of it stays
    // here (a provider not sure without something kept back hands the question to this
    // computer instead; see `answer_quality.rs`).
    l.h.learn(
        "The dentist is on Thursday.",
        "The dentist is on Thursday.",
        "Dentist",
        "Thursday",
    );
    l.turn_on(true, false, 200);
    l.script(json!({"reply": "NOT_SURE"}));
    let d = l.ask("When is the dentist?", None);
    assert_eq!(last(&d).phase, AskPhase::NotSure);
    assert_eq!(last(&d).answered_by, online());
    assert!(!d.iter().any(|x| x.phase == AskPhase::Streaming));
    assert_eq!(l.questions().len(), 1);
    assert!(
        mind.answered_from.lock().unwrap().is_empty(),
        "this computer is offered, not asked"
    );
}

#[test]
fn local_failure_never_goes_online() {
    let mind = TestMind::new(AnswerMode::NotSure);
    let l = lab_with(mind.clone());
    dentist(&l);
    l.turn_on(true, false, 200);
    // Online fails, this computer is not sure: nothing goes online again.
    l.script(json!({"status": 500}));
    let d = l.ask("When is the dentist?", None);
    assert_eq!(last(&d).phase, AskPhase::NotSure);
    assert_eq!(last(&d).answered_by, here(LocalReason::CouldntReach));
    assert_eq!(l.questions().len(), 1);
    // Chosen here and not sure: nothing goes online at all.
    l.script(json!({"status": null}));
    let d = l.ask("When is the dentist?", Some("thisComputer"));
    assert_eq!(last(&d).phase, AskPhase::NotSure);
    assert_eq!(l.questions().len(), 1);
}

#[test]
fn turn_off_mid_answer_switches() {
    let l = lab();
    dentist(&l);
    l.turn_on(true, false, 200);
    l.script(json!({"delay_ms": 15000}));
    let seen = l.h.ask_deltas();
    let id =
        l.h.ok("ask", json!({"question": "When is the dentist?"}))
            .as_str()
            .unwrap()
            .to_string();
    wait_for("the question to go online", || {
        deltas_for(&seen, &id)
            .iter()
            .any(|d| d.phase == AskPhase::AskingOnline)
    });
    let began = std::time::Instant::now();
    l.h.ok("turn_off_online", json!({"forget": false}));
    wait_for("the answer", || {
        deltas_for(&seen, &id).iter().any(|d| is_terminal(d.phase))
    });
    assert!(
        began.elapsed() < Duration::from_secs(12),
        "it did not wait for the provider"
    );
    let d = deltas_for(&seen, &id);
    assert!(phases(&d).contains(&AskPhase::SwitchingToComputer));
    assert_eq!(last(&d).answered_by, here(LocalReason::TurnedOff));
    assert_eq!(last(&d).phase, AskPhase::Done);
    assert_eq!(l.port.current(), None, "the engine has no route");
}

#[test]
fn answered_by_on_every_terminal_delta() {
    let l = lab();
    dentist(&l);
    l.turn_on(true, false, 200);
    // Done online, not sure online, done here, nothing found.
    let d = l.ask("When is the dentist?", None);
    assert!(last(&d).answered_by.is_some());
    l.script(json!({"reply": "NOT_SURE"}));
    assert!(
        l.ask("When is the dentist?", None)
            .last()
            .unwrap()
            .answered_by
            .is_some()
    );
    assert!(
        l.ask("When is the dentist?", Some("thisComputer"))
            .last()
            .unwrap()
            .answered_by
            .is_some()
    );
    let d = l.ask("zzqx wvut", None);
    assert!(is_terminal(last(&d).phase));
    for x in &d {
        if is_terminal(x.phase) {
            assert!(x.answered_by.is_some() || x.phase == AskPhase::Nothing);
        }
    }
}

#[test]
fn at_most_one_request_per_question() {
    let l = lab();
    dentist(&l);
    l.turn_on(true, false, 200);
    for (i, script) in [
        json!({"status": null, "reply": "NOT_SURE"}),
        json!({"reply": "No receipt."}),
        json!({"status": 500}),
        json!({"status": null, "reply": "Thursday [m1]."}),
    ]
    .into_iter()
    .enumerate()
    {
        l.script(script);
        l.ask("When is the dentist?", None);
        assert_eq!(l.questions().len(), i + 1);
    }
}

/// With Online answers on, keeping, reading the note's memories, the page and the privacy and
/// advanced views send nothing; one question sends exactly one request.
#[test]
fn only_ask_reaches_the_route() {
    let l = lab();
    l.turn_on(true, false, 200);
    let before = l.chats().len();
    l.h.ok(
        "keep_text",
        json!({"text": "The bins go out on Tuesday.", "source": "quickCapture"}),
    );
    dentist(&l);
    l.h.save_todays_page();
    l.h.ok("get_privacy", json!({}));
    l.h.ok("get_advanced", json!({}));
    assert_eq!(
        l.chats().len(),
        before,
        "nothing but a question reaches the route"
    );
    l.ask("When is the dentist?", None);
    assert_eq!(l.chats().len(), before + 1);
}

#[test]
fn download_window_answers_here() {
    let l = lab();
    dentist(&l);
    l.turn_on(true, false, 200);
    l.port
        .downloading
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let d = l.ask("When is the dentist?", None);
    assert_eq!(last(&d).answered_by, here(LocalReason::Downloading));
    assert!(l.questions().is_empty());
}

/// An engine that restarted and did not take its route (its key request failed): nothing goes
/// out, the question is answered here, and Online answers shows a problem until a launch takes
/// the route again.
#[test]
fn a_route_the_engine_did_not_take_sends_nothing() {
    let l = lab();
    dentist(&l);
    l.turn_on(true, false, 200);
    let before = l.chats().len();
    l.port.set_route_taken(Some(false));
    let d = l.ask("When is the dentist?", None);
    assert_eq!(last(&d).answered_by, here(LocalReason::Problem));
    assert_eq!(l.chats().len(), before, "nothing reached the provider");
    let view = l.h.ok("get_online", json!({}));
    assert_eq!(view["state"]["type"], "problem", "{view}");
    assert_eq!(view["state"]["code"], "unreachable", "{view}");
    // A later launch took it: the problem lifts and the next question goes online, without
    // another relaunch (which would read the provider's list again).
    l.port.set_route_taken(Some(true));
    let calls = l.port.calls();
    let d = l.ask("When is the dentist?", None);
    assert_eq!(last(&d).answered_by, online());
    assert_eq!(l.chats().len(), before + 1);
    assert_eq!(l.port.calls(), calls, "no relaunch");
}

/// While the engine is between launches, what it holds is not known: the question is answered
/// here, and no problem is raised.
#[test]
fn a_question_while_the_engine_starts_is_answered_here_quietly() {
    let l = lab();
    dentist(&l);
    l.turn_on(true, false, 200);
    let before = l.chats().len();
    l.port
        .starting
        .store(true, std::sync::atomic::Ordering::SeqCst);
    l.port.set_route_taken(Some(false));
    let d = l.ask("When is the dentist?", None);
    assert_eq!(last(&d).answered_by, here(LocalReason::Problem));
    assert_eq!(l.chats().len(), before);
    let view = l.h.ok("get_online", json!({}));
    assert_eq!(view["state"]["type"], "on", "{view}");
    l.port
        .starting
        .store(false, std::sync::atomic::Ordering::SeqCst);
    l.port.set_route_taken(Some(true));
    let d = l.ask("When is the dentist?", None);
    assert_eq!(last(&d).answered_by, online());
}

/// The engine unloaded the online model (a local model failed to load): checking again with the
/// same connection relaunches the engine instead of trusting what it was once given.
#[test]
fn a_check_after_the_engine_lost_its_route_relaunches_it() {
    let l = lab();
    dentist(&l);
    l.turn_on(true, false, 200);
    l.port.set_route_taken(Some(false));
    let calls = l.port.calls();
    let out = l.h.ok(
        "check_online",
        json!({"form": {"baseUrl": "https://api.example.com/v1", "key": KEY,
                        "model": "stand-in-model"}}),
    );
    assert_eq!(out["type"], "works", "{out}");
    assert!(
        l.port.calls() > calls,
        "the engine was given the route again"
    );
    let d = l.ask("When is the dentist?", None);
    assert_eq!(last(&d).answered_by, online());
}

/// Turning on with a route the engine refuses: no question goes out, and "Try again" gives the
/// engine the route again.
#[test]
fn a_refused_key_request_keeps_the_gate_closed() {
    let l = lab();
    dentist(&l);
    l.turn_on(true, false, 200);
    let calls = l.port.calls();
    l.port
        .refuse
        .store(true, std::sync::atomic::Ordering::SeqCst);
    l.h.ok("turn_off_online", json!({"forget": false}));
    let view = l.h.ok(
        "turn_on_online",
        json!({"settings": {"phones": true, "sendSensitive": false,
                            "limit": {"kind": "questions", "questions": 200, "usd": null}}}),
    );
    assert_eq!(view["state"]["type"], "problem", "{view}");
    let before = l.chats().len();
    let d = l.ask("When is the dentist?", None);
    assert_eq!(last(&d).answered_by, here(LocalReason::Problem));
    assert_eq!(l.chats().len(), before, "nothing reached the provider");
    l.port
        .refuse
        .store(false, std::sync::atomic::Ordering::SeqCst);
    let view = l.h.ok(
        "turn_on_online",
        json!({"settings": {"phones": true, "sendSensitive": false,
                            "limit": {"kind": "questions", "questions": 200, "usd": null}}}),
    );
    assert_eq!(view["state"]["type"], "on", "{view}");
    assert!(l.port.calls() > calls + 2, "each try relaunched the engine");
    let d = l.ask("When is the dentist?", None);
    assert_eq!(last(&d).answered_by, online());
}

/// Turned off after the question's route was chosen and before it was sent: nothing goes out,
/// and this computer answers.
#[test]
fn turn_off_between_choice_and_send_sends_nothing() {
    let l = lab();
    dentist(&l);
    l.turn_on(true, false, 200);
    let before = l.chats().len();
    let hub = nocturne_app_lib::online::hub(l.h.app.handle()).unwrap();
    let hold = Arc::new(nocturne_app_lib::online::controller::SendHold::default());
    hub.hold_sends(Some(hold.clone()));
    let seen = l.h.ask_deltas();
    let id =
        l.h.ok("ask", json!({"question": "When is the dentist?"}))
            .as_str()
            .unwrap()
            .to_string();
    block_on(async {
        tokio::time::timeout(Duration::from_secs(60), hold.reached.notified())
            .await
            .expect("the question reached the send");
    });
    l.h.ok("turn_off_online", json!({"forget": false}));
    hold.go.notify_one();
    wait_for("the answer", || {
        deltas_for(&seen, &id).iter().any(|d| is_terminal(d.phase))
    });
    let d = deltas_for(&seen, &id);
    assert!(!phases(&d).contains(&AskPhase::AskingOnline));
    assert_eq!(last(&d).answered_by, here(LocalReason::TurnedOff));
    assert_eq!(last(&d).phase, AskPhase::Done);
    assert_eq!(l.chats().len(), before, "nothing reached the provider");
    hub.hold_sends(None);
}

/// While on with host A, a check of host B begins: a question already on its way, and one
/// asked during the check, never reach B; they are answered here or by A.
#[test]
fn a_check_of_another_provider_never_carries_questions() {
    let l = lab();
    dentist(&l);
    let a = format!("http://127.0.0.1:{}/v1", l.s.port);
    let out = l.h.ok(
        "check_online",
        json!({"form": {"baseUrl": a, "key": KEY, "model": "stand-in-model"}}),
    );
    assert_eq!(out["type"], "works");
    l.h.ok(
        "turn_on_online",
        json!({"settings": {"phones": true, "sendSensitive": false,
                            "limit": {"kind": "questions", "questions": 200, "usd": null}}}),
    );
    let b = StandIn::start(&fake_provider());
    let b_chats = || -> Vec<Value> {
        let all: Vec<Value> = l.port.own(block_on(async {
            reqwest::Client::new()
                .get(b.url("/fake/requests"))
                .send()
                .await
                .unwrap()
                .json()
                .await
                .unwrap()
        }));
        all.into_iter()
            .filter(|r| r["path"] == "/v1/chat/completions")
            .collect()
    };
    // A is slow, B's check is slow: there is a window with a question in flight.
    l.script(json!({"delay_ms": 4000}));
    block_on(async {
        reqwest::Client::new()
            .post(b.url("/fake/script"))
            .json(&json!({"delay_ms": 3000}))
            .send()
            .await
            .unwrap();
    });
    let seen = l.h.ask_deltas();
    let first =
        l.h.ok("ask", json!({"question": "When is the dentist?"}))
            .as_str()
            .unwrap()
            .to_string();
    wait_for("the question to go online", || {
        deltas_for(&seen, &first)
            .iter()
            .any(|d| d.phase == AskPhase::AskingOnline)
    });
    let hub = nocturne_app_lib::online::hub(l.h.app.handle()).unwrap();
    let form: nocturne_contract::OnlineForm = serde_json::from_value(json!({
        "baseUrl": b.url("/v1"), "key": "sk-other-key-000111222", "model": "stand-in-model"
    }))
    .unwrap();
    let checking = tauri::async_runtime::spawn(async move { hub.check(form).await });
    wait_for("the check to begin", || {
        l.h.ok("get_online", json!({}))["state"]["type"] == "checking"
    });
    let second =
        l.h.ok("ask", json!({"question": "When is the dentist?"}))
            .as_str()
            .unwrap()
            .to_string();
    for id in [&first, &second] {
        wait_for("the answer", || {
            deltas_for(&seen, id).iter().any(|d| is_terminal(d.phase))
        });
        let d = deltas_for(&seen, id);
        assert_eq!(last(&d).phase, AskPhase::Done);
        assert!(
            matches!(last(&d).answered_by, Some(AnsweredBy::Computer { .. })),
            "{:?}",
            last(&d).answered_by
        );
    }
    let out = block_on(checking).unwrap().unwrap();
    assert!(matches!(out, nocturne_contract::CheckOutcome::Works { .. }));
    let b_seen = b_chats();
    assert_eq!(b_seen.len(), 1, "only the check reached B");
    assert_eq!(b_seen[0]["body"]["max_tokens"], 8);
    // The route is back to A: the next question goes there.
    l.script(json!({"delay_ms": 0}));
    let d = l.ask("When is the dentist?", None);
    assert_eq!(
        last(&d).answered_by,
        Some(AnsweredBy::Online {
            host: format!("127.0.0.1:{}", l.s.port)
        })
    );
    assert_eq!(b_chats().len(), 1);
}

/// Whether the provider was reached: a status came back from it. The engine's own network
/// failures, and a question cut off, did not reach it.
#[test]
fn the_sent_log_says_whether_the_provider_was_reached() {
    for (script, reached) in [
        (json!({"status": null}), true),
        (json!({"status": 500}), true),
        (json!({"status": 401}), true),
        (
            json!({"status": 500, "marker": "Network error: CURL error: Couldn't resolve host name"}),
            false,
        ),
        (
            json!({"status": 500, "marker": "Network error: CURL error: Couldn't connect to server"}),
            false,
        ),
    ] {
        let l = lab();
        dentist(&l);
        l.turn_on(true, false, 200);
        l.script(script.clone());
        l.ask("When is the dentist?", None);
        let sends =
            l.h.ok("list_online_sends", json!({"limit": 1, "before": null}));
        assert_eq!(sends[0]["kind"], "question");
        assert_eq!(sends[0]["reached"], reached, "{script}");
    }
}

#[test]
fn a_question_cut_off_by_a_turn_off_is_logged_unreached() {
    let l = lab();
    dentist(&l);
    l.turn_on(true, false, 200);
    l.script(json!({"delay_ms": 15000}));
    let seen = l.h.ask_deltas();
    let id =
        l.h.ok("ask", json!({"question": "When is the dentist?"}))
            .as_str()
            .unwrap()
            .to_string();
    wait_for("the question to go online", || {
        deltas_for(&seen, &id)
            .iter()
            .any(|d| d.phase == AskPhase::AskingOnline)
    });
    l.h.ok("turn_off_online", json!({"forget": false}));
    wait_for("the answer", || {
        deltas_for(&seen, &id).iter().any(|d| is_terminal(d.phase))
    });
    let sends =
        l.h.ok("list_online_sends", json!({"limit": 1, "before": null}));
    assert_eq!(
        sends[0]["result"],
        json!({"type": "failed", "reason": "turnedOff"})
    );
    assert_eq!(sends[0]["reached"], false);
}

/// Two questions at once, from the desktop and a phone, with one question left this month:
/// only one goes online.
#[test]
fn questions_at_once_cannot_pass_the_limit_together() {
    let l = lab();
    dentist(&l);
    l.turn_on(true, false, 1);
    l.script(json!({"delay_ms": 1500}));
    let seen = l.h.ask_deltas();
    let desk =
        l.h.ok("ask", json!({"question": "When is the dentist?"}))
            .as_str()
            .unwrap()
            .to_string();
    let phone = phone_ask(&l, "When is the dentist?");
    wait_for("the desktop answer", || {
        deltas_for(&seen, &desk)
            .iter()
            .any(|d| is_terminal(d.phase))
    });
    let marks = [
        last(&deltas_for(&seen, &desk)).answered_by.clone(),
        last(&phone).answered_by.clone(),
    ];
    assert_eq!(
        marks.iter().filter(|m| **m == online()).count(),
        1,
        "{marks:?}"
    );
    assert!(marks.contains(&here(LocalReason::Limit)), "{marks:?}");
    assert_eq!(l.questions().len(), 1);
}

#[test]
fn left_out_is_not_said_of_an_answer_written_here() {
    let l = lab();
    dentist(&l);
    let (_, p) =
        l.h.learn("p", "The dentist bill is high.", "Dentist", "bill");
    l.h.engine().set_private(&p, true).unwrap();
    l.turn_on(true, false, 200);
    l.script(json!({"status": 500}));
    let d = l.ask("What about the dentist?", None);
    assert_eq!(last(&d).answered_by, here(LocalReason::CouldntReach));
    assert_eq!(last(&d).left_out, None);
}

/// The download window opened after the route was chosen: the 404 that follows is a wait,
/// not a missing model.
#[test]
fn a_404_while_a_download_opens_is_not_a_missing_model() {
    let l = lab();
    dentist(&l);
    l.turn_on(true, false, 200);
    let hub = nocturne_app_lib::online::hub(l.h.app.handle()).unwrap();
    let hold = Arc::new(nocturne_app_lib::online::controller::SendHold::default());
    hub.hold_sends(Some(hold.clone()));
    let seen = l.h.ask_deltas();
    let id =
        l.h.ok("ask", json!({"question": "When is the dentist?"}))
            .as_str()
            .unwrap()
            .to_string();
    block_on(async {
        tokio::time::timeout(Duration::from_secs(60), hold.reached.notified())
            .await
            .expect("the question reached the send");
    });
    l.port
        .downloading
        .store(true, std::sync::atomic::Ordering::SeqCst);
    l.script(json!({"status": 404}));
    hold.go.notify_one();
    wait_for("the answer", || {
        deltas_for(&seen, &id).iter().any(|d| is_terminal(d.phase))
    });
    hub.hold_sends(None);
    let d = deltas_for(&seen, &id);
    assert_eq!(last(&d).answered_by, here(LocalReason::Downloading));
    l.port
        .downloading
        .store(false, std::sync::atomic::Ordering::SeqCst);
    let v = l.h.ok("get_online", json!({}));
    assert_eq!(v["state"]["type"], "on", "no lasting problem: {v}");
}

/// Someone who turned Online answers on before only what is marked private was held back sees
/// the notice first: until then nothing goes online, from the window or a phone.
#[test]
fn an_install_that_agreed_before_sends_nothing_until_the_notice_is_seen() {
    let l = lab_agreed_before(Arc::new(FakeMind::new()));
    dentist(&l);
    assert_eq!(
        l.h.ok("get_online", json!({}))["changeNotice"],
        false,
        "off: nothing to tell"
    );
    l.turn_on(true, false, 200);
    assert_eq!(l.h.ok("get_online", json!({}))["changeNotice"], true);
    let d = l.ask("When is the dentist?", None);
    assert_eq!(last(&d).answered_by, here(LocalReason::Changed));
    assert!(!phases(&d).contains(&AskPhase::AskingOnline));
    phone_ask(&l, "When is the dentist?");
    assert!(l.questions().is_empty(), "nothing went online");

    let view = l.h.ok("see_online_change", json!({}));
    assert_eq!(view["changeNotice"], false);
    let d = l.ask("When is the dentist?", None);
    assert_eq!(last(&d).answered_by, online());
    assert_eq!(l.questions().len(), 1);
}

/// A setup through the consent step has seen the new words: no notice, and questions go.
#[test]
fn a_new_setup_has_no_notice() {
    let l = lab();
    dentist(&l);
    l.turn_on(true, false, 200);
    assert_eq!(l.h.ok("get_online", json!({}))["changeNotice"], false);
    let d = l.ask("When is the dentist?", None);
    assert_eq!(last(&d).answered_by, online());
}
