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

  it("says the other phones send again when a memory is replaced", async () => {
    const engine = manualEngine("onAlone");
    await openAway("/settings/away", "onAlone", "Away from home", engine);
    act(() => engine.setMove({ ...offered(true), others: true }));
    const dialog = await screen.findByRole("alertdialog");
    expect(dialog).toHaveTextContent(
      "Your other phones send their calendar and health again the next time they connect, since what they sent stays with the memory put aside.",
    );
  });

  it("takes focus, and Escape means Not now", async () => {
    const { engine, dialog } = await withOffer();
    await waitFor(() => expect(dialog).toHaveFocus());
    const answer = vi.spyOn(engine, "answerMove");
    await userEvent.keyboard("{Escape}");
    expect(answer).toHaveBeenCalledWith("notNow");
  });

  it("stays while the memory arrives and is checked, and can be stopped until the restart", async () => {
    const { engine } = await withOffer();
    act(() => engine.setMove({ phase: "arriving", name: "Maya’s phone", done: 7e8, size: 1.4e9 }));
    const bar = await screen.findByRole("progressbar");
    expect(bar).toHaveAccessibleName("Arriving from Maya’s phone · 700\u00a0MB of 1.4\u00a0GB");
    await userEvent.keyboard("{Escape}");
    expect(screen.getByRole("alertdialog")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Stop" }));
    expect(
      screen.getByText("Stop this move? Nothing changes here, and Maya’s phone keeps its memory."),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Keep moving" }));
    expect(await screen.findByRole("progressbar")).toBeInTheDocument();

    act(() => engine.setMove({ phase: "checking", name: "Maya’s phone" }));
    expect(await screen.findByRole("progressbar")).toHaveAccessibleName(
      "Checking · every count must match before Maya’s phone lets go of it",
    );
    const stop = vi.spyOn(engine, "stopMove");
    await userEvent.click(screen.getByRole("button", { name: "Stop" }));
    await userEvent.click(screen.getByRole("button", { name: "Stop moving" }));
    expect(stop).toHaveBeenCalled();
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
  });

  it("says Nocturne restarts to open it, with nothing to press", async () => {
    const { engine } = await withOffer();
    act(() => engine.setMove({ phase: "restarting", name: "Maya’s phone" }));
    expect(
      await screen.findByText(
        "Nocturne restarts to open the memory from Maya’s phone. It opens again by itself.",
      ),
    ).toBeInTheDocument();
    expect(within(screen.getByRole("alertdialog")).queryByRole("button")).toBeNull();
  });

  it("waits for room before moving it here", async () => {
    const { engine } = await withOffer();
    act(() =>
      engine.setMove({
        ...offered(false),
        room: { need: 2_900_000_000, free: 1_200_000_000 },
      }),
    );
    const dialog = await screen.findByRole("alertdialog");
    await waitFor(() =>
      expect(dialog).toHaveTextContent(
        "This computer needs about 2.9 GB free for it and has 1.2 GB. Free some room here, then choose Move it here.",
      ),
    );
    expect(within(dialog).getByRole("button", { name: "Move it here" })).toBeDisabled();
  });

  it.each([
    [
      { why: "noRoom" as const, room: { need: 2_900_000_000, free: 0 } },
      "There wasn’t room for Maya’s phone’s memory, so nothing changed. Free about 2.9 GB here, then move it again from Maya’s phone.",
    ],
    [
      { why: "notOpened" as const, room: null },
      "The memory from Maya’s phone couldn’t be opened here, so nothing changed. What was on this computer is as it was.",
    ],
  ])("says how a move ended with nothing changed, with OK", async (failed, line) => {
    const { engine } = await withOffer();
    act(() => engine.setMove({ phase: "failed", name: "Maya’s phone", ...failed }));
    const dialog = await screen.findByRole("alertdialog");
    await waitFor(() => expect(dialog).toHaveTextContent(line));
    await userEvent.click(within(dialog).getByRole("button", { name: "OK" }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
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
