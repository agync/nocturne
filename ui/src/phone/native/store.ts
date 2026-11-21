// The paired device as the app's Rust side keeps it. The keys never come to the web view: the
// screens read the names and the address, and the Rust side does the rest.

import { invoke } from "@tauri-apps/api/core";
import type { RemoteConnection } from "../../engine/bindings";
import { fromBase64Url } from "../bytes";
import type { KeyStore, PairedDevice } from "../engine";

/** What the Rust side says about the pairing. */
export interface DeviceInfo {
  deviceId: string;
  address: string;
  homeOrigin: string;
  deviceName: string;
  computerName: string | null;
  connection: RemoteConnection | null;
  version: string | null;
  pairedAt: number;
  lastConnectedAt: number | null;
}

export function fromInfo(info: DeviceInfo): PairedDevice {
  return {
    deviceId: fromBase64Url(info.deviceId),
    publicKey: new Uint8Array(),
    cpub: new Uint8Array(),
    homeOrigin: info.homeOrigin,
    address: info.address,
    deviceName: info.deviceName,
    computerName: info.computerName,
    connection: info.connection,
    version: info.version,
    pairedAt: info.pairedAt,
    lastConnectedAt: info.lastConnectedAt,
  };
}

/** A store whose listeners hear when the pairing comes or goes. */
export interface NativeKeyStore extends KeyStore {
  /** The pairing changed on the Rust side (paired, removed, disconnected). */
  changed(): void;
  subscribe(listener: () => void): () => void;
}

export function nativeKeyStore(): NativeKeyStore {
  const listeners = new Set<() => void>();
  return {
    async device() {
      const info = await invoke<DeviceInfo | null>("device");
      return info ? fromInfo(info) : null;
    },
    // The Rust side keeps the device as it pairs and connects; nothing is saved from here.
    async save() {},
    async clear() {
      listeners.forEach((l) => {
        l();
      });
    },
    changed() {
      listeners.forEach((l) => {
        l();
      });
    },
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
  };
}
