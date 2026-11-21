// The sealed channel's constants, shared with the computer (`crates/channel/src/protocol.rs`).
//
// Plan 12 "Sealed channel" is the normative description; `scripts/remote_vectors.py` is the
// independent reference and `src-tauri/tests/fixtures/remote-vectors.json` its output. The drift
// tests hold this file, the computer's copy and the fixture equal. Text labels are ASCII and used
// as their bytes, with no terminator.

import wordList from "./words.json";

export const VERSION = 1;
/** `ver(1) kind(1) sid(16) ctr(8, big-endian)`. */
export const HEADER_LEN = 26;
export const KIND_HELLO = 1;
export const KIND_PAIR_COMMIT = 2;
export const KIND_PAIR_NONCE = 3;
export const KIND_PAIR_REVEAL = 4;
export const KIND_PAIR_OUTCOME = 5;
export const KIND_SESSION_OPEN = 6;
export const KIND_SESSION_ACCEPT = 7;
export const KIND_DATA = 8;
export const KIND_REVOKED = 9;

/** The pairing secret S: 120 bits. */
export const S_LEN = 15;
/** S typed as Crockford base32, most significant bits first. */
export const CODE_LEN = 24;
export const CODE_GROUP = 6;
export const CODE_SEPARATOR = " ";
export const CODE_ALPHABET = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";
export const PAIRING_ID_LEN = 16;
/** np and nc. */
export const NONCE_PAIR_LEN = 32;
export const DEVICE_ID_LEN = 16;
export const SID_LEN = 16;
export const OPEN_NONCE_LEN = 12;
export const GCM_NONCE_LEN = 12;
export const GCM_TAG_LEN = 16;
/** Uncompressed SEC1 P-256 point; the first byte is `0x04`. */
export const PUBKEY_LEN = 65;
/** The x-coordinate, leading zeros kept. */
export const ECDH_OUT_LEN = 32;
/** Bytes of UTF-8. */
export const MAX_DEVICE_NAME = 40;

export const LINK_VERSION = "v1";
export const PATH_PAIR = "/p";
export const PATH_RELINK = "/r";
export const PATH_SESSION = "/s";

export const INFO_PAIR_ID = "nocturne pair id v1";
export const INFO_PAIR = "nocturne pair v1";
export const INFO_WORDS = "nocturne words v1";
export const INFO_DEVICE = "nocturne device v1";
export const INFO_SESSION = "nocturne session v1";
export const HASH_QR = "nocturne qr v1";
export const HASH_COMMIT = "nocturne commit v1";
export const HASH_WORDS = "nocturne words v1";
export const HASH_SESSION = "nocturne session v1";
export const PROOF_OPEN = "open v1";
export const PROOF_REVOKED = "revoked v1";
export const NONCE_UP = "up__";
export const NONCE_DOWN = "down";
export const PATH_SCANNED = 1;
export const PATH_TYPED = 2;

export const WORDS_OUT_LEN = 5;
export const WORD_COUNT = 3;
export const WORD_BITS = 11;

export const TYPE_JSON = 1;
export const TYPE_CHUNK = 2;
/** `type(1) padLen(2, big-endian)`. */
export const ENVELOPE_HEADER_LEN = 3;
/** `callId u32, seq u32, last u8`. */
export const CHUNK_HEADER_LEN = 9;
export const PAD_BLOCK = 256;
export const DELTA_FLUSH_MS = 100;

export const PAIR_TTL_SECS = 300;
export const REVEAL_WINDOW_SECS = 10;
export const DECIDE_TTL_SECS = 120;
export const PENDING_SESSION_TTL_SECS = 10;
export const SESSION_TTL_SECS = 900;
export const HANDSHAKE_TIMEOUT_SECS = 10;
export const LONG_POLL_HOLD_SECS = 25;
export const TAKE_BACK_SECS = 600;
export const CITED_TTL_SECS = 3600;

export const CHUNK = 262144;
export const MAX_FRAME = 270336;
export const MAX_TEXT = 102400;
export const MAX_QUESTION = 2048;
/** Characters, not bytes. */
export const MAX_CAPTION = 120;
/** Request, client, ask and item ids: 1 to `MAX_ID` characters from `ID_ALPHABET`. */
export const MAX_ID = 36;
export const ID_ALPHABET = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_-";
/**
 * UTF-8 bytes of an item's summary and of its text sent to a phone, each cut at a character
 * boundary, so the largest item still fits in one frame.
 */
export const MAX_ITEM_TEXT = 16384;
export const MAX_ITEM = 52428800;
export const MAX_DAY = 524288000;
export const MAX_INCOMING_TOTAL = 104857600;
export const MAX_LONG_POLL_BATCH = 8;
export const MAX_CONNECTIONS = 16;
/** Live sessions; pending ones do not count. */
export const MAX_SESSIONS_PER_DEVICE = 3;
/**
 * A phone app's calendar or health send: changes in one batch, the batch's JSON in bytes, and the
 * changes taken under one sync id.
 */
export const MAX_SYNC_BATCH = 100;
export const MAX_SYNC_JSON = 65536;
export const MAX_CALENDAR_PER_SEND = 500;
export const MAX_READINGS_PER_SEND = 5000;

/** Per minute. */
export const LIMIT_NO_KEY = 20;
export const LIMIT_STATIC = 300;
export const LIMIT_PAIRING_FRAMES = 20;
export const LIMIT_FAILED_SESSION_OPEN = 10;
export const LIMIT_SESSION_OPENS_PER_DEVICE = 6;
/** Chunks are not counted. */
export const LIMIT_REQUESTS = 60;
export const LIMIT_ASKS = 10;
export const LIMIT_KEEPS = 30;
/** Calendar and health sends from a phone app, per device. */
export const LIMIT_SYNCS = 12;
export const PAGES_BACK = 7;
export const EVENTS_KEPT_DAYS = 90;

/** The build of the phone code in this tree. */
export const PHONE_BUILD = 1;
/** The oldest phone build the computer accepts. */
export const MIN_PHONE_BUILD = 1;

/** HTTP statuses between the listener and the phone. */
/** Away from home is off. */
export const HTTP_OFF = 503;
/** The session or pairing has ended; open a new one. */
export const HTTP_ENDED = 410;
/** Another poll is already held. */
export const HTTP_POLL_BUSY = 409;
/** Unsealed refusal. */
export const HTTP_REFUSED = 403;
/** Anything else. */
export const HTTP_NOT_FOUND = 404;

/**
 * Code points a device name may not hold, inclusive: C0 and C1 controls, DEL, the bidi controls
 * (with U+061C), zero-width characters, the line and paragraph separators and the byte order
 * mark.
 */
export const NAME_FORBIDDEN: readonly (readonly [number, number])[] = [
  [0x0000, 0x001f],
  [0x007f, 0x009f],
  [0x061c, 0x061c],
  [0x200b, 0x200f],
  [0x2028, 0x202e],
  [0x2066, 0x2069],
  [0xfeff, 0xfeff],
];

/**
 * An allowed MIME type and its magic bytes: any one alternative must match. Each is an offset
 * and a hex pattern in which `.` matches any nibble.
 */
export interface MimeMagic {
  readonly mime: string;
  readonly magic: readonly (readonly [number, string])[];
}

export const MIME_MAGIC: readonly MimeMagic[] = [
  { mime: "image/jpeg", magic: [[0, "ffd8ff"]] },
  { mime: "image/png", magic: [[0, "89504e47"]] },
  {
    mime: "image/heic",
    magic: [
      [4, "6674797068656963"],
      [4, "6674797068656978"],
      [4, "667479706d696631"],
    ],
  },
  { mime: "image/webp", magic: [[0, "52494646........57454250"]] },
  { mime: "application/pdf", magic: [[0, "255044462d"]] },
  { mime: "audio/webm", magic: [[0, "1a45dfa3"]] },
  { mime: "audio/mp4", magic: [[4, "66747970"]] },
  { mime: "audio/ogg", magic: [[0, "4f676753"]] },
  {
    mime: "audio/mpeg",
    magic: [
      [0, "494433"],
      [0, "fff."],
    ],
  },
  { mime: "audio/wav", magic: [[0, "52494646........57415645"]] },
];

/**
 * The BIP-39 English word list (2048 words, MIT): `bip-0039/english.txt` from the bitcoin/bips
 * repository at commit b34d9f751791d4feb22feff4f2ed00bcac83961b.
 */
export const WORDS: readonly string[] = wordList;
