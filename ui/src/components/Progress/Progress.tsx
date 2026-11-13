import { t } from "../../copy/t";
import styles from "./Progress.module.css";

interface ProgressProps {
  value: number;
  max: number;
  /** What is happening, in words: "Getting ready". */
  label: string;
  /** The measure, in words: "4.1 of 11 GB · about 18 min". */
  detail?: string;
  paused?: boolean;
}

/** A bar is never alone: it always comes with the words that say what it means. */
export function Progress({ value, max, label, detail, paused = false }: ProgressProps) {
  const fraction = max > 0 ? Math.min(1, Math.max(0, value / max)) : 0;
  const spoken = [label, paused ? t("progress.paused") : null, detail].filter(Boolean).join(", ");
  return (
    <div className={styles.progress} data-paused={paused}>
      <div className={styles.words}>
        <span className={styles.label}>{label}</span>
        {detail ? <span className={styles.detail}>{detail}</span> : null}
      </div>
      <div
        className={styles.track}
        role="progressbar"
        aria-label={label}
        aria-valuemin={0}
        aria-valuemax={max}
        aria-valuenow={Math.min(max, Math.max(0, value))}
        aria-valuetext={spoken}
      >
        <div className={styles.fill} style={{ width: `${fraction * 100}%` }} />
      </div>
    </div>
  );
}
