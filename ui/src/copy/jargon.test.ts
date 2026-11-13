import { describe, expect, it } from "vitest";
import neverSay from "./never-say.fixture.json";
import { copyDeck } from "./t";

const EM_DASH = String.fromCharCode(0x2014);
const JARGON =
  /\b(model|LLM|inference|token|NPU|GPU|embedding|vector|graph|triple|provenance|quarantine|consolidation|backend|server|Lemonade)\b/i;

const interfaceCopy = Object.entries(copyDeck).filter(([id]) => !id.startsWith("advanced."));

describe("no jargon outside Advanced details", () => {
  it("keeps the technical words out of every other string", () => {
    for (const [id, text] of interfaceCopy) {
      expect(JARGON.test(text), `${id}: ${text}`).toBe(false);
    }
  });

  it("keeps every never-say term out of the interface", () => {
    for (const { never } of neverSay) {
      for (const word of never) {
        const re = new RegExp(`\\b${word}\\b`, word === "AI" ? "" : "i");
        for (const [id, text] of interfaceCopy) {
          expect(re.test(text), `${id} says "${word}": ${text}`).toBe(false);
        }
      }
    }
  });

  it("uses British spelling and no em dashes", () => {
    for (const [id, text] of Object.entries(copyDeck)) {
      expect(text, id).not.toContain(EM_DASH);
      expect(text, id).not.toMatch(/\b(color|organize|recognize|favorite|canceled)\b/i);
    }
  });
});
