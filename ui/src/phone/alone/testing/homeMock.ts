// A phone alone, answered in the page, for the browser tests and the screen tests. `?m=` picks
// what the phone holds. Names and words are the design's fictional ones. Nothing here is in the
// shipped app.

import { emit } from "@tauri-apps/api/event";
import type {
  CheckOutcome,
  FoundItem,
  HomeAnswer,
  HomeCall,
  HomeStats,
  JournalDay,
  KeptCounts,
  KeptHow,
  PageView,
  PhoneItem,
  PhoneMode,
  PutAsideMemory,
  RemoteAnswer,
  RemoteCall,
  TimelineRow,
} from "../../../engine/bindings";
import { emptyDay, itemView } from "../../testing/journalFixtures";
import type { MindChosen, MindSent, MindStatus, ProviderForm } from "../mind";
import { memoryNotes, SHOWN, SINCE } from "../reminder";

export const HOME_DAY = "2026-11-21";

export type HomeScenario = "first" | "fresh" | "kept" | "reminder" | "damaged" | "aside" | "full";

export const HOME_SCENARIOS: readonly HomeScenario[] = [
  "first",
  "fresh",
  "kept",
  "reminder",
  "damaged",
  "aside",
  "full",
];

/** The backup file the open sheet offers, as a person who picks the first one. */
export const PICKED_BACKUP = "nocturne-backup-2026-11-21.nocturne";

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

/** The stand-in service the screens and shots use; no real one is named. */
export const MIND_HOST = "answers.example";
export const MIND_MODEL = "reader-small";

/** `&mind=on` or `&mind=needsKey` starts with the online mind chosen. */
function firstMind(): MindChosen {
  const m = new URLSearchParams(location.search).get("mind");
  if (m === "on" || m === "needsKey") return { state: m, host: MIND_HOST, model: MIND_MODEL };
  return { state: "off" };
}

/**
 * The check, as a stand-in service answers it: a key "refused" is refused, a model "missing"
 * is not known, an address with "offline" in it finds no internet, and plain http:// online is
 * not allowed. Anything else works.
 */
export function scriptedCheck(form: ProviderForm): CheckOutcome {
  const url = form.url.trim();
  let host = url;
  try {
    host = new URL(url).host;
  } catch {
    return { type: "addressNotAllowed" };
  }
  if (url.startsWith("http://")) return { type: "notHttps" };
  if (url.includes("offline")) return { type: "noInternet" };
  if (form.key.trim() === "refused") return { type: "keyRefused" };
  if (form.model.trim() === "missing") return { type: "modelMissing" };
  return { type: "works", host, model: form.model.trim() };
}

const pageLine = (id: string, text: string, itemId: string, time: string) => ({
  id,
  kind: "changed" as const,
  text,
  memoryIds: [],
  receipts: [
    {
      itemId,
      kind: "text" as const,
      source: "phone" as const,
      label: "",
      keptAt: `${HOME_DAY}T${time}:00.000Z`,
      durationMs: null,
      span: null,
      unconfirmed: false,
      fromDevice: null,
    },
  ],
  choices: [],
  conflictId: null,
  subject: null,
  done: false,
  forgotten: false,
  on: null,
  lastUsed: null,
});

/** The page the online mind wrote this morning, from what the phone kept. */
export function writtenPage(): PageView {
  return {
    date: HOME_DAY,
    pageNumber: 1,
    edition: "phone",
    variant: "first",
    headline: "The swap for the 28th is still open.",
    dek: "One thing to ask about, and a quiet morning.",
    needsYou: [],
    changed: [
      pageLine("l1", "You meant to ask about the swap for the 28th.", "note-swap", "07:05"),
    ],
    letGo: [],
    upcoming: [],
    learned: [],
    question: null,
    moreNeedsYou: 0,
    moreChanged: 0,
    keptCount: 3,
    changesCount: 1,
    letGoCount: 0,
    catchUpDays: 0,
    privateSkipped: 0,
    pending: 0,
    tidiedFrom: null,
    tidiedTo: null,
    update: null,
  };
}

const SENT: MindSent[] = [
  {
    id: "s4",
    at: `${HOME_DAY}T07:40:00.000Z`,
    kind: "read",
    host: MIND_HOST,
    bytes: 2_310,
    said: [],
    forgotten: true,
  },
  {
    id: "s3",
    at: `${HOME_DAY}T07:12:00.000Z`,
    kind: "read",
    host: MIND_HOST,
    bytes: 1_840,
    said: [
      { kind: "words", text: "Read this note and list what it says will happen." },
      { kind: "words", text: "Ask about the swap for the 28th" },
    ],
    forgotten: false,
  },
  {
    id: "s2",
    at: `${HOME_DAY}T06:30:00.000Z`,
    kind: "page",
    host: MIND_HOST,
    bytes: 3_210,
    said: [{ kind: "words", text: "You meant to ask about the swap for the 28th." }],
    forgotten: false,
  },
  {
    id: "s1",
    at: "2026-11-20T21:02:00.000Z",
    kind: "check",
    host: MIND_HOST,
    bytes: 412,
    said: [],
    forgotten: false,
  },
];

/**
 * What a screen test changes while the app runs: how the service is doing (over `&trouble=`),
 * whether the page is written, the reading under way, and the questions stopped. The mock
 * counts the page's loads.
 */
export const scriptedMind = {
  trouble: undefined as string | null | undefined,
  page: true,
  reading: { done: 12, count: 46 },
  pageLoads: 0,
  stopped: [] as string[],
};

export function resetScriptedMind(): void {
  scriptedMind.trouble = undefined;
  scriptedMind.page = true;
  scriptedMind.reading = { done: 12, count: 46 };
  scriptedMind.pageLoads = 0;
  scriptedMind.stopped = [];
  waiting.clear();
}

/** Questions under way that wait to be stopped, by their ids. */
const waiting = new Map<string, () => void>();

/**
 * `&trouble=` names how the service is doing once the mind is on: `unreachable`, `offline`,
 * `keyRefused`, `noCredit` or `paused`. With none it is fine.
 */
function mindStatus(mind: MindChosen): MindStatus {
  if (mind.state !== "on") return { state: "off" };
  const host = mind.host;
  const trouble =
    scriptedMind.trouble !== undefined
      ? scriptedMind.trouble
      : new URLSearchParams(location.search).get("trouble");
  switch (trouble) {
    case "offline":
      return { state: "offline", host };
    case "unreachable":
      return { state: "unreachable", host, since: `${HOME_DAY}T06:40:00.000Z` };
    case "keyRefused":
      return { state: "keyRefused", host };
    case "noCredit":
      return { state: "noCredit", host };
    case "paused":
      return { state: "paused", host, until: "2026-12-01" };
    default:
      return { state: "fine", host };
  }
}

/** The answer the scripted online mind gives, with a receipt for the line it used. */
function answered(host: string): RemoteAnswer {
  const receipt = pageLine("answer", "", "note-swap", "07:05").receipts[0];
  return {
    type: "askDelta",
    delta: {
      askId: "ask-1",
      phase: "done",
      textDelta: "You meant to ask about the swap for the 28th.",
      receipts: receipt ? [receipt] : [],
      memoryIds: [],
      near: [],
      found: 1,
      etaSeconds: null,
      plain: true,
      answeredBy: { type: "online", host },
    },
  };
}

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
  let rows = ["kept", "reminder", "aside", "full"].includes(scenario) ? keptRows() : [];
  let aside: PutAsideMemory[] =
    scenario === "aside"
      ? [
          {
            id: "store-put-aside-2026-11-20-081500",
            at: "2026-11-20T08:15:00.000Z",
            bytes: 388_000_000,
            counts: { things: 117, journalEntries: 41, photosAndRecordings: 36, checkins: 5 },
          },
        ]
      : [];
  if (scenario === "aside" && new URLSearchParams(location.search).has("twice"))
    aside.unshift({
      id: "store-put-aside-2026-11-20-171000",
      at: "2026-11-20T17:10:00.000Z",
      bytes: 401_000_000,
      counts: { things: 120, journalEntries: 42, photosAndRecordings: 37, checkins: 5 },
    });
  let mind = firstMind();
  let calendarKept = 6;
  let healthKept = 412;
  const how = new Map<string, KeptHow>([["note-swap", "typed"]]);
  const forgotten = new Set<string>();
  let lastBackup: HomeStats["lastBackup"] = null;
  const privates = new Set<string>();
  let voice: "granted" | "denied" | "prompt" = "prompt";
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
      case "ask":
        return mind.state === "on" ? answered(mind.host) : { type: "error", code: "notReady" };
      case "getPage":
        scriptedMind.pageLoads += 1;
        return mind.state === "on" && scriptedMind.page
          ? { type: "page", page: writtenPage() }
          : { type: "error", code: "notFound" };
      case "getItem":
        if (forgotten.has(call.id)) return { type: "error", code: "notFound" };
        return { type: "item", item: { ...phoneItem(call.id), private: privates.has(call.id) } };
      case "keepText": {
        if (call.text === "no room left") return { type: "error", code: "full" };
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
      case "keepCheckIn":
        return {
          type: "checkIn",
          checkin: {
            id: call.clientId,
            keptAt: at("09:30"),
            day: HOME_DAY,
            feeling: call.input.feeling,
            line: call.input.line,
            urge: call.input.urge,
            about: null,
            tried: [],
            fromDevice: null,
            mentions: [],
            leftOut: false,
          },
        };
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
            items: rows.length,
            pages: 0,
            originals: rows.length,
            bytes: 412_000_000,
          },
        };
      case "checkBackup":
        if (call.passphrase === "from the future") return { type: "error", code: "newerBackup" };
        if (call.passphrase === "torn pages") return { type: "error", code: "damagedBackup" };
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
            putAside: aside[0]?.at ?? null,
          },
          backup: { things: 128, journalEntries: 52, photosAndRecordings: 40, checkins: 6 },
          here:
            call.passphrase === "already here"
              ? ({
                  things: 2,
                  journalEntries: 0,
                  photosAndRecordings: 1,
                  checkins: 0,
                } satisfies KeptCounts)
              : null,
        };
      case "restoreBackup":
        if (call.passphrase === "already here" && !call.replace)
          return { type: "error", code: "invalid" };
        mode = "alone";
        broken = false;
        damaged = false;
        rows = keptRows();
        void emit("mode_changed", mode);
        return { type: "restored" };
      case "setPrivate":
        if (call.private) privates.add(call.id);
        else privates.delete(call.id);
        return { type: "privateSet", item: { ...phoneItem(call.id), private: call.private } };
      case "deleteEverything":
        if (call.phrase !== "delete everything") return { type: "error", code: "wrongPhrase" };
        rows = [];
        aside = [];
        lastBackup = null;
        mind = { state: "off" };
        mode = null;
        void emit("mode_changed", null);
        return { type: "deleted" };
      case "putAside":
        return { type: "putAside", memories: aside };
      case "setKeptHow":
        how.set(call.id, call.how);
        return { type: "keptHowSet" };
      case "itemKept":
        return {
          type: "itemKept",
          kept: { how: how.get(call.id) ?? null, device: null, workedOut: false },
        };
      case "bringBack":
        if (!aside.some((m) => m.id === call.id)) return { type: "error", code: "notFound" };
        aside = [
          {
            id: "store-put-aside-2026-11-21-093000",
            at: at("09:30"),
            bytes: 412_000_000,
            counts: { things: rows.length, journalEntries: 0, photosAndRecordings: 1, checkins: 0 },
          },
        ];
        return { type: "restored" };
      case "deletePutAside":
        aside = aside.filter((m) => m.id !== call.id);
        return { type: "putAsideDeleted" };
      case "previewSourceDelete":
        if ((call.source === "health" ? healthKept : calendarKept) === 0)
          return { type: "error", code: "notFound" };
        return {
          type: "sourceDeletePreview",
          preview: {
            token: `kept-${call.source}`,
            items: call.source === "health" ? healthKept : calendarKept,
            versions: 0,
            memories: 0,
            pageLines: 0,
            recapLines: 0,
            noticing: 0,
          },
        };
      case "deleteSource":
        if (call.token !== `kept-${call.source}`) return { type: "error", code: "tooLate" };
        if (call.source === "health") healthKept = 0;
        else calendarKept = 0;
        return { type: "sourceDeleted" };
      case "previewForgetItem": {
        const row = rows.find((r) => r.item.id === call.id);
        if (!row || !call.withOriginal) return { type: "error", code: "notFound" };
        return {
          type: "forgetPreview",
          preview: {
            token: `forget-${call.id}`,
            memories: [],
            originals: [
              {
                itemId: call.id,
                label: row.item.title,
                kind: row.item.kind,
                keptAt: row.item.keptAt,
                durationMs: null,
                removable: true,
                otherMemories: 0,
              },
            ],
            includeOriginals: true,
            pagesAffected: 0,
          },
        };
      }
      case "forget": {
        const id = call.token.replace(/^forget-/, "");
        forgotten.add(id);
        rows = rows.filter((r) => r.item.id !== id);
        return {
          type: "forgotten",
          report: { memoriesRemoved: 0, originalsRemoved: 1, pagesRedacted: 0 },
        };
      }
      default:
        return { type: "error", code: "invalid" };
    }
  }

  /**
   * A question for the online mind: one that says "slowly" waits until it is stopped, "no
   * answer" gets none, and "which" is not sure. Anything else is answered.
   */
  function asked(question: string, askId: string): RemoteAnswer | Promise<RemoteAnswer> {
    if (mind.state !== "on") return { type: "error", code: "notReady" };
    const reply = answered(mind.host);
    if (reply.type !== "askDelta") return reply;
    const delta = { ...reply.delta, askId };
    const ended = (phase: "unavailable" | "notSure"): RemoteAnswer => ({
      type: "askDelta",
      delta: {
        ...delta,
        phase,
        textDelta: null,
        receipts: [],
        near:
          phase === "notSure"
            ? delta.receipts.map((receipt) => ({
                text: "Ask about the swap for the 28th",
                receipt,
              }))
            : [],
      },
    });
    if (question.includes("slowly"))
      return new Promise((done) => waiting.set(askId, () => done(ended("unavailable"))));
    if (question.includes("no answer")) return ended("unavailable");
    if (question.includes("which")) return ended("notSure");
    return { type: "askDelta", delta };
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
      case "home_ask":
        return asked(String(args.question), String(args.askId));
      case "home_cancel_ask": {
        const askId = String(args.askId);
        scriptedMind.stopped.push(askId);
        waiting.get(askId)?.();
        waiting.delete(askId);
        return null;
      }
      case "hear_voice_notes":
        return false;
      case "voice_access":
        return voice;
      case "voice_request":
        voice = "granted";
        return voice;
      case "free_space":
        return scenario === "full" ? 312_000_000 : 24_000_000_000;
      case "save_backup_file": {
        const passphrase = String(args.passphrase);
        if (passphrase === "closes the sheet") return null;
        return local({ cmd: "saveBackup", dest: PICKED_BACKUP, passphrase });
      }
      case "choose_backup_file":
        return {
          file: `/cache/backup-inbox/${PICKED_BACKUP}`,
          name: PICKED_BACKUP,
          size: 412_000_000,
          modified: at("09:32"),
        };
      case "let_go_backup_file":
        return null;
      case "mind_chosen":
        return mind;
      case "mind_check":
        return scriptedCheck(args.form as ProviderForm);
      case "mind_turn_on": {
        const outcome = scriptedCheck(args.form as ProviderForm);
        if (outcome.type === "notHttps") throw "notHttps";
        if (outcome.type !== "works") throw "invalid";
        mind = { state: "on", host: outcome.host, model: outcome.model };
        return null;
      }
      case "mind_turn_off":
        mind = { state: "off" };
        return null;
      case "mind_set_limit":
        return null;
      case "mind_month":
        return { count: 3, limit: { kind: "things", count: 200 } };
      case "mind_reading":
        return scriptedMind.reading;
      case "mind_sent":
        return SENT;
      case "mind_status":
        return mindStatus(mind);
      default:
        return undefined;
    }
  }

  return { answer };
}

/** What the scripted phone noted for itself; it lasts across reloads in one test run. */
export const scriptedNoteRows = new Map<string, string>();
export const scriptedNotes = memoryNotes(scriptedNoteRows);

/** The reminder's notes, as after 31 days of keeping with no backup. */
export function seedReminder(now: number): void {
  scriptedNoteRows.set(SINCE, String(now - 31 * 86_400_000));
  scriptedNoteRows.delete(SHOWN);
}
