import { type ReactNode, useEffect, useRef, useState } from "react";
import { goTo, useQueryParam } from "../../app/query";
import { ActionBar } from "../../components/ActionBar/ActionBar";
import { Button } from "../../components/Button/Button";
import { tc } from "../../components/ControlChip/copy";
import { Disclosure } from "../../components/Disclosure/Disclosure";
import { Progress } from "../../components/Progress/Progress";
import { useToast } from "../../components/Toast/Toast";
import { formatBytes } from "../../copy/format";
import { type CopyId, t } from "../../copy/t";
import type { RemoteConnection, RemoteView, SignInStep } from "../../engine/bindings";
import { useEngine } from "../../engine/context";
import styles from "./Away.module.css";
import { AwayPage, SwitchDialog } from "./AwayPage";
import { computerKind, mayAskAboutFirewall, mostPrivateAvailable } from "./os";
import { companyOf, connectionName, Frame, PhoneFacts, SeesText } from "./parts";
import { useBusyAct, useRemoteAct } from "./useRemote";

/** Roughly what each part weighs; the real size is shown while it downloads. */
const ESTIMATE: Record<RemoteConnection, number> = { simple: 40e6, mostPrivate: 50e6 };

type Step = "choice" | "download" | "signIn" | "approve" | "allowLinks" | "phoneApp";

const STEP_NUMBER: Record<RemoteConnection, Record<Step, number>> = {
  simple: { choice: 1, download: 2, signIn: 2, approve: 2, allowLinks: 2, phoneApp: 2 },
  mostPrivate: { choice: 1, download: 2, signIn: 3, approve: 3, allowLinks: 4, phoneApp: 5 },
};
const TOTAL: Record<RemoteConnection, number> = { simple: 3, mostPrivate: 6 };

const TODO: Record<RemoteConnection, Array<{ n: number; text: CopyId }>> = {
  simple: [{ n: 3, text: "away.todo.scan" }],
  mostPrivate: [
    { n: 3, text: "away.todo.signIn" },
    { n: 4, text: "away.todo.allowLinks" },
    { n: 5, text: "away.todo.phoneApp" },
    { n: 6, text: "away.todo.scan" },
  ],
};

function StepLabel({
  connection,
  step,
  named,
}: {
  connection: RemoteConnection;
  step: Step;
  named: boolean;
}) {
  const values = {
    step: STEP_NUMBER[connection][step],
    total: TOTAL[connection],
    connection: connectionName(connection),
  };
  const key = !named ? "away.step.first" : "away.step.named";
  return <span className={styles.label}>{t(key, values)}</span>;
}

function StillToCome({ connection, after }: { connection: RemoteConnection; after: number }) {
  const rest = TODO[connection].filter((item) => item.n > after);
  return (
    <>
      {rest.length > 0 ? (
        <>
          <h2 className={styles.label}>{t("away.todo.heading")}</h2>
          <ol className={styles.steps}>
            {rest.map((item) => (
              <li key={item.n}>
                <span className={styles.badge} aria-hidden="true">
                  {item.n}
                </span>
                <span>{t(item.text)}</span>
              </li>
            ))}
          </ol>
        </>
      ) : null}
      <p className={styles.muted}>{t("away.todo.leaving")}</p>
    </>
  );
}

function Firewall({ connection }: { connection: RemoteConnection }) {
  if (connection !== "mostPrivate" || !mayAskAboutFirewall(computerKind())) return null;
  return <p className={styles.note}>{t("away.firewall")}</p>;
}

/** Turning Away from home on, one step at a time, wherever the connection has got to. */
export function SetupFlow({ view }: { view: RemoteView }) {
  const engine = useEngine();
  const { run, busy } = useBusyAct();
  const [switching, setSwitching] = useState(false);
  // From the first run's "Yes, bring my memory here": Simple, straight to turning it on.
  const forMove = useQueryParam("phone") === "1";
  const [choice, setChoice] = useState<RemoteConnection>(
    !forMove && view.connection === "mostPrivate" && mostPrivateAvailable()
      ? "mostPrivate"
      : "simple",
  );
  const [asking, setAsking] = useState(forMove);
  const [phoneApp, setPhoneApp] = useState(false);
  const handed = useRef(false);
  const state = view.state;

  // Leaving before a phone is added turns it off again, even while a start is still in flight;
  // stopping what is already off does no harm. Handing on to Add a phone or the page does not.
  useEffect(
    () => () => {
      if (!handed.current) void engine.stopRemote(false).catch(() => undefined);
    },
    [engine],
  );

  const finished =
    state.type === "on" && (view.devices.length > 0 || view.connection === "simple" || phoneApp);
  useEffect(() => {
    if (state.type !== "on") return;
    if (view.devices.length > 0) {
      handed.current = true;
      goTo("/settings/away");
    } else if (finished) {
      handed.current = true;
      goTo("/settings/away/add", { setup: "1" });
    }
  }, [state.type, view.devices.length, finished]);

  const leave = async () => {
    handed.current = true;
    await run(() => engine.stopRemote(false));
    goTo("/settings/away");
  };

  if (state.type === "problem") {
    handed.current = true;
    return <AwayPage view={view} />;
  }
  if (state.type === "needsRelink") {
    handed.current = true;
    return <AwayPage view={view} />;
  }

  const connection = state.type === "off" ? choice : view.connection;
  const aside = (step: Step) => (
    <StillToCome connection={connection} after={STEP_NUMBER[connection][step]} />
  );

  if (state.type === "off" && !asking) {
    return (
      <Choice
        view={view}
        choice={choice}
        onChoice={setChoice}
        busy={busy}
        onContinue={() => {
          // Phones paired the other way stop working, so that is asked about first.
          if (view.devices.length > 0 && choice !== view.connection) setSwitching(true);
          else setAsking(true);
        }}
        onLeave={() => goTo("/settings/away")}
        switchDialog={
          switching ? <SwitchDialog view={view} onClose={() => setSwitching(false)} stay /> : null
        }
      />
    );
  }
  if (state.type === "off") {
    const here = view.installed.includes(choice);
    const company = companyOf(choice);
    return (
      <Frame
        back={{ to: "/settings", label: t("screen.settings.title") }}
        aside={aside("download")}
      >
        <StepLabel connection={choice} step="download" named />
        <h1 className="t-page-title">
          {here
            ? t("away.turnOn.title", { company })
            : t("away.ask.title", { company, size: formatBytes(ESTIMATE[choice]) })}
        </h1>
        <p className={styles.body}>{t("away.turnOn.lead", { company })}</p>
        {here ? null : (
          <>
            <p className={styles.body}>{tc("away.ask.body", { company })}</p>
            <dl className={styles.facts}>
              <div>
                <dt>{t("away.ask.from")}</dt>
                <dd>{company}</dd>
              </div>
              <div>
                <dt>{t("away.ask.size")}</dt>
                <dd>{t("away.ask.about", { size: formatBytes(ESTIMATE[choice]) })}</dd>
              </div>
              <div>
                <dt>{t("away.ask.runs")}</dt>
                <dd>{t("away.ask.runsOnly")}</dd>
              </div>
            </dl>
          </>
        )}
        <section className={styles.sec}>
          <h2 className={styles.smallHeading}>{t("away.sees.heading")}</h2>
          <SeesText connection={choice} />
          <p className={styles.body}>{t("away.sees.either")}</p>
        </section>
        <section className={styles.sec}>
          <h2 className={styles.smallHeading}>{t("away.phone.heading")}</h2>
          <p className={styles.body}>{t("away.phone.body")}</p>
          <p className={styles.body}>{t("away.lost")}</p>
        </section>
        <Firewall connection={choice} />
        <ActionBar>
          <Button
            variant="primary"
            disabled={busy}
            onClick={() => void run(() => engine.startRemote(choice, !here))}
          >
            {t(here ? "away.ask.turnOn" : "away.ask.download")}
          </Button>
          <Button variant="quiet" onClick={() => goTo(forMove ? "/setup" : "/settings/away")}>
            {t("away.notNow")}
          </Button>
        </ActionBar>
      </Frame>
    );
  }
  if (state.type === "downloading") {
    return (
      <Frame
        back={{ to: "/settings", label: t("screen.settings.title") }}
        aside={aside("download")}
      >
        <StepLabel connection={connection} step="download" named />
        <h1 className="t-page-title">
          {t("away.download.title", { company: companyOf(connection) })}
        </h1>
        <p className={styles.body}>{t("away.download.body")}</p>
        <div className={styles.downloadBox}>
          <Progress
            label={t("away.download.part")}
            value={state.receivedBytes}
            max={state.totalBytes}
            paused={state.paused}
            detail={t("away.download.detail", {
              done: formatBytes(state.receivedBytes),
              total: formatBytes(state.totalBytes),
            })}
          />
          <div className={styles.downloadFoot}>
            <span className={styles.muted}>{t("away.download.leaving")}</span>
            {state.paused ? (
              <Button
                variant="secondary"
                disabled={busy}
                onClick={() => void run(() => engine.startRemote(connection, true))}
              >
                {t("away.download.resume")}
              </Button>
            ) : (
              <Button
                variant="secondary"
                disabled={busy}
                onClick={() => void run(() => engine.pauseRemoteDownload())}
              >
                {t("away.download.pause")}
              </Button>
            )}
          </div>
        </div>
        <ActionBar>
          <Button variant="secondary" disabled={busy} onClick={() => void leave()}>
            {t("away.cancel")}
          </Button>
        </ActionBar>
      </Frame>
    );
  }
  if (state.type === "starting") {
    return (
      <Frame
        back={{ to: "/settings", label: t("screen.settings.title") }}
        aside={aside("download")}
      >
        <StepLabel connection={connection} step="download" named />
        <h1 className="t-page-title">
          {t("away.starting.title", { company: companyOf(connection) })}
        </h1>
        <p className={styles.body} role="status">
          {t("away.starting")}
        </p>
        <Firewall connection={connection} />
        <ActionBar>
          <Button variant="secondary" disabled={busy} onClick={() => void leave()}>
            {t("away.cancel")}
          </Button>
        </ActionBar>
      </Frame>
    );
  }
  if (state.type === "needsSignIn") {
    return (
      <SignIn
        connection={connection}
        step={state.step}
        url={state.url}
        aside={aside(state.step)}
        onLeave={() => void leave()}
      />
    );
  }
  if (
    state.type === "on" &&
    connection === "mostPrivate" &&
    !phoneApp &&
    view.devices.length === 0
  ) {
    return (
      <Frame
        back={{ to: "/settings", label: t("screen.settings.title") }}
        aside={<PhoneAppAside connection={connection} />}
      >
        <StepLabel connection={connection} step="phoneApp" named />
        <h1 className="t-page-title">{t("away.phoneApp.title")}</h1>
        <p className={styles.body}>{t("away.phoneApp.body")}</p>
        <ul className={styles.features}>
          <li>
            <span className={styles.name}>{t("away.phoneApp.iphone")}</span>
            <span className={styles.sub}>{t("away.phoneApp.iphone.where")}</span>
          </li>
          <li>
            <span className={styles.name}>{t("away.phoneApp.android")}</span>
            <span className={styles.sub}>{t("away.phoneApp.android.where")}</span>
          </li>
        </ul>
        <p className={styles.ok}>{t("away.phoneApp.signedIn")}</p>
        <ActionBar>
          <Button variant="primary" onClick={() => setPhoneApp(true)}>
            {t("away.phoneApp.done")}
          </Button>
          <Button variant="quiet" onClick={() => setPhoneApp(true)}>
            {t("away.phoneApp.have")}
          </Button>
        </ActionBar>
      </Frame>
    );
  }
  return null;
}

function PhoneAppAside({ connection }: { connection: RemoteConnection }) {
  return (
    <>
      <h2 className={styles.label}>{t("away.phoneApp.onPhone")}</h2>
      <ol className={styles.steps}>
        {(["away.phoneApp.a", "away.phoneApp.b", "away.phoneApp.c"] as const).map((key, i) => (
          <li key={key}>
            <span className={styles.badge} aria-hidden="true">
              {"abc"[i]}
            </span>
            <span>{t(key)}</span>
          </li>
        ))}
      </ol>
      <StillToCome connection={connection} after={5} />
    </>
  );
}

interface ChoiceProps {
  view: RemoteView;
  choice: RemoteConnection;
  onChoice: (next: RemoteConnection) => void;
  onContinue: () => void;
  onLeave: () => void;
  busy: boolean;
  switchDialog: ReactNode;
}

function Choice({ choice, onChoice, onContinue, onLeave, view, busy, switchDialog }: ChoiceProps) {
  const options: Array<{ value: RemoteConnection; show: boolean }> = [
    { value: "simple", show: true },
    { value: "mostPrivate", show: mostPrivateAvailable() },
  ];
  const canMostPrivate = mostPrivateAvailable();
  return (
    <Frame back={{ to: "/settings", label: t("screen.settings.title") }}>
      <StepLabel connection={choice} step="choice" named={false} />
      <h1 className="t-page-title">{t("away.off.lead")}</h1>
      <fieldset className={styles.choices}>
        <legend className={styles.legend}>{t("away.choice.legend")}</legend>
        {options
          .filter((o) => o.show)
          .map((o) => {
            const simple = o.value === "simple";
            return (
              <label key={o.value} className={styles.choice} data-checked={choice === o.value}>
                <input
                  type="radio"
                  name="connection"
                  value={o.value}
                  checked={choice === o.value}
                  onChange={() => onChoice(o.value)}
                />
                <span className={styles.choiceText}>
                  <span className={styles.name}>{connectionName(o.value)}</span>
                  <span>
                    <b>{t("away.choice.needs")}</b>{" "}
                    {t(simple ? "away.choice.simple.needs" : "away.choice.mostPrivate.needs")}
                  </span>
                  <span>
                    <b>{t("away.choice.sees")}</b>{" "}
                    {t(simple ? "away.choice.simple.sees" : "away.choice.mostPrivate.sees")}
                  </span>
                  {simple && view.trial ? (
                    <span className={styles.sub}>{t("away.simple.hint")}</span>
                  ) : null}
                </span>
              </label>
            );
          })}
      </fieldset>
      <dl className={`${styles.dl} ${styles.short}`}>
        <div>
          <dt>{t("away.short.phone.label")}</dt>
          <dd>{t("away.short.phone.body")}</dd>
        </div>
        <div>
          <dt>{t("away.short.private.label")}</dt>
          <dd>{t("away.short.private.body")}</dd>
        </div>
      </dl>
      <Disclosure label={t("away.more")}>
        <dl className={styles.dl}>
          <div>
            <dt>{connectionName("simple")}</dt>
            <dd>
              <SeesText connection="simple" />
            </dd>
          </div>
          {canMostPrivate ? (
            <div>
              <dt>{connectionName("mostPrivate")}</dt>
              <dd>
                <SeesText connection="mostPrivate" />
              </dd>
            </div>
          ) : null}
          <div>
            <dt>{t("away.sees.either.label")}</dt>
            <dd>{t("away.sees.either")}</dd>
          </div>
        </dl>
        <PhoneFacts />
        <p className={styles.sub}>{t("away.lost")}</p>
        <p className={styles.sub}>{t("away.switch.warns")}</p>
        <p className={styles.sub}>{t("away.todo.leaving")}</p>
      </Disclosure>
      <ActionBar line={t("away.bar.first")}>
        <Button variant="primary" disabled={busy} onClick={onContinue}>
          {t("away.choice.continue")}
        </Button>
        <Button variant="quiet" onClick={onLeave}>
          {t("away.notNow")}
        </Button>
      </ActionBar>
      {switchDialog}
    </Frame>
  );
}

const SIGN_IN_WORDS: Record<
  SignInStep,
  Record<"whyHeading" | "why" | "name" | "title" | "body" | "waiting", CopyId>
> = {
  signIn: {
    whyHeading: "away.signIn.sees.heading",
    why: "away.signIn.sees",
    name: "away.signIn.name",
    title: "away.signIn.title",
    body: "away.signIn.body",
    waiting: "away.signIn.waiting",
  },
  approve: {
    whyHeading: "away.signIn.why.heading",
    why: "away.approve.why",
    name: "away.signIn.name",
    title: "away.approve.title",
    body: "away.approve.body",
    waiting: "away.approve.waiting",
  },
  allowLinks: {
    whyHeading: "away.signIn.why.heading",
    why: "away.links.why",
    name: "away.links.name",
    title: "away.links.title",
    body: "away.links.body",
    waiting: "away.links.waiting",
  },
};

interface SignInProps {
  connection: RemoteConnection;
  step: SignInStep;
  url: string;
  aside: ReactNode;
  onLeave: () => void;
}

function SignIn({ connection, step, url, aside, onLeave }: SignInProps) {
  const engine = useEngine();
  const toast = useToast();
  const act = useRemoteAct();
  const opened = useRef<string | null>(null);
  // The page opens by itself when a step starts, once; "Open the page again" is for the person.
  useEffect(() => {
    if (opened.current === step) return;
    opened.current = step;
    void engine.openRemoteSignIn().catch(() => undefined);
  }, [engine, step]);

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(url);
      toast.show({ message: t("away.signIn.copied") });
    } catch {
      toast.show({ message: t("away.signIn.copyFailed") });
    }
  };
  const links = step === "allowLinks";
  const words = SIGN_IN_WORDS[step];
  return (
    <Frame
      back={{ to: "/settings", label: t("screen.settings.title") }}
      aside={
        <>
          <h2 className={styles.label}>{t(words.whyHeading)}</h2>
          <p className={styles.muted}>{t(words.why)}</p>
          <p className={styles.muted}>{t(words.name)}</p>
          {aside}
        </>
      }
    >
      <StepLabel connection={connection} step={step} named />
      <h1 className="t-page-title">{t(words.title)}</h1>
      <p className={styles.body}>{t(words.body)}</p>
      {links ? <p className={styles.callout}>{t("away.links.switch")}</p> : null}
      <p className={styles.waiting} role="status">
        <span className={styles.pulse} aria-hidden="true" />
        {t(words.waiting)}
      </p>
      <p className={styles.muted}>
        {t("away.signIn.noBrowser")}{" "}
        <button type="button" className={`${styles.linkButton} hit`} onClick={() => void copy()}>
          {t("away.signIn.copy")}
        </button>
      </p>
      <ActionBar>
        <Button variant="secondary" onClick={() => void act(() => engine.openRemoteSignIn())}>
          {t("away.signIn.again")}
        </Button>
        <Button variant="quiet" onClick={onLeave}>
          {t("away.cancel")}
        </Button>
      </ActionBar>
    </Frame>
  );
}
