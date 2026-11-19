import { describe, expect, it } from "vitest";
import { enGB } from "./deck";

const files = import.meta.glob<Record<string, string>>("./en-GB/*.json", {
  eager: true,
  import: "default",
});

describe("the copy deck is split by area", () => {
  it("has one file per area and every file is part of the deck", () => {
    expect(
      Object.keys(files)
        .map((f) => f.replace("./en-GB/", "").replace(".json", ""))
        .sort(),
    ).toEqual([
      "advanced",
      "ask",
      "away",
      "backup",
      "capture",
      "common",
      "editions",
      "fading",
      "feelings",
      "guide",
      "journal",
      "memories",
      "models",
      "needs-you",
      "notice",
      "online",
      "page",
      "phone",
      "privacy",
      "recent",
      "sent",
      "settings",
      "setup",
      "shell",
    ]);
    const total = Object.values(files).reduce((n, f) => n + Object.keys(f).length, 0);
    expect(Object.keys(enGB).length).toBe(total);
  });

  it("never defines an id in two files", () => {
    const seen = new Map<string, string>();
    const duplicates: string[] = [];
    for (const [file, deck] of Object.entries(files)) {
      for (const id of Object.keys(deck)) {
        const first = seen.get(id);
        if (first) duplicates.push(`${id} in ${first} and ${file}`);
        seen.set(id, file);
      }
    }
    expect(duplicates).toEqual([]);
  });
});
