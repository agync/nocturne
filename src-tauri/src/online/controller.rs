//! [`OnlineHub`]: Online answers' one owner in the app. It keeps the route the engine has, the
//! key in memory while it is needed, and the settings row in step, and it decides per question
//! whether the answer may be written online. Every change goes through one lock, so a check, a
//! turn on and a turn off never interleave.

use std::sync::{Arc, Mutex};

use jiff::Timestamp;
use nocturne_contract::{
    CheckOutcome, EVENT_ONLINE, KeyStorage, LedgerKind, LocalReason, NoticeCode, OnlineForm,
    OnlinePause, OnlineProblem, OnlineSettings, OnlineSettingsPatch, OnlineState, OnlineView,
    SendKind, SendResult, SentLine,
};
use nocturne_engine::{BoxFuture, Engine, NewSend, OnlineFailure, OnlineReceipt, OnlineRow};
use nocturne_lemonade::mind::online::{OnlineMind, check};
use nocturne_lemonade::online::{CloudRoute, OnlineKey, UrlProblem, host_of};
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use super::keys::KeyStore;
use super::limit;
use super::notices;
use super::route::{EnginePort, RouteChoice};
use crate::ask::AskLane;
use crate::error::{CommandError, ErrorCode};
use crate::state::AppState;

/// One question counted against the month's limit while it is on its way. Given back when
/// dropped, by which time a question that went out is in the sent log.
pub struct Reservation(Option<Arc<Mutex<Routes>>>);

impl Drop for Reservation {
    fn drop(&mut self) {
        if let Some(routes) = self.0.take() {
            let mut r = routes.lock().unwrap_or_else(|e| e.into_inner());
            r.reserved = r.reserved.saturating_sub(1);
        }
    }
}

/// What happened to one question that went toward the route, for the log and the counts.
pub struct AskOutcome {
    pub question: String,
    pub host: String,
    pub model: String,
    pub receipt: OnlineReceipt,
    pub result: SendResult,
    pub failure: Option<OnlineFailure>,
    /// The request reached the engine route.
    pub counted: bool,
}

#[derive(Default)]
struct Routes {
    /// The route in use while Online answers is on.
    active: Option<CloudRoute>,
    /// A route being set up: checked, not yet agreed to.
    provisional: Option<CloudRoute>,
    /// The provisional route passed its check: only then may it be turned on.
    passed: bool,
    /// What the engine was last given, when known. A relaunch that failed leaves it unknown,
    /// so the next change always reaches the engine.
    engine: Option<CloudRoute>,
    engine_known: bool,
    /// Questions on their way online now, counted against the month's limit before they are
    /// logged.
    reserved: u32,
    /// The key could not be removed: it may still be in the keychain or the file.
    key_left: bool,
    /// A route the person refused in the app's own window: turning on with it is refused
    /// without asking again, until the person leaves setup.
    refused: Option<Fingerprint>,
    /// The check under way, which `cancel_check` stops.
    checking: Option<CancellationToken>,
    /// The month the limit notice was last raised for.
    limit_noticed: Option<String>,
    loaded: bool,
}

/// Asks the person, outside the web view, whether Online answers may be turned on with an host.
pub type ConfirmFn = Arc<dyn Fn(&str) -> BoxFuture<'static, bool> + Send + Sync>;

pub struct OnlineHub<R: Runtime> {
    app: AppHandle<R>,
    port: Arc<dyn EnginePort>,
    keys: KeyStore,
    routes: Arc<Mutex<Routes>>,
    ops: tokio::sync::Mutex<()>,
    /// Bumped when Online answers turns off: questions on their way online stop at once.
    gate: watch::Sender<u64>,
    send_hold: Mutex<Option<Arc<SendHold>>>,
    /// Asks before turning on with a route just checked. Without it, nothing is turned on.
    confirm: Mutex<Option<ConfirmFn>>,
    /// Held while the person is asked: one question at a time.
    confirming: tokio::sync::Mutex<()>,
}

/// Holds a question just before it is sent, for a test that turns Online answers off then.
#[doc(hidden)]
#[derive(Default)]
pub struct SendHold {
    pub reached: tokio::sync::Notify,
    pub go: tokio::sync::Notify,
}

fn same_route(a: Option<&CloudRoute>, b: Option<&CloudRoute>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            a.base_url() == b.base_url() && a.model == b.model && a.key.expose() == b.key.expose()
        }
        _ => false,
    }
}

fn internal(e: impl std::fmt::Display) -> CommandError {
    CommandError::internal(e)
}

/// A route from what the person entered. Builds with the stand-in feature take a loopback
/// address over plain HTTP.
fn route_from(base_url: &str, model: &str, key: OnlineKey) -> Result<CloudRoute, UrlProblem> {
    #[cfg(feature = "online-stand-in")]
    if base_url.starts_with("http://") {
        return CloudRoute::stand_in(base_url, model.to_string(), key);
    }
    CloudRoute::new(base_url, model.to_string(), key)
}

/// A route as the person refused it: its address, model and a hash of its key, never the key.
#[derive(Clone, PartialEq, Eq)]
struct Fingerprint {
    base_url: String,
    model: String,
    key: [u8; 32],
}

impl Fingerprint {
    fn of(route: &CloudRoute) -> Self {
        use sha2_10::{Digest, Sha256};
        Fingerprint {
            base_url: route.base_url().to_string(),
            model: route.model.clone(),
            key: Sha256::digest(route.key.expose().as_bytes()).into(),
        }
    }
}

/// Why a question fell back to this computer, from why the online answer did not come.
pub fn reason_for(f: OnlineFailure) -> LocalReason {
    match f {
        OnlineFailure::NoInternet => LocalReason::NoInternet,
        OnlineFailure::Slow => LocalReason::Slow,
        OnlineFailure::KeyRefused => LocalReason::KeyRefused,
        OnlineFailure::NoCredit => LocalReason::NoCredit,
        OnlineFailure::ModelMissing => LocalReason::Problem,
        OnlineFailure::Unreachable
        | OnlineFailure::Busy
        | OnlineFailure::Refused
        | OnlineFailure::Other(_) => LocalReason::CouldntReach,
    }
}

impl<R: Runtime> OnlineHub<R> {
    pub fn new(app: AppHandle<R>, port: Arc<dyn EnginePort>, keys: KeyStore) -> Self {
        OnlineHub {
            app,
            port,
            keys,
            routes: Arc::new(Mutex::new(Routes::default())),
            ops: tokio::sync::Mutex::new(()),
            gate: watch::channel(0).0,
            send_hold: Mutex::new(None),
            confirm: Mutex::new(None),
            confirming: tokio::sync::Mutex::new(()),
        }
    }

    fn routes(&self) -> std::sync::MutexGuard<'_, Routes> {
        self.routes.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn engine(&self) -> Result<Engine, CommandError> {
        self.app
            .try_state::<AppState>()
            .ok_or_else(|| CommandError::unavailable("Nocturne's store is not open"))?
            .engine()
    }

    fn tz(&self) -> jiff::tz::TimeZone {
        self.engine()
            .map(|e| e.clock().tz())
            .unwrap_or(jiff::tz::TimeZone::UTC)
    }

    /// Changes whenever Online answers turns off. Taken before a question's route is chosen,
    /// so a turn-off at any point after that is seen.
    pub fn gate(&self) -> watch::Receiver<u64> {
        self.gate.subscribe()
    }

    /// Whether a download the person agreed to holds the window open now.
    pub async fn downloading(&self) -> bool {
        self.port.downloading().await
    }

    /// Whether a question may go out now: a route is in use, no check is running, and the
    /// engine is known to hold exactly that route. Checked just before a send.
    pub fn still_on(&self) -> bool {
        let r = self.routes();
        r.active.is_some()
            && r.checking.is_none()
            && r.engine_known
            && same_route(r.engine.as_ref(), r.active.as_ref())
    }

    /// How the person is asked before Online answers is turned on with a route just checked:
    /// the app's own window, which script in the web view cannot answer.
    pub fn set_confirm(&self, confirm: ConfirmFn) {
        *self.confirm.lock().unwrap_or_else(|e| e.into_inner()) = Some(confirm);
    }

    /// Whether the person agrees to turn on with `address`; no way to ask is a no.
    async fn confirmed(&self, address: &str) -> bool {
        let confirm = self
            .confirm
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        match confirm {
            Some(ask) => ask(address).await,
            None => false,
        }
    }

    #[doc(hidden)]
    pub fn hold_sends(&self, hold: Option<Arc<SendHold>>) {
        *self.send_hold.lock().unwrap_or_else(|e| e.into_inner()) = hold;
    }

    /// Waits while a test holds sends.
    #[doc(hidden)]
    pub async fn before_send(&self) {
        let hold = self
            .send_hold
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if let Some(h) = hold {
            h.reached.notify_one();
            h.go.notified().await;
        }
    }

    /// The route the engine's next launch uses: only the one in use. A route being set up or
    /// one whose check failed is never launched with; a check gives it to the engine itself.
    pub fn route_at_launch(&self) -> Option<CloudRoute> {
        self.load();
        let mut r = self.routes();
        let route = r.active.clone();
        r.engine = route.clone();
        r.engine_known = true;
        route
    }

    /// Reads the row and the key: with Online answers on, the route is made from them. A key
    /// that cannot be read is a problem, and questions are answered here. A read that failed
    /// (the store, or a keychain that was not available) is tried again at the next use.
    pub fn load(&self) {
        if self.routes().loaded {
            return;
        }
        let Ok(engine) = self.engine() else {
            return;
        };
        let Ok(row) = engine.online_state() else {
            return;
        };
        if !row.wanted_on {
            self.routes().loaded = true;
            return;
        }
        let key = match self.keys.load() {
            Ok(k) => {
                self.routes().loaded = true;
                k
            }
            Err(_) => None,
        };
        let route = key.and_then(|(key, _)| {
            row.base_url
                .as_deref()
                .zip(row.model.as_deref())
                .and_then(|(url, model)| route_from(url, model, key).ok())
        });
        match route {
            Some(r) => {
                self.routes().active = Some(r);
                if row.problem == Some(OnlineProblem::KeyUnavailable) {
                    let _ = engine.update_online_state(|s| {
                        s.problem = None;
                        s.problem_since = None;
                    });
                }
            }
            None if row.problem != Some(OnlineProblem::KeyUnavailable) => {
                let now = engine.clock_now();
                let _ = engine.update_online_state(|s| {
                    s.problem = Some(OnlineProblem::KeyUnavailable);
                    s.problem_since = Some(now);
                });
            }
            None => {}
        }
    }

    /// Gives the engine `route` unless it already has it. While the relaunch runs, what the
    /// engine holds is unknown: the engine takes the route before it answers, so a caller
    /// that stops waiting (a check cancelled) leaves it unknown, and the next call always
    /// reaches the engine.
    async fn give_engine(&self, route: Option<CloudRoute>) -> Result<(), String> {
        {
            let mut r = self.routes();
            if r.engine_known && same_route(r.engine.as_ref(), route.as_ref()) {
                return Ok(());
            }
            r.engine_known = false;
        }
        // A route that is not the one in use: questions on their way out stop first and are
        // answered here, so none reaches a provider the person has not agreed to.
        if !same_route(self.routes().active.as_ref(), route.as_ref()) {
            self.gate.send_modify(|g| *g += 1);
        }
        match self.port.set_cloud(route.clone()).await {
            Ok(()) => {
                let mut r = self.routes();
                r.engine = route;
                r.engine_known = true;
                Ok(())
            }
            Err(e) => {
                self.routes().engine_known = false;
                Err(e)
            }
        }
    }

    /// After launch: an engine that started before the route was known is relaunched once.
    pub async fn settle_after_launch(&self) {
        let _ops = self.ops.lock().await;
        self.load();
        let want = self.routes().active.clone();
        let has = self.port.has_cloud().await;
        if let Some(has) = has
            && has != want.is_some()
        {
            self.routes().engine_known = false;
            let _ = self.give_engine(want).await;
        }
    }

    fn emit(&self) {
        let view = self.view_now();
        if let Err(e) = self.app.emit(EVENT_ONLINE, &view) {
            tracing::warn!("could not tell the interface about Online answers: {e}");
        }
    }

    /// A limit pause whose month has ended is lifted.
    fn lift_expired(&self, engine: &Engine, row: OnlineRow) -> OnlineRow {
        let now = engine.clock_now();
        match &row.pause {
            Some(p) if limit::lifted(p, now) => engine
                .update_online_state(|s| s.pause = None)
                .unwrap_or(row),
            _ => row,
        }
    }

    pub async fn view(&self) -> OnlineView {
        self.load();
        self.view_now()
    }

    fn view_now(&self) -> OnlineView {
        let Ok(engine) = self.engine() else {
            return super::off_view();
        };
        let Ok(row) = engine.online_state() else {
            return super::off_view();
        };
        let row = self.lift_expired(&engine, row);
        let tz = self.tz();
        let checking = self.routes().checking.is_some();
        let fmt = |t: Option<Timestamp>| t.map(|t| t.to_string());
        let state = if checking {
            OnlineState::Checking
        } else if !row.wanted_on {
            OnlineState::Off
        } else if let Some(code) = row.problem {
            OnlineState::Problem {
                code,
                since: fmt(row.problem_since).unwrap_or_default(),
            }
        } else if let Some(pause) = row.pause.clone() {
            OnlineState::Paused { pause }
        } else {
            OnlineState::On
        };
        let host = row
            .base_url
            .as_deref()
            .and_then(|u| url::Url::parse(u).ok())
            .map(|u| host_of(&u));
        let (last, _) = engine.last_online_send().unwrap_or((None, 0));
        OnlineView {
            state,
            set_up: row.set_up,
            host,
            base_url: row.base_url.clone(),
            model: row.model.clone(),
            phones: row.phones,
            phones_available: engine.remote_state().map(|r| r.wanted_on).unwrap_or(false),
            send_sensitive: row.send_sensitive,
            limit: row.limit.clone(),
            questions_this_month: engine.questions_this_month(&tz).unwrap_or(0),
            spent_this_month_usd: engine.spent_this_month(&tz).unwrap_or(None),
            cost_reported: engine.online_cost_reported().unwrap_or(false),
            resets_on: engine.online_resets_on(&tz),
            last_sent_at: last.map(|l| l.at.to_string()),
            on_since: fmt(row.on_since),
            last_on_at: fmt(row.last_on_at),
            sent_bytes: row.sent_bytes,
            key_storage: row.key_storage,
        }
    }

    /// One connection check with what the person entered. The engine is relaunched with this
    /// route (a provisional one until the person agrees), then asked one short question. An
    /// empty key while set up means the key already kept.
    pub async fn check(&self, form: OnlineForm) -> Result<CheckOutcome, CommandError> {
        let OnlineForm {
            base_url,
            key,
            model,
        } = form;
        let key = OnlineKey::new(key);
        let _ops = self.ops.lock().await;
        self.load();
        let engine = self.engine()?;
        let row = engine.online_state().map_err(internal)?;
        let model = model.trim().to_string();
        // The address is judged first, with no key in it.
        let entered = match route_from(base_url.trim(), &model, OnlineKey::new(String::new())) {
            Ok(r) => r.base_url().to_string(),
            Err(UrlProblem::NotHttps) => return Ok(CheckOutcome::NotHttps),
            Err(_) => return Ok(CheckOutcome::AddressNotAllowed),
        };
        let key = if key.expose().trim().is_empty() {
            // A blank key means the one kept, and only for the address it was kept for: a
            // changed address never gets it.
            let kept_for = row
                .base_url
                .as_deref()
                .and_then(|u| route_from(u, &model, OnlineKey::new(String::new())).ok())
                .map(|r| r.base_url().to_string());
            if !row.set_up || kept_for.as_deref() != Some(entered.as_str()) {
                return Ok(CheckOutcome::KeyRefused);
            }
            match self.keys.load() {
                Ok(Some((k, _))) => k,
                _ => return Ok(CheckOutcome::KeyRefused),
            }
        } else {
            OnlineKey::new(key.expose().trim().to_string())
        };
        let route = match route_from(&entered, &model, key) {
            Ok(r) => r,
            Err(UrlProblem::NotHttps) => return Ok(CheckOutcome::NotHttps),
            Err(_) => return Ok(CheckOutcome::AddressNotAllowed),
        };
        let host = host_of(route.base_url());
        let token = CancellationToken::new();
        {
            let mut r = self.routes();
            r.provisional = Some(route.clone());
            r.passed = false;
            r.checking = Some(token.clone());
        }
        // Questions on their way out stop: during the check the engine holds its route.
        self.gate.send_modify(|g| *g += 1);
        self.emit();
        let outcome = tokio::select! {
            o = self.run_check(&route, &host, &model) => o,
            () = token.cancelled() => None,
        };
        self.routes().checking = None;
        let Some((outcome, receipt)) = outcome else {
            self.routes().provisional = None;
            let back = self.routes().active.clone();
            let _ = self.give_engine(back).await;
            self.emit();
            return Ok(CheckOutcome::Cancelled);
        };
        if let Some(receipt) = receipt {
            let reached = receipt.status.is_some()
                && !matches!(
                    outcome,
                    CheckOutcome::NoInternet | CheckOutcome::Unreachable
                );
            let result = match &outcome {
                CheckOutcome::Works { .. } => SendResult::Answered { receipts: 0 },
                _ => SendResult::Failed {
                    reason: LocalReason::CouldntReach,
                },
            };
            self.log(
                &engine,
                SendKind::Check,
                CHECK_LOG_QUESTION,
                &host,
                &model,
                receipt,
                result,
                false,
                reached,
            );
        }
        if let CheckOutcome::Works { .. } = &outcome {
            if let Err(e) =
                engine.record_download(LedgerKind::OnlineConnected, &host, Some(&host), 0.0)
            {
                tracing::warn!("the connection could not be written to the ledger: {e}");
            }
            let same = same_route(Some(&route), self.routes().active.as_ref());
            self.routes().passed = true;
            if row.wanted_on && same {
                // "Check again" with the route in use: it works, so a pause lifts.
                self.apply_while_on(&engine, route).await?;
            }
        }
        let works = matches!(outcome, CheckOutcome::Works { .. });
        if row.wanted_on || !works {
            // While on, a check never changes what questions use: the engine goes back to the
            // route in use, and a new route waits for the person to agree (turn on). A check
            // that failed hands the engine back too, with nothing while off; its route stays
            // only as one that may not be turned on.
            let back = {
                let mut r = self.routes();
                if row.wanted_on && !r.passed {
                    r.provisional = None;
                }
                r.active.clone()
            };
            let _ = self.give_engine(back).await;
        }
        self.emit();
        Ok(outcome)
    }

    async fn run_check(
        &self,
        route: &CloudRoute,
        host: &str,
        model: &str,
    ) -> Option<(CheckOutcome, Option<OnlineReceipt>)> {
        if self.give_engine(Some(route.clone())).await.is_err() {
            return Some((CheckOutcome::Unreachable, None));
        }
        let Some(client) = self.port.client().await else {
            return Some((CheckOutcome::Unreachable, None));
        };
        let (outcome, receipt) = check(&client, host, model).await;
        Some((outcome, Some(receipt)))
    }

    /// A check that worked while Online answers is on: its route, key and model are used from
    /// now on, and a pause for the key or credit, or a problem, is lifted.
    async fn apply_while_on(&self, engine: &Engine, route: CloudRoute) -> Result<(), CommandError> {
        let storage = self
            .keys
            .save(&route.key)
            .map_err(CommandError::unavailable)?;
        engine
            .update_online_state(|s| {
                s.base_url = Some(route.base_url().to_string());
                s.model = Some(route.model.clone());
                s.key_storage = Some(storage);
                if !matches!(s.pause, Some(OnlinePause::Limit { .. })) {
                    s.pause = None;
                }
                s.problem = None;
                s.problem_since = None;
            })
            .map_err(internal)?;
        {
            let mut r = self.routes();
            r.active = Some(route);
            r.provisional = None;
        }
        notices::clear(
            &self.app,
            &[NoticeCode::OnlineKeyRefused, NoticeCode::OnlineNoCredit],
        )
        .await;
        Ok(())
    }

    /// Stops a check under way; its route and key are dropped.
    pub fn cancel_check(&self) {
        if let Some(t) = self.routes().checking.take() {
            t.cancel();
        }
    }

    /// Turns Online answers on after the person agreed (O6): the key is kept, the settings
    /// saved, and the checked route stays with the engine.
    pub async fn turn_on(&self, settings: OnlineSettings) -> Result<OnlineView, CommandError> {
        self.load();
        let engine = self.engine()?;
        // A route just checked is asked about in the app's own window first, the full address
        // shown, before the lock, so turning off is never kept waiting on the person.
        let asked = {
            let r = self.routes();
            r.provisional.clone().filter(|_| r.passed)
        };
        let _asking = match &asked {
            Some(route) => {
                let Ok(asking) = self.confirming.try_lock() else {
                    return Err(CommandError::unavailable(
                        "the person is already being asked in the app's window",
                    ));
                };
                let print = Fingerprint::of(route);
                if self.routes().refused.as_ref() == Some(&print) {
                    return Err(CommandError::new(
                        ErrorCode::Declined,
                        "the person said no to this connection; leave setup to start again",
                    ));
                }
                if !self.confirmed(route.base_url().as_str()).await {
                    self.routes().refused = Some(print);
                    return Err(CommandError::new(
                        ErrorCode::Declined,
                        "Online answers was not turned on: the person did not agree in the \
                         app's window",
                    ));
                }
                Some(asking)
            }
            None => None,
        };
        let _ops = self.ops.lock().await;
        let row = engine.online_state().map_err(internal)?;
        let (provisional, passed) = {
            let mut r = self.routes();
            // A route that passed its check while the person was asked is asked about too.
            if let Some(p) = r.provisional.as_ref().filter(|_| r.passed)
                && !same_route(asked.as_ref(), Some(p))
            {
                return Err(CommandError::unavailable(
                    "the connection changed while waiting; turn on again",
                ));
            }
            (
                r.provisional.take(),
                std::mem::replace(&mut r.passed, false),
            )
        };
        let route = match provisional {
            Some(r) if passed => r,
            Some(_) => {
                return Err(CommandError::unavailable(
                    "the connection did not pass its check; check it again before turning on",
                ));
            }
            None => {
                let key = match self.keys.load() {
                    Ok(Some((k, _))) => k,
                    _ => {
                        return Err(CommandError::unavailable(
                            "check the connection before turning Online answers on",
                        ));
                    }
                };
                row.base_url
                    .as_deref()
                    .zip(row.model.as_deref())
                    .and_then(|(u, m)| route_from(u, m, key).ok())
                    .ok_or_else(|| {
                        CommandError::unavailable(
                            "check the connection before turning Online answers on",
                        )
                    })?
            }
        };
        let storage = self
            .keys
            .save(&route.key)
            .map_err(CommandError::unavailable)?;
        let now = engine.clock_now();
        engine
            .update_online_state(|s| {
                s.wanted_on = true;
                s.set_up = true;
                s.base_url = Some(route.base_url().to_string());
                s.model = Some(route.model.clone());
                s.phones = settings.phones;
                s.send_sensitive = settings.send_sensitive;
                s.limit = settings.limit.clone();
                s.pause = None;
                s.problem = None;
                s.problem_since = None;
                s.key_storage = Some(storage);
                s.first_on_at.get_or_insert(now);
                s.on_since = Some(now);
                s.last_on_at = Some(now);
            })
            .map_err(internal)?;
        let _ = engine.record_download(LedgerKind::OnlineOnOff, "on", None, 0.0);
        self.routes().active = Some(route.clone());
        if let Err(e) = self.give_engine(Some(route)).await {
            tracing::warn!("the engine did not take the route: {e}");
        }
        self.emit();
        Ok(self.view_now())
    }

    /// Turns Online answers off at once: questions on their way online stop and are answered
    /// here, then the engine is relaunched without the route. With `forget`, the key is
    /// removed and Online answers is no longer set up.
    pub async fn turn_off(&self, forget: bool) -> Result<OnlineView, CommandError> {
        // At once, before waiting for anything: no question goes out from here on.
        self.gate.send_modify(|g| *g += 1);
        self.cancel_check();
        {
            let mut r = self.routes();
            r.active = None;
            r.provisional = None;
            r.passed = false;
        }
        let _ops = self.ops.lock().await;
        let engine = self.engine()?;
        let now = engine.clock_now();
        let was_on = engine.online_state().map(|r| r.wanted_on).unwrap_or(false);
        let key_left = forget && self.keys.forget().is_err();
        engine
            .update_online_state(|s| {
                s.wanted_on = false;
                s.on_since = None;
                s.pause = None;
                s.problem = None;
                s.problem_since = None;
                if was_on {
                    s.last_off_at = Some(now);
                }
                if forget {
                    s.set_up = false;
                    // A key that could not be removed is still said to be kept.
                    if !key_left {
                        s.key_storage = None;
                    }
                }
            })
            .map_err(internal)?;
        if key_left {
            tracing::warn!("the key could not be removed");
            self.routes().key_left = true;
        }
        if was_on {
            let _ = engine.record_download(LedgerKind::OnlineOnOff, "off", None, 0.0);
        }
        if let Err(e) = self.give_engine(None).await {
            tracing::warn!("the engine did not let go of the route: {e}");
        }
        notices::clear(
            &self.app,
            &[
                NoticeCode::OnlineKeyRefused,
                NoticeCode::OnlineNoCredit,
                NoticeCode::OnlineLimit,
            ],
        )
        .await;
        self.emit();
        if key_left {
            return Err(CommandError::unavailable(
                "Online answers is off, but the key could not be removed from where it is kept",
            ));
        }
        Ok(self.view_now())
    }

    /// Leaving setup before agreeing: the checked route and its key are dropped, and the
    /// engine goes back to what it had before.
    pub async fn abandon_setup(&self) -> Result<(), CommandError> {
        self.cancel_check();
        let _ops = self.ops.lock().await;
        let back = {
            let mut r = self.routes();
            r.provisional = None;
            r.passed = false;
            r.refused = None;
            r.active.clone()
        };
        self.give_engine(back)
            .await
            .map_err(CommandError::unavailable)?;
        self.emit();
        Ok(())
    }

    pub async fn update(&self, patch: OnlineSettingsPatch) -> Result<OnlineView, CommandError> {
        let _ops = self.ops.lock().await;
        let engine = self.engine()?;
        let tz = self.tz();
        let questions = engine.questions_this_month(&tz).map_err(internal)?;
        let spent = engine.spent_this_month(&tz).map_err(internal)?;
        let cost_reported = engine.online_cost_reported().map_err(internal)?;
        engine
            .update_online_state(|s| {
                if let Some(p) = patch.phones {
                    s.phones = p;
                }
                if let Some(v) = patch.send_sensitive {
                    s.send_sensitive = v;
                }
                if let Some(l) = patch.limit.clone() {
                    s.limit = l;
                    if matches!(s.pause, Some(OnlinePause::Limit { .. }))
                        && !limit::reached(&s.limit, questions, spent, cost_reported)
                    {
                        s.pause = None;
                    }
                }
            })
            .map_err(internal)?;
        self.emit();
        Ok(self.view_now())
    }

    /// Where one question may be answered. Online only when Online answers is on and working,
    /// the person did not choose this computer, no download holds the window, the month's
    /// limit is not reached, and the question comes from the desktop or from a phone while
    /// phones may use it. Never online after a failure here: this is asked once per question.
    pub async fn route_for(
        &self,
        lane: &AskLane,
        place: nocturne_contract::AskPlace,
    ) -> RouteChoice {
        self.load();
        let Ok(engine) = self.engine() else {
            return RouteChoice::Off;
        };
        let Ok(row) = engine.online_state() else {
            return RouteChoice::Off;
        };
        if !row.wanted_on {
            return RouteChoice::Off;
        }
        if matches!(lane, AskLane::Phone(_)) && !row.phones {
            return RouteChoice::Off;
        }
        if place == nocturne_contract::AskPlace::ThisComputer {
            return RouteChoice::Computer(LocalReason::Chosen);
        }
        // During a check the engine holds the route being checked, which the person has not
        // agreed to use for questions.
        if self.routes().checking.is_some() {
            return RouteChoice::Computer(LocalReason::Problem);
        }
        let row = self.lift_expired(&engine, row);
        if row.problem.is_some() {
            return RouteChoice::Computer(LocalReason::Problem);
        }
        match row.pause {
            Some(OnlinePause::Limit { .. }) => return RouteChoice::Computer(LocalReason::Limit),
            Some(OnlinePause::KeyRefused) => {
                return RouteChoice::Computer(LocalReason::KeyRefused);
            }
            Some(OnlinePause::NoCredit) => return RouteChoice::Computer(LocalReason::NoCredit),
            None => {}
        }
        let Some(route) = self.routes().active.clone() else {
            return RouteChoice::Computer(LocalReason::Problem);
        };
        // A route found after the engine started (a keychain that came back), or one a
        // cancelled change left unknown, is given to it, never while a change is under way.
        let given = {
            let r = self.routes();
            r.engine_known && same_route(r.engine.as_ref(), Some(&route))
        };
        if !given {
            let Ok(_ops) = self.ops.try_lock() else {
                return RouteChoice::Computer(LocalReason::Problem);
            };
            let still = same_route(self.routes().active.as_ref(), Some(&route));
            if !still || self.give_engine(Some(route.clone())).await.is_err() {
                return RouteChoice::Computer(LocalReason::Problem);
            }
        }
        if self.port.downloading().await {
            return RouteChoice::Computer(LocalReason::Downloading);
        }
        // The question is counted against the limit now, before it is logged, so questions
        // asked at once (the desktop and a phone) cannot go past it together.
        let tz = self.tz();
        let questions = engine.questions_this_month(&tz).unwrap_or(0);
        let spent = engine.spent_this_month(&tz).unwrap_or(None);
        let cost_reported = engine.online_cost_reported().unwrap_or(false);
        let reserved = {
            let mut r = self.routes();
            if limit::reached(&row.limit, questions + r.reserved, spent, cost_reported) {
                false
            } else {
                r.reserved += 1;
                true
            }
        };
        if !reserved {
            self.pause_for_limit(&engine).await;
            return RouteChoice::Computer(LocalReason::Limit);
        }
        let reservation = Reservation(Some(self.routes.clone()));
        let Some(client) = self.port.client().await else {
            return RouteChoice::Computer(LocalReason::Problem);
        };
        let allow = row.send_sensitive;
        let check_engine = engine.clone();
        let sendable = Arc::new(move |ids: &[String]| {
            check_engine
                .sendable(ids, allow)
                .unwrap_or_else(|_| vec![false; ids.len()])
        });
        RouteChoice::Online {
            mind: Arc::new(OnlineMind::new(client, sendable)),
            host: host_of(route.base_url()),
            send_sensitive: allow,
            reservation,
        }
    }

    /// The model name in use, for the log.
    pub fn model(&self) -> String {
        self.routes()
            .active
            .as_ref()
            .map(|r| r.model.clone())
            .unwrap_or_default()
    }

    async fn pause_for_limit(&self, engine: &Engine) {
        let now = engine.clock_now();
        let tz = self.tz();
        let until = limit::next_month_start(now, &tz);
        let _ = engine.update_online_state(|s| {
            s.pause = Some(OnlinePause::Limit {
                until: until.clone(),
            })
        });
        let month = now.to_zoned(tz).strftime("%Y-%m").to_string();
        let first = {
            let mut r = self.routes();
            let first = r.limit_noticed.as_deref() != Some(month.as_str());
            r.limit_noticed = Some(month.clone());
            first
        };
        if first {
            notices::raise(&self.app, NoticeCode::OnlineLimit, Some(&month)).await;
        }
        self.emit();
    }

    #[allow(clippy::too_many_arguments)]
    fn log(
        &self,
        engine: &Engine,
        kind: SendKind,
        question: &str,
        host: &str,
        model: &str,
        receipt: OnlineReceipt,
        result: SendResult,
        counted: bool,
        reached: bool,
    ) -> Option<i64> {
        let mut template = receipt.template;
        let instructions = match template.first() {
            Some(SentLine::Text { .. }) => match template.remove(0) {
                SentLine::Text { text } => text,
                SentLine::Memory { .. } => String::new(),
            },
            _ => String::new(),
        };
        let memory_lines = receipt
            .sent_ids
            .into_iter()
            .zip(receipt.line_hashes)
            .collect();
        let send = NewSend {
            at: engine.clock_now(),
            kind,
            host: host.to_string(),
            model: model.to_string(),
            question: question.to_string(),
            instructions,
            template,
            memory_lines,
            bytes_up: receipt.bytes_up,
            bytes_down: receipt.bytes_down,
            cost_usd: receipt.cost_usd,
            status: receipt.status,
            seconds: Some(receipt.seconds),
            result,
            counted,
            reached,
        };
        if let Err(e) = engine.add_sent_online(receipt.bytes_up) {
            tracing::warn!("the bytes sent could not be counted: {e}");
        }
        match engine.log_online_send(send) {
            Ok(id) => Some(id),
            Err(e) => {
                tracing::warn!("a send could not be logged: {e}");
                None
            }
        }
    }

    /// Logs and counts one question that reached the route, pauses for a refused key, no
    /// credit or the limit, and raises the notice for each. Returns the sent log's id.
    pub async fn record(&self, outcome: AskOutcome) -> Option<i64> {
        let engine = self.engine().ok()?;
        // Reached: a status came back that is not the engine's own network failure.
        let reached = outcome.receipt.status.is_some()
            && !matches!(
                outcome.failure,
                Some(OnlineFailure::NoInternet | OnlineFailure::Unreachable)
            );
        let id = self.log(
            &engine,
            SendKind::Question,
            &outcome.question,
            &outcome.host,
            &outcome.model,
            outcome.receipt,
            outcome.result,
            outcome.counted,
            reached,
        );
        let now = engine.clock_now();
        match outcome.failure {
            Some(OnlineFailure::KeyRefused) => {
                let _ = engine.update_online_state(|s| s.pause = Some(OnlinePause::KeyRefused));
                notices::raise(&self.app, NoticeCode::OnlineKeyRefused, None).await;
            }
            Some(OnlineFailure::NoCredit) => {
                let _ = engine.update_online_state(|s| s.pause = Some(OnlinePause::NoCredit));
                notices::raise(&self.app, NoticeCode::OnlineNoCredit, None).await;
            }
            Some(OnlineFailure::ModelMissing) => {
                let _ = engine.update_online_state(|s| {
                    s.problem = Some(OnlineProblem::ModelMissing);
                    s.problem_since = Some(now);
                });
            }
            _ => {}
        }
        if let Ok(row) = engine.online_state() {
            let tz = self.tz();
            let questions = engine.questions_this_month(&tz).unwrap_or(0);
            let spent = engine.spent_this_month(&tz).unwrap_or(None);
            let cost_reported = engine.online_cost_reported().unwrap_or(false);
            // This question is both in the log now and still reserved: count it once.
            let reserved = self.routes().reserved.saturating_sub(1);
            if row.pause.is_none()
                && limit::reached(&row.limit, questions + reserved, spent, cost_reported)
            {
                self.pause_for_limit(&engine).await;
            }
        }
        self.emit();
        id
    }

    /// Logs a question that was routed online but never left: nothing was sent, nothing is
    /// counted.
    pub async fn record_not_sent(&self, question: &str, host: &str, reason: LocalReason) {
        let Ok(engine) = self.engine() else {
            return;
        };
        let model = self.model();
        self.log(
            &engine,
            SendKind::Question,
            question,
            host,
            &model,
            OnlineReceipt::default(),
            SendResult::NotSent { reason },
            false,
            false,
        );
        self.emit();
    }

    /// Before everything is deleted: questions on their way online stop, the key is removed
    /// from both homes, and the engine is relaunched without the route.
    pub async fn before_delete(&self) {
        self.gate.send_modify(|g| *g += 1);
        self.cancel_check();
        let _ops = self.ops.lock().await;
        {
            let mut r = self.routes();
            r.active = None;
            r.provisional = None;
            r.passed = false;
        }
        if let Err(e) = self.keys.forget() {
            tracing::warn!("the key could not be removed: {e}");
            self.routes().key_left = true;
        }
        if let Err(e) = self.give_engine(None).await {
            tracing::warn!("the engine did not let go of the route: {e}");
        }
    }

    /// After everything is deleted: Online answers' own folder goes, and the interface hears
    /// that it is off.
    pub fn after_delete(&self) {
        if let Some(state) = self.app.try_state::<AppState>() {
            let _ = std::fs::remove_dir_all(state.data_dir.join(super::keys::FOLDER));
        }
        let key_left = {
            let mut r = self.routes();
            r.limit_noticed = None;
            r.reserved = 0;
            r.key_left
        };
        // The person is told the key may still be in the keychain: the view says it is kept.
        if key_left && let Ok(engine) = self.engine() {
            let _ = engine.update_online_state(|s| s.key_storage = Some(KeyStorage::Keychain));
        }
        self.emit();
    }

    /// Prunes the sent log to its last 90 days.
    pub fn prune(&self) {
        let Ok(engine) = self.engine() else {
            return;
        };
        let days = i64::from(nocturne_lemonade::online::SENT_KEPT_DAYS);
        if let Ok(before) = engine
            .clock_now()
            .checked_sub(jiff::SignedDuration::from_hours(24 * days))
            && let Err(e) = engine.prune_online_sends(before)
        {
            tracing::warn!("the sent log could not be pruned: {e}");
        }
    }
}

/// What a check is logged as asking.
const CHECK_LOG_QUESTION: &str = nocturne_lemonade::online::CHECK_QUESTION;
