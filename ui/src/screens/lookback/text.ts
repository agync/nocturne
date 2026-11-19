import { formatTime12, formatWeekday, localDay } from "../../copy/format";
import { t } from "../../copy/t";
import type { CheckIn, FeelingPart, NameRef, Receipt } from "../../engine/bindings";
import { PARTS } from "../../journal/feelings";

const LOCALE = "en-GB";

const toDate = (input: string | Date): Date => {
  if (input instanceof Date) return input;
  const day = /^(\d{4})-(\d{2})-(\d{2})$/.exec(input);
  return day ? new Date(Number(day[1]), Number(day[2]) - 1, Number(day[3]), 12) : new Date(input);
};

const safely = (input: string | Date, format: (d: Date) => string): string => {
  const d = toDate(input);
  return Number.isNaN(d.getTime()) ? "" : format(d);
};

/** "3:15 pm", twelve hours, as the design writes times. */
export function clock(input: string | Date): string {
  return formatTime12(input);
}

/** The pieces of a day's name, for "{weekday} {day} {month}". */
export function dayParts(input: string | Date): { weekday: string; day: number; month: string } {
  const d = toDate(input);
  return {
    weekday: new Intl.DateTimeFormat(LOCALE, { weekday: "long" }).format(d),
    day: d.getDate(),
    month: new Intl.DateTimeFormat(LOCALE, { month: "long" }).format(d),
  };
}

export const monthName = (input: string | Date, style: "long" | "short" = "long") =>
  safely(input, (d) =>
    new Intl.DateTimeFormat(LOCALE, { month: style }).format(d).replace("Sept", "Sep"),
  );

/** "Tue 10", a short weekday and the day of the month. */
export function weekdayDay(input: string | Date): string {
  return safely(input, (d) => `${formatWeekday(d)} ${d.getDate()}`);
}

/** "a, b and c" for words that go in one sentence. */
export function listWords(words: readonly string[]): string {
  return new Intl.ListFormat(LOCALE, { style: "long", type: "conjunction" }).format(words);
}

export const partName = (part: FeelingPart): string => t(PARTS[part].name);

/** The words a check-in carries, or its part's name when it has none. */
export function wordsOf(checkin: CheckIn): string {
  return checkin.feeling.words.length > 0
    ? listWords(checkin.feeling.words)
    : partName(checkin.feeling.part).toLowerCase();
}

export function checkinLabel(checkin: CheckIn): string {
  return t("journal.lookback.felt.dot", {
    part: partName(checkin.feeling.part),
    words: wordsOf(checkin),
    time: clock(checkin.keptAt),
  });
}

/** Where an item opens. */
export const itemHref = (itemId: string): string => `#/items/${itemId}`;
export const entityHref = (entityId: string): string => `#/memories/${entityId}`;
export const dayHref = (day: string): string => `#/journal/${day}`;

export const receiptKey = (r: Receipt): string => `${r.itemId}:${JSON.stringify(r.span)}`;

export const isPlace = (n: NameRef): boolean => n.kind !== "person";

export const dayOf = (iso: string): string => localDay(iso);
