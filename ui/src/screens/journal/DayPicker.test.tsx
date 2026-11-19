import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import { createMockEngine } from "../../engine/mock/engine";
import { renderApp } from "../../test/render";

afterEach(() => {
  window.location.hash = "";
});

describe("choosing a day, with nothing kept", () => {
  it("says which kind of period has nothing yet", async () => {
    const engine = createMockEngine({
      now: new Date("2026-11-19T21:20:00Z"),
      journal: { mode: "day", delayMs: 0 },
    });
    engine.listPeriods = async () => [];
    renderApp({ hash: "#/journal", engine });
    await userEvent.click(await screen.findByRole("button", { name: "Choose a day" }));
    const dialog = await screen.findByRole("dialog", { name: "Choose a day" });
    const expected = {
      Weeks: "No weeks to look back on yet.",
      Months: "No months to look back on yet.",
      Years: "No years to look back on yet.",
    };
    for (const [tab, line] of Object.entries(expected)) {
      await userEvent.click(within(dialog).getByRole("tab", { name: tab }));
      expect(await within(dialog).findByText(line)).toBeInTheDocument();
    }
  });
});
