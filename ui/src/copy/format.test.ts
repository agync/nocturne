import { describe, expect, it } from "vitest";
import {
  addDays,
  formatBytes,
  formatDay,
  formatDuration,
  formatDurationSpoken,
  formatEta,
  formatLongDay,
  formatTime,
  localDay,
} from "./format";

describe("format", () => {
  it("formats days the way the design writes them", () => {
    expect(formatDay("2026-11-11")).toBe("Wed 11 Nov");
    expect(formatLongDay("2026-11-13")).toBe("Friday 13 November");
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
    expect(formatBytes(84_000_000)).toBe("84 MB");
    expect(formatBytes(4_100_000_000)).toBe("4.1 GB");
  });

  it("moves days across a clock change", () => {
    expect(addDays("2026-03-28", 1)).toBe("2026-03-29");
    expect(addDays("2026-03-29", 1)).toBe("2026-03-30");
    expect(localDay("2026-11-13T07:02:00Z")).toBe("2026-11-13");
  });
});
