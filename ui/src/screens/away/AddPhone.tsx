import { useCallback, useEffect, useRef, useState } from "react";
import { goTo, useQueryParam } from "../../app/query";
import { ActionBar } from "../../components/ActionBar/ActionBar";
import { Button } from "../../components/Button/Button";
import { useToast } from "../../components/Toast/Toast";
import { formatDuration } from "../../copy/format";
import { t } from "../../copy/t";
import type { PairingOffer, PairingPhase, RemoteView } from "../../engine/bindings";
import { useEngine } from "../../engine/context";
import styles from "./Away.module.css";
import { PairingDialog } from "./PairingDialog";
import { Frame } from "./parts";
import { QrMatrix } from "./QrMatrix";
import { useRemoteAct } from "./useRemote";

const OFFER_MS = 5 * 60_000;

/** The typed code, in the four groups of six it is read out in. */
export const groups = (code: string): string[] => code.match(/.{1,6}/g) ?? [];

type Shown = "waiting" | "added" | "expired" | "refused" | "timedOut" | "spent" | "unavailable";

function shownFor(phase: PairingPhase | null, expired: boolean): Shown {
  switch (phase?.type) {
    case "allowed":
      return "added";
    case "expired":
      return "expired";
    case "refused":
      return "refused";
    case "timedOut":
      return "timedOut";
    case "spent":
      return "spent";
    default:
      return expired ? "expired" : "waiting";
  }
}

const MESSAGE = {
  expired: ["away.add.expired.title", "away.add.expired.body"],
  refused: ["away.add.refused.title", "away.add.refused.body"],
  timedOut: ["away.add.timedOut.title", "away.add.timedOut.body"],
  spent: ["away.add.spent.title", "away.add.spent.body"],
} as const;

/** #/settings/away/add: one code, one phone, and the person's own yes. */
export function AddPhone({ view }: { view: RemoteView }) {
  const engine = useEngine();
  const act = useRemoteAct();
  const toast = useToast();
  const first = useQueryParam("setup") === "1";
  const [offer, setOffer] = useState<PairingOffer | null>(null);
  const [failed, setFailed] = useState(false);
  const [now, setNow] = useState(() => Date.now());
  const [deciding, setDeciding] = useState(false);
  const [announced, setAnnounced] = useState("");
  const started = useRef(false);
  const added = useRef(false);
  const leaving = useRef({ first, devices: view.devices.length });
  leaving.current = { first, devices: view.devices.length };
  const on = view.state.type === "on";

  const begin = useCallback(async () => {
    setFailed(false);
    setOffer(null);
    try {
      setOffer(await engine.beginPairing());
    } catch {
      setFailed(true);
    }
  }, [engine]);

  useEffect(() => {
    if (!on || started.current) return;
    started.current = true;
    void begin();
  }, [on, begin]);

  // Leaving drops the code. If this was the first phone and none was added, the connection goes off again.
  useEffect(
    () => () => {
      void engine.cancelPairing().catch(() => undefined);
      if (leaving.current.first && !added.current && leaving.current.devices === 0) {
        void engine.stopRemote(false).catch(() => undefined);
      }
    },
    [engine],
  );

  useEffect(() => {
    if (!offer) return;
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, [offer]);

  const status =
    view.pairing && offer && view.pairing.pairingId === offer.pairingId ? view.pairing : null;
  const phase = status?.phase ?? null;
  if (phase?.type === "allowed") added.current = true;
  const remaining = offer ? Math.max(0, new Date(offer.expiresAt).getTime() - now) : OFFER_MS;
  const expired = offer !== null && remaining <= 0;
  const shown = shownFor(phase, expired);

  // Once a minute, in words, for someone who cannot see the clock.
  const minute = Math.ceil(remaining / 60_000);
  useEffect(() => {
    if (shown === "waiting" && offer) setAnnounced(t("away.add.minutesLeft", { count: minute }));
    else setAnnounced("");
  }, [minute, shown, offer]);

  const resultHeading = useRef<HTMLHeadingElement>(null);
  const deciding_ = useRef(false);
  // The result of an offer is what the person needs next, so focus goes to it once it appears.
  useEffect(() => {
    if (shown !== "waiting" || !on || failed) resultHeading.current?.focus();
  }, [shown, on, failed]);

  const decide = async (allow: boolean) => {
    if (phase?.type !== "waitingForYou" || deciding_.current) return;
    deciding_.current = true;
    setDeciding(true);
    await act(() => engine.decidePairing(phase.request.pairingId, allow));
    deciding_.current = false;
    setDeciding(false);
  };

  const copyCode = async () => {
    if (!offer) return;
    try {
      await navigator.clipboard.writeText(offer.typedCode);
      toast.show({ message: t("away.add.copied") });
    } catch {
      toast.show({ message: t("away.add.copyFailed") });
    }
  };

  const cancel = () => goTo("/settings/away");

  let body: React.ReactNode;
  if (!on || failed) {
    body = (
      <div className={styles.panel} role="alert">
        <h2 className={styles.statement} tabIndex={-1} ref={resultHeading}>
          {t("away.add.unavailable.title")}
        </h2>
        <p className={styles.body}>{t("away.add.unavailable.body")}</p>
        <ActionBar>
          <Button variant="primary" onClick={cancel}>
            {t("away.add.back")}
          </Button>
        </ActionBar>
      </div>
    );
  } else if (!offer) {
    body = (
      <p className={styles.body} role="status">
        {t("away.add.making")}
      </p>
    );
  } else if (shown === "added" && phase?.type === "allowed") {
    body = (
      <div className={styles.panel} role="status">
        <h2 className={styles.statement} tabIndex={-1} ref={resultHeading}>
          {t("away.add.added.title", { name: phase.deviceName })}
        </h2>
        <p className={styles.body}>{t("away.add.added.body", { name: phase.deviceName })}</p>
        <ActionBar>
          <Button variant="primary" onClick={() => goTo("/settings/away")}>
            {t("away.add.done")}
          </Button>
          <Button
            variant="quiet"
            onClick={() => {
              added.current = false;
              void begin();
            }}
          >
            {t("away.add.another")}
          </Button>
        </ActionBar>
      </div>
    );
  } else if (
    shown === "expired" ||
    shown === "refused" ||
    shown === "timedOut" ||
    shown === "spent"
  ) {
    const [title, text] = MESSAGE[shown];
    body = (
      <div className={styles.panel} role="status">
        <h2 className={styles.statement} tabIndex={-1} ref={resultHeading}>
          {t(title)}
        </h2>
        <p className={styles.body}>{t(text)}</p>
        <ActionBar>
          <Button variant="primary" onClick={() => void begin()}>
            {t("away.add.newCode")}
          </Button>
          <Button variant="quiet" onClick={cancel}>
            {t("away.cancel")}
          </Button>
        </ActionBar>
      </div>
    );
  } else {
    const address = offer.typedAddress;
    body = (
      <div className={styles.addCode}>
        <div className={styles.codeColumn}>
          <QrMatrix qr={offer.qr} label={t("away.add.qr")} />
          <div className={styles.countdown}>
            <div className={styles.bar} aria-hidden="true">
              <div
                className={styles.barFill}
                style={{ width: `${(remaining / OFFER_MS) * 100}%` }}
              />
            </div>
            <div className={styles.countdownText}>
              <span>{t("away.add.worksOnce")}</span>
              <span role="timer" className={styles.mono}>
                {t("away.add.left", { time: formatDuration(remaining) })}
              </span>
            </div>
          </div>
          <p className="sr-only" role="status">
            {announced}
          </p>
        </div>
        <div className={styles.codeText}>
          <p className={styles.statement}>{t("away.add.point")}</p>
          <ol className={styles.steps}>
            {(["away.add.step1", "away.add.step2", "away.add.step3"] as const).map((key, i) => (
              <li key={key}>
                <span className={styles.badge} aria-hidden="true">
                  {i + 1}
                </span>
                <span>{t(key)}</span>
              </li>
            ))}
          </ol>
          <p className={styles.muted}>{t("away.add.warning")}</p>
          <div className={styles.typed}>
            <span className={styles.label}>{t("away.add.cantScan")}</span>
            <span className={styles.sub}>
              {t("away.add.typedOpen")} <span className={styles.mono}>{address}</span>{" "}
              {t("away.add.typedCode")}
            </span>
            <ul className={styles.codeGroups} aria-label={t("away.add.codeLabel")}>
              {groups(offer.typedCode).map((g, i) => (
                <li key={`${i}-${g}`} className={styles.mono}>
                  {g}
                </li>
              ))}
            </ul>
            <div>
              <Button variant="quiet" onClick={() => void copyCode()}>
                {t("away.add.copy")}
              </Button>
            </div>
          </div>
          <ActionBar>
            <Button variant="secondary" onClick={cancel}>
              {t("away.cancel")}
            </Button>
          </ActionBar>
        </div>
      </div>
    );
  }

  return (
    <Frame back={{ to: "/settings/away", label: t("screen.away.title") }}>
      <h1 className="t-page-title">{t("away.add.title")}</h1>
      {body}
      {first ? <p className={styles.muted}>{t("away.todo.leaving")}</p> : null}
      {phase?.type === "waitingForYou" ? (
        <PairingDialog
          request={phase.request}
          busy={deciding}
          onDecide={(allow) => void decide(allow)}
        />
      ) : null}
    </Frame>
  );
}
