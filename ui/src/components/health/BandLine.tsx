import { t } from "../../copy/t";
import type { Band, SeriesPoint } from "../../engine/bindings";
import styles from "./Charts.module.css";
import { DayLink, DayMarkers, DayRing } from "./DayMarkers";
import { dayIndex } from "./derive";

interface BandLineProps {
  points: readonly SeriesPoint[];
  from: string;
  to: string;
  /** The usual band, or none before two weeks of readings. */
  band: Band | null;
  /** The floor and ceiling of the scale. A sensible floor for the kind, never cropped to a change. */
  min: number;
  max: number;
  /** Days with a journal entry. */
  journalDays?: readonly string[];
  /** The day looked at: a filled dot with its value. Defaults to the last point. */
  at?: string;
  /** A ring round a day, such as the one noticing counted. */
  ring?: string;
  format?: (value: number) => string;
  sentence: string;
  width?: number;
  height?: number;
  /** Labels under the axis: the first day, the middle and today. */
  labels?: ReadonlyArray<{ day: string; text: string }>;
  /** Each day's point opens its readings for a pointer; the list holds the same receipts. */
  href?: (day: string) => string;
}

const PAD = 8;

/** A thin line in the ink colour on the person's usual band; today as a filled dot with its value. */
export function BandLine({
  points,
  from,
  to,
  band,
  min,
  max,
  journalDays = [],
  at,
  ring,
  format = String,
  sentence,
  width = 520,
  height = 110,
  labels = [],
  href,
}: BandLineProps) {
  const days = Math.max(1, dayIndex(from, to));
  const bottom = height - 22;
  const topY = 14;
  const X = (day: string) => PAD + (dayIndex(from, day) * (width - PAD * 2)) / days;
  const Y = (v: number) => bottom - ((v - min) / (max - min)) * (bottom - topY);
  const sorted = [...points].sort((a, b) => a.day.localeCompare(b.day));
  const focus = sorted.find((p) => p.day === at) ?? sorted[sorted.length - 1];
  const journal = journalDays.filter((d) => d >= from && d <= to);
  return (
    <div className={styles.frame}>
      <svg
        className={styles.svg}
        viewBox={`0 0 ${width} ${height}`}
        role="img"
        aria-label={sentence}
        data-chart="line"
      >
        {band ? (
          <>
            <rect
              className={styles.band}
              x={PAD}
              y={Y(band.high)}
              width={width - PAD * 2}
              height={Math.max(1, Y(band.low) - Y(band.high))}
              rx={3}
            />
            <text className={styles.axis} x={PAD + 4} y={Y(band.high) - 4}>
              {t("health.usual.axis")}
            </text>
          </>
        ) : null}
        {sorted.length > 1 ? (
          <polyline
            className={styles.line}
            points={sorted.map((p) => `${X(p.day)},${Y(p.value)}`).join(" ")}
          />
        ) : null}
        <DayMarkers at={journal.map(X)} baseline={bottom} />
        {focus ? (
          <>
            <circle className={styles.dot} cx={X(focus.day)} cy={Y(focus.value)} r={4} />
            <text
              className={styles.value}
              x={X(focus.day) - 8}
              y={Y(focus.value) - 9}
              textAnchor="end"
            >
              {format(focus.value)}
            </text>
          </>
        ) : null}
        {ring
          ? (() => {
              const p = sorted.find((q) => q.day === ring);
              return p ? <DayRing x={X(p.day)} y={Y(p.value)} /> : null;
            })()
          : null}
        {labels.map((l) => (
          <text key={l.day} className={styles.axis} x={X(l.day)} y={height - 2} textAnchor="middle">
            {l.text}
          </text>
        ))}
      </svg>
      {href ? (
        <svg
          className={styles.overlay}
          viewBox={`0 0 ${width} ${height}`}
          aria-hidden="true"
          focusable="false"
        >
          {href
            ? sorted.map((p) => (
                <DayLink key={p.day} href={href(p.day)} day={p.day}>
                  <circle className={styles.hit} cx={X(p.day)} cy={Y(p.value)} r={9} />
                </DayLink>
              ))
            : null}
        </svg>
      ) : null}
    </div>
  );
}
