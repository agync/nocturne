// Byte helpers for the sealed channel. Every buffer is an ArrayBuffer-backed `Bytes`, which is
// what WebCrypto accepts without a copy.

export type Bytes = Uint8Array<ArrayBuffer>;

const encoder = new TextEncoder();
const decoder = new TextDecoder("utf-8", { fatal: false });

export function bytes(length: number): Bytes {
  return new Uint8Array(new ArrayBuffer(length));
}

export function copy(source: ArrayLike<number>): Bytes {
  const out = bytes(source.length);
  out.set(source);
  return out;
}

export function utf8(text: string): Bytes {
  return copy(encoder.encode(text));
}

export function fromUtf8(data: Uint8Array): string {
  return decoder.decode(data);
}

export function concat(...parts: Uint8Array[]): Bytes {
  const out = bytes(parts.reduce((n, part) => n + part.length, 0));
  let at = 0;
  for (const part of parts) {
    out.set(part, at);
    at += part.length;
  }
  return out;
}

/** Equal in time that does not depend on where the bytes first differ. */
export function equal(a: Uint8Array, b: Uint8Array): boolean {
  let diff = a.length ^ b.length;
  const n = Math.min(a.length, b.length);
  for (let i = 0; i < n; i += 1) diff |= (a[i] ?? 0) ^ (b[i] ?? 0);
  return diff === 0;
}

export function toHex(data: Uint8Array): string {
  let out = "";
  for (const byte of data) out += byte.toString(16).padStart(2, "0");
  return out;
}

export function fromHex(text: string): Bytes {
  if (text.length % 2 !== 0 || /[^0-9a-f]/i.test(text)) throw new Error("not hex");
  const out = bytes(text.length / 2);
  for (let i = 0; i < out.length; i += 1)
    out[i] = Number.parseInt(text.slice(2 * i, 2 * i + 2), 16);
  return out;
}

export function toBase64Url(data: Uint8Array): string {
  let binary = "";
  for (const byte of data) binary += String.fromCharCode(byte);
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

export function fromBase64Url(text: string): Bytes {
  if (!/^[A-Za-z0-9_-]*$/.test(text) || text.length % 4 === 1) throw new Error("not base64url");
  const padded =
    text.replace(/-/g, "+").replace(/_/g, "/") + "=".repeat((4 - (text.length % 4)) % 4);
  const binary = atob(padded);
  const out = bytes(binary.length);
  for (let i = 0; i < binary.length; i += 1) out[i] = binary.charCodeAt(i);
  return out;
}

export function u16(value: number): Bytes {
  const out = bytes(2);
  new DataView(out.buffer).setUint16(0, value, false);
  return out;
}

export function u32(value: number): Bytes {
  const out = bytes(4);
  new DataView(out.buffer).setUint32(0, value, false);
  return out;
}

/** An 8-byte big-endian counter. Counters stay far below 2^53. */
export function u64(value: number): Bytes {
  const out = bytes(8);
  const view = new DataView(out.buffer);
  view.setUint32(0, Math.floor(value / 0x1_0000_0000), false);
  view.setUint32(4, value >>> 0, false);
  return out;
}

export function readU64(data: Uint8Array, at = 0): number {
  const view = new DataView(data.buffer, data.byteOffset, data.byteLength);
  return view.getUint32(at, false) * 0x1_0000_0000 + view.getUint32(at + 4, false);
}

export function readU32(data: Uint8Array, at = 0): number {
  return new DataView(data.buffer, data.byteOffset, data.byteLength).getUint32(at, false);
}

export function readU16(data: Uint8Array, at = 0): number {
  return new DataView(data.buffer, data.byteOffset, data.byteLength).getUint16(at, false);
}
