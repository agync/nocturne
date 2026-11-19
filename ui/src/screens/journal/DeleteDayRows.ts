import { formatDay, formatDuration } from "../../copy/format";
import { t } from "../../copy/t";
import type {
  EntryBlock,
  FeelingPart,
  ForgetOriginal,
  ItemView,
  JournalDay,
  TimelineRow,
} from "../../engine/bindings";
import { PARTS } from "../../journal/feelings";
import type { GlyphName } from "./glyphs";
import { clock12 } from "./time";

export type RowIcon = GlyphName | "sun" | FeelingPart;

/** One thing in the delete dialog: what the person sees and every item id that goes with it. */
export interface DeleteRow {
  key: string;
  /** The row's own original first, then its earlier versions and the feeling tapped with it. */
  ids: string[];
  section: "entry" | "day" | "other";
  icon: RowIcon;
  time: string;
  tag: string | null;
  /** "Low · tired", for a check-in. */
  feel: string | null;
  text: string;
  tail: string | null;
  changed: { time: string; earlier: Array<{ time: string; text: string }> } | null;
}

/** The words of a row as one name, so assistive technology and a hover read it whole. */
export function rowName(row: DeleteRow): string {
  return [row.tag, row.feel, row.text, row.tail].filter(Boolean).join(" ");
}

/** Cuts at a word with "…" when the text is longer than `max`. */
export function cut(text: string, max = 90): string {
  const flat = text.replace(/\s+/g, " ").trim();
  if (flat.length <= max) return flat;
  const head = flat.slice(0, max);
  const space = head.lastIndexOf(" ");
  const word = space > max * 0.5 ? head.slice(0, space) : head;
  return `${word.replace(/[\s,.;:!?-]+$/, "")}…`;
}

type Seen = Pick<
  ItemView,
  "kind" | "title" | "durationMs" | "text" | "summary" | "status" | "fromDevice" | "source"
>;

const quoted = (text: string | null | undefined): string => (text ? `“${text}”` : "");

function tagOf(item: Seen): string | null {
  switch (item.kind) {
    case "audio":
      return t("journal.entry.voice", { duration: formatDuration(item.durationMs ?? 0) });
    case "image":
      return t("item.kind.image");
    case "document":
      return t("item.kind.document");
    case "web":
      return t("item.kind.web");
    default:
      return null;
  }
}

const unread = (item: Seen) => item.status === "kept" || item.status === "reading";

function iconOf(item: Seen): RowIcon {
  switch (item.kind) {
    case "audio":
      return "mic";
    case "image":
      return "image";
    case "document":
      return "file";
    case "web":
      return "web";
    default:
      return item.fromDevice ? "phone" : "pen";
  }
}

function snippet(item: Seen, alt: string | null = null): { tag: string | null; text: string } {
  switch (item.kind) {
    case "audio":
      return { tag: tagOf(item), text: quoted(item.text) };
    case "image": {
      const seen = alt ?? item.summary;
      return seen
        ? { tag: tagOf(item), text: seen }
        : { tag: null, text: t("journal.entry.photoUnread") };
    }
    case "document":
    case "web":
      return { tag: tagOf(item), text: item.title };
    default:
      return { tag: null, text: item.text ?? item.title };
  }
}

function tailOf(item: Seen): string | null {
  if (item.fromDevice) return t("journal.timeline.fromDevice", { phone: item.fromDevice });
  if (item.source === "drop") return t("journal.delete.droppedIn");
  return null;
}

export function blockRow(block: EntryBlock): DeleteRow {
  const { item } = block;
  const first = snippet(item, block.alt);
  return {
    key: `item:${item.id}`,
    ids: [item.id, ...block.earlier.map((e) => e.itemId)],
    section: "entry",
    icon: iconOf(item),
    time: clock12(item.keptAt),
    tag: first.tag,
    feel: null,
    text: unread(item) && item.kind === "audio" ? "" : first.text,
    tail: null,
    changed:
      block.earlier.length > 0 && block.changedAt
        ? {
            time: clock12(block.changedAt),
            earlier: block.earlier.map((e) => ({
              time: clock12(e.keptAt),
              text: quoted(e.text ?? ""),
            })),
          }
        : null,
  };
}

export function timelineRow(row: TimelineRow): DeleteRow | null {
  if (row.kind === "entry") return null;
  const { item } = row;
  const ids = [item.id, ...(row.feeling ? [row.feeling.id] : [])];
  const base = { key: `item:${item.id}`, ids, section: "day" as const, time: clock12(item.keptAt) };
  if (row.kind === "checkin" && row.checkin) {
    const words = row.checkin.feeling.words.join(", ");
    return {
      ...base,
      icon: row.checkin.feeling.part,
      tag: null,
      feel: t("journal.checkin.feel", { part: t(PARTS[row.checkin.feeling.part].name), words }),
      text: quoted(row.checkin.line),
      tail: null,
      changed: null,
    };
  }
  if (row.kind === "good") {
    return {
      ...base,
      icon: "sun",
      tag: t("journal.delete.goodThing"),
      feel: null,
      text: row.good?.text ?? item.title,
      tail: null,
      changed: null,
    };
  }
  const first = snippet(item);
  return {
    ...base,
    icon: iconOf(item),
    tag: first.tag,
    feel: null,
    text: first.text,
    tail: tailOf(item),
    changed: null,
  };
}

/** Group one: the entry's blocks in order, then the rest of the day as the timeline orders it. */
export function dayRows(day: JournalDay): DeleteRow[] {
  const blocks = [...day.entry].sort((a, b) => a.block - b.block).map(blockRow);
  const rest = day.rows.flatMap((r) => timelineRow(r) ?? []);
  return [...blocks, ...rest];
}

/** Group two: an original from another day, as the preview lists it. */
export function otherRow(original: ForgetOriginal): DeleteRow {
  const stand: Seen = {
    kind: original.kind,
    title: original.label,
    durationMs: original.durationMs,
    text: null,
    summary: original.label,
    fromDevice: null,
    source: "journal",
    status: "understood",
  };
  const first = original.kind === "audio" ? { tag: tagOf(stand), text: "" } : snippet(stand);
  return {
    key: `item:${original.itemId}`,
    ids: [original.itemId],
    section: "other",
    icon: iconOf(stand),
    time: formatDay(original.keptAt),
    tag: first.tag,
    feel: null,
    text: first.text,
    tail: null,
    changed: null,
  };
}
