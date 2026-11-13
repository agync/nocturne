//! Turning Away from home on and off, and everything that follows from it: the helper's
//! download on consent, the listener, the helper itself, the address, re-linking after a Simple
//! restart, problems, auto-off and keep-awake.
//!
//! Off is the fresh-store state and means off: with `wanted_on = 0` nothing here downloads,
//! spawns, binds or connects. App events are never read as instructions: every decision reads
//! the store and this hub's own state.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use jiff::Timestamp;
use nocturne_contract::{
    DeviceView, LastContact, LedgerKind, PairingOffer, PrivacyView, RemoteAction, RemoteAdvanced,
    RemoteConnection, RemoteProblem, RemoteResult, RemoteSettingsPatch, RemoteState, RemoteView,
    SignInStep,
};
use nocturne_engine::{DeviceRow, Engine};
use nocturne_helpers::{
    HelperError, HelperKind, InstallProgress, Installed, KeyCarrier, LaunchSpec, Transport,
    TransportState,
};
use nocturne_intelligence::{Host, SleepGuard};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use super::limits::Mono;
use super::listener::{self, Bind, ListenerConfig, ListenerHandle};
use super::pairing::{PairError, fmt_wall};
use super::protocol::*;
use super::session::{Core, OnState, Shell};
use super::static_files::PhoneFiles;
use super::{autooff, crypto, keepawake, name, qr};
use crate::error::CommandError;

/// How a helper is found, checked, fetched and run. The app uses the helpers crate; tests use
/// fakes that start nothing.
pub trait Helpers: Send + Sync + 'static {
    fn installed(&self, kind: HelperKind) -> Option<Installed>;
    fn verify(&self, installed: &Installed) -> Result<(), HelperError>;
    fn install(
        &self,
        kind: HelperKind,
        progress: watch::Sender<InstallProgress>,
        cancel: CancellationToken,
    ) -> nocturne_helpers::BoxFuture<'static, Result<Installed, HelperError>>;
    fn transport(&self, kind: HelperKind, installed: &Installed) -> Box<dyn Transport>;
}

/// The real helpers, under `<data>/remote`.
pub struct RealHelpers {
    pub dirs: nocturne_helpers::HelperDirs,
}

impl Helpers for RealHelpers {
    fn installed(&self, kind: HelperKind) -> Option<Installed> {
        nocturne_helpers::installed(kind, &self.dirs)
    }

    fn verify(&self, installed: &Installed) -> Result<(), HelperError> {
        nocturne_helpers::verify(installed)
    }

    fn install(
        &self,
        kind: HelperKind,
        progress: watch::Sender<InstallProgress>,
        cancel: CancellationToken,
    ) -> nocturne_helpers::BoxFuture<'static, Result<Installed, HelperError>> {
        let dirs = self.dirs.clone();
        Box::pin(async move { nocturne_helpers::install(kind, &dirs, progress, cancel).await })
    }

    fn transport(&self, kind: HelperKind, installed: &Installed) -> Box<dyn Transport> {
        nocturne_helpers::transport_for(kind, &self.dirs, installed)
    }
}

pub fn kind_of(c: RemoteConnection) -> HelperKind {
    match c {
        RemoteConnection::Simple => HelperKind::Cloudflared,
        RemoteConnection::MostPrivate => HelperKind::Tailnet,
    }
}

/// A helper down for less than this is not shown.
pub const BLIP: Duration = Duration::from_secs(30);
/// Restarts in an hour that make a problem.
pub const RESTARTS_AN_HOUR: usize = 5;
/// The housekeeping timer while no phone is connected and no pairing is open.
pub const IDLE_PERIOD: Duration = Duration::from_secs(30);
/// ... and while one is.
pub const BUSY_PERIOD: Duration = Duration::from_secs(1);
/// How often old activity is pruned while on.
pub const PRUNE_PERIOD: Duration = Duration::from_secs(600);

#[derive(Default)]
struct Ctl {
    transport: Option<Arc<dyn Transport>>,
    listener: Option<ListenerHandle>,
    tasks: Vec<tauri::async_runtime::JoinHandle<()>>,
    download: Option<CancellationToken>,
}

struct Rt {
    state: RemoteState,
    up: Option<(String, Option<String>)>,
    down_since: Option<Instant>,
    restarts: VecDeque<Instant>,
    awake: Option<SleepGuard>,
    listener: Option<String>,
    origin: Option<nocturne_helpers::Origin>,
    helper_checked: bool,
    started: Option<Instant>,
    last_prune: Option<Instant>,
}

impl Default for Rt {
    fn default() -> Rt {
        Rt {
            state: RemoteState::Off,
            up: None,
            down_since: None,
            restarts: VecDeque::new(),
            awake: None,
            listener: None,
            origin: None,
            helper_checked: false,
            started: None,
            last_prune: None,
        }
    }
}

pub struct RemoteHub {
    pub core: Arc<Core>,
    shell: Arc<dyn Shell>,
    helpers: Arc<dyn Helpers>,
    host: Arc<dyn Host>,
    files: Arc<dyn PhoneFiles>,
    mono: Arc<dyn Mono>,
    dir: PathBuf,
    ctl: tokio::sync::Mutex<Ctl>,
    rt: Mutex<Rt>,
}

fn not_on() -> CommandError {
    CommandError::unavailable("Away from home is off")
}

fn failed(e: impl std::fmt::Display) -> CommandError {
    CommandError::unavailable(format!("Away from home could not do that: {e}"))
}

fn problem_for(e: &HelperError) -> RemoteProblem {
    match e {
        HelperError::NoInternet => RemoteProblem::NoInternet,
        HelperError::Download(_) => RemoteProblem::DownloadFailed,
        HelperError::Mismatch => RemoteProblem::CheckFailed,
        HelperError::AccountBlocked => RemoteProblem::AccountBlocked,
        HelperError::Unsupported => RemoteProblem::Unsupported,
        _ => RemoteProblem::HelperFailed,
    }
}

/// The host of an address (`https://x.example/` to `x.example`).
pub fn host_of(address: &str) -> String {
    let rest = address
        .strip_prefix("https://")
        .or_else(|| address.strip_prefix("http://"))
        .unwrap_or(address);
    rest.split('/').next().unwrap_or(rest).to_string()
}

/// The re-link code's words: the address's first label, read out.
pub fn address_words(host: &str) -> String {
    host.split('.').next().unwrap_or(host).replace('-', " ")
}

impl RemoteHub {
    pub fn new(
        shell: Arc<dyn Shell>,
        helpers: Arc<dyn Helpers>,
        host: Arc<dyn Host>,
        files: Arc<dyn PhoneFiles>,
        mono: Arc<dyn Mono>,
        dir: PathBuf,
    ) -> Arc<RemoteHub> {
        let core = Core::new(shell.clone(), mono.clone(), dir.clone());
        Arc::new(RemoteHub {
            core,
            shell,
            helpers,
            host,
            files,
            mono,
            dir,
            ctl: tokio::sync::Mutex::new(Ctl::default()),
            rt: Mutex::new(Rt::default()),
        })
    }

    fn rt(&self) -> MutexGuard<'_, Rt> {
        self.rt.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn engine(&self) -> Result<Engine, CommandError> {
        self.shell
            .engine()
            .ok_or_else(|| CommandError::unavailable("Nocturne's store is not open"))
    }

    pub fn dir(&self) -> &std::path::Path {
        &self.dir
    }

    fn set_state(&self, state: RemoteState) {
        let changed = {
            let mut rt = self.rt();
            let changed = rt.state != state;
            rt.state = state;
            changed
        };
        if changed {
            self.shell.changed();
        }
    }

    pub fn state(&self) -> RemoteState {
        self.refresh();
        self.rt().state.clone()
    }

    // The view -----------------------------------------------------------------------------

    pub async fn view(&self) -> RemoteView {
        self.refresh();
        let engine = self.shell.engine();
        let row = engine.as_ref().and_then(|e| e.remote_state().ok());
        let devices = engine
            .as_ref()
            .and_then(|e| e.devices(false).ok())
            .unwrap_or_default();
        let live = self.core.live_devices();
        let connection = row
            .as_ref()
            .map(|r| r.connection)
            .unwrap_or(RemoteConnection::Simple);
        let host = self.core.host();
        let relink = connection == RemoteConnection::Simple;
        let state = self.rt().state.clone();
        let installed = [RemoteConnection::Simple, RemoteConnection::MostPrivate]
            .into_iter()
            .filter(|c| self.helpers.installed(kind_of(*c)).is_some())
            .collect();
        let pairing = self
            .core
            .pairings
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .status(self.mono.now());
        RemoteView {
            devices: devices
                .iter()
                .map(|d| DeviceView {
                    id: d.id.clone(),
                    name: d.name.clone(),
                    paired_at: fmt_wall(d.paired_at),
                    last_used_at: d.last_used_at.map(fmt_wall),
                    active: live.contains(&d.id),
                    needs_address: relink
                        && host.is_some()
                        && d.address.as_deref() != host.as_deref(),
                })
                .collect(),
            connection,
            auto_off: row
                .as_ref()
                .map(|r| r.auto_off)
                .unwrap_or(nocturne_contract::AutoOff::Unused30Days),
            keep_awake: row.as_ref().is_some_and(|r| r.keep_awake),
            sent_to_phones_bytes: row.as_ref().map(|r| r.sent_bytes).unwrap_or(0.0),
            last_on_at: row.as_ref().and_then(|r| r.last_on_at).map(fmt_wall),
            on_since: row.as_ref().and_then(|r| r.on_since).map(fmt_wall),
            set_up_before: row.as_ref().is_some_and(|r| r.first_on_at.is_some()),
            installed,
            trial: connection == RemoteConnection::Simple,
            pairing,
            computer_name: name::computer_name(),
            state,
        }
    }

    pub fn device_view(&self, d: &DeviceRow) -> DeviceView {
        let host = self.core.host();
        DeviceView {
            id: d.id.clone(),
            name: d.name.clone(),
            paired_at: fmt_wall(d.paired_at),
            last_used_at: d.last_used_at.map(fmt_wall),
            active: self.core.live_devices().contains(&d.id),
            needs_address: d.connection == RemoteConnection::Simple
                && host.is_some()
                && d.address.as_deref() != host.as_deref(),
        }
    }

    /// Recompute what follows from time and the store: a helper down long enough is a
    /// problem; an address every phone has is no longer a re-link. The new state is written
    /// under the same lock it was decided under, so a state the helper reports meanwhile (a
    /// failure) is never overwritten by one decided from the state before it.
    pub fn refresh(&self) {
        let now = self.mono.now();
        let changed = {
            let mut rt = self.rt();
            while rt
                .restarts
                .front()
                .is_some_and(|t| now.duration_since(*t) > Duration::from_secs(3600))
            {
                rt.restarts.pop_front();
            }
            let mut next = None;
            let down_long = rt.down_since.is_some_and(|t| now.duration_since(t) >= BLIP);
            let many = rt.restarts.len() >= RESTARTS_AN_HOUR;
            if (down_long || many)
                && !matches!(rt.state, RemoteState::Problem { .. } | RemoteState::Off)
            {
                next = Some(RemoteState::Problem {
                    code: RemoteProblem::HelperFailed,
                    attempts: rt.restarts.len() as u32,
                    since: fmt_wall(self.shell.wall_now()),
                });
            } else if rt.down_since.is_none()
                && let Some((address, machine)) = rt.up.clone()
                && matches!(
                    rt.state,
                    RemoteState::On { .. } | RemoteState::NeedsRelink { .. }
                )
            {
                next = Some(self.up_state(&address, machine));
            }
            match next {
                Some(s) if s != rt.state => {
                    rt.state = s;
                    true
                }
                _ => false,
            }
        };
        if changed {
            self.shell.changed();
        }
    }

    /// On, or needs re-link when a Simple phone has not seen this address yet.
    fn up_state(&self, host: &str, machine: Option<String>) -> RemoteState {
        let relink = self.shell.engine().is_some_and(|e| {
            e.remote_state()
                .is_ok_and(|r| r.connection == RemoteConnection::Simple)
                && e.devices(false)
                    .is_ok_and(|ds| ds.iter().any(|d| d.address.as_deref() != Some(host)))
        });
        if relink {
            let link = format!("https://{host}{PATH_RELINK}#{host}");
            RemoteState::NeedsRelink {
                address: host.to_string(),
                qr: qr::matrix(&link),
                pairing_link: link,
                words: address_words(host),
            }
        } else {
            RemoteState::On {
                address: host.to_string(),
                machine_name: machine,
            }
        }
    }

    // Turning on and off ---------------------------------------------------------------------

    /// Turn on with `connection`. `allow_download` is the person's yes to fetching its helper.
    pub async fn start(
        self: &Arc<Self>,
        connection: RemoteConnection,
        allow_download: bool,
    ) -> Result<RemoteView, CommandError> {
        let engine = self.engine()?;
        let kind = kind_of(connection);
        let installed = self.helpers.installed(kind);
        if installed.is_none() && !allow_download {
            return Err(CommandError::invalid(
                "the connection must be downloaded first",
            ));
        }
        let previous = engine.remote_state().map_err(failed)?.connection;
        if previous != connection {
            self.unpair_all(&engine, RemoteAction::SwitchedConnection)?;
        }
        let mut ctl = self.ctl.lock().await;
        if ctl.transport.is_some() || ctl.download.is_some() {
            drop(ctl);
            let problem = matches!(self.rt().state, RemoteState::Problem { .. });
            if previous == connection && !problem {
                return Ok(self.view().await);
            }
            self.stop_inner(previous == RemoteConnection::MostPrivate, false)
                .await;
            ctl = self.ctl.lock().await;
        }
        engine
            .update_remote_state(|r| {
                r.connection = connection;
                r.wanted_on = true;
            })
            .map_err(failed)?;
        match installed {
            Some(i) => {
                self.set_state(RemoteState::Starting);
                self.launch(&mut ctl, connection, i).await;
            }
            None => {
                let cancel = CancellationToken::new();
                ctl.download = Some(cancel.clone());
                self.set_state(RemoteState::Downloading {
                    received_bytes: 0.0,
                    total_bytes: 0.0,
                    paused: false,
                });
                let me = self.clone();
                ctl.tasks.push(tauri::async_runtime::spawn(async move {
                    me.download(connection, cancel).await;
                }));
            }
        }
        drop(ctl);
        Ok(self.view().await)
    }

    async fn download(self: Arc<Self>, connection: RemoteConnection, cancel: CancellationToken) {
        let kind = kind_of(connection);
        let (tx, mut rx) = watch::channel(InstallProgress::default());
        let me = self.clone();
        let progress = tauri::async_runtime::spawn(async move {
            while rx.changed().await.is_ok() {
                let p = *rx.borrow_and_update();
                me.set_state(RemoteState::Downloading {
                    received_bytes: p.received as f64,
                    total_bytes: p.total as f64,
                    paused: false,
                });
            }
        });
        let result = self.helpers.install(kind, tx, cancel.clone()).await;
        progress.abort();
        let mut ctl = self.ctl.lock().await;
        if ctl.download.as_ref().is_some_and(|c| c.is_cancelled()) || cancel.is_cancelled() {
            // Paused (the part is kept) or stopped: the state is the caller's.
            ctl.download = None;
            return;
        }
        ctl.download = None;
        match result {
            Ok(installed) => {
                let bytes = std::fs::metadata(&installed.path)
                    .map(|m| m.len() as f64)
                    .unwrap_or(0.0);
                if let Ok(e) = self.engine() {
                    let _ = e.record_download(
                        LedgerKind::RemoteHelper,
                        helper_label(kind),
                        None,
                        bytes,
                    );
                }
                self.set_state(RemoteState::Starting);
                self.launch(&mut ctl, connection, installed).await;
            }
            Err(e) => self.problem(problem_for(&e)),
        }
    }

    fn problem(&self, code: RemoteProblem) {
        let attempts = self.rt().restarts.len() as u32;
        self.set_state(RemoteState::Problem {
            code,
            attempts,
            since: fmt_wall(self.shell.wall_now()),
        });
    }

    /// Check the helper, start the listener and the helper, and follow the helper's state.
    async fn launch(self: &Arc<Self>, ctl: &mut Ctl, connection: RemoteConnection, i: Installed) {
        let kind = kind_of(connection);
        let checked = self.helpers.verify(&i);
        self.rt().helper_checked = checked.is_ok();
        if let Err(e) = checked {
            self.problem(problem_for(&e));
            return;
        }
        let computer = match crypto::load_or_create_computer_key(&self.dir) {
            Ok(k) => Arc::new(k),
            Err(e) => {
                tracing::warn!("the computer key could not be read: {e}");
                self.problem(RemoteProblem::HelperFailed);
                return;
            }
        };
        self.core.incoming.clear_all();
        self.core.set_on(Some(OnState {
            computer,
            connection,
            host: None,
        }));
        let transport: Arc<dyn Transport> = Arc::from(self.helpers.transport(kind, &i));
        let bind = match transport.key_carrier() {
            KeyCarrier::UnixSocket if cfg!(unix) => {
                Bind::Unix(self.dir.join("run").join("listener.sock"))
            }
            carrier => Bind::Tcp {
                key: crypto::b64(&crypto::random::<32>()).replace(['-', '_'], "x"),
                carrier,
            },
        };
        let handle = match listener::serve(
            ListenerConfig {
                bind,
                files: self.files.clone(),
            },
            self.core.clone(),
        )
        .await
        {
            Ok(h) => h,
            Err(e) => {
                tracing::warn!("the listener for phones did not start: {e}");
                self.core.set_on(None);
                self.problem(RemoteProblem::HelperFailed);
                return;
            }
        };
        let spec = LaunchSpec {
            origin: handle.origin.clone(),
            machine_name: name::machine_name(&self.dir),
        };
        {
            let mut rt = self.rt();
            rt.listener = Some(handle.describe());
            rt.origin = Some(handle.origin.clone());
            rt.started = Some(self.mono.now());
            rt.down_since = None;
            rt.up = None;
        }
        ctl.listener = Some(handle);
        let rx = match transport.start(spec).await {
            Ok(rx) => rx,
            Err(e) => {
                if let Some(l) = ctl.listener.take() {
                    l.stop().await;
                }
                self.core.set_on(None);
                self.problem(problem_for(&e));
                return;
            }
        };
        ctl.transport = Some(transport);
        if let Ok(e) = self.engine() {
            let now = self.shell.wall_now();
            let _ = e.update_remote_state(|r| {
                r.first_on_at.get_or_insert(now);
                r.on_since.get_or_insert(now);
                r.last_on_at = Some(now);
            });
            let _ = e.record_download(LedgerKind::RemoteOnOff, "on", None, 0.0);
        }
        self.core
            .audit(None, RemoteAction::TurnedOn, RemoteResult::Ok);
        let me = self.clone();
        ctl.tasks.push(tauri::async_runtime::spawn(
            async move { me.follow(rx).await },
        ));
        let me = self.clone();
        ctl.tasks.push(tauri::async_runtime::spawn(async move {
            me.housekeeping().await
        }));
        self.keep_awake();
    }

    /// Follow the helper's own state.
    async fn follow(self: Arc<Self>, mut rx: watch::Receiver<TransportState>) {
        loop {
            let st = rx.borrow_and_update().clone();
            self.on_transport(st);
            if rx.changed().await.is_err() {
                break;
            }
        }
    }

    pub fn on_transport(&self, st: TransportState) {
        let now = self.mono.now();
        match st {
            TransportState::Stopped => {}
            TransportState::Starting => {
                let shown = !matches!(
                    self.rt().state,
                    RemoteState::On { .. } | RemoteState::NeedsRelink { .. }
                );
                if shown {
                    self.set_state(RemoteState::Starting);
                }
            }
            TransportState::NeedsSignIn { url, allow_links } => {
                self.set_state(RemoteState::NeedsSignIn {
                    url,
                    step: if allow_links {
                        SignInStep::AllowLinks
                    } else {
                        SignInStep::SignIn
                    },
                });
            }
            TransportState::NeedsApproval { url } => {
                self.set_state(RemoteState::NeedsSignIn {
                    url,
                    step: SignInStep::Approve,
                });
            }
            TransportState::Up { address, route } => {
                let host = host_of(&address);
                let machine = (self.core.connection() == Some(RemoteConnection::MostPrivate))
                    .then(|| name::machine_name(&self.dir));
                self.core.set_host(Some(host.clone()));
                if let Ok(e) = self.engine() {
                    let now_wall = self.shell.wall_now();
                    let _ = e.update_remote_state(|r| {
                        if r.address.as_deref() != Some(host.as_str()) {
                            r.address = Some(host.clone());
                            r.address_since = Some(now_wall);
                        }
                        r.machine_name = machine.clone();
                    });
                }
                {
                    let mut rt = self.rt();
                    rt.down_since = None;
                    rt.up = Some((host.clone(), machine.clone()));
                }
                let _ = route;
                self.set_state(self.up_state(&host, machine));
            }
            TransportState::Restarting { .. } => {
                let mut rt = self.rt();
                rt.restarts.push_back(now);
                rt.down_since.get_or_insert(now);
                drop(rt);
                self.refresh();
            }
            TransportState::Failed { error } => {
                self.problem(problem_for(&error));
            }
        }
    }

    /// The housekeeping timer: every second while a phone is connected or a pairing is open,
    /// every thirty seconds otherwise.
    pub fn period(&self) -> Duration {
        let pairing = self
            .core
            .pairings
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .status(self.mono.now())
            .is_some_and(|s| {
                matches!(
                    s.phase,
                    nocturne_contract::PairingPhase::WaitingForScan
                        | nocturne_contract::PairingPhase::WaitingForYou { .. }
                )
            });
        if self.core.conv_count() > 0 || pairing {
            BUSY_PERIOD
        } else {
            IDLE_PERIOD
        }
    }

    async fn housekeeping(self: Arc<Self>) {
        let mut flushed = Instant::now();
        loop {
            tokio::time::sleep(self.period()).await;
            self.tick();
            if flushed.elapsed() >= IDLE_PERIOD {
                self.core.flush_sent();
                flushed = Instant::now();
            }
        }
    }

    /// One round of housekeeping (tests call it directly).
    pub fn tick(self: &Arc<Self>) {
        self.core.sweep();
        self.refresh();
        self.keep_awake();
        let now = self.mono.now();
        let due = {
            let mut rt = self.rt();
            let due = rt
                .last_prune
                .is_none_or(|t| now.duration_since(t) >= PRUNE_PERIOD);
            if due {
                rt.last_prune = Some(now);
            }
            due
        };
        if due {
            super::audit::prune(&self.core);
        }
        // Every round, so a computer waking after the due time turns off at once.
        if self.auto_off_due() && self.core.is_on() {
            let me = self.clone();
            tauri::async_runtime::spawn(async move {
                let _ = me.stop(false).await;
            });
        }
    }

    pub fn auto_off_due(&self) -> bool {
        let Some(e) = self.shell.engine() else {
            return false;
        };
        let (Ok(row), Ok(devices)) = (e.remote_state(), e.devices(false)) else {
            return false;
        };
        let used = devices.iter().filter_map(|d| d.last_used_at).max();
        autooff::due(row.auto_off, row.on_since, used, self.shell.wall_now())
    }

    /// Hold the computer awake while on, asked for, and on mains power.
    pub fn keep_awake(&self) {
        let on = !matches!(self.rt().state, RemoteState::Off) && self.core.is_on();
        let asked = self
            .shell
            .engine()
            .and_then(|e| e.remote_state().ok())
            .is_some_and(|r| r.keep_awake);
        let want = keepawake::wanted(on, asked, self.host.on_ac_power());
        let mut rt = self.rt();
        match (want, rt.awake.is_some()) {
            (true, false) => rt.awake = self.host.inhibit_sleep(),
            (false, true) => rt.awake = None,
            _ => {}
        }
    }

    /// Turn off. `remove_all` also unpairs every phone and, for Most private, signs out.
    pub async fn stop(&self, remove_all: bool) -> Result<RemoteView, CommandError> {
        let engine = self.engine()?;
        self.stop_inner(remove_all, true).await;
        if remove_all {
            self.unpair_all(&engine, RemoteAction::Removed)?;
        }
        Ok(self.view().await)
    }

    async fn stop_inner(&self, sign_out: bool, forget_wanted: bool) {
        let mut ctl = self.ctl.lock().await;
        let was_on = ctl.transport.is_some();
        if let Some(d) = ctl.download.take() {
            d.cancel();
        }
        for t in ctl.tasks.drain(..) {
            t.abort();
        }
        // Signing out of Tailscale needs its helper running, so it comes before the stop. With
        // the helper off, its sign-out only forgets the local state and starts nothing.
        if sign_out {
            let running = ctl
                .transport
                .clone()
                .filter(|t| t.kind() == HelperKind::Tailnet);
            let t = running.or_else(|| {
                self.helpers
                    .installed(HelperKind::Tailnet)
                    .map(|i| Arc::from(self.helpers.transport(HelperKind::Tailnet, &i)))
            });
            if let Some(t) = t {
                let _ = t.sign_out().await;
            }
        }
        if let Some(t) = ctl.transport.take()
            && let Err(e) = t.stop().await
        {
            tracing::warn!("the connection helper did not stop cleanly: {e}");
        }
        self.core.flush_sent();
        self.core.set_on(None);
        if let Some(l) = ctl.listener.take() {
            l.stop().await;
        }
        drop(ctl);
        {
            let mut rt = self.rt();
            rt.awake = None;
            rt.up = None;
            rt.down_since = None;
            rt.listener = None;
            rt.origin = None;
            rt.started = None;
        }
        if let Ok(e) = self.engine() {
            let now = self.shell.wall_now();
            let _ = e.update_remote_state(|r| {
                if forget_wanted {
                    r.wanted_on = false;
                }
                if was_on {
                    r.last_off_at = Some(now);
                }
                r.on_since = None;
            });
            if was_on {
                let _ = e.record_download(LedgerKind::RemoteOnOff, "off", None, 0.0);
                self.core
                    .audit(None, RemoteAction::TurnedOff, RemoteResult::Ok);
            }
        }
        self.set_state(RemoteState::Off);
    }

    /// Stop for the app quitting: the helper and the listener, nothing else changes.
    pub async fn shutdown(&self) {
        let mut ctl = self.ctl.lock().await;
        if let Some(d) = ctl.download.take() {
            d.cancel();
        }
        for t in ctl.tasks.drain(..) {
            t.abort();
        }
        if let Some(t) = ctl.transport.take() {
            let _ = t.stop().await;
        }
        self.core.flush_sent();
        self.core.set_on(None);
        if let Some(l) = ctl.listener.take() {
            l.stop().await;
        }
        self.rt().awake = None;
    }

    pub async fn pause_download(&self) -> Result<RemoteView, CommandError> {
        let mut ctl = self.ctl.lock().await;
        let Some(d) = ctl.download.take() else {
            drop(ctl);
            return Ok(self.view().await);
        };
        d.cancel();
        drop(ctl);
        let (received, total) = match self.rt().state {
            RemoteState::Downloading {
                received_bytes,
                total_bytes,
                ..
            } => (received_bytes, total_bytes),
            _ => (0.0, 0.0),
        };
        self.set_state(RemoteState::Downloading {
            received_bytes: received,
            total_bytes: total,
            paused: true,
        });
        Ok(self.view().await)
    }

    fn unpair_all(&self, engine: &Engine, action: RemoteAction) -> Result<u32, CommandError> {
        let devices = engine.devices(false).map_err(failed)?;
        let n = engine.revoke_all_devices().map_err(failed)?;
        for d in &devices {
            self.core.drop_device(&d.id);
        }
        if n > 0 {
            self.core.audit(None, action, RemoteResult::Ok);
        }
        self.shell.changed();
        Ok(n)
    }

    // Phones ---------------------------------------------------------------------------------

    pub async fn begin_pairing(&self) -> Result<PairingOffer, CommandError> {
        let (Some(computer), Some(host)) = (self.core.computer(), self.core.host()) else {
            return Err(not_on());
        };
        let offer = self
            .core
            .pairings
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .begin(computer, &host, self.mono.now(), self.shell.wall_now());
        let offer = offer.map_err(|e| match e {
            PairError::Paused => CommandError::invalid("adding a phone is paused for a while"),
            _ => not_on(),
        })?;
        self.shell.changed();
        Ok(offer)
    }

    pub fn cancel_pairing(&self) {
        let had = self
            .core
            .pairings
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .cancel();
        if had {
            self.shell.changed();
        }
    }

    pub async fn decide(&self, pairing_id: &str, allow: bool) -> Result<(), CommandError> {
        let engine = self.engine()?;
        let now = self.mono.now();
        let device = self
            .core
            .pairings
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .waiting(pairing_id, now)
            .map_err(|_| CommandError::not_found("that phone"))?;
        let connection = self.core.connection().ok_or_else(not_on)?;
        let id = if allow {
            let row = engine
                .add_device(
                    &device.name,
                    &device.public_key,
                    &device.home_origin,
                    connection,
                    &device.address,
                )
                .map_err(failed)?;
            Some(row.id)
        } else {
            None
        };
        let sent = self
            .core
            .pairings
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .decide(id.as_deref());
        if let Some((conv, frame)) = sent {
            self.core.send_to(conv, frame);
        }
        let action = if allow {
            RemoteAction::Allowed
        } else {
            RemoteAction::Refused
        };
        self.core.audit(id.as_deref(), action, RemoteResult::Ok);
        self.shell.changed();
        Ok(())
    }

    pub async fn rename_device(&self, id: &str, name: &str) -> Result<DeviceView, CommandError> {
        let name = name.trim();
        if crypto::check_device_name(name).is_err() {
            return Err(CommandError::invalid("that name cannot be used"));
        }
        let row = self
            .engine()?
            .rename_device(id, name)
            .map_err(|_| CommandError::not_found("that phone"))?;
        self.core
            .audit(Some(id), RemoteAction::Renamed, RemoteResult::Ok);
        self.shell.changed();
        Ok(self.device_view(&row))
    }

    /// Remove a phone: its sessions and held polls close now, its answers stop. Its next open
    /// gets a sealed REVOKED.
    pub async fn remove_device(&self, id: &str) -> Result<(), CommandError> {
        let removed = self.engine()?.revoke_device(id).map_err(failed)?;
        if !removed {
            return Err(CommandError::not_found("that phone"));
        }
        self.core.drop_device(id);
        self.core
            .audit(Some(id), RemoteAction::Removed, RemoteResult::Ok);
        self.shell.changed();
        Ok(())
    }

    pub async fn update_settings(
        self: &Arc<Self>,
        patch: RemoteSettingsPatch,
    ) -> Result<RemoteView, CommandError> {
        let engine = self.engine()?;
        let before = engine.remote_state().map_err(failed)?;
        engine
            .update_remote_state(|r| {
                if let Some(a) = patch.auto_off {
                    r.auto_off = a;
                }
                if let Some(k) = patch.keep_awake {
                    r.keep_awake = k;
                }
            })
            .map_err(failed)?;
        if let Some(to) = patch.connection
            && to != before.connection
        {
            self.unpair_all(&engine, RemoteAction::SwitchedConnection)?;
            let (running, downloading) = {
                let ctl = self.ctl.lock().await;
                (ctl.transport.is_some(), ctl.download.is_some())
            };
            let leaving_tailscale = before.connection == RemoteConnection::MostPrivate;
            if running || downloading || leaving_tailscale {
                self.stop_inner(leaving_tailscale, false).await;
            }
            if running || downloading {
                engine
                    .update_remote_state(|r| r.connection = to)
                    .map_err(failed)?;
                // Mid-download the person is turning on: fetching the other connection's
                // helper is the same yes, given for the connection now chosen.
                return self.start(to, downloading).await;
            }
            engine
                .update_remote_state(|r| r.connection = to)
                .map_err(failed)?;
        }
        self.keep_awake();
        self.shell.changed();
        Ok(self.view().await)
    }

    /// The sign-in address the helper asked for, never one from the interface.
    pub fn sign_in_url(&self) -> Option<String> {
        match &self.rt().state {
            RemoteState::NeedsSignIn { url, .. } => Some(url.clone()),
            _ => None,
        }
    }

    pub async fn new_computer_key(&self) -> Result<RemoteView, CommandError> {
        let engine = self.engine()?;
        let key = crypto::replace_computer_key(&self.dir).map_err(failed)?;
        self.unpair_all(&engine, RemoteAction::Removed)?;
        if let (Some(connection), host) = (self.core.connection(), self.core.host()) {
            self.core.set_on(None);
            self.core.set_on(Some(OnState {
                computer: Arc::new(key),
                connection,
                host,
            }));
        }
        Ok(self.view().await)
    }

    // Other views ----------------------------------------------------------------------------

    pub fn privacy(&self, view: &mut PrivacyView) {
        let Some(row) = self.shell.engine().and_then(|e| e.remote_state().ok()) else {
            return;
        };
        view.sent_to_phones_bytes = row.sent_bytes;
        view.remote_on = row.wanted_on && !matches!(self.rt().state, RemoteState::Off);
        view.remote_connection = row.first_on_at.map(|_| row.connection);
        view.remote_last_on_at = row.last_on_at.map(fmt_wall);
    }

    pub fn advanced(&self) -> Option<RemoteAdvanced> {
        let engine = self.shell.engine()?;
        let row = engine.remote_state().ok()?;
        row.first_on_at?;
        let kind = kind_of(row.connection);
        let installed = self.helpers.installed(kind);
        let info = self
            .ctl
            .try_lock()
            .ok()
            .and_then(|c| c.transport.as_ref().and_then(|t| t.info()));
        let rt = self.rt();
        let up = rt.up.clone();
        let devices = engine.devices(true).unwrap_or_default();
        let last = devices
            .iter()
            .filter_map(|d| d.last_used_at.map(|t| (t, d.name.clone())))
            .max_by_key(|(t, _)| *t);
        Some(RemoteAdvanced {
            connection: row.connection,
            helper_name: Some(helper_name(kind).to_string()),
            helper_version: installed.as_ref().map(|i| i.version.clone()),
            helper_checked: rt.helper_checked,
            helper_uptime_seconds: info
                .as_ref()
                .and_then(|i| i.uptime)
                .map(|d| d.as_secs() as u32),
            helper_restarts: rt.restarts.len() as u32,
            address: up.as_ref().map(|(a, _)| a.clone()),
            address_since: row.address_since.map(fmt_wall),
            listener: rt.listener.clone(),
            route: None,
            machine_name: row.machine_name.clone(),
            last_contact: last.map(|(t, n)| LastContact {
                device_name: n,
                at: fmt_wall(t),
                session_renewed_at: None,
            }),
            helper_estimate_bytes_per_day: None,
            audit_events: engine
                .remote_events(10_000)
                .map(|e| e.len() as u32)
                .unwrap_or(0),
        })
    }

    /// Before delete everything: stop, and sign out of Most private.
    pub async fn before_delete(&self) {
        self.stop_inner(true, true).await;
    }

    /// After delete everything: Away from home's own folder goes.
    pub fn after_delete(&self) {
        let _ = std::fs::remove_dir_all(&self.dir);
        *self.rt() = Rt::default();
    }

    /// Where the listener is while on (tests reach it there).
    pub fn listener_origin(&self) -> Option<nocturne_helpers::Origin> {
        self.rt().origin.clone()
    }

    pub fn wall_now(&self) -> Timestamp {
        self.shell.wall_now()
    }
}

fn helper_name(kind: HelperKind) -> &'static str {
    match kind {
        HelperKind::Cloudflared => "cloudflared",
        HelperKind::Tailnet => "nocturne-tailnet",
    }
}

fn helper_label(kind: HelperKind) -> &'static str {
    match kind {
        HelperKind::Cloudflared => "Cloudflare",
        HelperKind::Tailnet => "Tailscale",
    }
}
