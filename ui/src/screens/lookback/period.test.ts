import { describe, expect, it } from "vitest";
import { lookbackPath, periodFor } from "./period";

describe("the period a look-back address names", () => {
  it("ends a day on itself and a week on the Sunday after a Monday", () => {
    expect(periodFor("day", "2026-11-18")).toEqual({
      kind: "day",
      start: "2026-11-18",
      end: "2026-11-18",
    });
    expect(periodFor("week", "2026-11-09")).toEqual({
      kind: "week",
      start: "2026-11-09",
      end: "2026-11-15",
    });
  });

  it("ends a month on its last day, leap years included", () => {
    expect(periodFor("month", "2026-11-01")?.end).toBe("2026-11-30");
    expect(periodFor("month", "2028-02-01")?.end).toBe("2028-02-29");
    expect(periodFor("month", "2027-02-01")?.end).toBe("2027-02-28");
  });

  it("ends a year on 31 December", () => {
    expect(periodFor("year", "2025-01-01")?.end).toBe("2025-12-31");
  });

  it("gives nothing for a kind or day that is not real", () => {
    expect(periodFor("decade", "2026-11-01")).toBeNull();
    expect(periodFor("day", "2026-13-01")).toBeNull();
    expect(periodFor("day", undefined)).toBeNull();
  });

  it("builds the path back", () => {
    expect(lookbackPath({ kind: "week", start: "2026-11-09", end: "2026-11-15" })).toBe(
      "/journal/look-back/week/2026-11-09",
    );
  });
});
