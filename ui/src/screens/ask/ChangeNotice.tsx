import { useEffect, useRef } from "react";
import { Button } from "../../components/Button/Button";
import { Icon } from "../../components/Icon/Icon";
import { t } from "../../copy/t";
import styles from "./Ask.module.css";

/**
 * Shown once, in place of an answer, to someone who turned Online answers on before only what
 * they mark private was held back. Their question waits here until they continue.
 */
export function ChangeNotice({
  question,
  busy,
  failed,
  onContinue,
  onSettings,
}: {
  question: string;
  busy: boolean;
  failed: boolean;
  onContinue: () => void;
  onSettings: () => void;
}) {
  const title = useRef<HTMLHeadingElement>(null);
  useEffect(() => {
    title.current?.focus({ preventScroll: true });
  }, []);
  return (
    <article className={styles.turnNow} aria-labelledby="ask-changed">
      <p className={styles.eyebrow}>{t("ask.eyebrow")}</p>
      <p className={styles.turnQuestion}>{t("ask.question", { question })}</p>
      <section className={styles.changed}>
        <h1 id="ask-changed" ref={title} tabIndex={-1} className={styles.changedTitle}>
          <Icon name="lock" size={18} />
          {t("ask.changed.title")}
        </h1>
        <p>{t("ask.changed.body")}</p>
        <p>{t("ask.changed.private")}</p>
        <p className={styles.changedWaits}>{t("ask.changed.waits")}</p>
        {failed ? (
          <p role="alert" className={styles.changedWaits}>
            {t("ask.changed.failed")}
          </p>
        ) : null}
        <div className={styles.changedButtons}>
          <Button variant="primary" disabled={busy} onClick={onContinue}>
            {t("ask.changed.continue")}
          </Button>
          <Button variant="quiet" onClick={onSettings}>
            {t("ask.changed.settings")}
          </Button>
        </div>
      </section>
    </article>
  );
}
