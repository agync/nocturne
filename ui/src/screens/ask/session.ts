import type {
  AnsweredBy,
  GuideCite,
  HelpView,
  LeftOut,
  NearMatch,
  Receipt,
} from "../../engine/bindings";
import type { AskState } from "../../engine/context";

/** Everything the screen shows about one answer, so it can be drawn again without asking again. */
export interface Snapshot {
  phase: AskState;
  text: string;
  receipts: Receipt[];
  near: NearMatch[];
  found: number | null;
  memoryIds: string[];
  eta: number | null;
  failed: boolean;
  /** Where the answer was written, once the engine says. Absent when Online answers is off. */
  answeredBy?: AnsweredBy | null;
  /** What was kept back from an online question. */
  leftOut?: LeftOut | null;
  /** The sent log entry for an online question. */
  sendId?: number | null;
  /** What the Nocturne guide added. */
  help?: HelpView | null;
}

/** What the person did to the answer on screen. */
export interface Extras {
  /** Set once the memories behind the answer were forgotten: what to say instead of the answer. */
  forgotten: string | null;
  corrected: string[];
}

/** A finished turn, kept on screen above the new question. */
export interface Turn {
  key: number;
  question: string;
  text: string;
  summary: string;
  receipts: Receipt[];
  memoryIds: string[];
  /** Where the answer was written, so Earlier keeps its mark. */
  answeredBy?: AnsweredBy | null;
  sendId?: number | null;
  /** How many things the answer used. */
  count?: number;
  /** The guide sections a help answer cited. */
  guide?: GuideCite[];
}

export interface Session {
  current: { question: string; snapshot: Snapshot; extras: Extras } | null;
  earlier: Turn[];
  turns: number;
}

const empty = (): Session => ({ current: null, earlier: [], turns: 0 });

let session: Session = empty();

/**
 * The conversation on the Ask page lives here, not in the page, so opening a receipt and coming
 * back finds the answer where it was. It lasts as long as the window does.
 */
export const askSession = {
  get: (): Session => session,
  set: (next: Session): void => {
    session = next;
  },
  reset: (): void => {
    session = empty();
  },
};
