import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { MemoryCounts } from "../../../engine/bindings";
import {
  type MoveApi,
  type Moved,
  type MoveError,
  MoveFlow,
  type MoveOffered,
  nativeMoveApi,
} from "./MoveFlow";
import { MEASURE_MS, MoveMoving, type MoveProgress } from "./MoveScreens";

afterEach(() => {
  cleanup();
  clearMocks();
});

const COUNTS: MemoryCounts = {
  items: 1284,
  memories: 0,
  pages: 0,
  originals: 40,
};

type Answer = MemoryCounts | null | MoveError | "opened" | "bring" | "empty" | "useHere";

interface Fake extends MoveApi {
  progress?: (p: MoveProgress) => void;
  finishMove(result: Moved | MoveError): void;
  answerOffer(result: Answer): void;
}

/** An empty computer by default; with `holds`, one that already holds a memory. */
function fakeApi(holds = false): Fake {
  let settleMove: ((r: Moved | MoveError) => void) | null = null;
  let settleOffer: ((r: Answer) => void) | null = null;
  // Answers given before the offer went, for the offer when it goes.
  const queued: Answer[] = [];
  const api: Fake = {
    computer: vi.fn(async () => "Maya’s laptop"),
    holds: vi.fn(async () => holds),
    facts: vi.fn(async () => ({
      counts: COUNTS,
      things: 1284,
      journal: 12,
      since: "2026-03-03",
      bytes: 1_400_000_000,
      media: 1_300_000_000,
    })),
    offer: vi.fn(
      () =>
        new Promise<MoveOffered>((ok, no) => {
          const settle = (r: Answer) =>
            r === "opened"
              ? ok({ opened: true, here: null, bring: true })
              : r === "bring"
                ? ok({ opened: false, here: COUNTS, bring: true })
                : r === "empty"
                  ? ok({ opened: false, here: null, bring: true })
                  : r === "useHere"
                    ? ok({ opened: false, here: null, useHere: true })
                    : typeof r === "string"
                      ? no(r)
                      : ok({ opened: false, here: r });
          if (queued.length > 0) settle(queued.shift() as Answer);
          else settleOffer = settle;
        }),
    ),
    move: vi.fn(
      (onProgress: (p: MoveProgress) => void) =>
        new Promise<Moved>((ok, no) => {
          api.progress = onProgress;
          settleMove = (r) => (typeof r === "string" ? no(r) : ok(r));
        }),
    ),
    stop: vi.fn(async () => undefined),
    undo: vi.fn(async () => undefined),
    putAside: vi.fn(async () => undefined),
    pending: vi.fn(async () => false),
    finishMove: (r) => act(() => settleMove?.(r)),
    answerOffer: (r) =>
      act(() => {
        const settle = settleOffer;
        settleOffer = null;
        if (settle) settle(r);
        else queued.push(r);
      }),
  };
  return api;
}

function open(api: Fake) {
  const onExit = vi.fn();
  const onDone = vi.fn();
  render(<MoveFlow api={api} onExit={onExit} onDone={onDone} />);
  return { onExit, onDone };
}

/** To an empty computer: it agrees to bring the things in as soon as it is offered them. */
async function toMoving(api: Fake) {
  fireEvent.click(await screen.findByRole("button", { name: "Continue" }));
  fireEvent.click(await screen.findByRole("button", { name: "Move my memory" }));
  api.answerOffer("empty");
  await waitFor(() => expect(api.move).toHaveBeenCalled());
}

describe("moving the memory to a computer", () => {
  it("asks first, and offers nothing to an empty computer until Move my memory", async () => {
    const api = fakeApi();
    open(api);
    expect(
      await screen.findByRole("heading", {
        name: "Move your memory to Maya’s laptop?",
      }),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        "Maya’s laptop becomes the home of your memory. It reads, tidies and writes your page from now on.",
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByText("Your memory lives in one place at a time, so it moves rather than copies."),
    ).toBeInTheDocument();
    await waitFor(() => expect(api.holds).toHaveBeenCalled());
    expect(api.offer).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    fireEvent.click(await screen.findByRole("button", { name: "Move my memory" }));
    await waitFor(() => expect(api.offer).toHaveBeenCalledTimes(1));
  });

  it("waits for Move my memory even when an empty computer agreed early", async () => {
    const api = fakeApi();
    api.holds = vi.fn(async () => Promise.reject(new Error("unreached")));
    open(api);
    await waitFor(() => expect(api.offer).toHaveBeenCalledTimes(1));
    api.answerOffer("empty");
    expect(
      await screen.findByRole("heading", {
        name: "Move your memory to Maya’s laptop?",
      }),
    ).toBeInTheDocument();
    expect(api.move).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    fireEvent.click(await screen.findByRole("button", { name: "Move my memory" }));
    await waitFor(() => expect(api.move).toHaveBeenCalled());
    expect(api.offer).toHaveBeenCalledTimes(1);
    expect(
      screen.getByRole("heading", {
        name: "Bringing your things to Maya’s laptop",
      }),
    ).toBeInTheDocument();
  });

  it("Not now undoes the pairing and leaves the phone as it was", async () => {
    const api = fakeApi();
    const { onExit } = open(api);
    fireEvent.click(await screen.findByRole("button", { name: "Not now" }));
    await waitFor(() => expect(onExit).toHaveBeenCalled());
    expect(api.undo).toHaveBeenCalled();
  });

  it("says what moves, how long it takes and what changes here", async () => {
    const api = fakeApi();
    open(api);
    fireEvent.click(await screen.findByRole("button", { name: "Continue" }));
    expect(await screen.findByRole("heading", { name: "What moves" })).toBeInTheDocument();
    expect(screen.getByText("1,284 things you kept since 3 March")).toBeInTheDocument();
    expect(
      screen.getByText(/^About 1\.4\sGB, most of it photos and recordings$/),
    ).toBeInTheDocument();
    expect(screen.getByText("12 journal entries")).toBeInTheDocument();
    expect(
      screen.getByText("Keep Nocturne open; it carries on if the phone allows."),
    ).toBeInTheDocument();
    expect(screen.queryByText(/minutes/)).toBeNull();
    expect(
      screen.getByText(
        "Your memory here is put aside once it has arrived. You choose then whether to remove it.",
      ),
    ).toBeInTheDocument();
    expect(screen.queryByText(/pages/)).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Back" }));
    expect(
      await screen.findByRole("heading", {
        name: "Move your memory to Maya’s laptop?",
      }),
    ).toBeInTheDocument();
  });

  it("says it is getting the things ready, with a bar of no amount, until the first part goes", async () => {
    const api = fakeApi();
    open(api);
    fireEvent.click(await screen.findByRole("button", { name: "Continue" }));
    fireEvent.click(await screen.findByRole("button", { name: "Move my memory" }));
    // Still waiting for the computer's word: getting ready already.
    expect(screen.getByText("Getting your things ready…")).toBeInTheDocument();
    api.answerOffer("empty");
    await waitFor(() => expect(api.move).toHaveBeenCalled());
    act(() => api.progress?.({ phase: "preparing", done: 300, total: 1000 }));
    const bar = screen.getByRole("progressbar");
    expect(bar).not.toHaveAttribute("aria-valuenow");
    expect(bar).toHaveAttribute("aria-valuetext", "Getting your things ready…");
    expect(screen.getByRole("button", { name: "Stop" })).toBeInTheDocument();
    expect(screen.queryByText(/ of /)).toBeNull();

    act(() => api.progress?.({ phase: "sending", done: 262_144, total: 1_400_000_000 }));
    expect(screen.queryByText("Getting your things ready…")).toBeNull();
    expect(screen.getByRole("progressbar")).toHaveAttribute("aria-valuenow", "0");
    expect(screen.getByText(/^262\skB of 1\.4\sGB$/)).toBeInTheDocument();
  });

  it("brings the things into an empty computer with no restart, then lets the person choose what happens here", async () => {
    const api = fakeApi();
    const { onDone } = open(api);
    await toMoving(api);
    expect(
      screen.getByRole("heading", {
        name: "Bringing your things to Maya’s laptop",
      }),
    ).toBeInTheDocument();
    act(() =>
      api.progress?.({
        phase: "sending",
        done: 820_000_000,
        total: 1_400_000_000,
      }),
    );
    expect(screen.getByText(/^820\sMB of 1\.4\sGB/)).toBeInTheDocument();
    expect(screen.getByRole("progressbar")).toHaveAttribute("aria-valuenow", "59");
    act(() =>
      api.progress?.({
        phase: "checking",
        done: 1_400_000_000,
        total: 1_400_000_000,
      }),
    );
    expect(screen.getByRole("status")).toHaveTextContent(
      "Adding them to the memory on Maya’s laptop…",
    );
    expect(screen.queryByText(/restarts/)).toBeNull();
    expect(screen.queryByRole("button", { name: "Stop" })).toBeNull();

    api.finishMove({
      items: 1284,
      brought: { things: 1172, entries: 12, checkins: 100 },
      keepUntil: "2026-12-23T09:00:00Z",
    });
    expect(
      await screen.findByRole("heading", {
        name: "Your things are on Maya’s laptop.",
      }),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        "1,172 things, 12 journal entries and 100 check-ins were added to the memory there. Maya’s laptop reads them over the next while. This phone now works with Maya’s laptop.",
      ),
    ).toBeInTheDocument();
    const choice = screen.getByRole("group", {
      name: "Your memory on this phone",
    });
    expect(
      within(choice).getByRole("radio", {
        name: "Keep it put aside for 30 days",
      }),
    ).toBeChecked();
    expect(screen.queryByText(/stay only on this phone/)).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Done" }));
    await waitFor(() => expect(onDone).toHaveBeenCalled());
    expect(api.putAside).toHaveBeenCalledWith(false);
  });

  it("says when the move is paused, and carries on", async () => {
    const api = fakeApi();
    open(api);
    await toMoving(api);
    act(() => api.progress?.({ phase: "paused", done: 0, total: 1_400_000_000 }));
    expect(
      screen.getByText("Moving paused. It carries on when Maya’s laptop is back."),
    ).toBeInTheDocument();
  });

  it("says the move is paused while the computer is out of reach, and carries on once it answers", async () => {
    const api = fakeApi();
    const { onExit } = open(api);
    await screen.findByRole("button", { name: "Continue" });
    api.answerOffer("paused");
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    fireEvent.click(await screen.findByRole("button", { name: "Move my memory" }));
    expect(
      await screen.findByText("Moving paused. It carries on when Maya’s laptop is back."),
    ).toBeInTheDocument();
    expect(screen.queryByText(/didn’t arrive whole/)).toBeNull();
    await waitFor(() => expect(api.offer).toHaveBeenCalledTimes(2), {
      timeout: 8000,
    });
    api.answerOffer("opened");
    await waitFor(() => expect(api.move).toHaveBeenCalled());
    expect(onExit).not.toHaveBeenCalled();
  }, 12_000);

  it("says this phone was removed when it opens again part way and the computer refuses it", async () => {
    const api = fakeApi();
    const onExit = vi.fn();
    render(<MoveFlow api={api} resumed onExit={onExit} onDone={vi.fn()} />);
    fireEvent.click(await screen.findByRole("button", { name: "Continue" }));
    fireEvent.click(await screen.findByRole("button", { name: "Move my memory" }));
    api.answerOffer("declined");
    expect(
      await screen.findByRole("heading", {
        name: "This phone was removed from Nocturne.",
      }),
    ).toBeInTheDocument();
    expect(onExit).not.toHaveBeenCalled();
  });

  it("names the computer kept from the pairing when it is out of reach", async () => {
    mockIPC((cmd) => {
      if (cmd === "connect") throw "unreached";
      if (cmd === "device") return { computerName: "Maya’s laptop" };
    });
    await expect(nativeMoveApi.computer()).resolves.toBe("Maya’s laptop");
    mockIPC((cmd) => {
      if (cmd === "connect") return { kind: "connected", hello: { computerName: "Studio" } };
    });
    await expect(nativeMoveApi.computer()).resolves.toBe("Studio");
  });

  it("Stop asks first, then stops and changes nothing", async () => {
    const api = fakeApi();
    const { onExit } = open(api);
    await toMoving(api);
    fireEvent.click(screen.getByRole("button", { name: "Stop" }));
    const sheet = await screen.findByRole("dialog");
    expect(sheet).toHaveTextContent(
      "Stop bringing your things in? Nothing changes on Maya’s laptop, and your memory stays on this phone.",
    );
    expect(within(sheet).getByRole("button", { name: "Keep going" })).toBeInTheDocument();
    fireEvent.click(within(sheet).getByRole("button", { name: "Stop" }));
    expect(
      await screen.findByText("Stopped. Nothing changed on Maya’s laptop."),
    ).toBeInTheDocument();
    expect(api.stop).toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "OK" }));
    await waitFor(() => expect(onExit).toHaveBeenCalled());
  });

  it("a memory that did not arrive whole can go again", async () => {
    const api = fakeApi();
    open(api);
    await toMoving(api);
    api.finishMove("notWhole");
    expect(
      await screen.findByText("Something didn’t arrive whole, so nothing changed. Try again."),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Try again" }));
    await waitFor(() => expect(api.offer).toHaveBeenCalledTimes(2));
  });

  it("an older computer is asked to update, and nothing was changed", async () => {
    const api = fakeApi();
    open(api);
    await toMoving(api);
    api.finishMove("computerOutdated");
    expect(
      await screen.findByText(
        "Update Nocturne on Maya’s laptop, then try again. Nothing was changed.",
      ),
    ).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Try again" })).toBeNull();
  });

  it("goes back to the phone alone when the computer keeps what it has", async () => {
    const api = fakeApi(true);
    const { onExit } = open(api);
    await screen.findByRole("heading", {
      name: "Maya’s laptop already holds a memory.",
    });
    api.answerOffer("declined");
    await waitFor(() => expect(onExit).toHaveBeenCalled());
  });

  it.each([
    [
      "noRoom",
      "Maya’s laptop needs more free room for your memory. Free some there, then try again.",
    ],
    [
      "ranOutOfRoom",
      "Maya’s laptop ran out of room, so nothing changed. Your memory is still on this phone.",
    ],
  ] as const)("after %s says so, with Try again and Not now", async (why, line) => {
    const api = fakeApi();
    const { onExit } = open(api);
    await toMoving(api);
    api.finishMove(why);
    expect(await screen.findByText(line)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Try again" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Not now" }));
    await waitFor(() => expect(onExit).toHaveBeenCalled());
    expect(api.undo).toHaveBeenCalled();
  });

  it("says when the computer stopped it, with OK", async () => {
    const api = fakeApi();
    const { onExit } = open(api);
    await toMoving(api);
    api.finishMove("stoppedThere");
    expect(
      await screen.findByText(
        "Maya’s laptop stopped the move. Nothing changed, and your memory is still on this phone.",
      ),
    ).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Try again" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "OK" }));
    await waitFor(() => expect(onExit).toHaveBeenCalled());
  });
});

describe("a move that went away part way", () => {
  it("finishes straight away when its things were already added there", async () => {
    const api = fakeApi(true);
    open(api);
    api.answerOffer("opened");
    await waitFor(() => expect(api.move).toHaveBeenCalled());
    expect(
      screen.getByRole("heading", {
        name: "Bringing your things to Maya’s laptop",
      }),
    ).toBeInTheDocument();
  });
});

describe("how long is left", () => {
  it("says nothing until the move has been measured, then the minutes left", () => {
    let clock = 0;
    const now = () => clock;
    const sending = (done: number): MoveProgress => ({
      phase: "sending",
      done,
      total: 1_200_000_000,
    });
    const { rerender } = render(
      <MoveMoving
        computer="Maya’s laptop"
        progress={sending(0)}
        onStop={() => undefined}
        now={now}
      />,
    );
    clock = MEASURE_MS - 1;
    rerender(
      <MoveMoving
        computer="Maya’s laptop"
        progress={sending(10_000_000)}
        onStop={() => undefined}
        now={now}
      />,
    );
    expect(screen.queryByText(/minutes left/)).toBeNull();
    // 20 MB in 10 seconds: 2 MB a second, so 1,180 MB left is about 10 minutes.
    clock = MEASURE_MS;
    rerender(
      <MoveMoving
        computer="Maya’s laptop"
        progress={sending(20_000_000)}
        onStop={() => undefined}
        now={now}
      />,
    );
    expect(screen.getByText(/About 10 minutes left/)).toBeInTheDocument();
  });

  it("says one minute, one thing and one journal entry in the singular", async () => {
    let clock = 0;
    const now = () => clock;
    const sending = (done: number): MoveProgress => ({
      phase: "sending",
      done,
      total: 50_000_000,
    });
    const { rerender } = render(
      <MoveMoving computer="Maya’s laptop" progress={sending(0)} onStop={vi.fn()} now={now} />,
    );
    clock = MEASURE_MS;
    rerender(
      <MoveMoving
        computer="Maya’s laptop"
        progress={sending(10_000_000)}
        onStop={vi.fn()}
        now={now}
      />,
    );
    expect(screen.getByText(/About 1 minute left/)).toBeInTheDocument();
    cleanup();
    const api = fakeApi();
    api.facts = vi.fn(async () => ({
      counts: { ...COUNTS, pages: 1 },
      things: 1,
      journal: 1,
      since: "2026-03-03",
      bytes: 488_000,
      media: 8_500,
    }));
    open(api);
    fireEvent.click(await screen.findByRole("button", { name: "Continue" }));
    expect(await screen.findByText("1 thing you kept since 3 March")).toBeInTheDocument();
    expect(screen.getByText("1 journal entry")).toBeInTheDocument();
    expect(screen.queryByText("1 page"), "pages stay put aside on the phone").toBeNull();
    // A small memory of mostly words does not say it is mostly photos.
    expect(screen.getByText(/^About 488\skB$/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Move my memory" }));
    api.answerOffer("empty");
    await waitFor(() => expect(api.move).toHaveBeenCalled());
    api.finishMove({
      items: 1,
      brought: { things: 1, entries: 1, checkins: 1 },
      keepUntil: "2026-12-23T09:00:00Z",
    });
    expect(
      await screen.findByText(
        "1 thing, 1 journal entry and 1 check-in were added to the memory there. Maya’s laptop reads them over the next while. This phone now works with Maya’s laptop.",
      ),
    ).toBeInTheDocument();
  });
});

describe("moving to a computer that already holds a memory", () => {
  it("waits while the person there chooses, and Cancel undoes the pairing", async () => {
    const api = fakeApi(true);
    const { onExit } = open(api);
    expect(
      await screen.findByRole("heading", {
        name: "Maya’s laptop already holds a memory.",
      }),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        "Choose on Maya’s laptop what happens to yours. Nothing changes until you do.",
      ),
    ).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent("Waiting for Maya’s laptop…");
    expect(
      screen.queryByRole("heading", {
        name: "Move your memory to Maya’s laptop?",
      }),
    ).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(onExit).toHaveBeenCalled());
    expect(api.undo).toHaveBeenCalled();
  });

  it("brings its things in by itself once chosen there, and says what was added", async () => {
    const api = fakeApi(true);
    const { onDone } = open(api);
    await screen.findByRole("heading", {
      name: "Maya’s laptop already holds a memory.",
    });
    api.answerOffer("bring");
    await waitFor(() => expect(api.move).toHaveBeenCalled());
    expect(
      screen.getByRole("heading", {
        name: "Bringing your things to Maya’s laptop",
      }),
    ).toBeInTheDocument();
    expect(screen.getByText("Getting your things ready…")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Stop" })).toBeInTheDocument();
    act(() =>
      api.progress?.({
        phase: "sending",
        done: 700_000_000,
        total: 1_400_000_000,
      }),
    );
    expect(screen.getByText(/^700\sMB of 1\.4\sGB/)).toBeInTheDocument();
    act(() =>
      api.progress?.({
        phase: "checking",
        done: 1_400_000_000,
        total: 1_400_000_000,
      }),
    );
    expect(screen.getByRole("status")).toHaveTextContent(
      "Adding them to the memory on Maya’s laptop…",
    );
    expect(screen.queryByRole("button", { name: "Stop" })).toBeNull();
    api.finishMove({
      items: 1284,
      brought: { things: 1284, entries: 212, checkins: 96 },
      behind: {
        pages: 3,
        reflections: 1,
        answers: 5,
        names: 2,
        choices: 0,
        appointments: 0,
        readings: 0,
        settings: true,
      },
      keepUntil: null,
    });
    expect(
      await screen.findByRole("heading", {
        name: "Your things are on Maya’s laptop.",
      }),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        "1,284 things, 212 journal entries and 96 check-ins were added to the memory there. Maya’s laptop reads them over the next while. This phone now works with Maya’s laptop.",
      ),
    ).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: "Keep it put aside" })).toBeChecked();
    expect(screen.queryByRole("radio", { name: "Keep it put aside for 30 days" })).toBeNull();
    expect(
      screen.getByText(
        "These stay only on this phone, so removing it deletes them for good: 3 pages, 1 reflection, 5 answers to the morning question, 2 names you taught it and its settings.",
      ),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("radio", { name: "Remove it from this phone" }));
    fireEvent.click(screen.getByRole("button", { name: "Done" }));
    await waitFor(() => expect(onDone).toHaveBeenCalled());
    expect(api.putAside).toHaveBeenCalledWith(true);
  });

  it("asks its own way before it stops bringing the things in, then says nothing changed there", async () => {
    const api = fakeApi(true);
    const { onExit } = open(api);
    await screen.findByRole("heading", {
      name: "Maya’s laptop already holds a memory.",
    });
    api.answerOffer("bring");
    await waitFor(() => expect(api.move).toHaveBeenCalled());
    fireEvent.click(screen.getByRole("button", { name: "Stop" }));
    const sheet = await screen.findByRole("dialog");
    expect(sheet).toHaveTextContent(
      "Stop bringing your things in? Nothing changes on Maya’s laptop, and your memory stays on this phone.",
    );
    expect(within(sheet).getByRole("button", { name: "Keep going" })).toBeInTheDocument();
    fireEvent.click(within(sheet).getByRole("button", { name: "Stop" }));
    expect(
      await screen.findByText("Stopped. Nothing changed on Maya’s laptop."),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("heading", {
        name: "Bringing your things to Maya’s laptop",
      }),
    ).toBeInTheDocument();
    expect(api.stop).toHaveBeenCalled();
    api.finishMove("stopped");
    fireEvent.click(screen.getByRole("button", { name: "OK" }));
    await waitFor(() => expect(onExit).toHaveBeenCalled());
  });

  it("says the computer stopped it under the bringing title", async () => {
    const api = fakeApi(true);
    open(api);
    await screen.findByRole("heading", {
      name: "Maya’s laptop already holds a memory.",
    });
    api.answerOffer("bring");
    await waitFor(() => expect(api.move).toHaveBeenCalled());
    api.finishMove("stoppedThere");
    expect(
      await screen.findByText(
        "Maya’s laptop stopped the move. Nothing changed, and your memory is still on this phone.",
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("heading", {
        name: "Bringing your things to Maya’s laptop",
      }),
    ).toBeInTheDocument();
  });

  it("says this phone was removed when the computer removes it part way", async () => {
    const api = fakeApi(false);
    const { onExit } = open(api);
    await toMoving(api);
    api.finishMove("declined");
    expect(
      await screen.findByRole("heading", {
        name: "This phone was removed from Nocturne.",
      }),
    ).toBeInTheDocument();
    expect(screen.getByText("To use it again, pair it from your computer.")).toBeInTheDocument();
    expect(onExit).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "OK" }));
    await waitFor(() => expect(onExit).toHaveBeenCalled());
    expect(api.undo).toHaveBeenCalled();
  });

  it("uses the computer's memory, keeping this one put aside unless removed", async () => {
    const api = fakeApi(true);
    const { onDone } = open(api);
    await screen.findByRole("heading", {
      name: "Maya’s laptop already holds a memory.",
    });
    api.answerOffer("useHere");
    expect(
      await screen.findByRole("heading", {
        name: "This phone now works with Maya’s laptop.",
      }),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        "Nothing went to Maya’s laptop. What this phone held is put aside here, and its calendar and health go to Maya’s laptop from now on.",
      ),
    ).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: "Keep it put aside" })).toBeChecked();
    expect(
      screen.getByText(
        "Nothing from this phone went to Maya’s laptop, so removing it deletes it for good.",
      ),
    ).toBeInTheDocument();
    expect(api.move).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Done" }));
    await waitFor(() => expect(onDone).toHaveBeenCalled());
    expect(api.putAside).toHaveBeenCalledWith(false);
  });

  it("keeps waiting, and asks again, when the connection goes while the person there chooses", async () => {
    const api = fakeApi(true);
    const { onExit } = open(api);
    await screen.findByRole("heading", {
      name: "Maya’s laptop already holds a memory.",
    });
    api.answerOffer("paused");
    await waitFor(() => expect(api.offer).toHaveBeenCalledTimes(2), {
      timeout: 8000,
    });
    expect(screen.getByRole("status")).toHaveTextContent("Waiting for Maya’s laptop…");
    expect(onExit).not.toHaveBeenCalled();
    api.answerOffer("useHere");
    expect(
      await screen.findByRole("heading", {
        name: "This phone now works with Maya’s laptop.",
      }),
    ).toBeInTheDocument();
  }, 12_000);

  it("follows the choice made there when the person here already chose to move", async () => {
    const api = fakeApi(false);
    open(api);
    fireEvent.click(await screen.findByRole("button", { name: "Continue" }));
    fireEvent.click(await screen.findByRole("button", { name: "Move my memory" }));
    api.answerOffer("useHere");
    expect(
      await screen.findByRole("heading", {
        name: "This phone now works with Maya’s laptop.",
      }),
    ).toBeInTheDocument();
    expect(api.move).not.toHaveBeenCalled();
  });

  it("asks as for any move when the computer takes the memory whole after all", async () => {
    const api = fakeApi(true);
    open(api);
    await screen.findByRole("heading", {
      name: "Maya’s laptop already holds a memory.",
    });
    api.answerOffer(null);
    expect(
      await screen.findByRole("heading", {
        name: "Move your memory to Maya’s laptop?",
      }),
    ).toBeInTheDocument();
  });
});
