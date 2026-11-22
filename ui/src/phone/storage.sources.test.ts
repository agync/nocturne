import { describe, expect, it } from "vitest";

const sources = import.meta.glob(
  ["./**/*.ts", "./**/*.tsx", "!./**/*.test.ts", "!./**/*.test.tsx", "!./testing/**"],
  { query: "?raw", import: "default", eager: true },
) as Record<string, string>;

const using = (needle: RegExp) =>
  Object.entries(sources)
    .filter(([, text]) =>
      needle.test(text.replace(/\/\/.*$/gm, "").replace(/\/\*[\s\S]*?\*\//g, "")),
    )
    .map(([file]) => file)
    .sort();

describe("what the phone app keeps and where", () => {
  it("keys.no_session_material_stored: nothing is shared between tabs, and only one file touches session storage", () => {
    expect(using(/localStorage/)).toEqual([]);
    expect(using(/BroadcastChannel|SharedWorker|postMessage/)).toEqual(["./shell.ts"]);
    // The only thing session storage holds is the version a reload was already tried for.
    expect(using(/sessionStorage/)).toEqual(["./shell.ts"]);
  });

  it("IndexedDB is reached by the key store, the queue and their helper only", () => {
    expect(using(/indexedDB/)).toEqual(["./idb.ts", "./support.ts"]);
    expect(using(/openDb\(/)).toEqual([
      "./alone/reminder.ts",
      "./idb.ts",
      "./keys.ts",
      "./queue.ts",
    ]);
  });

  it("no secret or session value is written to a log", () => {
    expect(using(/console\.(log|info|debug|warn|error)/)).toEqual([]);
  });

  it("the shipped app holds no scripted or test code", () => {
    for (const [file, text] of Object.entries(sources)) {
      // The scripted engine is only ever imported by the scripted build; the end-to-end bundle
      // test holds the shipped bundle to that.
      if (file === "./services.ts" || file === "./mock.ts") continue;
      expect(text, file).not.toMatch(/fakeComputer|mockServices|createMockPhoneEngine/);
    }
  });
});
