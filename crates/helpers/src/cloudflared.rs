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
    /// How often the helper's output is checked while waiting for the address.
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
/// `/usr/local/etc/cloudflared`. Written by Nocturne; with a Unix socket it holds the one
/// ingress rule that names the socket.
pub(crate) fn config_path(dirs: &HelperDirs) -> PathBuf {
    home(dirs).join("config.yml")
}

const CONFIG_HEAD: &str =
    "# Written by Nocturne. It keeps the helper from reading any other configuration.\n";

/// A YAML double-quoted string.
fn yaml_quoted(text: &str) -> String {
    let mut out = String::from('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// What the configuration file says. cloudflared starts a quick tunnel only with `--url`, and
/// its `--unix-socket` flag alone is refused, so a Unix socket is the configuration's single
/// catch-all ingress rule, which cloudflared prefers over `--url`. A loopback port keeps the
/// command-line origin, where the helper key travels as the Host header.
pub(crate) fn config_text(origin: &Origin) -> String {
    match origin {
        Origin::Unix(path) => format!(
            "{CONFIG_HEAD}ingress:\n  - service: {}\n",
            yaml_quoted(&format!("unix:{}", path.to_string_lossy()))
        ),
        Origin::Tcp { .. } => CONFIG_HEAD.to_string(),
    }
}

/// The exact command line, after `cloudflared`. The helper picks its own metrics port and names
/// it in its output, so no other program can be waiting on a port chosen for it.
pub(crate) fn tunnel_args(origin: &Origin, config: &std::path::Path) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "tunnel".into(),
        "--no-autoupdate".into(),
        "--config".into(),
        config.to_string_lossy().into_owned(),
        "--metrics".into(),
        "127.0.0.1:0".into(),
        "--loglevel".into(),
        "info".into(),
    ];
    if allowed_flags_for(HelperKind::Cloudflared, "tunnel").contains(&"--management-diagnostics") {
        args.push("--management-diagnostics=false".into());
    }
    args.push("--url".into());
    match origin {
        // Asks for a quick tunnel; the ingress rule in the configuration picks the socket. Were
        // the rule ever ignored, cloudflared would read this as a host named `unix` and fail: it
        // names no loopback port another program could hold.
        Origin::Unix(path) => args.push(format!("unix:{}", path.to_string_lossy())),
        Origin::Tcp { port, .. } => args.push(format!("http://127.0.0.1:{port}")),
    }
    args
}

/// The exact environment: nothing is inherited.
pub(crate) fn tunnel_env(dirs: &HelperDirs) -> Vec<(OsString, OsString)> {
    let home = home(dirs);
    let env: Vec<(OsString, OsString)> = vec![
        ("PATH".into(), "".into()),
        ("HOME".into(), home.clone().into()),
        ("TMPDIR".into(), home.join("tmp").into()),
    ];
    #[cfg(windows)]
    let env = {
        let mut env = env;
        if let Some(root) = std::env::var_os("SystemRoot") {
            env.push(("SystemRoot".into(), root));
        }
        env
    };
    env
}

/// The helper key, when there is one, travels in the environment as the Host the origin sees,
/// never in the arguments.
pub(crate) fn tunnel_secret_env(origin: &Origin) -> Vec<(OsString, OsString)> {
    match origin {
        Origin::Tcp { key, .. } => vec![("TUNNEL_HTTP_HOST_HEADER".into(), key.into())],
        Origin::Unix(_) => Vec::new(),
    }
}

fn write_config(path: &std::path::Path, origin: &Origin) -> Result<(), HelperError> {
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
        .and_then(|mut f| f.write_all(config_text(origin).as_bytes()))
        .map_err(|e| HelperError::Spawn(format!("could not write the settings file: {e}")))
}

/// A quick tunnel's own name: `<letters, digits and hyphens>.trycloudflare.com`, nothing else.
pub fn is_quick_tunnel_host(host: &str) -> bool {
    host.strip_suffix(".trycloudflare.com")
        .is_some_and(|label| {
            !label.is_empty()
                && label
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
}

/// The loopback metrics port the helper says it listens on, from its own output.
fn metrics_port(output: &str) -> Option<u16> {
    const MARK: &str = "Starting metrics server on ";
    let at = output.rfind(MARK)? + MARK.len();
    let addr = output[at..].split(['/', ' ', '\n']).next()?;
    let addr: std::net::SocketAddr = addr.trim().parse().ok()?;
    (addr.ip().is_loopback() && addr.port() != 0).then_some(addr.port())
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
            write_config(&config_path(&self.dirs), &spec.origin)?;
            let cmd = HelperCommand {
                name: "cloudflared",
                program: self.installed.path.clone(),
                sha256: self.installed.sha256.clone(),
                args: tunnel_args(&spec.origin, &config_path(&self.dirs))
                    .into_iter()
                    .map(OsString::from)
                    .collect(),
                env: tunnel_env(&self.dirs),
                secret_env: tunnel_secret_env(&spec.origin),
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
                    found = wait_ready(&log, now.log_offset, &opts) => match found {
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

/// The public address once the tunnel is ready, read again from each run's own output.
async fn wait_ready(
    log: &std::path::Path,
    offset: u64,
    opts: &CloudflaredOptions,
) -> Option<String> {
    let deadline = Instant::now() + opts.ready_within;
    loop {
        if let Some(address) = ready_once(log, offset).await {
            return Some(address);
        }
        if Instant::now() >= deadline {
            return None;
        }
        tokio::time::sleep(opts.poll).await;
    }
}

/// The address comes only from what this run of the helper printed to its owner-only log, and
/// only as a quick tunnel's name. The metrics port it names there may confirm it is ready; when
/// that port names another address, something else is answering on it and neither is used.
async fn ready_once(log: &std::path::Path, offset: u64) -> Option<String> {
    let own_output = std::fs::read(log)
        .map(|b| String::from_utf8_lossy(b.get(offset as usize..).unwrap_or_default()).into_owned())
        .unwrap_or_default();
    let address = last_https_host(&own_output, ".trycloudflare.com")?;
    let host = address.strip_prefix("https://")?;
    if !is_quick_tunnel_host(host) {
        return None;
    }
    let metrics = metrics_port(&own_output);
    if let Some(port) = metrics
        && let Some((200, body)) = local_get(port, "/quicktunnel").await
    {
        let named = serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .and_then(|v| v.get("hostname")?.as_str().map(|h| h.trim().to_string()));
        if named.as_deref() != Some(host) {
            return None;
        }
    }
    let ready = own_output.contains("Registered tunnel connection")
        || match metrics {
            Some(port) => matches!(local_get(port, "/ready").await, Some((200, _))),
            None => false,
        };
    ready.then_some(address)
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn args_are_exact() {
        let cfg = config_path(&dirs());
        let unix = tunnel_args(&Origin::Unix("/data/remote/run/listener.sock".into()), &cfg);
        let config = under(&["cloudflared", "config.yml"]);
        let mut expected: Vec<String> = [
            "tunnel",
            "--no-autoupdate",
            "--config",
            config.as_str(),
            "--metrics",
            "127.0.0.1:0",
            "--loglevel",
            "info",
            "--management-diagnostics=false",
            "--url",
            "unix:/data/remote/run/listener.sock",
        ]
        .map(String::from)
        .to_vec();
        assert_eq!(unix, expected);
        let tcp = tunnel_args(
            &Origin::Tcp {
                port: 5000,
                key: "k".into(),
            },
            &cfg,
        );
        expected.truncate(10);
        expected.push("http://127.0.0.1:5000".into());
        assert_eq!(tcp, expected);
    }

    #[test]
    fn a_unix_socket_is_the_one_ingress_rule() {
        let text = config_text(&Origin::Unix(
            "/data/a b/Application Support/run/l\"q\\.sock".into(),
        ));
        assert_eq!(
            text,
            format!(
                "{CONFIG_HEAD}ingress:\n  - service: \"unix:/data/a b/Application Support/run/l\\\"q\\\\.sock\"\n"
            )
        );
        let tcp = config_text(&Origin::Tcp {
            port: 5000,
            key: "s3cret".into(),
        });
        assert_eq!(
            tcp, CONFIG_HEAD,
            "the key and the port stay out of the file"
        );
        assert_eq!(
            yaml_quoted("a\nb"),
            "\"a\\u000ab\"",
            "no line break reaches the file"
        );
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
            for arg in tunnel_args(origin, std::path::Path::new("/c.yml")) {
                if arg.starts_with("--") {
                    let name = arg.split('=').next().unwrap();
                    assert!(allowed.contains(&name), "{name} is not in the fixture");
                }
            }
        }
    }

    #[test]
    fn a_unix_origin_has_no_secret_environment() {
        assert!(tunnel_secret_env(&Origin::Unix("/s".into())).is_empty());
    }

    #[test]
    fn windows_key_by_environment_not_argv() {
        let origin = Origin::Tcp {
            port: 5000,
            key: "s3cret-key-value".into(),
        };
        assert!(
            !tunnel_args(&origin, std::path::Path::new("/c.yml"))
                .iter()
                .any(|a| a.contains("s3cret-key-value"))
        );
        let env = tunnel_secret_env(&origin);
        assert!(env.contains(&(
            OsString::from("TUNNEL_HTTP_HOST_HEADER"),
            OsString::from("s3cret-key-value")
        )));
    }

    #[test]
    fn only_a_quick_tunnel_name_is_an_address() {
        assert!(is_quick_tunnel_host("amber-otter-42.trycloudflare.com"));
        for bad in [
            "trycloudflare.com",
            ".trycloudflare.com",
            "a.b.trycloudflare.com",
            "Amber.trycloudflare.com",
            "amber_otter.trycloudflare.com",
            "amber.trycloudflare.com.evil.example",
            "amber.trycloudflare.com:443",
            "evil.example/x.trycloudflare.com",
        ] {
            assert!(!is_quick_tunnel_host(bad), "{bad}");
        }
    }

    #[test]
    fn metrics_port_from_own_output() {
        let out = "INF Starting metrics server on 127.0.0.1:41234/metrics\nINF other";
        assert_eq!(metrics_port(out), Some(41234));
        assert_eq!(
            metrics_port("INF Starting metrics server on 192.0.2.1:41234/metrics"),
            None
        );
        assert_eq!(metrics_port("nothing here"), None);
    }

    #[test]
    fn own_home_not_the_persons() {
        let env = tunnel_env(&dirs());
        let get = |k: &str| {
            env.iter()
                .find(|(key, _)| key == k)
                .map(|(_, v)| v.to_string_lossy().into_owned())
        };
        assert_eq!(get("HOME"), Some(under(&["cloudflared"])));
        assert_eq!(get("PATH").as_deref(), Some(""));
        assert_eq!(get("TMPDIR"), Some(under(&["cloudflared", "tmp"])));
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
