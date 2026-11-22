import { Button } from "../../components/Button/Button";
import { Mark } from "../../components/Mark/Mark";
import { t } from "../../copy/t";
import styles from "./Setup.module.css";
import { useArrivalFocus } from "./useArrivalFocus";

/** Before step 1: a phone that already holds a memory can bring it here instead. */
export function PhoneQuestion({ onYes, onNo }: { onYes: () => void; onNo: () => void }) {
  const heading = useArrivalFocus();
  return (
    <section className={`${styles.card} ${styles.night}`} aria-labelledby="phone-question">
      <Mark state="resting" size={36} decorative />
      <h1
        ref={heading}
        tabIndex={-1}
        id="phone-question"
        className={`t-page-title ${styles.welcomeTitle}`}
      >
        {t("moving.setup.title")}
      </h1>
      <div className={styles.welcomeFoot}>
        <div className={styles.welcomeButtons}>
          <Button variant="primary" onClick={onYes}>
            {t("moving.setup.yes")}
          </Button>
          <Button variant="secondary" onClick={onNo}>
            {t("moving.setup.no")}
          </Button>
        </div>
      </div>
    </section>
  );
}
