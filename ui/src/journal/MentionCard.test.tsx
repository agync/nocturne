import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createMockEngine, type MockEngine } from "../engine/mock/engine";
import { renderApp, setViewport } from "../test/render";

const NOW = new Date("2026-11-19T21:20:00Z");

beforeEach(() => setViewport(1280));
afterEach(() => {
  window.location.hash = "";
});

const open = async (): Promise<MockEngine> => {
  const engine = createMockEngine({ now: NOW, journal: { mode: "day", delayMs: 0 } });
  renderApp({ hash: "#/journal", engine });
  await screen.findByRole("list", { name: "In this entry" });
  return engine;
};

const strip = () => screen.getByRole("list", { name: "In this entry" });
const chip = (name: RegExp) => within(strip()).getByRole("button", { name });

describe("the card for a name in an entry", () => {
  it("asks about someone new and keeps them with the role that was typed", async () => {
    const engine = await open();
    const spy = vi.spyOn(engine, "confirmMention");
    await userEvent.click(chip(/Priya/));
    const card = await screen.findByRole("dialog", { name: "Priya" });
    expect(within(card).getByText("Nocturne thinks Priya is someone new.")).toBeInTheDocument();
    await userEvent.type(
      within(card).getByLabelText("Who is Priya to you? (optional)"),
      "a friend from climbing",
    );
    await userEvent.click(within(card).getByRole("button", { name: "That’s right" }));
    expect(spy).toHaveBeenCalledWith(expect.stringMatching(/^men-/), "a friend from climbing");
    expect(await screen.findByText("Thanks. Priya is kept.")).toBeInTheDocument();
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Priya" })).toBeNull());
  });

  it("shows a known person with their role and the memories this entry added", async () => {
    await open();
    await userEvent.click(chip(/^Gran$/));
    const card = await screen.findByRole("dialog", { name: "Gran" });
    expect(within(card).getByText("Mother · 1 memory from this entry")).toBeInTheDocument();
    expect(within(card).getByRole("link", { name: "Open Gran" })).toHaveAttribute(
      "href",
      "#/memories/ent-gran",
    );
    expect(within(card).queryByLabelText(/Who is Gran to you/)).toBeNull();
  });

  it("offers the wrong-kind action for the kind of the name", async () => {
    await open();
    await userEvent.click(chip(/Mill Lane café/));
    const card = await screen.findByRole("dialog", { name: "Mill Lane café" });
    expect(within(card).getByRole("button", { name: "Not a place" })).toBeInTheDocument();
    expect(
      within(card).getByRole("button", { name: "Not about Mill Lane café" }),
    ).toBeInTheDocument();
  });

  it("removes the link, says what that did, and Undo brings it back", async () => {
    const engine = await open();
    const remove = vi.spyOn(engine, "removeMention");
    const restore = vi.spyOn(engine, "restoreMention");
    await userEvent.click(chip(/Gran/));
    await userEvent.click(await screen.findByRole("button", { name: "Not about Gran" }));
    expect(remove).toHaveBeenCalled();
    expect(
      await screen.findByText(
        "Removed Gran from this entry. What it said about them is let go, not deleted.",
      ),
    ).toBeInTheDocument();
    await waitFor(() =>
      expect(within(strip()).queryByRole("button", { name: /^Gran$/ })).toBeNull(),
    );
    await userEvent.click(screen.getByRole("button", { name: "Undo" }));
    expect(restore).toHaveBeenCalledWith(remove.mock.calls[0]?.[0]);
    await waitFor(() =>
      expect(within(strip()).getByRole("button", { name: /^Gran$/ })).toBeInTheDocument(),
    );
  });

  it("says it for a place or a thing in its own words", async () => {
    const engine = await open();
    vi.spyOn(engine, "removeMention");
    await userEvent.click(chip(/the allotment/));
    await userEvent.click(await screen.findByRole("button", { name: "Not about the allotment" }));
    expect(
      await screen.findByText(
        "Removed the allotment from this entry. What it said about it is let go, not deleted.",
      ),
    ).toBeInTheDocument();
  });

  it("files the entry's memories under someone else through the mention", async () => {
    const engine = await open();
    const refile = vi.spyOn(engine, "refileMention");
    const memory = vi.spyOn(engine, "refileMemory");
    await userEvent.click(chip(/Priya/));
    await userEvent.click(await screen.findByRole("button", { name: "Someone else…" }));
    const dialog = await screen.findByRole("dialog", { name: "Is this about something else?" });
    expect(
      within(dialog).getByText(
        "Choose who or what this belongs to instead of Priya. Only the memories from this entry move, and their words stay as they are.",
      ),
    ).toBeInTheDocument();
    await userEvent.click(await within(dialog).findByRole("radio", { name: /^Leo Person/ }));
    await userEvent.click(within(dialog).getByRole("button", { name: "Move it" }));
    await waitFor(() => expect(refile).toHaveBeenCalled());
    expect(refile.mock.calls[0]?.[1]).toEqual({ type: "existing", entityId: "ent-leo" });
    expect(memory).not.toHaveBeenCalled();
    expect(await screen.findByText("Moved to Leo.")).toBeInTheDocument();
  });

  it("starts with the kind when it is the wrong kind", async () => {
    const engine = await open();
    const refile = vi.spyOn(engine, "refileMention");
    await userEvent.click(chip(/Priya/));
    await userEvent.click(await screen.findByRole("button", { name: "Not a person" }));
    const dialog = await screen.findByRole("dialog", { name: "Is this about something else?" });
    expect(within(dialog).getByRole("radio", { name: "A place" })).toBeChecked();
    expect(within(dialog).queryByRole("radio", { name: "A person" })).toBeNull();
    await userEvent.click(within(dialog).getByRole("radio", { name: "Someone or something new" }));
    await userEvent.type(within(dialog).getByLabelText("What is it called?"), "Priya’s wall");
    await userEvent.click(within(dialog).getByRole("button", { name: "Move it" }));
    await waitFor(() => expect(refile).toHaveBeenCalled());
    expect(refile.mock.calls[0]?.[1]).toEqual({ type: "new", name: "Priya’s wall", kind: "place" });
  });

  it("closes on Escape and gives focus back to the chip", async () => {
    await open();
    const priya = chip(/Priya/);
    await userEvent.click(priya);
    await screen.findByRole("dialog", { name: "Priya" });
    await userEvent.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Priya" })).toBeNull());
    expect(priya).toHaveFocus();
  });

  it("gives focus back to the chip after confirm, remove and refile", async () => {
    const engine = await open();
    vi.spyOn(engine, "refileMention");
    const returns = async (name: RegExp, act: () => Promise<void>) => {
      const el = chip(name);
      const focus = vi.spyOn(el, "focus");
      await userEvent.click(el);
      await screen.findByRole("dialog");
      focus.mockClear();
      await act();
      await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
      expect(focus).toHaveBeenCalled();
    };
    await returns(/Priya/, async () => {
      await userEvent.click(screen.getByRole("button", { name: "That’s right" }));
    });
    await returns(/^Gran$/, async () => {
      await userEvent.click(screen.getByRole("button", { name: "Not about Gran" }));
    });
    await returns(/the allotment/, async () => {
      await userEvent.click(screen.getByRole("button", { name: "Someone else…" }));
      const dialog = await screen.findByRole("dialog", { name: "Is this about something else?" });
      await userEvent.click(await within(dialog).findByRole("radio", { name: /^Leo Person/ }));
      await userEvent.click(within(dialog).getByRole("button", { name: "Move it" }));
    });
  });

  it("keeps Tab inside the card", async () => {
    await open();
    await userEvent.click(chip(/Gran/));
    const card = await screen.findByRole("dialog", { name: "Gran" });
    const buttons = within(card).getAllByRole("button");
    const last = buttons[buttons.length - 1] as HTMLElement;
    last.focus();
    await userEvent.tab();
    expect(card.contains(document.activeElement)).toBe(true);
    await userEvent.tab({ shift: true });
    expect(card.contains(document.activeElement)).toBe(true);
    (within(card).getByRole("link", { name: "Open Gran" }) as HTMLElement).focus();
    await userEvent.tab({ shift: true });
    expect(card.contains(document.activeElement)).toBe(true);
  });

  it("opens the same card from the side column", async () => {
    await open();
    const side = screen.getByRole("heading", { level: 2, name: "People and places today" })
      .parentElement as HTMLElement;
    await userEvent.click(within(side).getByRole("button", { name: /Priya/ }));
    expect(await screen.findByRole("dialog", { name: "Priya" })).toBeInTheDocument();
  });
});
