//! A phone written in Rust, and fakes, for tests. Built only in debug builds: never shipped.
//!
//! [`PhonePair`] plays the phone's side of pairing frame by frame; [`PhoneKeys`] holds what a
//! paired phone keeps and opens sessions; the networked client is in `testing::client`.

use p256::SecretKey;

use super::crypto::{self, DeviceId, Envelope, PairKeys, PairingId, Refusal};
use super::protocol::*;

/// The phone's side of one pairing.
pub struct PhonePair {
    pub s: [u8; S_LEN],
    pub pid: PairingId,
    pub keys: PairKeys,
    pub cpub: Vec<u8>,
    pub d: SecretKey,
    pub name: String,
    pub np: [u8; NONCE_PAIR_LEN],
    pub nc: Option<[u8; NONCE_PAIR_LEN]>,
    pub path: u8,
    /// The host the phone sees in its address bar.
    pub host: String,
}

impl PhonePair {
    /// From a scanned link: S and Cpub from the fragment.
    pub fn scanned(link: &str, name: &str) -> PhonePair {
        let rest = link.strip_prefix("https://").expect("an https link");
        let (host_path, fragment) = rest.split_once('#').expect("a fragment");
        let host = host_path.split('/').next().unwrap().to_string();
        let mut parts = fragment.split('.');
        assert_eq!(parts.next(), Some(LINK_VERSION));
        let s: [u8; S_LEN] = crypto::unb64(parts.next().unwrap())
            .unwrap()
            .try_into()
            .unwrap();
        let cpub = crypto::unb64(parts.next().unwrap()).unwrap();
        PhonePair::new(s, cpub, name, PATH_SCANNED, &host)
    }

    /// From a typed code, before HELLO: Cpub is not known yet.
    pub fn typed(code: &str, host: &str, name: &str) -> PhonePair {
        let s = crypto::code_decode(code).expect("a good code");
        PhonePair::new(s, Vec::new(), name, PATH_TYPED, host)
    }

    fn new(s: [u8; S_LEN], cpub: Vec<u8>, name: &str, path: u8, host: &str) -> PhonePair {
        let pid = crypto::pairing_id(&s);
        PhonePair {
            keys: crypto::pair_keys(&s, &cpub, &pid),
            s,
            pid,
            cpub,
            d: crypto::new_secret(),
            name: name.to_string(),
            np: crypto::random(),
            nc: None,
            path,
            host: host.to_string(),
        }
    }

    pub fn hello(&self) -> Vec<u8> {
        crypto::hello_request(&self.pid)
    }

    /// Take Cpub from the HELLO answer; the pair keys follow from it.
    pub fn take_hello(&mut self, answer: &[u8]) -> Result<(), Refusal> {
        let c = crypto::hello_answer_open(answer, &self.pid)?;
        self.cpub = crypto::public_bytes(&c).to_vec();
        self.keys = crypto::pair_keys(&self.s, &self.cpub, &self.pid);
        Ok(())
    }

    pub fn dpub(&self) -> Vec<u8> {
        crypto::public_bytes(&self.d.public_key()).to_vec()
    }

    pub fn commit_body(&self) -> Vec<u8> {
        let c = crypto::commit(&self.dpub(), &self.np, self.name.as_bytes());
        serde_json::to_vec(&crypto::PairCommit {
            path: self.path,
            device_name: self.name.clone(),
            commit: crypto::b64(&c),
        })
        .unwrap()
    }

    pub fn seal_up(&self, kind: u8, body: &[u8]) -> Vec<u8> {
        crypto::pair_seal(kind, &self.pid, &self.keys.up, &crypto::envelope_json(body))
    }

    pub fn commit(&self) -> Vec<u8> {
        self.seal_up(KIND_PAIR_COMMIT, &self.commit_body())
    }

    pub fn take_nonce(&mut self, frame: &[u8]) -> Result<(), Refusal> {
        let Envelope::Json(body) =
            crypto::pair_open(frame, KIND_PAIR_NONCE, &self.pid, &self.keys.down)?
        else {
            return Err(Refusal::BadEnvelope);
        };
        let n: crypto::PairNonce =
            serde_json::from_slice(&body).map_err(|_| Refusal::BadEnvelope)?;
        self.nc = Some(
            crypto::unb64(&n.nc)
                .and_then(|v| v.try_into().ok())
                .ok_or(Refusal::BadEnvelope)?,
        );
        Ok(())
    }

    pub fn reveal(&self) -> Vec<u8> {
        let body = serde_json::to_vec(&crypto::PairReveal {
            dpub: crypto::b64(&self.dpub()),
            np: crypto::b64(&self.np),
        })
        .unwrap();
        self.seal_up(KIND_PAIR_REVEAL, &body)
    }

    /// The three words this phone shows.
    pub fn words(&self) -> Vec<String> {
        let c = crypto::check_public_key(&self.cpub).unwrap();
        let shared = crypto::ecdh(&self.d, &c);
        crypto::words(
            shared.as_ref(),
            &crypto::WordsInput {
                path: self.path,
                pid: &self.pid,
                host: &self.host,
                cpub: &self.cpub,
                dpub: &self.dpub(),
                name: self.name.as_bytes(),
                np: &self.np,
                nc: &self.nc.expect("a nonce"),
            },
        )
        .iter()
        .map(|w| w.to_string())
        .collect()
    }

    pub fn take_outcome(&self, frame: &[u8]) -> Result<crypto::PairOutcome, Refusal> {
        let Envelope::Json(body) =
            crypto::pair_open(frame, KIND_PAIR_OUTCOME, &self.pid, &self.keys.down)?
        else {
            return Err(Refusal::BadEnvelope);
        };
        serde_json::from_slice(&body).map_err(|_| Refusal::BadEnvelope)
    }

    /// What a paired phone keeps after an allowed outcome.
    pub fn paired(self, outcome: &crypto::PairOutcome) -> PhoneKeys {
        let id: DeviceId = crypto::unb64(outcome.device_id.as_deref().expect("allowed"))
            .unwrap()
            .try_into()
            .unwrap();
        PhoneKeys {
            device_id: id,
            d: self.d,
            cpub: self.cpub,
        }
    }
}

/// What a paired phone keeps: its device id, its key D and the computer's Cpub.
pub struct PhoneKeys {
    pub device_id: DeviceId,
    pub d: SecretKey,
    pub cpub: Vec<u8>,
}

impl PhoneKeys {
    pub fn id(&self) -> String {
        crypto::b64(&self.device_id)
    }

    pub fn kd(&self) -> crypto::Key32 {
        let c = crypto::check_public_key(&self.cpub).unwrap();
        crypto::device_key(crypto::ecdh(&self.d, &c).as_ref(), &self.device_id)
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
}

impl TestShell {
    pub fn new(dir: &std::path::Path) -> Arc<TestShell> {
        let clock = Arc::new(ManualClock::at("2026-11-13T07:00:00Z", "Europe/London"));
        Arc::new(TestShell {
            engine: Engine::open(&dir.join("store"), clock.clone()).unwrap(),
            clock,
            shown: Mutex::new(Vec::new()),
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
            version: "0.0.0".into(),
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
