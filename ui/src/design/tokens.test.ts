// @vitest-environment node
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { generate, readTokens } from "../../scripts/build-tokens.mjs";

const tokens = readTokens();
const css = readFileSync(join(dirname(fileURLToPath(import.meta.url)), "tokens.css"), "utf8");

function luminance(hex: string): number {
  const n = Number.parseInt(hex.slice(1), 16);
  const [r, g, b] = [(n >> 16) & 255, (n >> 8) & 255, n & 255].map((v) => {
    const c = v / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * (r ?? 0) + 0.7152 * (g ?? 0) + 0.0722 * (b ?? 0);
}

function contrast(a: string, b: string): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return ((hi ?? 0) + 0.05) / ((lo ?? 0) + 0.05);
}

const themes = ["light", "dark"] as const;

describe("generated tokens", () => {
  it("defines the colour roles for both themes and the system preference", () => {
    for (const role of ["sheet", "ink", "ink3", "accent", "ember", "marker", "focus"]) {
      expect(css, role).toMatch(new RegExp(`:root\\[data-theme="light"\\][^}]*--${role}:`));
      expect(css, role).toMatch(new RegExp(`:root\\[data-theme="dark"\\] \\{[^}]*--${role}:`));
      expect(css, role).toMatch(
        new RegExp(
          `@media \\(prefers-color-scheme: dark\\) \\{\\s*:root:not\\(\\[data-theme\\]\\) \\{[^}]*--${role}:`,
        ),
      );
    }
  });

  it("is not stale", () => {
    expect(css).toBe(generate(tokens));
  });

  it("exposes space, radius, motion, size and type", () => {
    expect(css).toContain("--s-1: 4px;");
    expect(css).toContain("--s-9: 96px;");
    expect(css).toContain("--r-lg: 12px;");
    expect(css).toContain("--m-act-duration: 120ms;");
    expect(css).toContain("--size-touch: 2.75rem;");
    expect(css).toContain("--size-control: 2.5rem;");
    expect(css).toContain(".t-page-line {");
    expect(css).toContain("--size-gutter: 28px;");
    expect(css).toContain(".t-item-title {");
    expect(css).toContain(".t-receipt {");
    expect(css).toContain("@media print");
  });
});

describe("contrast rules (B6)", () => {
  const text: Array<[string, string]> = [
    ["ink", "sheet"],
    ["ink2", "sheet"],
    ["ink3", "sheet"],
    ["accent", "sheet"],
    ["ember", "sheet"],
    ["ink", "bg"],
    ["ink2", "bg"],
    ["ink3", "bg"],
    ["accent", "bg"],
    ["ember", "bg"],
    ["ink", "raised"],
    ["ink2", "raised"],
    ["ink", "marker"],
    ["ink", "accentSoft"],
    ["ink", "emberSoft"],
    ["onAccent", "accent"],
    ["ember", "emberSoft"],
  ];
  const edges: Array<[string, string]> = [
    ["rule2", "sheet"],
    ["rule2", "raised"],
    ["ink3", "sheet"],
    ["ink3", "raised"],
  ];

  for (const theme of themes) {
    const palette = tokens.color[theme];
    const at = (k: string) => palette[k] as string;

    it(`${theme}: every text pair is at least 4.5`, () => {
      for (const [fg, bg] of text) {
        expect(contrast(at(fg), at(bg)), `${fg} on ${bg}`).toBeGreaterThanOrEqual(4.5);
      }
    });

    it(`${theme}: every control edge is at least 3`, () => {
      for (const [edge, bg] of edges) {
        expect(contrast(at(edge), at(bg)), `${edge} on ${bg}`).toBeGreaterThanOrEqual(3);
      }
    });
  }
});
