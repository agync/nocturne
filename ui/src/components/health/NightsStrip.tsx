import { markLabel, marks } from "./axis";
import styles from "./Charts.module.css";
import { clockHours, type Night, nightHours } from "./derive";
import { dayLabel } from "./sentence";

interface NightsStripProps {
  nights: readonly Night[];
  usual?: { bed: string; wake: string } | null;
  sentence: string;
  width?: number;
  /** The night drawn in the ink colour: the one being looked at. */
  current?: string;
}

const LABEL = 52;
const ROW = 22;

/** One night bar per row, the same track, the day at the left and a faint band where the usual night falls. */
export function NightsStrip({ nights, usual, sentence, width = 860, current }: NightsStripProps) {
  const span = width - LABEL;
  const px = span / 16;
  const x = (hours: number) => LABEL + Math.min(16, Math.max(0, hours)) * px;
  const top = 4;
  const axisY = top + nights.length * ROW + 14;
  return (
    <svg
      className={styles.svg}
      viewBox={`0 0 ${width} ${axisY + 4}`}
      role="img"
      aria-label={sentence}
      data-chart="nights"
    >
      {usual ? (
        <rect
          className={styles.band}
          x={x(clockHours(usual.bed))}
          y={0}
          width={Math.max(0, x(clockHours(usual.wake)) - x(clockHours(usual.bed)))}
          height={nights.length * ROW + top}
          rx={4}
        />
      ) : null}
      {nights.map((night, i) => {
        const y = top + i * ROW + ROW / 2 - 5;
        return (
          <g key={night.day}>
            <text className={styles.axis} x={0} y={y + 9}>
              {dayLabel(night.day)}
            </text>
            <rect className={styles.track} x={LABEL} y={y} width={span} height={10} rx={5} />
            {night.periods.map((p) => (
              <rect
                key={p.from}
                className={styles.bar}
                data-tone={night.day === current ? "ink" : undefined}
                x={x(nightHours(p.from))}
                y={y}
                width={Math.max(1, x(nightHours(p.to)) - x(nightHours(p.from)))}
                height={10}
                rx={3}
              />
            ))}
            {night.hasEntry ? (
              <circle className={styles.entry} cx={width - 4} cy={y + 5} r={2.5} />
            ) : null}
          </g>
        );
      })}
      {marks(true).map((m) => (
        <text
          key={m.at}
          className={styles.axis}
          x={LABEL + m.at * px}
          y={axisY}
          textAnchor="middle"
        >
          {markLabel(m.hour)}
        </text>
      ))}
    </svg>
  );
}
