import { formatDuration, formatTime, localDay } from "../../copy/format";
import { t } from "../../copy/t";
import type { CouldntReadReason, ItemKind, ItemView, Source } from "../../engine/bindings";

/** The small code on a thumbnail that is not a picture: the file's own ending, or a plain default. */
export function thumbCode(item: ItemView): string {
  const ext = item.fileName?.split(".").pop();
  if (ext && ext.length <= 4 && ext !== item.fileName) return ext.toUpperCase();
  const fallback: Record<ItemKind, string> = {
    text: "TXT",
    audio: "M4A",
    web: "WEB",
    document: "PDF",
    image: "",
  };
  return fallback[item.kind];
}

function kindWord(item: ItemView): string {
  switch (item.kind) {
    case "audio":
      return t("kind.audio");
    case "image":
      return t("kind.image");
    case "web":
      return t("kind.web");
    case "text":
      return item.source === "paste" ? t("kind.pasted") : t("kind.text");
    case "document":
      return item.source === "drop" ? t("recent.kind.dropped") : t("kind.document");
  }
}

const SOURCE_NOTE: Partial<Record<Source, Parameters<typeof t>[0]>> = {
  quickCapture: "recent.source.quickCapture",
  watchedFolder: "recent.source.watchedFolder",
  screenshot: "recent.source.screenshot",
  agent: "recent.source.agent",
};

export function reasonText(reason: CouldntReadReason): string {
  return t(`recent.reason.${reason}`);
}

/** "Photo · 08:41 · quick capture", "Voice note · 0:06 · 08:15", "Dropped · yesterday · the file is password-locked". */
export function subline(item: ItemView, today: string): string {
  const parts = [kindWord(item)];
  if (item.kind === "audio" && item.durationMs !== null)
    parts.push(formatDuration(item.durationMs));
  parts.push(
    localDay(item.keptAt) === today ? formatTime(item.keptAt) : t("recent.when.yesterday"),
  );
  if (item.status === "couldntRead" && item.reason) {
    parts.push(reasonText(item.reason));
  } else {
    const note = SOURCE_NOTE[item.source];
    if (note) parts.push(t(note));
  }
  return parts.join(" · ");
}

export type RowAction =
  | { type: "readNow" }
  | { type: "open" }
  | { type: "which" }
  | { type: "confirm" }
  | { type: "retry" };

/** What the person can do with this item, or null when there is nothing to do. */
export function actionFor(item: ItemView): RowAction | null {
  switch (item.status) {
    case "kept":
      return { type: "readNow" };
    case "understood":
    case "partlyUnderstood":
      return { type: "open" };
    case "needsYou":
      return { type: "which" };
    case "unconfirmed":
      return { type: "confirm" };
    case "couldntRead":
      return item.reason === "corrupt" || item.reason === "mindUnavailable" || item.reason === null
        ? { type: "retry" }
        : null;
    case "reading":
      return null;
  }
}

/** A password-locked file has no button: the person fixes it outside Nocturne and drops it again. */
export const needsUnlockedCopy = (item: ItemView) =>
  item.status === "couldntRead" && item.reason === "locked";
