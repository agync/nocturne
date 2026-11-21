import { useEffect, useRef, useState } from "react";
import { tc } from "../../components/ControlChip/copy";
import { t } from "../../copy/t";
import { checkDeviceName, cleanDeviceName, Refusal } from "../crypto";
import type { PairedDevice } from "../engine";
import type { PairingLike, Services } from "../services";
import { Bar, Button, Icon, Page, PrivacyLine, Rich } from "./ui";

type Step =
  | { name: "name"; busy?: boolean; error?: boolean }
  | { name: "check"; words: string[]; label: string }
  | { name: "allowed"; label: string }
  | { name: "problem"; kind: "refused" | "expired" | "spent" };

interface InstallPrompt extends Event {
  prompt(): Promise<void>;
}

function useInstallPrompt() {
  const saved = useRef<InstallPrompt | null>(null);
  const [available, setAvailable] = useState(false);
  useEffect(() => {
    const on = (e: Event) => {
      e.preventDefault();
      saved.current = e as InstallPrompt;
      setAvailable(true);
    };
    window.addEventListener("beforeinstallprompt", on);
    return () => window.removeEventListener("beforeinstallprompt", on);
  }, []);
  return {
    available,
    async prompt() {
      await saved.current?.prompt();
    },
  };
}

interface PairProps {
  services: Services;
  pairing: PairingLike;
  /** The person is done; the app opens with the new key. */
  onPaired(device: PairedDevice | null): void;
  /** Back to the start (a new code is needed). */
  onExit(): void;
}

/** Name this phone, check the words, wait for the computer, and be told the outcome. */
export function Pair({ services, pairing, onPaired, onExit }: PairProps) {
  const [step, setStep] = useState<Step>({ name: "name" });
  const [name, setName] = useState(() => cleanDeviceName(t("phone.pair.default")));
  const typed = useRef(false);
  const install = useInstallPrompt();
  const native = services.platform.native === true;

  // The phone's own name, where the app can read it, unless the person has typed already.
  useEffect(() => {
    let gone = false;
    services.deviceName?.().then(
      (own) => {
        const clean = own ? cleanDeviceName(own.trim()) : "";
        if (!gone && !typed.current && clean !== "") setName(clean);
      },
      () => undefined,
    );
    return () => {
      gone = true;
    };
  }, [services]);
  const device = useRef<PairedDevice | null>(null);
  const heading = useRef<HTMLHeadingElement>(null);

  // Each step's heading takes focus, so a screen reader hears where it is.
  const key = step.name === "problem" ? `problem-${step.kind}` : step.name;
  useEffect(() => {
    void key;
    heading.current?.focus();
  }, [key]);

  const connect = async () => {
    const label = name.trim();
    try {
      checkDeviceName(label);
    } catch (e) {
      if (e instanceof Refusal) return setStep({ name: "name", error: true });
      throw e;
    }
    setStep({ name: "name", busy: true });
    const result = await pairing.connect(label, (words) =>
      setStep({ name: "check", words, label }),
    );
    if (result.kind === "allowed") {
      device.current = result.device;
      void services.persist();
      setStep({ name: "allowed", label });
    } else {
      setStep({ name: "problem", kind: result.kind });
    }
  };

  if (step.name === "name") {
    return (
      <Page bar={<Bar />}>
        <main className="ph-main">
          <span className="ph-lbl">{t("phone.pair.label")}</span>
          <h1 className="ph-h1 ph-h1--big" tabIndex={-1} ref={heading}>
            {t("phone.pair.title")}
          </h1>
          <p className="ph-body">{t("phone.pair.lead")}</p>
          <form
            className="ph-stack"
            onSubmit={(e) => {
              e.preventDefault();
              if (!step.busy) void connect();
            }}
          >
            <div className="ph-field">
              <label htmlFor="pname">{t("phone.pair.field")}</label>
              <input
                id="pname"
                className="ph-input"
                value={name}
                autoComplete="off"
                autoCapitalize="words"
                spellCheck={false}
                aria-invalid={step.error ? true : undefined}
                aria-describedby={step.error ? "pname-error" : undefined}
                onChange={(e) => {
                  typed.current = true;
                  setName(cleanDeviceName(e.target.value.trimStart()));
                }}
              />
              {step.error && (
                <p id="pname-error" className="ph-error" role="alert">
                  {t("phone.pair.badName")}
                </p>
              )}
            </div>
            <Button kind="primary" type="submit" disabled={step.busy || name.trim() === ""}>
              {step.busy ? t("phone.pair.connecting") : t("phone.pair.connect")}
            </Button>
          </form>
          <p className="ph-hint">{t("phone.pair.next")}</p>
          {native ? null : <p className="ph-hint">{t("phone.pair.app")}</p>}
          <div className="ph-spacer" />
          <PrivacyLine id="phone.pair.private" />
        </main>
      </Page>
    );
  }

  if (step.name === "check") {
    return (
      <Page bar={<Bar />}>
        <main className="ph-main">
          <span className="ph-lbl">{t("phone.check.label", { name: step.label })}</span>
          <h1 className="ph-h1 ph-h1--small" tabIndex={-1} ref={heading}>
            {t("phone.check.title")}
          </h1>
          <ul
            className="ph-words"
            aria-label={t("phone.check.words", { words: step.words.join(", ") })}
          >
            {step.words.map((w) => (
              <li className="ph-word" key={w}>
                {w}
              </li>
            ))}
          </ul>
          <p className="ph-body">{tc("phone.check.body")}</p>
          <p className="ph-hint ph-hint--ink2">{t("phone.check.warning")}</p>
          <p
            className="ph-status"
            role="status"
            style={{ color: "var(--ink)", fontSize: "0.9375rem" }}
          >
            <span className="ph-dot ph-dot--breathe" aria-hidden="true" />
            {t("phone.check.waiting")}
          </p>
          <div className="ph-spacer" />
          <Button kind="quiet" onClick={onExit}>
            {t("phone.cancel")}
          </Button>
        </main>
      </Page>
    );
  }

  if (step.name === "allowed" && native) {
    return (
      <Page bar={<Bar />}>
        <main className="ph-main">
          <span
            className="ph-icon-box ph-icon-box--round"
            aria-hidden="true"
            style={{ borderRadius: "50%" }}
          >
            <Icon name="check" size={26} />
          </span>
          <h1 className="ph-h1 ph-h1--big" tabIndex={-1} ref={heading}>
            {t("phone.allowed.title", { name: step.label })}
          </h1>
          <p className="ph-body">{t("phone.app.allowed.body")}</p>
          <div className="ph-spacer" />
          <Button kind="primary" onClick={() => onPaired(device.current)}>
            {t("phone.app.allowed.continue")}
          </Button>
        </main>
      </Page>
    );
  }

  if (step.name === "allowed") {
    return (
      <Page bar={<Bar />}>
        <main className="ph-main">
          <span
            className="ph-icon-box ph-icon-box--round"
            aria-hidden="true"
            style={{ borderRadius: "50%" }}
          >
            <Icon name="check" size={26} />
          </span>
          <h1 className="ph-h1 ph-h1--big" tabIndex={-1} ref={heading}>
            {t("phone.allowed.title", { name: step.label })}
          </h1>
          <p className="ph-body">{t("phone.allowed.body")}</p>
          <ol className="ph-steps">
            <li>
              <span className="ph-rc">1</span>
              <span>{t("phone.allowed.step1")}</span>
            </li>
            <li>
              <span className="ph-rc">2</span>
              <span>
                <Rich id="phone.allowed.step2" />
              </span>
            </li>
          </ol>
          <div className="ph-spacer" />
          <div className="ph-stack">
            {install.available && (
              <Button kind="primary" onClick={() => void install.prompt()}>
                {t("phone.allowed.add")}
              </Button>
            )}
            <Button
              kind={install.available ? "quiet" : "primary"}
              onClick={() => onPaired(device.current)}
            >
              {t("phone.allowed.skip")}
            </Button>
          </div>
        </main>
      </Page>
    );
  }

  return (
    <Page bar={<Bar />}>
      <main className="ph-main">
        <div className="ph-live" role="alert" style={{ flex: "none" }}>
          <span className="ph-icon-box" aria-hidden="true">
            <Icon name="unlink" size={24} />
          </span>
          <h1 className="ph-h1" tabIndex={-1} ref={heading}>
            {PROBLEM[step.kind].title()}
          </h1>
          <p className="ph-body">{PROBLEM[step.kind].body()}</p>
        </div>
        <div className="ph-spacer" />
        <Button kind="primary" onClick={onExit}>
          {t("phone.problem.again")}
        </Button>
      </main>
    </Page>
  );
}

const PROBLEM = {
  refused: {
    title: () => t("phone.problem.refused.title"),
    body: () => t("phone.problem.refused.body"),
  },
  expired: {
    title: () => t("phone.problem.expired.title"),
    body: () => t("phone.problem.expired.body"),
  },
  spent: {
    title: () => t("phone.problem.spent.title"),
    body: () => t("phone.problem.spent.body"),
  },
} as const;
