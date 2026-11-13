//! The Most private connection: the official `tailscaled` in userspace mode, driven by the
//! official `tailscale` command, serving Nocturne's listener on the tailnet only. Its state lives
//! under `<data>/remote/tailscale`, it uploads no logs and it is never asked to share publicly.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::Value;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

#[cfg(test)]
use crate::fixtures::allowed_flags_for;
use crate::install::make_private_dir;
use crate::supervise::{HelperCommand, Phase, StopHandle, SupState, SuperviseOptions, Supervisor};
use crate::util::{run_cli, run_cli_code, run_cli_watch};
use crate::{
    BoxFuture, HelperDirs, HelperError, HelperInfo, HelperKind, Installed, KeyCarrier, LaunchSpec,
    Origin, Transport, TransportState, key_carrier, verify,
};

#[derive(Debug, Clone)]
pub struct TailscaledOptions {
    /// How often the connection is checked once it is up.
    pub poll: Duration,
    /// How long the helper may take to answer after it starts.
    pub ready_within: Duration,
    /// How often files received through Taildrop are deleted.
    pub purge_every: Duration,
    pub backoff: Option<Vec<Duration>>,
    pub grace: Option<Duration>,
}

impl Default for TailscaledOptions {
    fn default() -> Self {
        Self {
            poll: Duration::from_secs(30),
            ready_within: Duration::from_secs(60),
            purge_every: Duration::from_secs(3600),
            backoff: None,
            grace: None,
        }
    }
}

pub struct TailscaledTransport {
    dirs: HelperDirs,
    installed: Installed,
    cli: Option<Installed>,
    opts: TailscaledOptions,
    run: Mutex<Option<Run>>,
}

struct Run {
    ctx: Arc<Ctx>,
    sup: Supervisor,
    cancel: CancellationToken,
    monitor: JoinHandle<()>,
    state: Arc<watch::Sender<TransportState>>,
    sup_state: watch::Receiver<SupState>,
}

impl TailscaledTransport {
    pub fn new(dirs: &HelperDirs, installed: &Installed, cli: Option<Installed>) -> Self {
        Self::with_options(dirs, installed, cli, TailscaledOptions::default())
    }

    pub fn with_options(
        dirs: &HelperDirs,
        installed: &Installed,
        cli: Option<Installed>,
        opts: TailscaledOptions,
    ) -> Self {
        Self {
            dirs: dirs.clone(),
            installed: installed.clone(),
            cli,
            opts,
            run: Mutex::new(None),
        }
    }

    fn take_run(&self) -> Option<Run> {
        self.run.lock().expect("run lock").take()
    }

    fn cli(&self) -> Result<Arc<Ctx>, HelperError> {
        let cli = self
            .cli
            .as_ref()
            .ok_or_else(|| HelperError::Spawn("the tailscale command is missing".into()))?;
        verify(cli)?;
        Ok(Arc::new(Ctx {
            pin: cli.clone(),
            paths: Paths::new(&self.dirs),
            env: cli_env(&self.dirs),
            opts: self.opts.clone(),
        }))
    }

    async fn run_daemon(&self) -> Result<Supervisor, HelperError> {
        verify(&self.installed)?;
        let paths = Paths::new(&self.dirs);
        if paths.socket.as_os_str().len() > 100 {
            return Err(HelperError::Spawn(
                "the data folder is too deep for a local socket".into(),
            ));
        }
        make_private_dir(&self.dirs.root)?;
        make_private_dir(&paths.state)?;
        make_private_dir(&paths.state.join("home"))?;
        make_private_dir(&paths.state.join("tmp"))?;
        make_private_dir(&self.dirs.root.join("logs"))?;
        purge_taildrop(&paths.state);
        let cmd = HelperCommand {
            name: "tailscaled",
            program: self.installed.path.clone(),
            sha256: self.installed.sha256.clone(),
            args: daemon_args(&paths)
                .into_iter()
                .map(OsString::from)
                .collect(),
            env: cli_env_with(&self.dirs, true),
            cwd: Some(paths.state.clone()),
        };
        let mut opts = SuperviseOptions::new(self.dirs.root.join("logs"));
        if let Some(b) = &self.opts.backoff {
            opts.backoff = b.clone();
        }
        if let Some(g) = self.opts.grace {
            opts.grace = g;
        }
        Supervisor::start(cmd, opts).await
    }
}

#[derive(Clone)]
pub(crate) struct Paths {
    pub state: PathBuf,
    pub socket: PathBuf,
}

impl Paths {
    pub fn new(dirs: &HelperDirs) -> Self {
        let state = dirs.root.join("tailscale");
        Self {
            socket: state.join("s.sock"),
            state,
        }
    }
}

/// `tailscaled` arguments. No SOCKS or HTTP proxy listener, no TUN device, no log upload.
pub(crate) fn daemon_args(paths: &Paths) -> Vec<String> {
    vec![
        "--tun=userspace-networking".into(),
        format!("--statedir={}", paths.state.display()),
        format!("--socket={}", paths.socket.display()),
        "--port=0".into(),
        "--no-logs-no-support".into(),
    ]
}

fn cli_env(dirs: &HelperDirs) -> Vec<(OsString, OsString)> {
    cli_env_with(dirs, false)
}

fn cli_env_with(dirs: &HelperDirs, daemon: bool) -> Vec<(OsString, OsString)> {
    let state = dirs.root.join("tailscale");
    let mut env: Vec<(OsString, OsString)> = vec![
        ("PATH".into(), "".into()),
        ("HOME".into(), state.join("home").into()),
        ("TMPDIR".into(), state.join("tmp").into()),
    ];
    if daemon {
        env.push(("TS_NO_LOGS_NO_SUPPORT".into(), "true".into()));
    }
    #[cfg(windows)]
    if let Some(root) = std::env::var_os("SystemRoot") {
        env.push(("SystemRoot".into(), root));
    }
    env
}

/// A name that says nothing about the person or the computer: `nocturne-` and four hex digits.
pub fn valid_machine_name(name: &str) -> bool {
    name.strip_prefix("nocturne-").is_some_and(|rest| {
        rest.len() == 4 && rest.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    })
}

/// A new random machine name. It is kept for the install, because it reaches public certificate
/// logs and should not change from one start to the next.
pub fn new_machine_name() -> String {
    use std::hash::{BuildHasher, Hasher};
    let n = std::collections::hash_map::RandomState::new()
        .build_hasher()
        .finish();
    format!("nocturne-{:04x}", n & 0xffff)
}

fn with_socket(paths: &Paths, rest: &[&str]) -> Vec<String> {
    let mut args = vec![format!("--socket={}", paths.socket.display())];
    args.extend(rest.iter().map(|s| s.to_string()));
    args
}

/// Refuse every inbound connection that serve does not handle. In userspace mode the helper would
/// otherwise pass tailnet connections on to this computer's loopback ports.
pub(crate) const SHIELDS_UP: &str = "--shields-up=true";

pub(crate) fn up_args(paths: &Paths, hostname: &str) -> Vec<String> {
    let host = format!("--hostname={hostname}");
    with_socket(
        paths,
        &[
            "up",
            &host,
            "--accept-dns=false",
            "--accept-routes=false",
            "--ssh=false",
            SHIELDS_UP,
            "--json",
        ],
    )
}

/// The settings every start applies. Each flag must be in the pinned help (a test checks), so none
/// is ever dropped quietly.
pub(crate) const SET_FLAGS: [&str; 2] = ["--auto-update=false", SHIELDS_UP];

pub(crate) fn set_args(paths: &Paths) -> Vec<String> {
    let mut rest = vec!["set"];
    rest.extend(SET_FLAGS);
    with_socket(paths, &rest)
}

/// How the listener is served on the tailnet: a Unix socket target in the arguments, or, for a
/// loopback origin with a key, a config file that holds the key so the arguments never do.
pub(crate) struct ServePlan {
    pub args: Vec<String>,
    pub config: Option<(PathBuf, String)>,
}

pub(crate) fn serve_plan(paths: &Paths, origin: &Origin) -> ServePlan {
    match origin {
        Origin::Unix(path) => {
            let target = format!("unix:{}", path.display());
            ServePlan {
                args: with_socket(paths, &["serve", "--bg", "--https=443", "--yes", &target]),
                config: None,
            }
        }
        Origin::Tcp { port, key } => {
            let file = paths.state.join("serve-config.json");
            let body = serde_json::json!({
                "version": "0.0.1",
                "services": {
                    "svc:nocturne": {
                        "endpoints": { "tcp:443": format!("http://127.0.0.1:{port}/{key}") }
                    }
                }
            });
            let name = file.to_string_lossy().into_owned();
            ServePlan {
                args: with_socket(paths, &["serve", "set-config", &name, "--all"]),
                config: Some((file, body.to_string())),
            }
        }
    }
}

fn write_private(path: &Path, text: &str) -> Result<(), HelperError> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.create(true).write(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(path)
        .and_then(|mut f| f.write_all(text.as_bytes()))
        .map_err(|e| HelperError::Spawn(format!("could not write the serve settings: {e}")))
}

/// Files received through Taildrop are not wanted: delete them.
pub(crate) fn purge_taildrop(state: &Path) {
    let _ = std::fs::remove_dir_all(state.join("files"));
}

fn parse_json(text: &str) -> Option<Value> {
    let start = text.find('{')?;
    serde_json::from_str(&text[start..]).ok().or_else(|| {
        let end = text.rfind('}')?;
        serde_json::from_str(&text[start..=end]).ok()
    })
}

/// True when a `serve status --json` document turns public sharing on anywhere.
pub(crate) fn funnel_enabled(doc: &Value) -> bool {
    match doc {
        Value::Object(map) => map.iter().any(|(k, v)| {
            if k == "AllowFunnel" {
                match v {
                    Value::Bool(b) => *b,
                    Value::Object(m) => m.values().any(|x| x.as_bool() == Some(true)),
                    _ => false,
                }
            } else {
                funnel_enabled(v)
            }
        }),
        Value::Array(items) => items.iter().any(funnel_enabled),
        _ => false,
    }
}

pub(crate) struct Seen {
    pub backend: String,
    pub dns_name: Option<String>,
    pub route: Option<String>,
}

/// What `status --json` says: the state, the tailnet name and whether a phone is connected
/// directly or through a relay.
pub(crate) fn read_status(text: &str) -> Option<Seen> {
    let doc = parse_json(text)?;
    let backend = doc.get("BackendState")?.as_str()?.to_string();
    let dns_name = doc
        .pointer("/Self/DNSName")
        .and_then(Value::as_str)
        .map(|n| n.trim_end_matches('.').to_string())
        .filter(|n| !n.is_empty());
    let mut direct = false;
    let mut relayed = false;
    if let Some(peers) = doc.get("Peer").and_then(Value::as_object) {
        for peer in peers.values() {
            if peer.get("Online").and_then(Value::as_bool) != Some(true) {
                continue;
            }
            let cur = peer.get("CurAddr").and_then(Value::as_str).unwrap_or("");
            let relay = peer.get("Relay").and_then(Value::as_str).unwrap_or("");
            if !cur.is_empty() {
                direct = true;
            } else if !relay.is_empty() {
                relayed = true;
            }
        }
    }
    let route = if direct {
        Some("direct".to_string())
    } else if relayed {
        Some("relayed".to_string())
    } else {
        None
    };
    Some(Seen {
        backend,
        dns_name,
        route,
    })
}

fn os(args: &[String]) -> Vec<OsString> {
    args.iter().map(OsString::from).collect()
}

struct Ctx {
    pin: Installed,
    paths: Paths,
    env: Vec<(OsString, OsString)>,
    opts: TailscaledOptions,
}

impl Ctx {
    async fn status(&self) -> Option<Seen> {
        let args = with_socket(&self.paths, &["status", "--json"]);
        let out = run_cli(&self.pin, &os(&args), &self.env, Duration::from_secs(10))
            .await
            .ok()?;
        read_status(&out)
    }

    async fn wait_daemon(&self) -> Result<(), HelperError> {
        let deadline = Instant::now() + self.opts.ready_within;
        loop {
            if self.status().await.is_some() {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(HelperError::Exited("the helper did not answer".into()));
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// Everything after the daemon answers: settings, sign-in, serve, checks. Reports the
    /// address and route.
    async fn bring_up(
        &self,
        spec: &LaunchSpec,
        tx: &watch::Sender<TransportState>,
        cancel: &CancellationToken,
    ) -> Result<(String, Option<String>), HelperError> {
        self.wait_daemon().await?;
        let (code, _) = run_cli_code(
            &self.pin,
            &os(&set_args(&self.paths)),
            &self.env,
            Duration::from_secs(20),
        )
        .await?;
        if code != Some(0) {
            return Err(HelperError::Exited(
                "the helper refused its settings".into(),
            ));
        }
        let up = up_args(&self.paths, &spec.machine_name);
        let mut last = String::new();
        let code = run_cli_watch(&self.pin, &os(&up), &self.env, cancel, |line| {
            last = line.to_string();
            if let Some(url) = parse_json(line)
                .and_then(|d| d.get("AuthURL").and_then(Value::as_str).map(String::from))
                .filter(|u| !u.is_empty())
            {
                tx.send_replace(TransportState::NeedsSignIn {
                    url,
                    allow_links: false,
                });
            }
        })
        .await?;
        if code != Some(0) {
            return Err(HelperError::SignIn(last));
        }
        tx.send_replace(TransportState::Starting);

        let plan = serve_plan(&self.paths, &spec.origin);
        if let Some((file, body)) = &plan.config {
            write_private(file, body)?;
        }
        let mut last = String::new();
        let code = run_cli_watch(&self.pin, &os(&plan.args), &self.env, cancel, |line| {
            last = line.to_string();
            let word = line
                .split_whitespace()
                .find(|w| w.starts_with("https://") && w.contains("/f/"));
            if let Some(url) = word {
                tx.send_replace(TransportState::NeedsSignIn {
                    url: url.to_string(),
                    allow_links: true,
                });
            }
        })
        .await?;
        if code != Some(0) {
            return Err(HelperError::SignIn(last));
        }
        tx.send_replace(TransportState::Starting);

        let args = with_socket(&self.paths, &["serve", "status", "--json"]);
        let out = run_cli(&self.pin, &os(&args), &self.env, Duration::from_secs(20)).await?;
        if parse_json(&out).is_some_and(|d| funnel_enabled(&d)) {
            let reset = with_socket(&self.paths, &["serve", "reset"]);
            let _ = run_cli(&self.pin, &os(&reset), &self.env, Duration::from_secs(20)).await;
            return Err(HelperError::Exited(
                "the helper was set to share publicly, so it was stopped".into(),
            ));
        }
        let seen = self
            .status()
            .await
            .ok_or_else(|| HelperError::Exited("the helper did not report its address".into()))?;
        let name = seen
            .dns_name
            .ok_or_else(|| HelperError::Exited("the helper has no address yet".into()))?;
        Ok((format!("https://{name}"), seen.route))
    }
}

impl Transport for TailscaledTransport {
    fn kind(&self) -> HelperKind {
        HelperKind::Tailscaled
    }

    fn key_carrier(&self) -> KeyCarrier {
        key_carrier(HelperKind::Tailscaled)
    }

    fn start(
        &self,
        spec: LaunchSpec,
    ) -> BoxFuture<'_, Result<watch::Receiver<TransportState>, HelperError>> {
        Box::pin(async move {
            if !valid_machine_name(&spec.machine_name) {
                return Err(HelperError::Spawn(
                    "the computer name is not allowed".into(),
                ));
            }
            if let Some(old) = self.take_run() {
                shut(old, &self.dirs).await?;
            }
            let ctx = self.cli()?;
            let sup = self.run_daemon().await?;
            let (tx, rx) = watch::channel(TransportState::Starting);
            let tx = Arc::new(tx);
            let cancel = CancellationToken::new();
            let sup_state = sup.subscribe();
            let monitor = tokio::spawn(monitor(
                ctx.clone(),
                sup.stop_handle(),
                spec,
                sup_state.clone(),
                tx.clone(),
                cancel.clone(),
            ));
            *self.run.lock().expect("run lock") = Some(Run {
                ctx,
                sup,
                cancel,
                monitor,
                state: tx,
                sup_state,
            });
            Ok(rx)
        })
    }

    fn stop(&self) -> BoxFuture<'_, Result<(), HelperError>> {
        Box::pin(async move {
            match self.take_run() {
                Some(run) => shut(run, &self.dirs).await,
                None => Ok(()),
            }
        })
    }

    /// While on: log out, stop and forget the state. While off nothing is started and nothing
    /// connects: only the local state folder is deleted.
    fn sign_out(&self) -> BoxFuture<'_, Result<(), HelperError>> {
        Box::pin(async move {
            let paths = Paths::new(&self.dirs);
            let Some(run) = self.take_run() else {
                let _ = std::fs::remove_dir_all(&paths.state);
                return Ok(());
            };
            let outcome = match self.cli() {
                Ok(ctx) => {
                    let args = with_socket(&paths, &["logout"]);
                    run_cli(&ctx.pin, &os(&args), &ctx.env, Duration::from_secs(30))
                        .await
                        .map(|_| ())
                }
                Err(e) => Err(e),
            };
            let stopped = shut(run, &self.dirs).await;
            let _ = std::fs::remove_dir_all(&paths.state);
            outcome.and(stopped)
        })
    }

    fn info(&self) -> Option<HelperInfo> {
        let guard = self.run.lock().expect("run lock");
        let run = guard.as_ref()?;
        let s = run.sup_state.borrow().clone();
        Some(HelperInfo {
            name: "tailscaled",
            version: self.installed.version.clone(),
            pid: s.pid,
            uptime: s.started.map(|t| t.elapsed()),
            restarts: s.restarts,
            rss_bytes: s.pid.and_then(crate::supervise::rss_bytes),
        })
    }
}

/// Stop: forget the serve settings, end the monitor, end the helper, delete Taildrop files.
async fn shut(run: Run, dirs: &HelperDirs) -> Result<(), HelperError> {
    run.cancel.cancel();
    let _ = run.monitor.await;
    let reset = with_socket(&run.ctx.paths, &["serve", "reset"]);
    let _ = run_cli(
        &run.ctx.pin,
        &os(&reset),
        &run.ctx.env,
        Duration::from_secs(10),
    )
    .await;
    let result = run.sup.stop().await;
    purge_taildrop(&Paths::new(dirs).state);
    run.state.send_replace(TransportState::Stopped);
    result
}

async fn monitor(
    ctx: Arc<Ctx>,
    halt: StopHandle,
    spec: LaunchSpec,
    mut sup: watch::Receiver<SupState>,
    tx: Arc<watch::Sender<TransportState>>,
    cancel: CancellationToken,
) {
    let mut last_purge = Instant::now();
    'cycle: loop {
        let now = sup.borrow_and_update().clone();
        match now.phase {
            Phase::Stopped => return,
            Phase::GaveUp(message) => {
                tx.send_replace(TransportState::Failed {
                    error: HelperError::Exited(message),
                });
                return;
            }
            Phase::Backoff { attempt } => {
                tx.send_replace(TransportState::Restarting { attempt });
            }
            Phase::Running => {
                let up = tokio::select! {
                    r = ctx.bring_up(&spec, &tx, &cancel) => r,
                    _ = sup.changed() => continue 'cycle,
                    () = cancel.cancelled() => return,
                };
                let (address, mut route) = match up {
                    Ok(found) => found,
                    Err(HelperError::Stopped) => return,
                    Err(error) => {
                        // Nothing stays served and nothing keeps running behind a failure.
                        let reset = with_socket(&ctx.paths, &["serve", "reset"]);
                        let _ =
                            run_cli(&ctx.pin, &os(&reset), &ctx.env, Duration::from_secs(10)).await;
                        halt.stop();
                        let _ = tokio::time::timeout(Duration::from_secs(15), async {
                            while !matches!(sup.borrow_and_update().phase, Phase::Stopped) {
                                if sup.changed().await.is_err() {
                                    break;
                                }
                            }
                        })
                        .await;
                        tx.send_replace(TransportState::Failed { error });
                        return;
                    }
                };
                tx.send_replace(TransportState::Up {
                    address: address.clone(),
                    route: route.clone(),
                });
                loop {
                    tokio::select! {
                        r = sup.changed() => {
                            if r.is_err() { return }
                            continue 'cycle;
                        }
                        () = cancel.cancelled() => return,
                        () = tokio::time::sleep(ctx.opts.poll) => {
                            if last_purge.elapsed() >= ctx.opts.purge_every {
                                purge_taildrop(&ctx.paths.state);
                                last_purge = Instant::now();
                            }
                            match ctx.status().await {
                                Some(seen) if seen.backend == "Running" => {
                                    if seen.route != route {
                                        route = seen.route;
                                        tx.send_replace(TransportState::Up {
                                            address: address.clone(),
                                            route: route.clone(),
                                        });
                                    }
                                }
                                Some(_) => continue 'cycle,
                                None => {}
                            }
                        }
                    }
                }
            }
        }
        tokio::select! {
            r = sup.changed() => if r.is_err() { return },
            () = cancel.cancelled() => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dirs() -> HelperDirs {
        HelperDirs {
            root: PathBuf::from("/data/remote"),
        }
    }

    fn unix() -> Origin {
        Origin::Unix("/data/remote/run/listener.sock".into())
    }

    fn windows_origin() -> Origin {
        Origin::Tcp {
            port: 5000,
            key: "s3cret-key-value".into(),
        }
    }

    /// Every command line Nocturne builds, with the fixture name of its command.
    fn all_cli_lines() -> Vec<(String, Vec<String>)> {
        let paths = Paths::new(&dirs());
        let mut lines = vec![
            ("up".to_string(), up_args(&paths, "nocturne-ab12")),
            (
                "status".to_string(),
                with_socket(&paths, &["status", "--json"]),
            ),
            (
                "serve-status".to_string(),
                with_socket(&paths, &["serve", "status", "--json"]),
            ),
            (
                "serve-reset".to_string(),
                with_socket(&paths, &["serve", "reset"]),
            ),
            ("logout".to_string(), with_socket(&paths, &["logout"])),
            ("serve".to_string(), serve_plan(&paths, &unix()).args),
            (
                "serve-set-config".to_string(),
                serve_plan(&paths, &windows_origin()).args,
            ),
        ];
        lines.push(("set".to_string(), set_args(&paths)));
        lines
    }

    #[test]
    fn daemon_args_are_exact() {
        let args = daemon_args(&Paths::new(&dirs()));
        assert_eq!(
            args,
            [
                "--tun=userspace-networking",
                "--statedir=/data/remote/tailscale",
                "--socket=/data/remote/tailscale/s.sock",
                "--port=0",
                "--no-logs-no-support",
            ]
        );
        let joined = args.join(" ");
        for banned in ["socks5", "outbound-http-proxy", "funnel", "--tun=tailscale"] {
            assert!(!joined.contains(banned), "{banned}");
        }
        let allowed = allowed_flags_for(HelperKind::Tailscaled, "daemon");
        for a in &args {
            assert!(allowed.contains(&a.split('=').next().unwrap()), "{a}");
        }
    }

    #[test]
    fn environment_says_no_logs() {
        let env = cli_env_with(&dirs(), true);
        let get = |k: &str| {
            env.iter()
                .find(|(key, _)| key == k)
                .map(|(_, v)| v.to_string_lossy().into_owned())
        };
        assert_eq!(get("TS_NO_LOGS_NO_SUPPORT").as_deref(), Some("true"));
        assert_eq!(get("HOME").as_deref(), Some("/data/remote/tailscale/home"));
        assert_eq!(get("PATH").as_deref(), Some(""));
        let keys: Vec<_> = env
            .iter()
            .map(|(k, _)| k.to_string_lossy().into_owned())
            .collect();
        assert!(keys.iter().all(|k| {
            [
                "PATH",
                "HOME",
                "TMPDIR",
                "TS_NO_LOGS_NO_SUPPORT",
                "SystemRoot",
            ]
            .contains(&k.as_str())
        }));
    }

    #[test]
    fn every_flag_used_is_in_the_fixture() {
        for (fixture, args) in all_cli_lines() {
            let allowed = allowed_flags_for(HelperKind::Tailscaled, &fixture);
            let flags = allowed_flags_for(HelperKind::Tailscaled, "tailscale");
            for a in args.iter().filter(|a| a.starts_with("--")) {
                let name = a.split('=').next().unwrap();
                assert!(
                    allowed.contains(&name) || flags.contains(&name),
                    "{name} is not in the {fixture} fixture"
                );
            }
        }
    }

    #[test]
    fn funnel_never_requested_and_refused_if_seen() {
        for (_, args) in all_cli_lines() {
            assert!(!args.iter().any(|a| a.contains("funnel")), "{args:?}");
        }
        let on = serde_json::json!({"AllowFunnel": {"host.ts.net:443": true}});
        let off = serde_json::json!({"AllowFunnel": {"host.ts.net:443": false}, "TCP": {}});
        let none = serde_json::json!({"Web": {"host:443": {"Handlers": {}}}});
        assert!(funnel_enabled(&on));
        assert!(!funnel_enabled(&off));
        assert!(!funnel_enabled(&none));
        let nested = serde_json::json!({"Services": {"svc:x": {"AllowFunnel": {"a": true}}}});
        assert!(funnel_enabled(&nested));
    }

    #[test]
    fn every_required_setting_is_in_the_fixture() {
        let known = allowed_flags_for(HelperKind::Tailscaled, "set");
        for flag in SET_FLAGS {
            assert!(known.contains(&flag.split('=').next().unwrap()), "{flag}");
        }
    }

    #[test]
    fn shields_stay_up() {
        let paths = Paths::new(&dirs());
        assert!(up_args(&paths, "nocturne-ab12").contains(&SHIELDS_UP.to_string()));
        assert!(set_args(&paths).contains(&SHIELDS_UP.to_string()));
        let known = allowed_flags_for(HelperKind::Tailscaled, "set");
        assert!(known.contains(&"--shields-up"));
        assert!(allowed_flags_for(HelperKind::Tailscaled, "up").contains(&"--shields-up"));
    }

    #[test]
    fn serve_targets_the_socket_or_key_path() {
        let plan = serve_plan(&Paths::new(&dirs()), &unix());
        assert_eq!(
            plan.args,
            [
                "--socket=/data/remote/tailscale/s.sock",
                "serve",
                "--bg",
                "--https=443",
                "--yes",
                "unix:/data/remote/run/listener.sock"
            ]
        );
        assert!(plan.config.is_none());
        assert!(!plan.args.iter().any(|a| a.contains("funnel")));
    }

    #[test]
    fn windows_serve_config_not_argv() {
        let plan = serve_plan(&Paths::new(&dirs()), &windows_origin());
        assert!(!plan.args.iter().any(|a| a.contains("s3cret-key-value")));
        let (file, body) = plan.config.expect("a config file");
        assert!(plan.args.contains(&file.to_string_lossy().into_owned()));
        assert!(body.contains("http://127.0.0.1:5000/s3cret-key-value"));
    }

    #[test]
    fn machine_name_neutral_and_stable() {
        assert!(valid_machine_name("nocturne-ab12"));
        assert!(!valid_machine_name("abns-laptop"));
        assert!(!valid_machine_name("nocturne-AB12"));
        assert!(!valid_machine_name("nocturne-ab123"));
        for _ in 0..20 {
            assert!(valid_machine_name(&new_machine_name()));
        }
        let up = up_args(&Paths::new(&dirs()), "nocturne-ab12");
        assert!(up.contains(&"--hostname=nocturne-ab12".to_string()));
    }

    #[test]
    fn status_gives_the_name_and_the_route() {
        let direct = r#"{"BackendState":"Running","Self":{"DNSName":"nocturne-ab12.tail-x.ts.net."},
            "Peer":{"a":{"Online":true,"CurAddr":"1.2.3.4:5","Relay":"fra"},"b":{"Online":false,"CurAddr":"","Relay":""}}}"#;
        let seen = read_status(direct).unwrap();
        assert_eq!(seen.backend, "Running");
        assert_eq!(
            seen.dns_name.as_deref(),
            Some("nocturne-ab12.tail-x.ts.net")
        );
        assert_eq!(seen.route.as_deref(), Some("direct"));
        let relayed =
            r#"{"BackendState":"Running","Peer":{"a":{"Online":true,"CurAddr":"","Relay":"fra"}}}"#;
        assert_eq!(
            read_status(relayed).unwrap().route.as_deref(),
            Some("relayed")
        );
        let idle = r#"{"BackendState":"NeedsLogin"}"#;
        let seen = read_status(idle).unwrap();
        assert!(seen.dns_name.is_none() && seen.route.is_none());
        assert!(read_status("not json").is_none());
    }
}
