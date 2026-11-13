//! Reading every memory again with a new finding model, giving way to asks and reading like the
//! backfill. The old index stays in use until the store swaps in the new vectors.
//!
//! Each round first gives the old index what it lacks (memories kept or re-worded meanwhile,
//! with the model in use), then reads the next chunk with the new model. The store swaps only
//! when both are complete, so nothing kept during the re-read is missing from either side.

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

use nocturne_contract::ReindexPhase;
use nocturne_engine::{Engine, Error as EngineError, Mind, MindError};
use tokio_util::sync::CancellationToken;

use crate::reader::Reader;
use crate::{MindSlot, current_mind};

/// Chunk sizes and waits. The defaults are the product's; tests shrink them.
#[derive(Debug, Clone, Copy)]
pub struct ReindexOptions {
    pub chunk: usize,
    /// A wait while someone is asking, and between chunks.
    pub pause: Duration,
    /// A wait after the mind could not embed.
    pub retry: Duration,
    /// How long one embedding call may take.
    pub embed_limit: Duration,
}

impl Default for ReindexOptions {
    fn default() -> Self {
        ReindexOptions {
            chunk: 32,
            pause: Duration::from_millis(250),
            retry: Duration::from_secs(10),
            embed_limit: Duration::from_secs(60),
        }
    }
}

/// How a re-read ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReindexEnd {
    /// The new vectors are in use.
    Done,
    /// Stopped from outside, or cancelled in the store.
    Stopped,
}

/// Items per second, from the last minute of progress.
#[derive(Debug, Default)]
pub struct RateMeter {
    samples: VecDeque<(Instant, u32)>,
}

impl RateMeter {
    pub fn note(&mut self, at: Instant, done: u32) {
        self.samples.push_back((at, done));
        while self
            .samples
            .front()
            .is_some_and(|(t, _)| at.duration_since(*t) > Duration::from_secs(60))
        {
            self.samples.pop_front();
        }
    }

    pub fn per_second(&self) -> Option<f64> {
        let ((t0, d0), (t1, d1)) = (self.samples.front()?, self.samples.back()?);
        let secs = t1.duration_since(*t0).as_secs_f64();
        (d1 > d0 && secs > 0.0).then(|| f64::from(d1 - d0) / secs)
    }

    /// Seconds left for `remaining` items at the measured rate.
    pub fn eta(&self, remaining: u32) -> Option<u32> {
        estimate_seconds(remaining, self.per_second()?)
    }
}

/// Seconds for `remaining` items at `per_second`.
pub fn estimate_seconds(remaining: u32, per_second: f64) -> Option<u32> {
    (per_second > 0.0).then(|| (f64::from(remaining) / per_second).ceil() as u32)
}

/// Embeds `texts` with `model`. A mind that cannot name its model (a test's fixed mind) embeds
/// with its own, which is then the only model it has.
pub async fn embed_as(
    mind: &dyn Mind,
    model: &str,
    texts: &[String],
) -> Result<Vec<Vec<f32>>, MindError> {
    match mind.embed_with(model, texts).await {
        Err(MindError::Unsupported(_)) => mind.embed(texts).await,
        other => other,
    }
}

/// How many times one memory may fail to embed, for a reason of its own, before a re-read goes
/// on without a vector for it. It stays findable by its words.
pub const TRIES: u32 = 3;

/// Whether `e` says something about the text rather than the mind's state at the moment.
fn about_the_text(e: &MindError) -> bool {
    matches!(
        e,
        MindError::TooLong | MindError::Language(_) | MindError::Failed(_)
    )
}

pub struct Reindexer {
    engine: Engine,
    slot: MindSlot,
    reader: Reader,
    opts: ReindexOptions,
}

/// Which side of the re-read a chunk is for: the index in use, or the one being built.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Side {
    Old,
    New,
}

impl Reindexer {
    pub fn new(engine: Engine, slot: MindSlot, reader: Reader, opts: ReindexOptions) -> Self {
        Reindexer {
            engine,
            slot,
            reader,
            opts,
        }
    }

    async fn eng<T, F>(&self, f: F) -> nocturne_engine::Result<T>
    where
        F: FnOnce(&Engine) -> nocturne_engine::Result<T> + Send + 'static,
        T: Send + 'static,
    {
        let engine = self.engine.clone();
        tokio::task::spawn_blocking(move || f(&engine))
            .await
            .unwrap_or_else(|_| Err(EngineError::Invalid("a worker thread failed".into())))
    }

    async fn still_running(&self) -> bool {
        self.eng(|e| e.reindex_state())
            .await
            .ok()
            .flatten()
            .is_some_and(|r| r.phase == ReindexPhase::Running)
    }

    /// Embeds `rows` with `model` and stores them on `side`. A chunk that fails for a reason
    /// of its text is tried one memory at a time; a memory that keeps failing is stored with
    /// no vector, so the re-read can finish. False when the mind could not be used at all.
    async fn embed_chunk(
        &self,
        mind: &dyn Mind,
        side: Side,
        model: &str,
        rows: Vec<(String, String)>,
        tries: &mut HashMap<(Side, String), u32>,
    ) -> bool {
        let texts: Vec<String> = rows.iter().map(|r| r.1.clone()).collect();
        let whole =
            tokio::time::timeout(self.opts.embed_limit, embed_as(mind, model, &texts)).await;
        let pairs: Vec<(String, Vec<f32>)> = match whole {
            Ok(Ok(vecs)) if vecs.len() == rows.len() => {
                rows.into_iter().map(|r| r.0).zip(vecs).collect()
            }
            Ok(Err(e)) if !about_the_text(&e) => return false,
            Err(_) => return false,
            _ => {
                let mut pairs = Vec::new();
                for (id, text) in rows {
                    let one = std::slice::from_ref(&text);
                    let got =
                        tokio::time::timeout(self.opts.embed_limit, embed_as(mind, model, one))
                            .await;
                    match got {
                        Ok(Ok(mut v)) if v.len() == 1 => pairs.push((id, v.remove(0))),
                        Ok(Err(e)) if !about_the_text(&e) => return false,
                        Err(_) => return false,
                        _ => {
                            let n = tries.entry((side, id.clone())).or_insert(0);
                            *n += 1;
                            if *n >= TRIES {
                                tracing::info!("a memory is left without a vector of {model}");
                                pairs.push((id, Vec::new()));
                            }
                        }
                    }
                }
                pairs
            }
        };
        let model = model.to_string();
        match side {
            Side::Old => self
                .eng(move |e| e.store_embeddings_from(&model, pairs))
                .await
                .is_ok(),
            Side::New => self.eng(move |e| e.store_reindexed(pairs)).await.is_ok(),
        }
    }

    /// Reads every memory with `model` until the store swaps, or `stop` fires. `progress` hears
    /// how many of how many are done after each chunk. The re-read must have been begun in the
    /// store (`begin_reindex`).
    pub async fn run(
        &self,
        model: &str,
        stop: CancellationToken,
        progress: impl Fn(u32, u32) + Send,
    ) -> ReindexEnd {
        let mut tries: HashMap<(Side, String), u32> = HashMap::new();
        loop {
            if stop.is_cancelled() {
                return ReindexEnd::Stopped;
            }
            if self.reader.is_asking() {
                self.nap(&stop, self.opts.pause).await;
                continue;
            }
            let Some(mind) = current_mind(&self.slot) else {
                self.nap(&stop, self.opts.retry).await;
                continue;
            };
            if !self.still_running().await {
                return ReindexEnd::Stopped;
            }
            let chunk = self.opts.chunk;
            let in_use = self.engine.recall_model();
            let old = self
                .eng(move |e| e.memories_without_embedding(chunk))
                .await
                .unwrap_or_default();
            if !old.is_empty() {
                if !self
                    .embed_chunk(mind.as_ref(), Side::Old, &in_use, old, &mut tries)
                    .await
                {
                    self.nap(&stop, self.opts.retry).await;
                }
                continue;
            }
            let Ok(todo) = self.eng(move |e| e.reindex_todo(chunk)).await else {
                self.nap(&stop, self.opts.retry).await;
                continue;
            };
            if todo.is_empty() {
                if let Ok(true) = self.eng(|e| e.finish_reindex()).await {
                    return ReindexEnd::Done;
                }
                self.nap(&stop, self.opts.pause).await;
                continue;
            }
            if !self
                .embed_chunk(mind.as_ref(), Side::New, model, todo, &mut tries)
                .await
            {
                self.nap(&stop, self.opts.retry).await;
                continue;
            }
            if let Ok((done, total)) = self.eng(|e| e.reindex_progress()).await {
                progress(done, total);
            }
            self.nap(&stop, self.opts.pause).await;
        }
    }

    async fn nap(&self, stop: &CancellationToken, d: Duration) {
        tokio::select! {
            _ = stop.cancelled() => {}
            _ = tokio::time::sleep(d) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimate_from_measured_rate() {
        let mut m = RateMeter::default();
        let t0 = Instant::now();
        assert_eq!(m.eta(100), None);
        m.note(t0, 0);
        m.note(t0 + Duration::from_secs(10), 50);
        assert_eq!(m.per_second(), Some(5.0));
        assert_eq!(m.eta(100), Some(20));
        // Old samples fall away: the rate follows the last minute.
        m.note(t0 + Duration::from_secs(100), 60);
        assert_eq!(m.per_second(), None);
        m.note(t0 + Duration::from_secs(110), 160);
        assert_eq!(m.eta(50), Some(5));
        assert_eq!(estimate_seconds(10, 0.0), None);
        assert_eq!(estimate_seconds(10, 3.0), Some(4));
    }
}
