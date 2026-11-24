// The nightly backup of a phone alone (docs/design/phone-backup.md): each call is one of the app's
// own backup commands. A night and a restore report progress as `backup-progress` events.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { formatLongDay, formatTimeOfDay } from "../../copy/format";
import { type CopyId, type CopyValues, t } from "../../copy/t";
import type { BackupProgress, BackupSummary, KeptCounts } from "../../engine/bindings";

/** When the last night finished, and what the account holds for it. */
export interface LastNight {
  at: string;
  bytes: number;
}

/** How the backup stands. */
export type BackupState =
  | { state: "off" }
  | { state: "unavailable" }
  | { state: "running" }
  | { state: "upToDate" }
  | { state: "waiting" }
  | { state: "waitingForWifi" }
  | { state: "notInUse" }
  | { state: "noRoom" }
  | { state: "signedOut" }
  | { state: "noPermission" }
  | { state: "anotherPhone"; madeAt: string }
  | { state: "newer" }
  | { state: "damaged" }
  | { state: "failed" };

export interface BackupStatus {
  on: boolean;
  wifiOnly: boolean;
  last: LastNight | null;
  state: BackupState;
  /** The memory moved to a computer; a kept backup is there only to be deleted. */
  moved: boolean;
}

export type BackupProblem =
  | "unavailable"
  | "notReady"
  | "off"
  | "waitingForWifi"
  | "noPhrase"
  | "wrongPhrase"
  | "noBackup"
  | "alreadyThere"
  | "anotherPhone"
  | "notInUse"
  | "newer"
  | "damaged"
  | "noRoom"
  | "signedOut"
  | "noPermission"
  | "offline"
  | "stopped"
  | "busy"
  | "noSafePlace"
  | "holdsMemory"
  | "failed";

/** A call that answered with one of the backup's own reasons. */
export class BackupError extends Error {
  constructor(readonly problem: BackupProblem) {
    super(problem);
  }
}

/** What came of asking for the account. */
export type Connection =
  | { outcome: "connected"; account: string | null; previous: string | null; free: number | null }
  | { outcome: "refused" }
  | { outcome: "noAccount" }
  | { outcome: "turnedOff" }
  | { outcome: "offline" }
  | { outcome: "failed" };

/** The newest backup in the account, before the phrase. */
export interface FoundBackup {
  at: string | null;
  bytes: number;
}

/** One night "Which night?" lists. `count` names it to `restore`. */
export interface NightView {
  count: number;
  madeAt: string;
  bytes: number;
}

export type Opening = { with: "phrase"; text: string } | { with: "recoveryKey"; text: string };

export type Restored =
  | { kind: "replaced"; summary: BackupSummary; kept: KeptCounts }
  | {
      kind: "broughtIn";
      summary: BackupSummary;
      things: number;
      entries: number;
      checkins: number;
      same: number;
      already: number;
    };

export type KeySaved = "saved" | "cancelled" | "failed";

export interface BackupApi {
  status(): Promise<BackupStatus>;
  newPhrase(): Promise<string>;
  connect(choose?: boolean): Promise<Connection>;
  turnOn(phrase: string): Promise<void>;
  recoveryKey(): Promise<string>;
  dropRecoveryKey(): Promise<void>;
  changePhrase(phrase: string): Promise<void>;
  setWifiOnly(on: boolean): Promise<void>;
  now(overMobileData: boolean, onProgress: (p: BackupProgress) => void): Promise<LastNight>;
  turnOff(alsoDelete: boolean): Promise<void>;
  turnOnAgain(): Promise<void>;
  found(): Promise<FoundBackup | null>;
  nights(opening: Opening): Promise<NightView[]>;
  restore(
    opening: Opening,
    night: number | null,
    onProgress: (p: BackupProgress) => void,
  ): Promise<Restored>;
  makeInUse(): Promise<void>;
  stop(): Promise<void>;
  /** The phone's save sheet for the recovery key. */
  saveKey(key: string): Promise<KeySaved>;
  /** Progress of whatever night or restore runs; returns the way to stop listening. */
  watch(onProgress: (p: BackupProgress) => void): () => void;
}

const PROBLEMS: ReadonlySet<string> = new Set<BackupProblem>([
  "unavailable",
  "notReady",
  "off",
  "waitingForWifi",
  "noPhrase",
  "wrongPhrase",
  "noBackup",
  "alreadyThere",
  "anotherPhone",
  "notInUse",
  "newer",
  "damaged",
  "noRoom",
  "signedOut",
  "noPermission",
  "offline",
  "stopped",
  "busy",
  "noSafePlace",
  "holdsMemory",
  "failed",
]);

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    throw new BackupError(
      typeof e === "string" && PROBLEMS.has(e) ? (e as BackupProblem) : "failed",
    );
  }
}

async function following<T>(
  onProgress: (p: BackupProgress) => void,
  run: () => Promise<T>,
): Promise<T> {
  const stop = await listen<BackupProgress>("backup-progress", (e) => onProgress(e.payload)).catch(
    () => () => undefined,
  );
  try {
    return await run();
  } finally {
    stop();
  }
}

export function createBackupApi(): BackupApi {
  return {
    status: () => call("backup_status"),
    newPhrase: () => call("backup_new_phrase"),
    connect: (choose) => call("backup_connect", { choose: choose ?? false }),
    turnOn: (phrase) => call("backup_turn_on", { phrase }),
    recoveryKey: () => call("backup_recovery_key"),
    dropRecoveryKey: () => call("backup_drop_recovery_key"),
    changePhrase: (phrase) => call("backup_change_phrase", { phrase }),
    setWifiOnly: (on) => call("backup_set_wifi_only", { on }),
    now: (overMobileData, onProgress) =>
      following(onProgress, () => call("backup_now", { overMobileData })),
    turnOff: (alsoDelete) => call("backup_turn_off", { alsoDelete }),
    turnOnAgain: () => call("backup_turn_on_again"),
    found: () => call("backup_found"),
    nights: (opening) => call("backup_nights", { opening }),
    restore: (opening, night, onProgress) =>
      following(onProgress, () => call("backup_restore", { opening, night })),
    makeInUse: () => call("backup_make_in_use"),
    stop: () => invoke<void>("backup_stop").catch(() => undefined),
    saveKey: (key) => call<KeySaved>("backup_save_key", { key }).catch(() => "failed" as const),
    watch: (onProgress) => {
      const stop = listen<BackupProgress>("backup-progress", (e) => onProgress(e.payload));
      return () => void stop.then((off) => off()).catch(() => undefined);
    },
  };
}

/** When the last recovery key was made, as this phone noted it. */
export const KEY_MADE = "backup-key-made";
/** The Today card's cause, once it was shown, so it shows once for each. */
export const CARD_SHOWN = "backup-card-shown";

export const problemOf = (e: unknown): BackupProblem =>
  e instanceof BackupError ? e.problem : "failed";

/** Stopped for something only the person can fix. */
export type StopCause = "noRoom" | "signedOut" | "noPermission" | "anotherPhone" | "newer";

export function stopCause(s: BackupStatus | null): StopCause | null {
  if (!s?.on) return null;
  switch (s.state.state) {
    case "noRoom":
    case "signedOut":
    case "noPermission":
    case "anotherPhone":
    case "newer":
      return s.state.state;
    default:
      return null;
  }
}

/** A backup turned off with the backup kept in the account. */
export const keptOff = (s: BackupStatus | null): boolean =>
  s !== null && !s.on && !s.moved && s.last !== null;

const DAY_MS = 86_400_000;
const weekday = new Intl.DateTimeFormat("en-GB", { weekday: "long" });

/** "today", "Sunday", or "Sunday 15 November" when a week or more ago. */
export function backupDay(iso: string, now = new Date(), start = false): string {
  const at = new Date(iso);
  if (at.toDateString() === now.toDateString())
    return t(start ? "phone.backup.todayStart" : "phone.backup.today");
  return now.getTime() - at.getTime() < 6 * DAY_MS ? weekday.format(at) : formatLongDay(at);
}

export const backupTime = (iso: string): string => formatTimeOfDay(iso);

/** The line for this phone's own account: iCloud on an iPhone, Google Drive on Android. */
export const onPhone = (ios: boolean, iphone: CopyId, android: CopyId, values?: CopyValues) =>
  t(ios ? iphone : android, values);
