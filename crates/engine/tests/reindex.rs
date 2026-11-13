//! Reading every memory again for a new finding model: the old index stays in use until one
//! atomic swap, cancelling keeps the old, and memories kept, re-worded or forgotten meanwhile
//! are handled on both sides.

mod common;

use common::*;
use nocturne_contract::ReindexPhase;
use nocturne_engine::{DEFAULT_RECALL_MODEL, Engine, Predicate};

const NEW: &str = "Qwen3-Embedding-4B-GGUF";

fn two(engine: &Engine) -> (String, String) {
    let (_, tooth) = learn(
        engine,
        "n1",
        cand(
            "Dentist appointment",
            Predicate::Scheduled,
            "Tuesday",
            "Your dentist appointment is Tuesday.",
        ),
    );
    let (_, swim) = learn(
        engine,
        "n2",
        cand("Leo", Predicate::Prefers, "swimming", "Leo likes swimming."),
    );
    engine
        .store_embeddings(vec![
            (tooth.clone(), vec![1.0, 0.0]),
            (swim.clone(), vec![0.0, 1.0]),
        ])
        .unwrap();
    (tooth, swim)
}

fn top(engine: &Engine, q: &[f32]) -> Option<String> {
    engine
        .retrieve("zzz", Some(q), 1)
        .unwrap()
        .first()
        .map(|h| h.memory.id.clone())
}

/// New vectors of three numbers that put the memories the other way round.
fn swapped(id: &str, tooth: &str) -> Vec<f32> {
    if id == tooth {
        vec![0.0, 1.0, 0.0]
    } else {
        vec![1.0, 0.0, 0.0]
    }
}

fn read_all(engine: &Engine, tooth: &str) {
    let todo = engine.reindex_todo(100).unwrap();
    let rows = todo
        .into_iter()
        .map(|(id, _)| {
            let v = swapped(&id, tooth);
            (id, v)
        })
        .collect();
    engine.store_reindexed(rows).unwrap();
}

#[test]
fn migration_10_backfills_the_model() {
    let Fixture { dir, engine, clock } = fixture();
    let (tooth, _) = two(&engine);
    drop(engine);
    // Vectors from before the column was added carry the recall model every tier used.
    let conn = rusqlite::Connection::open(dir.path().join("nocturne.db")).unwrap();
    conn.execute("DELETE FROM embeddings", []).unwrap();
    conn.execute(
        "INSERT INTO embeddings(memory_id, dim, vec) VALUES (?1, 2, x'0000803f00000000')",
        [&tooth],
    )
    .unwrap();
    drop(conn);
    let f2 = Engine::open(dir.path(), clock).unwrap();
    assert_eq!(f2.recall_model(), DEFAULT_RECALL_MODEL);
    assert_eq!(top(&f2, &[1.0, 0.0]), Some(tooth));
    assert_eq!(
        f2.indexed_model().unwrap().as_deref(),
        Some(DEFAULT_RECALL_MODEL)
    );
}

#[test]
fn old_index_used_until_swap() {
    let f = fixture();
    let (tooth, swim) = two(&f.engine);
    let row = f.engine.begin_reindex(NEW, Some(3)).unwrap();
    assert_eq!(row.phase, ReindexPhase::Running);
    assert_eq!(row.from_model, DEFAULT_RECALL_MODEL);
    assert_eq!(row.total, 2);
    read_all(&f.engine, &tooth);
    assert_eq!(f.engine.reindex_progress().unwrap(), (2, 2));
    // Everything is read again, yet nothing changes until the swap.
    assert_eq!(top(&f.engine, &[1.0, 0.0]), Some(tooth.clone()));
    assert!(
        f.engine
            .retrieve("zzz", Some(&[1.0, 0.0, 0.0]), 1)
            .unwrap()
            .is_empty()
    );
    assert!(f.engine.finish_reindex().unwrap());
    // The store has swapped; the app names the new model, and only its vectors are used.
    assert_eq!(
        top(&f.engine, &[1.0, 0.0]),
        None,
        "old vectors are not mixed in"
    );
    f.engine.set_recall_model(NEW);
    assert_eq!(top(&f.engine, &[1.0, 0.0, 0.0]), Some(swim));
    assert_eq!(f.engine.indexed_model().unwrap().as_deref(), Some(NEW));
    assert_eq!(
        f.engine.reindex_state().unwrap().unwrap().phase,
        ReindexPhase::Done
    );
}

#[test]
fn swap_is_atomic() {
    let f = fixture();
    let (tooth, _) = two(&f.engine);
    f.engine.begin_reindex(NEW, Some(3)).unwrap();
    // Half done: no swap, the old rows untouched.
    let first = f.engine.reindex_todo(1).unwrap();
    let id = first[0].0.clone();
    f.engine
        .store_reindexed(vec![(id.clone(), swapped(&id, &tooth))])
        .unwrap();
    assert!(!f.engine.finish_reindex().unwrap());
    assert_eq!(
        sql_count(
            f.dir.path(),
            &format!("SELECT COUNT(*) FROM embeddings WHERE model = '{DEFAULT_RECALL_MODEL}'")
        ),
        2
    );
    read_all(&f.engine, &tooth);
    assert!(f.engine.finish_reindex().unwrap());
    assert_eq!(
        sql_count(f.dir.path(), "SELECT COUNT(*) FROM embeddings_next"),
        0
    );
    assert_eq!(
        sql_count(
            f.dir.path(),
            &format!("SELECT COUNT(*) FROM embeddings WHERE model = '{NEW}'")
        ),
        2
    );
    assert_eq!(
        sql_count(f.dir.path(), "SELECT COUNT(*) FROM embeddings"),
        2
    );
    // A second finish has nothing to do.
    assert!(!f.engine.finish_reindex().unwrap());
}

#[test]
fn cancel_keeps_old() {
    let f = fixture();
    let (tooth, _) = two(&f.engine);
    f.engine.begin_reindex(NEW, Some(3)).unwrap();
    read_all(&f.engine, &tooth);
    f.engine.cancel_reindex().unwrap();
    assert_eq!(
        f.engine.reindex_state().unwrap().unwrap().phase,
        ReindexPhase::Cancelled
    );
    assert_eq!(
        sql_count(f.dir.path(), "SELECT COUNT(*) FROM embeddings_next"),
        0
    );
    assert!(!f.engine.finish_reindex().unwrap());
    assert!(f.engine.reindex_todo(10).unwrap().is_empty());
    f.engine
        .store_reindexed(vec![(tooth.clone(), vec![1.0, 0.0, 0.0])])
        .unwrap();
    assert_eq!(
        sql_count(f.dir.path(), "SELECT COUNT(*) FROM embeddings_next"),
        0
    );
    assert_eq!(top(&f.engine, &[1.0, 0.0]), Some(tooth));
}

#[test]
fn memories_added_during_reindex_included() {
    let f = fixture();
    let (tooth, _) = two(&f.engine);
    f.engine.begin_reindex(NEW, Some(3)).unwrap();
    read_all(&f.engine, &tooth);
    let (_, bins) = learn(
        &f.engine,
        "n3",
        cand(
            "Bins",
            Predicate::Scheduled,
            "Monday",
            "The bins go out on Monday.",
        ),
    );
    // Not yet in the old index either: no swap.
    assert!(!f.engine.finish_reindex().unwrap());
    let todo = f.engine.reindex_todo(10).unwrap();
    assert_eq!(todo.len(), 1);
    assert_eq!(todo[0].0, bins);
    f.engine
        .store_reindexed(vec![(bins.clone(), vec![0.0, 0.0, 1.0])])
        .unwrap();
    // The old index still lacks it, as a backfill would give it.
    assert!(!f.engine.finish_reindex().unwrap());
    assert_eq!(f.engine.memories_without_embedding(10).unwrap().len(), 1);
    f.engine
        .store_embeddings(vec![(bins.clone(), vec![0.5, 0.5])])
        .unwrap();
    // Storing an old-model vector drops the new one: it is read again.
    assert_eq!(f.engine.reindex_todo(10).unwrap().len(), 1);
    f.engine
        .store_reindexed(vec![(bins.clone(), vec![0.0, 0.0, 1.0])])
        .unwrap();
    assert!(f.engine.finish_reindex().unwrap());
    f.engine.set_recall_model(NEW);
    assert_eq!(top(&f.engine, &[0.0, 0.0, 1.0]), Some(bins));
}

#[test]
fn forgotten_during_reindex_absent() {
    let f = fixture();
    let (tooth, swim) = two(&f.engine);
    f.engine.begin_reindex(NEW, Some(3)).unwrap();
    read_all(&f.engine, &tooth);
    let preview = f
        .engine
        .preview_forget(std::slice::from_ref(&swim), false)
        .unwrap();
    f.engine.forget(&preview.token).unwrap();
    assert_eq!(
        sql_count(
            f.dir.path(),
            &format!("SELECT COUNT(*) FROM embeddings_next WHERE memory_id = '{swim}'")
        ),
        0
    );
    assert!(f.engine.finish_reindex().unwrap());
    f.engine.set_recall_model(NEW);
    assert_eq!(top(&f.engine, &[0.0, 1.0, 0.0]), Some(tooth.clone()));
    assert_eq!(
        sql_count(
            f.dir.path(),
            &format!("SELECT COUNT(*) FROM embeddings WHERE memory_id = '{swim}'")
        ),
        0
    );
}

#[test]
fn crash_resumes() {
    let f = fixture();
    let (tooth, _) = two(&f.engine);
    f.engine.begin_reindex(NEW, Some(3)).unwrap();
    let first = f.engine.reindex_todo(1).unwrap();
    let id = first[0].0.clone();
    f.engine
        .store_reindexed(vec![(id.clone(), swapped(&id, &tooth))])
        .unwrap();
    drop(f.engine);
    let engine = Engine::open(f.dir.path(), f.clock.clone()).unwrap();
    let row = engine.reindex_state().unwrap().unwrap();
    assert_eq!(row.phase, ReindexPhase::Running);
    assert_eq!(row.model, NEW);
    // Beginning the same re-read again carries on from what was built.
    engine.begin_reindex(NEW, Some(3)).unwrap();
    assert_eq!(engine.reindex_progress().unwrap(), (1, 2));
    assert_eq!(engine.reindex_todo(10).unwrap().len(), 1);
    // A re-read for another model starts over.
    engine
        .begin_reindex("nomic-embed-text-v1-GGUF", Some(768))
        .unwrap();
    assert_eq!(engine.reindex_progress().unwrap(), (0, 2));
}

#[test]
fn dimension_change_handled() {
    let f = fixture();
    let (tooth, swim) = two(&f.engine);
    let (_, bins) = learn(
        &f.engine,
        "n3",
        cand(
            "Bins",
            Predicate::Scheduled,
            "Monday",
            "The bins go out on Monday.",
        ),
    );
    f.engine
        .store_embeddings(vec![(bins.clone(), vec![0.7, 0.7])])
        .unwrap();
    f.engine.begin_reindex(NEW, Some(3)).unwrap();
    read_all(&f.engine, &tooth);
    assert!(f.engine.finish_reindex().unwrap());
    f.engine.set_recall_model(NEW);
    // A question embedded with the old size finds nothing by meaning, rather than nonsense.
    assert!(
        f.engine
            .retrieve("zzz", Some(&[1.0, 0.0]), 5)
            .unwrap()
            .is_empty()
    );
    assert_eq!(top(&f.engine, &[0.0, 1.0, 0.0]), Some(tooth.clone()));
    // A stray vector of another size is ignored, whichever row is read first.
    f.engine
        .store_embeddings(vec![(swim, vec![0.0, 1.0])])
        .unwrap();
    assert_eq!(top(&f.engine, &[0.0, 1.0, 0.0]), Some(tooth));
}
