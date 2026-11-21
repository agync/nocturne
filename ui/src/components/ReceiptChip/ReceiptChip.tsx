import { formatDay, formatDuration } from "../../copy/format";
import { t } from "../../copy/t";
import type { Receipt } from "../../engine/bindings";
import type { IconName } from "../Icon/Icon";
import { Thumb } from "../Thumb/Thumb";
import styles from "./ReceiptChip.module.css";

interface ReceiptChipProps {
  receipt: Receipt;
  /** A voice chip that is playing right now. */
  playing?: boolean;
  /** Opens the original in context; voice chips play inline. Without it the chip is plain text. */
  onOpen?: (receipt: Receipt) => void;
  /** The words on the chip, for a screen that words it its own way (the journal's "Entry · …"). */
  text?: string;
  /** The small picture before the words, when the kind's own does not say it (sleep is a moon). */
  icon?: IconName;
}

function page(receipt: Receipt): number | null {
  return receipt.span?.type === "page" ? receipt.span.page : null;
}

function isPasted(receipt: Receipt): boolean {
  return receipt.source === "paste";
}

/** "Voice note · 0:10 · Mon 10 Nov": plain kind, then the detail, then the date. */
export function receiptText(receipt: Receipt): string {
  const date = formatDay(receipt.keptAt);
  const parts: string[] = [];
  switch (receipt.kind) {
    case "audio":
      parts.push(t("kind.audio"));
      if (receipt.durationMs !== null) parts.push(formatDuration(receipt.durationMs));
      parts.push(date);
      break;
    case "text":
      parts.push(isPasted(receipt) ? t("kind.paste") : t("kind.text"), date);
      break;
    case "web":
      parts.push(t("kind.web"), date);
      break;
    case "calendar":
      // "Calendar · Family · Sam’s phone": the calendar it came from, then the phone.
      parts.push(t("kind.calendar"));
      if (receipt.calendar) parts.push(receipt.calendar);
      if (receipt.fromDevice) parts.push(receipt.fromDevice);
      break;
    case "reading":
      parts.push(t("kind.reading"));
      if (receipt.fromDevice) parts.push(receipt.fromDevice);
      break;
    case "image":
      parts.push(receipt.label || t("kind.image"), date);
      break;
    case "document": {
      parts.push(receipt.label || t("kind.document"));
      const p = page(receipt);
      parts.push(p !== null ? t("receipt.page", { page: p }) : date);
      break;
    }
  }
  if (receipt.unconfirmed) parts.push(t("receipt.unconfirmed"));
  return parts.join(" · ");
}

/**
 * What a screen reader says: the visible words first, so the name always contains what is on
 * screen, then what the chip is.
 */
export function receiptLabel(receipt: Receipt): string {
  return t("receipt.aria", { visible: receiptText(receipt) });
}

/** The signature component: every claim points at where it came from. */
export function ReceiptChip({
  receipt,
  playing = false,
  onOpen,
  text: given,
  icon,
}: ReceiptChipProps) {
  const text = given ?? receiptText(receipt);
  const common = {
    className: `${styles.chip} hit ${receipt.unconfirmed ? styles.unconfirmed : ""}`.trim(),
    "data-kind": receipt.kind,
    "data-unconfirmed": receipt.unconfirmed,
    "data-playing": playing,
  };
  const inner = (
    <>
      <Thumb kind={receipt.kind} size={receipt.kind === "image" ? 18 : 14} icon={icon} />
      <span>{text}</span>
    </>
  );
  if (!onOpen) {
    return (
      <span {...common} role="img" aria-label={t("receipt.aria", { visible: text })}>
        {inner}
      </span>
    );
  }
  return (
    <button
      type="button"
      {...common}
      aria-label={t("receipt.aria", { visible: text })}
      onClick={() => onOpen(receipt)}
    >
      {inner}
    </button>
  );
}
