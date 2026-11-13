import { type FormEvent, useId, useState } from "react";
import { Button } from "../../components/Button/Button";
import { type CopyId, t } from "../../copy/t";
import type { CheckOutcome, OnlineForm } from "../../engine/bindings";
import { useEngine } from "../../engine/context";
import styles from "./Online.module.css";
import { Frame, LockNote } from "./parts";
import { StepAside, StepLabel } from "./SetupBits";
import { sentence } from "./when";

/** What the person has typed. The key lives here and in the check's call, nowhere else. */
export type ConnectForm = OnlineForm;

/** The words for each way a check can end, other than working: a heading and what to do next. */
const FAILURE: Record<
  Exclude<CheckOutcome["type"], "works" | "cancelled">,
  { title: CopyId; detail: CopyId; field: "baseUrl" | "key" | "model" | null }
> = {
  unreachable: {
    title: "online.connect.unreachable",
    detail: "online.connect.unreachable.detail",
    field: "baseUrl",
  },
  keyRefused: {
    title: "online.connect.keyRefused",
    detail: "online.connect.keyRefused.detail",
    field: "key",
  },
  noCredit: {
    title: "online.connect.noCredit",
    detail: "online.connect.noCredit.detail",
    field: "key",
  },
  modelMissing: {
    title: "online.connect.modelMissing",
    detail: "online.connect.modelMissing.detail",
    field: "model",
  },
  noInternet: {
    title: "online.connect.noInternet",
    detail: "online.connect.noInternet.detail",
    field: null,
  },
  notHttps: {
    title: "online.connect.notHttps",
    detail: "online.connect.notHttps.detail",
    field: "baseUrl",
  },
  addressNotAllowed: {
    title: "online.connect.addressNotAllowed",
    detail: "online.connect.addressNotAllowed.detail",
    field: "baseUrl",
  },
  busy: { title: "online.connect.busy", detail: "online.connect.busy.detail", field: null },
  serverError: {
    title: "online.connect.serverError",
    detail: "online.connect.serverError.detail",
    field: null,
  },
  emptyReply: {
    title: "online.connect.emptyReply",
    detail: "online.connect.emptyReply.detail",
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
}

/** O4: the address, the key and the name to use, with a check that must pass before going on. */
export function Connect({
  form,
  setForm,
  outcome,
  setOutcome,
  onChecking,
  onNext,
  onLeave,
}: Props) {
  const engine = useEngine();
  const id = useId();
  const [checking, setChecking] = useState(false);
  const [shown, setShown] = useState(false);
  const host = hostOfAddress(form.baseUrl);
  const filled = form.baseUrl.trim() !== "" && form.key.trim() !== "" && form.model.trim() !== "";
  const works = outcome?.type === "works";
  const failure =
    outcome && outcome.type !== "works" && outcome.type !== "cancelled"
      ? FAILURE[outcome.type]
      : null;
  const named = host || sentence(t("online.connect.thatAddress"));

  const edit = (patch: Partial<ConnectForm>) => {
    setForm({ ...form, ...patch });
    if (outcome) setOutcome(null);
  };

  const check = async (event?: FormEvent) => {
    event?.preventDefault();
    if (!filled || checking) return;
    setChecking(true);
    setOutcome(null);
    onChecking();
    try {
      const result = await engine.checkOnline({
        baseUrl: form.baseUrl.trim(),
        key: form.key.trim(),
        model: form.model.trim(),
      });
      setOutcome(result.type === "cancelled" ? null : result);
    } catch {
      setOutcome({ type: "unreachable" });
    } finally {
      setChecking(false);
    }
  };

  const invalid = (field: "baseUrl" | "key" | "model") => failure?.field === field;

  return (
    <Frame
      back={{ to: "/settings/online", label: t("screen.online.title") }}
      aside={
        <>
          <StepAside
            heading={t("online.connect.aside.heading")}
            paragraphs={[t("online.connect.aside.any"), t("online.connect.aside.neutral")]}
          />
          <LockNote>{t("online.connect.aside.lock")}</LockNote>
        </>
      }
    >
      <StepLabel step={2} />
      <h1 className={styles.stepTitle} tabIndex={-1}>
        {t("online.connect.title")}
      </h1>
      <p className={styles.body}>{t("online.connect.lead")}</p>
      <form className={styles.fields} onSubmit={(event) => void check(event)} noValidate>
        <div className={styles.field}>
          <label className={styles.fieldLabel} htmlFor={`${id}-address`}>
            {t("online.connect.address")}
            <small>{t("online.connect.address.word")}</small>
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
            aria-describedby={`${id}-address-hint`}
            onChange={(event) => edit({ baseUrl: event.target.value })}
          />
          <span id={`${id}-address-hint`} className={styles.sub}>
            {t("online.connect.address.hint")}
          </span>
        </div>
        <div className={styles.field}>
          <label className={styles.fieldLabel} htmlFor={`${id}-key`}>
            {t("online.connect.key")}
            <small>{t("online.connect.key.word")}</small>
          </label>
          <div className={styles.fieldRow}>
            <input
              id={`${id}-key`}
              className={styles.input}
              type={shown ? "text" : "password"}
              autoComplete="off"
              autoCapitalize="off"
              spellCheck={false}
              value={form.key}
              aria-invalid={invalid("key")}
              aria-describedby={`${id}-key-hint`}
              onChange={(event) => edit({ key: event.target.value })}
            />
            <Button variant="secondary" touch aria-pressed={shown} onClick={() => setShown(!shown)}>
              {t(shown ? "online.connect.key.hide" : "online.connect.key.show")}
            </Button>
          </div>
          <span id={`${id}-key-hint`} className={styles.sub}>
            {t("online.connect.key.hint")}
          </span>
        </div>
        <div className={styles.field}>
          <label className={styles.fieldLabel} htmlFor={`${id}-model`}>
            {t("online.connect.name")}
            <small>{t("online.connect.name.word")}</small>
          </label>
          <input
            id={`${id}-model`}
            className={styles.input}
            type="text"
            autoComplete="off"
            autoCapitalize="off"
            spellCheck={false}
            placeholder={t("online.connect.name.placeholder")}
            value={form.model}
            aria-invalid={invalid("model")}
            onChange={(event) => edit({ model: event.target.value })}
          />
        </div>
        <button type="submit" hidden />
      </form>

      <div role="status" aria-live="polite">
        {checking ? (
          <div className={styles.result}>
            <span className={styles.dot} aria-hidden="true" />
            <span className={styles.resultText}>
              <span className={styles.resultTitle}>
                {host
                  ? t("online.connect.checkingWith", { host })
                  : t("online.connect.checkingNoHost")}
              </span>
              <span className={styles.sub}>{t("online.connect.checkingDetail")}</span>
            </span>
          </div>
        ) : null}
        {outcome?.type === "works" ? (
          <div className={styles.result} data-kind="ok">
            <span className={styles.resultText}>
              <span className={styles.resultTitle}>
                {t("online.connect.works", { host: outcome.host, used: outcome.model })}
              </span>
              <span className={styles.sub}>
                {t("online.connect.works.detail", { host: outcome.host })}
              </span>
            </span>
          </div>
        ) : null}
        {outcome && failure ? (
          <div className={styles.result} data-kind="bad">
            <span className={styles.resultText}>
              <span className={styles.resultTitle}>
                {t(failure.title, { host: named, used: form.model.trim() })}
              </span>
              <span className={`${styles.body} ${styles.strong}`}>{t(failure.detail)}</span>
            </span>
          </div>
        ) : null}
      </div>

      <div className={styles.footer}>
        {checking ? (
          <>
            <Button variant="secondary" disabled>
              {t("online.connect.checking")}
            </Button>
            <Button variant="quiet" onClick={() => void engine.cancelOnlineCheck()}>
              {t("online.connect.cancel")}
            </Button>
          </>
        ) : (
          <>
            {works ? null : (
              <Button variant="secondary" disabled={!filled} onClick={() => void check()}>
                {t(failure ? "online.connect.checkAgain" : "online.connect.check")}
              </Button>
            )}
            {works ? (
              <>
                <Button variant="quiet" onClick={() => void check()}>
                  {t("online.connect.checkAgain")}
                </Button>
                <Button variant="primary" onClick={onNext}>
                  {t("online.continue")}
                </Button>
              </>
            ) : (
              <Button variant="primary" disabled aria-describedby={`${id}-why`}>
                {t("online.continue")}
              </Button>
            )}
          </>
        )}
        <Button variant="quiet" onClick={onLeave}>
          {t("online.notNow")}
        </Button>
        {works || checking ? null : (
          <span id={`${id}-why`} className={styles.footerHint}>
            {t("online.connect.needsCheck")}
          </span>
        )}
      </div>
    </Frame>
  );
}
