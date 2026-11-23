import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { manualEngine, openAway } from "./testing";

afterEach(() => {
  window.location.hash = "";
  vi.restoreAllMocks();
});

const offered = {
  phase: "offered" as const,
  name: "Maya’s phone",
  items: 1284,
  bytes: 1_400_000_000,
};

/** An empty computer asks nothing: the phone's things are on their way at once. */
async function withThingsComing() {
  const engine = manualEngine("onAlone");
  const view = await openAway("/settings/away", "onAlone", "Away from home", engine);
  act(() => engine.setMove({ phase: "preparing", name: "Maya’s phone" }));
  const dialog = await screen.findByRole("alertdialog", {
    name: "Maya’s phone is getting its things ready…",
  });
  return { ...view, engine, dialog };
}

const CHOOSE = "Maya’s phone has its own memory, and this computer has one too.";

async function withChoice(room: { need: number; free: number } | null = null) {
  const engine = manualEngine("onAlone");
  const view = await openAway("/settings/away", "onAlone", "Away from home", engine);
  act(() => engine.setMove({ ...offered, since: "2026-03-03", room }));
  const dialog = await screen.findByRole("alertdialog", { name: CHOOSE });
  return { ...view, engine, dialog };
}

describe("a phone bringing its things to an empty computer", () => {
  it("asks nothing, says the phone is getting its things ready with a bar of no amount, and can be stopped", async () => {
    const { engine, dialog } = await withThingsComing();
    await waitFor(() => expect(dialog).toHaveFocus());
    expect(within(dialog).queryByRole("radio")).toBeNull();
    const bar = within(dialog).getByRole("progressbar");
    expect(bar).toHaveAccessibleName("Maya’s phone is getting its things ready…");
    expect(bar).not.toHaveAttribute("aria-valuenow");
    expect(screen.queryByText(/0\sB of/)).toBeNull();
    await userEvent.keyboard("{Escape}");
    expect(screen.getByRole("alertdialog")).toBeInTheDocument();
    const stop = vi.spyOn(engine, "stopMove");
    await userEvent.click(screen.getByRole("button", { name: "Stop" }));
    expect(
      screen.getByText(
        "Stop bringing them in? Nothing changes here, and Maya’s phone keeps its memory.",
      ),
    ).toBeInTheDocument();
    const sure = within(screen.getByRole("alertdialog")).getAllByRole("button", { name: "Stop" });
    const confirm = sure.at(-1);
    if (!confirm) throw new Error("a Stop to confirm");
    await userEvent.click(confirm);
    expect(stop).toHaveBeenCalled();
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
  });

  it("says one thing in the singular when it is here", async () => {
    const { engine } = await withThingsComing();
    act(() =>
      engine.setMove({
        phase: "brought",
        name: "Maya’s phone",
        added: { things: 1, entries: 1, checkins: 1 },
      }),
    );
    expect(
      await screen.findByText(/^1 thing, 1 journal entry and 1 check-in were added\./),
    ).toBeInTheDocument();
  });

  it("says room ran out, with OK", async () => {
    const { engine } = await withThingsComing();
    act(() =>
      engine.setMove({
        phase: "failed",
        name: "Maya’s phone",
        why: "noRoom",
        room: { need: 4_300_000_000, free: 0 },
      }),
    );
    const dialog = await screen.findByRole("alertdialog");
    await waitFor(() =>
      expect(dialog).toHaveTextContent(
        "There wasn’t room for Maya’s phone’s memory, so nothing changed. Free about 4.3 GB here, then move it again from Maya’s phone.",
      ),
    );
    await userEvent.click(within(dialog).getByRole("button", { name: "OK" }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
  });

  it("closes once Open today’s page is pressed, though the computer sends no change", async () => {
    const { engine } = await withThingsComing();
    act(() =>
      engine.setMove({
        phase: "brought",
        name: "Maya’s phone",
        added: { things: 3, entries: 0, checkins: 0 },
      }),
    );
    const dialog = await screen.findByRole("alertdialog", {
      name: "Maya’s phone’s things are here.",
    });
    let waiting = true;
    vi.spyOn(engine, "finishMove").mockImplementation(async () => {
      waiting = false;
    });
    const get = engine.getMove;
    vi.spyOn(engine, "getMove").mockImplementation(async () => (waiting ? get() : null));
    await userEvent.click(within(dialog).getByRole("button", { name: "Open today’s page" }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
  });
});

describe("a phone joining a computer that already holds a memory", () => {
  it("asks how it joins, with bringing its things in chosen", async () => {
    const { dialog } = await withChoice();
    await waitFor(() => expect(dialog).toHaveFocus());
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

  it("drops the answered question once the things come in", async () => {
    const { engine } = await withChoice();
    act(() => engine.setMove({ phase: "preparing", name: "Maya’s phone" }));
    const dialog = await screen.findByRole("alertdialog", {
      name: "Maya’s phone is getting its things ready…",
    });
    expect(dialog).not.toHaveTextContent("this computer has one too");
    act(() => engine.setMove({ phase: "adding", name: "Maya’s phone" }));
    expect(
      await screen.findByRole("alertdialog", { name: "Adding them to the memory here…" }),
    ).not.toHaveTextContent("this computer has one too");
  });

  it("stops bringing them in when asked", async () => {
    const { engine } = await withChoice();
    act(() => engine.setMove({ phase: "preparing", name: "Maya’s phone" }));
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

  it("says the phone was removed, so nothing came in, or that the phone stopped it", async () => {
    const { engine } = await withChoice();
    act(() => engine.setMove({ phase: "ended", name: "Maya’s phone", why: "removed" }));
    expect(
      await screen.findByRole("alertdialog", {
        name: "Maya’s phone was removed, so nothing came in from it.",
      }),
    ).toBeInTheDocument();
    act(() => engine.setMove({ phase: "ended", name: "Maya’s phone", why: "stopped" }));
    expect(
      await screen.findByRole("alertdialog", {
        name: "Maya’s phone stopped bringing its things in. Nothing changed here.",
      }),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "OK" }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
  });

  it("says what was added though the phone was removed while it was added", async () => {
    const { engine } = await withChoice();
    act(() =>
      engine.setMove({
        phase: "brought",
        name: "Maya’s phone",
        added: { things: 3, entries: 2, checkins: 1 },
        removed: true,
      }),
    );
    const dialog = await screen.findByRole("alertdialog", {
      name: "Maya’s phone’s things are here.",
    });
    expect(dialog).toHaveTextContent(
      "Maya’s phone was removed while they were added, so it still holds its own memory.",
    );
    expect(within(dialog).getByRole("button", { name: "Open today’s page" })).toBeInTheDocument();
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

  it("says how many things without a first day, and one in the singular", async () => {
    const engine = manualEngine("onAlone");
    await openAway("/settings/away", "onAlone", "Away from home", engine);
    act(() => engine.setMove({ ...offered, items: 1 }));
    const dialog = await screen.findByRole("alertdialog", { name: CHOOSE });
    expect(dialog).toHaveTextContent("1 thing kept · about 1.4 GB");
  });

  it("means Not now on Escape, and never offers to put this memory aside", async () => {
    const { engine, dialog } = await withChoice();
    expect(dialog).not.toHaveTextContent("Put this aside");
    const answer = vi.spyOn(engine, "answerMove");
    await userEvent.keyboard("{Escape}");
    expect(answer).toHaveBeenCalledWith("notNow");
  });
});
