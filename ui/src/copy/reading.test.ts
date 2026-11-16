import { describe, expect, it } from "vitest";
import { copyDeck } from "./t";

/**
 * The reading rules for setup (docs/design/control-chips.md), as far as a string can show them.
 * Arguments such as {size} count as one word.
 */
const SETUP = Object.entries(copyDeck).filter(([id]) => id.startsWith("setup."));

const sentences = (text: string) =>
  text
    .replace(/\{[^}]*\}/g, "X")
    .split(/(?<=[.!?])\s+/)
    .filter((s) => s.trim() !== "");

const words = (sentence: string) => sentence.split(/\s+/).filter(Boolean).length;

describe("reading rules for setup copy", () => {
  it("has at most two sentences in a paragraph", () => {
    const over = SETUP.filter(([, text]) => sentences(text).length > 2).map(([id]) => id);
    expect(over).toEqual([]);
  });

  it("keeps a sentence to 20 words", () => {
    const over = SETUP.filter(([, text]) => sentences(text).some((s) => words(s) > 20)).map(
      ([id]) => id,
    );
    expect(over).toEqual([]);
  });

  it("has no parentheses or semicolons", () => {
    const over = SETUP.filter(([, text]) => /[();]/.test(text)).map(([id]) => id);
    expect(over).toEqual([]);
  });

  it("joins at most three facts with a dot on one line", () => {
    const over = SETUP.filter(([, text]) => text.split("·").length > 3).map(([id]) => id);
    expect(over).toEqual([]);
  });

  it("keeps a number with its unit", () => {
    const loose = SETUP.filter(([, text]) => /(\}|\d) (GB|MB)\b/.test(text)).map(([id]) => id);
    expect(loose).toEqual([]);
  });
});
