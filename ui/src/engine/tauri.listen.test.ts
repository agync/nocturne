import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const listen = vi.fn();
vi.mock("@tauri-apps/api/event", () => ({ listen: (...a: unknown[]) => listen(...a) }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { TauriEngine } from "./tauri";

beforeEach(() => {
  vi.useFakeTimers();
  listen.mockReset();
});
afterEach(() => {
  vi.useRealTimers();
});

describe("TauriEngine subscribing", () => {
  it("logs a failed subscription and tries once more, without throwing into the page", async () => {
    const error = vi.spyOn(console, "error").mockImplementation(() => undefined);
    listen.mockRejectedValueOnce(new Error("not ready")).mockResolvedValue(() => undefined);
    const unhandled = vi.fn();
    process.on("unhandledRejection", unhandled);
    const engine = new TauriEngine();
    await vi.advanceTimersByTimeAsync(600);
    expect(error).toHaveBeenCalledWith(
      expect.stringContaining("status-changed"),
      expect.any(Error),
    );
    // 7 events, plus one retry for the one that failed.
    expect(listen).toHaveBeenCalledTimes(8);
    expect(unhandled).not.toHaveBeenCalled();
    engine.dispose();
    process.off("unhandledRejection", unhandled);
    error.mockRestore();
  });

  it("gives up quietly after the retry fails too", async () => {
    const error = vi.spyOn(console, "error").mockImplementation(() => undefined);
    listen.mockRejectedValue(new Error("down"));
    const engine = new TauriEngine();
    await vi.advanceTimersByTimeAsync(2000);
    expect(listen).toHaveBeenCalledTimes(14);
    engine.dispose();
    error.mockRestore();
  });
});

describe("TauriEngine in a window that may not listen", () => {
  it("never subscribes, so nothing is refused or logged", async () => {
    const error = vi.spyOn(console, "error").mockImplementation(() => undefined);
    const engine = new TauriEngine({ listen: false });
    await vi.advanceTimersByTimeAsync(2000);
    expect(listen).not.toHaveBeenCalled();
    expect(error).not.toHaveBeenCalled();
    const heard = vi.fn();
    engine.on("status-changed", heard)();
    engine.dispose();
    error.mockRestore();
  });
});
