//! The sealed channel's pure steps: key checks, the typed code, pairing keys and frames, the
//! commitment, the check words, the device key, session open, accept and keys, REVOKED, the
//! plaintext envelope and DATA frames.
//!
//! Plan 12 "Sealed channel" is the description and `src-tauri/tests/fixtures/remote-vectors.json`
//! the reference; every step here reproduces it byte for byte, and every refusal names the code
//! and follows the check order the fixture states. Nothing here does I/O; the computer's key
//! file lives in the shell.

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use hkdf::Hkdf;
use p256::ecdh::diffie_hellman;
use p256::{PublicKey, SecretKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

use crate::protocol::*;

/// Why a frame, key or input was refused. The names are the fixture's refusal codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Refusal {
    BadPublicKey,
    SamePublicKey,
    BadCode,
    BadDeviceName,
    TooShort,
    BadVersion,
    WrongKind,
    WrongSid,
    BadCounter,
    CounterGap,
    BadTag,
    BadProof,
    CommitMismatch,
    BadPadding,
    BadEnvelope,
    ChunkGap,
    BadMagic,
    BadId,
}

impl Refusal {
    pub fn code(self) -> &'static str {
        match self {
            Refusal::BadPublicKey => "badPublicKey",
            Refusal::SamePublicKey => "samePublicKey",
            Refusal::BadCode => "badCode",
            Refusal::BadDeviceName => "badDeviceName",
            Refusal::TooShort => "tooShort",
            Refusal::BadVersion => "badVersion",
            Refusal::WrongKind => "wrongKind",
            Refusal::WrongSid => "wrongSid",
            Refusal::BadCounter => "badCounter",
            Refusal::CounterGap => "counterGap",
            Refusal::BadTag => "badTag",
            Refusal::BadProof => "badProof",
            Refusal::CommitMismatch => "commitMismatch",
            Refusal::BadPadding => "badPadding",
            Refusal::BadEnvelope => "badEnvelope",
            Refusal::ChunkGap => "chunkGap",
            Refusal::BadMagic => "badMagic",
            Refusal::BadId => "badId",
        }
    }
}

pub type Checked<T> = std::result::Result<T, Refusal>;

pub type Key32 = Zeroizing<[u8; 32]>;
pub type Sid = [u8; SID_LEN];
pub type PairingId = [u8; PAIRING_ID_LEN];
pub type DeviceId = [u8; DEVICE_ID_LEN];

/// Which way a frame travels: up is phone to computer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    Up,
    Down,
}

// Primitives --------------------------------------------------------------------------------

pub fn random<const N: usize>() -> [u8; N] {
    let mut out = [0u8; N];
    getrandom::fill(&mut out).expect("the operating system provides random bytes");
    out
}

pub fn sha256(parts: &[&[u8]]) -> [u8; 32] {
    let mut h = Sha256::new();
    for p in parts {
        h.update(p);
    }
    h.finalize().into()
}

fn hkdf(ikm: &[u8], salt: &[u8], info: &str, out: &mut [u8]) {
    Hkdf::<Sha256>::new(Some(salt), ikm)
        .expand(info.as_bytes(), out)
        .expect("HKDF output lengths here are far below the limit");
}

fn hkdf32(ikm: &[u8], salt: &[u8], info: &str) -> Key32 {
    let mut out = Zeroizing::new([0u8; 32]);
    hkdf(ikm, salt, info, out.as_mut());
    out
}

fn split64(ikm: &[u8], salt: &[u8], info: &str) -> (Key32, Key32) {
    let mut okm = Zeroizing::new([0u8; 64]);
    hkdf(ikm, salt, info, okm.as_mut());
    let mut up = Zeroizing::new([0u8; 32]);
    let mut down = Zeroizing::new([0u8; 32]);
    up.copy_from_slice(&okm[..32]);
    down.copy_from_slice(&okm[32..]);
    (up, down)
}

fn seal(key: &[u8; 32], nonce: &[u8], aad: &[u8], msg: &[u8]) -> Vec<u8> {
    Aes256Gcm::new_from_slice(key)
        .expect("a 32-byte key")
        .encrypt(
            &Nonce::try_from(nonce).expect("a 12-byte nonce"),
            Payload { msg, aad },
        )
        .expect("sealing in memory does not fail")
}

fn open(key: &[u8; 32], nonce: &[u8], aad: &[u8], msg: &[u8]) -> Checked<Zeroizing<Vec<u8>>> {
    let nonce = Nonce::try_from(nonce).map_err(|_| Refusal::BadTag)?;
    Aes256Gcm::new_from_slice(key)
        .expect("a 32-byte key")
        .decrypt(&nonce, Payload { msg, aad })
        .map(Zeroizing::new)
        .map_err(|_| Refusal::BadTag)
}

fn lp(x: &[u8]) -> Vec<u8> {
    let len = u16::try_from(x.len()).expect("length-prefixed fields are short");
    [&len.to_be_bytes()[..], x].concat()
}

pub fn b64(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

pub fn unb64(text: &str) -> Option<Vec<u8>> {
    URL_SAFE_NO_PAD.decode(text).ok()
}

pub fn eq_ct(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && bool::from(a.ct_eq(b))
}

// Keys ---------------------------------------------------------------------------------------

/// A 65-byte uncompressed point on P-256: length and the `0x04` prefix first, then the parse.
pub fn check_public_key(bytes: &[u8]) -> Checked<PublicKey> {
    if bytes.len() != PUBKEY_LEN || bytes[0] != 0x04 {
        return Err(Refusal::BadPublicKey);
    }
    PublicKey::from_sec1_bytes(bytes).map_err(|_| Refusal::BadPublicKey)
}

pub fn public_bytes(key: &PublicKey) -> [u8; PUBKEY_LEN] {
    let point = key.to_sec1_bytes();
    let mut out = [0u8; PUBKEY_LEN];
    out.copy_from_slice(&point);
    out
}

pub fn new_secret() -> SecretKey {
    loop {
        let bytes = Zeroizing::new(random::<32>());
        if let Ok(key) = SecretKey::from_slice(bytes.as_ref()) {
            return key;
        }
    }
}

/// The shared x-coordinate, leading zeros kept.
pub fn ecdh(secret: &SecretKey, public: &PublicKey) -> Key32 {
    let shared = diffie_hellman(secret.to_nonzero_scalar(), public.as_affine());
    let mut out = Zeroizing::new([0u8; ECDH_OUT_LEN]);
    out.copy_from_slice(shared.raw_secret_bytes());
    out
}

// The typed code -----------------------------------------------------------------------------

pub fn code_encode(s: &[u8; S_LEN]) -> String {
    let alphabet = CODE_ALPHABET.as_bytes();
    let n = s.iter().fold(0u128, |n, b| (n << 8) | u128::from(*b));
    (0..CODE_LEN)
        .map(|i| alphabet[((n >> (5 * (CODE_LEN - 1 - i))) & 31) as usize] as char)
        .collect()
}

/// Four groups of six.
pub fn code_display(code: &str) -> String {
    code.as_bytes()
        .chunks(CODE_GROUP)
        .map(|c| std::str::from_utf8(c).unwrap_or_default())
        .collect::<Vec<_>>()
        .join(CODE_SEPARATOR)
}

/// Upper case, spaces and hyphens dropped, `O` to `0`, `I` and `L` to `1`; anything else
/// outside the alphabet, or a length other than 24, is refused.
pub fn code_normalise(typed: &str) -> Checked<String> {
    let mut out = String::with_capacity(CODE_LEN);
    for ch in typed.chars().flat_map(char::to_uppercase) {
        let ch = match ch {
            ' ' | '-' => continue,
            'O' => '0',
            'I' | 'L' => '1',
            c => c,
        };
        if !CODE_ALPHABET.contains(ch) {
            return Err(Refusal::BadCode);
        }
        out.push(ch);
    }
    if out.len() == CODE_LEN {
        Ok(out)
    } else {
        Err(Refusal::BadCode)
    }
}

/// S from a normalised code.
pub fn code_decode(code: &str) -> Checked<[u8; S_LEN]> {
    let code = code_normalise(code)?;
    let n = code.chars().fold(0u128, |n, ch| {
        (n << 5) | CODE_ALPHABET.find(ch).expect("normalised") as u128
    });
    let mut out = [0u8; S_LEN];
    out.copy_from_slice(&n.to_be_bytes()[16 - S_LEN..]);
    Ok(out)
}

// Names, ids, text, magic bytes ---------------------------------------------------------------

/// 1 to 40 bytes of UTF-8 with no code point from `NAME_FORBIDDEN`.
pub fn check_device_name(name: &str) -> Checked<&[u8]> {
    let raw = name.as_bytes();
    let forbidden = |c: char| {
        NAME_FORBIDDEN
            .iter()
            .any(|(lo, hi)| (*lo..=*hi).contains(&(c as u32)))
    };
    if raw.is_empty() || raw.len() > MAX_DEVICE_NAME || name.chars().any(forbidden) {
        return Err(Refusal::BadDeviceName);
    }
    Ok(raw)
}

pub fn check_id(id: &str) -> Checked<()> {
    if (1..=MAX_ID).contains(&id.len()) && id.chars().all(|c| ID_ALPHABET.contains(c)) {
        Ok(())
    } else {
        Err(Refusal::BadId)
    }
}

/// The longest prefix of at most `MAX_ITEM_TEXT` bytes that ends on a character, and whether
/// anything was cut.
pub fn cut_text(text: &str) -> (&str, bool) {
    if text.len() <= MAX_ITEM_TEXT {
        return (text, false);
    }
    let mut end = MAX_ITEM_TEXT;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    (&text[..end], true)
}

/// Whether `bytes` start the way `mime` must: an allowed type with one of its patterns.
pub fn magic_matches(mime: &str, bytes: &[u8]) -> Checked<()> {
    let rule = MIME_MAGIC
        .iter()
        .find(|r| r.mime == mime)
        .ok_or(Refusal::BadMagic)?;
    let matches = |offset: usize, pattern: &str| {
        let want = pattern.as_bytes();
        let Some(window) = bytes.get(offset..offset + want.len() / 2) else {
            return false;
        };
        window.iter().enumerate().all(|(i, b)| {
            let nib = |p: u8, v: u8| p == b'.' || (p as char).to_digit(16) == Some(u32::from(v));
            nib(want[2 * i], b >> 4) && nib(want[2 * i + 1], b & 0xf)
        })
    };
    if rule.magic.iter().any(|(o, p)| matches(*o, p)) {
        Ok(())
    } else {
        Err(Refusal::BadMagic)
    }
}

// Headers ------------------------------------------------------------------------------------

pub fn header(kind: u8, sid: &[u8; SID_LEN], ctr: u64) -> [u8; HEADER_LEN] {
    let mut h = [0u8; HEADER_LEN];
    h[0] = VERSION;
    h[1] = kind;
    h[2..18].copy_from_slice(sid);
    h[18..26].copy_from_slice(&ctr.to_be_bytes());
    h
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    pub version: u8,
    pub kind: u8,
    pub sid: Sid,
    pub ctr: u64,
}

pub fn parse_header(frame: &[u8]) -> Checked<Header> {
    if frame.len() < HEADER_LEN {
        return Err(Refusal::TooShort);
    }
    let mut sid = [0u8; SID_LEN];
    sid.copy_from_slice(&frame[2..18]);
    let mut ctr = [0u8; 8];
    ctr.copy_from_slice(&frame[18..26]);
    Ok(Header {
        version: frame[0],
        kind: frame[1],
        sid,
        ctr: u64::from_be_bytes(ctr),
    })
}

enum Len {
    AtLeast(usize),
    Exactly(usize),
}

/// Length, version, kind and sid, in the fixture's order. The counter is the caller's.
fn check_frame(frame: &[u8], len: Len, kind: u8, sid: &Sid) -> Checked<Header> {
    let h = parse_header(frame)?;
    let short = match len {
        Len::AtLeast(n) => frame.len() < n,
        Len::Exactly(n) => frame.len() != n,
    };
    if short {
        return Err(Refusal::TooShort);
    }
    if h.version != VERSION {
        return Err(Refusal::BadVersion);
    }
    if h.kind != kind {
        return Err(Refusal::WrongKind);
    }
    if !eq_ct(&h.sid, sid) {
        return Err(Refusal::WrongSid);
    }
    Ok(h)
}

fn handshake(frame: &[u8], len: Len, kind: u8, sid: &Sid) -> Checked<Header> {
    let h = check_frame(frame, len, kind, sid)?;
    if h.ctr != 0 {
        return Err(Refusal::BadCounter);
    }
    Ok(h)
}

const ZERO_SID: Sid = [0u8; SID_LEN];

// Envelope -----------------------------------------------------------------------------------

/// A JSON body padded so the envelope is a whole number of `PAD_BLOCK`s.
pub fn envelope_json(body: &[u8]) -> Vec<u8> {
    let pad = (PAD_BLOCK - (ENVELOPE_HEADER_LEN + body.len()) % PAD_BLOCK) % PAD_BLOCK;
    let mut env = Vec::with_capacity(ENVELOPE_HEADER_LEN + body.len() + pad);
    env.push(TYPE_JSON);
    env.extend_from_slice(&(pad as u16).to_be_bytes());
    env.extend_from_slice(body);
    env.resize(env.len() + pad, 0);
    env
}

pub fn envelope_chunk(call_id: u32, seq: u32, last: bool, bytes: &[u8]) -> Vec<u8> {
    debug_assert!(bytes.len() <= CHUNK);
    let mut env = Vec::with_capacity(ENVELOPE_HEADER_LEN + CHUNK_HEADER_LEN + bytes.len());
    env.push(TYPE_CHUNK);
    env.extend_from_slice(&0u16.to_be_bytes());
    env.extend_from_slice(&call_id.to_be_bytes());
    env.extend_from_slice(&seq.to_be_bytes());
    env.push(u8::from(last));
    env.extend_from_slice(bytes);
    env
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Envelope {
    Json(Vec<u8>),
    Chunk(Chunk),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    pub call_id: u32,
    pub seq: u32,
    pub last: bool,
    pub bytes: Vec<u8>,
}

pub fn envelope_open(env: &[u8]) -> Checked<Envelope> {
    if env.len() < ENVELOPE_HEADER_LEN {
        return Err(Refusal::TooShort);
    }
    let pad = usize::from(u16::from_be_bytes([env[1], env[2]]));
    let rest = &env[ENVELOPE_HEADER_LEN..];
    if pad > rest.len() {
        return Err(Refusal::BadPadding);
    }
    let (body, padding) = rest.split_at(rest.len() - pad);
    if padding.iter().any(|b| *b != 0) {
        return Err(Refusal::BadPadding);
    }
    match env[0] {
        TYPE_JSON if env.len().is_multiple_of(PAD_BLOCK) => Ok(Envelope::Json(body.to_vec())),
        TYPE_JSON => Err(Refusal::BadPadding),
        TYPE_CHUNK if pad == 0 && body.len() >= CHUNK_HEADER_LEN => Ok(Envelope::Chunk(Chunk {
            call_id: u32::from_be_bytes(body[0..4].try_into().expect("4 bytes")),
            seq: u32::from_be_bytes(body[4..8].try_into().expect("4 bytes")),
            last: body[8] != 0,
            bytes: body[CHUNK_HEADER_LEN..].to_vec(),
        })),
        TYPE_CHUNK => Err(Refusal::BadPadding),
        _ => Err(Refusal::BadEnvelope),
    }
}

/// Chunk bodies (`callId || seq || last || bytes`) must count up from 0 without a gap.
pub fn chunks_in_order(bodies: &[&[u8]]) -> Checked<()> {
    for (expected, body) in bodies.iter().enumerate() {
        let seq = body
            .get(4..8)
            .map(|s| u32::from_be_bytes(s.try_into().expect("4 bytes")))
            .ok_or(Refusal::TooShort)?;
        if seq as usize != expected {
            return Err(Refusal::ChunkGap);
        }
    }
    Ok(())
}

// Pairing ------------------------------------------------------------------------------------

pub fn pairing_id(s: &[u8; S_LEN]) -> PairingId {
    let mut out = [0u8; PAIRING_ID_LEN];
    hkdf(s, &[], INFO_PAIR_ID, &mut out);
    out
}

/// The scanned link; the secret and the key travel only after the fragment mark.
pub fn pairing_link(host: &str, s: &[u8; S_LEN], cpub: &[u8]) -> String {
    format!(
        "https://{host}{PATH_PAIR}#{LINK_VERSION}.{}.{}",
        b64(s),
        b64(cpub)
    )
}

pub struct PairKeys {
    pub up: Key32,
    pub down: Key32,
}

impl PairKeys {
    pub fn get(&self, dir: Dir) -> &[u8; 32] {
        match dir {
            Dir::Up => &self.up,
            Dir::Down => &self.down,
        }
    }
}

pub fn pair_keys(s: &[u8; S_LEN], cpub: &[u8], pid: &PairingId) -> PairKeys {
    let salt = sha256(&[HASH_QR.as_bytes(), cpub, pid]);
    let (up, down) = split64(s, &salt, INFO_PAIR);
    PairKeys { up, down }
}

pub fn hello_request(pid: &PairingId) -> Vec<u8> {
    header(KIND_HELLO, pid, 0).to_vec()
}

/// A HELLO from a phone that typed the code: just the header. Returns the pairing id it names.
pub fn hello_request_open(frame: &[u8]) -> Checked<PairingId> {
    let h = parse_header(frame)?;
    handshake(frame, Len::Exactly(HEADER_LEN), KIND_HELLO, &h.sid)?;
    Ok(h.sid)
}

/// The unsealed answer: the header and Cpub, nothing else.
pub fn hello_answer(pid: &PairingId, cpub: &[u8]) -> Vec<u8> {
    [&header(KIND_HELLO, pid, 0)[..], cpub].concat()
}

pub fn hello_answer_open(frame: &[u8], pid: &PairingId) -> Checked<PublicKey> {
    handshake(
        frame,
        Len::Exactly(HEADER_LEN + PUBKEY_LEN),
        KIND_HELLO,
        pid,
    )?;
    check_public_key(&frame[HEADER_LEN..])
}

/// `H || n || GCM(Kp_dir, n, H, envelope)` with a fresh random n each time.
pub fn pair_seal(kind: u8, pid: &PairingId, key: &[u8; 32], env: &[u8]) -> Vec<u8> {
    pair_seal_with(kind, pid, key, &random::<GCM_NONCE_LEN>(), env)
}

pub fn pair_seal_with(
    kind: u8,
    pid: &PairingId,
    key: &[u8; 32],
    nonce: &[u8; GCM_NONCE_LEN],
    env: &[u8],
) -> Vec<u8> {
    let h = header(kind, pid, 0);
    [&h[..], nonce, &seal(key, nonce, &h, env)].concat()
}

pub fn pair_open(frame: &[u8], kind: u8, pid: &PairingId, key: &[u8; 32]) -> Checked<Envelope> {
    handshake(
        frame,
        Len::AtLeast(HEADER_LEN + GCM_NONCE_LEN + GCM_TAG_LEN),
        kind,
        pid,
    )?;
    let nonce = &frame[HEADER_LEN..HEADER_LEN + GCM_NONCE_LEN];
    let env = open(
        key,
        nonce,
        &frame[..HEADER_LEN],
        &frame[HEADER_LEN + GCM_NONCE_LEN..],
    )?;
    envelope_open(&env)
}

/// What each pairing frame carries, as JSON with binary fields in base64url.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PairCommit {
    pub path: u8,
    pub device_name: String,
    pub commit: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PairNonce {
    pub nc: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PairReveal {
    pub dpub: String,
    pub np: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PairOutcome {
    pub allowed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
}

/// `SHA256("nocturne commit v1" || Dpub || np || SHA256(deviceName))`.
pub fn commit(dpub: &[u8], np: &[u8], name: &[u8]) -> [u8; 32] {
    sha256(&[HASH_COMMIT.as_bytes(), dpub, np, &sha256(&[name])])
}

/// The reveal against the commitment: a good key, not the computer's own, and the same
/// commitment over the name received with the commit, compared in constant time.
pub fn check_reveal(
    cpub: &[u8],
    dpub: &[u8],
    np: &[u8],
    name: &[u8],
    committed: &[u8],
) -> Checked<PublicKey> {
    let key = check_public_key(dpub)?;
    if eq_ct(dpub, cpub) {
        return Err(Refusal::SamePublicKey);
    }
    if !eq_ct(&commit(dpub, np, name), committed) {
        return Err(Refusal::CommitMismatch);
    }
    Ok(key)
}

/// Everything the check words bind.
pub struct WordsInput<'a> {
    pub path: u8,
    pub pid: &'a PairingId,
    pub host: &'a str,
    pub cpub: &'a [u8],
    pub dpub: &'a [u8],
    pub name: &'a [u8],
    pub np: &'a [u8],
    pub nc: &'a [u8],
}

pub fn words_salt(w: &WordsInput<'_>) -> [u8; 32] {
    sha256(&[
        HASH_WORDS.as_bytes(),
        &[w.path],
        w.pid,
        &lp(w.host.as_bytes()),
        w.cpub,
        w.dpub,
        &lp(w.name),
        w.np,
        w.nc,
    ])
}

/// Three indices into the word list from the first 33 bits, most significant first.
pub fn word_indices(shared: &[u8], salt: &[u8]) -> [usize; WORD_COUNT] {
    let mut out = [0u8; WORDS_OUT_LEN];
    hkdf(shared, salt, INFO_WORDS, &mut out);
    let bits = out.iter().fold(0u64, |n, b| (n << 8) | u64::from(*b));
    std::array::from_fn(|i| {
        ((bits >> (40 - WORD_BITS as usize * (i + 1))) & ((1 << WORD_BITS) - 1)) as usize
    })
}

pub fn words(shared: &[u8], w: &WordsInput<'_>) -> [&'static str; WORD_COUNT] {
    word_indices(shared, &words_salt(w)).map(|i| WORDS[i])
}

// Sessions -----------------------------------------------------------------------------------

/// `Kd = HKDF(ECDH(D, C), deviceId, "nocturne device v1", 32)`.
pub fn device_key(shared_dc: &[u8], device_id: &DeviceId) -> Key32 {
    hkdf32(shared_dc, device_id, INFO_DEVICE)
}

const OPEN_LEN: usize =
    HEADER_LEN + DEVICE_ID_LEN + PUBKEY_LEN + OPEN_NONCE_LEN + PROOF_OPEN.len() + GCM_TAG_LEN;
const REVOKED_LEN: usize = HEADER_LEN + GCM_NONCE_LEN + PROOF_REVOKED.len() + GCM_TAG_LEN;
const ACCEPT_LEN: usize = HEADER_LEN + PUBKEY_LEN;

/// `H (sid zeros) || deviceId || Epub || on || GCM(Kd, on, H || deviceId || Epub, "open v1")`.
pub fn session_open(
    device_id: &DeviceId,
    epub: &[u8],
    on: &[u8; OPEN_NONCE_LEN],
    kd: &[u8; 32],
) -> Vec<u8> {
    let aad = [
        &header(KIND_SESSION_OPEN, &ZERO_SID, 0)[..],
        device_id,
        epub,
    ]
    .concat();
    let proof = seal(kd, on, &aad, PROOF_OPEN.as_bytes());
    [&aad[..], on, &proof].concat()
}

/// A SESSION_OPEN whose proof verified, from a registered phone.
pub struct OpenRequest<T> {
    pub device_id: DeviceId,
    pub epub: PublicKey,
    pub on: [u8; OPEN_NONCE_LEN],
    pub kd: Key32,
    pub device: T,
}

/// Check a SESSION_OPEN. `lookup` gives a device id's phone key and whatever the caller keeps
/// with it. An unknown device and a bad proof are the same refusal, and the proof is checked
/// before Epub is parsed, so no ECDH ever runs on a key the proof did not cover.
pub fn verify_open<T>(
    frame: &[u8],
    lookup: impl FnOnce(&DeviceId) -> Option<(Vec<u8>, T)>,
    c: &SecretKey,
) -> Checked<OpenRequest<T>> {
    handshake(frame, Len::Exactly(OPEN_LEN), KIND_SESSION_OPEN, &ZERO_SID)?;
    let mut at = HEADER_LEN;
    let mut device_id = [0u8; DEVICE_ID_LEN];
    device_id.copy_from_slice(&frame[at..at + DEVICE_ID_LEN]);
    at += DEVICE_ID_LEN;
    let epub = &frame[at..at + PUBKEY_LEN];
    at += PUBKEY_LEN;
    let mut on = [0u8; OPEN_NONCE_LEN];
    on.copy_from_slice(&frame[at..at + OPEN_NONCE_LEN]);
    at += OPEN_NONCE_LEN;
    let (dpub, device) = lookup(&device_id).ok_or(Refusal::BadProof)?;
    let dpub = check_public_key(&dpub).map_err(|_| Refusal::BadProof)?;
    let kd = device_key(ecdh(c, &dpub).as_ref(), &device_id);
    let aad = &frame[..HEADER_LEN + DEVICE_ID_LEN + PUBKEY_LEN];
    let proof = open(&kd, &on, aad, &frame[at..]).map_err(|_| Refusal::BadProof)?;
    if !eq_ct(&proof, PROOF_OPEN.as_bytes()) {
        return Err(Refusal::BadProof);
    }
    let epub = check_public_key(epub)?;
    Ok(OpenRequest {
        device_id,
        epub,
        on,
        kd,
        device,
    })
}

/// `H || n || GCM(Kd, n, H || deviceId || on, "revoked v1")`: an answer to that one open.
pub fn revoked(kd: &[u8; 32], device_id: &DeviceId, on: &[u8; OPEN_NONCE_LEN]) -> Vec<u8> {
    revoked_with(kd, device_id, on, &random::<GCM_NONCE_LEN>())
}

pub fn revoked_with(
    kd: &[u8; 32],
    device_id: &DeviceId,
    on: &[u8; OPEN_NONCE_LEN],
    n: &[u8; GCM_NONCE_LEN],
) -> Vec<u8> {
    let h = header(KIND_REVOKED, &ZERO_SID, 0);
    let aad = [&h[..], device_id, on].concat();
    [&h[..], n, &seal(kd, n, &aad, PROOF_REVOKED.as_bytes())].concat()
}

/// The phone's check of a REVOKED: sealed under its own Kd and bound to its pending open.
pub fn revoked_open(
    frame: &[u8],
    kd: &[u8; 32],
    device_id: &DeviceId,
    pending_on: &[u8; OPEN_NONCE_LEN],
) -> Checked<()> {
    handshake(frame, Len::Exactly(REVOKED_LEN), KIND_REVOKED, &ZERO_SID)?;
    let aad = [&frame[..HEADER_LEN], device_id, pending_on].concat();
    let n = &frame[HEADER_LEN..HEADER_LEN + GCM_NONCE_LEN];
    let proof = open(kd, n, &aad, &frame[HEADER_LEN + GCM_NONCE_LEN..])?;
    if eq_ct(&proof, PROOF_REVOKED.as_bytes()) {
        Ok(())
    } else {
        Err(Refusal::BadTag)
    }
}

pub fn session_accept(sid: &Sid, fpub: &[u8]) -> Vec<u8> {
    [&header(KIND_SESSION_ACCEPT, sid, 0)[..], fpub].concat()
}

/// The phone's read of a SESSION_ACCEPT: the sid and a good Fpub.
pub fn session_accept_open(frame: &[u8]) -> Checked<(Sid, PublicKey)> {
    let h = parse_header(frame)?;
    handshake(frame, Len::Exactly(ACCEPT_LEN), KIND_SESSION_ACCEPT, &h.sid)?;
    Ok((h.sid, check_public_key(&frame[HEADER_LEN..])?))
}

pub struct SessionKeys {
    pub up: Key32,
    pub down: Key32,
}

/// What both sides bind into the session keys besides the four ECDH terms.
pub struct SessionInput<'a> {
    pub device_id: &'a DeviceId,
    pub epub: &'a [u8],
    pub on: &'a [u8],
    pub sid: &'a Sid,
    pub fpub: &'a [u8],
    pub cpub: &'a [u8],
    pub dpub: &'a [u8],
}

fn session_keys(terms: [Key32; 4], s: &SessionInput<'_>) -> SessionKeys {
    let ikm = Zeroizing::new(
        terms
            .iter()
            .flat_map(|t| t.iter().copied())
            .collect::<Vec<_>>(),
    );
    let salt = sha256(&[
        HASH_SESSION.as_bytes(),
        s.device_id,
        s.epub,
        s.on,
        s.sid,
        s.fpub,
        s.cpub,
        s.dpub,
    ]);
    let (up, down) = split64(&ikm, &salt, INFO_SESSION);
    SessionKeys { up, down }
}

/// The computer's side: F and C are secret, E and D public.
pub fn session_keys_computer(
    f: &SecretKey,
    c: &SecretKey,
    epub: &PublicKey,
    dpub: &PublicKey,
    s: &SessionInput<'_>,
) -> SessionKeys {
    session_keys(
        [ecdh(f, epub), ecdh(c, epub), ecdh(f, dpub), ecdh(c, dpub)],
        s,
    )
}

/// The phone's side: E and D are secret, F and C public.
pub fn session_keys_phone(
    e: &SecretKey,
    d: &SecretKey,
    fpub: &PublicKey,
    cpub: &PublicKey,
    s: &SessionInput<'_>,
) -> SessionKeys {
    session_keys(
        [ecdh(e, fpub), ecdh(e, cpub), ecdh(d, fpub), ecdh(d, cpub)],
        s,
    )
}

// Data frames --------------------------------------------------------------------------------

fn data_nonce(dir: Dir, ctr: u64) -> [u8; GCM_NONCE_LEN] {
    let prefix = match dir {
        Dir::Up => NONCE_UP,
        Dir::Down => NONCE_DOWN,
    };
    let mut n = [0u8; GCM_NONCE_LEN];
    n[..4].copy_from_slice(prefix.as_bytes());
    n[4..].copy_from_slice(&ctr.to_be_bytes());
    n
}

/// `H || GCM(k_dir, prefix || ctr, H, envelope)`.
pub fn data_seal(key: &[u8; 32], dir: Dir, sid: &Sid, ctr: u64, env: &[u8]) -> Vec<u8> {
    let h = header(KIND_DATA, sid, ctr);
    [&h[..], &seal(key, &data_nonce(dir, ctr), &h, env)].concat()
}

/// Open the frame that must carry counter `last + 1`; returns that counter and the envelope.
/// The caller moves its counter only on success.
pub fn data_open(
    frame: &[u8],
    key: &[u8; 32],
    dir: Dir,
    sid: &Sid,
    last: u64,
) -> Checked<(u64, Envelope)> {
    let h = check_frame(
        frame,
        Len::AtLeast(HEADER_LEN + GCM_TAG_LEN),
        KIND_DATA,
        sid,
    )?;
    if Some(h.ctr) != last.checked_add(1) {
        return Err(Refusal::CounterGap);
    }
    let env = open(
        key,
        &data_nonce(dir, h.ctr),
        &frame[..HEADER_LEN],
        &frame[HEADER_LEN..],
    )?;
    Ok((h.ctr, envelope_open(&env)?))
}

#[cfg(test)]
#[path = "crypto_tests.rs"]
mod tests;
