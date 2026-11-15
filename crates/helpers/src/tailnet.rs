//! The Most private connection: Nocturne's own helper, `nocturne-tailnet` (built from
//! `helpers/tailnet`), which serves the listener on port 443 of the person's tailnet to their own
//! devices only, uploads no logs and forwards nothing to this computer's loopback. Nocturne talks
//! to it in JSON lines (`helpers/tailnet/proto.go`); its state lives under `<data>/remote/tailscale`.

use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Deserialize;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use crate::install::make_private_dir;
use crate::supervise::{
    HelperCommand, Phase, RestartRule, Secret, StopHandle, SupState, SuperviseOptions, Supervisor,
};
use crate::{
    BoxFuture, HelperDirs, HelperError, HelperInfo, HelperKind, Installed, KeyCarrier, LaunchSpec,
    Origin, Transport, TransportState, key_carrier, verify,
};

/// Where the account's owner approves a new computer.
pub const APPROVE_URL: &str = "https://login.tailscale.com/admin/machines";
/// Where the account turns on the names and certificates secure links need.
pub const HTTPS_URL: &str = "https://login.tailscale.com/admin/dns";

pub(crate) const CMD_STATUS: &str = r#"{"cmd":"status"}"#;
pub(crate) const CMD_STOP: &str = r#"{"cmd":"stop"}"#;
pub(crate) const CMD_LOGOUT: &str = r#"{"cmd":"logout"}"#;

#[derive(Debug, Clone)]
pub struct TailnetOptions {
    /// How often the helper is asked for its state.
    pub poll: Duration,
    /// How long a certificate may keep failing before it is reported.
    pub cert_grace: Duration,
    /// How long the helper has to stop by itself before it is sent SIGTERM.
    pub stop_wait: Duration,
    /// How long signing out may take before the helper is stopped anyway.
    pub logout_wait: Duration,
    pub backoff: Option<Vec<Duration>>,
    pub grace: Option<Duration>,
}

impl Default for TailnetOptions {
    fn default() -> Self {
        Self {
            poll: Duration::from_secs(30),
            cert_grace: Duration::from_secs(30),
            stop_wait: Duration::from_secs(15),
            logout_wait: Duration::from_secs(75),
            backoff: None,
            grace: None,
        }
    }
}

pub struct TailnetTransport {
    dirs: HelperDirs,
    installed: Installed,
    opts: TailnetOptions,
    run: Mutex<Option<Run>>,
}

struct Run {
    sup: Supervisor,
    input: mpsc::UnboundedSender<String>,
    cancel: CancellationToken,
    monitor: JoinHandle<()>,
    state: Arc<watch::Sender<TransportState>>,
    sup_state: watch::Receiver<SupState>,
    logout_failed: Arc<AtomicBool>,
    /// Set before the sign-out command, so the helper's own end is not taken for a crash.
    leaving: Arc<AtomicBool>,
}

/// The helper's state folder.
pub(crate) fn state_dir(dirs: &HelperDirs) -> PathBuf {
    dirs.root.join("tailscale")
}

/// Where the helper forwards to: the owner-only socket, or loopback with the key in a header.
pub(crate) fn target(origin: &Origin) -> String {
    match origin {
        Origin::Unix(path) => format!("unix:{}", path.display()),
        Origin::Tcp { port, .. } => format!("http://127.0.0.1:{port}"),
    }
}

/// The exact command line, after the program. The key is never here.
pub(crate) fn helper_args(dirs: &HelperDirs, machine_name: &str, origin: &Origin) -> Vec<String> {
    vec![
        "--state-dir".into(),
        state_dir(dirs).to_string_lossy().into_owned(),
        "--hostname".into(),
        machine_name.into(),
        "--target".into(),
        target(origin),
    ]
}

/// The exact environment: nothing is inherited, so no Tailscale setting reaches the helper.
pub(crate) fn helper_env(dirs: &HelperDirs) -> Vec<(OsString, OsString)> {
    let state = state_dir(dirs);
    #[allow(unused_mut)]
    let mut env: Vec<(OsString, OsString)> = vec![
        ("PATH".into(), "".into()),
        ("HOME".into(), state.join("home").into()),
        ("TMPDIR".into(), state.join("tmp").into()),
    ];
    #[cfg(windows)]
    if let Some(root) = std::env::var_os("SystemRoot") {
        env.push(("SystemRoot".into(), root));
    }
    env
}

/// The first input line with a loopback target: the key, which never reaches argv or the
/// environment.
pub(crate) fn key_line(origin: &Origin) -> Option<Secret> {
    match origin {
        Origin::Tcp { key, .. } => Some(Secret(serde_json::json!({ "key": key }).to_string())),
        Origin::Unix(_) => None,
    }
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Event {
    state: String,
    #[serde(rename = "authUrl", default)]
    auth_url: Option<String>,
    #[serde(default)]
    host: Option<String>,
    #[serde(default)]
    reason: Option<String>,
}

/// What one line from the helper means.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Seen {
    State(TransportState),
    /// The certificate is not there yet; the helper tries again by itself.
    Certificate,
    /// Trouble the helper reports and carries on from; its exit code, if it ends, decides.
    Transient,
    /// A failure that ends this run: the helper is stopped and not started again.
    Fatal(HelperError),
    LogoutFailed,
    Stopped,
}

/// The hosts a sign-in link may point at: Tailscale's own login servers.
const SIGN_IN_HOSTS: [&str; 2] = ["login.tailscale.com", "controlplane.tailscale.com"];

/// An https link on one of Tailscale's login hosts, with no user part or port.
fn sign_in_link(url: &str) -> bool {
    url.len() <= 2048
        && !url.contains(|c: char| c.is_whitespace() || c.is_control())
        && url.strip_prefix("https://").is_some_and(|rest| {
            let host = rest.split(['/', '?', '#']).next().unwrap_or("");
            SIGN_IN_HOSTS.contains(&host)
        })
}

fn tailnet_host(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 253
        && host
            .bytes()
            .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.'))
        && !host.starts_with('.')
        && !host.contains("..")
}

/// One output line, read. Anything that is not a known event is ignored.
pub(crate) fn read_event(line: &str) -> Option<Seen> {
    let e: Event = serde_json::from_str(line).ok()?;
    let seen = match e.state.as_str() {
        "starting" => Seen::State(TransportState::Starting),
        "needsLogin" => {
            let url = e.auth_url.filter(|u| sign_in_link(u))?;
            Seen::State(TransportState::NeedsSignIn {
                url,
                allow_links: false,
            })
        }
        "needsHttps" => Seen::State(TransportState::NeedsSignIn {
            url: HTTPS_URL.into(),
            allow_links: true,
        }),
        "needsApproval" => Seen::State(TransportState::NeedsApproval {
            url: APPROVE_URL.into(),
        }),
        "running" => {
            let host = e.host.filter(|h| tailnet_host(h))?;
            Seen::State(TransportState::Up {
                address: format!("https://{host}"),
                route: None,
            })
        }
        "stopped" => Seen::Stopped,
        "error" => match e.reason.as_deref()? {
            "certificate" => Seen::Certificate,
            "logout" => Seen::LogoutFailed,
            "blocked" => Seen::Fatal(HelperError::AccountBlocked),
            "login" => Seen::Fatal(HelperError::SignIn(
                "the sign-in link was not Tailscale's".into(),
            )),
            "key" => Seen::Fatal(HelperError::Exited("the helper refused its key".into())),
            "input" => Seen::Fatal(HelperError::Exited("the helper refused its input".into())),
            "start" => Seen::Fatal(HelperError::Exited("the helper could not start".into())),
            "backend" => Seen::Transient,
            "listen" => Seen::Fatal(HelperError::Exited(
                "the helper could not open its address".into(),
            )),
            _ => return None,
        },
        _ => return None,
    };
    Some(seen)
}

/// Whether a run that ended by itself is started again. Not after a failure, not when Nocturne
/// asked it to sign out (`leaving`), and not after a refusal (exit code 2 before any event, so a
/// wrong command line or setting). Anything else is a crash or a stop Nocturne did not ask for,
/// a signal from outside among them, and is.
pub(crate) fn restart_after(code: Option<i32>, tail: &[String], leaving: bool) -> bool {
    let events: Vec<Seen> = tail.iter().filter_map(|l| read_event(l)).collect();
    if leaving || events.iter().any(|e| matches!(e, Seen::Fatal(_))) {
        return false;
    }
    !(code == Some(2) && events.is_empty())
}

impl TailnetTransport {
    pub fn new(dirs: &HelperDirs, installed: &Installed) -> Self {
        Self::with_options(dirs, installed, TailnetOptions::default())
    }

    pub fn with_options(dirs: &HelperDirs, installed: &Installed, opts: TailnetOptions) -> Self {
        Self {
            dirs: dirs.clone(),
            installed: installed.clone(),
            opts,
            run: Mutex::new(None),
        }
    }

    fn take_run(&self) -> Option<Run> {
        self.run.lock().expect("run lock").take()
    }

    async fn spawn(
        &self,
        spec: &LaunchSpec,
        leaving: Arc<AtomicBool>,
    ) -> Result<Supervisor, HelperError> {
        verify(&self.installed)?;
        let state = state_dir(&self.dirs);
        make_private_dir(&self.dirs.root)?;
        make_private_dir(&state)?;
        make_private_dir(&state.join("home"))?;
        make_private_dir(&state.join("tmp"))?;
        make_private_dir(&self.dirs.root.join("logs"))?;
        let cmd = HelperCommand {
            name: "nocturne-tailnet",
            program: self.installed.path.clone(),
            sha256: self.installed.sha256.clone(),
            args: helper_args(&self.dirs, &spec.machine_name, &spec.origin)
                .into_iter()
                .map(OsString::from)
                .collect(),
            env: helper_env(&self.dirs),
            cwd: Some(state),
        };
        let mut opts = SuperviseOptions::new(self.dirs.root.join("logs"));
        opts.lines = true;
        opts.first_line = key_line(&spec.origin);
        opts.stop_line = Some(CMD_STOP.into());
        opts.stop_wait = self.opts.stop_wait;
        opts.restart = RestartRule(Arc::new(move |code, tail| {
            restart_after(code, tail, leaving.load(Ordering::SeqCst))
        }));
        if let Some(b) = &self.opts.backoff {
            opts.backoff = b.clone();
        }
        if let Some(g) = self.opts.grace {
            opts.grace = g;
        }
        Supervisor::start(cmd, opts).await
    }
}

impl Transport for TailnetTransport {
    fn kind(&self) -> HelperKind {
        HelperKind::Tailnet
    }

    fn key_carrier(&self) -> KeyCarrier {
        key_carrier(HelperKind::Tailnet)
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
                shut(old).await?;
            }
            let leaving = Arc::new(AtomicBool::new(false));
            let mut sup = self.spawn(&spec, leaving.clone()).await?;
            let lines = sup
                .take_lines()
                .ok_or_else(|| HelperError::Spawn("the helper has no output".into()))?;
            let (tx, rx) = watch::channel(TransportState::Starting);
            let tx = Arc::new(tx);
            let cancel = CancellationToken::new();
            let sup_state = sup.subscribe();
            let input = sup.input();
            let logout_failed = Arc::new(AtomicBool::new(false));
            let monitor = tokio::spawn(monitor(Watch {
                lines,
                sup: sup_state.clone(),
                halt: sup.stop_handle(),
                input: input.clone(),
                tx: tx.clone(),
                cancel: cancel.clone(),
                opts: self.opts.clone(),
                logout_failed: logout_failed.clone(),
                leaving: leaving.clone(),
            }));
            *self.run.lock().expect("run lock") = Some(Run {
                sup,
                input,
                cancel,
                monitor,
                state: tx,
                sup_state,
                logout_failed,
                leaving,
            });
            Ok(rx)
        })
    }

    fn stop(&self) -> BoxFuture<'_, Result<(), HelperError>> {
        Box::pin(async move {
            match self.take_run() {
                Some(run) => shut(run).await,
                None => Ok(()),
            }
        })
    }

    /// While on: ask the helper to sign out, wait for it to end, then forget the state. While
    /// off nothing is started and nothing connects: only the local state folder is deleted.
    fn sign_out(&self) -> BoxFuture<'_, Result<(), HelperError>> {
        Box::pin(async move {
            let state = state_dir(&self.dirs);
            let Some(run) = self.take_run() else {
                let _ = std::fs::remove_dir_all(&state);
                return Ok(());
            };
            run.leaving.store(true, Ordering::SeqCst);
            let _ = run.input.send(CMD_LOGOUT.into());
            let mut phase = run.sup_state.clone();
            let ended = tokio::time::timeout(self.opts.logout_wait, async {
                loop {
                    if matches!(
                        phase.borrow_and_update().phase,
                        Phase::Ended(_) | Phase::Stopped | Phase::GaveUp(_)
                    ) {
                        return;
                    }
                    if phase.changed().await.is_err() {
                        return;
                    }
                }
            })
            .await
            .is_ok();
            let failed = run.logout_failed.load(Ordering::SeqCst);
            let stopped = shut(run).await;
            let _ = std::fs::remove_dir_all(&state);
            if !ended || failed {
                return Err(HelperError::SignIn("signing out did not finish".into()));
            }
            stopped
        })
    }

    fn info(&self) -> Option<HelperInfo> {
        let guard = self.run.lock().expect("run lock");
        let run = guard.as_ref()?;
        let s = run.sup_state.borrow().clone();
        Some(HelperInfo {
            name: "nocturne-tailnet",
            version: self.installed.version.clone(),
            pid: s.pid,
            uptime: s.started.map(|t| t.elapsed()),
            restarts: s.restarts,
            rss_bytes: s.pid.and_then(crate::supervise::rss_bytes),
        })
    }
}

/// End the monitor, then the helper: the stop command, then SIGTERM, then the group.
async fn shut(run: Run) -> Result<(), HelperError> {
    run.cancel.cancel();
    let _ = run.monitor.await;
    let result = run.sup.stop().await;
    run.state.send_replace(TransportState::Stopped);
    result
}

struct Watch {
    lines: mpsc::UnboundedReceiver<String>,
    sup: watch::Receiver<SupState>,
    halt: StopHandle,
    input: mpsc::UnboundedSender<String>,
    tx: Arc<watch::Sender<TransportState>>,
    cancel: CancellationToken,
    opts: TailnetOptions,
    logout_failed: Arc<AtomicBool>,
    leaving: Arc<AtomicBool>,
}

fn set(tx: &watch::Sender<TransportState>, next: TransportState) {
    tx.send_if_modified(|now| {
        if *now == next {
            false
        } else {
            *now = next;
            true
        }
    });
}

#[derive(Default)]
struct Run1 {
    failed: bool,
    cert_since: Option<Instant>,
    /// The certificate problem is shown; only progress replaces it, not a repeated `starting`.
    cert_shown: bool,
}

impl Watch {
    fn on_line(&mut self, line: &str, r: &mut Run1) {
        let Some(seen) = read_event(line) else {
            return;
        };
        if r.failed {
            return;
        }
        match seen {
            Seen::State(st) => {
                if r.cert_shown && st == TransportState::Starting {
                    return;
                }
                if matches!(st, TransportState::Up { .. }) {
                    r.cert_since = None;
                }
                r.cert_shown = false;
                set(&self.tx, st);
            }
            Seen::Certificate => {
                r.cert_since.get_or_insert_with(Instant::now);
            }
            Seen::Fatal(error) => {
                r.failed = true;
                set(&self.tx, TransportState::Failed { error });
                self.halt.stop();
            }
            Seen::LogoutFailed => self.logout_failed.store(true, Ordering::SeqCst),
            Seen::Transient | Seen::Stopped => {}
        }
    }

    fn drain(&mut self, r: &mut Run1) {
        while let Ok(line) = self.lines.try_recv() {
            self.on_line(&line, r);
        }
    }

    /// Follows the supervisor; returns when nothing more will happen.
    fn on_phase(&mut self, r: &mut Run1) -> bool {
        let phase = self.sup.borrow_and_update().phase.clone();
        match phase {
            Phase::Running => {
                if matches!(*self.tx.borrow(), TransportState::Restarting { .. }) {
                    set(&self.tx, TransportState::Starting);
                }
                true
            }
            Phase::Backoff { attempt } => {
                r.cert_since = None;
                r.cert_shown = false;
                set(&self.tx, TransportState::Restarting { attempt });
                true
            }
            Phase::GaveUp(message) => {
                if !r.failed {
                    set(
                        &self.tx,
                        TransportState::Failed {
                            error: HelperError::Exited(message),
                        },
                    );
                }
                false
            }
            // Stopped is published only by Nocturne's own stop. A run that ended without a
            // failure and without being asked to sign out was refused.
            Phase::Ended(_) => {
                if !r.failed && !self.leaving.load(Ordering::SeqCst) {
                    set(
                        &self.tx,
                        TransportState::Failed {
                            error: HelperError::Spawn("the helper refused to start".into()),
                        },
                    );
                }
                false
            }
            Phase::Stopped => false,
        }
    }
}

async fn monitor(mut w: Watch) {
    let mut r = Run1::default();
    let mut poll = tokio::time::interval_at(Instant::now() + w.opts.poll, w.opts.poll);
    loop {
        let cert_due = r.cert_since.map(|t| t + w.opts.cert_grace);
        let cert_wait = async {
            match cert_due {
                Some(at) => tokio::time::sleep_until(at).await,
                None => std::future::pending().await,
            }
        };
        tokio::select! {
            () = w.cancel.cancelled() => return,
            line = w.lines.recv() => match line {
                Some(line) => w.on_line(&line, &mut r),
                None => return,
            },
            changed = w.sup.changed() => {
                if changed.is_err() {
                    return;
                }
                w.drain(&mut r);
                if !w.on_phase(&mut r) {
                    return;
                }
            }
            _ = poll.tick() => {
                let _ = w.input.send(CMD_STATUS.into());
            }
            () = cert_wait => {
                r.cert_since = None;
                if !r.failed && !matches!(*w.tx.borrow(), TransportState::Up { .. }) {
                    r.cert_shown = true;
                    set(&w.tx, TransportState::Failed {
                        error: HelperError::Exited("the secure address is not ready yet".into()),
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::allowed_flags_for;

    fn dirs() -> HelperDirs {
        HelperDirs {
            root: PathBuf::from("/data/remote"),
        }
    }

    /// A path under the test root as the platform writes it: `/data/remote\\x` on Windows.
    fn under(parts: &[&str]) -> String {
        let path = parts.iter().fold(dirs().root, |p, part| p.join(part));
        path.to_string_lossy().into_owned()
    }

    fn unix() -> Origin {
        Origin::Unix("/data/remote/run/listener.sock".into())
    }

    fn windows_origin() -> Origin {
        Origin::Tcp {
            port: 5000,
            key: "s3cret-key-value-0123".into(),
        }
    }

    fn state(line: &str) -> Option<TransportState> {
        match read_event(line)? {
            Seen::State(s) => Some(s),
            _ => None,
        }
    }

    #[test]
    fn args_exact() {
        assert_eq!(
            helper_args(&dirs(), "nocturne-ab12", &unix()),
            [
                "--state-dir",
                under(&["tailscale"]).as_str(),
                "--hostname",
                "nocturne-ab12",
                "--target",
                "unix:/data/remote/run/listener.sock",
            ]
        );
        assert_eq!(
            helper_args(&dirs(), "nocturne-ab12", &windows_origin())[5],
            "http://127.0.0.1:5000"
        );
        let allowed = allowed_flags_for(HelperKind::Tailnet, "help");
        for origin in [unix(), windows_origin()] {
            for a in helper_args(&dirs(), "nocturne-ab12", &origin) {
                if a.starts_with("--") {
                    assert!(allowed.contains(&a.as_str()), "{a} is not in the fixture");
                }
            }
        }
    }

    #[test]
    fn env_scrubbed_and_key_only_on_input() {
        let env = helper_env(&dirs());
        let keys: Vec<_> = env
            .iter()
            .map(|(k, _)| k.to_string_lossy().into_owned())
            .collect();
        assert!(
            keys.iter()
                .all(|k| ["PATH", "HOME", "TMPDIR", "SystemRoot"].contains(&k.as_str()))
        );
        assert!(
            !keys
                .iter()
                .any(|k| k.to_ascii_uppercase().starts_with("TS"))
        );
        let origin = windows_origin();
        let args = helper_args(&dirs(), "nocturne-ab12", &origin).join(" ");
        assert!(!args.contains("s3cret"));
        let line = key_line(&origin).unwrap();
        assert_eq!(line.0, r#"{"key":"s3cret-key-value-0123"}"#);
        assert_eq!(format!("{line:?}"), "<hidden>");
        assert!(key_line(&unix()).is_none());
    }

    #[test]
    fn every_event_maps() {
        assert_eq!(
            state(r#"{"state":"starting"}"#),
            Some(TransportState::Starting)
        );
        assert_eq!(
            state(r#"{"state":"needsLogin","authUrl":"https://login.tailscale.com/a/x1"}"#),
            Some(TransportState::NeedsSignIn {
                url: "https://login.tailscale.com/a/x1".into(),
                allow_links: false
            })
        );
        assert_eq!(
            state(r#"{"state":"needsLogin","authUrl":"https://controlplane.tailscale.com/a/x2"}"#),
            Some(TransportState::NeedsSignIn {
                url: "https://controlplane.tailscale.com/a/x2".into(),
                allow_links: false
            })
        );
        for bad in [
            "http://login.tailscale.com/a/x",
            "https://evil.example/a/x",
            "https://login.tailscale.com.evil.example/a/x",
            "https://user@login.tailscale.com/a/x",
            "https://login.tailscale.com:8443/a/x",
            "https://login.tailscale.com/a/x y",
        ] {
            let line = serde_json::json!({"state": "needsLogin", "authUrl": bad}).to_string();
            assert_eq!(state(&line), None, "{bad}");
        }
        assert_eq!(
            state(r#"{"state":"needsHttps"}"#),
            Some(TransportState::NeedsSignIn {
                url: HTTPS_URL.into(),
                allow_links: true
            })
        );
        assert_eq!(
            state(r#"{"state":"needsApproval"}"#),
            Some(TransportState::NeedsApproval {
                url: APPROVE_URL.into()
            })
        );
        assert_eq!(
            state(r#"{"state":"running","host":"nocturne-ab12.tail-x.ts.net"}"#),
            Some(TransportState::Up {
                address: "https://nocturne-ab12.tail-x.ts.net".into(),
                route: None
            })
        );
        assert_eq!(state(r#"{"state":"running","host":"a b"}"#), None);
        for reason in ["key", "input", "start", "listen"] {
            let line = format!(r#"{{"state":"error","reason":"{reason}"}}"#);
            assert!(
                matches!(read_event(&line), Some(Seen::Fatal(HelperError::Exited(_)))),
                "{reason}"
            );
        }
        assert_eq!(
            read_event(r#"{"state":"error","reason":"blocked"}"#),
            Some(Seen::Fatal(HelperError::AccountBlocked))
        );
        assert!(matches!(
            read_event(r#"{"state":"error","reason":"login"}"#),
            Some(Seen::Fatal(HelperError::SignIn(_)))
        ));
        assert_eq!(
            read_event(r#"{"state":"error","reason":"certificate"}"#),
            Some(Seen::Certificate)
        );
        assert_eq!(
            read_event(r#"{"state":"error","reason":"logout"}"#),
            Some(Seen::LogoutFailed)
        );
        assert_eq!(read_event(r#"{"state":"stopped"}"#), Some(Seen::Stopped));
        assert_eq!(
            read_event(r#"{"state":"error","reason":"backend"}"#),
            Some(Seen::Transient)
        );
        for junk in [
            "",
            "not json",
            r#"{"state":"error","reason":"new"}"#,
            r#"{"state":"running","host":"h.ts.net","extra":1}"#,
        ] {
            assert_eq!(read_event(junk), None, "{junk}");
        }
    }

    #[test]
    fn restart_only_after_a_crash_or_an_outside_stop() {
        let lines = |l: &[&str]| l.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let started = lines(&[r#"{"state":"starting"}"#]);
        assert!(restart_after(None, &started, false), "killed by a signal");
        assert!(restart_after(Some(1), &started, false));
        assert!(
            restart_after(Some(2), &started, false),
            "a Go panic after starting"
        );
        assert!(!restart_after(Some(2), &[], false), "a refusal");
        let stopped = lines(&[r#"{"state":"starting"}"#, r#"{"state":"stopped"}"#]);
        assert!(
            restart_after(Some(0), &stopped, false),
            "stopped from outside"
        );
        assert!(!restart_after(Some(0), &stopped, true), "signed out");
        let failed = lines(&[
            r#"{"state":"error","reason":"start"}"#,
            r#"{"state":"stopped"}"#,
        ]);
        assert!(!restart_after(Some(1), &failed, false));
        let key = lines(&[r#"{"state":"error","reason":"key"}"#]);
        assert!(!restart_after(Some(1), &key, false));
        let backend = lines(&[
            r#"{"state":"error","reason":"backend"}"#,
            r#"{"state":"stopped"}"#,
        ]);
        assert!(restart_after(Some(1), &backend, false), "the exit decides");
        assert!(
            restart_after(Some(0), &started, false),
            "exit without stopped"
        );
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
    }
}
