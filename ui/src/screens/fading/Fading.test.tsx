import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createMockEngine } from "../../engine/mock/engine";
import { renderApp, setViewport } from "../../test/render";
import { patch } from "../settings/patch";

beforeEach(() => setViewport(1280));
afterEach(() => {
  window.location.hash = "";
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

const open = async (hash = "#/fading", engine = createMockEngine()) => {
  const view = renderApp({ hash, engine });
  await screen.findByRole("heading", { level: 1, name: "Fading & let go" });
  await screen.findByText("Ava’s old swimming teacher is called Jo.");
  return { ...view, engine };
};

const motion = (reduce: boolean) =>
  vi.stubGlobal("matchMedia", (query: string) => ({
    matches: reduce && query.includes("reduce"),
    media: query,
    addEventListener: () => undefined,
    removeEventListener: () => undefined,
  }));

describe("Fading & let go", () => {
  it("explains itself and lists what is fading and what was let go, in words", async () => {
    await open();
    expect(screen.getByText(/Nothing here is deleted/)).toBeInTheDocument();
    const fading = screen.getByRole("region", { name: "Fading · will be let go soon" });
    expect(
      within(fading).getByText("Fading · last used 7 Sep · let go in 3 weeks"),
    ).toBeInTheDocument();
    expect(
      within(fading).getByText("Fading · last used 21 Aug · let go in 6 days"),
    ).toBeInTheDocument();
    const letGo = screen.getByRole("region", { name: "Let go · 2" });
    expect(within(letGo).getByText("The old gym door code is 4821.")).toBeInTheDocument();
    expect(within(letGo).getByText("Let go 13 Nov · not used since August")).toBeInTheDocument();
  });

  it.each([
    ["gentle", 45, 120],
    ["normal", 30, 90],
    ["brisk", 14, 45],
  ] as const)("says the real days for the %s pace", async (pace, fade, letGo) => {
    motion(true);
    const engine = createMockEngine();
    await engine.updateSettings(patch({ lettingGo: pace }));
    await open("#/fading", engine);
    expect(
      await screen.findByText(
        `Things you haven’t used fade after ${fade} days and are let go after ${letGo}. Nothing here is deleted. Bring anything back with one click.`,
      ),
    ).toBeInTheDocument();
    await userEvent.click(
      screen.getByRole("button", { name: "Keep it: Ava’s old swimming teacher is called Jo." }),
    );
    expect(await screen.findByText(`Kept. It will not fade for ${fade} days.`)).toBeInTheDocument();
  });

  it("keeps a fading memory, announces it, and moves focus to the next row", async () => {
    motion(true);
    const { engine } = await open();
    const spy = vi.spyOn(engine, "restore");
    const snooze = vi.spyOn(engine, "snooze");
    await userEvent.click(
      screen.getByRole("button", { name: "Keep it: Ava’s old swimming teacher is called Jo." }),
    );
    // Hiding it from the page would not stop it fading; using it again does.
    expect(spy).toHaveBeenCalledWith(["mem-jo"]);
    expect(snooze).not.toHaveBeenCalled();
    await waitFor(() =>
      expect(screen.queryByText("Ava’s old swimming teacher is called Jo.")).toBeNull(),
    );
    expect(screen.getByText("Kept. It will not fade for 30 days.")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Keep it: Bin day moved to Tuesdays over summer." }),
    ).toHaveFocus();
  });

  it("moves focus on to the next section when the last row of one leaves", async () => {
    motion(true);
    await open();
    await userEvent.click(
      screen.getByRole("button", { name: "Keep it: Ava’s old swimming teacher is called Jo." }),
    );
    await userEvent.click(
      await screen.findByRole("button", {
        name: "Keep it: Bin day moved to Tuesdays over summer.",
      }),
    );
    await waitFor(() =>
      expect(
        screen.getByRole("checkbox", { name: "The old gym door code is 4821." }),
      ).toHaveFocus(),
    );
    expect(screen.getByText("Nothing is fading right now.")).toBeInTheDocument();
  });

  it("moves focus to the heading when nothing is left to land on", async () => {
    motion(true);
    const engine = createMockEngine();
    const all = await engine.listFading();
    vi.spyOn(engine, "listFading").mockResolvedValue(all.filter((m) => m.id === "mem-jo"));
    renderApp({ hash: "#/fading", engine });
    await userEvent.click(
      await screen.findByRole("button", {
        name: "Keep it: Ava’s old swimming teacher is called Jo.",
      }),
    );
    await waitFor(() =>
      expect(screen.getByRole("heading", { level: 1, name: "Fading & let go" })).toHaveFocus(),
    );
  });

  it("runs an action once however many times the button is pressed", async () => {
    motion(true);
    const { engine } = await open();
    let release: () => void = () => undefined;
    const spy = vi.spyOn(engine, "restore").mockImplementation(
      () =>
        new Promise<void>((resolve) => {
          release = resolve;
        }),
    );
    const button = screen.getByRole("button", {
      name: "Keep it: Ava’s old swimming teacher is called Jo.",
    });
    await userEvent.click(button);
    await userEvent.click(button);
    await userEvent.click(button);
    expect(spy).toHaveBeenCalledTimes(1);
    expect(button).toHaveAttribute("aria-disabled", "true");
    await act(async () => {
      release();
    });
    await waitFor(() =>
      expect(screen.queryByText("Ava’s old swimming teacher is called Jo.")).toBeNull(),
    );
  });

  it("brings one back with its own button", async () => {
    motion(true);
    const { engine } = await open();
    const spy = vi.spyOn(engine, "restore");
    await userEvent.click(
      screen.getByRole("button", { name: "Bring back: The pizza place on Elm Street is good." }),
    );
    expect(spy).toHaveBeenCalledWith(["mem-pizza"]);
    expect(
      await screen.findByText(/Brought back\. It will not fade for 30 days\./),
    ).toBeInTheDocument();
    await waitFor(() =>
      expect(screen.queryByText("The pizza place on Elm Street is good.")).toBeNull(),
    );
  });

  it("selects let-go rows, counts them, and brings them back together", async () => {
    motion(true);
    const { engine } = await open();
    const spy = vi.spyOn(engine, "restore");
    expect(screen.queryByRole("button", { name: /^Bring back \d/ })).toBeNull();
    await userEvent.click(screen.getByRole("checkbox", { name: "The old gym door code is 4821." }));
    await userEvent.click(
      screen.getByRole("checkbox", { name: "The pizza place on Elm Street is good." }),
    );
    expect(screen.getByRole("region", { name: "Let go · 2 selected" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Bring back 2" }));
    expect(spy).toHaveBeenCalledWith(["mem-gym", "mem-pizza"]);
    expect(await screen.findByText(/Brought back 2 things/)).toBeInTheDocument();
  });

  it("forgets the selected through the forget dialog and then clears them from the list", async () => {
    motion(true);
    await open();
    await userEvent.click(screen.getByRole("checkbox", { name: "The old gym door code is 4821." }));
    await userEvent.click(screen.getByRole("button", { name: "Forget 1 completely…" }));
    const dialog = await screen.findByRole("dialog", { name: "Forget 1 thing?" });
    await userEvent.click(await within(dialog).findByRole("button", { name: /^Forget 2 things/ }));
    expect(await within(dialog).findByText("Gone.")).toBeInTheDocument();
    await userEvent.click(within(dialog).getByRole("button", { name: "OK" }));
    await waitFor(() => expect(screen.queryByText("The old gym door code is 4821.")).toBeNull());
    expect(screen.getByText("The pizza place on Elm Street is good.")).toBeInTheDocument();
  });

  it("refreshes the lists when forgetting finishes even after the screen has been left", async () => {
    motion(true);
    const { engine } = await open();
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
    await userEvent.click(screen.getByRole("checkbox", { name: "The old gym door code is 4821." }));
    await userEvent.click(screen.getByRole("button", { name: "Forget 1 completely…" }));
    const dialog = await screen.findByRole("dialog", { name: "Forget 1 thing?" });
    await userEvent.click(await within(dialog).findByRole("button", { name: /^Forget 2 things/ }));
    act(() => {
      window.location.hash = "#/today";
    });
    await screen.findByRole("heading", { level: 1, name: /need/i });
    expect(screen.queryByRole("heading", { name: "Fading & let go" })).toBeNull();
    const status = vi.spyOn(engine, "getStatus");
    await act(async () => {
      finish({ memoriesRemoved: 1, originalsRemoved: 1, pagesRedacted: 0 });
    });
    await waitFor(() => expect(status).toHaveBeenCalled());
  });

  it("fades a row away before removing it, unless motion is reduced", async () => {
    motion(false);
    vi.useFakeTimers({ shouldAdvanceTime: true });
    await open();
    await act(async () => {
      screen
        .getByRole("button", { name: "Keep it: Ava’s old swimming teacher is called Jo." })
        .click();
    });
    const row = screen.getByText("Ava’s old swimming teacher is called Jo.").closest("li");
    await waitFor(() => expect(row).toHaveAttribute("data-leaving", "true"));
    await act(async () => {
      await vi.advanceTimersByTimeAsync(600);
    });
    expect(screen.queryByText("Ava’s old swimming teacher is called Jo.")).toBeNull();
  });

  it("goes to the let-go section when asked with ?show=letGo", async () => {
    await open("#/fading?show=letGo");
    await waitFor(() =>
      expect(screen.getByRole("heading", { level: 2, name: "Let go · 2" })).toHaveFocus(),
    );
  });

  it("has a calm empty state", async () => {
    const engine = createMockEngine();
    vi.spyOn(engine, "listFading").mockResolvedValue([]);
    renderApp({ hash: "#/fading", engine });
    expect(await screen.findByText(/Nothing is fading or let go/)).toBeInTheDocument();
  });

  it("says so and offers to try again when the list cannot be read", async () => {
    const engine = createMockEngine();
    const spy = vi.spyOn(engine, "listFading").mockRejectedValueOnce(new Error("no"));
    renderApp({ hash: "#/fading", engine });
    expect(await screen.findByRole("alert")).toHaveTextContent("Nocturne couldn’t open this list.");
    await userEvent.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByText("Ava’s old swimming teacher is called Jo.")).toBeInTheDocument();
    expect(spy).toHaveBeenCalledTimes(2);
  });

  it("tells the person when an action fails and keeps the row", async () => {
    motion(true);
    const { engine } = await open();
    vi.spyOn(engine, "restore").mockRejectedValue(new Error("no"));
    await userEvent.click(
      screen.getByRole("button", { name: "Keep it: Ava’s old swimming teacher is called Jo." }),
    );
    expect(await screen.findByText("That didn’t work. Nothing was changed.")).toBeInTheDocument();
    expect(screen.getByText("Ava’s old swimming teacher is called Jo.")).toBeInTheDocument();
  });
});
