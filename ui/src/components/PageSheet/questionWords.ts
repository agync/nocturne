import { formatDay, formatDuration, formatTimeOfDay } from "../../copy/format";
import { t } from "../../copy/t";
import type { JournalAsk, QuestionChoice, Receipt } from "../../engine/bindings";

const WEEKDAY = new Intl.DateTimeFormat("en-GB", { weekday: "long" });
const MONTH = new Intl.DateTimeFormat("en-GB", { month: "long" });

const noon = (day: string): Date => {
  const [y = 1970, m = 1, d = 1] = day.split("-").map(Number);
  return new Date(y, m - 1, d, 12);
};

/** Whole days from the day the block was written to the day the event was on. */
function daysBetween(writtenOn: string, eventDay: string): number {
  return Math.round((noon(eventDay).getTime() - noon(writtenOn).getTime()) / 86_400_000);
}

/** "Yesterday you wrote:", "On Monday you said:", "On 4 November you wrote:". */
export function opening(ask: JournalAsk): string {
  const days = daysBetween(ask.writtenOn, ask.eventDay);
  const written = noon(ask.writtenOn);
  if (days <= 0) {
    return ask.voice
      ? t("journal.question.opening.sameDay.said")
      : t("journal.question.opening.sameDay.wrote");
  }
  if (days < 7) {
    const weekday = WEEKDAY.format(written);
    return ask.voice
      ? t("journal.question.opening.weekday.said", { weekday })
      : t("journal.question.opening.weekday.wrote", { weekday });
  }
  const date = { day: written.getDate(), month: MONTH.format(written) };
  return ask.voice
    ? t("journal.question.opening.date.said", date)
    : t("journal.question.opening.date.wrote", date);
}

/** "Entry · Thu 19 Nov · 8:12 am", or "Voice note · 0:42". */
export function chipText(receipt: Receipt, ask: JournalAsk): string {
  if (ask.voice) {
    return t("journal.question.chip.voice", { duration: formatDuration(receipt.durationMs ?? 0) });
  }
  return t("journal.question.chip.entry", {
    day: formatDay(receipt.keptAt),
    time: formatTimeOfDay(receipt.keptAt),
  });
}

export function choiceText(choice: QuestionChoice): string {
  switch (choice) {
    case "fine":
      return t("journal.question.fine");
    case "notGreat":
      return t("journal.question.notGreat");
    default:
      return t("journal.question.notYet");
  }
}
