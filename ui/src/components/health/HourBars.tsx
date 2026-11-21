import type { HourRange } from "../../engine/bindings";
import styles from "./Charts.module.css";
import { hourWord } from "./sentence";

interface HourBarsProps {
  /** Up to 24 hours; an hour with no reading is simply not drawn. */
  hours: readonly HourRange[];
  /** The hours of the day (0 to 23) a workout covers, drawn in the ink colour. */
  workoutHours?: readonly number[];
  min?: number;
  max?: number;
  sentence: string;
  width?: number;
  height?: number;
}

const hourOf = (start: string) => new Date(start).getHours();

/** 24 thin range bars, one per hour, on the same scale as the day range. */
export function HourBars({
  hours,
  workoutHours = [],
  min = 40,
  max = 160,
  sentence,
  width = 520,
  height = 170,
}: HourBarsProps) {
  const step = width / 24;
  const bottom = height - 18;
  const topY = 6;
  const Y = (v: number) =>
    bottom - ((Math.min(max, Math.max(min, v)) - min) / (max - min)) * (bottom - topY);
  return (
    <svg
      className={styles.svg}
      viewBox={`0 0 ${width} ${height}`}
      role="img"
      aria-label={sentence}
      data-chart="hours"
    >
      {hours.map((h) => {
        const hour = hourOf(h.start);
        const x = hour * step + step / 2;
        return (
          <g key={h.start}>
            <rect
              className={styles.bar}
              data-tone={workoutHours.includes(hour) ? "ink" : undefined}
              x={x - 3}
              y={Y(h.high)}
              width={6}
              height={Math.max(2, Y(h.low) - Y(h.high))}
              rx={3}
            />
            <line className={styles.mid} x1={x - 4} x2={x + 4} y1={Y(h.middle)} y2={Y(h.middle)} />
          </g>
        );
      })}
      {[0, 6, 12, 18].map((hour) => (
        <text
          key={hour}
          className={styles.axis}
          x={hour * step + step / 2}
          y={height - 2}
          textAnchor="middle"
        >
          {hourWord(hour)}
        </text>
      ))}
    </svg>
  );
}
