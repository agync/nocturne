import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { BackupProgress } from "../bindings";
import { createMockEngine } from "./engine";

const NOW = new Date("2026-11-13T07:02:00Z");
const PASS = "lantern orchard tuesday";
const FILE = "/backups/Nocturne backup 2026-11-13.nocturne";

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval"] });
});
afterEach(() => {
  vi.useRealTimers();
});

const engine = () => createMockEngine({ now: NOW });

describe("a mock backup", () => {
  it("walks saving then checking, with bytes, and records the last one", async () => {
    const mock = engine();
    const seen: BackupProgress[] = [];
    mock.on("backup-progress", (progress) => seen.push(progress));

    const run = mock.makeBackup(FILE, PASS);
    await vi.advanceTimersByTimeAsync(1000);
    const report = await run;

    expect(report).toMatchObject({ path: FILE, bytes: await mock.estimateBackup() });
    expect(report.memories).toBeGreaterThan(0);
    expect(report.pages).toBeGreaterThan(0);
    expect(seen.some((p) => p.phase === "saving")).toBe(true);
    expect(seen.some((p) => p.phase === "checking")).toBe(true);
    expect(seen.every((p) => p.total === report.bytes)).toBe(true);
    expect(seen.at(-1)).toMatchObject({ phase: "checking", done: report.bytes });
    expect((await mock.getBackups()).last).toEqual({
      at: NOW.toISOString(),
      bytes: report.bytes,
    });
    expect((await mock.getBackups()).running).toBeNull();
  });

  it("needs at least eight characters, before anything runs", async () => {
    const mock = engine();
    const seen: BackupProgress[] = [];
    mock.on("backup-progress", (progress) => seen.push(progress));
    await expect(mock.makeBackup(FILE, "short")).rejects.toMatchObject({ code: "invalid" });
    expect(seen).toEqual([]);
    expect((await mock.getBackups()).last).toBeNull();
  });

  it("stops at once when the person says so, and saves nothing", async () => {
    const mock = engine();
    const run = mock.makeBackup(FILE, PASS);
    await mock.cancelBackup();
    await expect(run).rejects.toMatchObject({ code: "cancelled" });
    expect((await mock.getBackups()).last).toBeNull();
    expect((await mock.getBackups()).running).toBeNull();
  });

  it("is busy while one runs", async () => {
    const mock = engine();
    const run = mock.makeBackup(FILE, PASS);
    await expect(mock.makeBackup("/backups/other.nocturne", PASS)).rejects.toMatchObject({
      code: "busy",
    });
    await vi.advanceTimersByTimeAsync(1000);
    await run;
  });

  it.each([
    ["noRoom", "noRoom"],
    ["cantWrite", "cantWrite"],
    ["notFound", "notFound"],
    ["damagedBackup", "damagedBackup"],
    ["other", "unknown"],
  ] as const)("fails the next one with %s", async (script, code) => {
    const mock = engine();
    mock.script.backup = { fail: script };
    const run = mock.makeBackup(FILE, PASS).catch((error: unknown) => error);
    await vi.advanceTimersByTimeAsync(1000);
    expect(await run).toMatchObject({ code });
    expect((await mock.getBackups()).last).toBeNull();
    expect((await mock.getBackups()).running).toBeNull();

    // The script was for one call only.
    const again = mock.makeBackup(FILE, PASS);
    await vi.advanceTimersByTimeAsync(1000);
    await expect(again).resolves.toMatchObject({ path: FILE });
  });

  it("shows a backup that was already running", async () => {
    const mock = engine();
    mock.script.backup.running = { phase: "saving", done: 400, total: 1000 };
    expect((await mock.getBackups()).running).toEqual({
      phase: "saving",
      done: 400,
      total: 1000,
    });
  });

  it("clears the notices about work that stopped unexpectedly, and a new backup does too", async () => {
    const mock = engine();
    mock.script.interruptedBackup = true;
    mock.script.interruptedRestore = true;
    expect(await mock.getBackups()).toMatchObject({
      interruptedBackup: true,
      interruptedRestore: true,
    });
    await mock.dismissBackupNotices();
    expect(await mock.getBackups()).toMatchObject({
      interruptedBackup: false,
      interruptedRestore: false,
    });

    mock.script.interruptedBackup = true;
    const run = mock.makeBackup(FILE, PASS);
    expect((await mock.getBackups()).interruptedBackup).toBe(false);
    await vi.advanceTimersByTimeAsync(1000);
    await run;
  });
});

describe("a mock restore", () => {
  it("refuses a passphrase of wrong passphrase, and a file that is not a backup", async () => {
    const mock = engine();
    await expect(mock.checkBackup(FILE, "wrong passphrase")).rejects.toMatchObject({
      code: "wrongPassphrase",
    });
    await expect(mock.checkBackup("/notes/words.txt", PASS)).rejects.toMatchObject({
      code: "notABackup",
    });
    await expect(mock.probeBackup("/notes/words.txt")).rejects.toMatchObject({
      code: "notABackup",
    });
    await expect(mock.probeBackup(FILE)).resolves.toBeUndefined();
    mock.script.probe = { fail: "damagedBackup" };
    await expect(mock.probeBackup(FILE)).rejects.toMatchObject({ code: "damagedBackup" });
  });

  it("can fail the next check as newer, damaged, missing or no room", async () => {
    const cases = [
      ["newerBackup", "newerBackup"],
      ["damagedBackup", "damagedBackup"],
      ["notFound", "notFound"],
      ["noRoom", "noRoom"],
      ["other", "unknown"],
    ] as const;
    for (const [script, code] of cases) {
      const mock = engine();
      mock.script.check = { fail: script };
      await expect(mock.checkBackup(FILE, PASS)).rejects.toMatchObject({ code });
    }
  });

  it("says what the backup holds and whether this computer is empty", async () => {
    const mock = engine();
    const check = await mock.checkBackup(FILE, PASS);
    expect(check.summary).toMatchObject({
      appVersion: "0.2.2",
      storeVersion: 1,
      onlineUsed: false,
      awayUsed: false,
    });
    expect(check.summary.bytes).toBe(await mock.estimateBackup());
    expect(check.here?.memories).toBeGreaterThan(0);
    expect(check.putAside).toBeNull();

    mock.script.here = null;
    expect((await mock.checkBackup(FILE, PASS)).here).toBeNull();
  });

  it("puts what was here aside when replacing, and keeps it for bringing back", async () => {
    const mock = engine();
    await expect(mock.restoreBackup(FILE, PASS, false)).rejects.toMatchObject({
      code: "invalid",
    });
    await mock.restoreBackup(FILE, PASS, true);
    const restored = await mock.getRestored();
    expect(restored?.putAsideAt).not.toBeNull();
    const copies = (await mock.getBackups()).putAside;
    expect(copies).toHaveLength(1);
    const id = copies[0]?.id as string;

    await mock.deletePutAside(id);
    expect((await mock.getBackups()).putAside).toEqual([]);
    await expect(mock.deletePutAside(id)).rejects.toMatchObject({ code: "notFound" });
  });

  it("restores from an empty computer without putting anything aside", async () => {
    const mock = engine();
    mock.script.here = null;
    await mock.restoreBackup(FILE, PASS, false);
    expect((await mock.getRestored())?.putAsideAt).toBeNull();
    expect((await mock.getBackups()).putAside).toEqual([]);
  });

  it("brings back a scripted copy, and forgets the restored view when told to", async () => {
    const mock = engine();
    mock.script.putAside = [
      {
        id: "2026-11-01 1010",
        at: "2026-11-01T10:10:00.000Z",
        memories: 12,
        pages: 3,
        originals: 4,
        bytes: 2_000_000,
      },
    ];
    await mock.restorePutAside("2026-11-01 1010");
    expect(await mock.getRestored()).toMatchObject({
      memories: 12,
      pages: 3,
      originals: 4,
    });
    await expect(mock.restorePutAside("no such copy")).rejects.toMatchObject({ code: "notFound" });
    await mock.finishRestore();
    expect(await mock.getRestored()).toBeNull();
  });

  it("can be scripted with a restored view, and a restore that fails", async () => {
    const mock = engine();
    mock.script.restored = {
      madeAt: "2026-11-02T10:00:00.000Z",
      memories: 5,
      pages: 2,
      originals: 1,
      onlineUsed: true,
      awayUsed: false,
      putAsideAt: null,
    };
    expect((await mock.getRestored())?.memories).toBe(5);

    mock.script.restore = { fail: "damagedBackup" };
    await expect(mock.restoreBackup(FILE, PASS, true)).rejects.toMatchObject({
      code: "damagedBackup",
    });
  });
});
