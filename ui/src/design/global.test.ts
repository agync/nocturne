// @vitest-environment node
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const dir = dirname(fileURLToPath(import.meta.url));
const global = readFileSync(join(dir, "global.css"), "utf8");
const fonts = readFileSync(join(dir, "fonts.css"), "utf8");

describe("global styles", () => {
  it("draws the focus ring as a 2 px outline offset by 2", () => {
    expect(global).toMatch(
      /:focus-visible\s*\{[^}]*outline:\s*2px solid var\(--focus\);[^}]*outline-offset:\s*2px/,
    );
  });

  it("zeroes durations under reduced motion", () => {
    expect(global).toMatch(
      /prefers-reduced-motion: reduce[\s\S]*animation-duration:\s*0s !important/,
    );
    expect(global).toMatch(/transition-duration:\s*0s !important/);
  });

  it("defines the breathe keyframes", () => {
    expect(global).toMatch(/@keyframes breathe/);
  });

  it("bundles every font file locally and loads nothing remote", () => {
    expect(fonts).not.toMatch(/https?:/);
    for (const family of ["Newsreader", "IBM Plex Sans", "IBM Plex Mono"]) {
      expect(fonts).toContain(`font-family: "${family}"`);
    }
    expect(fonts).toContain("opsz-italic.woff2");
  });
});
