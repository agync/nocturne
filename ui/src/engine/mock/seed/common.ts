import { addDays, localDay } from "../../../copy/format";
import type { ItemView, PageLine, Receipt, Span } from "../../bindings";

export const DEFAULT_NOW = "2026-11-13T07:02:00Z";

/** Predicates that make a memory a safety memory: kept forever, never the daily question. */
export const SAFETY_PREDICATES: ReadonlySet<string> = new Set([
  "takes",
  "allergicTo",
  "allergy",
  "medication",
  "diagnosedWith",
  "condition",
]);

const longWeekday = (day: string) =>
  new Intl.DateTimeFormat("en-GB", { weekday: "long", day: "numeric", month: "short" })
    .format(new Date(`${day}T12:00:00`))
    .replace(/,/g, "");

/** The day offset to the next given weekday (0 = Sunday), at least one day ahead. */
function nextWeekday(today: string, weekday: number): number {
  const current = new Date(`${today}T12:00:00`).getDay();
  const delta = (weekday - current + 7) % 7;
  return delta === 0 ? 7 : delta;
}

/** Everything the area seeds share: the clock, and the days the household's week is built from. */
export interface BaseContext {
  today: string;
  /** An instant `offset` days from today at `hh:mm`, as an RFC 3339 string. */
  at: (offset: number, hh: number, mm?: number) => string;
  tuesday: string;
  thursday: string;
  friday: string;
  tuesdayText: string;
  thursdayText: string;
  nextMonday: string;
}

export interface SeedContext extends BaseContext {
  receipt: (itemId: string, span?: Span | null, label?: string) => Receipt;
}

export function makeBase(now: Date): BaseContext {
  const today = localDay(now);
  const at = (offset: number, hh: number, mm = 0): string => {
    const day = addDays(today, offset);
    const [y, m, d] = day.split("-").map(Number) as [number, number, number];
    return new Date(y, m - 1, d, hh, mm).toISOString();
  };
  const tuesday = addDays(today, nextWeekday(today, 2));
  const thursday = addDays(today, nextWeekday(today, 4));
  const friday = addDays(today, nextWeekday(today, 5));
  return {
    today,
    at,
    tuesday,
    thursday,
    friday,
    tuesdayText: `${longWeekday(tuesday)}, 14:00`,
    thursdayText: `${longWeekday(thursday)}, 10:00`,
    nextMonday: addDays(today, nextWeekday(today, 1)),
  };
}

/** Receipts that point at a real item, so every claim in the seed resolves. */
export function makeReceipt(items: ItemView[]): SeedContext["receipt"] {
  const byId = new Map(items.map((i) => [i.id, i]));
  return (itemId, span = null, label) => {
    const found = byId.get(itemId);
    if (!found) throw new Error(`Seed receipt points at a missing item: ${itemId}`);
    return {
      itemId,
      kind: found.kind,
      source: found.source,
      label: label ?? found.title,
      keptAt: found.keptAt,
      durationMs: found.durationMs,
      span,
      unconfirmed: found.status === "unconfirmed",
    };
  };
}

export function line(
  id: string,
  kind: PageLine["kind"],
  text: string,
  memoryIds: string[],
  receipts: Receipt[],
  extra: Partial<PageLine> = {},
): PageLine {
  return {
    id,
    kind,
    text,
    memoryIds,
    receipts,
    choices: [],
    conflictId: null,
    subject: null,
    done: false,
    forgotten: false,
    on: null,
    lastUsed: null,
    ...extra,
  };
}
