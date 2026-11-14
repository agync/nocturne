import { type ReactNode, useId, useRef, useState } from "react";
import { goTo } from "../../app/query";
import { Button } from "../../components/Button/Button";
import { Toggle } from "../../components/Toggle/Toggle";
import { formatBytes, formatLongDay, formatTime } from "../../copy/format";
import { t } from "../../copy/t";
import type { OnlineLimit, OnlineView } from "../../engine/bindings";
import { useEngine } from "../../engine/context";
import { limitText, QUESTION_STEPS, USD_STEPS } from "./limits";
import styles from "./Online.module.css";
import { Frame, LockNote, providerName } from "./parts";
import { MonthAside } from "./Status";
import { ForgetKeyDialog, settingsOf, useTurnOff } from "./TurnOff";
import { useOnlineAct } from "./useOnline";
import { dayText, sentence, whenText } from "./when";

type Back = { to: string; label: string };
const toSetup = () => goTo("/settings/online/setup");
const toConnect = () => goTo("/settings/advanced/online", { from: "online" });

function Title({ tag, trouble }: { tag: string; trouble?: boolean }) {
  return (
    <div className={styles.titleRow}>
      <h1 className="t-page-title">{t("screen.online.title")}</h1>
      <span className={styles.tag} data-kind={trouble ? "trouble" : undefined}>
        {tag}
      </span>
    </div>
  );
}

function Row({
  title,
  hint,
  children,
}: {
  title: ReactNode;
  hint: ReactNode;
  children?: ReactNode;
}) {
  return (
    <li className={styles.row}>
      <span className={styles.textStack}>
        <span className={styles.name}>{title}</span>
        <span className={styles.sub}>{hint}</span>
      </span>
      {children}
    </li>
  );
}

/** What the person sees behind "Online answers", by where it has got to. */
export function OnlinePage({ view }: { view: OnlineView }) {
  const back = { to: "/settings", label: t("screen.settings.title") };
  switch (view.state.type) {
    case "checking":
      return <CheckingPage back={back} />;
    case "off":
      return view.setUp ? (
        <OffBefore view={view} back={back} />
      ) : (
        <NeverSetUp view={view} back={back} />
      );
    default:
      return <OnPage view={view} back={back} />;
  }
}

/** A connection is being checked: nothing is on, and nothing here can turn it on. */
function CheckingPage({ back }: { back: Back }) {
  return (
    <Frame back={back}>
      <Title tag={t("online.tag.checking")} />
      <p className={styles.lede} role="status">
        {t("online.checking.body")}
      </p>
    </Frame>
  );
}

function NeverSetUp({ view, back }: { view: OnlineView; back: Back }) {
  const features = [{ title: t("online.off.view.title"), body: t("online.off.view.body") }];
  const stays = [
    t("online.stays.photos"),
    t("online.stays.private"),
    t("online.stays.keeping"),
    t("online.stays.page"),
  ];
  return (
    <Frame
      back={back}
      aside={
        <>
          <h2 className={styles.label}>{t("online.stays.heading")}</h2>
          <ul className={styles.rules}>
            {stays.map((line) => (
              <li key={line}>{line}</li>
            ))}
          </ul>
          <LockNote>{t("online.stays.sensitive")}</LockNote>
        </>
      }
    >
      <Title tag={t("online.tag.off")} />
      <p className={styles.lede}>{t("online.off.lead")}</p>
      <ul className={styles.features}>
        {features.map((f) => (
          <li key={f.title}>
            <span className={styles.textStack}>
              <span className={styles.name}>{f.title}</span>
              <span className={styles.sub}>{f.body}</span>
            </span>
          </li>
        ))}
      </ul>
      <p className={styles.body}>{t("online.off.status")}</p>
      {view.lastSentAt ? (
        <p className={styles.sub}>
          {t("online.off.history")}{" "}
          <a className="hit" href="#/settings/online/sent">
            {t("online.link.sent")}
          </a>
        </p>
      ) : null}
      <div className={styles.actions}>
        <Button variant="primary" onClick={() => toSetup()}>
          {t("online.off.setUp")}
        </Button>
        <span className={styles.muted}>{t("online.off.setUpHint")}</span>
      </div>
    </Frame>
  );
}

function ProviderRow({ view, kept }: { view: OnlineView; kept: boolean }) {
  return (
    <Row
      title={providerName(view.host)}
      hint={`${t("online.provider.writes", { used: view.model ?? "" })} ${t(
        kept ? "online.provider.kept" : "online.provider.engine",
      )}`}
    >
      <Button variant="quiet" onClick={() => toConnect()}>
        {t("online.change")}
      </Button>
    </Row>
  );
}

function OffBefore({ view, back }: { view: OnlineView; back: Back }) {
  const engine = useEngine();
  const { run, busy } = useOnlineAct();
  const [forgetting, setForgetting] = useState(false);
  const last = view.lastOnAt ?? view.onSince;
  return (
    <Frame
      back={back}
      aside={
        <>
          <span className={styles.label}>{t("online.sent.heading")}</span>
          <span>
            {view.sentBytes > 0
              ? t("online.before.sent.size", { size: formatBytes(view.sentBytes) })
              : t("online.before.sent.none")}
          </span>
          {view.lastSentAt ? (
            <span className={styles.sub}>
              {t("online.before.sent.last", { when: whenText(view.lastSentAt) })}
            </span>
          ) : null}
        </>
      }
    >
      <Title tag={t("online.tag.off")} />
      <p className={styles.lede}>{t("online.before.lead")}</p>
      <div className={styles.card}>
        <Toggle
          label={t("online.before.switch")}
          hint={t("online.before.hint")}
          checked={false}
          disabled={busy}
          onChange={() => void run(() => engine.turnOnOnline(settingsOf(view)))}
        />
      </div>
      <ul className={styles.rows}>
        <ProviderRow view={view} kept />
        {last ? (
          <Row
            title={t("online.lastOn.label")}
            hint={`${formatLongDay(last)}, ${formatTime(last)}`}
          >
            <a className="hit" href="#/settings/online/sent">
              {t("online.link.sent")}
            </a>
          </Row>
        ) : null}
      </ul>
      <div className={styles.links}>
        <a className="hit" href="#/settings/advanced">
          {t("online.link.advanced")}
        </a>
        <button
          type="button"
          className={`${styles.linkButton} ${styles.linkEnd} hit`}
          data-kind="danger"
          onClick={() => setForgetting(true)}
        >
          {t("online.forgetKey")}
        </button>
      </div>
      {forgetting ? <ForgetKeyDialog view={view} onClose={() => setForgetting(false)} /> : null}
    </Frame>
  );
}

const limitKey = (l: OnlineLimit) =>
  `${l.kind}:${l.kind === "usd" ? l.usd : l.kind === "questions" ? l.questions : ""}`;

/** The choices in the limit menu: the usual steps, money only when reported, and the current one. */
function limitOptions(
  view: OnlineView,
): Array<{ value: string; label: string; limit: OnlineLimit }> {
  const questions = (n: number): OnlineLimit => ({ kind: "questions", questions: n, usd: null });
  const money = (usd: number): OnlineLimit => ({
    kind: "usd",
    questions: view.limit.questions,
    usd,
  });
  const list: OnlineLimit[] = [...QUESTION_STEPS].map(questions);
  if (view.costReported) list.push(...[...USD_STEPS].map(money));
  list.push({ kind: "none", questions: view.limit.questions, usd: null });
  if (!list.some((l) => limitKey(l) === limitKey(view.limit))) list.unshift(view.limit);
  return list.map((limit) => ({ value: limitKey(limit), label: limitText(limit), limit }));
}

function OnPage({ view, back }: { view: OnlineView; back: Back }) {
  const engine = useEngine();
  const { run, busy } = useOnlineAct();
  const turn = useTurnOff(view);
  const limitId = useId();
  const limitRef = useRef<HTMLSelectElement>(null);
  const [forgetting, setForgetting] = useState(false);
  const state = view.state;
  const trouble = state.type !== "on";
  const options = limitOptions(view);

  const patch = (change: { phones?: boolean; sendSensitive?: boolean; limit?: OnlineLimit }) =>
    run(() =>
      engine.updateOnlineSettings({
        phones: change.phones ?? null,
        sendSensitive: change.sendSensitive ?? null,
        limit: change.limit ?? null,
      }),
    );

  const changeLimit = (value: string) => {
    const picked = options.find((o) => o.value === value);
    if (!picked) return;
    void patch({ limit: picked.limit });
  };

  const focusLimit = () => {
    limitRef.current?.scrollIntoView?.({ block: "center" });
    limitRef.current?.focus();
  };

  let warning: ReactNode = null;
  if (state.type === "paused" && state.pause.type === "limit") {
    warning = (
      <Warning
        title={t("online.limit.paused.title")}
        body={t("online.limit.paused.body", {
          count: view.questionsThisMonth,
          day: dayText(state.pause.until),
        })}
      >
        <Button variant="secondary" onClick={focusLimit}>
          {t("online.limit.paused.action")}
        </Button>
      </Warning>
    );
  } else if (state.type === "paused" && state.pause.type === "keyRefused") {
    warning = (
      <Warning
        title={sentence(t("online.refused.title"))}
        body={t("online.refused.body", { host: view.host ?? "" })}
      >
        <Button variant="secondary" onClick={() => toConnect()}>
          {t("online.refused.action")}
        </Button>
      </Warning>
    );
  } else if (state.type === "paused") {
    warning = (
      <Warning title={sentence(t("online.noCredit.title"))} body={t("online.noCredit.body")}>
        <Button variant="secondary" onClick={() => toConnect()}>
          {t("online.noCredit.action")}
        </Button>
      </Warning>
    );
  } else if (state.type === "problem") {
    const host = view.host ?? "";
    const time = formatTime(state.since);
    const tryAgain = (
      <Button
        variant="secondary"
        disabled={busy}
        onClick={() => void run(() => engine.turnOnOnline(settingsOf(view)))}
      >
        {t("online.problem.action")}
      </Button>
    );
    const check = (
      action: "online.problem.modelMissing.action" | "online.problem.keyUnavailable.action",
    ) => (
      <Button variant="secondary" onClick={() => toConnect()}>
        {t(action)}
      </Button>
    );
    if (state.code === "modelMissing") {
      warning = (
        <Warning
          title={t("online.problem.modelMissing.title")}
          body={t("online.problem.modelMissing.body", { host })}
        >
          {check("online.problem.modelMissing.action")}
        </Warning>
      );
    } else if (state.code === "keyUnavailable") {
      warning = (
        <Warning
          title={t("online.problem.keyUnavailable.title")}
          body={t("online.problem.keyUnavailable.body", { host })}
        >
          {check("online.problem.keyUnavailable.action")}
        </Warning>
      );
    } else {
      warning = (
        <Warning
          title={t("online.problem.title")}
          body={t(
            state.code === "noInternet"
              ? "online.problem.noInternet"
              : "online.problem.unreachable",
            { host, time },
          )}
        >
          {tryAgain}
        </Warning>
      );
    }
  }

  return (
    <Frame back={back} aside={<MonthAside view={view} />}>
      <Title tag={t(trouble ? "online.tag.trouble" : "online.tag.on")} trouble={trouble} />
      <p className={styles.lede}>{t("online.on.lead")}</p>
      {warning ? (
        <div className={styles.card} data-kind="warn">
          {warning}
          <Button variant="quiet" disabled={turn.busy} onClick={() => void turn.turnOff()}>
            {t("online.turnOff")}
          </Button>
        </div>
      ) : (
        <div className={styles.card}>
          <Toggle
            label={t("online.on.switch")}
            hint={
              view.onSince ? t("online.on.since", { day: formatLongDay(view.onSince) }) : undefined
            }
            checked
            disabled={turn.busy}
            onChange={() => void turn.turnOff()}
          />
        </div>
      )}
      <ul className={styles.rows}>
        <ProviderRow view={view} kept={false} />
        {view.phonesAvailable ? (
          <li className={styles.row}>
            <Toggle
              label={t("online.row.phones.label")}
              hint={t("online.row.phones.hint")}
              checked={view.phones}
              disabled={busy}
              onChange={(phones) => void patch({ phones })}
            />
          </li>
        ) : null}
        <li className={styles.row}>
          <Toggle
            label={t("online.row.sensitive.label")}
            hint={t(view.sendSensitive ? "online.row.sensitive.on" : "online.row.sensitive.off")}
            checked={view.sendSensitive}
            disabled={busy}
            onChange={(sendSensitive) => void patch({ sendSensitive })}
          />
        </li>
        <Row title={t("online.row.private.label")} hint={t("online.row.private.hint")}>
          <span className={styles.value}>{t("online.row.private.value")}</span>
        </Row>
        <li className={styles.row}>
          <span className={styles.textStack}>
            <label className={styles.name} htmlFor={limitId}>
              {t("online.row.limit.label")}
            </label>
            <span className={styles.sub}>{t("online.row.limit.hint")}</span>
          </span>
          <select
            id={limitId}
            ref={limitRef}
            className={styles.select}
            value={limitKey(view.limit)}
            disabled={busy}
            onChange={(event) => changeLimit(event.target.value)}
          >
            {options.map((o) => (
              <option key={o.value} value={o.value}>
                {o.label}
              </option>
            ))}
          </select>
        </li>
      </ul>
      <div className={styles.links}>
        <a className="hit" href="#/settings/online/sent">
          {t("online.link.sent")}
        </a>
        <a className="hit" href="#/settings/advanced">
          {t("online.link.advanced")}
        </a>
        <button
          type="button"
          className={`${styles.linkButton} ${styles.linkEnd} hit`}
          data-kind="danger"
          onClick={() => setForgetting(true)}
        >
          {t("online.turnOffForget")}
        </button>
      </div>
      {forgetting ? <ForgetKeyDialog view={view} onClose={() => setForgetting(false)} /> : null}
    </Frame>
  );
}

function Warning({ title, body, children }: { title: string; body: string; children: ReactNode }) {
  return (
    <>
      <span className={styles.cardText} role="status">
        <span className={styles.name}>{title}</span>
        <span className={styles.body}>{body}</span>
      </span>
      {children}
    </>
  );
}
