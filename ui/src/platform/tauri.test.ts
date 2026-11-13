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
      return cmd === "plugin:dialog|open" ? "/chosen" : null;
    },
    { shouldMockEvents: true },
  );
});
afterEach(() => clearMocks());

describe("TauriPlatform shared pieces", () => {
  it("asks for a folder through the dialog plugin", async () => {
    await expect(new TauriPlatform().pickFolder()).resolves.toBe("/chosen");
    expect(calls[0]).toEqual({
      cmd: "plugin:dialog|open",
      args: { options: { multiple: false, directory: true } },
    });
  });

  it("opens a path with the opener plugin", async () => {
    await new TauriPlatform().openPath("/some/path");
    expect(calls[0]).toEqual({ cmd: "plugin:opener|open_path", args: { path: "/some/path" } });
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
});
