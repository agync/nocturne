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
import { HTTP_NOT_FOUND, HTTP_OFF, HTTP_REFUSED, MIN_PHONE_BUILD, PHONE_BUILD } from "./protocol";
import { delta, FakeComputer } from "./testing/fakeComputer";
import { DAY_PERIOD, WEEK_PERIOD, YESTERDAY, YESTERDAY_PERIOD } from "./testing/journalFixtures";
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

  it("socket_lost_before_session: an opened socket that closes first falls back to long-poll, once", async () => {
    const { computer, device } = await paired();
    computer.dropSocketOpens = 1;
    const before = computer.opens;
    const s = await open(computer, device);
    expect(s.linkKind).toBe("poll");
    expect(computer.opens - before).toBe(1);
    const status = await s.request({ cmd: "getStatus" }, (a) =>
      a.type === "status" ? { done: a.status } : undefined,
    ).result;
    expect(status.needsYou).toBe(2);
    s.close();

    const engine = createPhoneEngine(memoryKeyStore(device), () => host(computer), {
      env: computer.env,
    });
    computer.dropSocketOpens = 1;
    expect((await engine.connect()).kind).toBe("connected");
  });

  it("socket_lost_before_session: the unsealed refusal is not retried", async () => {
    const { computer, device } = await paired();
    computer.refuseOpens = true;
    const before = computer.opens;
    await expect(open(computer, device)).rejects.toMatchObject({ failure: "refused" });
    expect(computer.opens - before).toBe(1);
  });

  it("a busy answer to a POST sends the same frames again", async () => {
    const { computer, device } = await paired();
    computer.busyPosts = 1;
    const before = computer.opens;
    const s = await open(computer, device, { preferPoll: true });
    expect(s.linkKind).toBe("poll");
    expect(computer.opens - before).toBe(1);
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

  it("a removal that only drops the answer is found out at once, not on the next tap", async () => {
    const { computer, device } = await paired();
    const store = memoryKeyStore(device);
    const engine = createPhoneEngine(store, () => host(computer), { env: computer.env });
    computer.handler = async (ctx) => {
      if (ctx.request.call.cmd === "ask") await ctx.reply({ type: "askStarted", askId: "ask-1" });
    };
    const asked = engine.ask("When is it?", () => undefined);
    await vi.waitFor(() => expect(computer.sessions.size).toBe(1));
    // The real computer ends the session without a word when a phone is removed; the phone
    // learns the reason from the sealed refusal to its next open.
    const known = computer.devices.get(toHex(device.deviceId));
    if (known) known.revoked = true;
    computer.dropAll();
    await expect(asked).rejects.toBeInstanceOf(ConnectionError);
    await vi.waitFor(() => expect(engine.connection().kind).toBe("removed"));
    expect(store.current()).toBeNull();
  });

  it("disconnect while a background open is in flight leaves no device record and no key", async () => {
    const { computer, device } = await paired();
    const inner = memoryKeyStore(device);
    // Reads the record at once, hands it over late: a stale copy, as an open in flight holds.
    let hold: Promise<void> | null = null;
    let release: () => void = () => undefined;
    let saves = 0;
    const store = {
      ...inner,
      async device() {
        const d = await inner.device();
        if (hold) await hold;
        return d;
      },
      async save(d: PairedDevice) {
        saves += 1;
        await inner.save(d);
      },
    };
    const engine = createPhoneEngine(store, () => host(computer), { env: computer.env });
    let arrived = false;
    computer.handler = async (ctx) => {
      if (ctx.request.call.cmd === "ask") {
        arrived = true;
        await ctx.reply({ type: "askStarted", askId: "ask-1" });
      } else await computer.defaultHandler(ctx);
    };
    const asked = engine.ask("When is it?", () => undefined);
    // Waits for the ask itself, sent only once its open has finished. A session the computer has
    // set up may still be opening; holding the record then stalls that open and the ask with it.
    await vi.waitFor(() => expect(arrived).toBe(true));
    saves = 0;
    hold = new Promise((done) => {
      release = done;
    });
    computer.dropAll();
    await expect(asked).rejects.toBeInstanceOf(ConnectionError);
    // The open that the dropped answer started is still reading the record when the person
    // disconnects; it finishes afterwards and must not bring the record back.
    const gone = engine.disconnect();
    release();
    await gone;
    await new Promise((done) => setTimeout(done, 50));
    expect(inner.current()).toBeNull();
    expect(saves).toBe(0);
    expect(engine.connection().kind).not.toBe("connected");
  });

  it("an open never saves over a record that was cleared meanwhile", async () => {
    const { computer, device } = await paired();
    const inner = memoryKeyStore(device);
    let reads = 0;
    let saves = 0;
    const store = {
      ...inner,
      async device() {
        reads += 1;
        // The record is read for the open, then found gone when the address is to be saved.
        return reads === 1 ? inner.device() : null;
      },
      async save(d: PairedDevice) {
        saves += 1;
        await inner.save(d);
      },
    };
    const engine = createPhoneEngine(store, () => host(computer), { env: computer.env });
    await expect(engine.hello()).rejects.toBeInstanceOf(ConnectionError);
    expect(saves).toBe(0);
    expect(engine.connection().kind).toBe("cantConnect");
    engine.close();
  });

  it("disconnect with no open session still reaches the computer", async () => {
    const { computer, device } = await paired();
    const store = memoryKeyStore(device);
    const engine = createPhoneEngine(store, () => host(computer), { env: computer.env });
    const told: string[] = [];
    computer.handler = async (ctx) => {
      told.push(ctx.request.call.cmd);
      await computer.defaultHandler(ctx);
    };
    expect(computer.sessions.size).toBe(0);
    expect(await engine.disconnect()).toEqual({ reached: true });
    expect(told).toEqual(["disconnect"]);
    expect(store.current()).toBeNull();
  });

  it("disconnect while the computer is unreachable forgets the phone and says it was not told", async () => {
    const { computer, device } = await paired();
    const store = memoryKeyStore(device);
    const engine = createPhoneEngine(store, () => host(computer), { env: computer.env });
    await engine.status();
    computer.networkDown = true;
    computer.dropAll();
    expect(await engine.disconnect()).toEqual({ reached: false });
    expect(store.current()).toBeNull();
  });

  it("an open racing disconnect saves nothing", async () => {
    const { computer, device } = await paired();
    const inner = memoryKeyStore(device);
    let hold: Promise<void> | null = null;
    let release: () => void = () => undefined;
    let saves = 0;
    const store = {
      ...inner,
      async device() {
        const d = await inner.device();
        if (hold) await hold;
        return d;
      },
      async save(d: PairedDevice) {
        saves += 1;
        await inner.save(d);
      },
    };
    const engine = createPhoneEngine(store, () => host(computer), { env: computer.env });
    hold = new Promise((done) => {
      release = done;
    });
    const racing = engine.connect();
    const gone = engine.disconnect();
    release();
    await racing.catch(() => undefined);
    expect(await gone).toEqual({ reached: true });
    expect(saves).toBe(0);
    expect(inner.current()).toBeNull();
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

  it("503 is off; 403 and a 4403 close are the unsealed refusal; every other status, and no network, is asleep or at a new address", async () => {
    const { computer, device } = await paired();
    computer.wsAvailable = false;
    const store = memoryKeyStore(device);
    const state = async () => {
      const engine = createPhoneEngine(store, () => host(computer), { env: computer.env });
      const kind = (await engine.connect()).kind;
      expect(store.current(), `${kind} kept the key`).not.toBeNull();
      return kind;
    };
    for (const [status, kind] of [
      [HTTP_OFF, "turnedOff"],
      [HTTP_REFUSED, "cantConnect"],
      [HTTP_NOT_FOUND, "asleep"],
      [502, "asleep"],
      [530, "asleep"],
      [500, "asleep"],
      [504, "asleep"],
      [429, "asleep"],
    ] as const) {
      computer.httpStatus = status;
      expect(await state(), String(status)).toBe(kind);
    }
    computer.httpStatus = null;
    computer.networkDown = true;
    expect(await state()).toBe("asleep");
    computer.networkDown = false;
    computer.off = true;
    expect(await state()).toBe("turnedOff");
  });

  it("a 530 or 502 never says the feature is off, for either connection", async () => {
    const { computer, device } = await paired();
    computer.wsAvailable = false;
    for (const connection of ["simple", "mostPrivate"] as const) {
      for (const status of [530, 502]) {
        computer.httpStatus = status;
        const store = memoryKeyStore({ ...device, connection });
        const engine = createPhoneEngine(store, () => host(computer), { env: computer.env });
        expect((await engine.connect()).kind).toBe("asleep");
      }
    }
  });

  it("over a WebSocket, only a 4403 close is the refusal", async () => {
    const { computer, device } = await paired();
    const store = memoryKeyStore(device);
    computer.refuseOpens = true;
    const refused = createPhoneEngine(store, () => host(computer), { env: computer.env });
    expect((await refused.connect()).kind).toBe("cantConnect");
    computer.refuseOpens = false;
    computer.mutate = null;
    computer.wsAvailable = true;
    computer.dropAll();
    const real = computer.env;
    const closesOther = createPhoneEngine(store, () => host(computer), {
      env: {
        ...real,
        WebSocket: class extends (real.WebSocket as NonNullable<typeof real.WebSocket>) {
          send() {
            queueMicrotask(() => this.onclose?.({ code: 1011 }));
          }
        },
      },
      handshakeMs: 2000,
    });
    computer.httpStatus = 502;
    expect((await closesOther.connect()).kind).toBe("asleep");
    expect(store.current()).not.toBeNull();
  });
});

describe("an old phone app", () => {
  it("a Hello that asks for a newer build stops the open, keeps the key, and saves nothing", async () => {
    const computer = await FakeComputer.create({ minPhoneBuild: PHONE_BUILD + 1 });
    const { device } = await paired({ computer });
    const before = { ...device };
    const store = memoryKeyStore(device);
    const engine = createPhoneEngine(store, () => host(computer), { env: computer.env });
    const state = await engine.connect();
    expect(state).toEqual({ kind: "outdated", minBuild: PHONE_BUILD + 1, computer: "0.1.0" });
    expect(store.current()).toEqual(before);
    await expect(engine.status()).rejects.toBeInstanceOf(ConnectionError);
    engine.close();
  });

  it("the same build, or an older ask, connects", async () => {
    for (const min of [undefined, 0, PHONE_BUILD]) {
      const computer = await FakeComputer.create({ minPhoneBuild: min });
      const { device } = await paired({ computer });
      const engine = createPhoneEngine(memoryKeyStore(device), () => host(computer), {
        env: computer.env,
      });
      expect((await engine.connect()).kind).toBe("connected");
      engine.close();
    }
  });
});

describe("the page says it is the phone page in a browser", () => {
  const app = {
    platform: "browser" as const,
    version: "0.8.0",
    minBuild: MIN_PHONE_BUILD,
    maxBuild: PHONE_BUILD,
  };

  it("introduces itself first to a computer that listens, and connects", async () => {
    const computer = await FakeComputer.create();
    const { device } = await paired({ computer });
    const engine = createPhoneEngine(memoryKeyStore(device), () => host(computer), {
      env: computer.env,
      app,
    });
    expect((await engine.connect()).kind).toBe("connected");
    expect(computer.introduced).toEqual([app]);
    engine.close();
  });

  it("says nothing to a computer from before the version window", async () => {
    const computer = await FakeComputer.create({ maxPhoneBuild: null });
    const { device } = await paired({ computer });
    const engine = createPhoneEngine(memoryKeyStore(device), () => host(computer), {
      env: computer.env,
      app,
    });
    expect((await engine.connect()).kind).toBe("connected");
    expect(computer.introduced).toEqual([]);
    engine.close();
  });

  it("with a computer from before the window, it speaks build 1 only", async () => {
    const computer = await FakeComputer.create({ maxPhoneBuild: null });
    const { device } = await paired({ computer });
    const engine = createPhoneEngine(memoryKeyStore(device), () => host(computer), {
      env: computer.env,
      app,
    });
    const state = await engine.connect();
    expect(state.kind === "connected" && state.agreed).toBe(1);
    expect((await engine.getPage(undefined)).question).toBeNull();
    await expect(
      engine.keepText("https://example.com/a-recipe", "c-web-1", { web: true }),
    ).rejects.toMatchObject({ code: "computerOutdated" });
    await expect(
      engine.answerJournalQuestion("2026-11-13", "fine", "c-answer-1", undefined),
    ).rejects.toMatchObject({ code: "computerOutdated" });
    expect(computer.items.size).toBe(0);
    await engine.keepText("Bins out on Tuesday", "c-plain-1", undefined);
    expect(computer.items.size).toBe(1);
    engine.close();
  });

  it("with a current computer, it keeps the journal question and web pages", async () => {
    const computer = await FakeComputer.create();
    const { device } = await paired({ computer });
    const engine = createPhoneEngine(memoryKeyStore(device), () => host(computer), {
      env: computer.env,
      app,
    });
    const state = await engine.connect();
    expect(state.kind === "connected" && state.agreed).toBe(PHONE_BUILD);
    expect((await engine.getPage(undefined)).question?.kind).toBe("journal");
    await engine.keepText("https://example.com/a-recipe", "c-web-2", { web: true });
    expect(computer.items.size).toBe(1);
    engine.close();
  });

  it("a page kept from before the computer updated is told to update", async () => {
    const computer = await FakeComputer.create();
    const { device } = await paired({ computer });
    const old = { ...app, minBuild: 0, maxBuild: 0 };
    const engine = createPhoneEngine(memoryKeyStore(device), () => host(computer), {
      env: computer.env,
      app: old,
    });
    expect(await engine.connect()).toEqual({
      kind: "outdated",
      minBuild: MIN_PHONE_BUILD,
      computer: "0.1.0",
    });
    engine.close();
  });
});

describe("the long-poll contract", () => {
  const poll = async () => {
    const { computer, device } = await paired();
    computer.wsAvailable = false;
    return { computer, device, store: memoryKeyStore(device) };
  };

  it("a held poll that is busy (409) waits and polls again; the session stays up", async () => {
    const { computer, device } = await poll();
    let busy = 0;
    const env = {
      ...computer.env,
      fetch: async (input: RequestInfo | URL, init?: RequestInit) => {
        if (init?.method === "GET" && busy < 2) {
          busy += 1;
          return new Response(null, { status: 409 });
        }
        return computer.env.fetch(input, init);
      },
    };
    const s = await Session.open({ address: host(computer), device, env });
    const status = await s.request({ cmd: "getStatus" }, (a) =>
      a.type === "status" ? { done: a.status } : undefined,
    ).result;
    expect(status.needsYou).toBe(2);
    expect(busy).toBe(2);
    expect(s.isOpen).toBe(true);
    s.close();
  });

  it("the computer answers a second poll for the same session with 409", async () => {
    const { computer, device } = await poll();
    const s = await Session.open({ address: host(computer), device, env: computer.env });
    const url = `https://${host(computer)}/s?sid=${toHex(s.sid)}`;
    const answers = await Promise.all([
      computer.env.fetch(url, { method: "GET" }),
      computer.env.fetch(url, { method: "GET" }),
    ]);
    expect(answers.map((a) => a.status)).toContain(409);
    s.close();
  });

  it("a session the computer ended is 410 on the held poll, and the link ends", async () => {
    const { computer, device } = await poll();
    const s = await Session.open({ address: host(computer), device, env: computer.env });
    const ended: string[] = [];
    s.onEnd = (why) => ended.push(why);
    computer.endSession(s.sid);
    await vi.waitFor(() => expect(ended).toEqual(["link http"]));
  });

  it("a request for a session the computer no longer has is 410, and the engine opens a new one and goes on", async () => {
    const { computer, device, store } = await poll();
    const engine = createPhoneEngine(store, () => host(computer), { env: computer.env });
    await engine.status();
    for (const ses of computer.sessions.values()) ses.closed = "ended";
    expect((await engine.status()).needsYou).toBe(2);
    expect(computer.opens).toBe(2);
    engine.close();
    void device;
  });

  it("an open that gets 410 is tried again once, and a second 410 is asleep", async () => {
    const { computer, store } = await poll();
    let calls = 0;
    const env = {
      ...computer.env,
      fetch: async (input: RequestInfo | URL, init?: RequestInit) => {
        if (init?.method === "POST") {
          calls += 1;
          return new Response(null, { status: 410 });
        }
        return computer.env.fetch(input, init);
      },
    };
    const engine = createPhoneEngine(store, () => host(computer), { env });
    expect((await engine.connect()).kind).toBe("asleep");
    expect(calls).toBe(2);
  });

  it("the HELLO of a pairing with no open offer is not answered (204), and a wrong frame ends nothing", async () => {
    const computer = await FakeComputer.create();
    const pid = C.randomBytes(16);
    const frame = C.helloRequest(pid);
    const body = new Uint8Array([0, 0, 0, frame.length, ...frame]);
    const res = await computer.env.fetch(`https://${host(computer)}/s`, { method: "POST", body });
    expect(res.status).toBe(204);
  });

  it("while off every keyed request is 503, and anything else is a bare 404", async () => {
    const computer = await FakeComputer.create();
    computer.off = true;
    for (const method of ["GET", "POST"]) {
      const r = await computer.env.fetch(
        `https://${host(computer)}/s${method === "GET" ? "?sid=00" : ""}`,
        { method },
      );
      expect(r.status).toBe(503);
    }
    computer.off = false;
    expect(
      (await computer.env.fetch(`https://${host(computer)}/elsewhere`, { method: "GET" })).status,
    ).toBe(404);
    expect(
      (await computer.env.fetch(`https://${host(computer)}/s?sid=nothex`, { method: "GET" }))
        .status,
    ).toBe(404);
  });

  it("a POST of data to a session that is not there is 410, an unknown session id on a GET is 410, and the refusal is 403", async () => {
    const { computer } = await poll();
    const ghost = C.header(P.KIND_DATA, C.randomBytes(16), 1);
    const frame = new Uint8Array([...ghost, ...new Uint8Array(16)]);
    const post = await computer.env.fetch(`https://${host(computer)}/s`, {
      method: "POST",
      body: new Uint8Array([0, 0, 0, frame.length, ...frame]),
    });
    expect(post.status).toBe(410);
    const get = await computer.env.fetch(`https://${host(computer)}/s?sid=${"00".repeat(16)}`, {
      method: "GET",
    });
    expect(get.status).toBe(410);
    computer.refuseOpens = true;
    const open = C.header(P.KIND_SESSION_OPEN, C.zeroSid(), 0);
    const refused = await computer.env.fetch(`https://${host(computer)}/s`, {
      method: "POST",
      body: new Uint8Array([0, 0, 0, open.length, ...open]),
    });
    expect(refused.status).toBe(403);
  });

  it("the first Hello rides back with SESSION_ACCEPT on the POST", async () => {
    const { computer, device } = await poll();
    const frames: number[] = [];
    const env = {
      ...computer.env,
      fetch: async (input: RequestInfo | URL, init?: RequestInit) => {
        const r = await computer.env.fetch(input, init);
        if (init?.method === "POST" && r.status === 200) {
          const body = new Uint8Array(await r.clone().arrayBuffer());
          let at = 0;
          let n = 0;
          while (at < body.length) {
            at += 4 + new DataView(body.buffer).getUint32(at, false);
            n += 1;
          }
          frames.push(n);
        }
        return r;
      },
    };
    const s = await Session.open({ address: host(computer), device, env });
    expect(frames).toEqual([2]);
    s.close();
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

  it("a voice note goes up as audio/wav, kept by the type and the RIFF/WAVE bytes", async () => {
    const { computer, device } = await paired();
    const engine = createPhoneEngine(memoryKeyStore(device), () => host(computer), {
      env: computer.env,
    });
    const wav = new Uint8Array(44 + 64);
    wav.set([0x52, 0x49, 0x46, 0x46, 0, 0, 0, 0, 0x57, 0x41, 0x56, 0x45]);
    await engine.keepRecording(new Blob([wav], { type: "audio/wav" }), "c-rec");
    const up = [...computer.uploads.values()].find((u) => u.clientId === "c-rec");
    expect(up?.mime).toBe("audio/wav");
    await expect(
      engine.keepRecording(
        new Blob([Uint8Array.of(0x1a, 0x45, 0xdf, 0xa3, 0, 0)], { type: "audio/wav" }),
        "c-bad",
      ),
    ).rejects.toMatchObject({ code: "invalid" });
    engine.close();
  });

  describe("the journal", () => {
    const glad = {
      part: "litUp" as const,
      point: { pleasant: 0.6, energy: 0.5 },
      words: ["glad"],
    };

    async function engineOn(computer: FakeComputer, device: PairedDevice) {
      return createPhoneEngine(memoryKeyStore(device), () => host(computer), {
        env: computer.env,
      });
    }

    it("a keep carries its feeling and its good-thing mark, and a plain keep neither", async () => {
      const { computer, device } = await paired();
      const engine = await engineOn(computer, device);
      await engine.keepText("Tea with Priya", "c-f1", { feeling: glad, good: true });
      await engine.keepText("Plain", "c-f2");
      const text = (id: string) => [...computer.items.values()].find((i) => i.clientId === id);
      expect(text("c-f1")).toMatchObject({ feeling: glad, good: true });
      expect(text("c-f2")).toMatchObject({ feeling: null, good: null });
      const jpeg = new Uint8Array(100);
      jpeg.set([0xff, 0xd8, 0xff, 0xe0]);
      await engine.keepBytes(new Blob([jpeg], { type: "image/jpeg" }), "c-f3", "Sun", {
        feeling: glad,
        good: true,
      });
      const wav = new Uint8Array(44 + 8);
      wav.set([0x52, 0x49, 0x46, 0x46, 0, 0, 0, 0, 0x57, 0x41, 0x56, 0x45]);
      await engine.keepRecording(new Blob([wav], { type: "audio/wav" }), "c-f4", {
        feeling: glad,
      });
      expect(text("c-f3")).toMatchObject({ feeling: glad, good: true });
      expect(text("c-f4")).toMatchObject({ feeling: glad, good: null });
      const sent = computer.seen.up.length;
      expect(sent).toBeGreaterThan(0);
      engine.close();
    });

    it("a keep says when it was made, and a check-in lands on that day", async () => {
      const { computer, device } = await paired();
      const engine = await engineOn(computer, device);
      const made = "2026-11-18T23:50:00.000Z";
      await engine.keepText("Late note", "c-t1", { capturedAt: made });
      await engine.keepText("Now note", "c-t2");
      const text = (id: string) => [...computer.items.values()].find((i) => i.clientId === id);
      expect(text("c-t1")).toMatchObject({ capturedAt: made });
      expect(text("c-t2")).toMatchObject({ capturedAt: null });
      await engine.keepText("https://example.org", "c-w1", { web: true });
      expect(text("c-w1")).toMatchObject({ web: true });
      expect(text("c-t2")).toMatchObject({ web: false });
      const jpeg = new Uint8Array(100);
      jpeg.set([0xff, 0xd8, 0xff, 0xe0]);
      await engine.keepBytes(new Blob([jpeg], { type: "image/jpeg" }), "c-t3", "Moon", {
        capturedAt: made,
      });
      expect(text("c-t3")).toMatchObject({ capturedAt: made });
      const input = { feeling: glad, line: null, urge: null, replaces: null };
      expect((await engine.keepCheckIn(input, "c-t4", made)).day).toBe("2026-11-18");
      expect((await engine.keepCheckIn(input, "c-t5")).day).toBe("2026-11-19");
      engine.close();
    });

    it("a feeling off the field, or a good thing that is not a photo, is not sent", async () => {
      const { computer, device } = await paired();
      const engine = await engineOn(computer, device);
      const off = { ...glad, point: { pleasant: 1.5, energy: 0 } };
      const many = { ...glad, words: ["a", "b", "c", "d"] };
      await expect(engine.keepText("x", "c-b1", { feeling: off })).rejects.toMatchObject({
        code: "invalid",
      });
      await expect(engine.keepText("x", "c-b2", { feeling: many })).rejects.toMatchObject({
        code: "invalid",
      });
      const wav = new Uint8Array(44 + 8);
      wav.set([0x52, 0x49, 0x46, 0x46, 0, 0, 0, 0, 0x57, 0x41, 0x56, 0x45]);
      await expect(
        engine.keepRecording(new Blob([wav], { type: "audio/wav" }), "c-b3", { good: true }),
      ).rejects.toMatchObject({ code: "invalid" });
      expect(computer.items.size).toBe(0);
      engine.close();
    });

    it("a check-in is kept once however often it is sent, and is never a change", async () => {
      const { computer, device } = await paired();
      const engine = await engineOn(computer, device);
      const input = { feeling: glad, line: "A good walk", urge: null, replaces: null };
      const first = await engine.keepCheckIn(input, "c-ci");
      const again = await engine.keepCheckIn({ ...input, line: "Again" }, "c-ci");
      expect(again.id).toBe(first.id);
      expect(first.line).toBe("A good walk");
      await expect(
        engine.keepCheckIn({ ...input, replaces: first.id }, "c-ci2"),
      ).rejects.toMatchObject({ code: "invalid" });
      await expect(
        engine.keepCheckIn({ ...input, feeling: { ...glad, words: ["", "b"] } }, "c-ci3"),
      ).rejects.toMatchObject({ code: "invalid" });
      engine.close();
    });

    it("reads today, another day and a day that has not come", async () => {
      const { computer, device } = await paired();
      const engine = await engineOn(computer, device);
      const today = await engine.getJournalDay();
      expect(today.today).toBe(true);
      expect(today.checkins.length).toBeGreaterThan(0);
      expect(today.sameName).toHaveLength(1);
      const yesterday = await engine.getJournalDay(YESTERDAY);
      expect(yesterday.reflection?.firstSentence).toContain("Leo");
      await expect(engine.getJournalDay("2026-12-31")).rejects.toMatchObject({ code: "invalid" });
      await expect(engine.getJournalDay("soon")).rejects.toMatchObject({ code: "invalid" });
      engine.close();
    });

    it("reads a day's reflection, a week's recap and a day not yet looked back on", async () => {
      const { computer, device } = await paired();
      const engine = await engineOn(computer, device);
      expect((await engine.getLookBack(YESTERDAY_PERIOD)).type).toBe("written");
      expect((await engine.getLookBack(WEEK_PERIOD)).type).toBe("recap");
      expect((await engine.getLookBack(DAY_PERIOD)).type).toBe("notYet");
      const written = await engine.lookBack(DAY_PERIOD);
      expect(written.type).toBe("written");
      await expect(
        engine.getLookBack({ kind: "day", start: "yesterday", end: "yesterday" }),
      ).rejects.toMatchObject({ code: "invalid" });
      engine.close();
    });

    it("the page’s question from the journal is answered once however often it is sent", async () => {
      const { computer, device } = await paired();
      const engine = await engineOn(computer, device);
      expect((await engine.getPage()).question?.kind).toBe("journal");
      const first = await engine.answerJournalQuestion("2026-11-13", "notGreat", "c-q");
      const again = await engine.answerJournalQuestion("2026-11-13", "notGreat", "c-q");
      expect(first.question?.answered).toBe("notGreat");
      expect(again.question?.answered).toBe("notGreat");
      const lined = await engine.answerJournalQuestion("2026-11-13", "notGreat", "c-q", " Long. ");
      expect(lined.question?.journal?.line).toBe("Long.");
      expect(computer.answer.kept).toBe(1);
      for (const [date, choice, id] of [
        ["2026-11-13", "yes", "c-q2"],
        ["soon", "fine", "c-q3"],
        ["2026-11-13", "fine", "bad id"],
      ] as const) {
        await expect(engine.answerJournalQuestion(date, choice, id)).rejects.toMatchObject({
          code: "invalid",
        });
      }
      engine.close();
    });

    it("answers which Sam, and the answer names who it was filed under", async () => {
      const { computer, device } = await paired();
      const engine = await engineOn(computer, device);
      const q = (await engine.getJournalDay()).sameName[0];
      const choice = q?.options[1];
      expect(q && choice).toBeTruthy();
      const answered = await engine.answerSameName(q?.id ?? "", {
        type: "entity",
        entityId: choice?.entityId ?? "",
      });
      expect(answered).toEqual({ name: "Sam", role: "from work" });
      expect(await engine.answerSameName(q?.id ?? "", { type: "leave" })).toEqual({
        name: "Sam",
        role: null,
      });
      await expect(engine.answerSameName("bad id", { type: "new" })).rejects.toMatchObject({
        code: "invalid",
      });
      engine.close();
    });

    it("online answers' notice: the question waits, is kept on the computer, or is cleared", async () => {
      const { computer, device } = await paired();
      const engine = await engineOn(computer, device);
      const calls: unknown[] = [];
      let seen = false;
      computer.handler = async (ctx) => {
        const call = ctx.request.call;
        calls.push(call);
        if (call.cmd === "ask" && !seen && call.place !== "thisComputer") {
          return ctx.reply({ type: "error", code: "onlineChanged" });
        }
        if (call.cmd === "seeOnlineChange") seen = true;
        return computer.defaultHandler(ctx);
      };
      await expect(engine.ask("Swimming?", () => undefined)).rejects.toMatchObject({
        code: "onlineChanged",
      });
      await engine.ask("Swimming?", () => undefined, "thisComputer");
      await engine.seeOnlineChange();
      await engine.ask("Swimming?", () => undefined);
      expect(calls).toEqual([
        { cmd: "ask", question: "Swimming?" },
        { cmd: "ask", question: "Swimming?", place: "thisComputer" },
        { cmd: "seeOnlineChange" },
        { cmd: "ask", question: "Swimming?" },
      ]);
      engine.close();
    });
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

  describe("an original from the computer", () => {
    const png = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 1, 2, 3, 4, 5]);
    type Send = (ctx: import("./testing/fakeComputer").CallContext) => Promise<void>;

    async function fetchWith(send: Send) {
      const { computer, device } = await paired();
      const engine = createPhoneEngine(memoryKeyStore(device), () => host(computer), {
        env: computer.env,
      });
      computer.handler = async (ctx) => {
        if (ctx.request.call.cmd !== "getOriginal") return computer.defaultHandler(ctx);
        await send(ctx);
      };
      return { engine, computer };
    }

    const refused = async (send: Send) => {
      const { engine } = await fetchWith(send);
      await expect(engine.original("item-1")).rejects.toBeInstanceOf(ConnectionError);
      engine.close();
    };

    it("a second originalStart is refused, whatever it says", async () => {
      await refused(async (ctx) => {
        await ctx.reply({ type: "originalStart", mime: "image/png", size: png.length });
        await ctx.reply({ type: "originalStart", mime: "image/jpeg", size: png.length });
        await ctx.chunk(0, true, png);
      });
    });

    it("an originalStart after a chunk is refused", async () => {
      await refused(async (ctx) => {
        await ctx.reply({ type: "originalStart", mime: "image/png", size: png.length });
        await ctx.chunk(0, false, png.slice(0, 5));
        await ctx.reply({ type: "originalStart", mime: "image/jpeg", size: png.length });
        await ctx.chunk(1, true, png.slice(5));
      });
    });

    it("a type that can run script is refused at the start", async () => {
      for (const mime of ["text/html", "image/svg+xml", "application/xml", "text/xml"]) {
        await refused(async (ctx) => {
          await ctx.reply({ type: "originalStart", mime, size: 4 });
          await ctx.chunk(0, true, new Uint8Array([0x3c, 0x73, 0x76, 0x67]));
        });
      }
    });

    it("a size over the limit, or not a size, is refused at the start", async () => {
      for (const size of [P.MAX_ITEM + 1, 0, -1, 1.5]) {
        await refused(async (ctx) => {
          await ctx.reply({ type: "originalStart", mime: "image/png", size });
          await ctx.chunk(0, true, png);
        });
      }
    });

    it("more bytes than declared, or fewer, are refused", async () => {
      await refused(async (ctx) => {
        await ctx.reply({ type: "originalStart", mime: "image/png", size: 5 });
        await ctx.chunk(0, false, png.slice(0, 4));
        await ctx.chunk(1, true, png.slice(4));
      });
      await refused(async (ctx) => {
        await ctx.reply({ type: "originalStart", mime: "image/png", size: png.length + 3 });
        await ctx.chunk(0, true, png);
      });
    });

    it("chunks with no start are refused, and one start with its bytes is accepted", async () => {
      await refused(async (ctx) => ctx.chunk(0, true, png));
      const { engine } = await fetchWith(async (ctx) => {
        await ctx.reply({ type: "originalStart", mime: "image/png", size: png.length });
        await ctx.chunk(0, true, png);
      });
      const blob = await engine.original("item-1");
      expect(blob.type).toBe("image/png");
      engine.close();
    });
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
