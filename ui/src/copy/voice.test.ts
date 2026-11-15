import { describe, expect, it } from "vitest";
import { copyDeck } from "./t";

/** Checkboxes the person ticks are in the person's own words, not Nocturne speaking. */
const PERSONAL = new Set(["backup.note.check"]);

/**
 * Nocturne speaks as "Nocturne", or says nothing about itself: "Nocturne couldn't open this",
 * "Settings didn't open". It never says "I", so one voice runs through every screen.
 */
describe("the voice of the interface", () => {
  it("never speaks as I", () => {
    for (const [id, text] of Object.entries(copyDeck)) {
      if (PERSONAL.has(id)) continue;
      expect(text, id).not.toMatch(/\bI\b|\bI[’']\w+/);
    }
  });
});
