// Everything the phone app reaches outside itself, in one place, so the shipped app and the
// scripted build (used by the end-to-end tests) differ only in what they hand to the screens.

import type { Bytes } from "./bytes";
import { defaultEnv, type Env } from "./channel";
import { createPhoneEngine } from "./client";
import type { ConnectedEngine, KeyStore, Queue } from "./engine";
import { idbKeyStore, requestPersistence } from "./keys";
import { Pairing, type PairingResult } from "./pairing";
import { MIN_PHONE_BUILD, PHONE_BUILD } from "./protocol";
import { idbQueue } from "./queue";
import { type RelinkResult, relink } from "./relink";
import { defaultScannerEnv, type ScannerEnv } from "./scanner";

export interface PairingLike {
  connect(name: string, onWords: (words: string[]) => void): Promise<PairingResult>;
  readonly spentAlready: boolean;
}

export interface Platform {
  /** Safari on iPhone and iPad, found by the `standalone` property only it has. */
  ios: boolean;
  /** Opened from the home screen. */
  standalone: boolean;
  /** The phone app, not the page in a browser. */
  native?: boolean;
}

/** Inside the phone app: Tauri's bridge is on the page. */
export function isNative(): boolean {
  return "__TAURI_INTERNALS__" in globalThis;
}

export interface Services {
  store: KeyStore;
  queue: Queue;
  engineFor(address: () => string): ConnectedEngine;
  pairFromLink(offer: { s: Bytes; cpub: Bytes }): Promise<PairingLike>;
  pairFromCode(typed: string): Promise<PairingLike>;
  relink(address: string): Promise<RelinkResult>;
  scanner: ScannerEnv;
  persist(): Promise<boolean>;
  platform: Platform;
  /** The page's own host and origin: where it was paired and where words are bound. */
  host: string;
  origin: string;
  now(): number;
  /** The version this build is. A different one in Hello reloads the shell once. */
  version: string;
  /** Scripted builds say so; the shipped app does not. */
  scripted: boolean;
  /** A scripted build can report a browser as too old. */
  unsupported?: string[];
  /** What the address bar held when the page opened (path and fragment). */
  location: { pathname: string; hash: string; search: string };
  /** The phone's own name, where the app can read it, for "Name this phone". */
  deviceName?(): Promise<string | null>;
}

export function detectPlatform(): Platform {
  const nav = globalThis.navigator as Navigator & { standalone?: boolean };
  const standalone =
    nav.standalone === true ||
    globalThis.matchMedia?.("(display-mode: standalone)").matches === true;
  return { ios: "standalone" in nav, standalone };
}

export function realServices(): Services {
  const env: Env = defaultEnv();
  const store = idbKeyStore();
  const ctx = () => ({ env, host: location.host, origin: location.origin, store });
  const version = typeof __APP_VERSION__ === "string" ? __APP_VERSION__ : "development";
  const app = {
    platform: "browser" as const,
    version,
    minBuild: MIN_PHONE_BUILD,
    maxBuild: PHONE_BUILD,
  };
  return {
    store,
    queue: idbQueue(),
    engineFor: (address) => createPhoneEngine(store, address, { env, app }),
    pairFromLink: (offer) => Pairing.fromLink(offer, ctx()),
    pairFromCode: (typed) => Pairing.fromCode(typed, ctx()),
    relink: (address) => relink(store, address, env),
    scanner: defaultScannerEnv(),
    persist: () => requestPersistence(),
    platform: detectPlatform(),
    host: location.host,
    origin: location.origin,
    now: () => Date.now(),
    version,
    scripted: false,
    location: { pathname: location.pathname, hash: location.hash, search: location.search },
  };
}

export async function loadServices(): Promise<Services> {
  if (import.meta.env.VITE_ENGINE === "phone-mock") {
    return (await import("./testing/mockServices")).mockServices();
  }
  if (import.meta.env.VITE_PHONE_NATIVE !== "0" && isNative())
    return (await import("./native/services")).nativeServices();
  return realServices();
}
