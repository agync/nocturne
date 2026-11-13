import { addDays, formatDayMonth, formatMonth, localDay } from "../../copy/format";
import { t } from "../../copy/t";
import type { ItemKind, MemoryView, Source } from "../../engine/bindings";

/** The small right-hand line of a memory: when it started, ended, was last used or was learned. */
export function memoryWhen(m: MemoryView): string {
  if (m.was) {
    return t("memory.when.changed", {
      day: formatDayMonth(m.validFrom ?? m.learnedAt),
      was: m.was,
    });
  }
  if (m.due) return t("memory.when.due", { day: formatDayMonth(m.due) });
  if (m.validTo) return t("memory.when.until", { day: formatDayMonth(m.validTo) });
  if ((m.status === "fading" || m.status === "letGo") && m.lastUsed) {
    return t("memory.when.lastUsed", { day: formatDayMonth(m.lastUsed) });
  }
  if (m.validFrom) return t("memory.when.since", { day: formatDayMonth(m.validFrom) });
  return t("memory.when.learned", { day: formatDayMonth(m.learnedAt) });
}

/** How soon a fading memory is let go, as words, or null when that is not known. */
export function letGoIn(m: MemoryView, today: string | null): string | null {
  let target: string | null = null;
  if (m.letGoAt) target = localDay(m.letGoAt);
  else if (m.lastUsed) target = addDays(localDay(m.lastUsed), 90);
  if (!target || !today) return null;
  const days = Math.round(
    (new Date(`${target}T12:00:00`).getTime() - new Date(`${today}T12:00:00`).getTime()) /
      86_400_000,
  );
  if (days <= 0) return t("fading.letGo.tonight");
  if (days < 14) return t("fading.letGo.days", { count: days });
  return t("fading.letGo.weeks", { count: Math.round(days / 7) });
}

/** "Fading · last used 7 Sep · let go in 3 weeks" */
export function fadingMeta(m: MemoryView, today: string | null): string {
  return [
    t("memory.status.fading"),
    m.lastUsed ? t("memory.when.lastUsed", { day: formatDayMonth(m.lastUsed) }) : null,
    letGoIn(m, today),
  ]
    .filter(Boolean)
    .join(" · ");
}

/** "Let go 13 Nov · not used since August" */
export function letGoMeta(m: MemoryView): string {
  return [
    m.letGoAt ? t("fading.letGo.on", { day: formatDayMonth(m.letGoAt) }) : t("memory.status.letGo"),
    m.lastUsed ? t("fading.notUsedSince", { month: formatMonth(m.lastUsed) }) : null,
  ]
    .filter(Boolean)
    .join(" · ");
}

const KIND_WORD: Record<
  ItemKind,
  "item.kind.text" | "item.kind.image" | "item.kind.audio" | "item.kind.document" | "item.kind.web"
> = {
  text: "item.kind.text",
  image: "item.kind.image",
  audio: "item.kind.audio",
  document: "item.kind.document",
  web: "item.kind.web",
};

export const kindWord = (kind: ItemKind): string => t(KIND_WORD[kind]);

const SOURCE_WORD: Record<Source, string> = {
  quickCapture: "item.source.quickCapture",
  drop: "item.source.drop",
  paste: "item.source.paste",
  pastedWeb: "item.source.pastedWeb",
  watchedFolder: "item.source.watchedFolder",
  screenshot: "item.source.screenshot",
  recording: "item.source.recording",
  agent: "item.source.agent",
  setup: "item.source.setup",
};

export const sourceWord = (source: Source): string =>
  t(SOURCE_WORD[source] as Parameters<typeof t>[0]);
