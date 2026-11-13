import { t } from "../../copy/t";
import type { Job, ModelsView } from "../../engine/bindings";
import { gb, JOB_SHORT, jobOf } from "./jobs";
import styles from "./Models.module.css";

/** Reading aloud runs on its own, so the busiest set is these three. */
const SET: Job[] = ["answering", "finding", "recordings"];

/** Memory at its busiest, against what Nocturne allows itself; the words say what the bar does. */
export function MemoryMeter({ view }: { view: ModelsView }) {
  const budget = view.budgetBytes;
  const busy = gb(view.busiestBytes);
  const allowed = gb(budget);
  return (
    <div className={styles.section}>
      <span className={styles.label}>{t("models.meter.label")}</span>
      <div
        className={styles.meter}
        role="img"
        aria-label={t("models.meter.aria", { busy, budget: allowed })}
      >
        {SET.map((job, i) => (
          <span
            key={job}
            className={styles.seg}
            data-n={i + 1}
            style={{
              width: `${budget > 0 ? Math.min(100, ((jobOf(view, job)?.memoryBytes ?? 0) / budget) * 100) : 0}%`,
            }}
          />
        ))}
      </div>
      <ul className={styles.legend}>
        {SET.map((job, i) => (
          <li key={job}>
            <span className={styles.swatch} data-n={i + 1} aria-hidden="true" />
            {t("models.meter.legend", {
              job: t(JOB_SHORT[job]),
              memory: gb(jobOf(view, job)?.memoryBytes ?? 0),
            })}
          </li>
        ))}
        <li className={styles.total}>{t("models.meter.of", { busy, budget: allowed })}</li>
      </ul>
      <p className={styles.hint}>{t("models.meter.note", { budget: allowed })}</p>
    </div>
  );
}
