import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { askRequests } from "../../app/ask-request";
import type { AskDelta } from "../../engine/bindings";
import { MockEngine } from "../../engine/mock/engine";
import { renderApp, setViewport } from "../../test/render";
import { askSession } from "./session";

// The shared forget dialog is a stub until the memories screens land; this one confirms at once.
vi.mock("../../components/ForgetDialog/ForgetDialog", () => ({
  ForgetDialog: ({ onDone }: { onDone: (r: unknown) => void }) => (
    <button
      type="button"
      onClick={() => onDone({ memoriesRemoved: 1, originalsRemoved: 0, pagesRedacted: 0 })}
    >
      Confirm forgetting
    </button>
  ),
}));

/** Every question gets the same two-memory answer. */
class TwoMemories extends MockEngine {
  count = 0;

  override async ask(): Promise<string> {
    const swim = (await this.listMemories(filter)).find((m) => m.id === "mem-swim");
    const coach = (await this.listMemories(filter)).find((m) => m.id === "mem-coach");
    if (!swim || !coach) throw new Error("seed");
    this.count += 1;
    const askId = `two-${this.count}`;
    const send = (step: Partial<AskDelta> & Pick<AskDelta, "phase">, ms: number) =>
      setTimeout(() => {
        (this as unknown as { emit(n: string, d: AskDelta): void }).emit("ask-delta", {
          askId,
          textDelta: null,
          receipts: [],
          memoryIds: [],
          near: [],
          found: null,
          etaSeconds: null,
          ...step,
        });
      }, ms);
    send(
      {
        phase: "reading",
        found: 2,
        receipts: [...swim.receipts, ...coach.receipts],
        memoryIds: [swim.id, coach.id],
      },
      5,
    );
    send({ phase: "streaming", textDelta: "Leo swims **on Mondays** with **Coach Sam**." }, 10);
    send({ phase: "done", memoryIds: [swim.id, coach.id] }, 15);
    return askId;
  }
}

const filter = { entityId: null, itemId: null, status: null, query: null, limit: null };

beforeEach(() => {
  askSession.reset();
  setViewport(1280);
});
afterEach(() => {
  askSession.reset();
  window.location.hash = "";
});

const announcer = () =>
  screen
    .getAllByRole("status")
    .find((s) => s.className.includes("sr-only") && s.getAttribute("aria-atomic") === "true");

const openWithEarlier = async () => {
  askRequests.request("When is Leo’s swimming?");
  renderApp({ hash: "#/ask", engine: new TwoMemories() });
  await screen.findByRole("button", { name: "That’s wrong" });
  await waitFor(() => expect(announcer()).toHaveTextContent("Answer ready."));
  await act(async () => askRequests.request("And who is the coach?"));
  await screen.findByRole("heading", { level: 1, name: "“And who is the coach?”" });
  await screen.findByRole("button", { name: "Forget this…" });
  await waitFor(() => expect(announcer()).toHaveTextContent("Answer ready."));
};

describe("forgetting what an answer used", () => {
  it("forgetting all of it clears what the screen reader holds, scrubs Earlier and says so", async () => {
    await openWithEarlier();
    const earlier = screen.getByRole("region", { name: "Earlier" });
    expect(earlier).toHaveTextContent(/Leo swims on Mondays/);
    await userEvent.click(screen.getByRole("button", { name: "Forget this…" }));
    const chooser = await screen.findByRole("group", { name: "Which one?" });
    await userEvent.click(within(chooser).getByRole("button", { name: "All of them" }));
    await userEvent.click(await screen.findByRole("button", { name: "Confirm forgetting" }));
    await waitFor(() => expect(announcer()).toHaveTextContent(/^$/));
    expect(document.body).not.toHaveTextContent(/Leo swims on Mondays/);
    expect(within(earlier).getByText("Forgotten. It is no longer kept.")).toBeInTheDocument();
    expect(within(earlier).queryByRole("button", { name: /^Source:/ })).toBeNull();
    const result = await screen.findByText("Forgotten. It is no longer kept.", {
      selector: "p[role=status]",
    });
    expect(result).toHaveFocus();
    expect(screen.queryByRole("button", { name: "That’s wrong" })).toBeNull();
  });

  it("forgetting one of several says which, hides the answer and keeps nothing that used it", async () => {
    await openWithEarlier();
    await userEvent.click(screen.getByRole("button", { name: "Forget this…" }));
    const chooser = await screen.findByRole("group", { name: "Which one?" });
    await userEvent.click(within(chooser).getAllByRole("button")[0] as HTMLElement);
    await userEvent.click(await screen.findByRole("button", { name: "Confirm forgetting" }));
    const said = await screen.findByText(/^Forgotten: Leo’s swimming is on Mondays/, {
      selector: "p[role=status]",
    });
    expect(said).toHaveTextContent("The answer that used it is hidden. Ask again for a fresh one.");
    expect(document.body).not.toHaveTextContent(/Leo swims on Mondays with/);
    const earlier = screen.getByRole("region", { name: "Earlier" });
    expect(within(earlier).getByText("Forgotten. It is no longer kept.")).toBeInTheDocument();
  });
});
