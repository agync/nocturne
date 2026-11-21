import { useEffect, useRef, useState } from "react";
import { markLabel, marks } from "./axis";
import styles from "./Charts.module.css";
import { DayLink } from "./DayMarkers";
import { clockHours, type Night, nightHours } from "./derive";
import { dayLabel } from "./sentence";

interface NightsStripProps {
  nights: readonly Night[];
  usual?: { bed: string; wake: string } | null;
  sentence: string;
  /** Drawn at this width. Left out, the strip is drawn at the width of its box, so its text stays its size. */
  width?: number;
  /** The night drawn in the ink colour: the one being looked at. */
  current?: string;
  /** Each night's row opens its readings for a pointer; the list holds the same receipts. */
  href?: (day: string) => string;
}

const LABEL = 52;
const ROW = 22;
const FALLBACK = 860;
const LEAST = 280;

/** The width of a box, kept up to date as it is resized. */
function useBoxWidth(): [React.RefObject<HTMLDivElement | null>, number | null] {
  const box = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState<number | null>(null);
  useEffect(() => {
    const el = box.current;
    if (!el) return;
    const read = () => {
      const w = Math.round(el.getBoundingClientRect().width);
      if (w > 0) setWidth(w);
    };
    read();
    if (typeof ResizeObserver === "undefined") return;
    const watch = new ResizeObserver(read);
    watch.observe(el);
    return () => watch.disconnect();
  }, []);
  return [box, width];
}

/** One night bar per row, the same track, the day at the left and a faint band where the usual night falls. */
export function NightsStrip({ nights, usual, sentence, width, current, href }: NightsStripProps) {
  const [box, measured] = useBoxWidth();
  const drawn = Math.max(LEAST, width ?? measured ?? FALLBACK);
  const span = drawn - LABEL;
  const px = span / 16;
  const x = (hours: number) => LABEL + Math.min(16, Math.max(0, hours)) * px;
  const top = 4;
  const axisY = top + nights.length * ROW + 14;
  return (
    <div ref={box} className={`${styles.nights} ${styles.frame}`}>
      <svg
        className={styles.svg}
        viewBox={`0 0 ${drawn} ${axisY + 4}`}
        width={drawn}
        height={axisY + 4}
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
                <circle className={styles.entry} cx={drawn - 4} cy={y + 5} r={2.5} />
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
      {href ? (
        <svg
          className={styles.overlay}
          viewBox={`0 0 ${drawn} ${axisY + 4}`}
          aria-hidden="true"
          focusable="false"
        >
          {nights.map((night, i) => (
            <DayLink key={night.day} href={href(night.day)} day={night.day}>
              <rect className={styles.hit} x={0} y={top + i * ROW} width={drawn} height={ROW} />
            </DayLink>
          ))}
        </svg>
      ) : null}
    </div>
  );
}
