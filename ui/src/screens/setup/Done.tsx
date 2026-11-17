import { ActionBar, ActionBarHost } from "../../components/ActionBar/ActionBar";
import { formatDayMonth } from "../../copy/format";
import { t } from "../../copy/t";
import styles from "./Setup.module.css";
import { useArrivalFocus } from "./useArrivalFocus";

/** Step 3: nothing left to do. */
export function Done({ restoredAt = null }: { restoredAt?: string | null }) {
  const heading = useArrivalFocus();
  return (
    <section className={styles.card} aria-labelledby="done">
      <ActionBarHost>
        <p className={`t-section ${styles.eyebrow}`}>{t("setup.step.three")}</p>
        <h1 ref={heading} tabIndex={-1} id="done" className={`t-page-title ${styles.title}`}>
          {t("setup.done.title")}
        </h1>
        <p className={styles.lead}>
          {restoredAt
            ? t("setup.done.body.restored", { day: formatDayMonth(restoredAt) })
            : t("setup.done.body")}
        </p>
        <ActionBar>
          <a className={styles.open} href="#/today">
            {t("setup.done.open")}
          </a>
        </ActionBar>
      </ActionBarHost>
    </section>
  );
}
