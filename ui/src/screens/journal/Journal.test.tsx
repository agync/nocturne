import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createMockEngine, type MockEngine } from "../../engine/mock/engine";
import type { JournalMode } from "../../engine/mock/seed/journal";
import { installFakeAudio, speak } from "../../test/fakeAudio";
import { fakePlatform, renderApp, setViewport } from "../../test/render";

const NOW = new Date("2026-11-19T21:20:00Z");
const TODAY = "2026-11-19";

const engineIn = (mode: JournalMode = "day"): MockEngine =>
  createMockEngine({ now: NOW, journal: { mode, delayMs: 0 } });

beforeEach(() => setViewport(1280));
afterEach(() => {
  Object.defineProperty(navigator, "mediaDevices", { configurable: true, value: undefined });
  vi.unstubAllGlobals();
  vi.useRealTimers();
  window.location.hash = "";
});

const open = async (
  mode: JournalMode = "day",
  hash = "#/journal",
  engine: MockEngine = engineIn(mode),
) => {
  const view = renderApp({ hash, engine });
  await screen.findByRole("heading", { level: 1 });
  await screen.findByRole("heading", { level: 2, name: "Through the day" });
  return { ...view, engine };
};

const timeline = () =>
  screen.getByRole("heading", { level: 2, name: "Through the day" }).parentElement as HTMLElement;
const LEO = "Leo forgot his PE kit again. Dropped it at the school office on the way to work.";
const rowOf = (text: string | RegExp) =>
  within(timeline()).getByText(text).closest("li") as HTMLElement;

describe("the journal day", () => {
  it("shows the seeded Thursday: head, entry, timeline groups and side blocks", async () => {
    await open();
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent("Thursday 19 November");
    expect(screen.getByRole("link", { name: "← Wednesday" })).toHaveAttribute(
      "href",
      "#/journal/2026-11-18",
    );
    expect(screen.getByRole("button", { name: "Choose a day" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Reflect on today" })).toHaveAttribute(
      "href",
      `#/journal/look-back/day/${TODAY}?write=1`,
    );

    expect(screen.getByRole("heading", { level: 2, name: "Today’s entry" })).toBeInTheDocument();
    expect(screen.getByDisplayValue(/Finally had lunch with Priya/)).toBeInTheDocument();
    expect(screen.getByDisplayValue(/Gran rang at six/)).toBeInTheDocument();
    expect(screen.getByText("Voice note, 1:42")).toBeInTheDocument();
    expect(
      screen.getByText("Tomatoes ripening on a windowsill.", { selector: "figcaption" }),
    ).toBeInTheDocument();
    expect(screen.getByText(/^Read /)).toBeInTheDocument();

    expect(
      within(timeline())
        .getAllByRole("heading", { level: 3 })
        .map((h) => h.textContent),
    ).toEqual(["Morning", "Afternoon", "Evening"]);

    for (const name of [
      "How today felt",
      "Good things",
      "People and places today",
      "Looked back on",
    ]) {
      expect(screen.getByRole("heading", { level: 2, name })).toBeInTheDocument();
    }
    expect(screen.getAllByText("Sam fixed the gate without being asked.").length).toBeGreaterThan(
      0,
    );
    expect(screen.getByText("A slow start, then lunch with Priya at last.")).toBeInTheDocument();
    expect(screen.getByText("3:15 pm, from 6 things you kept")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Open" })).toHaveAttribute(
      "href",
      `#/journal/look-back/day/${TODAY}`,
    );
  });

  it("gives each row a link to its item, and one marks the entry", async () => {
    await open();
    expect(within(rowOf(LEO)).getByRole("link", { name: /Leo forgot/ })).toHaveAttribute(
      "href",
      "#/items/j-note-leo",
    );
    expect(within(rowOf(/Year 4 trip form/)).getByRole("link")).toHaveAttribute(
      "href",
      "#/items/j-form",
    );
    const marker = rowOf("Today’s entry");
    expect(within(marker).getByRole("button")).toBeInTheDocument();
    const tired = rowOf("“Bad night.”");
    expect(within(tired).getByText("Low · tired")).toBeInTheDocument();
    expect(within(tired).getByText("Change")).toHaveAttribute(
      "href",
      "#/journal/check-in?change=j-ci-tired",
    );
    expect(within(rowOf("Lunch with Priya, finally")).getByText("pleased")).toBeInTheDocument();
    expect(
      within(rowOf("Lunch with Priya, finally")).getByText("from My phone"),
    ).toBeInTheDocument();
  });

  it("lists the day's check-ins beside the field and opens one from its dot", async () => {
    await open();
    expect(screen.getByRole("button", { name: "Low, tired, 8:05 am" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Low, tired, 8:05 am" }));
    expect(window.location.hash).toBe("#/items/j-ci-tired");
  });

  it("disables Reflect on an empty day and says why", async () => {
    await open("empty");
    const reflect = screen.getByRole("button", { name: "Reflect on today" });
    expect(reflect).toBeDisabled();
    expect(screen.getByText("Keep something about today first.")).toBeInTheDocument();
    expect(
      screen.getByText(
        "Nothing kept today yet. Anything you keep, from here, the keep bar or your phone, shows up here.",
      ),
    ).toBeInTheDocument();
    expect(screen.getByText("No check-ins today.")).toBeInTheDocument();
    expect(screen.getAllByRole("link", { name: "Check in" })).toHaveLength(2);
    expect(
      screen.getByPlaceholderText("Write about today, or hold Space to talk."),
    ).toBeInTheDocument();
  });

  it("shows the first-use card once and Got it hides it", async () => {
    const { engine } = await open("first");
    const spy = vi.spyOn(engine, "dismissJournalIntro");
    expect(screen.getByText("Your journal")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Got it" }));
    expect(spy).toHaveBeenCalled();
    await waitFor(() => expect(screen.queryByText("Your journal")).toBeNull());
  });

  it("tells a day that is still being read, and a computer still getting ready", async () => {
    await open("notready");
    expect(screen.getByText("Nocturne is getting ready.")).toBeInTheDocument();
    expect(
      screen.getByText("What you keep is safe and will be read when it is."),
    ).toBeInTheDocument();
    expect(screen.getAllByText("Reading…").length).toBeGreaterThan(0);
    expect(
      screen.getByText("Nocturne will read this when you stop writing for a while."),
    ).toBeInTheDocument();
    expect(screen.getByText("Photo, not read yet")).toBeInTheDocument();
  });

  it("opens a past day with its own wording", async () => {
    await open("day", "#/journal/2026-11-18");
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent("Wednesday 18 November");
    expect(screen.getByRole("link", { name: "Reflect on this day" })).toHaveAttribute(
      "href",
      "#/journal/look-back/day/2026-11-18?write=1",
    );
    expect(
      screen.getByRole("heading", { level: 2, name: "Wednesday’s entry" }),
    ).toBeInTheDocument();
    expect(screen.getByDisplayValue(/Quiet evening/)).toBeInTheDocument();
  });

  it("does not open a day that has not come yet", async () => {
    renderApp({ hash: "#/journal/2026-12-25", engine: engineIn() });
    await waitFor(() =>
      expect(screen.queryByRole("heading", { level: 1, name: /December/ })).toBeNull(),
    );
    expect(screen.queryByRole("link", { name: "Reflect on this day" })).toBeNull();
  });
});

describe("choosing a day", () => {
  it("offers days, weeks, months and years, and never a day past today", async () => {
    await open();
    await userEvent.click(screen.getByRole("button", { name: "Choose a day" }));
    const dialog = await screen.findByRole("dialog", { name: "Choose a day" });
    expect(
      within(dialog)
        .getAllByRole("tab")
        .map((tab) => tab.textContent),
    ).toEqual(["Days", "Weeks", "Months", "Years"]);
    const days = within(dialog)
      .getAllByRole("link")
      .map((a) => a.getAttribute("href"));
    expect(days[0]).toBe("#/journal");
    expect(days).toContain("#/journal/2026-11-18");
    expect(days.every((href) => !href?.includes("2026-11-2"))).toBe(true);
    await userEvent.click(within(dialog).getByRole("tab", { name: "Weeks" }));
    const week = await within(dialog).findAllByRole("link");
    expect(week[0]).toHaveAttribute(
      "href",
      expect.stringMatching(/^#\/journal\/look-back\/week\//),
    );
    await userEvent.click(week[0] as HTMLElement);
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });
});

describe("writing the entry", () => {
  it("saves what is typed once the person pauses", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const { engine } = await open("empty");
    const spy = vi.spyOn(engine, "saveEntryText");
    const field = screen.getByPlaceholderText("Write about today, or hold Space to talk.");
    const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
    await user.click(field);
    await user.type(field, "A quiet morning");
    expect(spy).not.toHaveBeenCalled();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1000);
    });
    expect(spy).toHaveBeenCalledTimes(1);
    expect(spy).toHaveBeenCalledWith(TODAY, null, 0, "A quiet morning");
    await waitFor(() => expect(screen.getByText(/^Saved /)).toBeInTheDocument());
    expect(screen.getByDisplayValue("A quiet morning")).toHaveFocus();
  });

  it("keeps typing into the same paragraph after it was saved", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const { engine } = await open("empty");
    const spy = vi.spyOn(engine, "saveEntryText");
    const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
    const field = screen.getByPlaceholderText("Write about today, or hold Space to talk.");
    await user.type(field, "One");
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1000);
    });
    await user.type(screen.getByDisplayValue("One"), " two");
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1000);
    });
    expect(spy).toHaveBeenCalledTimes(2);
    expect(spy.mock.calls[1]?.[1]).toEqual(expect.stringMatching(/^j-new-/));
    expect(spy.mock.calls[1]?.[3]).toBe("One two");
  });

  it("starts a new paragraph when Enter ends a paragraph", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const { engine } = await open("empty");
    const spy = vi.spyOn(engine, "saveEntryText");
    const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
    await user.type(
      screen.getByPlaceholderText("Write about today, or hold Space to talk."),
      "First{Enter}Second",
    );
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1000);
    });
    expect(screen.getByLabelText("Your entry, paragraph 2")).toHaveValue("Second");
    await waitFor(() => expect(spy).toHaveBeenCalledWith(TODAY, null, 1, "Second"));
    expect(spy).toHaveBeenCalledWith(TODAY, null, 0, "First");
  });

  it("reads the entry now when the person leaves", async () => {
    const { engine } = await open();
    const spy = vi.spyOn(engine, "readEntryNow");
    window.location.hash = "#/today";
    await waitFor(() => expect(spy).toHaveBeenCalledWith(TODAY));
  });

  it("saves what was typed before it reads, when the person leaves in a hurry", async () => {
    const { engine } = await open("empty");
    const order: string[] = [];
    vi.spyOn(engine, "saveEntryText").mockImplementation(async () => {
      order.push("save");
      return null;
    });
    vi.spyOn(engine, "readEntryNow").mockImplementation(async () => {
      order.push("read");
    });
    await userEvent.type(
      screen.getByPlaceholderText("Write about today, or hold Space to talk."),
      "Late",
    );
    window.location.hash = "#/today";
    await waitFor(() => expect(order).toEqual(["save", "read"]));
  });

  it("says the microphone is unavailable when Space is held in an empty paragraph", async () => {
    await open("empty");
    const field = screen.getByPlaceholderText("Write about today, or hold Space to talk.");
    fireEvent.keyDown(field, { key: " " });
    expect(
      await screen.findByText(/This window can’t record sound|Recording only works/),
    ).toBeInTheDocument();
    fireEvent.keyUp(field, { key: " " });
    expect(field).toHaveValue("");
  });

  it("records a voice note while Space is held and places it in the entry", async () => {
    installFakeAudio(48000, true);
    Object.defineProperty(navigator, "mediaDevices", {
      configurable: true,
      value: { getUserMedia: vi.fn().mockResolvedValue({ getTracks: () => [] }) },
    });
    const { engine } = await open("empty");
    const keep = vi.spyOn(engine, "keepBytes");
    const place = vi.spyOn(engine, "placeInEntry");
    const field = screen.getByPlaceholderText("Write about today, or hold Space to talk.");
    fireEvent.keyDown(field, { key: " " });
    await waitFor(() => expect(navigator.mediaDevices.getUserMedia).toHaveBeenCalled());
    await act(async () => {
      speak(500);
    });
    fireEvent.keyUp(field, { key: " " });
    await waitFor(() => expect(place).toHaveBeenCalled());
    expect(keep.mock.calls[0]?.[1]).toBe("audio/wav");
    expect(keep.mock.calls[0]?.[2]).toBe("journal");
    expect(place.mock.calls[0]?.[1]).toBe(TODAY);
    expect(await screen.findByText(/^Voice note, /)).toBeInTheDocument();
  });

  it("keeps a photo for the entry and places it", async () => {
    const platform = fakePlatform();
    platform.pickFiles = async () => ["/pics/windowsill.jpg"];
    const engine = engineIn("empty");
    const keep = vi.spyOn(engine, "keepFiles");
    const place = vi.spyOn(engine, "placeInEntry");
    renderApp({ hash: "#/journal", engine, platform });
    await screen.findByRole("heading", { level: 2, name: "Through the day" });
    await userEvent.click(screen.getByRole("button", { name: "Add a photo" }));
    await waitFor(() => expect(place).toHaveBeenCalled());
    expect(keep).toHaveBeenCalledWith(["/pics/windowsill.jpg"], "journal");
    expect(place.mock.calls[0]?.[1]).toBe(TODAY);
  });

  it("changes a photo's description", async () => {
    const { engine } = await open();
    const spy = vi.spyOn(engine, "describePhoto");
    await userEvent.click(screen.getByRole("button", { name: "Describe this photo" }));
    const field = screen.getByRole("textbox", { name: "Describe this photo" });
    await userEvent.clear(field);
    await userEvent.type(field, "Green tomatoes");
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(spy).toHaveBeenCalledWith("j-block-photo", "Green tomatoes");
  });

  it("shows the words of a voice note under What was said", async () => {
    await open();
    expect(screen.queryByText(/beginners’ night/)).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "What was said" }));
    expect(screen.getByText(/beginners’ night/)).toBeInTheDocument();
  });

  it("offers an earlier version of a paragraph that was changed", async () => {
    const engine = engineIn();
    const real = engine.getJournalDay.bind(engine);
    vi.spyOn(engine, "getJournalDay").mockImplementation(async (day) => {
      const view = await real(day);
      const first = view.entry[0];
      if (first) {
        first.changedAt = "2026-11-19T21:30:00Z";
        first.earlier = [
          { itemId: "old", text: "Finally had lunch.", keptAt: "2026-11-19T13:40:00Z" },
        ];
      }
      return view;
    });
    await open("day", "#/journal", engine);
    expect(screen.getByText(/^Changed /)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Earlier version" }));
    expect(screen.getByText("Finally had lunch.")).toBeInTheDocument();
  });
});

describe("names in the entry", () => {
  it("lists them under the entry and outlines the words while a chip is hovered", async () => {
    await open();
    const strip = screen.getByRole("list", { name: "In this entry" });
    const chips = within(strip)
      .getAllByRole("button")
      .map((b) => b.textContent);
    expect(chips).toEqual([
      "Priyanew",
      "Mill Lane café",
      "the climbing wallnew",
      "Sam",
      "Gran",
      "the allotment",
    ]);
    expect(document.querySelector("mark")).toBeNull();
    await userEvent.hover(within(strip).getByRole("button", { name: /Priya/ }));
    expect(document.querySelector("mark")).toHaveTextContent("Priya");
    await userEvent.unhover(within(strip).getByRole("button", { name: /Priya/ }));
    expect(document.querySelector("mark")).toBeNull();
  });
});

describe("rows of the timeline", () => {
  it("leaves a row out of reflections, and Undo puts it back", async () => {
    const { engine } = await open();
    const set = vi.spyOn(engine, "setLeftOut");
    const row = rowOf(
      "Leo forgot his PE kit again. Dropped it at the school office on the way to work.",
    );
    await userEvent.click(within(row).getByRole("button", { name: "More about this" }));
    await userEvent.click(
      await screen.findByRole("menuitemcheckbox", { name: "Leave out of reflections" }),
    );
    expect(set).toHaveBeenCalledWith("j-note-leo", true);
    expect(await screen.findByText("Left out. It stays in your journal.")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Undo" }));
    expect(set).toHaveBeenLastCalledWith("j-note-leo", false);
  });

  it("marks a left-out row and lets the same menu item put it back", async () => {
    const { engine } = await open();
    const set = vi.spyOn(engine, "setLeftOut");
    const menuOf = async () => {
      await userEvent.click(within(rowOf(LEO)).getByRole("button", { name: "More about this" }));
      return screen.findByRole("menuitemcheckbox", { name: "Leave out of reflections" });
    };
    expect(within(rowOf(LEO)).queryByRole("img", { name: "Leave out of reflections" })).toBeNull();
    expect(await menuOf()).toHaveAttribute("aria-checked", "false");
    await userEvent.click(await menuOf());
    expect(set).toHaveBeenLastCalledWith("j-note-leo", true);
    expect(
      await within(rowOf(LEO)).findByRole("img", { name: "Leave out of reflections" }),
    ).toBeInTheDocument();
    expect(await menuOf()).toHaveAttribute("aria-checked", "true");
    await userEvent.click(screen.getByRole("menuitemcheckbox", { checked: true }));
    expect(set).toHaveBeenLastCalledWith("j-note-leo", false);
    await waitFor(() =>
      expect(
        within(rowOf(LEO)).queryByRole("img", { name: "Leave out of reflections" }),
      ).toBeNull(),
    );
  });

  it("opens the menu from the keyboard without adding a tab stop", async () => {
    await open();
    const row = rowOf(
      "Leo forgot his PE kit again. Dropped it at the school office on the way to work.",
    );
    expect(within(row).getByRole("button", { name: "More about this" })).toHaveAttribute(
      "tabindex",
      "-1",
    );
    const link = within(row).getByRole("link", { name: /Leo forgot/ });
    link.focus();
    fireEvent.keyDown(link, { key: "F10", shiftKey: true });
    const item = await screen.findByRole("menuitemcheckbox", {
      name: "Leave out of reflections",
    });
    expect(item).toHaveFocus();
    fireEvent.keyDown(item, { key: "Escape" });
    await waitFor(() => expect(link).toHaveFocus());
  });

  it("offers Move to a day when the engine says a note was about another day", async () => {
    const engine = engineIn();
    const real = engine.getJournalDay.bind(engine);
    vi.spyOn(engine, "getJournalDay").mockImplementation(async (day) => {
      const view = await real(day);
      const row = view.rows.find((r) => r.item.id === "j-note-leo");
      if (row) row.moveTo = "2026-11-18";
      return view;
    });
    await open("day", "#/journal", engine);
    const move = vi.spyOn(engine, "moveToDay");
    const row = rowOf(LEO);
    await userEvent.click(within(row).getByRole("button", { name: "More about this" }));
    await userEvent.click(await screen.findByRole("menuitem", { name: "Move to Wednesday" }));
    expect(move).toHaveBeenCalledWith("j-note-leo", "2026-11-18");
  });

  it("says so, with Try again, when the day cannot be got", async () => {
    const engine = engineIn();
    const real = engine.getJournalDay.bind(engine);
    vi.spyOn(engine, "getJournalDay").mockImplementation(async (day) => {
      if (day === "2026-11-18") throw new Error("no");
      return real(day);
    });
    renderApp({ hash: "#/journal/2026-11-18", engine });
    expect(
      await screen.findByText("Nocturne couldn’t get this day. What you kept is safe."),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Try again" })).toBeInTheDocument();
  });
});

describe("good things", () => {
  it("adds one in the side column with a line and keeps it", async () => {
    const { engine } = await open("empty");
    const spy = vi.spyOn(engine, "keepGoodThing");
    const side = screen.getByRole("heading", { level: 2, name: "Good things" })
      .parentElement as HTMLElement;
    await userEvent.click(within(side).getByRole("button", { name: "Add a good thing" }));
    const line = screen.getByPlaceholderText("Something good, big or small.");
    expect(line).toHaveFocus();
    await userEvent.type(line, "Sam fixed the gate");
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(spy).toHaveBeenCalledWith(TODAY, "Sam fixed the gate", null);
    expect(await screen.findAllByText("Sam fixed the gate")).not.toHaveLength(0);
  });

  it("opens the same line from the entry's toolbar", async () => {
    await open("empty");
    const toolbar = screen.getByRole("button", { name: "Record" }).parentElement as HTMLElement;
    await userEvent.click(within(toolbar).getByRole("button", { name: "Add a good thing" }));
    expect(screen.getByPlaceholderText("Something good, big or small.")).toHaveFocus();
  });

  it("offers a photo already kept today", async () => {
    const { engine } = await open("day", "#/journal", engineIn("day"));
    const spy = vi.spyOn(engine, "keepGoodThing");
    expect(spy).not.toHaveBeenCalled();
  });

  it("hides the button once there are three", async () => {
    await open();
    expect(screen.queryByRole("button", { name: "Add a good thing" })).toBeNull();
  });
});

describe("the day menu", () => {
  it("deletes the whole day through the usual dialog", async () => {
    await open();
    await userEvent.click(screen.getByRole("button", { name: "About this day" }));
    await userEvent.click(screen.getByRole("menuitem", { name: "Delete this day…" }));
    expect(await screen.findByRole("dialog")).toBeInTheDocument();
  });
});
