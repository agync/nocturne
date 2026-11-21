import { useEffect, useRef, useState } from "react";
import {
  beatsText,
  between,
  clock,
  duration,
  num,
  pressureText,
  stepsText,
  workoutText,
} from "../../components/health";
import { formatDay } from "../../copy/format";
import { t } from "../../copy/t";
import type { ReadingRow } from "../../engine/bindings";
import styles from "./Health.module.css";

const minutesOf = (row: ReadingRow): number =>
  row.value.type === "sleep"
    ? row.value.asleep.reduce((sum, p) => sum + (Date.parse(p.to) - Date.parse(p.from)) / 60_000, 0)
    : 0;

/** The bold value of a row, by kind: "6 h 40 min", "52 to 148 beats a minute", "128 over 82". */
export function valueText(row: ReadingRow): string {
  const v = row.value;
  switch (v.type) {
    case "sleep":
      return duration(minutesOf(row));
    case "heartRate":
      if (v.hours.length === 0) return "";
      return t("health.beats.range", {
        range: between(
          num(Math.min(...v.hours.map((h) => h.low))),
          num(Math.max(...v.hours.map((h) => h.high))),
        ),
      });
    case "restingHeartRate":
      return beatsText(v.bpm);
    case "bloodPressure":
      return pressureText(v.top, v.bottom);
    case "steps":
      return stepsText(v.total);
    case "workout":
      return workoutText(v.kind, v.lengthS, v.distanceM);
    case "weight":
      return `${num(v.value)} ${v.unit}`;
  }
}

/** The time of a row, where the kind has one: sleep's span, a reading's moment. */
function timeText(row: ReadingRow): string | null {
  const v = row.value;
  switch (v.type) {
    case "sleep": {
      const first = v.asleep[0];
      const last = v.asleep[v.asleep.length - 1];
      return first && last ? between(clock(first.from), clock(last.to)) : null;
    }
    case "bloodPressure":
    case "workout":
    case "weight":
      return clock(row.start);
    default:
      return null;
  }
}

const sourceText = (row: ReadingRow): string =>
  row.sourceDevice
    ? t("health.source", { device: row.sourceDevice, phone: row.phone })
    : t("health.source.noDevice", { phone: row.phone });

export type OnDelete = (row: ReadingRow, opener: HTMLElement | null) => void;

/** A reading's small menu: Delete… alone, which opens the delete dialog for that reading. */
function RowMenu({ row, onDelete }: { row: ReadingRow; onDelete: OnDelete }) {
  const [open, setOpen] = useState(false);
  const wrap = useRef<HTMLSpanElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!open) return;
    wrap.current?.querySelector<HTMLElement>('[role="menuitem"]')?.focus();
    const away = (event: PointerEvent) => {
      if (!wrap.current?.contains(event.target as Node)) setOpen(false);
    };
    const onEscape = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.stopPropagation();
      setOpen(false);
      trigger.current?.focus();
    };
    document.addEventListener("pointerdown", away);
    document.addEventListener("keydown", onEscape);
    return () => {
      document.removeEventListener("pointerdown", away);
      document.removeEventListener("keydown", onEscape);
    };
  }, [open]);

  return (
    <span ref={wrap} className={`${styles.rowMenu} ${styles.noPrint}`}>
      <button
        ref={trigger}
        type="button"
        className={styles.rowMore}
        aria-haspopup="menu"
        aria-expanded={open}
        aria-label={t("health.row.menu", { day: formatDay(row.day) })}
        onClick={() => setOpen(!open)}
      >
        …
      </button>
      {open ? (
        <span
          className={styles.menu}
          role="menu"
          aria-label={t("health.row.menu", { day: formatDay(row.day) })}
        >
          <button
            type="button"
            role="menuitem"
            className={styles.menuItem}
            onClick={() => {
              setOpen(false);
              onDelete(row, trigger.current);
            }}
          >
            {t("health.row.delete")}
          </button>
        </span>
      ) : null}
    </span>
  );
}

/** The readings by day, newest first, each with where it came from and the journal day it belongs to. */
export function ReadingsList({
  rows,
  onDelete,
  selected = null,
}: {
  rows: readonly ReadingRow[];
  onDelete?: OnDelete;
  /** The day the page was opened on from a receipt: marked, and brought into view. */
  selected?: string | null;
}) {
  const list = useRef<HTMLUListElement>(null);
  const first = rows.find((r) => r.day === selected)?.itemId;
  useEffect(() => {
    if (first === undefined) return;
    list.current
      ?.querySelector<HTMLElement>('[aria-current="true"]')
      ?.scrollIntoView?.({ block: "center" });
  }, [first]);
  return (
    <ul ref={list} className={styles.list}>
      {rows.map((row) => (
        <li
          key={row.itemId}
          className={styles.row}
          data-removed={row.removed}
          aria-current={row.day === selected ? "true" : undefined}
        >
          <span className={styles.rowDay}>{formatDay(row.day)}</span>
          <span className={styles.rowValue}>{valueText(row)}</span>
          <span className={styles.rowSource}>
            {row.removed
              ? t("health.removed")
              : [timeText(row), sourceText(row)].filter(Boolean).join(" · ")}
          </span>
          {row.removed ? null : (
            <a className={styles.rowLink} href={`#/journal/${row.day}`}>
              {t("health.journal")}
            </a>
          )}
          {onDelete ? <RowMenu row={row} onDelete={onDelete} /> : null}
        </li>
      ))}
    </ul>
  );
}
