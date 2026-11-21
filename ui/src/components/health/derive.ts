// What the charts need that the engine does not hand over ready: nights built from a night's
// readings, and the positions of times on the night track. The usual comes from the engine.

import type { ReadingRow, TimeSpan } from "../../engine/bindings";

/** One night as the strip and the night bar draw it. */
export interface Night {
  /** The day the night ended on. */
  day: string;
  periods: TimeSpan[];
  asleepMin: number;
  /** The item of the night's own page. */
  itemId: string;
  /** A journal entry was written that day. */
  hasEntry: boolean;
  from: string;
  to: string;
  awakeCount: number;
  removed: boolean;
}

/** The nights of a list of sleep readings, newest last, leaving out what was removed in Health. */
export function nightsOf(rows: readonly ReadingRow[], journalDays: readonly string[]): Night[] {
  const nights: Night[] = [];
  for (const row of rows) {
    if (row.removed || row.value.type !== "sleep") continue;
    const periods = [...row.value.asleep].sort((a, b) => a.from.localeCompare(b.from));
    const first = periods[0];
    const last = periods[periods.length - 1];
    if (!first || !last) continue;
    const asleepMin = periods.reduce(
      (sum, p) => sum + (new Date(p.to).getTime() - new Date(p.from).getTime()) / 60_000,
      0,
    );
    nights.push({
      day: row.day,
      periods,
      asleepMin,
      itemId: row.itemId,
      hasEntry: journalDays.includes(row.day),
      from: first.from,
      to: last.to,
      awakeCount: periods.length - 1,
      removed: row.removed,
    });
  }
  return nights.sort((a, b) => a.day.localeCompare(b.day));
}

/** Hours after 8 pm, so a night runs left to right from 0 to 16. */
export function nightHours(instant: string | Date): number {
  const d = typeof instant === "string" ? new Date(instant) : instant;
  const v = d.getHours() + d.getMinutes() / 60 - 20;
  return v < 0 ? v + 24 : v;
}

/** The same, from a time of day `HH:MM`. */
export function clockHours(hhmm: string): number {
  const [h = 0, m = 0] = hhmm.split(":").map(Number);
  const v = h + m / 60 - 20;
  return v < 0 ? v + 24 : v;
}

/** Whole days from `from` to `day`, both `YYYY-MM-DD`. */
export function dayIndex(from: string, day: string): number {
  const a = Date.UTC(+from.slice(0, 4), +from.slice(5, 7) - 1, +from.slice(8, 10));
  const b = Date.UTC(+day.slice(0, 4), +day.slice(5, 7) - 1, +day.slice(8, 10));
  return Math.round((b - a) / 86_400_000);
}
