import { describe, expect, it } from "vitest";
import type { MindReading, MindStatus } from "./mind";
import { readBlock, readWords, rowState } from "./readNow";

const fine: MindStatus = { state: "fine", host: "answers.example" };
const battery = (r: Partial<MindReading> = {}): MindReading => ({
  done: 0,
  count: 89,
  charging: false,
  percent: 80,
  saver: false,
  now: null,
  stopped: null,
  ...r,
});

describe("reading on battery", () => {
  it("says it waits to charge, why, and offers Read now", () => {
    expect(rowState(fine, battery())).toBe("Waiting to charge · 89 to read");
    const android = readBlock(fine, battery(), false);
    expect(android?.lines).toEqual([
      "Nocturne reads what you keep while this phone charges, so it doesn’t run the battery down.",
      "Read now reads straight away and uses some battery. It carries on if you leave Nocturne, with a notification to stop it.",
    ]);
    expect(android?.button).toEqual({ action: "readNow", label: "Read now" });
    expect(readBlock(fine, battery(), true)?.lines[1]).toBe(
      "Read now reads straight away and uses some battery. iPhone may pause it when you leave Nocturne; it carries on when you come back.",
    );
  });

  it("keeps today's words on the charger and says nothing more", () => {
    const charging = battery({ charging: true, done: 12 });
    expect(rowState(fine, charging)).toBe("Reading · 12 of 89");
    expect(readBlock(fine, charging, false)).toBeNull();
    expect(
      readBlock(fine, { ...charging, stopped: { why: "charging", done: 12 } }, false)?.said,
    ).toBe("Charging, so reading carries on by itself.");
  });

  it("shows reading now with its progress and Stop", () => {
    const now = battery({ now: "now", done: 12 });
    expect(rowState(fine, now)).toBe("Reading now · 12 of 89");
    const block = readBlock(fine, now, false);
    expect(block?.progress).toEqual({ done: 12, count: 89 });
    expect(block?.button).toEqual({ action: "stop", label: "Stop" });
    expect(block?.lines).toEqual([
      "You can leave Nocturne. Reading carries on, and the notification shows how far it has got.",
    ]);
    expect(readBlock(fine, { ...now, quiet: true }, false)?.lines[1]).toBe(
      "Notifications are off for Nocturne, so its progress shows here.",
    );
    expect(readBlock(fine, { ...now, away: 12 }, true)?.said).toBe(
      "iPhone paused reading while you were away, after 12. It carries on now.",
    );
    expect(readBlock(fine, { ...now, now: "anyway" }, false)?.said).toBe(
      "Reading anyway, down to 5% battery.",
    );
  });

  it("stops at 20 per cent and with battery saving, and offers Read anyway", () => {
    const low = battery({ done: 31, percent: 20, stopped: { why: "low", done: 31 } });
    expect(rowState(fine, low)).toBe("Waiting to charge · 58 to read");
    expect(readBlock(fine, low, false)).toEqual({
      said: "Read now stopped at 20% battery, after 31. The rest is read while this phone charges.",
      lines: ["Read anyway reads until everything is read or you stop it, down to 5% battery."],
      button: { action: "anyway", label: "Read anyway" },
    });
    const saver = battery({ saver: true, stopped: { why: "saver", done: 4 } });
    expect(readBlock(fine, saver, false)?.said).toBe(
      "Read now stopped because Battery Saver is on, after 4.",
    );
    expect(readBlock(fine, saver, true)?.said).toBe(
      "Read now stopped because Low Power Mode is on, after 4.",
    );
    expect(readBlock(fine, battery({ percent: 15 }), false)?.said).toBe("The battery is at 15%.");
    expect(readBlock(fine, battery({ saver: true }), true)?.said).toBe("Low Power Mode is on.");
  });

  it("stops at 5 per cent with no button", () => {
    const floor = battery({ done: 40, percent: 5, stopped: { why: "floor", done: 40 } });
    expect(readBlock(fine, floor, false)).toEqual({
      said: "Reading stopped at 5% battery, after 40. The rest is read while this phone charges.",
      lines: ["Read now works above 5% battery, or while charging."],
    });
  });

  it("says when everything is read", () => {
    const done = battery({ done: 89, stopped: { why: "done", done: 89 } });
    expect(rowState(fine, done)).toBeNull();
    expect(readBlock(fine, done, false)).toEqual({
      said: "Everything you kept is read.",
      tick: true,
      lines: [],
    });
  });

  it("says the limit, the service and the network in their own words", () => {
    const paused: MindStatus = { state: "paused", host: "answers.example", until: "2026-12-01" };
    expect(rowState(paused, battery({ done: 12 }))).toBe("Paused by your limit · 77 to read");
    expect(readBlock(paused, battery(), false)?.button?.action).toBe("changeLimit");
    const away: MindStatus = {
      state: "unreachable",
      host: "answers.example",
      since: "2026-11-24T09:12:00Z",
    };
    expect(rowState(away, battery({ done: 12 }))).toBe("Not answering · 77 to read");
    expect(readBlock(away, battery(), false)?.lines).toEqual([
      "Read now looks once more, then reads straight away, and uses some battery.",
    ]);
    expect(readBlock(away, battery({ stillNot: true }), false)?.said).toBe(
      "answers.example still isn’t answering. Nocturne looks again on its own.",
    );
    const offline: MindStatus = { state: "offline", host: "answers.example" };
    expect(rowState(offline, battery())).toBe("Not online · 89 to read");
    expect(readBlock(offline, battery(), false)?.button?.needs).toBe(
      "Read now works once this phone is online.",
    );
    expect(rowState({ state: "keyRefused", host: "answers.example" }, battery())).toBe(
      "Stopped · 89 to read",
    );
  });

  it("gives the phone words with counts only", () => {
    const words = readWords(false);
    expect(words.progress).toBe("{done} of {count}");
    expect(words.saverTitle).toBe("Read now stopped: Battery Saver is on");
    expect(readWords(true).saverTitle).toBe("Read now stopped: Low Power Mode is on");
    expect(words.pausedText).toBe(
      "iPhone paused it when you left Nocturne, after {done} of {count}. Open Nocturne to carry on.",
    );
    expect(JSON.stringify(words)).not.toContain("answers.example");
  });
});
