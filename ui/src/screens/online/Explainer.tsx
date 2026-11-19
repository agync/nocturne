import { ActionBar } from "../../components/ActionBar/ActionBar";
import { Button } from "../../components/Button/Button";
import { tc } from "../../components/ControlChip/copy";
import { Disclosure } from "../../components/Disclosure/Disclosure";
import { t } from "../../copy/t";
import styles from "./Online.module.css";
import { Example, Frame } from "./parts";
import { StepLabel } from "./SetupBits";

/** O2: three short lines, and one expander holding every fact and what stays on this computer. */
export function Explainer({ onNext, onLeave }: { onNext: () => void; onLeave: () => void }) {
  const short = [
    { label: t("online.explain.short.sent.label"), body: t("online.explain.short.sent.body") },
    { label: t("online.explain.short.never.label"), body: t("online.explain.short.never.body") },
    { label: t("online.explain.short.needs.label"), body: t("online.explain.short.needs.body") },
  ];
  const rows = [
    { label: t("online.explain.never.label"), body: t("online.explain.never.body") },
    { label: t("online.explain.sees.label"), body: t("online.explain.sees.body") },
    { label: t("online.explain.changes.label"), body: t("online.explain.changes.body") },
    { label: t("online.explain.costs.label"), body: t("online.explain.costs.body") },
    { label: t("online.explain.needs.label"), body: t("online.explain.needs.body") },
  ];
  const stays = [
    t("online.explain.aside.finding"),
    t("online.explain.aside.keeping"),
    t("online.explain.aside.tidy"),
    t("online.explain.aside.receipts"),
    t("journal.online.step1.more.staysHere"),
  ];
  return (
    <Frame back={{ to: "/settings/online", label: t("screen.online.title") }}>
      <StepLabel step={1} />
      <h1 className={styles.stepTitle} tabIndex={-1}>
        {t("online.explain.title")}
      </h1>
      <dl className={`${styles.dl} ${styles.short}`}>
        {short.map((row) => (
          <div key={row.label}>
            <dt>{row.label}</dt>
            <dd>{row.body}</dd>
          </div>
        ))}
      </dl>
      <Disclosure label={t("online.explain.more")}>
        <dl className={styles.dl}>
          <div>
            <dt>{t("online.explain.sent.label")}</dt>
            <dd>
              <span>{t("online.explain.sent.body")}</span>
              <span className={styles.exampleNote}>{t("online.example.intro")}</span>
              <Example />
            </dd>
          </div>
          {rows.map((row) => (
            <div key={row.label}>
              <dt>{row.label}</dt>
              <dd>{row.body}</dd>
            </div>
          ))}
        </dl>
        <h2 className={styles.label}>{t("online.explain.aside.heading")}</h2>
        <ul className={styles.rules}>
          {stays.map((line) => (
            <li key={line}>{line}</li>
          ))}
        </ul>
        <p className={styles.sub}>{t("online.explain.aside.fallback")}</p>
        <p className={styles.sub}>{tc("online.explain.aside.off")}</p>
      </Disclosure>
      <ActionBar line={t("online.bar.line")}>
        <Button variant="primary" onClick={onNext}>
          {t("online.continue")}
        </Button>
        <Button variant="quiet" onClick={onLeave}>
          {t("online.notNow")}
        </Button>
      </ActionBar>
    </Frame>
  );
}
