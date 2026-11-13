import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { NeedsYouEntry } from "../../engine/bindings";
import { MockEngine } from "../../engine/mock/engine";
import { renderApp, setViewport } from "../../test/render";

/** The nut allergy waits, with one other thing behind it; forgetting really removes the memory. */
class Waiting extends MockEngine {
  override async listNeedsYou(): Promise<NeedsYouEntry[]> {
    const memories = await this.listMemories({
      entityId: null,
      itemId: null,
      status: null,
      query: null,
      limit: null,
    });
    const nut = memories.find((m) => m.id === "mem-nut");
    const trip = memories.find((m) => m.id === "mem-trip");
    if (!trip) throw new Error("seed");
    const out: NeedsYouEntry[] = [];
    if (nut?.status === "unconfirmed") {
      out.push({ type: "unconfirmed", memory: nut, agreeingSources: 0, used: false });
    }
    out.push({ type: "due", memory: trip, due: "2026-11-20", done: false });
    return out;
  }
}

beforeEach(() => setViewport(1280));
afterEach(() => {
  window.location.hash = "";
});

describe("Not true, delete it, with the real forget dialog", () => {
  it("settles the entry once, as soon as it is forgotten, and keeps the dialog's own words until closed", async () => {
    const engine = new Waiting();
    const forget = vi.spyOn(engine, "forget");
    const listed = vi.spyOn(engine, "listNeedsYou");
    renderApp({ hash: "#/needs-you", engine });
    await userEvent.click(await screen.findByRole("button", { name: "Not true, delete it…" }));
    const dialog = await screen.findByRole("dialog", { name: "Forget 1 memory?" });
    await userEvent.click(await within(dialog).findByRole("button", { name: "Forget 1 memory" }));

    // The dialog says so while the screen behind it has already moved on.
    expect(await within(dialog).findByText("Gone.")).toBeInTheDocument();
    expect(forget).toHaveBeenCalledOnce();
    await waitFor(() => expect(listed.mock.calls.length).toBeGreaterThan(1));
    await waitFor(() => expect(screen.getByText("Forgotten.")).toBeInTheDocument());
    expect(document.body.textContent).not.toMatch(/original is still kept/);
    expect(screen.getByRole("dialog")).toBeInTheDocument();

    await userEvent.click(within(dialog).getByRole("button", { name: "OK" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(forget).toHaveBeenCalledOnce();
    // The entry after it takes the focus.
    await waitFor(() =>
      expect(document.activeElement).toHaveAttribute("data-entry", "due:mem-trip"),
    );
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent("One thing is waiting");
  });

  it("says only Forgotten, because whether the original went depends on what was ticked", async () => {
    const engine = new Waiting();
    renderApp({ hash: "#/needs-you", engine });
    await userEvent.click(await screen.findByRole("button", { name: "Not true, delete it…" }));
    const dialog = await screen.findByRole("dialog", { name: /Forget/ });
    const box = await within(dialog).findByRole("checkbox", { name: /Also delete the original/ });
    if (!(box as HTMLInputElement).checked) await userEvent.click(box);
    await userEvent.click(await within(dialog).findByRole("button", { name: /^Forget \d+ thing/ }));
    await within(dialog).findByText("Gone.");
    const said = await screen.findByText("Forgotten.");
    expect(said).toHaveAttribute("aria-live", "polite");
    expect(document.body.textContent).not.toMatch(/original is still kept/);
  });

  it("Keep them leaves everything as it was", async () => {
    const engine = new Waiting();
    const forget = vi.spyOn(engine, "forget");
    renderApp({ hash: "#/needs-you", engine });
    await userEvent.click(await screen.findByRole("button", { name: "Not true, delete it…" }));
    await userEvent.click(await screen.findByRole("button", { name: "Keep it" }));
    expect(forget).not.toHaveBeenCalled();
    expect(await screen.findByRole("heading", { level: 1 })).toHaveTextContent(
      "2 things are waiting",
    );
  });
});
