//! A phone alone's mind (ADR 0020): an online provider the person chose, reached by this
//! phone's own core through `nocturne-provider`. While it is off nothing here makes a request,
//! and nothing reads or writes pages: a phone alone stays as it was. While it is on, each
//! [`PhoneMind::pass`] reads a few things that were kept and lets the scheduler write the page
//! and the journal's reflections with it, against the store the home has open now.
//!
//! The screens call [`PhoneMind::check`], [`PhoneMind::turn_on`], [`PhoneMind::turn_off`] and
//! [`PhoneMind::chosen`]. The address and model are kept in a file of the app's; the key is held
//! by a [`Keys`], the phone's safe place ([`SafeKeys`]). With no key held, a chosen provider is
//! not reached.

pub mod ask;
pub mod commands;
pub mod events;
pub mod host;
pub mod keys;
pub mod sent;
pub mod sound;
pub mod status;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use jiff::Timestamp;
use nocturne_contract::{CheckOutcome, ItemKind};
use nocturne_engine::{Clock, Engine};
use nocturne_intelligence::{
    MindSlot, NullEvents, Outcome, Reader, Scheduler, Step, Tidy, new_mind_slot,
};
use nocturne_provider::address::{OnlineKey, Reach, UrlProblem, host_of, validate_url};
use nocturne_provider::client::Exchange;
use nocturne_provider::{Job, Provider, ProviderMind};
use serde::{Deserialize, Serialize};

use crate::home::Home;
use crate::sync::ledger::write_private;
use host::PhoneHost;
pub use keys::{Keys, MemoryKeys, SafeKeys};
pub use sent::{Sent, SentFor, SentLog};

const FILE: &str = "mind.json";
/// How long between passes while nothing is left to read.
const TICK: std::time::Duration = std::time::Duration::from_secs(30);
/// The most things one pass reads, so a store wanted for a restore or a move is let go soon.
const FEW: usize = 3;
/// How often a pass looks whether the store is wanted.
const WANTED_POLL: std::time::Duration = std::time::Duration::from_millis(200);
/// Minutes between looks at a service that stopped answering, or while this phone is offline:
/// the last one repeats.
const LOOK_AGAIN: [i64; 4] = [1, 5, 15, 30];

/// The person's provider, as typed into the form.
#[derive(Debug, Clone)]
pub struct ProviderSettings {
    pub url: String,
    pub key: String,
    pub model: String,
}

/// The provider chosen, as the screens show it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Chosen {
    Off,
    On {
        host: String,
        model: String,
    },
    /// Chosen, but its key is not held on this phone: nothing goes until it is given again.
    NeedsKey {
        host: String,
        model: String,
    },
}

/// What one pass did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pass {
    /// No mind is on: nothing was asked.
    Off,
    /// The phone is not known to charge: reading and the page wait for the charger.
    NotCharging,
    /// The month's limit is reached: nothing goes until next month.
    Paused,
    /// The service stopped answering: nothing is read or written until a look again finds it.
    Waiting,
    /// The phone holds no memory now (not alone, or the store will not open).
    NotReady,
    /// How many things the reader took, and whether the scheduler started or wrote something.
    /// A tidy waiting on its reading is not counted: the reading is.
    Ran { read: usize, scheduled: bool },
}

impl Pass {
    /// A pass that read all it may at once, or wrote something, may have more to do.
    pub fn more(&self) -> bool {
        matches!(
            self,
            Pass::Ran { read: FEW, .. }
                | Pass::Ran {
                    scheduled: true,
                    ..
                }
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Saved {
    url: String,
    model: String,
    /// When it was turned on: the first page waits for the next morning, as the screens promise.
    #[serde(default)]
    since: Option<String>,
}

/// The mind in the slot, with its host and model.
#[derive(Clone)]
struct On {
    mind: Arc<ProviderMind>,
    host: String,
    model: String,
    since: Option<Timestamp>,
    /// Things read to an end since it was turned on, this run.
    read: u32,
    /// Notes each request on the Sent online list and what it says about the service.
    noting: Arc<Noting>,
}

/// What each request of one chosen provider leaves behind: its row on the Sent online list and
/// what its answer says about the service.
struct Noting {
    sent: Arc<SentLog>,
    trouble: Arc<Mutex<Option<status::Trouble>>>,
    clock: Arc<dyn Clock>,
    host: String,
    model: String,
    /// The service is on the person's own network, where a name not found or no route means
    /// the service is away, not this phone.
    at_home: bool,
    /// Cleared as it is turned off or replaced: a request that ends after that is not noted.
    live: AtomicBool,
}

impl Noting {
    fn note(&self, job: Job, exchange: &Exchange) {
        if !self.live.load(Ordering::SeqCst) {
            return;
        }
        self.sent.note(job, &self.host, &self.model, exchange);
        // A request stopped here says nothing of the service.
        if exchange.stopped {
            return;
        }
        let mut trouble = lock(&self.trouble);
        *trouble = status::Trouble::after(exchange.status, self.clock.now(), *trouble);
    }

    /// A request that never left for want of a way there: this phone is offline, or, for a
    /// service on the person's own network, the service is not answering. Nothing is listed,
    /// since nothing went.
    fn unsent(&self, failure: nocturne_engine::OnlineFailure) {
        if !self.live.load(Ordering::SeqCst)
            || failure != nocturne_engine::OnlineFailure::NoInternet
        {
            return;
        }
        let now = self.clock.now();
        let mut trouble = lock(&self.trouble);
        *trouble = if self.at_home {
            status::Trouble::after(None, now, *trouble)
        } else {
            Some(status::Trouble::offline(now, *trouble))
        };
    }
}

/// The things and the memories the Sent online list carried that were kept before a forget.
#[derive(Debug, Default)]
pub struct Kept {
    items: std::collections::HashSet<String>,
    memories: std::collections::HashSet<String>,
}

/// Whether a lookup found nothing.
fn missing(found: nocturne_engine::Result<()>) -> bool {
    matches!(found, Err(nocturne_engine::Error::NotFound(_)))
}

/// The looks again at a service that stopped answering, since it stopped.
#[derive(Debug, Default)]
struct Looks {
    since: Option<Timestamp>,
    made: usize,
    last: Option<Timestamp>,
}

/// The reader, the tidy and the scheduler over one open store.
struct Thinking {
    generation: u64,
    reader: Reader,
    scheduler: Scheduler,
}

/// The key as it is held: tied to the address it was given for, so it is never sent to another.
fn bound(url: &str, key: &OnlineKey) -> OnlineKey {
    OnlineKey::new(format!("{}:{}", address_tag(url), key.expose()))
}

/// The held key, when it was given for `url`.
fn unbound(url: &str, held: &OnlineKey) -> Option<OnlineKey> {
    let (tag, key) = held.expose().split_once(':')?;
    (tag == address_tag(url)).then(|| OnlineKey::new(key.to_string()))
}

fn address_tag(url: &str) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(url.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

pub struct PhoneMind {
    dir: PathBuf,
    keys: Box<dyn Keys>,
    clock: Arc<dyn Clock>,
    sent: Arc<SentLog>,
    /// What the last request's answer said about the service; none while all is well.
    trouble: Arc<Mutex<Option<status::Trouble>>>,
    /// Held while this month's count is written.
    counting: Mutex<()>,
    /// The phone's decoder, for recordings the provider takes only as WAV.
    decode: Arc<std::sync::OnceLock<nocturne_provider::Decode>>,
    /// Who hears of things read and pages saved.
    events: Mutex<Arc<dyn nocturne_intelligence::Events>>,
    slot: MindSlot,
    host: Arc<PhoneHost>,
    on: Mutex<Option<On>>,
    thinking: Arc<Mutex<Option<Thinking>>>,
    /// A new mind is in the slot: the next pass tries again what waited for the last one.
    retry: AtomicBool,
    looks: Mutex<Looks>,
    /// Questions under way, by their ids, each with its way to stop it.
    asking: Mutex<HashMap<String, tokio::sync::watch::Sender<bool>>>,
    /// Questions stopped before they began: none is asked when it does.
    stopped_early: Mutex<Vec<String>>,
    /// Looks names up in place of the phone, and whether the service then counts as online
    /// rather than at home, so a test can take it offline.
    #[cfg(test)]
    lookup: Mutex<Option<(nocturne_provider::address::Lookup, bool)>>,
}

impl PhoneMind {
    /// The mind kept in `dir`, on again when its key is held.
    pub fn new(dir: PathBuf, keys: Box<dyn Keys>, clock: Arc<dyn Clock>) -> PhoneMind {
        let mind = PhoneMind {
            sent: Arc::new(SentLog::new(dir.clone(), clock.clone())),
            events: Mutex::new(Arc::new(NullEvents)),
            trouble: Arc::default(),
            decode: Arc::default(),
            counting: Mutex::new(()),
            dir,
            keys,
            clock,
            slot: new_mind_slot(),
            host: Arc::new(PhoneHost::default()),
            on: Mutex::new(None),
            thinking: Arc::default(),
            retry: AtomicBool::new(false),
            looks: Mutex::default(),
            asking: Mutex::default(),
            stopped_early: Mutex::default(),
            #[cfg(test)]
            lookup: Mutex::default(),
        };
        if let Some(saved) = mind.saved()
            && let Some(key) = mind.keys.key().and_then(|held| unbound(&saved.url, &held))
            && let Err(e) = mind.fill(&saved, key)
        {
            tracing::warn!("the chosen provider's address no longer passes: {e:?}");
        }
        mind
    }

    /// Lets go of the reader and the scheduler whenever `home` lets go of its store.
    pub fn follow(&self, home: &Home) {
        let thinking = self.thinking.clone();
        home.on_close(Box::new(move || {
            lock(&thinking).take();
        }));
    }

    /// Decodes recordings with `decode`; the first one given stays.
    pub fn decode_with(&self, decode: nocturne_provider::Decode) {
        let _ = self.decode.set(decode);
    }

    /// Tells `events` of things read and pages saved, from the next store opened.
    pub fn tell(&self, events: Arc<dyn nocturne_intelligence::Events>) {
        *lock(&self.events) = events;
    }

    /// Where the phone's charging is told.
    pub fn host(&self) -> &PhoneHost {
        &self.host
    }

    fn path(&self) -> PathBuf {
        self.dir.join(FILE)
    }

    fn saved(&self) -> Option<Saved> {
        let bytes = std::fs::read(self.path()).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    fn fill(&self, saved: &Saved, key: OnlineKey) -> Result<(), UrlProblem> {
        let provider = Provider::new(&saved.url, &saved.model, key)?;
        let at_home = validate_url(&saved.url).is_ok_and(|a| a.reach == Reach::Own);
        #[cfg(test)]
        let (provider, at_home) = match lock(&self.lookup).clone() {
            Some((lookup, online)) => (provider.with_lookup(lookup), at_home && !online),
            None => (provider, at_home),
        };
        let (host, model) = (provider.host().to_string(), provider.model().to_string());
        let noting = Arc::new(Noting {
            sent: self.sent.clone(),
            trouble: self.trouble.clone(),
            clock: self.clock.clone(),
            host: host.clone(),
            model: model.clone(),
            at_home,
            live: AtomicBool::new(true),
        });
        let record: nocturne_provider::Record = {
            let noting = noting.clone();
            Arc::new(move |job, exchange| noting.note(job, exchange))
        };
        let unsent: nocturne_provider::Unsent = {
            let noting = noting.clone();
            Arc::new(move |_job, failure| noting.unsent(failure))
        };
        let decode: nocturne_provider::Decode = {
            let decode = self.decode.clone();
            Arc::new(move |path| decode.get().and_then(|d| d(path)))
        };
        let mind = Arc::new(
            ProviderMind::new(provider)
                .with_record(record)
                .with_unsent(unsent)
                .with_decoder(decode),
        );
        let slotted: Arc<dyn nocturne_engine::Mind> = mind.clone();
        *self.slot.write().unwrap_or_else(|e| e.into_inner()) = Some(slotted);
        let on = On {
            mind,
            host,
            model,
            since: saved.since.as_deref().and_then(|s| s.parse().ok()),
            read: 0,
            noting,
        };
        lock(&self.trouble).take();
        if let Some(before) = lock(&self.on).replace(on) {
            before.noting.live.store(false, Ordering::SeqCst);
            before.mind.provider().stop();
        }
        self.retry.store(true, Ordering::SeqCst);
        Ok(())
    }

    /// The connection check for `settings`: one look at the provider's list of models and one
    /// short question, no memories. Each request goes on the Sent online list; nothing else is
    /// kept.
    pub async fn check(&self, settings: &ProviderSettings) -> CheckOutcome {
        let key = OnlineKey::new(settings.key.trim().to_string());
        match Provider::new(settings.url.trim(), &settings.model, key) {
            Ok(provider) => {
                let (outcome, made) = provider.check().await;
                for exchange in &made {
                    self.sent
                        .note(Job::Check, provider.host(), provider.model(), exchange);
                }
                outcome
            }
            Err(UrlProblem::NotHttps) => CheckOutcome::NotHttps,
            Err(_) => CheckOutcome::AddressNotAllowed,
        }
    }

    /// Turns the mind on with `settings`, in place of any chosen before. The first page it
    /// writes is the next morning's.
    pub fn turn_on(&self, settings: ProviderSettings) -> Result<(), UrlProblem> {
        let saved = Saved {
            url: settings.url.trim().to_string(),
            model: settings.model.trim().to_string(),
            since: Some(self.clock.now().to_string()),
        };
        let key = OnlineKey::new(settings.key.trim().to_string());
        Provider::new(&saved.url, &saved.model, key.clone())?;
        let bytes = serde_json::to_vec(&saved).expect("the choice serialises");
        write_private(&self.path(), &bytes).map_err(|e| {
            tracing::warn!("the chosen provider could not be kept: {e}");
            UrlProblem::Invalid
        })?;
        // A key kept before belongs to the provider chosen before: none outlives a new choice.
        if !self.keys.keep(bound(&saved.url, &key)) {
            self.keys.forget();
            tracing::warn!("the mind works until the app closes, and then asks for its key");
        }
        self.fill(&saved, key)
    }

    /// Turns the mind off: the slot is emptied, the key forgotten and the choice removed, so
    /// nothing more goes, and a request under way is dropped. What was already read stays.
    pub fn turn_off(&self) {
        self.slot.write().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(on) = lock(&self.on).take() {
            on.noting.live.store(false, Ordering::SeqCst);
            on.mind.provider().stop();
        }
        self.keys.forget();
        let _ = std::fs::remove_file(self.path());
    }

    /// "Delete everything": turned off, and the Sent online list goes with the key.
    pub fn erase(&self) {
        self.turn_off();
        self.sent.clear();
        self.forget_month();
    }

    pub fn chosen(&self) -> Chosen {
        if let Some(On { host, model, .. }) = lock(&self.on).clone() {
            return Chosen::On { host, model };
        }
        match self.saved().and_then(|s| {
            validate_url(&s.url)
                .ok()
                .map(|a| (host_of(&a.url), s.model))
        }) {
            Some((host, model)) => Chosen::NeedsKey { host, model },
            None => Chosen::Off,
        }
    }

    /// The Sent online list: up to `limit` requests, newest first, before the one with id
    /// `before` when given.
    pub fn sent(&self, limit: usize, before: Option<u64>) -> Vec<Sent> {
        self.sent.list(limit, before)
    }

    /// What the Sent online list's rows carried that `engine` still keeps, before a forget, an
    /// unkeep or a delete: [`PhoneMind::forget_words`] then removes the words of what it took.
    pub fn still_kept(&self, engine: &Engine) -> Kept {
        let (items, memories) = self.sent.carried();
        Kept {
            items: items
                .into_iter()
                .filter(|id| !missing(engine.get_item(id).map(drop)))
                .collect(),
            memories: memories
                .into_iter()
                .filter(|id| !missing(engine.get_memory(id).map(drop)))
                .collect(),
        }
    }

    /// After a forget, an unkeep or a delete: the words of every row on the Sent online list
    /// that carried a thing or a memory `before` held and `engine` no longer keeps are removed,
    /// the row kept. What went missing some other way, such as by a reading again, stays.
    pub fn forget_words(&self, engine: &Engine, before: &Kept) {
        self.forget_words_after(engine, 0, |id| {
            before.items.contains(id) || before.memories.contains(id)
        });
    }

    /// The words of rows after `after` that carried something `was` held and that is gone.
    fn forget_words_after(&self, engine: &Engine, after: u64, was: impl Fn(&str) -> bool) {
        let items = std::cell::RefCell::new(std::collections::HashMap::new());
        let memories = std::cell::RefCell::new(std::collections::HashMap::new());
        let gone = |cache: &std::cell::RefCell<std::collections::HashMap<String, bool>>,
                    id: &String,
                    found: &dyn Fn(&str) -> nocturne_engine::Result<()>| {
            *cache
                .borrow_mut()
                .entry(id.clone())
                .or_insert_with(|| was(id) && missing(found(id)))
        };
        self.sent.forget_words(after, |row| {
            row.items
                .iter()
                .any(|id| gone(&items, id, &|id| engine.get_item(id).map(drop)))
                || row
                    .memories
                    .iter()
                    .any(|id| gone(&memories, id, &|id| engine.get_memory(id).map(drop)))
        });
    }

    /// After a request that began at row `from`, with the store's forgets at `forgets`: when a
    /// forget ran meanwhile, what it took was not yet on the list, so its words go now.
    fn after_request(&self, engine: &Engine, from: u64, forgets: u64) {
        if engine.forget_generation() != forgets {
            self.forget_words_after(engine, from, |_| true);
        }
    }

    pub fn is_on(&self) -> bool {
        lock(&self.on).is_some()
    }

    /// While the service is not answering or this phone is offline, one look at the service's
    /// list of models when it is due ([`LOOK_AGAIN`]), noted as a connection check. Never while
    /// paused by the limit. When it is answered, the trouble ends and the next pass reads what
    /// waited. True when a look was made. Blocks on `rt`: call it off the async workers.
    pub fn look_again(&self, rt: &tokio::runtime::Handle) -> bool {
        use status::Trouble;
        let trouble = *lock(&self.trouble);
        let since = match trouble {
            Some(Trouble::Unreachable(since) | Trouble::Offline(since)) => since,
            _ => {
                *lock(&self.looks) = Looks::default();
                return false;
            }
        };
        let Some(on) = lock(&self.on).clone() else {
            return false;
        };
        if self.paused_until().is_some() {
            return false;
        }
        let now = self.clock.now();
        {
            let mut looks = lock(&self.looks);
            if looks.since != Some(since) {
                *looks = Looks {
                    since: Some(since),
                    ..Looks::default()
                };
            }
            let wait = LOOK_AGAIN[looks.made.min(LOOK_AGAIN.len() - 1)];
            let from = looks.last.unwrap_or(since);
            if now < from + jiff::SignedDuration::from_mins(wait) {
                return false;
            }
            looks.made += 1;
            looks.last = Some(now);
        }
        match rt.block_on(on.mind.provider().models()) {
            Ok((_, exchange)) => on.noting.note(Job::Check, &exchange),
            Err(f) if f.refused => on.noting.unsent(f.failure),
            Err(f) => on.noting.note(Job::Check, &f.exchange),
        }
        if lock(&self.trouble).is_none() {
            self.retry.store(true, Ordering::SeqCst);
        }
        true
    }

    /// The service stopped answering, or this phone is offline: only [`PhoneMind::look_again`]
    /// reaches it until it answers.
    fn waiting(&self) -> bool {
        matches!(
            *lock(&self.trouble),
            Some(status::Trouble::Unreachable(_) | status::Trouble::Offline(_))
        )
    }

    /// Whether the morning after the mind was turned on has come, so a page may be written.
    fn morning_come(&self) -> bool {
        let clock = &self.clock;
        let since = lock(&self.on).as_ref().and_then(|on| on.since);
        let Some(since) = since else {
            return true;
        };
        let tz = clock.tz();
        clock.now().to_zoned_datetime(tz.clone()).date() > since.to_zoned_datetime(tz).date()
    }

    /// One pass: up to a few things read, then one look by the scheduler. Blocks on `rt`: call
    /// it off the async workers. The store is held for use throughout, so nothing swaps it
    /// under the reader.
    pub fn pass(&self, home: &Home, rt: &tokio::runtime::Handle) -> Pass {
        if !self.is_on() {
            // Let go of the store too, unless a pass that began while on is ending.
            if let Ok(mut thinking) = self.thinking.try_lock() {
                thinking.take();
            }
            return Pass::Off;
        }
        if !self.host.charging() {
            return Pass::NotCharging;
        }
        if self.paused_until().is_some() {
            return Pass::Paused;
        }
        if self.waiting() {
            return Pass::Waiting;
        }
        let Ok((_use, open)) = home.using() else {
            return Pass::NotReady;
        };
        let mut thinking = lock(&self.thinking);
        if !self.is_on() {
            thinking.take();
            return Pass::Off;
        }
        let generation = home.generation();
        let clock = home.clock();
        if thinking.as_ref().is_none_or(|t| t.generation != generation) {
            let events = lock(&self.events).clone();
            let host: Arc<dyn nocturne_intelligence::Host> = self.host.clone();
            let reader = Reader::new(
                open.engine.clone(),
                self.slot.clone(),
                host.clone(),
                clock.clone(),
                events.clone(),
            );
            let tidy = Tidy::new(
                open.engine.clone(),
                reader.clone(),
                self.slot.clone(),
                host.clone(),
                clock.clone(),
                events.clone(),
            );
            let scheduler = Scheduler::new(
                open.engine.clone(),
                reader.clone(),
                tidy,
                host,
                clock.clone(),
                events,
            );
            let voice = home.voice();
            reader.hold_while(Arc::new(move |p| {
                p.kind == ItemKind::Audio
                    && p.text.as_deref().is_none_or(|t| t.trim().is_empty())
                    && !voice.turn_over(&p.id)
            }));
            scheduler.set_reflecting(self.slot.clone(), Arc::new(|| true));
            *thinking = Some(Thinking {
                generation,
                reader,
                scheduler,
            });
        }
        let t = thinking.as_ref().expect("made above");
        if self.retry.swap(false, Ordering::SeqCst) {
            t.reader.retry_now();
        }
        let wanted = || async {
            while !home.store_wanted() {
                tokio::time::sleep(WANTED_POLL).await;
            }
        };
        let (pass, abandoned) = rt.block_on(async {
            let reading = async {
                let mut read = 0;
                // Each thing read counts toward the month: the one that reaches a limit is the
                // last before the pause.
                while read < FEW && !home.store_wanted() && self.paused_until().is_none() {
                    let (from, forgets) = (self.sent.last_id(), open.engine.forget_generation());
                    let step = t.reader.tick().await;
                    if let Step::Read { item, .. } = &step {
                        self.sent.carry(from, item);
                    }
                    self.after_request(&open.engine, from, forgets);
                    match step {
                        Step::Read {
                            outcome: Outcome::Returned | Outcome::Lost,
                            ..
                        } => break,
                        Step::Read { outcome, .. } => {
                            read += 1;
                            if outcome != Outcome::Retry {
                                self.count(1);
                                if let Some(on) = lock(&self.on).as_mut() {
                                    on.read += 1;
                                }
                            }
                        }
                        _ => break,
                    }
                }
                read
            };
            // A move, a restore or a fresh start waits for the store: the thing being read is
            // given back at once, uncounted, rather than after its requests end.
            tokio::pin!(reading);
            let read = tokio::select! {
                read = &mut reading => read,
                () = wanted() => {
                    let _giving_way = t.reader.ask_begin();
                    reading.await
                }
            };
            // Turned off, paused by the limit, or before the first morning: nothing more is
            // written.
            if home.store_wanted()
                || !self.is_on()
                || !self.morning_come()
                || self.paused_until().is_some()
            {
                return (
                    Pass::Ran {
                        read,
                        scheduled: false,
                    },
                    false,
                );
            }
            // The page or a reflection being written is dropped unsaved for the store, and the
            // reader, the tidy and the scheduler are made anew at the next pass.
            let (from, forgets) = (self.sent.last_id(), open.engine.forget_generation());
            tokio::select! {
                step = t.scheduler.tick() => {
                    self.after_request(&open.engine, from, forgets);
                    use nocturne_intelligence::{SchedStep, TidyStep};
                    let scheduled = matches!(
                        step,
                        SchedStep::Started(_)
                            | SchedStep::Reflected(_)
                            | SchedStep::Tidy(
                                TidyStep::Finished(_) | TidyStep::Resaved(_) | TidyStep::Skipped
                            )
                    );
                    (Pass::Ran { read, scheduled }, false)
                }
                () = wanted() => (Pass::Ran { read, scheduled: false }, true),
            }
        });
        if abandoned {
            thinking.take();
        }
        pass
    }
}

/// Runs a pass every [`TICK`] while the app runs, sooner while there is more to do. Before each,
/// the phone says whether it charges. While the mind or the nightly backup is on the system is
/// asked for background windows while the phone charges, the backup's on Wi-Fi alone while
/// "Wi-Fi only" is on, and for none while both are off; an
/// open window is ended once a pass finds nothing more to do. Each pass also gives the nightly
/// backup its look ([`crate::backup::PhoneBackup::pass`]).
pub fn run<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    use tauri::Manager;
    use tauri_plugin_nocturne_system::SystemExt;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut asked: Option<tauri_plugin_nocturne_system::Windows> = None;
        loop {
            let worker = app.clone();
            let more = tauri::async_runtime::spawn_blocking(move || {
                let rt = tokio::runtime::Handle::current();
                let system = worker.system();
                let mind = worker.state::<PhoneMind>();
                let backup = worker.state::<crate::backup::PhoneBackup>();
                let power = system.power().unwrap_or_default();
                mind.host().set_charging(power.charging);
                backup.set_unmetered(power.unmetered);
                let windows = tauri_plugin_nocturne_system::Windows {
                    mind: mind.is_on(),
                    backup: backup.is_on(),
                    wifi_only: backup.wifi_only(),
                };
                let was = mind.status();
                mind.look_again(&rt);
                if asked != Some(windows) {
                    match system.background(windows) {
                        Ok(()) => asked = Some(windows),
                        Err(e) => tracing::warn!("background work could not be arranged: {e:?}"),
                    }
                }
                let home = worker.state::<Home>();
                let more = mind.pass(&home, &rt).more();
                let progress = crate::home::commands::progress(worker.clone());
                let more = backup.pass_with(&home, power.charging == Some(true), &progress) || more;
                if mind.status() != was {
                    events::status_changed(&worker);
                }
                if power.window
                    && !more
                    && let Err(e) = system.background_done()
                {
                    tracing::warn!("the background window could not be ended: {e:?}");
                }
                (more, asked)
            })
            .await;
            let more = match more {
                Ok((more, now_asked)) => {
                    asked = now_asked;
                    more
                }
                Err(_) => false,
            };
            if !more {
                tokio::time::sleep(TICK).await;
            }
        }
    });
}
