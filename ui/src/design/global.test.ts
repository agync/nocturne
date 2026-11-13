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

  it("keeps the focus ring off programmatically focused headings", () => {
    expect(global).toMatch(/:is\(h1, h2\)\[tabindex="-1"\]:focus-visible\s*\{\s*outline:\s*none;/);
  });

  it("offers a 44 px hit area utility", () => {
    expect(global).toMatch(/\.hit\s*\{\s*position:\s*relative;/);
    expect(global).toMatch(/\.hit::after\s*\{[^}]*max\(100%, var\(--size-touch\)\)/);
  });

  it("grows controls and titles when narrow", () => {
    const block = global.match(/@media \(max-width: 44\.99em\)\s*\{([\s\S]*?)\n\}\n/)?.[1] ?? "";
    expect(block).toMatch(/--size-control:\s*var\(--size-touch\)/);
    expect(block).toMatch(/--toast-bottom:\s*5\.5rem/);
    expect(block).toMatch(/\.t-page-title\s*\{\s*font-size:\s*2rem/);
    expect(block).toMatch(/\.t-display\s*\{\s*font-size:\s*2\.5rem/);
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
