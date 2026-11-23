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

pub mod host;
pub mod keys;
pub mod sent;
#[cfg(test)]
mod tests;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use jiff::Timestamp;
use nocturne_contract::CheckOutcome;
use nocturne_engine::Clock;
use nocturne_intelligence::{
    MindSlot, NullEvents, Outcome, Reader, Scheduler, Step, Tidy, new_mind_slot,
};
use nocturne_provider::address::{OnlineKey, UrlProblem, host_of, validate_url};
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
    /// The phone holds no memory now (not alone, or the store will not open).
    NotReady,
    /// How many things were read to an end (understood, or not readable), and whether the
    /// scheduler did anything.
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
    slot: MindSlot,
    host: Arc<PhoneHost>,
    on: Mutex<Option<On>>,
    thinking: Arc<Mutex<Option<Thinking>>>,
    /// A new mind is in the slot: the next pass tries again what waited for the last one.
    retry: AtomicBool,
}

impl PhoneMind {
    /// The mind kept in `dir`, on again when its key is held.
    pub fn new(dir: PathBuf, keys: Box<dyn Keys>, clock: Arc<dyn Clock>) -> PhoneMind {
        let mind = PhoneMind {
            sent: Arc::new(SentLog::new(dir.clone(), clock.clone())),
            dir,
            keys,
            clock,
            slot: new_mind_slot(),
            host: Arc::new(PhoneHost::default()),
            on: Mutex::new(None),
            thinking: Arc::default(),
            retry: AtomicBool::new(false),
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
        let (host, model) = (provider.host().to_string(), provider.model().to_string());
        let record: nocturne_provider::Record = {
            let (sent, host, model) = (self.sent.clone(), host.clone(), model.clone());
            Arc::new(move |job, exchange| sent.note(job, &host, &model, exchange))
        };
        let mind = Arc::new(ProviderMind::new(provider).with_record(record));
        let slotted: Arc<dyn nocturne_engine::Mind> = mind.clone();
        *self.slot.write().unwrap_or_else(|e| e.into_inner()) = Some(slotted);
        let on = On {
            mind,
            host,
            model,
            since: saved.since.as_deref().and_then(|s| s.parse().ok()),
        };
        if let Some(before) = lock(&self.on).replace(on) {
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
            on.mind.provider().stop();
        }
        self.keys.forget();
        let _ = std::fs::remove_file(self.path());
    }

    /// "Delete everything": turned off, and the Sent online list goes with the key.
    pub fn erase(&self) {
        self.turn_off();
        self.sent.clear();
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

    pub fn is_on(&self) -> bool {
        lock(&self.on).is_some()
    }

    /// Whether the morning after the mind was turned on has come, so a page may be written.
    fn morning_come(&self) -> bool {
        let clock = &self.clock;
        let since = lock(&self.on).as_ref().and_then(|on| on.since);
        let Some(since) = since else {
            return true;
        };
        let tz = clock.tz();
        clock.now().to_zoned(tz.clone()).date() > since.to_zoned(tz).date()
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
            let events: Arc<dyn nocturne_intelligence::Events> = Arc::new(NullEvents);
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
                while read < FEW && !home.store_wanted() {
                    match t.reader.tick().await {
                        Step::Read {
                            outcome: Outcome::Returned | Outcome::Lost,
                            ..
                        } => break,
                        Step::Read { .. } => read += 1,
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
            // Turned off meanwhile, or before the first morning: nothing more is written.
            if home.store_wanted() || !self.is_on() || !self.morning_come() {
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
            tokio::select! {
                step = t.scheduler.tick() => {
                    let scheduled = step != nocturne_intelligence::SchedStep::Idle;
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
/// the phone says whether it charges. While the mind is on the system is asked for background
/// windows while the phone charges, and for none while it is off; an open window is ended once
/// a pass finds nothing more to do. While the mind is off a pass returns at once and asks nothing.
pub fn run<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    use tauri::Manager;
    use tauri_plugin_nocturne_system::SystemExt;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut asked: Option<bool> = None;
        loop {
            let worker = app.clone();
            let more = tauri::async_runtime::spawn_blocking(move || {
                let rt = tokio::runtime::Handle::current();
                let system = worker.system();
                let mind = worker.state::<PhoneMind>();
                let power = system.power().unwrap_or_default();
                mind.host().set_charging(power.charging);
                let on = mind.is_on();
                if asked != Some(on) {
                    match system.background(on) {
                        Ok(()) => asked = Some(on),
                        Err(e) => tracing::warn!("background work could not be arranged: {e:?}"),
                    }
                }
                let more = mind.pass(&worker.state::<Home>(), &rt).more();
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
