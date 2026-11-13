// @vitest-environment node
import { describe, expect, it } from "vitest";
import { concat, fromHex, toHex } from "./bytes";
import * as C from "./crypto";
import { offerFromFragment, Pairing, PairingError } from "./pairing";
import * as P from "./protocol";
import { FakeComputer } from "./testing/fakeComputer";
import { memoryKeyStore } from "./testing/memoryStore";

async function setup(options: { host?: string } = {}) {
  const computer = await FakeComputer.create(options);
  const offer = await computer.offer();
  const store = memoryKeyStore();
  const ctx = {
    env: computer.env,
    host: computer.host,
    origin: `https://${computer.host}`,
    store,
  };
  const scanned = async () =>
    Pairing.fromLink(offerFromFragment(new URL(offer.link).hash) ?? fail(), ctx);
  return { computer, offer, store, ctx, scanned };
}

function fail(): never {
  throw new Error("no offer");
}

const pairFrames = (c: FakeComputer) =>
  c.seen.up.filter((f) => f[1] === P.KIND_PAIR_COMMIT || f[1] === P.KIND_PAIR_REVEAL);

describe("pairing", () => {
  it("pair.name_then_words_then_allowed: the words match the computer's, and Allow saves the key", async () => {
    const { computer, offer, store, scanned } = await setup();
    const pairing = await scanned();
    let words: string[] = [];
    const running = pairing.connect("Pixel", (w) => {
      words = w;
    });
    await expect.poll(() => offer.words).not.toBeNull();
    expect(words).toHaveLength(3);
    expect(words).toEqual(offer.words);
    for (const w of words) expect(P.WORDS).toContain(w);
    expect(store.current()).toBeNull();
    await computer.decide(offer, true);
    const result = await running;
    expect(result.kind).toBe("allowed");
    expect(store.current()?.deviceName).toBe("Pixel");
    expect(store.current()?.homeOrigin).toBe(`https://${computer.host}`);
    expect(toHex(store.current()?.deviceId ?? new Uint8Array())).toBe(
      [...computer.devices.keys()][0],
    );
  });

  it("the saved key cannot be exported", async () => {
    const { computer, offer, store, scanned } = await setup();
    computer.autoDecide = "allow";
    const result = await (await scanned()).connect("Pixel", () => undefined);
    expect(result.kind).toBe("allowed");
    const key = store.current()?.privateKey as CryptoKey;
    expect(key.extractable).toBe(false);
    await expect(crypto.subtle.exportKey("pkcs8", key)).rejects.toThrow();
    await expect(crypto.subtle.exportKey("jwk", key)).rejects.toThrow();
    expect(offer.words).not.toBeNull();
  });

  it("pair.refused_shows_start_again: a refusal leaves no key", async () => {
    const { computer, offer, store, scanned } = await setup();
    const running = (await scanned()).connect("Pixel", () => undefined);
    await expect.poll(() => offer.words).not.toBeNull();
    await computer.decide(offer, false);
    expect((await running).kind).toBe("refused");
    expect(store.current()).toBeNull();
  });

  it("pair.expired_code: a code past its five minutes gets no answer", async () => {
    const { computer, offer, ctx } = await setup();
    computer.now = () => Date.now() + (P.PAIR_TTL_SECS + 1) * 1000;
    const typed = C.codeEncode(offer.s);
    await expect(Pairing.fromCode(typed, { ...ctx, answerWaitMs: 200 })).rejects.toMatchObject({
      kind: "expired",
    });
    const late = await Pairing.fromLink(offerFromFragment(new URL(offer.link).hash) ?? fail(), {
      ...ctx,
      answerWaitMs: 200,
    });
    expect((await late.connect("Pixel", () => undefined)).kind).toBe("expired");
  });

  it("pair.spent_code: connecting twice on one page load sends nothing the second time", async () => {
    const { computer, offer, scanned } = await setup();
    const pairing = await scanned();
    const first = pairing.connect("Pixel", () => undefined);
    await expect.poll(() => offer.words).not.toBeNull();
    const before = pairFrames(computer).length;
    expect(pairing.spentAlready).toBe(true);
    expect(await pairing.connect("Pixel", () => undefined)).toEqual({ kind: "spent" });
    expect(pairFrames(computer).length).toBe(before);
    await computer.decide(offer, false);
    await first;
  });

  it("commit_never_retried: a transport error after the commit ends the pairing", async () => {
    const { computer, offer, scanned } = await setup();
    const pairing = await scanned();
    const running = pairing.connect("Pixel", () => undefined);
    await expect.poll(() => offer.words).not.toBeNull();
    computer.dropAll();
    expect((await running).kind).toBe("spent");
    expect(computer.seen.up.filter((f) => f[1] === P.KIND_PAIR_COMMIT)).toHaveLength(1);
    expect(await pairing.connect("Pixel", () => undefined)).toEqual({ kind: "spent" });
    expect(computer.seen.up.filter((f) => f[1] === P.KIND_PAIR_COMMIT)).toHaveLength(1);
  });

  it("reload_never_repeats_a_nonce: two page loads on one code never send the same nonce", async () => {
    const { computer, scanned } = await setup();
    const a = await scanned();
    const b = await scanned();
    const ra = a.connect("Pixel", () => undefined);
    const rb = b.connect("Pixel", () => undefined);
    await Promise.all(
      [ra, rb].map((r) => Promise.race([r, new Promise((x) => setTimeout(x, 300))])),
    );
    computer.dropAll();
    const nonces = pairFrames(computer).map((f) =>
      toHex(f.subarray(P.HEADER_LEN, P.HEADER_LEN + 12)),
    );
    expect(nonces.length).toBeGreaterThanOrEqual(2);
    expect(new Set(nonces).size).toBe(nonces.length);
    const commits = computer.seen.up.filter((f) => f[1] === P.KIND_PAIR_COMMIT);
    expect(new Set(commits.map(toHex)).size).toBe(commits.length);
  });

  it("accepts_one_nonce: a second PAIR_NONCE changes nothing", async () => {
    const { computer, offer, scanned } = await setup();
    const pairing = await scanned();
    const shown: string[][] = [];
    const running = pairing.connect("Pixel", (w) => shown.push(w));
    await expect.poll(() => offer.words).not.toBeNull();
    const second = await C.sealPairFrame(
      P.KIND_PAIR_NONCE,
      offer.pid,
      offer.keys.down,
      C.randomBytes(12),
      C.envelopeJson(new TextEncoder().encode(JSON.stringify({ nc: "A".repeat(43) }))),
    );
    (offer.sink as { send(f: Uint8Array): void }).send(second as never);
    await computer.decide(offer, false);
    await running;
    expect(shown).toHaveLength(1);
  });

  it("a junk or forged PAIR_NONCE, or a junk outcome, is ignored and the pairing goes on", async () => {
    const { computer, offer, store, ctx } = await setup();
    const junk = (kind: number) => concat(C.header(kind, offer.pid, 0), C.randomBytes(12 + 60));
    const env = {
      ...computer.env,
      fetch: async (input: RequestInfo | URL, init?: RequestInit) => {
        const real = await computer.env.fetch(input, init);
        if (init?.method !== "POST" || real.status !== 200) return real;
        // A forged nonce arrives ahead of the real one.
        const forged = junk(P.KIND_PAIR_NONCE);
        const body = new Uint8Array(await real.arrayBuffer());
        const prefix = new Uint8Array([0, 0, 0, forged.length, ...forged]);
        return new Response(new Uint8Array([...prefix, ...body]), { status: 200 });
      },
    };
    const pairing = await Pairing.fromLink(offerFromFragment(new URL(offer.link).hash) ?? fail(), {
      ...ctx,
      env,
      preferPoll: true,
    });
    const shown: string[][] = [];
    const running = pairing.connect("Pixel", (w) => shown.push(w));
    await expect.poll(() => offer.words).not.toBeNull();
    expect(shown).toHaveLength(1);
    // A junk outcome and a second junk nonce do nothing either.
    const sink = offer.sink as { send(f: Uint8Array): void };
    sink.send(junk(P.KIND_PAIR_OUTCOME) as never);
    sink.send(junk(P.KIND_PAIR_NONCE) as never);
    await computer.decide(offer, true);
    expect((await running).kind).toBe("allowed");
    expect(store.current()).not.toBeNull();
  });

  it("pair.typed_code_path: the code, typed, makes the same pairing", async () => {
    const { computer, offer, store, ctx } = await setup();
    const typed = C.codeDisplay(C.codeEncode(offer.s)).toLowerCase().replace(/ /g, " - ");
    const pairing = await Pairing.fromCode(typed, ctx);
    const running = pairing.connect("Pixel", () => undefined);
    await expect.poll(() => offer.words).not.toBeNull();
    expect(offer.commit?.path).toBe(P.PATH_TYPED);
    await computer.decide(offer, true);
    expect((await running).kind).toBe("allowed");
    expect(store.current()).not.toBeNull();
  });

  it("pair.typed_code_normalised: look-alikes and spaces are the same code", async () => {
    const { offer, ctx } = await setup();
    const code = C.codeEncode(offer.s);
    const mangled = code.replace(/0/g, "o").replace(/1/g, "l");
    const display = `  ${mangled.slice(0, 6)}-${mangled.slice(6, 12)} ${mangled.slice(12)}  `;
    const pairing = await Pairing.fromCode(display, ctx);
    expect(pairing).toBeInstanceOf(Pairing);
    await expect(Pairing.fromCode(`${code.slice(0, 23)}U`, ctx)).rejects.toMatchObject({
      kind: "badCode",
    });
    await expect(Pairing.fromCode("short", ctx)).rejects.toBeInstanceOf(PairingError);
  });

  it("a relay that answers HELLO with its own key cannot complete a typed pairing", async () => {
    const { offer, ctx, computer } = await setup();
    const relay = await C.generateDeviceKey();
    let hellos = 0;
    const env = {
      ...computer.env,
      fetch: async (input: RequestInfo | URL, init?: RequestInit) => {
        const real = await computer.env.fetch(input, init);
        if (init?.method !== "POST" || hellos > 0) return real;
        hellos += 1;
        const body = new Uint8Array(await real.arrayBuffer());
        const frame = body.slice(4, 4 + P.HEADER_LEN + P.PUBKEY_LEN);
        frame.set(relay.publicKey, P.HEADER_LEN);
        return new Response(new Uint8Array([0, 0, 0, frame.length, ...frame]), { status: 200 });
      },
    };
    const pairing = await Pairing.fromCode(C.codeEncode(offer.s), {
      ...ctx,
      env,
      preferPoll: true,
    });
    const result = await pairing.connect("Pixel", () => undefined);
    expect(result.kind).toBe("expired");
    expect(offer.words).toBeNull();
    expect(computer.refusals).toContain("pairing");
  });

  it("pair.device_name_capped: forty bytes pass, forty-one are refused, and the field is trimmed to fit", async () => {
    const { scanned } = await setup();
    const pairing = await scanned();
    await expect(pairing.connect("x".repeat(41), () => undefined)).rejects.toMatchObject({
      code: "badDeviceName",
    });
    expect(pairing.spentAlready).toBe(false);
    expect(C.cleanDeviceName("é".repeat(30))).toBe("é".repeat(20));
    expect(C.cleanDeviceName("Pixel‮ 9 ")).toBe("Pixel 9");
    expect(fromHex("00")).toHaveLength(1);
  });

  it("look_alike_host_changes_words: a phone that sees another host shows other words", async () => {
    const { computer, offer, ctx } = await setup();
    const shown: string[][] = [];
    const lookAlike = await Pairing.fromLink(
      { s: offer.s, cpub: computer.cpub },
      { ...ctx, host: `${computer.host.replace(".com", ".co")}` },
    );
    const running = lookAlike.connect("Pixel", (w) => shown.push(w));
    await expect.poll(() => offer.words).not.toBeNull();
    await expect.poll(() => shown.length).toBe(1);
    expect(shown[0]).not.toEqual(offer.words);
    await computer.decide(offer, false);
    await running;
  });
});
