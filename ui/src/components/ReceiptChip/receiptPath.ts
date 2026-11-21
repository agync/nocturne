import { localDay } from "../../copy/format";
import type { Receipt } from "../../engine/bindings";

/** Where a receipt on a page, an answer or a memory leads: the original's own page. */
export type ReceiptFrom = "today" | "journal";

/**
 * A health reading has no page of its own to read, send, keep or forget: its receipt opens the
 * Health page of the day it was taken. A calendar entry remembers which screen sent the person.
 */
export function receiptPath(receipt: Receipt, from?: ReceiptFrom): string {
  if (receipt.kind === "reading") {
    return `/journal/health?day=${localDay(receipt.keptAt)}`;
  }
  if (receipt.kind === "calendar" && from) return `/items/${receipt.itemId}?from=${from}`;
  return `/items/${receipt.itemId}`;
}
