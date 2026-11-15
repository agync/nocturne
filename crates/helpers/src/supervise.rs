//! Run one helper process: its own process group (a job object on Windows), a scrubbed
//! environment, output kept in an owner-only log, restarts with backoff, and a stop that proves
//! nothing is left running. A helper that speaks in lines keeps its input open and has its
//! output read line by line instead of logged.

use std::collections::VecDeque;
use std::ffi::OsString;
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use process_wrap::tokio::ChildWrapper;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::ChildStdin;
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinHandle;
use tokio::time::Instant;

use crate::HelperError;

/// What to run. The environment is exactly `env`: nothing is inherited.
#[derive(Debug, Clone)]
pub struct HelperCommand {
    pub name: &'static str,
    pub program: PathBuf,
    /// Lowercase hex pin of `program`, checked before every start, restarts included.
    pub sha256: String,
    pub args: Vec<OsString>,
    pub env: Vec<(OsString, OsString)>,
    pub cwd: Option<PathBuf>,
}

/// A line written to a helper that must never show in `Debug` output or logs.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(pub String);

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("<hidden>")
    }
}

/// Whether a helper that ended by itself is started again, from its exit code and the last
/// lines it printed.
pub type RestartFn = dyn Fn(Option<i32>, &[String]) -> bool + Send + Sync;

#[derive(Clone)]
pub struct RestartRule(pub Arc<RestartFn>);

impl RestartRule {
    pub fn always() -> Self {
        Self(Arc::new(|_, _| true))
    }
}

impl std::fmt::Debug for RestartRule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RestartRule")
    }
}

/// The longest output line kept; a longer one is dropped.
const MAX_LINE: usize = 8192;
/// How many of a run's last output lines the restart rule sees.
const TAIL: usize = 16;

#[derive(Debug, Clone)]
pub struct SuperviseOptions {
    /// Folder for `helper-<name>.log`.
    pub logs: PathBuf,
    /// Waits before the 1st, 2nd, ... restart; the last repeats.
    pub backoff: Vec<Duration>,
    /// Restarts allowed within `window` before giving up.
    pub max_restarts: u32,
    pub window: Duration,
    /// Time to let a helper end after SIGTERM before it is killed.
    pub grace: Duration,
    /// A log larger than this is rotated: when the helper starts and, while it runs, every
    /// `rotate_every`.
    pub log_limit: u64,
    pub rotate_every: Duration,
    /// Keep the helper's input open and read its output as lines (see [`Supervisor::take_lines`])
    /// instead of logging it. Errors still go to the log.
    pub lines: bool,
    /// Written as the first input line of every start, restarts included.
    pub first_line: Option<Secret>,
    /// Written to ask the helper to stop, `stop_wait` before it is sent SIGTERM.
    pub stop_line: Option<String>,
    pub stop_wait: Duration,
    /// Decides whether a helper that ended by itself is started again.
    pub restart: RestartRule,
}

impl SuperviseOptions {
    pub fn new(logs: PathBuf) -> Self {
        Self {
            logs,
            backoff: [1, 2, 4, 8, 16].map(Duration::from_secs).to_vec(),
            max_restarts: 5,
            window: Duration::from_secs(3600),
            grace: Duration::from_secs(3),
            log_limit: 1 << 20,
            rotate_every: Duration::from_secs(30),
            lines: false,
            first_line: None,
            stop_line: None,
            stop_wait: Duration::from_secs(15),
            restart: RestartRule::always(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Phase {
    Running,
    Backoff {
        attempt: u32,
    },
    GaveUp(String),
    /// Ended by itself with this exit code and, by the restart rule, not started again.
    Ended(Option<i32>),
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupState {
    pub phase: Phase,
    pub pid: Option<u32>,
    /// Counts every start, the first included.
    pub generation: u32,
    pub restarts: u32,
    pub started: Option<Instant>,
    /// Size of the log when the current process started: its own output begins there.
    pub log_offset: u64,
}

struct Running {
    child: Box<dyn ChildWrapper>,
    pgid: u32,
    stdin: Option<ChildStdin>,
    /// Forwards output lines and returns the run's last ones at the end of its output.
    reader: Option<JoinHandle<Vec<String>>>,
}

/// Asks a supervised helper to stop; the stop itself is still waited for by `Supervisor::stop`.
#[derive(Clone)]
pub struct StopHandle(Arc<Mutex<Option<oneshot::Sender<()>>>>);

impl StopHandle {
    pub fn stop(&self) {
        if let Some(tx) = self.0.lock().expect("stop lock").take() {
            let _ = tx.send(());
        }
    }
}

pub struct Supervisor {
    state: watch::Receiver<SupState>,
    log: PathBuf,
    stop: Arc<Mutex<Option<oneshot::Sender<()>>>>,
    task: Option<JoinHandle<Result<(), HelperError>>>,
    input: mpsc::UnboundedSender<String>,
    lines: Option<mpsc::UnboundedReceiver<String>>,
}

impl Supervisor {
    /// Start the helper and keep it running. Fails when the first start fails.
    pub async fn start(cmd: HelperCommand, opts: SuperviseOptions) -> Result<Self, HelperError> {
        std::fs::create_dir_all(&opts.logs).map_err(|e| HelperError::Spawn(e.to_string()))?;
        let log = opts.logs.join(format!("helper-{}.log", cmd.name));
        let (lines_tx, lines_rx) = mpsc::unbounded_channel();
        let (input_tx, input_rx) = mpsc::unbounded_channel();
        let (mut running, offset) = spawn(&cmd, &opts, &log, &lines_tx)?;
        greet(&mut running, &opts).await;
        let (tx, rx) = watch::channel(SupState {
            phase: Phase::Running,
            pid: running.child.id(),
            generation: 1,
            restarts: 0,
            started: Some(Instant::now()),
            log_offset: offset,
        });
        let (stop_tx, stop_rx) = oneshot::channel();
        let task = tokio::spawn(run(
            cmd,
            opts,
            log.clone(),
            running,
            tx,
            stop_rx,
            Io {
                lines: lines_tx,
                input: input_rx,
            },
        ));
        Ok(Self {
            state: rx,
            log,
            stop: Arc::new(Mutex::new(Some(stop_tx))),
            task: Some(task),
            input: input_tx,
            lines: Some(lines_rx),
        })
    }

    /// The helper's output lines, in order, across restarts. Only with `lines`; taken once.
    pub fn take_lines(&mut self) -> Option<mpsc::UnboundedReceiver<String>> {
        self.lines.take()
    }

    /// Writes lines to the running helper's input. Lines sent while none runs wait for the next
    /// start.
    pub fn input(&self) -> mpsc::UnboundedSender<String> {
        self.input.clone()
    }

    /// A handle that asks the helper to stop from elsewhere (a monitor that found a failure).
    pub fn stop_handle(&self) -> StopHandle {
        StopHandle(self.stop.clone())
    }

    pub fn subscribe(&self) -> watch::Receiver<SupState> {
        self.state.clone()
    }

    pub fn state(&self) -> SupState {
        self.state.borrow().clone()
    }

    pub fn log_path(&self) -> &Path {
        &self.log
    }

    /// Stop the helper and return once no process of its group remains.
    pub async fn stop(mut self) -> Result<(), HelperError> {
        self.stop_handle().stop();
        match self.task.take() {
            Some(task) => task.await.map_err(|e| HelperError::Exited(e.to_string()))?,
            None => Ok(()),
        }
    }
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

fn open_log(path: &Path, limit: u64) -> Result<(std::fs::File, u64), HelperError> {
    if std::fs::metadata(path).is_ok_and(|m| m.len() > limit) {
        let _ = std::fs::rename(path, path.with_extension("log.1"));
    }
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options
        .open(path)
        .map_err(|e| HelperError::Spawn(format!("could not open the log: {e}")))?;
    let offset = file.metadata().map_or(0, |m| m.len());
    Ok((file, offset))
}

fn spawn(
    cmd: &HelperCommand,
    opts: &SuperviseOptions,
    log: &Path,
    lines: &mpsc::UnboundedSender<String>,
) -> Result<(Running, u64), HelperError> {
    crate::verify_file(&cmd.program, &cmd.sha256)?;
    let (file, offset) = open_log(log, opts.log_limit)?;
    let err = file
        .try_clone()
        .map_err(|e| HelperError::Spawn(e.to_string()))?;
    let mut command = nocturne_lemonade::process::tokio_command(&cmd.program);
    command
        .args(&cmd.args)
        .env_clear()
        .envs(cmd.env.iter().map(|(k, v)| (k, v)))
        .stderr(Stdio::from(err));
    if opts.lines {
        command.stdin(Stdio::piped()).stdout(Stdio::piped());
    } else {
        command.stdin(Stdio::null()).stdout(Stdio::from(file));
    }
    if let Some(cwd) = &cmd.cwd {
        command.current_dir(cwd);
    }
    let mut child = nocturne_lemonade::process::group(command)
        .spawn()
        .map_err(|e| HelperError::Spawn(format!("could not run {}: {e}", cmd.name)))?;
    let pgid = child.id().unwrap_or(0);
    let stdin = child.stdin().take();
    let reader = child
        .stdout()
        .take()
        .map(|out| tokio::spawn(read_lines(out, lines.clone())));
    Ok((
        Running {
            child,
            pgid,
            stdin,
            reader,
        },
        offset,
    ))
}

/// Forward each output line until the output ends; return the last few.
async fn read_lines(out: impl AsyncRead + Unpin, to: mpsc::UnboundedSender<String>) -> Vec<String> {
    let mut reader = BufReader::new(out);
    let mut tail = VecDeque::new();
    let mut buf = Vec::new();
    let limit = MAX_LINE as u64 + 1;
    loop {
        buf.clear();
        match (&mut reader).take(limit).read_until(b'\n', &mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        if buf.len() > MAX_LINE {
            // Too long: skip to the end of this line without holding it.
            while !buf.ends_with(b"\n") {
                buf.clear();
                match (&mut reader).take(limit).read_until(b'\n', &mut buf).await {
                    Ok(0) | Err(_) => return tail.into(),
                    Ok(_) => {}
                }
            }
            continue;
        }
        let line = String::from_utf8_lossy(&buf).trim_end().to_string();
        if tail.len() == TAIL {
            tail.pop_front();
        }
        tail.push_back(line.clone());
        let _ = to.send(line);
    }
    tail.into()
}

async fn write_line(running: &mut Running, line: &str) {
    if let Some(stdin) = running.stdin.as_mut() {
        let mut bytes = line.as_bytes().to_vec();
        bytes.push(b'\n');
        let wrote = tokio::time::timeout(Duration::from_secs(5), async {
            stdin.write_all(&bytes).await?;
            stdin.flush().await
        })
        .await;
        if !matches!(wrote, Ok(Ok(()))) {
            running.stdin = None;
        }
    }
}

/// The first line of a start, when there is one.
async fn greet(running: &mut Running, opts: &SuperviseOptions) {
    if let Some(Secret(line)) = &opts.first_line {
        write_line(running, line).await;
    }
}

/// The last lines of a run that ended, once its output is read to the end.
async fn tail_of(running: &mut Running) -> Vec<String> {
    match running.reader.take() {
        Some(reader) => tokio::time::timeout(Duration::from_secs(2), reader)
            .await
            .ok()
            .and_then(Result::ok)
            .unwrap_or_default(),
        None => Vec::new(),
    }
}

struct Io {
    lines: mpsc::UnboundedSender<String>,
    input: mpsc::UnboundedReceiver<String>,
}

async fn run(
    cmd: HelperCommand,
    opts: SuperviseOptions,
    log: PathBuf,
    first: Running,
    tx: watch::Sender<SupState>,
    mut stop: oneshot::Receiver<()>,
    mut io: Io,
) -> Result<(), HelperError> {
    let mut running = first;
    let mut restarts: VecDeque<Instant> = VecDeque::new();
    let mut total = 0u32;
    let mut generation = 1u32;
    let mut ticker = tokio::time::interval(opts.rotate_every);
    loop {
        let status = tokio::select! {
            s = running.child.wait() => s,
            _ = ticker.tick() => {
                rotate_running(&log, opts.log_limit);
                continue;
            }
            Some(line) = io.input.recv() => {
                write_line(&mut running, &line).await;
                continue;
            }
            _ = &mut stop => {
                let result = terminate(&mut running, &opts).await;
                publish(&tx, |s| {
                    s.phase = Phase::Stopped;
                    s.pid = None;
                });
                return result;
            }
        };
        let (why, code) = match status {
            Ok(s) => (s.to_string(), s.code()),
            Err(e) => (e.to_string(), None),
        };
        let tail = tail_of(&mut running).await;
        reap_group(&mut running).await;
        publish(&tx, |s| s.pid = None);
        if !(opts.restart.0)(code, &tail) {
            publish(&tx, |s| s.phase = Phase::Ended(code));
            let _ = (&mut stop).await;
            publish(&tx, |s| s.phase = Phase::Stopped);
            return Ok(());
        }
        // A new start is tried until one starts or the limit is reached.
        loop {
            let now = Instant::now();
            while restarts
                .front()
                .is_some_and(|t| now.duration_since(*t) > opts.window)
            {
                restarts.pop_front();
            }
            if restarts.len() as u32 >= opts.max_restarts {
                let message = format!("{} stopped too often: {why}", cmd.name);
                publish(&tx, |s| s.phase = Phase::GaveUp(message.clone()));
                let _ = (&mut stop).await;
                return Ok(());
            }
            let attempt = restarts.len() as u32 + 1;
            let wait = opts
                .backoff
                .get(restarts.len())
                .or(opts.backoff.last())
                .copied()
                .unwrap_or_default();
            publish(&tx, |s| s.phase = Phase::Backoff { attempt });
            tokio::select! {
                () = tokio::time::sleep(wait) => {}
                _ = &mut stop => {
                    publish(&tx, |s| s.phase = Phase::Stopped);
                    return Ok(());
                }
            }
            restarts.push_back(Instant::now());
            total += 1;
            match spawn(&cmd, &opts, &log, &io.lines) {
                Ok((next, offset)) => {
                    running = next;
                    greet(&mut running, &opts).await;
                    generation += 1;
                    let pid = running.child.id();
                    publish(&tx, |s| {
                        s.phase = Phase::Running;
                        s.pid = pid;
                        s.generation = generation;
                        s.restarts = total;
                        s.started = Some(Instant::now());
                        s.log_offset = offset;
                    });
                    break;
                }
                Err(HelperError::Mismatch) => {
                    let message = format!("{} is not the pinned one", cmd.name);
                    publish(&tx, |s| s.phase = Phase::GaveUp(message.clone()));
                    let _ = (&mut stop).await;
                    return Ok(());
                }
                Err(_) => continue,
            }
        }
    }
}

/// Rotate a log that is open for appending in a running helper: copy it aside and cut it short.
/// The helper keeps writing at the end of the same file.
pub(crate) fn rotate_running(log: &Path, limit: u64) {
    if std::fs::metadata(log).is_ok_and(|m| m.len() > limit) {
        let old = log.with_extension("log.1");
        if std::fs::copy(log, &old).is_ok() {
            let _ = OpenOptions::new()
                .write(true)
                .open(log)
                .and_then(|f| f.set_len(0));
        }
    }
}

fn publish(tx: &watch::Sender<SupState>, f: impl FnOnce(&mut SupState)) {
    tx.send_modify(f);
}

/// The stop line where there is one, then SIGTERM to the group, then a kill, then proof that
/// nothing is left.
async fn terminate(running: &mut Running, opts: &SuperviseOptions) -> Result<(), HelperError> {
    if let Some(line) = &opts.stop_line
        && running.stdin.is_some()
    {
        write_line(running, line).await;
        // A line that could not be written was not read either: go straight to SIGTERM.
        if running.stdin.is_some()
            && tokio::time::timeout(opts.stop_wait, running.child.wait())
                .await
                .is_ok()
        {
            let _ = tail_of(running).await;
            reap_group(running).await;
            return left_over(running);
        }
    }
    #[cfg(unix)]
    let _ = running.child.signal(15);
    if tokio::time::timeout(opts.grace, running.child.wait())
        .await
        .is_err()
    {
        let _ = running.child.start_kill();
        let _ = tokio::time::timeout(Duration::from_secs(5), running.child.wait()).await;
    }
    let _ = tail_of(running).await;
    reap_group(running).await;
    left_over(running)
}

fn left_over(running: &Running) -> Result<(), HelperError> {
    let left = members(running.pgid);
    if left.is_empty() {
        Ok(())
    } else {
        Err(HelperError::Exited(format!(
            "processes outlived the helper: {left:?}"
        )))
    }
}

/// After the leader is gone, anything it started in the group is ended too.
async fn reap_group(running: &mut Running) {
    for _ in 0..60 {
        if members(running.pgid).is_empty() {
            return;
        }
        #[cfg(unix)]
        {
            let _ = running.child.signal(9);
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Live processes of the group, where the platform can say; a Windows job object ends all of
/// its members when the leader is killed.
pub fn members(pgid: u32) -> Vec<u32> {
    nocturne_lemonade::supervisor::group_members(pgid)
}

/// Resident memory of a process, where the platform exposes it cheaply.
pub fn rss_bytes(pid: u32) -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
        let kb: u64 = status
            .lines()
            .find_map(|l| l.strip_prefix("VmRSS:"))?
            .split_whitespace()
            .next()?
            .parse()
            .ok()?;
        Some(kb * 1024)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = pid;
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_running_log_is_rotated_by_size() {
        let dir =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/helpers-test-tmp/rotate");
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("helper-x.log");
        std::fs::write(&log, vec![b'a'; 100]).unwrap();
        rotate_running(&log, 200);
        assert_eq!(std::fs::metadata(&log).unwrap().len(), 100);
        rotate_running(&log, 50);
        assert_eq!(std::fs::metadata(&log).unwrap().len(), 0);
        assert_eq!(
            std::fs::metadata(dir.join("helper-x.log.1")).unwrap().len(),
            100
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
