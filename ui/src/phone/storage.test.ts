// @vitest-environment node
import { describe, expect, it } from "vitest";
import { copy, fromHex } from "./bytes";
import { generateDeviceKey } from "./crypto";
import type { PairedDevice, PhoneEngine } from "./engine";
import { ConnectionError, PhoneRequestError } from "./engine";
import { DEVICE_FIELDS, idbKeyStore, readDevice, requestPersistence } from "./keys";
import { idbQueue, memoryQueue } from "./queue";
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

  it("clear removes everything waiting", async () => {
    const queue = idbQueue(fakeIndexedDb());
    await queue.add(keep("c-1", 1));
    await queue.clear();
    expect(await queue.count()).toBe(0);
  });
});
