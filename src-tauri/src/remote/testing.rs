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
