mod common;

use std::time::Instant;

use common::*;
use nocturne_contract::{MemoryStatus, Source};
use nocturne_engine::{Predicate, hash_embedding};

fn learn_embedded(f: &Fixture, text: &str, c: nocturne_engine::Candidate) -> String {
    let (_, id) = learn(&f.engine, text, c);
    f.engine
        .store_embeddings(vec![(id.clone(), hash_embedding(text))])
        .unwrap();
    id
}

#[test]
fn fts_only_query_finds_dentist_memories() {
    let f = fixture();
    let (_, dentist) = learn(
        &f.engine,
        "n1",
        cand(
            "Dentist appointment",
            Predicate::Scheduled,
            "Tuesday 14:00",
            "Your dentist appointment is Tuesday at 14:00.",
        ),
    );
    learn(
        &f.engine,
        "n2",
        cand("Gran", Predicate::LivesIn, "Leeds", "Gran lives in Leeds."),
    );
    let hits = f
        .engine
        .retrieve("when is my dentist appointment?", None, 5)
        .unwrap();
    assert_eq!(
        hits.len(),
        1,
        "stop words do not drag in unrelated memories"
    );
    assert_eq!(hits[0].memory.id, dentist);
    assert!(!hits[0].let_go);
    assert!(hits[0].score > 0.0);
    assert!(f.engine.retrieve("   ", None, 5).unwrap().is_empty());
    assert!(f.engine.retrieve("dentist", None, 0).unwrap().is_empty());
}

#[test]
fn with_vectors_a_paraphrase_finds_the_right_memory() {
    let f = fixture();
    // Scripted vectors: the paraphrase shares no words with the memory.
    let (_, tooth) = learn(
        &f.engine,
        "n1",
        cand(
            "Dentist appointment",
            Predicate::Scheduled,
            "Tuesday",
            "Your dentist appointment is Tuesday.",
        ),
    );
    let (_, swim) = learn(
        &f.engine,
        "n2",
        cand("Leo", Predicate::Prefers, "swimming", "Leo likes swimming."),
    );
    f.engine
        .store_embeddings(vec![
            (tooth.clone(), vec![1.0, 0.0, 0.1]),
            (swim.clone(), vec![0.0, 1.0, 0.1]),
        ])
        .unwrap();
    assert!(
        f.engine
            .retrieve("tooth check-up", None, 5)
            .unwrap()
            .is_empty(),
        "no words, no vector: nothing"
    );
    let hits = f
        .engine
        .retrieve("tooth check-up", Some(&[0.95, 0.05, 0.1]), 5)
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].memory.id, tooth);
    // Vectors of another size are ignored rather than mis-scored.
    assert!(
        f.engine
            .retrieve("tooth", Some(&[1.0, 0.0]), 5)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn fusion_prefers_what_both_rankings_agree_on() {
    let f = fixture();
    let a = learn_embedded(
        &f,
        "swimming lessons on Wednesday",
        cand(
            "Leo",
            Predicate::Prefers,
            "swimming",
            "Leo has swimming lessons on Wednesday.",
        ),
    );
    let b = learn_embedded(
        &f,
        "swimming club history",
        cand(
            "Club",
            Predicate::Knows,
            "swimming",
            "The club talks about swimming.",
        ),
    );
    let q = "swimming lessons Wednesday";
    let hits = f
        .engine
        .retrieve(q, Some(&hash_embedding("swimming lessons on Wednesday")), 5)
        .unwrap();
    assert_eq!(hits[0].memory.id, a);
    assert!(hits.iter().any(|h| h.memory.id == b));
}

#[test]
fn active_and_fading_come_before_let_go_and_let_go_is_flagged() {
    let f = fixture();
    let (_, old) = learn(
        &f.engine,
        "n1",
        cand(
            "Gym",
            Predicate::Knows,
            "door code 1234",
            "The old gym door code was 1234.",
        ),
    );
    f.clock.advance_days(100);
    f.engine.decay().unwrap();
    let (_, fresh) = learn(
        &f.engine,
        "n2",
        cand(
            "Gym",
            Predicate::Knows,
            "membership",
            "The gym membership is monthly.",
        ),
    );
    let hits = f.engine.retrieve("gym door code", None, 5).unwrap();
    assert_eq!(hits.len(), 2);
    assert_eq!(
        hits[0].memory.id, fresh,
        "the better match is let go, but it ranks after live ones"
    );
    assert!(!hits[0].let_go);
    assert_eq!(hits[1].memory.id, old);
    assert!(hits[1].let_go);
    assert_eq!(hits[1].memory.status, MemoryStatus::LetGo);
    // When live hits fill k, let-go ones are left out.
    let hits = f.engine.retrieve("gym door code", None, 1).unwrap();
    assert_eq!(hits.len(), 1);
    assert!(!hits[0].let_go);
}

#[test]
fn unconfirmed_hits_are_flagged_so_they_are_never_stated_as_true() {
    let f = fixture();
    let web = keep_as(&f.engine, "web", Source::PastedWeb);
    f.engine
        .apply(
            &web,
            vec![cand(
                "Clinic",
                Predicate::Knows,
                "opens at nine",
                "The clinic opens at nine.",
            )],
        )
        .unwrap();
    let hits = f.engine.retrieve("clinic opens", None, 5).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].memory.status, MemoryStatus::Unconfirmed);
    assert!(hits[0].memory.receipts[0].unconfirmed);
}

#[test]
fn only_current_memories_are_found_and_forgotten_ones_never_come_back() {
    let f = fixture();
    let (_, old) = learn(
        &f.engine,
        "n1",
        cand(
            "Leo",
            Predicate::Scheduled,
            "Wednesday",
            "Leo swims on Wednesday.",
        ),
    );
    f.engine
        .store_embeddings(vec![(old.clone(), vec![1.0, 0.0])])
        .unwrap();
    assert_eq!(
        f.engine
            .retrieve("swims", Some(&[1.0, 0.0]), 5)
            .unwrap()
            .len(),
        1
    );
    let new = f
        .engine
        .correct(&old, nocturne_engine::CorrectPatch::object("Monday"))
        .unwrap();
    let hits = f.engine.retrieve("swims", Some(&[1.0, 0.0]), 5).unwrap();
    assert_eq!(hits.len(), 1, "the superseded row is history, not a hit");
    assert_eq!(hits[0].memory.id, new);
    let p = f.engine.preview_forget(&[new], true).unwrap();
    f.engine.forget(&p.token).unwrap();
    assert!(
        f.engine
            .retrieve("swims", Some(&[1.0, 0.0]), 5)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn searching_is_not_use() {
    let f = fixture();
    let (_, id) = learn(
        &f.engine,
        "n",
        cand("Leo", Predicate::Knows, "Sam", "Leo knows Sam."),
    );
    f.engine.retrieve("Leo", None, 5).unwrap();
    assert_eq!(f.engine.get_memory(&id).unwrap().last_used, None);
}

#[test]
fn memories_without_vectors_are_listed_for_embedding() {
    let f = fixture();
    let (_, a) = learn(
        &f.engine,
        "n1",
        cand("Leo", Predicate::Knows, "Sam", "Leo knows Sam."),
    );
    let (_, b) = learn(
        &f.engine,
        "n2",
        cand("Gran", Predicate::Knows, "Pip", "Gran knows Pip."),
    );
    f.engine
        .store_embeddings(vec![(a, vec![1.0]), ("mem_unknown".into(), vec![1.0])])
        .unwrap();
    let todo = f.engine.memories_without_embedding(10).unwrap();
    assert_eq!(todo, vec![(b, "Gran knows Pip.".to_string())]);
}

/// Ten thousand memories: retrieval stays under 150 ms. Run with
/// `cargo test -p nocturne-engine --release -- --ignored bench`. It reports its numbers and fails
/// only at five times the budget.
#[test]
#[ignore]
fn bench_retrieve_10k_memories() {
    let f = fixture();
    let words = [
        "dentist", "swimming", "school", "garden", "passport", "bakery", "invoice", "holiday",
        "doctor", "bicycle", "recipe", "birthday",
    ];
    let item = keep(&f.engine, "bulk");
    let mut cands = Vec::new();
    for i in 0..10_000 {
        let (a, b, c) = (words[i % 12], words[(i / 12) % 12], words[(i / 144) % 12]);
        cands.push(cand(
            &format!("Subject {i}"),
            Predicate::Knows,
            &format!("{a} {b} {c} {i}"),
            &format!("Subject {i} is about {a}, {b} and {c} number {i}."),
        ));
    }
    let t = Instant::now();
    f.engine.apply(&item, cands).unwrap();
    println!("bench: applied 10000 candidates in {:?}", t.elapsed());
    let mems = f.engine.memories(Default::default()).unwrap();
    assert_eq!(mems.len(), 10_000);
    let rows: Vec<(String, Vec<f32>)> = mems
        .iter()
        .map(|m| (m.id.clone(), hash_embedding_384(&m.sentence)))
        .collect();
    let t = Instant::now();
    f.engine.store_embeddings(rows).unwrap();
    println!("bench: stored 10000 vectors (dim 384) in {:?}", t.elapsed());

    let queries = [
        "dentist appointment",
        "swimming school garden",
        "when is the passport invoice due",
        "holiday bicycle",
        "bakery recipe birthday",
    ];
    let t = Instant::now();
    let first = f
        .engine
        .retrieve(queries[0], Some(&hash_embedding_384(queries[0])), 8)
        .unwrap();
    let cold = t.elapsed();
    assert!(!first.is_empty());
    let mut times = Vec::new();
    for i in 0..50 {
        let q = queries[i % queries.len()];
        let t = Instant::now();
        let hits = f
            .engine
            .retrieve(q, Some(&hash_embedding_384(q)), 8)
            .unwrap();
        times.push(t.elapsed());
        assert!(!hits.is_empty());
    }
    times.sort();
    let (p50, p95, max) = (times[25], times[47], times[49]);
    println!(
        "bench: retrieve at 10000 memories: cold (builds vector index) {cold:?}, warm p50 {p50:?}, p95 {p95:?}, max {max:?} (budget 150ms)"
    );
    assert!(
        cold.as_millis() < 750,
        "cold retrieve {cold:?} is over five times the budget"
    );
    assert!(
        p95.as_millis() < 750,
        "p95 {p95:?} is over five times the budget"
    );
    if p95.as_millis() >= 150 {
        println!("bench: WARNING p95 is over the 150 ms budget");
    }
}

fn hash_embedding_384(text: &str) -> Vec<f32> {
    let mut v = vec![0f32; 384];
    for word in text
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
    {
        let h = word
            .bytes()
            .fold(5381u32, |h, b| h.wrapping_mul(33) ^ u32::from(b));
        v[(h as usize) % 384] += 1.0;
        v[(h.rotate_left(7) as usize) % 384] += 0.5;
    }
    let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    v.iter_mut().for_each(|x| *x /= n.max(1e-6));
    v
}

#[test]
fn who_someone_is_comes_first_when_asked_who_they_are() {
    let f = fixture();
    // Memories that say Leo more often outrank the one that says who he is.
    for (i, (object, sentence)) in [
        (
            "swimming",
            "Leo swims; Leo likes it and Leo's coach is Dee.",
        ),
        ("trip", "Leo's trip: Leo needs a packed lunch, Leo says."),
        ("piano", "Leo plays piano and Leo practises when Leo can."),
    ]
    .into_iter()
    .enumerate()
    {
        learn(
            &f.engine,
            &format!("leo note {i}"),
            cand("Leo", Predicate::Prefers, object, sentence),
        );
    }
    let (_, son) = learn(
        &f.engine,
        "Leo is my son",
        cand("Leo", Predicate::Knows, "my son", "Leo is my son."),
    );
    let (_, teacher) = learn(
        &f.engine,
        "Mrs Carver is Leo's teacher",
        cand(
            "Mrs Carver",
            Predicate::Knows,
            "Leo's teacher",
            "Mrs Carver is Leo's teacher.",
        ),
    );

    let hits = f.engine.retrieve("Who is Leo?", None, 2).unwrap();
    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].memory.id, son, "the relationship leads");
    assert!(hits.iter().all(|h| h.memory.id != teacher));

    let hits = f.engine.retrieve("who's mrs carver", None, 3).unwrap();
    assert_eq!(hits[0].memory.id, teacher);

    // Any other question about Leo is ranked as before.
    let hits = f.engine.retrieve("What does Leo need?", None, 1).unwrap();
    assert_ne!(hits[0].memory.id, son);
}

/// Two memories of book club disagreeing, and an unrelated one.
fn club_dispute(f: &Fixture) -> (String, String) {
    let (_, a) = learn(
        &f.engine,
        "Club on Tuesday",
        cand(
            "Book club",
            Predicate::Scheduled,
            "Tuesday",
            "Book club is on Tuesday.",
        ),
    );
    let item = keep(&f.engine, "Club on Wednesday");
    let r = f
        .engine
        .apply(
            &item,
            vec![cand(
                "Book club",
                Predicate::Scheduled,
                "Wednesday",
                "Book club is on Wednesday.",
            )],
        )
        .unwrap();
    (a, r.created[0].clone())
}

fn sql(f: &Fixture, statement: &str, id: &str) {
    let conn = rusqlite::Connection::open(f.dir.path().join("nocturne.db")).unwrap();
    conn.execute(statement, [id]).unwrap();
}

#[test]
fn an_open_which_is_right_question_names_both_sides() {
    let f = fixture();
    let (a, b) = club_dispute(&f);
    let (_, other) = learn(
        &f.engine,
        "Gran in York",
        cand("Gran", Predicate::LivesIn, "York", "Gran lives in York."),
    );

    let pair = vec![(a.clone(), b.clone())];
    assert_eq!(
        f.engine.open_disputes(std::slice::from_ref(&a)).unwrap(),
        pair
    );
    assert_eq!(
        f.engine.open_disputes(&[b.clone(), a.clone()]).unwrap(),
        pair
    );
    assert!(f.engine.open_disputes(&[other]).unwrap().is_empty());

    let conflict = match &f.engine.needs_you().unwrap()[0] {
        nocturne_contract::NeedsYouEntry::Which { conflict_id, .. } => conflict_id.clone(),
        other => panic!("{other:?}"),
    };
    f.engine.resolve_which(&conflict, &b).unwrap();
    assert!(f.engine.open_disputes(&[a, b]).unwrap().is_empty());
}

/// A side that is no longer current is not a side: replaced, let go or being forgotten, the
/// question is left out, as Needs you leaves it out.
#[test]
fn a_question_with_a_side_that_is_gone_is_not_open() {
    for statement in [
        "UPDATE memories SET superseded_at = '2026-11-13T08:00:00Z' WHERE id = ?1",
        "UPDATE memories SET status = 'letGo', let_go_at = '2026-11-13T08:00:00Z' WHERE id = ?1",
        "UPDATE memories SET forgetting = 1 WHERE id = ?1",
    ] {
        let f = fixture();
        let (a, b) = club_dispute(&f);
        assert_eq!(
            f.engine
                .open_disputes(std::slice::from_ref(&a))
                .unwrap()
                .len(),
            1
        );
        sql(&f, statement, &b);
        assert!(
            f.engine
                .open_disputes(&[a.clone(), b.clone()])
                .unwrap()
                .is_empty(),
            "{statement}"
        );
    }
}

/// Only a predicate that names a relationship is brought forward: "Leo is tall" is not who
/// Leo is.
#[test]
fn a_plain_is_does_not_say_who_someone_is() {
    let f = fixture();
    learn(
        &f.engine,
        "Leo swims",
        cand(
            "Leo",
            Predicate::Prefers,
            "swimming",
            "Leo likes swimming, Leo says.",
        ),
    );
    let (_, tall) = learn(
        &f.engine,
        "Leo is tall",
        cand("Leo", Predicate::Other("is".into()), "tall", "Leo is tall."),
    );
    let hits = f.engine.retrieve("Who is Leo?", None, 1).unwrap();
    assert_ne!(hits[0].memory.id, tall);
}
