import { formatBytes, formatDayMonth } from "../../copy/format";
import { t } from "../../copy/t";
import type { OnlineView } from "../../engine/bindings";
import { usdText } from "./limits";
import styles from "./Online.module.css";
import { dayText, sentence, whenText } from "./when";

/** What this month has used. Money appears only when the provider reports cost on its answers. */
function Month({ view }: { view: OnlineView }) {
  const { limit, questionsThisMonth: count } = view;
  const reset = dayText(view.resetsOn);
  const lastSent = view.lastSentAt ? whenText(view.lastSentAt) : t("online.month.lastNever");
  const reached = view.state.type === "paused" && view.state.pause.type === "limit";
  const money = view.costReported && view.spentThisMonthUsd !== null;
  const spent = view.spentThisMonthUsd ?? 0;
  const questionLimit = limit.kind === "questions" ? limit.questions : null;
  const usdLimit = limit.kind === "usd" ? (limit.usd ?? null) : null;

  let share = 0;
  if (money && usdLimit) share = spent / usdLimit;
  else if (!money && questionLimit) share = count / questionLimit;
  const percent = Math.min(100, Math.round(share * 100));
  const metered = money ? usdLimit !== null : questionLimit !== null;

  let meterLabel = "";
  if (money && usdLimit !== null) {
    meterLabel = t("online.month.meterUsd", { spent: usdText(spent), limit: usdText(usdLimit) });
  } else if (questionLimit !== null) {
    meterLabel = t("online.month.meter", { count, limit: questionLimit });
  }

  let note: string;
  if (limit.kind === "none") note = t("online.month.nolimit", { day: reset });
  else if (money) note = t("online.month.usd.note", { day: reset });
  else {
    note = t(reached ? "online.month.all" : "online.month.of", {
      limit: questionLimit ?? 0,
      day: reset,
    });
  }

  let unit: string;
  if (!money) unit = t("online.month.questions");
  else if (usdLimit !== null) unit = t("online.month.spent", { limit: usdText(usdLimit) });
  else unit = t("online.month.spent.nolimit");

  return (
    <div className={styles.month}>
      <span className={styles.big}>
        <span className={styles.bigNumber}>{money ? usdText(spent) : count}</span>
        <span className={styles.body}>{unit}</span>
      </span>
      {metered ? (
        <div className={styles.meter} role="img" data-full={reached} aria-label={meterLabel}>
          <span className={styles.meterFill} style={{ width: `${percent}%` }} />
        </div>
      ) : null}
      <span className={styles.sub}>{note}</span>
      <dl className={styles.facts}>
        {money ? (
          <div>
            <dt>{t("online.month.sentCount")}</dt>
            <dd>{count}</dd>
          </div>
        ) : null}
        <div>
          <dt>{t("online.month.lastSent")}</dt>
          <dd>{lastSent}</dd>
        </div>
        {money ? null : (
          <div>
            <dt>{t("online.month.cost")}</dt>
            <dd className={styles.value}>{t("online.month.notTold")}</dd>
          </div>
        )}
      </dl>
      <p className={styles.sub}>
        {money ? t("online.month.noBalance") : sentence(t("online.month.notTold.note"))}
      </p>
    </div>
  );
}

/** How much has been sent, counted by Nocturne. */
function SentBox({ view }: { view: OnlineView }) {
  const since = view.onSince ?? view.lastOnAt;
  return (
    <div className={styles.sentBox}>
      <span className={styles.label}>{t("online.sent.heading")}</span>
      <span>
        {view.sentBytes > 0 ? (
          <>
            <strong>{formatBytes(view.sentBytes)}</strong>
            {since ? ` ${t("online.sent.since", { day: formatDayMonth(since) })}` : ""}
          </>
        ) : (
          t("online.sent.nothing")
        )}
      </span>
      <span className={styles.sub}>{t("online.sent.counted")}</span>
    </div>
  );
}

/** The aside of the page while it is on, paused or in trouble. */
export function MonthAside({ view }: { view: OnlineView }) {
  return (
    <>
      <h2 className={styles.label}>{t("online.month.heading")}</h2>
      <Month view={view} />
      <SentBox view={view} />
    </>
  );
}
