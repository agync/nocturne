import { middleOf } from "../../components/FeelingField/FeelingField";
import type { Feeling, FeelingPart, FeelingPoint } from "../../engine/bindings";
import { PART_ORDER, partOf, WORDS } from "../../journal/feelings";

/** A check-in holds at most three words, because a moment can be mixed. */
export const MAX_WORDS = 3;

/** The part a stored word belongs to. */
export function partOfWord(word: string): FeelingPart | undefined {
  return PART_ORDER.find((part) => WORDS[part].some((entry) => entry.word === word));
}

/** The words with one added or taken away. A fourth is not added. */
export function toggleWord(words: readonly string[], word: string): string[] {
  if (words.includes(word)) return words.filter((w) => w !== word);
  return words.length >= MAX_WORDS ? [...words] : [...words, word];
}

/**
 * What would be saved. On the field the point decides the part. In the list the first word
 * chosen does, and the point stays where it was if it is already in that part.
 */
export function feelingOf(
  mode: "field" | "list",
  point: FeelingPoint | null,
  words: readonly string[],
): Feeling | null {
  if (mode === "field") return point ? { part: partOf(point), point, words: [...words] } : null;
  const first = words[0];
  const part = first ? partOfWord(first) : undefined;
  if (!part) return null;
  const kept = point && partOf(point) === part ? point : middleOf(part);
  return { part, point: kept, words: [...words] };
}

/** "relieved, content" becomes "Relieved, content". */
export function sentence(words: readonly string[]): string {
  const text = words.join(", ");
  return text.charAt(0).toUpperCase() + text.slice(1);
}
