mod common;

use common::*;
use nocturne_contract::{
    Edition, LineKind, MemoryStatus, NeedsYouEntry, PageVariant, PageView, QuestionChoice,
    QuestionKind, Source,
};
use nocturne_engine::{Candidate, HEAVY_ITEMS, PAGE_LINE_CAP, Predicate};

fn compose(f: &Fixture, day: &str) -> PageView {
    f.engine.compose_page(date(day), Edition::Screen).unwrap()
}

fn save_day(f: &Fixture, day: &str) -> PageView {
    let page = compose(f, day);
    f.engine.save_page(&page).unwrap();
    page
}

fn lines(p: &PageView) -> Vec<&nocturne_contract::PageLine> {
    p.needs_you
        .iter()
        .chain(&p.changed)
        .chain(&p.learned)
        .chain(&p.upcoming)
        .chain(&p.let_go)
        .collect()
}

fn due(subject: &str, on: &str) -> Candidate {
    let mut c = cand(
        subject,
        Predicate::Due,
        on,
        &format!("{subject} is due {on}."),
    );
    c.due = Some(date(on));
    c
}

// ---- windows through the engine -------------------------------------------------------------

#[test]
fn a_capture_at_0130_on_the_night_clocks_go_back_lands_in_the_right_page() {
    // Review focus 2, through the engine. London fall-back night, 2026-10-25.
    let f = fixture_at("2026-10-24T20:00:00Z", "Europe/London");
    save_day(&f, "2026-10-24");
    let mut ids = Vec::new();
    for (when, text) in [
        ("2026-10-25T00:30:00Z", "first 01:30"),
        ("2026-10-25T01:30:00Z", "second 01:30"),
    ] {
        f.clock.set_str(when);
        let id = keep(&f.engine, text);
        f.engine.apply(&id, vec![]).unwrap();
        ids.push(id);
    }
    f.clock.set_str("2026-10-25T07:00:00Z");
    let p25 = compose(&f, "2026-10-25");
    assert_eq!(
        p25.kept_count, 2,
        "both 01:30s belong to the 25 October page"
    );
    f.engine.save_page(&p25).unwrap();
    f.clock.set_str("2026-10-26T07:00:00Z");
    assert_eq!(compose(&f, "2026-10-26").kept_count, 0);
    assert_eq!(compose(&f, "2026-10-26").variant, PageVariant::Quiet);
}

#[test]
fn a_time_zone_change_between_pages_loses_and_duplicates_nothing() {
    let f = fixture_at("2026-11-10T06:00:00Z", "Europe/London");
    let a = keep(&f.engine, "in London");
    f.engine.apply(&a, vec![]).unwrap();
    f.clock.set_str("2026-11-10T08:00:00Z");
    let p10 = save_day(&f, "2026-11-10");
    assert_eq!(p10.kept_count, 1);
    // Fly to Tokyo and keep things before and after its 07:00 page time (22:00Z the day before).
    f.clock
        .set_tz(jiff::tz::TimeZone::get("Asia/Tokyo").unwrap());
    let mut counted = 0;
    for (when, text) in [
        ("2026-11-10T21:00:00Z", "tokyo evening"),
        ("2026-11-10T23:00:00Z", "tokyo morning"),
    ] {
        f.clock.set_str(when);
        let id = keep(&f.engine, text);
        f.engine.apply(&id, vec![]).unwrap();
    }
    f.clock.set_str("2026-11-11T23:00:00Z");
    let p11 = save_day(&f, "2026-11-11");
    counted += p11.kept_count;
    let p12 = compose(&f, "2026-11-12");
    counted += p12.kept_count;
    assert_eq!(counted, 2, "each item lands in exactly one page");
    assert_eq!(
        p11.kept_count, 1,
        "the Tokyo window starts where the London one ended and ends at 07:00 in Tokyo"
    );
}

#[test]
fn three_days_off_is_one_catch_up_page_covering_all_three() {
    let f = fixture_at("2026-11-09T06:00:00Z", "Europe/London");
    save_day(&f, "2026-11-09");
    for (d, text) in [
        ("2026-11-10", "monday"),
        ("2026-11-11", "tuesday"),
        ("2026-11-12", "wednesday"),
    ] {
        f.clock.set_str(&format!("{d}T12:00:00Z"));
        let id = keep(&f.engine, text);
        f.engine.apply(&id, vec![]).unwrap();
    }
    f.clock.set_str("2026-11-13T07:00:00Z");
    let p = compose(&f, "2026-11-13");
    assert_eq!(p.variant, PageVariant::CatchUp);
    assert_eq!(p.catch_up_days, 3);
    assert_eq!(p.kept_count, 3);
}

// ---- ranking, caps and variants ---------------------------------------------------------------

#[test]
fn needs_you_is_ranked_due_soonest_then_promises_then_which_then_unconfirmed() {
    let f = fixture();
    learn(&f.engine, "form", due("Trip form", "2026-11-16"));
    learn(&f.engine, "rent", due("Rent", "2026-11-14"));
    learn(
        &f.engine,
        "promise",
        cand(
            "Call Gran",
            Predicate::Promised,
            "Gran",
            "You promised to call Gran.",
        ),
    );
    learn(
        &f.engine,
        "d1",
        cand(
            "Dentist appointment",
            Predicate::Scheduled,
            "Tuesday",
            "Dentist is Tuesday.",
        ),
    );
    let item = keep(&f.engine, "d2");
    f.engine
        .apply(
            &item,
            vec![cand(
                "Dentist appointment",
                Predicate::Scheduled,
                "Thursday",
                "Dentist is Thursday.",
            )],
        )
        .unwrap();
    let web = keep_as(&f.engine, "web", Source::PastedWeb);
    f.engine
        .apply(
            &web,
            vec![cand(
                "Clinic",
                Predicate::Knows,
                "opens at 9",
                "The clinic opens at 9.",
            )],
        )
        .unwrap();

    let kinds: Vec<_> = f
        .engine
        .needs_you()
        .unwrap()
        .iter()
        .map(|e| match e {
            NeedsYouEntry::Due { due, .. } => format!("due {due}"),
            NeedsYouEntry::Promise { .. } => "promise".into(),
            NeedsYouEntry::Which { .. } => "which".into(),
            NeedsYouEntry::Unconfirmed { .. } => "unconfirmed".into(),
            NeedsYouEntry::SameName { .. } => "same name".into(),
        })
        .collect();
    assert_eq!(
        kinds,
        [
            "due 2026-11-14",
            "due 2026-11-16",
            "promise",
            "which",
            "unconfirmed"
        ]
    );

    let page = compose(&f, "2026-11-14");
    let page_kinds: Vec<_> = page.needs_you.iter().map(|l| l.kind).collect();
    assert_eq!(
        page_kinds,
        [
            LineKind::Due,
            LineKind::Due,
            LineKind::Promise,
            LineKind::Which,
            LineKind::Unconfirmed
        ]
    );
    assert_eq!(page.needs_you[0].on.as_deref(), Some("2026-11-14"));
    assert_eq!(page.needs_you[3].choices.len(), 2);
    assert!(page.needs_you[3].conflict_id.is_some());
    assert!(
        page.learned.is_empty() && page.changed.is_empty(),
        "needs-you items are not repeated below"
    );
    // Bold marks the object.
    assert_eq!(page.needs_you[0].text, "Rent is due **2026-11-14**.");
}

#[test]
fn a_page_caps_at_twelve_lines_and_counts_the_rest() {
    let f = fixture();
    for i in 0..10 {
        learn(
            &f.engine,
            &format!("p{i}"),
            cand(
                &format!("Chore {i}"),
                Predicate::Promised,
                "someone",
                &format!("Promise {i}."),
            ),
        );
    }
    for i in 0..10 {
        learn(
            &f.engine,
            &format!("k{i}"),
            cand(
                &format!("Thing {i}"),
                Predicate::Knows,
                "x",
                &format!("Thing {i} is known."),
            ),
        );
    }
    let p = compose(&f, "2026-11-14");
    let shown = p.needs_you.len() + p.changed.len() + p.learned.len();
    assert_eq!(shown, PAGE_LINE_CAP);
    assert_eq!((p.needs_you.len(), p.more_needs_you), (8, 2));
    assert_eq!((p.changed.len() + p.learned.len(), p.more_changed), (4, 6));
    assert_eq!(p.changes_count, 10);

    // Plenty of room for needs-you when little else changed; changed fills what is left.
    let g = fixture();
    for i in 0..14 {
        learn(
            &g.engine,
            &format!("p{i}"),
            cand(
                &format!("Chore {i}"),
                Predicate::Promised,
                "x",
                &format!("Promise {i}."),
            ),
        );
    }
    let p = compose(&g, "2026-11-14");
    assert_eq!((p.needs_you.len(), p.more_needs_you), (12, 2));
}

#[test]
fn what_changed_sorts_by_how_often_the_subject_is_used() {
    let f = fixture();
    // Leo is used three times, Gran once; Gran is the newest.
    learn(
        &f.engine,
        "l1",
        cand("Leo", Predicate::Prefers, "swimming", "Leo likes swimming."),
    );
    learn(
        &f.engine,
        "l2",
        cand("Leo", Predicate::Knows, "Sam", "Leo knows Sam."),
    );
    learn(
        &f.engine,
        "l3",
        cand(
            "Leo",
            Predicate::WorksOn,
            "a project",
            "Leo works on a project.",
        ),
    );
    f.clock.set_str("2026-11-13T08:00:00Z");
    learn(
        &f.engine,
        "g1",
        cand("Gran", Predicate::Knows, "Pip", "Gran knows Pip."),
    );
    let p = compose(&f, "2026-11-14");
    let order: Vec<_> = p.learned.iter().map(|l| l.text.clone()).collect();
    assert!(
        order[0].starts_with("Leo") && order[1].starts_with("Leo") && order[2].starts_with("Leo")
    );
    assert!(order[3].starts_with("Gran"));
}

/// A change is news; a new fact is not. When the cap cuts, it cuts learned lines first, however
/// often their subjects are used.
#[test]
fn changed_lines_go_before_learned_lines_when_the_page_is_cut() {
    let f = fixture();
    learn(
        &f.engine,
        "gran",
        cand("Gran", Predicate::LivesIn, "Leeds", "Gran lives in Leeds."),
    );
    save_day(&f, "2026-11-14");
    f.clock.set_str("2026-11-14T09:00:00Z");
    for i in 0..PAGE_LINE_CAP + 2 {
        learn(
            &f.engine,
            &format!("leo{i}"),
            cand(
                "Leo",
                Predicate::Knows,
                &format!("friend {i}"),
                &format!("Leo knows friend {i}."),
            ),
        );
    }
    let mut york = cand("Gran", Predicate::LivesIn, "York", "Gran lives in York.");
    york.valid_from = Some(date("2026-11-01"));
    learn(&f.engine, "gran moved", york);
    let p = compose(&f, "2026-11-15");
    assert!(p.more_changed > 0, "the page was cut");
    assert_eq!(p.changed.len(), 1);
    assert_eq!(p.changed[0].text, "Gran lives in **York**.");
}

#[test]
fn a_changed_value_is_a_changed_line_and_a_new_one_is_learned() {
    let f = fixture();
    let (_, id) = learn(
        &f.engine,
        "swim",
        cand(
            "Leo",
            Predicate::Scheduled,
            "Wednesday",
            "Leo swims on Wednesday.",
        ),
    );
    save_day(&f, "2026-11-14");
    f.clock.set_str("2026-11-14T12:00:00Z");
    f.engine
        .correct(&id, nocturne_engine::CorrectPatch::object("Monday"))
        .unwrap();
    learn(
        &f.engine,
        "new",
        cand("Gran", Predicate::Knows, "Pip", "Gran knows Pip."),
    );
    let p = compose(&f, "2026-11-15");
    assert_eq!(p.changed.len(), 1);
    assert_eq!(p.changed[0].text, "Leo swims on **Monday**.");
    assert_eq!(p.learned.len(), 1);
    assert_eq!(
        p.changed[0].receipts.len(),
        1,
        "every line carries a receipt"
    );
}

#[test]
fn variants() {
    // first
    let f = fixture();
    learn(
        &f.engine,
        "n",
        cand("Leo", Predicate::Knows, "Sam", "Leo knows Sam."),
    );
    let p1 = save_day(&f, "2026-11-14");
    assert_eq!((p1.variant, p1.page_number), (PageVariant::First, 1));

    // quiet, with upcoming this week
    let mut dentist = cand(
        "Dentist",
        Predicate::Scheduled,
        "Tuesday",
        "Dentist is on Tuesday.",
    );
    dentist.valid_from = Some(date("2026-11-17"));
    f.clock.set_str("2026-11-14T08:00:00Z");
    let (_, _) = learn(&f.engine, "dentist", dentist);
    f.clock.set_str("2026-11-15T07:00:00Z");
    let p2 = save_day(&f, "2026-11-15");
    assert_eq!(p2.page_number, 2);
    f.clock.set_str("2026-11-15T09:00:00Z");
    let quiet = compose(&f, "2026-11-16");
    assert_eq!(quiet.variant, PageVariant::Quiet);
    assert_eq!(quiet.upcoming.len(), 1);
    assert_eq!(quiet.upcoming[0].on.as_deref(), Some("2026-11-17"));
    assert_eq!(quiet.upcoming[0].kind, LineKind::Upcoming);
    f.engine.save_page(&quiet).unwrap();

    // being written: an item in the window is not read yet
    f.clock.set_str("2026-11-16T10:00:00Z");
    let unread = keep(&f.engine, "unread");
    let writing = compose(&f, "2026-11-17");
    assert_eq!(
        (writing.variant, writing.pending),
        (PageVariant::BeingWritten, 1)
    );
    f.engine.apply(&unread, vec![]).unwrap();
    assert_eq!(compose(&f, "2026-11-17").variant, PageVariant::Normal);
    f.clock.set_str("2026-11-17T07:00:00Z");
    save_day(&f, "2026-11-17");

    // heavy
    f.clock.set_str("2026-11-17T12:00:00Z");
    for i in 0..=HEAVY_ITEMS {
        let id = keep(&f.engine, &format!("item {i}"));
        f.engine.apply(&id, vec![]).unwrap();
    }
    f.clock.set_str("2026-11-17T20:00:00Z");
    let heavy = compose(&f, "2026-11-18");
    assert_eq!(
        (heavy.variant, heavy.kept_count),
        (PageVariant::Heavy, HEAVY_ITEMS + 1)
    );
    // A phone is not told how much was kept, by a count or by the kind of page.
    let phone = f
        .engine
        .compose_page(date("2026-11-18"), Edition::Phone)
        .unwrap();
    assert_eq!((phone.variant, phone.kept_count), (PageVariant::Normal, 0));
    save_day(&f, "2026-11-18");

    // catch up: nothing saved for two days
    f.clock.set_str("2026-11-20T07:00:00Z");
    let catch_up = compose(&f, "2026-11-20");
    assert_eq!(
        (catch_up.variant, catch_up.catch_up_days),
        (PageVariant::CatchUp, 1)
    );
}

#[test]
fn compose_page_only_reads() {
    let f = fixture();
    let (_, id) = learn(
        &f.engine,
        "n",
        cand("Leo", Predicate::Knows, "Sam", "Leo knows Sam."),
    );
    compose(&f, "2026-11-14");
    assert_eq!(f.engine.pages().unwrap().len(), 0);
    assert_eq!(f.engine.get_memory(&id).unwrap().last_used, None);
    assert!(f.engine.page(None).unwrap().is_none());
}

// ---- editions ---------------------------------------------------------------------------------

#[test]
fn audio_and_print_editions_exclude_private_memories_and_the_screen_shows_them() {
    let f = fixture();
    let (_, secret) = learn(
        &f.engine,
        "s",
        cand(
            "Leo",
            Predicate::Knows,
            "a diagnosis",
            "Leo has a diagnosis.",
        ),
    );
    learn(
        &f.engine,
        "o",
        cand("Gran", Predicate::Knows, "Pip", "Gran knows Pip."),
    );
    learn(
        &f.engine,
        "pr",
        cand(
            "Secret errand",
            Predicate::Promised,
            "Leo",
            "You promised Leo something private.",
        ),
    );
    let errand = f
        .engine
        .memories(Default::default())
        .unwrap()
        .iter()
        .find(|m| m.predicate == "promised")
        .unwrap()
        .id
        .clone();
    f.engine.set_private(&secret, true).unwrap();
    f.engine.set_private(&errand, true).unwrap();

    let screen = f
        .engine
        .compose_page(date("2026-11-14"), Edition::Screen)
        .unwrap();
    assert_eq!(lines(&screen).len(), 3);
    assert_eq!(screen.private_skipped, 0);
    for edition in [Edition::Audio, Edition::Print, Edition::Phone] {
        let p = f.engine.compose_page(date("2026-11-14"), edition).unwrap();
        let ls = lines(&p);
        assert_eq!(ls.len(), 1, "{edition:?}");
        assert!(ls[0].text.contains("Gran"));
        if edition == Edition::Phone {
            // A phone is not told how much stayed home, in a count or in the dek.
            assert_eq!(
                (
                    p.private_skipped,
                    p.kept_count,
                    p.let_go_count,
                    p.more_needs_you,
                    p.more_changed,
                    p.pending,
                ),
                (0, 0, 0, 0, 0, 0)
            );
            assert_eq!(p.changes_count as usize, p.changed.len());
            assert!(!matches!(
                p.variant,
                PageVariant::Quiet | PageVariant::Heavy
            ));
            assert!(p.update.is_none());
            assert_eq!(p.catch_up_days, screen.catch_up_days);
            assert_eq!(
                nocturne_engine::phone_view(p.clone()).unwrap(),
                p,
                "settled"
            );
            assert!(screen.kept_count > 0);
            let json = serde_json::to_string(&p).unwrap();
            assert!(!json.contains(&screen.dek), "{json}");
            assert!(!p.dek.contains(&screen.kept_count.to_string()), "{}", p.dek);
            assert!(!p.dek.contains("kept") && !p.dek.contains("let go"));
        } else {
            assert_eq!(p.private_skipped, 2);
        }
        assert_eq!(p.edition, edition);
    }
    assert!(
        f.engine
            .save_page(
                &f.engine
                    .compose_page(date("2026-11-14"), Edition::Audio)
                    .unwrap()
            )
            .is_err()
    );
}

// ---- the question -----------------------------------------------------------------------------

#[test]
fn the_unsaved_first_page_question_can_be_answered_with_each_choice() {
    for (choice, want) in [
        (QuestionChoice::Earlier, "06:30"),
        (QuestionChoice::Later, "07:30"),
        (QuestionChoice::Right, "07:00"),
    ] {
        let f = fixture();
        let day = date("2026-11-13");
        let shown = compose(&f, "2026-11-13");
        assert_eq!(shown.variant, PageVariant::First);
        assert_eq!(
            shown.question.as_ref().unwrap().kind,
            QuestionKind::PageTime
        );
        assert!(f.engine.page(Some(day)).unwrap().is_none(), "never saved");

        f.engine.answer_question(day, choice).unwrap();
        assert_eq!(f.engine.settings().unwrap().page_time, want);
        let after = compose(&f, "2026-11-13");
        assert_eq!(after.question.as_ref().unwrap().answered, Some(choice));
        assert!(f.engine.answer_question(day, choice).is_err(), "once");

        f.engine.save_page(&after).unwrap();
        let saved = f.engine.page(Some(day)).unwrap().unwrap();
        assert_eq!(saved.question.unwrap().answered, Some(choice));
    }
}

#[test]
fn a_day_that_is_not_the_next_page_has_no_question_to_answer() {
    let f = fixture();
    for day in ["2026-11-12", "2026-11-14"] {
        assert!(
            f.engine
                .answer_question(date(day), QuestionChoice::Right)
                .is_err()
        );
    }
}

#[test]
fn page_one_asks_for_the_page_time_and_earlier_or_later_moves_it() {
    let f = fixture();
    let p = save_day(&f, "2026-11-14");
    let q = p.question.unwrap();
    assert_eq!(
        (q.kind, q.text.as_str(), q.memory_id),
        (QuestionKind::PageTime, "07:00", None)
    );
    f.engine
        .answer_question(date("2026-11-14"), QuestionChoice::Earlier)
        .unwrap();
    assert_eq!(f.engine.settings().unwrap().page_time, "06:30");
    assert_eq!(
        f.engine
            .page(Some(date("2026-11-14")))
            .unwrap()
            .unwrap()
            .question
            .unwrap()
            .answered,
        Some(QuestionChoice::Earlier)
    );
    assert!(
        f.engine
            .answer_question(date("2026-11-14"), QuestionChoice::Later)
            .is_err(),
        "once"
    );

    let g = fixture();
    save_day(&g, "2026-11-14");
    g.engine
        .answer_question(date("2026-11-14"), QuestionChoice::Later)
        .unwrap();
    assert_eq!(g.engine.settings().unwrap().page_time, "07:30");
    let h = fixture();
    save_day(&h, "2026-11-14");
    h.engine
        .answer_question(date("2026-11-14"), QuestionChoice::Right)
        .unwrap();
    assert_eq!(h.engine.settings().unwrap().page_time, "07:00");
    assert!(
        h.engine
            .answer_question(date("2026-11-15"), QuestionChoice::Yes)
            .is_err(),
        "no question that day"
    );
}

#[test]
fn the_daily_question_picks_by_the_rules() {
    let f = fixture();
    // One-source memory A, two-source memory B, a safety memory, a keep-forever memory.
    let (_, a) = learn(
        &f.engine,
        "a",
        cand("Leo", Predicate::Prefers, "swimming", "Leo likes swimming."),
    );
    let (_, b) = learn(
        &f.engine,
        "b1",
        cand("Gran", Predicate::LivesIn, "Leeds", "Gran lives in Leeds."),
    );
    learn(
        &f.engine,
        "b2",
        cand("Gran", Predicate::LivesIn, "Leeds", "Gran lives in Leeds."),
    );
    let (_, dose) = learn(
        &f.engine,
        "c",
        cand("Leo", Predicate::Takes, "metformin", "Leo takes metformin."),
    );
    let (_, forever) = learn(
        &f.engine,
        "d",
        cand("Pat", Predicate::Knows, "a PIN", "Pat knows a PIN."),
    );
    f.engine.set_keep_forever(&forever, true).unwrap();
    save_day(&f, "2026-11-14"); // page 1: the page-time question

    f.clock.set_str("2026-11-14T20:00:00Z");
    let p2 = compose(&f, "2026-11-15");
    let q = p2.question.clone().unwrap();
    assert_eq!(q.kind, QuestionKind::YesNo);
    assert_eq!(
        q.memory_id.as_deref(),
        Some(a.as_str()),
        "active, one source, not yet confirmed"
    );
    assert_eq!(q.text, "Leo likes swimming.");
    assert!(q.receipt.is_some());
    f.engine.save_page(&p2).unwrap();

    // Not asked again for 14 days; never a safety or keep-forever memory.
    f.clock.set_str("2026-11-15T20:00:00Z");
    let q3 = compose(&f, "2026-11-16").question.unwrap();
    assert_eq!(q3.memory_id.as_deref(), Some(b.as_str()));
    for id in [&dose, &forever] {
        assert_ne!(q3.memory_id.as_deref(), Some(id.as_str()));
    }
    f.engine.save_page(&compose(&f, "2026-11-16")).unwrap();
    f.clock.set_str("2026-11-16T20:00:00Z");
    assert!(
        compose(&f, "2026-11-17").question.is_none(),
        "nothing eligible is better than a bad question"
    );
    f.clock.set_str("2026-12-01T20:00:00Z");
    assert_eq!(
        compose(&f, "2026-12-02")
            .question
            .unwrap()
            .memory_id
            .as_deref(),
        Some(a.as_str())
    );
}

#[test]
fn answering_yes_confirms_and_counts_as_use_and_no_asks_again() {
    let f = fixture();
    let (_, a) = learn(
        &f.engine,
        "a",
        cand("Leo", Predicate::Prefers, "swimming", "Leo likes swimming."),
    );
    f.clock.set_str("2026-11-13T07:30:00Z");
    let (_, b) = learn(
        &f.engine,
        "b",
        cand("Gran", Predicate::Knows, "Pip", "Gran knows Pip."),
    );
    save_day(&f, "2026-11-14");
    f.clock.set_str("2026-11-14T20:00:00Z");
    f.engine.save_page(&compose(&f, "2026-11-15")).unwrap();
    f.clock.set_str("2026-11-15T09:00:00Z");
    f.engine
        .answer_question(date("2026-11-15"), QuestionChoice::Yes)
        .unwrap();
    let m = f.engine.get_memory(&a).unwrap();
    assert_eq!(m.certainty, nocturne_contract::Certainty::Sure);
    assert_eq!(m.last_used.as_deref(), Some("2026-11-15T09:00:00.000Z"));

    f.engine.save_page(&compose(&f, "2026-11-16")).unwrap();
    f.engine
        .answer_question(date("2026-11-16"), QuestionChoice::No)
        .unwrap();
    assert_eq!(
        f.engine.get_memory(&b).unwrap().status,
        MemoryStatus::Unconfirmed
    );
    assert!(
        f.engine
            .needs_you()
            .unwrap()
            .iter()
            .any(|e| matches!(e, NeedsYouEntry::Unconfirmed { memory, .. } if memory.id == b))
    );
    assert!(
        f.engine
            .answer_question(date("2026-11-16"), QuestionChoice::Earlier)
            .is_err()
    );
}

// ---- saved pages ------------------------------------------------------------------------------

#[test]
fn saving_marks_cited_memories_used_once_but_not_let_go_lines() {
    let f = fixture();
    let (_, kept) = learn(
        &f.engine,
        "k",
        cand("Leo", Predicate::Knows, "Sam", "Leo knows Sam."),
    );
    let (_, gone) = learn(
        &f.engine,
        "g",
        cand("Old", Predicate::Knows, "a PIN", "The old gym PIN."),
    );
    f.clock.advance_days(100);
    f.engine.decay().unwrap();
    assert_eq!(
        f.engine.get_memory(&gone).unwrap().status,
        MemoryStatus::LetGo
    );
    f.engine
        .mark_used(
            std::slice::from_ref(&kept),
            nocturne_engine::UseKind::Answer,
        )
        .unwrap();
    learn(
        &f.engine,
        "n",
        cand("Gran", Predicate::Knows, "Pip", "Gran knows Pip."),
    );
    let page = compose(&f, "2027-02-22");
    assert_eq!(page.let_go.len(), 1);
    assert_eq!(page.let_go[0].kind, LineKind::LetGo);
    assert!(page.let_go[0].last_used.is_some());
    assert_eq!(page.let_go_count, 1);
    f.engine.save_page(&page).unwrap();
    assert_eq!(
        f.engine.get_memory(&gone).unwrap().status,
        MemoryStatus::LetGo,
        "the page does not undo its own news"
    );
    let cited = f.engine.memories(Default::default()).unwrap();
    let gran = cited.iter().find(|m| m.subject == "Gran").unwrap();
    let first_use = gran.last_used.clone();
    assert!(first_use.is_some(), "publishing on a page is use");

    f.clock.advance_days(1);
    f.engine.save_page(&page).unwrap();
    assert_eq!(
        f.engine.get_memory(&gran.id).unwrap().last_used,
        first_use,
        "once per page"
    );
}

#[test]
fn saved_pages_round_trip_with_numbers_summaries_and_ticks() {
    let f = fixture();
    let (_, promise) = learn(
        &f.engine,
        "p",
        cand(
            "Call Gran",
            Predicate::Promised,
            "Gran",
            "You promised to call Gran.",
        ),
    );
    let p1 = save_day(&f, "2026-11-14");
    assert_eq!(f.engine.page(None).unwrap().unwrap(), {
        let mut expected = p1.clone();
        expected.question.as_mut().unwrap().answered = None;
        expected
    });
    f.clock.set_str("2026-11-14T20:00:00Z");
    let p2 = save_day(&f, "2026-11-15");
    assert_eq!(p2.page_number, 2);
    let summaries = f.engine.pages().unwrap();
    assert_eq!(
        summaries
            .iter()
            .map(|s| s.date.as_str())
            .collect::<Vec<_>>(),
        ["2026-11-15", "2026-11-14"]
    );
    assert_eq!(summaries[1].needs_you, 1);
    assert_eq!(f.engine.page(None).unwrap().unwrap().date, "2026-11-15");
    assert!(f.engine.page(Some(date("2026-11-10"))).unwrap().is_none());
    // Saving again keeps the number.
    f.engine.save_page(&p2).unwrap();
    assert_eq!(
        f.engine
            .page(Some(date("2026-11-15")))
            .unwrap()
            .unwrap()
            .page_number,
        2
    );

    // Ticking something off shows on the saved page.
    f.engine.mark_done(&promise).unwrap();
    let again = f.engine.page(Some(date("2026-11-14"))).unwrap().unwrap();
    assert!(again.needs_you[0].done);
    assert!(f.engine.needs_you().unwrap().is_empty());
}

#[test]
fn a_dst_page_time_in_the_spring_gap_still_composes() {
    let f = fixture_at("2026-03-28T12:00:00Z", "Europe/London");
    f.engine
        .update_settings(nocturne_contract::SettingsPatch {
            page_time: Some("01:30".into()),
            ..Default::default()
        })
        .unwrap();
    save_day(&f, "2026-03-28");
    let id = keep(&f.engine, "just before the gap");
    f.engine.apply(&id, vec![]).unwrap();
    f.clock.set_str("2026-03-29T05:00:00Z");
    let p = compose(&f, "2026-03-29");
    assert_eq!(p.kept_count, 1);
    assert_eq!(p.date, "2026-03-29");
}

#[test]
fn audio_and_print_never_carry_a_headline_written_with_private_content() {
    let f = fixture();
    let (_, secret) = learn(
        &f.engine,
        "s",
        cand(
            "Zebrafish47",
            Predicate::Knows,
            "a diagnosis",
            "Zebrafish47 has a diagnosis.",
        ),
    );
    learn(
        &f.engine,
        "o",
        cand("Gran", Predicate::Knows, "Pip", "Gran knows Pip."),
    );
    let mut page = compose(&f, "2026-11-14");
    page.headline = "News about Zebrafish47".into();
    page.dek = "Zebrafish47 saw the doctor.".into();
    f.engine.save_page(&page).unwrap();

    // Nothing private yet: audio keeps the headline the mind wrote.
    let audio = f
        .engine
        .page_for(Some(date("2026-11-14")), Edition::Audio)
        .unwrap()
        .unwrap();
    assert_eq!(audio.headline, "News about Zebrafish47");

    f.engine.set_private(&secret, true).unwrap();
    for edition in [Edition::Audio, Edition::Print, Edition::Phone] {
        let p = f
            .engine
            .page_for(Some(date("2026-11-14")), edition)
            .unwrap()
            .unwrap();
        let json = serde_json::to_string(&p).unwrap().to_lowercase();
        assert!(!json.contains("zebrafish47"), "{edition:?}: {json}");
        assert_eq!(p.headline, "Saturday 14 November");
        assert_eq!(p.private_skipped >= 1, edition != Edition::Phone);
        assert!(
            lines(&p).iter().any(|l| l.text.contains("Gran")),
            "other content stays"
        );
    }
    // The screen keeps everything, headline included.
    let screen = f
        .engine
        .page_for(Some(date("2026-11-14")), Edition::Screen)
        .unwrap()
        .unwrap();
    assert_eq!(screen.headline, "News about Zebrafish47");

    // A private daily question drops the headline too.
    let g = fixture();
    let (_, q1) = learn(
        &g.engine,
        "q",
        cand(
            "Zebrafish47",
            Predicate::Prefers,
            "swimming",
            "Zebrafish47 likes swimming.",
        ),
    );
    g.clock.set_str("2026-11-14T06:00:00Z");
    save_day(&g, "2026-11-14");
    g.clock.set_str("2026-11-14T20:00:00Z");
    let mut p2 = compose(&g, "2026-11-15");
    assert_eq!(
        p2.question.as_ref().unwrap().memory_id.as_deref(),
        Some(q1.as_str())
    );
    p2.headline = "About Zebrafish47".into();
    g.engine.save_page(&p2).unwrap();
    g.engine.set_private(&q1, true).unwrap();
    let p = g
        .engine
        .page_for(Some(date("2026-11-15")), Edition::Audio)
        .unwrap()
        .unwrap();
    assert!(p.question.is_none());
    assert_eq!(p.headline, "Sunday 15 November");
}

/// What a phone lists: the template headline once a line is private, and only public needs-you
/// lines in the count, the same count its phone edition of the page gives.
#[test]
fn phone_summaries_never_count_or_headline_private_lines() {
    let f = fixture();
    learn(
        &f.engine,
        "g",
        cand(
            "Gran",
            Predicate::Promised,
            "a call",
            "You promised Gran a call.",
        ),
    );
    let (_, errand) = learn(
        &f.engine,
        "pr",
        cand(
            "Secret errand",
            Predicate::Promised,
            "Leo",
            "You promised Leo something private.",
        ),
    );
    let mut page = compose(&f, "2026-11-14");
    page.headline = "Zebrafish47 and Gran".into();
    f.engine.save_page(&page).unwrap();

    let screen = f.engine.pages_for(Edition::Screen).unwrap();
    assert_eq!(screen, f.engine.pages().unwrap());
    assert_eq!(screen[0].needs_you, 2);
    let phone = f.engine.pages_for(Edition::Phone).unwrap();
    assert_eq!(phone[0].needs_you, 2, "nothing private yet");
    assert_eq!(
        phone[0].headline, "Saturday 14 November",
        "a phone never gets the headline written with everything in view"
    );

    f.engine.set_private(&errand, true).unwrap();
    let phone = f.engine.pages_for(Edition::Phone).unwrap();
    assert_eq!(phone.len(), 1);
    assert_eq!(phone[0].needs_you, 1);
    assert_eq!(phone[0].headline, "Saturday 14 November");
    let json = serde_json::to_string(&phone).unwrap().to_lowercase();
    assert!(!json.contains("zebrafish47"), "{json}");
    let edition = f
        .engine
        .page_for(Some(date("2026-11-14")), Edition::Phone)
        .unwrap()
        .unwrap();
    assert_eq!(
        nocturne_engine::needs_you_count(&edition),
        phone[0].needs_you
    );
    assert_eq!(f.engine.pages_for(Edition::Screen).unwrap()[0].needs_you, 2);
}

// ---- the name question is counted once -----------------------------------------------------------

/// Two people called Sam and an entry saying "Sam": one open "Which Sam is this?" question.
fn a_name_question(f: &Fixture) {
    let conn = rusqlite::Connection::open(f.dir.path().join("nocturne.db")).unwrap();
    for (full, role) in [
        ("Sam Jones", "your neighbour"),
        ("Sam Patel", "Leo\u{2019}s teacher"),
    ] {
        let mut c = cand(full, Predicate::Knows, "you", &format!("{full} is {role}."));
        c.subject_kind = Some(nocturne_contract::EntityKind::Person);
        let (_, memory) = learn(&f.engine, &format!("{full} is {role}."), c);
        conn.execute(
            "UPDATE entities SET role = ?2 WHERE id = (SELECT entity_id FROM memories WHERE id = ?1)",
            rusqlite::params![memory, role],
        )
        .unwrap();
    }
    let entry = keep(&f.engine, "Priya made tea. Sam came round to fix the sink.");
    conn.execute(
        "UPDATE items SET metadata = json_set(metadata, '$.journal', 'entry', \
         '$.journal_day', '2026-11-12') WHERE id = ?1",
        [&entry],
    )
    .unwrap();
    let mut sam = cand(
        "Sam",
        Predicate::Promised,
        "the sink",
        "Sam will fix the sink.",
    );
    sam.subject_kind = Some(nocturne_contract::EntityKind::Person);
    f.engine.apply(&entry, vec![sam]).unwrap();
    assert_eq!(f.engine.same_name_questions(None).unwrap().len(), 1);
}

#[test]
fn the_page_counts_the_name_question_with_the_lines_that_need_you() {
    let f = fixture();
    learn(&f.engine, "form", due("Trip form", "2026-11-16"));
    learn(&f.engine, "rent", due("Rent", "2026-11-14"));
    a_name_question(&f);
    let p = compose(&f, "2026-11-14");
    assert_eq!(
        p.needs_you.len(),
        2,
        "the name question has no line of its own"
    );
    assert_eq!(
        p.more_needs_you, 1,
        "it is counted in the link under the lines"
    );
    assert!(p.dek.contains(" 3 need you."), "{}", p.dek);

    let saved = save_day(&f, "2026-11-14");
    assert_eq!(saved.more_needs_you, 1);
    let p = f.engine.page(Some(date("2026-11-14"))).unwrap().unwrap();
    assert_eq!((p.needs_you.len(), p.more_needs_you), (2, 1));

    let print = f
        .engine
        .page_for(Some(date("2026-11-14")), Edition::Print)
        .unwrap()
        .unwrap();
    assert_eq!(print.more_needs_you, 1);
    let phone = f
        .engine
        .page_for(Some(date("2026-11-14")), Edition::Phone)
        .unwrap()
        .unwrap();
    assert_eq!(
        phone.dek, "3 need you.",
        "a phone can answer the name question too"
    );
    assert_eq!(phone.more_needs_you, 1);
    assert_eq!(
        nocturne_engine::phone_view(phone.clone()).unwrap(),
        phone,
        "settled"
    );
}

#[test]
fn a_page_with_no_name_question_counts_as_before() {
    let f = fixture();
    learn(&f.engine, "rent", due("Rent", "2026-11-14"));
    let p = compose(&f, "2026-11-14");
    assert_eq!(p.more_needs_you, 0);
    assert!(p.dek.contains(" 1 needs you."), "{}", p.dek);
}
