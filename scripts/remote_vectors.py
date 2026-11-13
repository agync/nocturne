#!/usr/bin/env python3
"""Reference test vectors for Away from home's sealed channel.

An independent implementation of the channel in plan 12 ("Sealed channel"), written with the
Python `cryptography` package only, from fixed scalars, secrets, nonces, names and hosts. It
writes src-tauri/tests/fixtures/remote-vectors.json, which the Rust shell and the phone app
test against. Every entry records its inputs, the intermediate values and the output; bytes
are lowercase hex. Negative entries name the refusal the receiving side must give.

Usage:
  python3 -I scripts/remote_vectors.py           write the fixture
  python3 -I scripts/remote_vectors.py --check   rebuild in memory, compare with the committed file

Never run by `make check`. A lane whose implementation disagrees stops and reports; it never
edits the fixture.
"""

import argparse
import hashlib
import json
import os
import struct
import sys

from cryptography import __version__ as CRYPTOGRAPHY_VERSION
from cryptography.hazmat.primitives import hashes
from cryptography.hazmat.primitives.asymmetric import ec
from cryptography.hazmat.primitives.ciphers.aead import AESGCM
from cryptography.hazmat.primitives.kdf.hkdf import HKDF
from cryptography.hazmat.primitives.serialization import Encoding, PublicFormat
from cryptography.exceptions import InvalidTag

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FIXTURE = os.path.join(ROOT, "src-tauri", "tests", "fixtures", "remote-vectors.json")
WORDS_FILE = os.path.join(ROOT, "src-tauri", "src", "remote", "words.txt")
# bip-0039/english.txt from github.com/bitcoin/bips at b34d9f751791d4feb22feff4f2ed00bcac83961b.
WORDS_SHA256 = "2f5eed53a4727b4bf8880d8f3f199efc90e58503646d9ff8eff3a2ed3b24dbda"

# Constants: plan 12 "Sealed channel" and 12-task0.md step 3. protocol.rs and protocol.ts
# carry the same names and values; the drift tests compare all three.
VERSION = 1
HEADER_LEN = 26
KIND_HELLO = 1
KIND_PAIR_COMMIT = 2
KIND_PAIR_NONCE = 3
KIND_PAIR_REVEAL = 4
KIND_PAIR_OUTCOME = 5
KIND_SESSION_OPEN = 6
KIND_SESSION_ACCEPT = 7
KIND_DATA = 8
KIND_REVOKED = 9

S_LEN = 15
CODE_LEN = 24
CODE_GROUP = 6
CODE_SEPARATOR = " "
CODE_ALPHABET = "0123456789ABCDEFGHJKMNPQRSTVWXYZ"
PAIRING_ID_LEN = 16
NONCE_PAIR_LEN = 32
DEVICE_ID_LEN = 16
SID_LEN = 16
OPEN_NONCE_LEN = 12
GCM_NONCE_LEN = 12
GCM_TAG_LEN = 16
PUBKEY_LEN = 65
ECDH_OUT_LEN = 32
MAX_DEVICE_NAME = 40

LINK_VERSION = "v1"
PATH_PAIR = "/p"
PATH_RELINK = "/r"
PATH_SESSION = "/s"

INFO_PAIR_ID = "nocturne pair id v1"
INFO_PAIR = "nocturne pair v1"
INFO_WORDS = "nocturne words v1"
INFO_DEVICE = "nocturne device v1"
INFO_SESSION = "nocturne session v1"
HASH_QR = "nocturne qr v1"
HASH_COMMIT = "nocturne commit v1"
HASH_WORDS = "nocturne words v1"
HASH_SESSION = "nocturne session v1"
PROOF_OPEN = "open v1"
PROOF_REVOKED = "revoked v1"
NONCE_UP = "up__"
NONCE_DOWN = "down"
PATH_SCANNED = 1
PATH_TYPED = 2

WORDS_OUT_LEN = 5
WORD_COUNT = 3
WORD_BITS = 11

TYPE_JSON = 1
TYPE_CHUNK = 2
ENVELOPE_HEADER_LEN = 3
CHUNK_HEADER_LEN = 9
PAD_BLOCK = 256
DELTA_FLUSH_MS = 100

PAIR_TTL_SECS = 300
REVEAL_WINDOW_SECS = 10
DECIDE_TTL_SECS = 120
PENDING_SESSION_TTL_SECS = 10
SESSION_TTL_SECS = 900
HANDSHAKE_TIMEOUT_SECS = 10
LONG_POLL_HOLD_SECS = 25
TAKE_BACK_SECS = 600
CITED_TTL_SECS = 3600

CHUNK = 262144
MAX_FRAME = 270336
MAX_TEXT = 102400
MAX_QUESTION = 2048
MAX_CAPTION = 120
MAX_ID = 36
# Request, client, ask and item ids: 1 to MAX_ID of these ASCII characters.
ID_ALPHABET = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_-"
# UTF-8 bytes of an item's summary and of its text sent to a phone, each cut at a character
# boundary, so the largest item still fits in one frame.
MAX_ITEM_TEXT = 16384
MAX_ITEM = 52428800
MAX_DAY = 524288000
MAX_INCOMING_TOTAL = 104857600
MAX_LONG_POLL_BATCH = 8
MAX_CONNECTIONS = 16
MAX_SESSIONS_PER_DEVICE = 3

LIMIT_NO_KEY = 20
LIMIT_STATIC = 300
LIMIT_PAIRING_FRAMES = 20
LIMIT_FAILED_SESSION_OPEN = 10
LIMIT_SESSION_OPENS_PER_DEVICE = 6
LIMIT_REQUESTS = 60
LIMIT_ASKS = 10
LIMIT_KEEPS = 30
PAGES_BACK = 7
EVENTS_KEPT_DAYS = 90
PHONE_BUILD = 1
MIN_PHONE_BUILD = 1
HTTP_OFF = 503
HTTP_ENDED = 410
HTTP_POLL_BUSY = 409
HTTP_REFUSED = 403
HTTP_NOT_FOUND = 404

# Code points a device name may not hold: C0 and C1 controls, DEL, the bidi controls (with
# U+061C), zero-width characters, the line and paragraph separators and the byte order mark.
NAME_FORBIDDEN = [
    [0x0000, 0x001F],
    [0x007F, 0x009F],
    [0x061C, 0x061C],
    [0x200B, 0x200F],
    [0x2028, 0x202E],
    [0x2066, 0x2069],
    [0xFEFF, 0xFEFF],
]

# Allowed MIME types and their magic bytes: alternatives of (offset, hex pattern), where a `.`
# matches any nibble.
MIME_MAGIC = [
    ("image/jpeg", [(0, "ffd8ff")]),
    ("image/png", [(0, "89504e47")]),
    ("image/heic", [(4, "6674797068656963"), (4, "6674797068656978"), (4, "667479706d696631")]),
    ("image/webp", [(0, "52494646........57454250")]),
    ("application/pdf", [(0, "255044462d")]),
    ("audio/webm", [(0, "1a45dfa3")]),
    ("audio/mp4", [(4, "66747970")]),
    ("audio/ogg", [(0, "4f676753")]),
    ("audio/mpeg", [(0, "494433"), (0, "fff.")]),
    ("audio/wav", [(0, "52494646........57415645")]),
]

REFUSALS = [
    "badPublicKey",
    "samePublicKey",
    "badCode",
    "badDeviceName",
    "tooShort",
    "badVersion",
    "wrongKind",
    "wrongSid",
    "badCounter",
    "counterGap",
    "badTag",
    "badProof",
    "commitMismatch",
    "badPadding",
    "badEnvelope",
    "chunkGap",
    "badMagic",
    "badId",
]

CHECK_ORDER = [
    "A frame shorter than its kind's fixed length (header, nonce, body and tag) is tooShort.",
    "Then version (badVersion), kind (wrongKind), sid (wrongSid), then the counter: a handshake",
    "frame must carry 0 (badCounter), a DATA frame exactly last + 1 (counterGap).",
    "Then the AEAD tag (badTag). The receive counter moves to ctr only after the tag verifies;",
    "a refused frame never advances it. Only then the envelope (badPadding, badEnvelope).",
    "Public keys are checked for length and the 0x04 prefix before any parse (badPublicKey).",
    "An unknown device id and a bad open proof give the same refusal (badProof).",
]


# Primitives -----------------------------------------------------------------------------------


def sha256(*parts):
    h = hashlib.sha256()
    for p in parts:
        h.update(p)
    return h.digest()


def hkdf(ikm, salt, info, length):
    return HKDF(algorithm=hashes.SHA256(), length=length, salt=salt, info=info.encode()).derive(ikm)


def gcm_seal(key, nonce, aad, pt):
    assert len(key) == 32 and len(nonce) == GCM_NONCE_LEN
    return AESGCM(key).encrypt(nonce, pt, aad)


def gcm_open(key, nonce, aad, ct):
    return AESGCM(key).decrypt(nonce, ct, aad)


def lp(b):
    assert len(b) < 1 << 16
    return struct.pack(">H", len(b)) + b


def b64url(b):
    import base64

    return base64.urlsafe_b64encode(b).rstrip(b"=").decode()


def fixed(label, n):
    """Deterministic bytes standing in for randomness."""
    out = b""
    counter = 0
    while len(out) < n:
        out += sha256(f"nocturne vectors {label} {counter}".encode())
        counter += 1
    return out[:n]


P256_ORDER = 0xFFFFFFFF00000000FFFFFFFFFFFFFFFFBCE6FAADA7179E84F3B9CAC2FC632551
P256_P = 0xFFFFFFFF00000001000000000000000000000000FFFFFFFFFFFFFFFFFFFFFFFF


def scalar_for(label, counter=0):
    value = int.from_bytes(sha256(f"nocturne vectors scalar {label} {counter}".encode()), "big")
    return value % (P256_ORDER - 1) + 1


class Key:
    def __init__(self, scalar):
        self.scalar = scalar
        self.private = ec.derive_private_key(scalar, ec.SECP256R1())
        self.pub = self.private.public_key().public_bytes(Encoding.X962, PublicFormat.UncompressedPoint)
        assert len(self.pub) == PUBKEY_LEN and self.pub[0] == 4

    def ecdh(self, peer_pub):
        peer = check_public_key(peer_pub)
        out = self.private.exchange(ec.ECDH(), peer)
        assert len(out) == ECDH_OUT_LEN
        return out

    def json(self):
        return {"scalar": f"{self.scalar:064x}", "public": self.pub.hex()}


class Refused(Exception):
    def __init__(self, code):
        assert code in REFUSALS, code
        super().__init__(code)
        self.code = code


def check_public_key(data):
    if len(data) != PUBKEY_LEN or data[0] != 0x04:
        raise Refused("badPublicKey")
    try:
        return ec.EllipticCurvePublicKey.from_encoded_point(ec.SECP256R1(), data)
    except ValueError:
        raise Refused("badPublicKey") from None


# Codes, names, words ----------------------------------------------------------------------------


def code_encode(s):
    assert len(s) == S_LEN
    n = int.from_bytes(s, "big")
    chars = []
    for i in range(CODE_LEN):
        shift = 5 * (CODE_LEN - 1 - i)
        chars.append(CODE_ALPHABET[(n >> shift) & 31])
    return "".join(chars)


def code_display(code):
    return CODE_SEPARATOR.join(code[i : i + CODE_GROUP] for i in range(0, CODE_LEN, CODE_GROUP))


def code_normalise(typed):
    out = []
    for ch in typed.upper():
        if ch in " -":
            continue
        ch = {"O": "0", "I": "1", "L": "1"}.get(ch, ch)
        if ch not in CODE_ALPHABET:
            raise Refused("badCode")
        out.append(ch)
    if len(out) != CODE_LEN:
        raise Refused("badCode")
    return "".join(out)


def code_decode(code):
    n = 0
    for ch in code:
        n = (n << 5) | CODE_ALPHABET.index(ch)
    return n.to_bytes(S_LEN, "big")


def check_device_name(name):
    raw = name.encode("utf-8")
    if not 1 <= len(raw) <= MAX_DEVICE_NAME:
        raise Refused("badDeviceName")
    for ch in name:
        cp = ord(ch)
        if any(lo <= cp <= hi for lo, hi in NAME_FORBIDDEN):
            raise Refused("badDeviceName")
    return raw


def load_words():
    with open(WORDS_FILE, "rb") as handle:
        raw = handle.read()
    assert hashlib.sha256(raw).hexdigest() == WORDS_SHA256, "words.txt is not the pinned BIP-39 list"
    words = raw.decode("utf-8").split("\n")
    if words and words[-1] == "":
        words.pop()
    assert len(words) == 2048 and len(set(words)) == 2048
    return words


# Channel steps ----------------------------------------------------------------------------------


def header(kind, sid, ctr, version=VERSION):
    assert len(sid) == SID_LEN
    return bytes([version, kind]) + sid + struct.pack(">Q", ctr)


def parse_header(frame):
    if len(frame) < HEADER_LEN:
        raise Refused("tooShort")
    return frame[0], frame[1], frame[2:18], struct.unpack(">Q", frame[18:26])[0]


def pairing_id(s):
    return hkdf(s, b"", INFO_PAIR_ID, PAIRING_ID_LEN)


def pair_keys(s, cpub, pid):
    salt = sha256(HASH_QR.encode(), cpub, pid)
    okm = hkdf(s, salt, INFO_PAIR, 64)
    return salt, okm[:32], okm[32:]


def envelope_json(body):
    pad = (-(ENVELOPE_HEADER_LEN + len(body))) % PAD_BLOCK
    return bytes([TYPE_JSON]) + struct.pack(">H", pad) + body + b"\x00" * pad


def chunk_body(call_id, seq, last, data):
    return struct.pack(">IIB", call_id, seq, 1 if last else 0) + data


def envelope_chunk(call_id, seq, last, data):
    assert len(data) <= CHUNK
    return bytes([TYPE_CHUNK]) + struct.pack(">H", 0) + chunk_body(call_id, seq, last, data)


def envelope_open(env):
    if len(env) < ENVELOPE_HEADER_LEN:
        raise Refused("tooShort")
    kind = env[0]
    pad = struct.unpack(">H", env[1:3])[0]
    rest = env[3:]
    if pad > len(rest):
        raise Refused("badPadding")
    body, padding = rest[: len(rest) - pad], rest[len(rest) - pad :]
    if any(padding):
        raise Refused("badPadding")
    if kind == TYPE_JSON:
        if len(env) % PAD_BLOCK:
            raise Refused("badPadding")
        return kind, body
    if kind == TYPE_CHUNK:
        if pad or len(body) < CHUNK_HEADER_LEN:
            raise Refused("badPadding")
        return kind, body
    raise Refused("badEnvelope")


def chunk_parse(body):
    call_id, seq, last = struct.unpack(">IIB", body[:CHUNK_HEADER_LEN])
    return call_id, seq, last, body[CHUNK_HEADER_LEN:]


def chunks_in_order(bodies):
    expected = 0
    for body in bodies:
        _, seq, _, _ = chunk_parse(body)
        if seq != expected:
            raise Refused("chunkGap")
        expected += 1


def pair_frame(kind, pid, key, nonce, env):
    h = header(kind, pid, 0)
    return h, h + nonce + gcm_seal(key, nonce, h, env)


def pair_open(frame, kind, pid, key):
    version, got_kind, sid, ctr = parse_header(frame)
    if len(frame) < HEADER_LEN + GCM_NONCE_LEN + GCM_TAG_LEN:
        raise Refused("tooShort")
    if version != VERSION:
        raise Refused("badVersion")
    if got_kind != kind:
        raise Refused("wrongKind")
    if sid != pid:
        raise Refused("wrongSid")
    if ctr != 0:
        raise Refused("badCounter")
    h = frame[:HEADER_LEN]
    nonce = frame[HEADER_LEN : HEADER_LEN + GCM_NONCE_LEN]
    try:
        return envelope_open(gcm_open(key, nonce, h, frame[HEADER_LEN + GCM_NONCE_LEN :]))
    except InvalidTag:
        raise Refused("badTag") from None


def commitment(dpub, np_, name_raw):
    return sha256(HASH_COMMIT.encode(), dpub, np_, sha256(name_raw))


def check_reveal(cpub, dpub, np_, name_raw, commit):
    check_public_key(dpub)
    if dpub == cpub:
        raise Refused("samePublicKey")
    if commitment(dpub, np_, name_raw) != commit:
        raise Refused("commitMismatch")


def words_salt(path, pid, host, cpub, dpub, name_raw, np_, nc):
    return sha256(
        HASH_WORDS.encode(), bytes([path]), pid, lp(host.encode()), cpub, dpub, lp(name_raw), np_, nc
    )


def words_for(shared, salt, wordlist):
    out = hkdf(shared, salt, INFO_WORDS, WORDS_OUT_LEN)
    bits = int.from_bytes(out, "big")
    indices = [(bits >> (40 - WORD_BITS * (i + 1))) & 0x7FF for i in range(WORD_COUNT)]
    return out, indices, [wordlist[i] for i in indices]


def bit(out, n):
    return (int.from_bytes(out, "big") >> (40 - 1 - n)) & 1


def device_key(shared, device_id):
    return hkdf(shared, device_id, INFO_DEVICE, 32)


def session_open(device_id, epub, on, kd):
    h = header(KIND_SESSION_OPEN, bytes(SID_LEN), 0)
    aad = h + device_id + epub
    proof = gcm_seal(kd, on, aad, PROOF_OPEN.encode())
    return h, aad, proof, h + device_id + epub + on + proof


def verify_open(frame, registry, ckey):
    """registry: deviceId -> Dpub. Returns (device_id, epub, on) or refuses."""
    version, kind, sid, ctr = parse_header(frame)
    need = HEADER_LEN + DEVICE_ID_LEN + PUBKEY_LEN + OPEN_NONCE_LEN + len(PROOF_OPEN) + GCM_TAG_LEN
    if len(frame) != need:
        raise Refused("tooShort")
    if version != VERSION:
        raise Refused("badVersion")
    if kind != KIND_SESSION_OPEN:
        raise Refused("wrongKind")
    if sid != bytes(SID_LEN):
        raise Refused("wrongSid")
    if ctr != 0:
        raise Refused("badCounter")
    at = HEADER_LEN
    device_id = frame[at : at + DEVICE_ID_LEN]
    at += DEVICE_ID_LEN
    epub = frame[at : at + PUBKEY_LEN]
    at += PUBKEY_LEN
    on = frame[at : at + OPEN_NONCE_LEN]
    at += OPEN_NONCE_LEN
    dpub = registry.get(device_id)
    if dpub is None:
        raise Refused("badProof")
    kd = device_key(ckey.ecdh(dpub), device_id)
    try:
        if gcm_open(kd, on, frame[: HEADER_LEN + DEVICE_ID_LEN + PUBKEY_LEN], frame[at:]) != PROOF_OPEN.encode():
            raise Refused("badProof")
    except InvalidTag:
        raise Refused("badProof") from None
    check_public_key(epub)
    return device_id, epub, on


def revoked_frame(kd, device_id, on, n):
    h = header(KIND_REVOKED, bytes(SID_LEN), 0)
    aad = h + device_id + on
    return h, aad, h + n + gcm_seal(kd, n, aad, PROOF_REVOKED.encode())


def revoked_open(frame, kd, device_id, pending_on):
    version, kind, sid, ctr = parse_header(frame)
    if len(frame) != HEADER_LEN + GCM_NONCE_LEN + len(PROOF_REVOKED) + GCM_TAG_LEN:
        raise Refused("tooShort")
    if version != VERSION:
        raise Refused("badVersion")
    if kind != KIND_REVOKED:
        raise Refused("wrongKind")
    if sid != bytes(SID_LEN):
        raise Refused("wrongSid")
    if ctr != 0:
        raise Refused("badCounter")
    h = frame[:HEADER_LEN]
    n = frame[HEADER_LEN : HEADER_LEN + GCM_NONCE_LEN]
    try:
        gcm_open(kd, n, h + device_id + pending_on, frame[HEADER_LEN + GCM_NONCE_LEN :])
    except InvalidTag:
        raise Refused("badTag") from None


def session_keys(e, f, c, d, device_id, on, sid):
    terms = {
        "ecdhEF": e.ecdh(f.pub),
        "ecdhEC": e.ecdh(c.pub),
        "ecdhDF": d.ecdh(f.pub),
        "ecdhDC": d.ecdh(c.pub),
    }
    # The computer's side reaches the same terms from its own keys.
    assert terms["ecdhEF"] == f.ecdh(e.pub) and terms["ecdhEC"] == c.ecdh(e.pub)
    assert terms["ecdhDF"] == f.ecdh(d.pub) and terms["ecdhDC"] == c.ecdh(d.pub)
    ikm = terms["ecdhEF"] + terms["ecdhEC"] + terms["ecdhDF"] + terms["ecdhDC"]
    salt = sha256(HASH_SESSION.encode(), device_id, e.pub, on, sid, f.pub, c.pub, d.pub)
    okm = hkdf(ikm, salt, INFO_SESSION, 64)
    return terms, ikm, salt, okm[:32], okm[32:]


def data_nonce(direction, ctr):
    prefix = NONCE_UP if direction == "up" else NONCE_DOWN
    return prefix.encode() + struct.pack(">Q", ctr)


def data_frame(key, direction, sid, ctr, env):
    h = header(KIND_DATA, sid, ctr)
    nonce = data_nonce(direction, ctr)
    return h, nonce, h + gcm_seal(key, nonce, h, env)


def data_open(frame, key, direction, sid, last_ctr):
    version, kind, got_sid, ctr = parse_header(frame)
    if len(frame) < HEADER_LEN + GCM_TAG_LEN:
        raise Refused("tooShort")
    if version != VERSION:
        raise Refused("badVersion")
    if kind != KIND_DATA:
        raise Refused("wrongKind")
    if got_sid != sid:
        raise Refused("wrongSid")
    if ctr != last_ctr + 1:
        raise Refused("counterGap")
    h = frame[:HEADER_LEN]
    try:
        env = gcm_open(key, data_nonce(direction, ctr), h, frame[HEADER_LEN:])
    except InvalidTag:
        raise Refused("badTag") from None
    return envelope_open(env)


def magic_matches(mime, data):
    rules = dict(MIME_MAGIC).get(mime)
    if rules is None:
        raise Refused("badMagic")
    for offset, pattern in rules:
        nibbles = pattern
        window = data[offset : offset + len(nibbles) // 2].hex()
        if len(window) == len(nibbles) and all(p in (".", w) for p, w in zip(nibbles, window)):
            return
    raise Refused("badMagic")


def check_id(value):
    if not 1 <= len(value) <= MAX_ID or any(ch not in ID_ALPHABET for ch in value):
        raise Refused("badId")


def cut_text(text):
    """The longest prefix of at most MAX_ITEM_TEXT UTF-8 bytes that ends on a character."""
    raw = text.encode("utf-8")
    if len(raw) <= MAX_ITEM_TEXT:
        return text, False
    return raw[:MAX_ITEM_TEXT].decode("utf-8", errors="ignore"), True


def js(obj):
    """JSON bodies: compact, keys in the order written, UTF-8."""
    return json.dumps(obj, separators=(",", ":"), ensure_ascii=False).encode("utf-8")


# The vectors --------------------------------------------------------------------------------------


def constants():
    names = [
        "VERSION", "HEADER_LEN", "KIND_HELLO", "KIND_PAIR_COMMIT", "KIND_PAIR_NONCE",
        "KIND_PAIR_REVEAL", "KIND_PAIR_OUTCOME", "KIND_SESSION_OPEN", "KIND_SESSION_ACCEPT",
        "KIND_DATA", "KIND_REVOKED", "S_LEN", "CODE_LEN", "CODE_GROUP", "CODE_SEPARATOR",
        "CODE_ALPHABET", "PAIRING_ID_LEN", "NONCE_PAIR_LEN", "DEVICE_ID_LEN", "SID_LEN",
        "OPEN_NONCE_LEN", "GCM_NONCE_LEN", "GCM_TAG_LEN", "PUBKEY_LEN", "ECDH_OUT_LEN",
        "MAX_DEVICE_NAME", "LINK_VERSION", "PATH_PAIR", "PATH_RELINK", "PATH_SESSION",
        "INFO_PAIR_ID", "INFO_PAIR", "INFO_WORDS", "INFO_DEVICE", "INFO_SESSION", "HASH_QR",
        "HASH_COMMIT", "HASH_WORDS", "HASH_SESSION", "PROOF_OPEN", "PROOF_REVOKED", "NONCE_UP",
        "NONCE_DOWN", "PATH_SCANNED", "PATH_TYPED", "WORDS_OUT_LEN", "WORD_COUNT", "WORD_BITS",
        "TYPE_JSON", "TYPE_CHUNK", "ENVELOPE_HEADER_LEN", "CHUNK_HEADER_LEN", "PAD_BLOCK",
        "DELTA_FLUSH_MS", "PAIR_TTL_SECS", "REVEAL_WINDOW_SECS", "DECIDE_TTL_SECS",
        "PENDING_SESSION_TTL_SECS", "SESSION_TTL_SECS", "HANDSHAKE_TIMEOUT_SECS",
        "LONG_POLL_HOLD_SECS", "TAKE_BACK_SECS", "CITED_TTL_SECS", "CHUNK",
        "MAX_FRAME", "MAX_TEXT", "MAX_QUESTION", "MAX_CAPTION", "MAX_ID", "ID_ALPHABET", "MAX_ITEM_TEXT", "MAX_ITEM", "MAX_DAY",
        "MAX_INCOMING_TOTAL", "MAX_LONG_POLL_BATCH", "MAX_CONNECTIONS", "MAX_SESSIONS_PER_DEVICE",
        "LIMIT_NO_KEY", "LIMIT_STATIC", "LIMIT_PAIRING_FRAMES", "LIMIT_FAILED_SESSION_OPEN",
        "LIMIT_SESSION_OPENS_PER_DEVICE", "LIMIT_REQUESTS", "LIMIT_ASKS", "LIMIT_KEEPS",
        "PAGES_BACK", "EVENTS_KEPT_DAYS",
        "PHONE_BUILD", "MIN_PHONE_BUILD", "HTTP_OFF", "HTTP_ENDED", "HTTP_POLL_BUSY", "HTTP_REFUSED",
        "HTTP_NOT_FOUND",
    ]
    g = globals()
    out = {name: g[name] for name in names}
    out["NAME_FORBIDDEN"] = NAME_FORBIDDEN
    out["MIME_MAGIC"] = [
        {"mime": mime, "magic": [[offset, pattern] for offset, pattern in rules]}
        for mime, rules in MIME_MAGIC
    ]
    return out


def build():
    wordlist = load_words()
    words_raw = ("\n".join(wordlist) + "\n").encode()
    positive, negative = [], []

    def pos(name, step, inputs, intermediate, output):
        positive.append(
            {"name": name, "step": step, "inputs": inputs, "intermediate": intermediate, "output": output}
        )

    def neg(name, step, inputs, expect, fn):
        try:
            fn()
        except Refused as refused:
            assert refused.code == expect, (name, refused.code, expect)
        else:
            raise AssertionError(f"negative vector {name} was accepted")
        negative.append({"name": name, "step": step, "inputs": inputs, "expect": expect})

    c = Key(scalar_for("computer C"))
    d = Key(scalar_for("phone D"))
    e = Key(scalar_for("phone session E"))
    f = Key(scalar_for("computer session F"))
    f2 = Key(scalar_for("computer session F second"))
    relay = Key(scalar_for("relay R"))
    other_d = Key(scalar_for("phone D other"))
    keys = {name: k.json() for name, k in [
        ("C", c), ("D", d), ("E", e), ("F", f), ("F2", f2), ("relay", relay), ("otherD", other_d)
    ]}

    # Offer: secret, code, link, pairing id, pair keys -------------------------------------------
    s = fixed("secret S", S_LEN)
    code = code_encode(s)
    assert code_decode(code) == s and code_normalise(code) == code
    pos("code_round_trip", "code", {"s": s.hex()}, {"bits": f"{int.from_bytes(s, 'big'):0120b}"},
        {"code": code, "display": code_display(code), "s": code_decode(code).hex()})
    for label, n in [("all_zero", bytes(S_LEN)), ("all_ones", b"\xff" * S_LEN)]:
        cc = code_encode(n)
        pos(f"code_round_trip_{label}", "code", {"s": n.hex()}, {}, {"code": cc, "display": code_display(cc), "s": code_decode(cc).hex()})

    typed_forms = [
        ("lower_case", code.lower()),
        ("spaces", code_display(code)),
        ("hyphens", "-".join(code[i : i + CODE_GROUP] for i in range(0, CODE_LEN, CODE_GROUP))),
        ("mixed_spacing", " " + code_display(code).lower().replace(" ", " - ") + " "),
    ]
    for label, typed in typed_forms:
        pos(f"code_normalise_{label}", "codeNormalise", {"typed": typed}, {}, {"code": code_normalise(typed), "s": s.hex()})
    look_alike = "0O1IL0" + "oil01i" + "000000" + "111111"
    pos("code_normalise_look_alikes", "codeNormalise", {"typed": look_alike}, {},
        {"code": code_normalise(look_alike), "s": code_decode(code_normalise(look_alike)).hex()})
    assert code_normalise(look_alike) == "001110" + "011011" + "000000" + "111111"

    pid = pairing_id(s)
    pos("pairing_id", "pairingId", {"s": s.hex(), "salt": "", "info": INFO_PAIR_ID}, {}, {"pairingId": pid.hex()})

    host = "amber-otter-lamp-quiet.trycloudflare.com"
    link = f"https://{host}{PATH_PAIR}#{LINK_VERSION}.{b64url(s)}.{b64url(c.pub)}"
    pos("pairing_link", "link", {"host": host, "s": s.hex(), "cpub": c.pub.hex()},
        {"sBase64url": b64url(s), "cpubBase64url": b64url(c.pub)}, {"link": link})

    salt_qr, kp_up, kp_down = pair_keys(s, c.pub, pid)
    pos("pair_keys", "pairKeys", {"s": s.hex(), "cpub": c.pub.hex(), "pairingId": pid.hex()},
        {"salt": salt_qr.hex(), "info": INFO_PAIR}, {"up": kp_up.hex(), "down": kp_down.hex()})

    # Pairing frames -------------------------------------------------------------------------------
    hello_req = header(KIND_HELLO, pid, 0)
    hello_ans = header(KIND_HELLO, pid, 0) + c.pub
    pos("hello", "hello", {"pairingId": pid.hex(), "cpub": c.pub.hex()}, {"request": hello_req.hex()},
        {"answer": hello_ans.hex()})

    name = "Ana\u2019s iPhone"
    name_raw = check_device_name(name)
    np_ = fixed("phone nonce np", NONCE_PAIR_LEN)
    nc = fixed("computer nonce nc", NONCE_PAIR_LEN)
    commit = commitment(d.pub, np_, name_raw)
    pos("commitment", "commit", {"dpub": d.pub.hex(), "np": np_.hex(), "deviceName": name, "deviceNameUtf8": name_raw.hex()},
        {"prefix": HASH_COMMIT, "nameHash": sha256(name_raw).hex()}, {"commit": commit.hex()})

    pair_frames = {}

    def pairing(label, kind, key_name, key, body_obj, nonce_label):
        body = js(body_obj)
        env = envelope_json(body)
        nonce = fixed(nonce_label, GCM_NONCE_LEN)
        h, frame = pair_frame(kind, pid, key, nonce, env)
        assert pair_open(frame, kind, pid, key) == (TYPE_JSON, body)
        pair_frames[label] = frame
        pos(f"pair_{label}", "pairFrame",
            {"kind": kind, "pairingId": pid.hex(), "key": key.hex(), "keyHalf": key_name, "nonce": nonce.hex(),
             "body": body.decode()},
            {"header": h.hex(), "envelope": env.hex(), "aad": h.hex()}, {"frame": frame.hex()})

    pairing("commit", KIND_PAIR_COMMIT, "up", kp_up,
            {"path": PATH_SCANNED, "deviceName": name, "commit": b64url(commit)}, "nonce pair commit")
    pairing("nonce", KIND_PAIR_NONCE, "down", kp_down, {"nc": b64url(nc)}, "nonce pair nonce")
    pairing("reveal", KIND_PAIR_REVEAL, "up", kp_up, {"dpub": b64url(d.pub), "np": b64url(np_)}, "nonce pair reveal")
    device_id = fixed("device id", DEVICE_ID_LEN)
    pairing("outcome_allowed", KIND_PAIR_OUTCOME, "down", kp_down, {"allowed": True, "deviceId": b64url(device_id)}, "nonce pair outcome")
    pairing("outcome_refused", KIND_PAIR_OUTCOME, "down", kp_down, {"allowed": False}, "nonce pair refused")

    check_reveal(c.pub, d.pub, np_, name_raw, commit)

    # Words --------------------------------------------------------------------------------------
    shared_cd = c.ecdh(d.pub)
    assert shared_cd == d.ecdh(c.pub)
    for label, path in [("scanned", PATH_SCANNED), ("typed", PATH_TYPED)]:
        salt_w = words_salt(path, pid, host, c.pub, d.pub, name_raw, np_, nc)
        out, idx, chosen = words_for(shared_cd, salt_w, wordlist)
        pos(f"words_{label}", "words",
            {"path": path, "pairingId": pid.hex(), "host": host, "cpub": c.pub.hex(), "dpub": d.pub.hex(),
             "deviceName": name, "np": np_.hex(), "nc": nc.hex()},
            {"ecdhCD": shared_cd.hex(), "salt": salt_w.hex(), "out": out.hex(), "bit32": bit(out, 32)},
            {"indices": idx, "words": chosen})
    look = "amber-otter-lamp-quiet.trycloudflare.co"
    salt_l = words_salt(PATH_SCANNED, pid, look, c.pub, d.pub, name_raw, np_, nc)
    out_l, idx_l, chosen_l = words_for(shared_cd, salt_l, wordlist)
    pos("words_look_alike_host", "words",
        {"path": PATH_SCANNED, "pairingId": pid.hex(), "host": look, "cpub": c.pub.hex(), "dpub": d.pub.hex(),
         "deviceName": name, "np": np_.hex(), "nc": nc.hex()},
        {"ecdhCD": shared_cd.hex(), "salt": salt_l.hex(), "out": out_l.hex(), "bit32": bit(out_l, 32)},
        {"indices": idx_l, "words": chosen_l})
    for attempt in range(256):
        nc_try = fixed(f"computer nonce nc bit32 {attempt}", NONCE_PAIR_LEN)
        salt_b = words_salt(PATH_TYPED, pid, host, c.pub, d.pub, name_raw, np_, nc_try)
        out_b, idx_b, chosen_b = words_for(shared_cd, salt_b, wordlist)
        if bit(out_b, 32) == 1 and idx_b[2] & 1 == 1:
            break
    else:
        raise AssertionError("no transcript with bit 32 set")
    pos("words_bit32_set", "words",
        {"path": PATH_TYPED, "pairingId": pid.hex(), "host": host, "cpub": c.pub.hex(), "dpub": d.pub.hex(),
         "deviceName": name, "np": np_.hex(), "nc": nc_try.hex(), "searchAttempt": attempt},
        {"ecdhCD": shared_cd.hex(), "salt": salt_b.hex(), "out": out_b.hex(), "bit32": 1},
        {"indices": idx_b, "words": chosen_b})

    # ECDH with a leading zero byte ----------------------------------------------------------------
    za = Key(scalar_for("leading zero A"))
    for counter in range(1, 100000):
        zb = Key(scalar_for("leading zero B", counter))
        z = za.ecdh(zb.pub)
        if z[0] == 0:
            break
    else:
        raise AssertionError("no leading-zero shared secret")
    zid = fixed("leading zero device id", DEVICE_ID_LEN)
    pos("ecdh_leading_zero", "ecdh",
        {"a": za.json(), "b": {**zb.json(), "searchCounter": counter}, "deviceId": zid.hex()},
        {"kd": device_key(z, zid).hex()}, {"shared": z.hex(), "length": len(z)})

    # Device key, session open, accept, revoked ---------------------------------------------------
    kd = device_key(shared_cd, device_id)
    pos("device_key", "deviceKey", {"dpub": d.pub.hex(), "cpub": c.pub.hex(), "deviceId": device_id.hex()},
        {"ecdhDC": shared_cd.hex(), "info": INFO_DEVICE}, {"kd": kd.hex()})

    on = fixed("open nonce", OPEN_NONCE_LEN)
    h_open, aad_open, proof, open_frame = session_open(device_id, e.pub, on, kd)
    registry = {device_id: d.pub}
    assert verify_open(open_frame, registry, c) == (device_id, e.pub, on)
    pos("session_open", "sessionOpen",
        {"deviceId": device_id.hex(), "epub": e.pub.hex(), "openNonce": on.hex(), "kd": kd.hex()},
        {"header": h_open.hex(), "aad": aad_open.hex(), "proof": proof.hex()}, {"frame": open_frame.hex()})

    sid = fixed("session id", SID_LEN)
    accept = header(KIND_SESSION_ACCEPT, sid, 0) + f.pub
    pos("session_accept", "sessionAccept", {"sid": sid.hex(), "fpub": f.pub.hex()}, {}, {"frame": accept.hex()})

    terms, ikm, salt_s, k_up, k_down = session_keys(e, f, c, d, device_id, on, sid)
    pos("session_keys", "sessionKeys",
        {"deviceId": device_id.hex(), "epub": e.pub.hex(), "openNonce": on.hex(), "sid": sid.hex(),
         "fpub": f.pub.hex(), "cpub": c.pub.hex(), "dpub": d.pub.hex()},
        {**{k: v.hex() for k, v in terms.items()}, "ikm": ikm.hex(), "salt": salt_s.hex(), "info": INFO_SESSION},
        {"up": k_up.hex(), "down": k_down.hex()})

    n_rev = fixed("revoked nonce", GCM_NONCE_LEN)
    h_rev, aad_rev, rev = revoked_frame(kd, device_id, on, n_rev)
    revoked_open(rev, kd, device_id, on)
    pos("revoked", "revoked", {"kd": kd.hex(), "deviceId": device_id.hex(), "openNonce": on.hex(), "nonce": n_rev.hex()},
        {"header": h_rev.hex(), "aad": aad_rev.hex()}, {"frame": rev.hex()})

    # Envelopes and data frames ------------------------------------------------------------------
    hello_body = js({"id": 0, "answer": {"type": "hello", "hello": {
        "computerName": "Kitchen laptop", "deviceName": name, "connection": "simple", "version": "0.1.0",
        "minPhoneBuild": MIN_PHONE_BUILD}}})
    status_req = js({"id": 1, "call": {"cmd": "getStatus"}})
    keep_req = js({"id": 2, "call": {"cmd": "keepText", "text": "Bin day moved to Thursday", "clientId": "c-0001"}})
    status_reply = js({"id": 1, "answer": {"type": "status", "status": {"readiness": "ready", "pageDate": "2026-11-13", "needsYou": 2}}})

    env = envelope_json(status_req)
    pos("envelope_json", "envelope", {"type": TYPE_JSON, "body": status_req.decode()},
        {"bodyLength": len(status_req), "padLen": env[1] << 8 | env[2]}, {"envelope": env.hex(), "length": len(env)})
    exact = js({"id": 7, "call": {"cmd": "ask", "question": "x" * 200}})
    exact = exact[:-3] + b"x" * (PAD_BLOCK - ENVELOPE_HEADER_LEN - len(exact)) + exact[-3:]
    exact_env = envelope_json(exact)
    assert len(exact_env) == PAD_BLOCK and exact_env[1:3] == b"\x00\x00"
    json.loads(exact)
    pos("envelope_json_exact_block", "envelope", {"type": TYPE_JSON, "body": exact.decode()},
        {"bodyLength": len(exact), "padLen": 0}, {"envelope": exact_env.hex(), "length": len(exact_env)})
    chunk_data = fixed("chunk bytes", 48)
    chunk_env = envelope_chunk(5, 0, True, chunk_data)
    pos("envelope_chunk", "envelope", {"type": TYPE_CHUNK, "callId": 5, "seq": 0, "last": True, "bytes": chunk_data.hex()},
        {"body": chunk_body(5, 0, True, chunk_data).hex(), "padLen": 0}, {"envelope": chunk_env.hex(), "length": len(chunk_env)})

    data_frames = {}
    for direction, ctr, body, key in [
        ("down", 1, hello_body, k_down),
        ("up", 1, status_req, k_up),
        ("up", 2, keep_req, k_up),
        ("down", 2, status_reply, k_down),
    ]:
        env_d = envelope_json(body)
        h, nonce, frame = data_frame(key, direction, sid, ctr, env_d)
        assert data_open(frame, key, direction, sid, ctr - 1) == (TYPE_JSON, body)
        data_frames[(direction, ctr)] = frame
        pos(f"data_{direction}_{ctr}", "data",
            {"direction": direction, "ctr": ctr, "sid": sid.hex(), "key": key.hex(), "body": body.decode()},
            {"header": h.hex(), "nonce": nonce.hex(), "envelope": env_d.hex()}, {"frame": frame.hex()})

    chunk_frame_env = envelope_chunk(2, 0, True, chunk_data)
    h, nonce, frame = data_frame(k_up, "up", sid, 3, chunk_frame_env)
    assert data_open(frame, k_up, "up", sid, 2) == (TYPE_CHUNK, chunk_body(2, 0, True, chunk_data))
    pos("data_up_3_chunk", "data",
        {"direction": "up", "ctr": 3, "sid": sid.hex(), "key": k_up.hex(), "envelope": chunk_frame_env.hex()},
        {"header": h.hex(), "nonce": nonce.hex()}, {"frame": frame.hex()})

    # Magic bytes --------------------------------------------------------------------------------
    samples = [
        ("image/jpeg", bytes.fromhex("ffd8ffe000104a464946")),
        ("image/png", bytes.fromhex("89504e470d0a1a0a")),
        ("image/heic", bytes.fromhex("0000001866747970686569630000")),
        ("image/webp", b"RIFF\x24\x00\x00\x00WEBPVP8 "),
        ("application/pdf", b"%PDF-1.7\n"),
        ("audio/webm", bytes.fromhex("1a45dfa39f4286")),
        ("audio/mp4", bytes.fromhex("0000001c667479704d344120")),
        ("audio/ogg", b"OggS\x00\x02"),
        ("audio/mpeg", b"ID3\x04\x00"),
        ("audio/mpeg", bytes.fromhex("fffb9064")),
        ("audio/wav", b"RIFF\x24\x00\x00\x00WAVEfmt "),
    ]
    for i, (mime, data) in enumerate(samples):
        magic_matches(mime, data)
        pos(f"magic_{mime.replace('/', '_')}_{i}", "magic", {"mime": mime, "bytes": data.hex()}, {}, {"accepted": True})

    # Negative vectors -----------------------------------------------------------------------------
    off_curve = bytearray(c.pub)
    off_curve[-1] ^= 1
    off_curve = bytes(off_curve)
    compressed = bytes([2 + (c.pub[-1] & 1)]) + c.pub[1:33]
    big_x = b"\x04" + P256_P.to_bytes(32, "big") + c.pub[33:]
    big_y = b"\x04" + c.pub[1:33] + P256_P.to_bytes(32, "big")
    for label, data in [
        ("off_curve", off_curve),
        ("compressed_33", compressed),
        ("single_zero", b"\x00"),
        ("x_not_below_p", big_x),
        ("y_not_below_p", big_y),
        ("no_0x04_prefix", b"\x05" + c.pub[1:]),
        ("too_long", c.pub + b"\x00"),
    ]:
        neg(f"public_key_{label}", "publicKey", {"key": data.hex()}, "badPublicKey",
            lambda data=data: check_public_key(data))
    neg("reveal_dpub_equals_cpub", "reveal",
        {"cpub": c.pub.hex(), "dpub": c.pub.hex(), "np": np_.hex(), "deviceName": name,
         "commit": commitment(c.pub, np_, name_raw).hex()},
        "samePublicKey", lambda: check_reveal(c.pub, c.pub, np_, name_raw, commitment(c.pub, np_, name_raw)))

    for label, typed in [
        ("with_u", code[:-1] + "U"),
        ("too_short", code[:-1]),
        ("too_long", code + "0"),
        ("other_character", code[:-1] + "*"),
        ("tab_separator", code[:12] + "\t" + code[12:]),
    ]:
        neg(f"code_{label}", "codeNormalise", {"typed": typed}, "badCode", lambda typed=typed: code_normalise(typed))

    for label, bad in [
        ("41_bytes", "x" * 41),
        ("multibyte_over_40", "\u00e9" * 21),
        ("empty", ""),
        ("bidi_rlo", "Pixel\u202e9"),
        ("bidi_lrm", "\u200ePixel"),
        ("bidi_isolate", "Pixel\u20669"),
        ("c0_control", "Pixel\n9"),
        ("c1_control", "Pixel\u00859"),
        ("arabic_letter_mark", "Pixel\u061c9"),
        ("zero_width_space", "Pixel\u200b9"),
        ("zero_width_non_joiner", "Pixel\u200c9"),
        ("zero_width_joiner", "Pixel\u200d9"),
        ("line_separator", "Pixel\u20289"),
        ("paragraph_separator", "Pixel\u20299"),
        ("byte_order_mark", "\ufeffPixel"),
    ]:
        neg(f"device_name_{label}", "deviceName", {"deviceName": bad, "utf8": bad.encode().hex()}, "badDeviceName",
            lambda bad=bad: check_device_name(bad))
    ok40 = "\u00e9" * 20
    pos("device_name_40_bytes", "deviceName", {"deviceName": ok40}, {}, {"utf8": check_device_name(ok40).hex()})

    frame_up_1 = data_frames[("up", 1)]
    neg("data_other_direction_key", "data",
        {"frame": frame_up_1.hex(), "key": k_down.hex(), "direction": "down", "sid": sid.hex(), "lastCtr": 0},
        "badTag", lambda: data_open(frame_up_1, k_down, "down", sid, 0))
    neg("data_up_key_down_nonce", "data",
        {"frame": frame_up_1.hex(), "key": k_up.hex(), "direction": "down", "sid": sid.hex(), "lastCtr": 0},
        "badTag", lambda: data_open(frame_up_1, k_up, "down", sid, 0))

    sid2 = fixed("session id second", SID_LEN)
    _, _, _, k2_up, _ = session_keys(e, f2, c, d, device_id, on, sid2)
    replayed = frame_up_1[:2] + sid2 + frame_up_1[18:]
    neg("data_replayed_under_other_sid", "data",
        {"frame": replayed.hex(), "key": k2_up.hex(), "direction": "up", "sid": sid2.hex(), "lastCtr": 0,
         "otherSession": {"fpub": f2.pub.hex(), "sid": sid2.hex()}},
        "badTag", lambda: data_open(replayed, k2_up, "up", sid2, 0))
    neg("data_unknown_sid", "data",
        {"frame": frame_up_1.hex(), "key": k2_up.hex(), "direction": "up", "sid": sid2.hex(), "lastCtr": 0},
        "wrongSid", lambda: data_open(frame_up_1, k2_up, "up", sid2, 0))
    _, _, frame_up_3 = data_frame(k_up, "up", sid, 3, envelope_json(status_req))
    neg("data_counter_gap", "data",
        {"frame": frame_up_3.hex(), "key": k_up.hex(), "direction": "up", "sid": sid.hex(), "lastCtr": 1},
        "counterGap", lambda: data_open(frame_up_3, k_up, "up", sid, 1))
    neg("data_counter_repeat", "data",
        {"frame": frame_up_1.hex(), "key": k_up.hex(), "direction": "up", "sid": sid.hex(), "lastCtr": 1},
        "counterGap", lambda: data_open(frame_up_1, k_up, "up", sid, 1))
    as_open = frame_up_1[:1] + bytes([KIND_SESSION_OPEN]) + frame_up_1[2:]
    neg("data_kind_changed", "data",
        {"frame": as_open.hex(), "key": k_up.hex(), "direction": "up", "sid": sid.hex(), "lastCtr": 0},
        "wrongKind", lambda: data_open(as_open, k_up, "up", sid, 0))
    v2 = bytes([2]) + frame_up_1[1:]
    neg("data_version_2", "data",
        {"frame": v2.hex(), "key": k_up.hex(), "direction": "up", "sid": sid.hex(), "lastCtr": 0},
        "badVersion", lambda: data_open(v2, k_up, "up", sid, 0))
    flipped = frame_up_1[:-1] + bytes([frame_up_1[-1] ^ 1])
    neg("data_tag_flipped", "data",
        {"frame": flipped.hex(), "key": k_up.hex(), "direction": "up", "sid": sid.hex(), "lastCtr": 0},
        "badTag", lambda: data_open(flipped, k_up, "up", sid, 0))
    neg("data_too_short", "data",
        {"frame": frame_up_1[:HEADER_LEN + 4].hex(), "key": k_up.hex(), "direction": "up", "sid": sid.hex(), "lastCtr": 0},
        "tooShort", lambda: data_open(frame_up_1[:HEADER_LEN + 4], k_up, "up", sid, 0))

    commit_frame = pair_frames["commit"]
    commit_as_reveal = commit_frame[:1] + bytes([KIND_PAIR_REVEAL]) + commit_frame[2:]
    neg("pair_kind_swapped", "pairFrame",
        {"frame": commit_as_reveal.hex(), "kind": KIND_PAIR_REVEAL, "pairingId": pid.hex(), "key": kp_up.hex()},
        "badTag", lambda: pair_open(commit_as_reveal, KIND_PAIR_REVEAL, pid, kp_up))
    neg("pair_other_direction_key", "pairFrame",
        {"frame": commit_frame.hex(), "kind": KIND_PAIR_COMMIT, "pairingId": pid.hex(), "key": kp_down.hex()},
        "badTag", lambda: pair_open(commit_frame, KIND_PAIR_COMMIT, pid, kp_down))
    # Typed path through a relay that answered HELLO with its own key: the phone's Kp differs.
    _, relay_up, _ = pair_keys(s, relay.pub, pid)
    relay_nonce = fixed("nonce pair commit relay", GCM_NONCE_LEN)
    relay_body = js({"path": PATH_TYPED, "deviceName": name, "commit": b64url(commit)})
    _, relay_commit = pair_frame(KIND_PAIR_COMMIT, pid, relay_up, relay_nonce, envelope_json(relay_body))
    neg("pair_relay_cpub", "pairFrame",
        {"frame": relay_commit.hex(), "kind": KIND_PAIR_COMMIT, "pairingId": pid.hex(), "key": kp_up.hex(),
         "relayCpub": relay.pub.hex(), "relayKpUp": relay_up.hex()},
        "badTag", lambda: pair_open(relay_commit, KIND_PAIR_COMMIT, pid, kp_up))

    wrong_np = fixed("phone nonce np wrong", NONCE_PAIR_LEN)
    neg("reveal_not_matching_commit", "reveal",
        {"cpub": c.pub.hex(), "dpub": d.pub.hex(), "np": wrong_np.hex(), "deviceName": name, "commit": commit.hex()},
        "commitMismatch", lambda: check_reveal(c.pub, d.pub, wrong_np, name_raw, commit))
    other_name = "Ana\u2019s iPad"
    neg("reveal_changed_device_name", "reveal",
        {"cpub": c.pub.hex(), "dpub": d.pub.hex(), "np": np_.hex(), "deviceName": other_name, "commit": commit.hex()},
        "commitMismatch", lambda: check_reveal(c.pub, d.pub, np_, check_device_name(other_name), commit))
    neg("reveal_other_dpub", "reveal",
        {"cpub": c.pub.hex(), "dpub": other_d.pub.hex(), "np": np_.hex(), "deviceName": name, "commit": commit.hex()},
        "commitMismatch", lambda: check_reveal(c.pub, other_d.pub, np_, name_raw, commit))

    e_other = Key(scalar_for("phone session E other"))
    open_new_e = open_frame[: HEADER_LEN + DEVICE_ID_LEN] + e_other.pub + open_frame[HEADER_LEN + DEVICE_ID_LEN + PUBKEY_LEN :]
    neg("open_proof_epub_changed", "sessionOpen",
        {"frame": open_new_e.hex(), "registry": {device_id.hex(): d.pub.hex()}, "computer": "C"},
        "badProof", lambda: verify_open(open_new_e, registry, c))
    other_id = fixed("device id other", DEVICE_ID_LEN)
    open_new_id = open_frame[:HEADER_LEN] + other_id + open_frame[HEADER_LEN + DEVICE_ID_LEN :]
    both = {device_id: d.pub, other_id: d.pub}
    neg("open_proof_device_id_changed", "sessionOpen",
        {"frame": open_new_id.hex(), "registry": {k.hex(): v.hex() for k, v in both.items()}, "computer": "C"},
        "badProof", lambda: verify_open(open_new_id, both, c))
    neg("open_unknown_device", "sessionOpen",
        {"frame": open_new_id.hex(), "registry": {device_id.hex(): d.pub.hex()}, "computer": "C"},
        "badProof", lambda: verify_open(open_new_id, registry, c))
    bad_e = bytearray(open_frame)
    bad_e[HEADER_LEN + DEVICE_ID_LEN + PUBKEY_LEN - 1] ^= 1
    bad_e = bytes(bad_e)
    neg("open_epub_off_curve", "sessionOpen",
        {"frame": bad_e.hex(), "registry": {device_id.hex(): d.pub.hex()}, "computer": "C",
         "note": "the proof binds Epub, so the proof fails first: no ECDH on Epub happens"},
        "badProof", lambda: verify_open(bad_e, registry, c))
    # A phone that sealed a good proof over a bad key: the proof passes, the key check after it
    # refuses before any ECDH.
    _, _, _, open_bad_key = session_open(device_id, off_curve, on, kd)
    neg("open_valid_proof_off_curve_epub", "sessionOpen",
        {"frame": open_bad_key.hex(), "registry": {device_id.hex(): d.pub.hex()}, "computer": "C"},
        "badPublicKey", lambda: verify_open(open_bad_key, registry, c))
    open_ctr = open_frame[:18] + struct.pack(">Q", 1) + open_frame[26:]
    neg("open_counter_not_zero", "sessionOpen",
        {"frame": open_ctr.hex(), "registry": {device_id.hex(): d.pub.hex()}, "computer": "C"},
        "badCounter", lambda: verify_open(open_ctr, registry, c))

    on_other = fixed("open nonce other", OPEN_NONCE_LEN)
    neg("revoked_for_other_open", "revoked",
        {"frame": rev.hex(), "kd": kd.hex(), "deviceId": device_id.hex(), "pendingOpenNonce": on_other.hex()},
        "badTag", lambda: revoked_open(rev, kd, device_id, on_other))

    rev_sid = rev[:2] + sid + rev[18:]
    neg("revoked_sid_not_zero", "revoked",
        {"frame": rev_sid.hex(), "kd": kd.hex(), "deviceId": device_id.hex(), "pendingOpenNonce": on.hex()},
        "wrongSid", lambda: revoked_open(rev_sid, kd, device_id, on))
    rev_ctr = rev[:18] + struct.pack(">Q", 1) + rev[26:]
    neg("revoked_counter_not_zero", "revoked",
        {"frame": rev_ctr.hex(), "kd": kd.hex(), "deviceId": device_id.hex(), "pendingOpenNonce": on.hex()},
        "badCounter", lambda: revoked_open(rev_ctr, kd, device_id, on))
    rev_long = rev + b"\x00"
    neg("revoked_wrong_length", "revoked",
        {"frame": rev_long.hex(), "kd": kd.hex(), "deviceId": device_id.hex(), "pendingOpenNonce": on.hex()},
        "tooShort", lambda: revoked_open(rev_long, kd, device_id, on))
    commit_ctr = commit_frame[:18] + struct.pack(">Q", 1) + commit_frame[26:]
    neg("pair_counter_not_zero", "pairFrame",
        {"frame": commit_ctr.hex(), "kind": KIND_PAIR_COMMIT, "pairingId": pid.hex(), "key": kp_up.hex()},
        "badCounter", lambda: pair_open(commit_ctr, KIND_PAIR_COMMIT, pid, kp_up))

    over = bytes([TYPE_JSON]) + struct.pack(">H", 300) + status_req + bytes(PAD_BLOCK - 3 - len(status_req))
    neg("envelope_pad_beyond_body", "envelope", {"envelope": over.hex()}, "badPadding", lambda: envelope_open(over))
    nonzero = bytearray(envelope_json(status_req))
    nonzero[-1] = 1
    nonzero = bytes(nonzero)
    neg("envelope_nonzero_padding", "envelope", {"envelope": nonzero.hex()}, "badPadding", lambda: envelope_open(nonzero))
    unpadded = bytes([TYPE_JSON]) + struct.pack(">H", 0) + status_req
    neg("envelope_json_not_block_multiple", "envelope", {"envelope": unpadded.hex()}, "badPadding", lambda: envelope_open(unpadded))
    neg("envelope_unknown_type", "envelope", {"envelope": (b"\x03" + envelope_json(status_req)[1:]).hex()}, "badEnvelope",
        lambda: envelope_open(b"\x03" + envelope_json(status_req)[1:]))

    seq = [chunk_body(9, 0, False, b"a" * 8), chunk_body(9, 2, True, b"b" * 8)]
    neg("chunk_sequence_gap", "chunks", {"bodies": [b.hex() for b in seq]}, "chunkGap", lambda: chunks_in_order(seq))

    for label, mime, data in [
        ("png_declared_jpeg", "image/jpeg", bytes.fromhex("89504e470d0a1a0a")),
        ("riff_wave_declared_webp", "image/webp", b"RIFF\x24\x00\x00\x00WAVEfmt "),
        ("mp3_bad_sync", "audio/mpeg", bytes.fromhex("ffe39064")),
        ("not_allowed_mime", "text/html", b"<!doctype html>"),
        ("short_heic", "image/heic", bytes.fromhex("00000018667479")),
    ]:
        neg(f"magic_{label}", "magic", {"mime": mime, "bytes": data.hex()}, "badMagic",
            lambda mime=mime, data=data: magic_matches(mime, data))

    # Ids ----------------------------------------------------------------------------------------
    for label, good in [
        ("item", "itm_" + fixed("item id", 8).hex()),
        ("ask", "ask_" + fixed("ask id", 8).hex()),
        ("client", "c-0001"),
        ("longest", "a" * MAX_ID),
    ]:
        check_id(good)
        pos(f"id_{label}", "id", {"id": good}, {}, {"accepted": True})
    for label, bad in [
        ("empty", ""),
        ("too_long", "a" * (MAX_ID + 1)),
        ("slash", "itm/1"),
        ("dot", "itm.1"),
        ("space", "itm 1"),
        ("non_ascii", "itm_\u00e9"),
    ]:
        neg(f"id_{label}", "id", {"id": bad}, "badId", lambda bad=bad: check_id(bad))

    # Item text sent to a phone ------------------------------------------------------------------
    for label, text in [
        ("fits", "a" * MAX_ITEM_TEXT),
        ("ascii_over", "a" * (MAX_ITEM_TEXT + 1)),
        ("multibyte_boundary", "a" * (MAX_ITEM_TEXT - 1) + "\u00e9"),
        ("four_byte_boundary", "a" * (MAX_ITEM_TEXT - 2) + "\U0001f319"),
    ]:
        out, truncated = cut_text(text)
        pos(f"item_text_{label}", "itemText",
            {"length": len(text.encode()), "sha256": sha256(text.encode()).hex(), "tail": text[-2:]},
            {}, {"length": len(out.encode()), "sha256": sha256(out.encode()).hex(), "truncated": truncated})
    worst = "\u0001" * MAX_ITEM_TEXT
    item = {"id": "itm_" + "f" * 32, "kind": "document", "title": "\u0001" * 255,
            "keptAt": "2026-11-13T07:00:00.000Z", "durationMs": 4294967295, "pages": 4294967295,
            "hasOriginal": True, "summary": worst, "text": worst, "truncated": True}
    body = js({"id": 4294967295, "answer": {"type": "item", "item": item}})
    env = envelope_json(body)
    budget = MAX_FRAME - HEADER_LEN - GCM_TAG_LEN
    assert len(env) <= budget, (len(env), budget)
    pos("item_text_worst_case_fits_one_frame", "itemText",
        {"summaryBytes": MAX_ITEM_TEXT, "textBytes": MAX_ITEM_TEXT, "titleChars": 255,
         "every": "U+0001, which JSON writes as six bytes"},
        {"bodyLength": len(body), "envelopeLength": len(env)}, {"envelopeBudget": budget, "fits": True})

    return {
        "format": 1,
        "about": "Reference vectors for Away from home's sealed channel. Written by scripts/remote_vectors.py; do not edit.",
        "generator": {"script": "scripts/remote_vectors.py", "library": "cryptography"},
        "checkOrder": CHECK_ORDER,
        "refusals": REFUSALS,
        "constants": constants(),
        "words": {"count": len(wordlist), "sha256": hashlib.sha256(words_raw).hexdigest()},
        "keys": keys,
        "positive": positive,
        "negative": negative,
    }


def render(doc):
    return json.dumps(doc, indent=2) + "\n"


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--check", action="store_true", help="compare with the committed fixture, write nothing")
    args = parser.parse_args(argv)
    doc = build()
    text = render(doc)
    summary = f"{len(doc['positive'])} positive, {len(doc['negative'])} negative (cryptography {CRYPTOGRAPHY_VERSION})"
    if args.check:
        try:
            with open(FIXTURE, encoding="utf-8") as handle:
                committed = handle.read()
        except OSError as err:
            print(f"remote vectors: {err}", file=sys.stderr)
            return 1
        if committed != text:
            print("remote vectors: the committed fixture differs from the reference", file=sys.stderr)
            return 1
        print(f"remote vectors match: {summary}")
        return 0
    os.makedirs(os.path.dirname(FIXTURE), exist_ok=True)
    with open(FIXTURE, "w", encoding="utf-8") as handle:
        handle.write(text)
    print(f"wrote {os.path.relpath(FIXTURE, ROOT)}: {summary}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
