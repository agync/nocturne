import { describe, expect, it } from "vitest";
import { copyDeck } from "./t";

const sources = import.meta.glob(
  ["../**/*.ts", "../**/*.tsx", "!../**/*.test.ts", "!../**/*.test.tsx"],
  {
    query: "?raw",
    import: "default",
    eager: true,
  },
) as Record<string, string>;

/** Keys that code builds at run time, so no literal names them. Each pattern says why. */
const BUILT_AT_RUN_TIME: Array<{ pattern: RegExp; why: string }> = [
  { pattern: /^kind\.[a-z]+(\.spoken)?$/, why: "t(`kind.${kind}`) and t(`kind.${kind}.spoken`)" },
  { pattern: /^mark\.[a-zA-Z]+$/, why: "t(`mark.${state}`)" },
  { pattern: /^certainty\.[a-zA-Z]+(\.hint)?$/, why: "t(`certainty.${level}`)" },
  { pattern: /^recent\.reason\.[a-zA-Z]+$/, why: "t(`recent.reason.${reason}`)" },
  { pattern: /^listen\.section\.[a-zA-Z]+$/, why: "t(`listen.section.${kind}`)" },
  { pattern: /^print\.size\.[a-z0-9]+$/, why: "t(`print.size.${size}`)" },
  { pattern: /^status\.state\.[a-zA-Z]+$/, why: "t(`status.state.${state}`)" },
];

const text = Object.values(sources).join("\n");

const named = (key: string) =>
  ['"', "'", "`"].some((quote) => text.includes(`${quote}${key}${quote}`));

describe("the copy deck", () => {
  it("has no key that no code uses", () => {
    const unused = Object.keys(copyDeck).filter(
      (key) => !named(key) && !BUILT_AT_RUN_TIME.some((rule) => rule.pattern.test(key)),
    );
    expect(unused, "delete these keys, or name the pattern that builds them").toEqual([]);
  });

  it("keeps every run-time pattern honest", () => {
    for (const { pattern, why } of BUILT_AT_RUN_TIME) {
      const keys = Object.keys(copyDeck).filter((key) => pattern.test(key));
      expect(keys.length, `${pattern} (${why}) matches no key`).toBeGreaterThan(0);
    }
  });
});
