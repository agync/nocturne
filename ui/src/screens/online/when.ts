import { addDays, formatDayMonth, formatMonth, formatTime, localDay } from "../../copy/format";
import { t } from "../../copy/t";

/** "Today, 09:14", "Yesterday, 21:40" or "2 Nov, 18:40". */
export function whenText(at: string, now: number = Date.now()): string {
  const time = formatTime(at);
  const day = localDay(at);
  const today = localDay(new Date(now));
  if (day === today) return t("online.when.today", { time });
  if (day === addDays(today, -1)) return t("online.when.yesterday", { time });
  return t("online.when.other", { day: formatDayMonth(at), time });
}

/** A `YYYY-MM-DD` day as "1 December", read at noon so no time zone moves it. */
export function dayText(day: string): string {
  const noon = new Date(`${day}T12:00:00`);
  return `${noon.getDate()} ${formatMonth(noon)}`;
}

/** The first letter capital: the copy writes "your provider" in lower case, as the allowed phrase is. */
export function sentence(text: string): string {
  return text.charAt(0).toUpperCase() + text.slice(1);
}
