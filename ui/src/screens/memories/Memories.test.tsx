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
    expect(leo).toHaveTextContent("5");
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
    expect(screen.getByText("Son · 5 memories from 4 things you kept")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Mark all private" })).toBeInTheDocument();
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
    ).toEqual(["Correct", "Keep forever", "Mark private", "Forget…"]);
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
    await userEvent.click(screen.getByRole("button", { name: "Forget…" }));
    const dialog = await screen.findByRole("dialog", { name: "Forget 1 thing?" });
    await userEvent.click(await within(dialog).findByRole("button", { name: /^Forget 2 things/ }));
    expect(await within(dialog).findByText("Gone.")).toBeInTheDocument();
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
    await waitFor(() => expect(spy).toHaveBeenCalledTimes(5));
    expect(await screen.findByText("Marked 5 memories private.")).toBeInTheDocument();
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
    expect(await screen.findByRole("alert")).toHaveTextContent("I couldn’t find that.");
    expect(
      screen.getByRole("link", { name: "Back to what Nocturne remembers" }),
    ).toBeInTheDocument();
  });

  it("says so when the list cannot be read", async () => {
    const engine = createMockEngine();
    vi.spyOn(engine, "listEntities").mockRejectedValue(new Error("no"));
    renderApp({ hash: "#/memories", engine });
    expect(
      await screen.findByText("I couldn’t open this. What you kept is safe."),
    ).toBeInTheDocument();
  });
});
