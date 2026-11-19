import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ForgetPreview } from "../../engine/bindings";
import { createMockEngine, type MockEngine } from "../../engine/mock/engine";
import { renderApp, setViewport } from "../../test/render";

const NOW = new Date("2026-11-19T21:20:00Z");

beforeEach(() => setViewport(1280));
afterEach(() => {
  window.location.hash = "";
  vi.restoreAllMocks();
});

const engineIn = (): MockEngine =>
  createMockEngine({ now: NOW, journal: { mode: "day", delayMs: 0 } });

async function open(engine: MockEngine = engineIn()) {
  renderApp({ hash: "#/journal", engine });
  await screen.findByRole("heading", { level: 2, name: "Through the day" });
  await userEvent.click(screen.getByRole("button", { name: "About this day" }));
  await userEvent.click(screen.getByRole("menuitem", { name: "Delete this day…" }));
  const dialog = await screen.findByRole("dialog", { name: "Delete Thursday 19 November?" });
  await within(dialog).findByRole("button", { name: /^Delete \d+ things?$/ });
  await within(dialog).findByRole("region", { name: "From other days" });
  return { engine, dialog };
}

const box = (dialog: HTMLElement, name: string | RegExp) =>
  within(dialog).getByRole("checkbox", { name }) as HTMLInputElement;

const settled = async (dialog: HTMLElement) => {
  await waitFor(() =>
    expect(within(dialog).queryByRole("button", { name: /^Delete/ })).not.toHaveAttribute(
      "aria-busy",
      "true",
    ),
  );
};

describe("Delete this day", () => {
  it("groups what was kept, with everything on the day ticked and the other day not", async () => {
    const { dialog } = await open();
    const kept = within(dialog).getByRole("region", { name: "Kept on this day" });
    expect(within(kept).getByRole("heading", { name: "Your entry" })).toBeInTheDocument();
    expect(within(kept).getByRole("heading", { name: "Through the day" })).toBeInTheDocument();
    const rows = within(kept).getAllByRole("checkbox");
    expect(rows.length).toBeGreaterThan(10);
    for (const row of rows) expect(row).toBeChecked();
    expect(within(kept).getByText(`${rows.length - 1} of ${rows.length - 1}`)).toBeInTheDocument();

    const other = within(dialog).getByRole("region", { name: "From other days" });
    expect(within(other).getByText("Wed 18 Nov")).toBeInTheDocument();
    expect(
      within(other).getByText(
        "These share a memory with this day. The shared memory goes; these stay unless you tick them.",
      ),
    ).toBeInTheDocument();
    expect(within(other).getByText("0 of 1")).toBeInTheDocument();
    expect(box(dialog, /Gran’s scan is tomorrow at ten/)).not.toBeChecked();
    expect(
      within(dialog).getByRole("button", { name: `Delete ${rows.length - 1} things` }),
    ).toBeEnabled();
  });

  it("shows each kind the way the design says", async () => {
    const { dialog } = await open();
    expect(within(dialog).getByText("Voice note, 1:42")).toBeInTheDocument();
    expect(within(dialog).getAllByText("Good thing")).toHaveLength(3);
    expect(within(dialog).getByText("Low · tired")).toBeInTheDocument();
    expect(within(dialog).getByText("“Bad night.”")).toBeInTheDocument();
    expect(within(dialog).getByText("Document")).toBeInTheDocument();
    expect(within(dialog).getAllByText("from My phone").length).toBeGreaterThan(0);
    expect(within(dialog).getAllByText("7:52 am").length).toBe(1);
    const leo = box(dialog, /Leo forgot his PE kit again/);
    expect(leo.closest("label")).toHaveAttribute("title", expect.stringContaining("school office"));
  });

  it("makes the group box tick, partly tick and clear its rows, and counts them", async () => {
    const { dialog } = await open();
    const kept = within(dialog).getByRole("region", { name: "Kept on this day" });
    const group = within(kept).getByRole("checkbox", {
      name: "Kept on this day",
    }) as HTMLInputElement;
    const total = within(kept).getAllByRole("checkbox").length - 1;
    await userEvent.click(box(dialog, /Leo forgot his PE kit again/));
    expect(group.indeterminate).toBe(true);
    expect(within(kept).getByText(`${total - 1} of ${total}`)).toBeInTheDocument();
    expect(
      within(dialog).getByRole("button", { name: `Delete ${total - 1} things` }),
    ).toBeInTheDocument();

    await userEvent.click(group);
    expect(group.checked).toBe(true);
    expect(group.indeterminate).toBe(false);
    await userEvent.click(group);
    expect(within(kept).getByText(`0 of ${total}`)).toBeInTheDocument();
    const none = within(dialog).getByRole("button", { name: "Delete" });
    expect(none).toBeDisabled();
    expect(
      within(dialog).getByText("Nothing is ticked, so nothing will be deleted."),
    ).toBeInTheDocument();
  });

  it("folds earlier versions into one row and ticks them together", async () => {
    const { engine, dialog } = await open();
    expect(within(dialog).getByText(/Changed 9:40 pm ·/)).toBeInTheDocument();
    const toggle = within(dialog).getByRole("button", { name: "1 earlier version, goes with it" });
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    await userEvent.click(toggle);
    const versions = within(dialog).getByRole("list", { name: "Earlier versions" });
    expect(within(versions).getByText(/Her scan came back fine/)).toBeInTheDocument();
    expect(within(versions).queryByRole("checkbox")).toBeNull();

    await userEvent.click(box(dialog, /Gran rang at six\. Her scan came back clear/));
    await settled(dialog);
    const preview = vi.spyOn(engine, "previewForget");
    await userEvent.click(box(dialog, /Gran rang at six\. Her scan came back clear/));
    await waitFor(() => expect(preview).toHaveBeenCalled());
    const also = preview.mock.calls.at(-1)?.[2]?.also ?? [];
    expect(also).toContain("j-block-2");
    expect(also).toContain("j-block-2-v1");
  });

  it("gives an original that cannot go no box, and says why", async () => {
    const engine = engineIn();
    const real = engine.previewForget.bind(engine);
    vi.spyOn(engine, "previewForget").mockImplementation(async (ids, include, items) => {
      const preview: ForgetPreview = await real(ids, include, items);
      return {
        ...preview,
        originals: [
          ...preview.originals.filter((o) => o.itemId !== "j-form"),
          {
            itemId: "j-form",
            label: "Year 4 trip form",
            kind: "document",
            keptAt: "2026-11-19T09:30:00Z",
            durationMs: null,
            removable: false,
            otherMemories: 2,
          },
        ],
      };
    });
    const { dialog } = await open(engine);
    await waitFor(() =>
      expect(
        within(dialog).getByText("Stays. Memories from other days still use it."),
      ).toBeInTheDocument(),
    );
    expect(within(dialog).queryByRole("checkbox", { name: /Year 4 trip form/ })).toBeNull();
    const kept = within(dialog).getByRole("region", { name: "Kept on this day" });
    const boxes = within(kept).getAllByRole("checkbox").length - 1;
    expect(within(kept).getByText(`${boxes} of ${boxes}`)).toBeInTheDocument();
  });

  it("says what else goes: the memories, the reflection and what is out of reach", async () => {
    const { dialog } = await open();
    expect(
      within(dialog).getByText(/^And the \d+ memories Nocturne read from them\.$/),
    ).toBeInTheDocument();
    expect(
      within(dialog).getByText(
        "Anything you exported or printed is out of reach. Nocturne can’t delete it.",
      ),
    ).toBeInTheDocument();
    expect(within(dialog).getByText(/^Your reflection on this day/)).toBeInTheDocument();
    expect(within(dialog).queryByRole("list", { name: "Memories that go with them" })).toBeNull();
    await userEvent.click(within(dialog).getByRole("button", { name: "Show them" }));
    const memories = within(dialog).getByRole("list", { name: "Memories that go with them" });
    expect(within(memories).getAllByRole("listitem").length).toBeGreaterThan(3);
    await userEvent.click(within(dialog).getByRole("button", { name: "Hide them" }));
    expect(within(dialog).queryByRole("list", { name: "Memories that go with them" })).toBeNull();
  });

  it("takes out only the lines that cite what is ticked when something is kept", async () => {
    const { dialog } = await open();
    await userEvent.click(box(dialog, /Leo forgot his PE kit again/));
    await waitFor(() =>
      expect(within(dialog).queryByText(/^Your reflection on this day/)).toBeNull(),
    );
    await waitFor(() =>
      expect(within(dialog).getByText(/cite these and will be taken out\.$/)).toBeInTheDocument(),
    );
  });

  it("deletes what is ticked, says what stays, and the day is empty after", async () => {
    const { engine, dialog } = await open();
    await userEvent.click(within(dialog).getByRole("button", { name: /^Delete \d+ things$/ }));
    const done = await screen.findByRole("dialog", { name: "Deleted." });
    expect(
      within(done).getByText(
        /things are gone from this computer, with the \d+ memories read from them\. They won’t appear in answers, pages or anywhere else\.$/,
      ),
    ).toBeInTheDocument();
    expect(within(done).getByText("1 thing from another day stays.")).toBeInTheDocument();
    const day = await engine.getJournalDay();
    expect(day.rows).toHaveLength(0);
    expect(day.entry).toHaveLength(0);
    await expect(engine.getItem("j-wed-gran")).resolves.toBeDefined();
    await userEvent.click(within(done).getByRole("button", { name: "OK" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(await screen.findByText(/Nothing kept today yet/)).toBeInTheDocument();
  });

  it("deletes a ticked row from another day too", async () => {
    const { engine, dialog } = await open();
    await userEvent.click(box(dialog, /Gran’s scan is tomorrow at ten/));
    await userEvent.click(within(dialog).getByRole("button", { name: /^Delete \d+ things$/ }));
    const done = await screen.findByRole("dialog", { name: "Deleted." });
    expect(within(done).queryByText(/stays\.$/)).toBeNull();
    await expect(engine.getItem("j-wed-gran")).rejects.toThrow();
  });

  it("starts on Keep the day, closes on Escape and changes nothing", async () => {
    const { engine, dialog } = await open();
    expect(within(dialog).getByRole("button", { name: "Keep the day" })).toHaveFocus();
    const forget = vi.spyOn(engine, "forget");
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(forget).not.toHaveBeenCalled();
    expect((await engine.getJournalDay()).rows.length).toBeGreaterThan(0);
  });

  it("closes with Keep the day", async () => {
    const { dialog } = await open();
    await userEvent.click(within(dialog).getByRole("button", { name: "Keep the day" }));
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("cannot be closed once deleting has started", async () => {
    const engine = engineIn();
    let release: () => void = () => undefined;
    const real = engine.forget.bind(engine);
    vi.spyOn(engine, "forget").mockImplementation(
      (token) =>
        new Promise((resolve) => {
          release = () => resolve(real(token));
        }),
    );
    const { dialog } = await open(engine);
    await userEvent.click(within(dialog).getByRole("button", { name: /^Delete \d+ things$/ }));
    await userEvent.keyboard("{Escape}");
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Deleting…" })).toBeDisabled();
    release();
    await screen.findByRole("dialog", { name: "Deleted." });
  });

  it("gives every item of the day a row", async () => {
    const { engine } = await open();
    const day = await engine.getJournalDay();
    const { dayRows } = await import("./DeleteDayRows");
    const ids = new Set(dayRows(day).flatMap((r) => r.ids));
    for (const c of day.checkins) expect(ids.has(c.id)).toBe(true);
    for (const g of day.good) expect(ids.has(g.id)).toBe(true);
    for (const b of day.entry) expect(ids.has(b.item.id)).toBe(true);
    for (const r of day.rows) expect(ids.has(r.item.id)).toBe(true);
  });
});
