//! The sealed channel's constants, shared with the phone app (`ui/src/phone/protocol.ts`).
//!
//! Plan 12 "Sealed channel" is the normative description; `scripts/remote_vectors.py` is the
//! independent reference and `src-tauri/tests/fixtures/remote-vectors.json` its output. The
//! drift tests hold this file, the phone's copy and the fixture equal. Text labels are ASCII
//! and used as their bytes, with no terminator.

use std::sync::LazyLock;

pub const VERSION: u8 = 1;
/// `ver(1) kind(1) sid(16) ctr(8, big-endian)`.
pub const HEADER_LEN: usize = 26;
pub const KIND_HELLO: u8 = 1;
pub const KIND_PAIR_COMMIT: u8 = 2;
pub const KIND_PAIR_NONCE: u8 = 3;
pub const KIND_PAIR_REVEAL: u8 = 4;
pub const KIND_PAIR_OUTCOME: u8 = 5;
pub const KIND_SESSION_OPEN: u8 = 6;
pub const KIND_SESSION_ACCEPT: u8 = 7;
pub const KIND_DATA: u8 = 8;
pub const KIND_REVOKED: u8 = 9;

/// The pairing secret S: 120 bits.
pub const S_LEN: usize = 15;
/// S typed as Crockford base32, most significant bits first.
pub const CODE_LEN: usize = 24;
pub const CODE_GROUP: usize = 6;
pub const CODE_SEPARATOR: &str = " ";
pub const CODE_ALPHABET: &str = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";
pub const PAIRING_ID_LEN: usize = 16;
/// np and nc.
pub const NONCE_PAIR_LEN: usize = 32;
pub const DEVICE_ID_LEN: usize = 16;
pub const SID_LEN: usize = 16;
pub const OPEN_NONCE_LEN: usize = 12;
pub const GCM_NONCE_LEN: usize = 12;
pub const GCM_TAG_LEN: usize = 16;
/// Uncompressed SEC1 P-256 point; the first byte is `0x04`.
pub const PUBKEY_LEN: usize = 65;
/// The x-coordinate, leading zeros kept.
pub const ECDH_OUT_LEN: usize = 32;
/// Bytes of UTF-8.
pub const MAX_DEVICE_NAME: usize = 40;

pub const LINK_VERSION: &str = "v1";
pub const PATH_PAIR: &str = "/p";
pub const PATH_RELINK: &str = "/r";
pub const PATH_SESSION: &str = "/s";

pub const INFO_PAIR_ID: &str = "nocturne pair id v1";
pub const INFO_PAIR: &str = "nocturne pair v1";
pub const INFO_WORDS: &str = "nocturne words v1";
pub const INFO_DEVICE: &str = "nocturne device v1";
pub const INFO_SESSION: &str = "nocturne session v1";
pub const HASH_QR: &str = "nocturne qr v1";
pub const HASH_COMMIT: &str = "nocturne commit v1";
pub const HASH_WORDS: &str = "nocturne words v1";
pub const HASH_SESSION: &str = "nocturne session v1";
pub const PROOF_OPEN: &str = "open v1";
pub const PROOF_REVOKED: &str = "revoked v1";
pub const NONCE_UP: &str = "up__";
pub const NONCE_DOWN: &str = "down";
pub const PATH_SCANNED: u8 = 1;
pub const PATH_TYPED: u8 = 2;

pub const WORDS_OUT_LEN: usize = 5;
pub const WORD_COUNT: usize = 3;
pub const WORD_BITS: u32 = 11;

pub const TYPE_JSON: u8 = 1;
pub const TYPE_CHUNK: u8 = 2;
/// `type(1) padLen(2, big-endian)`.
pub const ENVELOPE_HEADER_LEN: usize = 3;
/// `callId u32, seq u32, last u8`.
pub const CHUNK_HEADER_LEN: usize = 9;
pub const PAD_BLOCK: usize = 256;
pub const DELTA_FLUSH_MS: u64 = 100;

pub const PAIR_TTL_SECS: u64 = 300;
pub const REVEAL_WINDOW_SECS: u64 = 10;
pub const DECIDE_TTL_SECS: u64 = 120;
pub const PENDING_SESSION_TTL_SECS: u64 = 10;
pub const SESSION_TTL_SECS: u64 = 900;
pub const HANDSHAKE_TIMEOUT_SECS: u64 = 10;
pub const LONG_POLL_HOLD_SECS: u64 = 25;
pub const TAKE_BACK_SECS: u64 = 600;
pub const CITED_TTL_SECS: u64 = 3600;

pub const CHUNK: usize = 262_144;
pub const MAX_FRAME: usize = 270_336;
pub const MAX_TEXT: usize = 102_400;
pub const MAX_QUESTION: usize = 2048;
/// Characters, not bytes.
pub const MAX_CAPTION: usize = 120;
/// Request, client, ask and item ids: 1 to `MAX_ID` characters from `ID_ALPHABET`.
pub const MAX_ID: usize = 36;
pub const ID_ALPHABET: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_-";
/// UTF-8 bytes of an item's summary and of its text sent to a phone, each cut at a character
/// boundary, so the largest item still fits in one frame.
pub const MAX_ITEM_TEXT: usize = 16_384;
pub const MAX_ITEM: u64 = 52_428_800;
pub const MAX_DAY: u64 = 524_288_000;
pub const MAX_INCOMING_TOTAL: u64 = 104_857_600;
pub const MAX_LONG_POLL_BATCH: usize = 8;
pub const MAX_CONNECTIONS: usize = 16;
/// Live sessions; pending ones do not count.
pub const MAX_SESSIONS_PER_DEVICE: usize = 3;
/// A phone app's calendar or health send: changes in one batch, the batch's JSON in bytes, and
/// the changes taken under one sync id.
pub const MAX_SYNC_BATCH: usize = 100;
pub const MAX_SYNC_JSON: usize = 65_536;
pub const MAX_CALENDAR_PER_SEND: usize = 500;
pub const MAX_READINGS_PER_SEND: usize = 5_000;

/// Per minute.
pub const LIMIT_NO_KEY: u32 = 20;
pub const LIMIT_STATIC: u32 = 300;
pub const LIMIT_PAIRING_FRAMES: u32 = 20;
pub const LIMIT_FAILED_SESSION_OPEN: u32 = 10;
pub const LIMIT_SESSION_OPENS_PER_DEVICE: u32 = 6;
/// Chunks are not counted.
pub const LIMIT_REQUESTS: u32 = 60;
pub const LIMIT_ASKS: u32 = 10;
pub const LIMIT_KEEPS: u32 = 30;
/// Calendar and health sends from a phone app, per device.
pub const LIMIT_SYNCS: u32 = 12;
pub const PAGES_BACK: u32 = 7;
pub const EVENTS_KEPT_DAYS: u32 = 90;

/// The build of the phone code in this tree.
pub const PHONE_BUILD: u32 = 1;
/// The oldest phone build the computer accepts.
pub const MIN_PHONE_BUILD: u32 = 1;

/// HTTP statuses between the listener and the phone.
///
/// - `HTTP_OFF` (503): Away from home is off.
/// - `HTTP_ENDED` (410): the session or pairing has ended; open a new one.
/// - `HTTP_POLL_BUSY` (409): another poll is already held.
/// - `HTTP_REFUSED` (403): unsealed refusal.
/// - `HTTP_NOT_FOUND` (404): anything else.
pub const HTTP_OFF: u16 = 503;
pub const HTTP_ENDED: u16 = 410;
pub const HTTP_POLL_BUSY: u16 = 409;
pub const HTTP_REFUSED: u16 = 403;
pub const HTTP_NOT_FOUND: u16 = 404;

/// Code points a device name may not hold, inclusive: C0 and C1 controls, DEL, the bidi
/// controls (with U+061C), zero-width characters, the line and paragraph separators and the
/// byte order mark.
pub const NAME_FORBIDDEN: &[(u32, u32)] = &[
    (0x0000, 0x001F),
    (0x007F, 0x009F),
    (0x061C, 0x061C),
    (0x200B, 0x200F),
    (0x2028, 0x202E),
    (0x2066, 0x2069),
    (0xFEFF, 0xFEFF),
];

/// An allowed MIME type and its magic bytes: any one alternative must match. Each is an
/// offset and a hex pattern in which `.` matches any nibble.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MimeMagic {
    pub mime: &'static str,
    pub magic: &'static [(usize, &'static str)],
}

pub const MIME_MAGIC: &[MimeMagic] = &[
    MimeMagic {
        mime: "image/jpeg",
        magic: &[(0, "ffd8ff")],
    },
    MimeMagic {
        mime: "image/png",
        magic: &[(0, "89504e47")],
    },
    MimeMagic {
        mime: "image/heic",
        magic: &[
            (4, "6674797068656963"),
            (4, "6674797068656978"),
            (4, "667479706d696631"),
        ],
    },
    MimeMagic {
        mime: "image/webp",
        magic: &[(0, "52494646........57454250")],
    },
    MimeMagic {
        mime: "application/pdf",
        magic: &[(0, "255044462d")],
    },
    MimeMagic {
        mime: "audio/webm",
        magic: &[(0, "1a45dfa3")],
    },
    MimeMagic {
        mime: "audio/mp4",
        magic: &[(4, "66747970")],
    },
    MimeMagic {
        mime: "audio/ogg",
        magic: &[(0, "4f676753")],
    },
    MimeMagic {
        mime: "audio/mpeg",
        magic: &[(0, "494433"), (0, "fff.")],
    },
    MimeMagic {
        mime: "audio/wav",
        magic: &[(0, "52494646........57415645")],
    },
];

/// The BIP-39 English word list (2048 words, MIT): `bip-0039/english.txt` from the bitcoin/bips
/// repository at commit b34d9f751791d4feb22feff4f2ed00bcac83961b.
pub static WORDS: LazyLock<Vec<&'static str>> =
    LazyLock::new(|| include_str!("words.txt").lines().collect());

/// Every constant above by name, as JSON, for the drift tests against the phone and the
/// reference vectors.
#[doc(hidden)]
pub fn table() -> serde_json::Map<String, serde_json::Value> {
    use serde_json::json;
    let mut m = serde_json::Map::new();
    macro_rules! put {
        ($($name:ident),* $(,)?) => { $( m.insert(stringify!($name).into(), json!($name)); )* };
    }
    put!(
        VERSION,
        HEADER_LEN,
        KIND_HELLO,
        KIND_PAIR_COMMIT,
        KIND_PAIR_NONCE,
        KIND_PAIR_REVEAL,
        KIND_PAIR_OUTCOME,
        KIND_SESSION_OPEN,
        KIND_SESSION_ACCEPT,
        KIND_DATA,
        KIND_REVOKED,
        S_LEN,
        CODE_LEN,
        CODE_GROUP,
        CODE_SEPARATOR,
        CODE_ALPHABET,
        PAIRING_ID_LEN,
        NONCE_PAIR_LEN,
        DEVICE_ID_LEN,
        SID_LEN,
        OPEN_NONCE_LEN,
        GCM_NONCE_LEN,
        GCM_TAG_LEN,
        PUBKEY_LEN,
        ECDH_OUT_LEN,
        MAX_DEVICE_NAME,
        LINK_VERSION,
        PATH_PAIR,
        PATH_RELINK,
        PATH_SESSION,
        INFO_PAIR_ID,
        INFO_PAIR,
        INFO_WORDS,
        INFO_DEVICE,
        INFO_SESSION,
        HASH_QR,
        HASH_COMMIT,
        HASH_WORDS,
        HASH_SESSION,
        PROOF_OPEN,
        PROOF_REVOKED,
        NONCE_UP,
        NONCE_DOWN,
        PATH_SCANNED,
        PATH_TYPED,
        WORDS_OUT_LEN,
        WORD_COUNT,
        WORD_BITS,
        TYPE_JSON,
        TYPE_CHUNK,
        ENVELOPE_HEADER_LEN,
        CHUNK_HEADER_LEN,
        PAD_BLOCK,
        DELTA_FLUSH_MS,
        PAIR_TTL_SECS,
        REVEAL_WINDOW_SECS,
        DECIDE_TTL_SECS,
        PENDING_SESSION_TTL_SECS,
        SESSION_TTL_SECS,
        HANDSHAKE_TIMEOUT_SECS,
        LONG_POLL_HOLD_SECS,
        TAKE_BACK_SECS,
        CITED_TTL_SECS,
        CHUNK,
        MAX_FRAME,
        MAX_TEXT,
        MAX_QUESTION,
        MAX_CAPTION,
        ID_ALPHABET,
        MAX_ITEM_TEXT,
        MAX_ID,
        MAX_ITEM,
        MAX_DAY,
        MAX_INCOMING_TOTAL,
        MAX_LONG_POLL_BATCH,
        MAX_CONNECTIONS,
        MAX_SESSIONS_PER_DEVICE,
        MAX_SYNC_BATCH,
        MAX_SYNC_JSON,
        MAX_CALENDAR_PER_SEND,
        MAX_READINGS_PER_SEND,
        LIMIT_NO_KEY,
        LIMIT_STATIC,
        LIMIT_PAIRING_FRAMES,
        LIMIT_FAILED_SESSION_OPEN,
        LIMIT_SESSION_OPENS_PER_DEVICE,
        LIMIT_REQUESTS,
        LIMIT_ASKS,
        LIMIT_KEEPS,
        LIMIT_SYNCS,
        PAGES_BACK,
        EVENTS_KEPT_DAYS,
        PHONE_BUILD,
        MIN_PHONE_BUILD,
        HTTP_OFF,
        HTTP_ENDED,
        HTTP_POLL_BUSY,
        HTTP_REFUSED,
        HTTP_NOT_FOUND,
    );
    let forbidden: Vec<_> = NAME_FORBIDDEN.iter().map(|(a, b)| json!([a, b])).collect();
    m.insert("NAME_FORBIDDEN".into(), json!(forbidden));
    let mime: Vec<_> = MIME_MAGIC
        .iter()
        .map(|r| json!({"mime": r.mime, "magic": r.magic.iter().map(|(o, p)| json!([o, p])).collect::<Vec<_>>()}))
        .collect();
    m.insert("MIME_MAGIC".into(), json!(mime));
    m
}

#[cfg(test)]
#[path = "../tests/common/vectors.rs"]
pub(crate) mod reference;

#[cfg(test)]
mod tests {
    use aes_gcm::aead::{Aead, KeyInit, Payload};
    use aes_gcm::{Aes256Gcm, Nonce};
    use hkdf::Hkdf;
    use p256::{PublicKey, SecretKey, ecdh::diffie_hellman};
    use sha2::{Digest, Sha256};

    use super::reference::{self, hex};
    use super::*;

    #[test]
    fn constants_match_reference() {
        let v = reference::load();
        assert_eq!(&table(), v.constants());
        let declared = include_str!("protocol.rs")
            .lines()
            .filter_map(|l| {
                let rest = l
                    .strip_prefix("pub const ")
                    .or(l.strip_prefix("pub static "))?;
                Some(rest.split(':').next()?.trim().to_string())
            })
            .filter(|n| n != "WORDS")
            .collect::<std::collections::BTreeSet<_>>();
        let listed = v.constants().keys().cloned().collect();
        assert_eq!(declared, listed, "every constant here is in the reference");
    }

    #[test]
    fn the_oldest_accepted_build_is_not_newer_than_this_one() {
        const { assert!(MIN_PHONE_BUILD <= PHONE_BUILD) };
    }

    #[test]
    fn words_match_reference() {
        let v = reference::load();
        assert_eq!(WORDS.len(), 2048);
        assert_eq!(
            WORDS
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            2048
        );
        let digest = Sha256::digest(include_bytes!("words.txt"));
        assert_eq!(hex::encode(&digest), v.words_sha256());
    }

    fn hkdf(ikm: &[u8], salt: &[u8], info: &str, len: usize) -> Vec<u8> {
        let mut out = vec![0; len];
        Hkdf::<Sha256>::new(Some(salt), ikm)
            .expand(info.as_bytes(), &mut out)
            .unwrap();
        out
    }

    fn secret(v: &reference::Vectors, name: &str) -> SecretKey {
        SecretKey::from_slice(&v.key_scalar(name)).unwrap()
    }

    /// The pinned crates reproduce the reference's primitives. The channel itself is tested
    /// against every vector by the lane that builds it.
    #[test]
    fn pinned_crates_reproduce_the_reference() {
        let v = reference::load();

        let z = v.positive("ecdh_leading_zero");
        let a =
            SecretKey::from_slice(&hex::decode(z.input("a")["scalar"].as_str().unwrap())).unwrap();
        let b_pub =
            PublicKey::from_sec1_bytes(&hex::decode(z.input("b")["public"].as_str().unwrap()))
                .unwrap();
        let shared = diffie_hellman(a.to_nonzero_scalar(), b_pub.as_affine());
        assert_eq!(
            shared.raw_secret_bytes().as_slice(),
            z.output_hex("shared").as_slice()
        );
        assert_eq!(shared.raw_secret_bytes()[0], 0);

        let c = secret(&v, "C");
        assert_eq!(
            c.public_key().to_sec1_bytes().as_ref(),
            v.key_public("C").as_slice()
        );

        let p = v.positive("pairing_id");
        assert_eq!(
            hkdf(&p.input_hex("s"), &[], INFO_PAIR_ID, PAIRING_ID_LEN),
            p.output_hex("pairingId")
        );

        let k = v.positive("pair_keys");
        let salt = Sha256::new()
            .chain_update(HASH_QR)
            .chain_update(k.input_hex("cpub"))
            .chain_update(k.input_hex("pairingId"))
            .finalize();
        assert_eq!(salt.as_slice(), k.intermediate_hex("salt").as_slice());
        let okm = hkdf(&k.input_hex("s"), &salt, INFO_PAIR, 64);
        assert_eq!(okm[..32], k.output_hex("up")[..]);
        assert_eq!(okm[32..], k.output_hex("down")[..]);

        let d = v.positive("data_up_1");
        let key = d.input_hex("key");
        let aad = d.intermediate_hex("header");
        let sealed = Aes256Gcm::new_from_slice(&key)
            .unwrap()
            .encrypt(
                &Nonce::try_from(d.intermediate_hex("nonce").as_slice()).unwrap(),
                Payload {
                    msg: &d.intermediate_hex("envelope"),
                    aad: &aad,
                },
            )
            .unwrap();
        assert_eq!([aad, sealed].concat(), d.output_hex("frame"));
    }
}
