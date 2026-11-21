import { localDay } from "../../copy/format";
import type { Receipt } from "../../engine/bindings";

/** Where a receipt on a page, an answer or a memory leads: the original's own page. */
export type ReceiptFrom = "today" | "journal";

/**
 * The day a health reading belongs to. The computer writes it as noon of that day with its own
 * zone's offset, so the date in the text is the day wherever the receipt is read; a reading kept
 * at a moment in UTC has the day it was kept on, where the person is.
 */
export function readingDay(keptAt: string): string {
  return /T12:00:00[+-]\d\d:\d\d$/.test(keptAt) ? keptAt.slice(0, 10) : localDay(keptAt);
}

/**
 * A health reading has no page of its own to read, send, keep or forget: its receipt opens the
 * Health page of the day it was taken. A calendar entry remembers which screen sent the person.
 */
export function receiptPath(receipt: Receipt, from?: ReceiptFrom): string {
  if (receipt.kind === "reading") {
    return `/journal/health?day=${readingDay(receipt.keptAt)}`;
  }
  if (receipt.kind === "calendar" && from) return `/items/${receipt.itemId}?from=${from}`;
  return `/items/${receipt.itemId}`;
}
