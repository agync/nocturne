//! The boundary to the AI. The engine never calls a [`Mind`]; the caller reads pending items,
//! asks the mind, and hands results back through the engine. The trait is dyn-safe and `Send`
//! so it can live in an `Arc<dyn Mind>` and be used from spawned tasks.

use std::future::Future;
use std::path::Path;
use std::pin::Pin;

use jiff::Timestamp;
use jiff::civil::Date;
use nocturne_contract::{EntityKind, ItemKind, MemoryView, PageView};
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

#[derive(Debug, Clone, PartialEq, Default)]
pub struct AnswerMeta {
    pub cited: Vec<MemoryId>,
    pub not_sure: bool,
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
    fn answer<'a>(
        &'a self,
        question: &'a str,
        facts: &'a [MemoryView],
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
}

#[cfg(any(test, feature = "fake"))]
pub use fake::{FAKE_DIM, FakeMind, hash_embedding};

#[cfg(any(test, feature = "fake"))]
mod fake {
    use std::collections::HashMap;
    use std::path::Path;
    use std::sync::Mutex;

    use nocturne_contract::{MemoryView, PageView};
    use tokio::sync::mpsc;

    use super::{
        AnswerMeta, BoxFuture, ExtractContext, Mind, MindError, Transcript, Understanding,
    };
    use crate::types::Candidate;

    #[derive(Default)]
    struct Script {
        down: bool,
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

        fn check(&self) -> Result<(), MindError> {
            if self.script().down {
                Err(MindError::Unavailable)
            } else {
                Ok(())
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
                self.record("transcribe");
                self.check()?;
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
                self.record("read_image");
                self.check()?;
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
                self.record("read_document");
                self.check()?;
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
                self.record("extract");
                self.check()?;
                Ok(self.script().extract.get(text).cloned().unwrap_or_default())
            })
        }

        fn embed<'a>(
            &'a self,
            texts: &'a [String],
        ) -> BoxFuture<'a, Result<Vec<Vec<f32>>, MindError>> {
            Box::pin(async move {
                self.record("embed");
                self.check()?;
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
            facts: &'a [MemoryView],
            out: mpsc::Sender<String>,
        ) -> BoxFuture<'a, Result<AnswerMeta, MindError>> {
            Box::pin(async move {
                self.record("answer");
                self.check()?;
                for f in facts {
                    let _ = out.send(f.sentence.clone()).await;
                }
                Ok(AnswerMeta {
                    cited: facts.iter().map(|f| f.id.clone()).collect(),
                    not_sure: facts.is_empty(),
                })
            })
        }

        fn write_headline<'a>(
            &'a self,
            page: &'a PageView,
        ) -> BoxFuture<'a, Result<(String, String), MindError>> {
            Box::pin(async move {
                self.record("write_headline");
                self.check()?;
                Ok((page.headline.clone(), page.dek.clone()))
            })
        }

        fn speak<'a>(
            &'a self,
            text: &'a str,
            _voice: &'a str,
        ) -> BoxFuture<'a, Result<Vec<u8>, MindError>> {
            Box::pin(async move {
                self.record("speak");
                self.check()?;
                Ok(text.as_bytes().to_vec())
            })
        }
    }
}
