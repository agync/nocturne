//! [`LemonadeMind`]: the Nocturne mind served by the private engine.

mod audio;
mod cite;
mod decide;
mod documents;
mod guard;
pub mod online;
mod prompts;
mod reflect;
mod sampling;
mod watch;

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use base64::Engine as _;
use futures_util::StreamExt;
use jiff::civil::Date;
use nocturne_contract::{ComponentRole, EntityKind, PageView, Span, Tier};
use nocturne_engine::coverage::{self, normalise};
use nocturne_engine::{
    AnswerMeta, BoxFuture, Candidate, Clock, ExtractContext, Fact, Mind, MindError, Predicate,
    ReflectInput, Reflected, Transcript, Understanding,
};
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::{Mutex, mpsc};

use crate::client::{ChatRequest, Client, LoadOptions, Message, Transcription};
use crate::error::RuntimeError;
use crate::npu::{Npu, Placement, Work};
use crate::selection::Picks;

use cite::CiteFilter;

const MAX_IMAGE_EDGE: u32 = 2048;
const MAX_AUDIO_SECONDS: f64 = 2.0 * 3600.0;
const EXTRACT_ATTEMPTS: usize = 2;
/// How many times a reading cut off at its token ceiling is halved and read again in parts.
const MAX_SPLITS: u32 = 2;
/// The ceiling every reading had before a short note got a lower one.
const ROOMY: u32 = 1024;

/// The token ceiling for reading `text`: room for a memory or two per sentence, and no more,
/// so a reply that loops stops early. A short note gets about six tokens a character (a memory
/// takes some 130, and a dense note holds one every 20 characters or so), never under 512.
fn extract_tokens(text: &str) -> u32 {
    let chars = text.chars().count();
    let long = (512 + chars * 3 / 2).clamp(1024, 3072);
    (384 + chars * 6).clamp(512, long) as u32
}

/// What has been checked or loaded since the engine last (re)started.
#[derive(Default)]
struct Seen {
    generation: u64,
    installed: HashSet<String>,
    loaded: HashSet<String>,
}

/// A reading reply.
struct Reading {
    /// What to read memories from: the whole reply, or the part before it began to loop.
    text: String,
    /// The whole reply as it came, for the reply trace.
    full: String,
    /// The engine stopped it at the token ceiling.
    cut: bool,
    /// It was ended early because it began to loop.
    looped: bool,
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
    async fn reading_reply(&self, mut req: ChatRequest, item: &str) -> Result<Reading, MindError> {
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

    async fn watched(&self, req: &ChatRequest, item: &str) -> Result<Reading, RuntimeError> {
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
        Ok(Reading {
            text,
            full: reply.text,
            cut: reply.cut,
            looped: watch.why.is_some(),
        })
    }

    /// A reading request: the item's grammar (a JSON schema on the NPU, whose runtime ignores a
    /// grammar without saying so), and a ceiling that suits `text`.
    /// `roomy` gives a short note the ceiling every reading had before short notes got a lower
    /// one.
    fn extract_request(
        &self,
        model: &str,
        messages: Vec<Message>,
        text: &str,
        roomy: bool,
    ) -> ChatRequest {
        let mut req = ChatRequest::new(model, messages).sampled(&self.sampling.extract);
        let ceiling = extract_tokens(text);
        req.max_tokens = Some(if roomy { ceiling.max(ROOMY) } else { ceiling });
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
                |m| Box::pin(async move { self.extract_part(&m, text, cx, 0, false).await }),
            )
            .await;
        tracing::info!("a reading took {} requests", self.reading_calls() - before);
        found
    }

    /// One reading of `text`, asked again once for the memories that were not valid. A reply
    /// that loops, is cut off at its ceiling, or is as full as a reply may be, is kept when its
    /// whole memories keep every fact of `text`; otherwise `text` is read again in halves,
    /// `depth` times at most. `roomy` as for [`Self::extract_request`].
    fn extract_part<'a>(
        &'a self,
        model: &'a str,
        text: &'a str,
        cx: &'a ExtractContext,
        depth: u32,
        roomy: bool,
    ) -> BoxFuture<'a, Result<Vec<Candidate>, MindError>> {
        Box::pin(async move {
            let nonce = prompts::nonce();
            let mut messages = vec![
                Message::system(&prompts::extraction_system(&nonce)),
                Message::user(&prompts::extraction_user(text, cx, &nonce)),
            ];
            let mut last_error = String::new();
            // What a reply got right is kept; a retry asks again only for what was wrong.
            let mut kept: Vec<Candidate> = Vec::new();
            for attempt in 0..EXTRACT_ATTEMPTS {
                let req = self.extract_request(model, messages.clone(), text, roomy);
                let reply = self.reading_reply(req, text).await?;
                raw_reply(
                    if depth == 0 {
                        "extract"
                    } else {
                        "extract_half"
                    },
                    &reply.full,
                );
                let parsed = parse_candidates(&reply.text, text);
                let cut = !reply.looped
                    && (reply.cut || parsed.as_ref().is_err_and(|e| e.contains("EOF while")));
                let full = match &parsed {
                    Ok(p) if p.total >= prompts::MAX_MEMORIES => Some(p.items.clone()),
                    _ => None,
                };
                if reply.looped || cut || full.is_some() {
                    let whole = merged(kept, full.unwrap_or_else(|| salvage(&reply.text, text)));
                    if covered(text, &whole, cx) {
                        tracing::info!(
                            "a reading that looped, was cut off or was full kept every fact; \
                             not reading it in parts"
                        );
                        return Ok(whole);
                    }
                    // A short note cut off at its lower ceiling, and that cannot be split, is
                    // read once more with the room every note had before.
                    let roomier = cut && !roomy && extract_tokens(text) < ROOMY;
                    return self
                        .read_in_parts(model, text, cx, depth, roomier, whole)
                        .await;
                }
                match parsed {
                    Ok(p) if p.invalid.is_empty() => return Ok(merged(kept, p.items)),
                    Ok(p) if attempt + 1 == EXTRACT_ATTEMPTS => {
                        tracing::warn!(
                            "dropped {} invalid memories: {:?}",
                            p.invalid.len(),
                            p.invalid
                        );
                        return Ok(merged(kept, p.items));
                    }
                    Ok(p) => {
                        kept = merged(kept, p.items);
                        last_error = p.invalid.join("; ");
                    }
                    Err(e) if attempt + 1 == EXTRACT_ATTEMPTS => {
                        if !kept.is_empty() {
                            tracing::warn!("the retry was unreadable; keeping the first answer");
                            return Ok(kept);
                        }
                        return Err(MindError::Failed(format!("unusable extraction: {e}")));
                    }
                    Err(e) => {
                        // The parser's words can quote the model; the retry names none of them.
                        tracing::warn!("an unreadable extraction: {e}");
                        last_error = "the reply did not match the format".into();
                    }
                }
                messages.push(Message {
                    role: "assistant".into(),
                    content: Value::String(reply.full),
                });
                messages.push(Message::user(&format!(
                    "That was not valid: {last_error}. Reply again with JSON only, matching the schema, \
                     with just those memories corrected; the others are kept."
                )));
            }
            Err(MindError::Failed(format!(
                "unusable extraction: {last_error}"
            )))
        })
    }

    /// A reply to `text` looped, was cut off, or held as many memories as a reply may, and
    /// left a fact out: each half of `text` is read on its own. When it cannot be split again,
    /// it is read once more with more room if `roomier`, or else `whole` (the complete memories
    /// the reply held) is kept.
    async fn read_in_parts(
        &self,
        model: &str,
        text: &str,
        cx: &ExtractContext,
        depth: u32,
        roomier: bool,
        whole: Vec<Candidate>,
    ) -> Result<Vec<Candidate>, MindError> {
        let halves = if depth < MAX_SPLITS {
            halves(text)
        } else {
            None
        };
        let Some((first, second)) = halves else {
            if roomier {
                tracing::warn!("a reading was cut off; reading the item again with more room");
                return match self.extract_part(model, text, cx, depth, true).await {
                    Ok(found) => Ok(merged(whole, found)),
                    Err(MindError::Failed(_)) if !whole.is_empty() => Ok(whole),
                    Err(e) => Err(e),
                };
            }
            tracing::warn!(
                "a reading was cut off or full; kept {} whole memories",
                whole.len()
            );
            if whole.is_empty() {
                return Err(MindError::Failed("the reading was cut off".into()));
            }
            return Ok(whole);
        };
        tracing::warn!("a reading was cut off or full; reading the item in two parts");
        let mut out = whole;
        let mut failed = None;
        for part in [first, second] {
            match self.extract_part(model, part, cx, depth + 1, false).await {
                Ok(found) => out = merged(out, found),
                Err(MindError::Failed(why)) => failed = Some(why),
                Err(e) => return Err(e),
            }
        }
        match failed {
            Some(why) if out.is_empty() => Err(MindError::Failed(why)),
            _ => Ok(out),
        }
    }

    /// Another look at `text` for the facts in `lines`: the whole item, with those lines
    /// pointed out. A reply that is cut off or full has the lines asked about in halves, once.
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
                |m| Box::pin(async move { self.focused(&m, text, cx, lines, 0).await }),
            )
            .await;
        tracing::info!(
            "a second look took {} requests",
            self.reading_calls() - before
        );
        found
    }

    fn focused<'a>(
        &'a self,
        model: &'a str,
        text: &'a str,
        cx: &'a ExtractContext,
        lines: &'a [String],
        depth: u32,
    ) -> BoxFuture<'a, Result<Vec<Candidate>, MindError>> {
        Box::pin(async move {
            let nonce = prompts::nonce();
            let messages = vec![
                Message::system(&prompts::extraction_system(&nonce)),
                Message::user(&prompts::focus_user(text, cx, &nonce, lines)),
            ];
            let req = self.extract_request(model, messages, &lines.join("\n"), false);
            let reply = self.reading_reply(req, text).await?;
            raw_reply("extract_second_look", &reply.full);
            if reply.looped {
                return Ok(salvage(&reply.text, text));
            }
            let parsed = parse_candidates(&reply.text, text);
            let cut = reply.cut || parsed.as_ref().is_err_and(|e| e.contains("EOF while"));
            let full = parsed
                .as_ref()
                .is_ok_and(|p| p.total >= prompts::MAX_MEMORIES);
            if (cut || full) && depth == 0 && lines.len() > 1 {
                let (a, b) = lines.split_at(lines.len() / 2);
                let mut out = Vec::new();
                for part in [a, b] {
                    out = merged(out, self.focused(model, text, cx, part, 1).await?);
                }
                return Ok(out);
            }
            match parsed {
                Ok(p) => Ok(p.items),
                Err(_) if cut => Ok(salvage(&reply.text, text)),
                Err(e) => Err(MindError::Failed(format!("unusable second look: {e}"))),
            }
        })
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
        req.max_tokens = Some(1024);
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
                Message::user_with_image("Describe and read this image.", uri),
            ],
        )
        .sampled(&self.sampling.image);
        req.max_tokens = Some(1024);
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
        req.max_tokens = Some(200);
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

#[derive(Deserialize)]
struct Extraction {
    memories: Vec<Value>,
}

/// `more` added to `kept`, leaving out what `kept` already says.
fn merged(mut kept: Vec<Candidate>, more: Vec<Candidate>) -> Vec<Candidate> {
    let key = |c: &Candidate| {
        (
            normalise(&c.subject),
            c.predicate.as_str().to_string(),
            normalise(&c.object),
        )
    };
    for c in more {
        if !kept.iter().any(|k| key(k) == key(&c)) {
            kept.push(c);
        }
    }
    kept
}

/// Where a model's raw replies are traced, for evaluation runs: at trace level on a target of
/// its own, so a log kept at info never holds the words of a memory.
pub const REPLY_TARGET: &str = "nocturne::reply";

/// A reading that found something: an empty one from the NPU is read again by the GPU mind.
fn non_empty_reading(found: &[Candidate]) -> bool {
    !found.is_empty()
}

fn raw_reply(step: &str, reply: &str) {
    tracing::trace!(target: REPLY_TARGET, step, reply = %reply);
}

/// `req` asking for a reply by the JSON schema instead of the grammar.
fn by_schema(req: &mut ChatRequest) {
    req.grammar = None;
    req.response_format = Some(crate::client::json_schema_format(
        &prompts::extraction_schema(),
    ));
}

/// Whether `found` keeps all of `text`: every number, name and role in it, by the check the
/// reader gives a reading before a second look, and every sentence of it quoted, or the reading
/// carried on to its last sentence. The first check alone misses a sentence with no number or
/// name in it, and counts a name another memory happens to hold.
fn covered(text: &str, found: &[Candidate], cx: &ExtractContext) -> bool {
    let known: Vec<String> = cx.known_entities.iter().map(|(n, _)| n.clone()).collect();
    if found.is_empty() || !coverage::left_out(text, found, &known).is_empty() {
        return false;
    }
    let quote = |c: &Candidate| match &c.span {
        Some(Span::Quote { text }) => normalise(text),
        _ => String::new(),
    };
    let quotes: Vec<String> = found.iter().map(quote).filter(|q| !q.is_empty()).collect();
    let sentences: Vec<String> = coverage::sentences(text)
        .iter()
        .map(|s| normalise(s))
        .filter(|s| !s.is_empty())
        .collect();
    let holds = |s: &str, q: &str| s.contains(q) || q.contains(s);
    let every = sentences.iter().all(|s| quotes.iter().any(|q| holds(s, q)));
    let to_the_end = match (sentences.last(), found.last().map(quote)) {
        (Some(last), Some(q)) => !q.is_empty() && holds(last, &q),
        _ => false,
    };
    every || to_the_end
}

/// `text` in two parts of about the same length, split between lines or else between
/// sentences. `None` for a single sentence.
fn halves(text: &str) -> Option<(&str, &str)> {
    let middle = text.len() / 2;
    let mut cuts: Vec<usize> = text.match_indices('\n').map(|(i, _)| i + 1).collect();
    if cuts.iter().all(|&c| c >= text.trim_end().len()) {
        cuts = text
            .match_indices(['.', '!', '?'])
            .map(|(i, _)| i + 1)
            .filter(|&i| text[i..].starts_with(char::is_whitespace))
            .collect();
    }
    let cut = cuts
        .into_iter()
        .filter(|&c| !text[..c].trim().is_empty() && !text[c..].trim().is_empty())
        .min_by_key(|&c| c.abs_diff(middle))?;
    Some((text[..cut].trim(), text[cut..].trim()))
}

/// The whole, valid and grounded memories in a reply that was cut off part way.
fn salvage(reply: &str, text: &str) -> Vec<Candidate> {
    let source = normalise(text);
    watch::Memories::default()
        .whole(reply)
        .into_iter()
        .filter_map(|r| serde_json::from_str::<Value>(&reply[r]).ok())
        .filter_map(|raw| candidate(&raw).ok())
        .filter(|c| grounded(c, &source))
        .map(|c| own_dates(dated(c, text), text))
        .collect()
}

/// A memory must be able to point at the words it came from.
fn grounded(c: &Candidate, source: &str) -> bool {
    match &c.span {
        Some(Span::Quote { text }) => {
            let quote = normalise(text);
            !quote.is_empty() && source.contains(&quote)
        }
        _ => false,
    }
}

/// What a reply came to.
#[derive(Debug)]
struct Parsed {
    /// Valid memories whose quote is really in the item.
    items: Vec<Candidate>,
    /// A note for each memory that did not validate.
    invalid: Vec<String>,
    /// How many memories the reply held, whatever came of them.
    total: usize,
}

/// The memories in `reply` to `text`. Memories quoting words that are not there are dropped
/// quietly, and a day the item never names is taken off the memory that gives it.
fn parse_candidates(reply: &str, text: &str) -> Result<Parsed, String> {
    let parsed: Extraction = crate::client::parse_json_text(reply).map_err(|e| e.to_string())?;
    let source = normalise(text);
    let mut items = Vec::new();
    let mut invalid = Vec::new();
    for (i, raw) in parsed.memories.iter().enumerate() {
        match candidate(raw) {
            Ok(c) if grounded(&c, &source) => items.push(own_dates(dated(c, text), text)),
            Ok(c) => tracing::warn!("dropped an ungrounded memory about {}", c.subject),
            Err(why) => invalid.push(format!("memory {}: {why}", i + 1)),
        }
    }
    Ok(Parsed {
        items,
        invalid,
        total: parsed.memories.len(),
    })
}

/// `c` with each date its sentence writes as YYYY-MM-DD, where the item does not, put back in
/// the words the item writes that day in ("1 December"): the person's own words are what an
/// answer gives and a search finds. The object is left as it is: the store tells memories
/// apart by it, and one object for a day must not become two.
fn own_dates(mut c: Candidate, text: &str) -> Candidate {
    c.sentence = restored(&c.sentence, text);
    c
}

fn restored(words: &str, text: &str) -> String {
    let mut out = words.to_string();
    let bytes = words.as_bytes();
    let digit = |i: usize| bytes.get(i).is_some_and(u8::is_ascii_digit);
    for at in (0..bytes.len().saturating_sub(9)).rev() {
        let shaped = (0..10).all(|k| {
            if k == 4 || k == 7 {
                bytes[at + k] == b'-'
            } else {
                digit(at + k)
            }
        });
        if !shaped || (at > 0 && digit(at - 1)) || digit(at + 10) {
            continue;
        }
        let iso = &words[at..at + 10];
        if text.contains(iso) {
            continue;
        }
        if let Ok(date) = iso.parse::<Date>()
            && let Some(own) = guard::written_day(date, text)
        {
            out.replace_range(at..at + 10, &own);
        }
    }
    out
}

/// `c` without a `due` that neither its quote nor the line it stands in (with the heading
/// above that line) names.
fn dated(mut c: Candidate, text: &str) -> Candidate {
    if let Some(due) = c.due {
        let quote = match &c.span {
            Some(Span::Quote { text }) => text.as_str(),
            _ => "",
        };
        if !guard::day_supported(due, &around(quote, text)) {
            c.due = None;
        }
    }
    c
}

/// `quote` with the line of `text` it stands in and the nearest line with words above that.
fn around(quote: &str, text: &str) -> String {
    let q = normalise(quote);
    let lines: Vec<&str> = text.lines().collect();
    let at = lines.iter().position(|l| {
        let l = normalise(l);
        !l.is_empty() && !q.is_empty() && (l.contains(&q) || q.contains(&l))
    });
    let mut out = quote.to_string();
    if let Some(i) = at {
        out.push_str(" | ");
        out.push_str(lines[i]);
        if let Some(heading) = lines[..i].iter().rev().find(|l| !l.trim().is_empty()) {
            out.push_str(" | ");
            out.push_str(heading);
        }
    }
    out
}

fn text_field(raw: &Value, key: &str) -> Result<String, String> {
    match raw[key].as_str().map(str::trim) {
        Some(s) if !s.is_empty() => Ok(s.to_string()),
        _ => Err(format!("{key} is missing")),
    }
}

fn date_field(raw: &Value, key: &str) -> Result<Option<Date>, String> {
    match raw[key].as_str().map(str::trim) {
        None | Some("") => Ok(None),
        Some(s) => s
            .parse::<Date>()
            .map(Some)
            .map_err(|_| format!("{key} is not a YYYY-MM-DD date")),
    }
}

/// Words that stand for someone or something named elsewhere: a memory about one of them cannot
/// say who or what it is about.
const PRONOUNS: [&str; 16] = [
    "he", "she", "it", "they", "him", "her", "them", "his", "hers", "its", "their", "theirs",
    "this", "that", "these", "those",
];

/// The reason names no words of the model's own: the retry goes back as the person's turn.
fn named_subject(subject: &str) -> Result<(), String> {
    if PRONOUNS.contains(&subject.trim().to_lowercase().as_str()) {
        return Err("the subject is a pronoun; name who or what it stands for".into());
    }
    Ok(())
}

fn candidate(raw: &Value) -> Result<Candidate, String> {
    let sentence = text_field(raw, "sentence")?;
    let subject = text_field(raw, "subject")?;
    named_subject(&subject)?;
    let object = text_field(raw, "object")?;
    let predicate = predicate(&text_field(raw, "predicate")?);
    let confidence = raw["confidence"]
        .as_f64()
        .ok_or("confidence is missing")?
        .clamp(0.0, 1.0) as f32;
    let subject_kind = match raw["subjectKind"].as_str() {
        Some("person") => Some(EntityKind::Person),
        Some("place") => Some(EntityKind::Place),
        Some("thing") => Some(EntityKind::Thing),
        _ => None,
    };
    let span = raw["span"]["quote"]
        .as_str()
        .map(str::trim)
        .filter(|q| !q.is_empty())
        .map(|q| Span::Quote {
            text: q.to_string(),
        });
    let tags = raw["tags"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|t| t.as_str())
                .map(|t| t.trim().to_lowercase())
                .filter(|t| !t.is_empty())
                .collect()
        })
        .unwrap_or_default();
    Ok(Candidate {
        subject,
        subject_kind,
        predicate,
        object,
        sentence,
        valid_from: date_field(raw, "validFrom")?,
        valid_to: date_field(raw, "validTo")?,
        due: date_field(raw, "due")?,
        confidence,
        span,
        tags,
        moved: raw["moved"].as_bool().unwrap_or(false),
    })
}

fn predicate(raw: &str) -> Predicate {
    let norm = raw.trim().to_lowercase().replace([' ', '-'], "_");
    match norm.strip_prefix("other:") {
        Some(word) => Predicate::Other(word.trim().to_string()),
        None => Predicate::parse(&norm),
    }
}

/// What both the local and the online answer do with a judged reply: the text goes out only
/// when the answer stands, and each cited number maps back to the fact it was given as.
async fn conclude(
    judged: Judged,
    facts: &[Fact],
    out: &mpsc::Sender<String>,
) -> Result<AnswerMeta, MindError> {
    let mut sent_any = false;
    if !judged.not_sure && !judged.text.is_empty() {
        sent_any = true;
        if out.send(judged.text.clone()).await.is_err() {
            return Err(MindError::Failed("the reader went away".into()));
        }
    }
    let ids = |numbers: &[usize]| -> Vec<String> {
        numbers
            .iter()
            .filter_map(|n| facts.get(n - 1))
            .map(|f| f.memory.id.clone())
            .collect()
    };
    let (cited, spans) = if judged.not_sure {
        (Vec::new(), Vec::new())
    } else {
        (
            ids(&judged.cited),
            judged.spans.as_deref().map(ids).unwrap_or_default(),
        )
    };
    let cited_none = cited.is_empty();
    Ok(AnswerMeta {
        cited,
        not_sure: judged.not_sure || !sent_any || cited_none,
        spans,
        online: None,
    })
}

/// What a reply comes to once its markers are taken out and the guards have looked at it.
struct Judged {
    /// What the person would read.
    text: String,
    not_sure: bool,
    /// The facts cited, from 1, in the order first cited.
    cited: Vec<usize>,
    /// The fact each bold span stands for, when they can all be paired.
    spans: Option<Vec<usize>>,
}

impl Judged {
    fn cited_sentences<'a>(&self, facts: &'a [Fact]) -> Vec<&'a str> {
        self.cited
            .iter()
            .filter_map(|n| facts.get(n - 1))
            .map(|f| f.memory.sentence.as_str())
            .collect()
    }
}

/// The cited facts that disagree with a fact the answer does not give: (cited, other side),
/// both from 1. A side is given when it is cited and the answer says what sets it apart from
/// the other side ([`guard::carries`]); a marker alone does not give it.
fn one_sided(judged: &Judged, facts: &[Fact]) -> Vec<(usize, usize)> {
    let given = |n: usize, m: usize| {
        let sentence = |i: usize| facts.get(i - 1).map(|f| f.memory.sentence.as_str());
        judged.cited.contains(&n)
            && sentence(n).is_some_and(|s| {
                guard::carries(
                    &judged.text,
                    s,
                    &sentence(m).into_iter().collect::<Vec<_>>(),
                )
            })
    };
    let mut out: Vec<(usize, usize)> = Vec::new();
    for &n in &judged.cited {
        let Some(fact) = facts.get(n - 1) else {
            continue;
        };
        for id in &fact.disputed_by {
            let Some(m) = facts.iter().position(|f| &f.memory.id == id).map(|i| i + 1) else {
                continue;
            };
            let seen = out.iter().any(|&(a, b)| (a, b) == (m, n));
            if !seen && !(given(n, m) && given(m, n)) {
                out.push((n, m));
            }
        }
    }
    out
}

/// What is wrong with an answer a Tiny mind gave, in the words it is told: it cites nothing, it
/// leaves out a number, date or time of a cited fact, or it cites a fact that disagrees with
/// another and not the other.
fn complaints(judged: &Judged, facts: &[Fact]) -> Vec<String> {
    let mut out = Vec::new();
    if judged.cited.is_empty() {
        out.push(
            "Write a whole sentence and end it with the number of the fact it comes from, like [m1]."
                .to_string(),
        );
    }
    let left_out = guard::left_out(&judged.text, &judged.cited_sentences(facts));
    if !left_out.is_empty() {
        out.push(format!(
            "You left out {}. Write {} exactly as the fact does.",
            left_out.join(", "),
            if left_out.len() == 1 { "it" } else { "them" }
        ));
    }
    for (n, m) in one_sided(judged, facts) {
        out.push(format!(
            "[m{n}] does not agree with [m{m}]. Give both, and say it is not settled which is right."
        ));
    }
    out
}

/// Both sides of a disagreement in the facts' own words, for when the mind gave one twice.
fn both_sides(n: usize, m: usize, facts: &[Fact]) -> String {
    let sentence = |i: usize| facts[i - 1].memory.sentence.trim().to_string();
    format!(
        "{} [m{n}] {} [m{m}] It is not settled which is right.",
        sentence(n),
        sentence(m)
    )
}

fn judge(reply: &str, facts: &[Fact], system: &str) -> Judged {
    let reply = guard::strip_control(reply);
    let mut filter = CiteFilter::new(facts.len());
    let mut text = filter.push(&reply);
    text.push_str(&filter.finish());
    let cited = filter.cited().to_vec();
    // Nothing but punctuation, or the instructions read back, is not an answer.
    let not_sure = guard::not_sure(&reply, !cited.is_empty()) || guard::degenerate(&text, system);
    Judged {
        text,
        not_sure,
        cited,
        spans: filter.bold_facts(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_worked_example_is_a_reply_the_mind_would_keep_whole() {
        let p = parse_candidates(prompts::EXAMPLE_REPLY, prompts::EXAMPLE_NOTE).unwrap();
        assert!(p.invalid.is_empty(), "{:?}", p.invalid);
        assert_eq!(p.items.len(), 3, "every memory in it is grounded");
        assert_eq!(p.total, 3);
        let dues: Vec<Option<Date>> = p.items.iter().map(|c| c.due).collect();
        assert_eq!(
            dues,
            [
                Some("2026-03-06".parse().unwrap()),
                Some("2026-03-13".parse().unwrap()),
                None
            ],
            "every day it gives is one the note names"
        );
        let system = prompts::extraction_system("n");
        assert!(system.contains(prompts::EXAMPLE_NOTE) && system.contains(prompts::EXAMPLE_REPLY));
    }

    /// Who someone is to the person stays in the memory, so "who is" has something to cite.
    #[test]
    fn who_someone_is_stays_in_the_sentence() {
        let items = parse_candidates(prompts::EXAMPLE_REPLY, prompts::EXAMPLE_NOTE)
            .unwrap()
            .items;
        assert!(
            items
                .iter()
                .any(|c| c.sentence.starts_with("My cousin Ana"))
        );
        let system = prompts::extraction_system("n");
        assert!(system.contains("Keep the words that say who someone is to the person"));
    }

    /// The grammar writes each memory's keys in reading order, the quote first, and the worked
    /// example the prompt shows is written the same way.
    #[test]
    fn memories_are_written_in_reading_order_within_bounds() {
        let grammar = prompts::extraction_grammar();
        let memory = grammar
            .lines()
            .find(|l| l.starts_with("memory ::="))
            .unwrap();
        let at: Vec<usize> = prompts::MEMORY_KEYS
            .iter()
            .map(|k| memory.find(&format!("\"\\\"{k}\\\"\"")).unwrap())
            .collect();
        assert!(at.windows(2).all(|w| w[0] < w[1]), "{memory}");
        assert!(grammar.contains("char{1,300}"), "{grammar}");
        assert!(grammar.contains("(\",\" ws memory){0,15}"), "{grammar}");
        let example: Value = serde_json::from_str(prompts::EXAMPLE_REPLY).unwrap();
        let reply = prompts::EXAMPLE_REPLY;
        let first = &example["memories"][0];
        let mut last = 0;
        for k in prompts::MEMORY_KEYS {
            assert!(first.get(k).is_some(), "{k}");
            let pos = reply.find(&format!("\"{k}\":")).unwrap();
            assert!(pos > last || k == "span", "{k} out of order");
            last = pos;
        }
    }

    #[test]
    fn a_date_rewritten_by_the_mind_is_put_back_in_the_items_words() {
        // Tiny's reading of the allotment newsletter in a real run.
        let text = "Riverside Allotment Society, autumn newsletter\n\nOur plot is number 17, next \
                    to the water tap.\n\nThe water is switched off for winter on 1 December.\n\n\
                    The annual fee of 30 pounds is due by 15 January.";
        assert_eq!(
            restored("The water is switched off for winter on 2026-12-01.", text),
            "The water is switched off for winter on 1 December."
        );
        assert_eq!(restored("by 2026-01-15", text), "by 15 January");
        assert_eq!(
            restored("Kept on 2026-11-12, due 2026-01-15.", text),
            "Kept on 2026-11-12, due 15 January.",
            "a day the item does not write stays as it is"
        );
        assert_eq!(restored("on 2026-01-150", text), "on 2026-01-150");
    }

    #[test]
    fn only_the_sentence_gets_the_items_words_for_a_day() {
        let text = "The water is switched off for winter on 1 December.";
        let raw = serde_json::json!({
            "span": {"quote": "The water is switched off for winter on 1 December"},
            "sentence": "The water is switched off on 2026-12-01.", "subject": "Water",
            "predicate": "scheduled", "object": "2026-12-01", "due": "2026-12-01",
            "confidence": 0.9,
        });
        let c = own_dates(candidate(&raw).unwrap(), text);
        assert_eq!(c.sentence, "The water is switched off on 1 December.");
        assert_eq!(
            c.object, "2026-12-01",
            "the object the store tells memories apart by"
        );
    }

    #[test]
    fn a_day_the_item_does_not_name_is_dropped_and_the_memory_kept() {
        let reply = |due: &str, quote: &str| {
            json!({"memories": [{"span": {"quote": quote}, "sentence": "Book club meets.",
                "subject": "Book club", "subjectKind": "thing", "predicate": "scheduled",
                "object": "first Tuesday", "due": due, "validFrom": null, "validTo": null,
                "moved": false, "confidence": 0.9, "tags": []}]})
            .to_string()
        };
        let text = "Book club\nIt meets on the first Tuesday.";
        let p = parse_candidates(&reply("2026-11-12", "It meets on the first Tuesday"), text);
        assert_eq!(p.unwrap().items[0].due, None);
        // The heading above the quoted line names the day.
        let text = "Tuesday 17 November\nBook club at the library.";
        let p = parse_candidates(&reply("2026-11-17", "Book club at the library"), text);
        assert_eq!(p.unwrap().items[0].due, Some("2026-11-17".parse().unwrap()));
    }

    #[test]
    fn the_whole_memories_of_a_cut_reply_are_kept() {
        let note = prompts::EXAMPLE_NOTE;
        let cut = &prompts::EXAMPLE_REPLY[..prompts::EXAMPLE_REPLY.len() - 60];
        assert!(parse_candidates(cut, note).is_err());
        let kept = salvage(cut, note);
        assert_eq!(kept.len(), 2, "{kept:?}");
        assert_eq!(kept[1].subject, "Dr Patel");
        assert!(salvage("{\"memories\": [{\"span\": {\"quote\": \"a}b", note).is_empty());
    }

    #[test]
    fn an_item_halves_between_lines_or_else_between_sentences() {
        assert_eq!(
            halves("one\ntwo\nthree\nfour"),
            Some(("one\ntwo", "three\nfour"))
        );
        assert_eq!(
            halves("First fact here. Second fact. Third fact here."),
            Some(("First fact here. Second fact.", "Third fact here."))
        );
        assert_eq!(halves("One sentence only."), None);
        assert_eq!(halves("One line\n"), None);
    }

    #[test]
    fn the_token_ceiling_grows_with_the_item_and_stays_bounded() {
        assert_eq!(extract_tokens("Sam likes tea"), 512);
        assert_eq!(extract_tokens(&"x".repeat(69)), 798);
        assert_eq!(extract_tokens(&"x".repeat(112)), 1024);
        assert_eq!(extract_tokens(&"x".repeat(1000)), 2012);
        assert_eq!(extract_tokens(&"x".repeat(6000)), 3072);
    }

    #[test]
    fn every_predicate_offered_is_one_the_engine_knows() {
        for (name, _) in prompts::PREDICATES {
            assert!(!predicate(name).is_other(), "{name}");
        }
    }

    #[test]
    fn a_pronoun_is_not_a_subject_but_a_name_that_starts_like_one_is() {
        for bad in ["He", " they", "It", "this"] {
            assert!(named_subject(bad.trim()).is_err(), "{bad}");
        }
        for good in [
            "Tom",
            "Gran's birthday",
            "Theatre trip",
            "Hermione",
            "Itinerary",
            "Her Majesty's Passport Office",
            "His Majesty's Theatre",
        ] {
            assert!(named_subject(good).is_ok(), "{good}");
        }
    }
}
