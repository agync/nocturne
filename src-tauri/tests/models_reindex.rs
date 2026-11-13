//! The re-read a new finding model needs, as the app runs it: the intelligence worker beside the
//! reader, giving way to asks, carrying on after a stop, swapping once, and a selection that a
//! crash at the swap left behind brought in line with the store.

mod common;

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use common::*;
use nocturne_app_lib::models;
use nocturne_contract::{ComponentRole, PageView, ReindexPhase, Tier};
use nocturne_engine::{
    AnswerMeta, BoxFuture, Candidate, ExtractContext, Fact, FakeMind, Mind, MindError, Transcript,
    Understanding,
};
use nocturne_intelligence::FakeHost;
use nocturne_intelligence::reindex::{ReindexEnd, ReindexOptions, Reindexer};
use nocturne_lemonade::record::{self, SetupRecord};
use nocturne_lemonade::selection::{self, Selection};
use tauri::async_runtime::block_on;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

const NEW: &str = "Qwen3-Embedding-4B-GGUF";

/// A mind whose re-read vectors have three numbers, and that counts them.
struct ReMind {
    inner: FakeMind,
    with: AtomicU32,
}

impl Mind for ReMind {
    fn transcribe<'a>(&'a self, audio: &'a Path) -> BoxFuture<'a, Result<Transcript, MindError>> {
        self.inner.transcribe(audio)
    }
    fn read_image<'a>(
        &'a self,
        image: &'a Path,
    ) -> BoxFuture<'a, Result<Understanding, MindError>> {
        self.inner.read_image(image)
    }
    fn read_document<'a>(
        &'a self,
        doc: &'a Path,
    ) -> BoxFuture<'a, Result<Understanding, MindError>> {
        self.inner.read_document(doc)
    }
    fn extract<'a>(
        &'a self,
        text: &'a str,
        cx: &'a ExtractContext,
    ) -> BoxFuture<'a, Result<Vec<Candidate>, MindError>> {
        self.inner.extract(text, cx)
    }
    fn embed<'a>(&'a self, texts: &'a [String]) -> BoxFuture<'a, Result<Vec<Vec<f32>>, MindError>> {
        self.inner.embed(texts)
    }
    fn embed_with<'a>(
        &'a self,
        model: &'a str,
        texts: &'a [String],
    ) -> BoxFuture<'a, Result<Vec<Vec<f32>>, MindError>> {
        Box::pin(async move {
            assert_eq!(model, NEW);
            self.with.fetch_add(texts.len() as u32, Ordering::SeqCst);
            Ok(texts.iter().map(|_| vec![0.0, 0.0, 1.0]).collect())
        })
    }
    fn answer<'a>(
        &'a self,
        question: &'a str,
        facts: &'a [Fact],
        out: mpsc::Sender<String>,
    ) -> BoxFuture<'a, Result<AnswerMeta, MindError>> {
        self.inner.answer(question, facts, out)
    }
    fn write_headline<'a>(
        &'a self,
        page: &'a PageView,
    ) -> BoxFuture<'a, Result<(String, String), MindError>> {
        self.inner.write_headline(page)
    }
    fn speak<'a>(
        &'a self,
        text: &'a str,
        voice: &'a str,
    ) -> BoxFuture<'a, Result<Vec<u8>, MindError>> {
        self.inner.speak(text, voice)
    }
}

struct Rig {
    h: Harness,
    mind: Arc<ReMind>,
    services: Arc<nocturne_app_lib::runtime::Services>,
}

fn rig(memories: usize) -> Rig {
    let mind = Arc::new(ReMind {
        inner: FakeMind::new(),
        with: AtomicU32::new(0),
    });
    let h = harness_with_mind(mind.clone());
    for i in 0..memories {
        h.learn(
            &format!("note {i}"),
            &format!("Sam likes thing {i}."),
            "Sam",
            &format!("thing {i}"),
        );
    }
    let services = start_manual(&h, FakeHost::new());
    Rig { h, mind, services }
}

fn opts() -> ReindexOptions {
    ReindexOptions {
        chunk: 2,
        pause: Duration::from_millis(5),
        retry: Duration::from_millis(20),
        embed_limit: Duration::from_secs(5),
    }
}

impl Rig {
    fn worker(&self) -> Reindexer {
        Reindexer::new(
            self.h.engine(),
            self.h.state().mind.shared(),
            self.services.reader.clone(),
            opts(),
        )
    }
}

#[test]
fn the_worker_reads_everything_then_swaps() {
    let r = rig(5);
    let e = r.h.engine();
    e.begin_reindex(NEW, Some(3)).unwrap();
    let seen = std::sync::Mutex::new(Vec::new());
    let end = block_on(
        r.worker()
            .run(NEW, CancellationToken::new(), |done, total| {
                seen.lock().unwrap().push((done, total));
            }),
    );
    assert_eq!(end, ReindexEnd::Done);
    assert_eq!(r.mind.with.load(Ordering::SeqCst), 5);
    let seen = seen.into_inner().unwrap();
    assert_eq!(seen.last(), Some(&(5, 5)));
    assert!(seen.windows(2).all(|w| w[0].0 <= w[1].0), "{seen:?}");
    assert_eq!(
        e.reindex_state().unwrap().unwrap().phase,
        ReindexPhase::Done
    );
    assert_eq!(e.indexed_model().unwrap().as_deref(), Some(NEW));
}

#[test]
fn gives_way_to_asks() {
    let r = rig(3);
    r.h.engine().begin_reindex(NEW, Some(3)).unwrap();
    let guard = r.services.reader.ask_begin();
    let stop = CancellationToken::new();
    let worker = r.worker();
    let ended = block_on(async {
        tokio::time::timeout(
            Duration::from_millis(300),
            worker.run(NEW, stop.clone(), |_, _| {}),
        )
        .await
    });
    assert!(ended.is_err(), "the re-read waits while someone asks");
    assert_eq!(r.mind.with.load(Ordering::SeqCst), 0);
    drop(guard);
    let end = block_on(worker.run(NEW, stop, |_, _| {}));
    assert_eq!(end, ReindexEnd::Done);
    assert_eq!(r.mind.with.load(Ordering::SeqCst), 3);
}

#[test]
fn a_stopped_re_read_carries_on_and_a_cancelled_one_ends() {
    let r = rig(4);
    let e = r.h.engine();
    e.begin_reindex(NEW, Some(3)).unwrap();
    let stop = CancellationToken::new();
    let first = stop.clone();
    let end = block_on(r.worker().run(NEW, stop, move |done, _| {
        if done >= 2 {
            first.cancel();
        }
    }));
    assert_eq!(end, ReindexEnd::Stopped);
    let (done, total) = e.reindex_progress().unwrap();
    assert!(done >= 2 && done < total, "{done} of {total}");
    // As after a restart: the same re-read is begun again and finishes from where it was.
    e.begin_reindex(NEW, Some(3)).unwrap();
    assert_eq!(e.reindex_progress().unwrap().0, done);
    assert_eq!(
        block_on(r.worker().run(NEW, CancellationToken::new(), |_, _| {})),
        ReindexEnd::Done
    );
    assert_eq!(r.mind.with.load(Ordering::SeqCst), 4);
    // Cancelled in the store, a running worker ends without swapping anything.
    e.begin_reindex("nomic-embed-text-v1-GGUF", Some(768))
        .unwrap();
    e.cancel_reindex().unwrap();
    assert_eq!(
        block_on(r.worker().run(NEW, CancellationToken::new(), |_, _| {})),
        ReindexEnd::Stopped
    );
    assert_eq!(e.indexed_model().unwrap().as_deref(), Some(NEW));
}

#[test]
fn selection_repaired_after_crash_at_swap() {
    let r = rig(2);
    let config = models::config_dir(&r.h.state().data_dir);
    record::write(
        &config,
        &SetupRecord {
            version: 1,
            tier: Some(Tier::Standard),
            done: true,
            completed: Some(Tier::Standard),
            backends: Default::default(),
        },
    )
    .unwrap();
    selection::write(&config, &Selection::preset(Tier::Standard)).unwrap();
    let e = r.h.engine();
    e.begin_reindex(NEW, Some(3)).unwrap();
    assert_eq!(
        block_on(r.worker().run(NEW, CancellationToken::new(), |_, _| {})),
        ReindexEnd::Done
    );
    // The crash: selection.json still names the old model.
    assert_ne!(selection::picks(&selection::read(&config)).recall, NEW);
    models::repair_after_swap(r.h.app.handle(), &config);
    let repaired = selection::read(&config);
    assert_eq!(selection::picks(&repaired).recall, NEW);
    assert_eq!(repaired.custom.get(&ComponentRole::Recall).unwrap(), NEW);
    // Nothing to repair the second time, and nothing while a re-read is under way.
    models::repair_after_swap(r.h.app.handle(), &config);
    assert_eq!(selection::read(&config), repaired);
}

#[test]
fn backfill_uses_the_finding_model_the_store_names() {
    let r = rig(2);
    let e = r.h.engine();
    assert!(e.memories_without_embedding(10).unwrap().len() <= 2);
    e.set_recall_model(NEW);
    // Vectors from the old model do not count for the new one.
    let waiting = e.memories_without_embedding(10).unwrap();
    assert_eq!(waiting.len(), 2);
}
