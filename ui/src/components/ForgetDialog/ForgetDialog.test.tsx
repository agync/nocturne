import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { ForgetPreview } from "../../engine/bindings";
import { EngineProvider } from "../../engine/context";
import { createMockEngine, type MockEngine } from "../../engine/mock/engine";
import { ForgetDialog } from "./ForgetDialog";

function open(
  props: Partial<React.ComponentProps<typeof ForgetDialog>> = {},
  engine: MockEngine = createMockEngine(),
) {
  const onClose = vi.fn();
  const onDone = vi.fn();
  const view = render(
    <EngineProvider engine={engine}>
      <div id="root">
        <button type="button">Opener</button>
      </div>
      <ForgetDialog
        memoryIds={["mem-nut", "mem-swim"]}
        defaultIncludeOriginals
        onClose={onClose}
        onDone={onDone}
        {...props}
      />
    </EngineProvider>,
  );
  return { ...view, engine, onClose, onDone };
}

const preview = (over: Partial<ForgetPreview> = {}): ForgetPreview => ({
  token: "t-1",
  memories: [{ memoryId: "a", sentence: "Mr Hughes at no. 14 borrowed the ladder" }],
  originals: [
    {
      itemId: "i-1",
      label: "Dentist moved",
      kind: "audio",
      keptAt: "2026-11-01T10:00:00Z",
      durationMs: 34000,
      removable: true,
      otherMemories: 0,
    },
  ],
  includeOriginals: true,
  pagesAffected: 1,
  ...over,
});

describe("ForgetDialog", () => {
  it("names every memory and the original, and states the count on the button", async () => {
    open();
    const dialog = await screen.findByRole("dialog", { name: "Forget 2 things?" });
    const list = await within(dialog).findByRole("list", { name: "What will be forgotten" });
    expect(within(list).getByText("Leo is allergic to nuts.")).toBeInTheDocument();
    expect(within(list).getByText("Leo’s swimming is on Mondays at 17:30.")).toBeInTheDocument();
    expect(within(list).getAllByRole("checkbox")).toHaveLength(2);
    expect(within(dialog).getByRole("button", { name: "Forget 4 things" })).toBeInTheDocument();
  });

  it("starts with focus on Keep them, and Enter there does not forget", async () => {
    const { engine, onClose } = open();
    const spy = vi.spyOn(engine, "forget");
    const keep = await screen.findByRole("button", { name: "Keep them" });
    expect(keep).toHaveFocus();
    await userEvent.keyboard("{Enter}");
    expect(onClose).toHaveBeenCalledOnce();
    expect(spy).not.toHaveBeenCalled();
  });

  it("states what stays: transcript with the original, exports and printed pages", async () => {
    open({ defaultIncludeOriginals: false });
    const dialog = await screen.findByRole("dialog");
    expect(
      await within(dialog).findByText(/If the original stays, so does its transcript/),
    ).toBeInTheDocument();
    expect(within(dialog).getByText(/exported or printed is out of reach/)).toBeInTheDocument();
  });

  it("forgets with the preview token, says Gone, and reports it", async () => {
    const { engine, onDone } = open({ memoryIds: ["mem-nut"] });
    const spy = vi.spyOn(engine, "forget");
    await userEvent.click(await screen.findByRole("button", { name: "Forget 2 things" }));
    expect(spy).toHaveBeenCalledOnce();
    expect(await screen.findByText("Gone.")).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent(
      /1 memory is deleted\. The original is deleted too\./,
    );
    expect(onDone).toHaveBeenCalledWith({
      memoriesRemoved: 1,
      originalsRemoved: 1,
      pagesRedacted: 0,
    });
    expect(
      await engine.listMemories({
        entityId: null,
        itemId: null,
        status: null,
        query: "nuts",
        limit: null,
      }),
    ).toEqual([]);
  });

  it("keeps the original when its box is unticked and asks for a matching preview", async () => {
    const { engine } = open({ memoryIds: ["mem-nut"] });
    const spy = vi.spyOn(engine, "previewForget");
    const box = await screen.findByRole("checkbox", { name: /Nut-free lunchbox ideas/ });
    expect(box).toBeChecked();
    await userEvent.click(box);
    expect(box).not.toBeChecked();
    await waitFor(() => expect(spy).toHaveBeenLastCalledWith(["mem-nut"], false));
    const button = await screen.findByRole("button", { name: "Forget 1 memory" });
    await userEvent.click(button);
    expect(await screen.findByText(/The original is kept\./)).toBeInTheDocument();
  });

  it("leaves an original unticked and disabled when other memories still use it", async () => {
    const engine = createMockEngine();
    vi.spyOn(engine, "previewForget").mockResolvedValue(
      preview({
        originals: [
          {
            itemId: "i-1",
            label: "Dentist moved",
            kind: "audio",
            keptAt: "2026-11-01T10:00:00Z",
            durationMs: 34000,
            removable: false,
            otherMemories: 2,
          },
        ],
      }),
    );
    open({ memoryIds: ["a"] }, engine);
    const box = await screen.findByRole("checkbox", { name: /Voice note, 0:34/ });
    expect(box).not.toBeChecked();
    expect(box).toBeDisabled();
    expect(screen.getByText("Other memories from that note are kept.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Forget 1 memory" })).toBeInTheDocument();
  });

  it("refreshes the preview when its token has expired, and does not forget", async () => {
    const engine = createMockEngine();
    const previews = vi
      .spyOn(engine, "previewForget")
      .mockResolvedValueOnce(preview({ token: "old" }))
      .mockResolvedValueOnce(preview({ token: "fresh" }));
    const forget = vi
      .spyOn(engine, "forget")
      .mockRejectedValueOnce(new Error("That preview has expired"))
      .mockResolvedValueOnce({ memoriesRemoved: 1, originalsRemoved: 1, pagesRedacted: 1 });
    open({ memoryIds: ["a"] }, engine);
    await userEvent.click(await screen.findByRole("button", { name: "Forget 2 things" }));
    expect(
      await screen.findByText(
        "That list was out of date, so I refreshed it. Check it, then forget.",
      ),
    ).toBeInTheDocument();
    expect(previews).toHaveBeenCalledTimes(2);
    expect(screen.queryByText("Gone.")).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Forget 2 things" }));
    expect(forget).toHaveBeenLastCalledWith("fresh");
    expect(await screen.findByText("Gone.")).toBeInTheDocument();
  });

  it("says nothing was forgotten when forgetting fails", async () => {
    const engine = createMockEngine();
    vi.spyOn(engine, "forget").mockRejectedValue(new Error("disk"));
    const { onDone } = open({ memoryIds: ["mem-nut"] }, engine);
    await userEvent.click(await screen.findByRole("button", { name: "Forget 2 things" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(/Nothing has been forgotten/);
    expect(onDone).not.toHaveBeenCalled();
  });

  it("says so when the list cannot be made, and offers to try again", async () => {
    const engine = createMockEngine();
    const spy = vi.spyOn(engine, "previewForget").mockRejectedValueOnce(new Error("no"));
    open({ memoryIds: ["mem-nut"] }, engine);
    expect(await screen.findByRole("alert")).toHaveTextContent("I couldn’t get the list ready.");
    await userEvent.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByRole("button", { name: "Forget 2 things" })).toBeInTheDocument();
    expect(spy).toHaveBeenCalledTimes(2);
  });

  it("closes on Escape and returns focus to what opened it", async () => {
    const onClose = vi.fn();
    const opener = document.createElement("button");
    document.body.appendChild(opener);
    opener.focus();
    open({ onClose });
    await screen.findByRole("button", { name: "Keep them" });
    await userEvent.keyboard("{Escape}");
    expect(onClose).toHaveBeenCalled();
    opener.remove();
  });

  it("adds the memories of an item when it is given item ids", async () => {
    open({ memoryIds: [], itemIds: ["item-swim"] });
    expect(await screen.findByText("Leo’s swimming is on Mondays at 17:30.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Forget 2 things" })).toBeInTheDocument();
  });

  it("says there is nothing to forget for an empty list", async () => {
    open({ memoryIds: [], itemIds: [] });
    expect(await screen.findByText("There is nothing left to forget here.")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^Forget \d/ })).toBeNull();
  });
});
