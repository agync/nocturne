//! [`ProviderMind`]: the Nocturne mind on the person's online provider, for a device whose own
//! core makes the calls (a phone alone, ADR 0020). It is the computer's mind in every word: the
//! prompts, the reading loop, the guards, the receipts and the judging all come from
//! `nocturne-mind`, and only the carrying is here: OpenAI-compatible chat requests to the one host
//! of a [`Provider`].
//!
//! Each job is a typed step, never a general chat: reading a thing's words, reading a photo or a
//! recording, answering a question from the few lines found, writing the page's headline and a
//! day's reflection. A photo or recording the provider refuses waits to be read
//! ([`MindError::NotTaken`]); after two in a row none is sent again until the mind is made anew.
//! A thing the provider refuses to read is counted against that thing, so the rest is read. It
//! makes no vectors and no speech: finding stays by words, and reading aloud needs a mind on the
//! device.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;

use base64::Engine as _;
use nocturne_contract::PageView;
use nocturne_engine::{
    AnswerMeta, BoxFuture, Candidate, ExtractContext, Fact, Mind, MindError, OnlineFailure,
    OnlineReceipt, ReflectInput, Reflected, Transcript, Understanding,
};
use nocturne_mind::judge::{conclude, judge};
use nocturne_mind::read::{self, Turn};
use nocturne_mind::{online, prompts, reading, reflect, temperature, watch};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::mpsc;

use crate::client::{Exchange, Failed, Provider, Reply, Stop};
use crate::lookup::{
    Answered, Asked, FIRST_LINES, LOOK_UP_SECONDS, LookUps, MAX_LINES, MAX_LOOK_UPS, MAX_PER_ROUND,
    MAX_REQUESTS, MAX_ROUNDS, Part, Parts, Tools,
};
use nocturne_engine::AskToday;
use nocturne_mind::lookup::{self as look, LookUp, Place};

const READ_SECONDS: u64 = 180;
const ANSWER_SECONDS: u64 = 60;
const WRITE_SECONDS: u64 = 120;
/// The longest recording sent to be heard, in bytes.
const MAX_RECORDING: u64 = 25 << 20;

/// What a request was for, as the Sent online list names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Job {
    /// The connection check, or its look at the list of models.
    Check,
    /// Reading a thing's words into memories.
    Reading,
    Photo,
    Recording,
    Answer,
    /// The page's headline.
    Page,
    Reflection,
}

/// Called after every request the mind makes, whatever came of it.
pub type Record = Arc<dyn Fn(Job, &Exchange) + Send + Sync>;

/// Called for a request that never left, with why: no way to the internet, an address refused,
/// or stopped before it went.
pub type Unsent = Arc<dyn Fn(Job, OnlineFailure) + Send + Sync>;

/// Turns a recording the provider does not take as it is (m4a, aac, webm) into WAV bytes, as
/// the device's own decoder can; `None` where it cannot.
pub type Decode = Arc<dyn Fn(&Path) -> Option<Vec<u8>> + Send + Sync>;

pub struct ProviderMind {
    provider: Provider,
    record: Option<Record>,
    unsent: Option<Unsent>,
    decode: Option<Decode>,
    /// The provider refused a reply by a JSON schema once: replies are asked for by the
    /// prompt alone from then on.
    schema_refused: AtomicBool,
    /// Photos, then recordings, refused since one was last taken. Two in a row means the
    /// provider takes none, and none is sent again.
    photos_refused: AtomicU32,
    sound_refused: AtomicU32,
    /// Whether the provider takes look-ups, and who to tell when that is learned.
    tools: std::sync::Mutex<Tools>,
    on_tools: Option<Arc<dyn Fn(Tools) + Send + Sync>>,
    /// Questions in a row whose look-ups were all broken.
    broken_in_a_row: AtomicU32,
}

/// One question on a phone alone's mind: the question, the lines found (the words' lines, then
/// the date step's), today, and the look-ups when the question takes the look-up path.
pub struct Asking<'a> {
    /// Names the question's parts on the Sent online list.
    pub ask: &'a str,
    pub question: &'a str,
    pub facts: &'a [Fact],
    pub today: &'a AskToday,
    pub look_ups: Option<&'a LookUps>,
    pub parts: Parts<'a>,
    pub stop: Option<Stop>,
}

/// Questions in a row with only broken look-ups that mean a provider does not take them.
const BROKEN_IN_A_ROW: u32 = 2;

/// Today as this device's clock and zone have it, for an answer asked without one.
fn today_here() -> AskToday {
    let now = jiff::ZonedDateTime::now();
    AskToday {
        day: now.date(),
        hour: now.hour(),
        minute: now.minute(),
        zone: now
            .time_zone()
            .iana_name()
            .map_or_else(|| format!("UTC{}", now.offset()), str::to_string),
        week_start: jiff::civil::Weekday::Monday,
    }
}

/// What a reply is: an answer, a set of look-ups, look-ups that do not read, or nothing.
enum Replied {
    Answer(String),
    Looks(Vec<Value>),
    /// Look-ups asked for in a shape that does not read: no name, or no id.
    Broken,
    /// Nothing to show: an empty reply, or one cut off before it said anything.
    Neither,
}

fn said_of(reply: &Reply, offered: bool) -> Replied {
    if offered && !reply.tool_calls.is_empty() {
        let named = reply
            .tool_calls
            .iter()
            .all(|c| c["function"]["name"].is_string() && c["id"].is_string());
        return if named {
            Replied::Looks(reply.tool_calls.clone())
        } else {
            Replied::Broken
        };
    }
    if reply.text.trim().is_empty() {
        Replied::Neither
    } else {
        Replied::Answer(reply.text.clone())
    }
}

/// How many parts of what a request carries `messages` make, as the Sent online list counts
/// them: one for each message with words.
fn said_count(messages: &[Value]) -> u32 {
    messages.iter().filter(|m| m["content"].is_string()).count() as u32
}

fn places(looks: &[LookUp]) -> Vec<Place> {
    let mut out: Vec<Place> = Vec::new();
    for l in looks {
        if !out.contains(&l.place()) {
            out.push(l.place());
        }
    }
    out
}

/// The things kept and the memories whose words a request was handed. A thing being read is the
/// reader's to name; the mind names what it was handed.
#[derive(Debug, Default)]
struct Handed {
    items: Vec<String>,
    memories: Vec<String>,
    part: Option<Part>,
}

impl Handed {
    fn of_facts(facts: &[Fact]) -> Handed {
        let mut carried = Handed::default();
        for f in facts {
            if !nocturne_engine::is_source_line(&f.memory.id) {
                carried.memories.push(f.memory.id.clone());
            }
            carried.add_items(f.memory.receipts.iter().map(|r| &r.item_id));
        }
        carried
    }

    fn of_page(page: &PageView) -> Handed {
        let mut carried = Handed::default();
        for line in page
            .needs_you
            .iter()
            .chain(&page.changed)
            .chain(&page.let_go)
            .chain(&page.upcoming)
            .chain(&page.learned)
        {
            for id in &line.memory_ids {
                if !carried.memories.contains(id) {
                    carried.memories.push(id.clone());
                }
            }
            carried.add_items(line.receipts.iter().map(|r| &r.item_id));
        }
        carried
    }

    fn add_items<'a>(&mut self, ids: impl Iterator<Item = &'a String>) {
        for id in ids {
            if !self.items.contains(id) {
                self.items.push(id.clone());
            }
        }
    }
}

/// Refusals in a row, of different things, that mean a provider takes no photos or no sound.
const REFUSED_IN_A_ROW: u32 = 2;

#[derive(Deserialize)]
struct ImageReading {
    #[serde(default)]
    title: String,
    #[serde(default)]
    text: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    language: String,
}

#[derive(Deserialize)]
struct Headline {
    headline: String,
    dek: String,
}

fn online(f: Failed) -> MindError {
    MindError::Online(f.failure)
}

/// What a failed request to read one thing means. The provider's refusal of that thing (a
/// 400, 413 or 422, or a reply that could not be read) is the thing's, counted like any reading
/// that failed, so the rest is read meanwhile. A 400 that names the model may be the model
/// missing or the thing too long for it: the thing waits, uncounted. Anything else is about the
/// mind (its key, credit, reach or load), and the reader waits for it.
fn reading_failure(f: Failed) -> MindError {
    match f.exchange.status {
        _ if f.refused => online(f),
        Some(400) if f.failure == OnlineFailure::ModelMissing => {
            MindError::NotTaken("this thing, for now".into())
        }
        Some(s @ (400 | 413 | 422 | 200..=299)) => {
            MindError::Failed(format!("the provider refused this thing (status {s})"))
        }
        _ => online(f),
    }
}

fn non_empty(s: String) -> Option<String> {
    let t = s.trim();
    (!t.is_empty()).then(|| t.to_string())
}

fn turns_json(turns: &[Turn]) -> Value {
    Value::Array(
        turns
            .iter()
            .map(|t| json!({"role": t.role.as_str(), "content": t.text}))
            .collect(),
    )
}

impl ProviderMind {
    pub fn new(provider: Provider) -> ProviderMind {
        ProviderMind {
            provider,
            record: None,
            unsent: None,
            decode: None,
            schema_refused: AtomicBool::new(false),
            photos_refused: AtomicU32::new(0),
            sound_refused: AtomicU32::new(0),
            tools: std::sync::Mutex::new(Tools::Unknown),
            on_tools: None,
            broken_in_a_row: AtomicU32::new(0),
        }
    }

    /// Starts from what is known of look-ups, and tells `learned` whenever it learns more.
    pub fn with_tools(
        mut self,
        known: Tools,
        learned: Arc<dyn Fn(Tools) + Send + Sync>,
    ) -> ProviderMind {
        self.tools = std::sync::Mutex::new(known);
        self.on_tools = Some(learned);
        self
    }

    /// Takes `known` as what is known of look-ups, as a new check found it.
    pub fn know_tools(&self, known: Tools) {
        *self.tools.lock().unwrap_or_else(|e| e.into_inner()) = known;
        self.broken_in_a_row.store(0, Ordering::Relaxed);
    }

    /// What is known now of whether the provider takes look-ups.
    pub fn tools(&self) -> Tools {
        self.tools.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn learn_tools(&self, known: Tools) {
        let mut held = self.tools.lock().unwrap_or_else(|e| e.into_inner());
        if *held == known {
            return;
        }
        *held = known.clone();
        drop(held);
        if let Some(tell) = &self.on_tools {
            tell(known);
        }
    }

    /// Tells `record` of every request made, for the Sent online list.
    pub fn with_record(mut self, record: Record) -> ProviderMind {
        self.record = Some(record);
        self
    }

    /// Tells `unsent` of every request that never left.
    pub fn with_unsent(mut self, unsent: Unsent) -> ProviderMind {
        self.unsent = Some(unsent);
        self
    }

    /// Sends a recording in another format as WAV, through `decode`.
    pub fn with_decoder(mut self, decode: Decode) -> ProviderMind {
        self.decode = Some(decode);
        self
    }

    pub fn provider(&self) -> &Provider {
        &self.provider
    }

    fn note(&self, job: Job, exchange: &Exchange) {
        if let Some(record) = &self.record {
            record(job, exchange);
        }
    }

    async fn ask(&self, job: Job, body: Value, seconds: u64) -> Result<Reply, Failed> {
        self.ask_until(job, body, seconds, None, &Handed::default())
            .await
    }

    /// One request for `job`, ended by `stop`, noted with the things and memories it was handed.
    async fn ask_until(
        &self,
        job: Job,
        body: Value,
        seconds: u64,
        stop: Option<Stop>,
        carried: &Handed,
    ) -> Result<Reply, Failed> {
        let mut reply = self
            .provider
            .chat_until(body, Duration::from_secs(seconds), stop)
            .await;
        let exchange = match &mut reply {
            Ok(r) => Some(&mut r.exchange),
            Err(f) if !f.refused => Some(&mut f.exchange),
            Err(_) => None,
        };
        if let Some(exchange) = exchange {
            exchange.carried.items = carried.items.clone();
            exchange.carried.memories = carried.memories.clone();
            exchange.carried.part = carried.part.clone();
        }
        match &reply {
            Ok(r) => self.note(job, &r.exchange),
            Err(f) if !f.refused => self.note(job, &f.exchange),
            Err(f) => {
                if let Some(unsent) = &self.unsent {
                    unsent(job, f.failure);
                }
            }
        }
        reply
    }

    /// Answers one question on a phone alone, ended as soon as its `stop` turns true: nothing
    /// is asked once it has, and a request under way is dropped. With look-ups it takes the
    /// look-up path (docs/design/ask-with-tools.md); without, the date step path, in one
    /// request. Each part answered is counted, and the month's limit is looked at before each.
    pub async fn answer_asked(
        &self,
        asking: Asking<'_>,
        out: mpsc::Sender<String>,
    ) -> Result<Answered, MindError> {
        let started = tokio::time::Instant::now();
        // This device is the person's home: every line found may go.
        let all = |ids: &[String]| vec![true; ids.len()];
        let Some(q) = online::question_of(
            asking.question,
            asking.facts,
            FIRST_LINES,
            &all,
            asking.today,
            asking.look_ups.is_some() && self.tools().offered(),
        ) else {
            return Ok(Answered {
                meta: AnswerMeta {
                    not_sure: true,
                    ..AnswerMeta::default()
                },
                ..Answered::default()
            });
        };
        let look_ups = asking.look_ups.filter(|_| self.tools().offered());
        let limit = look_ups.map_or(Duration::from_secs(ANSWER_SECONDS), |l| l.limit);
        let mut system = q.system.clone();
        if look_ups.is_some() {
            system = format!("{system}\n{}", look::SYSTEM);
        }
        let first = vec![
            json!({"role": "system", "content": system}),
            json!({"role": "user", "content": q.user}),
        ];
        let mut messages = first.clone();
        let mut facts: Vec<Fact> = q.facts.clone();
        let mut asked_before: Vec<Asked> = Vec::new();
        let mut new_from = 0u32;
        let mut parts = 0u32;
        let mut rounds = 0u32;
        let mut looked = 0usize;
        let mut broken_only = None::<bool>;
        let mut offered = look_ups.is_some();
        let mut learn_refusal = false;
        loop {
            let n = parts + 1;
            if !(asking.parts.may_send)() {
                // The month's limit is reached between parts: no more go.
                return Err(MindError::Unavailable);
            }
            let left = limit.saturating_sub(started.elapsed());
            if left.is_zero() {
                return Err(MindError::Online(OnlineFailure::Slow));
            }
            let tools_now = offered && rounds < MAX_ROUNDS && n < MAX_REQUESTS;
            if offered && !tools_now {
                new_from = said_count(&messages);
                messages.push(json!({"role": "user", "content": look::ANSWER_NOW}));
            }
            let mut body = json!({
                "messages": messages,
                "temperature": temperature::ANSWER,
                "max_tokens": online::MAX_TOKENS,
            });
            if tools_now && let Some(l) = look_ups {
                body["tools"] = look::tools(&l.kinds);
                body["tool_choice"] = json!("auto");
            }
            let mut carried = Handed::of_facts(&facts);
            carried.part = Some(Part {
                ask: asking.ask.to_string(),
                n,
                asked: std::mem::take(&mut asked_before),
                from: new_from,
            });
            let seconds = left.as_secs().clamp(1, ANSWER_SECONDS);
            let reply = self
                .ask_until(Job::Answer, body, seconds, asking.stop.clone(), &carried)
                .await;
            let reply = match reply {
                Ok(r) => r,
                // Refused with look-ups offered, the first time: the date step path at once,
                // with the same lines, and remembered.
                // Refused with look-ups offered, naming them, the first time: the date step path
                // at once, with the same lines. It is remembered only once that is answered.
                Err(f)
                    if n == 1
                        && tools_now
                        && f.names_tools
                        && matches!(f.exchange.status, Some(400 | 422)) =>
                {
                    tracing::warn!("the provider refused look-ups; answering without them");
                    learn_refusal = true;
                    offered = false;
                    messages = vec![
                        json!({"role": "system", "content": q.system}),
                        json!({"role": "user", "content": q.user}),
                    ];
                    continue;
                }
                Err(f) => return Err(online(f)),
            };
            parts += 1;
            (asking.parts.counted)();
            // The same lines without look-ups were answered: the provider does not take them.
            if std::mem::take(&mut learn_refusal) {
                self.learn_tools(Tools::Refuses {
                    on: asking.today.day.to_string(),
                });
            }
            match said_of(&reply, tools_now) {
                Replied::Answer(text) => {
                    if let Some(only) = broken_only {
                        self.after_broken(only, asking.today.day);
                    }
                    let judged = judge(&text, &facts, &system);
                    let meta = conclude(judged, &facts, &out).await?;
                    return Ok(Answered {
                        meta,
                        facts,
                        parts,
                        nothing: false,
                    });
                }
                Replied::Broken | Replied::Neither if tools_now && n == 1 && rounds == 0 => {
                    // Not a well-formed answer or look-up: the date step path at once. Look-ups
                    // asked for in a shape that does not read are a refusal, remembered once the
                    // same lines are answered without them; an empty or cut reply is not.
                    learn_refusal = matches!(said_of(&reply, tools_now), Replied::Broken);
                    offered = false;
                    messages = vec![
                        json!({"role": "system", "content": q.system}),
                        json!({"role": "user", "content": q.user}),
                    ];
                }
                Replied::Broken | Replied::Neither => {
                    return Ok(Answered {
                        meta: AnswerMeta {
                            not_sure: true,
                            ..AnswerMeta::default()
                        },
                        facts,
                        parts,
                        nothing: false,
                    });
                }
                Replied::Looks(calls) => {
                    let Some(l) = look_ups else {
                        return Err(MindError::Failed("look-ups that were not offered".into()));
                    };
                    if matches!(self.tools(), Tools::Unknown) {
                        self.learn_tools(Tools::Takes {
                            on: asking.today.day.to_string(),
                        });
                    }
                    rounds += 1;
                    messages
                        .push(json!({"role": "assistant", "content": null, "tool_calls": calls}));
                    new_from = said_count(&messages);
                    // Each look-up asked for is answered, in the order asked.
                    let mut results: Vec<(String, String)> = Vec::new();
                    let mut wanted: Vec<(usize, LookUp)> = Vec::new();
                    let mut any_good = false;
                    let mut any_broken = false;
                    for (i, call) in calls.iter().enumerate() {
                        let id = call["id"].as_str().unwrap_or_default().to_string();
                        let f = &call["function"];
                        let name = f["name"].as_str().unwrap_or_default();
                        let args = match &f["arguments"] {
                            Value::String(a) => a.clone(),
                            other => other.to_string(),
                        };
                        if i >= MAX_PER_ROUND || looked >= MAX_LOOK_UPS {
                            results.push((id, look::TOO_MANY.to_string()));
                            continue;
                        }
                        looked += 1;
                        match look::parse(name, &args, asking.today.day) {
                            Ok(lookup) => {
                                any_good = true;
                                wanted.push((results.len(), lookup));
                                results.push((id, String::new()));
                            }
                            Err(w) => {
                                any_broken |= w.broken;
                                any_good |= !w.broken;
                                results.push((id, w.line));
                            }
                        }
                    }
                    broken_only = Some(match broken_only {
                        Some(only) => only && any_broken && !any_good,
                        None => any_broken && !any_good,
                    });
                    if !wanted.is_empty() {
                        (l.looking)(&places(
                            &wanted.iter().map(|(_, w)| w.clone()).collect::<Vec<_>>(),
                        ));
                    }
                    let mut all_empty = !wanted.is_empty();
                    for (at, lookup) in wanted {
                        let text = if facts.len() >= MAX_LINES {
                            asked_before.push(Asked::of(&lookup, 0));
                            look::NO_MORE.to_string()
                        } else {
                            let run = l.run.clone();
                            let job = lookup.clone();
                            let found = tokio::time::timeout(
                                LOOK_UP_SECONDS,
                                tokio::task::spawn_blocking(move || run(&job)),
                            )
                            .await;
                            match found {
                                Ok(Ok(Some((lines, more)))) => {
                                    let found_any = !lines.is_empty() || more > 0;
                                    let fresh: Vec<Fact> = lines
                                        .into_iter()
                                        .filter(|f| {
                                            !facts.iter().any(|h| h.memory.id == f.memory.id)
                                        })
                                        .take(MAX_LINES - facts.len())
                                        .collect();
                                    if found_any {
                                        all_empty = false;
                                    }
                                    // Lines found but all handed before are said to be so,
                                    // never "nothing kept".
                                    let empty = if found_any {
                                        look::ALREADY
                                    } else {
                                        lookup.nothing()
                                    };
                                    asked_before.push(Asked::of(&lookup, fresh.len()));
                                    let start = facts.len();
                                    let text = look::lines(
                                        prompts::ONLINE_TIER,
                                        &fresh,
                                        start,
                                        more,
                                        &q.nonce,
                                        empty,
                                    );
                                    facts.extend(fresh);
                                    text
                                }
                                // Not looked up is not "nothing kept": the question goes on.
                                _ => {
                                    all_empty = false;
                                    asked_before.push(Asked::of(&lookup, 0));
                                    look::COULD_NOT.to_string()
                                }
                            }
                        };
                        results[at].1 = text;
                    }
                    // Nothing came back, and the question has no lines at all: the mind has
                    // nothing it could cite, so nothing more is asked.
                    if all_empty && facts.is_empty() {
                        return Ok(Answered {
                            meta: AnswerMeta {
                                not_sure: true,
                                ..AnswerMeta::default()
                            },
                            facts,
                            parts,
                            nothing: true,
                        });
                    }
                    for (id, text) in results {
                        messages.push(json!({"role": "tool", "tool_call_id": id, "content": text}));
                    }
                }
            }
        }
    }

    /// After a question whose look-ups were all broken, or not: two in a row mean the provider
    /// does not take look-ups, until the next check.
    fn after_broken(&self, only: bool, day: jiff::civil::Date) {
        if !only {
            self.broken_in_a_row.store(0, Ordering::Relaxed);
        } else if self.broken_in_a_row.fetch_add(1, Ordering::Relaxed) + 1 >= BROKEN_IN_A_ROW {
            self.learn_tools(Tools::Refuses {
                on: day.to_string(),
            });
        }
    }

    /// `body` asking for a reply matching `schema`, and asked again by the prompt alone when
    /// the provider refuses it. Only when the second asking is answered is the schema taken to
    /// be what was refused, and not asked for again.
    async fn ask_shaped(
        &self,
        job: Job,
        mut body: Value,
        schema: &Value,
        seconds: u64,
        carried: &Handed,
    ) -> Result<Reply, Failed> {
        if self.schema_refused.load(Ordering::Relaxed) {
            return self.ask_until(job, body, seconds, None, carried).await;
        }
        body["response_format"] = prompts::response_format(schema);
        match self
            .ask_until(job, body.clone(), seconds, None, carried)
            .await
        {
            Err(f) if f.exchange.status == Some(400) => {
                if let Some(o) = body.as_object_mut() {
                    o.remove("response_format");
                }
                let again = self.ask_until(job, body, seconds, None, carried).await;
                if again.is_ok() {
                    tracing::warn!("the provider refused a reply by schema; asking by the prompt");
                    self.schema_refused.store(true, Ordering::Relaxed);
                }
                again
            }
            other => other,
        }
    }

    async fn read_image_impl(&self, image: &Path) -> Result<Understanding, MindError> {
        if self.photos_refused.load(Ordering::Relaxed) >= REFUSED_IN_A_ROW {
            return Err(MindError::NotTaken("photos".into()));
        }
        let path = image.to_path_buf();
        let uri = tokio::task::spawn_blocking(move || crate::image::image_data_uri(&path))
            .await
            .map_err(|e| MindError::Failed(e.to_string()))??;
        let body = json!({
            "messages": [
                {"role": "system", "content": prompts::image_system()},
                {"role": "user", "content": [
                    {"type": "text", "text": prompts::IMAGE_ASK},
                    {"type": "image_url", "image_url": {"url": uri}},
                ]},
            ],
            "temperature": temperature::IMAGE,
            "max_tokens": prompts::IMAGE_TOKENS,
        });
        let reply = match self
            .ask_shaped(
                Job::Photo,
                body,
                &prompts::image_schema(),
                READ_SECONDS,
                &Handed::default(),
            )
            .await
        {
            Ok(r) => {
                self.photos_refused.store(0, Ordering::Relaxed);
                r
            }
            Err(f) if f.exchange.status == Some(400) => {
                if self.photos_refused.fetch_add(1, Ordering::Relaxed) + 1 >= REFUSED_IN_A_ROW {
                    tracing::warn!("the provider does not take photos; they wait to be read");
                }
                return Err(MindError::NotTaken("this photo".into()));
            }
            Err(f) => return Err(reading_failure(f)),
        };
        let reading: ImageReading = reading::json_text(&reply.text)
            .map_err(|e| MindError::Failed(format!("an unexpected reply: {e}")))?;
        let title = match non_empty(reading.title) {
            Some(t) => t,
            None => image
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
        };
        Ok(Understanding {
            title,
            text: reading.text.trim().to_string(),
            summary: non_empty(reading.summary),
            language: non_empty(reading.language),
            pages: None,
        })
    }

    async fn transcribe_impl(&self, audio: &Path) -> Result<Transcript, MindError> {
        if self.sound_refused.load(Ordering::Relaxed) >= REFUSED_IN_A_ROW {
            return Err(MindError::NotTaken("recordings".into()));
        }
        // Providers take WAV and MP3 sound; anything else goes as WAV once decoded here.
        let (format, decode) = match audio
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("wav") => ("wav", None),
            Some("mp3") => ("mp3", None),
            _ => match &self.decode {
                Some(d) => ("wav", Some(d.clone())),
                None => return Err(MindError::NotTaken("this kind of recording".into())),
            },
        };
        let path = audio.to_path_buf();
        let data = tokio::task::spawn_blocking(move || -> Result<String, MindError> {
            let fail = |e: std::io::Error| MindError::Failed(e.to_string());
            let bytes = match decode {
                Some(decode) => decode(&path)
                    .ok_or_else(|| MindError::NotTaken("this kind of recording".into()))?,
                None => {
                    if std::fs::metadata(&path).map_err(fail)?.len() > MAX_RECORDING {
                        return Err(MindError::NotTaken("a recording this long".into()));
                    }
                    std::fs::read(&path).map_err(fail)?
                }
            };
            if bytes.len() as u64 > MAX_RECORDING {
                return Err(MindError::NotTaken("a recording this long".into()));
            }
            Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
        })
        .await
        .map_err(|e| MindError::Failed(e.to_string()))??;
        let body = json!({
            "messages": [
                {"role": "system", "content": prompts::transcript_system()},
                {"role": "user", "content": [
                    {"type": "text", "text": prompts::TRANSCRIPT_ASK},
                    {"type": "input_audio", "input_audio": {"data": data, "format": format}},
                ]},
            ],
            "temperature": temperature::EXTRACT,
        });
        let reply = match self.ask(Job::Recording, body, READ_SECONDS).await {
            Ok(r) => {
                self.sound_refused.store(0, Ordering::Relaxed);
                r
            }
            Err(f) if f.exchange.status == Some(400) => {
                if self.sound_refused.fetch_add(1, Ordering::Relaxed) + 1 >= REFUSED_IN_A_ROW {
                    tracing::warn!("the provider does not take recordings; they wait to be read");
                }
                return Err(MindError::NotTaken("this recording".into()));
            }
            Err(f) => return Err(reading_failure(f)),
        };
        Ok(Transcript {
            text: reply.text.trim().to_string(),
            ..Transcript::default()
        })
    }

    async fn answer_impl(
        &self,
        question: &str,
        facts: &[Fact],
        out: mpsc::Sender<String>,
        stop: Option<Stop>,
    ) -> Result<AnswerMeta, MindError> {
        // This device is the person's home: every line found may go.
        let all = |ids: &[String]| vec![true; ids.len()];
        let Some(q) = online::question(question, facts, &all, &today_here()) else {
            return Ok(AnswerMeta {
                not_sure: true,
                ..AnswerMeta::default()
            });
        };
        let body = json!({
            "messages": [
                {"role": "system", "content": q.system},
                {"role": "user", "content": q.user},
            ],
            "temperature": temperature::ANSWER,
            "max_tokens": online::MAX_TOKENS,
        });
        let mut receipt = OnlineReceipt {
            template: q.template,
            sent_ids: q.facts.iter().map(|f| f.memory.id.clone()).collect(),
            line_hashes: q.line_hashes,
            ..OnlineReceipt::default()
        };
        let carried = Handed::of_facts(&q.facts);
        let reply = self
            .ask_until(Job::Answer, body, ANSWER_SECONDS, stop, &carried)
            .await;
        let exchange = match &reply {
            Ok(r) => &r.exchange,
            Err(f) => &f.exchange,
        };
        receipt.status = exchange.status;
        receipt.bytes_up = exchange.bytes_up;
        receipt.bytes_down = exchange.bytes_down;
        receipt.seconds = exchange.seconds;
        receipt.cost_usd = exchange.cost_usd;
        let reply = reply.map_err(online)?;
        let judged = judge(&reply.text, &q.facts, &q.system);
        conclude(judged, &q.facts, &out).await.map(|mut m| {
            m.online = Some(receipt);
            m
        })
    }

    async fn headline_impl(&self, page: &PageView) -> Result<(String, String), MindError> {
        let nonce = prompts::nonce();
        let body = json!({
            "messages": [
                {"role": "system", "content": prompts::headline_system(&nonce)},
                {"role": "user", "content": prompts::headline_user(page, &nonce)},
            ],
            "temperature": temperature::HEADLINE,
            "max_tokens": prompts::HEADLINE_TOKENS,
        });
        let reply = self
            .ask_shaped(
                Job::Page,
                body,
                &prompts::headline_schema(),
                WRITE_SECONDS,
                &Handed::of_page(page),
            )
            .await
            .map_err(online)?;
        let h: Headline = reading::json_text(&reply.text)
            .map_err(|e| MindError::Failed(format!("an unexpected reply: {e}")))?;
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
        let system = prompts::reflect_system(prompts::ONLINE_TIER, &nonce);
        let user = prompts::reflect_user(prompts::ONLINE_TIER, input, &nonce);
        let body = json!({
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": user},
            ],
            "temperature": temperature::REFLECTION,
            "max_tokens": prompts::REPLY_TOKENS,
        });
        let carried = Handed {
            items: input.items.iter().map(|i| i.id.to_string()).collect(),
            ..Handed::default()
        };
        let reply = self
            .ask_until(Job::Reflection, body, WRITE_SECONDS, None, &carried)
            .await
            .map_err(online)?;
        read::trace_reply("reflect", &reply.text);
        Ok(reflect::sentences(&reply.text, input.items.len(), &system))
    }
}

impl read::Reads for ProviderMind {
    fn reply<'a>(
        &'a self,
        turns: &'a [Turn],
        item: &'a str,
        ceiling: u32,
    ) -> BoxFuture<'a, Result<read::Reply, MindError>> {
        Box::pin(async move {
            let body = json!({
                "messages": turns_json(turns),
                "temperature": temperature::EXTRACT,
                "max_tokens": ceiling,
            });
            let reply = self
                .ask_shaped(
                    Job::Reading,
                    body,
                    &prompts::extraction_schema(),
                    READ_SECONDS,
                    &Handed::default(),
                )
                .await
                .map_err(reading_failure)?;
            let prompt: Vec<&str> = turns.iter().map(|t| t.text.as_str()).collect();
            let mut watch = watch::Watch::new(item, &prompt.join("\n"));
            let looped = watch.looping(&reply.text);
            let text = if looped {
                watch.kept(&reply.text).to_string()
            } else {
                reply.text.clone()
            };
            Ok(read::Reply {
                text,
                full: reply.text,
                cut: reply.cut,
                looped,
            })
        })
    }
}

impl Mind for ProviderMind {
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
        _doc: &'a Path,
    ) -> BoxFuture<'a, Result<Understanding, MindError>> {
        Box::pin(async { Err(MindError::NotTaken("documents".into())) })
    }

    fn extract<'a>(
        &'a self,
        text: &'a str,
        cx: &'a ExtractContext,
    ) -> BoxFuture<'a, Result<Vec<Candidate>, MindError>> {
        Box::pin(read::extract(self, text, cx))
    }

    fn extract_lines<'a>(
        &'a self,
        text: &'a str,
        cx: &'a ExtractContext,
        lines: &'a [String],
    ) -> BoxFuture<'a, Result<Vec<Candidate>, MindError>> {
        Box::pin(read::second_look(self, text, cx, lines))
    }

    fn embed<'a>(
        &'a self,
        _texts: &'a [String],
    ) -> BoxFuture<'a, Result<Vec<Vec<f32>>, MindError>> {
        Box::pin(async { Err(MindError::Unsupported("vectors".into())) })
    }

    fn embed_query<'a>(&'a self, _question: &'a str) -> BoxFuture<'a, Result<Vec<f32>, MindError>> {
        Box::pin(async { Err(MindError::Unsupported("vectors".into())) })
    }

    fn answer<'a>(
        &'a self,
        question: &'a str,
        facts: &'a [Fact],
        out: mpsc::Sender<String>,
    ) -> BoxFuture<'a, Result<AnswerMeta, MindError>> {
        Box::pin(self.answer_impl(question, facts, out, None))
    }

    fn write_headline<'a>(
        &'a self,
        page: &'a PageView,
    ) -> BoxFuture<'a, Result<(String, String), MindError>> {
        Box::pin(self.headline_impl(page))
    }

    fn speak<'a>(
        &'a self,
        _text: &'a str,
        _voice: &'a str,
    ) -> BoxFuture<'a, Result<Vec<u8>, MindError>> {
        Box::pin(async { Err(MindError::Unsupported("speech".into())) })
    }

    fn reflect<'a>(
        &'a self,
        input: &'a ReflectInput,
    ) -> BoxFuture<'a, Result<Reflected, MindError>> {
        Box::pin(self.reflect_impl(input))
    }
}
