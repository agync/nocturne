// What the phone app hands its screens: the same `Services` the phone page uses, with the
// engine, the store, pairing and re-linking going to the app's Rust side.

import { Channel, invoke } from "@tauri-apps/api/core";
import { createHomeApi, type HomeApi } from "../alone/api";
import { createLocalEngine } from "../alone/engine";
import type { MoveApi } from "../alone/screens/MoveFlow";
import type { OpenFailure } from "../channel";
import { PairingError, type PairingResult } from "../pairing";
import { idbQueue } from "../queue";
import type { RelinkResult } from "../relink";
import { defaultScannerEnv } from "../scanner";
import type { PairingLike, Services } from "../services";
import { asError, createNativeEngine } from "./engine";
import { onIphone } from "./sources";
import { type DeviceInfo, fromInfo, type NativeKeyStore, nativeKeyStore } from "./store";

/** How a pairing ended on the Rust side without a paired phone. */
export type PairFailure =
  | "refused"
  | "expired"
  | "spent"
  | "badCode"
  | "badAddress"
  | "badName"
  | "noAnswer";

/** A pairing the Rust side holds, from a scan or a typed code; it commits once. */
export function nativePairing(): PairingLike {
  let committed = false;
  return {
    get spentAlready() {
      return committed;
    },
    async connect(name, onWords): Promise<PairingResult> {
      if (committed) return { kind: "spent" };
      committed = true;
      const words = new Channel<string[]>();
      words.onmessage = onWords;
      try {
        // The screens move on from "allowed" when the person does; nothing is announced here.
        const info = await invoke<DeviceInfo>("pair_connect", { name, onWords: words });
        return { kind: "allowed", device: fromInfo(info) };
      } catch (e) {
        if (e === "refused") return { kind: "refused" };
        if (e === "expired") return { kind: "expired" };
        return { kind: "spent" };
      }
    },
  };
}

export async function pairScanned(link: string): Promise<PairingLike> {
  await invoke("pair_scanned", { link });
  return nativePairing();
}

/** Whether the computer answers at the typed address; the address it resolved to. */
export function checkAddress(address: string): Promise<string> {
  return invoke<string>("check_address", { address });
}

export async function pairTyped(address: string, code: string): Promise<PairingLike> {
  try {
    await invoke("pair_typed", { address, code });
  } catch (e) {
    throw new PairingError(e === "badCode" ? "badCode" : "expired");
  }
  return nativePairing();
}

function whyFor(e: unknown): OpenFailure {
  const err = asError(e);
  const state = (err as { state?: string }).state;
  if (state === "removed") return "revoked";
  if (state === "cantConnect") return "refused";
  if (state === "turnedOff") return "off";
  return "network";
}

export interface NativeServices extends Services {
  store: NativeKeyStore;
  /** The phone's own memory, when it is a phone alone. */
  home: HomeApi;
  /** Moving that memory to a computer; the app's own commands when not set. */
  move?: MoveApi;
}

export function nativeServices(): NativeServices {
  const store = nativeKeyStore();
  const home = createHomeApi();
  const version = typeof __APP_VERSION__ === "string" ? __APP_VERSION__ : "development";
  let name = "";
  let address = "";
  const remember = (d: { address: string } | null) => {
    if (d) address = d.address;
  };
  void store.device().then(remember, () => undefined);
  // The app's Rust side says which phone this is; until it answers, not an iPhone.
  const platform = { ios: false, standalone: true, native: true };
  void onIphone().then((ios) => {
    platform.ios = ios;
  });
  const deviceName = () =>
    invoke<string | null>("plugin:nocturne-system|device_name").catch(() => null);
  void deviceName().then((n) => {
    name = n ?? "";
  });
  return {
    store,
    home,
    queue: idbQueue(),
    engineFor: () =>
      home.known() === "alone"
        ? createLocalEngine({ deviceName: () => name, version })
        : createNativeEngine({ onForgotten: () => store.changed() }),
    pairFromLink: () => Promise.reject(new PairingError("badCode")),
    pairFromCode: async (typed) => {
      const device = await store.device();
      if (!device) throw new PairingError("badCode");
      return pairTyped(device.address, typed);
    },
    async relink(next): Promise<RelinkResult> {
      try {
        const kept = await invoke<string>("relink", { address: next });
        address = kept;
        store.changed();
        return { ok: true, address: kept };
      } catch (e) {
        return { ok: false, why: whyFor(e) };
      }
    },
    scanner: defaultScannerEnv(),
    persist: () => Promise.resolve(true),
    platform,
    get host() {
      return address;
    },
    get origin() {
      return address ? `https://${address}` : "";
    },
    now: () => Date.now(),
    version,
    scripted: false,
    location: { pathname: "/", hash: "", search: "" },
    deviceName,
  };
}
