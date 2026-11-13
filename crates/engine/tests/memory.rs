mod common;

use common::*;
use nocturne_contract::{Certainty, MemoryFilter, MemoryStatus, NeedsYouEntry, Source};
use nocturne_engine::Predicate;

fn all(f: &Fixture) -> Vec<nocturne_contract::MemoryView> {
    f.engine.memories(MemoryFilter::default()).unwrap()
}

#[test]
fn same_candidate_twice_reinforces_one_row() {
    let f = fixture();
    let c = || cand("Leo", Predicate::Prefers, "swimming", "Leo likes swimming.");
    let (_, id) = learn(&f.engine, "Leo likes swimming", c());
    let first = &all(&f)[0];
    assert_eq!(first.certainty, Certainty::FairlySure);
    assert_eq!(first.last_used, None);
    assert_eq!(first.receipts.len(), 1);

    f.clock.set_str("2026-11-14T09:00:00Z");
    let (_, id2) = learn(&f.engine, "Leo really likes swimming", c());
    assert_eq!(id, id2);
    let rows = all(&f);
    assert_eq!(rows.len(), 1, "one row");
    assert_eq!(rows[0].receipts.len(), 2, "source count 2");
    assert_eq!(rows[0].certainty, Certainty::Sure, "certainty rises");
    assert_eq!(
        rows[0].last_used.as_deref(),
        Some("2026-11-14T09:00:00.000Z")
    );
}

#[test]
fn applying_an_item_again_replaces_what_it_said_and_is_not_a_second_voice() {
    let f = fixture();
    let item = keep(&f.engine, "Gran lives in Leeds, the cat is called Pip");
    let leeds = cand("Gran", Predicate::LivesIn, "Leeds", "Gran lives in Leeds.");
    let pip = cand("Cat", Predicate::Knows, "Pip", "The cat is called Pip.");
    f.engine
        .apply_candidates(&item, vec![leeds.clone(), pip])
        .unwrap();
    assert_eq!(all(&f).len(), 2);

    f.clock.set_str("2026-11-14T09:00:00Z");
    let report = f.engine.apply_candidates(&item, vec![leeds]).unwrap();
    assert_eq!(report.removed.len(), 1, "what the item no longer says goes");
    let rows = all(&f);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].receipts.len(), 1);
    assert_eq!(rows[0].last_used, None, "re-reading is not reinforcement");
    assert_eq!(rows[0].certainty, Certainty::FairlySure);
}

#[test]
fn a_confirmed_memory_survives_its_item_being_reread() {
    let f = fixture();
    let (item, id) = learn(
        &f.engine,
        "Gran lives in Leeds",
        cand("Gran", Predicate::LivesIn, "Leeds", "Gran lives in Leeds."),
    );
    f.engine.confirm(&id, true).unwrap();
    f.engine.apply_candidates(&item, vec![]).unwrap();
    let rows = all(&f);
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].receipts.len(),
        1,
        "every memory keeps at least one source"
    );
}

#[test]
fn untrusted_sources_never_create_active_memories() {
    let f = fixture();
    let web = keep_as(&f.engine, "Clinic opens at 9 (web page)", Source::PastedWeb);
    let c = || {
        cand(
            "Clinic",
            Predicate::Knows,
            "opens at 9",
            "The clinic opens at 9.",
        )
    };
    let r = f.engine.apply_candidates(&web, vec![c()]).unwrap();
    assert_eq!(r.unconfirmed.len(), 1);
    let m = &all(&f)[0];
    assert_eq!(m.status, MemoryStatus::Unconfirmed);
    assert_eq!(m.certainty, Certainty::NotSure);
    assert!(m.receipts[0].unconfirmed);
    assert_eq!(
        f.engine.get_item(&web).unwrap().status,
        nocturne_contract::ItemStatus::Unconfirmed
    );
    let needs = f.engine.needs_you().unwrap();
    assert!(matches!(
        &needs[0],
        NeedsYouEntry::Unconfirmed {
            agreeing_sources: 1,
            ..
        }
    ));

    // A second untrusted voice does not promote it.
    let agent = keep_as(&f.engine, "agent says clinic opens at 9", Source::Agent);
    f.engine.apply_candidates(&agent, vec![c()]).unwrap();
    assert_eq!(all(&f)[0].status, MemoryStatus::Unconfirmed);

    // A trusted, independent item does.
    let note = keep(&f.engine, "I rang: the clinic opens at 9");
    let r = f.engine.apply_candidates(&note, vec![c()]).unwrap();
    assert_eq!(r.promoted.len(), 1);
    let m = &all(&f)[0];
    assert_eq!(m.status, MemoryStatus::Active);
    assert_eq!(m.receipts.len(), 3);
    assert!(f.engine.needs_you().unwrap().is_empty());
}

#[test]
fn a_trusted_item_from_the_same_origin_does_not_corroborate() {
    let f = fixture();
    let web = f
        .engine
        .keep_text_from("page text", Source::PastedWeb, Some("example.org"))
        .unwrap();
    let web = match web {
        nocturne_contract::Kept::New { item } => item.id,
        _ => unreachable!(),
    };
    let c = || {
        cand(
            "Clinic",
            Predicate::Knows,
            "opens at 9",
            "The clinic opens at 9.",
        )
    };
    f.engine.apply_candidates(&web, vec![c()]).unwrap();
    let mirrored = match f
        .engine
        .keep_text_from("copy of that page", Source::Paste, Some("example.org"))
        .unwrap()
    {
        nocturne_contract::Kept::New { item } => item.id,
        _ => unreachable!(),
    };
    f.engine.apply_candidates(&mirrored, vec![c()]).unwrap();
    assert_eq!(all(&f)[0].status, MemoryStatus::Unconfirmed);
}

#[test]
fn unknown_predicates_are_unconfirmed_until_the_person_confirms() {
    let f = fixture();
    let (_, id) = learn(
        &f.engine,
        "Leo is allergic to nothing",
        cand(
            "Leo",
            Predicate::Other("owns".into()),
            "a bike",
            "Leo owns a bike.",
        ),
    );
    assert_eq!(all(&f)[0].status, MemoryStatus::Unconfirmed);
    f.engine.confirm(&id, true).unwrap();
    let m = &all(&f)[0];
    assert_eq!(m.status, MemoryStatus::Active);
    assert_eq!(m.certainty, Certainty::Sure);
    f.engine.confirm(&id, false).unwrap();
    assert_eq!(all(&f)[0].status, MemoryStatus::Unconfirmed);
}

#[test]
fn scheduled_with_different_object_and_overlapping_validity_conflicts() {
    let f = fixture();
    let (_, a) = learn(
        &f.engine,
        "Dentist Tuesday 14:00",
        cand(
            "The dentist appointment",
            Predicate::Scheduled,
            "Tuesday 14:00",
            "Your dentist appointment is Tuesday at 14:00.",
        ),
    );
    let item_b = keep(&f.engine, "Dentist moved to Thursday 10:00");
    let r = f
        .engine
        .apply_candidates(
            &item_b,
            vec![cand(
                "dentist appointment",
                Predicate::Scheduled,
                "Thursday 10:00",
                "Your dentist appointment is Thursday at 10:00.",
            )],
        )
        .unwrap();
    assert_eq!(r.conflicts.len(), 1);
    assert_eq!(
        f.engine.get_item(&item_b).unwrap().status,
        nocturne_contract::ItemStatus::NeedsYou
    );
    let b = r.created[0].clone();

    let needs = f.engine.needs_you().unwrap();
    assert_eq!(needs.len(), 1);
    let NeedsYouEntry::Which {
        conflict_id,
        options,
        ..
    } = &needs[0]
    else {
        panic!("which")
    };
    assert_eq!(options.len(), 2);
    assert_eq!(options[0].memory_id, a);
    assert_eq!(options[1].memory_id, b);

    f.engine.resolve_which(conflict_id, &b).unwrap();
    assert!(f.engine.needs_you().unwrap().is_empty());
    let rows = all(&f);
    assert_eq!(rows.len(), 1, "the other is history, not a current memory");
    assert_eq!(rows[0].id, b);
    assert_eq!(rows[0].certainty, Certainty::Sure);
    assert_eq!(rows[0].was.as_deref(), Some("Tuesday 14:00"));
    assert!(
        f.engine.resolve_which(conflict_id, &b).is_err(),
        "decided once"
    );
}

#[test]
fn contradiction_detection_ignores_memories_whose_valid_time_does_not_overlap() {
    // Review focus 4: a past address is history, not a conflict.
    let f = fixture();
    let mut old = cand("Gran", Predicate::LivesIn, "Leeds", "Gran lived in Leeds.");
    old.valid_to = Some(date("2020-01-01"));
    learn(&f.engine, "Gran used to live in Leeds", old);
    let mut now = cand("Gran", Predicate::LivesIn, "York", "Gran lives in York.");
    now.valid_from = Some(date("2020-01-01"));
    let (_, york) = learn(&f.engine, "Gran lives in York", now);
    assert!(f.engine.needs_you().unwrap().is_empty());
    assert_eq!(all(&f).len(), 2);

    // The same fact with overlapping validity does conflict.
    let item = keep(&f.engine, "Gran lives in Hull");
    let r = f
        .engine
        .apply_candidates(
            &item,
            vec![cand(
                "Gran",
                Predicate::LivesIn,
                "Hull",
                "Gran lives in Hull.",
            )],
        )
        .unwrap();
    assert_eq!(r.conflicts.len(), 1);
    assert_eq!(
        r.conflicts.len(),
        1,
        "only the York row overlaps; Leeds ended in 2020"
    );
    let NeedsYouEntry::Which { options, .. } = &f.engine.needs_you().unwrap()[0] else {
        panic!()
    };
    assert_eq!(options[0].memory_id, york);
}

#[test]
fn a_real_world_change_closes_the_old_value_at_the_new_start() {
    let f = fixture();
    learn(
        &f.engine,
        "Gran lives in Leeds",
        cand("Gran", Predicate::LivesIn, "Leeds", "Gran lives in Leeds."),
    );
    let mut moved = cand("Gran", Predicate::LivesIn, "York", "Gran moved to York.");
    moved.valid_from = Some(date("2026-03-01"));
    let item = keep(&f.engine, "Gran moved to York in March");
    let r = f.engine.apply_candidates(&item, vec![moved]).unwrap();
    assert!(r.conflicts.is_empty());
    let rows = all(&f);
    let leeds = rows.iter().find(|m| m.object == "Leeds").unwrap();
    let york = rows.iter().find(|m| m.object == "York").unwrap();
    assert_eq!(leeds.valid_to.as_deref(), Some("2026-03-01"));
    assert_eq!(york.was.as_deref(), Some("Leeds"));
}

#[test]
fn only_single_valued_predicates_conflict() {
    let f = fixture();
    learn(
        &f.engine,
        "a",
        cand("Leo", Predicate::Knows, "Sam", "Leo knows Sam."),
    );
    let item = keep(&f.engine, "b");
    let r = f
        .engine
        .apply_candidates(
            &item,
            vec![cand("Leo", Predicate::Knows, "Maya", "Leo knows Maya.")],
        )
        .unwrap();
    assert!(r.conflicts.is_empty());

    // Preferences conflict only when their tags share a category.
    let mut tea = cand("Mum", Predicate::Prefers, "tea", "Mum prefers tea.");
    tea.tags = vec!["drink".into()];
    learn(&f.engine, "c", tea);
    let mut coffee = cand("Mum", Predicate::Prefers, "coffee", "Mum prefers coffee.");
    coffee.tags = vec!["drink".into()];
    let item = keep(&f.engine, "d");
    assert_eq!(
        f.engine
            .apply_candidates(&item, vec![coffee])
            .unwrap()
            .conflicts
            .len(),
        1
    );
    let mut blue = cand("Mum", Predicate::Prefers, "blue", "Mum prefers blue.");
    blue.tags = vec!["colour".into()];
    let item = keep(&f.engine, "e");
    assert!(
        f.engine
            .apply_candidates(&item, vec![blue])
            .unwrap()
            .conflicts
            .is_empty()
    );
}

#[test]
fn an_untrusted_disagreement_creates_unconfirmed_and_never_a_which() {
    let f = fixture();
    learn(
        &f.engine,
        "Gran lives in Leeds",
        cand("Gran", Predicate::LivesIn, "Leeds", "Gran lives in Leeds."),
    );
    let agent = keep_as(&f.engine, "gran lives in Hull", Source::Agent);
    let r = f
        .engine
        .apply_candidates(
            &agent,
            vec![cand(
                "Gran",
                Predicate::LivesIn,
                "Hull",
                "Gran lives in Hull.",
            )],
        )
        .unwrap();
    assert!(r.conflicts.is_empty());
    assert_eq!(r.unconfirmed.len(), 1);
    assert!(
        f.engine
            .needs_you()
            .unwrap()
            .iter()
            .all(|e| matches!(e, NeedsYouEntry::Unconfirmed { .. }))
    );
}

#[test]
fn correct_closes_the_old_row_into_history_and_cites_the_same_sources() {
    let f = fixture();
    let mut c = cand(
        "Leo",
        Predicate::Scheduled,
        "Wednesday",
        "Leo swims on Wednesday.",
    );
    c.valid_from = Some(date("2026-11-01"));
    let (item, old) = learn(&f.engine, "Leo swims on Wednesday", c);
    f.clock.set_str("2026-11-15T10:00:00Z");
    let new = f.engine.correct(&old, "Monday").unwrap();
    assert_ne!(new, old);

    let rows = all(&f);
    assert_eq!(rows.len(), 1, "current rows only");
    let m = &rows[0];
    assert_eq!(m.id, new);
    assert_eq!(m.sentence, "Leo swims on Monday.");
    assert_eq!(m.was.as_deref(), Some("Wednesday"));
    assert_eq!(m.certainty, Certainty::Sure);
    assert_eq!(m.receipts.len(), 1);
    assert_eq!(m.receipts[0].item_id, item);
    assert_eq!(m.valid_from.as_deref(), Some("2026-11-01"));

    // The old row is history: superseded, dates untouched (wrong, not ended), still has its source.
    assert_eq!(
        sql_count(
            f.dir.path(),
            "SELECT COUNT(*) FROM memories WHERE superseded_at IS NOT NULL"
        ),
        1
    );
    assert_eq!(
        sql_count(
            f.dir.path(),
            &format!(
                "SELECT COUNT(*) FROM memories WHERE id = '{old}' AND valid_to IS NULL AND superseded_by = '{new}'"
            )
        ),
        1
    );
    assert_eq!(
        sql_count(
            f.dir.path(),
            &format!("SELECT COUNT(*) FROM memory_sources WHERE memory_id = '{old}'")
        ),
        1
    );
    assert!(
        f.engine.correct(&old, "Tuesday").is_err(),
        "only current rows can be corrected"
    );

    // A re-read of the original does not resurrect the wrong value.
    let mut again = cand(
        "Leo",
        Predicate::Scheduled,
        "Wednesday",
        "Leo swims on Wednesday.",
    );
    again.valid_from = Some(date("2026-11-01"));
    let r = f.engine.apply_candidates(&item, vec![again]).unwrap();
    assert!(r.created.is_empty() && r.conflicts.is_empty());
    assert_eq!(all(&f).len(), 1);
}

#[test]
fn certainty_mapping() {
    let f = fixture();
    let mut low = cand("Pat", Predicate::Knows, "a rumour", "Pat knows a rumour.");
    low.confidence = 0.4;
    learn(&f.engine, "x", low);
    let (_, clear) = learn(
        &f.engine,
        "y",
        cand("Pat", Predicate::Decided, "to move", "Pat decided to move."),
    );
    let rows = all(&f);
    assert_eq!(
        rows.iter().find(|m| m.id == clear).unwrap().certainty,
        Certainty::FairlySure
    );
    assert_eq!(
        rows.iter()
            .find(|m| m.object == "a rumour")
            .unwrap()
            .certainty,
        Certainty::NotSure
    );
    f.engine.confirm(&clear, true).unwrap();
    assert_eq!(
        f.engine.get_memory(&clear).unwrap().certainty,
        Certainty::Sure
    );
}

#[test]
fn safety_memories_are_keep_forever_and_suggest_private() {
    let f = fixture();
    learn(
        &f.engine,
        "Leo takes metformin",
        cand(
            "Leo",
            Predicate::Takes,
            "metformin 500mg",
            "Leo takes metformin 500mg.",
        ),
    );
    let mut allergy = cand(
        "Leo",
        Predicate::Knows,
        "peanut allergy",
        "Leo is allergic to peanuts.",
    );
    allergy.tags = vec!["allergy".into()];
    learn(&f.engine, "Leo allergy", allergy);
    learn(
        &f.engine,
        "Leo likes jam",
        cand("Leo", Predicate::Prefers, "jam", "Leo likes jam."),
    );
    for m in all(&f) {
        let safety = m.object != "jam";
        assert_eq!(m.keep_forever, safety, "{}", m.sentence);
        assert_eq!(m.suggest_private, safety, "{}", m.sentence);
        assert!(!m.private, "never hidden silently");
    }
}

#[test]
fn a_new_dose_closes_the_old_one_and_the_same_dose_agrees() {
    let f = fixture();
    learn(
        &f.engine,
        "metformin 500",
        cand(
            "Leo",
            Predicate::Takes,
            "metformin 500mg",
            "Leo takes metformin 500mg.",
        ),
    );
    let mut up = cand(
        "Leo",
        Predicate::Takes,
        "metformin 850mg",
        "Leo takes metformin 850mg.",
    );
    up.valid_from = Some(date("2026-11-10"));
    learn(&f.engine, "metformin 850", up);
    let rows = all(&f);
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows.iter()
            .find(|m| m.object == "metformin 500mg")
            .unwrap()
            .valid_to
            .as_deref(),
        Some("2026-11-10")
    );
    assert!(f.engine.needs_you().unwrap().is_empty());
    // A different drug is independent.
    let item = keep(&f.engine, "aspirin");
    let r = f
        .engine
        .apply_candidates(
            &item,
            vec![cand(
                "Leo",
                Predicate::Takes,
                "aspirin",
                "Leo takes aspirin.",
            )],
        )
        .unwrap();
    assert!(r.conflicts.is_empty());
}

#[test]
fn memory_filters_and_search_text() {
    let f = fixture();
    learn(
        &f.engine,
        "n1",
        cand("Leo", Predicate::Prefers, "swimming", "Leo likes swimming."),
    );
    learn(
        &f.engine,
        "n2",
        cand("Gran", Predicate::LivesIn, "Leeds", "Gran lives in Leeds."),
    );
    let hits = f
        .engine
        .memories(MemoryFilter {
            query: Some("swim".into()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].subject, "Leo");
    let hits = f
        .engine
        .memories(MemoryFilter {
            limit: Some(1),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(hits.len(), 1);
}

#[test]
fn entities_group_memories() {
    let f = fixture();
    let mut c = cand("Leo", Predicate::Prefers, "swimming", "Leo likes swimming.");
    c.subject_kind = Some(nocturne_contract::EntityKind::Person);
    learn(&f.engine, "n1", c);
    // A later candidate for the same subject, with no kind given, joins the entity.
    learn(
        &f.engine,
        "n2",
        cand("leo", Predicate::Knows, "Sam", "Leo knows Sam."),
    );
    let people = f
        .engine
        .entities(Some(nocturne_contract::EntityKind::Person))
        .unwrap();
    assert_eq!(people.len(), 1);
    assert_eq!((people[0].memory_count, people[0].item_count), (2, 2));
    let (view, mems) = f.engine.entity(&people[0].id).unwrap();
    assert_eq!(view.name, "Leo");
    assert_eq!(mems.len(), 2);
    assert!(
        f.engine
            .entities(Some(nocturne_contract::EntityKind::Place))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn an_untrusted_reread_cannot_rewrite_a_memory_trusted_sources_support() {
    let f = fixture();
    let (_, id) = learn(
        &f.engine,
        "I rang: clinic opens at 9",
        cand(
            "Clinic",
            Predicate::Knows,
            "opens at 9",
            "The clinic opens at 9.",
        ),
    );
    let web = keep_as(&f.engine, "web page about the clinic", Source::PastedWeb);
    let agrees = cand(
        "Clinic",
        Predicate::Knows,
        "opens at 9",
        "The clinic opens at 9.",
    );
    f.engine.apply_candidates(&web, vec![agrees]).unwrap();

    // Read again, now claiming different wording, confidence and dates.
    let mut rewritten = cand(
        "Clinic",
        Predicate::Knows,
        "opens at 9",
        "Everything here is free.",
    );
    rewritten.confidence = 0.1;
    rewritten.valid_from = Some(date("2020-01-01"));
    rewritten.due = Some(date("2020-01-02"));
    f.engine.apply_candidates(&web, vec![rewritten]).unwrap();
    let m = f.engine.get_memory(&id).unwrap();
    assert_eq!(m.sentence, "The clinic opens at 9.");
    assert_eq!(
        m.certainty,
        Certainty::FairlySure,
        "the web source does not add certainty"
    );
    assert_eq!((m.valid_from, m.due), (None, None));
    assert_eq!(m.receipts.len(), 2);
}

#[test]
fn an_untrusted_reread_may_refine_an_unconfirmed_memory_it_alone_supports() {
    let f = fixture();
    let web = keep_as(&f.engine, "page", Source::PastedWeb);
    let r = f
        .engine
        .apply_candidates(
            &web,
            vec![cand(
                "Clinic",
                Predicate::Knows,
                "opens at 9",
                "The clinic opens at 9.",
            )],
        )
        .unwrap();
    f.engine
        .apply_candidates(
            &web,
            vec![cand(
                "Clinic",
                Predicate::Knows,
                "opens at 9",
                "The clinic opens at nine.",
            )],
        )
        .unwrap();
    let m = f.engine.get_memory(&r.created[0]).unwrap();
    assert_eq!(m.sentence, "The clinic opens at nine.");
    assert_eq!(m.status, MemoryStatus::Unconfirmed);
}

#[test]
fn a_date_only_match_never_promotes_an_unconfirmed_memory() {
    let f = fixture();
    let web = keep_as(&f.engine, "web: form due Friday", Source::PastedWeb);
    let mut a = cand(
        "Trip form",
        Predicate::Due,
        "Friday",
        "The trip form is due Friday.",
    );
    a.due = Some(date("2026-11-14"));
    let r = f.engine.apply_candidates(&web, vec![a]).unwrap();
    let unconfirmed = r.created[0].clone();

    // An untrusted candidate with the same date but different words: a new unconfirmed memory.
    let agent = keep_as(&f.engine, "agent: form due 14 Nov", Source::Agent);
    let mut b = cand(
        "Trip form",
        Predicate::Due,
        "14 Nov",
        "The trip form is due on 14 November.",
    );
    b.due = Some(date("2026-11-14"));
    let r = f.engine.apply_candidates(&agent, vec![b]).unwrap();
    assert_eq!(r.created.len(), 1);
    assert!(r.promoted.is_empty() && r.reinforced.is_empty());
    assert_eq!(
        f.engine.get_memory(&unconfirmed).unwrap().status,
        MemoryStatus::Unconfirmed
    );

    // A trusted item with the same date but different words does not promote it either.
    let note = keep(&f.engine, "my note: form due 14 Nov");
    let mut c = cand(
        "Trip form",
        Predicate::Due,
        "Fri 14th",
        "The trip form is due on Fri 14th.",
    );
    c.due = Some(date("2026-11-14"));
    let r = f.engine.apply_candidates(&note, vec![c]).unwrap();
    assert!(r.promoted.is_empty());
    assert_eq!(
        f.engine.get_memory(&unconfirmed).unwrap().status,
        MemoryStatus::Unconfirmed
    );
}
