//! Owns one private `lemond`: starts it with a scrubbed environment and a fresh key, watches
//! it, restarts it with backoff, and stops it without leaving a process behind.

use std::collections::VecDeque;
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use process_wrap::tokio::{ChildWrapper, CommandWrap, KillOnDrop};
use tokio::process::{ChildStdin, Command};
use tokio::sync::{mpsc, oneshot, watch};

use crate::admin::{Admin, Keys};
use crate::bundle::{self, Dirs};
use crate::client::{Client, Endpoint, Timeouts};
use crate::config;
use crate::error::RuntimeError;
use crate::logpipe::{self, Redactor};
use crate::online::{self, CloudRoute, OnlineKey, RELAUNCH_IDLE_WAIT_OFF, RELAUNCH_IDLE_WAIT_ON};
use crate::{residency, selection};

const PORT_TRIES: u32 = 5;
const RESTART_WINDOW: Duration = Duration::from_secs(3600);
const LIVE_POLL: Duration = Duration::from_millis(100);
const HUNG_AFTER: u32 = 3;
/// How long each stage of a stop may take: the request, the wait after it, SIGTERM, SIGKILL.
/// Together about twelve seconds; quitting cuts the first three short (see [`Stop::quick`]). A process that exits without running `Drop` (as a Tauri
/// `exit()` does) skips all of this; the engine's own watchdog then ends it, in about six seconds.
const SHUTDOWN_REQUEST: Duration = Duration::from_secs(2);
const AFTER_REQUEST: Duration = Duration::from_secs(4);
const AFTER_TERM: Duration = Duration::from_secs(4);
const AFTER_KILL: Duration = Duration::from_secs(2);
/// How long a route change waits for the running engine to forget the key before it goes on
/// to the relaunch, which ends that engine anyway.
const TAKE_BACK_WAIT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone)]
pub struct SupervisorOptions {
    /// Restarts allowed inside one hour before the engine is given up on.
    pub restart_limit: u32,
    pub health_interval: Duration,
    pub start_timeout: Duration,
    /// The first restart waits this long; each further one doubles it.
    pub backoff_base: Duration,
}

impl Default for SupervisorOptions {
    fn default() -> Self {
        SupervisorOptions {
            restart_limit: 3,
            health_interval: Duration::from_secs(5),
            start_timeout: Duration::from_secs(60),
            backoff_base: Duration::from_secs(1),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeState {
    Stopped,
    Starting,
    Ready { port: u16, version: String },
    Restarting { attempt: u32 },
    Failed { reason: String },
}

/// How patient a stop is: how long the engine has to stop on request, and after SIGTERM.
#[derive(Debug, Clone, Copy)]
struct Stop {
    polite: Duration,
    after_term: Duration,
}

impl Stop {
    fn patient() -> Stop {
        Stop {
            polite: SHUTDOWN_REQUEST + AFTER_REQUEST,
            after_term: AFTER_TERM,
        }
    }

    /// For quitting: the request gets `budget`, then the group gets SIGTERM and a moment.
    fn quick(budget: Duration) -> Stop {
        Stop {
            polite: budget,
            after_term: QUICK_AFTER_TERM,
        }
    }
}

/// How long a quitting app waits after SIGTERM before SIGKILL.
const QUICK_AFTER_TERM: Duration = Duration::from_millis(500);

enum Cmd {
    Shutdown(oneshot::Sender<Result<(), RuntimeError>>, Stop),
    /// Stop the engine and start it again, so launch-time settings take effect.
    Relaunch(Relaunch),
}

/// Which launch-time setting a relaunch changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Changes {
    /// The download window ([`Control::set_online`]).
    Window,
    /// The route for Online answers ([`Control::set_cloud`]).
    Route,
}

/// The answer to a relaunch, and what to put back if it fails.
struct Relaunch {
    reply: oneshot::Sender<Result<(), RuntimeError>>,
    changes: Changes,
    /// Set when this relaunch opens the download window: a relaunch that fails closes it
    /// again, so a window that did not open never opens at a later restart. A failed close is
    /// never undone: the window stays closed.
    closes_on_failure: bool,
    /// Set when this relaunch gives the engine a route: a relaunch that fails takes the route
    /// away again, so no later restart launches the engine with the key unasked.
    drops_cloud_on_failure: bool,
}

impl Relaunch {
    fn new(reply: oneshot::Sender<Result<(), RuntimeError>>, closes_on_failure: bool) -> Self {
        Relaunch {
            reply,
            changes: Changes::Window,
            closes_on_failure,
            drops_cloud_on_failure: false,
        }
    }

    fn route(reply: oneshot::Sender<Result<(), RuntimeError>>, with_route: bool) -> Self {
        Relaunch {
            reply,
            changes: Changes::Route,
            closes_on_failure: false,
            drops_cloud_on_failure: with_route,
        }
    }

    fn succeeded(self) {
        let _ = self.reply.send(Ok(()));
    }

    fn failed(self, shared: &Shared, e: RuntimeError) {
        if self.closes_on_failure {
            shared.online.store(false, Ordering::SeqCst);
        }
        if self.drops_cloud_on_failure {
            *shared.cloud.lock().unwrap_or_else(|e| e.into_inner()) = None;
        }
        let _ = self.reply.send(Err(e));
    }

    /// A failure whose setting a later relaunch already changed again: that later one owns it
    /// now, so nothing is put back.
    fn failed_after(self, shared: &Shared, e: RuntimeError, later: &[Relaunch]) {
        if later.iter().any(|l| l.changes == self.changes) {
            let _ = self.reply.send(Err(e));
        } else {
            self.failed(shared, e);
        }
    }
}

struct Shared {
    restarts: AtomicU32,
    pid: AtomicU32,
    online: AtomicBool,
    ready_since: Mutex<Option<Instant>>,
    /// The route for Online answers every launch outside the download window uses.
    cloud: Mutex<Option<CloudRoute>>,
    /// The running engine holds the route: it took the key and has the online model loaded.
    route_taken: AtomicBool,
    /// Rises whenever the route or the download window changes. A launch notes it when it reads
    /// the route, and gives the engine the key only while it has not moved.
    route_gen: AtomicU64,
    /// How long a relaunch for Online answers waits for model requests in flight to end: when
    /// turning off, and when turning on or checking.
    idle_waits: Mutex<(Duration, Duration)>,
    /// Holders of the download window, and the lock that keeps opening and closing it in order.
    holds: tokio::sync::Mutex<u32>,
}

/// A cheap, cloneable handle on a running supervisor's settings.
#[derive(Clone)]
pub struct Control {
    dirs: Dirs,
    client: Client,
    admin: Admin,
    shared: Arc<Shared>,
    cmd: mpsc::Sender<Cmd>,
}

impl Control {
    pub fn dirs(&self) -> &Dirs {
        &self.dirs
    }

    pub fn client(&self) -> Client {
        self.client.clone()
    }

    pub fn admin(&self) -> Admin {
        self.admin.clone()
    }

    /// Lets the engine reach the internet (for a download the person approved) or not, and
    /// fetch executables or not. The engine reads both only when it starts, so a change
    /// relaunches it and this returns once the new one answers. The choice outlives a restart.
    pub async fn set_online(&self, online: bool) -> Result<(), RuntimeError> {
        let before = self.shared.online.swap(online, Ordering::SeqCst);
        if before == online {
            return Ok(());
        }
        self.shared.route_gen.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        if self
            .cmd
            .send(Cmd::Relaunch(Relaunch::new(tx, online)))
            .await
            .is_err()
        {
            if online {
                self.shared.online.store(false, Ordering::SeqCst);
            }
            return Err(RuntimeError::Unavailable);
        }
        rx.await.map_err(|_| RuntimeError::Unavailable)?
    }

    /// Sets the route for Online answers (`None` for off) and relaunches the engine, which reads
    /// providers only when it starts. Returns once the new one answers. A relaunch is not a
    /// restart: it never counts toward the restart limit. It first waits, up to a limit, for
    /// the model requests in flight (reading, the tidy, the voice) to end. A relaunch asked for
    /// while another one starts the engine waits for it, then the engine starts once more with
    /// the latest window and route: the last change wins, and none stops the engine.
    ///
    /// The running engine forgets the key it holds at once, before any wait. With a route, the
    /// new engine is given it before it answers anyone; one that does not take it runs without
    /// it, and this returns [`RuntimeError::RouteRefused`].
    pub async fn set_cloud(&self, route: Option<CloudRoute>) -> Result<(), RuntimeError> {
        // A launch under way now hands nothing over: what it read is out of date.
        self.shared.route_gen.fetch_add(1, Ordering::SeqCst);
        if self.shared.route_taken.swap(false, Ordering::SeqCst) {
            let _ = tokio::time::timeout(TAKE_BACK_WAIT, online::take_back(&self.admin)).await;
        }
        let (off, on) = *self
            .shared
            .idle_waits
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let cap = if route.is_some() { on } else { off };
        let until = Instant::now() + cap;
        while self.client.busy_total() > 0 && Instant::now() < until {
            tokio::time::sleep(LIVE_POLL).await;
        }
        let with_route = route.is_some();
        *self.shared.cloud.lock().unwrap_or_else(|e| e.into_inner()) = route;
        // Nor does one that started during the wait above, with the route as it was.
        self.shared.route_gen.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        if self
            .cmd
            .send(Cmd::Relaunch(Relaunch::route(tx, with_route)))
            .await
            .is_err()
        {
            *self.shared.cloud.lock().unwrap_or_else(|e| e.into_inner()) = None;
            return Err(RuntimeError::Unavailable);
        }
        rx.await.map_err(|_| RuntimeError::Unavailable)??;
        // A later change that won (off, or the download window) is not a refusal.
        if with_route && self.has_cloud() && !self.download_window_open() && !self.route_taken() {
            return Err(RuntimeError::RouteRefused);
        }
        Ok(())
    }

    /// Whether the running engine holds the route for Online answers: it took the key and has
    /// the online model loaded. False while no engine runs, and after a launch that could not
    /// give it the route.
    pub fn route_taken(&self) -> bool {
        self.shared.route_taken.load(Ordering::SeqCst)
    }

    /// Whether the route still holds: the engine took the key and the online model is still
    /// loaded. The engine unloads every model, the online one too, when a local model fails to
    /// load, and cannot load the online one again until it restarts. An engine that does not
    /// answer the check is given the benefit of the doubt: a send to it fails on its own.
    pub async fn route_held(&self) -> bool {
        if !self.route_taken() {
            return false;
        }
        match self.client.health().await {
            Ok(h) => h
                .all_models_loaded
                .iter()
                .any(|m| m.model_name == online::MODEL_ID),
            Err(_) => true,
        }
    }

    /// Whether the engine is between launches (starting, restarting, or not running): what it
    /// holds is not known yet.
    pub fn starting(&self) -> bool {
        self.shared.pid.load(Ordering::SeqCst) == 0
    }

    /// Shorter waits for [`Control::set_cloud`], for tests.
    #[doc(hidden)]
    pub fn set_idle_waits(&self, off: Duration, on: Duration) {
        *self
            .shared
            .idle_waits
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = (off, on);
    }

    /// Whether the download window is open: the engine then runs without the route.
    pub fn download_window_open(&self) -> bool {
        self.shared.online.load(Ordering::SeqCst)
    }

    /// How many holders the download window has now, or `None` while one is opening or closing
    /// it. For tests that report what a stuck wait was waiting on.
    #[doc(hidden)]
    pub fn download_holds(&self) -> Option<u32> {
        self.shared.holds.try_lock().ok().map(|h| *h)
    }

    /// Whether a route for Online answers is set for the launches outside the download window.
    pub fn has_cloud(&self) -> bool {
        self.shared
            .cloud
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some()
    }

    /// Opens the download window for as long as the returned hold lives. Holds are counted: the
    /// first opens the window (a relaunch), the last one dropped or released closes it.
    pub async fn hold_online(&self) -> Result<OnlineHold, RuntimeError> {
        let mut holds = self.shared.holds.lock().await;
        if *holds == 0 {
            self.set_online(true).await?;
        }
        *holds += 1;
        Ok(OnlineHold {
            control: Some(self.clone()),
        })
    }

    async fn release_online(&self) {
        let mut holds = self.shared.holds.lock().await;
        *holds = holds.saturating_sub(1);
        if *holds == 0
            && let Err(e) = self.set_online(false).await
        {
            tracing::warn!("could not close the download window: {e}");
        }
    }
}

/// A share of the download window. See [`Control::hold_online`].
pub struct OnlineHold {
    control: Option<Control>,
}

impl OnlineHold {
    /// Gives the hold back and waits until the window is closed, if this was the last.
    pub async fn release(mut self) {
        if let Some(c) = self.control.take() {
            c.release_online().await;
        }
    }
}

impl Drop for OnlineHold {
    fn drop(&mut self) {
        let Some(c) = self.control.take() else {
            return;
        };
        match tokio::runtime::Handle::try_current() {
            Ok(rt) => {
                rt.spawn(async move { c.release_online().await });
            }
            Err(_) => tracing::warn!("a download window hold was dropped outside the runtime"),
        }
    }
}

pub struct Supervisor {
    dirs: Dirs,
    client: Client,
    admin: Admin,
    state: watch::Receiver<RuntimeState>,
    shared: Arc<Shared>,
    cmd: mpsc::Sender<Cmd>,
    task: tokio::task::JoinHandle<()>,
}

struct Running {
    child: Box<dyn ChildWrapper>,
    stdin: Option<ChildStdin>,
    pgid: u32,
    /// Reads the engine's output into its log; ends with the process.
    _log: Option<tokio::task::JoinHandle<()>>,
    /// The engine's ports, which no address for Online answers may name.
    _ports: Vec<online::OwnPort>,
}

struct Actor {
    dirs: Dirs,
    exe: std::path::PathBuf,
    opts: SupervisorOptions,
    client: Client,
    admin: Admin,
    state: watch::Sender<RuntimeState>,
    shared: Arc<Shared>,
}

impl Supervisor {
    /// Starts the engine and returns once it answers. Fails when it cannot be started.
    pub async fn start(dirs: Dirs, opts: SupervisorOptions) -> Result<Supervisor, RuntimeError> {
        Self::start_with(dirs, opts, None).await
    }

    /// Like [`Supervisor::start`], with the route for Online answers the first launch uses.
    pub async fn start_with(
        dirs: Dirs,
        opts: SupervisorOptions,
        cloud: Option<CloudRoute>,
    ) -> Result<Supervisor, RuntimeError> {
        let exe = bundle::locate(&dirs.bundle)?;
        dirs.ensure()?;
        let client = Client::detached(Timeouts::default());
        let admin = Admin::detached();
        let (state_tx, state_rx) = watch::channel(RuntimeState::Starting);
        let shared = Arc::new(Shared {
            restarts: AtomicU32::new(0),
            pid: AtomicU32::new(0),
            online: AtomicBool::new(false),
            ready_since: Mutex::new(None),
            cloud: Mutex::new(cloud),
            route_taken: AtomicBool::new(false),
            route_gen: AtomicU64::new(0),
            idle_waits: Mutex::new((RELAUNCH_IDLE_WAIT_OFF, RELAUNCH_IDLE_WAIT_ON)),
            holds: tokio::sync::Mutex::new(0),
        });
        let (cmd_tx, cmd_rx) = mpsc::channel(4);
        let (first_tx, first_rx) = oneshot::channel();
        let actor = Actor {
            dirs: dirs.clone(),
            exe,
            opts,
            client: client.clone(),
            admin: admin.clone(),
            state: state_tx,
            shared: shared.clone(),
        };
        let task = tokio::spawn(actor.run(cmd_rx, first_tx));
        match first_rx.await {
            Ok(Ok(())) => Ok(Supervisor {
                dirs,
                client,
                admin,
                state: state_rx,
                shared,
                cmd: cmd_tx,
                task,
            }),
            Ok(Err(e)) => {
                let _ = task.await;
                Err(e)
            }
            Err(_) => Err(RuntimeError::Start("the supervisor stopped".into())),
        }
    }

    /// A cheap clone that follows restarts.
    pub fn client(&self) -> Client {
        self.client.clone()
    }

    /// The engine's `/internal/*` endpoints, with the admin key the client never holds.
    pub fn admin(&self) -> Admin {
        self.admin.clone()
    }

    pub fn state(&self) -> watch::Receiver<RuntimeState> {
        self.state.clone()
    }

    pub fn restarts(&self) -> u32 {
        self.shared.restarts.load(Ordering::SeqCst)
    }

    /// The process id of the running engine, which is also its process group.
    pub fn pid(&self) -> Option<u32> {
        match self.shared.pid.load(Ordering::SeqCst) {
            0 => None,
            pid => Some(pid),
        }
    }

    /// How long the current engine process has been answering.
    pub fn uptime(&self) -> Option<Duration> {
        self.shared
            .ready_since
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .map(|t| t.elapsed())
    }

    /// See [`Control::set_online`].
    pub async fn set_online(&self, online: bool) -> Result<(), RuntimeError> {
        self.control().set_online(online).await
    }

    /// A handle for the parts of the supervisor that other components need.
    pub fn control(&self) -> Control {
        Control {
            dirs: self.dirs.clone(),
            client: self.client.clone(),
            admin: self.admin.clone(),
            shared: self.shared.clone(),
            cmd: self.cmd.clone(),
        }
    }

    pub fn dirs(&self) -> &Dirs {
        &self.dirs
    }

    /// Stops the engine and everything it started. Asks politely, then insists.
    pub async fn shutdown(self) -> Result<(), RuntimeError> {
        self.stop(Stop::patient()).await
    }

    /// Stops the engine for a quitting app: asks, waits up to `budget`, then sends the engine's
    /// whole process group SIGTERM and, half a second later, SIGKILL.
    pub async fn shutdown_within(self, budget: Duration) -> Result<(), RuntimeError> {
        self.stop(Stop::quick(budget)).await
    }

    async fn stop(self, how: Stop) -> Result<(), RuntimeError> {
        let (tx, rx) = oneshot::channel();
        if self.cmd.send(Cmd::Shutdown(tx, how)).await.is_err() {
            return Ok(());
        }
        let result = rx.await.unwrap_or(Ok(()));
        let _ = self.task.await;
        result
    }
}

fn random_bytes<const N: usize>() -> Result<[u8; N], RuntimeError> {
    let mut buf = [0u8; N];
    getrandom::fill(&mut buf).map_err(|e| RuntimeError::Start(format!("no randomness: {e}")))?;
    Ok(buf)
}

fn new_key() -> Result<String, RuntimeError> {
    Ok(random_bytes::<32>()?
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

/// How long the name of a provider on the person's own network may take to look up at launch.
const LOOKUP_WAIT: Duration = Duration::from_secs(5);

/// Whether a loopback port can be taken. The probe only binds: it never listens, so the app is
/// never, even for an instant, a listener of its own (the network check reads any listen by the
/// app as a break of "off means off").
fn port_is_free(port: u16) -> bool {
    tokio::net::TcpSocket::new_v4()
        .and_then(|socket| socket.bind(std::net::SocketAddr::from(([127, 0, 0, 1], port))))
        .is_ok()
}

/// A random loopback port nothing is listening on right now.
fn free_port(avoid: &[Option<u16>]) -> Result<u16, RuntimeError> {
    for _ in 0..50 {
        let [a, b] = random_bytes::<2>()?;
        let port = 20000 + u16::from_le_bytes([a, b]) % 40000;
        if avoid.contains(&Some(port)) {
            continue;
        }
        if port_is_free(port) {
            return Ok(port);
        }
    }
    Err(RuntimeError::NoPort(50))
}

/// Turns core dumps off for this process, and so for the engine and its children, which inherit
/// the limit: a dump of the app's or the engine's memory could hold the Online answers key. A
/// crash handler the system pipes crashes to (systemd-coredump) still records the process and
/// its environment whatever the limit, which is why the key is never in the engine's
/// environment. On Linux the app is also made undumpable, so other processes of the same user
/// cannot read its memory or environment through `/proc`. Called once at app start, before anything reads the key. It is set on the whole app
/// because the workspace forbids the `unsafe` a per-spawn `pre_exec` hook needs.
#[cfg(unix)]
pub fn harden_process() -> Result<(), RuntimeError> {
    use nix::sys::resource::{Resource, setrlimit};
    setrlimit(Resource::RLIMIT_CORE, 0, 0)
        .map_err(|e| RuntimeError::Start(format!("could not turn core dumps off: {e}")))?;
    #[cfg(target_os = "linux")]
    nix::sys::prctl::set_dumpable(false)
        .map_err(|e| RuntimeError::Start(format!("could not make the app undumpable: {e}")))?;
    Ok(())
}

/// Windows writes no core files by default; nothing to do there.
#[cfg(not(unix))]
pub fn harden_process() -> Result<(), RuntimeError> {
    Ok(())
}

fn read_backends(config_dir: &Path) -> (Option<String>, Option<String>) {
    let record = crate::record::read(config_dir);
    match record {
        Some(r) => (
            r.backends.get("llamacpp").cloned(),
            r.backends.get("whispercpp").cloned(),
        ),
        None => (None, None),
    }
}

impl Actor {
    async fn run(
        self,
        mut cmd: mpsc::Receiver<Cmd>,
        first: oneshot::Sender<Result<(), RuntimeError>>,
    ) {
        let mut first = Some(first);
        let mut crashes: VecDeque<Instant> = VecDeque::new();
        let mut attempt = 0u32;
        // The relaunches the launch under way answers.
        let mut waiting: Vec<Relaunch> = Vec::new();
        loop {
            // Relaunches asked for while the engine starts. The launch may have read the
            // settings before they changed, so it starts again with the latest ones, and every
            // relaunch is answered once that one is up: the last setting wins, nothing stops.
            let mut later: Vec<Relaunch> = Vec::new();
            let launched = {
                let launch = self.launch();
                tokio::pin!(launch);
                loop {
                    tokio::select! {
                        r = &mut launch => break r,
                        c = cmd.recv() => match c {
                            Some(Cmd::Relaunch(r)) => later.push(r),
                            Some(Cmd::Shutdown(reply, _)) => {
                                self.give_up(waiting, later);
                                let _ = reply.send(Ok(()));
                                self.set_state(RuntimeState::Stopped);
                                return;
                            }
                            None => {
                                self.give_up(waiting, later);
                                self.set_state(RuntimeState::Stopped);
                                return;
                            }
                        },
                    }
                }
            };
            match launched {
                Ok(mut running) => {
                    if let Some(f) = first.take() {
                        let _ = f.send(Ok(()));
                    }
                    if !later.is_empty() {
                        let _ = self.stop_gracefully(&mut running, Stop::patient()).await;
                        self.forget_endpoints();
                        self.clear_ready();
                        self.set_state(RuntimeState::Starting);
                        waiting.append(&mut later);
                        continue;
                    }
                    for r in waiting.drain(..) {
                        r.succeeded();
                    }
                    match self.supervise(running, &mut cmd).await {
                        Exit::Shutdown => return,
                        Exit::Relaunch(reply) => {
                            waiting.push(reply);
                            continue;
                        }
                        Exit::Crashed(why) => {
                            tracing::warn!("engine stopped unexpectedly: {why}");
                        }
                    }
                }
                Err(e) => {
                    for r in waiting.drain(..) {
                        r.failed_after(&self.shared, e.clone(), &later);
                    }
                    waiting = later;
                    if let Some(f) = first.take() {
                        self.give_up(std::mem::take(&mut waiting), Vec::new());
                        self.set_state(RuntimeState::Stopped);
                        let _ = f.send(Err(e));
                        return;
                    }
                    tracing::warn!("engine did not restart: {e}");
                }
            }
            let now = Instant::now();
            crashes.retain(|t| now.duration_since(*t) < RESTART_WINDOW);
            if crashes.len() as u32 >= self.opts.restart_limit {
                self.give_up(std::mem::take(&mut waiting), Vec::new());
                self.fail(
                    format!(
                        "the engine stopped {} times within an hour",
                        crashes.len() + 1
                    ),
                    &mut cmd,
                )
                .await;
                return;
            }
            crashes.push_back(now);
            attempt += 1;
            self.shared.restarts.fetch_add(1, Ordering::SeqCst);
            self.set_state(RuntimeState::Restarting { attempt });
            let delay = self.opts.backoff_base * 2u32.pow(crashes.len() as u32 - 1);
            tokio::select! {
                _ = tokio::time::sleep(delay) => {}
                c = cmd.recv() => {
                    match c {
                        Some(Cmd::Relaunch(reply)) => waiting.push(reply),
                        Some(Cmd::Shutdown(reply, _)) => {
                            self.give_up(std::mem::take(&mut waiting), Vec::new());
                            let _ = reply.send(Ok(()));
                            self.set_state(RuntimeState::Stopped);
                            return;
                        }
                        None => {
                            self.give_up(std::mem::take(&mut waiting), Vec::new());
                            self.set_state(RuntimeState::Stopped);
                            return;
                        }
                    }
                }
            }
        }
    }

    /// Answers relaunches that will not happen, putting back what each would have changed.
    fn give_up(&self, waiting: Vec<Relaunch>, later: Vec<Relaunch>) {
        for r in waiting.into_iter().chain(later) {
            r.failed(&self.shared, RuntimeError::Unavailable);
        }
    }

    /// Reports the failure and waits to be told to shut down.
    async fn fail(&self, reason: String, cmd: &mut mpsc::Receiver<Cmd>) {
        self.forget_endpoints();
        self.clear_ready();
        self.set_state(RuntimeState::Failed { reason });
        while let Some(c) = cmd.recv().await {
            match c {
                Cmd::Shutdown(reply, _) => {
                    let _ = reply.send(Ok(()));
                    return;
                }
                Cmd::Relaunch(r) => r.failed(&self.shared, RuntimeError::Unavailable),
            }
        }
    }

    fn set_state(&self, state: RuntimeState) {
        let _ = self.state.send(state);
    }

    fn clear_ready(&self) {
        self.shared.pid.store(0, Ordering::SeqCst);
        self.shared.route_taken.store(false, Ordering::SeqCst);
        *self
            .shared
            .ready_since
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = None;
    }

    /// Spawns the engine, retrying with new ports, and waits until it answers with our key.
    async fn launch(&self) -> Result<Running, RuntimeError> {
        self.forget_endpoints();
        self.clear_ready();
        if !matches!(*self.state.borrow(), RuntimeState::Restarting { .. }) {
            self.set_state(RuntimeState::Starting);
        }
        let mut last = String::new();
        for _ in 0..PORT_TRIES {
            // Noted before the window and the route are read, so any change after this point
            // stops the key from being handed over.
            let read_at = self.shared.route_gen.load(Ordering::SeqCst);
            let online = self.shared.online.load(Ordering::SeqCst);
            // A download-window launch never carries the route: no entry, no row, no key.
            let route: Option<CloudRoute> = if online {
                None
            } else {
                self.shared
                    .cloud
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .clone()
            };
            // The engine never takes the port of a provider on this computer.
            let provider = route.as_ref().and_then(CloudRoute::loopback_port);
            let port = free_port(&[provider])?;
            let ws_port = free_port(&[provider, Some(port)])?;
            let keys = Keys::new(new_key()?, new_key()?)?;
            let (llama, whisper) = read_backends(&self.dirs.config());
            let installed = crate::record::read(&self.dirs.config()).and_then(|r| r.working_tier());
            config::write(
                &self.dirs.config(),
                &config::Settings {
                    models_dir: &self.dirs.models(),
                    offline: !online,
                    no_fetch_executables: !online,
                    websocket_port: ws_port,
                    llamacpp_backend: llama.as_deref(),
                    whispercpp_backend: whisper.as_deref(),
                    llamacpp_args: installed.map(config::llamacpp_args).unwrap_or(""),
                    cloud: route.as_ref().map(online::entry),
                },
            )?;
            online::write_row(&self.dirs.config(), route.as_ref())?;
            let saved = residency::saved_options_for(&selection::read(&self.dirs.config()));
            let saved: Vec<(&str, _)> =
                saved.iter().map(|(m, o)| (m.as_str(), o.clone())).collect();
            // Without them the engine still runs, only with its own sizes and timeouts.
            if let Err(e) = config::write_model_options(&self.dirs.config(), &saved) {
                tracing::warn!("could not save the models' load options: {e}");
            }
            self.client.set_saved_options(saved);
            let mut running = self.spawn(port, &keys, route.as_ref().map(|r| &r.key))?;
            running._ports = vec![online::OwnPort::hold(port), online::OwnPort::hold(ws_port)];
            match self
                .await_ready(&mut running, port, ws_port, &keys.api)
                .await
            {
                Ok(version) => {
                    // Before the endpoints are known to anyone, so nothing is sent with a route
                    // that is half given.
                    if let Some(r) = &route {
                        let taken = self.give_route(port, &keys, r, read_at).await;
                        self.shared.route_taken.store(taken, Ordering::SeqCst);
                    }
                    self.client.set_endpoint(Some(Endpoint {
                        port,
                        key: keys.api,
                    }));
                    if let Err(e) = self.admin.set_endpoint(Some((port, keys.admin))) {
                        stop_hard(&mut running).await;
                        return Err(e);
                    }
                    *self
                        .shared
                        .ready_since
                        .lock()
                        .unwrap_or_else(|e| e.into_inner()) = Some(Instant::now());
                    self.shared.pid.store(running.pgid, Ordering::SeqCst);
                    self.set_state(RuntimeState::Ready { port, version });
                    return Ok(running);
                }
                Err(Launch::Retry(why)) => {
                    last = why;
                    stop_hard(&mut running).await;
                }
                Err(Launch::Fatal(e)) => {
                    stop_hard(&mut running).await;
                    return Err(e);
                }
            }
        }
        Err(RuntimeError::Start(format!(
            "gave up after {PORT_TRIES} tries: {last}"
        )))
    }

    /// Hands the new engine the route's key; on any failure it is taken back, and the engine
    /// runs without the route. True when the engine holds it.
    /// A route read at generation `read_at` is handed over only while the generation stays there.
    async fn give_route(&self, port: u16, keys: &Keys, route: &CloudRoute, read_at: u64) -> bool {
        let Ok(admin) = Admin::new(port, keys.admin.clone()) else {
            return false;
        };
        // A name on the person's own network is judged again by what it resolves to now.
        let lookup = online::system_lookup();
        match tokio::time::timeout(
            LOOKUP_WAIT,
            online::resolved_reach(route.base_url(), &lookup),
        )
        .await
        {
            Ok(Ok(_)) => {}
            Ok(Err(_)) => {
                tracing::warn!(
                    "the address for Online answers no longer points at the person's own network; the route is not taken"
                );
                return false;
            }
            Err(_) => {
                tracing::warn!("the address for Online answers could not be looked up in time");
                return false;
            }
        }
        let shared = self.shared.clone();
        let still = move || shared.route_gen.load(Ordering::SeqCst) == read_at;
        match online::give(&admin, route, &still).await {
            Ok(Some(listed)) => {
                tracing::info!(
                    "the engine took the route for Online answers; the provider listed {listed} models"
                );
                true
            }
            Ok(None) => {
                tracing::info!("the route for Online answers changed while the engine started");
                online::take_back(&admin).await;
                false
            }
            Err(e) => {
                tracing::warn!(
                    "the engine did not take the route for Online answers: {}",
                    online::kind_of(&e)
                );
                online::take_back(&admin).await;
                false
            }
        }
    }

    /// Neither key works against a stopped engine.
    fn forget_endpoints(&self) {
        self.client.set_endpoint(None);
        let _ = self.admin.set_endpoint(None);
    }

    /// Starts the engine. Its log goes to standard output and its errors to standard error,
    /// which the log pipe redacts into `lemond.log`, hiding `key` (the route's, if there is
    /// one). The key is never in the engine's environment: it is handed over after the engine
    /// answers (see `give_route`).
    fn spawn(
        &self,
        port: u16,
        keys: &Keys,
        key: Option<&OnlineKey>,
    ) -> Result<Running, RuntimeError> {
        let mut cmd = Command::new(&self.exe);
        cmd.arg(self.dirs.cache())
            .arg(self.dirs.config())
            .args(["--port", &port.to_string()])
            .args(["--host", "127.0.0.1", "--no-broadcast"]);
        #[cfg(unix)]
        cmd.args(["--watchdog-fd", "0"]);
        // Windows has no clean-up of its own for the engine's temporary files.
        #[cfg(windows)]
        let temp = self.dirs.launch_temp()?;
        #[cfg(not(windows))]
        let temp = self.dirs.run();
        cmd.env_clear();
        for (name, value) in spawn_env(&self.dirs, keys, &temp) {
            cmd.env(name, value);
        }
        // A redactor of its own for each launch and output: the key it hides is this launch's.
        let redactor = || {
            let r = Arc::new(Redactor::new());
            r.set_key(key);
            r
        };
        let (redact, errors) = (redactor(), redactor());
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut wrap = CommandWrap::from(cmd);
        #[cfg(unix)]
        wrap.wrap(process_wrap::tokio::ProcessGroup::leader());
        #[cfg(windows)]
        wrap.wrap(process_wrap::tokio::JobObject);
        wrap.wrap(KillOnDrop);
        let mut child = wrap
            .spawn()
            .map_err(|e| RuntimeError::Start(format!("could not run lemond: {e}")))?;
        let pgid = child.id().unwrap_or(0);
        let stdin = child.stdin().take();
        let log = match (child.stdout().take(), child.stderr().take()) {
            (Some(out), Some(err)) => Some(logpipe::spawn_both(
                out,
                err,
                self.dirs.logs().join("lemond.log"),
                redact,
                errors,
            )),
            _ => None,
        };
        Ok(Running {
            child,
            stdin,
            pgid,
            _log: log,
            _ports: Vec::new(),
        })
    }

    async fn await_ready(
        &self,
        running: &mut Running,
        port: u16,
        ws_port: u16,
        key: &str,
    ) -> Result<String, Launch> {
        let probe = Client::new(port, key);
        let deadline = Instant::now() + self.opts.start_timeout;
        loop {
            if let Ok(Some(status)) = running.child.try_wait() {
                return Err(Launch::Retry(format!("exited early with {status}")));
            }
            if probe.live().await.is_ok() {
                // The wrong key is tried first and the right one last. A busy or half-started
                // engine can fail the first for other reasons, which only means try again.
                let wrong = Client::new(port, "not-the-key");
                match wrong.health().await {
                    Err(RuntimeError::Unauthorized) => match probe.health().await {
                        Ok(h) => {
                            if h.websocket_port != Some(ws_port) {
                                return Err(Launch::Retry(format!(
                                    "the engine reports websocket port {:?}, not {ws_port}",
                                    h.websocket_port
                                )));
                            }
                            return Ok(h.version);
                        }
                        Err(RuntimeError::Unauthorized) => {
                            return Err(Launch::Retry("something else holds the port".into()));
                        }
                        Err(_) => {}
                    },
                    Ok(_) => {
                        return Err(Launch::Retry("the engine does not enforce its key".into()));
                    }
                    Err(_) => {}
                }
            }
            if Instant::now() >= deadline {
                return Err(Launch::Fatal(RuntimeError::Start(
                    "the engine did not answer in time".into(),
                )));
            }
            tokio::time::sleep(LIVE_POLL).await;
        }
    }

    async fn supervise(&self, mut running: Running, cmd: &mut mpsc::Receiver<Cmd>) -> Exit {
        let mut misses = 0u32;
        let mut next_check = Instant::now() + self.opts.health_interval;
        loop {
            let wait = next_check.saturating_duration_since(Instant::now());
            tokio::select! {
                status = running.child.wait() => {
                    reap_group(&mut running).await;
                    self.forget_endpoints();
                    self.clear_ready();
                    return Exit::Crashed(match status {
                        Ok(s) => s.to_string(),
                        Err(e) => e.to_string(),
                    });
                }
                _ = tokio::time::sleep(wait) => {
                    next_check = Instant::now() + self.opts.health_interval;
                    match tokio::time::timeout(Duration::from_secs(2), self.client.live()).await {
                        Ok(Ok(())) => misses = 0,
                        _ => misses += 1,
                    }
                    if misses >= HUNG_AFTER {
                        stop_hard(&mut running).await;
                        self.forget_endpoints();
                        self.clear_ready();
                        return Exit::Crashed("the engine stopped answering".into());
                    }
                }
                c = cmd.recv() => {
                    let exit = match c {
                        Some(Cmd::Shutdown(reply, how)) => {
                            let r = self.stop_gracefully(&mut running, how).await;
                            let _ = reply.send(r);
                            self.set_state(RuntimeState::Stopped);
                            Exit::Shutdown
                        }
                        Some(Cmd::Relaunch(reply)) => {
                            let _ = self.stop_gracefully(&mut running, Stop::patient()).await;
                            self.set_state(RuntimeState::Starting);
                            Exit::Relaunch(reply)
                        }
                        None => {
                            stop_hard(&mut running).await;
                            self.set_state(RuntimeState::Stopped);
                            Exit::Shutdown
                        }
                    };
                    self.forget_endpoints();
                    self.clear_ready();
                    return exit;
                }
            }
        }
    }

    /// The shutdown request, then SIGTERM to the group, then SIGKILL, each stage with its own
    /// short limit.
    async fn stop_gracefully(&self, running: &mut Running, how: Stop) -> Result<(), RuntimeError> {
        let begun = tokio::time::Instant::now();
        let request = SHUTDOWN_REQUEST.min(how.polite);
        let _ = tokio::time::timeout(request, self.admin.shutdown()).await;
        let rest = how.polite.saturating_sub(begun.elapsed());
        if wait_exit(running, rest).await.is_none() {
            drop(running.stdin.take());
            #[cfg(unix)]
            let _ = running.child.signal(15);
            if wait_exit(running, how.after_term).await.is_none() {
                let _ = running.child.start_kill();
                wait_exit(running, AFTER_KILL).await;
            }
        }
        drop(running.stdin.take());
        reap_group(running).await;
        let left = group_members(running.pgid);
        if left.is_empty() {
            Ok(())
        } else {
            Err(RuntimeError::Start(format!(
                "processes outlived the engine: {left:?}"
            )))
        }
    }
}

enum Exit {
    Shutdown,
    Relaunch(Relaunch),
    Crashed(String),
}

enum Launch {
    Retry(String),
    Fatal(RuntimeError),
}

/// The whole environment the engine starts with: a home, a path, its runtime folder, a locale
/// and the launch's two keys, and on Windows its profile and temporary folders. Nothing else, no
/// provider key and no proxy.
#[cfg_attr(not(windows), allow(unused_variables))]
fn spawn_env(dirs: &Dirs, keys: &Keys, temp: &Path) -> Vec<(&'static str, std::ffi::OsString)> {
    let env: Vec<(&'static str, std::ffi::OsString)> = vec![
        ("HOME", dirs.root.clone().into_os_string()),
        ("PATH", default_path().into()),
        ("XDG_RUNTIME_DIR", dirs.run().into_os_string()),
        ("LANG", "C.UTF-8".into()),
        ("LEMONADE_API_KEY", keys.api.clone().into()),
        ("LEMONADE_ADMIN_API_KEY", keys.admin.clone().into()),
    ];
    // On Windows the engine reads its home from USERPROFILE, and stops at once without it. Its
    // temporary files go to `temp`, a folder of their own under its runtime folder.
    #[cfg(windows)]
    let env = {
        let mut env = env;
        env.push(("USERPROFILE", dirs.root.clone().into_os_string()));
        env.push(("TMP", temp.into()));
        env.push(("TEMP", temp.into()));
        if let Some(root) = std::env::var_os("SystemRoot") {
            env.push(("SystemRoot", root));
        }
        env
    };
    env
}

fn default_path() -> &'static str {
    if cfg!(windows) {
        "C:\\Windows\\System32"
    } else {
        "/usr/bin:/bin"
    }
}

async fn wait_exit(running: &mut Running, within: Duration) -> Option<()> {
    tokio::time::timeout(within, running.child.wait())
        .await
        .ok()
        .map(|_| ())
}

/// Kills the whole group, then makes sure nothing in it is left.
async fn stop_hard(running: &mut Running) {
    drop(running.stdin.take());
    let _ = running.child.start_kill();
    let _ = tokio::time::timeout(AFTER_KILL, running.child.wait()).await;
    reap_group(running).await;
}

/// After the leader is gone, backends it started may linger: end them.
async fn reap_group(running: &mut Running) {
    for _ in 0..40 {
        if group_members(running.pgid).is_empty() {
            return;
        }
        #[cfg(unix)]
        {
            let _ = running.child.signal(9);
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Processes still in the engine's process group: `/proc` on Linux, `ps` on macOS. Elsewhere the
/// job object or the watchdog does this work and nothing is reported.
#[cfg(target_os = "linux")]
pub fn group_members(pgid: u32) -> Vec<u32> {
    let mut out = Vec::new();
    if pgid == 0 {
        return out;
    }
    let Ok(dir) = std::fs::read_dir("/proc") else {
        return out;
    };
    for entry in dir.flatten() {
        let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
            continue;
        };
        let Ok(stat) = std::fs::read_to_string(entry.path().join("stat")) else {
            continue;
        };
        let Some((_, rest)) = stat.rsplit_once(')') else {
            continue;
        };
        let fields: Vec<&str> = rest.split_whitespace().collect();
        let alive = fields.first().is_some_and(|s| *s != "Z");
        if alive && fields.get(2).and_then(|g| g.parse::<u32>().ok()) == Some(pgid) {
            out.push(pid);
        }
    }
    out
}

#[cfg(target_os = "macos")]
pub fn group_members(pgid: u32) -> Vec<u32> {
    if pgid == 0 {
        return Vec::new();
    }
    let listing = std::process::Command::new("/bin/ps")
        .args(["-ax", "-o", "pid=,pgid=,stat="])
        .env_clear()
        .env("LC_ALL", "C")
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output();
    match listing {
        Ok(out) if out.status.success() => {
            parse_ps_group(&String::from_utf8_lossy(&out.stdout), pgid)
        }
        _ => Vec::new(),
    }
}

/// The live processes of group `pgid` in `ps -ax -o pid=,pgid=,stat=` output. A zombie (state
/// `Z`) has already ended and only waits to be reaped, so it does not count.
#[cfg(any(target_os = "macos", test))]
fn parse_ps_group(listing: &str, pgid: u32) -> Vec<u32> {
    listing
        .lines()
        .filter_map(|line| {
            let mut cols = line.split_whitespace();
            let pid = cols.next()?.parse::<u32>().ok()?;
            let group = cols.next()?.parse::<u32>().ok()?;
            let state = cols.next()?;
            (group == pgid && !state.starts_with('Z')).then_some(pid)
        })
        .collect()
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn group_members(_pgid: u32) -> Vec<u32> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_taken_port_is_not_free_and_a_probe_takes_nothing() {
        let held = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let taken = held.local_addr().unwrap().port();
        assert!(!port_is_free(taken));
        drop(held);
        assert!(port_is_free(taken));
        // The probe let go: the port can be listened on straight after.
        assert!(std::net::TcpListener::bind(("127.0.0.1", taken)).is_ok());
    }

    #[test]
    fn free_port_gives_a_port_that_can_be_listened_on() {
        let port = free_port(&[]).unwrap();
        assert!(std::net::TcpListener::bind(("127.0.0.1", port)).is_ok());
        assert_ne!(free_port(&[Some(port)]).unwrap(), port);
    }

    #[test]
    fn the_spawn_environment_holds_no_provider_key() {
        let dirs = Dirs::new("/bundle", "/root");
        let keys = Keys::new("api-key".into(), "admin-key".into()).unwrap();
        let env = spawn_env(&dirs, &keys, &dirs.run());
        let names: Vec<&str> = env.iter().map(|(n, _)| *n).collect();
        let keys_given: Vec<&str> = names
            .iter()
            .copied()
            .filter(|n| n.ends_with("_API_KEY"))
            .collect();
        assert_eq!(keys_given, ["LEMONADE_API_KEY", "LEMONADE_ADMIN_API_KEY"]);
        assert!(!names.contains(&online::KEY_ENV));
        assert!(
            names
                .iter()
                .all(|n| !n.to_ascii_lowercase().contains("proxy")),
            "{names:?}"
        );
        #[cfg(not(windows))]
        assert_eq!(
            names,
            [
                "HOME",
                "PATH",
                "XDG_RUNTIME_DIR",
                "LANG",
                "LEMONADE_API_KEY",
                "LEMONADE_ADMIN_API_KEY"
            ]
        );
    }

    /// The engine on Windows resolves its default folders from USERPROFILE while it reads its
    /// arguments, and exits with 1 when it is not set, even though the folders are given. Its
    /// temporary files go to the folder it is given.
    #[cfg(windows)]
    #[test]
    fn the_engine_on_windows_has_its_profile_and_temp_inside_its_root() {
        let dirs = Dirs::new(r"C:\bundle", r"C:\root");
        let keys = Keys::new("api-key".into(), "admin-key".into()).unwrap();
        let env = spawn_env(&dirs, &keys, &dirs.run());
        let given = |name: &str| env.iter().find(|(n, _)| *n == name).map(|(_, v)| v.clone());
        assert_eq!(
            given("USERPROFILE"),
            Some(dirs.root.clone().into_os_string())
        );
        assert_eq!(given("TMP"), Some(dirs.run().into_os_string()));
        assert_eq!(given("TEMP"), Some(dirs.run().into_os_string()));
    }

    #[test]
    fn ps_output_yields_the_live_members_of_one_group() {
        let listing = "\
    1     1 Ss
  612   612 Ss
  613   612 S
  614   612 Z
  700   700 S+
  615   612 U
";
        assert_eq!(parse_ps_group(listing, 612), vec![612, 613, 615]);
        assert!(parse_ps_group(listing, 999).is_empty());
    }

    #[test]
    fn unreadable_ps_lines_are_skipped() {
        assert!(parse_ps_group("", 1).is_empty());
        assert!(parse_ps_group("pid pgid stat\nabc 1 S\n 5 x S\n 6 1\n", 1).is_empty());
    }
}
