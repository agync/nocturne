import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { NeedsYouEntry } from "../../engine/bindings";
import { MockEngine } from "../../engine/mock/engine";
import { renderApp, setViewport } from "../../test/render";

// The shared forget dialog is a stub until the memories screens land; this stands in for its
// promise: it reports what it was given and calls onDone once the person confirms.
vi.mock("../../components/ForgetDialog/ForgetDialog", () => ({
  ForgetDialog: ({ memoryIds, onDone }: { memoryIds: string[]; onDone: (r: unknown) => void }) => (
    <button
      type="button"
      onClick={() =>
        onDone({ memoriesRemoved: memoryIds.length, originalsRemoved: 0, pagesRedacted: 0 })
      }
    >
      {`Confirm forgetting ${memoryIds.join(",")}`}
    </button>
  ),
}));

class Waiting extends MockEngine {
  override async listNeedsYou(): Promise<NeedsYouEntry[]> {
    const memories = await this.listMemories({
      entityId: null,
      itemId: null,
      status: null,
      query: null,
      limit: null,
    });
    const memory = memories.find((m) => m.id === "mem-nut");
    if (!memory) throw new Error("seed");
    return [{ type: "unconfirmed", memory, agreeingSources: 0, used: false }];
  }
}

beforeEach(() => setViewport(1280));
afterEach(() => {
  window.location.hash = "";
});

describe("Not true, forget it", () => {
  it("once forgotten the entry goes and the screen says what happened", async () => {
    renderApp({ hash: "#/needs-you", engine: new Waiting() });
    await userEvent.click(await screen.findByRole("button", { name: "Not true, forget it" }));
    await userEvent.click(screen.getByRole("button", { name: "Confirm forgetting mem-nut" }));
    expect(await screen.findByText("Nothing needs you.")).toBeInTheDocument();
    await waitFor(() =>
      expect(
        screen.getByText(
          "Forgotten. The original is still kept; nothing from it is treated as true.",
        ),
      ).toBeInTheDocument(),
    );
  });
});
