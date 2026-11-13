// @vitest-environment node
import { describe, expect, it } from "vitest";
import { type Bytes, fromBase64Url, fromHex, toBase64Url, toHex, utf8 } from "./bytes";
import * as C from "./crypto";
import * as P from "./protocol";
import { verifyOpen } from "./testing/fakeComputer";
import { testKey } from "./testing/keys";
import {
  loadReference,
  negative,
  positive,
  step,
  type Vector,
  bytes as vbytes,
  hex as vhex,
} from "./testing/vectors";

const ref = loadReference();
const str = (f: Record<string, unknown>, k: string) => f[k] as string;

async function refusal(fn: () => unknown): Promise<string> {
  try {
    await fn();
  } catch (e) {
    if (e instanceof C.Refusal) return e.code;
    throw e;
  }
  return "accepted";
}

const sids = (v: Vector) => vbytes(v.inputs, "sid");

describe("vectors_match_reference: codes, ids and names", () => {
  it("encodes and decodes S", () => {
    for (const v of step(ref.positive, "code")) {
      const s = vbytes(v.inputs, "s");
      expect(C.codeEncode(s), v.name).toBe(str(v.output, "code"));
      expect(C.codeDisplay(C.codeEncode(s)), v.name).toBe(str(v.output, "display"));
      expect(toHex(C.codeDecode(str(v.output, "code"))), v.name).toBe(str(v.output, "s"));
    }
  });

  it("normalises what a person types", () => {
    for (const v of step(ref.positive, "codeNormalise")) {
      const code = C.codeNormalise(str(v.inputs, "typed"));
      expect(code, v.name).toBe(str(v.output, "code"));
      expect(toHex(C.codeDecode(code)), v.name).toBe(str(v.output, "s"));
    }
  });

  it("refuses every bad code", async () => {
    for (const v of step(ref.negative, "codeNormalise")) {
      expect(await refusal(() => C.codeNormalise(str(v.inputs, "typed"))), v.name).toBe(v.expect);
    }
  });

  it("names: the 40-byte name is accepted, the rest are refused", async () => {
    for (const v of step(ref.positive, "deviceName")) {
      expect(toHex(C.checkDeviceName(str(v.inputs, "deviceName"))), v.name).toBe(
        str(v.output, "utf8"),
      );
    }
    for (const v of step(ref.negative, "deviceName")) {
      expect(await refusal(() => C.checkDeviceName(str(v.inputs, "deviceName"))), v.name).toBe(
        v.expect,
      );
    }
  });

  it("ids and magic bytes", async () => {
    for (const v of step(ref.positive, "id")) {
      expect(await refusal(() => C.checkId(str(v.inputs, "id"))), v.name).toBe("accepted");
    }
    for (const v of step(ref.negative, "id")) {
      expect(await refusal(() => C.checkId(str(v.inputs, "id"))), v.name).toBe(v.expect);
    }
    for (const v of step(ref.positive, "magic")) {
      expect(C.magicMatches(str(v.inputs, "mime"), vbytes(v.inputs, "bytes")), v.name).toBe(true);
    }
    for (const v of step(ref.negative, "magic")) {
      expect(
        await refusal(() => C.checkMagic(str(v.inputs, "mime"), vbytes(v.inputs, "bytes"))),
        v.name,
      ).toBe(v.expect);
    }
  });

  it("sniffs the type of a file whose own type is empty", () => {
    expect(C.sniffMime(vhex("ffd8ffe000104a464946"))).toBe("image/jpeg");
    expect(C.sniffMime(vhex("3c21646f63747970652068746d6c3e"))).toBeNull();
  });
});

describe("vectors_match_reference: public keys", () => {
  it("refuses every bad public key before any import", async () => {
    for (const v of step(ref.negative, "publicKey")) {
      expect(await refusal(() => C.checkPublicKey(vbytes(v.inputs, "key"))), v.name).toBe(v.expect);
      expect(await refusal(() => C.importPublicKey(vbytes(v.inputs, "key"))), v.name).toBe(
        v.expect,
      );
    }
  });

  it("public_key_checks_before_import: the check runs without WebCrypto", () => {
    const original = globalThis.crypto.subtle.importKey;
    let imports = 0;
    globalThis.crypto.subtle.importKey = ((...args: Parameters<typeof original>) => {
      imports += 1;
      return original.apply(globalThis.crypto.subtle, args);
    }) as typeof original;
    try {
      for (const v of step(ref.negative, "publicKey")) {
        expect(() => C.checkPublicKey(vbytes(v.inputs, "key"))).toThrow();
      }
      expect(imports).toBe(0);
    } finally {
      globalThis.crypto.subtle.importKey = original;
    }
  });

  it("accepts every public key in the fixture", () => {
    for (const k of Object.values(ref.keys)) C.checkPublicKey(vhex(k.public));
  });

  it("derives the shared secret with its leading zero kept", async () => {
    const v = positive(ref, "ecdh_leading_zero");
    const a = v.inputs.a as { scalar: string; public: string };
    const b = v.inputs.b as { public: string };
    const key = await testKey(a.scalar, a.public);
    const shared = await C.ecdh(key.privateKey, vhex(b.public));
    expect(shared).toHaveLength(P.ECDH_OUT_LEN);
    expect(shared[0]).toBe(0);
    expect(toHex(shared)).toBe(str(v.output, "shared"));
    const kd = await C.deviceKey(shared, vbytes(v.inputs, "deviceId"));
    expect(toHex(kd)).toBe(str(v.intermediate, "kd"));
  });
});

describe("vectors_match_reference: pairing", () => {
  it("pairing id, link and keys", async () => {
    const id = positive(ref, "pairing_id");
    const pid = await C.pairingId(vbytes(id.inputs, "s"));
    expect(toHex(pid)).toBe(str(id.output, "pairingId"));

    const link = positive(ref, "pairing_link");
    const s = vbytes(link.inputs, "s");
    const cpub = vbytes(link.inputs, "cpub");
    const text = C.pairingLink(str(link.inputs, "host"), s, cpub);
    expect(text).toBe(str(link.output, "link"));
    const parsed = C.parsePairingFragment(new URL(text).hash);
    expect(parsed && toHex(parsed.s)).toBe(toHex(s));
    expect(parsed && toHex(parsed.cpub)).toBe(toHex(cpub));

    const k = positive(ref, "pair_keys");
    const keys = await C.pairKeys(s, cpub, pid);
    expect(toHex(keys.salt)).toBe(str(k.intermediate, "salt"));
    expect(toHex(keys.up)).toBe(str(k.output, "up"));
    expect(toHex(keys.down)).toBe(str(k.output, "down"));
  });

  it("a fragment that is not a pairing link gives nothing", () => {
    expect(C.parsePairingFragment("")).toBeNull();
    expect(C.parsePairingFragment("#v2.abc.def")).toBeNull();
    expect(C.parsePairingFragment("#v1.AAAA.BBBB")).toBeNull();
    const link = positive(ref, "pairing_link");
    const bad = `#v1.${toBase64Url(vhex("00"))}.${toBase64Url(vbytes(link.inputs, "cpub"))}`;
    expect(C.parsePairingFragment(bad)).toBeNull();
  });

  it("the unsealed HELLO and its answer", () => {
    const v = positive(ref, "hello");
    const pid = vbytes(v.inputs, "pairingId");
    expect(toHex(C.helloRequest(pid))).toBe(str(v.intermediate, "request"));
    const cpub = C.openHelloAnswer(vhex(str(v.output, "answer")), pid);
    expect(toHex(cpub)).toBe(str(v.inputs, "cpub"));
  });

  it("refuses a HELLO answer for another offer, or with a bad key", async () => {
    const v = positive(ref, "hello");
    const answer = vhex(str(v.output, "answer"));
    const other = vhex("11".repeat(16));
    expect(await refusal(() => C.openHelloAnswer(answer, other))).toBe("wrongSid");
    const bad = answer.slice();
    bad[bad.length - 1] = (bad[bad.length - 1] ?? 0) ^ 1;
    expect(await refusal(() => C.openHelloAnswer(bad, vbytes(v.inputs, "pairingId")))).toBe(
      "badPublicKey",
    );
    expect(await refusal(() => C.openHelloAnswer(answer.slice(0, 40), other))).toBe("tooShort");
  });

  it("commitment", async () => {
    const v = positive(ref, "commitment");
    const name = vbytes(v.inputs, "deviceNameUtf8");
    expect(toHex(C.checkDeviceName(str(v.inputs, "deviceName")))).toBe(toHex(name));
    const commit = await C.commitment(vbytes(v.inputs, "dpub"), vbytes(v.inputs, "np"), name);
    expect(toHex(commit)).toBe(str(v.output, "commit"));
  });

  it("every pairing frame, sealed and opened, with its random nonce fixed", async () => {
    for (const v of step(ref.positive, "pairFrame")) {
      const key = vbytes(v.inputs, "key");
      const pid = vbytes(v.inputs, "pairingId");
      const kind = v.inputs.kind as number;
      const env = vhex(str(v.intermediate, "envelope"));
      const frame = await C.sealPairFrame(kind, pid, key, vbytes(v.inputs, "nonce"), env);
      expect(toHex(frame), v.name).toBe(str(v.output, "frame"));
      const opened = await C.openPairFrame(frame, kind, pid, key);
      expect(new TextDecoder().decode(opened.body), v.name).toBe(str(v.inputs, "body"));
    }
  });

  it("refuses every bad pairing frame with its named refusal", async () => {
    for (const v of step(ref.negative, "pairFrame")) {
      const got = await refusal(() =>
        C.openPairFrame(
          vbytes(v.inputs, "frame"),
          v.inputs.kind as number,
          vbytes(v.inputs, "pairingId"),
          vbytes(v.inputs, "key"),
        ),
      );
      expect(got, v.name).toBe(v.expect);
    }
  });

  it("reveal_matches_commit: the computer's checks, positive and negative", async () => {
    const c = positive(ref, "commitment");
    const words = positive(ref, "words_scanned");
    await C.checkReveal(
      vbytes(words.inputs, "cpub"),
      vbytes(c.inputs, "dpub"),
      vbytes(c.inputs, "np"),
      vbytes(c.inputs, "deviceNameUtf8"),
      vhex(str(c.output, "commit")),
    );
    for (const v of step(ref.negative, "reveal")) {
      const got = await refusal(() =>
        C.checkReveal(
          vbytes(v.inputs, "cpub"),
          vbytes(v.inputs, "dpub"),
          vbytes(v.inputs, "np"),
          C.checkDeviceName(str(v.inputs, "deviceName")),
          vbytes(v.inputs, "commit"),
        ),
      );
      expect(got, v.name).toBe(v.expect);
    }
  });

  it("words_from_list: both paths, a look-alike host, and a transcript with bit 32 set", async () => {
    const d = await testKey(ref.keys.D?.scalar ?? "", ref.keys.D?.public ?? "");
    for (const v of step(ref.positive, "words")) {
      const input: C.WordsInput = {
        path: v.inputs.path as number,
        pairingId: vbytes(v.inputs, "pairingId"),
        host: str(v.inputs, "host"),
        cpub: vbytes(v.inputs, "cpub"),
        dpub: vbytes(v.inputs, "dpub"),
        nameRaw: utf8(str(v.inputs, "deviceName")),
        np: vbytes(v.inputs, "np"),
        nc: vbytes(v.inputs, "nc"),
      };
      expect(toHex(await C.wordsSalt(input)), v.name).toBe(str(v.intermediate, "salt"));
      const words = await C.checkWords(d.privateKey, input);
      expect(words, v.name).toEqual(v.output.words);
      for (const w of words) expect(P.WORDS).toContain(w);
    }
    const typed = positive(ref, "words_typed").output.words;
    const look = positive(ref, "words_look_alike_host").output.words;
    expect(look).not.toEqual(typed);
  });
});

describe("vectors_match_reference: sessions", () => {
  it("device key, session open and accept", async () => {
    const dk = positive(ref, "device_key");
    const d = await testKey(ref.keys.D?.scalar ?? "", ref.keys.D?.public ?? "");
    const shared = await C.ecdh(d.privateKey, vbytes(dk.inputs, "cpub"));
    expect(toHex(shared)).toBe(str(dk.intermediate, "ecdhDC"));
    const kd = await C.deviceKey(shared, vbytes(dk.inputs, "deviceId"));
    expect(toHex(kd)).toBe(str(dk.output, "kd"));

    const so = positive(ref, "session_open");
    const frame = await C.sealSessionOpen(
      vbytes(so.inputs, "deviceId"),
      vbytes(so.inputs, "epub"),
      vbytes(so.inputs, "openNonce"),
      vbytes(so.inputs, "kd"),
    );
    expect(toHex(frame)).toBe(str(so.output, "frame"));

    const sa = positive(ref, "session_accept");
    expect(toHex(C.sealSessionAccept(vbytes(sa.inputs, "sid"), vbytes(sa.inputs, "fpub")))).toBe(
      str(sa.output, "frame"),
    );
    const accept = C.openSessionAccept(vhex(str(sa.output, "frame")));
    expect(toHex(accept.sid)).toBe(str(sa.inputs, "sid"));
    expect(toHex(accept.fpub)).toBe(str(sa.inputs, "fpub"));
  });

  it("session keys: four ECDH terms, salt and both halves", async () => {
    const v = positive(ref, "session_keys");
    const e = await testKey(ref.keys.E?.scalar ?? "", ref.keys.E?.public ?? "");
    const d = await testKey(ref.keys.D?.scalar ?? "", ref.keys.D?.public ?? "");
    const terms: C.SessionTerms = {
      deviceId: vbytes(v.inputs, "deviceId"),
      epub: vbytes(v.inputs, "epub"),
      openNonce: vbytes(v.inputs, "openNonce"),
      sid: vbytes(v.inputs, "sid"),
      fpub: vbytes(v.inputs, "fpub"),
      cpub: vbytes(v.inputs, "cpub"),
      dpub: vbytes(v.inputs, "dpub"),
    };
    expect(toHex(await C.ecdh(e.privateKey, terms.fpub))).toBe(str(v.intermediate, "ecdhEF"));
    expect(toHex(await C.ecdh(e.privateKey, terms.cpub))).toBe(str(v.intermediate, "ecdhEC"));
    expect(toHex(await C.ecdh(d.privateKey, terms.fpub))).toBe(str(v.intermediate, "ecdhDF"));
    expect(toHex(await C.ecdh(d.privateKey, terms.cpub))).toBe(str(v.intermediate, "ecdhDC"));
    expect(toHex(await C.sessionSalt(terms))).toBe(str(v.intermediate, "salt"));
    const ikm = (["ecdhEF", "ecdhEC", "ecdhDF", "ecdhDC"] as const).map((k) =>
      vbytes(v.intermediate, k),
    ) as [Bytes, Bytes, Bytes, Bytes];
    const keys = await C.sessionKeyBytes(terms, ikm);
    expect(toHex(keys.up)).toBe(str(v.output, "up"));
    expect(toHex(keys.down)).toBe(str(v.output, "down"));
    const imported = await C.phoneSessionKeys(e.privateKey, d.privateKey, terms);
    expect(imported.up.algorithm.name).toBe("AES-GCM");
  });

  it("revoked: sealed by the computer, opened only for the phone's own open", async () => {
    const v = positive(ref, "revoked");
    const frame = await C.sealRevoked(
      vbytes(v.inputs, "kd"),
      vbytes(v.inputs, "deviceId"),
      vbytes(v.inputs, "openNonce"),
      vbytes(v.inputs, "nonce"),
    );
    expect(toHex(frame)).toBe(str(v.output, "frame"));
    await C.openRevoked(
      frame,
      vbytes(v.inputs, "kd"),
      vbytes(v.inputs, "deviceId"),
      vbytes(v.inputs, "openNonce"),
    );
    for (const n of step(ref.negative, "revoked")) {
      const got = await refusal(() =>
        C.openRevoked(
          vbytes(n.inputs, "frame"),
          vbytes(n.inputs, "kd"),
          vbytes(n.inputs, "deviceId"),
          vbytes(n.inputs, "pendingOpenNonce"),
        ),
      );
      expect(got, n.name).toBe(n.expect);
    }
  });
});

describe("vectors_match_reference: the computer's check of a session open", () => {
  it("verifies the reference open, and refuses every bad one with the same refusal for an unknown device", async () => {
    const c = await testKey(ref.keys.C?.scalar ?? "", ref.keys.C?.public ?? "");
    const so = positive(ref, "session_open");
    const d = positive(ref, "device_key");
    const good = await verifyOpen(
      vhex(str(so.output, "frame")),
      new Map([[str(so.inputs, "deviceId"), vbytes(d.inputs, "dpub")]]),
      c.privateKey,
    );
    expect(toHex(good.epub)).toBe(str(so.inputs, "epub"));
    expect(toHex(good.on)).toBe(str(so.inputs, "openNonce"));
    for (const v of step(ref.negative, "sessionOpen")) {
      const registry = new Map(
        Object.entries(v.inputs.registry as Record<string, string>).map(
          ([id, key]) => [id, vhex(key)] as const,
        ),
      );
      const got = await refusal(() =>
        verifyOpen(vbytes(v.inputs, "frame"), registry, c.privateKey),
      );
      expect(got, v.name).toBe(v.expect);
    }
  });
});

describe("vectors_match_reference: envelopes and data frames", () => {
  it("envelopes pad to blocks of 256, and chunks are not padded", () => {
    for (const v of step(ref.positive, "envelope")) {
      const env =
        v.inputs.type === P.TYPE_JSON
          ? C.envelopeJson(utf8(str(v.inputs, "body")))
          : C.envelopeChunk(
              v.inputs.callId as number,
              v.inputs.seq as number,
              v.inputs.last as boolean,
              vbytes(v.inputs, "bytes"),
            );
      expect(toHex(env), v.name).toBe(str(v.output, "envelope"));
      expect(env.length, v.name).toBe(v.output.length);
      if (v.inputs.type === P.TYPE_JSON) expect(env.length % P.PAD_BLOCK).toBe(0);
      C.envelopeOpen(env);
    }
  });

  it("envelope_padding_rules: padding beyond the body, non-zero, short blocks and unknown types", async () => {
    for (const v of step(ref.negative, "envelope")) {
      expect(await refusal(() => C.envelopeOpen(vbytes(v.inputs, "envelope"))), v.name).toBe(
        v.expect,
      );
    }
  });

  it("a chunk sequence gap is refused", async () => {
    for (const v of step(ref.negative, "chunks")) {
      const bodies = (v.inputs.bodies as string[]).map((b) => vhex(b));
      expect(await refusal(() => C.chunksInOrder(bodies)), v.name).toBe(v.expect);
    }
    const ok = [C.chunkBody(1, 0, false, utf8("a")), C.chunkBody(1, 1, true, utf8("b"))];
    C.chunksInOrder(ok);
  });

  it("DATA frames in both directions at counters 1 and 2, and a chunk at 3", async () => {
    for (const v of step(ref.positive, "data")) {
      const dir = v.inputs.direction as "up" | "down";
      const env = v.intermediate.envelope
        ? vhex(str(v.intermediate, "envelope"))
        : vhex(str(v.inputs, "envelope"));
      const ctr = v.inputs.ctr as number;
      const frame = await C.sealDataFrame(vbytes(v.inputs, "key"), dir, sids(v), ctr, env);
      expect(toHex(frame), v.name).toBe(str(v.output, "frame"));
      expect(v.intermediate.nonce ? toHex(C.dataNonce(dir, ctr)) : "", v.name).toBe(
        v.intermediate.nonce ? str(v.intermediate, "nonce") : "",
      );
      const opened = await C.openDataFrame(frame, vbytes(v.inputs, "key"), dir, sids(v), ctr - 1);
      if (v.inputs.body) expect(new TextDecoder().decode(opened.body)).toBe(str(v.inputs, "body"));
      else expect(opened.type).toBe(P.TYPE_CHUNK);
    }
  });

  it("refuses every bad DATA frame with its named refusal", async () => {
    for (const v of step(ref.negative, "data")) {
      const got = await refusal(() =>
        C.openDataFrame(
          vbytes(v.inputs, "frame"),
          vbytes(v.inputs, "key"),
          v.inputs.direction as "up" | "down",
          sids(v),
          v.inputs.lastCtr as number,
        ),
      );
      expect(got, v.name).toBe(v.expect);
    }
  });

  it("every negative vector in the fixture is covered by a step this file runs", () => {
    const covered = new Set([
      "publicKey",
      "reveal",
      "codeNormalise",
      "deviceName",
      "data",
      "pairFrame",
      "revoked",
      "envelope",
      "chunks",
      "magic",
      "id",
      "sessionOpen",
    ]);
    for (const v of ref.negative) expect(covered.has(v.step), v.name).toBe(true);
    expect(negative(ref, "open_unknown_device").expect).toBe("badProof");
  });
});

describe("the base64url helpers agree with the fixture's links", () => {
  it("round trips", () => {
    const v = positive(ref, "pairing_link");
    expect(toBase64Url(fromBase64Url(str(v.intermediate, "cpubBase64url")))).toBe(
      str(v.intermediate, "cpubBase64url"),
    );
    expect(fromHex(toHex(vhex("00ff10")))).toEqual(vhex("00ff10"));
  });
});
