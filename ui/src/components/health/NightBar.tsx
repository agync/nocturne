import { markLabel, marks } from "./axis";
import styles from "./Charts.module.css";
import { clockHours, nightHours } from "./derive";

interface NightBarProps {
  /** The asleep periods, both ends RFC 3339. Gaps between them are awake. */
  periods: ReadonlyArray<{ from: string; to: string }>;
  /** The person's usual bedtime and waking time, `HH:MM`. */
  usualBed?: string | null;
  usualWake?: string | null;
  /** The chart's sentence: its accessible name. */
  sentence: string;
  width?: number;
  /** Small, with every other hour marked: the side block and the phone. */
  compact?: boolean;
}

const TRACK = 16;

/** A track from 8 pm to noon with the asleep periods in the accent colour and the awake gaps as breaks. */
export function NightBar({
  periods,
  usualBed,
  usualWake,
  sentence,
  width = 520,
  compact = false,
}: NightBarProps) {
  const px = width / TRACK;
  const y = 8;
  const bh = compact ? 10 : 12;
  const x = (hours: number) => Math.min(width, Math.max(0, hours * px));
  return (
    <svg
      className={styles.svg}
      viewBox={`0 0 ${width} ${y + bh + 20}`}
      role="img"
      aria-label={sentence}
      data-chart="night"
    >
      <rect className={styles.track} x={0} y={y} width={width} height={bh} rx={bh / 2} />
      {periods.map((p) => (
        <rect
          key={p.from}
          className={styles.bar}
          x={x(nightHours(p.from))}
          y={y}
          width={Math.max(1, x(nightHours(p.to)) - x(nightHours(p.from)))}
          height={bh}
          rx={3}
        />
      ))}
      {[usualBed, usualWake].map((hhmm) =>
        hhmm ? (
          <line
            key={hhmm}
            className={styles.usual}
            x1={x(clockHours(hhmm))}
            x2={x(clockHours(hhmm))}
            y1={y - 4}
            y2={y + bh + 4}
          />
        ) : null,
      )}
      {marks(compact).map((m) => (
        <text key={m.at} className={styles.axis} x={m.at * px} y={y + bh + 16} textAnchor="middle">
          {markLabel(m.hour)}
        </text>
      ))}
    </svg>
  );
}
