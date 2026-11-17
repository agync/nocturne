import { type FormEvent, useId, useRef, useState } from "react";
import { ActionBar } from "../../components/ActionBar/ActionBar";
import { Button } from "../../components/Button/Button";
import { type CopyId, t } from "../../copy/t";
import type { CheckOutcome, OnlineForm } from "../../engine/bindings";
import { useEngine } from "../../engine/context";
import styles from "./Online.module.css";
import { Frame, LockNote } from "./parts";
import { StepAside } from "./SetupBits";
import { sentence } from "./when";

/** What the person has typed. The key lives here and in the check's call, nowhere else. */
export type ConnectForm = OnlineForm;

/** The words for each way a check can end, other than working: a heading and what to do next. */
const FAILURE: Record<
  Exclude<CheckOutcome["type"], "works" | "cancelled">,
  { title: CopyId; detail: CopyId; field: "baseUrl" | "key" | "model" | null }
> = {
  unreachable: {
    title: "advanced.online.connect.unreachable",
    detail: "advanced.online.connect.unreachable.detail",
    field: "baseUrl",
  },
  keyRefused: {
    title: "advanced.online.connect.keyRefused",
    detail: "advanced.online.connect.keyRefused.detail",
    field: "key",
  },
  noCredit: {
    title: "advanced.online.connect.noCredit",
    detail: "advanced.online.connect.noCredit.detail",
    field: "key",
  },
  modelMissing: {
    title: "advanced.online.connect.modelMissing",
    detail: "advanced.online.connect.modelMissing.detail",
    field: "model",
  },
  noInternet: {
    title: "advanced.online.connect.noInternet",
    detail: "advanced.online.connect.noInternet.detail",
    field: null,
  },
  notHttps: {
    title: "advanced.online.connect.notHttps",
    detail: "advanced.online.connect.notHttps.detail",
    field: "baseUrl",
  },
  addressNotAllowed: {
    title: "advanced.online.connect.addressNotAllowed",
    detail: "advanced.online.connect.addressNotAllowed.detail",
    field: "baseUrl",
  },
  busy: {
    title: "advanced.online.connect.busy",
    detail: "advanced.online.connect.busy.detail",
    field: null,
  },
  serverError: {
    title: "advanced.online.connect.serverError",
    detail: "advanced.online.connect.serverError.detail",
    field: null,
  },
  emptyReply: {
    title: "advanced.online.connect.emptyReply",
    detail: "advanced.online.connect.emptyReply.detail",
    field: "model",
  },
};

/** The host of the address typed so far, or "" while it is not an address yet. */
export function hostOfAddress(address: string): string {
  try {
    return new URL(address.trim()).host;
  } catch {
    return "";
  }
}

interface Props {
  form: ConnectForm;
  setForm: (next: ConnectForm) => void;
  outcome: CheckOutcome | null;
  setOutcome: (next: CheckOutcome | null) => void;
  /** A check has started, so leaving the setup must tell the computer to drop what it was given. */
  onChecking: () => void;
  onNext: () => void;
  onLeave: () => void;
  back: { to: string; label: string };
}

/**
 * O4, under Advanced details: the address, the key and the name to use, with a check that must
 * pass before going on.
 */
export function Connect({
  form,
  setForm,
  outcome,
  setOutcome,
  onChecking,
  onNext,
  onLeave,
  back,
}: Props) {
  const engine = useEngine();
  const id = useId();
  const [checking, setChecking] = useState(false);
  const [shown, setShown] = useState(false);
  const live = useRef(form);
  live.current = form;
  const region = useRef<HTMLDivElement>(null);
  const host = hostOfAddress(form.baseUrl);
  const filled = form.baseUrl.trim() !== "" && form.key.trim() !== "" && form.model.trim() !== "";
  const works = outcome?.type === "works";
  const failure =
    outcome && outcome.type !== "works" && outcome.type !== "cancelled"
      ? FAILURE[outcome.type]
      : null;
  const named = host || sentence(t("advanced.online.connect.thatAddress"));

  const edit = (patch: Partial<ConnectForm>) => {
    setForm({ ...form, ...patch });
    if (outcome) setOutcome(null);
  };

  const check = async (event?: FormEvent) => {
    event?.preventDefault();
    if (!filled || checking) return;
    const asked = {
      baseUrl: form.baseUrl.trim(),
      key: form.key.trim(),
      model: form.model.trim(),
    };
    setChecking(true);
    setOutcome(null);
    onChecking();
    region.current?.focus({ preventScroll: true });
    let result: CheckOutcome;
    try {
      result = await engine.checkOnline(asked);
    } catch {
      result = { type: "unreachable" };
    }
    setChecking(false);
    // An answer about a form that is no longer on screen says nothing about this one.
    const now = live.current;
    const same =
      now.baseUrl.trim() === asked.baseUrl &&
      now.key.trim() === asked.key &&
      now.model.trim() === asked.model;
    if (!same || result.type === "cancelled") return;
    setOutcome(result);
    // The computer holds the key now; the page does not keep a second copy.
    if (result.type === "works") setForm({ ...now, key: "" });
    region.current?.focus({ preventScroll: true });
  };

  const invalid = (field: "baseUrl" | "key" | "model") => failure?.field === field;
  const errorId = `${id}-error`;
  const describe = (field: "baseUrl" | "key" | "model", hint?: string) =>
    [hint, invalid(field) ? errorId : null].filter(Boolean).join(" ") || undefined;

  return (
    <Frame
      back={back}
      aside={
        <>
          <StepAside
            heading={t("advanced.online.connect.aside.heading")}
            paragraphs={[
              t("advanced.online.connect.aside.any"),
              t("advanced.online.connect.aside.neutral"),
            ]}
          />
          <LockNote>{t("advanced.online.connect.aside.lock")}</LockNote>
        </>
      }
    >
      <h1 className={styles.stepTitle} tabIndex={-1}>
        {t("advanced.online.connect.title")}
      </h1>
      <p className={styles.body}>{t("advanced.online.connect.lead")}</p>
      <form className={styles.fields} onSubmit={(event) => void check(event)} noValidate>
        <div className={styles.field}>
          <label className={styles.fieldLabel} htmlFor={`${id}-address`}>
            {t("advanced.online.connect.address")}
            <small>{t("advanced.online.connect.address.word")}</small>
          </label>
          <input
            id={`${id}-address`}
            className={styles.input}
            type="url"
            placeholder="https://"
            autoComplete="off"
            spellCheck={false}
            value={form.baseUrl}
            aria-invalid={invalid("baseUrl")}
            aria-describedby={describe("baseUrl", `${id}-address-hint`)}
            readOnly={checking}
            onChange={(event) => edit({ baseUrl: event.target.value })}
          />
          <span id={`${id}-address-hint`} className={styles.sub}>
            {t("advanced.online.connect.address.hint")}
          </span>
        </div>
        <div className={styles.field}>
          <label className={styles.fieldLabel} htmlFor={`${id}-key`}>
            {t("advanced.online.connect.key")}
            <small>{t("advanced.online.connect.key.word")}</small>
          </label>
          <div className={styles.fieldRow}>
            <input
              id={`${id}-key`}
              className={styles.input}
              type={shown ? "text" : "password"}
              autoComplete="new-password"
              autoCapitalize="off"
              spellCheck={false}
              value={form.key}
              aria-invalid={invalid("key")}
              aria-describedby={describe("key", `${id}-key-hint`)}
              readOnly={checking}
              onChange={(event) => edit({ key: event.target.value })}
            />
            <Button variant="secondary" touch aria-pressed={shown} onClick={() => setShown(!shown)}>
              {t(shown ? "advanced.online.connect.key.hide" : "advanced.online.connect.key.show")}
            </Button>
          </div>
          <span id={`${id}-key-hint`} className={styles.sub}>
            {t("advanced.online.connect.key.hint")}
          </span>
        </div>
        <div className={styles.field}>
          <label className={styles.fieldLabel} htmlFor={`${id}-model`}>
            {t("advanced.online.connect.name")}
            <small>{t("advanced.online.connect.name.word")}</small>
          </label>
          <input
            id={`${id}-model`}
            className={styles.input}
            type="text"
            autoComplete="off"
            autoCapitalize="off"
            spellCheck={false}
            placeholder={t("advanced.online.connect.name.placeholder")}
            value={form.model}
            aria-invalid={invalid("model")}
            aria-describedby={describe("model")}
            readOnly={checking}
            onChange={(event) => edit({ model: event.target.value })}
          />
        </div>
        <button type="submit" hidden />
      </form>

      <div ref={region} tabIndex={-1} role="status" aria-live="polite">
        {checking ? (
          <div className={styles.result}>
            <span className={styles.dot} aria-hidden="true" />
            <span className={styles.resultText}>
              <span className={styles.resultTitle}>
                {host
                  ? t("advanced.online.connect.checkingWith", { host })
                  : t("advanced.online.connect.checkingNoHost")}
              </span>
              <span className={styles.sub}>{t("advanced.online.connect.checkingDetail")}</span>
            </span>
          </div>
        ) : null}
        {outcome?.type === "works" ? (
          <div className={styles.result} data-kind="ok">
            <span className={styles.resultText}>
              <span className={styles.resultTitle}>
                {t("advanced.online.connect.works", { host: outcome.host, used: outcome.model })}
              </span>
              <span className={styles.sub}>
                {t("advanced.online.connect.works.detail", { host: outcome.host })}
              </span>
            </span>
          </div>
        ) : null}
        {outcome && failure ? (
          <div className={styles.result} data-kind="bad" id={errorId}>
            <span className={styles.resultText}>
              <span className={styles.resultTitle}>
                {t(failure.title, { host: named, used: form.model.trim() })}
              </span>
              <span className={`${styles.body} ${styles.strong}`}>{t(failure.detail)}</span>
            </span>
          </div>
        ) : null}
      </div>

      <ActionBar
        lineId={`${id}-why`}
        line={works || checking ? undefined : t("advanced.online.connect.needsCheck")}
      >
        {checking ? (
          <>
            <Button variant="secondary" disabled>
              {t("advanced.online.connect.checking")}
            </Button>
            <Button variant="quiet" onClick={() => void engine.cancelOnlineCheck()}>
              {t("advanced.online.connect.cancel")}
            </Button>
          </>
        ) : (
          <>
            {works ? (
              <Button variant="primary" onClick={onNext}>
                {t("online.continue")}
              </Button>
            ) : (
              <Button variant="primary" disabled aria-describedby={`${id}-why`}>
                {t("online.continue")}
              </Button>
            )}
            <Button
              variant={works ? "quiet" : "secondary"}
              disabled={!filled}
              onClick={() => void check()}
            >
              {t(
                works || failure
                  ? "advanced.online.connect.checkAgain"
                  : "advanced.online.connect.check",
              )}
            </Button>
          </>
        )}
        <Button variant="quiet" onClick={onLeave}>
          {t("online.notNow")}
        </Button>
      </ActionBar>
    </Frame>
  );
}
