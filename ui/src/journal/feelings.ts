import type { CopyId } from "../copy/t";
import type { FeelingPart, FeelingPoint } from "../engine/bindings";

/** The four parts of the field, in the order the design lists them. */
export const PART_ORDER: readonly FeelingPart[] = ["woundUp", "litUp", "low", "settled"];

export type FeelingShape = "wound" | "lit" | "low" | "settled";

export interface PartInfo {
  part: FeelingPart;
  /** Copy id of the part's name: "Wound up", "Lit up", "Low", "Settled". */
  name: CopyId;
  shape: FeelingShape;
}

export const PARTS: Record<FeelingPart, PartInfo> = {
  woundUp: { part: "woundUp", name: "feeling.part.woundUp", shape: "wound" },
  litUp: { part: "litUp", name: "feeling.part.litUp", shape: "lit" },
  low: { part: "low", name: "feeling.part.low", shape: "low" },
  settled: { part: "settled", name: "feeling.part.settled", shape: "settled" },
};

export interface FeelingWord {
  /** camelCase of the word: "atEase", "leftOut". */
  id: string;
  /** The word as it is stored in a check-in and shown, the same as `feeling.word.<id>`. */
  word: string;
  /** Copy id of its one-line meaning. */
  meaningId: CopyId;
  /** Copy id of the word. */
  wordId: CopyId;
}

/** How many words a part shows before "More words". */
export const MAIN_WORDS = 12;

const word = (id: string, text: string): FeelingWord => ({
  id,
  word: text,
  wordId: `feeling.word.${id}` as CopyId,
  meaningId: `feeling.meaning.${id}` as CopyId,
});

/** Each part's 20 words, mild near the middle to strong near the corner: 12 shown, then 8 more. */
export const WORDS: Record<FeelingPart, readonly FeelingWord[]> = {
  woundUp: [
    word("uneasy", "uneasy"),
    word("restless", "restless"),
    word("irritated", "irritated"),
    word("nervous", "nervous"),
    word("frustrated", "frustrated"),
    word("worried", "worried"),
    word("stressed", "stressed"),
    word("tense", "tense"),
    word("overwhelmed", "overwhelmed"),
    word("anxious", "anxious"),
    word("angry", "angry"),
    word("panicked", "panicked"),
    word("jittery", "jittery"),
    word("impatient", "impatient"),
    word("annoyed", "annoyed"),
    word("onEdge", "on edge"),
    word("pressured", "pressured"),
    word("defensive", "defensive"),
    word("resentful", "resentful"),
    word("furious", "furious"),
  ],
  litUp: [
    word("pleased", "pleased"),
    word("cheerful", "cheerful"),
    word("curious", "curious"),
    word("hopeful", "hopeful"),
    word("motivated", "motivated"),
    word("playful", "playful"),
    word("proud", "proud"),
    word("energised", "energised"),
    word("excited", "excited"),
    word("joyful", "joyful"),
    word("delighted", "delighted"),
    word("thrilled", "thrilled"),
    word("amused", "amused"),
    word("eager", "eager"),
    word("focused", "focused"),
    word("inspired", "inspired"),
    word("confident", "confident"),
    word("lively", "lively"),
    word("elated", "elated"),
    word("overjoyed", "overjoyed"),
  ],
  low: [
    word("tired", "tired"),
    word("bored", "bored"),
    word("flat", "flat"),
    word("disappointed", "disappointed"),
    word("leftOut", "left out"),
    word("lonely", "lonely"),
    word("sad", "sad"),
    word("drained", "drained"),
    word("guilty", "guilty"),
    word("discouraged", "discouraged"),
    word("numb", "numb"),
    word("hopeless", "hopeless"),
    word("weary", "weary"),
    word("wistful", "wistful"),
    word("homesick", "homesick"),
    word("hurt", "hurt"),
    word("embarrassed", "embarrassed"),
    word("ashamed", "ashamed"),
    word("burntOut", "burnt out"),
    word("grieving", "grieving"),
  ],
  settled: [
    word("calm", "calm"),
    word("relaxed", "relaxed"),
    word("content", "content"),
    word("comfortable", "comfortable"),
    word("grateful", "grateful"),
    word("relieved", "relieved"),
    word("peaceful", "peaceful"),
    word("cosy", "cosy"),
    word("safe", "safe"),
    word("loved", "loved"),
    word("rested", "rested"),
    word("atEase", "at ease"),
    word("mellow", "mellow"),
    word("thoughtful", "thoughtful"),
    word("satisfied", "satisfied"),
    word("tender", "tender"),
    word("balanced", "balanced"),
    word("fulfilled", "fulfilled"),
    word("serene", "serene"),
    word("sleepy", "sleepy"),
  ],
};

/** Which part a point is in. A point on an axis counts as pleasant and as more energy. */
export function partOf(point: FeelingPoint): FeelingPart {
  const pleasant = point.pleasant >= 0;
  const more = point.energy >= 0;
  if (more) return pleasant ? "litUp" : "woundUp";
  return pleasant ? "settled" : "low";
}

/** How far into its part a point is: 0 at the middle, 1 at a corner or an edge. */
export function strength(point: FeelingPoint): number {
  return Math.min(1, Math.max(Math.abs(point.pleasant), Math.abs(point.energy)));
}

/** The part's 12 main words, the ones nearest the point first. Equal distances keep list order. */
export function nearestWords(part: FeelingPart, point: FeelingPoint): FeelingWord[] {
  const s = strength(point);
  return WORDS[part]
    .slice(0, MAIN_WORDS)
    .map((entry, index) => ({ entry, index, distance: Math.abs((index + 0.5) / MAIN_WORDS - s) }))
    .sort((a, b) => a.distance - b.distance || a.index - b.index)
    .map(({ entry }) => entry);
}

/** The words a part shows: the first 12, or all 20 after "More words". */
export function wordsFor(part: FeelingPart, more: boolean): readonly FeelingWord[] {
  return more ? WORDS[part] : WORDS[part].slice(0, MAIN_WORDS);
}

/** The word entry for a stored word, wherever it is in the lists. */
export function findWord(text: string): FeelingWord | undefined {
  for (const part of PART_ORDER) {
    const found = WORDS[part].find((entry) => entry.word === text);
    if (found) return found;
  }
  return undefined;
}

/** The four steps of how strongly a point is felt, for "Lit up, a little" to "Lit up, strongly". */
export type Strength = "little" | "quite" | "strongly";

export function strengthStep(point: FeelingPoint): Strength {
  const s = strength(point);
  if (s < 1 / 3) return "little";
  if (s < 2 / 3) return "quite";
  return "strongly";
}
