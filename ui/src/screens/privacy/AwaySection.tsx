import { Button } from "../../components/Button/Button";
import { t } from "../../copy/t";
import type { PrivacyView } from "../../engine/bindings";
import { useEngine } from "../../engine/context";
import { useRemoteAct } from "../away/useRemote";
import { whenText } from "../away/when";
import styles from "./Privacy.module.css";

/** What Away from home sends and who sees it, in the words the person is owed. */
export function AwaySection({ view }: { view: PrivacyView }) {
  const engine = useEngine();
  const act = useRemoteAct();
  if (!view.remoteOn && !view.remoteLastOnAt) return null;

  if (!view.remoteOn) {
    return (
      <section aria-labelledby="away" className={styles.block}>
        <h2 id="away" className={styles.blockTitle}>
          {t("privacy.away.heading")}
        </h2>
        <p>
          {view.remoteLastOnAt
            ? t("privacy.away.lastOn", { when: whenText(view.remoteLastOnAt) })
            : t("privacy.away.off")}
        </p>
        <p>
          <a href="#/settings/away">{t("privacy.away.settings")}</a>
        </p>
      </section>
    );
  }

  const simple = view.remoteConnection !== "mostPrivate";
  const company = t(simple ? "away.company.simple" : "away.company.mostPrivate");
  const sees = simple
    ? ([
        t("privacy.away.simple.see1"),
        t("privacy.away.simple.see2"),
        t("privacy.away.simple.see3"),
        t("privacy.away.simple.see4"),
        t("privacy.away.simple.see5"),
      ] as const)
    : ([
        t("privacy.away.mostPrivate.see1"),
        t("privacy.away.mostPrivate.see2"),
        t("privacy.away.mostPrivate.see3"),
      ] as const);
  const cant = [t("privacy.away.cant1"), t("privacy.away.cant2"), t("privacy.away.cant3")];

  return (
    <section aria-labelledby="away" className={styles.block}>
      <h2 id="away" className={styles.blockTitle}>
        {t("privacy.away.heading")}
      </h2>
      <p>{t(simple ? "privacy.away.on.simple" : "privacy.away.on.mostPrivate")}</p>
      <div className={styles.awayActions}>
        <Button variant="secondary" onClick={() => void act(() => engine.stopRemote(false))}>
          {t("privacy.away.turnOff")}
        </Button>
        <a href="#/settings/away">{t("privacy.away.settings")}</a>
      </div>
      <h3 className={styles.awaySub}>{t("privacy.away.middle", { company })}</h3>
      <div className={styles.awayLists}>
        <div>
          <span className={styles.awayHead}>{t("privacy.away.canSee")}</span>
          <ul>
            {sees.map((line) => (
              <li key={line}>{line}</li>
            ))}
          </ul>
        </div>
        <div>
          <span className={styles.awayHead}>{t("privacy.away.cantSee")}</span>
          <ul>
            {cant.map((line) => (
              <li key={line}>{line}</li>
            ))}
          </ul>
        </div>
      </div>
      <h3 className={styles.awaySub}>{t("privacy.away.never.heading")}</h3>
      <p>{t("privacy.away.never.body")}</p>
      <h3 className={styles.awaySub}>{t("privacy.away.risks.heading")}</h3>
      <ul>
        <li>{t("privacy.away.risks.phone")}</li>
        <li>{t("privacy.away.risks.code")}</li>
        {simple ? <li>{t("privacy.away.risks.local")}</li> : null}
      </ul>
    </section>
  );
}
