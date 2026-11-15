//! Test parts for Online answers: a keychain in memory, an engine port that points a client at
//! the stand-in provider and notes each route it is given (never the key itself), and the
//! stand-in's process. Nothing here reaches the system keychain or the network.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use nocturne_engine::BoxFuture;
use nocturne_lemonade::Client;
use nocturne_lemonade::online::CloudRoute;
use zeroize::Zeroizing;

use super::keys::{KeyError, Keychain};
use super::route::EnginePort;

/// A keychain in memory that can be made unavailable.
#[derive(Default)]
pub struct MemoryKeychain {
    stored: Mutex<Option<String>>,
    pub unavailable: AtomicBool,
}

impl MemoryKeychain {
    pub fn unavailable() -> Self {
        let k = Self::default();
        k.unavailable.store(true, Ordering::SeqCst);
        k
    }

    /// Whether a key is kept here.
    pub fn holds_a_key(&self) -> bool {
        self.stored
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some()
    }

    fn down(&self) -> bool {
        self.unavailable.load(Ordering::SeqCst)
    }
}

impl Keychain for MemoryKeychain {
    fn get(&self) -> Result<Option<Zeroizing<String>>, KeyError> {
        if self.down() {
            return Err(KeyError::Unavailable);
        }
        Ok(self
            .stored
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .map(Zeroizing::new))
    }

    fn set(&self, key: &str) -> Result<(), KeyError> {
        if self.down() {
            return Err(KeyError::Unavailable);
        }
        *self.stored.lock().unwrap_or_else(|e| e.into_inner()) = Some(key.to_string());
        Ok(())
    }

    fn delete(&self) -> Result<(), KeyError> {
        if self.down() {
            return Err(KeyError::Unavailable);
        }
        *self.stored.lock().unwrap_or_else(|e| e.into_inner()) = None;
        Ok(())
    }
}

/// A route as the engine was given it, without the key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteSeen {
    pub base_url: String,
    pub model: String,
    pub with_key: bool,
}

/// A confirmation that always agrees, and notes the addresses it was asked about.
pub fn agree(asked: Arc<Mutex<Vec<String>>>) -> super::controller::ConfirmFn {
    confirm_with(asked, true)
}

/// A confirmation that answers `yes`, and notes the addresses it was asked about.
pub fn confirm_with(asked: Arc<Mutex<Vec<String>>>, yes: bool) -> super::controller::ConfirmFn {
    Arc::new(move |address: &str| {
        asked
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(address.to_string());
        Box::pin(async move { yes })
    })
}

/// Holds one relaunch after the engine took its route, as the supervisor does while the new
/// engine starts.
#[derive(Default)]
pub struct RelaunchHold {
    /// Notified once the engine has the route and the relaunch waits.
    pub reached: tokio::sync::Notify,
    /// Lets the relaunch finish.
    pub go: tokio::sync::Notify,
}

/// An engine port whose client talks to the stand-in provider directly, as the engine would.
/// A route to `http://127.0.0.1:<port>` (builds with the stand-in feature) moves the client to
/// the stand-in on that port.
#[derive(Default)]
pub struct StandInPort {
    client: Mutex<Option<Client>>,
    seen: Mutex<Vec<Option<RouteSeen>>>,
    has: Mutex<Option<bool>>,
    pub downloading: AtomicBool,
    /// While set, every relaunch fails, and the engine is left without a route, as the
    /// supervisor leaves it.
    pub fail: AtomicBool,
    /// While set, the engine starts but does not take a route: its key request fails, and the
    /// relaunch says so, as the supervisor does.
    pub refuse: AtomicBool,
    /// While set, the engine is between launches.
    pub starting: AtomicBool,
    taken: Mutex<Option<bool>>,
    calls: std::sync::atomic::AtomicUsize,
    hold: Mutex<Option<Arc<RelaunchHold>>>,
    /// The engine key this port's client sends, its own, so a test can tell its requests from
    /// those of another test's port that once used the same loopback port.
    key: String,
}

impl StandInPort {
    /// A port for a stand-in on `port`; `None` for no engine at all.
    pub fn new(port: Option<u16>) -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let key = format!(
            "engine-key-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        );
        StandInPort {
            client: Mutex::new(port.map(|p| Client::new(p, key.clone()))),
            key,
            ..Self::default()
        }
    }

    /// The engine key this port's client sends.
    pub fn engine_key(&self) -> &str {
        &self.key
    }

    /// `requests` as the stand-in logged them, keeping only those that prove they are this
    /// lab's: the engine key is this port's own, or one of `extra_keys` the lab typed. A
    /// finished test's client, or an unrelated process, can reach a stand-in that was given its
    /// freed loopback port, and sends no key or another one.
    pub fn own_with(
        &self,
        extra_keys: &[&str],
        requests: Vec<serde_json::Value>,
    ) -> Vec<serde_json::Value> {
        let mut accepted = vec![format!("Bearer {}", self.key)];
        accepted.extend(extra_keys.iter().map(|k| format!("Bearer {k}")));
        requests
            .into_iter()
            .filter(|r| {
                r["headers"]["authorization"]
                    .as_str()
                    .is_some_and(|a| accepted.iter().any(|m| m == a))
                    && r["path"].as_str().is_some_and(|p| p.starts_with("/v1/"))
                    && r["body"]["model"]
                        .as_str()
                        .is_none_or(|m| m == "stand-in-model" || m == "user.nocturne-online")
            })
            .collect()
    }

    /// `requests` that carry this port's own engine key.
    pub fn own(&self, requests: Vec<serde_json::Value>) -> Vec<serde_json::Value> {
        self.own_with(&[], requests)
    }

    /// Every route given to the engine, in order (`None` for none).
    pub fn routes(&self) -> Vec<Option<RouteSeen>> {
        self.seen.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// The route the engine has now.
    pub fn current(&self) -> Option<RouteSeen> {
        self.routes().last().cloned().flatten()
    }

    /// How many times the engine was asked to relaunch with a route or none.
    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }

    /// Holds the next relaunch once the engine has its route, until `go` or until the hub
    /// stops waiting for it.
    pub fn hold_next_relaunch(&self) -> Arc<RelaunchHold> {
        let hold = Arc::new(RelaunchHold::default());
        *self.hold.lock().unwrap_or_else(|e| e.into_inner()) = Some(hold.clone());
        hold
    }

    /// What the running engine reports about its route, for a launch the hub did not make.
    pub fn set_has_cloud(&self, has: Option<bool>) {
        *self.has.lock().unwrap_or_else(|e| e.into_inner()) = has;
        *self.taken.lock().unwrap_or_else(|e| e.into_inner()) = has;
    }

    /// Whether the running engine holds its route, for a launch the hub did not make (a
    /// restart whose key request failed).
    pub fn set_route_taken(&self, taken: Option<bool>) {
        *self.taken.lock().unwrap_or_else(|e| e.into_inner()) = taken;
    }
}

impl EnginePort for StandInPort {
    fn set_cloud(&self, route: Option<CloudRoute>) -> BoxFuture<'_, Result<(), String>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.fail.load(Ordering::SeqCst) {
            *self.has.lock().unwrap_or_else(|e| e.into_inner()) = Some(false);
            *self.taken.lock().unwrap_or_else(|e| e.into_inner()) = Some(false);
            self.seen
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(None);
            return Box::pin(async { Err("the engine did not start".to_string()) });
        }
        let seen = route.as_ref().map(|r| RouteSeen {
            base_url: r.base_url().to_string(),
            model: r.model.clone(),
            with_key: !r.key.expose().is_empty(),
        });
        *self.has.lock().unwrap_or_else(|e| e.into_inner()) = Some(seen.is_some());
        let refused = seen.is_some() && self.refuse.load(Ordering::SeqCst);
        *self.taken.lock().unwrap_or_else(|e| e.into_inner()) = Some(seen.is_some() && !refused);
        // A route to a stand-in on loopback is followed there, as the engine would.
        if let Some(r) = &route
            && r.base_url().host_str() == Some("127.0.0.1")
            && let Some(port) = r.base_url().port()
        {
            *self.client.lock().unwrap_or_else(|e| e.into_inner()) =
                Some(Client::new(port, self.key.clone()));
        }
        self.seen
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(seen);
        let hold = self.hold.lock().unwrap_or_else(|e| e.into_inner()).take();
        Box::pin(async move {
            if let Some(h) = hold {
                h.reached.notify_one();
                h.go.notified().await;
            }
            if refused {
                return Err("the engine did not take the route for Online answers".to_string());
            }
            Ok(())
        })
    }

    fn client(&self) -> BoxFuture<'_, Option<Client>> {
        let c = self
            .client
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        Box::pin(async move { c })
    }

    fn downloading(&self) -> BoxFuture<'_, bool> {
        let d = self.downloading.load(Ordering::SeqCst);
        Box::pin(async move { d })
    }

    fn has_cloud(&self) -> BoxFuture<'_, Option<bool>> {
        let h = *self.has.lock().unwrap_or_else(|e| e.into_inner());
        Box::pin(async move { h })
    }

    fn route_taken(&self) -> BoxFuture<'_, Option<bool>> {
        let t = *self.taken.lock().unwrap_or_else(|e| e.into_inner());
        Box::pin(async move { t })
    }

    fn starting(&self) -> BoxFuture<'_, bool> {
        let s = self.starting.load(Ordering::SeqCst);
        Box::pin(async move { s })
    }
}

/// The stand-in provider's process, stopped when dropped.
pub struct StandIn {
    child: Child,
    pub port: u16,
}

impl Drop for StandIn {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl StandIn {
    /// Starts the stand-in from `exe` on a free loopback port.
    pub fn start(exe: &Path) -> StandIn {
        use std::io::BufRead;
        let mut child = Command::new(exe)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("the stand-in starts");
        let mut line = String::new();
        std::io::BufReader::new(child.stdout.take().expect("its output"))
            .read_line(&mut line)
            .expect("it says where it listens");
        let port = line
            .trim()
            .strip_prefix("ready ")
            .and_then(|p| p.parse().ok())
            .expect("a port");
        StandIn { child, port }
    }

    pub fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{path}", self.port)
    }
}

/// The stand-in provider binary, built once for the test run in the profile of the test
/// binary that asks. `NOCTURNE_FAKE_PROVIDER` names a prebuilt one instead.
pub fn fake_provider() -> PathBuf {
    static BUILT: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    BUILT.get_or_init(build_fake_provider).clone()
}

fn build_fake_provider() -> PathBuf {
    if let Some(path) = std::env::var_os("NOCTURNE_FAKE_PROVIDER") {
        return PathBuf::from(path);
    }
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let mut build = Command::new(cargo);
    build
        .args(["build", "--locked", "-p", "nocturne-lemonade"])
        .args(["--features", "fake-server", "--bin", "fake-provider"])
        .args([
            "--profile",
            if cfg!(debug_assertions) {
                "dev"
            } else {
                "release"
            },
        ])
        .arg("--message-format=json-render-diagnostics");
    let out = build.stderr(Stdio::inherit()).output().expect("cargo runs");
    assert!(out.status.success(), "the stand-in is built");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|m| m["reason"] == "compiler-artifact" && m["target"]["name"] == "fake-provider")
        .find_map(|m| m["executable"].as_str().map(PathBuf::from))
        .expect("cargo says where the stand-in is")
}
