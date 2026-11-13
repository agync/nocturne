import { formatDay, formatDuration, formatDurationSpoken, formatLongDay } from "../../copy/format";
import { t } from "../../copy/t";
import type { Receipt } from "../../engine/bindings";
import { Thumb } from "../Thumb/Thumb";
import styles from "./ReceiptChip.module.css";

interface ReceiptChipProps {
  receipt: Receipt;
  /** A voice chip that is playing right now. */
  playing?: boolean;
  /** Opens the original in context; voice chips play inline. Without it the chip is plain text. */
  onOpen?: (receipt: Receipt) => void;
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

function spokenKind(receipt: Receipt): string {
  switch (receipt.kind) {
    case "audio":
      return t("kind.audio.spoken");
    case "text":
      return isPasted(receipt) ? t("kind.paste.spoken") : t("kind.text.spoken");
    case "web":
      return t("kind.web.spoken");
    case "image":
      return receipt.label || t("kind.image.spoken");
    case "document":
      return receipt.label || t("kind.document.spoken");
  }
}

/** "Source: voice note, 10 seconds, Monday 10 November. Press Enter to play." */
export function receiptLabel(receipt: Receipt, interactive: boolean): string {
  let detail = "";
  if (receipt.kind === "audio" && receipt.durationMs !== null) {
    detail = t("receipt.aria.duration", { duration: formatDurationSpoken(receipt.durationMs) });
  }
  const p = page(receipt);
  if (receipt.kind === "document" && p !== null) detail = t("receipt.aria.page", { page: p });
  const action = [
    receipt.unconfirmed ? t("receipt.aria.unconfirmed") : "",
    interactive ? t("receipt.aria.open") : "",
  ].join("");
  return t("receipt.aria", {
    kind: spokenKind(receipt),
    detail,
    date: formatLongDay(receipt.keptAt),
    action,
  });
}

/** The signature component: every claim points at where it came from. */
export function ReceiptChip({ receipt, playing = false, onOpen }: ReceiptChipProps) {
  const text = receiptText(receipt);
  const common = {
    className: `${styles.chip} ${receipt.unconfirmed ? styles.unconfirmed : ""}`.trim(),
    "data-kind": receipt.kind,
    "data-unconfirmed": receipt.unconfirmed,
    "data-playing": playing,
  };
  const inner = (
    <>
      <Thumb kind={receipt.kind} size={receipt.kind === "image" ? 18 : 14} />
      <span>{text}</span>
    </>
  );
  if (!onOpen) {
    return (
      <span {...common} role="img" aria-label={receiptLabel(receipt, false)}>
        {inner}
      </span>
    );
  }
  return (
    <button
      type="button"
      {...common}
      aria-label={receiptLabel(receipt, true)}
      onClick={() => onOpen(receipt)}
    >
      {inner}
    </button>
  );
}
