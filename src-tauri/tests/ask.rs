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

fn two_memories(mode: AnswerMode) -> (Harness, Vec<String>) {
    let h = harness_with_mind(TestMind::new(mode));
    let (_, a) = h.learn("swim note", "Leo swims on Mondays.", "Leo", "swimming");
    let (_, b) = h.learn(
        "coach note",
        "Coach Dee runs the swim.",
        "Coach Dee",
        "swim",
    );
    (h, vec![a, b])
}

fn done_delta(h: &Harness, question: &str) -> (String, nocturne_contract::AskDelta) {
    let seen = h.ask_deltas();
    let id = ask(h, question);
    wait_for("the answer", || finished(&seen, &id));
    let got = deltas_for(&seen, &id);
    let text = got.iter().filter_map(|d| d.text_delta.clone()).collect();
    (text, got.last().unwrap().clone())
}

/// The nth bold span stands for the nth cited memory, so the memories go out in the order the
/// answer cites them, not the order they were found in.
#[test]
fn bold_spans_match_the_memories_in_the_order_they_are_cited() {
    let (h, mems) = two_memories(AnswerMode::Bold(2));
    let (text, done) = done_delta(&h, "When does Leo swim with Coach Dee?");
    assert_eq!(done.phase, AskPhase::Done);
    assert!(!done.plain, "{text}");
    let mut by_place = mems.clone();
    by_place.sort_by_key(|m| text.find(&h.memory(m).sentence).unwrap());
    assert_eq!(done.memory_ids, by_place);
}

/// One bold span for two memories cannot be matched up: the answer is shown without bold, and
/// its receipts stay grouped by memory.
#[test]
fn bold_spans_that_do_not_match_the_cited_memories_are_dropped() {
    let (h, _) = two_memories(AnswerMode::Bold(1));
    let (text, done) = done_delta(&h, "When does Leo swim with Coach Dee?");
    assert_eq!(done.phase, AskPhase::Done);
    assert_eq!(text.matches("**").count(), 2);
    assert!(done.plain);
    assert_eq!(done.memory_ids.len(), 2);
    assert_eq!(done.receipts.len(), 2);
}

/// As many bold spans as memories is not enough: when the spans do not go with the memories in
/// order (both stand for the first), the bold is dropped.
#[test]
fn bold_spans_that_stand_for_other_memories_than_their_place_are_dropped() {
    let (h, _) = two_memories(AnswerMode::BoldBothForTheFirst);
    let (text, done) = done_delta(&h, "When does Leo swim with Coach Dee?");
    assert_eq!(done.phase, AskPhase::Done);
    assert_eq!(text.matches("**").count(), 4);
    assert_eq!(done.memory_ids.len(), 2);
    assert!(done.plain);
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

#[test]
fn a_mind_that_panics_still_ends_the_ask_with_a_terminal_phase() {
    let (h, _) = dentist_world(TestMind::new(AnswerMode::Panics));
    let seen = h.ask_deltas();
    let id = ask(&h, "dentist");
    wait_for("the answer", || finished(&seen, &id));
    assert_eq!(phases(&seen, &id).last(), Some(&AskPhase::Unavailable));
    wait_for("the ask to be forgotten", || {
        h.state().asks.in_flight() == 0
    });
    // The app carries on.
    let again = ask(&h, "dentist");
    wait_for("another answer", || finished(&seen, &again));
}

#[test]
fn a_busy_mind_is_not_waited_for_once_words_have_gone_out() {
    let (h, _) = dentist_world(TestMind::new(AnswerMode::BusyAfterStreaming));
    let seen = h.ask_deltas();
    let id = ask(&h, "dentist");
    wait_for("the answer", || finished(&seen, &id));
    let order = phases(&seen, &id);
    assert!(!order.contains(&AskPhase::Waking), "{order:?}");
    assert_eq!(order.last(), Some(&AskPhase::Unavailable));
    let words: Vec<_> = deltas_for(&seen, &id)
        .iter()
        .filter_map(|d| d.text_delta.clone())
        .collect();
    assert_eq!(words, vec!["Half an ".to_string()], "not started over");
}

#[test]
fn a_third_ask_cancels_the_oldest() {
    let (h, _) = dentist_world(TestMind::new(AnswerMode::Slow(Duration::from_millis(400))));
    let seen = h.ask_deltas();
    let a = ask(&h, "dentist");
    let b = ask(&h, "dentist");
    let c = ask(&h, "dentist");
    assert_eq!(h.state().asks.in_flight(), 2);
    wait_for("the two newest", || {
        finished(&seen, &b) && finished(&seen, &c)
    });
    std::thread::sleep(Duration::from_millis(200));
    assert!(
        !finished(&seen, &a),
        "the oldest was cancelled: {:?}",
        phases(&seen, &a)
    );
}

/// Forget racing an ask: once the forget has returned, no word of the forgotten memory reaches
/// the page, and the answer that was reading it is stopped.
#[test]
fn forgetting_while_an_answer_is_reading_stops_it() {
    let (h, mem) = dentist_world(TestMind::new(AnswerMode::Slow(Duration::from_millis(300))));
    let seen = h.ask_deltas();
    let id = ask(&h, "dentist");
    wait_for("the answer to be reading", || {
        phases(&seen, &id).contains(&AskPhase::Reading)
    });
    let preview = h.ok(
        "preview_forget",
        json!({"ids": [mem], "includeOriginals": true}),
    );
    h.ok("forget", json!({"previewToken": preview["token"]}));
    let at_forget = deltas_for(&seen, &id).len();
    std::thread::sleep(Duration::from_millis(600));
    let after = deltas_for(&seen, &id);
    assert_eq!(after.len(), at_forget, "nothing followed the forget");
    assert!(
        after.iter().all(|d| d.text_delta.is_none()),
        "no words went out"
    );
    assert!(!finished(&seen, &id));
    assert_eq!(h.state().asks.in_flight(), 0);
    // And a new question finds nothing of it.
    let again = ask(&h, "dentist");
    wait_for("the new answer", || finished(&seen, &again));
    assert_eq!(phases(&seen, &again).last(), Some(&AskPhase::Nothing));
}

#[test]
fn deleting_everything_stops_the_answers_in_flight() {
    let (h, _) = dentist_world(TestMind::new(AnswerMode::Slow(Duration::from_millis(300))));
    let seen = h.ask_deltas();
    let id = ask(&h, "dentist");
    wait_for("reading", || {
        phases(&seen, &id).contains(&AskPhase::Reading)
    });
    h.ok(
        "delete_everything",
        json!({"confirmPhrase": "delete everything"}),
    );
    std::thread::sleep(Duration::from_millis(500));
    assert!(!finished(&seen, &id));
    assert_eq!(h.state().asks.in_flight(), 0);
}

/// An open which-is-right question goes to the mind whole: the memory on the other side comes
/// along even when the search did not find it, each marked as disagreeing with the other, and
/// both can be cited with their receipts.
#[test]
fn both_sides_of_an_open_question_reach_the_mind() {
    let mind = TestMind::new(AnswerMode::Normal);
    let h = harness_with_mind(mind.clone());
    let club = |object: &str, sentence: &str| {
        nocturne_engine::Candidate::new(
            "Book club",
            nocturne_engine::Predicate::Scheduled,
            object,
            sentence,
            0.9,
        )
    };
    let (_, a) = h.learn_candidate(
        "club note",
        club("Tuesday", "Book club meets at the library on Tuesday."),
    );
    let (_, b) = h.learn_candidate(
        "Sam's note",
        club("Wednesday", "Sam says it is on Wednesday instead."),
    );
    let (a, b) = (a[0].clone(), b[0].clone());
    let (found, unrelated) = (h.engine().retrieve("library", None, 12).unwrap(), &b);
    assert!(
        found.iter().all(|hit| &hit.memory.id != unrelated),
        "the search alone does not find the other side"
    );

    let (_, done) = done_delta(&h, "Is it at the library?");
    assert_eq!(done.phase, AskPhase::Done);
    let given = mind.answered_from.lock().unwrap().last().cloned().unwrap();
    let side = |id: &str| given.iter().find(|f| f.memory.id == id).cloned().unwrap();
    assert_eq!(side(&a).disputed_by, vec![b.clone()]);
    assert_eq!(side(&b).disputed_by, vec![a.clone()]);
    assert!(done.memory_ids.contains(&a) && done.memory_ids.contains(&b));
    assert_eq!(done.receipts.len(), 2);
}
