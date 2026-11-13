//! The boundary to the AI. The engine never calls a [`Mind`]; the caller reads pending items,
//! asks the mind, and hands results back through the engine. The trait is dyn-safe and `Send`
//! so it can live in an `Arc<dyn Mind>` and be used from spawned tasks.

use std::future::Future;
use std::path::Path;
use std::pin::Pin;

use jiff::Timestamp;
use jiff::civil::Date;
use nocturne_contract::{EntityKind, ItemKind, MemoryView, PageView, SentLine};
use tokio::sync::mpsc;

use crate::types::{Candidate, MemoryId};

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Transcript {
    pub text: String,
    pub language: Option<String>,
    pub duration_ms: u32,
    /// `(start_ms, end_ms, text)` per segment.
    pub segments: Vec<(u32, u32, String)>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Understanding {
    pub title: String,
    pub text: String,
    pub summary: Option<String>,
    pub language: Option<String>,
    pub pages: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExtractContext {
    pub item_kind: ItemKind,
    pub kept_at: Timestamp,
    pub today: Date,
    pub known_entities: Vec<(String, EntityKind)>,
}

/// A memory handed to the mind to answer from.
#[derive(Debug, Clone, PartialEq)]
pub struct Fact {
    pub memory: MemoryView,
    /// The memories it disagrees with in an open which-is-right question. Each of them is among
    /// the facts handed over with it.
    pub disputed_by: Vec<MemoryId>,
}

impl From<MemoryView> for Fact {
    fn from(memory: MemoryView) -> Self {
        Fact {
            memory,
            disputed_by: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct AnswerMeta {
    pub cited: Vec<MemoryId>,
    pub not_sure: bool,
    /// The memory each bold span in the answer stands for, in order: the one cited by the next
    /// marker after it. Empty when there is no bold, or when a span could not be paired.
    pub spans: Vec<MemoryId>,
    /// Set when the answer was written online: what went out and what came back.
    pub online: Option<OnlineReceipt>,
}

/// What an online answer sent and received. `template` is the body as sent with each memory
/// sentence replaced by its number; `sent_ids` are the memories sent, in `[mN]` order, and
/// `line_hashes` the hash of each exact line sent, in the same order.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct OnlineReceipt {
    pub bytes_up: u64,
    pub bytes_down: u64,
    pub status: Option<u16>,
    pub seconds: f32,
    pub cost_usd: Option<f64>,
    pub template: Vec<SentLine>,
    pub sent_ids: Vec<MemoryId>,
    pub line_hashes: Vec<String>,
}

/// Why an online answer did not come.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum OnlineFailure {
    #[error("no internet")]
    NoInternet,
    #[error("the provider could not be reached")]
    Unreachable,
    #[error("the provider took too long")]
    Slow,
    #[error("the provider refused the key")]
    KeyRefused,
    #[error("the provider account has no credit")]
    NoCredit,
    #[error("the provider does not have the model")]
    ModelMissing,
    #[error("the provider is busy")]
    Busy,
    #[error("the provider refused the request")]
    Refused,
    #[error("the provider answered with status {0}")]
    Other(u16),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MindError {
    #[error("the mind is not available")]
    Unavailable,
    #[error("the mind is busy")]
    Busy,
    #[error("unsupported: {0}")]
    Unsupported(String),
    #[error("too long")]
    TooLong,
    #[error("language not supported: {0}")]
    Language(String),
    #[error("failed: {0}")]
    Failed(String),
    /// A model could not be loaded because memory is short. Never the item's fault.
    #[error("not enough memory to load a model")]
    OutOfMemory,
    /// An online answer did not come.
    #[error("online: {0}")]
    Online(OnlineFailure),
}

pub trait Mind: Send + Sync {
    fn transcribe<'a>(&'a self, audio: &'a Path) -> BoxFuture<'a, Result<Transcript, MindError>>;
    fn read_image<'a>(&'a self, image: &'a Path)
    -> BoxFuture<'a, Result<Understanding, MindError>>;
    fn read_document<'a>(
        &'a self,
        doc: &'a Path,
    ) -> BoxFuture<'a, Result<Understanding, MindError>>;
    fn extract<'a>(
        &'a self,
        text: &'a str,
        cx: &'a ExtractContext,
    ) -> BoxFuture<'a, Result<Vec<Candidate>, MindError>>;
    fn embed<'a>(&'a self, texts: &'a [String]) -> BoxFuture<'a, Result<Vec<Vec<f32>>, MindError>>;
    /// Embed with a named recall model rather than the current one, for a re-read.
    fn embed_with<'a>(
        &'a self,
        model: &'a str,
        texts: &'a [String],
    ) -> BoxFuture<'a, Result<Vec<Vec<f32>>, MindError>> {
        let _ = (model, texts);
        Box::pin(async {
            Err(MindError::Unsupported(
                "embedding with another model".into(),
            ))
        })
    }
    fn answer<'a>(
        &'a self,
        question: &'a str,
        facts: &'a [Fact],
        out: mpsc::Sender<String>,
    ) -> BoxFuture<'a, Result<AnswerMeta, MindError>>;
    fn write_headline<'a>(
        &'a self,
        page: &'a PageView,
    ) -> BoxFuture<'a, Result<(String, String), MindError>>;
    fn speak<'a>(
        &'a self,
        text: &'a str,
        voice: &'a str,
    ) -> BoxFuture<'a, Result<Vec<u8>, MindError>>;
    /// Called once before a run of `speak` calls, such as a page read aloud, so a mind that
    /// must free memory for its voice does it once rather than for every part.
    fn ready_to_speak(&self) -> BoxFuture<'_, ()> {
        Box::pin(async {})
    }
}

#[cfg(any(test, feature = "fake"))]
pub use fake::{FAKE_DIM, FakeMind, Gate, hash_embedding};

#[cfg(any(test, feature = "fake"))]
mod fake {
    use std::collections::{HashMap, VecDeque};
    use std::path::Path;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use nocturne_contract::PageView;
    use tokio::sync::{Notify, mpsc, watch};

    use super::{
        AnswerMeta, BoxFuture, ExtractContext, Fact, Mind, MindError, Transcript, Understanding,
    };
    use crate::types::Candidate;

    /// Holds a call mid-flight. The call announces itself on `entered`, then waits for `open`.
    pub struct Gate {
        entered: Notify,
        open: watch::Sender<bool>,
    }

    impl Gate {
        fn new() -> Self {
            Gate {
                entered: Notify::new(),
                open: watch::channel(false).0,
            }
        }

        /// Wait until a gated call has started (one permit per call that entered).
        pub async fn entered(&self) {
            self.entered.notified().await;
        }

        /// Let held calls, and later ones, go through.
        pub fn open(&self) {
            self.open.send_replace(true);
        }

        /// Hold calls again.
        pub fn close(&self) {
            self.open.send_replace(false);
        }

        async fn pass(&self) {
            self.entered.notify_one();
            let mut rx = self.open.subscribe();
            let _ = rx.wait_for(|open| *open).await;
        }
    }

    #[derive(Default)]
    struct Script {
        down: bool,
        latency: Duration,
        /// Errors to return once each, in order, per call name.
        errors: HashMap<String, VecDeque<MindError>>,
        /// Errors every call of that name returns until cleared.
        always: HashMap<String, MindError>,
        gates: HashMap<String, Arc<Gate>>,
        headline: Option<(String, String)>,
        extract: HashMap<String, Vec<Candidate>>,
        embed: HashMap<String, Vec<f32>>,
        transcripts: HashMap<String, Transcript>,
        understandings: HashMap<String, Understanding>,
    }

    /// A scripted mind for tests. Responses are keyed by input: the text for `extract` and
    /// `embed`, the file name for `transcribe`, `read_image` and `read_document`. Unscripted
    /// extraction returns nothing; unscripted embedding is a small deterministic hash of the words.
    #[derive(Default)]
    pub struct FakeMind {
        script: Mutex<Script>,
        calls: Mutex<Vec<String>>,
    }

    pub const FAKE_DIM: usize = 16;

    /// A deterministic bag-of-words embedding, normalised.
    pub fn hash_embedding(text: &str) -> Vec<f32> {
        let mut v = vec![0f32; FAKE_DIM];
        for word in text.to_lowercase().split(|c: char| !c.is_alphanumeric()) {
            if word.is_empty() {
                continue;
            }
            let h = word
                .bytes()
                .fold(5381u32, |h, b| h.wrapping_mul(33) ^ u32::from(b));
            v[(h as usize) % FAKE_DIM] += 1.0;
        }
        let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            v.iter_mut().for_each(|x| *x /= norm);
        }
        v
    }

    impl FakeMind {
        pub fn new() -> Self {
            Self::default()
        }

        fn script(&self) -> std::sync::MutexGuard<'_, Script> {
            self.script.lock().unwrap_or_else(|e| e.into_inner())
        }

        pub fn with_extract(self, text: &str, candidates: Vec<Candidate>) -> Self {
            self.script().extract.insert(text.to_string(), candidates);
            self
        }

        pub fn with_embedding(self, text: &str, vec: Vec<f32>) -> Self {
            self.script().embed.insert(text.to_string(), vec);
            self
        }

        pub fn with_transcript(self, file_name: &str, t: Transcript) -> Self {
            self.script().transcripts.insert(file_name.to_string(), t);
            self
        }

        pub fn with_understanding(self, file_name: &str, u: Understanding) -> Self {
            self.script()
                .understandings
                .insert(file_name.to_string(), u);
            self
        }

        /// Every call fails with `Unavailable`, as when the mind is not running.
        pub fn unavailable(self) -> Self {
            self.script().down = true;
            self
        }

        /// Switch the runtime down or up while a test runs.
        pub fn set_down(&self, down: bool) {
            self.script().down = down;
        }

        /// Every call waits this long (tokio time, so `tokio::time::pause` controls it).
        pub fn set_latency(&self, latency: Duration) {
            self.script().latency = latency;
        }

        /// The next call named `call` (`"extract"`, `"embed"`, `"transcribe"`, `"read_image"`,
        /// `"read_document"`, `"answer"`, `"write_headline"`, `"speak"`) fails with `err`.
        /// Queue several to fail several calls in a row.
        pub fn fail_next(&self, call: &str, err: MindError) {
            self.script()
                .errors
                .entry(call.to_string())
                .or_default()
                .push_back(err);
        }

        /// Every call named `call` fails with `err` until `clear_failures`.
        pub fn fail_always(&self, call: &str, err: MindError) {
            self.script().always.insert(call.to_string(), err);
        }

        pub fn clear_failures(&self, call: &str) {
            let mut script = self.script();
            script.errors.remove(call);
            script.always.remove(call);
        }

        /// A gate on calls named `call`: they stop at it until it is opened.
        pub fn gate(&self, call: &str) -> Arc<Gate> {
            let gate = Arc::new(Gate::new());
            self.script().gates.insert(call.to_string(), gate.clone());
            gate
        }

        /// The headline and dek `write_headline` returns (otherwise the page's own).
        pub fn with_headline(self, headline: &str, dek: &str) -> Self {
            self.script().headline = Some((headline.to_string(), dek.to_string()));
            self
        }

        /// How many calls named `call` have been made.
        pub fn call_count(&self, call: &str) -> usize {
            self.calls().iter().filter(|c| *c == call).count()
        }

        /// The names of the calls made so far, in order.
        pub fn calls(&self) -> Vec<String> {
            self.calls.lock().unwrap_or_else(|e| e.into_inner()).clone()
        }

        fn record(&self, call: &str) {
            self.calls
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(call.to_string());
        }

        /// Record the call, then hold at its gate, wait out the latency and apply the script:
        /// down, then a queued error, then a standing one.
        async fn check(&self, call: &str) -> Result<(), MindError> {
            self.record(call);
            let (gate, latency) = {
                let script = self.script();
                (script.gates.get(call).cloned(), script.latency)
            };
            if let Some(gate) = gate {
                gate.pass().await;
            }
            if !latency.is_zero() {
                tokio::time::sleep(latency).await;
            }
            let mut script = self.script();
            if script.down {
                return Err(MindError::Unavailable);
            }
            if let Some(err) = script.errors.get_mut(call).and_then(VecDeque::pop_front) {
                return Err(err);
            }
            match script.always.get(call) {
                Some(err) => Err(err.clone()),
                None => Ok(()),
            }
        }

        fn file_key(path: &Path) -> String {
            path.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        }
    }

    impl Mind for FakeMind {
        fn transcribe<'a>(
            &'a self,
            audio: &'a Path,
        ) -> BoxFuture<'a, Result<Transcript, MindError>> {
            Box::pin(async move {
                self.check("transcribe").await?;
                self.script()
                    .transcripts
                    .get(&Self::file_key(audio))
                    .cloned()
                    .ok_or_else(|| MindError::Unsupported("no scripted transcript".into()))
            })
        }

        fn read_image<'a>(
            &'a self,
            image: &'a Path,
        ) -> BoxFuture<'a, Result<Understanding, MindError>> {
            Box::pin(async move {
                self.check("read_image").await?;
                self.script()
                    .understandings
                    .get(&Self::file_key(image))
                    .cloned()
                    .ok_or_else(|| MindError::Unsupported("no scripted reading".into()))
            })
        }

        fn read_document<'a>(
            &'a self,
            doc: &'a Path,
        ) -> BoxFuture<'a, Result<Understanding, MindError>> {
            Box::pin(async move {
                self.check("read_document").await?;
                self.script()
                    .understandings
                    .get(&Self::file_key(doc))
                    .cloned()
                    .ok_or_else(|| MindError::Unsupported("no scripted reading".into()))
            })
        }

        fn extract<'a>(
            &'a self,
            text: &'a str,
            _cx: &'a ExtractContext,
        ) -> BoxFuture<'a, Result<Vec<Candidate>, MindError>> {
            Box::pin(async move {
                self.check("extract").await?;
                Ok(self.script().extract.get(text).cloned().unwrap_or_default())
            })
        }

        fn embed<'a>(
            &'a self,
            texts: &'a [String],
        ) -> BoxFuture<'a, Result<Vec<Vec<f32>>, MindError>> {
            Box::pin(async move {
                self.check("embed").await?;
                let script = self.script();
                Ok(texts
                    .iter()
                    .map(|t| {
                        script
                            .embed
                            .get(t)
                            .cloned()
                            .unwrap_or_else(|| hash_embedding(t))
                    })
                    .collect())
            })
        }

        fn answer<'a>(
            &'a self,
            _question: &'a str,
            facts: &'a [Fact],
            out: mpsc::Sender<String>,
        ) -> BoxFuture<'a, Result<AnswerMeta, MindError>> {
            Box::pin(async move {
                self.check("answer").await?;
                for f in facts {
                    let _ = out.send(f.memory.sentence.clone()).await;
                }
                Ok(AnswerMeta {
                    cited: facts.iter().map(|f| f.memory.id.clone()).collect(),
                    not_sure: facts.is_empty(),
                    spans: Vec::new(),
                    online: None,
                })
            })
        }

        fn write_headline<'a>(
            &'a self,
            page: &'a PageView,
        ) -> BoxFuture<'a, Result<(String, String), MindError>> {
            Box::pin(async move {
                self.check("write_headline").await?;
                Ok(self
                    .script()
                    .headline
                    .clone()
                    .unwrap_or_else(|| (page.headline.clone(), page.dek.clone())))
            })
        }

        fn speak<'a>(
            &'a self,
            text: &'a str,
            _voice: &'a str,
        ) -> BoxFuture<'a, Result<Vec<u8>, MindError>> {
            Box::pin(async move {
                self.check("speak").await?;
                Ok(text.as_bytes().to_vec())
            })
        }
    }
}
