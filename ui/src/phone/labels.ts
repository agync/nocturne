import { formatDay, formatDuration } from "../copy/format";
import { t } from "../copy/t";
import type { Receipt } from "../engine/bindings";

/** "Calendar · Family": an appointment's receipt on the phone, which leaves out the phone. */
export const calendarMeta = (calendar: string): string =>
  t("phone.app.receipt.calendar", { calendar });

/** "Sleep · Health · Sam’s phone": a night's receipt. */
export const sleepMeta = (phone: string): string => t("page.lastNight.receipt", { phone });

/** What kind of thing a receipt points at, and when it was kept: "Voice note · 0:08 · Mon 10 Nov". */
export function receiptMeta(receipt: Receipt): string {
  const date = formatDay(receipt.keptAt);
  switch (receipt.kind) {
    case "audio":
      return [
        t("kind.audio"),
        ...(receipt.durationMs !== null ? [formatDuration(receipt.durationMs)] : []),
        date,
      ].join(" · ");
    case "image":
      return [t("kind.image"), date].join(" · ");
    case "document":
      return [t("kind.document"), date].join(" · ");
    case "web":
      return [t("kind.web"), date].join(" · ");
    case "checkin":
      return [t("kind.checkin"), date].join(" · ");
    // "Calendar · Family · Sam’s phone": the calendar it came from, then the phone.
    case "calendar":
      return [t("kind.calendar"), receipt.calendar, receipt.fromDevice]
        .filter((part): part is string => Boolean(part))
        .join(" · ");
    // "Steps · Health · Sam’s phone": a reading is titled by its kind.
    case "reading":
      return [receipt.label, t("kind.reading"), receipt.fromDevice]
        .filter((part): part is string => Boolean(part))
        .join(" · ");
    case "text":
      return [receipt.source === "paste" ? t("kind.paste") : t("kind.text"), date].join(" · ");
  }
}
