use std::collections::HashSet;

use super::super::testing::PhonePair;
use super::*;

const HOST: &str = "amber-otter-lamp-quiet.trycloudflare.com";
/// Far more wrong frames than any limit would let through in a minute.
const TRIES: u32 = 15;

struct Bench {
    p: Pairings,
    now: Instant,
    wall: Timestamp,
    c: Arc<SecretKey>,
}

fn bench() -> Bench {
    Bench {
        p: Pairings::default(),
        now: Instant::now(),
        wall: "2026-11-13T07:00:00Z".parse().unwrap(),
        c: Arc::new(crypto::new_secret()),
    }
}

impl Bench {
    fn begin(&mut self) -> PairingOffer {
        self.p
            .begin(self.c.clone(), HOST, self.now, self.wall)
            .unwrap()
    }

    fn send(&mut self, frame: &[u8]) -> PairStep {
        self.p.handle(frame, 1, self.now, self.wall)
    }

    fn wait(&mut self, secs: u64) {
        self.now += Duration::from_secs(secs);
        self.wall = wall_after(self.wall, Duration::from_secs(secs));
    }

    fn phase(&mut self) -> PairingPhase {
        self.p.status(self.now).unwrap().phase
    }

    /// Commit, nonce and reveal for `phone`; the request if it reached the person.
    fn pair(&mut self, phone: &mut PhonePair) -> Option<PairingRequest> {
        let step = self.send(&phone.commit());
        phone.take_nonce(step.reply.first()?).ok()?;
        self.send(&phone.reveal()).request
    }
}

#[test]
fn code_works_once() {
    let mut b = bench();
    let offer = b.begin();
    let mut phone = PhonePair::scanned(&offer.link, "Pixel");
    let req = b.pair(&mut phone).unwrap();
    assert_eq!(req.check_phrase, phone.words());
    b.p.waiting(&req.pairing_id, b.now).unwrap();
    let (conv, frame) = b.p.decide(Some("AAAAAAAAAAAAAAAAAAAAAA")).unwrap();
    assert_eq!(conv, 1);
    assert!(phone.take_outcome(&frame).unwrap().allowed);
    let mut again = PhonePair::scanned(&offer.link, "Other");
    let step = b.send(&again.commit());
    assert!(step.reply.is_empty(), "a used code answers nothing");
    assert!(b.pair(&mut again).is_none());
}

#[test]
fn code_expires_after_5_minutes() {
    let mut b = bench();
    let offer = b.begin();
    b.wait(PAIR_TTL_SECS - 1);
    assert_eq!(b.phase(), PairingPhase::WaitingForScan);
    b.wait(1);
    assert_eq!(b.phase(), PairingPhase::Expired);
    let phone = PhonePair::scanned(&offer.link, "Pixel");
    assert!(b.send(&phone.commit()).reply.is_empty());
}

#[test]
fn wrong_tries_are_noted_and_never_pause() {
    let mut b = bench();
    let offer = b.begin();
    for i in 1..=TRIES {
        let wrong = crypto::code_encode(&crypto::random());
        let phone = PhonePair::typed(&wrong, HOST, "Pixel");
        let step = b.send(&phone.hello());
        assert!(step.wrong && step.reply.is_empty(), "try {i}");
    }
    assert_eq!(b.phase(), PairingPhase::WaitingForScan);
    let mut phone = PhonePair::scanned(&offer.link, "Pixel");
    assert!(
        b.pair(&mut phone).is_some(),
        "the person still adds the phone"
    );
}

#[test]
fn junk_before_the_commit_leaves_the_offer_open() {
    let mut b = bench();
    let offer = b.begin();
    let mut good = PhonePair::scanned(&offer.link, "Pixel");
    let mut bad = good.commit();
    let last = bad.len() - 1;
    bad[last] ^= 1;
    for _ in 0..TRIES {
        assert!(b.send(&bad).wrong);
    }
    assert_eq!(b.phase(), PairingPhase::WaitingForScan);
    assert!(b.pair(&mut good).is_some());
    b.begin();
}

#[test]
fn second_commit_spends_the_offer() {
    let mut b = bench();
    let offer = b.begin();
    let phone = PhonePair::scanned(&offer.link, "Pixel");
    assert_eq!(b.send(&phone.commit()).reply.len(), 1);
    let step = b.send(&phone.commit());
    assert!(step.reply.is_empty());
    assert_eq!(b.phase(), PairingPhase::Spent);
    assert!(b.send(&phone.reveal()).request.is_none());
}

#[test]
fn one_nonce_per_offer() {
    let mut b = bench();
    let offer = b.begin();
    let one = PhonePair::scanned(&offer.link, "Pixel");
    let two = PhonePair::scanned(&offer.link, "Pixel");
    let nonces: usize = [one.commit(), two.commit()]
        .iter()
        .map(|f| b.send(f).reply.len())
        .sum();
    assert_eq!(nonces, 1);
}

#[test]
fn refused_leaves_no_device() {
    let mut b = bench();
    let offer = b.begin();
    let mut phone = PhonePair::scanned(&offer.link, "Pixel");
    b.pair(&mut phone).unwrap();
    let (_, frame) = b.p.decide(None).unwrap();
    let out = phone.take_outcome(&frame).unwrap();
    assert!(!out.allowed);
    assert_eq!(out.device_id, None);
    assert_eq!(b.phase(), PairingPhase::Refused);
    assert_eq!(
        b.p.waiting("anything", b.now).err(),
        Some(PairError::NoRequest)
    );
}

#[test]
fn undecided_after_2_minutes_times_out() {
    let mut b = bench();
    let offer = b.begin();
    let mut phone = PhonePair::scanned(&offer.link, "Pixel");
    let req = b.pair(&mut phone).unwrap();
    assert_eq!(req.decide_by, "2026-11-13T07:02:00.000Z");
    b.wait(DECIDE_TTL_SECS);
    let (changed, sent) = b.p.tick(b.now);
    assert!(changed);
    let (_, frame) = sent.unwrap();
    assert!(!phone.take_outcome(&frame).unwrap().allowed);
    assert_eq!(b.phase(), PairingPhase::TimedOut);
    assert!(b.p.decide(Some("late")).is_none());
}

#[test]
fn each_pairing_kind_once() {
    let mut b = bench();
    let offer = b.begin();
    let mut phone = PhonePair::scanned(&offer.link, "Pixel");
    assert!(b.pair(&mut phone).is_some());
    let step = b.send(&phone.reveal());
    assert!(step.request.is_none() && step.reply.is_empty());
}

#[test]
fn reveal_after_10_seconds_spends_the_offer() {
    let mut b = bench();
    let offer = b.begin();
    let mut phone = PhonePair::scanned(&offer.link, "Pixel");
    let step = b.send(&phone.commit());
    phone.take_nonce(&step.reply[0]).unwrap();
    b.wait(REVEAL_WINDOW_SECS);
    assert!(b.send(&phone.reveal()).request.is_none());
    assert_eq!(b.phase(), PairingPhase::Spent);
}

#[test]
fn reload_never_repeats_a_nonce() {
    let mut b = bench();
    let offer = b.begin();
    let first = PhonePair::scanned(&offer.link, "Pixel");
    let second = PhonePair::scanned(&offer.link, "Pixel");
    let mut frames = vec![first.commit()];
    frames.extend(b.send(&frames[0].clone()).reply);
    let reload = second.commit();
    assert!(b.send(&reload).reply.is_empty());
    frames.push(reload);
    assert_eq!(b.phase(), PairingPhase::Spent);
    let nonces: HashSet<Vec<u8>> = frames
        .iter()
        .map(|f| f[HEADER_LEN..HEADER_LEN + GCM_NONCE_LEN].to_vec())
        .collect();
    assert_eq!(nonces.len(), frames.len());
    assert_ne!(first.np, second.np);
}

#[test]
fn typed_hello_only_while_offer_open() {
    let mut b = bench();
    let offer = b.begin();
    let phone = PhonePair::typed(&offer.typed_code, HOST, "Pixel");
    b.p.cancel();
    assert!(b.send(&phone.hello()).reply.is_empty());
    let offer = b.begin();
    let phone = PhonePair::typed(&offer.typed_code, HOST, "Pixel");
    b.wait(PAIR_TTL_SECS);
    assert!(b.send(&phone.hello()).reply.is_empty());
}

#[test]
fn typed_hello_has_no_name() {
    let mut b = bench();
    let offer = b.begin();
    let phone = PhonePair::typed(&offer.typed_code, HOST, "Pixel");
    let answer = b.send(&phone.hello()).reply.remove(0);
    assert_eq!(answer.len(), HEADER_LEN + PUBKEY_LEN);
    let name = super::super::name::computer_name();
    assert!(!answer.windows(name.len()).any(|w| w == name.as_bytes()));
    assert_eq!(
        &answer[HEADER_LEN..],
        crypto::public_bytes(&b.c.public_key())
    );
}

#[test]
fn typed_and_scanned_reach_the_same_keys() {
    let mut b = bench();
    let offer = b.begin();
    let scanned = PhonePair::scanned(&offer.link, "Pixel");
    let mut typed = PhonePair::typed(&offer.typed_code.to_lowercase(), HOST, "Pixel");
    let answer = b.send(&typed.hello()).reply.remove(0);
    typed.take_hello(&answer).unwrap();
    assert_eq!(typed.pid, scanned.pid);
    assert_eq!(typed.keys.up.to_vec(), scanned.keys.up.to_vec());
    assert_eq!(typed.keys.down.to_vec(), scanned.keys.down.to_vec());
    let req = b.pair(&mut typed).unwrap();
    assert_eq!(req.check_phrase, typed.words());
}

#[test]
fn typed_relay_cannot_pair_phone_to_itself() {
    let mut b = bench();
    let offer = b.begin();
    let mut phone = PhonePair::typed(&offer.typed_code, HOST, "Pixel");
    // The relay answers HELLO with its own key.
    let relay = crypto::new_secret();
    phone
        .take_hello(&crypto::hello_answer(
            &phone.pid,
            &crypto::public_bytes(&relay.public_key()),
        ))
        .unwrap();
    let step = b.send(&phone.commit());
    assert!(step.reply.is_empty() && step.wrong);
    assert!(b.p.waiting(&offer.pairing_id, b.now).is_err());
}

#[test]
fn look_alike_host_changes_words() {
    let mut b = bench();
    let offer = b.begin();
    let mut phone = PhonePair::scanned(&offer.link, "Pixel");
    let req = b.pair(&mut phone).unwrap();
    assert_eq!(req.check_phrase, phone.words());
    phone.host = format!("{}.evil.example", &HOST[..20]);
    assert_ne!(req.check_phrase, phone.words());
}

#[test]
fn name_bound_by_commitment() {
    let mut b = bench();
    let offer = b.begin();
    let mut phone = PhonePair::scanned(&offer.link, "Pixel");
    // The commitment covers "Pixel" but the name sent with it is another one.
    let c = crypto::commit(&phone.dpub(), &phone.np, b"Pixel");
    let body = serde_json::to_vec(&crypto::PairCommit {
        path: PATH_SCANNED,
        device_name: "Someone else".into(),
        commit: crypto::b64(&c),
    })
    .unwrap();
    let step = b.send(&phone.seal_up(KIND_PAIR_COMMIT, &body));
    phone.take_nonce(&step.reply[0]).unwrap();
    assert!(b.send(&phone.reveal()).request.is_none());
    assert_eq!(b.phase(), PairingPhase::Spent);
}

#[test]
fn device_name_rules() {
    for (name, ok) in [
        ("Pixel 9", true),
        ("Ana\u{2019}s iPhone", true),
        (&"x".repeat(40), true),
        (&"x".repeat(41), false),
        ("", false),
        ("Pixel\u{202e}9", false),
        ("Pixel\n9", false),
    ] {
        let mut b = bench();
        let offer = b.begin();
        let mut phone = PhonePair::scanned(&offer.link, name);
        assert_eq!(b.pair(&mut phone).is_some(), ok, "{name:?}");
    }
}

#[test]
fn dpub_equal_cpub_refused() {
    let mut b = bench();
    let offer = b.begin();
    let mut phone = PhonePair::scanned(&offer.link, "Pixel");
    // A phone claiming the computer's own key: only the computer's secret could make it, so
    // the test borrows it.
    phone.d = (*b.c).clone();
    assert!(b.pair(&mut phone).is_none());
    assert_eq!(b.phase(), PairingPhase::Spent);
}

#[test]
fn grinding_needs_the_commitment() {
    let mut b = bench();
    let offer = b.begin();
    let mut phone = PhonePair::scanned(&offer.link, "Pixel");
    let step = b.send(&phone.commit());
    phone.take_nonce(&step.reply[0]).unwrap();
    // Seeing nc, try another np (or key) for chosen words: the commitment no longer matches.
    phone.np = crypto::random();
    assert!(b.send(&phone.reveal()).request.is_none());
    assert_eq!(b.phase(), PairingPhase::Spent);
}

#[test]
fn home_origin_set_by_computer() {
    let mut b = bench();
    let offer = b.begin();
    let mut phone = PhonePair::scanned(&offer.link, "Pixel");
    let step = b.send(&phone.commit());
    phone.take_nonce(&step.reply[0]).unwrap();
    let body = serde_json::json!({
        "dpub": crypto::b64(&phone.dpub()),
        "np": crypto::b64(&phone.np),
        "origin": "https://evil.example",
    });
    let claimed = phone.seal_up(KIND_PAIR_REVEAL, body.to_string().as_bytes());
    assert!(
        b.send(&claimed).request.is_none(),
        "an extra field is refused"
    );
    let mut b = bench();
    let offer = b.begin();
    let mut phone = PhonePair::scanned(&offer.link, "Pixel");
    let req = b.pair(&mut phone).unwrap();
    let dev = b.p.waiting(&req.pairing_id, b.now).unwrap();
    assert_eq!(dev.home_origin, format!("https://{HOST}"));
}

#[test]
fn one_pairing_at_a_time() {
    let mut b = bench();
    let first = b.begin();
    let second = b.begin();
    assert_ne!(first.pairing_id, second.pairing_id);
    let old = PhonePair::scanned(&first.link, "Pixel");
    let step = b.send(&old.commit());
    assert!(step.reply.is_empty() && step.wrong);
}

#[test]
fn secret_only_after_the_fragment_mark() {
    let mut b = bench();
    let offer = b.begin();
    let (before, after) = offer.link.split_once('#').unwrap();
    assert_eq!(before, format!("https://{HOST}{PATH_PAIR}"));
    let s = PhonePair::scanned(&offer.link, "Pixel").s;
    assert!(after.contains(&crypto::b64(&s)));
    assert!(!before.contains(&crypto::b64(&s)));
    assert_eq!(offer.typed_address, format!("{HOST}{PATH_PAIR}"));
    assert_eq!(offer.typed_code.len(), CODE_LEN + 3);
}

#[test]
fn qr_decodes_to_the_link() {
    let mut b = bench();
    let offer = b.begin();
    let want =
        qrcode::QrCode::with_error_correction_level(offer.link.as_bytes(), qrcode::EcLevel::M)
            .unwrap();
    assert_eq!(offer.qr.size as usize, want.width());
    let dark: String = want
        .to_colors()
        .iter()
        .map(|c| if *c == qrcode::Color::Dark { '1' } else { '0' })
        .collect();
    assert_eq!(offer.qr.modules, dark);
    assert_eq!(offer.qr.modules.len(), want.width() * want.width());
}

#[test]
fn offer_waiting_for_you_survives_foreign_frames() {
    let mut b = bench();
    let offer = b.begin();
    let mut phone = PhonePair::scanned(&offer.link, "Pixel");
    let req = b.pair(&mut phone).unwrap();
    let mut junk = phone.reveal();
    let last = junk.len() - 1;
    junk[last] ^= 1;
    for _ in 0..TRIES {
        let other = PhonePair::typed(&crypto::code_encode(&crypto::random()), HOST, "X");
        for frame in [other.hello(), other.commit(), junk.clone(), phone.commit()] {
            let step = b.send(&frame);
            assert!(!step.wrong && step.reply.is_empty() && step.request.is_none());
        }
    }
    assert!(matches!(b.phase(), PairingPhase::WaitingForYou { .. }));
    b.p.waiting(&req.pairing_id, b.now).unwrap();
    assert!(b.p.decide(Some("AAAAAAAAAAAAAAAAAAAAAA")).is_some());
}
