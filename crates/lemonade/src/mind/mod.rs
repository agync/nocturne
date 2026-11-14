//! [`LemonadeMind`]: the Nocturne mind served by the private engine.

mod audio;
mod cite;
mod documents;
mod guard;
pub mod online;
mod prompts;

use std::collections::HashSet;
use std::path::Path;

use base64::Engine as _;
use futures_util::StreamExt;
use jiff::civil::Date;
use nocturne_contract::{ComponentRole, EntityKind, PageView, Span, Tier};
use nocturne_engine::{
    AnswerMeta, BoxFuture, Candidate, ExtractContext, Fact, Mind, MindError, Predicate, Transcript,
    Understanding,
};
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::{Mutex, mpsc};

use crate::client::{ChatRequest, Client, LoadOptions, Message};
use crate::error::RuntimeError;
use crate::selection::Picks;

use cite::CiteFilter;

const MAX_IMAGE_EDGE: u32 = 2048;
const MAX_AUDIO_SECONDS: f64 = 2.0 * 3600.0;
const EXTRACT_ATTEMPTS: usize = 2;

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
    overrides: Vec<(ComponentRole, String)>,
    seen: Mutex<Seen>,
}

impl LemonadeMind {
    /// A mind using `picks` for each job; `tier` sets how the answering model is run.
    pub fn new(client: Client, picks: Picks, tier: Tier) -> Self {
        LemonadeMind {
            client,
            picks,
            tier,
            overrides: Vec::new(),
            seen: Mutex::new(Seen::default()),
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

    /// Loads the thinking model once per engine launch, with a context size to suit the tier.
    async fn warm_mind(&self) -> Result<String, MindError> {
        let model = self.installed(ComponentRole::Mind).await?;
        let mut seen = self.seen.lock().await;
        if !seen.loaded.contains(&model) {
            let ctx = crate::residency::mind_context(self.tier);
            self.client
                .load(
                    &model,
                    &LoadOptions {
                        ctx_size: Some(ctx),
                        ..LoadOptions::default()
                    },
                )
                .await
                .map_err(|e| match e {
                    RuntimeError::Rejected { .. } => MindError::Unavailable,
                    other => other.into(),
                })?;
            seen.loaded.insert(model.clone());
        }
        Ok(model)
    }

    async fn forget_loaded(&self) {
        self.seen.lock().await.loaded.clear();
    }

    async fn chat_text(&self, req: &ChatRequest) -> Result<String, MindError> {
        match self.client.chat_complete(req).await {
            Ok(t) => Ok(t),
            Err(e) => {
                if matches!(e, RuntimeError::Unavailable | RuntimeError::StreamCut) {
                    self.forget_loaded().await;
                }
                Err(e.into())
            }
        }
    }

    async fn extract_impl(
        &self,
        text: &str,
        cx: &ExtractContext,
    ) -> Result<Vec<Candidate>, MindError> {
        if text.trim().is_empty() {
            return Ok(Vec::new());
        }
        let model = self.warm_mind().await?;
        let nonce = prompts::nonce();
        let source = normalise(text);
        let mut messages = vec![
            Message::system(&prompts::extraction_system(&nonce)),
            Message::user(&prompts::extraction_user(text, cx, &nonce)),
        ];
        let mut last_error = String::new();
        // What a reply got right is kept; a retry asks again only for what was wrong.
        let mut kept: Vec<Candidate> = Vec::new();
        for attempt in 0..EXTRACT_ATTEMPTS {
            let mut req = ChatRequest::new(&model, messages.clone());
            req.temperature = Some(0.0);
            req.max_tokens = Some(2048);
            req.timeout = Some(self.client.timeouts().json);
            req.response_format = Some(crate::client::json_schema_format(
                &prompts::extraction_schema(),
            ));
            let reply = self.chat_text(&req).await?;
            match parse_candidates(&reply, &source) {
                Ok((items, invalid)) if invalid.is_empty() => return Ok(merged(kept, items)),
                Ok((items, invalid)) if attempt + 1 == EXTRACT_ATTEMPTS => {
                    tracing::warn!("dropped {} invalid memories: {invalid:?}", invalid.len());
                    return Ok(merged(kept, items));
                }
                Ok((items, invalid)) => {
                    kept = merged(kept, items);
                    last_error = invalid.join("; ");
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
                content: Value::String(reply),
            });
            messages.push(Message::user(&format!(
                "That was not valid: {last_error}. Reply again with JSON only, matching the schema, \
                 with just those memories corrected; the others are kept."
            )));
        }
        Err(MindError::Failed(format!(
            "unusable extraction: {last_error}"
        )))
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
        let model = self.warm_mind().await?;
        let nonce = prompts::nonce();
        let shown = prompts::facts_for(self.tier, facts);
        let facts = shown.as_slice();
        let system = prompts::answer_system(self.tier, &nonce);
        let mut messages = vec![
            Message::system(&system),
            Message::user(&prompts::answer_user(self.tier, question, facts, &nonce)),
        ];
        let mut reply = self.reply_to(&model, &messages).await?;
        let mut judged = judge(&reply, facts, &system);
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
                if let Ok(second) = self.reply_to(&model, &messages).await {
                    let again = judge(&second, facts, &system);
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
            judged = judge(&both_sides(n, m, facts), facts, &system);
        }
        conclude(judged, facts, &out).await
    }

    /// The whole reply to `messages`, read before any of it is shown: a reply that ends in the
    /// not-sure marker is an answer the mind did not stand behind, and none of it may reach the
    /// person. Answers are a sentence or two, so this costs little time.
    async fn reply_to(&self, model: &str, messages: &[Message]) -> Result<String, MindError> {
        let mut req = ChatRequest::new(model, messages.to_vec());
        req.temperature = Some(0.2);
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
        Ok(reply)
    }

    async fn read_image_impl(&self, image: &Path) -> Result<Understanding, MindError> {
        let model = self.warm_mind().await?;
        let path = image.to_path_buf();
        let uri = tokio::task::spawn_blocking(move || image_data_uri(&path))
            .await
            .map_err(|e| MindError::Failed(e.to_string()))??;
        let mut req = ChatRequest::new(
            &model,
            vec![
                Message::system(&prompts::image_system()),
                Message::user_with_image("Describe and read this image.", &uri),
            ],
        );
        req.temperature = Some(0.0);
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
        let model = self.warm_mind().await?;
        let nonce = prompts::nonce();
        let mut req = ChatRequest::new(
            &model,
            vec![
                Message::system(&prompts::headline_system(&nonce)),
                Message::user(&prompts::headline_user(page, &nonce)),
            ],
        );
        req.temperature = Some(0.4);
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

    async fn transcribe_impl(&self, audio: &Path) -> Result<Transcript, MindError> {
        if audio::estimate_seconds(audio) > MAX_AUDIO_SECONDS {
            return Err(MindError::TooLong);
        }
        let model = self.installed(ComponentRole::Speech).await?;
        let t = self.client.transcribe(audio, &model, None).await?;
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

/// Lower case, with every run of non-alphanumeric characters made one space.
fn normalise(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut gap = true;
    for c in text.chars() {
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
            gap = false;
        } else if !gap {
            out.push(' ');
            gap = true;
        }
    }
    out.truncate(out.trim_end().len());
    out
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

/// Valid candidates whose quote is really in `source` (already normalised), and a note for each
/// item that did not validate. Items quoting words that are not there are dropped quietly.
fn parse_candidates(reply: &str, source: &str) -> Result<(Vec<Candidate>, Vec<String>), String> {
    let parsed: Extraction = crate::client::parse_json_text(reply).map_err(|e| e.to_string())?;
    let mut items = Vec::new();
    let mut invalid = Vec::new();
    for (i, raw) in parsed.memories.iter().enumerate() {
        match candidate(raw) {
            Ok(c) if grounded(&c, source) => items.push(c),
            Ok(c) => tracing::warn!("dropped an ungrounded memory about {}", c.subject),
            Err(why) => invalid.push(format!("memory {}: {why}", i + 1)),
        }
    }
    Ok((items, invalid))
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

/// The cited facts that disagree with a fact the answer does not cite: (cited, other side),
/// both from 1.
fn one_sided(judged: &Judged, facts: &[Fact]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for n in &judged.cited {
        let Some(fact) = facts.get(n - 1) else {
            continue;
        };
        for id in &fact.disputed_by {
            let other = facts.iter().position(|f| &f.memory.id == id).map(|i| i + 1);
            if let Some(m) = other.filter(|m| !judged.cited.contains(m)) {
                out.push((*n, m));
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
    // Nothing but punctuation, or the instructions read back, is not an answer.
    let not_sure = guard::not_sure(&reply) || guard::degenerate(&text, system);
    Judged {
        text,
        not_sure,
        cited: filter.cited().to_vec(),
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

    #[test]
    fn the_worked_example_is_a_reply_the_mind_would_keep_whole() {
        let source = normalise(prompts::EXAMPLE_NOTE);
        let (items, invalid) = parse_candidates(prompts::EXAMPLE_REPLY, &source).unwrap();
        assert!(invalid.is_empty(), "{invalid:?}");
        assert_eq!(items.len(), 3, "every memory in it is grounded");
        let system = prompts::extraction_system("n");
        assert!(system.contains(prompts::EXAMPLE_NOTE) && system.contains(prompts::EXAMPLE_REPLY));
    }

    /// Who someone is to the person stays in the memory, so "who is" has something to cite.
    #[test]
    fn who_someone_is_stays_in_the_sentence() {
        let source = normalise(prompts::EXAMPLE_NOTE);
        let (items, _) = parse_candidates(prompts::EXAMPLE_REPLY, &source).unwrap();
        assert!(
            items
                .iter()
                .any(|c| c.sentence.starts_with("My cousin Ana"))
        );
        let system = prompts::extraction_system("n");
        assert!(system.contains("Keep the words that say who someone is to the person"));
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
