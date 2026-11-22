import { afterEach, describe, expect, it } from "vitest";
import {
  addDays,
  formatBytes,
  formatDay,
  formatDayMonth,
  formatDuration,
  formatDurationSpoken,
  formatEta,
  formatLongDay,
  formatTime,
  formatTimeOfDay,
  isAfterMidnight,
  isDay,
  journalDay,
  localDay,
  setHourClock,
} from "./format";

describe("format", () => {
  it("formats days the way the design writes them", () => {
    expect(formatDay("2026-11-11")).toBe("Wed 11 Nov");
    expect(formatLongDay("2026-11-13")).toBe("Friday 13 November");
  });

  it("formats a day and month with no weekday, spelling September Sep", () => {
    expect(formatDayMonth("2026-11-13")).toBe("13 Nov");
    expect(formatDayMonth("2026-09-07T15:00:00Z")).toBe("7 Sep");
    expect(formatDayMonth("nonsense")).toBe("");
  });

  it("reads the stamp that names a copy put aside, with or without its counter", () => {
    expect(formatDayMonth("2026-11-15 120000")).toBe("15 Nov");
    expect(formatDayMonth("2026-09-07 235959 (2)")).toBe("7 Sep");
  });

  it("reads a bare day as a calendar day regardless of zone", () => {
    expect(formatDay("2026-03-29")).toBe("Sun 29 Mar");
  });

  it("formats times in 24 hours in the person’s zone", () => {
    expect(formatTime("2026-11-13T07:02:00Z")).toBe("07:02");
    expect(formatTime("2026-07-01T06:30:00Z")).toBe("07:30");
  });

  it("formats durations", () => {
    expect(formatDuration(12_000)).toBe("0:12");
    expect(formatDuration(65_000)).toBe("1:05");
    expect(formatDuration(3_725_000)).toBe("1:02:05");
    expect(formatDurationSpoken(10_000)).toBe("10 seconds");
    expect(formatDurationSpoken(65_000)).toBe("1 minute 5 seconds");
  });

  it("formats estimates and sizes", () => {
    expect(formatEta(40)).toBe("about 40 seconds");
    expect(formatEta(180)).toBe("about 3 min");
    expect(formatBytes(84_000_000)).toBe("84\u00a0MB");
    expect(formatBytes(4_100_000_000)).toBe("4.1\u00a0GB");
  });

  it("keeps a number with its unit", () => {
    expect(formatBytes(9_000_000_000)).toMatch(/^9\u00a0GB$/);
  });

  it("moves days across a clock change", () => {
    expect(addDays("2026-03-28", 1)).toBe("2026-03-29");
    expect(addDays("2026-03-29", 1)).toBe("2026-03-30");
    expect(localDay("2026-11-13T07:02:00Z")).toBe("2026-11-13");
  });

  it("gives an empty string, never an error, for something that is not a date", () => {
    for (const bad of ["not-a-date", "", "2026-13-45", "2026-02-31"]) {
      expect(formatDay(bad), bad).toBe("");
      expect(formatLongDay(bad), bad).toBe("");
      expect(formatTime(bad), bad).toBe("");
      expect(localDay(bad), bad).toBe("");
      expect(addDays(bad, 1), bad).toBe("");
    }
  });

  it("knows a real calendar day from an impossible one", () => {
    expect(isDay("2026-11-13")).toBe(true);
    expect(isDay("2026-02-29")).toBe(false);
    expect(isDay("not-a-date")).toBe(false);
  });
});

describe("the journal day on a phone alone", () => {
  it("runs from 4 in the morning to 4 the next morning", () => {
    expect(journalDay(new Date(2026, 10, 23, 0, 8))).toBe("2026-11-22");
    expect(journalDay(new Date(2026, 10, 23, 3, 59))).toBe("2026-11-22");
    expect(journalDay(new Date(2026, 10, 23, 4, 0))).toBe("2026-11-23");
    expect(journalDay(new Date(2026, 10, 23, 23, 59))).toBe("2026-11-23");
    expect(journalDay(new Date(2026, 0, 1, 1, 0))).toBe("2025-12-31");
  });

  it("is after midnight only before 4 am", () => {
    expect(isAfterMidnight(new Date(2026, 10, 23, 0, 0))).toBe(true);
    expect(isAfterMidnight(new Date(2026, 10, 23, 3, 59))).toBe(true);
    expect(isAfterMidnight(new Date(2026, 10, 23, 4, 0))).toBe(false);
    expect(isAfterMidnight("nonsense")).toBe(false);
  });
});

describe("the time on the phone's screens", () => {
  const evening = new Date(2026, 10, 23, 18, 10);
  const small = new Date(2026, 10, 23, 0, 9);

  afterEach(() => setHourClock(null));

  it("reads in twelve hours until the phone says otherwise", () => {
    expect(formatTimeOfDay(evening)).toBe("6:10 pm");
    expect(formatTimeOfDay(small)).toBe("12:09 am");
  });

  it("follows the phone's 24-hour setting, and its 12-hour one", () => {
    setHourClock(true);
    expect(formatTimeOfDay(evening)).toBe("18:10");
    expect(formatTimeOfDay(small)).toBe("00:09");
    setHourClock(false);
    expect(formatTimeOfDay(evening)).toBe("6:10 pm");
  });

  it("is empty for what is not a time", () => {
    expect(formatTimeOfDay("nonsense")).toBe("");
  });
});

describe("a time before the phone says how its clock is set", () => {
  const evening = new Date(2026, 10, 23, 18, 10);

  it("reads as it always did", () => {
    expect(formatTimeOfDay(evening)).toBe("6:10 pm");
    expect(formatTimeOfDay(evening, "locale")).toBe("18:10");
  });
});
