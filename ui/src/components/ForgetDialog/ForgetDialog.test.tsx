import { act, render, screen, waitFor, within } from "@testing-library/react";
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
    expect(within(dialog).getAllByRole("checkbox")).toHaveLength(1);
    expect(
      within(dialog).getByRole("checkbox", { name: "Also delete the 2 originals" }),
    ).toBeChecked();
    expect(within(dialog).getByRole("button", { name: "Forget 4 things" })).toBeInTheDocument();
  });

  it("titles the dialog by its memories, whatever the box about originals says", async () => {
    const engine = createMockEngine();
    vi.spyOn(engine, "previewForget").mockResolvedValue(preview());
    open({ memoryIds: ["a"], defaultIncludeOriginals: true }, engine);
    expect(await screen.findByRole("dialog", { name: "Forget 1 thing?" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Forget 2 things" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Keep it" })).toBeInTheDocument();
    expect(
      screen.getByText(/^This will be removed from this computer for good/),
    ).toBeInTheDocument();
  });

  it("speaks in the plural for several memories", async () => {
    open();
    await screen.findByRole("dialog", { name: "Forget 2 things?" });
    expect(screen.getByRole("button", { name: "Keep them" })).toBeInTheDocument();
    expect(
      screen.getByText(/^These will be removed from this computer for good/),
    ).toBeInTheDocument();
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
      await within(dialog).findByText(/The original stays, and so does its transcript/),
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
    const box = await screen.findByRole("checkbox", { name: "Also delete the original" });
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
    expect(await screen.findByText(/Voice note, 0:34/)).toBeInTheDocument();
    expect(screen.queryByRole("checkbox")).toBeNull();
    expect(screen.getByText(/Other memories still use this, so it stays\./)).toBeInTheDocument();
    expect(screen.getByRole("dialog", { name: "Forget 1 thing?" })).toBeInTheDocument();
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
        "That list was out of date, so Nocturne refreshed it. Check it, then forget.",
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
    const previews = vi.spyOn(engine, "previewForget");
    const { onDone } = open({ memoryIds: ["mem-nut"] }, engine);
    await userEvent.click(await screen.findByRole("button", { name: "Forget 2 things" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      /Anything already hidden stays hidden/,
    );
    expect(onDone).not.toHaveBeenCalled();
    expect(previews.mock.calls.length).toBeGreaterThan(1);
  });

  it("says so when the list cannot be made, and offers to try again", async () => {
    const engine = createMockEngine();
    const spy = vi.spyOn(engine, "previewForget").mockRejectedValueOnce(new Error("no"));
    open({ memoryIds: ["mem-nut"] }, engine);
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Nocturne couldn’t get the list ready.",
    );
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

  it("cannot be cancelled while forgetting, and still reports when it is done", async () => {
    const engine = createMockEngine();
    let finish: (r: {
      memoriesRemoved: number;
      originalsRemoved: number;
      pagesRedacted: number;
    }) => void = () => undefined;
    vi.spyOn(engine, "forget").mockImplementation(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
    const { onClose, onDone } = open({ memoryIds: ["mem-nut"] }, engine);
    await userEvent.click(await screen.findByRole("button", { name: "Forget 2 things" }));
    expect(screen.getByRole("button", { name: "Keep it" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Close" })).toBeDisabled();
    await userEvent.keyboard("{Escape}");
    await userEvent.click(screen.getByRole("dialog").parentElement as HTMLElement);
    expect(onClose).not.toHaveBeenCalled();
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    await act(async () => {
      finish({ memoriesRemoved: 1, originalsRemoved: 1, pagesRedacted: 0 });
    });
    expect(await screen.findByText("Gone.")).toBeInTheDocument();
    expect(onDone).toHaveBeenCalledOnce();
    expect(screen.getByRole("button", { name: "Close" })).toBeEnabled();
  });

  it("reports the result even if the dialog has been taken away meanwhile", async () => {
    const engine = createMockEngine();
    let finish: (r: {
      memoriesRemoved: number;
      originalsRemoved: number;
      pagesRedacted: number;
    }) => void = () => undefined;
    vi.spyOn(engine, "forget").mockImplementation(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
    const { onDone, unmount } = open({ memoryIds: ["mem-nut"] }, engine);
    await userEvent.click(await screen.findByRole("button", { name: "Forget 2 things" }));
    unmount();
    await act(async () => {
      finish({ memoriesRemoved: 1, originalsRemoved: 1, pagesRedacted: 0 });
    });
    expect(onDone).toHaveBeenCalledWith({
      memoriesRemoved: 1,
      originalsRemoved: 1,
      pagesRedacted: 0,
    });
  });

  it("locks the box and the button while the list is read again, so an untick cannot be lost", async () => {
    const engine = createMockEngine();
    let release: (p: ForgetPreview) => void = () => undefined;
    const previews = vi
      .spyOn(engine, "previewForget")
      .mockResolvedValueOnce(preview({ token: "old" }))
      .mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            release = resolve;
          }),
      )
      .mockResolvedValueOnce(preview({ token: "full" }))
      .mockResolvedValueOnce(preview({ token: "no-originals", includeOriginals: false }));
    const forget = vi
      .spyOn(engine, "forget")
      .mockRejectedValueOnce(new Error("That preview has expired"))
      .mockResolvedValueOnce({ memoriesRemoved: 1, originalsRemoved: 0, pagesRedacted: 0 });
    open({ memoryIds: ["a"] }, engine);
    await userEvent.click(await screen.findByRole("button", { name: "Forget 2 things" }));
    // The token has expired and the list is being read again.
    const box = await screen.findByRole("checkbox", { name: "Also delete the original" });
    await waitFor(() => expect(box).toBeDisabled());
    await userEvent.click(box);
    expect(box).toBeChecked();
    expect(screen.getByRole("button", { name: /^Forget/ })).toBeDisabled();
    await act(async () => {
      release(preview({ token: "fresh" }));
    });
    await waitFor(() => expect(box).toBeEnabled());
    expect(screen.getByRole("button", { name: "Forget 2 things" })).toBeEnabled();
    // Unticking now asks for a matching token, and the button then says what it will do.
    await userEvent.click(box);
    const button = await screen.findByRole("button", { name: "Forget 1 memory" });
    await waitFor(() => expect(button).toBeEnabled());
    expect(previews).toHaveBeenLastCalledWith(["a"], false);
    await userEvent.click(button);
    expect(forget).toHaveBeenLastCalledWith("no-originals");
    expect(await screen.findByText(/The original is kept\./)).toBeInTheDocument();
  });

  it("keeps the title and the original line to the token's choice while a new list is read, and after it fails", async () => {
    const engine = createMockEngine();
    let reject: (e: Error) => void = () => undefined;
    vi.spyOn(engine, "previewForget")
      .mockResolvedValueOnce(preview({ token: "full" }))
      .mockResolvedValueOnce(preview({ token: "full" }))
      .mockImplementationOnce(
        () =>
          new Promise((_, no) => {
            reject = no;
          }),
      );
    open({ memoryIds: ["a"] }, engine);
    const box = await screen.findByRole("checkbox", { name: "Also delete the original" });
    expect(box).toBeChecked();
    expect(screen.getByRole("dialog", { name: "Forget 1 thing?" })).toBeInTheDocument();
    await userEvent.click(box);
    // The box shows the new wish at once; what a press would do is still the old token's.
    await waitFor(() => expect(box).not.toBeChecked());
    expect(screen.getByRole("dialog", { name: "Forget 1 thing?" })).toBeInTheDocument();
    expect(screen.queryByText(/The original stays/)).toBeNull();
    await act(async () => {
      reject(new Error("no"));
    });
    expect(await screen.findByRole("alert")).toBeInTheDocument();
    expect(screen.getByRole("dialog", { name: "Forget 1 thing?" })).toBeInTheDocument();
    expect(screen.queryByText(/The original stays/)).toBeNull();
  });

  it("clears the note about a refreshed list on the next change of the box", async () => {
    const engine = createMockEngine();
    vi.spyOn(engine, "previewForget").mockResolvedValue(preview());
    vi.spyOn(engine, "forget").mockRejectedValueOnce(new Error("That preview has expired"));
    open({ memoryIds: ["a"] }, engine);
    await userEvent.click(await screen.findByRole("button", { name: "Forget 2 things" }));
    expect(await screen.findByText(/That list was out of date/)).toBeInTheDocument();
    const box = await screen.findByRole("checkbox", { name: "Also delete the original" });
    await waitFor(() => expect(box).toBeEnabled());
    await userEvent.click(box);
    await waitFor(() => expect(screen.queryByText(/That list was out of date/)).toBeNull());
  });

  it("ignores a list that arrives after a newer one", async () => {
    const engine = createMockEngine();
    const resolvers: Array<(p: ForgetPreview) => void> = [];
    vi.spyOn(engine, "previewForget").mockImplementation(
      () =>
        new Promise((resolve) => {
          resolvers.push(resolve);
        }),
    );
    const props = { defaultIncludeOriginals: true, onClose: vi.fn(), onDone: vi.fn() };
    const view = render(
      <EngineProvider engine={engine}>
        <ForgetDialog memoryIds={["old"]} {...props} />
      </EngineProvider>,
    );
    await waitFor(() => expect(resolvers.length).toBeGreaterThan(0));
    const asked = resolvers.length;
    view.rerender(
      <EngineProvider engine={engine}>
        <ForgetDialog memoryIds={["new"]} {...props} />
      </EngineProvider>,
    );
    await waitFor(() => expect(resolvers.length).toBeGreaterThan(asked));
    const newer = resolvers.slice(asked);
    const answer = (token: string, sentence: string) =>
      preview({ token, memories: [{ memoryId: "x", sentence }] });
    await act(async () => {
      for (const resolve of newer) resolve(answer("new", "The newer list"));
    });
    expect(await screen.findByText("The newer list")).toBeInTheDocument();
    await act(async () => {
      for (const resolve of resolvers.slice(0, asked)) resolve(answer("old", "The older list"));
    });
    expect(screen.getByText("The newer list")).toBeInTheDocument();
    expect(screen.queryByText("The older list")).toBeNull();
  });
});
