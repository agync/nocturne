import { addDays, formatDayMonth, formatTime, localDay } from "../../copy/format";
import { t } from "../../copy/t";

type Relation = "today" | "yesterday" | "other";

function relation(at: string, now: number): Relation {
  const day = localDay(at);
  const today = localDay(new Date(now));
  if (day === today) return "today";
  if (day === addDays(today, -1)) return "yesterday";
  return "other";
}

/** "today, 06:48", "yesterday, 18:40" or "2 Nov, 18:40". */
export function whenText(at: string, now: number = Date.now()): string {
  const time = formatTime(at);
  const day = formatDayMonth(at);
  switch (relation(at, now)) {
    case "today":
      return t("away.when.today", { time });
    case "yesterday":
      return t("away.when.yesterday", { time });
    default:
      return t("away.when.other", { day, time });
  }
}

/** "at 03:41 today", "at 03:41 yesterday" or "at 03:41 on 2 Nov". */
export function atText(at: string, now: number = Date.now()): string {
  const time = formatTime(at);
  const day = formatDayMonth(at);
  switch (relation(at, now)) {
    case "today":
      return t("away.at.today", { time });
    case "yesterday":
      return t("away.at.yesterday", { time });
    default:
      return t("away.at.other", { day, time });
  }
}

/** "Since 07:02 today" or "Since 2 Nov, 07:02". */
export function sinceText(at: string, now: number = Date.now()): string {
  const time = formatTime(at);
  const day = formatDayMonth(at);
  switch (relation(at, now)) {
    case "today":
      return t("away.since.today", { time });
    case "yesterday":
      return t("away.since.yesterday", { time });
    default:
      return t("away.since.other", { day, time });
  }
}

/** The heading of a day in the activity list: Today, Yesterday or "Mon 10 Nov". */
export function dayHeading(day: string, now: number = Date.now()): string {
  const today = localDay(new Date(now));
  if (day === today) return t("away.day.today");
  if (day === addDays(today, -1)) return t("away.day.yesterday");
  return formatDayMonth(`${day}T12:00:00`);
}
