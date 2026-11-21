//! The phone's side of pairing and of opening a session, as pure steps.
//!
//! [`PhonePairing`] plays one pairing frame by frame, from a scanned link or from a typed code
//! and address. [`PhoneIdentity`] is what a paired phone keeps; it opens sessions and turns the
//! computer's accept into a [`Channel`]. Nothing here does I/O.

use p256::SecretKey;
use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

use crate::crypto::{self, DeviceId, Envelope, PairKeys, PairingId, Refusal};
use crate::frame::{Channel, Side};
use crate::protocol::*;

/// Quick tunnel addresses are four words under this name.
pub const TUNNEL_SUFFIX: &str = ".trycloudflare.com";

/// Why a pairing could not start or finish.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairError {
    /// Not a pairing link from a computer.
    BadLink,
    /// Not a code: wrong length or characters.
    BadCode,
    /// Not four words and not a name.
    BadAddress,
    /// Empty, too long, or with characters a name may not have.
    BadName,
    /// A frame from the computer did not open.
    Refused(Refusal),
}

impl std::fmt::Display for PairError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for PairError {}

impl From<Refusal> for PairError {
    fn from(r: Refusal) -> PairError {
        PairError::Refused(r)
    }
}

fn is_label(l: &str) -> bool {
    !l.is_empty()
        && l.len() <= 63
        && l.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !l.starts_with('-')
        && !l.ends_with('-')
}

/// A host name with at least two labels and a top label that is not a number, lowercase.
pub fn check_host(host: &str) -> Option<String> {
    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    let top = host.rsplit('.').next().unwrap_or_default();
    let ok = (4..=253).contains(&host.len())
        && host.contains('.')
        && host.split('.').all(is_label)
        && top.bytes().any(|b| b.is_ascii_lowercase());
    ok.then_some(host)
}

/// Four words, in any case, make the quick tunnel's address; anything else makes none.
pub fn address_from_words(words: &[&str]) -> Option<String> {
    let cleaned: Vec<String> = words
        .iter()
        .map(|w| w.trim().to_ascii_lowercase())
        .collect();
    let good = |w: &String| {
        (2..=32).contains(&w.len())
            && w.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
    };
    (cleaned.len() == 4 && cleaned.iter().all(good))
        .then(|| format!("{}{TUNNEL_SUFFIX}", cleaned.join("-")))
}

/// What a person typed as the computer's address: a name when it has a dot (Most private), or
/// four words split by spaces or dashes (Simple). Capitals do not matter.
pub fn address_from_typed(text: &str) -> Option<String> {
    let text = text.trim();
    if text.contains('.') {
        return check_host(text);
    }
    let words: Vec<&str> = text
        .split(|c: char| c.is_whitespace() || c == '-')
        .filter(|w| !w.is_empty())
        .collect();
    address_from_words(&words)
}

/// The phone's side of one pairing.
pub struct PhonePairing {
    /// The code's secret: wiped when the pairing goes.
    pub s: [u8; S_LEN],
    pub pid: PairingId,
    pub keys: PairKeys,
    pub cpub: Vec<u8>,
    pub d: SecretKey,
    pub name: String,
    pub np: [u8; NONCE_PAIR_LEN],
    pub nc: Option<[u8; NONCE_PAIR_LEN]>,
    pub path: u8,
    /// The computer's address: where frames go, and what the words bind.
    pub host: String,
}

impl PhonePairing {
    /// From a scanned link, `https://<host>/p#v1.<S>.<Cpub>`.
    pub fn scanned(link: &str, name: &str) -> Result<PhonePairing, PairError> {
        let rest = link
            .trim()
            .strip_prefix("https://")
            .ok_or(PairError::BadLink)?;
        let (host_path, fragment) = rest.split_once('#').ok_or(PairError::BadLink)?;
        let (host, path) = host_path.split_once('/').unwrap_or((host_path, ""));
        if format!("/{path}") != PATH_PAIR {
            return Err(PairError::BadLink);
        }
        let host = check_host(host).ok_or(PairError::BadLink)?;
        let mut parts = fragment.split('.');
        if parts.next() != Some(LINK_VERSION) {
            return Err(PairError::BadLink);
        }
        let s: [u8; S_LEN] = parts
            .next()
            .and_then(crypto::unb64)
            .and_then(|v| v.try_into().ok())
            .ok_or(PairError::BadLink)?;
        let cpub = parts
            .next()
            .and_then(crypto::unb64)
            .ok_or(PairError::BadLink)?;
        if parts.next().is_some() || crypto::check_public_key(&cpub).is_err() {
            return Err(PairError::BadLink);
        }
        PhonePairing::new(s, cpub, name, PATH_SCANNED, host)
    }

    /// From a typed code and the computer's address (four words or a name), before HELLO: Cpub
    /// is not known yet.
    pub fn typed(code: &str, address: &str, name: &str) -> Result<PhonePairing, PairError> {
        let host = address_from_typed(address).ok_or(PairError::BadAddress)?;
        let s = crypto::code_normalise(code)
            .and_then(|c| crypto::code_decode(&c))
            .map_err(|_| PairError::BadCode)?;
        PhonePairing::new(s, Vec::new(), name, PATH_TYPED, host)
    }

    fn new(
        s: [u8; S_LEN],
        cpub: Vec<u8>,
        name: &str,
        path: u8,
        host: String,
    ) -> Result<PhonePairing, PairError> {
        let pid = crypto::pairing_id(&s);
        let mut p = PhonePairing {
            keys: crypto::pair_keys(&s, &cpub, &pid),
            s,
            pid,
            cpub,
            d: crypto::new_secret(),
            name: String::new(),
            np: crypto::random(),
            nc: None,
            path,
            host,
        };
        p.set_name(name)?;
        Ok(p)
    }

    /// The name the computer shows; set before the commit. An empty name waits to be set.
    pub fn set_name(&mut self, name: &str) -> Result<(), PairError> {
        let name = name.trim();
        if !name.is_empty() {
            crypto::check_device_name(name).map_err(|_| PairError::BadName)?;
        }
        self.name = name.to_string();
        Ok(())
    }

    /// `https://<host>`, the phone's home for this pairing.
    pub fn home_origin(&self) -> String {
        format!("https://{}", self.host)
    }

    /// Whether Cpub is known: always after a scan, after HELLO for a typed code.
    pub fn knows_computer(&self) -> bool {
        !self.cpub.is_empty()
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
        .expect("a commit serialises")
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
        .expect("a reveal serialises");
        self.seal_up(KIND_PAIR_REVEAL, &body)
    }

    /// The three words this phone shows; empty until the nonce is in.
    pub fn words(&self) -> Vec<String> {
        let (Some(nc), Ok(c)) = (self.nc, crypto::check_public_key(&self.cpub)) else {
            return Vec::new();
        };
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
                nc: &nc,
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
    pub fn paired(mut self, outcome: &crypto::PairOutcome) -> Result<PhoneIdentity, PairError> {
        let device_id: DeviceId = outcome
            .device_id
            .as_deref()
            .filter(|_| outcome.allowed)
            .and_then(crypto::unb64)
            .and_then(|v| v.try_into().ok())
            .ok_or(PairError::Refused(Refusal::BadEnvelope))?;
        Ok(PhoneIdentity {
            device_id,
            home_origin: self.home_origin(),
            address: std::mem::take(&mut self.host),
            device_name: std::mem::take(&mut self.name),
            secret: std::mem::replace(&mut self.d, crypto::new_secret()),
            cpub: std::mem::take(&mut self.cpub),
        })
    }
}

impl Drop for PhonePairing {
    fn drop(&mut self) {
        self.s.zeroize();
        self.np.zeroize();
    }
}

/// What a paired phone keeps: its device id, its key D, the computer's Cpub, where it was
/// paired and where the computer is now.
#[derive(Clone)]
pub struct PhoneIdentity {
    pub device_id: DeviceId,
    pub secret: SecretKey,
    pub cpub: Vec<u8>,
    /// `https://<host>` at pairing; never changes.
    pub home_origin: String,
    /// The computer's current address, a host. Changes only after a session there opens.
    pub address: String,
    pub device_name: String,
}

impl std::fmt::Debug for PhoneIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PhoneIdentity")
            .field("device_id", &self.id())
            .field("address", &self.address)
            .finish_non_exhaustive()
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Stored {
    v: u8,
    device_id: String,
    secret: String,
    cpub: String,
    home_origin: String,
    address: String,
    device_name: String,
}

impl PhoneIdentity {
    pub fn id(&self) -> String {
        crypto::b64(&self.device_id)
    }

    pub fn dpub(&self) -> Vec<u8> {
        crypto::public_bytes(&self.secret.public_key()).to_vec()
    }

    pub fn kd(&self) -> Result<crypto::Key32, Refusal> {
        let c = crypto::check_public_key(&self.cpub)?;
        Ok(crypto::device_key(
            crypto::ecdh(&self.secret, &c).as_ref(),
            &self.device_id,
        ))
    }

    /// A SESSION_OPEN frame, and what reading the answer to it needs.
    pub fn session_open(&self) -> Result<(Vec<u8>, Opening), Refusal> {
        let e = crypto::new_secret();
        let epub = crypto::public_bytes(&e.public_key()).to_vec();
        let on = crypto::random::<OPEN_NONCE_LEN>();
        let kd = self.kd()?;
        let frame = crypto::session_open(&self.device_id, &epub, &on, &kd);
        Ok((frame, Opening { e, epub, on, kd }))
    }

    /// The identity as kept on the phone. It holds the private key: keep it private.
    pub fn to_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(&Stored {
            v: 1,
            device_id: self.id(),
            secret: crypto::b64(&self.secret.to_bytes()),
            cpub: crypto::b64(&self.cpub),
            home_origin: self.home_origin.clone(),
            address: self.address.clone(),
            device_name: self.device_name.clone(),
        })
        .expect("an identity serialises")
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<PhoneIdentity> {
        let s: Stored = serde_json::from_slice(bytes).ok()?;
        if s.v != 1 {
            return None;
        }
        let secret = crypto::unb64(&s.secret).and_then(|b| SecretKey::from_slice(&b).ok())?;
        let cpub = crypto::unb64(&s.cpub)?;
        crypto::check_public_key(&cpub).ok()?;
        Some(PhoneIdentity {
            device_id: crypto::unb64(&s.device_id)?.try_into().ok()?,
            secret,
            cpub,
            home_origin: s.home_origin,
            address: check_host(&s.address)?,
            device_name: s.device_name,
        })
    }
}

/// A SESSION_OPEN in flight: the ephemeral key and nonce it carried.
pub struct Opening {
    e: SecretKey,
    epub: Vec<u8>,
    on: [u8; OPEN_NONCE_LEN],
    kd: crypto::Key32,
}

impl Opening {
    /// Whether `frame` is a sealed REVOKED answering this open (the phone was removed).
    pub fn revoked(&self, frame: &[u8], identity: &PhoneIdentity) -> bool {
        crypto::revoked_open(frame, &self.kd, &identity.device_id, &self.on).is_ok()
    }

    /// The session from the computer's SESSION_ACCEPT.
    pub fn accept(self, frame: &[u8], identity: &PhoneIdentity) -> Result<Channel, Refusal> {
        let (sid, fkey) = crypto::session_accept_open(frame)?;
        let ckey = crypto::check_public_key(&identity.cpub)?;
        let fpub = crypto::public_bytes(&fkey);
        let dpub = identity.dpub();
        let keys = crypto::session_keys_phone(
            &self.e,
            &identity.secret,
            &fkey,
            &ckey,
            &crypto::SessionInput {
                device_id: &identity.device_id,
                epub: &self.epub,
                on: &self.on,
                sid: &sid,
                fpub: &fpub,
                cpub: &identity.cpub,
                dpub: &dpub,
            },
        );
        Ok(Channel::new(Side::Phone, sid, keys))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_addresses() {
        let four = "amber-otter-lamp-quiet.trycloudflare.com";
        for t in [
            "amber otter lamp quiet",
            "Amber Otter Lamp Quiet",
            "amber-otter-lamp-quiet",
            "  amber  otter - lamp quiet ",
        ] {
            assert_eq!(address_from_typed(t).as_deref(), Some(four), "{t}");
        }
        assert_eq!(
            address_from_typed("Desk.Tail1234.ts.net").as_deref(),
            Some("desk.tail1234.ts.net")
        );
        for bad in [
            "amber otter lamp",
            "a b c d",
            "amber otter lamp quiet five",
            "",
            "x.",
            "a..b",
            "127.0.0.1",
        ] {
            assert_eq!(address_from_typed(bad), None, "{bad}");
        }
    }

    #[test]
    fn scanned_links() {
        let s = crypto::random::<S_LEN>();
        let c = crypto::new_secret();
        let cpub = crypto::public_bytes(&c.public_key());
        let link = crypto::pairing_link("amber-otter-lamp-quiet.trycloudflare.com", &s, &cpub);
        let p = PhonePairing::scanned(&link, "Pixel").unwrap();
        assert_eq!(p.host, "amber-otter-lamp-quiet.trycloudflare.com");
        assert_eq!(p.s, s);
        assert!(p.knows_computer());
        for bad in [
            link.replace("https://", "http://"),
            link.replace("/p#", "/r#"),
            link.replace("#v1.", "#v2."),
            link.split_once('#').unwrap().0.to_string(),
        ] {
            assert!(PhonePairing::scanned(&bad, "Pixel").is_err(), "{bad}");
        }
    }

    #[test]
    fn typed_needs_code_and_address() {
        let code = crypto::code_encode(&crypto::random());
        let p = PhonePairing::typed(&code.to_lowercase(), "amber otter lamp quiet", "").unwrap();
        assert!(!p.knows_computer());
        assert_eq!(
            p.home_origin(),
            "https://amber-otter-lamp-quiet.trycloudflare.com"
        );
        assert_eq!(
            PhonePairing::typed("nope", "amber otter lamp quiet", "Pixel").err(),
            Some(PairError::BadCode)
        );
        assert_eq!(
            PhonePairing::typed(&code, "amber otter", "Pixel").err(),
            Some(PairError::BadAddress)
        );
    }

    #[test]
    fn identity_round_trips() {
        let id = PhoneIdentity {
            device_id: crypto::random(),
            secret: crypto::new_secret(),
            cpub: crypto::public_bytes(&crypto::new_secret().public_key()).to_vec(),
            home_origin: "https://a-b-c-d.trycloudflare.com".into(),
            address: "a-b-c-d.trycloudflare.com".into(),
            device_name: "Pixel".into(),
        };
        let back = PhoneIdentity::from_bytes(&id.to_bytes()).unwrap();
        assert_eq!(back.device_id, id.device_id);
        assert_eq!(back.secret.to_bytes(), id.secret.to_bytes());
        assert_eq!(back.cpub, id.cpub);
        assert_eq!(back.address, id.address);
        assert!(PhoneIdentity::from_bytes(b"{}").is_none());
    }
}
