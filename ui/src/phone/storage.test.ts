// @vitest-environment node
import { describe, expect, it } from "vitest";
import { copy, fromHex } from "./bytes";
import { generateDeviceKey } from "./crypto";
import type { PairedDevice, PhoneEngine } from "./engine";
import { ConnectionError, PhoneRequestError } from "./engine";
import { DEVICE_FIELDS, idbKeyStore, readDevice, requestPersistence } from "./keys";
import { idbQueue, memoryQueue, withKeepLock } from "./queue";
import { missingFeatures } from "./support";
import { fakeIndexedDb } from "./testing/fakeIdb";

async function device(): Promise<PairedDevice> {
  const key = await generateDeviceKey();
  const cpub = (await generateDeviceKey()).publicKey;
  return {
    deviceId: fromHex("00112233445566778899aabbccddeeff"),
    privateKey: key.privateKey,
    publicKey: key.publicKey,
    cpub,
    homeOrigin: "https://amber-otter-lamp-quiet.trycloudflare.com",
    address: "amber-otter-lamp-quiet.trycloudflare.com",
    deviceName: "Pixel",
    computerName: null,
    connection: null,
    version: null,
    pairedAt: 1,
    lastConnectedAt: null,
  };
}

describe("the key store", () => {
  it("keys.private_key_not_extractable: what is stored cannot be read out", async () => {
    const d = await device();
    expect(d.privateKey.extractable).toBe(false);
    await expect(crypto.subtle.exportKey("pkcs8", d.privateKey)).rejects.toThrow();
    const store = idbKeyStore(fakeIndexedDb());
    await store.save(d);
    const back = await store.device();
    expect(back?.privateKey.extractable).toBe(false);
    await expect(crypto.subtle.exportKey("jwk", back?.privateKey as CryptoKey)).rejects.toThrow();
  });

  it("keys.survive_reload: a new store over the same database finds the device", async () => {
    const factory = fakeIndexedDb();
    const d = await device();
    await idbKeyStore(factory).save(d);
    const reopened = await idbKeyStore(factory).device();
    expect(reopened?.deviceName).toBe("Pixel");
    expect(reopened?.deviceId).toEqual(d.deviceId);
    expect(reopened?.cpub).toEqual(d.cpub);
  });

  it("keys.no_session_material_stored: only the device's own fields are written", async () => {
    const factory = fakeIndexedDb();
    const d = { ...(await device()), sessionKey: copy(new Uint8Array(32)), counter: 7, sid: "x" };
    const store = idbKeyStore(factory);
    await store.save(d as PairedDevice);
    const db = await new Promise<{ stores: Map<string, { rows: Map<string, unknown> }> }>(
      (resolve) => {
        const r = factory.open("nocturne-phone", 1) as unknown as {
          onsuccess: () => void;
          result: never;
        };
        r.onsuccess = () => resolve(r.result);
      },
    );
    const record = db.stores.get("device")?.rows.get("current") as Record<string, unknown>;
    expect(Object.keys(record).sort()).toEqual([...DEVICE_FIELDS].sort());
  });

  it("clears the device and reads nothing back", async () => {
    const store = idbKeyStore(fakeIndexedDb());
    await store.save(await device());
    await store.clear();
    expect(await store.device()).toBeNull();
  });

  it("refuses a record that is not a device", () => {
    expect(readDevice(null)).toBeNull();
    expect(readDevice({ deviceId: new Uint8Array(3) })).toBeNull();
    expect(readDevice("x")).toBeNull();
  });

  it("asks for persistence where offered, and says no where not", async () => {
    let asked = 0;
    expect(
      await requestPersistence({
        persisted: async () => false,
        persist: async () => {
          asked += 1;
          return true;
        },
      }),
    ).toBe(true);
    expect(asked).toBe(1);
    expect(await requestPersistence(undefined)).toBe(false);
    expect(
      await requestPersistence({
        persisted: async () => {
          throw new Error("no");
        },
        persist: async () => true,
      }),
    ).toBe(false);
  });
});

describe("browser support", () => {
  const good = {
    isSecureContext: true,
    crypto: { subtle: { generateKey() {} }, getRandomValues() {} },
    indexedDB: {},
    fetch() {},
    TextEncoder: class {},
    Blob: class {},
    URL: class {},
  };

  it("support.old_browser_state: any missing piece is named, and a full set is fine", () => {
    expect(missingFeatures(good)).toEqual([]);
    expect(missingFeatures({ ...good, isSecureContext: false })).toEqual(["secure context"]);
    expect(missingFeatures({ ...good, crypto: undefined })).toEqual([
      "WebCrypto",
      "random numbers",
    ]);
    expect(missingFeatures({ ...good, indexedDB: undefined })).toEqual(["storage"]);
    expect(missingFeatures({})).toHaveLength(8);
  });
});

function engineThatSends(failOn: string | null, how: "connection" | "invalid" = "connection") {
  const sent: string[] = [];
  const reply = (id: string) => ({
    item: {
      id,
      kind: "text",
      title: id,
      keptAt: "",
      durationMs: null,
      pages: null,
      hasOriginal: false,
      summary: null,
      text: null,
      truncated: false,
    },
    duplicate: false,
    takeBackUntil: "",
  });
  const engine = {
    async keepText(text: string, clientId: string) {
      if (clientId === failOn) {
        throw how === "connection"
          ? new ConnectionError("asleep")
          : new PhoneRequestError("invalid");
      }
      sent.push(`${clientId}:${text}`);
      return reply(clientId);
    },
    async keepBytes(_b: Blob, clientId: string) {
      sent.push(`${clientId}:bytes`);
      return reply(clientId);
    },
    async keepRecording(_b: Blob, clientId: string) {
      sent.push(`${clientId}:recording`);
      return reply(clientId);
    },
  } as unknown as PhoneEngine;
  return { engine, sent };
}

describe("the waiting queue", () => {
  const keep = (id: string, at: number) => ({
    clientId: id,
    kind: "text" as const,
    text: `note ${id}`,
    size: 6,
    createdAt: at,
  });

  it("queue.flush_in_order_once: each keep goes in the order made, and only once", async () => {
    for (const queue of [memoryQueue(), idbQueue(fakeIndexedDb())]) {
      await queue.add(keep("c-2", 20));
      await queue.add({
        clientId: "c-3",
        kind: "photo",
        blob: new Blob(["x"]),
        size: 1,
        createdAt: 30,
      });
      await queue.add(keep("c-1", 10));
      await queue.add({
        clientId: "c-4",
        kind: "recording",
        blob: new Blob(["y"]),
        size: 1,
        createdAt: 40,
      });
      expect(await queue.count()).toBe(4);
      const { engine, sent } = engineThatSends(null);
      const result = await queue.flush(engine);
      expect(sent).toEqual(["c-1:note c-1", "c-2:note c-2", "c-3:bytes", "c-4:recording"]);
      expect(result).toMatchObject({ sent: 4, failed: 0, remaining: 0 });
      expect(await queue.count()).toBe(0);
      await queue.flush(engine);
      expect(sent).toHaveLength(4);
    }
  });

  it("a connection failure leaves that keep and the rest waiting, in order", async () => {
    const queue = memoryQueue();
    for (const [i, id] of ["c-1", "c-2", "c-3"].entries()) await queue.add(keep(id, i));
    const { engine, sent } = engineThatSends("c-2");
    const result = await queue.flush(engine);
    expect(sent).toEqual(["c-1:note c-1"]);
    expect(result).toMatchObject({ sent: 1, remaining: 2 });
    expect((await queue.list()).map((k) => k.clientId)).toEqual(["c-2", "c-3"]);
  });

  it("one the computer will never take stays visible and does not block the rest", async () => {
    const queue = memoryQueue();
    for (const [i, id] of ["c-1", "c-2", "c-3"].entries()) await queue.add(keep(id, i));
    const { engine, sent } = engineThatSends("c-2", "invalid");
    const result = await queue.flush(engine);
    expect(sent).toEqual(["c-1:note c-1", "c-3:note c-3"]);
    expect(result).toMatchObject({ sent: 2, failed: 1, remaining: 1 });
  });

  const answer = (id: string, at: number, line: string | null = null) => ({
    clientId: id,
    kind: "answer" as const,
    answer: { pageDate: "2026-11-13", choice: "notGreat" as const, line },
    size: 0,
    createdAt: at,
  });

  it("an answer whose question is gone is let go, and the keeps after it still go", async () => {
    for (const code of ["notFound", "invalid", "tooLate"] as const) {
      const queue = memoryQueue();
      await queue.add(answer("c-a", 1));
      await queue.add(keep("c-2", 2));
      const sent: string[] = [];
      const engine = {
        answerJournalQuestion: async () => {
          throw new PhoneRequestError(code);
        },
        keepText: async (_text: string, id: string) => {
          sent.push(id);
          return {} as never;
        },
      } as unknown as PhoneEngine;
      const result = await queue.flush(engine);
      expect(sent, code).toEqual(["c-2"]);
      expect(result, code).toMatchObject({ sent: 1, remaining: 0 });
    }
  });

  it("a line added while its answer is going across stays to go after it", async () => {
    const queue = memoryQueue();
    await queue.add(answer("c-a", 1));
    const lines: Array<string | undefined> = [];
    const engine = {
      answerJournalQuestion: async (_d: string, _c: string, _id: string, line?: string) => {
        lines.push(line);
        if (lines.length === 1) await queue.add(answer("c-a", 1, "Long queue."));
        return {} as never;
      },
    } as unknown as PhoneEngine;
    await queue.flush(engine);
    expect(await queue.count()).toBe(1);
    await queue.flush(engine);
    expect(lines).toEqual([undefined, "Long queue."]);
    expect(await queue.count()).toBe(0);
  });

  it("clear removes everything waiting", async () => {
    const queue = idbQueue(fakeIndexedDb());
    await queue.add(keep("c-1", 1));
    await queue.clear();
    expect(await queue.count()).toBe(0);
  });
});

describe("a keep waiting with its feeling", () => {
  const glad = { part: "litUp" as const, point: { pleasant: 0.6, energy: 0.5 }, words: ["glad"] };

  function recorder() {
    const calls: Array<{
      what: string;
      clientId: string;
      marks?: unknown;
      input?: unknown;
      capturedAt?: string;
    }> = [];
    const kept = (clientId: string) => ({
      item: {
        id: `item-${clientId}`,
        kind: "text",
        title: "",
        keptAt: "",
        durationMs: null,
        pages: null,
        hasOriginal: false,
        summary: null,
        text: null,
        truncated: false,
      },
      duplicate: false,
      takeBackUntil: "",
    });
    const engine = {
      async keepText(_t: string, clientId: string, marks?: unknown) {
        calls.push({ what: "text", clientId, marks });
        return kept(clientId);
      },
      async keepBytes(_b: Blob, clientId: string, _c?: string, marks?: unknown) {
        calls.push({ what: "photo", clientId, marks });
        return kept(clientId);
      },
      async keepRecording(_b: Blob, clientId: string, marks?: unknown) {
        calls.push({ what: "recording", clientId, marks });
        return kept(clientId);
      },
      async keepCheckIn(input: unknown, clientId: string, capturedAt?: string) {
        calls.push({ what: "checkin", clientId, input, capturedAt });
        return { id: `checkin-${clientId}` };
      },
    } as unknown as PhoneEngine;
    return { engine, calls };
  }

  it("keeps its feeling and its good-thing mark until it goes, whichever queue holds it", async () => {
    for (const queue of [memoryQueue(), idbQueue(fakeIndexedDb())]) {
      await queue.add({
        clientId: "c-1",
        kind: "text",
        text: "Tea",
        feeling: glad,
        good: true,
        size: 3,
        createdAt: 1,
      });
      await queue.add({
        clientId: "c-2",
        kind: "photo",
        blob: new Blob(["x"]),
        feeling: glad,
        size: 1,
        createdAt: 2,
      });
      await queue.add({
        clientId: "c-3",
        kind: "recording",
        blob: new Blob(["y"]),
        feeling: glad,
        size: 1,
        createdAt: 3,
      });
      const waiting = await queue.list();
      expect(waiting.map((k) => k.feeling)).toEqual([glad, glad, glad]);
      const { engine, calls } = recorder();
      await queue.flush(engine);
      const at = (n: number) => new Date(n).toISOString();
      expect(calls).toEqual([
        { what: "text", clientId: "c-1", marks: { feeling: glad, good: true, capturedAt: at(1) } },
        {
          what: "photo",
          clientId: "c-2",
          marks: { feeling: glad, good: undefined, capturedAt: at(2) },
        },
        {
          what: "recording",
          clientId: "c-3",
          marks: { feeling: glad, good: undefined, capturedAt: at(3) },
        },
      ]);
    }
  });

  it("holds a check-in kept while asleep, and sends it once with its client id", async () => {
    for (const queue of [memoryQueue(), idbQueue(fakeIndexedDb())]) {
      const input = { feeling: glad, line: "A walk", urge: null, replaces: null };
      await queue.add({
        clientId: "c-ci",
        kind: "checkin",
        checkIn: input,
        size: 6,
        createdAt: 1,
      });
      expect(await queue.count()).toBe(1);
      const { engine, calls } = recorder();
      const result = await queue.flush(engine);
      expect(calls).toEqual([
        { what: "checkin", clientId: "c-ci", input, capturedAt: new Date(1).toISOString() },
      ]);
      expect(result).toMatchObject({ sent: 1, remaining: 0 });
      expect(result.checkIns).toHaveLength(1);
      expect(await queue.count()).toBe(0);
    }
  });

  it("sends the instant a keep was made, not the instant it is sent", async () => {
    const made = Date.parse("2026-11-12T23:50:00Z");
    for (const queue of [memoryQueue(), idbQueue(fakeIndexedDb())]) {
      await queue.add({ clientId: "c-1", kind: "text", text: "Late", size: 4, createdAt: made });
      await queue.add({
        clientId: "c-2",
        kind: "checkin",
        checkIn: { feeling: glad, line: null, urge: null, replaces: null },
        size: 0,
        createdAt: made + 1000,
      });
      const { engine, calls } = recorder();
      await queue.flush(engine);
      expect(calls.map((c) => c.marks ?? c.capturedAt)).toEqual([
        { feeling: undefined, good: undefined, capturedAt: "2026-11-12T23:50:00.000Z" },
        "2026-11-12T23:50:01.000Z",
      ]);
    }
  });

  it("leaves a check-in waiting when the computer cannot be reached", async () => {
    const queue = memoryQueue();
    await queue.add({
      clientId: "c-ci",
      kind: "checkin",
      checkIn: { feeling: glad, line: null, urge: null, replaces: null },
      size: 0,
      createdAt: 1,
    });
    const engine = {
      async keepCheckIn() {
        throw new ConnectionError("asleep");
      },
    } as unknown as PhoneEngine;
    const result = await queue.flush(engine);
    expect(result).toMatchObject({ sent: 0, remaining: 1 });
  });
});

describe("one upload in flight across tabs", () => {
  it("direct keeps take the same lock as the queue, so two tabs never send at once", async () => {
    let held = false;
    let overlap = false;
    const waiting: Array<() => void> = [];
    const locks = {
      async request(_name: string, fn: () => Promise<unknown>) {
        while (held) await new Promise<void>((r) => waiting.push(r));
        held = true;
        try {
          return await fn();
        } finally {
          held = false;
          waiting.shift()?.();
        }
      },
    };
    Object.defineProperty(globalThis, "navigator", { value: { locks }, configurable: true });
    try {
      let inFlight = 0;
      const send = async () => {
        inFlight += 1;
        if (inFlight > 1) overlap = true;
        await new Promise((r) => setTimeout(r, 5));
        inFlight -= 1;
        return "ok";
      };
      await Promise.all([withKeepLock(send), withKeepLock(send), withKeepLock(send)]);
      expect(overlap).toBe(false);
    } finally {
      Reflect.deleteProperty(globalThis, "navigator");
    }
  });

  it("without Web Locks a keep just runs", async () => {
    expect(await withKeepLock(async () => 7)).toBe(7);
  });
});
