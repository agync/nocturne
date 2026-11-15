import { describe, expect, it } from "vitest";
import { strengthOf } from "./strength";

const level = (passphrase: string) => strengthOf(passphrase).level;
const marks = (passphrase: string) => strengthOf(passphrase).marks;

describe("passphrase strength", () => {
  it("asks for eight characters first, with no marks", () => {
    for (const phrase of ["", "short", "1234567", "🌙🌙🌙🌙"]) {
      expect(strengthOf(phrase)).toMatchObject({
        level: "tooShort",
        marks: 0,
        copy: "backup.strength.tooShort",
      });
    }
  });

  it("counts characters, not UTF-16 units, so an emoji is one", () => {
    // Eight code points, even though a lone emoji takes two units.
    expect(level("🌙🌙🌙🌙abcd")).toBe("weak");
    expect(Array.from("🌙🌙🌙🌙").length).toBe(4);
  });

  it("calls a common passphrase weak", () => {
    for (const phrase of ["password", "12345678", "correct horse battery staple"]) {
      expect(strengthOf(phrase)).toMatchObject({
        level: "weak",
        marks: 1,
        copy: "backup.strength.weak",
      });
    }
  });

  it("calls one plain word weak however long it is", () => {
    expect(level("longwordhere")).toBe("weak");
    expect(level("Extraordinary")).toBe("weak");
  });

  it("calls anything under twelve characters weak", () => {
    expect(level("sunshine2")).toBe("weak");
    expect(level("x7#kQp2LmN")).toBe("weak");
  });

  it("is fair for a little length or a pair of words", () => {
    expect(level("lantern orchard")).toBe("fair");
    expect(level("abcdef123456")).toBe("fair");
    expect(marks("lantern orchard")).toBe(2);
  });

  it("is strong for a few words that do not go together", () => {
    expect(strengthOf("lantern-orchard-tuesday-wool")).toMatchObject({
      level: "strong",
      marks: 3,
      copy: "backup.strength.strong",
    });
    expect(level("lantern orchard tuesday")).toBe("strong");
    expect(level("a long random looking line")).toBe("strong");
  });

  it("is strong for one long mixed string", () => {
    expect(level("x7#kQp2LmN9aBcDe")).toBe("strong");
  });

  it("treats spaces as characters, and a whitespace-only passphrase as weak, not short", () => {
    expect(strengthOf("        ").level).toBe("weak");
    expect(level("  lantern orchard  ")).toBe("fair");
  });

  it("reads unicode words as words, without breaking on accents", () => {
    expect(level("café au lait")).toBe("fair");
    expect(level("déjà vu là-bas")).toBe("strong");
  });

  it("gives a copy id for every level", () => {
    expect(strengthOf("short").copy).toBe("backup.strength.tooShort");
    expect(strengthOf("sunshine2").copy).toBe("backup.strength.weak");
    expect(strengthOf("lantern orchard").copy).toBe("backup.strength.fair");
    expect(strengthOf("lantern orchard tuesday").copy).toBe("backup.strength.strong");
  });
});
