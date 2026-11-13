//! Adding a phone: one open offer at a time, its secret S shown as a code to scan and as a
//! code to type, then commit, nonce, reveal, the three check words and the person's decision.
//!
//! The offer is single use and lasts five minutes. Each pairing kind is accepted once. A second
//! commit, or a reveal later than ten seconds after the nonce, spends the offer. A frame that
//! does not open under the pair key (a wrongly typed code included) is noted in the activity
//! list and changes nothing: the code is 120 bits, so a pause would protect nothing and would
//! only keep the person from adding their phone. The frames are rate limited where they arrive.
//! Nothing becomes a device until the person allows it.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use jiff::Timestamp;
use nocturne_contract::{PairingOffer, PairingPhase, PairingRequest, PairingStatus};
use p256::SecretKey;
use zeroize::Zeroizing;

use super::crypto::{self, Envelope, PairKeys, PairingId, Refusal};
use super::protocol::*;

/// Which conversation a frame came on, so the answers go back the same way.
pub type ConvId = u64;

/// What the computer needs to make a device once the person allows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewDevice {
    pub name: String,
    pub public_key: Vec<u8>,
    pub home_origin: String,
    pub address: String,
}

/// What handling a frame asks of the caller.
#[derive(Debug, Default)]
pub struct PairStep {
    /// Frames to send back on the conversation the frame came on.
    pub reply: Vec<Vec<u8>>,
    /// The person must decide: show S6.
    pub request: Option<PairingRequest>,
    /// The phase changed: refresh the view.
    pub changed: bool,
    /// A frame that did not open: noted in the activity list (`WrongCode`).
    pub wrong: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairError {
    /// Away from home is off, or has no address yet.
    NotOn,
    /// No such offer waiting for a decision.
    NoRequest,
}

struct Committed {
    path: u8,
    name: String,
    commit: [u8; 32],
}

struct Revealed {
    dpub: Vec<u8>,
    conv: ConvId,
    request: PairingRequest,
    decide_by: Instant,
}

struct Offer {
    pid: PairingId,
    keys: PairKeys,
    cpub: Vec<u8>,
    computer: Arc<SecretKey>,
    host: String,
    expires: Instant,
    expires_at: Timestamp,
    seen: HashSet<u8>,
    commit: Option<Committed>,
    nonce: Option<([u8; NONCE_PAIR_LEN], Instant)>,
    revealed: Option<Revealed>,
    phase: PairingPhase,
}

impl Offer {
    fn open(&self) -> bool {
        matches!(
            self.phase,
            PairingPhase::WaitingForScan | PairingPhase::WaitingForYou { .. }
        )
    }
}

#[derive(Default)]
pub struct Pairings {
    offer: Option<Offer>,
}

fn wall_after(now_wall: Timestamp, d: Duration) -> Timestamp {
    now_wall
        .checked_add(jiff::SignedDuration::try_from(d).unwrap_or_default())
        .unwrap_or(now_wall)
}

pub fn fmt_wall(t: Timestamp) -> String {
    t.strftime("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

impl Pairings {
    /// Expire what is due: the offer after five minutes, an undecided request after two, a
    /// reveal window after ten seconds. Returns whether anything changed and, for a request
    /// that timed out, the refusal to send and where.
    pub fn tick(&mut self, now: Instant) -> (bool, Option<(ConvId, Vec<u8>)>) {
        let mut changed = false;
        let mut send = None;
        if let Some(o) = self.offer.as_mut() {
            match &o.phase {
                PairingPhase::WaitingForYou { .. } => {
                    let r = o.revealed.as_ref().expect("a request has a reveal");
                    if now >= r.decide_by {
                        o.phase = PairingPhase::TimedOut;
                        send = Some((r.conv, outcome(o, false, None)));
                        changed = true;
                    }
                }
                PairingPhase::WaitingForScan if now >= o.expires => {
                    o.phase = PairingPhase::Expired;
                    changed = true;
                }
                PairingPhase::WaitingForScan => {
                    if let Some((_, at)) = o.nonce
                        && o.revealed.is_none()
                        && now >= at + Duration::from_secs(REVEAL_WINDOW_SECS)
                    {
                        o.phase = PairingPhase::Spent;
                        changed = true;
                    }
                }
                _ => {}
            }
        }
        (changed, send)
    }

    /// A new offer, cancelling any other. `host` is the computer's current address.
    pub fn begin(
        &mut self,
        computer: Arc<SecretKey>,
        host: &str,
        now: Instant,
        now_wall: Timestamp,
    ) -> Result<PairingOffer, PairError> {
        self.tick(now);
        let s = Zeroizing::new(crypto::random::<S_LEN>());
        let pid = crypto::pairing_id(&s);
        let cpub = crypto::public_bytes(&computer.public_key()).to_vec();
        let keys = crypto::pair_keys(&s, &cpub, &pid);
        let link = crypto::pairing_link(host, &s, &cpub);
        let expires_at = wall_after(now_wall, Duration::from_secs(PAIR_TTL_SECS));
        let offer = PairingOffer {
            pairing_id: crypto::b64(&pid),
            qr: super::qr::matrix(&link),
            link,
            typed_address: format!("{host}{PATH_PAIR}"),
            typed_code: crypto::code_display(&crypto::code_encode(&s)),
            expires_at: fmt_wall(expires_at),
        };
        self.offer = Some(Offer {
            pid,
            keys,
            cpub,
            computer,
            host: host.to_string(),
            expires: now + Duration::from_secs(PAIR_TTL_SECS),
            expires_at,
            seen: HashSet::new(),
            commit: None,
            nonce: None,
            revealed: None,
            phase: PairingPhase::WaitingForScan,
        });
        Ok(offer)
    }

    /// Withdraw the offer (the person closed S5, or Away from home turned off).
    pub fn cancel(&mut self) -> bool {
        self.offer.take().is_some()
    }

    pub fn status(&mut self, now: Instant) -> Option<PairingStatus> {
        self.tick(now);
        self.offer.as_ref().map(|o| PairingStatus {
            pairing_id: crypto::b64(&o.pid),
            phase: o.phase.clone(),
            expires_at: fmt_wall(o.expires_at),
        })
    }

    /// The pairing id of the offer that is open now.
    pub fn open_pid(&self) -> Option<PairingId> {
        self.offer.as_ref().filter(|o| o.open()).map(|o| o.pid)
    }

    /// Whether `sid` is the pairing id of the offer that is open now.
    pub fn is_open_offer(&self, sid: &[u8]) -> bool {
        self.offer
            .as_ref()
            .is_some_and(|o| o.open() && crypto::eq_ct(sid, &o.pid))
    }

    /// The conversation that revealed a phone for the person to decide on, while it may still
    /// carry the outcome: until ten seconds past the decision deadline.
    pub fn revealed_conv(&self, now: Instant) -> Option<ConvId> {
        let r = self.offer.as_ref()?.revealed.as_ref()?;
        (now < r.decide_by + Duration::from_secs(HANDSHAKE_TIMEOUT_SECS)).then_some(r.conv)
    }

    /// Handle one pairing frame (HELLO, PAIR_COMMIT or PAIR_REVEAL) from conversation `conv`.
    pub fn handle(
        &mut self,
        frame: &[u8],
        conv: ConvId,
        now: Instant,
        now_wall: Timestamp,
    ) -> PairStep {
        let mut step = PairStep::default();
        let (changed, _) = self.tick(now);
        step.changed = changed;
        let Some(o) = self.offer.as_mut().filter(|o| o.open()) else {
            return step;
        };
        let Ok(h) = crypto::parse_header(frame) else {
            return step;
        };
        // Once a phone has committed, a frame that does not open is dropped: anyone at the
        // relay can see the pairing id, and junk must not end a pairing the person is deciding.
        let committed = o.commit.is_some();
        if !crypto::eq_ct(&h.sid, &o.pid) {
            // Another pairing id: a wrongly typed code, or a stale offer.
            if !committed {
                step.wrong = true;
            }
            return step;
        }
        if o.revealed.is_some() {
            // The person is deciding: a replayed commit or reveal changes nothing.
            return step;
        }
        match h.kind {
            KIND_HELLO => {
                if crypto::hello_request_open(frame).is_ok() && o.seen.insert(KIND_HELLO) {
                    step.reply.push(crypto::hello_answer(&o.pid, &o.cpub));
                }
            }
            KIND_PAIR_COMMIT => {
                let Ok(Envelope::Json(body)) =
                    crypto::pair_open(frame, KIND_PAIR_COMMIT, &o.pid, &o.keys.up)
                else {
                    if !committed {
                        step.wrong = true;
                    }
                    return step;
                };
                if !o.seen.insert(KIND_PAIR_COMMIT) {
                    o.phase = PairingPhase::Spent;
                    step.changed = true;
                    return step;
                }
                let Some(committed) = parse_commit(&body) else {
                    o.phase = PairingPhase::Spent;
                    step.changed = true;
                    return step;
                };
                o.commit = Some(committed);
                let nc = crypto::random::<NONCE_PAIR_LEN>();
                o.nonce = Some((nc, now));
                let body = serde_json::to_vec(&crypto::PairNonce {
                    nc: crypto::b64(&nc),
                })
                .expect("serialises");
                step.reply.push(crypto::pair_seal(
                    KIND_PAIR_NONCE,
                    &o.pid,
                    &o.keys.down,
                    &crypto::envelope_json(&body),
                ));
            }
            KIND_PAIR_REVEAL => {
                let Ok(Envelope::Json(body)) =
                    crypto::pair_open(frame, KIND_PAIR_REVEAL, &o.pid, &o.keys.up)
                else {
                    if !committed {
                        step.wrong = true;
                    }
                    return step;
                };
                let in_time = o
                    .nonce
                    .is_some_and(|(_, at)| now < at + Duration::from_secs(REVEAL_WINDOW_SECS));
                let (Some(c), Some((nc, _)), true, true) = (
                    o.commit.as_ref(),
                    o.nonce,
                    in_time,
                    o.seen.insert(KIND_PAIR_REVEAL),
                ) else {
                    o.phase = PairingPhase::Spent;
                    step.changed = true;
                    return step;
                };
                let Some((dpub, np)) = parse_reveal(&body) else {
                    o.phase = PairingPhase::Spent;
                    step.changed = true;
                    return step;
                };
                let checked: Result<_, Refusal> =
                    crypto::check_reveal(&o.cpub, &dpub, &np, c.name.as_bytes(), &c.commit);
                let Ok(dkey) = checked else {
                    o.phase = PairingPhase::Spent;
                    step.changed = true;
                    return step;
                };
                let shared = crypto::ecdh(&o.computer, &dkey);
                let words = crypto::words(
                    shared.as_ref(),
                    &crypto::WordsInput {
                        path: c.path,
                        pid: &o.pid,
                        host: &o.host,
                        cpub: &o.cpub,
                        dpub: &dpub,
                        name: c.name.as_bytes(),
                        np: &np,
                        nc: &nc,
                    },
                );
                let decide = Duration::from_secs(DECIDE_TTL_SECS);
                let request = PairingRequest {
                    pairing_id: crypto::b64(&o.pid),
                    device_name: c.name.clone(),
                    check_phrase: words.iter().map(|w| w.to_string()).collect(),
                    decide_by: fmt_wall(wall_after(now_wall, decide)),
                };
                o.revealed = Some(Revealed {
                    dpub,
                    conv,
                    request: request.clone(),
                    decide_by: now + decide,
                });
                o.phase = PairingPhase::WaitingForYou {
                    request: request.clone(),
                };
                step.request = Some(request);
                step.changed = true;
            }
            _ if !committed => step.wrong = true,
            _ => {}
        }
        step
    }

    /// The device the person is deciding on, if `pairing_id` names the request waiting.
    pub fn waiting(&mut self, pairing_id: &str, now: Instant) -> Result<NewDevice, PairError> {
        self.tick(now);
        let o = self.offer.as_ref().ok_or(PairError::NoRequest)?;
        match (&o.phase, &o.revealed, &o.commit) {
            (PairingPhase::WaitingForYou { .. }, Some(r), Some(c))
                if r.request.pairing_id == pairing_id =>
            {
                Ok(NewDevice {
                    name: c.name.clone(),
                    public_key: r.dpub.clone(),
                    home_origin: format!("https://{}", o.host),
                    address: o.host.clone(),
                })
            }
            _ => Err(PairError::NoRequest),
        }
    }

    /// Record the decision. `device_id` is the new device's id when allowed. Returns where to
    /// send the outcome and the frame.
    pub fn decide(&mut self, device_id: Option<&str>) -> Option<(ConvId, Vec<u8>)> {
        let o = self.offer.as_mut()?;
        let r = o.revealed.as_ref()?;
        if !matches!(o.phase, PairingPhase::WaitingForYou { .. }) {
            return None;
        }
        let conv = r.conv;
        let name = r.request.device_name.clone();
        let frame = outcome(o, device_id.is_some(), device_id);
        o.phase = match device_id {
            Some(_) => PairingPhase::Allowed { device_name: name },
            None => PairingPhase::Refused,
        };
        Some((conv, frame))
    }
}

fn outcome(o: &Offer, allowed: bool, device_id: Option<&str>) -> Vec<u8> {
    let body = serde_json::to_vec(&crypto::PairOutcome {
        allowed,
        device_id: device_id.map(str::to_string),
    })
    .expect("serialises");
    crypto::pair_seal(
        KIND_PAIR_OUTCOME,
        &o.pid,
        &o.keys.down,
        &crypto::envelope_json(&body),
    )
}

fn parse_commit(body: &[u8]) -> Option<Committed> {
    let c: crypto::PairCommit = serde_json::from_slice(body).ok()?;
    if c.path != PATH_SCANNED && c.path != PATH_TYPED {
        return None;
    }
    crypto::check_device_name(&c.device_name).ok()?;
    let commit: [u8; 32] = crypto::unb64(&c.commit)?.try_into().ok()?;
    Some(Committed {
        path: c.path,
        name: c.device_name,
        commit,
    })
}

fn parse_reveal(body: &[u8]) -> Option<(Vec<u8>, Vec<u8>)> {
    let r: crypto::PairReveal = serde_json::from_slice(body).ok()?;
    let np = crypto::unb64(&r.np)?;
    (np.len() == NONCE_PAIR_LEN).then_some((crypto::unb64(&r.dpub)?, np))
}

#[cfg(test)]
#[path = "pairing_tests.rs"]
mod tests;
