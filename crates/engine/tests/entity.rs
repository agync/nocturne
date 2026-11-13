mod common;

use common::*;
use nocturne_contract::{
    ChangeKind, EntityKind, MemoryFilter, MemoryView, RefileTarget, RenameOutcome,
};
use nocturne_engine::{Candidate, Error, Predicate};

/// A claim about a named thing, so the engine files it under one.
fn thing(subject: &str, predicate: Predicate, object: &str, sentence: &str) -> Candidate {
    Candidate {
        subject_kind: Some(EntityKind::Thing),
        ..cand(subject, predicate, object, sentence)
    }
}

fn names(f: &Fixture) -> Vec<String> {
    f.engine
        .entities(None)
        .unwrap()
        .into_iter()
        .map(|e| e.name)
        .collect()
}

fn of(f: &Fixture, entity: &str) -> Vec<MemoryView> {
    f.engine
        .memories(MemoryFilter {
            entity_id: Some(entity.to_string()),
            ..Default::default()
        })
        .unwrap()
}

/// A thing the voice note got wrong: "Drinks" heard as "Trinks".
fn trinks(f: &Fixture) -> (String, String, String) {
    let (item, id) = learn(
        &f.engine,
        "Trinks are 5 pounds on Friday",
        thing(
            "Trinks",
            Predicate::Knows,
            "5 pounds on Friday",
            "Trinks are 5 pounds on Friday.",
        ),
    );
    let entity = f.engine.get_memory(&id).unwrap().entity_id.unwrap();
    (item, id, entity)
}

fn renamed(out: RenameOutcome) -> (nocturne_contract::EntityView, Option<String>) {
    match out {
        RenameOutcome::Renamed { entity, change_id } => (entity, change_id),
        other => panic!("expected a rename, got {other:?}"),
    }
}

#[test]
fn renaming_changes_the_name_everywhere_and_keeps_the_old_one() {
    let f = fixture();
    let (_, id, entity) = trinks(&f);
    let (view, change) = renamed(f.engine.rename_entity(&entity, "  Drinks ").unwrap());
    assert_eq!(view.name, "Drinks");
    assert_eq!(view.former_names, vec!["Trinks".to_string()]);
    assert!(change.is_some());
    assert_eq!(view.last_change.as_ref().unwrap().from, "Trinks");
    assert_eq!(view.last_change.as_ref().unwrap().kind, ChangeKind::Rename);

    let m = f.engine.get_memory(&id).unwrap();
    assert_eq!(m.subject, "Drinks");
    assert_eq!(m.sentence, "Drinks are 5 pounds on Friday.");
    assert_eq!(names(&f), ["Drinks"]);
    // Found by the new name and by the old one.
    for q in ["Drinks", "Trinks"] {
        assert_eq!(
            f.engine.entities_matching(None, Some(q)).unwrap().len(),
            1,
            "{q}"
        );
    }
    let found = f
        .engine
        .memories(MemoryFilter {
            query: Some("drinks".into()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(found.len(), 1, "search follows the new name");
    let old = f
        .engine
        .memories(MemoryFilter {
            query: Some("trinks".into()),
            ..Default::default()
        })
        .unwrap();
    assert!(old.is_empty());
}

#[test]
fn a_rename_is_reversible_and_only_the_newest_can_be_taken_back() {
    let f = fixture();
    let (_, id, entity) = trinks(&f);
    let (_, first) = renamed(f.engine.rename_entity(&entity, "Drinks").unwrap());
    let (view, second) = renamed(f.engine.rename_entity(&entity, "Cold drinks").unwrap());
    assert_eq!(view.former_names, ["Drinks", "Trinks"]);
    assert_eq!(
        f.engine.get_memory(&id).unwrap().sentence,
        "Cold drinks are 5 pounds on Friday."
    );

    // The older change is not offered while a newer one stands.
    assert_eq!(view.last_change.unwrap().id, second.clone().unwrap());
    f.engine.undo_change(&second.unwrap()).unwrap();
    let (view, _) = f.engine.entity(&entity).unwrap();
    assert_eq!(view.name, "Drinks");
    assert_eq!(view.last_change.unwrap().id, first.clone().unwrap());
    f.engine.undo_change(&first.unwrap()).unwrap();
    let (view, _) = f.engine.entity(&entity).unwrap();
    assert_eq!(view.name, "Trinks");
    assert_eq!(
        f.engine.get_memory(&id).unwrap().sentence,
        "Trinks are 5 pounds on Friday."
    );
    assert!(view.last_change.is_none());
    assert!(matches!(
        f.engine.undo_change("chg_nope"),
        Err(Error::NotFound(_))
    ));
}

#[test]
fn a_change_cannot_be_taken_back_twice_or_after_a_later_rename() {
    let f = fixture();
    let (_, _, entity) = trinks(&f);
    let (_, first) = renamed(f.engine.rename_entity(&entity, "Drinks").unwrap());
    let first = first.unwrap();
    f.engine.rename_entity(&entity, "Cold drinks").unwrap();
    assert!(matches!(
        f.engine.undo_change(&first),
        Err(Error::Invalid(_))
    ));
}

#[test]
fn a_name_another_thing_has_offers_a_merge_and_changes_nothing() {
    let f = fixture();
    let (_, _, trinks_id) = trinks(&f);
    let (_, drinks_mem) = learn(
        &f.engine,
        "Drinks are in the fridge",
        thing(
            "Drinks",
            Predicate::Knows,
            "in the fridge",
            "Drinks are in the fridge.",
        ),
    );
    let drinks_id = f.engine.get_memory(&drinks_mem).unwrap().entity_id.unwrap();
    match f.engine.rename_entity(&trinks_id, "drinks").unwrap() {
        RenameOutcome::Exists { entity } => assert_eq!(entity.id, drinks_id),
        other => panic!("{other:?}"),
    }
    assert_eq!(f.engine.entity(&trinks_id).unwrap().0.name, "Trinks");
}

#[test]
fn names_are_checked() {
    let f = fixture();
    let (_, _, entity) = trinks(&f);
    for bad in ["", "   ", "...", &"x".repeat(121)] {
        assert!(
            matches!(f.engine.rename_entity(&entity, bad), Err(Error::Invalid(_))),
            "{bad:?}"
        );
    }
    assert!(matches!(
        f.engine.rename_entity("ent_nope", "Drinks"),
        Err(Error::NotFound(_))
    ));
    // The same name again changes nothing and logs nothing.
    let (view, change) = renamed(f.engine.rename_entity(&entity, "Trinks").unwrap());
    assert!(change.is_none() && view.last_change.is_none());
    // A different case is a rename of how it is written.
    let (view, _) = renamed(f.engine.rename_entity(&entity, "TRINKS").unwrap());
    assert_eq!(view.name, "TRINKS");
}

/// The overnight tidy reads what is unread, and a re-read says what the voice note said:
/// "Trinks". It must file that under what the person chose.
#[test]
fn a_reread_respects_a_name_the_person_set() {
    let f = fixture();
    let (item, id, entity) = trinks(&f);
    f.engine.rename_entity(&entity, "Drinks").unwrap();

    let again = thing(
        "Trinks",
        Predicate::Knows,
        "5 pounds on Friday",
        "Trinks are 5 pounds on Friday.",
    );
    let report = f.engine.apply(&item, vec![again]).unwrap();
    assert!(report.created.is_empty(), "no second memory: {report:?}");
    assert!(report.removed.is_empty(), "nothing was dropped: {report:?}");
    assert_eq!(names(&f), ["Drinks"], "no new thing named Trinks");
    let rows = of(&f, &entity);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, id);
    assert_eq!(rows[0].sentence, "Drinks are 5 pounds on Friday.");

    // A new claim about "Trinks" from a later note files under the thing, in its words too.
    let (_, other) = learn(
        &f.engine,
        "Trinks run out on Sunday",
        thing(
            "Trinks",
            Predicate::Knows,
            "run out on Sunday",
            "Trinks run out on Sunday.",
        ),
    );
    let m = f.engine.get_memory(&other).unwrap();
    assert_eq!(m.entity_id.as_deref(), Some(entity.as_str()));
    assert_eq!(m.subject, "Drinks");
    assert_eq!(m.sentence, "Drinks run out on Sunday.");
    assert_eq!(names(&f), ["Drinks"]);
}

#[test]
fn a_reread_does_not_bring_back_a_forgotten_claim_under_an_old_name() {
    let f = fixture();
    let (item, id, entity) = trinks(&f);
    f.engine.rename_entity(&entity, "Drinks").unwrap();
    let preview = f.engine.preview_forget(&[id], false).unwrap();
    f.engine.forget(&preview.token).unwrap();
    let again = thing(
        "Trinks",
        Predicate::Knows,
        "5 pounds on Friday",
        "Trinks are 5 pounds on Friday.",
    );
    // The forget moved the item on; a read that began after it may write, and must not
    // write what was forgotten.
    let _ = f.engine.apply(&item, vec![again]);
    assert!(
        f.engine
            .memories(MemoryFilter::default())
            .unwrap()
            .is_empty()
    );
}

fn two_things(f: &Fixture) -> ((String, String), (String, String)) {
    let (_, a) = learn(
        &f.engine,
        "Trinks are 5 pounds on Friday",
        thing(
            "Trinks",
            Predicate::Knows,
            "5 pounds on Friday",
            "Trinks are 5 pounds on Friday.",
        ),
    );
    let (_, b) = learn(
        &f.engine,
        "Drinks are in the fridge",
        thing(
            "Drinks",
            Predicate::Knows,
            "in the fridge",
            "Drinks are in the fridge.",
        ),
    );
    let ea = f.engine.get_memory(&a).unwrap().entity_id.unwrap();
    let eb = f.engine.get_memory(&b).unwrap().entity_id.unwrap();
    ((a, ea), (b, eb))
}

#[test]
fn merging_moves_everything_and_can_be_taken_back() {
    let f = fixture();
    let ((a, ea), (b, eb)) = two_things(&f);
    let out = f.engine.merge_entities(&ea, &eb).unwrap();
    assert_eq!(out.moved, 1);
    assert_eq!(out.entity.id, eb);
    assert_eq!(out.entity.memory_count, 2);
    assert!(out.entity.former_names.contains(&"Trinks".to_string()));
    assert_eq!(
        out.entity.last_change.as_ref().unwrap().kind,
        ChangeKind::Merge
    );
    assert_eq!(out.entity.last_change.as_ref().unwrap().from, "Trinks");
    assert_eq!(names(&f), ["Drinks"]);
    let moved = f.engine.get_memory(&a).unwrap();
    assert_eq!(moved.subject, "Drinks");
    assert_eq!(moved.sentence, "Drinks are 5 pounds on Friday.");
    assert_eq!(moved.entity_id.as_deref(), Some(eb.as_str()));
    assert!(matches!(f.engine.entity(&ea), Err(Error::NotFound(_))));

    // The name of what was merged in finds the one that remains.
    let found = f.engine.entities_matching(None, Some("trinks")).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].id, eb);

    f.engine.undo_change(&out.change_id).unwrap();
    assert_eq!(names(&f).len(), 2);
    let back = f.engine.get_memory(&a).unwrap();
    assert_eq!(back.entity_id.as_deref(), Some(ea.as_str()));
    assert_eq!(back.subject, "Trinks");
    assert_eq!(back.sentence, "Trinks are 5 pounds on Friday.");
    assert_eq!(
        f.engine.get_memory(&b).unwrap().entity_id.as_deref(),
        Some(eb.as_str())
    );
    let (view, _) = f.engine.entity(&eb).unwrap();
    assert!(view.former_names.is_empty() && view.last_change.is_none());
    assert!(matches!(
        f.engine.undo_change(&out.change_id),
        Err(Error::Invalid(_))
    ));
}

#[test]
fn merging_keeps_a_claim_both_held_once_with_both_receipts() {
    let f = fixture();
    let (_, a) = learn(
        &f.engine,
        "Trinks are in the fridge",
        thing(
            "Trinks",
            Predicate::Knows,
            "in the fridge",
            "Trinks are in the fridge.",
        ),
    );
    let (_, b) = learn(
        &f.engine,
        "Drinks are in the fridge",
        thing(
            "Drinks",
            Predicate::Knows,
            "in the fridge",
            "Drinks are in the fridge.",
        ),
    );
    let ea = f.engine.get_memory(&a).unwrap().entity_id.unwrap();
    let eb = f.engine.get_memory(&b).unwrap().entity_id.unwrap();
    let out = f.engine.merge_entities(&ea, &eb).unwrap();
    assert_eq!(out.entity.memory_count, 1);
    let rows = of(&f, &eb);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, b);
    assert_eq!(rows[0].receipts.len(), 2);
    assert!(rows[0].was.is_none(), "folding is not a change of value");

    f.engine.undo_change(&out.change_id).unwrap();
    assert_eq!(of(&f, &ea).len(), 1);
    assert_eq!(of(&f, &eb).len(), 1);
    assert_eq!(f.engine.get_memory(&b).unwrap().receipts.len(), 1);
    assert_eq!(f.engine.get_memory(&a).unwrap().receipts.len(), 1);
}

#[test]
fn a_reread_after_a_merge_files_under_the_one_that_remains() {
    let f = fixture();
    let ((a, ea), (_, eb)) = two_things(&f);
    let item = f.engine.get_memory(&a).unwrap().receipts[0].item_id.clone();
    f.engine.merge_entities(&ea, &eb).unwrap();
    let report = f
        .engine
        .apply(
            &item,
            vec![thing(
                "Trinks",
                Predicate::Knows,
                "5 pounds on Friday",
                "Trinks are 5 pounds on Friday.",
            )],
        )
        .unwrap();
    assert!(
        report.created.is_empty() && report.removed.is_empty(),
        "{report:?}"
    );
    assert_eq!(names(&f), ["Drinks"]);
    assert_eq!(
        f.engine.get_memory(&a).unwrap().entity_id.as_deref(),
        Some(eb.as_str())
    );
}

#[test]
fn merging_needs_two_different_live_things() {
    let f = fixture();
    let ((_, ea), (_, eb)) = two_things(&f);
    assert!(matches!(
        f.engine.merge_entities(&ea, &ea),
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        f.engine.merge_entities(&ea, "ent_nope"),
        Err(Error::NotFound(_))
    ));
    f.engine.merge_entities(&ea, &eb).unwrap();
    assert!(matches!(
        f.engine.merge_entities(&ea, &eb),
        Err(Error::NotFound(_))
    ));
}

#[test]
fn a_merge_is_not_offered_again_once_a_later_rename_stands() {
    let f = fixture();
    let ((_, ea), (_, eb)) = two_things(&f);
    let out = f.engine.merge_entities(&ea, &eb).unwrap();
    let (view, rename) = renamed(f.engine.rename_entity(&eb, "Fizzy drinks").unwrap());
    assert_eq!(view.last_change.unwrap().kind, ChangeKind::Rename);
    f.engine.undo_change(&rename.unwrap()).unwrap();
    // Newest first: the merge is the newest again.
    let (view, _) = f.engine.entity(&eb).unwrap();
    assert_eq!(view.last_change.unwrap().id, out.change_id);
}

#[test]
fn re_filing_moves_one_memory_and_keeps_where_it_came_from() {
    let f = fixture();
    let ((a, ea), (b, eb)) = two_things(&f);
    let out = f
        .engine
        .refile_memory(
            &a,
            RefileTarget::Existing {
                entity_id: eb.clone(),
            },
        )
        .unwrap();
    assert_eq!(out.memory.entity_id.as_deref(), Some(eb.as_str()));
    assert_eq!(out.memory.subject, "Drinks");
    assert_eq!(out.memory.moved_from.as_deref(), Some("Trinks"));
    assert_eq!(
        out.memory.sentence, "Trinks are 5 pounds on Friday.",
        "its words are the person's"
    );
    assert_eq!(out.entity.memory_count, 2);
    assert_eq!(
        f.engine.get_memory(&b).unwrap().entity_id.as_deref(),
        Some(eb.as_str())
    );
    assert_eq!(names(&f), ["Drinks"], "nothing is left under Trinks");
    assert!(matches!(f.engine.entity(&ea).map(|e| e.1.len()), Ok(0)));

    f.engine.undo_change(&out.change_id).unwrap();
    let back = f.engine.get_memory(&a).unwrap();
    assert_eq!(back.entity_id.as_deref(), Some(ea.as_str()));
    assert_eq!(back.subject, "Trinks");
    assert!(back.moved_from.is_none());
    assert_eq!(names(&f).len(), 2);
}

#[test]
fn re_filing_to_a_new_thing_makes_it_and_undo_removes_it() {
    let f = fixture();
    let (_, id, entity) = trinks(&f);
    let out = f
        .engine
        .refile_memory(
            &id,
            RefileTarget::New {
                name: " Beer  garden ".into(),
                kind: EntityKind::Place,
            },
        )
        .unwrap();
    assert_eq!(out.entity.name, "Beer garden");
    assert_eq!(out.entity.kind, EntityKind::Place);
    assert_ne!(out.entity.id, entity);
    assert_eq!(names(&f), ["Beer garden"]);

    // A name something already has files under that thing instead of making a second.
    let (_, id2) = learn(
        &f.engine,
        "Trinks again",
        thing("Trinks", Predicate::Knows, "cold", "Trinks are cold."),
    );
    let again = f
        .engine
        .refile_memory(
            &id2,
            RefileTarget::New {
                name: "beer garden".into(),
                kind: EntityKind::Thing,
            },
        )
        .unwrap();
    assert_eq!(again.entity.id, out.entity.id);

    f.engine.undo_change(&again.change_id).unwrap();
    f.engine.undo_change(&out.change_id).unwrap();
    assert_eq!(names(&f), ["Trinks"]);
    assert!(matches!(
        f.engine.entity(&out.entity.id),
        Err(Error::NotFound(_))
    ));
}

#[test]
fn re_filing_is_checked() {
    let f = fixture();
    let ((a, ea), _) = two_things(&f);
    assert!(matches!(
        f.engine
            .refile_memory(&a, RefileTarget::Existing { entity_id: ea }),
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        f.engine.refile_memory(
            &a,
            RefileTarget::Existing {
                entity_id: "ent_nope".into()
            }
        ),
        Err(Error::NotFound(_))
    ));
    assert!(matches!(
        f.engine.refile_memory(
            "mem_nope",
            RefileTarget::New {
                name: "X".into(),
                kind: EntityKind::Thing
            }
        ),
        Err(Error::NotFound(_))
    ));
    assert!(matches!(
        f.engine.refile_memory(
            &a,
            RefileTarget::New {
                name: " ".into(),
                kind: EntityKind::Thing
            }
        ),
        Err(Error::Invalid(_))
    ));
}

#[test]
fn a_reread_finds_a_memory_the_person_moved() {
    let f = fixture();
    let ((a, _), (_, eb)) = two_things(&f);
    let item = f.engine.get_memory(&a).unwrap().receipts[0].item_id.clone();
    f.engine
        .refile_memory(
            &a,
            RefileTarget::Existing {
                entity_id: eb.clone(),
            },
        )
        .unwrap();
    let report = f
        .engine
        .apply(
            &item,
            vec![thing(
                "Trinks",
                Predicate::Knows,
                "5 pounds on Friday",
                "Trinks are 5 pounds on Friday.",
            )],
        )
        .unwrap();
    assert!(report.created.is_empty(), "{report:?}");
    assert!(report.removed.is_empty(), "{report:?}");
    assert_eq!(names(&f), ["Drinks"], "no Trinks again");
    let m = f.engine.get_memory(&a).unwrap();
    assert_eq!(m.entity_id.as_deref(), Some(eb.as_str()));
    assert_eq!(m.receipts.len(), 1);
}

#[test]
fn renaming_where_a_memory_was_moved_keeps_it_findable() {
    let f = fixture();
    let ((a, _), (_, eb)) = two_things(&f);
    let item = f.engine.get_memory(&a).unwrap().receipts[0].item_id.clone();
    f.engine
        .refile_memory(
            &a,
            RefileTarget::Existing {
                entity_id: eb.clone(),
            },
        )
        .unwrap();
    f.engine.rename_entity(&eb, "Cold drinks").unwrap();
    let report = f
        .engine
        .apply(
            &item,
            vec![thing(
                "Trinks",
                Predicate::Knows,
                "5 pounds on Friday",
                "Trinks are 5 pounds on Friday.",
            )],
        )
        .unwrap();
    assert!(
        report.created.is_empty() && report.removed.is_empty(),
        "{report:?}"
    );
    assert_eq!(f.engine.get_memory(&a).unwrap().subject, "Cold drinks");
}

#[test]
fn a_forget_drops_what_can_no_longer_be_taken_back() {
    let f = fixture();
    let ((a, ea), (b, eb)) = two_things(&f);
    let out = f.engine.merge_entities(&ea, &eb).unwrap();
    let preview = f.engine.preview_forget(&[a, b], false).unwrap();
    f.engine.forget(&preview.token).unwrap();
    assert!(matches!(
        f.engine.undo_change(&out.change_id),
        Err(Error::NotFound(_) | Error::Invalid(_))
    ));
    assert_eq!(
        sql_count(f.dir.path(), "SELECT COUNT(*) FROM entities"),
        0,
        "no trace of the names is left"
    );
    assert_eq!(
        sql_count(f.dir.path(), "SELECT COUNT(*) FROM entity_names"),
        0
    );
}

fn db(f: &Fixture) -> rusqlite::Connection {
    rusqlite::Connection::open(f.dir.path().join("nocturne.db")).unwrap()
}

fn new_claim(f: &Fixture, text: &str, c: Candidate) -> nocturne_engine::ApplyReport {
    let item = keep(&f.engine, text);
    f.engine.apply(&item, vec![c]).unwrap()
}

#[test]
fn a_moved_memory_answers_to_its_old_subject_only_for_the_item_that_said_it() {
    let f = fixture();
    let (_, m) = learn(
        &f.engine,
        "Trinks live in Leeds",
        thing(
            "Trinks",
            Predicate::LivesIn,
            "Leeds",
            "Trinks live in Leeds.",
        ),
    );
    let (_, other) = learn(
        &f.engine,
        "Drinks are in the fridge",
        thing(
            "Drinks",
            Predicate::Knows,
            "in the fridge",
            "Drinks are in the fridge.",
        ),
    );
    let eb = f.engine.get_memory(&other).unwrap().entity_id.unwrap();
    f.engine
        .refile_memory(&m, RefileTarget::Existing { entity_id: eb })
        .unwrap();

    // A different item that says something else about "Trinks" is about Trinks: the moved
    // memory is not closed, put in conflict or changed by it.
    let report = new_claim(
        &f,
        "Trinks live in York now",
        thing("Trinks", Predicate::LivesIn, "York", "Trinks live in York."),
    );
    assert!(report.conflicts.is_empty(), "{report:?}");
    assert_eq!(report.created.len(), 1);
    let kept = f.engine.get_memory(&m).unwrap();
    assert_eq!(kept.valid_to, None);
    assert_eq!(kept.subject, "Drinks");
    let york = f.engine.get_memory(&report.created[0]).unwrap();
    assert_eq!(york.subject, "Trinks");
    assert_ne!(york.entity_id, kept.entity_id);

    // The same words from another item are a second voice for Trinks, not for the moved one.
    let report = new_claim(
        &f,
        "Trinks live in Leeds, I think",
        thing(
            "Trinks",
            Predicate::LivesIn,
            "Leeds",
            "Trinks live in Leeds.",
        ),
    );
    assert!(report.reinforced.is_empty(), "{report:?}");
    assert_eq!(f.engine.get_memory(&m).unwrap().receipts.len(), 1);
}

#[test]
fn a_correction_of_a_moved_memory_does_not_block_the_old_subject() {
    let f = fixture();
    let (_, m) = learn(
        &f.engine,
        "Trinks live in Leeds",
        thing(
            "Trinks",
            Predicate::LivesIn,
            "Leeds",
            "Trinks live in Leeds.",
        ),
    );
    let (_, other) = learn(
        &f.engine,
        "Drinks are in the fridge",
        thing(
            "Drinks",
            Predicate::Knows,
            "in the fridge",
            "Drinks are in the fridge.",
        ),
    );
    let eb = f.engine.get_memory(&other).unwrap().entity_id.unwrap();
    f.engine
        .refile_memory(&m, RefileTarget::Existing { entity_id: eb })
        .unwrap();
    f.engine
        .correct(&m, nocturne_engine::CorrectPatch::object("Hull"))
        .unwrap();
    let report = new_claim(
        &f,
        "Trinks live in Leeds again",
        thing(
            "Trinks",
            Predicate::LivesIn,
            "Leeds",
            "Trinks live in Leeds.",
        ),
    );
    assert_eq!(report.created.len(), 1, "{report:?}");
}

#[test]
fn delete_everything_leaves_no_row_of_filing_behind() {
    let f = fixture();
    let ((a, ea), (_, eb)) = two_things(&f);
    f.engine.rename_entity(&eb, "Pop").unwrap();
    f.engine.merge_entities(&ea, &eb).unwrap();
    f.engine
        .refile_memory(
            &a,
            RefileTarget::New {
                name: "Cellar".into(),
                kind: EntityKind::Place,
            },
        )
        .unwrap();
    assert!(sql_count(f.dir.path(), "SELECT COUNT(*) FROM entity_changes") > 0);

    f.engine
        .delete_everything(nocturne_engine::DELETE_PHRASE)
        .unwrap();
    let conn = db(&f);
    let tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table'")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    for table in tables {
        // The full-text indexes keep their own bookkeeping, and the Away from home state is
        // put back as its one empty row.
        if table.starts_with("sqlite_") || table.contains("_fts") || table == "remote_state" {
            continue;
        }
        let n: i64 = conn
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0, "{table} still has rows");
    }
}

#[test]
fn a_merge_survives_a_forget_that_has_nothing_to_do_with_it() {
    let f = fixture();
    let ((a, ea), (_, eb)) = two_things(&f);
    let out = f.engine.merge_entities(&ea, &eb).unwrap();
    let (_, unrelated) = learn(
        &f.engine,
        "Gran lives in Leeds",
        Candidate {
            subject_kind: Some(EntityKind::Person),
            ..cand("Gran", Predicate::LivesIn, "Leeds", "Gran lives in Leeds.")
        },
    );
    let preview = f.engine.preview_forget(&[unrelated], false).unwrap();
    f.engine.forget(&preview.token).unwrap();

    assert_eq!(
        sql_count(f.dir.path(), "SELECT COUNT(*) FROM entities"),
        2,
        "the hidden one stays"
    );
    assert_eq!(
        f.engine.entity(&eb).unwrap().0.last_change.unwrap().id,
        out.change_id
    );
    f.engine.undo_change(&out.change_id).unwrap();
    assert_eq!(
        f.engine.get_memory(&a).unwrap().entity_id.as_deref(),
        Some(ea.as_str())
    );
}

#[test]
fn a_merge_goes_when_what_it_was_merged_into_is_forgotten_entirely() {
    let f = fixture();
    let ((a, ea), (b, eb)) = two_things(&f);
    let out = f.engine.merge_entities(&ea, &eb).unwrap();
    let preview = f.engine.preview_forget(&[a, b], false).unwrap();
    f.engine.forget(&preview.token).unwrap();
    assert_eq!(sql_count(f.dir.path(), "SELECT COUNT(*) FROM entities"), 0);
    assert!(f.engine.undo_change(&out.change_id).is_err());
}

#[test]
fn a_claim_held_as_private_or_kept_forever_is_not_folded_into_one_that_is_not() {
    for private in [true, false] {
        let f = fixture();
        let (_, a) = learn(
            &f.engine,
            "Trinks are in the fridge",
            thing(
                "Trinks",
                Predicate::Knows,
                "in the fridge",
                "Trinks are in the fridge.",
            ),
        );
        let (_, b) = learn(
            &f.engine,
            "Drinks are in the fridge",
            thing(
                "Drinks",
                Predicate::Knows,
                "in the fridge",
                "Drinks are in the fridge.",
            ),
        );
        if private {
            f.engine.set_private(&a, true).unwrap();
        } else {
            f.engine.set_keep_forever(&a, true).unwrap();
        }
        let ea = f.engine.get_memory(&a).unwrap().entity_id.unwrap();
        let eb = f.engine.get_memory(&b).unwrap().entity_id.unwrap();
        let out = f.engine.merge_entities(&ea, &eb).unwrap();
        assert_eq!(out.entity.memory_count, 2, "both stay (private: {private})");
        assert_eq!(f.engine.get_memory(&b).unwrap().receipts.len(), 1);
        assert_eq!(f.engine.get_memory(&a).unwrap().private, private);
    }
}

#[test]
fn an_undo_that_cannot_finish_says_so_and_changes_nothing() {
    let f = fixture();
    let ((a, _), (_, eb)) = two_things(&f);
    let first = f
        .engine
        .refile_memory(
            &a,
            RefileTarget::Existing {
                entity_id: eb.clone(),
            },
        )
        .unwrap();
    f.engine
        .refile_memory(
            &a,
            RefileTarget::New {
                name: "Cellar".into(),
                kind: EntityKind::Place,
            },
        )
        .unwrap();
    assert!(matches!(
        f.engine.undo_change(&first.change_id),
        Err(Error::Invalid(_))
    ));
    assert_eq!(f.engine.get_memory(&a).unwrap().subject, "Cellar");

    // A merge whose memory has gone elsewhere by some other road is not half undone.
    let f = fixture();
    let ((a, ea), (_, eb)) = two_things(&f);
    let out = f.engine.merge_entities(&ea, &eb).unwrap();
    let conn = db(&f);
    conn.execute(
        "INSERT INTO entities(id, kind, name, key) VALUES ('ent_elsewhere', 'thing', 'Elsewhere', 'thing:elsewhere')",
        [],
    )
    .unwrap();
    conn.execute(
        "UPDATE memories SET entity_id = 'ent_elsewhere' WHERE id = ?1",
        [&a],
    )
    .unwrap();
    assert!(f.engine.undo_change(&out.change_id).is_err());
    let hidden = sql_count(
        f.dir.path(),
        "SELECT COUNT(*) FROM entities WHERE merged_into IS NOT NULL",
    );
    assert_eq!(hidden, 1, "still merged");
}

#[test]
fn what_a_read_writes_for_a_renamed_name_goes_back_with_the_rename() {
    let f = fixture();
    let (_, _, entity) = trinks(&f);
    let (_, change) = renamed(f.engine.rename_entity(&entity, "Drinks").unwrap());
    let report = new_claim(
        &f,
        "Trinks run out on Sunday",
        thing(
            "Trinks",
            Predicate::Knows,
            "run out on Sunday",
            "Trinks run out on Sunday.",
        ),
    );
    let made = f.engine.get_memory(&report.created[0]).unwrap();
    assert_eq!(made.sentence, "Drinks run out on Sunday.");

    f.engine.undo_change(&change.unwrap()).unwrap();
    let made = f.engine.get_memory(&report.created[0]).unwrap();
    assert_eq!(made.subject, "Trinks");
    assert_eq!(made.sentence, "Trinks run out on Sunday.");
    assert_eq!(names(&f), ["Trinks"]);
}

#[test]
fn what_a_read_writes_for_a_merged_in_name_goes_back_with_the_merge() {
    let f = fixture();
    let ((_, ea), (_, eb)) = two_things(&f);
    let out = f.engine.merge_entities(&ea, &eb).unwrap();
    let report = new_claim(
        &f,
        "Trinks run out on Sunday",
        thing(
            "Trinks",
            Predicate::Knows,
            "run out on Sunday",
            "Trinks run out on Sunday.",
        ),
    );
    let id = report.created[0].clone();
    assert_eq!(
        f.engine.get_memory(&id).unwrap().entity_id.as_deref(),
        Some(eb.as_str())
    );

    f.engine.undo_change(&out.change_id).unwrap();
    let back = f.engine.get_memory(&id).unwrap();
    assert_eq!(back.entity_id.as_deref(), Some(ea.as_str()));
    assert_eq!(back.sentence, "Trinks run out on Sunday.");
}

#[test]
fn a_merge_cannot_be_undone_while_a_later_change_touches_either_thing() {
    let f = fixture();
    let ((a, ea), (_, eb)) = two_things(&f);
    let out = f.engine.merge_entities(&ea, &eb).unwrap();
    let (_, rename) = renamed(f.engine.rename_entity(&eb, "Pop").unwrap());
    assert!(matches!(
        f.engine.undo_change(&out.change_id),
        Err(Error::Invalid(_))
    ));
    assert_eq!(names(&f), ["Pop"], "nothing was undone");
    f.engine.undo_change(&rename.unwrap()).unwrap();

    let moved = f
        .engine
        .refile_memory(
            &a,
            RefileTarget::New {
                name: "Cellar".into(),
                kind: EntityKind::Place,
            },
        )
        .unwrap();
    assert!(
        f.engine.entity(&eb).unwrap().0.last_change.is_none(),
        "not offered"
    );
    assert!(matches!(
        f.engine.undo_change(&out.change_id),
        Err(Error::Invalid(_))
    ));
    f.engine.undo_change(&moved.change_id).unwrap();
    assert_eq!(
        f.engine.entity(&eb).unwrap().0.last_change.unwrap().id,
        out.change_id
    );
    f.engine.undo_change(&out.change_id).unwrap();
}

#[test]
fn a_subject_is_filed_under_the_thing_with_that_name_whatever_its_kind() {
    let f = fixture();
    let (_, _, thing_id) = trinks(&f);
    let made = new_claim(
        &f,
        "Trinks are cold",
        Candidate {
            subject_kind: Some(EntityKind::Person),
            ..cand("Trinks", Predicate::Knows, "cold", "Trinks are cold.")
        },
    );
    let m = f.engine.get_memory(&made.created[0]).unwrap();
    assert_eq!(m.entity_id.as_deref(), Some(thing_id.as_str()));
    assert_eq!(names(&f), ["Trinks"], "no second Trinks");
}

#[test]
fn duplicates_of_one_name_are_chosen_between_the_same_way_every_time() {
    let f = fixture();
    let (_, _, thing_id) = trinks(&f);
    db(&f)
        .execute(
            "INSERT INTO entities(id, kind, name, key) VALUES ('ent_0000000000000000', 'place', 'Trinks', 'place:trinks')",
            [],
        )
        .unwrap();
    let pick = |kind: EntityKind, text: &str| {
        let made = new_claim(
            &f,
            text,
            Candidate {
                subject_kind: Some(kind),
                ..cand("Trinks", Predicate::Knows, text, &format!("{text}."))
            },
        );
        f.engine
            .get_memory(&made.created[0])
            .unwrap()
            .entity_id
            .unwrap()
    };
    assert_eq!(
        pick(EntityKind::Thing, "one"),
        thing_id,
        "the kind asked for"
    );
    assert_eq!(pick(EntityKind::Place, "two"), "ent_0000000000000000");
    assert_eq!(
        pick(EntityKind::Person, "three"),
        "ent_0000000000000000",
        "place before thing"
    );
}

#[test]
fn a_name_typed_with_a_combining_accent_is_the_same_name() {
    let f = fixture();
    let (_, id, entity) = trinks(&f);
    let (_, other) = learn(
        &f.engine,
        "Café is open",
        thing("Café", Predicate::Knows, "open", "Café is open."),
    );
    let cafe = f.engine.get_memory(&other).unwrap().entity_id.unwrap();
    match f.engine.rename_entity(&entity, "Cafe\u{301}").unwrap() {
        RenameOutcome::Exists { entity } => assert_eq!(entity.id, cafe),
        o => panic!("{o:?}"),
    }
    let (view, _) = renamed(f.engine.rename_entity(&entity, "Zoe\u{301}").unwrap());
    assert_eq!(view.name, "Zoé");
    assert!(
        f.engine
            .get_memory(&id)
            .unwrap()
            .sentence
            .starts_with("Zoé ")
    );
}
