//! The calendar and Health on This phone: the screens' commands, the plugins behind the sync's
//! [`Device`], the paired phone behind its [`Link`], and when a send starts. A send starts when
//! the app opens and the computer answers, each time the app comes back to the front, and when
//! the screens ask; a send that just ran is not repeated for a little while.

use std::time::{Duration, Instant};

use nocturne_channel::client::{CallError, Connection, Phone};
use nocturne_contract::{
    CalendarBatch, PhoneCalendar, PhoneError, PhoneFeature, PhoneMode, ReadingBatch, ReadingKind,
    RemoteAnswer, RemoteCall, SyncReceipt,
};
use serde::Deserialize;
use tauri::{AppHandle, Emitter, Manager, Runtime, State};
use tauri_plugin_nocturne_calendar::{self as calendar, CalendarExt};
use tauri_plugin_nocturne_health::{self as health, HealthExt};

use crate::home::Home;
use crate::state::AppState;
use crate::sync::{
    Access, Availability, CalendarSettings, Changes, Device, EngineSink, FullRead, Granted,
    HealthSettings, Instances, LastSent, Link, LocalStore, SourceError, SyncReport,
    ThisPhoneSources,
};

/// A send started on its own within this long of the last is skipped.
const QUIET: Duration = Duration::from_secs(20);

fn calendar_error(e: calendar::Error) -> SourceError {
    match e {
        calendar::Error::NotAllowed => SourceError::NotAllowed,
        calendar::Error::Unavailable => SourceError::Unavailable,
        calendar::Error::Failed(s) => SourceError::Failed(s),
    }
}

fn health_error(e: health::Error) -> SourceError {
    match e {
        health::Error::NotAllowed => SourceError::NotAllowed,
        health::Error::Unavailable => SourceError::Unavailable,
        health::Error::Failed(s) => SourceError::Failed(s),
    }
}

fn access(a: calendar::Access) -> Access {
    match a {
        calendar::Access::Granted => Access::Granted,
        calendar::Access::Prompt => Access::Ask,
        calendar::Access::PromptWithRationale => Access::Refused,
        calendar::Access::Denied => Access::Blocked,
    }
}

fn availability(a: health::Availability) -> Availability {
    match a {
        health::Availability::Available => Availability::Available,
        health::Availability::NeedsUpdate => Availability::NeedsUpdate,
        health::Availability::Missing => Availability::Missing,
    }
}

fn granted(g: health::Granted) -> Granted {
    Granted {
        kinds: g.kinds,
        history: g.history,
        distance: g.distance,
        refusals_hidden: g.refusals_hidden,
    }
}

/// The phone's own calendar and Health, through the plugins.
pub struct Plugins<R: Runtime>(pub AppHandle<R>);

/// A plugin call waits on the phone's own side: off the async workers, so they keep running.
fn waiting<T>(f: impl FnOnce() -> T) -> T {
    match tokio::runtime::Handle::try_current() {
        Ok(h) if h.runtime_flavor() == tokio::runtime::RuntimeFlavor::MultiThread => {
            tokio::task::block_in_place(f)
        }
        _ => f(),
    }
}

impl<R: Runtime> Device for Plugins<R> {
    fn calendar_access(&self) -> Access {
        waiting(|| access(self.0.calendar().access()))
    }

    fn calendars(&self) -> Result<Vec<PhoneCalendar>, SourceError> {
        waiting(|| self.0.calendar().calendars()).map_err(calendar_error)
    }

    fn instances(
        &self,
        back: u32,
        ahead: u32,
        calendar_ids: &[String],
        with_notes: bool,
    ) -> Result<Instances, SourceError> {
        waiting(|| {
            self.0
                .calendar()
                .instances(back, ahead, calendar_ids, with_notes)
        })
        .map(|i| Instances {
            window_from: i.window_from,
            window_to: i.window_to,
            entries: i.entries,
        })
        .map_err(calendar_error)
    }

    fn health_availability(&self) -> Availability {
        waiting(|| availability(self.0.health().availability()))
    }

    fn health_granted(&self) -> Result<Granted, SourceError> {
        waiting(|| self.0.health().granted())
            .map(granted)
            .map_err(health_error)
    }

    fn health_read(&self, kinds: &[ReadingKind], days: u32) -> Result<FullRead, SourceError> {
        waiting(|| self.0.health().read(kinds, days))
            .map(|r| FullRead {
                readings: r.readings,
                token: r.token,
                from_day: r.from_day,
            })
            .map_err(health_error)
    }

    fn health_changes(
        &self,
        kinds: &[ReadingKind],
        token: &str,
        refresh_days: u32,
    ) -> Result<Changes, SourceError> {
        waiting(|| self.0.health().changes(kinds, token, refresh_days))
            .map(|c| Changes {
                readings: c.readings,
                removed: c.removed,
                token: c.token,
                expired: c.expired,
                refreshed: c.refreshed,
            })
            .map_err(health_error)
    }
}

/// The paired phone's way to its computer.
pub struct PhoneLink(pub Phone);

impl Link for PhoneLink {
    fn send(
        &self,
        call: RemoteCall,
    ) -> impl std::future::Future<Output = Result<RemoteAnswer, CallError>> + Send {
        let phone = self.0.clone();
        async move { phone.call(call).await }
    }

    fn pause(&self, how_long: Duration) -> impl std::future::Future<Output = ()> + Send {
        tokio::time::sleep(how_long)
    }
}

fn now() -> String {
    jiff::Timestamp::now().to_string()
}

/// This phone's own memory as the place a send lands. A memory that is not there to take the
/// batch answers `Busy` and the send is tried again after a pause; once the phone is no longer
/// alone it answers `SessionExpired`, which ends the send at once with the changes still
/// waiting. Nothing is refused or dropped either way.
impl LocalStore for Home {
    fn take_calendar(&self, batch: CalendarBatch) -> Result<SyncReceipt, PhoneError> {
        self.with_local(|o| o.engine.take_calendar(&o.device, batch))
            .ok_or_else(|| self.not_there())?
            .map_err(|_| PhoneError::Invalid)
    }

    fn take_readings(&self, batch: ReadingBatch) -> Result<SyncReceipt, PhoneError> {
        self.with_local(|o| o.engine.take_readings(&o.device, batch))
            .ok_or_else(|| self.not_there())?
            .map_err(|_| PhoneError::Invalid)
    }
}

impl Home {
    fn not_there(&self) -> PhoneError {
        match self.mode_file() {
            Some(f) if f.mode == PhoneMode::Alone => PhoneError::Busy,
            _ => PhoneError::SessionExpired,
        }
    }
}

/// The managed [`Home`], as a store a send can own.
pub struct HomeStore<R: Runtime>(pub AppHandle<R>);

impl<R: Runtime> LocalStore for HomeStore<R> {
    fn take_calendar(&self, batch: CalendarBatch) -> Result<SyncReceipt, PhoneError> {
        self.0.state::<Home>().inner().take_calendar(batch)
    }

    fn take_readings(&self, batch: ReadingBatch) -> Result<SyncReceipt, PhoneError> {
        self.0.state::<Home>().inner().take_readings(batch)
    }
}

/// Whether a send that was not asked for is too soon after the last.
fn too_soon(state: &AppState, asked: bool) -> bool {
    let mut last = state.last_send.lock().unwrap();
    if !asked && last.is_some_and(|t| t.elapsed() < QUIET) {
        return true;
    }
    *last = Some(Instant::now());
    false
}

/// Send what changed: into this phone's own memory while it is alone, else to the computer if
/// the phone is paired and the computer answers. `asked`: the screens asked for it, so it runs
/// even right after another and opens a session if none is open. The mode is read at each send,
/// so after a move the next send uses the other sink and the same ledger.
pub async fn send<R: Runtime>(app: &AppHandle<R>, asked: bool) -> Option<SyncReport> {
    let state = app.state::<AppState>();
    let alone =
        app.state::<Home>().mode(state.store.identity().is_some()) == Some(PhoneMode::Alone);
    let report = if alone {
        if too_soon(&state, asked) {
            return None;
        }
        let sink = EngineSink::new(HomeStore(app.clone()));
        let takes = takes(app, &state);
        state
            .syncer
            .sync_to(&Plugins(app.clone()), &sink, &takes, now())
            .await?
    } else {
        let phone = state.phone(app)?;
        let connection = match phone.state() {
            connected @ Connection::Connected { .. } => connected,
            _ if asked => phone.connect().await,
            _ => return None,
        };
        let Connection::Connected { hello, .. } = connection else {
            return None;
        };
        if too_soon(&state, asked) {
            return None;
        }
        state
            .syncer
            .sync(
                &Plugins(app.clone()),
                &PhoneLink(phone),
                &hello.features,
                now(),
            )
            .await?
    };
    let _ = app.emit("sync-report", &report);
    Some(report)
}

/// Start a send in the background.
pub fn start<R: Runtime>(app: &AppHandle<R>) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        send(&app, false).await;
    });
}

/// What the memory takes: everything while it lives on this phone, else what the computer says.
fn takes<R: Runtime>(app: &AppHandle<R>, state: &AppState) -> Vec<PhoneFeature> {
    if alone(app, state) {
        return vec![PhoneFeature::Calendar, PhoneFeature::Health];
    }
    state.store.meta().features
}

fn alone<R: Runtime>(app: &AppHandle<R>, state: &AppState) -> bool {
    app.state::<Home>().mode(state.store.identity().is_some()) == Some(PhoneMode::Alone)
}

#[tauri::command]
pub fn sources<R: Runtime>(app: AppHandle<R>, state: State<'_, AppState>) -> ThisPhoneSources {
    let takes = takes(&app, &state);
    state.syncer.sources(&Plugins(app), &takes)
}

#[tauri::command]
pub fn calendar_status<R: Runtime>(app: AppHandle<R>) -> Access {
    access(app.calendar().access())
}

/// The phone's own prompt; answers what it says.
#[tauri::command]
pub async fn calendar_request<R: Runtime>(app: AppHandle<R>) -> Result<Access, String> {
    waiting(|| app.calendar().request())
        .map(access)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn calendar_list<R: Runtime>(app: AppHandle<R>) -> Result<Vec<PhoneCalendar>, String> {
    waiting(|| app.calendar().calendars()).map_err(|e| e.to_string())
}

/// The calendar's settings, with every calendar on the phone and its switch.
#[tauri::command]
pub async fn calendar_settings<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
) -> Result<CalendarSettings, String> {
    let all = waiting(|| app.calendar().calendars()).unwrap_or_default();
    Ok(state.syncer.ledger.get().calendar.settings.filled(&all))
}

/// Keep the person's choice and send at once; turned off, nothing more goes across.
#[tauri::command]
pub async fn calendar_save<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    settings: CalendarSettings,
) -> Result<ThisPhoneSources, String> {
    state.syncer.ledger.update(|l| {
        if settings.bring {
            let off = settings.calendars.clone();
            crate::sync::calendar::prune(&mut l.calendar, |c| off.get(c) != Some(&false));
        } else {
            l.calendar.pending.clear();
        }
        l.calendar.settings = settings;
    });
    if state.syncer.ledger.get().calendar.settings.bring {
        start_asked(&app);
    }
    let takes = takes(&app, &state);
    Ok(state.syncer.sources(&Plugins(app.clone()), &takes))
}

#[tauri::command]
pub fn health_availability<R: Runtime>(app: AppHandle<R>) -> Availability {
    availability(app.health().availability())
}

/// Whether "Also bring older readings" can be offered: this Health Connect has them.
#[tauri::command]
pub async fn health_older_available<R: Runtime>(app: AppHandle<R>) -> bool {
    waiting(|| app.health().older_available())
}

/// Health Connect's own screen for the kinds chosen; answers what is allowed after it.
#[tauri::command]
pub async fn health_request<R: Runtime>(
    app: AppHandle<R>,
    kinds: Vec<ReadingKind>,
    history: bool,
) -> Result<Granted, String> {
    waiting(|| app.health().request(&kinds, history))
        .map(granted)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn health_settings(state: State<'_, AppState>) -> HealthSettings {
    state.syncer.ledger.get().health.settings
}

#[tauri::command]
pub async fn health_save<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    settings: HealthSettings,
) -> Result<ThisPhoneSources, String> {
    state.syncer.ledger.update(|l| {
        if settings.bring {
            crate::sync::health::prune(&mut l.health, &settings.kinds);
        } else {
            l.health.pending.clear();
        }
        l.health.settings = settings;
    });
    if state.syncer.ledger.get().health.settings.bring {
        start_asked(&app);
    }
    let takes = takes(&app, &state);
    Ok(state.syncer.sources(&Plugins(app.clone()), &takes))
}

fn start_asked<R: Runtime>(app: &AppHandle<R>) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        send(&app, true).await;
    });
}

/// Where Health is turned on and off, outside the app. (The calendar's permission is on this
/// app's own settings page, which the system plugin opens.)
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SettingsTarget {
    /// Health Connect's own settings.
    Health,
    /// Health Connect in the store, where it is missing.
    HealthStore,
}

#[tauri::command]
pub async fn open_settings<R: Runtime>(
    app: AppHandle<R>,
    target: SettingsTarget,
) -> Result<(), String> {
    match target {
        SettingsTarget::Health => app.health().open_settings(),
        SettingsTarget::HealthStore => app.health().open_store(),
    }
    .map_err(|e| e.to_string())
}

/// Send now; none when nothing could be sent. `asked: false` is the screens coming back to the
/// front, which a send that just ran makes unnecessary.
#[tauri::command]
pub async fn sync_now<R: Runtime>(app: AppHandle<R>, asked: Option<bool>) -> Option<SyncReport> {
    send(&app, asked.unwrap_or(true)).await
}

#[tauri::command]
pub fn last_sent(state: State<'_, AppState>) -> Option<LastSent> {
    state.syncer.ledger.get().last_sent
}

/// The page the app was opened on from Health Connect, once.
#[tauri::command]
pub fn launch_route<R: Runtime>(app: AppHandle<R>) -> Option<String> {
    app.health().take_route()
}

/// The phone's system, `ios` or `android`, for the words that differ between them.
#[tauri::command]
pub fn phone_platform() -> &'static str {
    std::env::consts::OS
}
