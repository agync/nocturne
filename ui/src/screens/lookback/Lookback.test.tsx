import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { Period, ReflectionView } from "../../engine/bindings";
import { createMockEngine, type MockEngine } from "../../engine/mock/engine";
import type { JournalMode } from "../../engine/mock/seed/journal";
import { renderApp } from "../../test/render";
import { recapPeriodText } from "./DeleteDialog";

const NOW = new Date("2026-11-19T21:20:00Z");

function engineIn(mode: JournalMode = "day"): MockEngine {
  return createMockEngine({ now: NOW, journal: { mode, delayMs: 0 } });
}

/** The same engine, with one view served in place of what it holds. */
function serve(engine: MockEngine, change: (view: ReflectionView) => ReflectionView) {
  const real = engine.getReflection.bind(engine);
  engine.getReflection = async (period: Period) => change(await real(period));
}

const open = (hash: string, engine: MockEngine = engineIn()) => renderApp({ hash, engine });

afterEach(() => {
  window.location.hash = "";
});

describe("a day's reflection", () => {
  it("says it was looked back on, and that it was written overnight", async () => {
    open("#/journal/look-back/day/2026-11-18");
    expect(await screen.findByText("Wednesday 18 November · Looked back on")).toBeTruthy();
    expect(screen.getByRole("heading", { level: 1, name: "Wednesday 18 November" })).toBeTruthy();
    expect(
      screen.getByText(
        /^Written overnight on this computer, from \d+ things? you kept that day\.$/,
      ),
    ).toBeTruthy();
    expect(screen.queryByText(/so far/i)).toBeNull();
  });

  it("opens the same card for a name as the day does", async () => {
    open("#/journal/look-back/day/2026-11-19");
    const people = await screen.findByRole("heading", { name: "People and places" });
    const section = people.closest("section") as HTMLElement;
    const priya = await within(section).findByRole("button", { name: /Priya/ });
    fireEvent.click(priya);
    expect(await screen.findByText("Nocturne thinks Priya is someone new.")).toBeTruthy();
  });

  it("labels today's reflection so far, with the time it was written", async () => {
    open("#/journal/look-back/day/2026-11-19");
    expect(await screen.findByText("Today so far · at 3:15 pm")).toBeTruthy();
    expect(
      screen.getByText(/^Written on this computer at 3:15 pm, from 6 things you kept\.$/),
    ).toBeTruthy();
  });

  it("shows the sections in order, with a link back to the journal day", async () => {
    open("#/journal/look-back/day/2026-11-19");
    await screen.findByRole("heading", { level: 2, name: "The day" });
    const headings = screen.getAllByRole("heading", { level: 2 }).map((h) => h.textContent);
    expect(headings).toEqual([
      "The day",
      "How it felt",
      "Good things",
      "People and places",
      "Still open",
    ]);
    const back = screen
      .getAllByRole("link", { name: "Journal" })
      .map((a) => a.getAttribute("href"));
    expect(back).toContain("#/journal/2026-11-19");
    expect(screen.getByRole("link", { name: "In Needs you ›" }).getAttribute("href")).toBe(
      "#/needs-you",
    );
  });

  it("makes each bold span a control that outlines its receipt chip", async () => {
    open("#/journal/look-back/day/2026-11-19");
    const span = await screen.findByRole("button", { name: "saw Priya" });
    const chips = screen.getByRole("list", { name: "Where this came from" });
    expect(within(chips).getAllByRole("link").length).toBeGreaterThan(0);
    expect(chips.querySelector('[data-on="true"]')).toBeNull();
    fireEvent.focus(span);
    expect(chips.querySelector('[data-on="true"]')).not.toBeNull();
    fireEvent.blur(span);
    expect(chips.querySelector('[data-on="true"]')).toBeNull();
  });

  it("writes one sentence for each check-in, and lists them under Show as a table", async () => {
    open("#/journal/look-back/day/2026-11-19");
    await screen.findByText(/you checked in as/);
    expect(screen.getByRole("link", { name: "tired" }).getAttribute("href")).toMatch(/^#\/items\//);
    expect(document.body.textContent).toContain("At 8:05 am you checked in as tired: “Bad night.”");
    fireEvent.click(screen.getByRole("button", { name: "Show as a table" }));
    const table = await screen.findByRole("table");
    const rows = within(table).getAllByRole("row");
    expect(rows).toHaveLength(4);
    expect(within(rows[1] as HTMLElement).getByText("8:05 am")).toBeTruthy();
    expect(within(rows[1] as HTMLElement).getByText("Low")).toBeTruthy();
    expect(within(rows[1] as HTMLElement).getByText("tired")).toBeTruthy();
  });

  it("shows a line with asterisks as it was written", async () => {
    const engine = engineIn();
    const line = "a **big** day, **really";
    serve(engine, (view) =>
      view.type === "written"
        ? {
            ...view,
            reflection: {
              ...view.reflection,
              felt: view.reflection.felt.map((c, at) => (at === 0 ? { ...c, line } : c)),
            },
          }
        : view,
    );
    open("#/journal/look-back/day/2026-11-19", engine);
    const sentence = (await screen.findByText(/you checked in as/, {
      selector: "p",
    })) as HTMLElement;
    expect(sentence.textContent).toContain(`: “${line}”`);
    expect(
      within(sentence)
        .getAllByRole("link")
        .map((a) => a.textContent),
    ).toEqual(["tired", "pleased", "relieved and content"]);
  });

  it("says only that the day was not described when there are no check-ins", async () => {
    const engine = engineIn();
    serve(engine, (view) =>
      view.type === "written" ? { ...view, reflection: { ...view.reflection, felt: [] } } : view,
    );
    open("#/journal/look-back/day/2026-11-19", engine);
    expect(await screen.findByText("You didn’t say how today felt.")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Show as a table" })).toBeNull();
  });

  it("says how many things are still being read, and offers to write it again when they are", async () => {
    const engine = engineIn();
    serve(engine, (view) =>
      view.type === "written"
        ? { ...view, reflection: { ...view.reflection, stillReading: 2 } }
        : view,
    );
    const reflect = vi.spyOn(engine, "reflect");
    open("#/journal/look-back/day/2026-11-19", engine);
    expect(
      await screen.findByText(
        "2 things from today are still being read, so they aren’t in this yet.",
      ),
    ).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Write it again when they’re read" }));
    await waitFor(() => expect(reflect).toHaveBeenCalledTimes(1));
  });

  describe("writing it again when the rest is read", () => {
    afterEach(() => vi.useRealTimers());

    const pending = async () => {
      const engine = engineIn();
      serve(engine, (view) =>
        view.type === "written"
          ? { ...view, reflection: { ...view.reflection, stillReading: 2 } }
          : view,
      );
      const state = { reading: true };
      const real = engine.getJournalDay.bind(engine);
      vi.spyOn(engine, "getJournalDay").mockImplementation(async (day) => {
        const view = await real(day);
        if (!state.reading) return view;
        return {
          ...view,
          rows: view.rows.map((row, at) =>
            at === 0 ? { ...row, item: { ...row.item, status: "reading" as const } } : row,
          ),
        };
      });
      const reflect = vi.spyOn(engine, "reflect");
      vi.useFakeTimers({ shouldAdvanceTime: true });
      const view = open("#/journal/look-back/day/2026-11-19", engine);
      const button = await screen.findByRole("button", {
        name: "Write it again when they’re read",
      });
      return { state, reflect, view, button };
    };

    it("waits until nothing from the day is being read, then writes", async () => {
      const { state, reflect, button } = await pending();
      fireEvent.click(button);
      await vi.advanceTimersByTimeAsync(20000);
      expect(reflect).not.toHaveBeenCalled();
      expect(button).toBeDisabled();
      state.reading = false;
      await vi.advanceTimersByTimeAsync(10000);
      expect(reflect).toHaveBeenCalledTimes(1);
    });

    it("stops waiting when the screen is left", async () => {
      const { state, reflect, view, button } = await pending();
      fireEvent.click(button);
      await vi.advanceTimersByTimeAsync(4000);
      view.unmount();
      state.reading = false;
      await vi.advanceTimersByTimeAsync(20000);
      expect(reflect).not.toHaveBeenCalled();
    });
  });

  it("says a line was taken out when what it came from was deleted", async () => {
    const engine = engineIn();
    serve(engine, (view) =>
      view.type === "written"
        ? { ...view, reflection: { ...view.reflection, linesRemoved: 1 } }
        : view,
    );
    open("#/journal/look-back/day/2026-11-19", engine);
    expect(
      await screen.findByText("One line was taken out because what it came from was deleted."),
    ).toBeTruthy();
    expect(screen.getAllByRole("button", { name: "Write it again" }).length).toBeGreaterThan(1);
  });

  it("prints through the platform and offers no Listen", async () => {
    const { platform } = open("#/journal/look-back/day/2026-11-19");
    fireEvent.click(await screen.findByRole("button", { name: "Print" }));
    await waitFor(() => expect((platform as unknown as { printed(): number }).printed()).toBe(1));
    expect(screen.queryByText(/listen/i)).toBeNull();
  });

  it("deletes only the writing and says the entries are untouched", async () => {
    const engine = engineIn();
    const remove = vi.spyOn(engine, "deleteReflection");
    open("#/journal/look-back/day/2026-11-19", engine);
    fireEvent.click(await screen.findByRole("button", { name: "Delete this reflection" }));
    const dialog = await screen.findByRole("dialog", { name: "Delete this reflection?" });
    fireEvent.click(within(dialog).getByRole("button", { name: "Delete reflection" }));
    await waitFor(() => expect(remove).toHaveBeenCalledTimes(1));
    expect(
      await screen.findByText("Deleted. Your entries are untouched. You can write it again."),
    ).toBeTruthy();
    expect(await screen.findByRole("button", { name: "Reflect on today" })).toBeTruthy();
  });

  it("confirms first, with focus on Keep it, and deletes nothing when kept", async () => {
    const engine = engineIn();
    const remove = vi.spyOn(engine, "deleteReflection");
    open("#/journal/look-back/day/2026-11-19", engine);
    fireEvent.click(await screen.findByRole("button", { name: "Delete this reflection" }));
    const dialog = await screen.findByRole("dialog", { name: "Delete this reflection?" });
    expect(
      within(dialog).getByText(
        "Only the writing goes. Everything you kept on Thursday 19 November stays: your entry, check-ins, good things, notes and photos.",
      ),
    ).toBeTruthy();
    expect(within(dialog).getByText("Nocturne won’t write it again unless you ask.")).toBeTruthy();
    expect(within(dialog).getByRole("button", { name: "Keep it" })).toHaveFocus();
    fireEvent.click(within(dialog).getByRole("button", { name: "Keep it" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(remove).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "Delete this reflection" })).toBeTruthy();
  });

  it("says so and keeps the reflection when it cannot be deleted", async () => {
    const engine = engineIn();
    vi.spyOn(engine, "deleteReflection").mockRejectedValue(new Error("no"));
    open("#/journal/look-back/day/2026-11-19", engine);
    fireEvent.click(await screen.findByRole("button", { name: "Delete this reflection" }));
    const dialog = await screen.findByRole("dialog", { name: "Delete this reflection?" });
    fireEvent.click(within(dialog).getByRole("button", { name: "Delete reflection" }));
    expect(await screen.findByText("That didn’t work. Nothing was changed.")).toBeTruthy();
    expect(screen.queryByText(/^Deleted\./)).toBeNull();
    expect(screen.getByRole("button", { name: "Delete this reflection" })).toBeTruthy();
  });
});

describe("writing it", () => {
  it("writes once on arrival with write=1, then drops the parameter", async () => {
    const engine = engineIn();
    const reflect = vi.spyOn(engine, "reflect");
    open("#/journal/look-back/day/2026-11-18?write=1", engine);
    await screen.findByText(/Looked back on/);
    await waitFor(() => expect(window.location.hash).toBe("#/journal/look-back/day/2026-11-18"));
    expect(reflect).toHaveBeenCalledTimes(1);
  });

  it("shows that it is writing, and stops when asked", async () => {
    const engine = engineIn("writing");
    const stop = vi.spyOn(engine, "stopReflecting");
    open("#/journal/look-back/day/2026-11-19", engine);
    expect(await screen.findByText("Looking back on your day…")).toBeTruthy();
    expect(screen.getByText("This can take a minute on this computer.")).toBeTruthy();
    expect(screen.getByRole("heading", { name: "At a glance" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Stop" }));
    await waitFor(() => expect(stop).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(screen.queryByText("Looking back on your day…")).toBeNull());
  });

  it("offers Reflect on this day when a day has no reflection yet", async () => {
    const engine = engineIn();
    await engine.deleteReflection({ kind: "day", start: "2026-11-18", end: "2026-11-18" });
    open("#/journal/look-back/day/2026-11-18", engine);
    expect(await screen.findByRole("button", { name: "Reflect on this day" })).toBeTruthy();
    expect(screen.getByRole("heading", { name: "At a glance" })).toBeTruthy();
  });

  it("says there is nothing to look back on, and says so for a period", async () => {
    open("#/journal/look-back/day/2026-10-01");
    expect(
      await screen.findByText(
        "Nothing to look back on yet. Keep something about today, and come back.",
      ),
    ).toBeTruthy();
    window.location.hash = "#/journal/look-back/week/2026-10-05";
    expect(await screen.findByText("Nothing was kept this week.")).toBeTruthy();
  });

  it("says it could not write, and tries again", async () => {
    const engine = engineIn();
    engine.reflect = async () => {
      throw new Error("no");
    };
    open("#/journal/look-back/day/2026-11-18?write=1", engine);
    expect(
      await screen.findByText("Nocturne couldn’t write this just now. Your entries are safe."),
    ).toBeTruthy();
    const again = vi.spyOn(engine, "reflect");
    fireEvent.click(screen.getByRole("button", { name: "Try again" }));
    await waitFor(() => expect(again).toHaveBeenCalledTimes(1));
  });
});

describe("at a glance", () => {
  it("leads with the line for a day that is not ready", async () => {
    open("#/journal/look-back/day/2026-11-19", engineIn("notready"));
    expect(
      await screen.findByText(
        "Nocturne can write about this day once it is ready. Until then, here is the day at a glance.",
      ),
    ).toBeTruthy();
    expect(screen.getByRole("heading", { name: "At a glance" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Reflect on today" })).toBeNull();
  });

  it("leads with the line for a computer that lists rather than writes", async () => {
    open("#/journal/look-back/day/2026-11-19", engineIn("glance"));
    expect(
      await screen.findByText(
        "On this computer, Nocturne lists the day rather than writing about it.",
      ),
    ).toBeTruthy();
  });

  it("lists what was kept, how it felt, who and where, and the entry, each one opening what it counts", async () => {
    open("#/journal/look-back/day/2026-11-19", engineIn("glance"));
    await screen.findByRole("heading", { name: "At a glance" });
    expect(document.body.textContent).toMatch(/\d+ things kept: .*notes?/);
    expect(document.body.textContent).toContain("Felt: tired (8:05 am)");
    expect(document.body.textContent).toMatch(/With: .*Priya \(new\)/);
    const priya = screen.getByRole("link", { name: "Priya" });
    expect(priya.getAttribute("href")).toMatch(/^#\/memories\//);
    const notes = screen.getByRole("button", { name: /notes$/ });
    expect(notes.getAttribute("aria-expanded")).toBe("false");
    fireEvent.click(notes);
    expect(await screen.findByRole("list", { name: "Items kept" })).toBeTruthy();
  });
});

describe("a week, month or year", () => {
  it("shows every section of the week in the design's order", async () => {
    open("#/journal/look-back/week/2026-11-09");
    expect(await screen.findByRole("heading", { level: 1, name: "Your week" })).toBeTruthy();
    expect(screen.getByText("9 to 15 November")).toBeTruthy();
    const headings = screen.getAllByRole("heading").map((h) => h.textContent);
    const order = [
      "Highlights",
      "How it felt",
      "Words you used most",
      "What tended to go together",
      "Good things",
      "People and places",
      "New this week",
      "What changed",
      "Still open",
    ];
    let at = -1;
    for (const name of order) {
      const found = headings.indexOf(name);
      expect(found, name).toBeGreaterThan(at);
      at = found;
    }
    expect(
      screen.getByText("On 3 of the 7 days with Leo in your journal, you checked in as low."),
    ).toBeTruthy();
    const words = screen.getByRole("button", { name: /^tired, / });
    expect(words.closest("li")?.textContent).toMatch(/tired\d+$/);
    expect(
      screen.getByText(
        /^Written on this computer overnight on Sunday, from \d+ days in your journal\.$/,
      ),
    ).toBeTruthy();
    expect(screen.getAllByRole("button", { name: "Show as a table" }).length).toBe(2);
    expect(screen.getByText("Gran · 4 days")).toBeTruthy();
  });

  describe("deleting a recap", () => {
    const week: Period = { kind: "week", start: "2026-11-09", end: "2026-11-15" };

    it("confirms first, with focus on Keep it, and says what stays", async () => {
      const engine = engineIn();
      const remove = vi.spyOn(engine, "deleteReflection");
      open("#/journal/look-back/week/2026-11-09", engine);
      const footer = (await screen.findByRole("button", { name: "Print" })).closest(
        "footer",
      ) as HTMLElement;
      expect(within(footer).getByRole("button", { name: "Delete this recap" })).toBeTruthy();
      fireEvent.click(within(footer).getByRole("button", { name: "Delete this recap" }));
      const dialog = await screen.findByRole("dialog", { name: "Delete this recap?" });
      expect(
        within(dialog).getByText(
          "Only the recap goes. Everything you kept from 9 to 15 November stays: entries, check-ins, good things, notes and photos.",
        ),
      ).toBeTruthy();
      expect(
        within(dialog).getByText(
          "Nocturne won’t put this week together again unless you ask. To keep one thing out of recaps for good, choose “Leave out of reflections” on it.",
        ),
      ).toBeTruthy();
      expect(within(dialog).getByRole("button", { name: "Keep it" })).toHaveFocus();
      fireEvent.click(within(dialog).getByRole("button", { name: "Keep it" }));
      await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
      expect(remove).not.toHaveBeenCalled();
    });

    it("deletes, says what is still here, and shows the week at a glance with one button", async () => {
      const engine = engineIn();
      const remove = vi.spyOn(engine, "deleteReflection");
      const reflect = vi.spyOn(engine, "reflect");
      open("#/journal/look-back/week/2026-11-09", engine);
      fireEvent.click(await screen.findByRole("button", { name: "Delete this recap" }));
      const dialog = await screen.findByRole("dialog", { name: "Delete this recap?" });
      fireEvent.click(within(dialog).getByRole("button", { name: "Delete recap" }));
      await waitFor(() => expect(remove).toHaveBeenCalledWith(week));
      expect(
        await screen.findByText("Deleted. Everything you kept from that week is still here."),
      ).toBeTruthy();
      expect(await screen.findByRole("heading", { name: "At a glance" })).toBeTruthy();
      expect(screen.queryByRole("heading", { name: "Highlights" })).toBeNull();
      const again = screen.getByRole("button", { name: "Look back on this week" });
      fireEvent.click(again);
      await waitFor(() => expect(reflect).toHaveBeenCalledWith(week));
      expect(await screen.findByRole("heading", { name: "Highlights" })).toBeTruthy();
    });

    it("names a month and a year by their own words", () => {
      expect(recapPeriodText({ kind: "month", start: "2026-11-01", end: "2026-11-30" })).toBe(
        "in November",
      );
      expect(recapPeriodText({ kind: "year", start: "2025-01-01", end: "2025-12-31" })).toBe(
        "in 2025",
      );
    });

    it("names a week across two months with both months", () => {
      expect(recapPeriodText({ kind: "week", start: "2026-10-26", end: "2026-11-01" })).toBe(
        "from 26 October to 1 November",
      );
    });

    it("has no delete of its own for a month that is still going before it is put together", async () => {
      const engine = engineIn();
      await engine.deleteReflection({ kind: "month", start: "2026-11-01", end: "2026-11-30" });
      open("#/journal/look-back/month/2026-11-01", engine);
      expect(
        await screen.findByRole("button", { name: "Look back on this month so far" }),
      ).toBeTruthy();
      expect(screen.queryByRole("button", { name: "Delete this recap" })).toBeNull();
    });
  });

  describe("on any computer", () => {
    it.each(["notready", "glance"] as const)(
      "shows the week as a recap, not a glance, when %s",
      async (mode) => {
        open("#/journal/look-back/week/2026-11-09", engineIn(mode));
        expect(await screen.findByRole("heading", { name: "Highlights" })).toBeTruthy();
        expect(screen.queryByRole("heading", { name: "At a glance" })).toBeNull();
        expect(screen.queryByText(/once it is ready/)).toBeNull();
      },
    );

    it("says how much is still being read, and puts it together again when it is read", async () => {
      const engine = engineIn("notready");
      serve(engine, (view) =>
        view.type === "recap" ? { ...view, recap: { ...view.recap, stillReading: 2 } } : view,
      );
      const reflect = vi.spyOn(engine, "reflect");
      open("#/journal/look-back/week/2026-11-09", engine);
      expect(
        await screen.findByText(
          "2 things from this week are still being read, so they aren’t in this yet.",
        ),
      ).toBeTruthy();
      expect(screen.getByRole("heading", { name: "Highlights" })).toBeTruthy();
      fireEvent.click(
        screen.getByRole("button", { name: "Put it together again when they’re read" }),
      );
      await waitFor(() => expect(reflect).toHaveBeenCalledTimes(1), { timeout: 4000 });
    });

    it("says it for one thing, in a month, with the month's own word", async () => {
      const engine = engineIn();
      serve(engine, (view) =>
        view.type === "recap" ? { ...view, recap: { ...view.recap, stillReading: 1 } } : view,
      );
      open("#/journal/look-back/month/2026-11-01", engine);
      expect(
        await screen.findByText(
          "1 thing from this month is still being read, so it isn’t in this yet.",
        ),
      ).toBeTruthy();
      expect(
        screen.getByRole("button", { name: "Put it together again when it’s read" }),
      ).toBeTruthy();
    });
  });

  it("lists the week's check-ins under Show as a table", async () => {
    open("#/journal/look-back/week/2026-11-09");
    await screen.findByRole("heading", { name: "How it felt" });
    fireEvent.click(screen.getAllByRole("button", { name: "Show as a table" })[0] as HTMLElement);
    const table = await screen.findByRole("table");
    expect(within(table).getByRole("columnheader", { name: "Day" })).toBeTruthy();
    expect(within(table).getAllByRole("row").length).toBeGreaterThan(1);
  });

  it("says there were no check-ins, and nothing else about feelings", async () => {
    const engine = engineIn();
    serve(engine, (view) =>
      view.type === "recap"
        ? { ...view, recap: { ...view.recap, checkins: [], words: [], together: [] } }
        : view,
    );
    open("#/journal/look-back/week/2026-11-09", engine);
    expect(await screen.findByText("No check-ins this week.")).toBeTruthy();
    expect(screen.queryByText("Words you used most")).toBeNull();
  });

  it("compares a month with the one before in counts only", async () => {
    open("#/journal/look-back/month/2026-11-01");
    expect(await screen.findByRole("heading", { level: 1, name: "Your month" })).toBeTruthy();
    expect(screen.getByText(/^November 2026 · So far, at 9:40 pm$/)).toBeTruthy();
    expect(screen.queryByText(/better|worse/)).toBeNull();
  });

  it("shows a year's Firsts and leaves out Still open", async () => {
    const engine = engineIn();
    const real = engine.getReflection.bind(engine);
    engine.getReflection = async (period: Period) => {
      if (period.kind !== "year") return real(period);
      const month = await real({ kind: "month", start: "2026-11-01", end: "2026-11-30" });
      if (month.type !== "recap") throw new Error("seed has no month");
      return {
        type: "recap",
        recap: { ...month.recap, period, soFar: false, stillOpen: [], changed: [] },
      };
    };
    open("#/journal/look-back/year/2026-01-01", engine);
    expect(await screen.findByRole("heading", { level: 1, name: "Your year" })).toBeTruthy();
    expect(screen.getByText("2026")).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Firsts" })).toBeTruthy();
    expect(screen.queryByRole("heading", { name: "Still open" })).toBeNull();
    expect(screen.getByRole("list", { name: "Check-ins for each month of the year" })).toBeTruthy();
  });

  it("offers to look back on a period that is still going", async () => {
    const engine = engineIn();
    await engine.deleteReflection({ kind: "month", start: "2026-11-01", end: "2026-11-30" });
    open("#/journal/look-back/month/2026-11-01", engine);
    expect(
      await screen.findByRole("button", { name: "Look back on this month so far" }),
    ).toBeTruthy();
  });
});
