// The phone's key store: one device record in IndexedDB at the phone's home origin. The private
// key is a non-extractable CryptoKey, so the page (and anything running in it) can use it to
// derive bits but can never read it out. Only what pairing and the first Hello give is stored;
// there is no field for session material.

import { type Bytes, copy } from "./bytes";
import type { KeyStore, PairedDevice } from "./engine";
import { DB_NAME, openDb, run, STORE_DEVICE } from "./idb";
import * as P from "./protocol";

const KEY = "current";

/** The fields of a stored device, in one place, so a test can hold the record to them. */
export const DEVICE_FIELDS = [
  "deviceId",
  "privateKey",
  "publicKey",
  "cpub",
  "homeOrigin",
  "address",
  "deviceName",
  "computerName",
  "connection",
  "version",
  "pairedAt",
  "lastConnectedAt",
] as const satisfies readonly (keyof PairedDevice)[];

function asBytes(value: unknown, length: number): Bytes | null {
  if (!(value instanceof Uint8Array) || value.length !== length) return null;
  return copy(value);
}

/** A record read back from storage, or null when it is not one this version wrote. */
export function readDevice(raw: unknown): PairedDevice | null {
  if (typeof raw !== "object" || raw === null) return null;
  const r = raw as Record<string, unknown>;
  const deviceId = asBytes(r.deviceId, P.DEVICE_ID_LEN);
  const publicKey = asBytes(r.publicKey, P.PUBKEY_LEN);
  const cpub = asBytes(r.cpub, P.PUBKEY_LEN);
  if (!deviceId || !publicKey || !cpub) return null;
  if (typeof r.privateKey !== "object" || r.privateKey === null) return null;
  if (typeof r.homeOrigin !== "string" || typeof r.address !== "string") return null;
  if (typeof r.deviceName !== "string") return null;
  return {
    deviceId,
    privateKey: r.privateKey as CryptoKey,
    publicKey,
    cpub,
    homeOrigin: r.homeOrigin,
    address: r.address,
    deviceName: r.deviceName,
    computerName: typeof r.computerName === "string" ? r.computerName : null,
    connection: r.connection === "simple" || r.connection === "mostPrivate" ? r.connection : null,
    version: typeof r.version === "string" ? r.version : null,
    pairedAt: typeof r.pairedAt === "number" ? r.pairedAt : 0,
    lastConnectedAt: typeof r.lastConnectedAt === "number" ? r.lastConnectedAt : null,
  };
}

/** Exactly the stored fields and nothing else, whatever the caller's object carries. */
export function deviceRecord(device: PairedDevice): Record<string, unknown> {
  return Object.fromEntries(DEVICE_FIELDS.map((f) => [f, device[f]]));
}

export function idbKeyStore(factory?: IDBFactory): KeyStore {
  let opened: Promise<IDBDatabase> | null = null;
  const db = () => {
    opened ??= openDb(factory);
    return opened;
  };
  return {
    async device() {
      return readDevice(await run(await db(), STORE_DEVICE, "readonly", (s) => s.get(KEY)));
    },
    async save(device) {
      await run(await db(), STORE_DEVICE, "readwrite", (s) => s.put(deviceRecord(device), KEY));
    },
    async clear() {
      await run(await db(), STORE_DEVICE, "readwrite", (s) => s.delete(KEY));
    },
  };
}

/** Asks the browser not to evict the key and the waiting keeps. Best effort. */
export async function requestPersistence(
  storage: Pick<StorageManager, "persist" | "persisted"> | undefined = globalThis.navigator
    ?.storage,
): Promise<boolean> {
  try {
    if (!storage?.persist) return false;
    return (await storage.persisted?.()) || (await storage.persist());
  } catch {
    return false;
  }
}

export { DB_NAME };
