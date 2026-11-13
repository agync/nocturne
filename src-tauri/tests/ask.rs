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

fn club(object: &str, sentence: &str) -> nocturne_engine::Candidate {
    nocturne_engine::Candidate::new(
        "Book club",
        nocturne_engine::Predicate::Scheduled,
        object,
        sentence,
        0.9,
    )
}

/// Book club on Tuesday at the library, and Sam's Wednesday, which a search for the library
/// does not find. Returns the two memory ids.
fn club_world(h: &Harness) -> (String, String) {
    let (_, a) = h.learn_candidate(
        "club note",
        club("Tuesday", "Book club meets at the library on Tuesday."),
    );
    let (_, b) = h.learn_candidate(
        "Sam's note",
        club("Wednesday", "Sam says it is on Wednesday instead."),
    );
    let (a, b) = (a[0].clone(), b[0].clone());
    let found = h.engine().retrieve("library", None, 12).unwrap();
    assert!(
        found.iter().all(|hit| hit.memory.id != b),
        "the search alone does not find the other side"
    );
    (a, b)
}

/// An open which-is-right question goes to the mind whole: the memory on the other side comes
/// along even when the search did not find it, each marked as disagreeing with the other, and
/// both can be cited with their receipts.
#[test]
fn both_sides_of_an_open_question_reach_the_mind() {
    let mind = TestMind::new(AnswerMode::Normal);
    let h = harness_with_mind(mind.clone());
    let (a, b) = club_world(&h);

    let (_, done) = done_delta(&h, "Is it at the library?");
    assert_eq!(done.phase, AskPhase::Done);
    let given = mind.answered_from.lock().unwrap().last().cloned().unwrap();
    let side = |id: &str| given.iter().find(|f| f.memory.id == id).cloned().unwrap();
    assert_eq!(side(&a).disputed_by, vec![b.clone()]);
    assert_eq!(side(&b).disputed_by, vec![a.clone()]);
    assert!(done.memory_ids.contains(&a) && done.memory_ids.contains(&b));
    assert_eq!(done.receipts.len(), 2);
}

/// A side that has been let go, or is being forgotten, does not come back as a live fact, and
/// the question is still answered from what is left.
#[test]
fn a_side_let_go_or_being_forgotten_stays_out() {
    for gone in ["let go", "forgetting"] {
        let mind = TestMind::new(AnswerMode::Normal);
        let h = harness_with_mind(mind.clone());
        let (a, b) = club_world(&h);
        let e = h.engine();
        if gone == "let go" {
            h.clock.advance_days(100);
            e.mark_used(std::slice::from_ref(&a), nocturne_engine::UseKind::Answer)
                .unwrap();
            e.decay().unwrap();
            assert_eq!(h.memory(&b).status, nocturne_contract::MemoryStatus::LetGo);
        } else {
            let preview = e.preview_forget(std::slice::from_ref(&b), false).unwrap();
            e.forget_begin_only(&preview.token).unwrap();
        }

        let (_, done) = done_delta(&h, "Is it at the library?");
        assert_eq!(done.phase, AskPhase::Done, "{gone}");
        let given = mind.answered_from.lock().unwrap().last().cloned().unwrap();
        assert_eq!(given.len(), 1, "{gone}");
        assert_eq!(given[0].memory.id, a);
        assert!(given[0].disputed_by.is_empty(), "{gone}");
    }
}

/// "Not sure" offers near matches from what the search found, never a side brought in.
#[test]
fn near_matches_come_from_the_search_only() {
    let h = harness_with_mind(TestMind::new(AnswerMode::NotSure));
    let (a, _) = club_world(&h);
    let seen = h.ask_deltas();
    let id = ask(&h, "Is it at the library?");
    wait_for("the answer", || finished(&seen, &id));
    let last = deltas_for(&seen, &id).last().cloned().unwrap();
    assert_eq!(last.phase, AskPhase::NotSure);
    assert_eq!(last.near.len(), 1);
    assert_eq!(last.near[0].text, h.memory(&a).sentence);
}

/// However many open questions the found memories are in, at most four other sides come in,
/// and the mind never gets more than twelve facts.
#[test]
fn sides_brought_in_are_capped() {
    let mind = TestMind::new(AnswerMode::Normal);
    let h = harness_with_mind(mind.clone());
    for n in 0..14 {
        let subject = format!("Club {n}");
        let c = |object: &str, sentence: String| {
            nocturne_engine::Candidate::new(
                &subject,
                nocturne_engine::Predicate::Scheduled,
                object,
                &sentence,
                0.9,
            )
        };
        h.learn_candidate(
            &format!("note {n}"),
            c(
                "Tuesday",
                format!("Club {n} meets at the library on Tuesday."),
            ),
        );
        h.learn_candidate(
            &format!("other note {n}"),
            c(
                "Wednesday",
                format!("Someone says club {n} is on Wednesday."),
            ),
        );
    }
    let (_, done) = done_delta(&h, "At the library?");
    assert_eq!(done.phase, AskPhase::Done);
    let given = mind.answered_from.lock().unwrap().last().cloned().unwrap();
    assert_eq!(given.len(), 12);
    let sides = given
        .iter()
        .filter(|f| f.memory.sentence.starts_with("Someone"))
        .count();
    assert_eq!(sides, 4);
    let ids: Vec<&String> = given.iter().map(|f| &f.memory.id).collect();
    for f in &given {
        assert!(
            f.disputed_by.iter().all(|other| ids.contains(&other)),
            "every side named is among the facts"
        );
    }
    assert_eq!(
        given.iter().filter(|f| !f.disputed_by.is_empty()).count(),
        8,
        "four found memories and their four sides"
    );
}

/// Start an ask the way a phone does: its own lane, its own channel, public memories only.
fn phone_ask(
    h: &Harness,
    device: &str,
    question: &str,
) -> (
    String,
    tokio::sync::mpsc::UnboundedReceiver<nocturne_contract::AskDelta>,
) {
    use nocturne_app_lib::ask::{AskLane, AskOptions, AskSink, start_with};
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let handle = h.app.handle().clone();
    let opts = AskOptions {
        public_only: true,
        lane: AskLane::Phone(device.into()),
        sink: AskSink::Channel(tx),
        place: nocturne_contract::AskPlace::Auto,
    };
    let question = question.to_string();
    let id = tauri::async_runtime::block_on(async move { start_with(&handle, question, opts) });
    (id, rx)
}

/// Drain a phone ask's channel until its terminal phase.
fn phone_deltas(
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<nocturne_contract::AskDelta>,
) -> Vec<nocturne_contract::AskDelta> {
    let mut got: Vec<nocturne_contract::AskDelta> = Vec::new();
    wait_for("the phone's answer", || {
        while let Ok(d) = rx.try_recv() {
            got.push(d);
        }
        got.last().is_some_and(|d| is_terminal(d.phase))
    });
    got
}

/// A phone's ask never reaches a private memory: not in what the mind is given (found or
/// brought in as the other side of a question), not in the answer, not in near matches. Its
/// deltas go to its own channel, never out as events.
#[test]
fn public_only_ask_never_sees_private() {
    for mode in [AnswerMode::Normal, AnswerMode::NotSure] {
        let mind = TestMind::new(mode);
        let h = harness_with_mind(mind.clone());
        let (a, b) = club_world(&h);
        let (_, secret) = h.learn(
            "secret note",
            "Book club at the library is really about Zebrafish47.",
            "Book club",
            "Zebrafish47",
        );
        let e = h.engine();
        e.set_private(&secret, true).unwrap();
        e.set_private(&b, true).unwrap();
        let events = h.ask_deltas();

        let (id, mut rx) = phone_ask(&h, "dev1", "Is book club at the library?");
        let got = phone_deltas(&mut rx);
        assert!(got.iter().all(|d| d.ask_id == id));
        let given = mind.answered_from.lock().unwrap().last().cloned().unwrap();
        assert!(
            given.iter().all(|f| f.memory.id == a),
            "only the public memory reaches the mind"
        );
        let text = serde_json::to_string(&got).unwrap();
        assert!(!text.contains("Zebrafish47"), "{text}");
        assert!(!text.contains(&secret) && !text.contains(&b), "{text}");
        let last = got.last().unwrap();
        match mode {
            AnswerMode::NotSure => assert_eq!(last.near.len(), 1),
            _ => assert_eq!(last.memory_ids, vec![a.clone()]),
        }
        assert!(deltas_for(&events, &id).is_empty(), "never an event");

        // The same question from the window still sees everything.
        let (_, done) = done_delta(&h, "Is book club at the library?");
        if matches!(mode, AnswerMode::Normal) {
            assert!(done.memory_ids.contains(&secret), "{:?}", done.memory_ids);
        }
    }
}

/// Each lane has its own room: a phone's asks evict only that phone's older ask, and the
/// windows' asks evict only the windows' oldest.
#[test]
fn phone_lane_never_evicts_desktop() {
    use nocturne_app_lib::ask::AskLane;
    let (h, _) = dentist_world(TestMind::new(AnswerMode::Slow(Duration::from_millis(400))));
    let seen = h.ask_deltas();
    let asks = &h.state().asks;
    let a = ask(&h, "dentist");
    let b = ask(&h, "dentist");
    let (_, mut first) = phone_ask(&h, "dev1", "dentist");
    let (_, mut second) = phone_ask(&h, "dev1", "dentist");
    let (_, mut other) = phone_ask(&h, "dev2", "dentist");
    assert_eq!(
        asks.in_flight_in(&AskLane::Desktop),
        2,
        "the windows keep both"
    );
    assert_eq!(asks.in_flight_in(&AskLane::Phone("dev1".into())), 1);
    assert_eq!(asks.in_flight_in(&AskLane::Phone("dev2".into())), 1);

    let c = ask(&h, "dentist");
    assert_eq!(asks.in_flight_in(&AskLane::Desktop), 2);
    assert_eq!(asks.in_flight(), 4, "a window's ask left the phones alone");

    wait_for("the windows' newest", || {
        finished(&seen, &b) && finished(&seen, &c)
    });
    assert_eq!(
        phone_deltas(&mut second).last().unwrap().phase,
        AskPhase::Done
    );
    assert_eq!(
        phone_deltas(&mut other).last().unwrap().phase,
        AskPhase::Done
    );
    std::thread::sleep(Duration::from_millis(200));
    assert!(!finished(&seen, &a), "the windows' oldest was cancelled");
    let mut evicted = Vec::new();
    while let Ok(d) = first.try_recv() {
        evicted.push(d.phase);
    }
    assert!(
        !evicted.iter().any(|p| is_terminal(*p)),
        "the phone's older ask was cancelled: {evicted:?}"
    );
}

/// A phone stops only its own asks: another phone's and the windows' go on.
#[test]
fn a_phone_cancels_only_its_own_asks() {
    use nocturne_app_lib::ask::AskLane;
    let (h, _) = dentist_world(TestMind::new(AnswerMode::Slow(Duration::from_millis(300))));
    let seen = h.ask_deltas();
    let asks = &h.state().asks;
    let desktop = ask(&h, "dentist");
    let (mine, mut mine_rx) = phone_ask(&h, "dev1", "dentist");
    let (theirs, mut theirs_rx) = phone_ask(&h, "dev2", "dentist");
    let lane = AskLane::Phone("dev1".into());
    assert!(!asks.cancel_in(&lane, &desktop), "not the windows' ask");
    assert!(!asks.cancel_in(&lane, &theirs), "not another phone's ask");
    assert!(asks.cancel_in(&lane, &mine));
    assert!(!asks.cancel_in(&lane, &mine), "cancelling twice is fine");

    wait_for("the windows' answer", || finished(&seen, &desktop));
    assert_eq!(
        phone_deltas(&mut theirs_rx).last().unwrap().phase,
        AskPhase::Done
    );
    std::thread::sleep(Duration::from_millis(200));
    let mut stopped = Vec::new();
    while let Ok(d) = mine_rx.try_recv() {
        stopped.push(d.phase);
    }
    assert!(!stopped.iter().any(|p| is_terminal(*p)), "{stopped:?}");
}

/// Removing a phone stops its answers and leaves everyone else's.
#[test]
fn removing_a_phone_stops_its_answers() {
    use nocturne_app_lib::ask::AskLane;
    let (h, _) = dentist_world(TestMind::new(AnswerMode::Slow(Duration::from_millis(300))));
    let seen = h.ask_deltas();
    let asks = &h.state().asks;
    let desktop = ask(&h, "dentist");
    let (_, mut removed_rx) = phone_ask(&h, "dev1", "dentist");
    let (_, mut other_rx) = phone_ask(&h, "dev2", "dentist");
    let lane = AskLane::Phone("dev1".into());
    asks.cancel_lane(&lane);
    assert_eq!(asks.in_flight_in(&lane), 0);
    assert_eq!(asks.in_flight(), 2);

    wait_for("the windows' answer", || finished(&seen, &desktop));
    assert_eq!(
        phone_deltas(&mut other_rx).last().unwrap().phase,
        AskPhase::Done
    );
    std::thread::sleep(Duration::from_millis(200));
    let mut stopped = Vec::new();
    while let Ok(d) = removed_rx.try_recv() {
        stopped.push(d.phase);
    }
    assert!(!stopped.iter().any(|p| is_terminal(*p)), "{stopped:?}");
}
