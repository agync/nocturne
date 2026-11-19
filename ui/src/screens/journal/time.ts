import { addDays, isDay, localDay } from "../../copy/format";

const CLOCK = new Intl.DateTimeFormat("en-GB", {
  hour: "numeric",
  minute: "2-digit",
  hour12: true,
});
const WEEKDAY = new Intl.DateTimeFormat("en-GB", { weekday: "long" });
const MONTH = new Intl.DateTimeFormat("en-GB", { month: "long" });

/** "6:10 pm": the time as the journal writes it, in words and numbers. */
export function clock12(input: string | Date): string {
  const d = new Date(input);
  if (Number.isNaN(d.getTime())) return "";
  return CLOCK.format(d).replace(/[  ]/g, " ").toLowerCase();
}

/** A `YYYY-MM-DD` journal day at local noon. */
export function dayDate(day: string): Date {
  const [y = 1970, m = 1, d = 1] = day.split("-").map(Number);
  return new Date(y, m - 1, d, 12);
}

export function weekdayOf(day: string): string {
  return WEEKDAY.format(dayDate(day));
}

/** The parts of "Thursday 19 November". */
export function dayWords(day: string): { weekday: string; day: number; month: string } {
  const d = dayDate(day);
  return { weekday: WEEKDAY.format(d), day: d.getDate(), month: MONTH.format(d) };
}

/** The journal day it is now: a day runs from 4 in the morning to 4 the next morning. */
export function journalDayNow(now: Date = new Date()): string {
  return localDay(new Date(now.getTime() - 4 * 3_600_000));
}

export { addDays, isDay };
