import { Button } from "../../components/Button/Button";
import { Mark } from "../../components/Mark/Mark";
import { t } from "../../copy/t";
import styles from "./Setup.module.css";
import { useArrivalFocus } from "./useArrivalFocus";

/** H1: the welcome, at night. */
export function Welcome({ onStart, onRestore }: { onStart: () => void; onRestore: () => void }) {
  const heading = useArrivalFocus();
  return (
    <section className={`${styles.card} ${styles.night}`} aria-labelledby="welcome">
      <Mark state="resting" size={36} decorative />
      <h1
        ref={heading}
        tabIndex={-1}
        id="welcome"
        className={`t-page-title ${styles.welcomeTitle}`}
      >
        {t("setup.welcome.title")}
      </h1>
      <p className={styles.welcomeBody}>{t("setup.welcome.body")}</p>
      <div className={styles.welcomeFoot}>
        <p>
          <strong>{t("setup.welcome.promise")}</strong>
          <span className={styles.detail}>{t("setup.welcome.promise.detail")}</span>
        </p>
        <div className={styles.welcomeButtons}>
          <Button variant="quiet" onClick={onRestore}>
            {t("setup.welcome.restore")}
          </Button>
          <Button variant="primary" onClick={onStart}>
            {t("setup.welcome.start")}
          </Button>
        </div>
      </div>
    </section>
  );
}
