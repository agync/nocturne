// @vitest-environment node
import { describe, expect, it, vi } from "vitest";
import { toHex } from "./bytes";
import { OpenError, Session } from "./channel";
import { createPhoneEngine } from "./client";
import * as C from "./crypto";
import type { PairedDevice } from "./engine";
import { ConnectionError, PhoneRequestError } from "./engine";
import { offerFromFragment, Pairing } from "./pairing";
import * as P from "./protocol";
import { delta, FakeComputer } from "./testing/fakeComputer";
import { memoryKeyStore } from "./testing/memoryStore";

async function paired(options: { computer?: FakeComputer } = {}) {
  const computer = options.computer ?? (await FakeComputer.create());
  const offer = await computer.offer();
  computer.autoDecide = "allow";
  const store = memoryKeyStore();
  const pairing = await Pairing.fromLink(offerFromFragment(new URL(offer.link).hash) ?? fail(), {
    env: computer.env,
    host: computer.host,
    origin: `https://${computer.host}`,
    store,
  });
  const result = await pairing.connect("Pixel", () => undefined);
  if (result.kind !== "allowed") throw new Error(`pairing ended ${result.kind}`);
  return { computer, store, device: result.device };
}

function fail(): never {
  throw new Error("no offer");
}

const host = (c: FakeComputer) => c.host;

async function open(
  c: FakeComputer,
  device: PairedDevice,
  extra: { preferPoll?: boolean } = {},
): Promise<Session> {
  return Session.open({ address: host(c), device, env: c.env, ...extra });
}

describe("a session", () => {
  it("opens, reads Hello and numbers frames from 1", async () => {
    const { computer, device } = await paired();
    const s = await open(computer, device);
    expect(s.hello.computerName).toBe("Home computer");
    expect(s.hello.deviceName).toBe("Pixel");
    const status = await s.request({ cmd: "getStatus" }, (a) =>
      a.type === "status" ? { done: a.status } : undefined,
    ).result;
    expect(status.needsYou).toBe(2);
    const ups = computerUp(computer);
    expect(ups.map((u) => u.ctr)).toEqual([1]);
    s.close();
  });

  it("session_up_only_after_hello_opens: a Hello that does not open ends the open", async () => {
    const { computer, device } = await paired();
    computer.mutate = (frame, ctr) => {
      if (ctr !== 1) return [frame];
      const bad = frame.slice();
      bad[bad.length - 1] = (bad[bad.length - 1] ?? 0) ^ 1;
      return [bad];
    };
    await expect(open(computer, device)).rejects.toMatchObject({ failure: "protocol" });
    const keys = memoryKeyStore(device);
    const states: string[] = [];
    const engine = createPhoneEngine(keys, () => host(computer), { env: computer.env });
    engine.subscribe((c) => states.push(c.kind));
    await engine.connect();
    expect(states).not.toContain("connected");
    expect(engine.connection().kind).toBe("cantConnect");
    expect(keys.current()).not.toBeNull();
  });

  it("replayed_frame_refused: a frame delivered twice closes the session", async () => {
    const { computer, device } = await paired();
    const s = await open(computer, device);
    const ended: string[] = [];
    s.onEnd = (why) => ended.push(why);
    computer.mutate = (frame, ctr) => (ctr === 2 ? [frame, frame] : [frame]);
    await s.request({ cmd: "getStatus" }, (a) =>
      a.type === "status" ? { done: a.status } : undefined,
    ).result;
    await vi.waitFor(() => expect(ended).toEqual(["counterGap"]));
  });

  it("reordered_frame_refused: frame 3 before frame 2 closes the session", async () => {
    const { computer, device } = await paired();
    const s = await open(computer, device);
    const ended: string[] = [];
    s.onEnd = (why) => ended.push(why);
    let held: Uint8Array<ArrayBuffer> | null = null;
    computer.mutate = (frame, ctr) => {
      if (ctr === 2) {
        held = frame;
        return [];
      }
      return held ? [frame, held] : [frame];
    };
    const first = s.request({ cmd: "getStatus" }, (a) =>
      a.type === "status" ? { done: a.status } : undefined,
    ).result;
    first.catch(() => undefined);
    await vi.waitFor(() => expect(held).not.toBeNull());
    const second = s.request({ cmd: "listPages" }, (a) =>
      a.type === "pages" ? { done: a.pages } : undefined,
    ).result;
    second.catch(() => undefined);
    await vi.waitFor(() => expect(ended).toEqual(["counterGap"]));
  });

  it("counter_gap_closes_session: a dropped frame is a gap, and nothing after it is read", async () => {
    const { computer, device } = await paired();
    const s = await open(computer, device);
    const ended: string[] = [];
    s.onEnd = (why) => ended.push(why);
    computer.mutate = (frame, ctr) => (ctr === 2 ? [] : [frame]);
    const a = s.request({ cmd: "getStatus" }, (x) =>
      x.type === "status" ? { done: x.status } : undefined,
    );
    a.result.catch(() => undefined);
    const b = s.request({ cmd: "listPages" }, (x) =>
      x.type === "pages" ? { done: x.pages } : undefined,
    );
    await expect(b.result).rejects.toThrow();
    expect(ended).toEqual(["counterGap"]);
    expect(s.isOpen).toBe(false);
  });

  it("tampered_frame_refused: a flipped bit closes the session", async () => {
    const { computer, device } = await paired();
    const s = await open(computer, device);
    const ended: string[] = [];
    s.onEnd = (why) => ended.push(why);
    computer.mutate = (frame, ctr) => {
      if (ctr !== 2) return [frame];
      const bad = frame.slice();
      bad[40] = (bad[40] ?? 0) ^ 1;
      return [bad];
    };
    const r = s.request({ cmd: "getStatus" }, (x) =>
      x.type === "status" ? { done: x.status } : undefined,
    ).result;
    await expect(r).rejects.toThrow();
    expect(ended).toEqual(["badTag"]);
  });

  it("falls_back_to_long_poll: no socket, calls and an answer's stream still work", async () => {
    const { computer, device } = await paired();
    computer.wsAvailable = false;
    const store = memoryKeyStore(device);
    const engine = createPhoneEngine(store, () => host(computer), { env: computer.env });
    const deltas: string[] = [];
    await engine.ask("When is swimming?", (d) => deltas.push(d.phase));
    expect(deltas).toEqual(["searching", "streaming", "streaming", "done"]);
    const kept = await engine.keepText("Bin day moved", "c-1");
    expect(kept.item.id).toBe("item-1");
    expect(engine.connection().kind).toBe("connected");
    engine.close();
  });

  it("cross_origin_to_current_address: frames go to the address given now, not the page's", async () => {
    const { computer, device } = await paired();
    computer.wsAvailable = false;
    const urls: string[] = [];
    const env = {
      ...computer.env,
      fetch: (input: RequestInfo | URL, init?: RequestInit) => {
        urls.push(String(input));
        expect(init?.credentials).toBe("omit");
        return computer.env.fetch(input, init);
      },
    };
    const store = memoryKeyStore({ ...device, homeOrigin: "https://old-name.trycloudflare.com" });
    const engine = createPhoneEngine(store, () => host(computer), { env });
    await engine.connect();
    expect(urls.length).toBeGreaterThan(0);
    for (const u of urls) {
      const url = new URL(u);
      expect(url.host).toBe(computer.host);
      expect(url.pathname).toBe("/s");
      expect(url.search === "" || /^\?sid=[0-9a-f]{32}$/.test(url.search)).toBe(true);
      expect(url.hash).toBe("");
    }
    expect(store.current()?.homeOrigin).toBe("https://old-name.trycloudflare.com");
    expect(store.current()?.address).toBe(computer.host);
    engine.close();
  });

  it("two_tabs_two_sessions: each tab opens its own session with its own keys", async () => {
    const { computer, device } = await paired();
    const store = memoryKeyStore(device);
    const a = createPhoneEngine(store, () => host(computer), { env: computer.env });
    const b = createPhoneEngine(store, () => host(computer), { env: computer.env });
    await Promise.all([a.status(), b.status()]);
    const live = [...computer.sessions.values()];
    expect(live).toHaveLength(2);
    expect(toHex(live[0]?.sid ?? new Uint8Array())).not.toBe(
      toHex(live[1]?.sid ?? new Uint8Array()),
    );
    expect(toHex(live[0]?.keys.up ?? new Uint8Array())).not.toBe(
      toHex(live[1]?.keys.up ?? new Uint8Array()),
    );
    a.close();
    b.close();
  });

  it("session_renews_while_used: a new session opens before the old one lapses", async () => {
    const computer = await FakeComputer.create();
    const { device } = await paired({ computer });
    const store = memoryKeyStore(device);
    const engine = createPhoneEngine(store, () => host(computer), { env: computer.env });
    await engine.status();
    expect(computer.opens).toBe(1);
    computer.now = () => Date.now() + (P.SESSION_TTL_SECS - 30) * 1000;
    await engine.status();
    expect(computer.opens).toBe(2);
    const sids = [...computer.sessions.keys()];
    expect(new Set(sids).size).toBe(2);
    await engine.status();
    expect(computer.opens).toBe(2);
    engine.close();
  });

  it("never_resends_a_frame and retry_after_new_session_by_request_id", async () => {
    const { computer, device } = await paired();
    const store = memoryKeyStore(device);
    const engine = createPhoneEngine(store, () => host(computer), { env: computer.env });
    let calls = 0;
    computer.handler = async (ctx) => {
      if (ctx.request.call.cmd === "keepText") {
        calls += 1;
        if (calls === 1) {
          computer.items.set("item-1", {
            clientId: ctx.request.call.clientId,
            text: ctx.request.call.text,
            mime: null,
            bytes: null,
          });
          computer.dropAll();
          return;
        }
      }
      await computer.defaultHandler(ctx);
    };
    const kept = await engine.keepText("Bin day moved to Thursday", "c-0001");
    expect(kept.duplicate).toBe(true);
    expect(computer.items.size).toBe(1);
    expect(computer.opens).toBe(2);
    const ups = computer.seen.up.map(toHex);
    expect(new Set(ups).size).toBe(ups.length);
    const bySession = new Map<string, number[]>();
    for (const u of computerUp(computer)) {
      const list = bySession.get(u.sid) ?? [];
      list.push(u.ctr);
      bySession.set(u.sid, list);
    }
    for (const ctrs of bySession.values()) expect(ctrs).toEqual(ctrs.map((_, i) => i + 1));
    engine.close();
  });
});

describe("revocation", () => {
  it("sealed_revoked_forgets_key: an open answered with a sealed REVOKED clears the keys", async () => {
    const { computer, device } = await paired();
    await computer.revoke(device.deviceId);
    const store = memoryKeyStore(device);
    let told = 0;
    const engine = createPhoneEngine(store, () => host(computer), {
      env: computer.env,
      onRevoked: () => {
        told += 1;
      },
    });
    await engine.connect();
    expect(engine.connection().kind).toBe("removed");
    expect(store.current()).toBeNull();
    expect(told).toBe(1);
  });

  it("a sealed Revoked in a live session also forgets the key", async () => {
    const { computer, device } = await paired();
    const store = memoryKeyStore(device);
    const engine = createPhoneEngine(store, () => host(computer), { env: computer.env });
    await engine.status();
    await computer.revoke(device.deviceId);
    await vi.waitFor(() => expect(engine.connection().kind).toBe("removed"));
    expect(store.current()).toBeNull();
  });

  it("unsealed_revoked_keeps_keys: a refusal, an HTTP error, a forged REVOKED and silence all keep the key", async () => {
    const { computer, device } = await paired();
    const store = memoryKeyStore(device);
    const engine = createPhoneEngine(store, () => host(computer), { env: computer.env });
    computer.refuseOpens = true;
    expect((await engine.connect()).kind).toBe("cantConnect");
    expect(store.current()).not.toBeNull();

    computer.refuseOpens = false;
    computer.wsAvailable = false;
    computer.httpStatus = 403;
    const poll = createPhoneEngine(store, () => host(computer), { env: computer.env });
    expect((await poll.connect()).kind).toBe("cantConnect");
    expect(store.current()).not.toBeNull();

    const forged = createPhoneEngine(store, () => host(computer), {
      env: {
        ...computer.env,
        fetch: async () => {
          const frame = C.header(P.KIND_REVOKED, C.zeroSid(), 0);
          const body = new Uint8Array([0, 0, 0, frame.length, ...frame]);
          return new Response(body, { status: 200 });
        },
      },
      preferPoll: true,
      handshakeMs: 300,
    });
    expect((await forged.connect()).kind).toBe("asleep");
    expect(store.current()).not.toBeNull();
  });

  it("revoked_for_another_open_ignored: a REVOKED sealed for a different open is not an answer", async () => {
    const { computer, device } = await paired();
    await computer.revoke(device.deviceId);
    computer.revokedForOtherOpen = true;
    const store = memoryKeyStore(device);
    const engine = createPhoneEngine(store, () => host(computer), {
      env: computer.env,
      handshakeMs: 300,
    });
    expect((await engine.connect()).kind).toBe("asleep");
    expect(store.current()).not.toBeNull();
  });

  it("classifies what the edge says without touching the keys", async () => {
    const { computer, device } = await paired();
    computer.wsAvailable = false;
    const store = memoryKeyStore(device);
    for (const [status, kind] of [
      [530, "newAddress"],
      [503, "turnedOff"],
      [403, "cantConnect"],
    ] as const) {
      computer.httpStatus = status;
      const engine = createPhoneEngine(store, () => host(computer), { env: computer.env });
      expect((await engine.connect()).kind).toBe(kind);
      expect(store.current()).not.toBeNull();
    }
    computer.httpStatus = null;
    computer.networkDown = true;
    const engine = createPhoneEngine(store, () => host(computer), { env: computer.env });
    expect((await engine.connect()).kind).toBe("asleep");
  });

  it("Most private never offers a new address", async () => {
    const { computer, device } = await paired();
    computer.wsAvailable = false;
    computer.httpStatus = 530;
    const store = memoryKeyStore({ ...device, connection: "mostPrivate" });
    const engine = createPhoneEngine(store, () => host(computer), { env: computer.env });
    expect((await engine.connect()).kind).toBe("asleep");
  });
});

describe("the engine's calls", () => {
  it("asks, streams, and cancels", async () => {
    const { computer, device } = await paired();
    const engine = createPhoneEngine(memoryKeyStore(device), () => host(computer), {
      env: computer.env,
    });
    computer.handler = async (ctx) => {
      if (ctx.request.call.cmd === "ask") {
        await ctx.reply({ type: "askStarted", askId: "ask-9" });
        await ctx.reply({ type: "askDelta", delta: delta("ask-9", "notSure", null) });
        return;
      }
      await computer.defaultHandler(ctx);
    };
    const seen: string[] = [];
    const id = await engine.ask("When?", (d) => seen.push(d.phase));
    expect(id).toBe("ask-9");
    expect(seen).toEqual(["notSure"]);
    await engine.cancelAsk(id);
    engine.close();
  });

  it("an answered error becomes a named PhoneRequestError", async () => {
    const { computer, device } = await paired();
    const engine = createPhoneEngine(memoryKeyStore(device), () => host(computer), {
      env: computer.env,
    });
    computer.handler = async (ctx) => ctx.reply({ type: "error", code: "tooMany" });
    await expect(engine.ask("Again?", () => undefined)).rejects.toMatchObject({ code: "tooMany" });
    await expect(engine.keepText("", "c-2")).rejects.toBeInstanceOf(PhoneRequestError);
    await expect(engine.keepText("x".repeat(P.MAX_TEXT + 1), "c-3")).rejects.toMatchObject({
      code: "tooLarge",
    });
    await expect(engine.ask("q".repeat(P.MAX_QUESTION + 1), () => undefined)).rejects.toMatchObject(
      { code: "invalid" },
    );
    await expect(engine.keepText("ok", "bad id")).rejects.toMatchObject({ code: "invalid" });
    engine.close();
  });

  it("uploads in chunks the computer reassembles, checking the type against the bytes", async () => {
    const { computer, device } = await paired();
    const engine = createPhoneEngine(memoryKeyStore(device), () => host(computer), {
      env: computer.env,
    });
    const jpeg = new Uint8Array(P.CHUNK * 2 + 100);
    jpeg.set([0xff, 0xd8, 0xff, 0xe0]);
    const kept = await engine.keepBytes(
      new Blob([jpeg], { type: "image/jpeg" }),
      "c-9",
      "  Leo’s form ",
    );
    expect(kept.item.title).toBe("Leo’s form");
    const stored = [...computer.items.values()].find((i) => i.clientId === "c-9");
    expect(stored?.bytes?.length).toBe(jpeg.length);
    await expect(
      engine.keepBytes(new Blob([new Uint8Array(20)], { type: "image/jpeg" }), "c-10"),
    ).rejects.toMatchObject({ code: "invalid" });
    await expect(
      engine.keepBytes(new Blob([jpeg]), "c-11", "x".repeat(P.MAX_CAPTION + 1)),
    ).rejects.toMatchObject({ code: "invalid" });
    engine.close();
  });

  it("reads an original from chunks and refuses one whose bytes are not its type", async () => {
    const { computer, device } = await paired();
    const engine = createPhoneEngine(memoryKeyStore(device), () => host(computer), {
      env: computer.env,
    });
    const png = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 1, 2, 3, 4, 5]);
    computer.handler = async (ctx) => {
      if (ctx.request.call.cmd !== "getOriginal") return computer.defaultHandler(ctx);
      const bad = ctx.request.call.id === "bad";
      await ctx.reply({
        type: "originalStart",
        mime: bad ? "image/jpeg" : "image/png",
        size: png.length,
      });
      await ctx.chunk(0, false, png.slice(0, 5));
      await ctx.chunk(1, true, png.slice(5));
    };
    const blob = await engine.original("item-1");
    expect(blob.type).toBe("image/png");
    expect(blob.size).toBe(png.length);
    await expect(engine.original("bad")).rejects.toBeInstanceOf(ConnectionError);
    engine.close();
  });

  it("disconnect tells the computer and forgets the keys", async () => {
    const { computer, device } = await paired();
    const store = memoryKeyStore(device);
    const engine = createPhoneEngine(store, () => host(computer), { env: computer.env });
    let told = false;
    computer.handler = async (ctx) => {
      told = told || ctx.request.call.cmd === "disconnect";
      await computer.defaultHandler(ctx);
    };
    await engine.status();
    await engine.disconnect();
    expect(told).toBe(true);
    expect(store.current()).toBeNull();
  });

  it("an open with nothing paired is a failure, not a crash", async () => {
    const computer = await FakeComputer.create();
    const engine = createPhoneEngine(memoryKeyStore(), () => host(computer), { env: computer.env });
    expect((await engine.connect()).kind).toBe("cantConnect");
  });
});

function computerUp(c: FakeComputer): Array<{ sid: string; ctr: number }> {
  return c.seen.up
    .filter((f) => f[1] === P.KIND_DATA)
    .map((f) => {
      const h = C.parseHeader(f);
      return { sid: toHex(h.sid), ctr: h.ctr };
    });
}

describe("opening", () => {
  it("an unknown device or a bad proof is the same unsealed refusal", async () => {
    const { computer, device } = await paired();
    const stranger = await C.generateDeviceKey();
    const other: PairedDevice = {
      ...device,
      privateKey: stranger.privateKey,
      publicKey: stranger.publicKey,
    };
    await expect(open(computer, other)).rejects.toBeInstanceOf(OpenError);
    const unknown: PairedDevice = { ...device, deviceId: C.randomBytes(16) };
    await expect(open(computer, unknown)).rejects.toMatchObject({ failure: "refused" });
  });
});
