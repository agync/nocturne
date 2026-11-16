import { describe, expect, it } from "vitest";
import guide from "../engine/mock/seed/guide.json";
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

/**
 * The guide help uses is part of the interface: every section in scope passes the same checks as
 * the interface's own strings (rule 4 of docs/design/in-app-help.md). The guide's file names and
 * commands are typed as they are and are not words of the interface.
 */
describe("the guide inside the app", () => {
  const sections = (guide.pages as Array<{ slug: string; sections: unknown[] }>).flatMap((page) =>
    page.sections.map((section) => {
      const s = section as { slug: string };
      const text: string[] = [];
      const walk = (node: unknown) => {
        if (Array.isArray(node)) node.forEach(walk);
        else if (node && typeof node === "object") {
          const o = node as Record<string, unknown>;
          if (o.type !== "code" && typeof o.text === "string") text.push(o.text);
          if (typeof o.heading === "string") text.push(o.heading);
          for (const [key, value] of Object.entries(o)) if (key !== "text") walk(value);
        }
      };
      walk(section);
      return [`${page.slug}#${s.slug}`, withoutAllowed(text.join(" "))] as const;
    }),
  );

  it("has sections to check", () => {
    expect(sections.length).toBeGreaterThan(20);
  });

  it("keeps the technical words and the never-say words out of every section", () => {
    for (const [id, text] of sections) {
      expect(JARGON.test(text), `${id}: ${text}`).toBe(false);
      for (const { never } of neverSay) {
        for (const word of never) {
          const re = new RegExp(`\\b${word}\\b`, word === "AI" ? "" : "i");
          expect(re.test(text), `${id} says "${word}"`).toBe(false);
        }
      }
      expect(text, id).not.toContain(EM_DASH);
    }
  });
});
