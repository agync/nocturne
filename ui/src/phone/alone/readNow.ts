// Reading on battery, in words: the "Its mind" row and the block under it on This phone, chosen
// from what the core reports (the phone's power, the person's Read now and why it stopped), never
// guessed. Nocturne offers Read now; it never starts it.

import { formatLongDay, formatTimeOfDay } from "../../copy/format";
import { type CopyId, t } from "../../copy/t";
import type { MindReading, MindStatus, ReadWords } from "./mind";

/** Read now stops at this level, and is not offered at it or below. */
export const LOW = 20;
/** Neither reads at this level or below. */
export const FLOOR = 5;

/** The button the block offers, and what pressing it does. */
export type ReadAction = "readNow" | "anyway" | "stop" | "changeLimit";

export interface ReadBlock {
  /** What happened, said first and plainly: a stop, the battery, the service. */
  said?: string;
  /** The "Everything you kept is read." line carries a tick. */
  tick?: boolean;
  /** A progress bar, "{done} of {count} read" to screen readers. */
  progress?: { done: number; count: number };
  lines: string[];
  button?: {
    action: ReadAction;
    label: string;
    /** Not pressable, with this reason joined to it. */
    needs?: string;
  };
}

/** The Android and iPhone words of one line, by the phone's own names. */
function pick(ios: boolean, android: CopyId, iphone: CopyId): CopyId {
  return ios ? iphone : android;
}

/** What still waits to be read. */
export function left(r: MindReading): number {
  return Math.max(0, r.count - r.done);
}

/** The words on the "Its mind" row, after "{host}, online". */
export function rowState(status: MindStatus, r: MindReading | null): string | null {
  if (!r) return null;
  const waiting = left(r);
  switch (status.state) {
    case "paused":
      return waiting > 0 ? t("phone.mind.row.paused", { left: waiting }) : null;
    case "unreachable":
      return waiting > 0 ? t("phone.mind.row.notAnswering", { left: waiting }) : null;
    case "offline":
      return waiting > 0 ? t("phone.mind.row.notOnline", { left: waiting }) : null;
    case "keyRefused":
    case "noCredit":
      return waiting > 0 ? t("phone.mind.row.stopped", { left: waiting }) : null;
    default:
      break;
  }
  if (waiting === 0) return null;
  if (r.now) return t("phone.mind.row.readingNow", { done: r.done, count: r.count });
  if (r.charging !== false) return t("phone.mind.row.reading", { done: r.done, count: r.count });
  return t("phone.mind.row.waiting", { left: waiting });
}

/** The block under the row: why reading waits, and what the person may do about it. */
export function readBlock(
  status: MindStatus,
  r: MindReading | null,
  ios: boolean,
): ReadBlock | null {
  if (!r || status.state === "off") return null;
  const waiting = left(r);
  const readNow = { action: "readNow" as const, label: t("phone.mind.readNow") };
  const anyway = { action: "anyway" as const, label: t("phone.mind.readNow.anyway") };
  if (status.state === "paused") {
    if (waiting === 0) return null;
    return {
      said: t("phone.mind.readNow.paused", { day: formatLongDay(status.until) }),
      lines: [],
      button: { action: "changeLimit", label: t("phone.mind.readNow.changeLimit") },
    };
  }
  if (status.state === "unreachable") {
    if (waiting === 0 || r.charging !== false) return null;
    if (r.stillNot)
      return {
        said: t("phone.mind.readNow.stillNot", { host: status.host }),
        lines: [],
        button: readNow,
      };
    return {
      said: t("phone.mind.readNow.notAnswering", {
        host: status.host,
        time: formatTimeOfDay(status.since),
      }),
      lines: [t("phone.mind.readNow.lookAgain")],
      button: readNow,
    };
  }
  if (status.state === "offline") {
    if (waiting === 0 || r.charging !== false) return null;
    return {
      said: t("phone.mind.readNow.offline"),
      lines: [],
      button: { ...readNow, needs: t("phone.mind.readNow.offlineNeeds") },
    };
  }
  if (status.state !== "fine") return null;
  const stopped = r.stopped ?? null;
  if (r.now) {
    const said =
      r.away != null && ios
        ? t("phone.mind.readNow.back", { done: r.away })
        : r.now === "anyway"
          ? t("phone.mind.readNow.anywayOn")
          : undefined;
    const lines = [
      t(pick(ios, "phone.mind.readNow.leaving.android", "phone.mind.readNow.leaving.iphone")),
    ];
    if (r.quiet && !ios) lines.push(t("phone.mind.readNow.noNotes"));
    return {
      said,
      progress: { done: r.done, count: r.count },
      lines,
      button: { action: "stop", label: t("phone.mind.readNow.stop") },
    };
  }
  if (waiting === 0) {
    return stopped?.why === "done"
      ? { said: t("phone.mind.readNow.done"), tick: true, lines: [] }
      : null;
  }
  if (r.charging !== false) {
    return stopped?.why === "charging"
      ? { said: t("phone.mind.readNow.charging"), lines: [] }
      : null;
  }
  const percent = r.percent ?? null;
  if (stopped?.why === "floor" || (percent !== null && percent <= FLOOR)) {
    return {
      said:
        stopped?.why === "floor"
          ? t("phone.mind.readNow.floor", { done: stopped.done })
          : t("phone.mind.readNow.lowNow", { percent: percent ?? FLOOR }),
      lines: [t("phone.mind.readNow.floorNeeds")],
    };
  }
  if (stopped?.why === "timeout") {
    return {
      said: t("phone.notify.readNow.stopped.title"),
      lines: [
        t("phone.mind.readNow.why"),
        t(pick(ios, "phone.mind.readNow.cost.android", "phone.mind.readNow.cost.iphone")),
      ],
      button: readNow,
    };
  }
  if (stopped?.why === "low") {
    return {
      said: t("phone.mind.readNow.low", { done: stopped.done }),
      lines: [t("phone.mind.readNow.anywayWhat")],
      button: anyway,
    };
  }
  if (stopped?.why === "saver") {
    return {
      said: t(pick(ios, "phone.mind.readNow.saver.android", "phone.mind.readNow.saver.iphone"), {
        done: stopped.done,
      }),
      lines: [
        t(
          pick(
            ios,
            "phone.mind.readNow.anywayWhatSaver.android",
            "phone.mind.readNow.anywayWhatSaver.iphone",
          ),
        ),
      ],
      button: anyway,
    };
  }
  if (percent !== null && percent <= LOW) {
    return {
      said: t("phone.mind.readNow.lowNow", { percent }),
      lines: [t("phone.mind.readNow.why"), t("phone.mind.readNow.anywayWhat")],
      button: anyway,
    };
  }
  if (r.saver) {
    return {
      said: t(
        pick(ios, "phone.mind.readNow.saverNow.android", "phone.mind.readNow.saverNow.iphone"),
      ),
      lines: [
        t("phone.mind.readNow.why"),
        t(
          pick(
            ios,
            "phone.mind.readNow.anywayWhatSaver.android",
            "phone.mind.readNow.anywayWhatSaver.iphone",
          ),
        ),
      ],
      button: anyway,
    };
  }
  return {
    lines: [
      t("phone.mind.readNow.why"),
      t(pick(ios, "phone.mind.readNow.cost.android", "phone.mind.readNow.cost.iphone")),
    ],
    button: readNow,
  };
}

/** The words the phone shows the reading with while the person is elsewhere. */
export function readWords(ios: boolean): ReadWords {
  return {
    channel: t("phone.notify.readNow.channel"),
    channelAbout: t("phone.notify.readNow.channelAbout"),
    title: t("phone.notify.readNow.title"),
    progress: t("phone.notify.readNow.progress", { done: "{done}", count: "{count}" }),
    stop: t("phone.notify.readNow.stop"),
    done: t("phone.notify.readNow.done"),
    lowTitle: t("phone.notify.readNow.low.title"),
    saverTitle: t(
      pick(
        ios,
        "phone.notify.readNow.saver.title.android",
        "phone.notify.readNow.saver.title.iphone",
      ),
    ),
    anywayText: t("phone.notify.readNow.anyway.text", { done: "{done}" }),
    floorTitle: t("phone.notify.readNow.floor.title"),
    floorText: t("phone.notify.readNow.floor.text", { done: "{done}" }),
    stoppedTitle: t("phone.notify.readNow.stopped.title"),
    stoppedText: t("phone.notify.readNow.stopped.text", { done: "{done}" }),
    pausedTitle: t("phone.notify.readNow.paused.title"),
    pausedText: t("phone.notify.readNow.paused.text", { done: "{done}", count: "{count}" }),
  };
}
