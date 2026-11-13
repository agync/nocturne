import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { act, cleanup, fireEvent, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { EngineClient } from "../../engine/client";
import { createMockEngine } from "../../engine/mock/engine";
import { fakePlatform, renderApp, setViewport } from "../../test/render";

/** The commands the shell lets the quick capture window call, read from the shell's own source. */
function allowedCommands(): string[] {
  const src = readFileSync(resolve(process.cwd(), "../src-tauri/src/os/mod.rs"), "utf8");
  const body = /QUICK_CAPTURE_COMMANDS: &\[&str\] = &\[([^\]]*)\]/.exec(src)?.[1] ?? "";
  return [...body.matchAll(/"([a-z_]+)"/g)].map((m) => m[1] as string);
}

/** The command each client method sends, read from the Tauri client. */
function commandOf(method: string): string | null {
  const src = readFileSync(resolve(process.cwd(), "src/engine/tauri.ts"), "utf8");
  const at = src.search(new RegExp(`^  ${method}(?::| =)`, "m"));
  if (at < 0) return null;
  return /(?:this\.call|invoke)(?:<[^(]*>)?\(\s*"([a-z_]+)"/.exec(src.slice(at))?.[1] ?? null;
}

class FakeRecorder {
  state: "inactive" | "recording" = "inactive";
  mimeType = "audio/webm";
  ondataavailable: ((e: { data: Blob }) => void) | null = null;
  onstop: (() => void) | null = null;
  start() {
    this.state = "recording";
  }
  stop() {
    this.state = "inactive";
    this.ondataavailable?.({ data: new Blob([new Uint8Array([1, 2, 3, 4])]) });
    this.onstop?.();
  }
}

beforeEach(() => {
  setViewport(560);
  vi.stubGlobal("MediaRecorder", FakeRecorder);
  Object.defineProperty(navigator, "mediaDevices", {
    configurable: true,
    value: {
      getUserMedia: vi.fn().mockResolvedValue({ getTracks: () => [{ stop: vi.fn() }] }),
    },
  });
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
  window.location.hash = "";
});

/** An engine that notes the name of every method called on it (events are not commands). */
function recording() {
  const calls = new Set<string>();
  const inner = createMockEngine();
  const engine = new Proxy(inner, {
    get(target, prop, receiver) {
      const value = Reflect.get(target, prop, receiver);
      if (typeof value !== "function" || typeof prop !== "string" || prop === "on") return value;
      return (...args: unknown[]) => {
        calls.add(prop);
        return (value as (...a: unknown[]) => unknown).apply(target, args);
      };
    },
  }) as EngineClient;
  return { engine, calls };
}

describe("what the quick capture window asks of the engine", () => {
  // Seven windows are opened one after another and typed into, so this one is given room.
  it("is only what the shell allows it to call", { timeout: 30_000 }, async () => {
    const allowed = allowedCommands();
    expect(allowed).toContain("keep_text");

    const { engine, calls } = recording();
    const platform = fakePlatform(null, { quickCapture: true });
    /** Each thing the window can do starts from a freshly opened window. */
    const fresh = async () => {
      cleanup();
      renderApp({ hash: "#/quick-capture", engine, platform });
      return screen.findByRole("textbox", { name: "Keep something, or ask" });
    };

    let box = await fresh();
    await userEvent.type(box, "Buy milk{Enter}");
    await userEvent.click(await screen.findByRole("button", { name: "Undo" }));
    await screen.findByText("Taken back. Nothing was kept.");

    box = await fresh();
    await userEvent.type(box, "Gran’s new number ends 4471.{Enter}");
    await userEvent.click(await screen.findByRole("button", { name: "Open it" }));

    box = await fresh();
    const file = new File([new Uint8Array([9])], "x.png", { type: "image/png" });
    fireEvent.paste(box, { clipboardData: { files: [file], types: ["Files"] } });
    await screen.findByText("Photo. I’ll read it tonight.");

    await fresh();
    act(() => platform.drop(["/tmp/a.jpg"]));
    await waitFor(() => expect(calls.has("keepFiles")).toBe(true));

    box = await fresh();
    await userEvent.type(box, "When is the dentist?{Enter}");

    box = await fresh();
    const start = 1_700_000_000_000;
    const now = vi.spyOn(Date, "now").mockReturnValue(start);
    box.focus();
    fireEvent.keyDown(box, { key: " " });
    await screen.findByText("Listening");
    now.mockReturnValue(start + 1200);
    fireEvent.keyUp(box, { key: " " });
    await waitFor(() => expect(calls.has("keepRecording")).toBe(true));

    // A microphone that is turned off, and the notice's button.
    box = await fresh();
    const media = navigator.mediaDevices.getUserMedia as ReturnType<typeof vi.fn>;
    media.mockRejectedValue(new DOMException("no", "NotAllowedError"));
    await userEvent.click(screen.getByRole("button", { name: "Record a voice note" }));
    await userEvent.click(await screen.findByRole("button", { name: "Open sound settings" }));
    await waitFor(() => expect(calls.has("openSoundSettings")).toBe(true));

    for (const method of [
      "keepText",
      "unkeep",
      "keepFiles",
      "keepBytes",
      "keepRecording",
      "openSoundSettings",
    ]) {
      expect(calls.has(method), `the session did not exercise ${method}`).toBe(true);
    }
    for (const method of calls) {
      const command = commandOf(method);
      expect(command, `${method} is not a command`).not.toBeNull();
      expect(allowed, `quick capture called ${method} (${command})`).toContain(command);
    }
  });
});
