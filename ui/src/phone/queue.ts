// Keeps that could not go across yet, waiting in IndexedDB in the phone's own storage. They are
// plain, not sealed at rest: the phone is the person's own, and the page does not claim more.
// Each has a client id, so the computer keeps it once however often it arrives.

import type { FlushResult, PendingKeep, PhoneEngine, Queue } from "./engine";
import { ConnectionError, PhoneRequestError } from "./engine";
import { openDb, run, STORE_QUEUE } from "./idb";

interface Backing {
  put(keep: PendingKeep): Promise<void>;
  remove(clientId: string): Promise<void>;
  all(): Promise<PendingKeep[]>;
  clear(): Promise<void>;
}

const KEEP_LOCK = "nocturne-keep";

/**
 * Runs `fn` while no other tab is sending a keep, so at most one upload is in flight across the
 * phone's tabs. Without Web Locks it just runs, one tab being the common case.
 */
export function withKeepLock<T>(fn: () => Promise<T>): Promise<T> {
  const locks = globalThis.navigator?.locks;
  return locks ? locks.request(KEEP_LOCK, fn) : fn();
}

const idle = (waiting: PendingKeep[]): FlushResult => ({
  sent: 0,
  failed: 0,
  remaining: waiting.length,
  kept: [],
  checkIns: [],
});

function build(backing: Backing): Queue {
  let flushing = false;
  const rejected = new Set<string>();

  const list = async () => (await backing.all()).sort((a, b) => a.createdAt - b.createdAt);

  const marks = (keep: PendingKeep) => ({ feeling: keep.feeling, good: keep.good });

  async function send(engine: PhoneEngine, keep: PendingKeep, out: FlushResult) {
    switch (keep.kind) {
      case "text":
        out.kept.push(await engine.keepText(keep.text ?? "", keep.clientId, marks(keep)));
        return;
      case "photo":
        out.kept.push(
          await engine.keepBytes(keep.blob ?? new Blob(), keep.clientId, keep.caption, marks(keep)),
        );
        return;
      case "recording":
        out.kept.push(
          await engine.keepRecording(keep.blob ?? new Blob(), keep.clientId, marks(keep)),
        );
        return;
      case "checkin":
        if (!keep.checkIn) throw new PhoneRequestError("invalid");
        out.checkIns.push(await engine.keepCheckIn(keep.checkIn, keep.clientId));
        return;
    }
  }

  async function flushOnce(engine: PhoneEngine): Promise<FlushResult> {
    const out: FlushResult = { sent: 0, failed: 0, remaining: 0, kept: [], checkIns: [] };
    for (const keep of await list()) {
      if (rejected.has(keep.clientId)) {
        out.failed += 1;
        continue;
      }
      try {
        await send(engine, keep, out);
        await backing.remove(keep.clientId);
        out.sent += 1;
      } catch (e) {
        if (e instanceof PhoneRequestError && (e.code === "invalid" || e.code === "tooLarge")) {
          // The computer will never take this one; it stays visible rather than vanishing.
          rejected.add(keep.clientId);
          out.failed += 1;
          continue;
        }
        if (!(e instanceof ConnectionError || e instanceof PhoneRequestError)) throw e;
        break;
      }
    }
    out.remaining = (await backing.all()).length;
    return out;
  }

  return {
    add: (keep) => backing.put(keep),
    remove: (clientId) => backing.remove(clientId),
    count: async () => (await backing.all()).length,
    list,
    clear: () => backing.clear(),
    async flush(engine) {
      if (flushing) {
        return idle(await backing.all());
      }
      flushing = true;
      try {
        const locks = globalThis.navigator?.locks;
        if (!locks) return await flushOnce(engine);
        // Two tabs never flush at once; the second finds the first at work and leaves.
        const result = await locks.request(KEEP_LOCK, { ifAvailable: true }, async (lock) =>
          lock ? flushOnce(engine) : null,
        );
        return result ?? idle(await backing.all());
      } finally {
        flushing = false;
      }
    },
  };
}

export function memoryQueue(): Queue {
  const items = new Map<string, PendingKeep>();
  return build({
    async put(keep) {
      items.set(keep.clientId, keep);
    },
    async remove(id) {
      items.delete(id);
    },
    async all() {
      return [...items.values()];
    },
    async clear() {
      items.clear();
    },
  });
}

export function idbQueue(factory?: IDBFactory): Queue {
  let opened: Promise<IDBDatabase> | null = null;
  const db = () => {
    opened ??= openDb(factory);
    return opened;
  };
  return build({
    async put(keep) {
      await run(await db(), STORE_QUEUE, "readwrite", (s) => s.put(keep));
    },
    async remove(id) {
      await run(await db(), STORE_QUEUE, "readwrite", (s) => s.delete(id));
    },
    async all() {
      return (await run(await db(), STORE_QUEUE, "readonly", (s) => s.getAll())) as PendingKeep[];
    },
    async clear() {
      await run(await db(), STORE_QUEUE, "readwrite", (s) => s.clear());
    },
  });
}
