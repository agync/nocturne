mod common;

use common::*;
use jiff::SignedDuration;
use nocturne_contract::{
    CheckInInput, Feeling, FeelingPart, FeelingPoint, GlanceKind, Kept, Period, PeriodKind,
    ReflectionView, Source, WrittenWhen,
};
use nocturne_engine::{Clock, ReflectInput, WrittenSentence};

fn day_period(engine: &nocturne_engine::Engine, day: &str) -> Period {
    engine.period_of(PeriodKind::Day, date(day)).unwrap()
}

fn feeling(part: FeelingPart, words: &[&str], pleasant: f32, energy: f32) -> Feeling {
    Feeling {
        part,
        point: FeelingPoint { pleasant, energy },
        words: words.iter().map(|w| w.to_string()).collect(),
    }
}

fn checkin(f: &Fixture, part: FeelingPart, words: &[&str]) -> String {
    f.engine
        .keep_checkin(
            CheckInInput {
                feeling: feeling(part, words, 0.8, 0.5),
                line: None,
                urge: None,
                replaces: None,
            },
            Source::Journal,
        )
        .unwrap()
        .id
}

fn sentence(text: &str, cites: &[usize]) -> WrittenSentence {
    WrittenSentence {
        text: text.to_string(),
        cites: cites.to_vec(),
    }
}

fn mention(f: &Fixture, item: &str, entity: &str, name: &str, new: bool) {
    let conn = rusqlite::Connection::open(f.dir.path().join("nocturne.db")).unwrap();
    conn.execute(
        "INSERT OR IGNORE INTO entities(id, kind, name, key, role) VALUES (?1, 'person', ?2, ?3, NULL)",
        rusqlite::params![entity, name, name.to_lowercase()],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO mentions(id, item_id, entity_id, name, start, end, ord, state, new, created_at) \
         VALUES (?1, ?2, ?3, ?4, 0, ?5, 0, 'found', ?6, '2026-11-13T07:00:00.000Z')",
        rusqlite::params![
            format!("men_{item}_{entity}"),
            item,
            entity,
            name,
            name.len() as u32,
            new
        ],
    )
    .unwrap();
}

/// What a reflection on the day is written from, once everything kept has been read.
fn written(f: &Fixture, day: &str) -> (Period, ReflectInput) {
    read_all(f);
    let period = day_period(&f.engine, day);
    let input = f.engine.reflection_input(&period).unwrap();
    (period, input)
}

/// Read every item still waiting, with nothing found in it.
fn read_all(f: &Fixture) {
    let conn = rusqlite::Connection::open(f.dir.path().join("nocturne.db")).unwrap();
    let ids: Vec<String> = conn
        .prepare("SELECT id FROM items WHERE status = 'kept'")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    for id in ids {
        // An item being forgotten cannot be read; it is left as it is.
        let _ = f.engine.apply(&id, vec![]);
    }
}

fn advance(f: &Fixture, minutes: i64) {
    f.clock.set(
        f.clock
            .now()
            .checked_add(SignedDuration::from_mins(minutes))
            .unwrap(),
    );
}

#[test]
fn a_glance_counts_what_was_kept_by_kind_and_leaves_check_ins_out_of_the_count() {
    let f = fixture();
    let day = date("2026-11-13");
    keep(&f.engine, "First note");
    keep(&f.engine, "Second note");
    f.engine
        .save_entry_text(day, None, 0, "Slow start\nThen tea")
        .unwrap();
    f.engine
        .keep_bytes(b"voice", "webm", Source::Recording)
        .unwrap();
    f.engine
        .keep_bytes(b"photo-a", "jpg", Source::Drop)
        .unwrap();
    f.engine
        .keep_bytes(b"photo-b", "jpg", Source::Drop)
        .unwrap();
    f.engine
        .keep_bytes(b"photo-c", "jpg", Source::Drop)
        .unwrap();
    let file = write_file(f.dir.path(), "form.txt", b"School trip");
    f.engine.keep_file(&file, Source::WatchedFolder).unwrap();
    checkin(&f, FeelingPart::Settled, &["calm"]);
    f.engine
        .keep_good_thing(day, "Sun on the table", None, Source::Journal)
        .unwrap();

    let glance = f
        .engine
        .glance(&day_period(&f.engine, "2026-11-13"))
        .unwrap();
    let counts: Vec<(GlanceKind, usize)> = glance
        .kept
        .iter()
        .map(|k| (k.kind, k.item_ids.len()))
        .collect();
    assert_eq!(
        counts,
        [
            (GlanceKind::Note, 4),
            (GlanceKind::Photo, 3),
            (GlanceKind::Voice, 1),
            (GlanceKind::Document, 1)
        ]
    );
    assert_eq!(glance.felt.len(), 1);
    assert_eq!(glance.felt[0].feeling.words, ["calm"]);
    assert_eq!(glance.good[0].text, "Sun on the table");
    assert_eq!(glance.entry_lines, ["Slow start", "Then tea"]);
    assert!(glance.entry_item_id.is_some());
}

#[test]
fn a_glance_lists_people_before_places_from_the_names_found() {
    let f = fixture();
    let a = keep(&f.engine, "Gran and the allotment");
    mention(&f, &a, "ent_gran", "Gran", true);
    let conn = rusqlite::Connection::open(f.dir.path().join("nocturne.db")).unwrap();
    conn.execute(
        "INSERT INTO entities(id, kind, name, key) VALUES ('ent_allot', 'place', 'the allotment', 'allotment')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO mentions(id, item_id, entity_id, name, start, end, ord, state, new, created_at) \
         VALUES ('men_x', ?1, 'ent_allot', 'the allotment', 9, 22, 0, 'found', 0, '2026-11-13T07:00:00.000Z')",
        [&a],
    )
    .unwrap();
    let g = f
        .engine
        .glance(&day_period(&f.engine, "2026-11-13"))
        .unwrap();
    let names: Vec<&str> = g.with.iter().map(|n| n.name.as_str()).collect();
    assert_eq!(names, ["Gran", "the allotment"]);
    assert!(g.with[0].new);
}

#[test]
fn a_reflection_is_written_from_the_days_items_without_check_ins_or_items_left_out() {
    let f = fixture();
    let day = date("2026-11-13");
    let note = keep(&f.engine, "Posted the parcel");
    let left = keep(&f.engine, "Something private to me");
    f.engine.set_left_out(&left, true).unwrap();
    f.engine
        .save_entry_text(day, None, 0, "Slow start")
        .unwrap();
    f.engine
        .keep_good_thing(day, "Sun on the table", None, Source::Journal)
        .unwrap();
    checkin(&f, FeelingPart::Low, &["tired"]);
    f.engine
        .keep_text("A page about school trips", Source::PastedWeb)
        .unwrap();
    let Kept::New { item: voice } = f
        .engine
        .keep_bytes(b"voice", "webm", Source::Recording)
        .unwrap()
    else {
        panic!("new")
    };

    let (_, input) = written(&f, "2026-11-13");
    assert_eq!(input.day, day);
    assert!(input.so_far, "the day is still going");
    assert!(input.items.iter().all(|i| i.id != left));
    assert!(
        input
            .items
            .iter()
            .any(|i| i.id == note && i.label == "note" && i.at == "7:00 am")
    );
    assert!(
        input
            .items
            .iter()
            .any(|i| i.label == "today's entry" && !i.background)
    );
    assert!(input.items.iter().any(|i| i.label == "good thing"));
    assert!(
        input
            .items
            .iter()
            .any(|i| i.label == "web page" && i.background)
    );
    assert!(
        input.items.iter().all(|i| i.id != voice.id),
        "a recording with no words yet has nothing to say"
    );
    assert_eq!(input.items.len(), 4);

    f.clock.set_str("2026-11-14T09:00:00Z");
    let (_, finished) = written(&f, "2026-11-13");
    assert!(!finished.so_far);
}

#[test]
fn a_saved_reflection_carries_receipts_and_a_finished_one_replaces_a_so_far_one() {
    let f = fixture();
    let day = date("2026-11-13");
    let block = f
        .engine
        .save_entry_text(day, None, 0, "Gran came for tea")
        .unwrap()
        .unwrap();
    checkin(&f, FeelingPart::Settled, &["calm", "content"]);
    f.engine
        .keep_good_thing(day, "Sun on the table", None, Source::Journal)
        .unwrap();
    let (period, input) = written(&f, "2026-11-13");
    let idx = input
        .items
        .iter()
        .position(|i| i.id == block.item.id)
        .unwrap();

    let so_far = f
        .engine
        .save_day_reflection(
            &period,
            &input,
            &[
                sentence("**Gran came for tea** this afternoon.", &[idx]),
                sentence("A line that cites nothing.", &[]),
                sentence("A line that cites something that is not there.", &[99]),
            ],
            WrittenWhen::OnDemand,
        )
        .unwrap();
    assert!(so_far.so_far);
    assert_eq!(so_far.the_day.len(), 1);
    assert_eq!(so_far.the_day[0].receipts[0].item_id, block.item.id);
    assert_eq!(
        so_far.the_day[0].receipts[0].label, "Gran came for tea",
        "the label is the title"
    );
    assert_eq!(so_far.from_count, 2);
    assert_eq!(so_far.felt.len(), 1);
    assert_eq!(so_far.good.len(), 1);
    assert_eq!(so_far.lines_removed, 0);
    assert!(f.engine.journal_day(day).unwrap().reflection.is_some());

    f.clock.set_str("2026-11-14T09:00:00Z");
    let (_, input) = written(&f, "2026-11-13");
    let idx = input
        .items
        .iter()
        .position(|i| i.id == block.item.id)
        .unwrap();
    let done = f
        .engine
        .save_day_reflection(
            &period,
            &input,
            &[sentence("You had **Gran for tea**.", &[idx])],
            WrittenWhen::Overnight,
        )
        .unwrap();
    assert!(!done.so_far);
    let Some(ReflectionView::Written { reflection }) = f.engine.saved_reflection(&period).unwrap()
    else {
        panic!("a day is Written")
    };
    assert_eq!(reflection.id, done.id);
    assert_eq!(reflection.the_day[0].text, "You had **Gran for tea**.");
    let summary = f.engine.journal_day(day).unwrap().reflection.unwrap();
    assert_eq!(summary.first_sentence, "You had Gran for tea.");
    assert_eq!(summary.from_count, 2);

    f.engine.delete_reflection(&period).unwrap();
    assert!(f.engine.saved_reflection(&period).unwrap().is_none());
}

#[test]
fn leaving_out_hides_reflection_lines_and_undo_brings_them_back() {
    let f = fixture();
    let day = date("2026-11-13");
    let keep_in = f
        .engine
        .save_entry_text(day, None, 0, "Walked to the shop")
        .unwrap()
        .unwrap();
    let leave = f
        .engine
        .save_entry_text(day, None, 1, "A hard conversation")
        .unwrap()
        .unwrap();
    let (period, input) = written(&f, "2026-11-13");
    let at = |id: &str| input.items.iter().position(|i| i.id == id).unwrap();
    f.engine
        .save_day_reflection(
            &period,
            &input,
            &[
                sentence("You **walked to the shop**.", &[at(&keep_in.item.id)]),
                sentence("You had **a hard conversation**.", &[at(&leave.item.id)]),
            ],
            WrittenWhen::OnDemand,
        )
        .unwrap();
    let lines = |f: &Fixture| match f.engine.saved_reflection(&period).unwrap() {
        Some(ReflectionView::Written { reflection }) => reflection.the_day.len(),
        other => panic!("{other:?}"),
    };
    assert_eq!(lines(&f), 2);
    f.engine.set_left_out(&leave.item.id, true).unwrap();
    assert_eq!(lines(&f), 1);
    assert!(
        f.engine
            .journal_day(day)
            .unwrap()
            .reflection
            .unwrap()
            .first_sentence
            .contains("shop")
    );
    f.engine.set_left_out(&leave.item.id, false).unwrap();
    assert_eq!(lines(&f), 2, "nothing was deleted");
    let (_, input) = written(&f, "2026-11-13");
    f.engine.set_left_out(&leave.item.id, true).unwrap();
    let again = f.engine.reflection_input(&period).unwrap();
    assert_eq!(again.items.len(), input.items.len() - 1);
}

#[test]
fn forgetting_a_cited_item_takes_the_line_out_counts_it_and_the_preview_counts_it_first() {
    let f = fixture();
    let (note, memory) = learn(
        &f.engine,
        "Posted the parcel to Sam",
        cand(
            "Sam",
            nocturne_engine::Predicate::Knows,
            "Parcel",
            "You posted a parcel to Sam.",
        ),
    );
    let other = keep(&f.engine, "Tea with Gran");
    let (period, input) = written(&f, "2026-11-13");
    let at = |id: &str| input.items.iter().position(|i| i.id == id).unwrap();
    f.engine
        .save_day_reflection(
            &period,
            &input,
            &[
                sentence("You **posted a parcel**.", &[at(&note)]),
                sentence("You had **tea with Gran**.", &[at(&other)]),
            ],
            WrittenWhen::OnDemand,
        )
        .unwrap();

    let preview = f.engine.preview_forget(&[memory], true).unwrap();
    assert_eq!(preview.reflection_lines, Some(1));
    f.engine.forget(&preview.token).unwrap();

    let Some(ReflectionView::Written { reflection }) = f.engine.saved_reflection(&period).unwrap()
    else {
        panic!("still written")
    };
    assert_eq!(reflection.the_day.len(), 1);
    assert_eq!(reflection.the_day[0].receipts[0].item_id, other);
    assert_eq!(reflection.lines_removed, 1);
    assert!(f.engine.get_item(&note).is_err());
    assert_eq!(
        sql_count(
            f.dir.path(),
            &format!("SELECT COUNT(*) FROM reflection_cites WHERE item_id = '{note}'")
        ),
        0
    );
}

#[test]
fn a_preview_with_nothing_cited_has_no_reflection_count() {
    let f = fixture();
    let (_, memory) = learn(
        &f.engine,
        "Dentist on Thursday",
        cand(
            "Dentist",
            nocturne_engine::Predicate::Scheduled,
            "Thursday",
            "Dentist on Thursday.",
        ),
    );
    assert_eq!(
        f.engine
            .preview_forget(&[memory], true)
            .unwrap()
            .reflection_lines,
        None
    );
}

#[test]
fn deleting_an_entry_block_deletes_its_earlier_versions_and_the_lines_that_cite_them() {
    let f = fixture();
    let day = date("2026-11-13");
    let first = f
        .engine
        .save_entry_text(day, None, 0, "Gran came for tea")
        .unwrap()
        .unwrap();
    f.engine.apply(&first.item.id, vec![]).unwrap();
    advance(&f, 5);
    let second = f
        .engine
        .save_entry_text(day, Some(&first.item.id), 0, "Gran came for tea and stayed")
        .unwrap()
        .unwrap();
    let (period, input) = written(&f, "2026-11-13");
    let cited = input
        .items
        .iter()
        .position(|i| i.id == second.item.id)
        .unwrap();
    f.engine
        .save_day_reflection(
            &period,
            &input,
            &[sentence("**Gran stayed**.", &[cited])],
            WrittenWhen::OnDemand,
        )
        .unwrap();
    // A line written earlier cited the version that was replaced.
    let conn = rusqlite::Connection::open(f.dir.path().join("nocturne.db")).unwrap();
    conn.execute(
        "INSERT INTO reflection_cites(reflection_id, item_id) SELECT id, ?1 FROM reflections",
        [&first.item.id],
    )
    .unwrap();

    f.engine.unkeep(&second.item.id).unwrap();
    assert!(f.engine.get_item(&second.item.id).is_err());
    assert!(
        f.engine.get_item(&first.item.id).is_err(),
        "earlier versions go with it"
    );
    assert_eq!(sql_count(f.dir.path(), "SELECT COUNT(*) FROM items"), 0);
    let Some(ReflectionView::Written { reflection }) = f.engine.saved_reflection(&period).unwrap()
    else {
        panic!("still there")
    };
    assert!(reflection.the_day.is_empty());
    assert_eq!(reflection.lines_removed, 1);
}

#[test]
fn a_recap_together_line_needs_three_days() {
    let f = fixture();
    let days = ["2026-11-09", "2026-11-10", "2026-11-11", "2026-11-12"];
    let mut together_at_two = None;
    for (n, d) in days.iter().enumerate() {
        f.clock.set_str(&format!("{d}T09:00:00Z"));
        let note = keep(&f.engine, &format!("Saw Gran on {d}"));
        mention(&f, &note, "ent_gran", "Gran", n == 0);
        checkin(&f, FeelingPart::Settled, &["calm"]);
        if n == 1 {
            let p = f.engine.period_of(PeriodKind::Week, date(d)).unwrap();
            together_at_two = Some(f.engine.compose_recap(&p, WrittenWhen::OnDemand).unwrap());
        }
    }
    assert!(
        together_at_two.unwrap().together.is_empty(),
        "two days is not enough"
    );

    f.clock.set_str("2026-11-13T07:00:00Z");
    let p = f
        .engine
        .period_of(PeriodKind::Week, date("2026-11-12"))
        .unwrap();
    let recap = f.engine.compose_recap(&p, WrittenWhen::OnDemand).unwrap();
    assert_eq!(recap.together.len(), 1);
    let t = &recap.together[0];
    assert_eq!((t.count, t.total), (4, 4));
    assert_eq!(t.with.name, "Gran");
    assert_eq!(t.part, Some(FeelingPart::Settled));
    assert_eq!(t.receipts.len(), 4);
    assert_eq!(recap.days, 4);
    assert!(recap.so_far, "the week runs to Sunday");
    assert_eq!(recap.people[0].days.len(), 4);
    assert_eq!(recap.new_names.len(), 1);
    assert_eq!(recap.words[0].word, "calm");
    assert_eq!(recap.words[0].checkin_ids.len(), 4);
    assert!(recap.compare.is_none(), "weeks do not compare");
    assert_eq!(recap.checkins.len(), 4);
}

#[test]
fn highlights_pick_one_moment_per_day_and_cap_a_week_at_five() {
    let f = fixture();
    for n in 9..=15 {
        f.clock.set_str(&format!("2026-11-{n:02}T09:00:00Z"));
        f.engine
            .keep_checkin(
                CheckInInput {
                    feeling: feeling(FeelingPart::LitUp, &["thrilled"], 0.9, 0.8),
                    line: Some(format!("A good day number {n}")),
                    urge: None,
                    replaces: None,
                },
                Source::Journal,
            )
            .unwrap();
        keep(&f.engine, &format!("Ordinary note {n}"));
    }
    f.clock.set_str("2026-11-16T09:00:00Z");
    let p = f
        .engine
        .period_of(PeriodKind::Week, date("2026-11-12"))
        .unwrap();
    let recap = f.engine.compose_recap(&p, WrittenWhen::Overnight).unwrap();
    assert_eq!(recap.highlights.len(), 5);
    let days: Vec<&str> = recap.highlights.iter().map(|h| h.day.as_str()).collect();
    let mut unique = days.clone();
    unique.dedup();
    assert_eq!(days, unique, "one a day");
    assert!(recap.highlights[0].quoted);
    assert!(recap.highlights[0].feeling.is_some());
    assert!(!recap.so_far);
}

#[test]
fn a_month_compares_with_the_one_before_in_counts_only() {
    let f = fixture();
    f.clock.set_str("2026-10-05T09:00:00Z");
    checkin(&f, FeelingPart::Low, &["tired"]);
    for d in ["2026-11-02", "2026-11-03", "2026-11-04"] {
        f.clock.set_str(&format!("{d}T09:00:00Z"));
        checkin(&f, FeelingPart::Settled, &["calm"]);
    }
    f.clock.set_str("2026-12-01T09:00:00Z");
    let p = f
        .engine
        .period_of(PeriodKind::Month, date("2026-11-20"))
        .unwrap();
    let recap = f.engine.compose_recap(&p, WrittenWhen::Overnight).unwrap();
    let cmp = recap.compare.expect("months compare");
    assert_eq!((cmp.count, cmp.previous), (3, 1));
    assert_eq!(cmp.previous_period.start, "2026-10-01");
    assert!(
        f.engine
            .compose_recap(&day_period(&f.engine, "2026-11-02"), WrittenWhen::OnDemand)
            .is_err()
    );
}

#[test]
fn a_saved_recap_is_read_back_and_hides_what_was_left_out() {
    let f = fixture();
    f.clock.set_str("2026-11-10T09:00:00Z");
    let c1 = checkin(&f, FeelingPart::Settled, &["calm"]);
    let c2 = checkin(&f, FeelingPart::LitUp, &["glad"]);
    f.clock.set_str("2026-11-16T09:00:00Z");
    let p = f
        .engine
        .period_of(PeriodKind::Week, date("2026-11-10"))
        .unwrap();
    let recap = f.engine.compose_recap(&p, WrittenWhen::Overnight).unwrap();
    f.engine.save_recap(&recap).unwrap();
    assert!(f.engine.periods(PeriodKind::Week).unwrap()[0].written);
    let Some(ReflectionView::Recap { recap: back }) = f.engine.saved_reflection(&p).unwrap() else {
        panic!("a week is a Recap")
    };
    assert_eq!(back.checkins.len(), 2);
    f.engine.set_left_out(&c1, true).unwrap();
    let Some(ReflectionView::Recap { recap: hidden }) = f.engine.saved_reflection(&p).unwrap()
    else {
        panic!()
    };
    assert_eq!(hidden.checkins.len(), 1);
    assert_eq!(hidden.checkins[0].id, c2);
    assert_eq!(hidden.words.len(), 1);
    f.engine.set_left_out(&c1, false).unwrap();
    f.engine.delete_reflection(&p).unwrap();
    assert!(f.engine.saved_reflection(&p).unwrap().is_none());
}

#[test]
fn private_items_and_items_being_forgotten_feed_no_reflection_recap_or_glance() {
    let f = fixture();
    let plain = keep(&f.engine, "Posted the parcel");
    let private = keep(&f.engine, "My own private worry");
    f.engine.set_item_private(&private, true).unwrap();
    let forgetting = keep(&f.engine, "Being forgotten right now");
    let conn = rusqlite::Connection::open(f.dir.path().join("nocturne.db")).unwrap();
    conn.execute(
        "INSERT INTO forget_jobs(ids, items, include_originals) VALUES ('[]', ?1, 0)",
        [format!("[\"{forgetting}\"]")],
    )
    .unwrap();

    let day = day_period(&f.engine, "2026-11-13");
    let (_, input) = written(&f, "2026-11-13");
    let ids: Vec<&str> = input.items.iter().map(|i| i.id.as_str()).collect();
    assert_eq!(ids, [plain.as_str()]);

    let glance = f.engine.glance(&day).unwrap();
    let counted: Vec<&String> = glance.kept.iter().flat_map(|k| &k.item_ids).collect();
    assert_eq!(counted, [&plain]);

    let week = f
        .engine
        .period_of(PeriodKind::Week, date("2026-11-13"))
        .unwrap();
    let recap = f
        .engine
        .compose_recap(&week, WrittenWhen::OnDemand)
        .unwrap();
    let text = serde_json::to_string(&recap).unwrap();
    assert!(!text.contains(&private) && !text.contains(&forgetting));
    let glance = f.engine.glance(&week).unwrap();
    let counted: Vec<&String> = glance.kept.iter().flat_map(|k| &k.item_ids).collect();
    assert_eq!(counted, [&plain]);
}

#[test]
fn a_source_of_a_private_memory_is_left_out_of_what_a_reflection_is_written_from() {
    let f = fixture();
    let plain = keep(&f.engine, "Posted the parcel");
    let source = keep(&f.engine, "Priya told me about her diagnosis");
    let report = f
        .engine
        .apply(
            &source,
            vec![cand(
                "Priya",
                nocturne_engine::Predicate::Knows,
                "a diagnosis",
                "Priya has a diagnosis.",
            )],
        )
        .unwrap();
    let conn = rusqlite::Connection::open(f.dir.path().join("nocturne.db")).unwrap();
    conn.execute(
        "UPDATE memories SET private = 1 WHERE id = ?1",
        [&report.created[0]],
    )
    .unwrap();
    let (_, input) = written(&f, "2026-11-13");
    let ids: Vec<&str> = input.items.iter().map(|i| i.id.as_str()).collect();
    assert_eq!(ids, [plain.as_str()]);
}

#[test]
fn a_reflection_is_shown_a_summary_for_background_items_and_a_capped_text_for_the_rest() {
    let f = fixture();
    let long = "word ".repeat(600);
    let note = keep(&f.engine, &long);
    let file = write_file(
        f.dir.path(),
        "form.txt",
        b"The full text of the school form",
    );
    let doc = match f.engine.keep_file(&file, Source::WatchedFolder).unwrap() {
        Kept::New { item } | Kept::Duplicate { item } => item.id,
    };
    let conn = rusqlite::Connection::open(f.dir.path().join("nocturne.db")).unwrap();
    conn.execute(
        "UPDATE items SET text = 'The full text of the school form', \
         summary = 'A school form about a trip' WHERE id = ?1",
        [&doc],
    )
    .unwrap();
    let (_, input) = written(&f, "2026-11-13");
    let text_of = |id: &str| &input.items.iter().find(|i| i.id == id).unwrap().text;
    assert_eq!(text_of(&doc), "A school form about a trip");
    let n = text_of(&note);
    assert!(n.chars().count() <= 1200, "{}", n.chars().count());
    assert!(n.ends_with("word"), "cut at a word");
}

#[test]
fn a_save_that_is_stopped_stores_nothing_and_leaves_what_was_there() {
    let f = fixture_at("2026-11-12T10:00:00Z", "Europe/London");
    let note = keep(&f.engine, "A day with something in it");
    f.clock.set_str("2026-11-13T08:00:00Z");
    let (period, input) = written(&f, "2026-11-12");
    let at = input.items.iter().position(|i| i.id == note).unwrap();
    f.engine
        .save_day_reflection(
            &period,
            &input,
            &[sentence("You **kept something**.", &[at])],
            WrittenWhen::Overnight,
        )
        .unwrap();
    let before = f.engine.saved_reflection(&period).unwrap();

    let stopped = f.engine.save_day_reflection_unless(
        &period,
        &input,
        &[sentence("You **kept it twice**.", &[at])],
        WrittenWhen::OnDemand,
        &|| true,
    );
    assert!(matches!(stopped, Err(nocturne_engine::Error::Cancelled)));
    assert_eq!(f.engine.saved_reflection(&period).unwrap(), before);

    let week = f
        .engine
        .period_of(PeriodKind::Week, date("2026-11-09"))
        .unwrap();
    let recap = f
        .engine
        .compose_recap(&week, WrittenWhen::Overnight)
        .unwrap();
    assert!(matches!(
        f.engine.save_recap_unless(&recap, &|| true),
        Err(nocturne_engine::Error::Cancelled)
    ));
    assert!(f.engine.saved_reflection(&week).unwrap().is_none());
}

#[test]
fn reflections_due_lists_an_ended_day_once_and_not_a_written_one() {
    let f = fixture_at("2026-11-12T10:00:00Z", "Europe/London");
    let note = keep(&f.engine, "A day with something in it");
    checkin(&f, FeelingPart::Settled, &["calm"]);
    read_all(&f);
    f.clock.set_str("2026-11-12T20:00:00Z");
    assert!(
        f.engine.reflections_due(7).unwrap().is_empty(),
        "the day has not ended"
    );

    f.clock.set_str("2026-11-13T08:00:00Z");
    let due = f.engine.reflections_due(7).unwrap();
    assert_eq!(due, [day_period(&f.engine, "2026-11-12")]);
    assert_eq!(
        f.engine.reflections_due(7).unwrap(),
        due,
        "listing is not writing"
    );

    let (period, input) = written(&f, "2026-11-12");
    let at = input.items.iter().position(|i| i.id == note).unwrap();
    f.engine
        .save_day_reflection(
            &period,
            &input,
            &[sentence("You **kept something**.", &[at])],
            WrittenWhen::Overnight,
        )
        .unwrap();
    assert!(f.engine.reflections_due(7).unwrap().is_empty());

    f.clock.set_str("2026-11-30T08:00:00Z");
    assert!(
        f.engine.reflections_due(7).unwrap().is_empty(),
        "too far back"
    );
}

#[test]
fn a_so_far_reflection_is_still_due_once_its_day_has_ended() {
    let f = fixture_at("2026-11-12T10:00:00Z", "Europe/London");
    let note = keep(&f.engine, "A day with something in it");
    let (period, input) = written(&f, "2026-11-12");
    assert!(input.so_far);
    let at = input.items.iter().position(|i| i.id == note).unwrap();
    f.engine
        .save_day_reflection(
            &period,
            &input,
            &[sentence("You **kept something**.", &[at])],
            WrittenWhen::OnDemand,
        )
        .unwrap();
    assert!(f.engine.reflections_due(7).unwrap().is_empty());

    f.clock.set_str("2026-11-13T08:00:00Z");
    assert_eq!(
        f.engine.reflections_due(7).unwrap(),
        std::slice::from_ref(&period)
    );

    let (_, input) = written(&f, "2026-11-12");
    assert!(!input.so_far);
    f.engine
        .save_day_reflection(
            &period,
            &input,
            &[sentence("You **kept something**.", &[at])],
            WrittenWhen::Overnight,
        )
        .unwrap();
    assert!(f.engine.reflections_due(7).unwrap().is_empty());
}

#[test]
fn reflections_due_lists_ended_weeks_months_and_years_after_the_days_newest_first() {
    let f = fixture_at("2028-12-31T10:00:00Z", "Europe/London");
    keep(&f.engine, "Last day of the year");
    read_all(&f);
    f.clock.set_str("2029-01-01T10:00:00Z");
    let due = f.engine.reflections_due(7).unwrap();
    let kinds: Vec<PeriodKind> = due.iter().map(|p| p.kind).collect();
    assert_eq!(
        kinds,
        [
            PeriodKind::Day,
            PeriodKind::Week,
            PeriodKind::Month,
            PeriodKind::Year
        ]
    );
}

#[test]
fn export_writes_a_journal_day_file_beside_its_originals_and_the_reflections_as_text() {
    let f = fixture();
    let day = date("2026-11-13");
    let block = f
        .engine
        .save_entry_text(day, None, 0, "Gran came for tea")
        .unwrap()
        .unwrap();
    let Kept::New { item: photo } = f
        .engine
        .keep_bytes(b"jpeg-bytes", "jpg", Source::Journal)
        .unwrap()
    else {
        panic!("new")
    };
    f.engine.place_in_entry(&photo.id, day, 1).unwrap();
    f.engine.describe_photo(&photo.id, "The allotment").unwrap();
    f.engine
        .keep_checkin(
            CheckInInput {
                feeling: feeling(FeelingPart::Settled, &["calm", "content"], 0.6, -0.4),
                line: Some("A slow, kind day".into()),
                urge: None,
                replaces: None,
            },
            Source::Journal,
        )
        .unwrap();
    f.engine
        .keep_good_thing(day, "Sun on the table", None, Source::Journal)
        .unwrap();
    let (period, input) = written(&f, "2026-11-13");
    let at = input
        .items
        .iter()
        .position(|i| i.id == block.item.id)
        .unwrap();
    f.engine
        .save_day_reflection(
            &period,
            &input,
            &[sentence("**Gran came for tea**.", &[at])],
            WrittenWhen::OnDemand,
        )
        .unwrap();

    let out = tempfile::tempdir().unwrap();
    let report = f.engine.export_to(out.path()).unwrap();
    let root = std::path::PathBuf::from(&report.path);
    let text = std::fs::read_to_string(root.join("journal/2026-11-13.txt")).unwrap();
    assert!(text.starts_with("Friday 13 November 2026"), "{text}");
    assert!(text.contains("Gran came for tea"));
    assert!(text.contains("The allotment"));
    assert!(
        text.contains(&format!("../originals/{}-", photo.id)),
        "{text}"
    );
    assert!(
        text.contains("settled (pleasant 0.60, energy -0.40), words: calm, content"),
        "{text}"
    );
    assert!(text.contains("What was going on: A slow, kind day"));
    assert!(text.contains("- Sun on the table"));
    let reflection =
        std::fs::read_to_string(root.join("journal/reflections/day-2026-11-13.txt")).unwrap();
    assert!(reflection.contains("Gran came for tea"));
    assert!(reflection.contains("from: Gran came for tea (2026-11-13)"));
    let data: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("nocturne.json")).unwrap())
            .unwrap();
    assert_eq!(data["journal"]["reflections"].as_array().unwrap().len(), 1);
    assert!(data["journal"]["reflections"][0]["body"]["theDay"].is_array());
    assert!(data["journal"]["mentions"].is_array());
    assert!(root.join("originals").read_dir().unwrap().count() >= 1);
}

#[test]
fn deleting_everything_removes_the_journal_tables_too() {
    let f = fixture();
    let day = date("2026-11-13");
    let block = f
        .engine
        .save_entry_text(day, None, 0, "Gran came for tea")
        .unwrap()
        .unwrap();
    mention(&f, &block.item.id, "ent_gran", "Gran", true);
    let (period, input) = written(&f, "2026-11-13");
    f.engine
        .save_day_reflection(
            &period,
            &input,
            &[sentence("**Gran**.", &[0])],
            WrittenWhen::OnDemand,
        )
        .unwrap();
    f.engine
        .delete_everything(nocturne_engine::DELETE_PHRASE)
        .unwrap();
    for table in [
        "reflections",
        "reflection_cites",
        "mentions",
        "name_questions",
        "items",
    ] {
        assert_eq!(
            sql_count(f.dir.path(), &format!("SELECT COUNT(*) FROM {table}")),
            0,
            "{table}"
        );
    }
    assert_eq!(f.engine.evening_reminder().unwrap().time, "21:00");
}

#[test]
fn a_reflection_is_not_written_from_what_is_still_being_read() {
    let f = fixture();
    let day = date("2026-11-13");
    let read = keep(&f.engine, "Posted the parcel");
    f.engine.apply(&read, vec![]).unwrap();
    let held = f
        .engine
        .save_entry_text(day, None, 0, "Still writing this")
        .unwrap()
        .unwrap();
    let waiting = keep(&f.engine, "Not read yet");
    let input = f
        .engine
        .reflection_input(&day_period(&f.engine, "2026-11-13"))
        .unwrap();
    let ids: Vec<&str> = input.items.iter().map(|i| i.id.as_str()).collect();
    assert_eq!(ids, vec![read.as_str()]);
    assert!(!ids.contains(&held.item.id.as_str()) && !ids.contains(&waiting.as_str()));
}

fn photo_good_thing(f: &Fixture) -> (String, String) {
    let Kept::New { item: photo } = f
        .engine
        .keep_bytes(b"jpeg-of-the-gate", "jpg", Source::Phone)
        .unwrap()
    else {
        panic!("new")
    };
    let good = f
        .engine
        .keep_good_thing(
            f.engine.journal_today().unwrap(),
            "",
            Some(&photo.id),
            Source::Phone,
        )
        .unwrap();
    (photo.id, good.id)
}

#[test]
fn a_highlight_in_a_photos_own_words_cites_the_photo() {
    let f = fixture();
    let (photo, good) = photo_good_thing(&f);
    f.clock.set_str("2026-11-16T09:00:00Z");
    let p = f
        .engine
        .period_of(PeriodKind::Week, date("2026-11-13"))
        .unwrap();
    let recap = f.engine.compose_recap(&p, WrittenWhen::Overnight).unwrap();
    let h = recap
        .highlights
        .iter()
        .find(|h| h.photo.is_some())
        .expect("the good thing is a highlight");
    let cited: Vec<&str> = h.receipts.iter().map(|r| r.item_id.as_str()).collect();
    assert!(cited.contains(&good.as_str()) && cited.contains(&photo.as_str()));
}

#[test]
fn a_recap_saved_after_its_item_was_deleted_does_not_bring_it_back() {
    let f = fixture();
    let (_, good) = photo_good_thing(&f);
    let p = f
        .engine
        .period_of(PeriodKind::Week, date("2026-11-13"))
        .unwrap();
    let recap = f.engine.compose_recap(&p, WrittenWhen::OnDemand).unwrap();
    assert!(
        recap
            .highlights
            .iter()
            .any(|h| h.receipts.iter().any(|r| r.item_id == good))
    );
    f.engine.unkeep(&good).unwrap();
    f.engine.save_recap(&recap).unwrap();
    let Some(ReflectionView::Recap { recap: back }) = f.engine.saved_reflection(&p).unwrap() else {
        panic!("a week is a Recap")
    };
    assert!(
        back.highlights
            .iter()
            .all(|h| h.receipts.iter().all(|r| r.item_id != good))
    );
    assert!(back.good.iter().all(|g| g.id != good));
    let conn = rusqlite::Connection::open(f.dir.path().join("nocturne.db")).unwrap();
    let cites: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM reflection_cites WHERE item_id = ?1",
            [&good],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(cites, 0);
}

fn exists(f: &Fixture, id: &str) -> bool {
    sql_count(
        f.dir.path(),
        &format!("SELECT COUNT(*) FROM items WHERE id = '{id}'"),
    ) == 1
}

#[test]
fn a_check_in_alone_can_be_deleted() {
    let f = fixture();
    let c = checkin(&f, FeelingPart::Low, &["tired"]);
    let preview = f
        .engine
        .preview_delete(&[], true, std::slice::from_ref(&c), false)
        .unwrap();
    assert!(preview.memories.is_empty());
    assert_eq!(preview.originals.len(), 1);
    assert!(preview.originals[0].removable);
    let report = f.engine.forget(&preview.token).unwrap().report;
    assert_eq!(report.originals_removed, 1);
    assert!(!exists(&f, &c));
}

#[test]
fn deleting_a_day_takes_its_own_items_and_spares_what_was_unticked() {
    let f = fixture();
    let promise = || {
        vec![cand(
            "me",
            nocturne_engine::Predicate::Promised,
            "Priya the book",
            "You promised Priya the book.",
        )]
    };
    f.clock.set_str("2026-11-12T09:00:00Z");
    let tuesday = keep(&f.engine, "Promised Priya the book.");
    f.engine.apply(&tuesday, promise()).unwrap();
    f.clock.set_str("2026-11-13T09:00:00Z");
    let wednesday = keep(&f.engine, "Still owe Priya the book.");
    f.engine.apply(&wednesday, promise()).unwrap();
    let feeling = checkin(&f, FeelingPart::Low, &["tired"]);
    let shared: Vec<String> = f
        .engine
        .memories(nocturne_contract::MemoryFilter {
            item_id: Some(wednesday.clone()),
            ..Default::default()
        })
        .unwrap()
        .into_iter()
        .map(|m| m.id)
        .collect();
    assert_eq!(shared.len(), 1);

    let day = [wednesday.clone(), feeling.clone()];
    let preview = f.engine.preview_delete(&shared, true, &day, true).unwrap();
    let listed: Vec<&str> = preview
        .originals
        .iter()
        .map(|o| o.item_id.as_str())
        .collect();
    assert!(
        listed.contains(&tuesday.as_str()),
        "shown, so it can be ticked"
    );
    f.engine.forget(&preview.token).unwrap();
    assert!(!exists(&f, &wednesday) && !exists(&f, &feeling));
    assert!(exists(&f, &tuesday), "unticked, so it stays");
}
