import { emit } from "@tauri-apps/api/event";
import { clearMocks, mockConvertFileSrc, mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { TauriPlatform } from "./tauri";

let calls: Array<{ cmd: string; args: unknown }>;

beforeEach(() => {
  calls = [];
  mockConvertFileSrc("linux");
  mockIPC(
    (cmd, args) => {
      calls.push({ cmd, args });
      return cmd === "pick_folder" ? "/chosen" : null;
    },
    { shouldMockEvents: true },
  );
});
afterEach(() => clearMocks());

describe("TauriPlatform shared pieces", () => {
  it("asks the app for a folder, not the dialog plugin", async () => {
    await expect(new TauriPlatform().pickFolder()).resolves.toBe("/chosen");
    expect(calls[0]).toEqual({ cmd: "pick_folder", args: {} });
  });

  it("asks the app to print the screen", async () => {
    await new TauriPlatform().print();
    expect(calls[0]).toEqual({ cmd: "print_page", args: {} });
  });

  it("asks the app for files", async () => {
    await new TauriPlatform().pickFiles();
    expect(calls[0]?.cmd).toBe("pick_files");
  });

  it("opens only through the app's own commands, never the opener plugin", async () => {
    const platform = new TauriPlatform();
    await platform.openPath("/exports/Nocturne export");
    await platform.openOriginal("itm_1");
    await platform.openLogs();
    await platform.openDataFolder();
    expect(calls).toEqual([
      { cmd: "open_export", args: { path: "/exports/Nocturne export" } },
      { cmd: "open_original", args: { itemId: "itm_1" } },
      { cmd: "open_logs", args: {} },
      { cmd: "open_data_folder", args: {} },
    ]);
    expect(calls.some((c) => c.cmd.startsWith("plugin:"))).toBe(false);
  });

  it("knows the quick capture window by its label", () => {
    mockWindows("main", "quick-capture");
    expect(new TauriPlatform().inQuickCapture()).toBe(false);
    mockWindows("quick-capture");
    expect(new TauriPlatform().inQuickCapture()).toBe(true);
  });

  it("hears files dropped on quick capture by asking the app, not by listening", async () => {
    vi.useFakeTimers();
    try {
      mockWindows("quick-capture");
      let held: string[] = [];
      mockIPC((cmd) => {
        calls.push({ cmd, args: {} });
        if (cmd !== "take_quick_drop") return null;
        const out = held;
        held = [];
        return out;
      });
      const dropped = vi.fn();
      const off = new TauriPlatform().onFileDrop(dropped, vi.fn());
      await vi.advanceTimersByTimeAsync(500);
      expect(dropped).not.toHaveBeenCalled();
      held = ["/files/note.txt"];
      await vi.advanceTimersByTimeAsync(500);
      expect(dropped).toHaveBeenCalledExactlyOnceWith(["/files/note.txt"]);
      expect(calls.some((c) => c.cmd.startsWith("plugin:event"))).toBe(false);
      off();
      const asked = calls.length;
      await vi.advanceTimersByTimeAsync(2000);
      expect(calls).toHaveLength(asked);
    } finally {
      vi.useRealTimers();
    }
  });

  it("turns a file path into a URL the page may show", () => {
    expect(new TauriPlatform().originalSrc("/data/originals/a.jpg")).toContain("asset");
  });

  it("hears a question handed off from another window", async () => {
    const heard = vi.fn();
    const off = new TauriPlatform().onAskHandOff(heard);
    await new Promise((r) => setTimeout(r, 0));
    await emit("ask-handoff", "When is the dentist?");
    expect(heard).toHaveBeenCalledWith("When is the dentist?");
    off();
  });

  it("hears a screen another window asked the main window to show", async () => {
    const heard = vi.fn();
    const off = new TauriPlatform().onOpenInMain(heard);
    await new Promise((r) => setTimeout(r, 0));
    await emit("open-in-main", "/items/item-1");
    expect(heard).toHaveBeenCalledWith("/items/item-1");
    off();
  });
});

describe("TauriPlatform without permission to listen", () => {
  it("is refused quietly, with no unhandled rejection", async () => {
    clearMocks();
    mockIPC(
      (cmd) => {
        if (cmd.startsWith("plugin:event|")) throw new Error("event.listen not allowed");
        return null;
      },
      { shouldMockEvents: false },
    );
    const unhandled = vi.fn();
    process.on("unhandledRejection", unhandled);
    const off = new TauriPlatform().onAskHandOff(() => undefined);
    await new Promise((r) => setTimeout(r, 20));
    off();
    await new Promise((r) => setTimeout(r, 20));
    expect(unhandled).not.toHaveBeenCalled();
    process.off("unhandledRejection", unhandled);
  });
});
