import { formatDay, formatDuration } from "../copy/format";
import { t } from "../copy/t";
import type { Receipt } from "../engine/bindings";

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
    case "text":
      return [receipt.source === "paste" ? t("kind.paste") : t("kind.text"), date].join(" · ");
  }
}
