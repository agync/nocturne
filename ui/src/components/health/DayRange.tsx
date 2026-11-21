import styles from "./Charts.module.css";
import { num } from "./sentence";

interface DayRangeProps {
  low: number;
  high: number;
  /** Where most readings sat: a short upright mark. */
  middle: number;
  min?: number;
  max?: number;
  sentence: string;
  width?: number;
}

/** A track from 40 to 180 with the day's lowest to highest as a bar and the middle as an upright mark. */
export function DayRange({
  low,
  high,
  middle,
  min = 40,
  max = 180,
  sentence,
  width = 520,
}: DayRangeProps) {
  const X = (v: number) => ((Math.min(max, Math.max(min, v)) - min) / (max - min)) * width;
  const ticks = [min, 80, 120, 160];
  return (
    <svg
      className={styles.svg}
      viewBox={`0 0 ${width} 40`}
      role="img"
      aria-label={sentence}
      data-chart="range"
    >
      <rect className={styles.track} x={0} y={6} width={width} height={12} rx={6} />
      <rect
        className={styles.bar}
        x={X(low)}
        y={6}
        width={Math.max(2, X(high) - X(low))}
        height={12}
        rx={4}
      />
      <line className={styles.mid} x1={X(middle)} x2={X(middle)} y1={4} y2={20} />
      {ticks.map((v) => (
        <text
          key={v}
          className={styles.axis}
          x={X(v)}
          y={36}
          textAnchor={v === min ? "start" : "middle"}
        >
          {num(v)}
        </text>
      ))}
    </svg>
  );
}
