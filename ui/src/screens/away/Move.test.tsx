import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { manualEngine, openAway } from "./testing";

afterEach(() => {
  window.location.hash = "";
  vi.restoreAllMocks();
});

const offered = (here: boolean) => ({
  phase: "offered" as const,
  name: "Maya’s phone",
  items: 1284,
  bytes: 1_400_000_000,
  here,
});

async function withOffer(here = false) {
  const engine = manualEngine("onAlone");
  const view = await openAway("/settings/away", "onAlone", "Away from home", engine);
  act(() => engine.setMove(offered(here)));
  const dialog = await screen.findByRole("alertdialog", {
    name: "Maya’s phone has its own memory. Move it here?",
  });
  return { ...view, engine, dialog };
}

describe("a phone with its own memory", () => {
  it("asks to move it here, with what it holds, and Move it here takes it", async () => {
    const { engine, dialog } = await withOffer();
    expect(dialog).toHaveTextContent("1,284 things kept · about 1.4 GB");
    expect(
      within(dialog).getByText(
        "This computer becomes its home: it reads, tidies and writes the page. Maya’s phone then works with it like your other phones.",
      ),
    ).toBeInTheDocument();
    const names = within(dialog)
      .getAllByRole("button")
      .map((b) => b.textContent);
    expect(names).toEqual(["Not now", "Move it here"]);
    const answer = vi.spyOn(engine, "answerMove");
    await userEvent.click(within(dialog).getByRole("button", { name: "Move it here" }));
    expect(answer).toHaveBeenCalledWith("move");
    expect(await screen.findByRole("progressbar")).toHaveAccessibleName(
      "Arriving from Maya’s phone · 0 B of 1.4 GB",
    );
  });

  it("gives the choice when this computer already holds a memory", async () => {
    const { engine, dialog } = await withOffer(true);
    expect(within(dialog).getByText(/This computer already holds a memory\./)).toBeInTheDocument();
    const names = within(dialog)
      .getAllByRole("button")
      .map((b) => b.textContent);
    expect(names).toEqual(["Keep what’s here", "Put this aside and move it here"]);
    const answer = vi.spyOn(engine, "answerMove");
    await userEvent.click(within(dialog).getByRole("button", { name: "Keep what’s here" }));
    expect(answer).toHaveBeenCalledWith("notNow");
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
  });

  it("takes focus, and Escape means Not now", async () => {
    const { engine, dialog } = await withOffer();
    await waitFor(() => expect(dialog).toHaveFocus());
    const answer = vi.spyOn(engine, "answerMove");
    await userEvent.keyboard("{Escape}");
    expect(answer).toHaveBeenCalledWith("notNow");
  });

  it("stays while the memory arrives and is checked, with nothing to press", async () => {
    const { engine } = await withOffer();
    act(() => engine.setMove({ phase: "arriving", name: "Maya’s phone", done: 7e8, size: 1.4e9 }));
    const bar = await screen.findByRole("progressbar");
    expect(bar).toHaveAccessibleName("Arriving from Maya’s phone · 700 MB of 1.4 GB");
    expect(within(screen.getByRole("alertdialog")).queryByRole("button")).toBeNull();
    await userEvent.keyboard("{Escape}");
    expect(screen.getByRole("alertdialog")).toBeInTheDocument();
    act(() => engine.setMove({ phase: "checking", name: "Maya’s phone" }));
    expect(await screen.findByRole("progressbar")).toHaveAccessibleName(
      "Checking · every count must match before Maya’s phone lets go of it",
    );
  });

  it("says the memory is here, and Open today’s page finishes it", async () => {
    const { engine } = await withOffer();
    act(() => engine.setMove({ phase: "arrived", name: "Maya’s phone", online: false }));
    const dialog = await screen.findByRole("alertdialog", {
      name: "The memory from Maya’s phone is here.",
    });
    expect(
      within(dialog).getByText("Reading carries on from where it was. Nothing is read again."),
    ).toBeInTheDocument();
    expect(dialog).toHaveTextContent(
      "To have answers written online here, set up Online answers in Settings.",
    );
    const finish = vi.spyOn(engine, "finishMove");
    await userEvent.click(within(dialog).getByRole("button", { name: "Open today’s page" }));
    expect(finish).toHaveBeenCalled();
    await waitFor(() => expect(window.location.hash).toBe("#/today"));
  });

  it("leaves out the Online answers line once it is set up here", async () => {
    const { engine } = await withOffer();
    act(() => engine.setMove({ phase: "arrived", name: "Maya’s phone", online: true }));
    const dialog = await screen.findByRole("alertdialog", {
      name: "The memory from Maya’s phone is here.",
    });
    expect(dialog).not.toHaveTextContent("Online answers");
  });
});
