//! Owns one private `lemond`: starts it with a scrubbed environment and a fresh key, watches
//! it, restarts it with backoff, and stops it without leaving a process behind.

use std::collections::VecDeque;
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use process_wrap::tokio::{ChildWrapper, CommandWrap, KillOnDrop};
use tokio::process::{ChildStdin, Command};
use tokio::sync::{mpsc, oneshot, watch};

use crate::bundle::{self, Dirs};
use crate::client::{Client, Endpoint, Timeouts};
use crate::config;
use crate::error::RuntimeError;
use crate::residency;

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
    Relaunch(oneshot::Sender<Result<(), RuntimeError>>),
}

struct Shared {
    restarts: AtomicU32,
    pid: AtomicU32,
    online: AtomicBool,
    ready_since: Mutex<Option<Instant>>,
}

/// A cheap, cloneable handle on a running supervisor's settings.
#[derive(Clone)]
pub struct Control {
    dirs: Dirs,
    client: Client,
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

    /// Lets the engine reach the internet (for a download the person approved) or not, and
    /// fetch executables or not. The engine reads both only when it starts, so a change
    /// relaunches it and this returns once the new one answers. The choice outlives a restart.
    pub async fn set_online(&self, online: bool) -> Result<(), RuntimeError> {
        let before = self.shared.online.swap(online, Ordering::SeqCst);
        if before == online {
            return Ok(());
        }
        let (tx, rx) = oneshot::channel();
        self.cmd
            .send(Cmd::Relaunch(tx))
            .await
            .map_err(|_| RuntimeError::Unavailable)?;
        rx.await.map_err(|_| RuntimeError::Unavailable)?
    }
}

pub struct Supervisor {
    dirs: Dirs,
    client: Client,
    state: watch::Receiver<RuntimeState>,
    shared: Arc<Shared>,
    cmd: mpsc::Sender<Cmd>,
    task: tokio::task::JoinHandle<()>,
}

struct Running {
    child: Box<dyn ChildWrapper>,
    stdin: Option<ChildStdin>,
    pgid: u32,
}

struct Actor {
    dirs: Dirs,
    exe: std::path::PathBuf,
    opts: SupervisorOptions,
    client: Client,
    state: watch::Sender<RuntimeState>,
    shared: Arc<Shared>,
}

impl Supervisor {
    /// Starts the engine and returns once it answers. Fails when it cannot be started.
    pub async fn start(dirs: Dirs, opts: SupervisorOptions) -> Result<Supervisor, RuntimeError> {
        let exe = bundle::locate(&dirs.bundle)?;
        dirs.ensure()?;
        let client = Client::detached(Timeouts::default());
        let (state_tx, state_rx) = watch::channel(RuntimeState::Starting);
        let shared = Arc::new(Shared {
            restarts: AtomicU32::new(0),
            pid: AtomicU32::new(0),
            online: AtomicBool::new(false),
            ready_since: Mutex::new(None),
        });
        let (cmd_tx, cmd_rx) = mpsc::channel(4);
        let (first_tx, first_rx) = oneshot::channel();
        let actor = Actor {
            dirs: dirs.clone(),
            exe,
            opts,
            client: client.clone(),
            state: state_tx,
            shared: shared.clone(),
        };
        let task = tokio::spawn(actor.run(cmd_rx, first_tx));
        match first_rx.await {
            Ok(Ok(())) => Ok(Supervisor {
                dirs,
                client,
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

/// A random loopback port nothing is listening on right now.
fn free_port(avoid: Option<u16>) -> Result<u16, RuntimeError> {
    for _ in 0..50 {
        let [a, b] = random_bytes::<2>()?;
        let port = 20000 + u16::from_le_bytes([a, b]) % 40000;
        if Some(port) == avoid {
            continue;
        }
        if std::net::TcpListener::bind(("127.0.0.1", port)).is_ok() {
            return Ok(port);
        }
    }
    Err(RuntimeError::NoPort(50))
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
        let mut relaunch: Option<oneshot::Sender<Result<(), RuntimeError>>> = None;
        loop {
            let launched = tokio::select! {
                r = self.launch() => r,
                c = cmd.recv() => {
                    match c {
                        Some(Cmd::Relaunch(reply)) => {
                            let _ = reply.send(Err(RuntimeError::Unavailable));
                        }
                        Some(Cmd::Shutdown(reply, _)) => {
                            let _ = reply.send(Ok(()));
                        }
                        None => {}
                    }
                    self.set_state(RuntimeState::Stopped);
                    return;
                }
            };
            match launched {
                Ok(running) => {
                    if let Some(f) = first.take() {
                        let _ = f.send(Ok(()));
                    }
                    if let Some(reply) = relaunch.take() {
                        let _ = reply.send(Ok(()));
                    }
                    match self.supervise(running, &mut cmd).await {
                        Exit::Shutdown => return,
                        Exit::Relaunch(reply) => {
                            relaunch = Some(reply);
                            continue;
                        }
                        Exit::Crashed(why) => {
                            tracing::warn!("engine stopped unexpectedly: {why}");
                        }
                    }
                }
                Err(e) => {
                    if let Some(reply) = relaunch.take() {
                        let _ = reply.send(Err(e.clone()));
                    }
                    if let Some(f) = first.take() {
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
                        Some(Cmd::Relaunch(reply)) => relaunch = Some(reply),
                        Some(Cmd::Shutdown(reply, _)) => {
                            let _ = reply.send(Ok(()));
                            self.set_state(RuntimeState::Stopped);
                            return;
                        }
                        None => {
                            self.set_state(RuntimeState::Stopped);
                            return;
                        }
                    }
                }
            }
        }
    }

    /// Reports the failure and waits to be told to shut down.
    async fn fail(&self, reason: String, cmd: &mut mpsc::Receiver<Cmd>) {
        self.client.set_endpoint(None);
        self.clear_ready();
        self.set_state(RuntimeState::Failed { reason });
        while let Some(c) = cmd.recv().await {
            match c {
                Cmd::Shutdown(reply, _) => {
                    let _ = reply.send(Ok(()));
                    return;
                }
                Cmd::Relaunch(reply) => {
                    let _ = reply.send(Err(RuntimeError::Unavailable));
                }
            }
        }
    }

    fn set_state(&self, state: RuntimeState) {
        let _ = self.state.send(state);
    }

    fn clear_ready(&self) {
        self.shared.pid.store(0, Ordering::SeqCst);
        *self
            .shared
            .ready_since
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = None;
    }

    /// Spawns the engine, retrying with new ports, and waits until it answers with our key.
    async fn launch(&self) -> Result<Running, RuntimeError> {
        self.client.set_endpoint(None);
        self.clear_ready();
        if !matches!(*self.state.borrow(), RuntimeState::Restarting { .. }) {
            self.set_state(RuntimeState::Starting);
        }
        let mut last = String::new();
        for _ in 0..PORT_TRIES {
            let port = free_port(None)?;
            let ws_port = free_port(Some(port))?;
            let key = new_key()?;
            let online = self.shared.online.load(Ordering::SeqCst);
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
                },
            )?;
            let saved = residency::saved_options();
            // Without them the engine still runs, only with its own sizes and timeouts.
            if let Err(e) = config::write_model_options(&self.dirs.config(), &saved) {
                tracing::warn!("could not save the models' load options: {e}");
            }
            self.client.set_saved_options(saved);
            let mut running = self.spawn(port, &key)?;
            match self.await_ready(&mut running, port, ws_port, &key).await {
                Ok(version) => {
                    self.client.set_endpoint(Some(Endpoint { port, key }));
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
            "no usable port after {PORT_TRIES} tries: {last}"
        )))
    }

    fn spawn(&self, port: u16, key: &str) -> Result<Running, RuntimeError> {
        let mut cmd = Command::new(&self.exe);
        cmd.arg(self.dirs.cache())
            .arg(self.dirs.config())
            .args(["--port", &port.to_string()])
            .args(["--host", "127.0.0.1", "--no-broadcast"])
            .arg("--log-file")
            .arg(self.dirs.logs().join("lemond.log"));
        #[cfg(unix)]
        cmd.args(["--watchdog-fd", "0"]);
        cmd.env_clear()
            .env("HOME", &self.dirs.root)
            .env("PATH", default_path())
            .env("XDG_RUNTIME_DIR", self.dirs.run())
            .env("LANG", "C.UTF-8")
            .env("LEMONADE_API_KEY", key);
        #[cfg(windows)]
        if let Some(root) = std::env::var_os("SystemRoot") {
            cmd.env("SystemRoot", root);
        }
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
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
        Ok(Running { child, stdin, pgid })
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
                    self.client.set_endpoint(None);
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
                        self.client.set_endpoint(None);
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
                    self.client.set_endpoint(None);
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
        let _ = tokio::time::timeout(request, self.client.shutdown()).await;
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
    Relaunch(oneshot::Sender<Result<(), RuntimeError>>),
    Crashed(String),
}

enum Launch {
    Retry(String),
    Fatal(RuntimeError),
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
