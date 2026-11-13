import { type ReactNode, useEffect, useId, useRef, useState } from "react";
import { Redirect } from "wouter";
import { goTo } from "../../app/query";
import { Button } from "../../components/Button/Button";
import { Toggle } from "../../components/Toggle/Toggle";
import { formatBytes, formatDayMonth, formatTime } from "../../copy/format";
import { t } from "../../copy/t";
import type {
  AutoOff,
  RemoteConnection,
  RemoteProblem,
  RemoteState,
  RemoteView,
} from "../../engine/bindings";
import { useEngine } from "../../engine/context";
import { AdvancedDetails } from "./Advanced";
import styles from "./Away.module.css";
import { mostPrivateAvailable } from "./os";
import { Confirm, companyOf, connectionName, DeviceStatus, Frame } from "./parts";
import { QrMatrix } from "./QrMatrix";
import { settingsPatch, useBusyAct, useRemoteAct } from "./useRemote";
import { sinceText, whenText } from "./when";

const AUTO_OFF: Array<{
  value: AutoOff;
  label: "away.autoOff.unused" | "away.autoOff.day" | "away.autoOff.week" | "away.autoOff.never";
}> = [
  { value: "unused30Days", label: "away.autoOff.unused" },
  { value: "afterDay", label: "away.autoOff.day" },
  { value: "afterWeek", label: "away.autoOff.week" },
  { value: "never", label: "away.autoOff.never" },
];

const PROBLEM = {
  noInternet: "away.problem.noInternet",
  downloadFailed: "away.problem.downloadFailed",
  checkFailed: "away.problem.checkFailed",
  helperFailed: "away.problem.helperFailed",
  accountBlocked: "away.problem.accountBlocked",
  unsupported: "away.problem.unsupported",
} as const satisfies Record<RemoteProblem, string>;

const other = (connection: RemoteConnection): RemoteConnection =>
  connection === "simple" ? "mostPrivate" : "simple";

/** What the person sees behind "Away from home", by where the connection is. */
export function AwayPage({ view }: { view: RemoteView }) {
  const state = view.state;
  switch (state.type) {
    case "off":
      return <OffPage view={view} />;
    case "downloading":
    case "needsSignIn":
      return <Redirect to="/settings/away/setup" replace />;
    case "starting":
      return <StartingPage />;
    case "on":
      return <OnPage view={view} state={state} />;
    case "needsRelink":
      return <RelinkPage view={view} state={state} />;
    case "problem":
      return <ProblemPage view={view} state={state} />;
  }
}

function Title({ tag, tagKind }: { tag?: string; tagKind?: "problem" }) {
  return (
    <div className={styles.titleRow}>
      <h1 className="t-page-title">{t("away.title")}</h1>
      {tag ? (
        <span className={styles.tag} data-kind={tagKind}>
          {tag}
        </span>
      ) : null}
    </div>
  );
}

function PhonesAside({ view, children }: { view: RemoteView; children?: ReactNode }) {
  const count = view.devices.length;
  return (
    <>
      <h2 className={styles.label}>{t("away.phones.heading", { count })}</h2>
      {count === 0 ? (
        <p className={styles.muted}>{t("away.phones.none")}</p>
      ) : (
        <ul className={styles.list}>
          {view.devices.map((d) => (
            <li key={d.id}>
              <span className={styles.name}>{d.name}</span>
              <span className={styles.sub}>
                <DeviceStatus device={d} />
              </span>
            </li>
          ))}
        </ul>
      )}
      {children}
    </>
  );
}

/** Switching how phones connect unpairs every phone, so it says so first and then starts the setup afresh. */
export function SwitchDialog({
  view,
  onClose,
  stay = false,
}: {
  view: RemoteView;
  onClose: () => void;
  /** True where the setup is already on screen: nothing to navigate to afterwards. */
  stay?: boolean;
}) {
  const engine = useEngine();
  const { run, busy } = useBusyAct();
  const to = other(view.connection);
  return (
    <Confirm
      title={t(to === "simple" ? "away.change.title.simple" : "away.change.title.mostPrivate")}
      confirm={t("away.change.confirm")}
      keep={t(
        view.connection === "simple" ? "away.change.keep.simple" : "away.change.keep.mostPrivate",
      )}
      busy={busy}
      onClose={onClose}
      onConfirm={() => {
        void (async () => {
          const ok = await run(() =>
            engine.updateRemoteSettings(settingsPatch({ connection: to })),
          );
          onClose();
          if (ok && !stay) goTo("/settings/away/setup");
        })();
      }}
    >
      <p>{t("away.change.body")}</p>
    </Confirm>
  );
}

function OffPage({ view }: { view: RemoteView }) {
  const engine = useEngine();
  const { run, busy } = useBusyAct();
  const ready = view.setUpBefore && view.installed.includes(view.connection);
  const rows = [
    { title: t("away.off.ask.title"), body: t("away.off.ask.body") },
    { title: t("away.off.keep.title"), body: t("away.off.keep.body") },
    { title: t("away.off.page.title"), body: t("away.off.page.body") },
  ];

  const turnOn = async () => {
    if (!ready) {
      goTo("/settings/away/setup");
      return;
    }
    await run(() => engine.startRemote(view.connection, false));
  };

  return (
    <Frame
      back={{ to: "/settings", label: t("screen.settings.title") }}
      aside={
        <>
          <h2 className={styles.label}>{t("away.cant.heading")}</h2>
          <ul className={styles.rules}>
            <li>{t("away.cant.forget")}</li>
            <li>{t("away.cant.export")}</li>
            <li>{t("away.cant.settings")}</li>
          </ul>
          <p className={styles.muted}>{t("away.cant.here")}</p>
          <p className={styles.private}>{t("away.private.note")}</p>
        </>
      }
    >
      <Title tag={t("away.tag.off")} />
      <p className={styles.lede}>{t("away.off.lead")}</p>
      <ul className={styles.features}>
        {rows.map((r) => (
          <li key={r.title}>
            <span className={styles.name}>{r.title}</span>
            <span className={styles.sub}>{r.body}</span>
          </li>
        ))}
      </ul>
      <p className={styles.body}>
        {view.setUpBefore ? t("away.off.statusBefore") : t("away.off.status")}
        {view.lastOnAt ? ` ${t("away.off.lastOn", { when: whenText(view.lastOnAt) })}` : ""}
      </p>
      {view.devices.length > 0 ? (
        <p className={styles.body}>{t("away.off.stayPaired", { count: view.devices.length })}</p>
      ) : null}
      <div className={styles.actions}>
        <Button variant="primary" disabled={busy} onClick={() => void turnOn()}>
          {view.setUpBefore ? t("away.off.turnOn") : t("away.off.setUp")}
        </Button>
        <span className={styles.muted}>
          {view.setUpBefore
            ? t("away.off.turnOnWith", {
                connection: connectionName(view.connection),
                company: companyOf(view.connection),
              })
            : t("away.off.setUpHint")}
        </span>
      </div>
    </Frame>
  );
}

function StartingPage() {
  return (
    <Frame back={{ to: "/settings", label: t("screen.settings.title") }}>
      <Title tag={t("away.tag.starting")} />
      <p className={styles.lede} role="status">
        {t("away.starting")}
      </p>
    </Frame>
  );
}

function RemoveAllDialog({ onClose }: { onClose: () => void }) {
  const engine = useEngine();
  const { run, busy } = useBusyAct();
  return (
    <Confirm
      title={t("away.removeAll.title")}
      confirm={t("away.removeAll.confirm")}
      keep={t("away.removeAll.keep")}
      destructive
      busy={busy}
      onClose={onClose}
      onConfirm={() => {
        void run(() => engine.stopRemote(true)).then(onClose);
      }}
    >
      <p>{t("away.removeAll.body")}</p>
      <p>{t("away.removeAll.kept")}</p>
    </Confirm>
  );
}

function OnPage({
  view,
  state,
}: {
  view: RemoteView;
  state: Extract<RemoteState, { type: "on" }>;
}) {
  const engine = useEngine();
  const act = useRemoteAct();
  const advancedId = useId();
  const [advanced, setAdvanced] = useState(false);
  const [dialog, setDialog] = useState<null | "change" | "off">(null);
  const [saved, setSaved] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(
    () => () => {
      if (timer.current) clearTimeout(timer.current);
    },
    [],
  );
  const canChange = other(view.connection) === "simple" || mostPrivateAvailable();

  const save = async (fields: Parameters<typeof settingsPatch>[0]) => {
    if (await act(() => engine.updateRemoteSettings(settingsPatch(fields)))) {
      setSaved(true);
      if (timer.current) clearTimeout(timer.current);
      timer.current = setTimeout(() => setSaved(false), 2500);
    }
  };
  const sentSince = view.onSince ?? view.lastOnAt;

  return (
    <Frame
      back={{ to: "/settings", label: t("screen.settings.title") }}
      aside={
        <>
          <PhonesAside view={view}>
            <div className={styles.actions}>
              <Button variant="primary" onClick={() => goTo("/settings/away/add")}>
                {t("away.phones.add")}
              </Button>
              {view.devices.length > 0 ? (
                <a className="hit" href="#/settings/away/phones">
                  {t("away.phones.see")}
                </a>
              ) : null}
            </div>
            <p className={styles.muted}>{t("away.phones.lost")}</p>
          </PhonesAside>
          <div className={styles.sent}>
            <span className={styles.label}>{t("away.sent.label")}</span>
            <span>
              {view.sentToPhonesBytes > 0 ? (
                <>
                  <strong>{formatBytes(view.sentToPhonesBytes)}</strong>
                  {sentSince ? ` ${t("away.sent.since", { day: formatDayMonth(sentSince) })}` : ""}
                </>
              ) : (
                t("away.sent.nothing")
              )}
            </span>
            <span className={styles.muted}>{t("away.sent.counted")}</span>
          </div>
        </>
      }
    >
      <Title />
      <p className={styles.lede}>{t("away.on.lead")}</p>
      <div className={styles.switch}>
        <Toggle
          label={t("away.on.switch")}
          hint={view.onSince ? sinceText(view.onSince) : undefined}
          checked
          onChange={() => void act(() => engine.stopRemote(false))}
        />
      </div>
      <p className={styles.saved} role="status">
        {saved ? t("away.saved") : ""}
      </p>
      <ul className={styles.rows}>
        <li className={styles.row}>
          <div className={styles.text}>
            <span className={styles.name}>
              {t(
                view.connection === "simple"
                  ? "away.connection.simple.through"
                  : "away.connection.mostPrivate.through",
              )}
            </span>
            <span className={styles.sub}>
              {t(view.connection === "simple" ? "away.simple.hint" : "away.mostPrivate.hint")}
            </span>
            <span className={styles.sub}>{t("away.switch.warns")}</span>
            {state.machineName ? (
              <span className={styles.sub}>
                {t("away.on.machine", { name: state.machineName })}
              </span>
            ) : null}
          </div>
          {canChange ? (
            <Button variant="quiet" onClick={() => setDialog("change")}>
              {t("away.change")}
            </Button>
          ) : null}
        </li>
        <li className={styles.row}>
          <div className={styles.text}>
            <label className={styles.name} htmlFor={`${advancedId}-auto`}>
              {t("away.autoOff.label")}
            </label>
            <span className={styles.sub} id={`${advancedId}-auto-hint`}>
              {t("away.autoOff.hint")}
            </span>
          </div>
          <select
            id={`${advancedId}-auto`}
            className={styles.select}
            aria-describedby={`${advancedId}-auto-hint`}
            value={view.autoOff}
            onChange={(event) => void save({ autoOff: event.target.value as AutoOff })}
          >
            {AUTO_OFF.map((o) => (
              <option key={o.value} value={o.value}>
                {t(o.label)}
              </option>
            ))}
          </select>
        </li>
        <li className={styles.row}>
          <Toggle
            label={t("away.awake.label")}
            hint={t("away.awake.hint")}
            checked={view.keepAwake}
            onChange={(keepAwake) => void save({ keepAwake })}
          />
        </li>
        <li className={styles.row}>
          <div className={styles.text}>
            <span className={styles.name}>{t("away.private.label")}</span>
            <span className={styles.sub}>{t("away.private.note")}</span>
          </div>
          <span className={styles.value}>{t("away.private.never")}</span>
        </li>
      </ul>
      <div className={styles.links}>
        <a className="hit" href="#/settings/away/activity">
          {t("away.link.activity")}
        </a>
        <button
          type="button"
          className={`${styles.linkButton} hit`}
          aria-expanded={advanced}
          aria-controls={advancedId}
          onClick={() => setAdvanced(!advanced)}
        >
          {advanced ? t("away.advanced.hide") : t("away.advanced.show")}
        </button>
        <Button variant="forget" onClick={() => setDialog("off")}>
          {t("away.removeAll")}
        </Button>
      </div>
      {advanced ? <AdvancedDetails id={advancedId} /> : null}
      {dialog === "change" ? <SwitchDialog view={view} onClose={() => setDialog(null)} /> : null}
      {dialog === "off" ? <RemoveAllDialog onClose={() => setDialog(null)} /> : null}
    </Frame>
  );
}

function RelinkPage({
  view,
  state,
}: {
  view: RemoteView;
  state: Extract<RemoteState, { type: "needsRelink" }>;
}) {
  const engine = useEngine();
  const act = useRemoteAct();
  const words = state.words.split(/[\s·]+/).filter(Boolean);
  const waiting = view.devices.filter((d) => d.needsAddress);
  return (
    <Frame narrow back={{ to: "/settings", label: t("screen.settings.title") }}>
      <Title />
      <div className={styles.relink}>
        <div className={styles.qrBox}>
          <QrMatrix qr={state.qr} label={t("away.relink.qr")} />
        </div>
        <div className={styles.relinkText}>
          <h2 className={styles.statement}>{t("away.relink.title")}</h2>
          <p className={styles.body}>{t("away.relink.how")}</p>
          <div className={styles.typed}>
            <span className={styles.sub}>{t("away.relink.cantScan")}</span>
            <span className={styles.mono}>{words.join(" · ")}</span>
          </div>
          <ul className={styles.list} aria-label={t("away.relink.list")}>
            {view.devices.map((d) => (
              <li key={d.id}>
                <span className={styles.name}>{d.name}</span>
                <span className={styles.sub}>
                  {d.needsAddress
                    ? t("away.relink.notYet")
                    : d.lastUsedAt
                      ? t("away.relink.reconnectedAt", { time: formatTime(d.lastUsedAt) })
                      : t("away.relink.reconnected")}
                </span>
              </li>
            ))}
          </ul>
        </div>
      </div>
      <div className={styles.rows}>
        <div className={styles.row}>
          <div className={styles.text}>
            <span className={styles.name}>{t("away.relink.on")}</span>
            <span className={styles.sub}>
              {t("away.relink.address", {
                connection: connectionName(view.connection),
                company: companyOf(view.connection),
              })}
            </span>
          </div>
          <Button variant="secondary" onClick={() => void act(() => engine.stopRemote(false))}>
            {t("away.relink.turnOff")}
          </Button>
        </div>
        <div className={styles.row}>
          <div className={styles.text}>
            <span className={styles.name}>
              {t("away.phones.heading", { count: view.devices.length })}
            </span>
            <span className={styles.sub}>
              {waiting.length === 0
                ? t("away.relink.allDone")
                : t("away.relink.waiting", { count: waiting.length, name: waiting[0]?.name ?? "" })}
            </span>
          </div>
          <a className="hit" href="#/settings/away/phones">
            {t("away.phones.see")}
          </a>
        </div>
      </div>
      <p className={styles.muted}>{t("away.relink.mostPrivate")}</p>
    </Frame>
  );
}

function ProblemPage({
  view,
  state,
}: {
  view: RemoteView;
  state: Extract<RemoteState, { type: "problem" }>;
}) {
  const engine = useEngine();
  const { run, busy } = useBusyAct();
  const company = companyOf(view.connection);
  const redownload =
    state.code === "noInternet" || state.code === "downloadFailed" || state.code === "checkFailed";
  const [switching, setSwitching] = useState(false);
  const retry = async () => {
    await run(() => engine.startRemote(view.connection, redownload));
  };
  const canSwitch = view.connection === "mostPrivate" || mostPrivateAvailable();
  return (
    <Frame
      back={{ to: "/settings", label: t("screen.settings.title") }}
      aside={
        view.devices.length > 0 ? (
          <PhonesAside view={view}>
            <p className={styles.muted}>{t("away.problem.stillPaired")}</p>
          </PhonesAside>
        ) : null
      }
    >
      <Title tag={t("away.tag.problem")} tagKind="problem" />
      <div className={styles.problem}>
        <div className={styles.alertText} role="alert">
          <div className={styles.text}>
            <span className={styles.name}>
              {t(view.connection === "simple" ? "away.problem.simple" : "away.problem.mostPrivate")}
            </span>
            <span className={styles.sub}>
              {t("away.problem.tried", { count: state.attempts, time: formatTime(state.since) })}
            </span>
          </div>
          <p>{t(PROBLEM[state.code], { company })}</p>
          <p>{t(view.devices.length > 0 ? "away.problem.restPhones" : "away.problem.rest")}</p>
        </div>
        <div className={styles.buttons}>
          <Button variant="primary" disabled={busy} onClick={() => void retry()}>
            {t("button.tryAgain")}
          </Button>
          <Button
            variant="secondary"
            disabled={busy}
            onClick={() => void run(() => engine.stopRemote(false))}
          >
            {t("away.problem.turnOff")}
          </Button>
        </div>
      </div>
      {view.connection === "simple" ? (
        <>
          <p className={styles.body}>{t("away.problem.trial")}</p>
          {canSwitch ? (
            <div>
              <Button variant="quiet" onClick={() => setSwitching(true)}>
                {t("away.problem.switch")}
              </Button>
            </div>
          ) : null}
        </>
      ) : view.connection === "mostPrivate" ? (
        <div>
          <Button variant="quiet" onClick={() => setSwitching(true)}>
            {t("away.problem.switchSimple")}
          </Button>
        </div>
      ) : null}
      {switching ? <SwitchDialog view={view} onClose={() => setSwitching(false)} /> : null}
    </Frame>
  );
}
