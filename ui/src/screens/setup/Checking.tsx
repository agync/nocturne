import { t } from "../../copy/t";
import styles from "./Setup.module.css";

/** H2, first half: the computer is being looked at. Words, not a bar, because nothing is counted. */
export function Checking() {
  return (
    <section className={styles.card} aria-labelledby="checking">
      <p className={`t-section ${styles.eyebrow}`}>{t("setup.step.one")}</p>
      <h1 id="checking" className={`t-page-title ${styles.title}`}>
        {t("setup.checking.title")}
      </h1>
      <p className={styles.lead} role="status">
        <span className={styles.breathe} aria-hidden="true" />
        {t("setup.checking.body")}
      </p>
    </section>
  );
}
