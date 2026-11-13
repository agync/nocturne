import { describe, expect, it, vi } from "vitest";
import { copyDeck, hasCopy, t } from "./t";

describe("t()", () => {
  it("formats an ICU message", () => {
    expect(t("capture.kept.toast", { kind: "Voice note", duration: "0:12" })).toBe(
      "Kept. Voice note, 0:12. Reading it now.",
    );
  });

  it("pluralises", () => {
    expect(t("page.more.changed", { count: 1 })).toBe("1 smaller change");
    expect(t("page.more.changed", { count: 2 })).toBe("2 smaller changes");
  });

  it("drops a count of nothing from a heading instead of writing 0", () => {
    expect(t("needs.eyebrow", { count: 0 })).toBe("Needs you");
    expect(t("needs.eyebrow", { count: 3 })).toBe("Needs you · 3");
    expect(t("page.section.needsYou", { count: 0 })).toBe("Needs you");
    expect(t("kept.heading", { count: 0 })).toBe("Kept today");
    expect(t("kept.heading", { count: 2 })).toBe("Kept today · 2");
  });

  it("throws on an unknown id under test rather than rendering the id", () => {
    // @ts-expect-error not a real id
    expect(() => t("nope.not.here")).toThrow(/Missing copy id/);
  });

  it("knows which ids exist", () => {
    expect(hasCopy("rail.today")).toBe(true);
    expect(hasCopy("rail.nowhere")).toBe(false);
  });

  it("logs and renders nothing for a missing id in production", async () => {
    vi.resetModules();
    vi.stubEnv("MODE", "production");
    const error = vi.spyOn(console, "error").mockImplementation(() => undefined);
    const prod = await import("./t");
    // @ts-expect-error not a real id
    expect(prod.t("nope.not.here")).toBe("");
    expect(error).toHaveBeenCalled();
    error.mockRestore();
    vi.unstubAllEnvs();
  });

  it("has no empty strings in the deck", () => {
    for (const [id, text] of Object.entries(copyDeck)) {
      expect(text.trim(), id).not.toBe("");
    }
  });
});
