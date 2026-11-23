import { describe, expect, it } from "vitest";
import { CHIPS, type ChipId } from "../components/ControlChip/registry";
import { enGB } from "./deck";
import { enGB as phoneDeck } from "./phone-deck";

const sources = import.meta.glob(
  ["../**/*.ts", "../**/*.tsx", "!../**/*.test.ts", "!../**/*.test.tsx"],
  { query: "?raw", import: "default", eager: true },
) as Record<string, string>;

const known = new Set(Object.keys(sources));

function resolve(from: string, spec: string): string | undefined {
  const parts = from.split("/").slice(0, -1);
  for (const part of spec.split("/")) {
    if (part === "..") parts.pop();
    else if (part !== ".") parts.push(part);
  }
  const base = parts.join("/");
  return [`${base}.ts`, `${base}.tsx`, `${base}/index.ts`, `${base}/index.tsx`, base].find((p) =>
    known.has(p),
  );
}

/** The source files the phone app reaches from its entry. */
function phoneFiles(): string[] {
  const seen = new Set<string>();
  const queue = ["../phone/main.tsx"];
  while (queue.length > 0) {
    const file = queue.pop() as string;
    if (seen.has(file) || !known.has(file)) continue;
    seen.add(file);
    const imports = (sources[file] ?? "").matchAll(/(?:from|import)\s*\(?\s*["'](\.[^"']*)["']/g);
    for (const [, spec] of imports) {
      const next = spec ? resolve(file, spec) : undefined;
      if (next) queue.push(next);
    }
  }
  return [...seen];
}

describe("the phone deck", () => {
  const files = phoneFiles();
  const text = files.map((f) => sources[f] ?? "").join("\n");

  it("finds the phone's own code", () => {
    expect(files.length).toBeGreaterThan(50);
    expect(files).toContain("../phone/App.tsx");
  });

  it("holds every id the phone's code names", () => {
    const missing = new Set<string>();
    for (const [, id = ""] of text.matchAll(/["'`]([A-Za-z][\w-]*(?:\.[\w-]+)+)["'`]/g)) {
      if (id in enGB && !(id in phoneDeck)) missing.add(id);
    }
    expect([...missing]).toEqual([]);
  });

  it("holds every id the phone builds from a prefix", () => {
    const missing = new Set<string>();
    for (const [, prefix = ""] of text.matchAll(/`([\w-]+(?:\.[\w-]+)*\.)\$\{/g)) {
      for (const id of Object.keys(enGB)) {
        if (id.startsWith(prefix) && !(id in phoneDeck)) missing.add(id);
      }
    }
    expect([...missing]).toEqual([]);
  });

  it("holds the words of every control a phone sentence names", () => {
    const missing = new Set<string>();
    for (const message of Object.values(phoneDeck)) {
      for (const [, name = ""] of message.matchAll(/\{(\w+)[,}]/g)) {
        const chip = CHIPS[name as ChipId];
        if (chip && !(chip.copy in phoneDeck)) missing.add(chip.copy);
      }
    }
    expect([...missing]).toEqual([]);
  });

  it("agrees with the full deck on every id it holds", () => {
    for (const [id, message] of Object.entries(phoneDeck)) {
      expect(enGB[id as keyof typeof enGB], id).toBe(message);
    }
  });
});
