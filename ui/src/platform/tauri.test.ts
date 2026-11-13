import { emit } from "@tauri-apps/api/event";
import { clearMocks, mockConvertFileSrc, mockIPC } from "@tauri-apps/api/mocks";
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
    expect(calls).toEqual([
      { cmd: "open_export", args: { path: "/exports/Nocturne export" } },
      { cmd: "open_original", args: { itemId: "itm_1" } },
      { cmd: "open_logs", args: {} },
    ]);
    expect(calls.some((c) => c.cmd.startsWith("plugin:"))).toBe(false);
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
