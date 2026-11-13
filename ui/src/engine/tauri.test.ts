import { emit } from "@tauri-apps/api/event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import type { EngineClient } from "./client";
import { TauriEngine } from "./tauri";

interface Call {
  cmd: string;
  args: Record<string, unknown> | undefined;
}

let calls: Call[];
let engine: TauriEngine;

beforeEach(() => {
  calls = [];
  mockIPC(
    (cmd, args) => {
      calls.push({ cmd, args: args as Record<string, unknown> | undefined });
      return null;
    },
    { shouldMockEvents: true },
  );
  engine = new TauriEngine();
});

afterEach(() => {
  engine.dispose();
  clearMocks();
});

const patch = {
  pageTime: null,
  delivery: null,
  autoPrint: null,
  voice: null,
  voiceSpeed: null,
  shortcut: null,
  inboxFolder: null,
  startAtLogin: null,
  lettingGo: null,
  theme: null,
  language: null,
  checkUpdates: null,
  reminders: null,
  lock: null,
  agents: null,
};

/** Every method, with the command and argument keys the roadmap contract names. */
const table: Array<[keyof EngineClient, unknown[], string, string[]]> = [
  ["getStatus", [], "get_status", []],
  ["getSetup", [], "get_setup", []],
  ["startSetup", ["standard"], "start_setup", ["tier"]],
  ["pauseSetup", [], "pause_setup", []],
  ["resumeSetup", [], "resume_setup", []],
  ["keepText", ["a note", "quickCapture"], "keep_text", ["text", "source"]],
  ["keepFiles", [["a.pdf"], "drop"], "keep_files", ["paths", "source"]],
  ["keepRecording", [[1, 2, 3], "audio/webm"], "keep_recording", ["bytes", "mime"]],
  ["listItems", [{ since: null, status: null, limit: null }], "list_items", ["filter"]],
  ["getItem", ["i1"], "get_item", ["id"]],
  ["retryItem", ["i1"], "retry_item", ["id"]],
  ["ask", ["When?"], "ask", ["question"]],
  ["getPage", ["2026-11-13"], "get_page", ["date"]],
  ["listPages", [], "list_pages", []],
  ["answerQuestion", ["2026-11-13", "yes"], "answer_question", ["pageDate", "choice"]],
  ["resolveWhich", ["c1", "m1"], "resolve_which", ["conflictId", "memoryId"]],
  ["confirmMemory", ["m1", true], "confirm_memory", ["id", "confirmed"]],
  ["markDone", ["m1"], "mark_done", ["memoryId"]],
  [
    "listMemories",
    [{ entityId: null, status: null, query: null, limit: null }],
    "list_memories",
    ["filter"],
  ],
  ["getEntity", ["e1"], "get_entity", ["id"]],
  ["correctMemory", ["m1", "Fixed."], "correct_memory", ["id", "text"]],
  ["setKeepForever", ["m1", true], "set_keep_forever", ["id", "keepForever"]],
  ["setPrivate", ["m1", true], "set_private", ["id", "private"]],
  ["restore", [["m1"]], "restore", ["ids"]],
  ["previewForget", [["m1"], true], "preview_forget", ["ids", "includeOriginals"]],
  ["forget", ["tok"], "forget", ["previewToken"]],
  ["listNeedsYou", [], "list_needs_you", []],
  ["listFading", [], "list_fading", []],
  ["getPrivacy", [], "get_privacy", []],
  ["exportTo", ["out"], "export_to", ["dir"]],
  ["deleteEverything", ["phrase"], "delete_everything", ["confirmPhrase"]],
  ["getSettings", [], "get_settings", []],
  ["updateSettings", [patch], "update_settings", ["patch"]],
  ["pause", [null], "pause", ["until"]],
  ["resume", [], "resume", []],
  ["getAdvanced", [], "get_advanced", []],
  ["speakPage", ["2026-11-13"], "speak_page", ["date"]],
];

describe("TauriEngine commands", () => {
  for (const [method, args, command, keys] of table) {
    it(`${method} invokes ${command}${keys.length ? ` with ${keys.join(", ")}` : ""}`, async () => {
      await (engine[method] as (...a: unknown[]) => Promise<unknown>)(...args);
      expect(calls).toHaveLength(1);
      expect(calls[0]?.cmd).toBe(command);
      expect(Object.keys(calls[0]?.args ?? {}).sort()).toEqual([...keys].sort());
    });
  }

  it("covers every method of the client", () => {
    const covered = new Set(table.map(([m]) => m));
    const proto = Object.getOwnPropertyNames(engine).filter(
      (k) =>
        typeof (engine as unknown as Record<string, unknown>)[k] === "function" &&
        !["on", "dispose", "call"].includes(k),
    );
    for (const name of proto) expect(covered.has(name as keyof EngineClient), name).toBe(true);
  });

  it("passes null, not undefined, for the default page date", async () => {
    await engine.getPage();
    expect(calls[0]?.args).toEqual({ date: null });
  });
});

describe("TauriEngine events", () => {
  it("fans an event out to every listener and stops after unsubscribe", async () => {
    const a: number[] = [];
    const b: number[] = [];
    const offA = engine.on("needs-you-changed", (n) => a.push(n));
    engine.on("needs-you-changed", (n) => b.push(n));
    await emit("needs-you-changed", 2);
    offA();
    await emit("needs-you-changed", 1);
    expect(a).toEqual([2]);
    expect(b).toEqual([2, 1]);
  });

  it("does not lose an event that arrives before a screen listens", async () => {
    await emit("status-changed", { state: "reading" });
    const seen: unknown[] = [];
    engine.on("status-changed", (s) => seen.push(s));
    await emit("status-changed", { state: "resting" });
    expect(seen).toEqual([{ state: "resting" }]);
  });
});
