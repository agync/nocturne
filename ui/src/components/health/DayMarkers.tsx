import type { ReactNode } from "react";
import { formatDay } from "../../copy/format";
import styles from "./Charts.module.css";

interface DayMarkersProps {
  /** The x position of each day with a journal entry. */
  at: readonly number[];
  /** The baseline the ticks hang under. */
  baseline: number;
}

/** A small tick under each day with a journal entry. */
export function DayMarkers({ at, baseline }: DayMarkersProps) {
  return (
    <g>
      {at.map((x) => (
        <line key={x} className={styles.tick} x1={x} x2={x} y1={baseline + 4} y2={baseline + 9} />
      ))}
    </g>
  );
}

/** A ring round the day being looked at. */
export function DayRing({ x, y }: { x: number; y: number }) {
  return <circle className={styles.ring} cx={x} cy={y} r={7} />;
}

/** A pointer's way to a day's readings. Left out of the tab order: the list holds the same receipts. */
export function DayLink({
  href,
  day,
  children,
}: {
  href: string;
  day: string;
  children: ReactNode;
}) {
  return (
    <a href={href} tabIndex={-1}>
      <title>{formatDay(day)}</title>
      {children}
    </a>
  );
}
