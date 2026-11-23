import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { AsideApi, MoveAside, Moved, MoveError } from "../alone/screens/MoveFlow";
import type { MoveProgress } from "../alone/screens/MoveScreens";
import type { Connection, PairedDevice } from "../engine";
import type { NativeChannel, ThisPhoneSources } from "./api";
import { ThisPhone } from "./screens/ThisPhone";

afterEach(cleanup);

const sources: ThisPhoneSources = {
  calendar: "on",
  calendars: 1,
  health: "notBrought",
  kinds: 0,
  lastSent: null,
  calendarLastSent: null,
  healthLastSent: null,
  healthFrom: null,
  computerTakes: ["calendar", "health"],
  calendarTooMany: false,
};

const device = {
  deviceName: "Maya’s phone",
  computerName: "Maya’s laptop",
  connection: "simple",
} as unknown as PairedDevice;

const connection = { kind: "connecting" } as Connection;

interface Fake extends AsideApi {
  progress?: (p: MoveProgress) => void;
  finish(r: Moved | MoveError): void;
}

function fakeAside(put: MoveAside | null): Fake {
  let settle: ((r: Moved | MoveError) => void) | null = null;
  const api: Fake = {
    aside: vi.fn(async () => put),
    bring: vi.fn(
      (onProgress: (p: MoveProgress) => void) =>
        new Promise<Moved>((ok, no) => {
          api.progress = onProgress;
          settle = (r) => (typeof r === "string" ? no(r) : ok(r));
        }),
    ),
    stop: vi.fn(async () => undefined),
    putAside: vi.fn(async () => undefined),
    finish: (r) => act(() => settle?.(r)),
  };
  return api;
}

function open(aside: AsideApi) {
  const channel = {
    sources: vi.fn(async () => sources),
  } as unknown as NativeChannel;
  render(
    <ThisPhone
      channel={channel}
      device={device}
      connection={connection}
      version="0.7.2"
      waiting={0}
      aside={aside}
      onBack={() => undefined}
      onCalendar={() => undefined}
      onHealth={() => undefined}
      onDisconnect={async () => undefined}
    />,
  );
}

describe("the memory a move put aside, in This phone", () => {
  it("offers to bring its things in when nothing of it went, and says what was added", async () => {
    const aside = fakeAside({ onComputer: false, keepUntil: null });
    open(aside);
    fireEvent.click(
      await screen.findByRole("button", {
        name: "Bring its things to Maya’s laptop…",
      }),
    );
    fireEvent.click(await screen.findByRole("button", { name: "Bring them in" }));
    expect(
      await screen.findByRole("heading", {
        name: "Bringing your things to Maya’s laptop",
      }),
    ).toBeInTheDocument();
    act(() => aside.progress?.({ phase: "checking", done: 10, total: 10 }));
    expect(screen.getByRole("status")).toHaveTextContent(
      "Adding them to the memory on Maya’s laptop…",
    );
    aside.finish({
      items: 40,
      brought: { things: 38, entries: 2, checkins: 0 },
      keepUntil: null,
    });
    expect(
      await screen.findByRole("heading", {
        name: "Your things are on Maya’s laptop.",
      }),
    ).toBeInTheDocument();
    // It was the only copy, so bringing it in never makes it go by itself.
    expect(screen.getByRole("radio", { name: "Keep it put aside" })).toBeChecked();
    fireEvent.click(screen.getByRole("button", { name: "Done" }));
    await waitFor(() => expect(aside.putAside).toHaveBeenCalledWith(false));
  });

  it("asks first, with what it holds, and brings nothing in on Not now", async () => {
    const aside = fakeAside({
      onComputer: false,
      keepUntil: null,
      at: "2026-11-23T08:00:00Z",
      counts: {
        things: 12,
        journalEntries: 1,
        photosAndRecordings: 3,
        checkins: 4,
      },
    });
    open(aside);
    fireEvent.click(
      await screen.findByRole("button", {
        name: "Bring its things to Maya’s laptop…",
      }),
    );
    expect(
      await screen.findByRole("heading", {
        name: "Bring the things put aside Monday 23 November to Maya’s laptop?",
      }),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        "12 things kept, 1 journal entry and 4 check-ins join the memory on Maya’s laptop, which reads them as new. What is put aside here stays until you remove it.",
      ),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Not now" }));
    expect(aside.bring).not.toHaveBeenCalled();
    expect(
      await screen.findByRole("button", {
        name: "Bring its things to Maya’s laptop…",
      }),
    ).toBeInTheDocument();
  });

  it("only offers to remove it once its things are on the computer", async () => {
    const aside = fakeAside({
      onComputer: true,
      keepUntil: "2026-12-23T09:00:00Z",
    });
    open(aside);
    expect(await screen.findByRole("button", { name: "Remove…" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Bring its things/ })).toBeNull();
  });

  it("asks to delete it for good, with what it holds, when nothing of it went", async () => {
    const aside = fakeAside({
      onComputer: false,
      keepUntil: null,
      at: "2026-11-23T08:00:00Z",
      counts: {
        things: 12,
        journalEntries: 1,
        photosAndRecordings: 3,
        checkins: 4,
      },
    });
    open(aside);
    fireEvent.click(await screen.findByRole("button", { name: "Remove…" }));
    expect(
      screen.getByRole("heading", {
        name: "Delete what was put aside Monday 23 November for good?",
      }),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        "Nothing from it went to Maya’s laptop. This deletes 12 things kept, 1 journal entry and 3 photos and recordings, and they can’t be brought back.",
      ),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Keep it" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Delete for good" }));
    await waitFor(() => expect(aside.putAside).toHaveBeenCalledWith(true));
  });

  it("asks to remove the copy left here once its things are on the computer", async () => {
    const aside = fakeAside({
      onComputer: true,
      keepUntil: "2026-12-23T08:00:00Z",
      at: "2026-11-23T08:00:00Z",
      counts: {
        things: 12,
        journalEntries: 1,
        photosAndRecordings: 3,
        checkins: 4,
      },
      behind: {
        pages: 1,
        reflections: 2,
        answers: 0,
        names: 0,
        choices: 1,
        appointments: 3,
        readings: 1,
        settings: false,
      },
    });
    open(aside);
    fireEvent.click(await screen.findByRole("button", { name: "Remove…" }));
    expect(
      screen.getByRole("heading", {
        name: "Remove what was put aside Monday 23 November?",
      }),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        "Its things are on Maya’s laptop now. This removes the copy left on this phone.",
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        "These stay only on this phone, so removing it deletes them for good: 1 page, 2 reflections, 1 thing you confirmed or settled, 3 appointments and 1 health reading.",
      ),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Keep it" }));
    expect(aside.putAside).not.toHaveBeenCalled();
  });

  it("says why bringing them in did not finish, and nothing changed", async () => {
    const aside = fakeAside({ onComputer: false, keepUntil: null });
    open(aside);
    fireEvent.click(
      await screen.findByRole("button", {
        name: "Bring its things to Maya’s laptop…",
      }),
    );
    fireEvent.click(await screen.findByRole("button", { name: "Bring them in" }));
    await waitFor(() => expect(aside.bring).toHaveBeenCalled());
    aside.finish("ranOutOfRoom");
    expect(
      await screen.findByText(
        "Maya’s laptop ran out of room, so nothing changed. Your memory is still on this phone.",
      ),
    ).toBeInTheDocument();
  });

  it("stops bringing them in when asked its own way, and says nothing changed there", async () => {
    const aside = fakeAside({ onComputer: false, keepUntil: null });
    open(aside);
    fireEvent.click(
      await screen.findByRole("button", {
        name: "Bring its things to Maya’s laptop…",
      }),
    );
    fireEvent.click(await screen.findByRole("button", { name: "Bring them in" }));
    await waitFor(() => expect(aside.bring).toHaveBeenCalled());
    fireEvent.click(screen.getByRole("button", { name: "Stop" }));
    const sheet = await screen.findByRole("dialog");
    expect(sheet).toHaveTextContent(
      "Stop bringing your things in? Nothing changes on Maya’s laptop, and your memory stays on this phone.",
    );
    fireEvent.click(within(sheet).getByRole("button", { name: "Stop" }));
    aside.finish("stopped");
    expect(
      await screen.findByText("Stopped. Nothing changed on Maya’s laptop."),
    ).toBeInTheDocument();
    expect(aside.stop).toHaveBeenCalled();
  });

  it("does not say it stopped when the computer is already adding them", async () => {
    const aside = fakeAside({ onComputer: false, keepUntil: null });
    aside.stop = vi.fn(async () => Promise.reject("failed"));
    open(aside);
    fireEvent.click(
      await screen.findByRole("button", {
        name: "Bring its things to Maya’s laptop…",
      }),
    );
    fireEvent.click(await screen.findByRole("button", { name: "Bring them in" }));
    await waitFor(() => expect(aside.bring).toHaveBeenCalled());
    fireEvent.click(screen.getByRole("button", { name: "Stop" }));
    fireEvent.click(
      within(await screen.findByRole("dialog")).getByRole("button", {
        name: "Stop",
      }),
    );
    await waitFor(() => expect(aside.stop).toHaveBeenCalled());
    aside.finish({
      items: 40,
      brought: { things: 38, entries: 2, checkins: 0 },
      keepUntil: null,
    });
    expect(
      await screen.findByRole("heading", {
        name: "Your things are on Maya’s laptop.",
      }),
    ).toBeInTheDocument();
    expect(screen.queryByText(/^Stopped\./)).toBeNull();
  });

  it("shows nothing of it when no move put a memory aside", async () => {
    const aside = fakeAside(null);
    open(aside);
    await waitFor(() => expect(aside.aside).toHaveBeenCalled());
    expect(screen.queryByRole("button", { name: "Remove…" })).toBeNull();
  });
});
