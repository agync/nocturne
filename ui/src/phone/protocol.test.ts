import { createHash, webcrypto } from "node:crypto";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import * as protocol from "./protocol";
import { bytes, hex, loadReference, positive, toHex } from "./testing/vectors";

const subtle = webcrypto.subtle;
const ref = loadReference();
const ascii = (s: string) => new TextEncoder().encode(s);

function concat(...parts: Uint8Array[]): Uint8Array<ArrayBuffer> {
  const out = new Uint8Array(new ArrayBuffer(parts.reduce((n, p) => n + p.length, 0)));
  let at = 0;
  for (const p of parts) {
    out.set(p, at);
    at += p.length;
  }
  return out;
}

const b64url = (b: Uint8Array) => Buffer.from(b).toString("base64url");

async function hkdf(
  ikm: Uint8Array<ArrayBuffer>,
  salt: Uint8Array<ArrayBuffer>,
  info: string,
  len: number,
) {
  const key = await subtle.importKey("raw", ikm, "HKDF", false, ["deriveBits"]);
  const bits = await subtle.deriveBits(
    { name: "HKDF", hash: "SHA-256", salt, info: ascii(info) },
    key,
    len * 8,
  );
  return new Uint8Array(bits);
}

describe("phone protocol", () => {
  it("matches the reference constants, name for name", () => {
    const exported = Object.keys(protocol).filter((k) => k !== "WORDS");
    expect(exported.sort()).toEqual(Object.keys(ref.constants).sort());
    for (const [name, value] of Object.entries(ref.constants)) {
      expect([name, (protocol as Record<string, unknown>)[name]]).toEqual([name, value]);
    }
  });

  it("carries the reference word list", () => {
    expect(protocol.WORDS).toHaveLength(ref.words.count);
    expect(new Set(protocol.WORDS).size).toBe(2048);
    const text = `${protocol.WORDS.join("\n")}\n`;
    expect(createHash("sha256").update(text).digest("hex")).toBe(ref.words.sha256);
    const shell = readFileSync(resolve(process.cwd(), "../crates/channel/src/words.txt"), "utf8");
    expect(shell).toBe(text);
  });

  it("names a known refusal on every negative vector", () => {
    for (const v of ref.negative) expect(ref.refusals).toContain(v.expect);
    const names = [...ref.positive, ...ref.negative].map((v) => v.name);
    expect(new Set(names).size).toBe(names.length);
  });

  // The channel itself is tested against every vector by the phone lane; this shows WebCrypto
  // reproduces the reference's primitives.
  it("WebCrypto reproduces the reference primitives", async () => {
    const z = positive(ref, "ecdh_leading_zero");
    const a = z.inputs.a as { scalar: string; public: string };
    const b = z.inputs.b as { public: string };
    const aPub = hex(a.public);
    const priv = await subtle.importKey(
      "jwk",
      {
        kty: "EC",
        crv: "P-256",
        d: b64url(hex(a.scalar)),
        x: b64url(aPub.slice(1, 33)),
        y: b64url(aPub.slice(33)),
      },
      { name: "ECDH", namedCurve: "P-256" },
      false,
      ["deriveBits"],
    );
    const peer = await subtle.importKey(
      "raw",
      hex(b.public),
      { name: "ECDH", namedCurve: "P-256" },
      false,
      [],
    );
    const shared = new Uint8Array(
      await subtle.deriveBits({ name: "ECDH", public: peer }, priv, 256),
    );
    expect(shared).toHaveLength(protocol.ECDH_OUT_LEN);
    expect(shared[0]).toBe(0);
    expect(toHex(shared)).toBe(z.output.shared);

    const p = positive(ref, "pairing_id");
    const pid = await hkdf(bytes(p.inputs, "s"), new Uint8Array(), protocol.INFO_PAIR_ID, 16);
    expect(toHex(pid)).toBe(p.output.pairingId);

    const k = positive(ref, "pair_keys");
    const salt = new Uint8Array(
      await subtle.digest(
        "SHA-256",
        concat(ascii(protocol.HASH_QR), bytes(k.inputs, "cpub"), bytes(k.inputs, "pairingId")),
      ),
    );
    expect(toHex(salt)).toBe(k.intermediate.salt);
    const okm = await hkdf(bytes(k.inputs, "s"), salt, protocol.INFO_PAIR, 64);
    expect(toHex(okm.slice(0, 32))).toBe(k.output.up);
    expect(toHex(okm.slice(32))).toBe(k.output.down);

    const d = positive(ref, "data_down_1");
    const key = await subtle.importKey("raw", bytes(d.inputs, "key"), "AES-GCM", false, [
      "encrypt",
    ]);
    const header = bytes(d.intermediate, "header");
    const nonce = concat(ascii(protocol.NONCE_DOWN), header.slice(18));
    expect(toHex(nonce)).toBe(d.intermediate.nonce);
    const sealed = await subtle.encrypt(
      { name: "AES-GCM", iv: nonce, additionalData: header },
      key,
      bytes(d.intermediate, "envelope"),
    );
    expect(toHex(concat(header, new Uint8Array(sealed)))).toBe(d.output.frame);
  });
});
