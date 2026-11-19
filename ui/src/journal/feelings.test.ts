import { describe, expect, it } from "vitest";
import { enGB } from "../copy/deck";
import {
  findWord,
  MAIN_WORDS,
  nearestWords,
  PART_ORDER,
  PARTS,
  partOf,
  strength,
  strengthStep,
  WORDS,
  wordsFor,
} from "./feelings";

describe("the parts", () => {
  it("lists the four parts in the design's order, each with a name and a shape", () => {
    expect(PART_ORDER).toEqual(["woundUp", "litUp", "low", "settled"]);
    expect(PART_ORDER.map((p) => PARTS[p].shape)).toEqual(["wound", "lit", "low", "settled"]);
    for (const part of PART_ORDER) expect(enGB[PARTS[part].name]).toBeTruthy();
  });

  it("puts a point in the part its two numbers name", () => {
    expect(partOf({ pleasant: -0.5, energy: 0.5 })).toBe("woundUp");
    expect(partOf({ pleasant: 0.5, energy: 0.5 })).toBe("litUp");
    expect(partOf({ pleasant: -0.5, energy: -0.5 })).toBe("low");
    expect(partOf({ pleasant: 0.5, energy: -0.5 })).toBe("settled");
  });

  it("counts a point on an axis as pleasant and as more energy", () => {
    expect(partOf({ pleasant: 0, energy: 0 })).toBe("litUp");
    expect(partOf({ pleasant: 0, energy: -0.2 })).toBe("settled");
    expect(partOf({ pleasant: -0.2, energy: 0 })).toBe("woundUp");
  });
});

describe("strength", () => {
  it("is how far the point is from the middle on its larger axis", () => {
    expect(strength({ pleasant: 0, energy: 0 })).toBe(0);
    expect(strength({ pleasant: -0.3, energy: 0.6 })).toBe(0.6);
    expect(strength({ pleasant: 0.9, energy: -0.2 })).toBe(0.9);
    expect(strength({ pleasant: -1, energy: 1 })).toBe(1);
  });

  it("falls into three steps for the announcement", () => {
    expect(strengthStep({ pleasant: 0.2, energy: 0.1 })).toBe("little");
    expect(strengthStep({ pleasant: 0.5, energy: 0.1 })).toBe("quite");
    expect(strengthStep({ pleasant: 0.9, energy: 0.1 })).toBe("strongly");
  });
});

describe("the words", () => {
  it("holds 80 words, 20 in each part, 12 shown first", () => {
    let total = 0;
    for (const part of PART_ORDER) {
      expect(WORDS[part]).toHaveLength(20);
      expect(wordsFor(part, false)).toHaveLength(MAIN_WORDS);
      expect(wordsFor(part, true)).toHaveLength(20);
      total += WORDS[part].length;
    }
    expect(total).toBe(80);
  });

  it("keeps every word in the deck with its meaning, and never repeats one", () => {
    const seen = new Set<string>();
    for (const part of PART_ORDER) {
      for (const entry of WORDS[part]) {
        expect(enGB[entry.wordId], entry.id).toBe(entry.word);
        expect(enGB[entry.meaningId], entry.id).toBeTruthy();
        expect(seen.has(entry.id), entry.id).toBe(false);
        seen.add(entry.id);
      }
    }
  });

  it("names two-word words in camelCase", () => {
    expect(findWord("left out")?.id).toBe("leftOut");
    expect(findWord("burnt out")?.id).toBe("burntOut");
    expect(findWord("at ease")?.id).toBe("atEase");
    expect(findWord("on edge")?.id).toBe("onEdge");
    expect(findWord("nonsense")).toBeUndefined();
  });

  it("shows the first 12 of a part in the design's order", () => {
    expect(wordsFor("woundUp", false)[0]?.word).toBe("uneasy");
    expect(wordsFor("litUp", false)[11]?.word).toBe("thrilled");
    expect(wordsFor("low", true)[19]?.word).toBe("grieving");
    expect(wordsFor("settled", false).at(-1)?.word).toBe("at ease");
  });
});

describe("nearestWords", () => {
  it("puts the mild words first near the middle and the strong words first near a corner", () => {
    const mild = nearestWords("woundUp", { pleasant: -0.04, energy: 0.04 });
    expect(mild[0]?.word).toBe("uneasy");
    const strong = nearestWords("woundUp", { pleasant: -0.95, energy: 0.95 });
    expect(strong[0]?.word).toBe("panicked");
  });

  it("returns the 12 main words and none of the extra eight", () => {
    const words = nearestWords("settled", { pleasant: 0.5, energy: -0.5 });
    expect(words).toHaveLength(12);
    expect(new Set(words.map((w) => w.id))).toEqual(
      new Set(WORDS.settled.slice(0, 12).map((w) => w.id)),
    );
  });

  it("puts the words at the middle of the range first for a point halfway out", () => {
    const ranked = nearestWords("litUp", { pleasant: 0.5, energy: 0.2 }).map((w) => w.id);
    expect(ranked.slice(0, 2).sort()).toEqual(["playful", "proud"]);
  });
});
