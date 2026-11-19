import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createMockEngine } from "../../engine/mock/engine";
import { renderApp, setViewport } from "../../test/render";

beforeEach(() => setViewport(1280));
afterEach(() => {
  window.location.hash = "";
});

const browse = async (hash = "#/memories", engine = createMockEngine()) => {
  const view = renderApp({ hash, engine });
  await screen.findByRole("tablist", { name: "Browse by" });
  return { ...view, engine };
};

const openLeo = async (engine = createMockEngine()) => {
  const view = await browse("#/memories/ent-leo", engine);
  await screen.findByRole("heading", { level: 1, name: "Leo" });
  await screen.findByText("Leo’s swimming is on Mondays at 17:30.");
  return view;
};

describe("What Nocturne remembers", () => {
  it("has People, Places and Things tabs and a link to days", async () => {
    await browse();
    expect(
      screen.getByRole("heading", { level: 1, name: "What Nocturne remembers" }),
    ).toBeVisible();
    expect(screen.getAllByRole("tab").map((tab) => tab.textContent)).toEqual([
      "People",
      "Places",
      "Things",
    ]);
    expect(screen.getByRole("tab", { name: "People" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("link", { name: /^Days/ })).toHaveAttribute("href", "#/pages");
  });

  it("lists people with their role and how many memories they hold", async () => {
    await browse();
    const leo = await screen.findByRole("link", { name: /^Leo/ });
    expect(leo).toHaveTextContent("Son");
    expect(leo).toHaveTextContent("6");
    expect(leo).toHaveAttribute("href", "#/memories/ent-leo");
    expect(screen.getByRole("link", { name: /Dr Chen/ })).toHaveTextContent("Dentist");
  });

  it("moves between tabs with the arrow keys and says when a tab is empty", async () => {
    await browse();
    await userEvent.click(screen.getByRole("tab", { name: "People" }));
    await userEvent.keyboard("{ArrowRight}");
    expect(screen.getByRole("tab", { name: "Places" })).toHaveAttribute("aria-selected", "true");
    expect(await screen.findByRole("link", { name: /Rowan Court/ })).toHaveTextContent(
      "Where Gran lives",
    );
    await userEvent.keyboard("{ArrowRight}");
    expect(await screen.findByRole("link", { name: /Bin day/ })).toBeInTheDocument();
  });

  it("opens a person: name, role, counts and Mark all private", async () => {
    await openLeo();
    expect(screen.getByText("Son · 6 memories from 5 things you kept")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Mark all private" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Rename" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /^Leo/ })).toHaveAttribute("aria-current", "page");
  });

  it("shows each memory's certainty, receipts and words for its state", async () => {
    await openLeo();
    const swim = screen
      .getByText("Leo’s swimming is on Mondays at 17:30.")
      .closest("li") as HTMLElement;
    expect(within(swim).getByText("Sure")).toBeInTheDocument();
    expect(within(swim).getByText("Changed")).toBeInTheDocument();
    expect(within(swim).getByText(/was Wednesday/)).toBeInTheDocument();
    expect(within(swim).getByRole("button", { name: /^Message you pasted/ })).toBeInTheDocument();
    const nut = screen.getByText("Leo is allergic to nuts.").closest("li") as HTMLElement;
    expect(within(nut).getByText("Unconfirmed")).toBeInTheDocument();
    expect(within(nut).getByText("Not sure")).toBeInTheDocument();
    expect(within(nut).getByText("Kept forever")).toBeInTheDocument();
  });

  it("marks private in words", async () => {
    await browse("#/memories/ent-gran");
    const tablets = (await screen.findByText(/amlodipine/)).closest("li") as HTMLElement;
    expect(within(tablets).getByText("Private")).toBeInTheDocument();
  });

  it("marks fading in words", async () => {
    await browse("#/memories/ent-ava");
    expect(
      within(
        (await screen.findByText(/old swimming teacher/)).closest("li") as HTMLElement,
      ).getByText("Fading"),
    ).toBeInTheDocument();
  });

  it("selects a memory to show its actions, and Escape puts them away", async () => {
    await openLeo();
    const row = screen.getByRole("button", { name: /^Leo is allergic to nuts\./ });
    expect(row).toHaveAttribute("aria-expanded", "false");
    await userEvent.click(row);
    expect(row).toHaveAttribute("aria-expanded", "true");
    const bar = screen.getByRole("group", { name: "Actions for: Leo is allergic to nuts." });
    expect(
      within(bar)
        .getAllByRole("button")
        .map((b) => b.textContent),
    ).toEqual([
      "Correct",
      "Keep forever",
      "Mark private",
      "Is this about something else?",
      "Delete…",
    ]);
    expect(within(bar).getByText(/health, money or ID information/)).toBeInTheDocument();
    await userEvent.keyboard("{Escape}");
    expect(row).toHaveAttribute("aria-expanded", "false");
    expect(row).toHaveFocus();
  });

  it("corrects a memory in place", async () => {
    const { engine } = await openLeo();
    const spy = vi.spyOn(engine, "correctMemory");
    await userEvent.click(screen.getByRole("button", { name: /^Leo is allergic to nuts\./ }));
    await userEvent.click(screen.getByRole("button", { name: "Correct" }));
    const field = screen.getByRole("textbox", { name: "What should it say?" });
    expect(field).toHaveFocus();
    await userEvent.clear(field);
    await userEvent.type(field, "Leo is allergic to peanuts.{Enter}");
    expect(spy).toHaveBeenCalledWith("mem-nut", "Leo is allergic to peanuts.");
    expect(await screen.findByText("Corrected.")).toBeInTheDocument();
    expect(await screen.findByText("Leo is allergic to peanuts.")).toBeInTheDocument();
  });

  it("keeps the typed correction in the box when saving fails", async () => {
    const engine = createMockEngine();
    vi.spyOn(engine, "correctMemory").mockRejectedValueOnce(new Error("no"));
    await openLeo(engine);
    await userEvent.click(screen.getByRole("button", { name: /^Leo is allergic to nuts\./ }));
    await userEvent.click(screen.getByRole("button", { name: "Correct" }));
    const field = screen.getByRole("textbox", { name: "What should it say?" });
    await userEvent.clear(field);
    await userEvent.type(field, "Leo is allergic to peanuts.{Enter}");
    expect(await screen.findByText("That didn’t work. Nothing was changed.")).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "What should it say?" })).toHaveValue(
      "Leo is allergic to peanuts.",
    );
    await userEvent.keyboard("{Enter}");
    expect(await screen.findByText("Corrected.")).toBeInTheDocument();
  });

  it("keeps forever and marks private, as pressed buttons", async () => {
    const { engine } = await openLeo();
    const keep = vi.spyOn(engine, "setKeepForever");
    const priv = vi.spyOn(engine, "setPrivate");
    await userEvent.click(
      screen.getByRole("button", { name: /^Leo’s swimming is on Mondays at 17:30\./ }),
    );
    await userEvent.click(screen.getByRole("button", { name: "Keep forever" }));
    expect(keep).toHaveBeenCalledWith("mem-swim", true);
    expect(await screen.findByText("Kept forever.")).toBeInTheDocument();
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Keep forever" })).toHaveAttribute(
        "aria-pressed",
        "true",
      ),
    );
    await userEvent.click(screen.getByRole("button", { name: "Mark private" }));
    expect(priv).toHaveBeenCalledWith("mem-swim", true);
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Mark private" })).toHaveAttribute(
        "aria-pressed",
        "true",
      ),
    );
  });

  it("forgets a memory through the dialog and then drops it from the list", async () => {
    await openLeo();
    await userEvent.click(screen.getByRole("button", { name: /^Leo is allergic to nuts\./ }));
    await userEvent.click(screen.getByRole("button", { name: "Delete…" }));
    const dialog = await screen.findByRole("dialog", { name: /^Delete \d+ (memory|things)\?$/ });
    await userEvent.click(await within(dialog).findByRole("button", { name: /^Delete 2 things/ }));
    expect(await within(dialog).findByText("Deleted.")).toBeInTheDocument();
    // The list behind is already right, before the dialog is closed.
    await waitFor(() => expect(screen.queryByText("Leo is allergic to nuts.")).toBeNull());
    await userEvent.click(within(dialog).getByRole("button", { name: "OK" }));
    await waitFor(() =>
      expect(screen.getByRole("heading", { level: 1, name: "Leo" })).toHaveFocus(),
    );
  });

  it("marks every memory private with one button", async () => {
    const { engine } = await openLeo();
    const spy = vi.spyOn(engine, "setPrivate");
    await userEvent.click(screen.getByRole("button", { name: "Mark all private" }));
    await waitFor(() => expect(spy).toHaveBeenCalledTimes(6));
    expect(await screen.findByText("Marked 6 memories private.")).toBeInTheDocument();
    expect(await screen.findByRole("button", { name: "All private" })).toBeDisabled();
  });

  it("searches everything kept from the address, and from the box", async () => {
    await browse("#/memories?q=swim");
    expect(screen.getByRole("searchbox", { name: "Search everything kept" })).toHaveValue("swim");
    expect(
      await screen.findByRole("heading", { name: "3 memories match “swim”" }),
    ).toBeInTheDocument();
    const box = screen.getByRole("searchbox", { name: "Search everything kept" });
    await userEvent.clear(box);
    await userEvent.type(box, "dentist");
    await waitFor(() => expect(window.location.hash).toBe("#/memories?q=dentist"));
    expect(
      await screen.findByRole("heading", { name: /memories? match “dentist”/ }),
    ).toBeInTheDocument();
    expect(await screen.findByRole("link", { name: /Dr Chen/ })).toBeInTheDocument();
  });

  it("says nothing matched, kindly", async () => {
    await browse("#/memories?q=zebra");
    expect(
      await screen.findByRole("heading", { name: "No memories match “zebra”" }),
    ).toBeInTheDocument();
    expect(screen.getAllByText(/Nothing matches “zebra”/).length).toBeGreaterThan(0);
  });

  it("puts the tab of the entity's own kind under it", async () => {
    await browse("#/memories/ent-bin");
    await screen.findByRole("heading", { level: 1, name: "Bin day" });
    expect(screen.getByRole("tab", { name: "Things" })).toHaveAttribute("aria-selected", "true");
  });

  it("says when a tab is empty", async () => {
    const engine = createMockEngine();
    const list = engine.listEntities.bind(engine);
    vi.spyOn(engine, "listEntities").mockImplementation(async (kind, query) =>
      kind === "place" ? [] : list(kind, query),
    );
    await browse("#/memories", engine);
    await userEvent.click(screen.getByRole("tab", { name: "Places" }));
    expect(await screen.findByText(/No places yet/)).toBeInTheDocument();
  });

  it("says so for something that is not there", async () => {
    renderApp({ hash: "#/memories/nobody" });
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("Nocturne couldn’t find that.");
    expect(within(alert).getByRole("link", { name: /What Nocturne remembers/ })).toHaveAttribute(
      "href",
      "#/memories",
    );
  });

  it("says so when the list cannot be read", async () => {
    const engine = createMockEngine();
    vi.spyOn(engine, "listEntities").mockRejectedValue(new Error("no"));
    renderApp({ hash: "#/memories", engine });
    expect(
      await screen.findByText("Nocturne couldn’t open this. What you kept is safe."),
    ).toBeInTheDocument();
  });

  it("offers to move a memory to something else once it has been corrected", async () => {
    const { engine } = await openLeo();
    const spy = vi.spyOn(engine, "refileMemory");
    await userEvent.click(screen.getByRole("button", { name: /^Leo is allergic to nuts\./ }));
    await userEvent.click(screen.getByRole("button", { name: "Correct" }));
    const field = screen.getByRole("textbox", { name: "What should it say?" });
    await userEvent.clear(field);
    await userEvent.type(field, "Ava is allergic to nuts.{Enter}");
    await screen.findByText("Corrected.");
    const toast = document.querySelector('[data-region="toast"]') as HTMLElement;
    await userEvent.click(
      within(toast).getByRole("button", { name: "Is this about something else?" }),
    );
    const dialog = await screen.findByRole("dialog", { name: "Is this about something else?" });
    expect(within(dialog).getByText("Ava is allergic to nuts.")).toBeInTheDocument();
    await userEvent.click(await within(dialog).findByRole("radio", { name: /^Ava/ }));
    await userEvent.click(within(dialog).getByRole("button", { name: "Move it" }));
    expect(spy).toHaveBeenCalledWith("mem-nut", { type: "existing", entityId: "ent-ava" });
    expect(await screen.findByText("Moved to Ava.")).toBeInTheDocument();
    await waitFor(() => expect(screen.queryByText("Ava is allergic to nuts.")).toBeNull());
  });

  it("moves one memory from its actions, and undoes it", async () => {
    await openLeo();
    await userEvent.click(screen.getByRole("button", { name: /^Leo needs a packed lunch/ }));
    await userEvent.click(screen.getByRole("button", { name: "Is this about something else?" }));
    const dialog = await screen.findByRole("dialog", { name: "Is this about something else?" });
    const move = within(dialog).getByRole("button", { name: "Move it" });
    expect(move).toBeDisabled();
    expect(within(dialog).queryByRole("radio", { name: /^Leo/ })).toBeNull();
    await userEvent.type(
      within(dialog).getByRole("textbox", { name: "Find a person, place or thing" }),
      "okaf",
    );
    await userEvent.click(await within(dialog).findByRole("radio", { name: /^Mrs Okafor/ }));
    const okafor = within(dialog).getByRole("radio", { name: "Mrs Okafor Person · Leo’s teacher" });
    expect(okafor).toBeChecked();
    expect(within(dialog).getByRole("group", { name: "People, places and things" })).toBeVisible();
    await userEvent.click(move);
    expect(await screen.findByText("Moved to Mrs Okafor.")).toBeInTheDocument();
    await waitFor(() =>
      expect(screen.queryByText("Leo needs a packed lunch on the trip day.")).toBeNull(),
    );
    await userEvent.click(screen.getByRole("button", { name: "Undo" }));
    expect(await screen.findByText("Moved back.")).toBeInTheDocument();
    expect(
      await screen.findByText("Leo needs a packed lunch on the trip day."),
    ).toBeInTheDocument();
  });

  it("moves a memory to something new", async () => {
    const { engine } = await openLeo();
    const spy = vi.spyOn(engine, "refileMemory");
    await userEvent.click(screen.getByRole("button", { name: /^Leo needs a packed lunch/ }));
    await userEvent.click(screen.getByRole("button", { name: "Is this about something else?" }));
    const dialog = await screen.findByRole("dialog", { name: "Is this about something else?" });
    await userEvent.click(within(dialog).getByRole("radio", { name: "Someone or something new" }));
    const name = within(dialog).getByRole("textbox", { name: "What is it called?" });
    await userEvent.type(name, "School trip");
    await userEvent.click(within(dialog).getByRole("radio", { name: "A place" }));
    await userEvent.click(within(dialog).getByRole("button", { name: "Move it" }));
    expect(spy).toHaveBeenCalledWith("mem-trip-lunch", {
      type: "new",
      name: "School trip",
      kind: "place",
    });
    expect(await screen.findByText("Moved to School trip.")).toBeInTheDocument();
  });

  it("says so when a memory cannot be moved, and keeps the dialog open", async () => {
    const engine = createMockEngine();
    vi.spyOn(engine, "refileMemory").mockRejectedValueOnce(new Error("no"));
    await openLeo(engine);
    await userEvent.click(screen.getByRole("button", { name: /^Leo needs a packed lunch/ }));
    await userEvent.click(screen.getByRole("button", { name: "Is this about something else?" }));
    const dialog = await screen.findByRole("dialog", { name: "Is this about something else?" });
    await userEvent.click(await within(dialog).findByRole("radio", { name: /^Ava/ }));
    await userEvent.click(within(dialog).getByRole("button", { name: "Move it" }));
    expect(
      await within(dialog).findByText("That didn’t work. Nothing was changed."),
    ).toBeInTheDocument();
    expect(screen.getByRole("dialog", { name: "Is this about something else?" })).toBeVisible();
  });

  it("renames a person everywhere, keeps the old name, and undoes it", async () => {
    const { engine } = await openLeo();
    await userEvent.click(screen.getByRole("button", { name: "Rename" }));
    const dialog = await screen.findByRole("dialog", { name: "Rename" });
    const field = within(dialog).getByRole("textbox", { name: "What is it called?" });
    expect(field).toHaveFocus();
    expect(field).toHaveValue("Leo");
    await userEvent.clear(field);
    await userEvent.type(field, "Leopold{Enter}");
    expect(await screen.findByText("Renamed to Leopold.")).toBeInTheDocument();
    expect(await screen.findByRole("heading", { level: 1, name: "Leopold" })).toBeInTheDocument();
    expect(await screen.findByText("Also called Leo.")).toBeInTheDocument();
    expect(
      await screen.findByText("Leopold’s swimming is on Mondays at 17:30."),
    ).toBeInTheDocument();
    expect((await engine.getEntity("ent-leo")).name).toBe("Leopold");
    await waitFor(() =>
      expect(screen.getByRole("heading", { level: 1, name: "Leopold" })).toHaveFocus(),
    );

    await userEvent.click(screen.getByRole("button", { name: "Undo" }));
    expect(await screen.findByRole("heading", { level: 1, name: "Leo" })).toBeInTheDocument();
    expect(screen.queryByText(/Also called/)).toBeNull();
    expect(await screen.findByText("Leo’s swimming is on Mondays at 17:30.")).toBeInTheDocument();
  });

  it("goes back to the old name from the page, long after the toast has gone", async () => {
    const engine = createMockEngine();
    await engine.renameEntity("ent-leo", "Leopold");
    await browse("#/memories/ent-leo", engine);
    await userEvent.click(await screen.findByRole("button", { name: "Go back to Leo" }));
    expect(await screen.findByRole("heading", { level: 1, name: "Leo" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^Go back to/ })).toBeNull();
  });

  it("offers a merge when the new name is one it already knows, and merges on a clear yes", async () => {
    const { engine } = await openLeo();
    const merge = vi.spyOn(engine, "mergeEntities");
    await userEvent.click(screen.getByRole("button", { name: "Rename" }));
    const field = screen.getByRole("textbox", { name: "What is it called?" });
    await userEvent.clear(field);
    await userEvent.type(field, "ava{Enter}");
    const dialog = await screen.findByRole("dialog", { name: "Is this the same as Ava?" });
    expect(within(dialog).getByText(/Nothing is deleted, and you can undo it/)).toBeInTheDocument();
    expect(merge).not.toHaveBeenCalled();
    expect((await engine.getEntity("ent-leo")).name).toBe("Leo");
    await userEvent.click(within(dialog).getByRole("button", { name: "Merge them" }));
    expect(merge).toHaveBeenCalledWith("ent-leo", "ent-ava");
    expect(await screen.findByText("Merged into Ava.")).toBeInTheDocument();
    expect(await screen.findByRole("heading", { level: 1, name: "Ava" })).toBeInTheDocument();
    expect(window.location.hash).toBe("#/memories/ent-ava");
    expect(await screen.findByText("Also called Leo.")).toBeInTheDocument();
    // What was said about Leo now says Ava, the name that remains.
    expect(await screen.findByText("Ava is allergic to nuts.")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Undo" }));
    expect(await screen.findByRole("heading", { level: 1, name: "Leo" })).toBeInTheDocument();
    expect(window.location.hash).toBe("#/memories/ent-leo");
    expect(await screen.findByText("Leo is allergic to nuts.")).toBeInTheDocument();
  });

  it("lets the person keep two things apart instead of merging", async () => {
    const { engine } = await openLeo();
    const merge = vi.spyOn(engine, "mergeEntities");
    await userEvent.click(screen.getByRole("button", { name: "Rename" }));
    const field = screen.getByRole("textbox", { name: "What is it called?" });
    await userEvent.clear(field);
    await userEvent.type(field, "Ava{Enter}");
    const dialog = await screen.findByRole("dialog", { name: "Is this the same as Ava?" });
    await userEvent.click(within(dialog).getByRole("button", { name: "Keep them separate" }));
    expect(await screen.findByRole("dialog", { name: "Rename" })).toBeVisible();
    expect(merge).not.toHaveBeenCalled();
    expect(screen.getByRole("textbox", { name: "What is it called?" })).toHaveValue("Ava");
  });

  it("asks for a name and says when renaming did not work", async () => {
    const engine = createMockEngine();
    vi.spyOn(engine, "renameEntity").mockRejectedValueOnce(new Error("no"));
    await openLeo(engine);
    await userEvent.click(screen.getByRole("button", { name: "Rename" }));
    const field = screen.getByRole("textbox", { name: "What is it called?" });
    await userEvent.clear(field);
    await userEvent.keyboard("{Enter}");
    expect(await screen.findByText("Type a name.")).toBeInTheDocument();
    await userEvent.type(field, "Leopold{Enter}");
    expect(await screen.findByText("That didn’t work. Nothing was changed.")).toBeInTheDocument();
    expect(screen.getByRole("dialog", { name: "Rename" })).toBeVisible();
    await userEvent.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(screen.getByRole("heading", { level: 1, name: "Leo" })).toBeInTheDocument();
  });
});

describe("In your journal", () => {
  it("lists the days a person appears in, newest first, each opening that day", async () => {
    const engine = createMockEngine();
    const days = await engine.entityJournalDays("ent-gran");
    expect(days.length).toBeGreaterThan(0);
    await browse("#/memories/ent-gran", engine);
    const section = await screen.findByRole("region", { name: "In your journal" });
    const links = within(section).getAllByRole("link");
    expect(links.map((a) => a.getAttribute("href"))).toEqual(days.map((d) => `#/journal/${d}`));
    expect(links[0]?.textContent).toMatch(/^[A-Z][a-z]{2} \d{1,2} [A-Z][a-z]{2}$/);
    expect(days).toEqual([...days].sort().reverse());
  });

  it("is absent when the person is in no entry", async () => {
    const engine = createMockEngine();
    vi.spyOn(engine, "entityJournalDays").mockResolvedValue([]);
    await browse("#/memories/ent-gran", engine);
    await screen.findByRole("heading", { level: 1, name: /Gran/ });
    expect(screen.queryByText("In your journal")).toBeNull();
  });

  it("is absent for a thing, and its days are not even asked for", async () => {
    const engine = createMockEngine();
    const things = await engine.listEntities("thing", null);
    const thing = things[0];
    expect(thing).toBeDefined();
    const spy = vi.spyOn(engine, "entityJournalDays");
    await browse(`#/memories/${thing?.id}`, engine);
    await screen.findByRole("heading", { level: 1, name: thing?.name });
    expect(screen.queryByText("In your journal")).toBeNull();
    expect(spy).not.toHaveBeenCalled();
  });
});
