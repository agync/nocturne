mod common;

use std::sync::Arc;
use std::time::{Duration, Instant};

use common::*;
use nocturne_contract::{ItemKind, ItemStatus, Source};
use nocturne_engine::{
    Candidate, Engine, ExtractContext, FakeMind, Mind, MindError, Predicate, Transcript,
};

// The trait is dyn-safe and its futures are Send.
#[allow(dead_code)]
fn _dyn(_: Arc<dyn Mind>) {}

#[tokio::test]
async fn the_mind_can_be_shared_and_spawned() {
    let mind: Arc<dyn Mind> = Arc::new(FakeMind::new());
    let texts = vec!["a dentist visit".to_string(), "swimming".to_string()];
    let m = mind.clone();
    let vecs = tokio::spawn(async move { m.embed(&texts).await })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(vecs.len(), 2);
    assert!(vecs[0] != vecs[1]);

    let (tx, mut rx) = tokio::sync::mpsc::channel(8);
    let meta = mind.answer("q", &[], tx).await.unwrap();
    assert!(meta.not_sure);
    assert!(rx.recv().await.is_none());
    assert_eq!(mind.speak("hello", "voice").await.unwrap(), b"hello");
}

fn context(engine: &Engine, kind: ItemKind) -> ExtractContext {
    ExtractContext {
        item_kind: kind,
        kept_at: engine.clock_now(),
        today: date("2026-11-13"),
        known_entities: engine
            .entities(None)
            .unwrap()
            .into_iter()
            .map(|e| (e.name, e.kind))
            .collect(),
    }
}

#[tokio::test]
async fn a_voice_note_goes_through_the_engine_with_a_scripted_mind() {
    let f = fixture();
    let src = tempfile::tempdir().unwrap();
    let path = write_file(src.path(), "note.m4a", b"audio bytes");
    // Capture works with the mind absent: the item is kept and waits.
    let down = FakeMind::new().unavailable();
    let item = match f.engine.keep_file(&path, Source::Recording).unwrap() {
        nocturne_contract::Kept::New { item } => item,
        _ => unreachable!(),
    };
    assert_eq!(item.status, ItemStatus::Kept);
    let pending = f.engine.lease_pending(5, Duration::from_secs(60)).unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(
        down.transcribe(pending[0].original.as_ref().unwrap()).await,
        Err(MindError::Unavailable)
    );
    // Unavailable: the lease runs out and the item is back in the queue; nothing was lost.
    f.clock.set_str("2026-11-13T07:05:00Z");
    let pending = f.engine.lease_pending(5, Duration::from_secs(60)).unwrap();
    assert_eq!(pending.len(), 1);

    // The mind comes back. Minds see the stored copy, which is named by its hash.
    let p = &pending[0];
    let stored = p
        .original
        .as_ref()
        .unwrap()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let mind = FakeMind::new()
        .with_transcript(
            &stored,
            Transcript {
                text: "Dentist moved to Thursday at ten".into(),
                language: Some("en".into()),
                duration_ms: 9_000,
                segments: vec![(0, 9_000, "Dentist moved to Thursday at ten".into())],
            },
        )
        .with_extract(
            "Dentist moved to Thursday at ten",
            vec![Candidate::new(
                "Dentist appointment",
                Predicate::Scheduled,
                "Thursday 10:00",
                "Your dentist appointment is Thursday at 10:00.",
                0.9,
            )],
        );

    let transcript = mind.transcribe(p.original.as_ref().unwrap()).await.unwrap();
    f.engine
        .record_transcript(&p.id, &p.lease, transcript.clone())
        .unwrap();
    let cx = context(&f.engine, p.kind);
    let candidates = mind.extract(&transcript.text, &cx).await.unwrap();
    let report = f
        .engine
        .apply_candidates(&p.id, &p.lease, candidates)
        .unwrap();
    assert_eq!(report.created.len(), 1);
    let got = f.engine.get_item(&p.id).unwrap();
    assert_eq!(
        (got.status, got.duration_ms, got.memory_count),
        (ItemStatus::Understood, Some(9_000), 1)
    );
    assert_eq!(
        got.text.as_deref(),
        Some("Dentist moved to Thursday at ten")
    );

    let todo = f.engine.memories_without_embedding(10).unwrap();
    let texts: Vec<String> = todo.iter().map(|t| t.1.clone()).collect();
    let vecs = mind.embed(&texts).await.unwrap();
    f.engine
        .store_embeddings(todo.into_iter().map(|t| t.0).zip(vecs).collect())
        .unwrap();
    let q = mind
        .embed(&["dentist appointment".to_string()])
        .await
        .unwrap()
        .remove(0);
    let hits = f
        .engine
        .retrieve("dentist appointment", Some(&q), 3)
        .unwrap();
    assert_eq!(
        hits[0].memory.sentence,
        "Your dentist appointment is Thursday at 10:00."
    );
    assert_eq!(hits[0].memory.receipts[0].item_id, p.id);
    assert_eq!(
        mind.calls()
            .iter()
            .filter(|c| c.as_str() == "transcribe")
            .count(),
        1
    );
}

#[test]
fn many_threads_capture_and_read_at_once() {
    let f = fixture();
    let engine = f.engine.clone();
    let writers: Vec<_> = (0..4)
        .map(|t| {
            let e = engine.clone();
            std::thread::spawn(move || {
                for i in 0..50 {
                    e.keep_text(&format!("note {t} {i}"), Source::QuickCapture)
                        .unwrap();
                }
            })
        })
        .collect();
    let readers: Vec<_> = (0..4)
        .map(|_| {
            let e = engine.clone();
            std::thread::spawn(move || {
                for _ in 0..50 {
                    e.items(Default::default()).unwrap();
                    e.needs_you().unwrap();
                    e.retrieve("note", None, 5).unwrap();
                }
            })
        })
        .collect();
    for h in writers.into_iter().chain(readers) {
        h.join().unwrap();
    }
    assert_eq!(engine.items(Default::default()).unwrap().len(), 200);
}

/// keep_text p95 under 20 ms. Run with `cargo test -p nocturne-engine --release -- --ignored bench`.
/// It reports its numbers and fails only at five times the budget.
#[test]
#[ignore]
fn bench_keep_text() {
    let f = fixture();
    let mut times = Vec::new();
    for i in 0..500 {
        let t = Instant::now();
        f.engine
            .keep_text(
                &format!("a note about thing number {i} with some words"),
                Source::QuickCapture,
            )
            .unwrap();
        times.push(t.elapsed());
    }
    times.sort();
    let (p50, p95, max) = (times[250], times[475], times[499]);
    println!(
        "bench: keep_text over 500 notes: p50 {p50:?}, p95 {p95:?}, max {max:?} (budget p95 20ms)"
    );
    assert!(
        p95.as_millis() < 100,
        "keep_text p95 {p95:?} is over five times the budget"
    );
}

/// A mind with no way of its own to embed a question embeds it like any text.
#[tokio::test]
async fn a_question_is_embedded_like_any_text_by_default() {
    let mind = FakeMind::new().with_embedding("Who is the plumber?", vec![1.0, 0.0]);
    let dyn_mind: Arc<dyn Mind> = Arc::new(mind);
    assert_eq!(
        dyn_mind.embed_query("Who is the plumber?").await.unwrap(),
        [1.0, 0.0]
    );
}
