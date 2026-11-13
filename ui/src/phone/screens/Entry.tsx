import { type ClipboardEvent, useEffect, useRef, useState } from "react";
import { t } from "../../copy/t";
import { CODE_GROUP, CODE_LEN } from "../protocol";
import { CameraError, type ScannerEnv, startScan } from "../scanner";
import { Bar, Button, Icon, Page } from "./ui";

/** Nothing paired yet: how to start. */
export function Welcome({
  waiting,
  onScan,
  onType,
  onClear,
}: {
  waiting: number;
  onScan(): void;
  onType(): void;
  onClear(): void;
}) {
  return (
    <Page bar={<Bar />}>
      <main className="ph-main">
        <h1 className="ph-h1 ph-h1--big">{t("phone.welcome.title")}</h1>
        <p className="ph-body">{t("phone.welcome.body")}</p>
        <div className="ph-stack">
          <Button kind="primary" onClick={onScan}>
            <Icon name="camera" size={20} />
            {t("phone.welcome.scan")}
          </Button>
          <Button kind="quiet" onClick={onType}>
            {t("phone.welcome.type")}
          </Button>
        </div>
        {waiting > 0 && (
          <div className="ph-card">
            <p className="ph-card-title">{t("phone.welcome.waiting", { count: waiting })}</p>
            <Button kind="quiet" onClick={onClear}>
              {t("phone.removed.clear")}
            </Button>
          </div>
        )}
      </main>
    </Page>
  );
}

export function TooOld() {
  return (
    <Page bar={<Bar />}>
      <main className="ph-main">
        <div className="ph-live" role="alert">
          <span className="ph-icon-box" aria-hidden="true">
            <Icon name="unlink" size={24} />
          </span>
          <h1 className="ph-h1">{t("phone.tooOld.title")}</h1>
          <p className="ph-body">{t("phone.tooOld.body")}</p>
        </div>
      </main>
    </Page>
  );
}

/** Safari on iPhone keeps a home-screen app's storage apart, so it is added first. */
export function AddToHomeScreen({ onDone, onSkip }: { onDone(): void; onSkip(): void }) {
  return (
    <Page bar={<Bar />}>
      <main className="ph-main" style={{ gap: 16 }}>
        <span className="ph-lbl">{t("phone.iphone.label")}</span>
        <h1 className="ph-h1">{t("phone.iphone.title")}</h1>
        <ol className="ph-steps">
          {(
            [
              ["share", "phone.iphone.step1"],
              ["addBox", "phone.iphone.step2"],
              ["check", "phone.iphone.step3"],
            ] as const
          ).map(([icon, id]) => (
            <li key={id}>
              <span className="ph-step-ic" aria-hidden="true">
                <Icon name={icon} size={20} />
              </span>
              <span>{boldText(t(id))}</span>
            </li>
          ))}
        </ol>
        <div className="ph-card">
          <p className="ph-card-title">{t("phone.iphone.note.title")}</p>
          <p className="ph-hint ph-hint--ink2">{t("phone.iphone.note.body")}</p>
        </div>
        <div className="ph-spacer" />
        <div className="ph-stack">
          <Button kind="primary" onClick={onDone}>
            {t("phone.iphone.done")}
          </Button>
          <Button kind="quiet" onClick={onSkip}>
            {t("phone.iphone.skip")}
          </Button>
        </div>
      </main>
    </Page>
  );
}

function boldText(text: string) {
  return text
    .split("**")
    .map((part, i) => (i % 2 === 1 ? <strong key={part}>{part}</strong> : part));
}

const GROUPS = CODE_LEN / CODE_GROUP;

/** The code typed in four groups of six, with whatever the keyboard or a paste gives. */
export function TypedCode({
  busy,
  error,
  onSubmit,
  onBack,
}: {
  busy: boolean;
  error: "bad" | "noAnswer" | null;
  onSubmit(code: string): void;
  onBack(): void;
}) {
  const [groups, setGroups] = useState<string[]>(Array.from({ length: GROUPS }, () => ""));
  const refs = useRef<Array<HTMLInputElement | null>>([]);
  const count = groups.join("").length;
  const complete = count === CODE_LEN;

  const clean = (text: string) => text.replace(/[\s-]+/g, "").toUpperCase();

  const spread = (from: number, text: string) => {
    const next = [...groups];
    let rest = clean(text);
    for (let i = from; i < GROUPS && rest.length > 0; i += 1) {
      next[i] = rest.slice(0, CODE_GROUP);
      rest = rest.slice(CODE_GROUP);
    }
    setGroups(next);
    const last = Math.min(GROUPS - 1, from + Math.floor(clean(text).length / CODE_GROUP));
    refs.current[last]?.focus();
  };

  return (
    <Page bar={<Bar />}>
      <main className="ph-main" style={{ gap: 16 }}>
        <span className="ph-lbl">{t("phone.typed.label")}</span>
        <h1 className="ph-h1 ph-h1--small">{t("phone.typed.title")}</h1>
        <p className="ph-body">{t("phone.typed.lead")}</p>
        <form
          className="ph-stack"
          style={{ gap: 16 }}
          onSubmit={(e) => {
            e.preventDefault();
            if (complete && !busy) onSubmit(groups.join(""));
          }}
        >
          <fieldset className="ph-groups">
            <legend className="sr-only">{t("phone.typed.legend")}</legend>
            {groups.map((value, i) => {
              const id = `code-${i + 1}`;
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
                    inputMode="text"
                    onPaste={(e: ClipboardEvent<HTMLInputElement>) => {
                      const text = e.clipboardData.getData("text");
                      if (clean(text).length > CODE_GROUP) {
                        e.preventDefault();
                        spread(i, text);
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
          <p className="ph-hint">{t("phone.typed.hint")}</p>
          {error && (
            <p className="ph-error" role="alert">
              {error === "bad" ? t("phone.typed.bad") : t("phone.typed.noAnswer")}
            </p>
          )}
          <div className="ph-spacer" />
          <p className="ph-hint ph-hint--ink2">{t("phone.typed.next")}</p>
          <Button kind="primary" type="submit" disabled={!complete || busy}>
            {busy ? t("phone.typed.checking") : t("phone.typed.continue")}
          </Button>
          <Button kind="quiet" onClick={onBack}>
            {t("phone.cancel")}
          </Button>
        </form>
      </main>
    </Page>
  );
}

/** The camera, reading a code. `accept` says whether the text is the code this screen wants. */
export function Scan({
  purpose,
  env,
  accept,
  onSwitch,
  onCancel,
}: {
  purpose: "pair" | "relink";
  env: ScannerEnv;
  accept(text: string): boolean;
  onSwitch(): void;
  onCancel(): void;
}) {
  const video = useRef<HTMLVideoElement>(null);
  const [state, setState] = useState<"starting" | "looking" | "noCamera" | "wrong">("starting");
  const acceptRef = useRef(accept);
  acceptRef.current = accept;

  useEffect(() => {
    let stop: (() => void) | undefined;
    let gone = false;
    const el = video.current;
    if (!el) return;
    startScan(
      el,
      (text) => {
        if (acceptRef.current(text)) return;
        setState("wrong");
      },
      env,
    ).then(
      (scan) => {
        if (gone) return scan.stop();
        stop = scan.stop;
        setState("looking");
      },
      (e) => {
        if (e instanceof CameraError) setState("noCamera");
        else throw e;
      },
    );
    return () => {
      gone = true;
      stop?.();
    };
  }, [env]);

  const looking = purpose === "pair" ? "phone.scan.hint.pair" : "phone.scan.hint.relink";
  return (
    <div className="ph-screen ph-scan ph-screen--fixed">
      <header className="ph-bar">
        <h1 className="ph-bar-title">
          {purpose === "pair" ? t("phone.scan.title.pair") : t("phone.scan.title.relink")}
        </h1>
        <button type="button" className="ph-ib" onClick={onCancel}>
          {t("phone.cancel")}
        </button>
      </header>
      <div className="ph-camera" role="img" aria-label={t("phone.scan.camera")}>
        <video ref={video} playsInline muted />
        <div className="ph-frame" aria-hidden="true">
          <span
            style={{ left: 0, top: 0, borderWidth: "3px 0 0 3px", borderRadius: "6px 0 0 0" }}
          />
          <span
            style={{ right: 0, top: 0, borderWidth: "3px 3px 0 0", borderRadius: "0 6px 0 0" }}
          />
          <span
            style={{ left: 0, bottom: 0, borderWidth: "0 0 3px 3px", borderRadius: "0 0 0 6px" }}
          />
          <span
            style={{ right: 0, bottom: 0, borderWidth: "0 3px 3px 0", borderRadius: "0 0 6px 0" }}
          />
        </div>
        <p className="ph-scan-status" role="status">
          {state === "looking" && (
            <>
              <span
                className="ph-dot ph-dot--breathe"
                style={{ background: "#ece8df" }}
                aria-hidden="true"
              />
              {t("phone.scan.looking")}
            </>
          )}
          {state === "noCamera" && t("phone.scan.noCamera")}
          {state === "wrong" && t("phone.scan.wrong")}
        </p>
      </div>
      <div className="ph-scan-foot">
        <p>{t(looking)}</p>
        <Button onClick={onSwitch}>
          {purpose === "pair" ? t("phone.scan.type") : t("phone.scan.words")}
        </Button>
        <p className="ph-hint">{t("phone.scan.privacy")}</p>
      </div>
    </div>
  );
}

/** The four words of a new address, typed. */
export function RelinkWords({
  busy,
  error,
  onSubmit,
  onScan,
  onBack,
}: {
  busy: boolean;
  error: boolean;
  onSubmit(words: string[]): void;
  onScan(): void;
  onBack(): void;
}) {
  const [words, setWords] = useState(["", "", "", ""]);
  const complete = words.every((w) => w.trim() !== "");
  return (
    <Page
      bar={
        <header className="ph-bar ph-bar--back">
          <button
            type="button"
            className="ph-ib ph-ib--ink"
            onClick={onBack}
            aria-label={t("phone.relink.back")}
          >
            <Icon name="back" />
          </button>
          <h1 className="ph-bar-title">{t("phone.relink.title")}</h1>
        </header>
      }
    >
      <main className="ph-main ph-main--tight" style={{ paddingTop: 24, gap: 16 }}>
        <p className="ph-body">{t("phone.relink.lead")}</p>
        <form
          className="ph-stack"
          style={{ gap: 16, flex: 1 }}
          onSubmit={(e) => {
            e.preventDefault();
            if (complete && !busy) onSubmit(words);
          }}
        >
          <div className="ph-stack">
            {words.map((w, i) => {
              const id = `word-${i + 1}`;
              return (
                <div className="ph-word-field" key={id}>
                  <label htmlFor={id}>{i + 1}</label>
                  <input
                    id={id}
                    value={w}
                    aria-label={t("phone.relink.word", { n: i + 1 })}
                    autoCapitalize="none"
                    autoComplete="off"
                    autoCorrect="off"
                    spellCheck={false}
                    onChange={(e) => {
                      const next = [...words];
                      next[i] = e.target.value.replace(/[^A-Za-z0-9]/g, "");
                      setWords(next);
                    }}
                  />
                </div>
              );
            })}
          </div>
          <p className="ph-hint">{t("phone.relink.hint")}</p>
          {error && (
            <p className="ph-error" role="alert">
              {t("phone.relink.failed")}
            </p>
          )}
          <div className="ph-spacer" />
          <Button kind="primary" type="submit" disabled={!complete || busy}>
            {busy ? t("phone.relink.checking") : t("phone.relink.go")}
          </Button>
          <Button kind="quiet" onClick={onScan}>
            {t("phone.relink.scan")}
          </Button>
        </form>
      </main>
    </Page>
  );
}

/** A re-link code opened by the system camera: it cannot do the job, and says where to go. */
export function RelinkHere() {
  return (
    <Page bar={<Bar />}>
      <main className="ph-main">
        <span className="ph-icon-box" aria-hidden="true">
          <Icon name="pin" size={24} />
        </span>
        <h1 className="ph-h1">{t("phone.relink.here.title")}</h1>
        <p className="ph-body">{t("phone.relink.here.body")}</p>
      </main>
    </Page>
  );
}
