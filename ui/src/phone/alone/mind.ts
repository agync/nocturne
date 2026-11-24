// A phone alone's mind: an online service the person chose, reached by the phone's own core.
// Each call is one of the app's own commands; the key goes in once, with turning on.

import { invoke } from "@tauri-apps/api/core";
import type { CheckOutcome, ModelList, ReadingKind } from "../../engine/bindings";

/** The form's three details, as typed. */
export interface ProviderForm {
  url: string;
  key: string;
  model: string;
}

/** What is chosen. `needsKey`: chosen, but the key is not held, so nothing goes until it is. */
export type MindChosen =
  | { state: "off" }
  | { state: "on"; host: string; model: string }
  | { state: "needsKey"; host: string; model: string };

/** How much may go online each month. */
export type MindLimit =
  | { kind: "none" }
  | { kind: "things"; count: number }
  | { kind: "amount"; usd: number };

/** This month so far. */
export interface MindMonth {
  count: number;
  limit: MindLimit;
}

/** Read now, down to 20% battery and never with battery saving on; or Read anyway, down to 5%. */
export type ReadKind = "now" | "anyway";

/** Why reading on battery ended. */
export type ReadStopped =
  | "done"
  | "charging"
  | "low"
  | "saver"
  | "floor"
  | "limit"
  | "trouble"
  | "person"
  /** The phone's own time limit for such work ran out. */
  | "timeout"
  /** The mind was turned off or replaced. */
  | "off";

/** Why Read now or Read anyway did not start. */
export type ReadRefused =
  | "off"
  | "floor"
  | "low"
  | "saver"
  | "limit"
  | "trouble"
  | "offline"
  | "notAnswering";

/**
 * What was kept, being read a few at a time, and the facts the words about it are chosen from:
 * the phone's power, the person's Read now and why it last stopped.
 */
export interface MindReading {
  done: number;
  count: number;
  /** Known to charge. */
  charging?: boolean;
  percent?: number | null;
  /** Battery saving is on. */
  saver?: boolean;
  now?: ReadKind | null;
  stopped?: { why: ReadStopped; done: number } | null;
  /** iPhone paused it while the person was away, after this many. */
  away?: number | null;
  /** The last Read now looked once more and the service was still not answering. */
  stillNot?: boolean;
  /** Android shows no notifications for Nocturne. */
  quiet?: boolean;
}

/** The words the phone shows the reading with, away from Nocturne: counts only, never a kept thing. */
export interface ReadWords {
  channel: string;
  channelAbout: string;
  title: string;
  progress: string;
  stop: string;
  done: string;
  lowTitle: string;
  saverTitle: string;
  anywayText: string;
  floorTitle: string;
  floorText: string;
  stoppedTitle: string;
  stoppedText: string;
  pausedTitle: string;
  pausedText: string;
}

export class ReadNowError extends Error {
  readonly refused: ReadRefused;
  constructor(refused: ReadRefused) {
    super(refused);
    this.name = "ReadNowError";
    this.refused = refused;
  }
}

/** A part of what a request carried: its words as they left, or which photo or recording. */
export type MindSaid = { kind: "words"; text: string } | { kind: "photo" } | { kind: "recording" };

/** One request to the chosen service, with what it carried. */
export interface MindSent {
  id: string;
  at: string;
  kind: "read" | "answer" | "page" | "check";
  host: string;
  /** Bytes that left this phone. */
  bytes: number;
  /** What it carried, as it left; empty once something it carried was forgotten. */
  said: MindSaid[];
  /** Its words were removed when something it carried was forgotten. */
  forgotten: boolean;
  /** For a question sent in parts: which part, and what the provider asked to look up. */
  part?: MindPart;
  /** The question ended with nothing found. */
  nothing?: boolean;
}

/** One look-up the provider asked for, and how many lines went back. */
export interface MindAsked {
  look: "calendar" | "journal" | "search" | "health";
  from?: string;
  to?: string;
  words?: string;
  kind?: ReadingKind;
  lines: number;
}

/**
 * Which part of a question a request was: the question's id, its number from 1, what the
 * provider asked for in the part before, and where in `said` the words new to it begin.
 */
export interface MindPart {
  ask: string;
  n: number;
  asked: MindAsked[];
  from: number;
}

/** How the mind is doing, for Today, Ask and This phone. */
export type MindStatus =
  | { state: "off" }
  | { state: "fine"; host: string }
  /** No answer came from the service since `since`, a time. */
  | { state: "unreachable"; host: string; since: string }
  /** This phone has no way to the internet: nothing left it. */
  | { state: "offline"; host: string }
  | { state: "keyRefused"; host: string }
  | { state: "noCredit"; host: string }
  /** The month's limit is reached: nothing goes until `until`, a date. */
  | { state: "paused"; host: string; until: string };

/** What Today and Ask tell of the mind when it is not answering: a status, or a key not held. */
export type MindTrouble = MindStatus | { state: "needsKey"; host: string };

/** Why turning on did not happen. */
export type TurnOnProblem = "notHttps" | "addressNotAllowed" | "invalid";

export class TurnOnError extends Error {
  readonly problem: TurnOnProblem;
  constructor(problem: TurnOnProblem) {
    super(problem);
    this.name = "TurnOnError";
    this.problem = problem;
  }
}

export interface MindApi {
  chosen(): Promise<MindChosen>;
  check(form: ProviderForm): Promise<CheckOutcome>;
  /** One look at the service's list with the form's address and key. */
  list(form: ProviderForm): Promise<ModelList>;
  turnOn(form: ProviderForm): Promise<void>;
  /** Read now or Read anyway, from the person's tap; refused with a {@link ReadNowError}. */
  readNow(kind: ReadKind, words: ReadWords): Promise<MindReading>;
  readStop(): Promise<MindReading | null>;
  turnOff(): Promise<void>;
  setLimit(limit: MindLimit): Promise<void>;
  month(): Promise<MindMonth | null>;
  reading(): Promise<MindReading>;
  /** Newest first, the last 90 days. */
  sent(): Promise<MindSent[]>;
  status(): Promise<MindStatus>;
}

export function createMindApi(): MindApi {
  return {
    chosen: () => invoke<MindChosen>("mind_chosen"),
    check: (form) => invoke<CheckOutcome>("mind_check", { form }),
    list: (form) => invoke<ModelList>("mind_list", { form }),
    async readNow(kind, words) {
      try {
        return await invoke<MindReading>("mind_read_now", { kind, words });
      } catch (e) {
        if (typeof e === "string") throw new ReadNowError(e as ReadRefused);
        throw e;
      }
    },
    readStop: () => invoke<MindReading | null>("mind_read_stop"),
    async turnOn(form) {
      try {
        await invoke("mind_turn_on", { form });
      } catch (e) {
        if (e === "notHttps" || e === "addressNotAllowed" || e === "invalid")
          throw new TurnOnError(e);
        throw e;
      }
    },
    async turnOff() {
      await invoke("mind_turn_off");
    },
    async setLimit(limit) {
      await invoke("mind_set_limit", { limit });
    },
    month: () => invoke<MindMonth | null>("mind_month"),
    reading: () => invoke<MindReading>("mind_reading"),
    sent: () => invoke<MindSent[]>("mind_sent", { limit: 1000 }),
    status: () => invoke<MindStatus>("mind_status"),
  };
}

/** The host an address names, for "Checking with {host}…"; the words as typed when none. */
export function hostOf(url: string): string {
  const typed = url.trim();
  try {
    return new URL(typed).host || typed;
  } catch {
    return typed;
  }
}

/** Whether `chosen` reads and answers, or would once its key is given again. */
export function chosenHost(chosen: MindChosen | null): string | null {
  return chosen && chosen.state !== "off" ? chosen.host : null;
}
