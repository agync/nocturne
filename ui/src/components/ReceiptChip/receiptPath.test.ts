import { describe, expect, it } from "vitest";
import type { Receipt } from "../../engine/bindings";
import { receiptPath } from "./receiptPath";

const receipt = (kind: Receipt["kind"]): Receipt => ({
  itemId: "i1",
  kind,
  source: "phone",
  label: "x",
  keptAt: "2026-11-13T07:05:00Z",
  durationMs: null,
  span: null,
  unconfirmed: false,
  fromDevice: null,
});

describe("where a receipt leads", () => {
  it("opens an item's own page", () => {
    expect(receiptPath(receipt("text"))).toBe("/items/i1");
  });

  it("opens a health reading on its Health page, for the day it was taken", () => {
    expect(receiptPath(receipt("reading"))).toBe("/journal/health?day=2026-11-13");
  });

  it("remembers which screen sent the person to a calendar entry", () => {
    expect(receiptPath(receipt("calendar"), "today")).toBe("/items/i1?from=today");
    expect(receiptPath(receipt("calendar"))).toBe("/items/i1");
  });
});
