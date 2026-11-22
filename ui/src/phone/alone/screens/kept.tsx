import { t } from "../../../copy/t";
import type { TimelineRow } from "../../../engine/bindings";
import { Icon, type IconName } from "../../screens/ui";

/** Kept, and still waiting for a mind to read it: a photo, a document, a voice note with no words. */
export function waitsToBeRead(row: TimelineRow): boolean {
  const item = row.item;
  if (item.status !== "kept" && item.status !== "reading") return false;
  if (row.kind === "photo") return !item.summary;
  if (row.kind === "voice") return !item.text;
  return row.kind === "elsewhere" && item.kind !== "text" && item.kind !== "web" && !item.text;
}

const kindIcon = (row: TimelineRow): IconName | null =>
  row.kind === "voice"
    ? "mic"
    : row.kind === "photo"
      ? "image"
      : row.kind === "elsewhere"
        ? "document"
        : null;

function kindWord(row: TimelineRow): string {
  if (row.item.kind === "audio") return t("kind.audio");
  if (row.item.kind === "image") return t("kind.image");
  return t("kind.document");
}

/** A voice note whose words this phone heard, before any mind has read it. */
export function heardByPhone(row: TimelineRow): boolean {
  return row.kind === "voice" && Boolean(row.item.text) && row.item.status === "kept";
}

/** One kept thing's line: its words, or its kind and "waiting to be read". */
export function KeptLine({ row }: { row: TimelineRow }) {
  const icon = kindIcon(row);
  if (waitsToBeRead(row)) {
    return (
      <span>
        {icon ? <Icon name={icon} size={14} /> : null} {kindWord(row)} ·{" "}
        <span className="ph-wait">{t("phone.alone.waiting")}</span>
      </span>
    );
  }
  const words =
    row.good?.text ?? row.item.text ?? row.item.summary ?? row.item.title ?? kindWord(row);
  return (
    <span>
      {icon ? (
        <>
          <Icon name={icon} size={14} /> {kindWord(row)} ·{" "}
        </>
      ) : null}
      {words}
      {heardByPhone(row) ? (
        <>
          {" "}
          <span className="ph-wait">{t("phone.alone.heard")}</span>
        </>
      ) : null}
    </span>
  );
}
