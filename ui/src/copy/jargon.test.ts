import { describe, expect, it } from "vitest";
import neverSay from "./never-say.fixture.json";
import { copyDeck } from "./t";

const EM_DASH = String.fromCharCode(0x2014);
const JARGON =
  /\b(model|LLM|inference|token|NPU|GPU|embedding|vector|graph|triple|provenance|quarantine|consolidation|backend|server|Lemonade)\b/i;

/**
 * Whole phrases the approved designs use on purpose (the bracketed words of the Online answers
 * form, and "your provider (host)"). Each is taken out of a string before the checks below, and
 * only where it stands whole: "API keys" or "the provider" are still caught.
 */
export const ALLOWED_PHRASES = ["API URL", "API key", "model name", "your provider"] as const;

const literal = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
const ALLOWED = new RegExp(`\\b(?:${ALLOWED_PHRASES.map(literal).join("|")})\\b`, "g");

export function withoutAllowed(text: string): string {
  return text.replace(ALLOWED, " ");
}

const interfaceCopy = Object.entries(copyDeck)
  .filter(([id]) => !id.startsWith("advanced."))
  .map(([id, text]) => [id, withoutAllowed(text)] as const);

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

describe("the allowed phrases", () => {
  it("are exactly the four the designs use", () => {
    expect([...ALLOWED_PHRASES]).toEqual(["API URL", "API key", "model name", "your provider"]);
  });

  it("are allowed only whole", () => {
    expect(withoutAllowed("Paste the API key here")).not.toContain("API");
    expect(withoutAllowed("your provider (api.example.com)")).not.toMatch(/provider/);
    expect(JARGON.test(withoutAllowed("Type the model name"))).toBe(false);
    expect(withoutAllowed("Your API keys")).toContain("API keys");
    expect(withoutAllowed("Ask the provider")).toContain("provider");
    expect(JARGON.test(withoutAllowed("Pick a model"))).toBe(true);
  });
});
