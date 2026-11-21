import { type ClipboardEvent, useEffect, useRef, useState } from "react";
import { tc } from "../../components/ControlChip/copy";
import { t } from "../../copy/t";
import { CODE_GROUP, CODE_LEN } from "../protocol";
import { Bar, Button, Icon, Page } from "../screens/ui";

/** Four words, or something shaped like a name: what lets "Next" light up. */
export function addressReady(text: string): boolean {
  const s = text.trim().toLowerCase();
  if (s.includes(".")) {
    const labels = s.split(".");
    const top = labels[labels.length - 1] ?? "";
    return (
      labels.length >= 2 &&
      labels.every((l) => /^[a-z0-9]([a-z0-9-]*[a-z0-9])?$/.test(l)) &&
      /[a-z]/.test(top)
    );
  }
  const words = s.split(/[\s-]+/).filter((w) => w !== "");
  return words.length === 4 && words.every((w) => /^[a-z0-9]{2,32}$/.test(w));
}

function useHeading(key: string) {
  const heading = useRef<HTMLHeadingElement>(null);
  useEffect(() => {
    void key;
    heading.current?.focus();
  }, [key]);
  return heading;
}

/** Not paired yet: Nocturne lives on the computer, and two ways in. */
export function NativeWelcome({
  waiting,
  unreached = false,
  wrongCode = false,
  onScan,
  onType,
  onSite,
  onClear,
}: {
  waiting: number;
  unreached?: boolean;
  /** The last scan read a code that is not from Nocturne. */
  wrongCode?: boolean;
  onScan(): void;
  onType(): void;
  onSite(): void;
  onClear(): void;
}) {
  const heading = useHeading("welcome");
  return (
    <Page bar={<Bar />}>
      <main className="ph-main">
        <h1 className="ph-h1 ph-h1--big" tabIndex={-1} ref={heading}>
          {t("phone.native.welcome.title")}
        </h1>
        <p className="ph-body">{t("phone.native.welcome.lead")}</p>
        <ol className="ph-native-steps">
          <li>
            <span className="ph-rc">1</span>
            <span className="ph-body">{tc("phone.native.welcome.step1")}</span>
          </li>
          <li>
            <span className="ph-rc">2</span>
            <span className="ph-body">{t("phone.native.welcome.step2")}</span>
          </li>
        </ol>
        {unreached && (
          <p className="ph-hint" role="status">
            {t("phone.welcome.unreached")}
          </p>
        )}
        {waiting > 0 && (
          <div className="ph-card">
            <p className="ph-card-title">{t("phone.welcome.waiting", { count: waiting })}</p>
            <Button kind="quiet" onClick={onClear}>
              {t("phone.removed.clear")}
            </Button>
          </div>
        )}
        <div className="ph-spacer" />
        {wrongCode && (
          <p className="ph-error" role="alert">
            {t("phone.scan.wrong")}
          </p>
        )}
        <div className="ph-stack">
          <Button kind="primary" onClick={onScan}>
            <Icon name="camera" size={20} />
            {t("phone.native.welcome.scan")}
          </Button>
          <Button kind="quiet" onClick={onType}>
            {t("phone.native.welcome.type")}
          </Button>
        </div>
        <p className="ph-hint ph-hint--ink2">{t("phone.native.welcome.foot")}</p>
        <button type="button" className="ph-native-link" onClick={onSite}>
          {t("phone.native.welcome.link")}
        </button>
      </main>
    </Page>
  );
}

/** The camera was refused: the phone's settings, or typing. */
export function CameraOff({
  onSettings,
  onType,
  onBack,
}: {
  onSettings(): void;
  onType(): void;
  onBack(): void;
}) {
  const heading = useHeading("cameraOff");
  return (
    <Page bar={<Bar />}>
      <main className="ph-main">
        <div className="ph-live" role="alert" style={{ flex: "none" }}>
          <span className="ph-icon-box" aria-hidden="true">
            <Icon name="camera" size={24} />
          </span>
          <h1 className="ph-h1" tabIndex={-1} ref={heading}>
            {t("phone.native.cameraOff.title")}
          </h1>
          <p className="ph-body">{t("phone.native.cameraOff.body")}</p>
        </div>
        <div className="ph-spacer" />
        <div className="ph-stack">
          <Button kind="primary" onClick={onSettings}>
            {t("phone.native.cameraOff.settings")}
          </Button>
          <Button kind="quiet" onClick={onType}>
            {t("phone.native.cameraOff.type")}
          </Button>
          <Button kind="quiet" onClick={onBack}>
            {t("phone.cancel")}
          </Button>
        </div>
      </main>
    </Page>
  );
}

/** Typing, first screen: the computer's address, four words or a name. */
export function AddressStep({
  initial,
  busy,
  error,
  onNext,
  onScan,
  onBack,
}: {
  initial: string;
  busy: boolean;
  error: "incomplete" | "noAnswer" | null;
  onNext(text: string): void;
  onScan(): void;
  onBack(): void;
}) {
  const [text, setText] = useState(initial);
  const heading = useHeading("address");
  const ready = addressReady(text);
  const named = text.includes(".");
  const message =
    error === "incomplete"
      ? t("phone.native.address.incomplete")
      : error === "noAnswer"
        ? t(
            named
              ? "phone.native.address.noAnswer.private"
              : "phone.native.address.noAnswer.simple",
          )
        : null;
  return (
    <Page bar={<Bar />}>
      <main className="ph-main" style={{ gap: 16 }}>
        <span className="ph-lbl">{t("phone.typed.label")}</span>
        <h1 className="ph-h1 ph-h1--small" tabIndex={-1} ref={heading}>
          {t("phone.native.address.title")}
        </h1>
        <p className="ph-body">{t("phone.native.address.lead")}</p>
        <form
          className="ph-stack"
          style={{ gap: 16 }}
          onSubmit={(e) => {
            e.preventDefault();
            if (ready && !busy) onNext(text);
          }}
        >
          <div className="ph-field">
            <label htmlFor="paddress">{t("phone.native.address.title")}</label>
            <input
              id="paddress"
              className="ph-input"
              value={text}
              placeholder={t("phone.native.address.placeholder")}
              autoCapitalize="none"
              autoComplete="off"
              autoCorrect="off"
              spellCheck={false}
              inputMode="url"
              aria-invalid={message ? true : undefined}
              aria-describedby={message ? "paddress-error" : "paddress-hint"}
              onChange={(e) => setText(e.target.value)}
            />
            <p id="paddress-hint" className="ph-hint">
              {t("phone.native.address.hint")}
            </p>
            {message && (
              <p id="paddress-error" className="ph-error" role="alert">
                {message}
              </p>
            )}
          </div>
          <div className="ph-spacer" />
          <Button kind="primary" type="submit" disabled={!ready || busy}>
            {busy ? t("phone.typed.checking") : t("phone.native.address.next")}
          </Button>
          <Button kind="quiet" onClick={onScan}>
            {t("phone.native.scanInstead")}
          </Button>
          <Button kind="quiet" onClick={onBack}>
            {t("phone.cancel")}
          </Button>
        </form>
      </main>
    </Page>
  );
}

const GROUPS = CODE_LEN / CODE_GROUP;

/** Typing, second screen: the 24 characters in four groups of six. */
export function CodeStep({
  busy,
  error,
  onConnect,
  onScan,
  onBack,
}: {
  busy: boolean;
  error: "bad" | "noAnswer" | null;
  onConnect(code: string): void;
  onScan(): void;
  onBack(): void;
}) {
  const [groups, setGroups] = useState<string[]>(Array.from({ length: GROUPS }, () => ""));
  const refs = useRef<Array<HTMLInputElement | null>>([]);
  const heading = useHeading("code");
  const complete = groups.join("").length === CODE_LEN;
  const clean = (text: string) => text.replace(/[\s-]+/g, "").toUpperCase();

  const spread = (from: number, text: string) => {
    const next = [...groups];
    let rest = clean(text);
    for (let i = from; i < GROUPS && rest.length > 0; i += 1) {
      next[i] = rest.slice(0, CODE_GROUP);
      rest = rest.slice(CODE_GROUP);
    }
    setGroups(next);
  };

  return (
    <Page bar={<Bar />}>
      <main className="ph-main" style={{ gap: 16 }}>
        <span className="ph-lbl">{t("phone.typed.label")}</span>
        <h1 className="ph-h1 ph-h1--small" tabIndex={-1} ref={heading}>
          {t("phone.native.code.title")}
        </h1>
        <p className="ph-body">{t("phone.native.code.lead")}</p>
        <form
          className="ph-stack"
          style={{ gap: 16 }}
          onSubmit={(e) => {
            e.preventDefault();
            if (complete && !busy) onConnect(groups.join(""));
          }}
        >
          <fieldset className="ph-groups">
            <legend className="sr-only">{t("phone.typed.legend")}</legend>
            {groups.map((value, i) => {
              const id = `ncode-${i + 1}`;
              return (
                <div className="ph-group" key={id}>
                  <label htmlFor={id}>{t("phone.typed.group", { n: i + 1 })}</label>
                  <input
                    id={id}
                    ref={(el) => {
                      refs.current[i] = el;
                    }}
                    value={value}
                    maxLength={CODE_GROUP}
                    autoCapitalize="characters"
                    autoComplete="off"
                    autoCorrect="off"
                    spellCheck={false}
                    onPaste={(e: ClipboardEvent<HTMLInputElement>) => {
                      const pasted = e.clipboardData.getData("text");
                      if (clean(pasted).length > CODE_GROUP) {
                        e.preventDefault();
                        spread(i, pasted);
                      }
                    }}
                    onChange={(e) => {
                      const next = [...groups];
                      next[i] = clean(e.target.value).slice(0, CODE_GROUP);
                      setGroups(next);
                      if (next[i]?.length === CODE_GROUP && i < GROUPS - 1)
                        refs.current[i + 1]?.focus();
                    }}
                  />
                </div>
              );
            })}
          </fieldset>
          <p className="ph-hint">{t("phone.native.code.hint")}</p>
          {error && (
            <p className="ph-error" role="alert">
              {error === "bad" ? t("phone.typed.bad") : t("phone.typed.noAnswer")}
            </p>
          )}
          <div className="ph-spacer" />
          <p className="ph-hint ph-hint--ink2">{t("phone.typed.warning")}</p>
          <Button kind="primary" type="submit" disabled={!complete || busy}>
            {busy ? t("phone.typed.checking") : t("phone.native.code.connect")}
          </Button>
          <Button kind="quiet" onClick={onScan}>
            {t("phone.native.scanInstead")}
          </Button>
          <Button kind="quiet" onClick={onBack}>
            {t("phone.cancel")}
          </Button>
        </form>
      </main>
    </Page>
  );
}
