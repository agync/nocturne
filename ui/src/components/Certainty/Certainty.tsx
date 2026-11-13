import { t } from "../../copy/t";
import type { Certainty as Level } from "../../engine/bindings";
import styles from "./Certainty.module.css";

const FILLED: Record<Level, number> = { notSure: 1, fairlySure: 2, sure: 3 };

interface CertaintyProps {
  level: Level;
  /** Adds the plain reason after the label, as in the design foundation. */
  showHint?: boolean;
}

/** Three bars and a label. Never a percentage. */
export function Certainty({ level, showHint = false }: CertaintyProps) {
  const filled = FILLED[level];
  return (
    <span className={styles.certainty} data-level={level}>
      <span className={styles.bars} aria-hidden="true">
        {[1, 2, 3].map((n) => (
          <span key={n} className={styles.bar} data-filled={n <= filled} />
        ))}
      </span>
      <span>
        {t(`certainty.${level}`)}
        {showHint ? ` · ${t(`certainty.${level}.hint`)}` : ""}
      </span>
    </span>
  );
}
