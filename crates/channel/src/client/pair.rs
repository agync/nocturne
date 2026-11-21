//! Pairing over the wire: the address check, HELLO for a typed code, then commit, reveal, the
//! words and the person's decision. A pairing commits once and never retries.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::link::{self, Got, Router};
use crate::phone::{PhoneIdentity, PhonePairing};
use crate::protocol::*;

/// An answer to HELLO is immediate when there is an offer; none in this long means none.
const HELLO_WAIT: Duration = Duration::from_secs(5);

/// How a pairing ended without a paired phone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PairFailure {
    /// The person said no on the computer.
    Refused,
    /// No open offer for this code, or it ran out.
    Expired,
    /// This pairing already committed, or a frame went wrong after it did.
    Spent,
    BadCode,
    BadAddress,
    BadName,
    /// Nothing answered at the address.
    NoAnswer,
}

/// Whether the computer answers at the typed address (four words or a name): its pairing page
/// loads there. Answers the address it resolved to.
pub async fn check_address(
    router: &Router,
    http: &reqwest::Client,
    typed: &str,
) -> Result<String, PairFailure> {
    let address = crate::phone::address_from_typed(typed).ok_or(PairFailure::BadAddress)?;
    let route = router(&address).ok_or(PairFailure::BadAddress)?;
    let res = http
        .get(route.url(PATH_PAIR))
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(|_| PairFailure::NoAnswer)?;
    if res.status().as_u16() == 200 {
        Ok(address)
    } else {
        Err(PairFailure::NoAnswer)
    }
}

/// For a typed code: ask the computer for its key while the offer is open.
pub async fn take_hello(
    pairing: &mut PhonePairing,
    router: &Router,
    http: &reqwest::Client,
) -> Result<(), PairFailure> {
    if pairing.knows_computer() {
        return Ok(());
    }
    let route = router(&pairing.host).ok_or(PairFailure::BadAddress)?;
    let (link, mut inbox) = link::open(&route, http, false, Duration::from_secs(5)).await;
    link.send(pairing.hello())
        .await
        .map_err(|_| PairFailure::Expired)?;
    // Over long-poll the answer rides the same exchange: none there means no open offer.
    let wait = match link.kind() {
        link::LinkKind::Poll => Duration::ZERO,
        link::LinkKind::WebSocket => HELLO_WAIT,
    };
    let out = match inbox.next(Some(wait)).await {
        Got::Frame(f) => pairing.take_hello(&f).map_err(|_| PairFailure::Expired),
        _ => Err(PairFailure::Expired),
    };
    link.close();
    out
}

/// Commit, reveal, show the words, then wait for the person's decision on the computer.
pub async fn pair(
    mut pairing: PhonePairing,
    router: &Router,
    http: &reqwest::Client,
    mut on_words: impl FnMut(Vec<String>) + Send,
) -> Result<PhoneIdentity, PairFailure> {
    if pairing.name.is_empty() {
        return Err(PairFailure::BadName);
    }
    if !pairing.knows_computer() {
        return Err(PairFailure::Expired);
    }
    let route = router(&pairing.host).ok_or(PairFailure::BadAddress)?;
    let (link, mut inbox) = link::open(&route, http, false, Duration::from_secs(5)).await;
    if link.send(pairing.commit()).await.is_err() {
        return Err(PairFailure::Expired);
    }
    // The pairing exists on the computer once the commit is in; later frames come down here.
    link.set_sid(&pairing.pid);
    let mut deadline = Instant::now() + Duration::from_secs(HANDSHAKE_TIMEOUT_SECS);
    let mut have_nonce = false;
    let out = loop {
        let left = deadline.saturating_duration_since(Instant::now());
        let frame = match inbox.next(Some(left)).await {
            Got::Frame(f) => f,
            _ => {
                break Err(if have_nonce {
                    PairFailure::Spent
                } else {
                    PairFailure::Expired
                });
            }
        };
        // A frame that does not open, or is not the next one expected, is junk: it ends nothing.
        match frame.get(1).copied() {
            Some(KIND_PAIR_NONCE) if !have_nonce => {
                if pairing.take_nonce(&frame).is_err() {
                    continue;
                }
                have_nonce = true;
                deadline = Instant::now() + Duration::from_secs(DECIDE_TTL_SECS + 20);
                if link.send(pairing.reveal()).await.is_err() {
                    break Err(PairFailure::Spent);
                }
                on_words(pairing.words());
            }
            Some(KIND_PAIR_OUTCOME) if have_nonce => {
                let Ok(outcome) = pairing.take_outcome(&frame) else {
                    continue;
                };
                if !outcome.allowed {
                    break Err(PairFailure::Refused);
                }
                break pairing.paired(&outcome).map_err(|_| PairFailure::Spent);
            }
            _ => continue,
        }
    };
    link.close();
    out
}
