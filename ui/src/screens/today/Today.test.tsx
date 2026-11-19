import { act, cleanup, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createMockEngine } from "../../engine/mock/engine";
import { renderApp, setViewport } from "../../test/render";
import { patch } from "../settings/patch";

beforeEach(() => setViewport(1280));
afterEach(() => {
  vi.useRealTimers();
  window.location.hash = "";
});

/** Lets the "Marked as done" toast run out, which is when the press is committed. */
const toastEnds = () =>
  act(async () => {
    await vi.advanceTimersByTimeAsync(10_500);
  });

const open = async (engine = createMockEngine()) => {
  const view = renderApp({ hash: "#/today", engine });
  await screen.findByRole("heading", { level: 1, name: "Two things need you today." });
  return { ...view, engine };
};

describe("the morning page on Today", () => {
  it("reads like the design: headline, sections, receipts", async () => {
    await open();
    expect(screen.getAllByRole("heading", { level: 2 }).map((h) => h.textContent)).toContain(
      "Needs you · 2",
    );
    expect(screen.getByText("Is Leo still doing swimming with Coach Sam?")).toBeInTheDocument();
    expect(screen.getByText("Tidied 02:10–02:16")).toBeInTheDocument();
  });

  it("choosing the right dentist time resolves it, announces it and lowers the count everywhere", async () => {
    const { engine } = await open();
    const spy = vi.spyOn(engine, "resolveWhich");
    const page = await engine.getPage();
    const thursday = page.needsYou.find((l) => l.kind === "which")?.choices[1]?.text ?? "";
    await userEvent.click(screen.getByRole("button", { name: new RegExp(thursday) }));
    expect(spy).toHaveBeenCalledWith("conflict-dentist", "mem-dentist-thu");
    const status = (await screen.findAllByRole("status")).find((s) =>
      s.textContent?.includes("set to"),
    );
    expect(status).toHaveTextContent(`Dentist set to ${thursday}.`);
    expect(
      await screen.findByRole("heading", { level: 1, name: "One thing needs you today." }),
    ).toBeInTheDocument();
    const rail = screen.getByRole("navigation", { name: "Main" });
    await waitFor(() =>
      expect(within(rail).getByRole("link", { name: /Needs you/ })).toHaveTextContent("2"),
    );
  });

  it("marks the trip form done and strikes the line, committing when the toast ends", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const { engine } = await open();
    const spy = vi.spyOn(engine, "markDone");
    await userEvent.click(screen.getByRole("button", { name: "Mark as done" }));
    expect(spy).not.toHaveBeenCalled();
    expect(screen.getAllByText("Marked as done.").length).toBeGreaterThan(0);
    expect(screen.getByText(/The Year 4 trip form is due/).closest("p")).toHaveAttribute(
      "data-done",
      "true",
    );
    expect(screen.queryByRole("button", { name: "Mark as done" })).toBeNull();
    await toastEnds();
    expect(spy).toHaveBeenCalledWith("mem-trip");
    expect(spy).toHaveBeenCalledTimes(1);
  });

  it("Undo brings the line back and never commits", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const { engine } = await open();
    const spy = vi.spyOn(engine, "markDone");
    await userEvent.click(screen.getByRole("button", { name: "Mark as done" }));
    await userEvent.click(screen.getByRole("button", { name: "Undo" }));
    expect(screen.getByRole("button", { name: "Mark as done" })).toBeInTheDocument();
    expect(screen.queryByText("Marked as done.")).toBeNull();
    await toastEnds();
    expect(spy).not.toHaveBeenCalled();
  });

  it("commits a press that is still waiting when the screen goes away", async () => {
    const { engine, unmount } = await open();
    const spy = vi.spyOn(engine, "markDone");
    await userEvent.click(screen.getByRole("button", { name: "Mark as done" }));
    expect(spy).not.toHaveBeenCalled();
    unmount();
    expect(spy).toHaveBeenCalledWith("mem-trip");
  });

  it("says Nothing needs you once both are done and the press is committed", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const { engine } = await open();
    const page = await engine.getPage();
    const thursday = page.needsYou.find((l) => l.kind === "which")?.choices[1]?.text ?? "";
    await userEvent.click(screen.getByRole("button", { name: "Mark as done" }));
    await userEvent.click(screen.getByRole("button", { name: new RegExp(thursday) }));
    await toastEnds();
    expect(
      await screen.findByRole("heading", { level: 1, name: "Nothing needs you today." }),
    ).toBeInTheDocument();
  });

  it("brings back a let-go line by asking the engine to restore it", async () => {
    const { engine } = await open();
    const spy = vi.spyOn(engine, "restore");
    await userEvent.click(
      screen.getByRole("button", { name: "Bring back: The old gym door code" }),
    );
    expect(spy).toHaveBeenCalledWith(["mem-gym"]);
    expect(
      await screen.findByText("Brought back. It will not fade for 30 days."),
    ).toBeInTheDocument();
  });

  it("says a brought-back line will not fade for as long as the letting-go pace says", async () => {
    const engine = createMockEngine();
    await engine.updateSettings(patch({ lettingGo: "gentle" }));
    await open(engine);
    await userEvent.click(
      screen.getByRole("button", { name: "Bring back: The old gym door code" }),
    );
    expect(
      await screen.findByText("Brought back. It will not fade for 45 days."),
    ).toBeInTheDocument();
  });

  it("thanks the person for answering and records it", async () => {
    const { engine } = await open();
    const spy = vi.spyOn(engine, "answerQuestion");
    await userEvent.click(screen.getByRole("button", { name: "Yes" }));
    expect(spy).toHaveBeenCalledWith("2026-11-13", "yes");
    expect(await screen.findByText("Kept as sure.")).toBeInTheDocument();
    expect(screen.getByText("Thanks. Nocturne will ask again in a while.")).toBeInTheDocument();
  });

  it("puts the change back and says so when the engine refuses", async () => {
    const { engine } = await open();
    vi.useFakeTimers({ shouldAdvanceTime: true });
    vi.spyOn(engine, "markDone").mockRejectedValue(new Error("no"));
    await userEvent.click(screen.getByRole("button", { name: "Mark as done" }));
    await toastEnds();
    expect(
      await screen.findByText("That did not go through, so nothing was changed. Try again."),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Mark as done" })).toBeInTheDocument();
  });

  it("claims nothing was sent to the internet only when that is measured", async () => {
    const measured = createMockEngine();
    const real = measured.getStatus.bind(measured);
    measured.getStatus = async () => ({ ...(await real()), sentMeasured: true });
    const first = await open(measured);
    expect(await screen.findByText("Sent to the internet: nothing")).toBeInTheDocument();
    first.unmount();

    const unmeasured = createMockEngine();
    const real2 = unmeasured.getStatus.bind(unmeasured);
    unmeasured.getStatus = async () => ({ ...(await real2()), sentMeasured: false });
    await open(unmeasured);
    expect(
      await screen.findByText(/Nocturne goes online only for downloads you agree to\./),
    ).toBeInTheDocument();
    expect(screen.queryByText(/Sent to the internet: nothing/)).toBeNull();
  });

  it("opens an original from a receipt", async () => {
    await open();
    await userEvent.click(screen.getAllByRole("button", { name: /^Letter/ })[0] as HTMLElement);
    await waitFor(() => expect(window.location.hash).toBe("#/items/item-letter"));
  });

  it("moves to Listen and Print from the edition tabs", async () => {
    await open();
    await userEvent.click(screen.getByRole("tab", { name: "Listen" }));
    await waitFor(() => expect(window.location.hash).toBe("#/listen/2026-11-13"));
  });

  it("goes back a day from the page header", async () => {
    await open();
    await userEvent.click(screen.getByRole("link", { name: /Previous page/ }));
    await waitFor(() => expect(window.location.hash).toBe("#/pages/2026-11-12"));
    expect(
      await screen.findByRole("heading", { level: 1, name: "One thing needs you today." }),
    ).toBeInTheDocument();
  });

  it("lists what was kept today beside the page, with a word for each", async () => {
    await open();
    const kept = await screen.findByRole("complementary", { name: "Recently kept" });
    expect(
      await within(kept).findByRole("heading", { level: 2, name: "Kept today · 3" }),
    ).toBeInTheDocument();
    expect(within(kept).getByText("Photo · reading…")).toBeInTheDocument();
    expect(within(kept).getByText("Voice note · understood")).toBeInTheDocument();
    expect(within(kept).getByText("Pasted · understood")).toBeInTheDocument();
    expect(within(kept).getByText("“Ask Sam about Saturday”")).toBeInTheDocument();
    expect(within(kept).getByText(/Tonight, from about 01:00/)).toBeInTheDocument();
  });

  it("adds a note kept just now to the list at once", async () => {
    await open();
    const bar = screen.getByRole("textbox", { name: "Keep something, or ask" });
    await userEvent.type(bar, "Buy milk{Enter}");
    const kept = screen.getByRole("complementary", { name: "Recently kept" });
    expect(await within(kept).findByText("Buy milk")).toBeInTheDocument();
    expect(
      within(kept).getByRole("heading", { level: 2, name: "Kept today · 4" }),
    ).toBeInTheDocument();
  });

  it("says kindly when the page cannot be read", async () => {
    const engine = createMockEngine();
    engine.getPage = async () => {
      throw new Error("down");
    };
    renderApp({ hash: "#/today", engine });
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Nocturne couldn’t get this page. What you kept is safe.",
    );
  });

  describe("Not true, delete it", () => {
    /** The gran line waits to be confirmed, so the page offers the choice. */
    const unconfirmedGran = (memoryIds?: string[]) => {
      const engine = createMockEngine();
      const real = engine.getPage.bind(engine);
      vi.spyOn(engine, "getPage").mockImplementation(async (date) => {
        const page = await real(date);
        return {
          ...page,
          changed: page.changed.map((l) =>
            l.id === "line-gran" && !l.forgotten
              ? { ...l, kind: "unconfirmed" as const, memoryIds: memoryIds ?? l.memoryIds }
              : l,
          ),
        };
      });
      return engine;
    };

    it("opens the forget dialog and only says Deleted once the engine has forgotten", async () => {
      const engine = unconfirmedGran();
      const confirm = vi.spyOn(engine, "confirmMemory");
      const forget = vi.spyOn(engine, "forget");
      await open(engine);
      await userEvent.click(screen.getByRole("button", { name: "Not true, delete it…" }));
      const dialog = await screen.findByRole("dialog", { name: "Delete 1 memory?" });
      expect(screen.queryByText(/^Forgotten\./)).toBeNull();
      expect(confirm).not.toHaveBeenCalled();
      await userEvent.click(await within(dialog).findByRole("button", { name: "Delete 1 memory" }));
      expect(await within(dialog).findByText("Deleted.")).toBeInTheDocument();
      expect(forget).toHaveBeenCalledOnce();
      await userEvent.click(within(dialog).getByRole("button", { name: "OK" }));
      const gone = await screen.findByText("Deleted. This line is gone from the page.");
      await waitFor(() => expect(gone).toHaveFocus());
      expect(document.body.textContent).not.toContain("4471");
    });

    it("forgets every memory behind the line, not only the first", async () => {
      const engine = unconfirmedGran(["mem-gran", "mem-swim"]);
      const preview = vi.spyOn(engine, "previewForget");
      await open(engine);
      await userEvent.click(screen.getByRole("button", { name: "Not true, delete it…" }));
      await screen.findByRole("dialog", { name: "Delete 2 memories?" });
      expect(preview.mock.calls[0]?.[0]).toEqual(["mem-gran", "mem-swim"]);
    });

    it("leaves the line alone when the person keeps it", async () => {
      const engine = unconfirmedGran();
      const forget = vi.spyOn(engine, "forget");
      await open(engine);
      await userEvent.click(screen.getByRole("button", { name: "Not true, delete it…" }));
      await userEvent.click(await screen.findByRole("button", { name: "Keep it" }));
      expect(forget).not.toHaveBeenCalled();
      expect(screen.queryByRole("dialog")).toBeNull();
      expect(screen.getByRole("button", { name: "Not true, delete it…" })).toBeInTheDocument();
    });
  });

  it("shows a forgotten line as redacted", async () => {
    const engine = createMockEngine();
    const preview = await engine.previewForget(["mem-gran"], true);
    await engine.forget(preview.token);
    await open(engine);
    expect(
      await screen.findByText("Deleted. This line is gone from the page."),
    ).toBeInTheDocument();
    expect(document.body.textContent).not.toContain("4471");
  });

  it("gives way to fresh data once it has arrived instead of keeping its own guess", async () => {
    const { engine } = await open();
    // The engine accepts the change but its page still says the line is open.
    vi.useFakeTimers({ shouldAdvanceTime: true });
    vi.spyOn(engine, "markDone").mockResolvedValue(undefined);
    await userEvent.click(screen.getByRole("button", { name: "Mark as done" }));
    await toastEnds();
    expect(await screen.findByRole("button", { name: "Mark as done" })).toBeInTheDocument();
  });

  it("says a past page was still being written, with no live progress", async () => {
    renderApp({ hash: "#/pages/2026-11-06" });
    expect(await screen.findByText(/This page was still being written\./)).toBeInTheDocument();
    expect(screen.queryByRole("progressbar")).toBeNull();
  });

  it("does not let a line with nothing behind it pretend to succeed", async () => {
    renderApp({ hash: "#/pages/2026-11-04" });
    await screen.findByRole("heading", {
      level: 1,
      name: "Seven things need you. Three are today.",
    });
    const buttons = screen.getAllByRole("button", { name: "Mark as done" });
    expect(buttons.some((b) => (b as HTMLButtonElement).disabled)).toBe(true);
  });
});

describe("the quiet line to a look back", () => {
  const withLine = async (period: {
    kind: "day" | "week" | "month" | "year";
    start: string;
    end: string;
  }) => {
    const engine = createMockEngine();
    const original = engine.getPage.bind(engine);
    vi.spyOn(engine, "getPage").mockImplementation(async (date) => ({
      ...(await original(date)),
      journalLine: period,
    }));
    return open(engine);
  };

  it("says Yesterday for a day and opens its look back", async () => {
    await withLine({ kind: "day", start: "2026-11-18", end: "2026-11-18" });
    const link = screen.getByRole("link", { name: "Yesterday, looked back on ›" });
    expect(link).toHaveAttribute("href", "#/journal/look-back/day/2026-11-18");
  });

  it("says Last week for a week", async () => {
    await withLine({ kind: "week", start: "2026-11-09", end: "2026-11-15" });
    expect(screen.getByRole("link", { name: "Last week, looked back on ›" })).toHaveAttribute(
      "href",
      "#/journal/look-back/week/2026-11-09",
    );
  });

  it("names a month and a year", async () => {
    await withLine({ kind: "month", start: "2026-10-01", end: "2026-10-31" });
    expect(screen.getByRole("link", { name: "October, looked back on ›" })).toBeVisible();
    cleanup();
    await withLine({ kind: "year", start: "2025-01-01", end: "2025-12-31" });
    expect(screen.getByRole("link", { name: "2025, looked back on ›" })).toBeVisible();
  });

  it("is not there when the night wrote nothing", async () => {
    await open();
    expect(screen.queryByText(/looked back on/)).toBeNull();
  });
});
