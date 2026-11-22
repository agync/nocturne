// The lost-phone reminder on Today: after 30 days of keeping with no backup, shown once. The day
// keeping began and that it was shown are noted on this phone, beside the phone's other stores.

import type { HomeStats } from "../../engine/bindings";
import { openNotesDb, run, STORE_NOTES } from "../idb";

export const REMINDER_DAYS = 30;
const DAY_MS = 86_400_000;
export const SINCE = "keepingSince";
export const SHOWN = "reminderShown";

export interface NoteStore {
  get(key: string): Promise<string | null>;
  set(key: string, value: string): Promise<void>;
}

/** The phone's notes in IndexedDB. A storage that refuses reads as nothing noted. */
export function idbNotes(factory?: IDBFactory): NoteStore {
  let db: Promise<IDBDatabase> | null = null;
  const ready = () => {
    db ??= openNotesDb(factory);
    return db;
  };
  return {
    async get(key) {
      try {
        const value = await run(await ready(), STORE_NOTES, "readonly", (s) => s.get(key));
        return typeof value === "string" ? value : null;
      } catch {
        return null;
      }
    },
    async set(key, value) {
      try {
        await run(await ready(), STORE_NOTES, "readwrite", (s) => s.put(value, key));
      } catch {
        // Not noted: the reminder may show again, which does no harm.
      }
    },
  };
}

export function memoryNotes(rows = new Map<string, string>()): NoteStore {
  return {
    get: async (key) => rows.get(key) ?? null,
    set: async (key, value) => void rows.set(key, value),
  };
}

/** Whether the reminder shows now. Notes the day keeping began the first time anything is kept. */
export async function reminderDue(
  stats: HomeStats,
  now: number,
  notes: NoteStore,
): Promise<boolean> {
  if (stats.counts.items === 0 || stats.lastBackup) return false;
  if (await notes.get(SHOWN)) return false;
  const since = Number(await notes.get(SINCE));
  if (!since) {
    await notes.set(SINCE, String(now));
    return false;
  }
  return now - since >= REMINDER_DAYS * DAY_MS;
}

/** Answered with either button: it does not come back. */
export function reminderDone(notes: NoteStore): Promise<void> {
  return notes.set(SHOWN, "1");
}

/** Everything was deleted: keeping starts again from nothing. */
export async function forgetReminder(notes: NoteStore): Promise<void> {
  await notes.set(SINCE, "");
  await notes.set(SHOWN, "");
}
