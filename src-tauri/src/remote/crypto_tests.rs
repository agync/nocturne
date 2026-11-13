use std::collections::BTreeSet;

use serde_json::Value;

use super::super::protocol::reference::{self, Vector, Vectors, hex};
use super::*;

fn arr<const N: usize>(b: &[u8]) -> [u8; N] {
    b.try_into().expect("fixed length")
}

fn secret(v: &Vectors, name: &str) -> SecretKey {
    SecretKey::from_slice(&v.key_scalar(name)).unwrap()
}

fn public(bytes: &[u8]) -> PublicKey {
    check_public_key(bytes).unwrap()
}

fn key32(b: &[u8]) -> [u8; 32] {
    arr(b)
}

fn dir(v: &Vector<'_>) -> Dir {
    match v.input_str("direction") {
        "up" => Dir::Up,
        _ => Dir::Down,
    }
}

fn refusal(code: &str) -> Refusal {
    let all = [
        Refusal::BadPublicKey,
        Refusal::SamePublicKey,
        Refusal::BadCode,
        Refusal::BadDeviceName,
        Refusal::TooShort,
        Refusal::BadVersion,
        Refusal::WrongKind,
        Refusal::WrongSid,
        Refusal::BadCounter,
        Refusal::CounterGap,
        Refusal::BadTag,
        Refusal::BadProof,
        Refusal::CommitMismatch,
        Refusal::BadPadding,
        Refusal::BadEnvelope,
        Refusal::ChunkGap,
        Refusal::BadMagic,
        Refusal::BadId,
    ];
    *all.iter()
        .find(|r| r.code() == code)
        .unwrap_or_else(|| panic!("no refusal {code}"))
}

fn registry(v: &Vector<'_>) -> impl Fn(&DeviceId) -> Option<(Vec<u8>, ())> + use<> {
    let map: Vec<(Vec<u8>, Vec<u8>)> = v
        .input("registry")
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, d)| (hex::decode(k), hex::decode(d.as_str().unwrap())))
        .collect();
    move |id: &DeviceId| {
        map.iter()
            .find(|(k, _)| k.as_slice() == id)
            .map(|(_, d)| (d.clone(), ()))
    }
}

fn words_input<'a>(
    v: &'a Vector<'a>,
    bufs: &'a [Vec<u8>; 5],
    pid: &'a PairingId,
) -> WordsInput<'a> {
    WordsInput {
        path: v.input("path").as_u64().unwrap() as u8,
        pid,
        host: v.input_str("host"),
        cpub: &bufs[0],
        dpub: &bufs[1],
        name: &bufs[2],
        np: &bufs[3],
        nc: &bufs[4],
    }
}

fn positive(v: &Vectors, p: &Vector<'_>) {
    match p.step {
        "code" => {
            let s: [u8; S_LEN] = arr(&p.input_hex("s"));
            let code = code_encode(&s);
            assert_eq!(code, p.output("code").as_str().unwrap());
            assert_eq!(code_display(&code), p.output("display").as_str().unwrap());
            assert_eq!(code_decode(&code).unwrap(), s);
        }
        "codeNormalise" => {
            let typed = p.input_str("typed");
            assert_eq!(
                code_normalise(typed).unwrap(),
                p.output("code").as_str().unwrap()
            );
            assert_eq!(code_decode(typed).unwrap().to_vec(), p.output_hex("s"));
        }
        "pairingId" => {
            let s = arr(&p.input_hex("s"));
            assert_eq!(pairing_id(&s).to_vec(), p.output_hex("pairingId"));
        }
        "link" => {
            let s = arr(&p.input_hex("s"));
            assert_eq!(
                pairing_link(p.input_str("host"), &s, &p.input_hex("cpub")),
                p.output("link").as_str().unwrap()
            );
        }
        "pairKeys" => {
            let k = pair_keys(
                &arr(&p.input_hex("s")),
                &p.input_hex("cpub"),
                &arr(&p.input_hex("pairingId")),
            );
            assert_eq!(k.up.to_vec(), p.output_hex("up"));
            assert_eq!(k.down.to_vec(), p.output_hex("down"));
        }
        "hello" => {
            let pid = arr(&p.input_hex("pairingId"));
            let req = hello_request(&pid);
            assert_eq!(req, p.intermediate_hex("request"));
            assert_eq!(hello_request_open(&req).unwrap(), pid);
            let ans = hello_answer(&pid, &p.input_hex("cpub"));
            assert_eq!(ans, p.output_hex("answer"));
            assert_eq!(
                public_bytes(&hello_answer_open(&ans, &pid).unwrap()).to_vec(),
                p.input_hex("cpub")
            );
        }
        "commit" => {
            let name = check_device_name(p.input_str("deviceName")).unwrap();
            assert_eq!(name, p.input_hex("deviceNameUtf8").as_slice());
            assert_eq!(
                commit(&p.input_hex("dpub"), &p.input_hex("np"), name).to_vec(),
                p.output_hex("commit")
            );
        }
        "pairFrame" => {
            let kind = p.input("kind").as_u64().unwrap() as u8;
            let pid = arr(&p.input_hex("pairingId"));
            let key = key32(&p.input_hex("key"));
            let body = p.input_str("body").as_bytes();
            let env = envelope_json(body);
            assert_eq!(env, p.intermediate_hex("envelope"));
            let frame = pair_seal_with(kind, &pid, &key, &arr(&p.input_hex("nonce")), &env);
            assert_eq!(frame, p.output_hex("frame"));
            assert_eq!(
                pair_open(&frame, kind, &pid, &key).unwrap(),
                Envelope::Json(body.to_vec())
            );
        }
        "words" => {
            let pid = arr(&p.input_hex("pairingId"));
            let bufs = [
                p.input_hex("cpub"),
                p.input_hex("dpub"),
                p.input_str("deviceName").as_bytes().to_vec(),
                p.input_hex("np"),
                p.input_hex("nc"),
            ];
            let w = words_input(p, &bufs, &pid);
            let shared = ecdh(&secret(v, "C"), &public(&v.key_public("D")));
            assert_eq!(shared.to_vec(), p.intermediate_hex("ecdhCD"));
            assert_eq!(words_salt(&w).to_vec(), p.intermediate_hex("salt"));
            let idx: Vec<u64> = word_indices(shared.as_ref(), &words_salt(&w))
                .iter()
                .map(|i| *i as u64)
                .collect();
            let want: Vec<u64> = p
                .output("indices")
                .as_array()
                .unwrap()
                .iter()
                .map(|i| i.as_u64().unwrap())
                .collect();
            assert_eq!(idx, want);
            let got: Vec<&str> = words(shared.as_ref(), &w).to_vec();
            let want: Vec<&str> = p
                .output("words")
                .as_array()
                .unwrap()
                .iter()
                .map(|w| w.as_str().unwrap())
                .collect();
            assert_eq!(got, want);
        }
        "ecdh" => {
            let a = SecretKey::from_slice(&hex::decode(p.input("a")["scalar"].as_str().unwrap()))
                .unwrap();
            let b = public(&hex::decode(p.input("b")["public"].as_str().unwrap()));
            let shared = ecdh(&a, &b);
            assert_eq!(shared.to_vec(), p.output_hex("shared"));
            assert_eq!(shared.len(), 32);
            assert_eq!(
                device_key(shared.as_ref(), &arr(&p.input_hex("deviceId"))).to_vec(),
                p.intermediate_hex("kd")
            );
        }
        "deviceKey" => {
            let shared = ecdh(&secret(v, "C"), &public(&p.input_hex("dpub")));
            let phone = ecdh(&secret(v, "D"), &public(&p.input_hex("cpub")));
            assert_eq!(shared.to_vec(), phone.to_vec());
            assert_eq!(shared.to_vec(), p.intermediate_hex("ecdhDC"));
            assert_eq!(
                device_key(shared.as_ref(), &arr(&p.input_hex("deviceId"))).to_vec(),
                p.output_hex("kd")
            );
        }
        "sessionOpen" => {
            let id: DeviceId = arr(&p.input_hex("deviceId"));
            let frame = session_open(
                &id,
                &p.input_hex("epub"),
                &arr(&p.input_hex("openNonce")),
                &key32(&p.input_hex("kd")),
            );
            assert_eq!(frame, p.output_hex("frame"));
            let dpub = v.key_public("D");
            let ok = verify_open(
                &frame,
                |got| (got == &id).then_some((dpub, ())),
                &secret(v, "C"),
            )
            .unwrap_or_else(|r| panic!("refused: {r:?}"));
            assert_eq!(ok.device_id, id);
            assert_eq!(ok.kd.to_vec(), p.input_hex("kd"));
            assert_eq!(ok.on.to_vec(), p.input_hex("openNonce"));
        }
        "sessionAccept" => {
            let sid = arr(&p.input_hex("sid"));
            let frame = session_accept(&sid, &p.input_hex("fpub"));
            assert_eq!(frame, p.output_hex("frame"));
            let (got, f) = session_accept_open(&frame).unwrap();
            assert_eq!(got, sid);
            assert_eq!(public_bytes(&f).to_vec(), p.input_hex("fpub"));
        }
        "sessionKeys" => {
            let (id, sid) = (arr(&p.input_hex("deviceId")), arr(&p.input_hex("sid")));
            let (epub, fpub, cpub, dpub) = (
                p.input_hex("epub"),
                p.input_hex("fpub"),
                p.input_hex("cpub"),
                p.input_hex("dpub"),
            );
            let on = p.input_hex("openNonce");
            let s = SessionInput {
                device_id: &id,
                epub: &epub,
                on: &on,
                sid: &sid,
                fpub: &fpub,
                cpub: &cpub,
                dpub: &dpub,
            };
            let c = session_keys_computer(
                &secret(v, "F"),
                &secret(v, "C"),
                &public(&epub),
                &public(&dpub),
                &s,
            );
            let ph = session_keys_phone(
                &secret(v, "E"),
                &secret(v, "D"),
                &public(&fpub),
                &public(&cpub),
                &s,
            );
            for k in [&c, &ph] {
                assert_eq!(k.up.to_vec(), p.output_hex("up"));
                assert_eq!(k.down.to_vec(), p.output_hex("down"));
            }
        }
        "revoked" => {
            let (kd, id, on) = (
                key32(&p.input_hex("kd")),
                arr(&p.input_hex("deviceId")),
                arr(&p.input_hex("openNonce")),
            );
            let frame = revoked_with(&kd, &id, &on, &arr(&p.input_hex("nonce")));
            assert_eq!(frame, p.output_hex("frame"));
            revoked_open(&frame, &kd, &id, &on).unwrap();
        }
        "envelope" => {
            let env = match p.input("type").as_u64().unwrap() as u8 {
                TYPE_JSON => {
                    let body = p.input_str("body").as_bytes().to_vec();
                    let env = envelope_json(&body);
                    assert_eq!(envelope_open(&env).unwrap(), Envelope::Json(body));
                    env
                }
                _ => {
                    let bytes = p.input_hex("bytes");
                    let call = p.input("callId").as_u64().unwrap() as u32;
                    let seq = p.input("seq").as_u64().unwrap() as u32;
                    let last = p.input("last").as_bool().unwrap();
                    let env = envelope_chunk(call, seq, last, &bytes);
                    assert_eq!(
                        envelope_open(&env).unwrap(),
                        Envelope::Chunk(Chunk {
                            call_id: call,
                            seq,
                            last,
                            bytes
                        })
                    );
                    env
                }
            };
            assert_eq!(env, p.output_hex("envelope"));
            assert_eq!(env.len() as u64, p.output("length").as_u64().unwrap());
        }
        "data" => {
            let env = match p.raw["inputs"].get("body") {
                Some(Value::String(b)) => envelope_json(b.as_bytes()),
                _ => p.input_hex("envelope"),
            };
            let (key, sid) = (key32(&p.input_hex("key")), arr(&p.input_hex("sid")));
            let ctr = p.input("ctr").as_u64().unwrap();
            let frame = data_seal(&key, dir(p), &sid, ctr, &env);
            assert_eq!(frame, p.output_hex("frame"));
            let (got, opened) = data_open(&frame, &key, dir(p), &sid, ctr - 1).unwrap();
            assert_eq!(got, ctr);
            assert_eq!(opened, envelope_open(&env).unwrap());
        }
        "magic" => {
            magic_matches(p.input_str("mime"), &p.input_hex("bytes")).unwrap();
        }
        "deviceName" => {
            assert_eq!(
                check_device_name(p.input_str("deviceName")).unwrap(),
                p.output_hex("utf8").as_slice()
            );
        }
        "id" => check_id(p.input_str("id")).unwrap(),
        "itemText" => match p.raw["inputs"].get("tail") {
            Some(Value::String(tail)) => {
                let len = p.input("length").as_u64().unwrap() as usize;
                let text = "a".repeat(len - tail.len()) + tail;
                assert_eq!(sha256(&[text.as_bytes()]).to_vec(), p.input_hex("sha256"));
                let (cut, truncated) = cut_text(&text);
                assert_eq!(truncated, p.output("truncated").as_bool().unwrap());
                assert_eq!(cut.len() as u64, p.output("length").as_u64().unwrap());
                assert_eq!(sha256(&[cut.as_bytes()]).to_vec(), p.output_hex("sha256"));
            }
            _ => {
                let worst = "\u{1}".repeat(MAX_ITEM_TEXT);
                let item = nocturne_contract::PhoneItem {
                    id: format!("itm_{}", "f".repeat(32)),
                    kind: nocturne_contract::ItemKind::Document,
                    title: "\u{1}".repeat(255),
                    kept_at: "2026-11-13T07:00:00.000Z".into(),
                    duration_ms: Some(u32::MAX),
                    pages: Some(u32::MAX),
                    has_original: true,
                    summary: Some(worst.clone()),
                    text: Some(worst),
                    truncated: true,
                };
                let reply = nocturne_contract::RemoteReply {
                    id: u32::MAX,
                    answer: nocturne_contract::RemoteAnswer::Item { item },
                };
                let env = envelope_json(&serde_json::to_vec(&reply).unwrap());
                assert_eq!(
                    env.len() as u64,
                    p.intermediate("envelopeLength").as_u64().unwrap()
                );
                assert!(env.len() <= MAX_FRAME - HEADER_LEN - GCM_TAG_LEN);
            }
        },
        other => panic!("positive vector {} has an unknown step {other}", p.name),
    }
}

fn negative(v: &Vectors, n: &Vector<'_>) -> Refusal {
    let frame = || n.input_hex("frame");
    match n.step {
        "publicKey" => check_public_key(&n.input_hex("key")).map(|_| ()),
        "reveal" => check_reveal(
            &n.input_hex("cpub"),
            &n.input_hex("dpub"),
            &n.input_hex("np"),
            n.input_str("deviceName").as_bytes(),
            &n.input_hex("commit"),
        )
        .map(|_| ()),
        "codeNormalise" => code_normalise(n.input_str("typed")).map(|_| ()),
        "deviceName" => check_device_name(n.input_str("deviceName")).map(|_| ()),
        "data" => data_open(
            &frame(),
            &key32(&n.input_hex("key")),
            dir(n),
            &arr(&n.input_hex("sid")),
            n.input("lastCtr").as_u64().unwrap(),
        )
        .map(|_| ()),
        "pairFrame" => pair_open(
            &frame(),
            n.input("kind").as_u64().unwrap() as u8,
            &arr(&n.input_hex("pairingId")),
            &key32(&n.input_hex("key")),
        )
        .map(|_| ()),
        "sessionOpen" => {
            assert_eq!(n.input_str("computer"), "C");
            verify_open(&frame(), registry(n), &secret(v, "C")).map(|_| ())
        }
        "revoked" => revoked_open(
            &frame(),
            &key32(&n.input_hex("kd")),
            &arr(&n.input_hex("deviceId")),
            &arr(&n.input_hex("pendingOpenNonce")),
        ),
        "envelope" => envelope_open(&n.input_hex("envelope")).map(|_| ()),
        "chunks" => {
            let bodies: Vec<Vec<u8>> = n
                .input("bodies")
                .as_array()
                .unwrap()
                .iter()
                .map(|b| hex::decode(b.as_str().unwrap()))
                .collect();
            let refs: Vec<&[u8]> = bodies.iter().map(Vec::as_slice).collect();
            let order = chunks_in_order(&refs);
            let mut reader = frame::ChunkOrder::default();
            let staged = bodies.iter().try_for_each(|b| {
                let mut env = vec![TYPE_CHUNK, 0, 0];
                env.extend_from_slice(b);
                match envelope_open(&env)? {
                    Envelope::Chunk(c) => reader.accept(&c),
                    Envelope::Json(_) => unreachable!(),
                }
            });
            assert_eq!(order, staged, "the reader agrees with the reference order");
            order
        }
        "magic" => magic_matches(n.input_str("mime"), &n.input_hex("bytes")),
        "id" => check_id(n.input_str("id")),
        other => panic!("negative vector {} has an unknown step {other}", n.name),
    }
    .expect_err(n.name)
}

use super::super::frame;

#[test]
fn vectors_match_reference() {
    let v = reference::load();
    let known: BTreeSet<&str> = v.refusals().into_iter().collect();
    for p in v.positives() {
        positive(&v, &p);
    }
    for n in v.negatives() {
        assert!(known.contains(n.expect()), "{}", n.name);
        let got = negative(&v, &n);
        assert_eq!(got.code(), n.expect(), "{}", n.name);
        assert_eq!(got, refusal(n.expect()));
    }
    assert!(v.positives().len() > 50 && v.negatives().len() > 70);
}

#[test]
fn public_key_checks() {
    let v = reference::load();
    let good = v.key_public("C");
    check_public_key(&good).unwrap();
    let mut off = good.clone();
    off[64] ^= 1;
    for bad in [
        &good[..64],
        &[&good[..], &[0]].concat()[..],
        &[0u8][..],
        &[],
        &[&[0x02], &good[1..33]].concat()[..],
        &[&[0x05], &good[1..]].concat()[..],
        &off[..],
        &[&[0x04], &[0u8; 64][..]].concat()[..],
    ] {
        assert_eq!(check_public_key(bad), Err(Refusal::BadPublicKey));
    }
}

#[test]
fn ecdh_keeps_leading_zeros() {
    let v = reference::load();
    let z = v.positive("ecdh_leading_zero");
    let a = SecretKey::from_slice(&hex::decode(z.input("a")["scalar"].as_str().unwrap())).unwrap();
    let b = public(&hex::decode(z.input("b")["public"].as_str().unwrap()));
    let shared = ecdh(&a, &b);
    assert_eq!(shared.len(), 32);
    assert_eq!(shared[0], 0);
}

#[test]
fn code_round_trip_and_normalise() {
    for _ in 0..64 {
        let s = random::<S_LEN>();
        let code = code_encode(&s);
        assert_eq!(code.len(), CODE_LEN);
        assert_eq!(code_decode(&code).unwrap(), s);
        let shown = code_display(&code);
        assert_eq!(shown.split(' ').count(), 4);
        assert_eq!(code_decode(&shown.to_lowercase()).unwrap(), s);
        assert_eq!(code_decode(&shown.replace(' ', "-")).unwrap(), s);
    }
    assert_eq!(code_normalise("oil").err(), Some(Refusal::BadCode));
    assert_eq!(
        code_normalise("UUUUUUUUUUUUUUUUUUUUUUUU").err(),
        Some(Refusal::BadCode)
    );
}

#[test]
fn envelope_padding_rules() {
    for len in [0usize, 1, 252, 253, 254, 500, 4000] {
        let body = vec![b'x'; len];
        let env = envelope_json(&body);
        assert_eq!(env.len() % PAD_BLOCK, 0, "{len}");
        assert!(env.len() < len + ENVELOPE_HEADER_LEN + PAD_BLOCK);
        assert_eq!(envelope_open(&env).unwrap(), Envelope::Json(body));
    }
    let mut env = envelope_json(b"{}");
    *env.last_mut().unwrap() = 1;
    assert_eq!(envelope_open(&env), Err(Refusal::BadPadding));
    let mut env = envelope_json(b"{}");
    env[1] = 0xff;
    assert_eq!(envelope_open(&env), Err(Refusal::BadPadding));
    let mut chunk = envelope_chunk(1, 0, true, b"abc");
    chunk[2] = 1;
    chunk.push(0);
    assert_eq!(envelope_open(&chunk), Err(Refusal::BadPadding));
    assert_eq!(
        envelope_open(&[TYPE_CHUNK, 0, 0, 1]),
        Err(Refusal::BadPadding)
    );
    assert_eq!(envelope_open(&[TYPE_JSON]), Err(Refusal::TooShort));
}

/// A phone, a computer and a session between them, from fresh keys.
struct World {
    c: SecretKey,
    d: SecretKey,
    id: DeviceId,
    kd: Key32,
}

fn world() -> World {
    let (c, d) = (new_secret(), new_secret());
    let id = random::<DEVICE_ID_LEN>();
    let kd = device_key(ecdh(&d, &c.public_key()).as_ref(), &id);
    World { c, d, id, kd }
}

impl World {
    fn lookup(&self) -> impl Fn(&DeviceId) -> Option<(Vec<u8>, ())> + '_ {
        |got| (got == &self.id).then(|| (public_bytes(&self.d.public_key()).to_vec(), ()))
    }

    fn open_frame(&self, epub: &[u8]) -> (Vec<u8>, [u8; OPEN_NONCE_LEN]) {
        let on = random::<OPEN_NONCE_LEN>();
        (session_open(&self.id, epub, &on, &self.kd), on)
    }

    /// Keys of both sides for one session with phone key `e` and computer key `f`.
    fn keys(
        &self,
        e: &SecretKey,
        f: &SecretKey,
        sid: &Sid,
        on: &[u8],
    ) -> (SessionKeys, SessionKeys) {
        let (epub, fpub) = (public_bytes(&e.public_key()), public_bytes(&f.public_key()));
        let (cpub, dpub) = (
            public_bytes(&self.c.public_key()),
            public_bytes(&self.d.public_key()),
        );
        let s = SessionInput {
            device_id: &self.id,
            epub: &epub,
            on,
            sid,
            fpub: &fpub,
            cpub: &cpub,
            dpub: &dpub,
        };
        (
            session_keys_computer(f, &self.c, &e.public_key(), &self.d.public_key(), &s),
            session_keys_phone(e, &self.d, &f.public_key(), &self.c.public_key(), &s),
        )
    }

    fn channels(&self) -> (frame::Channel, frame::Channel) {
        let (e, f) = (new_secret(), new_secret());
        let sid = random::<SID_LEN>();
        let (k_c, k_p) = self.keys(&e, &f, &sid, &random::<OPEN_NONCE_LEN>());
        (
            frame::Channel::new(frame::Side::Computer, sid, k_c),
            frame::Channel::new(frame::Side::Phone, sid, k_p),
        )
    }
}

#[test]
fn proof_checked_before_ecdh_on_epub() {
    let w = world();
    // An off-curve Epub under a proof that does not cover it: the proof fails first.
    let e = public_bytes(&new_secret().public_key());
    let (frame, _) = w.open_frame(&e);
    let mut bad = frame.clone();
    bad[HEADER_LEN + DEVICE_ID_LEN + PUBKEY_LEN - 1] ^= 1;
    assert_eq!(
        verify_open(&bad, w.lookup(), &w.c).err().map(|r| r.code()),
        Some("badProof")
    );
    // A good proof over an off-curve key: refused as a bad key, after the proof.
    let mut off = e;
    off[64] ^= 1;
    let (frame, _) = w.open_frame(&off);
    assert_eq!(
        verify_open(&frame, w.lookup(), &w.c).err(),
        Some(Refusal::BadPublicKey)
    );
}

#[test]
fn unknown_device_and_bad_proof_refusals_identical() {
    let w = world();
    let (frame, _) = w.open_frame(&public_bytes(&new_secret().public_key()));
    let unknown = verify_open(&frame, |_| None::<(Vec<u8>, ())>, &w.c).err();
    let mut tampered = frame.clone();
    *tampered.last_mut().unwrap() ^= 1;
    let bad_proof = verify_open(&tampered, w.lookup(), &w.c).err();
    assert_eq!(unknown, Some(Refusal::BadProof));
    assert_eq!(unknown, bad_proof);
}

#[test]
fn revoked_bound_to_its_open() {
    let w = world();
    let (_, on) = w.open_frame(&public_bytes(&new_secret().public_key()));
    let r = revoked(&w.kd, &w.id, &on);
    revoked_open(&r, &w.kd, &w.id, &on).unwrap();
    let other = random::<OPEN_NONCE_LEN>();
    assert_eq!(revoked_open(&r, &w.kd, &w.id, &other), Err(Refusal::BadTag));
    let stranger = world();
    assert_eq!(
        revoked_open(&r, &stranger.kd, &w.id, &on),
        Err(Refusal::BadTag)
    );
}

fn offer() -> ([u8; S_LEN], PairingId, SecretKey) {
    let s = random::<S_LEN>();
    (s, pairing_id(&s), new_secret())
}

#[test]
fn pair_seal_round_trip() {
    let (s, pid, c) = offer();
    let keys = pair_keys(&s, &public_bytes(&c.public_key()), &pid);
    let env = envelope_json(br#"{"nc":"x"}"#);
    let a = pair_seal(KIND_PAIR_NONCE, &pid, keys.get(Dir::Down), &env);
    let b = pair_seal(KIND_PAIR_NONCE, &pid, keys.get(Dir::Down), &env);
    assert_ne!(a, b, "a fresh nonce each time");
    for f in [a, b] {
        assert_eq!(
            pair_open(&f, KIND_PAIR_NONCE, &pid, keys.get(Dir::Down)).unwrap(),
            Envelope::Json(br#"{"nc":"x"}"#.to_vec())
        );
    }
}

#[test]
fn wrong_pair_key_does_not_open() {
    let (s, pid, c) = offer();
    let keys = pair_keys(&s, &public_bytes(&c.public_key()), &pid);
    let relay = pair_keys(&s, &public_bytes(&new_secret().public_key()), &pid);
    let env = envelope_json(b"{}");
    let f = pair_seal(KIND_PAIR_COMMIT, &pid, keys.get(Dir::Up), &env);
    for key in [keys.get(Dir::Down), relay.get(Dir::Up)] {
        assert_eq!(
            pair_open(&f, KIND_PAIR_COMMIT, &pid, key),
            Err(Refusal::BadTag)
        );
    }
}

#[test]
fn reveal_must_match_commit() {
    let (c, d) = (new_secret(), new_secret());
    let (cpub, dpub) = (public_bytes(&c.public_key()), public_bytes(&d.public_key()));
    let np = random::<NONCE_PAIR_LEN>();
    let committed = commit(&dpub, &np, b"Pixel");
    check_reveal(&cpub, &dpub, &np, b"Pixel", &committed).unwrap();
    let other = public_bytes(&new_secret().public_key());
    for (dpub, np, name) in [
        (&other[..], &np[..], &b"Pixel"[..]),
        (&dpub[..], &random::<NONCE_PAIR_LEN>()[..], &b"Pixel"[..]),
        (&dpub[..], &np[..], &b"Pixel 2"[..]),
    ] {
        assert_eq!(
            check_reveal(&cpub, dpub, np, name, &committed).err(),
            Some(Refusal::CommitMismatch)
        );
    }
    let same = commit(&cpub, &np, b"Pixel");
    assert_eq!(
        check_reveal(&cpub, &cpub, &np, b"Pixel", &same).err(),
        Some(Refusal::SamePublicKey)
    );
}

fn some_words(nc: &[u8], np: &[u8]) -> [&'static str; 3] {
    let v = reference::load();
    let (c, d) = (secret(&v, "C"), secret(&v, "D"));
    let (cpub, dpub) = (public_bytes(&c.public_key()), public_bytes(&d.public_key()));
    let shared = ecdh(&c, &d.public_key());
    let pid = [7u8; PAIRING_ID_LEN];
    words(
        shared.as_ref(),
        &WordsInput {
            path: PATH_SCANNED,
            pid: &pid,
            host: "a.example",
            cpub: &cpub,
            dpub: &dpub,
            name: b"Pixel",
            np,
            nc,
        },
    )
}

#[test]
fn words_are_three_from_the_list() {
    for _ in 0..32 {
        let got = some_words(&random::<32>(), &random::<32>());
        assert_eq!(got.len(), 3);
        assert!(got.iter().all(|w| WORDS.contains(w)));
    }
}

#[test]
fn words_change_with_either_nonce() {
    let (np, nc) = (random::<32>(), random::<32>());
    let base = some_words(&nc, &np);
    let mut changed = (0, 0);
    for _ in 0..8 {
        changed.0 += usize::from(some_words(&random::<32>(), &np) != base);
        changed.1 += usize::from(some_words(&nc, &random::<32>()) != base);
    }
    assert!(changed.0 >= 7 && changed.1 >= 7, "{changed:?}");
}

#[test]
fn session_keys_differ_by_direction() {
    let w = world();
    let (e, f) = (new_secret(), new_secret());
    let (c, p) = w.keys(&e, &f, &random::<SID_LEN>(), &random::<OPEN_NONCE_LEN>());
    assert_ne!(c.up.to_vec(), c.down.to_vec());
    assert_eq!(c.up.to_vec(), p.up.to_vec());
    assert_eq!(c.down.to_vec(), p.down.to_vec());
}

#[test]
fn session_keys_change_with_f() {
    let w = world();
    let e = new_secret();
    let (sid, on) = (random::<SID_LEN>(), random::<OPEN_NONCE_LEN>());
    let (a, _) = w.keys(&e, &new_secret(), &sid, &on);
    let (b, _) = w.keys(&e, &new_secret(), &sid, &on);
    assert_ne!(a.up.to_vec(), b.up.to_vec());
    assert_ne!(a.down.to_vec(), b.down.to_vec());
}

#[test]
fn replayed_frame_refused() {
    let (mut computer, mut phone) = world().channels();
    let f = phone.seal(&envelope_json(b"{}")).unwrap();
    computer.open(&f).unwrap();
    assert_eq!(computer.open(&f), Err(Refusal::CounterGap));
    assert!(computer.closed().is_some());
}

#[test]
fn reordered_frame_refused() {
    let (mut computer, mut phone) = world().channels();
    let a = phone.seal(&envelope_json(b"1")).unwrap();
    let b = phone.seal(&envelope_json(b"2")).unwrap();
    assert_eq!(computer.open(&b), Err(Refusal::CounterGap));
    assert_eq!(
        computer.open(&a),
        Err(Refusal::CounterGap),
        "closed for good"
    );
}

#[test]
fn counter_gap_closes_session() {
    let (mut computer, mut phone) = world().channels();
    let _skipped = phone.seal(&envelope_json(b"1")).unwrap();
    let next = phone.seal(&envelope_json(b"2")).unwrap();
    assert_eq!(computer.open(&next), Err(Refusal::CounterGap));
    assert_eq!(computer.closed(), Some(Refusal::CounterGap));
    assert!(
        computer.seal(b"x").is_none(),
        "a closed channel seals nothing"
    );
    assert_eq!(
        computer.received(),
        0,
        "a refused frame never moves the counter"
    );
}

#[test]
fn tampered_header_refused() {
    let (mut computer, mut phone) = world().channels();
    let mut f = phone.seal(&envelope_json(b"{}")).unwrap();
    f[1] = KIND_SESSION_OPEN;
    assert_eq!(computer.open(&f), Err(Refusal::WrongKind));
    let (mut computer, mut phone) = world().channels();
    let mut f = phone.seal(&envelope_json(b"{}")).unwrap();
    f[5] ^= 1;
    assert_eq!(computer.open(&f), Err(Refusal::WrongSid));
}

#[test]
fn tampered_body_refused() {
    let (mut computer, mut phone) = world().channels();
    let mut f = phone.seal(&envelope_json(b"{}")).unwrap();
    f[HEADER_LEN + 3] ^= 0x40;
    assert_eq!(computer.open(&f), Err(Refusal::BadTag));
}

#[test]
fn wrong_session_key_refused() {
    let w = world();
    let (mut computer, _) = w.channels();
    let (_, mut other_phone) = w.channels();
    let f = other_phone.seal(&envelope_json(b"{}")).unwrap();
    // Another session's frame under this sid still fails on the key.
    let mut f = f;
    f[2..18].copy_from_slice(computer.sid());
    assert_eq!(computer.open(&f), Err(Refusal::BadTag));
}

#[test]
fn open_proof_nonce_is_random() {
    let w = world();
    let epub = public_bytes(&new_secret().public_key());
    let seen: BTreeSet<Vec<u8>> = (0..16)
        .map(|_| w.open_frame(&epub).0[HEADER_LEN + DEVICE_ID_LEN + PUBKEY_LEN..][..12].to_vec())
        .collect();
    assert_eq!(seen.len(), 16);
}

#[test]
fn computer_key_owner_only() {
    let dir = tempfile::tempdir().unwrap();
    let remote = dir.path().join("remote");
    assert!(!remote.join(COMPUTER_KEY_FILE).exists());
    let a = load_or_create_computer_key(&remote).unwrap();
    let b = load_or_create_computer_key(&remote).unwrap();
    assert_eq!(a.to_bytes(), b.to_bytes(), "made on first use only");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&remote.join(COMPUTER_KEY_FILE)), 0o600);
        assert_eq!(mode(&remote), 0o700);
    }
    let c = replace_computer_key(&remote).unwrap();
    assert_ne!(a.to_bytes(), c.to_bytes());
}
