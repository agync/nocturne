import { t } from "../../copy/t";
import type { PrivacyView } from "../../engine/bindings";
import styles from "./Privacy.module.css";

/** Whether Online answers has ever been turned on, so the page says anything about it. */
export function onlineKnown(view: PrivacyView): boolean {
  return view.onlineOn || view.onlineHost !== null || view.onlineLastOnAt !== null;
}

/** What Online answers sends and who sees it, in the words the person is owed. */
export function OnlineSection({ view }: { view: PrivacyView }) {
  if (!onlineKnown(view)) return null;
  const sees = [t("privacy.online.see1"), t("privacy.online.see2"), t("privacy.online.see3")];
  const seesAddress = t("privacy.online.see4");
  const cant = [t("privacy.online.cant1"), t("privacy.online.cant2")];
  return (
    <section aria-labelledby="online" className={styles.block}>
      <h2 id="online" className={styles.blockTitle}>
        {t("privacy.online.heading")}
      </h2>
      {view.onlineOn ? (
        <>
          <p>{t("privacy.online.on", { host: view.onlineHost ?? "" })}</p>
          <div className={styles.awayLists}>
            <div>
              <span className={styles.awayHead}>{t("privacy.online.canSee")}</span>
              <ul>
                {[...sees, seesAddress].map((line) => (
                  <li key={line}>{line}</li>
                ))}
              </ul>
            </div>
            <div>
              <span className={styles.awayHead}>{t("privacy.online.cantSee")}</span>
              <ul>
                {cant.map((line) => (
                  <li key={line}>{line}</li>
                ))}
              </ul>
            </div>
          </div>
          <p className={styles.detail}>{t("privacy.online.keeps")}</p>
        </>
      ) : (
        <p>{t("privacy.online.off")}</p>
      )}
      <div className={styles.awayActions}>
        <a href="#/settings/online/sent">{t("privacy.online.seeSent")}</a>
        {view.onlineOn ? <a href="#/settings/online">{t("privacy.online.turnOff")}</a> : null}
      </div>
    </section>
  );
}
