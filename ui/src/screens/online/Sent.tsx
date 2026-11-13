import { useQuery } from "@tanstack/react-query";
import { useEffect, useMemo, useState } from "react";
import { useQueryParam } from "../../app/query";
import { BackLink } from "../../components/BackLink/BackLink";
import { Button } from "../../components/Button/Button";
import { ScreenSheet } from "../../components/ScreenSheet/ScreenSheet";
import { formatBytes, formatLongDay, formatTime, localDay } from "../../copy/format";
import { t } from "../../copy/t";
import type { LocalReason, OnlineSend, SentLine } from "../../engine/bindings";
import { useEngine } from "../../engine/context";
import styles from "./Sent.module.css";
import { useOnline } from "./useOnline";

const PAGE = 50;

const money = new Intl.NumberFormat("en-GB", {
  style: "currency",
  currency: "USD",
  maximumFractionDigits: 4,
});

function reasonOf(reason: LocalReason): string {
  switch (reason) {
    case "chosen":
      return t("sent.reason.chosen");
    case "noInternet":
      return t("sent.reason.noInternet");
    case "couldntReach":
      return t("sent.reason.couldntReach");
    case "slow":
      return t("sent.reason.slow");
    case "limit":
      return t("sent.reason.limit");
    case "noCredit":
      return t("sent.reason.noCredit");
    case "keyRefused":
      return t("sent.reason.keyRefused");
    case "keptHere":
      return t("sent.reason.keptHere");
    case "problem":
      return t("sent.reason.problem");
    case "downloading":
      return t("sent.reason.downloading");
  }
}

const isSent = (send: OnlineSend) => send.result.type !== "notSent";

function rowMeta(send: OnlineSend): string {
  const time = formatTime(send.at);
  const size = formatBytes(send.bytesUp);
  switch (send.result.type) {
    case "answered":
      return t(send.kind === "check" ? "sent.row.worked" : "sent.row.answered", { time, size });
    case "notSure":
      return t("sent.row.notSure", { time, size });
    case "failed":
      return t("sent.row.failed", { time, size, reason: reasonOf(send.result.reason) });
    case "notSent":
      return t("sent.row.notSent", { time, reason: reasonOf(send.result.reason) });
  }
}

function resultText(send: OnlineSend): string {
  switch (send.result.type) {
    case "answered":
      return send.kind === "check"
        ? t("sent.result.worked")
        : t("sent.result.answered", { count: send.result.receipts });
    case "notSure":
      return t("sent.result.notSure");
    case "failed":
      return t("sent.result.failed", { reason: reasonOf(send.result.reason) });
    case "notSent":
      return t("sent.result.notSent", { reason: reasonOf(send.result.reason) });
  }
}

const title = (send: OnlineSend) => (send.kind === "check" ? t("sent.check") : send.question);

function dayLabel(day: string, today: string): string {
  return day === today ? t("sent.today") : formatLongDay(day);
}

/** One line of what was sent, with a memory that is gone or changed said in words, never shown. */
function Line({ line }: { line: SentLine }) {
  if (line.type === "text") return <span className={styles.line}>{line.text}</span>;
  const prefix = `[m${line.n}] `;
  if (line.state === "same" && line.text !== null) {
    return (
      <span className={styles.line}>
        {prefix}
        {line.text}
      </span>
    );
  }
  return (
    <span className={styles.line}>
      {prefix}
      <i>{t(line.state === "deleted" ? "sent.memory.deleted" : "sent.memory.changed")}</i>
    </span>
  );
}

function Entry({ send, today }: { send: OnlineSend; today: string }) {
  const [showInstructions, setShowInstructions] = useState(false);
  const sent = isSent(send);
  const when = t("sent.when", {
    day: dayLabel(localDay(send.at), today),
    time: formatTime(send.at),
  });
  const gone = send.lines.some((l) => l.type === "memory" && l.state !== "same");
  return (
    <section aria-label={t("sent.entry")} className={styles.entry}>
      <h2 className={styles.entryTitle}>{title(send)}</h2>
      <dl className={styles.fields}>
        <div>
          <dt>{t(sent ? "sent.field.sent" : "sent.field.when")}</dt>
          <dd>{when}</dd>
        </div>
        {sent ? (
          <div>
            <dt>{t("sent.field.to")}</dt>
            <dd>{t("sent.to", { host: send.host })}</dd>
          </div>
        ) : null}
        <div>
          <dt>{t("sent.field.size")}</dt>
          <dd>
            {sent
              ? t("sent.size", { up: formatBytes(send.bytesUp), down: formatBytes(send.bytesDown) })
              : t("sent.size.none")}
          </dd>
        </div>
        {sent ? (
          <div>
            <dt>{t("sent.field.cost")}</dt>
            <dd className={send.costUsd === null ? styles.quiet : undefined}>
              {send.costUsd === null
                ? t("sent.cost.none")
                : t("sent.cost.amount", { amount: money.format(send.costUsd) })}
            </dd>
          </div>
        ) : null}
        <div>
          <dt>{t("sent.field.result")}</dt>
          <dd>{resultText(send)}</dd>
        </div>
      </dl>
      {send.lines.length > 0 ? (
        <>
          <span className={styles.label}>{t("sent.exact")}</span>
          <p className={styles.sent}>
            {send.lines.map((line, i) => (
              // The log is append-only text: a line has no id of its own.
              <Line key={i} line={line} />
            ))}
          </p>
          {gone ? <p className={styles.note}>{t("sent.memory.note")}</p> : null}
          {send.instructions !== "" ? (
            <>
              <button
                type="button"
                className={styles.link}
                aria-expanded={showInstructions}
                onClick={() => setShowInstructions((v) => !v)}
              >
                {t(showInstructions ? "sent.instructions.hide" : "sent.instructions.show")}
              </button>
              {showInstructions ? <p className={styles.sent}>{send.instructions}</p> : null}
            </>
          ) : null}
        </>
      ) : null}
      {!sent ? <p className={styles.body}>{t("sent.notSent.note")}</p> : null}
    </section>
  );
}

/** #/settings/online/sent: every question sent for online answers, word for word. */
export default function Sent() {
  const engine = useEngine();
  const { data: online } = useOnline();
  const wanted = useQueryParam("send");
  const first = useQuery({
    queryKey: ["onlineSends"],
    queryFn: () => engine.listOnlineSends(PAGE, null),
    staleTime: 0,
  });
  const [older, setOlder] = useState<OnlineSend[]>([]);
  const [exhausted, setExhausted] = useState(false);
  const [moreFailed, setMoreFailed] = useState(false);
  const [chosen, setChosen] = useState<number | null>(null);

  const sends = useMemo(() => [...(first.data ?? []), ...older], [first.data, older]);
  const selected =
    sends.find((s) => s.id === chosen) ??
    sends.find((s) => String(s.id) === wanted) ??
    sends[0] ??
    null;
  useEffect(() => {
    if (first.data && first.data.length < PAGE) setExhausted(true);
  }, [first.data]);

  const today = localDay(new Date());
  const days = useMemo(() => {
    const out: Array<{ day: string; sends: OnlineSend[] }> = [];
    for (const send of sends) {
      const day = localDay(send.at);
      const last = out.at(-1);
      if (last && last.day === day) last.sends.push(send);
      else out.push({ day, sends: [send] });
    }
    return out;
  }, [sends]);

  const showEarlier = async () => {
    const last = sends.at(-1);
    if (!last) return;
    setMoreFailed(false);
    try {
      const page = await engine.listOnlineSends(PAGE, last.id);
      setOlder((all) => [...all, ...page]);
      if (page.length < PAGE) setExhausted(true);
    } catch {
      setMoreFailed(true);
    }
  };

  const heading = (
    <>
      <BackLink to="/settings/online" label={t("sent.back")} />
      <h1 className="t-page-title">{t("sent.heading")}</h1>
      <p className={styles.dek}>{t("sent.dek")}</p>
    </>
  );

  if (first.isError) {
    return (
      <ScreenSheet>
        {heading}
        <div className={styles.message} role="alert">
          <p>{t("sent.error")}</p>
          <Button variant="secondary" onClick={() => void first.refetch()}>
            {t("button.tryAgain")}
          </Button>
        </div>
      </ScreenSheet>
    );
  }
  if (!first.data) {
    return (
      <ScreenSheet>
        {heading}
        <p className={styles.message} role="status">
          {t("sent.loading")}
        </p>
      </ScreenSheet>
    );
  }
  if (sends.length === 0 || !selected) {
    return (
      <ScreenSheet>
        {heading}
        <div className={styles.empty}>
          <p className={styles.emptyTitle}>{t("sent.empty.title")}</p>
          <p className={styles.body}>{t("sent.empty.body")}</p>
        </div>
      </ScreenSheet>
    );
  }

  return (
    <ScreenSheet>
      {heading}
      <div className={styles.split}>
        <nav aria-label={t("sent.nav")} className={styles.list}>
          {days.map(({ day, sends: group }) => (
            <div key={day} className={styles.day}>
              <h2 className={styles.label}>{dayLabel(day, today)}</h2>
              <ul>
                {group.map((send) => (
                  <li key={send.id}>
                    <button
                      type="button"
                      className={styles.row}
                      aria-current={send.id === selected.id ? "true" : undefined}
                      onClick={() => setChosen(send.id)}
                    >
                      <span className={styles.rowTitle}>{title(send)}</span>
                      <span className={styles.rowMeta}>{rowMeta(send)}</span>
                    </button>
                  </li>
                ))}
              </ul>
            </div>
          ))}
          {!exhausted ? (
            <Button variant="quiet" onClick={() => void showEarlier()}>
              {t("sent.more")}
            </Button>
          ) : null}
          {moreFailed ? (
            <p className={styles.note} role="alert">
              {t("sent.more.failed")}
            </p>
          ) : null}
          <p className={styles.note}>
            {online
              ? t("sent.footer.count", { count: online.questionsThisMonth })
              : t("sent.footer")}
          </p>
        </nav>
        <Entry key={selected.id} send={selected} today={today} />
      </div>
    </ScreenSheet>
  );
}
