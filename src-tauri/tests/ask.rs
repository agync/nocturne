//! Streamed answers over the mock runtime, with scripted minds.

mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use nocturne_contract::AskPhase;
use nocturne_engine::FakeMind;
use serde_json::json;

fn ask(h: &Harness, question: &str) -> String {
    h.ok("ask", json!({"question": question}))
        .as_str()
        .unwrap()
        .to_string()
}

fn phases(all: &std::sync::Mutex<Vec<nocturne_contract::AskDelta>>, id: &str) -> Vec<AskPhase> {
    deltas_for(all, id).iter().map(|d| d.phase).collect()
}

fn finished(all: &std::sync::Mutex<Vec<nocturne_contract::AskDelta>>, id: &str) -> bool {
    deltas_for(all, id).iter().any(|d| is_terminal(d.phase))
}

fn dentist_world(mind: Arc<dyn nocturne_engine::Mind>) -> (Harness, String) {
    let h = harness_with_mind(mind);
    let (_, mem) = h.learn(
        "dentist note",
        "The dentist is on Thursday.",
        "Dentist",
        "Thursday",
    );
    (h, mem)
}

#[test]
fn deltas_arrive_in_phase_order_and_end_in_one_terminal_phase() {
    let (h, mem) = dentist_world(Arc::new(FakeMind::new()));
    assert!(h.memory(&mem).last_used.is_none());
    let seen = h.ask_deltas();
    let id = ask(&h, "When is the dentist?");
    wait_for("the answer", || finished(&seen, &id));
    let got = deltas_for(&seen, &id);
    let order: Vec<_> = got.iter().map(|d| d.phase).collect();
    assert_eq!(
        order,
        vec![
            AskPhase::Searching,
            AskPhase::Reading,
            AskPhase::Streaming,
            AskPhase::Done
        ]
    );
    assert_eq!(got[1].found, Some(1));
    assert_eq!(
        got[2].text_delta.as_deref(),
        Some("The dentist is on Thursday.")
    );
    let done = got.last().unwrap();
    assert_eq!(done.memory_ids, vec![mem.clone()]);
    assert_eq!(done.receipts.len(), 1, "every answer carries its receipt");
    assert_eq!(
        got.iter().filter(|d| is_terminal(d.phase)).count(),
        1,
        "exactly one terminal phase"
    );
    // Being cited counts as use.
    assert!(h.memory(&mem).last_used.is_some());
    wait_for("the ask to be forgotten", || {
        h.state().asks.in_flight() == 0
    });
}

/// Review focus 2: two asks in flight keep their deltas apart.
#[test]
fn concurrent_asks_do_not_mix_their_deltas() {
    let (h, _) = dentist_world(TestMind::new(AnswerMode::Slow(Duration::from_millis(150))));
    h.learn("soup note", "Soup night is Friday.", "Soup", "Friday");
    let seen = h.ask_deltas();
    let a = ask(&h, "dentist");
    let b = ask(&h, "soup");
    assert_ne!(a, b);
    wait_for("both answers", || {
        finished(&seen, &a) && finished(&seen, &b)
    });
    let text = |id: &str| {
        deltas_for(&seen, id)
            .iter()
            .filter_map(|d| d.text_delta.clone())
            .collect::<String>()
    };
    assert_eq!(text(&a), "The dentist is on Thursday.");
    assert_eq!(text(&b), "Soup night is Friday.");
    for id in [&a, &b] {
        assert_eq!(phases(&seen, id).last(), Some(&AskPhase::Done));
        assert!(deltas_for(&seen, id).iter().all(|d| d.ask_id == *id));
    }
}

/// Review focus 2: cancelling one ask stops that task only, and no delta follows the cancel.
#[test]
fn cancelling_one_ask_stops_only_that_one() {
    let (h, _) = dentist_world(TestMind::new(AnswerMode::Slow(Duration::from_millis(300))));
    h.learn("soup note", "Soup night is Friday.", "Soup", "Friday");
    let seen = h.ask_deltas();
    let a = ask(&h, "dentist");
    let b = ask(&h, "soup");
    wait_for("both to be reading", || {
        phases(&seen, &a).contains(&AskPhase::Reading)
            && phases(&seen, &b).contains(&AskPhase::Reading)
    });
    h.ok("cancel_ask", json!({"askId": a}));
    let frozen = deltas_for(&seen, &a).len();
    wait_for("the other ask", || finished(&seen, &b));
    std::thread::sleep(Duration::from_millis(400));
    assert_eq!(
        deltas_for(&seen, &a).len(),
        frozen,
        "no delta after the cancel"
    );
    assert!(!finished(&seen, &a), "a cancelled ask does not finish");
    assert_eq!(phases(&seen, &b).last(), Some(&AskPhase::Done));
    // Cancelling again, or something unknown, is fine.
    h.ok("cancel_ask", json!({"askId": a}));
    h.ok("cancel_ask", json!({"askId": "ask_unknown"}));
    assert_eq!(h.state().asks.in_flight(), 0);
}

#[test]
fn an_empty_mind_slot_yields_unavailable() {
    let h = harness();
    h.learn(
        "dentist note",
        "The dentist is on Thursday.",
        "Dentist",
        "Thursday",
    );
    let seen = h.ask_deltas();
    let id = ask(&h, "dentist");
    wait_for("the answer", || finished(&seen, &id));
    assert_eq!(phases(&seen, &id), vec![AskPhase::Unavailable]);
}

#[test]
fn a_mind_that_is_down_yields_unavailable() {
    let (h, _) = dentist_world(Arc::new(FakeMind::new().unavailable()));
    let seen = h.ask_deltas();
    let id = ask(&h, "dentist");
    wait_for("the answer", || finished(&seen, &id));
    assert_eq!(
        phases(&seen, &id),
        vec![AskPhase::Searching, AskPhase::Unavailable]
    );
}

#[test]
fn nothing_found_says_nothing() {
    let (h, mem) = dentist_world(Arc::new(FakeMind::new()));
    let seen = h.ask_deltas();
    let id = ask(&h, "xylophone lessons");
    wait_for("the answer", || finished(&seen, &id));
    assert_eq!(
        phases(&seen, &id),
        vec![AskPhase::Searching, AskPhase::Nothing]
    );
    assert!(h.memory(&mem).last_used.is_none(), "nothing was used");
}

#[test]
fn not_sure_offers_the_two_nearest_and_marks_nothing_used() {
    let (h, mem) = dentist_world(TestMind::new(AnswerMode::NotSure));
    let seen = h.ask_deltas();
    let id = ask(&h, "dentist");
    wait_for("the answer", || finished(&seen, &id));
    let got = deltas_for(&seen, &id);
    let last = got.last().unwrap();
    assert_eq!(last.phase, AskPhase::NotSure);
    assert_eq!(last.near.len(), 1);
    assert_eq!(last.near[0].text, "The dentist is on Thursday.");
    assert!(h.memory(&mem).last_used.is_none());
}

#[test]
fn an_answer_that_cites_nothing_is_not_an_answer() {
    let (h, mem) = dentist_world(TestMind::new(AnswerMode::CitesNothing));
    let seen = h.ask_deltas();
    let id = ask(&h, "dentist");
    wait_for("the answer", || finished(&seen, &id));
    assert_eq!(phases(&seen, &id).last(), Some(&AskPhase::NotSure));
    assert!(h.memory(&mem).last_used.is_none());
}

#[test]
fn a_waking_mind_is_waited_for_and_the_person_is_told() {
    let (h, _) = dentist_world(TestMind::waking(1));
    let seen = h.ask_deltas();
    let id = ask(&h, "dentist");
    wait_for("the answer", || finished(&seen, &id));
    let order = phases(&seen, &id);
    assert_eq!(order[0], AskPhase::Searching);
    assert_eq!(
        order.iter().filter(|p| **p == AskPhase::Waking).count(),
        1,
        "told once: {order:?}"
    );
    assert_eq!(order.last(), Some(&AskPhase::Done));
}

#[test]
fn a_let_go_memory_does_not_answer() {
    let (h, mem) = dentist_world(Arc::new(FakeMind::new()));
    // Far enough in the future that it has been let go.
    h.clock.advance_days(400);
    h.engine().decay().unwrap();
    assert_eq!(
        h.memory(&mem).status,
        nocturne_contract::MemoryStatus::LetGo
    );
    let seen = h.ask_deltas();
    let id = ask(&h, "dentist");
    wait_for("the answer", || finished(&seen, &id));
    assert_eq!(phases(&seen, &id).last(), Some(&AskPhase::Nothing));
}

#[test]
fn an_empty_question_is_refused_at_the_door() {
    let h = harness();
    assert_eq!(h.code("ask", json!({"question": "   "})), "invalid");
    assert_eq!(h.state().asks.in_flight(), 0);
}
