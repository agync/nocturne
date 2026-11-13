//! The Simple connection: a Cloudflare quick tunnel run by `cloudflared`, with its own home under
//! `<data>/remote/`, no auto-update and no log level above `info`.

use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use crate::fixtures::allowed_flags_for;
use crate::install::make_private_dir;
use crate::supervise::{HelperCommand, Phase, SupState, SuperviseOptions, Supervisor};
use crate::util::{last_https_host, local_get};
use crate::{
    BoxFuture, HelperDirs, HelperError, HelperInfo, HelperKind, Installed, KeyCarrier, LaunchSpec,
    Origin, Transport, TransportState, key_carrier, verify,
};

#[derive(Debug, Clone)]
pub struct CloudflaredOptions {
    /// How often the metrics port and the log are checked while waiting for the address.
    pub poll: Duration,
    /// How long a start may take before it is reported as failed.
    pub ready_within: Duration,
    pub backoff: Option<Vec<Duration>>,
    pub grace: Option<Duration>,
}

impl Default for CloudflaredOptions {
    fn default() -> Self {
        Self {
            poll: Duration::from_millis(500),
            ready_within: Duration::from_secs(90),
            backoff: None,
            grace: None,
        }
    }
}

pub struct CloudflaredTransport {
    dirs: HelperDirs,
    installed: Installed,
    opts: CloudflaredOptions,
    run: Mutex<Option<Run>>,
}

struct Run {
    sup: Supervisor,
    cancel: CancellationToken,
    monitor: JoinHandle<()>,
    state: Arc<watch::Sender<TransportState>>,
    sup_state: watch::Receiver<SupState>,
}

impl CloudflaredTransport {
    pub fn new(dirs: &HelperDirs, installed: &Installed) -> Self {
        Self::with_options(dirs, installed, CloudflaredOptions::default())
    }

    pub fn with_options(
        dirs: &HelperDirs,
        installed: &Installed,
        opts: CloudflaredOptions,
    ) -> Self {
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
}

/// The helper's own home, never the person's.
pub(crate) fn home(dirs: &HelperDirs) -> PathBuf {
    dirs.root.join("cloudflared")
}

/// The helper's own configuration file, which stops it reading `/etc/cloudflared` or
/// `/usr/local/etc/cloudflared`. Written by Nocturne and empty of settings.
pub(crate) fn config_path(dirs: &HelperDirs) -> PathBuf {
    home(dirs).join("config.yml")
}

const CONFIG_TEXT: &str =
    "# Written by Nocturne. It keeps the helper from reading any other configuration.\n";

/// The exact command line, after `cloudflared`.
pub(crate) fn tunnel_args(
    origin: &Origin,
    metrics_port: u16,
    config: &std::path::Path,
) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "tunnel".into(),
        "--no-autoupdate".into(),
        "--config".into(),
        config.to_string_lossy().into_owned(),
        "--metrics".into(),
        format!("127.0.0.1:{metrics_port}"),
        "--loglevel".into(),
        "info".into(),
    ];
    if allowed_flags_for(HelperKind::Cloudflared, "tunnel").contains(&"--management-diagnostics") {
        args.push("--management-diagnostics=false".into());
    }
    match origin {
        Origin::Unix(path) => {
            args.push("--unix-socket".into());
            args.push(path.to_string_lossy().into_owned());
        }
        Origin::Tcp { port, .. } => {
            args.push("--url".into());
            args.push(format!("http://127.0.0.1:{port}"));
        }
    }
    args
}

/// The exact environment: nothing is inherited. The helper key, when there is one, travels here
/// as the Host the origin sees, never in the arguments.
pub(crate) fn tunnel_env(dirs: &HelperDirs, origin: &Origin) -> Vec<(OsString, OsString)> {
    let home = home(dirs);
    let mut env: Vec<(OsString, OsString)> = vec![
        ("PATH".into(), "".into()),
        ("HOME".into(), home.clone().into()),
        ("TMPDIR".into(), home.join("tmp").into()),
    ];
    #[cfg(windows)]
    if let Some(root) = std::env::var_os("SystemRoot") {
        env.push(("SystemRoot".into(), root));
    }
    if let Origin::Tcp { key, .. } = origin {
        env.push(("TUNNEL_HTTP_HOST_HEADER".into(), key.into()));
    }
    env
}

fn write_config(path: &std::path::Path) -> Result<(), HelperError> {
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
        .and_then(|mut f| f.write_all(CONFIG_TEXT.as_bytes()))
        .map_err(|e| HelperError::Spawn(format!("could not write the settings file: {e}")))
}

fn free_port() -> Result<u16, HelperError> {
    std::net::TcpListener::bind(("127.0.0.1", 0))
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .map_err(|e| HelperError::Spawn(format!("no free port: {e}")))
}

impl Transport for CloudflaredTransport {
    fn kind(&self) -> HelperKind {
        HelperKind::Cloudflared
    }

    fn key_carrier(&self) -> KeyCarrier {
        key_carrier(HelperKind::Cloudflared)
    }

    fn start(
        &self,
        spec: LaunchSpec,
    ) -> BoxFuture<'_, Result<watch::Receiver<TransportState>, HelperError>> {
        Box::pin(async move {
            if let Some(old) = self.take_run() {
                shut(old).await?;
            }
            verify(&self.installed)?;
            let home = home(&self.dirs);
            let logs = self.dirs.root.join("logs");
            make_private_dir(&self.dirs.root)?;
            make_private_dir(&home)?;
            make_private_dir(&home.join("tmp"))?;
            make_private_dir(&logs)?;
            write_config(&config_path(&self.dirs))?;
            let metrics = free_port()?;
            let cmd = HelperCommand {
                name: "cloudflared",
                program: self.installed.path.clone(),
                sha256: self.installed.sha256.clone(),
                args: tunnel_args(&spec.origin, metrics, &config_path(&self.dirs))
                    .into_iter()
                    .map(OsString::from)
                    .collect(),
                env: tunnel_env(&self.dirs, &spec.origin),
                cwd: Some(home),
            };
            let mut sup_opts = SuperviseOptions::new(logs);
            if let Some(b) = &self.opts.backoff {
                sup_opts.backoff = b.clone();
            }
            if let Some(g) = self.opts.grace {
                sup_opts.grace = g;
            }
            let sup = Supervisor::start(cmd, sup_opts).await?;
            let (tx, rx) = watch::channel(TransportState::Starting);
            let tx = Arc::new(tx);
            let cancel = CancellationToken::new();
            let sup_state = sup.subscribe();
            let monitor = tokio::spawn(monitor(
                metrics,
                sup.log_path().to_path_buf(),
                sup_state.clone(),
                tx.clone(),
                cancel.clone(),
                self.opts.clone(),
            ));
            *self.run.lock().expect("run lock") = Some(Run {
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
                Some(run) => shut(run).await,
                None => Ok(()),
            }
        })
    }

    fn sign_out(&self) -> BoxFuture<'_, Result<(), HelperError>> {
        Box::pin(async { Ok(()) })
    }

    fn info(&self) -> Option<HelperInfo> {
        let guard = self.run.lock().expect("run lock");
        let run = guard.as_ref()?;
        let s = run.sup_state.borrow().clone();
        Some(HelperInfo {
            name: "cloudflared",
            version: self.installed.version.clone(),
            pid: s.pid,
            uptime: s.started.map(|t| t.elapsed()),
            restarts: s.restarts,
            rss_bytes: s.pid.and_then(crate::supervise::rss_bytes),
        })
    }
}

async fn shut(run: Run) -> Result<(), HelperError> {
    run.cancel.cancel();
    let _ = run.monitor.await;
    let result = run.sup.stop().await;
    run.state.send_replace(TransportState::Stopped);
    result
}

async fn monitor(
    metrics: u16,
    log: PathBuf,
    mut sup: watch::Receiver<SupState>,
    tx: Arc<watch::Sender<TransportState>>,
    cancel: CancellationToken,
    opts: CloudflaredOptions,
) {
    loop {
        let now = sup.borrow_and_update().clone();
        match now.phase {
            Phase::Stopped => return,
            Phase::GaveUp(message) => {
                tx.send_replace(TransportState::Failed {
                    error: HelperError::Exited(message),
                });
                return;
            }
            Phase::Ended(code) => {
                tx.send_replace(TransportState::Failed {
                    error: HelperError::Exited(format!("the helper ended ({code:?})")),
                });
                return;
            }
            Phase::Backoff { attempt } => {
                tx.send_replace(TransportState::Restarting { attempt });
            }
            Phase::Running => {
                tokio::select! {
                    found = wait_ready(metrics, &log, now.log_offset, &opts) => match found {
                        Some(address) => {
                            tx.send_replace(TransportState::Up { address, route: None });
                        }
                        None => {
                            tx.send_replace(TransportState::Failed {
                                error: HelperError::Exited("the connection did not come up".into()),
                            });
                            return;
                        }
                    },
                    _ = sup.changed() => continue,
                    () = cancel.cancelled() => return,
                }
            }
        }
        tokio::select! {
            r = sup.changed() => if r.is_err() { return },
            () = cancel.cancelled() => return,
        }
    }
}

/// The public address once the tunnel is ready: from the metrics port when it answers, else from
/// the line the helper prints.
async fn wait_ready(
    metrics: u16,
    log: &std::path::Path,
    offset: u64,
    opts: &CloudflaredOptions,
) -> Option<String> {
    let deadline = Instant::now() + opts.ready_within;
    loop {
        if let Some(address) = ready_once(metrics, log, offset).await {
            return Some(address);
        }
        if Instant::now() >= deadline {
            return None;
        }
        tokio::time::sleep(opts.poll).await;
    }
}

async fn ready_once(metrics: u16, log: &std::path::Path, offset: u64) -> Option<String> {
    let own_output = std::fs::read(log)
        .map(|b| String::from_utf8_lossy(b.get(offset as usize..).unwrap_or_default()).into_owned())
        .unwrap_or_default();
    match local_get(metrics, "/quicktunnel").await {
        Some((200, body)) => {
            let host = serde_json::from_str::<serde_json::Value>(&body)
                .ok()?
                .get("hostname")?
                .as_str()?
                .trim()
                .to_string();
            if host.is_empty() {
                return None;
            }
            let ready = local_get(metrics, "/ready").await;
            matches!(ready, Some((200, _))).then(|| format!("https://{host}"))
        }
        Some(_) => None,
        None => own_output
            .contains("Registered tunnel connection")
            .then(|| last_https_host(&own_output, ".trycloudflare.com"))
            .flatten(),
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

    #[test]
    fn args_are_exact() {
        let cfg = config_path(&dirs());
        let unix = tunnel_args(
            &Origin::Unix("/data/remote/run/listener.sock".into()),
            4321,
            &cfg,
        );
        let mut expected: Vec<String> = [
            "tunnel",
            "--no-autoupdate",
            "--config",
            "/data/remote/cloudflared/config.yml",
            "--metrics",
            "127.0.0.1:4321",
            "--loglevel",
            "info",
            "--management-diagnostics=false",
            "--unix-socket",
            "/data/remote/run/listener.sock",
        ]
        .map(String::from)
        .to_vec();
        assert_eq!(unix, expected);
        let tcp = tunnel_args(
            &Origin::Tcp {
                port: 5000,
                key: "k".into(),
            },
            4321,
            &cfg,
        );
        expected.truncate(9);
        expected.extend(["--url".into(), "http://127.0.0.1:5000".into()]);
        assert_eq!(tcp, expected);
    }

    #[test]
    fn every_flag_used_is_in_the_fixture() {
        let allowed = allowed_flags_for(HelperKind::Cloudflared, "tunnel");
        let origins = [
            Origin::Unix("/s".into()),
            Origin::Tcp {
                port: 1,
                key: "k".into(),
            },
        ];
        for origin in &origins {
            for arg in tunnel_args(origin, 1, std::path::Path::new("/c.yml")) {
                if arg.starts_with("--") {
                    let name = arg.split('=').next().unwrap();
                    assert!(allowed.contains(&name), "{name} is not in the fixture");
                }
            }
        }
    }

    #[test]
    fn windows_key_by_environment_not_argv() {
        let origin = Origin::Tcp {
            port: 5000,
            key: "s3cret-key-value".into(),
        };
        assert!(
            !tunnel_args(&origin, 1, std::path::Path::new("/c.yml"))
                .iter()
                .any(|a| a.contains("s3cret-key-value"))
        );
        let env = tunnel_env(&dirs(), &origin);
        assert!(env.contains(&(
            OsString::from("TUNNEL_HTTP_HOST_HEADER"),
            OsString::from("s3cret-key-value")
        )));
    }

    #[test]
    fn own_home_not_the_persons() {
        let env = tunnel_env(&dirs(), &Origin::Unix("/s".into()));
        let get = |k: &str| {
            env.iter()
                .find(|(key, _)| key == k)
                .map(|(_, v)| v.to_string_lossy().into_owned())
        };
        assert_eq!(get("HOME").as_deref(), Some("/data/remote/cloudflared"));
        assert_eq!(get("PATH").as_deref(), Some(""));
        assert_eq!(
            get("TMPDIR").as_deref(),
            Some("/data/remote/cloudflared/tmp")
        );
        let keys: Vec<_> = env
            .iter()
            .map(|(k, _)| k.to_string_lossy().into_owned())
            .collect();
        assert!(
            keys.iter()
                .all(|k| ["PATH", "HOME", "TMPDIR", "SystemRoot"].contains(&k.as_str()))
        );
    }
}
