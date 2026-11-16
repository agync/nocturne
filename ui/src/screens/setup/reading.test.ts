import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const css = readFileSync(resolve(__dirname, "Setup.module.css"), "utf8");

/** The body of the first rule whose selector is exactly `selector`. */
const rule = (selector: string) => {
  const match = new RegExp(`(?:^|\\n)${selector.replace(/[.]/g, "\\.")}\\s*\\{([^}]*)\\}`).exec(
    css,
  );
  expect(match, `${selector} has a rule`).not.toBeNull();
  return match?.[1] ?? "";
};

describe("reading rules for the setup card", () => {
  it("caps running text at 60 characters a line", () => {
    expect(rule(".card p")).toMatch(/max-width:\s*60ch/);
    expect(rule(".later p")).toMatch(/max-width:\s*60ch/);
  });

  it("sets running text at a line height of at least 1.5", () => {
    expect(rule(".card")).toMatch(/line-height:\s*1\.5/);
    expect(rule(".later p")).toMatch(/line-height:\s*1\.5/);
    const detail = /font:\s*\d+\s+[\d.]+rem\s*\/\s*([\d.]+)/.exec(rule(".tierDetail"));
    expect(Number(detail?.[1])).toBeGreaterThanOrEqual(1.5);
  });

  it("sets what informs a choice at 15px or more", () => {
    const size = /font:\s*\d+\s+([\d.]+)rem/.exec(rule(".tierDetail"));
    expect(Number(size?.[1]) * 16).toBeGreaterThanOrEqual(15);
  });
});
