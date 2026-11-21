import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { Receipt } from "../../engine/bindings";
import { Chips } from "./Receipts";

const receipt = (itemId: string, kind: Receipt["kind"]): Receipt => ({
  itemId,
  kind,
  source: "phone",
  label: itemId,
  keptAt: "2026-11-13T07:05:00Z",
  durationMs: null,
  span: null,
  unconfirmed: false,
  fromDevice: null,
});

const idle = { active: null, setActive: () => {}, register: () => () => {} };

describe("a recap's receipt chips", () => {
  it("open a reading on its day in Health and a note on its own page", () => {
    render(
      <Chips
        receipts={[receipt("night", "reading"), receipt("note", "text")]}
        receipt={idle as never}
      />,
    );
    const [night, note] = screen.getAllByRole("link");
    expect(night).toHaveAttribute("href", "#/journal/health?day=2026-11-13");
    expect(note).toHaveAttribute("href", "#/items/note");
  });
});
