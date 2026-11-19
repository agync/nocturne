import type { CheckIn, FeelingPart } from "../engine/bindings";
import styles from "./FeelingMap.module.css";
import { FeelingMark } from "./FeelingMark";

const FILL: Record<FeelingPart, string> = {
  woundUp: "var(--part-wound)",
  litUp: "var(--part-lit)",
  low: "var(--part-low)",
  settled: "var(--part-settled)",
};

interface FeelingMapProps {
  checkins: readonly CheckIn[];
  /** Width and height in pixels. */
  size?: number;
  /** Shows the part marks in the four corners, as the larger fields do. */
  corners?: boolean;
  /** Each dot opens its check-in; without it the dots are decorative. */
  onOpen?: (checkin: CheckIn) => void;
  /** Names a dot for assistive technology, for example "Low, tired, 8:05 am". */
  dotLabel?: (checkin: CheckIn) => string;
  className?: string;
}

/**
 * The feelings field, read only: four tinted parts and a dot where each check-in was placed,
 * carrying its part's shape so colour never carries the meaning alone. Dots are receipts.
 */
export function FeelingMap({
  checkins,
  size = 220,
  corners = true,
  onOpen,
  dotLabel,
  className,
}: FeelingMapProps) {
  const dot = Math.max(14, Math.round(size / 10));
  return (
    <div
      className={[styles.map, className].filter(Boolean).join(" ")}
      style={{ width: size, height: size }}
    >
      {(["woundUp", "litUp", "low", "settled"] as const).map((part) => (
        <div key={part} className={styles.part} style={{ background: FILL[part] }} data-part={part}>
          {corners ? (
            <span className={styles.corner}>
              <FeelingMark mark={part} size={Math.max(10, Math.round(size / 18))} />
            </span>
          ) : null}
        </div>
      ))}
      {checkins.map((c) => {
        const left = ((c.feeling.point.pleasant + 1) / 2) * 100;
        const top = ((1 - c.feeling.point.energy) / 2) * 100;
        const style = { left: `${left}%`, top: `${top}%`, width: dot, height: dot };
        const mark = <FeelingMark mark={c.feeling.part} size={Math.round(dot * 0.62)} />;
        return onOpen ? (
          <button
            key={c.id}
            type="button"
            className={styles.dot}
            style={style}
            aria-label={dotLabel?.(c)}
            onClick={() => onOpen(c)}
          >
            {mark}
          </button>
        ) : (
          <span key={c.id} className={styles.dot} style={style} aria-hidden="true">
            {mark}
          </span>
        );
      })}
    </div>
  );
}
