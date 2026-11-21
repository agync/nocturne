import type { SeriesPoint } from "../../engine/bindings";
import styles from "./Charts.module.css";
import { DayLink, DayMarkers } from "./DayMarkers";
import { dayIndex } from "./derive";
import { num } from "./sentence";

interface DayBarsProps {
  points: readonly SeriesPoint[];
  from: string;
  to: string;
  /** The day in the accent colour, with its value above it. Defaults to the last. */
  at?: string;
  journalDays?: readonly string[];
  /** The top of the scale; the floor is always zero. */
  max?: number;
  /** A letter or short word under each day, when there is room. */
  label?: (day: string) => string;
  sentence: string;
  width?: number;
  height?: number;
  /** Each day's bar opens its readings for a pointer; the list holds the same receipts. */
  href?: (day: string) => string;
}

/** One bar per day, today in the accent and the rest in a quiet grey, with the value above today. */
export function DayBars({
  points,
  from,
  to,
  at,
  journalDays = [],
  max,
  label,
  sentence,
  width = 520,
  height = 90,
  href,
}: DayBarsProps) {
  const n = Math.max(1, dayIndex(from, to) + 1);
  const step = width / n;
  const bw = Math.max(1.5, Math.min(28, step * 0.62));
  const bottom = height - 18;
  const topY = 16;
  const top = max ?? Math.max(1, ...points.map((p) => p.value));
  const Y = (v: number) => bottom - (v / top) * (bottom - topY);
  const focus = at ?? points[points.length - 1]?.day;
  const cx = (day: string) => dayIndex(from, day) * step + step / 2;
  return (
    <div className={styles.frame}>
      <svg
        className={styles.svg}
        viewBox={`0 0 ${width} ${height}`}
        role="img"
        aria-label={sentence}
        data-chart="bars"
      >
        {points.map((p) => (
          <g key={p.day}>
            <rect
              className={styles.bar}
              data-tone={p.day === focus ? undefined : "soft"}
              x={cx(p.day) - bw / 2}
              y={Y(p.value)}
              width={bw}
              height={Math.max(0, bottom - Y(p.value))}
              rx={Math.min(3, bw / 2)}
            />
            {label && n <= 16 ? (
              <text className={styles.axis} x={cx(p.day)} y={height - 2} textAnchor="middle">
                {label(p.day)}
              </text>
            ) : null}
          </g>
        ))}
        {points
          .filter((p) => p.day === focus)
          .map((p) => (
            <text
              key={p.day}
              className={styles.value}
              x={cx(p.day)}
              y={Y(p.value) - 5}
              textAnchor="middle"
            >
              {num(p.value)}
            </text>
          ))}
        <DayMarkers
          at={journalDays.filter((d) => d >= from && d <= to).map(cx)}
          baseline={bottom - 4}
        />
      </svg>
      {href ? (
        <svg
          className={styles.overlay}
          viewBox={`0 0 ${width} ${height}`}
          aria-hidden="true"
          focusable="false"
        >
          {href
            ? points.map((p) => (
                <DayLink key={p.day} href={href(p.day)} day={p.day}>
                  <rect
                    className={styles.hit}
                    x={cx(p.day) - step / 2}
                    y={topY - 12}
                    width={step}
                    height={bottom - topY + 12}
                  />
                </DayLink>
              ))
            : null}
        </svg>
      ) : null}
    </div>
  );
}
