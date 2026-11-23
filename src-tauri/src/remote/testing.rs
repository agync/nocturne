//! A phone written in Rust, and fakes, for tests. Built only in debug builds: never shipped.
//!
//! [`PhonePair`] plays the phone's side of pairing frame by frame, through the app's own steps in
//! `nocturne_channel::phone`; [`PhoneKeys`] holds what a paired phone keeps and opens sessions;
//! the networked client is in `testing::client`.

use p256::SecretKey;

use nocturne_channel::phone::{PhoneIdentity, PhonePairing};

use super::crypto::{self, DeviceId};

/// The phone's side of one pairing: the app's own steps, which panic on a bad link or code.
pub struct PhonePair(pub PhonePairing);

impl std::ops::Deref for PhonePair {
    type Target = PhonePairing;
    fn deref(&self) -> &PhonePairing {
        &self.0
    }
}

impl std::ops::DerefMut for PhonePair {
    fn deref_mut(&mut self) -> &mut PhonePairing {
        &mut self.0
    }
}

impl PhonePair {
    /// From a scanned link: S and Cpub from the fragment.
    pub fn scanned(link: &str, name: &str) -> PhonePair {
        PhonePair::named(
            PhonePairing::scanned(link, "").expect("a pairing link"),
            name,
        )
    }

    /// From a typed code, before HELLO: Cpub is not known yet.
    pub fn typed(code: &str, host: &str, name: &str) -> PhonePair {
        PhonePair::named(
            PhonePairing::typed(code, host, "").expect("a good code"),
            name,
        )
    }

    /// Any name, unchecked: tests send names the computer must refuse.
    fn named(mut p: PhonePairing, name: &str) -> PhonePair {
        p.name = name.to_string();
        PhonePair(p)
    }

    pub fn take_hello(&mut self, answer: &[u8]) -> Result<(), crypto::Refusal> {
        self.0.take_hello(answer)
    }

    pub fn take_nonce(&mut self, frame: &[u8]) -> Result<(), crypto::Refusal> {
        self.0.take_nonce(frame)
    }

    /// What a paired phone keeps after an allowed outcome.
    pub fn paired(self, outcome: &crypto::PairOutcome) -> PhoneKeys {
        PhoneKeys::from(self.0.paired(outcome).expect("allowed"))
    }
}

/// What a paired phone keeps: its device id, its key D and the computer's Cpub.
pub struct PhoneKeys {
    pub device_id: DeviceId,
    pub d: SecretKey,
    pub cpub: Vec<u8>,
}

impl From<PhoneIdentity> for PhoneKeys {
    fn from(i: PhoneIdentity) -> PhoneKeys {
        PhoneKeys {
            device_id: i.device_id,
            d: i.secret,
            cpub: i.cpub,
        }
    }
}

impl PhoneKeys {
    pub fn id(&self) -> String {
        crypto::b64(&self.device_id)
    }

    pub fn kd(&self) -> crypto::Key32 {
        let c = crypto::check_public_key(&self.cpub).unwrap();
        crypto::device_key(crypto::ecdh(&self.d, &c).as_ref(), &self.device_id)
    }

    /// The identity the phone app keeps, at `address`.
    pub fn identity(&self, address: &str, name: &str) -> PhoneIdentity {
        PhoneIdentity {
            device_id: self.device_id,
            secret: self.d.clone(),
            cpub: self.cpub.clone(),
            home_origin: format!("https://{address}"),
            address: address.to_string(),
            device_name: name.to_string(),
        }
    }
}

pub mod client;

use std::sync::{Arc, Mutex};

use jiff::Timestamp;
use nocturne_contract::{
    PairingRequest, PhoneHello, PhoneReadiness, PhoneStatus, RemoteAnswer, RemoteCall,
    RemoteConnection,
};
use nocturne_engine::{DeviceRow, Engine, ManualClock};

use super::limits::ManualMono;
use super::session::{BoxFuture, CallCtx, Core, Dispatch, OnState, Shell};

/// What a [`TestShell`] was asked to show.
#[derive(Debug, Clone, PartialEq)]
pub enum Shown {
    Changed,
    Request(PairingRequest),
    Active(String),
    Stopped(String),
}

/// A shell over a bare engine, recording what it would have shown.
pub struct TestShell {
    pub engine: Engine,
    pub clock: Arc<ManualClock>,
    pub shown: Mutex<Vec<Shown>>,
    /// Leave the newest phone build out of the hello, as a computer from before the version
    /// window did.
    pub before_window: std::sync::atomic::AtomicBool,
}

impl TestShell {
    pub fn new(dir: &std::path::Path) -> Arc<TestShell> {
        let clock = Arc::new(ManualClock::at("2026-11-13T07:00:00Z", "Europe/London"));
        Arc::new(TestShell {
            engine: Engine::open(&dir.join("store"), clock.clone()).unwrap(),
            clock,
            shown: Mutex::new(Vec::new()),
            before_window: std::sync::atomic::AtomicBool::new(false),
        })
    }

    pub fn shown(&self) -> Vec<Shown> {
        self.shown.lock().unwrap().clone()
    }

    fn show(&self, s: Shown) {
        self.shown.lock().unwrap().push(s);
    }
}

impl Shell for TestShell {
    fn engine(&self) -> Option<Engine> {
        Some(self.engine.clone())
    }

    fn wall_now(&self) -> Timestamp {
        nocturne_engine::Clock::now(self.clock.as_ref())
    }

    fn changed(&self) {
        self.show(Shown::Changed);
    }

    fn pairing_request(&self, request: &PairingRequest) {
        self.show(Shown::Request(request.clone()));
    }

    fn device_active(&self, device: &DeviceRow) {
        self.show(Shown::Active(device.id.clone()));
    }

    fn hello(&self, device: &DeviceRow, connection: RemoteConnection) -> PhoneHello {
        PhoneHello {
            computer_name: "Test computer".into(),
            device_name: device.name.clone(),
            connection,
            min_phone_build: super::protocol::MIN_PHONE_BUILD,
            max_phone_build: (!self.before_window.load(std::sync::atomic::Ordering::SeqCst))
                .then_some(super::protocol::PHONE_BUILD),
            version: "0.0.0".into(),
            features: Vec::new(),
            memory_here: false,
        }
    }

    fn stop_phone(&self, device_id: &str) {
        self.show(Shown::Stopped(device_id.to_string()));
    }
}

/// Answers `getStatus` and nothing else: enough to see frames cross the listener.
pub struct EchoDispatch;

impl Dispatch for EchoDispatch {
    fn call(&self, _core: Arc<Core>, ctx: CallCtx) -> BoxFuture<'static, ()> {
        Box::pin(async move {
            let answer = match ctx.req.call {
                RemoteCall::GetStatus => RemoteAnswer::Status {
                    status: PhoneStatus {
                        readiness: PhoneReadiness::Ready,
                        page_date: None,
                        needs_you: 0,
                    },
                },
                _ => RemoteAnswer::Done,
            };
            ctx.out.send(answer);
        })
    }
}

/// A core that is on, with a computer key and an address, over a test shell.
pub struct Bench {
    pub core: Arc<Core>,
    pub shell: Arc<TestShell>,
    pub mono: Arc<ManualMono>,
    pub computer: Arc<SecretKey>,
}

pub const TEST_HOST: &str = "amber-otter-lamp-quiet.trycloudflare.com";

/// Everything lives under `dir`, which the test owns.
pub fn bench(dir: &std::path::Path) -> Bench {
    let shell = TestShell::new(dir);
    let mono = ManualMono::new();
    let core = Core::new(shell.clone(), mono.clone(), dir.join("remote"));
    core.set_dispatch(Arc::new(EchoDispatch));
    let computer = Arc::new(crypto::new_secret());
    core.set_on(Some(OnState {
        computer: computer.clone(),
        connection: RemoteConnection::Simple,
        host: Some(TEST_HOST.to_string()),
    }));
    Bench {
        core,
        shell,
        mono,
        computer,
    }
}

impl Bench {
    /// A phone paired straight into the store, as an allowed pairing would leave it.
    pub fn paired(&self, name: &str) -> PhoneKeys {
        paired_in(&self.shell.engine, &self.computer, name)
    }
}

/// Add a phone to `engine` for computer key `c`, as an allowed pairing would.
pub fn paired_in(engine: &Engine, c: &SecretKey, name: &str) -> PhoneKeys {
    let d = crypto::new_secret();
    let row = engine
        .add_device(
            name,
            &crypto::public_bytes(&d.public_key()),
            &format!("https://{TEST_HOST}"),
            RemoteConnection::Simple,
            TEST_HOST,
        )
        .unwrap();
    PhoneKeys {
        device_id: crypto::unb64(&row.id).unwrap().try_into().unwrap(),
        d,
        cpub: crypto::public_bytes(&c.public_key()).to_vec(),
    }
}

// Fake helpers -------------------------------------------------------------------------------

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use nocturne_helpers::{
    HelperError, HelperInfo, HelperKind, InstallProgress, Installed, KeyCarrier, LaunchSpec,
    Transport, TransportState,
};
use tokio::sync::{Notify, watch};
use tokio_util::sync::CancellationToken;

use super::controller::{Helpers, RemoteHub};

/// A helper that runs nothing: it reports what a test tells it to.
pub struct FakeTransport {
    pub kind: Mutex<HelperKind>,
    pub carrier: Mutex<KeyCarrier>,
    tx: Mutex<Option<watch::Sender<TransportState>>>,
    pub starts: AtomicUsize,
    pub stops: AtomicUsize,
    pub sign_outs: AtomicUsize,
    /// Whether the last sign-out came while the helper ran.
    pub signed_out_running: AtomicBool,
    pub last_spec: Mutex<Option<LaunchSpec>>,
    /// Addresses handed out on each start, in turn; the last repeats.
    pub addresses: Mutex<Vec<String>>,
    /// Report `Up` as soon as it starts.
    pub auto_up: AtomicBool,
    /// What it reports on start instead of `Up` (sign-in, a failure).
    pub on_start: Mutex<Option<TransportState>>,
    /// Make the next sign-outs fail, as one that could not reach Tailscale.
    pub sign_out_fails: AtomicBool,
}

impl FakeTransport {
    pub fn new() -> Arc<FakeTransport> {
        Arc::new(FakeTransport {
            kind: Mutex::new(HelperKind::Cloudflared),
            carrier: Mutex::new(if cfg!(unix) {
                KeyCarrier::UnixSocket
            } else {
                KeyCarrier::HostHeader
            }),
            tx: Mutex::new(None),
            starts: AtomicUsize::new(0),
            stops: AtomicUsize::new(0),
            sign_outs: AtomicUsize::new(0),
            signed_out_running: AtomicBool::new(false),
            last_spec: Mutex::new(None),
            addresses: Mutex::new(vec![format!("https://{TEST_HOST}")]),
            auto_up: AtomicBool::new(true),
            on_start: Mutex::new(None),
            sign_out_fails: AtomicBool::new(false),
        })
    }

    /// Report a state, as the helper would.
    pub fn set(&self, st: TransportState) {
        if let Some(tx) = self.tx.lock().unwrap().as_ref() {
            let _ = tx.send(st);
        }
    }

    pub fn running(&self) -> bool {
        self.starts.load(Ordering::SeqCst) > self.stops.load(Ordering::SeqCst)
    }

    fn next_address(&self) -> String {
        let mut a = self.addresses.lock().unwrap();
        if a.len() > 1 {
            a.remove(0)
        } else {
            a[0].clone()
        }
    }
}

struct Shared(Arc<FakeTransport>);

impl Transport for Shared {
    fn kind(&self) -> HelperKind {
        *self.0.kind.lock().unwrap()
    }

    fn key_carrier(&self) -> KeyCarrier {
        *self.0.carrier.lock().unwrap()
    }

    fn start(
        &self,
        spec: LaunchSpec,
    ) -> nocturne_helpers::BoxFuture<'_, Result<watch::Receiver<TransportState>, HelperError>> {
        Box::pin(async move {
            let t = &self.0;
            t.starts.fetch_add(1, Ordering::SeqCst);
            *t.last_spec.lock().unwrap() = Some(spec);
            let first = match t.on_start.lock().unwrap().clone() {
                Some(st) => st,
                None if t.auto_up.load(Ordering::SeqCst) => TransportState::Up {
                    address: t.next_address(),
                    route: None,
                },
                None => TransportState::Starting,
            };
            let (tx, rx) = watch::channel(first);
            *t.tx.lock().unwrap() = Some(tx);
            Ok(rx)
        })
    }

    fn stop(&self) -> nocturne_helpers::BoxFuture<'_, Result<(), HelperError>> {
        Box::pin(async move {
            self.0.stops.fetch_add(1, Ordering::SeqCst);
            *self.0.tx.lock().unwrap() = None;
            Ok(())
        })
    }

    fn sign_out(&self) -> nocturne_helpers::BoxFuture<'_, Result<(), HelperError>> {
        Box::pin(async move {
            self.0.sign_outs.fetch_add(1, Ordering::SeqCst);
            self.0
                .signed_out_running
                .store(self.0.running(), Ordering::SeqCst);
            if self.0.sign_out_fails.load(Ordering::SeqCst) {
                return Err(HelperError::SignIn("signing out did not finish".into()));
            }
            Ok(())
        })
    }

    fn info(&self) -> Option<HelperInfo> {
        None
    }
}

/// Helpers that download, check and run nothing real.
pub struct FakeHelpers {
    pub dir: std::path::PathBuf,
    pub installed: Arc<Mutex<HashSet<HelperKind>>>,
    /// The binary on disk no longer matches its pin.
    pub changed: AtomicBool,
    pub transport: Arc<FakeTransport>,
    pub installs: AtomicUsize,
    /// Lets a waiting download finish.
    pub finish: Arc<Notify>,
    pub progress: Mutex<Vec<InstallProgress>>,
}

impl FakeHelpers {
    pub fn new(dir: &std::path::Path) -> Arc<FakeHelpers> {
        Arc::new(FakeHelpers {
            dir: dir.to_path_buf(),
            installed: Arc::new(Mutex::new(HashSet::new())),
            changed: AtomicBool::new(false),
            transport: FakeTransport::new(),
            installs: AtomicUsize::new(0),
            finish: Arc::new(Notify::new()),
            progress: Mutex::new(Vec::new()),
        })
    }

    pub fn install_now(&self, kind: HelperKind) {
        self.installed.lock().unwrap().insert(kind);
    }

    fn pinned(&self, kind: HelperKind) -> Installed {
        Installed {
            kind,
            version: "2026.1.0".into(),
            sha256: "0".repeat(64),
            path: self.dir.join("fake-helper"),
        }
    }
}

impl Helpers for FakeHelpers {
    fn installed(&self, kind: HelperKind) -> Option<Installed> {
        self.installed
            .lock()
            .unwrap()
            .contains(&kind)
            .then(|| self.pinned(kind))
    }

    fn verify(&self, _installed: &Installed) -> Result<(), HelperError> {
        if self.changed.load(Ordering::SeqCst) {
            Err(HelperError::Mismatch)
        } else {
            Ok(())
        }
    }

    fn install(
        &self,
        kind: HelperKind,
        progress: watch::Sender<InstallProgress>,
        cancel: CancellationToken,
    ) -> nocturne_helpers::BoxFuture<'static, Result<Installed, HelperError>> {
        self.installs.fetch_add(1, Ordering::SeqCst);
        let steps = self.progress.lock().unwrap().clone();
        let (finish, installed, pinned) = (
            self.finish.clone(),
            self.installed.clone(),
            self.pinned(kind),
        );
        Box::pin(async move {
            for p in steps {
                let _ = progress.send(p);
                tokio::task::yield_now().await;
            }
            // A paused or stopped download must never take the finish meant for the next one.
            tokio::select! {
                biased;
                () = cancel.cancelled() => Err(HelperError::Stopped),
                () = finish.notified() => {
                    installed.lock().unwrap().insert(kind);
                    Ok(pinned)
                }
            }
        })
    }

    fn transport(&self, kind: HelperKind, _installed: &Installed) -> Box<dyn Transport> {
        *self.transport.kind.lock().unwrap() = kind;
        Box::new(Shared(self.transport.clone()))
    }
}

/// Manage a hub on fakes for `app`; call before anything else touches Away from home.
pub fn install_fakes<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    host: Arc<dyn nocturne_intelligence::Host>,
    files: &std::path::Path,
) -> (Arc<RemoteHub>, Arc<FakeHelpers>, Arc<ManualMono>) {
    use tauri::Manager;
    let dir = app.state::<crate::state::AppState>().data_dir.clone();
    let helpers = FakeHelpers::new(&dir);
    let mono = ManualMono::new();
    let hub = super::manage_hub(
        app,
        helpers.clone(),
        host,
        Arc::new(super::static_files::DirFiles(files.to_path_buf())),
        mono.clone(),
    )
    .expect("an app with a data folder");
    (hub, helpers, mono)
}
