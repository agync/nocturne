import { useState } from "react";
import { goTo } from "../../app/query";
import { Button } from "../../components/Button/Button";
import { tc } from "../../components/ControlChip/copy";
import { Icon } from "../../components/Icon/Icon";
import { type CopyId, t } from "../../copy/t";
import { useEngine } from "../../engine/context";
import type { StepProps } from "./draft";
import { limitText, usdText } from "./limits";
import styles from "./Online.module.css";
import { Example, Frame } from "./parts";
import { StepLabel } from "./SetupBits";
import { useOnlineAct } from "./useOnline";

const NEVER: CopyId[] = [
  "online.consent.never.photos",
  "online.consent.never.private",
  "online.consent.never.sensitive",
  "online.consent.never.rest",
];

/** O6: everything that happens once it is on, then the one button that turns it on. */
export function Consent({
  draft,
  provider,
  phonesAvailable,
  onGo,
  onHandOver,
  onLeave,
}: StepProps & {
  onGo: (to: "connect" | "send" | "limit") => void;
  onHandOver: () => void;
  onLeave: () => void;
}) {
  const engine = useEngine();
  const { run, busy } = useOnlineAct();
  const [failed, setFailed] = useState<false | "error" | "declined">(false);
  const { limit } = draft;

  const turnOn = async () => {
    setFailed(false);
    let why: "error" | "declined" = "error";
    const done = await run(() => engine.turnOnOnline(draft), {
      quiet: true,
      onError: (code) => {
        if (code === "declined") why = "declined";
      },
    });
    if (!done) {
      setFailed(why);
      return;
    }
    onHandOver();
    goTo("/settings/online");
  };

  let mind: string;
  if (limit.kind === "none") mind = t("online.consent.mind.none");
  else if (limit.kind === "usd") {
    mind = t("online.consent.mind.money", { limit: usdText(limit.usd ?? 0) });
  } else mind = t("online.consent.mind.limit", { limit: limit.questions });

  const summary: Array<{ label: string; value: string; to?: "connect" | "send" | "limit" }> = [
    { label: t("online.consent.aside.service"), value: provider.host, to: "connect" },
    { label: t("online.consent.aside.using"), value: provider.model },
    {
      label: t("online.consent.aside.here"),
      value: t("online.consent.aside.online"),
      to: "send",
    },
  ];
  if (phonesAvailable) {
    summary.push({
      label: t("online.consent.aside.phones"),
      value: t(draft.phones ? "online.consent.aside.online" : "online.consent.aside.thisComputer"),
      to: "send",
    });
  }
  summary.push(
    {
      label: t("online.consent.aside.sensitive"),
      value: t(
        draft.sendSensitive ? "online.consent.aside.mayGo" : "online.consent.aside.keptHere",
      ),
      to: "send",
    },
    { label: t("online.consent.aside.private"), value: t("online.consent.aside.neverSent") },
    { label: t("online.consent.aside.limit"), value: limitText(limit), to: "limit" },
  );

  return (
    <Frame
      back={{ to: "/settings/online", label: t("screen.online.title") }}
      aside={
        <>
          <h2 className={styles.label}>{t("online.consent.aside.heading")}</h2>
          <dl className={styles.sum}>
            {summary.map((row) => (
              <div key={row.label}>
                <dt>{row.label}</dt>
                <dd>
                  <span
                    className={
                      row.label === t("online.consent.aside.using") ? styles.mono : undefined
                    }
                  >
                    {row.value}
                  </span>
                  {row.to ? (
                    <button
                      type="button"
                      className={`${styles.linkButton} hit`}
                      aria-label={`${t("online.consent.aside.change")}: ${row.label}`}
                      onClick={() => onGo(row.to ?? "connect")}
                    >
                      {t("online.consent.aside.change")}
                    </button>
                  ) : null}
                </dd>
              </div>
            ))}
          </dl>
          <p className={styles.sub}>{t("online.consent.aside.nothingYet")}</p>
        </>
      }
    >
      <StepLabel step={5} />
      <h1 className={styles.stepTitle} tabIndex={-1}>
        {t("online.consent.title")}
      </h1>
      <p className={styles.lede}>{t("online.consent.lead")}</p>

      <div className={styles.sec}>
        <h2>{t("online.consent.sent.heading")}</h2>
        <p className={styles.body}>{t("online.consent.sent.body")}</p>
        <Example short />
      </div>

      <div className={styles.sec}>
        <h2>{t("online.consent.goes.heading")}</h2>
        <div
          className={styles.flow}
          role="img"
          aria-label={t("online.consent.goes.label", { host: provider.host })}
        >
          <div className={styles.node}>
            <span className={styles.nodeName}>{t("online.consent.goes.here")}</span>
            {t("online.consent.goes.finds")}
          </div>
          <span className={styles.arrow} aria-hidden="true">
            →
          </span>
          <div className={styles.node} data-out="true">
            <span className={styles.nodeName}>{provider.host}</span>
            {t("online.consent.goes.writes")}
          </div>
          <span className={styles.arrow} aria-hidden="true">
            →
          </span>
          <div className={styles.node}>
            <span className={styles.nodeName}>{t("online.consent.goes.here")}</span>
            {t("online.consent.goes.checks")}
          </div>
        </div>
        <p className={styles.body}>{t("online.consent.goes.body")}</p>
      </div>

      <div className={styles.sec}>
        <h2>{t("online.consent.sees.heading")}</h2>
        <p className={styles.body}>
          <span className={styles.strong}>
            {t("online.consent.sees.body", { host: provider.host, used: provider.model })}
          </span>
        </p>
        <p className={styles.body}>{t("online.consent.sees.keeps")}</p>
      </div>

      <div className={styles.sec}>
        <h2>{t("online.consent.never.heading")}</h2>
        <ul className={styles.never}>
          {NEVER.map((line) => (
            <li key={line}>
              <Icon name="lock" size={16} />
              {t(line)}
            </li>
          ))}
        </ul>
      </div>

      <div className={styles.sec}>
        <h2>{t("online.consent.mind.heading")}</h2>
        <p className={styles.body}>
          {mind} {tc("online.consent.mind.rest")}
        </p>
        <p className={styles.body}>{tc("online.consent.mind.where")}</p>
      </div>

      {failed ? (
        <p role="alert" className={styles.alert}>
          {t(failed === "declined" ? "online.consent.declined" : "online.consent.failed")}
        </p>
      ) : null}
      <div className={styles.footer}>
        {failed === "declined" ? (
          <Button
            variant="primary"
            onClick={() => goTo("/settings/advanced/online", { from: "online" })}
          >
            {t("online.consent.toAdvanced")}
          </Button>
        ) : (
          <Button variant="primary" disabled={busy} aria-busy={busy} onClick={() => void turnOn()}>
            {busy
              ? t("online.consent.turningOn")
              : t(failed ? "online.consent.tryAgain" : "online.consent.turnOn")}
          </Button>
        )}
        <Button variant="quiet" onClick={onLeave}>
          {t("online.notNow")}
        </Button>
      </div>
    </Frame>
  );
}
