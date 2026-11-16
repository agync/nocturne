import { useId, useState } from "react";
import { Button } from "../../components/Button/Button";
import { tc } from "../../components/ControlChip/copy";
import { Chevron } from "../../components/Disclosure/Chevron";
import { t } from "../../copy/t";
import styles from "./Online.module.css";
import { Example, Frame } from "./parts";
import { StepLabel } from "./SetupBits";

/** O2: what changes, what is sent, who sees it, what it costs and what is needed. */
export function Explainer({ onNext, onLeave }: { onNext: () => void; onLeave: () => void }) {
  const [open, setOpen] = useState(false);
  const exampleId = useId();
  const rows = [
    { label: t("online.explain.changes.label"), body: t("online.explain.changes.body") },
    { label: t("online.explain.never.label"), body: t("online.explain.never.body") },
    { label: t("online.explain.sees.label"), body: t("online.explain.sees.body") },
    { label: t("online.explain.costs.label"), body: t("online.explain.costs.body") },
    { label: t("online.explain.needs.label"), body: t("online.explain.needs.body") },
  ];
  const stays = [
    t("online.explain.aside.finding"),
    t("online.explain.aside.keeping"),
    t("online.explain.aside.tidy"),
    t("online.explain.aside.receipts"),
  ];
  return (
    <Frame
      back={{ to: "/settings/online", label: t("screen.online.title") }}
      aside={
        <>
          <h2 className={styles.label}>{t("online.explain.aside.heading")}</h2>
          <ul className={styles.rules}>
            {stays.map((line) => (
              <li key={line}>{line}</li>
            ))}
          </ul>
          <p className={styles.sub}>{t("online.explain.aside.fallback")}</p>
          <p className={styles.sub} style={{ paddingTop: 12, borderTop: "1px solid var(--rule)" }}>
            {tc("online.explain.aside.off")}
          </p>
        </>
      }
    >
      <StepLabel step={1} />
      <h1 className={styles.stepTitle} tabIndex={-1}>
        {t("online.explain.title")}
      </h1>
      <p className={styles.body}>{t("online.explain.lead")}</p>
      <dl className={styles.dl}>
        <div>
          <dt>{t("online.explain.sent.label")}</dt>
          <dd>
            <span>{t("online.explain.sent.body")}</span>
            <button
              type="button"
              className={styles.toggleLink}
              aria-expanded={open}
              aria-controls={exampleId}
              onClick={() => setOpen(!open)}
            >
              <Chevron size={14} />
              {t("online.explain.exactly")}
            </button>
            {open ? (
              <div id={exampleId}>
                <p className={styles.exampleNote} style={{ marginBottom: 8 }}>
                  {t("online.example.intro")}
                </p>
                <Example />
              </div>
            ) : null}
          </dd>
        </div>
        {rows.map((row) => (
          <div key={row.label}>
            <dt>{row.label}</dt>
            <dd>{row.body}</dd>
          </div>
        ))}
      </dl>
      <p className={`${styles.body} ${styles.strong}`}>{t("online.explain.closing")}</p>
      <div className={styles.buttons}>
        <Button variant="primary" onClick={onNext}>
          {t("online.continue")}
        </Button>
        <Button variant="quiet" onClick={onLeave}>
          {t("online.notNow")}
        </Button>
      </div>
    </Frame>
  );
}
