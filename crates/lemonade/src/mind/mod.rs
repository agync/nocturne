//! [`LemonadeMind`]: the Nocturne mind served by the private engine.

mod audio;
mod decide;
mod documents;
pub mod online;
mod sampling;

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use base64::Engine as _;
use futures_util::StreamExt;
use jiff::civil::Date;
use nocturne_contract::{ComponentRole, PageView, Tier};
use nocturne_engine::{
    AnswerMeta, BoxFuture, Candidate, Clock, ExtractContext, Fact, Mind, MindError, ReflectInput,
    Reflected, Transcript, Understanding,
};
use nocturne_mind::judge::{Judged, both_sides, complaints, conclude, judge, one_sided};
use nocturne_mind::read::{self, Turn};
use nocturne_mind::{guard, prompts, reflect, watch};
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::{Mutex, mpsc};

use crate::client::{ChatRequest, Client, LoadOptions, Message, Transcription};
use crate::error::RuntimeError;
use crate::npu::{Npu, Placement, Work};
use crate::selection::Picks;

const MAX_IMAGE_EDGE: u32 = 2048;
const MAX_AUDIO_SECONDS: f64 = 2.0 * 3600.0;
/// What has been checked or loaded since the engine last (re)started.
#[derive(Default)]
struct Seen {
    generation: u64,
    installed: HashSet<String>,
    loaded: HashSet<String>,
}

pub struct LemonadeMind {
    client: Client,
    picks: Picks,
    tier: Tier,
    sampling: sampling::Presets,
    overrides: Vec<(ComponentRole, String)>,
    seen: Mutex<Seen>,
    clock: Option<Arc<dyn Clock>>,
    /// The engine refused the reading grammar once: reading goes by a JSON schema from then on.
    grammar_refused: AtomicBool,
    /// Requests made to read items, retries and second looks included.
    reading_calls: AtomicU64,
    /// Where recordings are read, when this computer has an NPU.
    npu: Option<Arc<Npu>>,
}

impl LemonadeMind {
    /// A mind using `picks` for each job; `tier` sets how the answering model is run.
    pub fn new(client: Client, picks: Picks, tier: Tier) -> Self {
        LemonadeMind {
            client,
            picks,
            tier,
            sampling: sampling::Presets::from_env(tier),
            overrides: Vec::new(),
            seen: Mutex::new(Seen::default()),
            clock: None,
            grammar_refused: AtomicBool::new(false),
            reading_calls: AtomicU64::new(0),
            npu: None,
        }
    }

    /// Reads recordings on the NPU whenever `npu` says so.
    pub fn with_npu(mut self, npu: Arc<Npu>) -> Self {
        self.npu = Some(npu);
        self
    }

    /// Takes today's date from `clock` (the engine's, with its time zone) rather than from
    /// this computer's own clock.
    pub fn with_clock(mut self, clock: Arc<dyn Clock>) -> Self {
        self.clock = Some(clock);
        self
    }

    /// How many requests reading has made so far, retries and second looks included: what
    /// reading costs, for measurement runs.
    pub fn reading_calls(&self) -> u64 {
        self.reading_calls.load(Ordering::Relaxed)
    }

    fn today(&self) -> Date {
        match &self.clock {
            Some(c) => c.now().to_zoned(c.tz()).date(),
            None => jiff::Zoned::now().date(),
        }
    }

    /// Uses `model` for `role` instead of the tier's own. For tests with small models.
    #[doc(hidden)]
    pub fn with_model(mut self, role: ComponentRole, model: &str) -> Self {
        self.overrides.push((role, model.to_string()));
        self
    }

    fn model(&self, role: ComponentRole) -> Result<String, MindError> {
        if let Some((_, m)) = self.overrides.iter().find(|(r, _)| *r == role) {
            return Ok(m.clone());
        }
        match self.picks.get(role) {
            "" => Err(MindError::Unavailable),
            m => Ok(m.to_string()),
        }
    }

    /// The model, once it is certain to be fully on disk. The engine downloads a missing model
    /// on first use even when told to stay offline, so nothing may reach it before this passes.
    async fn installed(&self, role: ComponentRole) -> Result<String, MindError> {
        let model = self.model(role)?;
        self.installed_model(model).await
    }

    async fn installed_model(&self, model: String) -> Result<String, MindError> {
        let mut seen = self.seen.lock().await;
        let generation = self.client.generation();
        if seen.generation != generation {
            *seen = Seen {
                generation,
                ..Seen::default()
            };
        }
        if seen.installed.contains(&model) {
            return Ok(model);
        }
        let listed = self.client.models().await?;
        if !listed.iter().any(|m| m.id == model && m.present()) {
            return Err(RuntimeError::NotInstalled(model).into());
        }
        seen.installed.insert(model.clone());
        Ok(model)
    }

    /// The vector for a question asked of the memories, made the way the recall model wants a
    /// search query made (see [`crate::client::query_text`]). Memories are embedded as they are,
    /// with [`Mind::embed`].
    pub fn embed_query<'a>(
        &'a self,
        question: &'a str,
    ) -> BoxFuture<'a, Result<Vec<f32>, MindError>> {
        Box::pin(async move {
            let model = self.installed(ComponentRole::Recall).await?;
            Ok(self.client.embed_query(&model, question).await?)
        })
    }

    /// Loads the thinking model once per engine launch, with a context size to suit the tier.
    async fn warm_mind(&self) -> Result<String, MindError> {
        let model = self.installed(ComponentRole::Mind).await?;
        let ctx = crate::residency::mind_context(self.tier);
        self.warm(&model, ctx).await.map_err(|e| match e {
            RuntimeError::Rejected { .. } => MindError::Unavailable,
            other => other.into(),
        })?;
        Ok(model)
    }

    /// Loads `model` with a context of `ctx` once per engine launch.
    /// The lock is not held while the engine loads, so a slow load of one model never holds up
    /// a request to another.
    async fn warm(&self, model: &str, ctx: u32) -> Result<(), RuntimeError> {
        if self.seen.lock().await.loaded.contains(model) {
            return Ok(());
        }
        self.client
            .load(
                model,
                &LoadOptions {
                    ctx_size: Some(ctx),
                    ..LoadOptions::default()
                },
            )
            .await?;
        self.seen.lock().await.loaded.insert(model.to_string());
        Ok(())
    }

    /// Does `work` with `run`, given a model: on the NPU's mind when it takes that work, else on
    /// the GPU mind, which also does it again when the NPU fails. While both minds are loaded, a
    /// result `kept` turns down (an empty reading) is done again on the GPU mind too, and counts as
    /// the NPU's failure only when the GPU mind's result is kept. With the NPU's mind alone, that
    /// would load the GPU mind in its place for every item with nothing to remember.
    async fn with_mind<'a, T>(
        &'a self,
        work: Work,
        kept: impl Fn(&T) -> bool,
        run: impl Fn(String) -> BoxFuture<'a, Result<T, MindError>>,
    ) -> Result<T, MindError> {
        let mut doubted = false;
        if let Some((npu, model)) = self.npu_mind(work).await {
            match run(model.to_string()).await {
                Ok(done) if kept(&done) || npu.placement() != Placement::Split => {
                    npu.mind_worked();
                    return Ok(done);
                }
                Ok(_) => doubted = true,
                Err(e) => self.npu_failed(npu, &e),
            }
        }
        let model = self.warm_mind().await?;
        let done = run(model).await;
        if doubted && let Some(npu) = &self.npu {
            match &done {
                Ok(d) if kept(d) => {
                    npu.mind_failed("an empty reading the GPU mind could read");
                    self.client.set_models_per_kind(npu.models_per_kind());
                }
                _ => npu.mind_worked(),
            }
        }
        done
    }

    /// Counts a failure of the NPU's mind. An unusable reply is named only as such: its words can
    /// quote the item.
    fn npu_failed(&self, npu: &Npu, e: &MindError) {
        match e {
            MindError::Failed(_) => npu.mind_failed("an unusable reply"),
            other => npu.mind_failed(&other.to_string()),
        }
        self.client.set_models_per_kind(npu.models_per_kind());
    }

    /// The NPU's mind, loaded, when it takes `work`. It is never fetched on use: when it is not
    /// on this computer the GPU mind does the work from then on.
    async fn npu_mind(&self, work: Work) -> Option<(&Npu, &'static str)> {
        let npu = self.npu.as_deref()?;
        let model = npu.model_for(work)?;
        if self.installed_model(model.to_string()).await.is_err() {
            if let Ok(listed) = self.client.models().await
                && !listed.iter().any(|m| m.id == model && m.present())
            {
                npu.mind_missing();
                self.client.set_models_per_kind(npu.models_per_kind());
            }
            return None;
        }
        if let Err(e) = self.warm(model, crate::npu::MIND_CONTEXT).await {
            // A failed load unloads every model, so the GPU mind is loaded again with its context.
            self.forget_loaded().await;
            npu.mind_failed(&engine_message(&e));
            self.client.set_models_per_kind(npu.models_per_kind());
            return None;
        }
        Some((npu, model))
    }

    async fn forget_loaded(&self) {
        self.seen.lock().await.loaded.clear();
    }

    /// One request to read `item`: by the grammar, or by a JSON schema once the engine refused
    /// the grammar. A refusal is asked again once, by the schema, and remembered. The reply is
    /// watched as it arrives and ended as soon as it begins to loop.
    async fn reading_reply(
        &self,
        mut req: ChatRequest,
        item: &str,
    ) -> Result<read::Reply, MindError> {
        if self.grammar_refused.load(Ordering::Relaxed) {
            by_schema(&mut req);
        }
        let first = self.watched(&req, item).await;
        let refused = match &first {
            Err(RuntimeError::Rejected {
                status,
                code,
                message,
            }) if req.grammar.is_some() => Some(format!("{status} {code} {message}")),
            Err(RuntimeError::Server { status, message })
                if req.grammar.is_some() && message.to_lowercase().contains("grammar") =>
            {
                Some(format!("{status} {message}"))
            }
            _ => None,
        };
        let reply = match refused {
            Some(kind) => {
                tracing::warn!("the engine refused the reading grammar ({kind}); using a schema");
                self.grammar_refused.store(true, Ordering::Relaxed);
                by_schema(&mut req);
                self.watched(&req, item).await
            }
            None => first,
        };
        match reply {
            Ok(r) => Ok(r),
            Err(e) => Err(self.map_chat_error(e).await),
        }
    }

    async fn watched(&self, req: &ChatRequest, item: &str) -> Result<read::Reply, RuntimeError> {
        self.reading_calls.fetch_add(1, Ordering::Relaxed);
        let prompt: Vec<&str> = req
            .messages
            .iter()
            .filter_map(|m| m.content.as_str())
            .collect();
        let mut watch = watch::Watch::new(item, &prompt.join("\n"));
        let reply = self
            .client
            .chat_watched(req, |so_far| watch.looping(so_far))
            .await?;
        let text = match watch.why {
            Some(why) => {
                tracing::info!(
                    "stopped a reading that began to {}",
                    match why {
                        watch::Loop::Repeat => "repeat itself",
                        watch::Loop::Echo => "copy its instructions",
                    }
                );
                watch.kept(&reply.text).to_string()
            }
            None => reply.text.clone(),
        };
        Ok(read::Reply {
            text,
            full: reply.text,
            cut: reply.cut,
            looped: watch.why.is_some(),
        })
    }

    /// A reading request: the item's grammar (a JSON schema on the NPU, whose runtime ignores a
    /// grammar without saying so), and a reply of at most `ceiling` tokens.
    fn extract_request(&self, model: &str, turns: &[Turn], ceiling: u32) -> ChatRequest {
        let messages = turns
            .iter()
            .map(|t| Message {
                role: t.role.as_str().into(),
                content: Value::String(t.text.clone()),
            })
            .collect();
        let mut req = ChatRequest::new(model, messages).sampled(&self.sampling.extract);
        req.max_tokens = Some(ceiling);
        req.timeout = Some(self.client.timeouts().json);
        if crate::npu::on_npu(model) {
            by_schema(&mut req);
        } else {
            req.grammar = Some(prompts::extraction_grammar());
        }
        req
    }

    async fn extract_impl(
        &self,
        text: &str,
        cx: &ExtractContext,
    ) -> Result<Vec<Candidate>, MindError> {
        if text.trim().is_empty() {
            return Ok(Vec::new());
        }
        let before = self.reading_calls();
        let found = self
            .with_mind(
                Work::Reading,
                |f: &Vec<Candidate>| non_empty_reading(f),
                |m| Box::pin(async move { read::extract(&self.reads(&m), text, cx).await }),
            )
            .await;
        tracing::info!("a reading took {} requests", self.reading_calls() - before);
        found
    }

    /// Another look at `text` for the facts in `lines` (see [`read::second_look`]).
    async fn extract_lines_impl(
        &self,
        text: &str,
        cx: &ExtractContext,
        lines: &[String],
    ) -> Result<Vec<Candidate>, MindError> {
        if lines.is_empty() || text.trim().is_empty() {
            return Ok(Vec::new());
        }
        let before = self.reading_calls();
        let found = self
            .with_mind(
                Work::Reading,
                |f: &Vec<Candidate>| non_empty_reading(f),
                |m| {
                    Box::pin(
                        async move { read::second_look(&self.reads(&m), text, cx, lines).await },
                    )
                },
            )
            .await;
        tracing::info!(
            "a second look took {} requests",
            self.reading_calls() - before
        );
        found
    }

    fn reads<'a>(&'a self, model: &'a str) -> ReadWith<'a> {
        ReadWith { mind: self, model }
    }

    async fn answer_impl(
        &self,
        question: &str,
        facts: &[Fact],
        out: mpsc::Sender<String>,
    ) -> Result<AnswerMeta, MindError> {
        if facts.is_empty() {
            return Ok(AnswerMeta {
                cited: Vec::new(),
                not_sure: true,
                spans: Vec::new(),
                online: None,
            });
        }
        let nonce = prompts::nonce();
        let shown = prompts::facts_for(self.tier, facts);
        let facts = shown.as_slice();
        let system = if prompts::is_help(facts) {
            prompts::help_system(&nonce)
        } else {
            prompts::answer_system(self.tier, &nonce)
        };
        let (system, nonce) = (system.as_str(), nonce.as_str());
        let judged = self
            .with_mind(
                Work::Answering,
                |_| true,
                |m| {
                    Box::pin(
                        async move { self.judged_answer(&m, question, facts, system, nonce).await },
                    )
                },
            )
            .await?;
        conclude(judged, facts, &out).await
    }

    /// An answer to `question` from `model`, judged against `facts` before any of it is shown.
    async fn judged_answer(
        &self,
        model: &str,
        question: &str,
        facts: &[Fact],
        system: &str,
        nonce: &str,
    ) -> Result<Judged, MindError> {
        let mut messages = vec![
            Message::system(system),
            Message::user(&prompts::answer_user(
                self.tier,
                question,
                facts,
                nonce,
                Some(self.today()),
            )),
        ];
        let mut reply = self.reply_to(model, &messages, "answer").await?;
        let mut judged = judge(&reply, facts, system);
        // A small mind often drops the date or number of the fact it cites, or gives one side of a
        // disagreement. It is asked once more, naming what was wrong; the better of the two
        // answers is kept.
        if self.tier == Tier::Tiny && !judged.not_sure {
            let wrong = complaints(&judged, facts);
            if !wrong.is_empty() {
                messages.push(Message {
                    role: "assistant".into(),
                    content: Value::String(std::mem::take(&mut reply)),
                });
                messages.push(Message::user(&format!(
                    "{} Answer again in one or two short sentences, with the number of the fact after each sentence, like [m1].",
                    wrong.join(" ")
                )));
                if let Ok(second) = self.reply_to(model, &messages, "answer_retry").await {
                    let again = judge(&second, facts, system);
                    if !again.not_sure
                        && !again.cited.is_empty()
                        && (again.text.len() >= judged.text.len() || judged.cited.is_empty())
                        && complaints(&again, facts).len() < wrong.len()
                    {
                        judged = again;
                    }
                }
            }
        }
        // Still no citation, but the answer is one fact's own words: that fact is its receipt.
        if self.tier == Tier::Tiny && !judged.not_sure && judged.cited.is_empty() {
            let sentences: Vec<&str> = facts.iter().map(|f| f.memory.sentence.as_str()).collect();
            if let Some(n) = guard::source_of(&judged.text, &sentences) {
                judged.cited = vec![n];
            }
        }
        if self.tier == Tier::Tiny
            && !judged.not_sure
            && let Some(&(n, m)) = one_sided(&judged, facts).first()
        {
            judged = judge(&both_sides(n, m, facts), facts, system);
        }
        Ok(judged)
    }

    /// The whole reply to `messages`, read before any of it is shown: a reply that ends in the
    /// not-sure marker is an answer the mind did not stand behind, and none of it may reach the
    /// person. Answers are a sentence or two, so this costs little time.
    async fn reply_to(
        &self,
        model: &str,
        messages: &[Message],
        step: &str,
    ) -> Result<String, MindError> {
        self.reply_sampled(model, messages, step, &self.sampling.answer)
            .await
    }

    async fn reply_sampled(
        &self,
        model: &str,
        messages: &[Message],
        step: &str,
        sampling: &crate::client::Sampling,
    ) -> Result<String, MindError> {
        let mut req = ChatRequest::new(model, messages.to_vec()).sampled(sampling);
        req.max_tokens = Some(prompts::REPLY_TOKENS);
        let mut stream = match self.client.chat(&req).await {
            Ok(s) => s,
            Err(e) => {
                self.forget_loaded().await;
                return Err(e.into());
            }
        };
        let mut reply = String::new();
        while let Some(item) = stream.next().await {
            match item {
                Ok(delta) => reply.push_str(&delta),
                Err(_) => {
                    self.forget_loaded().await;
                    return Err(MindError::Unavailable);
                }
            }
        }
        raw_reply(step, &reply);
        Ok(reply)
    }

    async fn read_image_impl(&self, image: &Path) -> Result<Understanding, MindError> {
        let path = image.to_path_buf();
        let uri = tokio::task::spawn_blocking(move || image_data_uri(&path))
            .await
            .map_err(|e| MindError::Failed(e.to_string()))??;
        let uri = uri.as_str();
        self.with_mind(
            Work::Reading,
            |_| true,
            |m| Box::pin(async move { self.read_image_with(&m, image, uri).await }),
        )
        .await
    }

    async fn read_image_with(
        &self,
        model: &str,
        image: &Path,
        uri: &str,
    ) -> Result<Understanding, MindError> {
        let mut req = ChatRequest::new(
            model,
            vec![
                Message::system(&prompts::image_system()),
                Message::user_with_image(prompts::IMAGE_ASK, uri),
            ],
        )
        .sampled(&self.sampling.image);
        req.max_tokens = Some(prompts::IMAGE_TOKENS);
        #[derive(Deserialize)]
        struct Reading {
            #[serde(default)]
            title: String,
            #[serde(default)]
            text: String,
            #[serde(default)]
            summary: String,
            #[serde(default)]
            language: String,
        }
        let reading: Reading = match self.client.chat_json(&req, &prompts::image_schema()).await {
            Ok(r) => r,
            Err(e) => return Err(self.map_chat_error(e).await),
        };
        let title = if reading.title.trim().is_empty() {
            image
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default()
        } else {
            reading.title.trim().to_string()
        };
        Ok(Understanding {
            title,
            text: reading.text.trim().to_string(),
            summary: non_empty(reading.summary),
            language: non_empty(reading.language),
            pages: None,
        })
    }

    async fn map_chat_error(&self, e: RuntimeError) -> MindError {
        if matches!(e, RuntimeError::Unavailable | RuntimeError::StreamCut) {
            self.forget_loaded().await;
        }
        e.into()
    }

    async fn headline_impl(&self, page: &PageView) -> Result<(String, String), MindError> {
        self.with_mind(
            Work::Answering,
            |_| true,
            |m| Box::pin(async move { self.headline_with(&m, page).await }),
        )
        .await
    }

    async fn headline_with(
        &self,
        model: &str,
        page: &PageView,
    ) -> Result<(String, String), MindError> {
        let nonce = prompts::nonce();
        let mut req = ChatRequest::new(
            model,
            vec![
                Message::system(&prompts::headline_system(&nonce)),
                Message::user(&prompts::headline_user(page, &nonce)),
            ],
        )
        .sampled(&self.sampling.headline);
        req.max_tokens = Some(prompts::HEADLINE_TOKENS);
        #[derive(Deserialize)]
        struct Headline {
            headline: String,
            dek: String,
        }
        let h: Headline = match self
            .client
            .chat_json(&req, &prompts::headline_schema())
            .await
        {
            Ok(h) => h,
            Err(e) => return Err(self.map_chat_error(e).await),
        };
        let (headline, dek) = (h.headline.trim().to_string(), h.dek.trim().to_string());
        if headline.is_empty() {
            return Err(MindError::Failed("the headline came back empty".into()));
        }
        Ok((headline, dek))
    }

    async fn reflect_impl(&self, input: &ReflectInput) -> Result<Reflected, MindError> {
        if input.items.is_empty() {
            return Ok(Reflected {
                sentences: Vec::new(),
                written: 0,
            });
        }
        let nonce = prompts::nonce();
        let system = prompts::reflect_system(self.tier, &nonce);
        let user = prompts::reflect_user(self.tier, input, &nonce);
        let (system, user) = (system.as_str(), user.as_str());
        self.with_mind(
            Work::Answering,
            |_| true,
            |m| {
                Box::pin(async move {
                    let messages = [Message::system(system), Message::user(user)];
                    let reply = self
                        .reply_sampled(&m, &messages, "reflect", &self.sampling.reflection)
                        .await?;
                    Ok(reflect::sentences(&reply, input.items.len(), system))
                })
            },
        )
        .await
    }

    async fn transcribe_impl(&self, audio: &Path) -> Result<Transcript, MindError> {
        if audio::estimate_seconds(audio) > MAX_AUDIO_SECONDS {
            return Err(MindError::TooLong);
        }
        let t = match self.transcribe_on_npu(audio).await {
            Some(read) => read?,
            None => {
                let model = self.installed(ComponentRole::Speech).await?;
                self.client.transcribe(audio, &model, None).await?
            }
        };
        if t.duration.is_some_and(|d| d > MAX_AUDIO_SECONDS) {
            return Err(MindError::TooLong);
        }
        let ms = |s: f64| (s.max(0.0) * 1000.0).round() as u32;
        let segments: Vec<(u32, u32, String)> = t
            .segments
            .iter()
            .map(|s| (ms(s.start), ms(s.end), s.text.trim().to_string()))
            .collect();
        let duration_ms = t
            .duration
            .map(ms)
            .or_else(|| segments.last().map(|s| s.1))
            .unwrap_or(0);
        Ok(Transcript {
            text: t.text.trim().to_string(),
            language: t.language.filter(|l| !l.is_empty()),
            duration_ms,
            segments,
        })
    }

    /// Reads a recording on the NPU when that is where recordings go. A failure there is read
    /// again by the tier's own speech model; when that works, the NPU is left alone until the
    /// engine next starts, and when it fails too, its error is the one returned. `None` means
    /// the recording is for the tier's own model.
    async fn transcribe_on_npu(&self, audio: &Path) -> Option<Result<Transcription, MindError>> {
        let npu = self.npu.as_ref()?;
        let generation = self.client.generation();
        let model = npu.speech_model(generation)?;
        if self.installed_model(model.to_string()).await.is_err() {
            let listed = self.client.models().await.ok()?;
            if !listed.iter().any(|m| m.id == model && m.present()) {
                npu.not_fetched(generation);
            }
            return None;
        }
        match self.client.transcribe(audio, model, None).await {
            Ok(t) => {
                npu.worked();
                Some(Ok(t))
            }
            Err(e) => {
                let own = match self.installed(ComponentRole::Speech).await {
                    Ok(own) => own,
                    Err(err) => return Some(Err(err)),
                };
                let t = match self.client.transcribe(audio, &own, None).await {
                    Ok(t) => t,
                    Err(err) => return Some(Err(err.into())),
                };
                npu.failed(generation, &engine_message(&e));
                Some(Ok(t))
            }
        }
    }
}

/// The engine's own message in an error, without Nocturne's words around it.
fn engine_message(e: &RuntimeError) -> String {
    match e {
        RuntimeError::Server { message, .. }
        | RuntimeError::Rejected { message, .. }
        | RuntimeError::LoadFailed { message, .. } => message.clone(),
        other => other.to_string(),
    }
}

fn non_empty(s: String) -> Option<String> {
    let t = s.trim();
    (!t.is_empty()).then(|| t.to_string())
}

fn image_data_uri(path: &Path) -> Result<String, MindError> {
    let bytes = std::fs::read(path).map_err(|e| MindError::Failed(e.to_string()))?;
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    let (bytes, mime) = match image::load_from_memory(&bytes) {
        Ok(img) if img.width().max(img.height()) > MAX_IMAGE_EDGE => {
            let small = img.resize(
                MAX_IMAGE_EDGE,
                MAX_IMAGE_EDGE,
                image::imageops::FilterType::Triangle,
            );
            let mut out = std::io::Cursor::new(Vec::new());
            let jpeg = matches!(ext.as_str(), "jpg" | "jpeg");
            let format = if jpeg {
                image::ImageFormat::Jpeg
            } else {
                image::ImageFormat::Png
            };
            let prepared = if jpeg {
                image::DynamicImage::ImageRgb8(small.to_rgb8())
            } else {
                small
            };
            prepared
                .write_to(&mut out, format)
                .map_err(|e| MindError::Failed(e.to_string()))?;
            (
                out.into_inner(),
                if jpeg { "image/jpeg" } else { "image/png" },
            )
        }
        Ok(_) => (bytes, mime_for(&ext)),
        Err(_) => match ext.as_str() {
            "gif" | "webp" | "bmp" => (bytes, mime_for(&ext)),
            _ => return Err(MindError::Unsupported(format!("image type .{ext}"))),
        },
    };
    let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    Ok(format!("data:{mime};base64,{b64}"))
}

fn mime_for(ext: &str) -> &'static str {
    match ext {
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        _ => "image/png",
    }
}

pub use nocturne_mind::read::REPLY_TARGET;

/// A reading that found something: an empty one from the NPU is read again by the GPU mind.
fn non_empty_reading(found: &[Candidate]) -> bool {
    !found.is_empty()
}

fn raw_reply(step: &str, reply: &str) {
    read::trace_reply(step, reply);
}

/// `req` asking for a reply by the JSON schema instead of the grammar.
fn by_schema(req: &mut ChatRequest) {
    req.grammar = None;
    req.response_format = Some(crate::client::json_schema_format(
        &prompts::extraction_schema(),
    ));
}

/// One reading reply from the engine, with the model [`LemonadeMind::with_mind`] gave.
struct ReadWith<'a> {
    mind: &'a LemonadeMind,
    model: &'a str,
}

impl read::Reads for ReadWith<'_> {
    fn reply<'a>(
        &'a self,
        turns: &'a [Turn],
        item: &'a str,
        ceiling: u32,
    ) -> BoxFuture<'a, Result<read::Reply, MindError>> {
        Box::pin(async move {
            let req = self.mind.extract_request(self.model, turns, ceiling);
            self.mind.reading_reply(req, item).await
        })
    }
}

impl Mind for LemonadeMind {
    fn transcribe<'a>(&'a self, audio: &'a Path) -> BoxFuture<'a, Result<Transcript, MindError>> {
        Box::pin(self.transcribe_impl(audio))
    }

    fn read_image<'a>(
        &'a self,
        image: &'a Path,
    ) -> BoxFuture<'a, Result<Understanding, MindError>> {
        Box::pin(self.read_image_impl(image))
    }

    fn read_document<'a>(
        &'a self,
        doc: &'a Path,
    ) -> BoxFuture<'a, Result<Understanding, MindError>> {
        Box::pin(async move {
            let path = doc.to_path_buf();
            tokio::task::spawn_blocking(move || documents::read(&path))
                .await
                .map_err(|e| MindError::Failed(e.to_string()))?
        })
    }

    fn extract<'a>(
        &'a self,
        text: &'a str,
        cx: &'a ExtractContext,
    ) -> BoxFuture<'a, Result<Vec<Candidate>, MindError>> {
        Box::pin(self.extract_impl(text, cx))
    }

    fn extract_lines<'a>(
        &'a self,
        text: &'a str,
        cx: &'a ExtractContext,
        lines: &'a [String],
    ) -> BoxFuture<'a, Result<Vec<Candidate>, MindError>> {
        Box::pin(self.extract_lines_impl(text, cx, lines))
    }

    fn embed<'a>(&'a self, texts: &'a [String]) -> BoxFuture<'a, Result<Vec<Vec<f32>>, MindError>> {
        Box::pin(async move {
            if texts.is_empty() {
                return Ok(Vec::new());
            }
            let model = self.installed(ComponentRole::Recall).await?;
            Ok(self.client.embeddings(&model, texts).await?)
        })
    }

    fn embed_with<'a>(
        &'a self,
        model: &'a str,
        texts: &'a [String],
    ) -> BoxFuture<'a, Result<Vec<Vec<f32>>, MindError>> {
        Box::pin(async move {
            if texts.is_empty() {
                return Ok(Vec::new());
            }
            let model = self.installed_model(model.to_string()).await?;
            Ok(self.client.embeddings(&model, texts).await?)
        })
    }

    fn embed_query<'a>(&'a self, question: &'a str) -> BoxFuture<'a, Result<Vec<f32>, MindError>> {
        LemonadeMind::embed_query(self, question)
    }

    fn answer<'a>(
        &'a self,
        question: &'a str,
        facts: &'a [Fact],
        out: mpsc::Sender<String>,
    ) -> BoxFuture<'a, Result<AnswerMeta, MindError>> {
        Box::pin(self.answer_impl(question, facts, out))
    }

    fn write_headline<'a>(
        &'a self,
        page: &'a PageView,
    ) -> BoxFuture<'a, Result<(String, String), MindError>> {
        Box::pin(self.headline_impl(page))
    }

    fn reflect<'a>(
        &'a self,
        input: &'a ReflectInput,
    ) -> BoxFuture<'a, Result<Reflected, MindError>> {
        Box::pin(self.reflect_impl(input))
    }

    fn decide<'a>(
        &'a self,
        decision: &'a nocturne_engine::decide::Decision,
    ) -> BoxFuture<'a, Result<nocturne_engine::decide::Answer, MindError>> {
        Box::pin(self.decide_impl(decision))
    }

    fn speak<'a>(
        &'a self,
        text: &'a str,
        voice: &'a str,
    ) -> BoxFuture<'a, Result<Vec<u8>, MindError>> {
        Box::pin(async move {
            let model = self.installed(ComponentRole::Voice).await?;
            Ok(self.client.speech(&model, voice, text).await?)
        })
    }

    fn ready_to_speak(&self) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            if let Ok(model) = self.model(ComponentRole::Voice)
                && self.client.make_room(&model).await
            {
                self.forget_loaded().await;
            }
        })
    }
}
