import { addDays, isDay } from "../../copy/format";
import type { Period, PeriodKind } from "../../engine/bindings";

const KINDS: readonly PeriodKind[] = ["day", "week", "month", "year"];

export function isKind(value: string | undefined): value is PeriodKind {
  return KINDS.includes(value as PeriodKind);
}

const pad = (n: number) => String(n).padStart(2, "0");

/** The period a look-back address names, or null when its kind or start is not real. */
export function periodFor(kind: string | undefined, start: string | undefined): Period | null {
  if (!isKind(kind) || !start || !isDay(start)) return null;
  const [y = 0, m = 1] = start.split("-").map(Number);
  switch (kind) {
    case "day":
      return { kind, start, end: start };
    case "week":
      return { kind, start, end: addDays(start, 6) };
    case "month":
      return { kind, start, end: `${y}-${pad(m)}-${pad(new Date(y, m, 0).getDate())}` };
    case "year":
      return { kind, start, end: `${y}-12-31` };
  }
}

/** The path of a look-back screen. */
export function lookbackPath(period: Period): string {
  return `/journal/look-back/${period.kind}/${period.start}`;
}
