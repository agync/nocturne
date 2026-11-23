// A phone alone's mind: an online service the person chose, reached by the phone's own core.
// The check, turning on and off and what is chosen are the app's commands. The limit, this
// month's count, the reading under way and the Sent online list are not answered by the phone
// yet: until they are, `WAITING` refuses each with `notReady` and the screens leave them out.

import { invoke } from "@tauri-apps/api/core";
import type { CheckOutcome } from "../../engine/bindings";
import { HomeRequestError } from "./api";

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

/** What was kept before turning on, being read a few at a time. */
export interface MindReading {
  done: number;
  count: number;
}

/** One request to the chosen service, word for word. */
export interface MindSent {
  id: string;
  at: string;
  kind: "read" | "answer" | "page" | "check";
  host: string;
  /** Bytes that left this phone. */
  bytes: number;
  words: string;
}

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

/** What the phone does not answer yet. */
export interface MindWaiting {
  /** Whether these are answered; until they are, the limit and Sent online are not offered. */
  ready: boolean;
  setLimit(limit: MindLimit): Promise<void>;
  month(): Promise<MindMonth>;
  reading(): Promise<MindReading>;
  /** Newest first, the last 90 days. */
  sent(): Promise<MindSent[]>;
}

export interface MindApi extends MindWaiting {
  chosen(): Promise<MindChosen>;
  check(form: ProviderForm): Promise<CheckOutcome>;
  turnOn(form: ProviderForm): Promise<void>;
  turnOff(): Promise<void>;
}

const notYet = () => Promise.reject(new HomeRequestError("notReady"));

export const WAITING: MindWaiting = {
  ready: false,
  setLimit: notYet,
  month: notYet,
  reading: notYet,
  sent: notYet,
};

export function createMindApi(waiting: MindWaiting = WAITING): MindApi {
  return {
    ...waiting,
    chosen: () => invoke<MindChosen>("mind_chosen"),
    check: (form) => invoke<CheckOutcome>("mind_check", { form }),
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
