import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { MemoryCounts } from "../../../engine/bindings";
import { type MoveApi, type MoveError, MoveFlow, type MoveOffered } from "./MoveFlow";
import { MEASURE_MS, MoveMoving, type MoveProgress } from "./MoveScreens";

afterEach(cleanup);

const COUNTS: MemoryCounts = { items: 1284, memories: 0, pages: 0, originals: 40 };

interface Fake extends MoveApi {
  progress?: (p: MoveProgress) => void;
  finishMove(result: { items: number } | MoveError): void;
  answerOffer(result: MemoryCounts | null | MoveError | "opened"): void;
}

function fakeApi(): Fake {
  let settleMove: ((r: { items: number } | MoveError) => void) | null = null;
  let settleOffer: ((r: MemoryCounts | null | MoveError | "opened") => void) | null = null;
  const api: Fake = {
    computer: vi.fn(async () => "Maya’s laptop"),
    facts: vi.fn(async () => ({
      counts: COUNTS,
      things: 1284,
      journal: 12,
      since: "2026-03-03",
      bytes: 1_400_000_000,
    })),
    offer: vi.fn(
      () =>
        new Promise<MoveOffered>((ok, no) => {
          settleOffer = (r) =>
            r === "opened"
              ? ok({ opened: true, here: null })
              : typeof r === "string"
                ? no(r)
                : ok({ opened: false, here: r });
        }),
    ),
    move: vi.fn(
      (onProgress: (p: MoveProgress) => void) =>
        new Promise<{ items: number }>((ok, no) => {
          api.progress = onProgress;
          settleMove = (r) => (typeof r === "string" ? no(r) : ok(r));
        }),
    ),
    stop: vi.fn(async () => undefined),
    undo: vi.fn(async () => undefined),
    putAside: vi.fn(async () => undefined),
    pending: vi.fn(async () => false),
    finishMove: (r) => act(() => settleMove?.(r)),
    answerOffer: (r) => act(() => settleOffer?.(r)),
  };
  return api;
}

function open(api: Fake) {
  const onExit = vi.fn();
  const onDone = vi.fn();
  render(<MoveFlow api={api} onExit={onExit} onDone={onDone} />);
  return { onExit, onDone };
}

async function toMoving(api: Fake) {
  fireEvent.click(await screen.findByRole("button", { name: "Continue" }));
  fireEvent.click(await screen.findByRole("button", { name: "Move my memory" }));
  api.answerOffer(null);
  await waitFor(() => expect(api.move).toHaveBeenCalled());
}

describe("moving the memory to a computer", () => {
  it("asks first, and the offer goes to the computer at once", async () => {
    const api = fakeApi();
    open(api);
    expect(
      await screen.findByRole("heading", { name: "Move your memory to Maya’s laptop?" }),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        "Maya’s laptop becomes the home of your memory. It reads, tidies and writes your page from now on.",
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByText("Your memory lives in one place at a time, so it moves rather than copies."),
    ).toBeInTheDocument();
    expect(api.offer).toHaveBeenCalledTimes(1);
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
      await screen.findByRole("heading", { name: "Move your memory to Maya’s laptop?" }),
    ).toBeInTheDocument();
  });

  it("says it is getting the memory ready, with a bar of no amount, until the first part goes", async () => {
    const api = fakeApi();
    open(api);
    fireEvent.click(await screen.findByRole("button", { name: "Continue" }));
    fireEvent.click(await screen.findByRole("button", { name: "Move my memory" }));
    // Still waiting for the computer's word: getting ready already.
    expect(screen.getByText("Getting your memory ready to move…")).toBeInTheDocument();
    api.answerOffer(null);
    await waitFor(() => expect(api.move).toHaveBeenCalled());
    act(() => api.progress?.({ phase: "preparing", done: 300, total: 1000 }));
    const bar = screen.getByRole("progressbar");
    expect(bar).not.toHaveAttribute("aria-valuenow");
    expect(bar).toHaveAttribute("aria-valuetext", "Getting your memory ready to move…");
    expect(screen.getByRole("button", { name: "Stop" })).toBeInTheDocument();
    expect(screen.queryByText(/ of /)).toBeNull();

    act(() => api.progress?.({ phase: "sending", done: 262_144, total: 1_400_000_000 }));
    expect(screen.queryByText("Getting your memory ready to move…")).toBeNull();
    expect(screen.getByRole("progressbar")).toHaveAttribute("aria-valuenow", "0");
    expect(screen.getByText(/^262\skB of 1\.4\sGB$/)).toBeInTheDocument();
  });

  it("moves, shows how far, then lets the person choose what happens to it here", async () => {
    const api = fakeApi();
    const { onDone } = open(api);
    await toMoving(api);
    expect(screen.getByRole("heading", { name: "Moving your memory" })).toBeInTheDocument();
    expect(
      screen.getByText("You can still keep things. They go across at the end."),
    ).toBeInTheDocument();
    act(() => api.progress?.({ phase: "sending", done: 820_000_000, total: 1_400_000_000 }));
    expect(screen.getByText(/^820\sMB of 1\.4\sGB$/)).toBeInTheDocument();
    expect(screen.getByRole("progressbar")).toHaveAttribute("aria-valuenow", "59");

    api.finishMove({ items: 1284 });
    expect(
      await screen.findByRole("heading", { name: "Your memory is on Maya’s laptop." }),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        "1,284 things arrived, and every count matches. This phone now works with Maya’s laptop.",
      ),
    ).toBeInTheDocument();
    const choice = screen.getByRole("group", { name: "Your memory on this phone" });
    expect(within(choice).getByRole("radio", { name: "Remove it from this phone" })).toBeChecked();
    fireEvent.click(within(choice).getByRole("radio", { name: "Keep it put aside for 30 days" }));
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

  it("Stop asks first, then stops and changes nothing", async () => {
    const api = fakeApi();
    const { onExit } = open(api);
    await toMoving(api);
    fireEvent.click(screen.getByRole("button", { name: "Stop" }));
    const sheet = await screen.findByRole("dialog");
    expect(
      within(sheet).getByText(
        "Stop moving? Nothing has changed here, and Maya’s laptop keeps nothing of it.",
      ),
    ).toBeInTheDocument();
    expect(within(sheet).getByRole("button", { name: "Keep moving" })).toBeInTheDocument();
    fireEvent.click(within(sheet).getByRole("button", { name: "Stop moving" }));
    await waitFor(() => expect(onExit).toHaveBeenCalled());
    expect(api.stop).toHaveBeenCalled();
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
    const api = fakeApi();
    const { onExit } = open(api);
    await screen.findByRole("button", { name: "Continue" });
    api.answerOffer("declined");
    await waitFor(() => expect(onExit).toHaveBeenCalled());
  });

  it("says it is finishing while the computer restarts, with nothing left to stop", async () => {
    const api = fakeApi();
    open(api);
    await toMoving(api);
    act(() => api.progress?.({ phase: "finishing", done: 1, total: 1 }));
    expect(
      screen.getByText(
        "Finishing on Maya’s laptop. Nocturne there restarts to open your memory; this takes a moment.",
      ),
    ).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Stop" })).toBeNull();
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
    [
      "notOpened",
      "Maya’s laptop couldn’t open your memory, so nothing changed. Your memory is still on this phone.",
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
  it("finishes straight away when the memory already opened there", async () => {
    const api = fakeApi();
    open(api);
    api.answerOffer("opened");
    await waitFor(() => expect(api.move).toHaveBeenCalled());
    expect(screen.getByRole("heading", { name: "Moving your memory" })).toBeInTheDocument();
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
});
