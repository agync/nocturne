// A phone alone, answered in the page, for the browser tests and the screen tests. `?m=` picks
// what the phone holds. Names and words are the design's fictional ones. Nothing here is in the
// shipped app.

import { emit } from "@tauri-apps/api/event";
import type {
  FoundItem,
  HomeAnswer,
  HomeCall,
  HomeStats,
  JournalDay,
  PhoneItem,
  PhoneMode,
  RemoteAnswer,
  RemoteCall,
  TimelineRow,
} from "../../../engine/bindings";
import { emptyDay, itemView } from "../../testing/journalFixtures";
import type { HomeFiles } from "../api";
import { memoryNotes, SHOWN, SINCE } from "../reminder";

export const HOME_DAY = "2026-11-21";

export type HomeScenario = "first" | "fresh" | "kept" | "reminder" | "damaged";

export const HOME_SCENARIOS: readonly HomeScenario[] = [
  "first",
  "fresh",
  "kept",
  "reminder",
  "damaged",
];

const at = (time: string) => `${HOME_DAY}T${time}:00.000Z`;

const ROW = {
  checkin: null,
  good: null,
  feeling: null,
  mentions: [],
  leftOut: false,
  moveTo: null,
} as const;

function keptRows(): TimelineRow[] {
  return [
    {
      ...ROW,
      mentions: [],
      kind: "note",
      part: "morning",
      item: itemView({
        id: "note-swap",
        kind: "text",
        keptAt: at("07:05"),
        source: "phone",
        status: "kept",
        text: "Ask about the swap for the 28th",
        title: "Ask about the swap for the 28th",
      }),
    },
    {
      ...ROW,
      mentions: [],
      kind: "voice",
      part: "morning",
      item: itemView({
        id: "voice-1",
        kind: "audio",
        keptAt: at("08:12"),
        source: "phone",
        status: "kept",
        durationMs: 8000,
        hasOriginal: true,
        title: "Voice note",
      }),
    },
  ];
}

const FOUND: FoundItem[] = [
  {
    receipt: {
      itemId: "note-swap",
      kind: "text",
      source: "phone",
      label: "Ask about the swap for the 28th",
      keptAt: at("07:05"),
      durationMs: null,
      span: null,
      unconfirmed: false,
      fromDevice: null,
    },
    excerpt: [
      { text: "Ask about the ", matched: false },
      { text: "swap", matched: true },
      { text: " for the 28th", matched: false },
    ],
    private: false,
  },
  {
    receipt: {
      itemId: "note-jo",
      kind: "audio",
      source: "phone",
      label: "Voice note",
      keptAt: "2026-11-19T18:48:00.000Z",
      durationMs: 9000,
      span: null,
      unconfirmed: false,
      fromDevice: null,
    },
    excerpt: [
      { text: "Jo can ", matched: false },
      { text: "swap", matched: true },
      { text: " earlies with me next week if Ward 6 agrees", matched: false },
    ],
    private: false,
  },
];

const phoneItem = (id: string): PhoneItem => ({
  id,
  kind: "text",
  title: "Ask about the swap for the 28th",
  keptAt: at("07:05"),
  durationMs: null,
  pages: null,
  hasOriginal: false,
  summary: null,
  text: "Ask about the swap for the 28th",
  truncated: false,
});

/** What the scripted phone holds, and how it answers. */
export function homeState(scenario: HomeScenario) {
  let mode: PhoneMode | null = scenario === "first" ? null : "alone";
  // A damaged memory is found, and put aside, when something first tries to open it.
  let broken = scenario === "damaged";
  let damaged = false;
  const opens = () => {
    if (!broken) return true;
    damaged = true;
    return false;
  };
  let rows = scenario === "kept" || scenario === "reminder" ? keptRows() : [];
  let lastBackup: HomeStats["lastBackup"] = null;
  const day = (): JournalDay => ({ ...emptyDay(HOME_DAY, true), rows });
  const stats = (): HomeStats => ({
    counts: { items: rows.length, memories: 0, pages: 0, originals: rows.length },
    bytes: 412_000_000,
    lastKept: rows.at(-1)?.item.keptAt ?? null,
    lastBackup,
  });

  function remote(call: RemoteCall): RemoteAnswer {
    if (!opens()) return { type: "error", code: "notReady" };
    switch (call.cmd) {
      case "getStatus":
        return { type: "status", status: { readiness: "ready", pageDate: null, needsYou: 0 } };
      case "getJournalDay":
        return { type: "journalDay", day: day() };
      case "getItem":
        return { type: "item", item: phoneItem(call.id) };
      case "keepText": {
        const item = itemView({
          id: call.clientId,
          kind: "text",
          keptAt: at("09:30"),
          source: "phone",
          status: "kept",
          text: call.text,
          title: call.text.slice(0, 40),
        });
        rows = [...rows, { ...ROW, mentions: [], kind: "note", part: "morning", item }];
        return {
          type: "kept",
          kept: {
            item: { ...phoneItem(call.clientId), text: call.text, title: item.title },
            duplicate: false,
            takeBackUntil: at("09:35"),
          },
        };
      }
      case "unkeep":
        rows = rows.filter((r) => r.item.id !== call.id);
        return { type: "done" };
      default:
        return { type: "error", code: "notFound" };
    }
  }

  function local(call: HomeCall): HomeAnswer {
    switch (call.cmd) {
      case "stats":
        return opens() ? { type: "stats", stats: stats() } : { type: "error", code: "notReady" };
      case "find":
        return {
          type: "found",
          items: rows.length === 0 ? [] : /swap/i.test(call.query) ? FOUND : [],
        };
      case "saveBackup":
        if (call.passphrase === "too full of words") return { type: "error", code: "full" };
        lastBackup = { at: at("09:32"), bytes: 412_000_000 };
        return {
          type: "backupSaved",
          report: {
            path: call.dest,
            memories: 0,
            pages: 0,
            originals: rows.length,
            bytes: 412_000_000,
          },
        };
      case "checkBackup":
        if (call.passphrase !== "correct horse battery" && call.passphrase !== "already here")
          return { type: "error", code: "wrongPassphrase" };
        return {
          type: "backupChecked",
          check: {
            summary: {
              madeAt: at("09:32"),
              appVersion: "0.9.0",
              storeVersion: 1,
              memories: 0,
              items: 128,
              pages: 0,
              originals: 40,
              onlineUsed: false,
              awayUsed: false,
              bytes: 412_000_000,
            },
            here:
              call.passphrase === "already here"
                ? {
                    memories: 0,
                    pages: 0,
                    originals: 2,
                    lastKept: at("08:12"),
                    keptAfterBackup: 2,
                    bytes: 1,
                  }
                : null,
            putAside: null,
          },
        };
      case "restoreBackup":
        if (call.replace) return { type: "error", code: "invalid" };
        mode = "alone";
        broken = false;
        damaged = false;
        rows = keptRows();
        void emit("mode_changed", mode);
        return { type: "restored" };
      case "deleteEverything":
        if (call.phrase !== "delete everything") return { type: "error", code: "wrongPhrase" };
        rows = [];
        lastBackup = null;
        return { type: "deleted" };
      default:
        return { type: "error", code: "invalid" };
    }
  }

  /** The app's home commands; undefined for a command that is not one. */
  function answer(cmd: string, args: Record<string, unknown>): unknown {
    switch (cmd) {
      case "mode":
        return mode;
      case "memory_damaged":
        return damaged ? "store-put-aside-1" : null;
      case "start_alone": {
        const name = String(args.name ?? "").trim();
        if (name === "" || name.length > 40) throw "invalid";
        mode = "alone";
        void emit("mode_changed", mode);
        return null;
      }
      case "home_call":
        return remote(args.call as RemoteCall);
      case "home_local":
        return local(args.call as HomeCall);
      case "hear_voice_notes":
        return false;
      default:
        return undefined;
    }
  }

  return { answer };
}

/** The save and open sheets, as a person who picks the first place offered. */
export const scriptedFiles: HomeFiles = {
  saveTo: async (suggested) => `/sdcard/Download/${suggested}`,
  open: async () => ({
    path: "/sdcard/Download/nocturne-backup-2026-11-21.nocturne",
    name: "nocturne-backup-2026-11-21.nocturne",
    bytes: 412_000_000,
  }),
};

/** What the scripted phone noted for itself; it lasts across reloads in one test run. */
export const scriptedNoteRows = new Map<string, string>();
export const scriptedNotes = memoryNotes(scriptedNoteRows);

/** The reminder's notes, as after 31 days of keeping with no backup. */
export function seedReminder(now: number): void {
  scriptedNoteRows.set(SINCE, String(now - 31 * 86_400_000));
  scriptedNoteRows.delete(SHOWN);
}
