import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { createMockEngine, type MockEngine } from "../../engine/mock/engine";
import { renderApp, setViewport } from "../../test/render";

const NOW = new Date("2026-11-19T21:20:00Z");

beforeEach(() => setViewport(1280));
afterEach(() => {
  window.location.hash = "";
});

const engineIn = (): MockEngine =>
  createMockEngine({ now: NOW, journal: { mode: "day", delayMs: 0 } });

async function open(id: string, engine = engineIn()) {
  renderApp({ hash: `#/items/${id}`, engine });
  const title = await screen.findByRole("heading", { level: 1 });
  return { engine, title };
}

describe("a check-in's own page", () => {
  it("shows the feeling, the line and where it sits, with a way back to the journal", async () => {
    const { title } = await open("j-ci-tired");
    expect(title).toHaveTextContent("Low · tired");
    expect(screen.getByText("Check-in · Thu 19 Nov · 8:05 am")).toBeInTheDocument();
    expect(within(screen.getByRole("main")).getByRole("link", { name: /Journal/ })).toHaveAttribute(
      "href",
      "#/journal/2026-11-19",
    );
    expect(screen.getByText("“Bad night.”")).toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 2, name: "What you wrote" })).toBeInTheDocument();
    expect(screen.getByText("Bad night.", { selector: "p" })).toBeInTheDocument();
    expect(
      screen.getByText(
        "Check-ins stay in your journal. Nocturne never makes memories from them, so they never fade and Ask never uses them.",
      ),
    ).toBeInTheDocument();
    expect(screen.getByRole("img", { name: "Low · tired, 8:05 am" })).toBeInTheDocument();
  });

  it("leaves out what a memory has and a check-in does not", async () => {
    await open("j-ci-tired");
    for (const name of ["Mark private", "Keep forever", "Read it again"]) {
      expect(screen.queryByRole("button", { name })).toBeNull();
    }
    expect(screen.queryByText(/Understood in/)).toBeNull();
    expect(screen.queryByText(/Memories from it/)).toBeNull();
    expect(screen.queryByText(/Nothing has been remembered/)).toBeNull();
  });

  it("shows the names in the check-in as chips", async () => {
    await open("j-ci-relieved");
    expect(screen.getByRole("heading", { level: 2, name: "In this check-in" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Gran/ })).toBeInTheDocument();
  });

  it("hides the sections that have nothing in them", async () => {
    await open("j-ci-pleased");
    expect(screen.queryByRole("heading", { level: 2, name: "What you wrote" })).toBeNull();
    expect(screen.queryByRole("heading", { level: 2, name: "In this check-in" })).toBeNull();
    expect(screen.queryByRole("heading", { level: 2, name: /feel like doing/ })).toBeNull();
  });

  it("changes the check-in through the check-in screen", async () => {
    await open("j-ci-tired");
    await userEvent.click(screen.getByRole("button", { name: "Change" }));
    expect(window.location.hash).toBe("#/journal/check-in?change=j-ci-tired");
  });

  it("leaves it out of reflections and brings it back", async () => {
    const { engine } = await open("j-ci-tired");
    await userEvent.click(screen.getByRole("button", { name: "Leave out of reflections" }));
    expect(await screen.findByText("Left out. It stays in your journal.")).toBeInTheDocument();
    const back = await screen.findByRole("button", { name: "Bring back into reflections" });
    expect(
      (await engine.getJournalDay()).checkins.find((c) => c.id === "j-ci-tired")?.leftOut,
    ).toBe(true);
    await userEvent.click(back);
    expect(await screen.findByText("Back in your reflections.")).toBeInTheDocument();
    expect(
      await screen.findByRole("button", { name: "Leave out of reflections" }),
    ).toBeInTheDocument();
  });
});

describe("deleting a check-in", () => {
  const openDialog = async (id = "j-ci-tired") => {
    const view = await open(id);
    await userEvent.click(screen.getByRole("button", { name: "Delete…" }));
    const dialog = await screen.findByRole("dialog", { name: "Delete this check-in?" });
    await within(dialog).findByRole("button", { name: "Delete check-in" });
    await waitFor(() =>
      expect(within(dialog).getByRole("button", { name: "Delete check-in" })).toBeEnabled(),
    );
    return { ...view, dialog };
  };

  it("says what goes, what cites it and what is out of reach, and starts on Keep it", async () => {
    const { dialog } = await openDialog();
    expect(
      within(dialog).getByText(
        "Low · tired at 8:05 am, and the line you wrote with it, go from this computer for good.",
      ),
    ).toBeInTheDocument();
    expect(
      within(dialog).getByText(
        "1 line in your reflections and recaps cites it and will be taken out.",
      ),
    ).toBeInTheDocument();
    expect(
      within(dialog).getByText(
        "Anything you exported or printed is out of reach. Nocturne can’t delete it.",
      ),
    ).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Keep it" })).toHaveFocus();
  });

  it("leaves out the line when there is none", async () => {
    const { dialog } = await openDialog("j-ci-relieved");
    expect(
      within(dialog).getByText(
        "Settled · relieved, content at 6:10 pm, and the line you wrote with it, go from this computer for good.",
      ),
    ).toBeInTheDocument();
  });

  it("closes with Keep it and changes nothing", async () => {
    const { engine, dialog } = await openDialog();
    await userEvent.click(within(dialog).getByRole("button", { name: "Keep it" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    await expect(engine.getItem("j-ci-tired")).resolves.toBeDefined();
  });

  it("deletes it, says so and goes to the journal day", async () => {
    const { engine, dialog } = await openDialog();
    await userEvent.click(within(dialog).getByRole("button", { name: "Delete check-in" }));
    expect(await screen.findByText("Deleted.")).toBeInTheDocument();
    expect(window.location.hash).toBe("#/journal/2026-11-19");
    await expect(engine.getItem("j-ci-tired")).rejects.toThrow();
  });
});
