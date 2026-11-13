// A key store and a queue that keep everything in memory, for tests and the mock build.

import type { KeyStore, PairedDevice } from "../engine";

export function memoryKeyStore(initial: PairedDevice | null = null): KeyStore & {
  current(): PairedDevice | null;
} {
  let device = initial;
  return {
    async device() {
      return device;
    },
    async save(d) {
      device = d;
    },
    async clear() {
      device = null;
    },
    current: () => device,
  };
}
