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

async function withOffer() {
  const engine = manualEngine("onAlone");
  const view = await openAway("/settings/away", "onAlone", "Away from home", engine);
  act(() => engine.setMove(offered(false)));
  const dialog = await screen.findByRole("alertdialog", {
    name: "Maya’s phone has its own memory. Move it here?",
  });
  return { ...view, engine, dialog };
}

const CHOOSE = "Maya’s phone has its own memory, and this computer has one too.";

async function withChoice(room: { need: number; free: number } | null = null) {
  const engine = manualEngine("onAlone");
  const view = await openAway("/settings/away", "onAlone", "Away from home", engine);
  act(() => engine.setMove({ ...offered(true), since: "2026-03-03", room }));
  const dialog = await screen.findByRole("alertdialog", { name: CHOOSE });
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
      "Maya’s phone is getting its memory ready…",
    );
  });

  it("takes focus, and Escape means Not now", async () => {
    const { engine, dialog } = await withOffer();
    await waitFor(() => expect(dialog).toHaveFocus());
    const answer = vi.spyOn(engine, "answerMove");
    await userEvent.keyboard("{Escape}");
    expect(answer).toHaveBeenCalledWith("notNow");
  });

  it("says the phone is getting its memory ready, with a bar of no amount and Stop, until the first part arrives", async () => {
    const { engine } = await withOffer();
    await userEvent.click(screen.getByRole("button", { name: "Move it here" }));
    const bar = await screen.findByRole("progressbar");
    expect(bar).toHaveAccessibleName("Maya’s phone is getting its memory ready…");
    expect(bar).not.toHaveAttribute("aria-valuenow");
    expect(screen.queryByText(/0\sB of/)).toBeNull();
    const stop = vi.spyOn(engine, "stopMove");
    await userEvent.click(screen.getByRole("button", { name: "Stop" }));
    await userEvent.click(screen.getByRole("button", { name: "Stop moving" }));
    expect(stop).toHaveBeenCalled();
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

describe("a phone joining a computer that already holds a memory", () => {
  it("asks how it joins, with bringing its things in chosen", async () => {
    const { dialog } = await withChoice();
    expect(dialog).toHaveTextContent(
      "1,284 things kept on Maya’s phone since 3 March · about 1.4 GB",
    );
    expect(dialog).toHaveTextContent(
      "A computer holds one person’s memory. Choose how Maya’s phone joins this one.",
    );
    const bring = within(dialog).getByRole("radio", { name: /Bring the phone’s things in/ });
    const use = within(dialog).getByRole("radio", { name: /Use this computer’s memory/ });
    expect(bring).toBeChecked();
    expect(use).not.toBeChecked();
    expect(dialog).toHaveTextContent(
      "What was kept on Maya’s phone, with its journal and check-ins, joins the memory here, and Nocturne reads it as new. Its calendar and health readings are sent again, with nothing doubled.",
    );
    expect(dialog).toHaveTextContent(
      "Nothing comes in from Maya’s phone. It works with this computer like your other phones, and what it holds now is put aside on it.",
    );
    const names = within(dialog)
      .getAllByRole("button")
      .map((b) => b.textContent);
    expect(names).toEqual(["Not now", "Continue"]);
  });

  it("brings the phone's things in, which can be stopped until they are added", async () => {
    const { engine, dialog } = await withChoice();
    const answer = vi.spyOn(engine, "answerMove");
    await userEvent.click(within(dialog).getByRole("button", { name: "Continue" }));
    expect(answer).toHaveBeenCalledWith("bring");
    expect(await screen.findByRole("progressbar")).toHaveAccessibleName(
      "Maya’s phone is getting its things ready…",
    );
    act(() =>
      engine.setMove({
        phase: "arriving",
        name: "Maya’s phone",
        done: 7e8,
        size: 1.4e9,
        bring: true,
      }),
    );
    expect(await screen.findByRole("progressbar")).toHaveAccessibleName(
      "Bringing in Maya’s phone’s things · 700\u00a0MB of 1.4\u00a0GB",
    );
    await userEvent.click(screen.getByRole("button", { name: "Stop" }));
    expect(
      screen.getByText(
        "Stop bringing them in? Nothing changes here, and Maya’s phone keeps its memory.",
      ),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Keep going" }));
    act(() => engine.setMove({ phase: "adding", name: "Maya’s phone" }));
    expect(await screen.findByRole("progressbar")).toHaveAccessibleName(
      "Adding them to the memory here…",
    );
    expect(within(screen.getByRole("alertdialog")).queryByRole("button")).toBeNull();
  });

  it("stops bringing them in when asked", async () => {
    const { engine } = await withChoice();
    act(() => engine.setMove({ phase: "preparing", name: "Maya’s phone", bring: true }));
    await screen.findByRole("progressbar");
    const stop = vi.spyOn(engine, "stopMove");
    await userEvent.click(screen.getByRole("button", { name: "Stop" }));
    const sure = within(screen.getByRole("alertdialog")).getAllByRole("button", { name: "Stop" });
    const confirm = sure.at(-1);
    if (!confirm) throw new Error("a Stop to confirm");
    await userEvent.click(confirm);
    expect(stop).toHaveBeenCalled();
  });

  it("says what was added, and Open today’s page finishes it", async () => {
    const { engine } = await withChoice();
    act(() =>
      engine.setMove({
        phase: "brought",
        name: "Maya’s phone",
        added: { things: 1284, entries: 212, checkins: 96 },
      }),
    );
    const dialog = await screen.findByRole("alertdialog", {
      name: "Maya’s phone’s things are here.",
    });
    expect(dialog).toHaveTextContent(
      "1,284 things, 212 journal entries and 96 check-ins were added. Nocturne reads them as new over the next while; finding them by their words works now.",
    );
    const finish = vi.spyOn(engine, "finishMove");
    await userEvent.click(within(dialog).getByRole("button", { name: "Open today’s page" }));
    expect(finish).toHaveBeenCalled();
    await waitFor(() => expect(window.location.hash).toBe("#/today"));
  });

  it("uses this computer's memory, and says nothing came in", async () => {
    const { engine, dialog } = await withChoice();
    await userEvent.click(
      within(dialog).getByRole("radio", { name: /Use this computer’s memory/ }),
    );
    const answer = vi.spyOn(engine, "answerMove");
    await userEvent.click(within(dialog).getByRole("button", { name: "Continue" }));
    expect(answer).toHaveBeenCalledWith("useHere");
    const used = await screen.findByRole("alertdialog", {
      name: "Maya’s phone now works with this computer.",
    });
    expect(used).toHaveTextContent(
      "Nothing came in from it. Its calendar and health come here from now on.",
    );
    await userEvent.click(within(used).getByRole("button", { name: "OK" }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
  });

  it("waits for room to bring the things in, but not to use this memory", async () => {
    const { dialog } = await withChoice({ need: 2_900_000_000, free: 1_200_000_000 });
    expect(dialog).toHaveTextContent(
      "This computer needs about 2.9 GB free for it and has 1.2 GB. Free some room here, then choose Continue.",
    );
    expect(within(dialog).getByRole("button", { name: "Continue" })).toBeDisabled();
    await userEvent.click(
      within(dialog).getByRole("radio", { name: /Use this computer’s memory/ }),
    );
    expect(within(dialog).getByRole("button", { name: "Continue" })).toBeEnabled();
    expect(dialog).not.toHaveTextContent("Free some room here");
  });

  it("means Not now on Escape, and never offers to put this memory aside", async () => {
    const { engine, dialog } = await withChoice();
    expect(dialog).not.toHaveTextContent("Put this aside");
    const answer = vi.spyOn(engine, "answerMove");
    await userEvent.keyboard("{Escape}");
    expect(answer).toHaveBeenCalledWith("notNow");
  });
});
