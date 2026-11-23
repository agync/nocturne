import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
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
  const channel = { sources: vi.fn(async () => sources) } as unknown as NativeChannel;
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
      await screen.findByRole("button", { name: "Bring its things to Maya’s laptop…" }),
    );
    expect(
      await screen.findByRole("heading", { name: "Bringing your things to Maya’s laptop" }),
    ).toBeInTheDocument();
    act(() => aside.progress?.({ phase: "checking", done: 10, total: 10 }));
    expect(screen.getByRole("status")).toHaveTextContent(
      "Adding them to the memory on Maya’s laptop…",
    );
    aside.finish({ items: 40, brought: { things: 38, entries: 2, checkins: 0 } });
    expect(
      await screen.findByRole("heading", { name: "Your things are on Maya’s laptop." }),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("radio", { name: "Keep it put aside for 30 days" }));
    fireEvent.click(screen.getByRole("button", { name: "Done" }));
    await waitFor(() => expect(aside.putAside).toHaveBeenCalledWith(false));
  });

  it("only offers to remove it once its things are on the computer", async () => {
    const aside = fakeAside({ onComputer: true, keepUntil: "2026-12-23T09:00:00Z" });
    open(aside);
    expect(await screen.findByRole("button", { name: "Remove…" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Bring its things/ })).toBeNull();
  });

  it("says removing deletes it for good when nothing of it went", async () => {
    const aside = fakeAside({ onComputer: false, keepUntil: null });
    open(aside);
    fireEvent.click(await screen.findByRole("button", { name: "Remove…" }));
    expect(
      screen.getByText(
        "Nothing from this phone went to Maya’s laptop, so removing it deletes it for good.",
      ),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Remove it from this phone" }));
    await waitFor(() => expect(aside.putAside).toHaveBeenCalledWith(true));
  });

  it("says why bringing them in did not finish, and nothing changed", async () => {
    const aside = fakeAside({ onComputer: false, keepUntil: null });
    open(aside);
    fireEvent.click(
      await screen.findByRole("button", { name: "Bring its things to Maya’s laptop…" }),
    );
    await waitFor(() => expect(aside.bring).toHaveBeenCalled());
    aside.finish("ranOutOfRoom");
    expect(
      await screen.findByText(
        "Maya’s laptop ran out of room, so nothing changed. Your memory is still on this phone.",
      ),
    ).toBeInTheDocument();
  });

  it("shows nothing of it when no move put a memory aside", async () => {
    const aside = fakeAside(null);
    open(aside);
    await waitFor(() => expect(aside.aside).toHaveBeenCalled());
    expect(screen.queryByRole("button", { name: "Remove…" })).toBeNull();
  });
});
