import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

const sheets = (dir: string): Array<[string, string]> =>
  readdirSync(dir)
    .filter((name) => name.endsWith(".css"))
    .map((name) => [name, readFileSync(join(dir, name), "utf8")]);

const sources = import.meta.glob(["./*.tsx", "./*.ts", "!./*.test.ts", "!./*.test.tsx"], {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

const charts: Array<[string, string]> = [...Object.entries(sources), ...sheets(__dirname)];
const screens = sheets(join(__dirname, "../../screens/health"));

const COLOUR_RULES: ReadonlyArray<[string, RegExp]> = [
  ["the ember token", /--ember/],
  ["a part token", /--part-/],
  ["a good or bad colour word", /\b(red|amber|green|warning|danger)\b/i],
  ["a hex colour", /#[0-9a-f]{3,8}\b/i],
  ["an rgb colour", /rgba?\(/i],
];

const CHART_WORDS = /\b(target|goal|normal|score|streak)s?\b/i;

describe("the forms keep to one colour and no verdicts", () => {
  it("finds the sources it scans", () => {
    expect(charts.length).toBeGreaterThan(10);
    expect(screens.length).toBeGreaterThan(0);
  });

  it.each([...charts, ...screens])("%s names no colour but the tokens", (file, source) => {
    if (file.endsWith(".css")) expect(source).toContain("var(--");
    for (const [what, rule] of COLOUR_RULES) {
      if (/\.(tsx|ts)$/.test(file) && what === "a hex colour") continue;
      expect(source.match(rule), `${file} mentions ${what}`).toBeNull();
    }
  });

  it.each(charts)("%s has no target, goal, normal, score or streak", (file, source) => {
    expect(source.match(CHART_WORDS), `${file} mentions a verdict word`).toBeNull();
  });
});
