import { t } from "./t";

const LOCALE = "en-GB";

/** A `YYYY-MM-DD` day or an instant. A bare day is a calendar day, so it is read at local noon. */
function toDate(input: string | Date): Date {
  if (input instanceof Date) return input;
  const day = /^(\d{4})-(\d{2})-(\d{2})$/.exec(input);
  if (day) {
    return isDay(input)
      ? new Date(Number(day[1]), Number(day[2]) - 1, Number(day[3]), 12)
      : new Date(Number.NaN);
  }
  // A copy put aside is named by the local time it was made: "2026-11-15 120000", and
  // " (2)" after it when two share a second.
  const stamp = /^(\d{4})-(\d{2})-(\d{2}) (\d{2})(\d{2})(\d{2})(?: \(\d+\))?$/.exec(input);
  if (stamp) {
    const [y = 0, mo = 1, d = 1, h = 0, mi = 0, sec = 0] = stamp.slice(1).map(Number);
    return new Date(y, mo - 1, d, h, mi, sec);
  }
  return new Date(input);
}

const clean = (s: string) => s.replace(/,/g, "");

const valid = (d: Date) => !Number.isNaN(d.getTime());

/** True for a real `YYYY-MM-DD` calendar day. */
export function isDay(input: string): boolean {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(input);
  if (!m) return false;
  const [y, mo, d] = [Number(m[1]), Number(m[2]), Number(m[3])];
  const date = new Date(y, mo - 1, d, 12);
  return date.getFullYear() === y && date.getMonth() === mo - 1 && date.getDate() === d;
}

/** Formats a date, or gives "" for one that is not a date, so a bad value never breaks a screen. */
function safely(input: string | Date, format: (d: Date) => string): string {
  const d = toDate(input);
  return valid(d) ? format(d) : "";
}

const fmt = (options: Intl.DateTimeFormatOptions) => new Intl.DateTimeFormat(LOCALE, options);

/** "Tue 11 Nov" */
export function formatDay(input: string | Date): string {
  return safely(input, (d) =>
    clean(fmt({ weekday: "short", day: "numeric", month: "short" }).format(d)),
  );
}

/** "13 Nov": a day and month with no weekday, for lines that sit beside other dates. */
export function formatDayMonth(input: string | Date): string {
  // Some locale data spells September "Sept"; the design and the other dates use "Sep".
  return safely(input, (d) =>
    fmt({ day: "numeric", month: "short" }).format(d).replace("Sept", "Sep"),
  );
}

/** "Thursday 13 November" */
export function formatLongDay(input: string | Date): string {
  return safely(input, (d) =>
    clean(fmt({ weekday: "long", day: "numeric", month: "long" }).format(d)),
  );
}

/** "Thursday 20 Nov, 10:00" style helper for a day plus time, used in spoken receipts. */
export function formatSpokenDay(input: string | Date): string {
  return safely(input, (d) =>
    clean(fmt({ weekday: "long", day: "numeric", month: "long" }).format(d)),
  );
}

/** "Thu" */
export function formatWeekday(input: string | Date): string {
  return safely(input, (d) => fmt({ weekday: "short" }).format(d));
}

/** "MON" source for the upcoming rows; styling uppercases it. */
export const formatWeekdayShort = formatWeekday;

/** "August" */
export function formatMonth(input: string | Date): string {
  return safely(input, (d) => fmt({ month: "long" }).format(d));
}

/** "07:00", 24-hour, in the person's time zone. */
export function formatTime(input: string | Date): string {
  return safely(input, (d) => fmt({ hour: "2-digit", minute: "2-digit", hour12: false }).format(d));
}

/** "6:10 pm": twelve hours, in words and numbers, as the journal writes times. */
export function formatTime12(input: string | Date): string {
  return safely(input, (d) =>
    fmt({ hour: "numeric", minute: "2-digit", hour12: true })
      .format(d)
      .replace(/[\u00a0\u202f]/g, " ")
      .toLowerCase(),
  );
}

let hour24: boolean | null = null;

/**
 * The phone's own 12 or 24-hour setting, once it says: every time on the phone's screens follows
 * it. Until then, and wherever the phone does not say, a time reads as it always did.
 */
export function setHourClock(is24: boolean | null): void {
  hour24 = is24;
}

/** Whether the language's own clock reads in 24 hours: en-GB does, en-US does not. */
function localeIs24(): boolean {
  return new Intl.DateTimeFormat(LOCALE, { hour: "numeric" }).resolvedOptions().hour12 === false;
}

/**
 * "6:10 pm" or "18:10", as the phone's clock is set. Until the phone says, a time reads as it
 * did before the setting was read: `unknown` is `"12"` for the screens that wrote twelve hours,
 * `"locale"` for the ones that followed the language.
 */
export function formatTimeOfDay(input: string | Date, unknown: "12" | "locale" = "12"): string {
  const is24 = hour24 ?? (unknown === "locale" ? localeIs24() : false);
  return is24 ? formatTime(input) : formatTime12(input);
}

/** `HH:MM` text kept as typed. */
export function formatClock(hhmm: string): string {
  return hhmm;
}

/** "0:12" or "1:04:09" */
export function formatDuration(ms: number): string {
  const total = Math.max(0, Math.round(ms / 1000));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const ss = String(s).padStart(2, "0");
  if (h > 0) return `${h}:${String(m).padStart(2, "0")}:${ss}`;
  return `${m}:${ss}`;
}

/** "1 minute 5 seconds" for screen readers. */
export function formatDurationSpoken(ms: number): string {
  const total = Math.max(0, Math.round(ms / 1000));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const parts: string[] = [];
  if (h > 0) parts.push(t("duration.hours", { count: h }));
  if (m > 0) parts.push(t("duration.minutes", { count: m }));
  if (s > 0 || parts.length === 0) parts.push(t("duration.seconds", { count: s }));
  return parts.join(" ");
}

/** "about 3 min" or "about 40 seconds" for a live estimate. */
export function formatEta(seconds: number): string {
  if (seconds < 90) return t("duration.about.seconds", { count: Math.max(1, Math.round(seconds)) });
  return t("duration.about.minutes", { count: Math.max(1, Math.round(seconds / 60)) });
}

/** "84 MB", "4.1 GB". Decimal units, as disk sizes are shown by the system. A no-break space keeps a number with its unit. */
export function formatBytes(bytes: number): string {
  const units = ["B", "kB", "MB", "GB", "TB"];
  let value = bytes;
  let i = 0;
  while (value >= 1000 && i < units.length - 1) {
    value /= 1000;
    i += 1;
  }
  const digits = value >= 100 || i === 0 ? 0 : 1;
  const n = new Intl.NumberFormat(LOCALE, { maximumFractionDigits: digits }).format(value);
  return `${n}\u00a0${units[i]}`;
}

/** `YYYY-MM-DD` of an instant in the person's time zone. */
export function localDay(input: string | Date): string {
  const d = toDate(input);
  if (!valid(d)) return "";
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  return `${y}-${m}-${day}`;
}

/** The hour a journal day starts at on a phone alone, which has no page time to follow. */
export const DAY_STARTS_AT = 4;

/** `YYYY-MM-DD` of the journal day an instant belongs to: half past midnight is still the evening. */
export function journalDay(input: string | Date): string {
  const d = toDate(input);
  if (!valid(d)) return "";
  return localDay(
    new Date(
      d.getFullYear(),
      d.getMonth(),
      d.getDate(),
      d.getHours() - DAY_STARTS_AT,
      d.getMinutes(),
    ),
  );
}

/** Whether an instant falls between midnight and the hour the journal day starts. */
export function isAfterMidnight(input: string | Date): boolean {
  const d = toDate(input);
  return valid(d) && d.getHours() < DAY_STARTS_AT;
}

/** Shifts a `YYYY-MM-DD` day by whole days. */
export function addDays(day: string, delta: number): string {
  const d = toDate(day);
  if (!valid(d)) return "";
  d.setDate(d.getDate() + delta);
  return localDay(d);
}
