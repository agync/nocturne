import { describe, expect, it } from "vitest";
import type { MarkState } from "../components/Mark/Mark";
import type { CouldntReadReason, EngineState, ItemKind, SpokenKind } from "../engine/bindings";
import type { PaperSize } from "../screens/editions/fit";
import { copyDeck } from "./t";

const sources = import.meta.glob(
  ["../**/*.ts", "../**/*.tsx", "!../**/*.test.ts", "!../**/*.test.tsx"],
  {
    query: "?raw",
    import: "default",
    eager: true,
  },
) as Record<string, string>;

type Level = "sure" | "fairlySure" | "notSure";

/** Each record names every member of a union, so a new member fails to compile until listed here. */
const ITEM_KINDS: Record<ItemKind, true> = {
  text: true,
  image: true,
  audio: true,
  document: true,
  web: true,
  checkin: true,
};
const MARK_STATES: Record<MarkState, true> = {
  resting: true,
  reading: true,
  tidying: true,
  needsYou: true,
  paused: true,
};
const LEVELS: Record<Level, true> = { sure: true, fairlySure: true, notSure: true };
const ENGINE_STATES: Record<EngineState, true> = {
  starting: true,
  gettingReady: true,
  resting: true,
  reading: true,
  tidying: true,
  needsYou: true,
  paused: true,
  stopped: true,
};
const REASONS: Record<CouldntReadReason, true> = {
  unsupported: true,
  corrupt: true,
  locked: true,
  tooLong: true,
  language: true,
  mindUnavailable: true,
};
const SPOKEN: Record<SpokenKind, true> = {
  opening: true,
  needsYou: true,
  changed: true,
  comingUp: true,
  letGo: true,
  question: true,
};
const PAPER: Record<PaperSize, true> = { a4: true, letter: true, receipt: true };

const each = (record: Record<string, true>, build: (member: string) => string[]) =>
  Object.keys(record).flatMap(build);

/**
 * Keys that code builds at run time, so no literal names them. Each family lists exactly what
 * the code can produce from the values it is built from; a key under the family's prefix that
 * code cannot produce is dead, and the first test below fails on it.
 */
const BUILT_AT_RUN_TIME: Array<{ prefix: string; why: string; producible: string[] }> = [
  {
    prefix: "kind.",
    why: "t(`kind.${kind}.spoken`) for an item kind",
    producible: each(ITEM_KINDS, (k) => [`kind.${k}`, `kind.${k}.spoken`]),
  },
  {
    prefix: "journal.",
    why: "the journal screens, built by the lanes that follow the foundation; tighten this to named keys as each screen lands",
    producible: Object.keys(copyDeck).filter((key) => key.startsWith("journal.")),
  },
  {
    prefix: "feeling.",
    why: "the check-in builds part, word and meaning ids from the word lists in ui/src/journal/feelings.ts",
    producible: Object.keys(copyDeck).filter((key) => key.startsWith("feeling.")),
  },
  {
    prefix: "mark.",
    why: "t(`mark.${state}`)",
    producible: each(MARK_STATES, (s) => [`mark.${s}`]),
  },
  {
    prefix: "certainty.",
    why: "t(`certainty.${level}`) and its hint",
    producible: each(LEVELS, (l) => [`certainty.${l}`, `certainty.${l}.hint`]),
  },
  {
    prefix: "status.state.",
    why: "t(`status.state.${state}`)",
    producible: each(ENGINE_STATES, (s) => [`status.state.${s}`]),
  },
  {
    prefix: "recent.reason.",
    why: "t(`recent.reason.${reason}`)",
    producible: each(REASONS, (r) => [`recent.reason.${r}`]),
  },
  {
    prefix: "listen.section.",
    why: "t(`listen.section.${kind}`), besides the count line that is named",
    producible: each(SPOKEN, (k) => [`listen.section.${k}`]),
  },
  {
    prefix: "print.size.",
    why: "t(`print.size.${size}`), besides the label that is named",
    producible: each(PAPER, (s) => [`print.size.${s}`]),
  },
];

const producible = new Set(BUILT_AT_RUN_TIME.flatMap((family) => family.producible));

const text = Object.values(sources).join("\n");

const named = (key: string) =>
  ['"', "'", "`"].some((quote) => text.includes(`${quote}${key}${quote}`));

describe("the copy deck", () => {
  it("has no key that no code uses", () => {
    const unused = Object.keys(copyDeck).filter((key) => !named(key) && !producible.has(key));
    expect(unused, "delete these keys: nothing names or builds them").toEqual([]);
  });

  it("lists only keys the deck has, so a family cannot go stale", () => {
    const missing = [...producible].filter((key) => !(key in copyDeck));
    expect(missing, "the code builds these keys but the deck lacks them").toEqual([]);
    for (const { prefix, why } of BUILT_AT_RUN_TIME) {
      expect(
        Object.keys(copyDeck).some((key) => key.startsWith(prefix)),
        `${prefix} (${why}) matches no key`,
      ).toBe(true);
    }
  });
});
