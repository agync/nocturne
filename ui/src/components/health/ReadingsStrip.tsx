import type { Band } from "../../engine/bindings";
import styles from "./Charts.module.css";
import { DayMarkers, DayRing } from "./DayMarkers";
import { dayIndex } from "./derive";
import { num } from "./sentence";

export interface PressurePoint {
  day: string;
  top: number;
  bottom: number;
}

interface ReadingsStripProps {
  readings: readonly PressurePoint[];
  from: string;
  to: string;
  bandTop?: Band | null;
  bandBottom?: Band | null;
  journalDays?: readonly string[];
  /** The day looked at, with a ring. */
  ring?: string;
  labels?: ReadonlyArray<{ day: string; text: string }>;
  sentence: string;
  width?: number;
  height?: number;
}

const LEFT = 36;

/** Each reading an upright line from the bottom number to the top, a dot at each end, by day. */
export function ReadingsStrip({
  readings,
  from,
  to,
  bandTop,
  bandBottom,
  journalDays = [],
  ring,
  labels = [],
  sentence,
  width = 520,
  height = 150,
}: ReadingsStripProps) {
  const days = Math.max(1, dayIndex(from, to));
  const bottom = height - 20;
  const topY = 8;
  const min = 60;
  const max = 150;
  const X = (day: string) => LEFT + (dayIndex(from, day) * (width - LEFT - 14)) / days;
  const Y = (v: number) => bottom - ((v - min) / (max - min)) * (bottom - topY);
  return (
    <svg
      className={styles.svg}
      viewBox={`0 0 ${width} ${height}`}
      role="img"
      aria-label={sentence}
      data-chart="pressure"
    >
      {[bandTop, bandBottom].map((band) =>
        band ? (
          <rect
            key={band.low}
            className={styles.band}
            x={0}
            y={Y(band.high)}
            width={width}
            height={Math.max(1, Y(band.low) - Y(band.high))}
            rx={3}
          />
        ) : null,
      )}
      {readings.map((r) => (
        <g key={`${r.day}-${r.top}-${r.bottom}`}>
          <line
            className={styles.pair}
            x1={X(r.day)}
            x2={X(r.day)}
            y1={Y(r.top)}
            y2={Y(r.bottom)}
          />
          <circle className={styles.dot} cx={X(r.day)} cy={Y(r.top)} r={3.5} />
          <circle className={styles.dot} cx={X(r.day)} cy={Y(r.bottom)} r={3.5} />
        </g>
      ))}
      {ring
        ? readings
            .filter((r) => r.day === ring)
            .slice(0, 1)
            .map((r) => <DayRing key={r.day} x={X(r.day)} y={Y(r.top)} />)
        : null}
      <DayMarkers at={journalDays.filter((d) => d >= from && d <= to).map(X)} baseline={bottom} />
      {labels.map((l) => (
        <text key={l.day} className={styles.axis} x={X(l.day)} y={height - 2} textAnchor="middle">
          {l.text}
        </text>
      ))}
      {[80, 120].map((v) => (
        <text key={v} className={styles.axis} x={0} y={Y(v) + 3}>
          {num(v)}
        </text>
      ))}
    </svg>
  );
}
