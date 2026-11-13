// The phone's side of the sealed channel, in WebCrypto: key checks, codes, pair and session
// keys, frames and envelopes. Pure functions of their inputs; nothing here keeps state, reads a
// clock or touches the network. The layout is plan 12 "Sealed channel", and every function is
// held to `src-tauri/tests/fixtures/remote-vectors.json` by `crypto.test.ts`.

import {
  type Bytes,
  bytes,
  concat,
  copy,
  equal,
  fromBase64Url,
  readU16,
  readU32,
  readU64,
  toBase64Url,
  u16,
  u32,
  u64,
  utf8,
} from "./bytes";
import * as P from "./protocol";

export type RefusalCode =
  | "badPublicKey"
  | "samePublicKey"
  | "badCode"
  | "badDeviceName"
  | "tooShort"
  | "badVersion"
  | "wrongKind"
  | "wrongSid"
  | "badCounter"
  | "counterGap"
  | "badTag"
  | "badProof"
  | "commitMismatch"
  | "badPadding"
  | "badEnvelope"
  | "chunkGap"
  | "badMagic"
  | "badId";

/** A frame, key or value the channel refuses. The code is the reference's name for the refusal. */
export class Refusal extends Error {
  readonly code: RefusalCode;
  constructor(code: RefusalCode) {
    super(code);
    this.name = "Refusal";
    this.code = code;
  }
}

const subtle = (): SubtleCrypto => globalThis.crypto.subtle;

export function randomBytes(length: number): Bytes {
  const out = bytes(length);
  globalThis.crypto.getRandomValues(out);
  return out;
}

// Public keys and ECDH ----------------------------------------------------------------------------

const P256_P = 0xffffffff00000001000000000000000000000000ffffffffffffffffffffffffn;
const P256_B = 0x5ac635d8aa3a93e7b3ebbd55769886bc651d06b0cc53b0f63bce3c3e27d2604bn;

function bigint(data: Uint8Array): bigint {
  let out = 0n;
  for (const byte of data) out = (out << 8n) | BigInt(byte);
  return out;
}

/**
 * Length, the 0x04 prefix, both coordinates below p and the curve equation, before any key is
 * imported. The library would refuse the same points; this does not depend on it.
 */
export function checkPublicKey(key: Uint8Array): void {
  if (key.length !== P.PUBKEY_LEN || key[0] !== 0x04) throw new Refusal("badPublicKey");
  const x = bigint(key.subarray(1, 33));
  const y = bigint(key.subarray(33, 65));
  if (x >= P256_P || y >= P256_P) throw new Refusal("badPublicKey");
  const lhs = (y * y) % P256_P;
  const rhs = (((x * x) % P256_P) * x - 3n * x + P256_B) % P256_P;
  if (lhs !== ((rhs % P256_P) + P256_P) % P256_P) throw new Refusal("badPublicKey");
}

const ECDH = { name: "ECDH", namedCurve: "P-256" } as const;

export async function importPublicKey(key: Uint8Array): Promise<CryptoKey> {
  checkPublicKey(key);
  try {
    return await subtle().importKey("raw", copy(key), ECDH, true, []);
  } catch {
    throw new Refusal("badPublicKey");
  }
}

/** A long-term phone key: ECDH P-256, never extractable, usable only to derive bits. */
export async function generateDeviceKey(): Promise<{ privateKey: CryptoKey; publicKey: Bytes }> {
  const pair = await subtle().generateKey(ECDH, false, ["deriveBits"]);
  const publicKey = copy(new Uint8Array(await subtle().exportKey("raw", pair.publicKey)));
  return { privateKey: pair.privateKey, publicKey };
}

/** A key made only to derive: one session's key E. */
export const generateSessionKey = generateDeviceKey;

/** The 32-byte x-coordinate, leading zeros kept. The peer's key is checked first. */
export async function ecdh(privateKey: CryptoKey, peer: Uint8Array): Promise<Bytes> {
  const peerKey = await importPublicKey(peer);
  const shared = copy(
    new Uint8Array(await subtle().deriveBits({ name: "ECDH", public: peerKey }, privateKey, 256)),
  );
  if (shared.length !== P.ECDH_OUT_LEN) throw new Refusal("badPublicKey");
  return shared;
}

// Hashes and key derivation ------------------------------------------------------------------------

export async function sha256(...parts: Uint8Array[]): Promise<Bytes> {
  return copy(new Uint8Array(await subtle().digest("SHA-256", concat(...parts))));
}

export async function hkdf(
  ikm: Uint8Array,
  salt: Uint8Array,
  info: string,
  length: number,
): Promise<Bytes> {
  const key = await subtle().importKey("raw", copy(ikm), "HKDF", false, ["deriveBits"]);
  const out = await subtle().deriveBits(
    { name: "HKDF", hash: "SHA-256", salt: copy(salt), info: utf8(info) },
    key,
    length * 8,
  );
  return copy(new Uint8Array(out));
}

export async function aesKey(raw: Uint8Array, usage: "encrypt" | "decrypt"): Promise<CryptoKey> {
  return subtle().importKey("raw", copy(raw), "AES-GCM", false, [usage]);
}

type AesKey = Uint8Array | CryptoKey;

async function sealGcm(key: AesKey, nonce: Uint8Array, aad: Uint8Array, plain: Uint8Array) {
  const k = key instanceof Uint8Array ? await aesKey(key, "encrypt") : key;
  return copy(
    new Uint8Array(
      await subtle().encrypt(
        { name: "AES-GCM", iv: copy(nonce), additionalData: copy(aad), tagLength: 128 },
        k,
        copy(plain),
      ),
    ),
  );
}

async function openGcm(key: AesKey, nonce: Uint8Array, aad: Uint8Array, sealed: Uint8Array) {
  const k = key instanceof Uint8Array ? await aesKey(key, "decrypt") : key;
  try {
    return copy(
      new Uint8Array(
        await subtle().decrypt(
          { name: "AES-GCM", iv: copy(nonce), additionalData: copy(aad), tagLength: 128 },
          k,
          copy(sealed),
        ),
      ),
    );
  } catch {
    throw new Refusal("badTag");
  }
}

export function lp(data: Uint8Array): Bytes {
  if (data.length >= 1 << 16) throw new Error("too long for lp");
  return concat(u16(data.length), data);
}

// Codes, names, ids -------------------------------------------------------------------------------

/** S as 24 Crockford characters, most significant bits first. */
export function codeEncode(s: Uint8Array): string {
  if (s.length !== P.S_LEN) throw new Error("S has the wrong length");
  let n = 0n;
  for (const byte of s) n = (n << 8n) | BigInt(byte);
  let out = "";
  for (let i = 0; i < P.CODE_LEN; i += 1) {
    const shift = BigInt(5 * (P.CODE_LEN - 1 - i));
    out += P.CODE_ALPHABET[Number((n >> shift) & 31n)];
  }
  return out;
}

/** The code as shown: four groups of six. */
export function codeDisplay(code: string): string {
  const groups: string[] = [];
  for (let i = 0; i < code.length; i += P.CODE_GROUP) groups.push(code.slice(i, i + P.CODE_GROUP));
  return groups.join(P.CODE_SEPARATOR);
}

/** Upper case, spaces and hyphens dropped, O to 0, I and L to 1; U and anything else refused. */
export function codeNormalise(typed: string): string {
  let out = "";
  for (const raw of typed.toUpperCase()) {
    if (raw === " " || raw === "-") continue;
    const ch = raw === "O" ? "0" : raw === "I" || raw === "L" ? "1" : raw;
    if (ch.length !== 1 || !P.CODE_ALPHABET.includes(ch)) throw new Refusal("badCode");
    out += ch;
  }
  if (out.length !== P.CODE_LEN) throw new Refusal("badCode");
  return out;
}

export function codeDecode(code: string): Bytes {
  let n = 0n;
  for (const ch of code) {
    const at = P.CODE_ALPHABET.indexOf(ch);
    if (at < 0) throw new Refusal("badCode");
    n = (n << 5n) | BigInt(at);
  }
  const out = bytes(P.S_LEN);
  for (let i = P.S_LEN - 1; i >= 0; i -= 1) {
    out[i] = Number(n & 0xffn);
    n >>= 8n;
  }
  return out;
}

/** 1 to 40 bytes of UTF-8, with no control, bidi, zero-width or separator characters. */
export function checkDeviceName(name: string): Bytes {
  const raw = utf8(name);
  if (raw.length < 1 || raw.length > P.MAX_DEVICE_NAME) throw new Refusal("badDeviceName");
  for (const ch of name) {
    const cp = ch.codePointAt(0) ?? 0;
    if (cp >= 0xd800 && cp <= 0xdfff) throw new Refusal("badDeviceName");
    if (P.NAME_FORBIDDEN.some(([lo, hi]) => cp >= lo && cp <= hi))
      throw new Refusal("badDeviceName");
  }
  return raw;
}

/** `name` cut to at most `MAX_DEVICE_NAME` bytes at a character boundary, forbidden characters dropped. */
export function cleanDeviceName(name: string): string {
  let out = "";
  let size = 0;
  for (const ch of name.trim()) {
    const cp = ch.codePointAt(0) ?? 0;
    if (cp >= 0xd800 && cp <= 0xdfff) continue;
    if (P.NAME_FORBIDDEN.some(([lo, hi]) => cp >= lo && cp <= hi)) continue;
    const n = utf8(ch).length;
    if (size + n > P.MAX_DEVICE_NAME) break;
    out += ch;
    size += n;
  }
  return out;
}

const ID_RE = new RegExp(`^[${P.ID_ALPHABET.replace("-", "\\-")}]{1,${P.MAX_ID}}$`);

/** Request, client, ask and item ids. */
export function checkId(id: string): void {
  if (!ID_RE.test(id)) throw new Refusal("badId");
}

/** True when the bytes begin as the allowed type says they must. */
export function magicMatches(mime: string, data: Uint8Array): boolean {
  const rule = P.MIME_MAGIC.find((m) => m.mime === mime);
  if (!rule) return false;
  const hex = Array.from(data.subarray(0, 32), (b) => b.toString(16).padStart(2, "0")).join("");
  return rule.magic.some(([offset, pattern]) => {
    const window = hex.slice(offset * 2, offset * 2 + pattern.length);
    return (
      window.length === pattern.length && [...pattern].every((p, i) => p === "." || p === window[i])
    );
  });
}

export function checkMagic(mime: string, data: Uint8Array): void {
  if (!magicMatches(mime, data)) throw new Refusal("badMagic");
}

/** The allowed type the bytes are, or null. For files whose own type is empty or odd. */
export function sniffMime(data: Uint8Array): string | null {
  return P.MIME_MAGIC.find((m) => magicMatches(m.mime, data))?.mime ?? null;
}

// Pairing -----------------------------------------------------------------------------------------

export function pairingId(s: Uint8Array): Promise<Bytes> {
  return hkdf(s, bytes(0), P.INFO_PAIR_ID, P.PAIRING_ID_LEN);
}

/** Kp, split by direction: `up` is for frames the phone sends, `down` for frames it receives. */
export async function pairKeys(
  s: Uint8Array,
  cpub: Uint8Array,
  pid: Uint8Array,
): Promise<{ salt: Bytes; up: Bytes; down: Bytes }> {
  const salt = await sha256(utf8(P.HASH_QR), cpub, pid);
  const okm = await hkdf(s, salt, P.INFO_PAIR, 64);
  return { salt, up: copy(okm.subarray(0, 32)), down: copy(okm.subarray(32)) };
}

export function pairingLink(host: string, s: Uint8Array, cpub: Uint8Array): string {
  return `https://${host}${P.PATH_PAIR}#${P.LINK_VERSION}.${toBase64Url(s)}.${toBase64Url(cpub)}`;
}

/** The secret and the computer's key from a link fragment, or null when it is not one. */
export function parsePairingFragment(fragment: string): { s: Bytes; cpub: Bytes } | null {
  const text = fragment.startsWith("#") ? fragment.slice(1) : fragment;
  const parts = text.split(".");
  if (parts.length !== 3 || parts[0] !== P.LINK_VERSION) return null;
  try {
    const s = fromBase64Url(parts[1] ?? "");
    const cpub = fromBase64Url(parts[2] ?? "");
    if (s.length !== P.S_LEN) return null;
    checkPublicKey(cpub);
    return { s, cpub };
  } catch {
    return null;
  }
}

export async function commitment(dpub: Uint8Array, np: Uint8Array, nameRaw: Uint8Array) {
  return sha256(utf8(P.HASH_COMMIT), dpub, np, await sha256(nameRaw));
}

/** The computer's check of a reveal: the key, `Dpub = Cpub`, then the commitment, in constant time. */
export async function checkReveal(
  cpub: Uint8Array,
  dpub: Uint8Array,
  np: Uint8Array,
  nameRaw: Uint8Array,
  commit: Uint8Array,
): Promise<void> {
  checkPublicKey(dpub);
  if (equal(dpub, cpub)) throw new Refusal("samePublicKey");
  if (!equal(await commitment(dpub, np, nameRaw), commit)) throw new Refusal("commitMismatch");
}

export interface WordsInput {
  path: number;
  pairingId: Uint8Array;
  host: string;
  cpub: Uint8Array;
  dpub: Uint8Array;
  nameRaw: Uint8Array;
  np: Uint8Array;
  nc: Uint8Array;
}

export function wordsSalt(i: WordsInput): Promise<Bytes> {
  return sha256(
    utf8(P.HASH_WORDS),
    bytes(1).fill(i.path),
    i.pairingId,
    lp(utf8(i.host)),
    i.cpub,
    i.dpub,
    lp(i.nameRaw),
    i.np,
    i.nc,
  );
}

/** Three BIP-39 indices from the first 33 bits, most significant first. */
export async function wordIndices(shared: Uint8Array, salt: Uint8Array): Promise<number[]> {
  const out = await hkdf(shared, salt, P.INFO_WORDS, P.WORDS_OUT_LEN);
  let bits = 0;
  for (const byte of out) bits = bits * 256 + byte;
  const indices: number[] = [];
  for (let i = 0; i < P.WORD_COUNT; i += 1) {
    const shift = 8 * P.WORDS_OUT_LEN - P.WORD_BITS * (i + 1);
    indices.push(Math.floor(bits / 2 ** shift) % 2 ** P.WORD_BITS);
  }
  return indices;
}

export async function checkWords(privateKey: CryptoKey, i: WordsInput): Promise<string[]> {
  const shared = await ecdh(privateKey, i.cpub);
  const indices = await wordIndices(shared, await wordsSalt(i));
  return indices.map((n) => P.WORDS[n] ?? "");
}

// Frames ------------------------------------------------------------------------------------------

export function header(kind: number, sid: Uint8Array, ctr: number, version = P.VERSION): Bytes {
  if (sid.length !== P.SID_LEN) throw new Error("sid has the wrong length");
  return concat(Uint8Array.of(version, kind), sid, u64(ctr));
}

export interface Header {
  version: number;
  kind: number;
  sid: Bytes;
  ctr: number;
}

export function parseHeader(frame: Uint8Array): Header {
  if (frame.length < P.HEADER_LEN) throw new Refusal("tooShort");
  return {
    version: frame[0] ?? 0,
    kind: frame[1] ?? 0,
    sid: copy(frame.subarray(2, 18)),
    ctr: readU64(frame, 18),
  };
}

export const zeroSid = (): Bytes => bytes(P.SID_LEN);

/** The unsealed HELLO that asks for the computer's key while an offer is open. */
export function helloRequest(pid: Uint8Array): Bytes {
  return header(P.KIND_HELLO, pid, 0);
}

/** The computer's key from its HELLO answer, checked; refuses anything that is not one. */
export function openHelloAnswer(frame: Uint8Array, pid: Uint8Array): Bytes {
  const h = parseHeader(frame);
  if (frame.length !== P.HEADER_LEN + P.PUBKEY_LEN) throw new Refusal("tooShort");
  if (h.version !== P.VERSION) throw new Refusal("badVersion");
  if (h.kind !== P.KIND_HELLO) throw new Refusal("wrongKind");
  if (!equal(h.sid, pid)) throw new Refusal("wrongSid");
  if (h.ctr !== 0) throw new Refusal("badCounter");
  const cpub = copy(frame.subarray(P.HEADER_LEN));
  checkPublicKey(cpub);
  return cpub;
}

// Envelopes ---------------------------------------------------------------------------------------

/** `type || padLen || body || zeros`, padded so the whole is a multiple of 256 bytes. */
export function envelopeJson(body: Uint8Array): Bytes {
  const pad = (P.PAD_BLOCK - ((P.ENVELOPE_HEADER_LEN + body.length) % P.PAD_BLOCK)) % P.PAD_BLOCK;
  return concat(Uint8Array.of(P.TYPE_JSON), u16(pad), body, bytes(pad));
}

export function chunkBody(callId: number, seq: number, last: boolean, data: Uint8Array): Bytes {
  return concat(u32(callId), u32(seq), Uint8Array.of(last ? 1 : 0), data);
}

export function envelopeChunk(callId: number, seq: number, last: boolean, data: Uint8Array): Bytes {
  if (data.length > P.CHUNK) throw new Error("chunk too large");
  return concat(Uint8Array.of(P.TYPE_CHUNK), u16(0), chunkBody(callId, seq, last, data));
}

export interface Envelope {
  type: number;
  body: Bytes;
}

export function envelopeOpen(env: Uint8Array): Envelope {
  if (env.length < P.ENVELOPE_HEADER_LEN) throw new Refusal("tooShort");
  const type = env[0] ?? 0;
  const pad = readU16(env, 1);
  const rest = env.subarray(P.ENVELOPE_HEADER_LEN);
  if (pad > rest.length) throw new Refusal("badPadding");
  const body = copy(rest.subarray(0, rest.length - pad));
  if (rest.subarray(rest.length - pad).some((b) => b !== 0)) throw new Refusal("badPadding");
  if (type === P.TYPE_JSON) {
    if (env.length % P.PAD_BLOCK !== 0) throw new Refusal("badPadding");
    return { type, body };
  }
  if (type === P.TYPE_CHUNK) {
    if (pad !== 0 || body.length < P.CHUNK_HEADER_LEN) throw new Refusal("badPadding");
    return { type, body };
  }
  throw new Refusal("badEnvelope");
}

export interface Chunk {
  callId: number;
  seq: number;
  last: boolean;
  data: Bytes;
}

export function chunkParse(body: Uint8Array): Chunk {
  if (body.length < P.CHUNK_HEADER_LEN) throw new Refusal("badPadding");
  return {
    callId: readU32(body, 0),
    seq: readU32(body, 4),
    last: body[8] === 1,
    data: copy(body.subarray(P.CHUNK_HEADER_LEN)),
  };
}

/** Chunks of one call must count up from 0 with no gap. */
export function chunksInOrder(bodies: Uint8Array[]): void {
  let expected = 0;
  for (const body of bodies) {
    if (chunkParse(body).seq !== expected) throw new Refusal("chunkGap");
    expected += 1;
  }
}

// Pairing frames: H || n || GCM(Kp_dir, n, H, envelope) ----------------------------------------------

export async function sealPairFrame(
  kind: number,
  pid: Uint8Array,
  key: AesKey,
  nonce: Uint8Array,
  envelope: Uint8Array,
): Promise<Bytes> {
  const h = header(kind, pid, 0);
  return concat(h, nonce, await sealGcm(key, nonce, h, envelope));
}

export async function openPairFrame(
  frame: Uint8Array,
  kind: number,
  pid: Uint8Array,
  key: AesKey,
): Promise<Envelope> {
  const h = parseHeader(frame);
  if (frame.length < P.HEADER_LEN + P.GCM_NONCE_LEN + P.GCM_TAG_LEN) throw new Refusal("tooShort");
  if (h.version !== P.VERSION) throw new Refusal("badVersion");
  if (h.kind !== kind) throw new Refusal("wrongKind");
  if (!equal(h.sid, pid)) throw new Refusal("wrongSid");
  if (h.ctr !== 0) throw new Refusal("badCounter");
  const head = frame.subarray(0, P.HEADER_LEN);
  const nonce = frame.subarray(P.HEADER_LEN, P.HEADER_LEN + P.GCM_NONCE_LEN);
  const plain = await openGcm(key, nonce, head, frame.subarray(P.HEADER_LEN + P.GCM_NONCE_LEN));
  return envelopeOpen(plain);
}

// Sessions ----------------------------------------------------------------------------------------

export async function deviceKey(shared: Uint8Array, deviceId: Uint8Array): Promise<Bytes> {
  return hkdf(shared, deviceId, P.INFO_DEVICE, 32);
}

/** `H || deviceId || Epub || on || GCM(Kd, on, H || deviceId || Epub, "open v1")`. */
export async function sealSessionOpen(
  deviceId: Uint8Array,
  epub: Uint8Array,
  on: Uint8Array,
  kd: AesKey,
): Promise<Bytes> {
  const h = header(P.KIND_SESSION_OPEN, zeroSid(), 0);
  const aad = concat(h, deviceId, epub);
  return concat(aad, on, await sealGcm(kd, on, aad, utf8(P.PROOF_OPEN)));
}

export interface Accept {
  sid: Bytes;
  fpub: Bytes;
}

export function openSessionAccept(frame: Uint8Array): Accept {
  const h = parseHeader(frame);
  if (frame.length !== P.HEADER_LEN + P.PUBKEY_LEN) throw new Refusal("tooShort");
  if (h.version !== P.VERSION) throw new Refusal("badVersion");
  if (h.kind !== P.KIND_SESSION_ACCEPT) throw new Refusal("wrongKind");
  if (h.ctr !== 0) throw new Refusal("badCounter");
  const fpub = copy(frame.subarray(P.HEADER_LEN));
  checkPublicKey(fpub);
  return { sid: h.sid, fpub };
}

export function sealSessionAccept(sid: Uint8Array, fpub: Uint8Array): Bytes {
  return concat(header(P.KIND_SESSION_ACCEPT, sid, 0), fpub);
}

/** The phone accepts a REVOKED only as the answer to its own pending open. */
export async function openRevoked(
  frame: Uint8Array,
  kd: AesKey,
  deviceId: Uint8Array,
  pendingOpenNonce: Uint8Array,
): Promise<void> {
  const h = parseHeader(frame);
  if (frame.length !== P.HEADER_LEN + P.GCM_NONCE_LEN + P.PROOF_REVOKED.length + P.GCM_TAG_LEN) {
    throw new Refusal("tooShort");
  }
  if (h.version !== P.VERSION) throw new Refusal("badVersion");
  if (h.kind !== P.KIND_REVOKED) throw new Refusal("wrongKind");
  if (!equal(h.sid, zeroSid())) throw new Refusal("wrongSid");
  if (h.ctr !== 0) throw new Refusal("badCounter");
  const head = frame.subarray(0, P.HEADER_LEN);
  const nonce = frame.subarray(P.HEADER_LEN, P.HEADER_LEN + P.GCM_NONCE_LEN);
  const aad = concat(head, deviceId, pendingOpenNonce);
  const plain = await openGcm(kd, nonce, aad, frame.subarray(P.HEADER_LEN + P.GCM_NONCE_LEN));
  if (!equal(plain, utf8(P.PROOF_REVOKED))) throw new Refusal("badTag");
}

export async function sealRevoked(
  kd: AesKey,
  deviceId: Uint8Array,
  openNonce: Uint8Array,
  nonce: Uint8Array,
): Promise<Bytes> {
  const h = header(P.KIND_REVOKED, zeroSid(), 0);
  const aad = concat(h, deviceId, openNonce);
  return concat(h, nonce, await sealGcm(kd, nonce, aad, utf8(P.PROOF_REVOKED)));
}

export interface SessionTerms {
  deviceId: Uint8Array;
  epub: Uint8Array;
  openNonce: Uint8Array;
  sid: Uint8Array;
  fpub: Uint8Array;
  cpub: Uint8Array;
  dpub: Uint8Array;
}

/** The salt and info that bind a session's keys to its open, ids and all four public keys. */
export function sessionSalt(t: SessionTerms): Promise<Bytes> {
  return sha256(
    utf8(P.HASH_SESSION),
    t.deviceId,
    t.epub,
    t.openNonce,
    t.sid,
    t.fpub,
    t.cpub,
    t.dpub,
  );
}

export interface SessionKeys {
  up: CryptoKey;
  down: CryptoKey;
}

/** ikm = ECDH(E,F) || ECDH(E,C) || ECDH(D,F) || ECDH(D,C), 64 bytes out split up then down. */
export async function sessionKeyBytes(
  terms: SessionTerms,
  ecdhTerms: [Uint8Array, Uint8Array, Uint8Array, Uint8Array],
): Promise<{ up: Bytes; down: Bytes }> {
  const okm = await hkdf(concat(...ecdhTerms), await sessionSalt(terms), P.INFO_SESSION, 64);
  return { up: copy(okm.subarray(0, 32)), down: copy(okm.subarray(32)) };
}

/** Phone side: E and D are the phone's, C and F the computer's. */
export async function phoneSessionKeys(
  e: CryptoKey,
  d: CryptoKey,
  terms: SessionTerms,
): Promise<SessionKeys> {
  const ikm = await Promise.all([
    ecdh(e, terms.fpub),
    ecdh(e, terms.cpub),
    ecdh(d, terms.fpub),
    ecdh(d, terms.cpub),
  ]);
  const { up, down } = await sessionKeyBytes(terms, ikm as [Bytes, Bytes, Bytes, Bytes]);
  return { up: await aesKey(up, "encrypt"), down: await aesKey(down, "decrypt") };
}

export const dataNonce = (direction: "up" | "down", ctr: number): Bytes =>
  concat(utf8(direction === "up" ? P.NONCE_UP : P.NONCE_DOWN), u64(ctr));

/** `H || GCM(k, prefix || ctr, H, envelope)`. */
export async function sealDataFrame(
  key: AesKey,
  direction: "up" | "down",
  sid: Uint8Array,
  ctr: number,
  envelope: Uint8Array,
): Promise<Bytes> {
  const h = header(P.KIND_DATA, sid, ctr);
  return concat(h, await sealGcm(key, dataNonce(direction, ctr), h, envelope));
}

/**
 * Checks in the reference's order: length, version, kind, sid, counter (exactly last + 1), then
 * the tag, then the envelope. The caller moves its counter only when this returns.
 */
export async function openDataFrame(
  frame: Uint8Array,
  key: AesKey,
  direction: "up" | "down",
  sid: Uint8Array,
  lastCtr: number,
): Promise<Envelope> {
  const h = parseHeader(frame);
  if (frame.length < P.HEADER_LEN + P.GCM_TAG_LEN) throw new Refusal("tooShort");
  if (h.version !== P.VERSION) throw new Refusal("badVersion");
  if (h.kind !== P.KIND_DATA) throw new Refusal("wrongKind");
  if (!equal(h.sid, sid)) throw new Refusal("wrongSid");
  if (h.ctr !== lastCtr + 1) throw new Refusal("counterGap");
  const plain = await openGcm(
    key,
    dataNonce(direction, h.ctr),
    frame.subarray(0, P.HEADER_LEN),
    frame.subarray(P.HEADER_LEN),
  );
  return envelopeOpen(plain);
}
