import { type FormEvent, useId, useRef, useState } from "react";
import { ActionBar } from "../../components/ActionBar/ActionBar";
import { Button } from "../../components/Button/Button";
import { Icon } from "../../components/Icon/Icon";
import { type CopyId, t } from "../../copy/t";
import type { CheckOutcome, OnlineForm } from "../../engine/bindings";
import { useEngine } from "../../engine/context";
import { ModelDialog } from "./ModelDialog";
import { afterLook, against, type ListProblem, type ListState, typingHelps } from "./modelList";
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

/** The words under the name field for a look that failed: the check's title, and the list's help. */
function listProblem(problem: ListProblem): [CopyId, CopyId] {
  const { title, detail } = FAILURE[problem];
  if (typingHelps(problem)) return [title, "advanced.online.connect.list.later.detail"];
  if (problem === "keyRefused") return [title, "advanced.online.connect.list.keyRefused.detail"];
  if (problem === "noInternet") return [title, "advanced.online.connect.list.noInternet.detail"];
  return [title, detail];
}

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
  // The list is the step's alone: leaving forgets it, and a new address or key drops it.
  const [looked, setLooked] = useState<ListState | { name: "error" }>({ name: "idle" });
  const [listOpen, setListOpen] = useState(false);
  const looks = useRef(0);
  const nameField = useRef<HTMLInputElement>(null);
  const pick = useRef<HTMLButtonElement>(null);
  const host = hostOfAddress(form.baseUrl);
  const filled = form.baseUrl.trim() !== "" && form.key.trim() !== "" && form.model.trim() !== "";
  const works = outcome?.type === "works";
  const failure =
    outcome && outcome.type !== "works" && outcome.type !== "cancelled"
      ? FAILURE[outcome.type]
      : null;
  const named = host || sentence(t("advanced.online.connect.thatAddress"));

  const ready = form.baseUrl.trim() !== "" && form.key.trim() !== "";
  const names = looked.name === "listed" ? looked.names : null;
  const stands = against(form.model, names);

  const edit = (patch: Partial<ConnectForm>) => {
    setForm({ ...form, ...patch });
    if (outcome) setOutcome(null);
    if (patch.baseUrl !== undefined || patch.key !== undefined) {
      looks.current += 1;
      setLooked({ name: "idle" });
    }
  };

  const look = async () => {
    if (!ready || checking || looked.name === "looking") return;
    if (names) return setListOpen(true);
    const mine = ++looks.current;
    setLooked({ name: "looking" });
    try {
      const found = afterLook(
        await engine.listOnlineModels({
          baseUrl: form.baseUrl.trim(),
          key: form.key.trim(),
          model: form.model.trim(),
        }),
      );
      if (mine !== looks.current) return;
      setLooked(found);
      if (found.name === "listed") setListOpen(true);
      else if (found.name === "none") nameField.current?.focus();
    } catch {
      if (mine === looks.current) setLooked({ name: "error" });
    }
  };

  const choose = (name: string) => {
    setListOpen(false);
    edit({ model: name });
    nameField.current?.focus();
  };

  const check = async (event?: FormEvent) => {
    event?.preventDefault();
    if (!filled || checking || looked.name === "looking") return;
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

  const listAgain = looked.name === "failed" || looked.name === "none" || looked.name === "error";
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
            ref={nameField}
            id={`${id}-model`}
            className={styles.input}
            type="text"
            autoComplete="off"
            autoCapitalize="off"
            spellCheck={false}
            placeholder={t("advanced.online.connect.name.placeholder")}
            value={form.model}
            aria-invalid={invalid("model")}
            aria-describedby={describe("model", stands.kind === "none" ? undefined : `${id}-under`)}
            readOnly={checking}
            onChange={(event) => edit({ model: event.target.value })}
          />
          {stands.kind === "on" ? (
            <p id={`${id}-under`} className={styles.from}>
              {t("advanced.online.connect.list.on", { host })}
            </p>
          ) : stands.kind === "notOn" ? (
            <p id={`${id}-under`} className={`${styles.from} ${styles.fromInk}`}>
              {t("advanced.online.connect.list.notOn", { name: form.model.trim(), host })}
            </p>
          ) : stands.kind === "capitals" ? (
            <>
              <p id={`${id}-under`} className={`${styles.from} ${styles.fromInk}`}>
                {t("advanced.online.connect.list.capitals", {
                  name: form.model.trim(),
                  host,
                  listed: stands.listed,
                })}
              </p>
              <button
                type="button"
                className={styles.useListed}
                onClick={() => choose(stands.listed)}
              >
                {t("advanced.online.connect.list.useListed", { listed: stands.listed })}
              </button>
            </>
          ) : null}
          <button
            ref={pick}
            type="button"
            className={styles.pick}
            data-busy={looked.name === "looking" ? "" : undefined}
            aria-disabled={ready && !checking && looked.name !== "looking" ? undefined : true}
            aria-describedby={ready ? undefined : `${id}-needs`}
            onClick={() => void look()}
          >
            {looked.name === "looking" ? (
              <span className={styles.dot} aria-hidden="true" />
            ) : (
              <Icon name={listAgain ? "rotate-ccw" : "list"} size={16} />
            )}
            <span>
              {looked.name === "looking"
                ? t("advanced.online.connect.list.looking", { host })
                : listAgain
                  ? t("advanced.online.connect.list.again")
                  : ready && host
                    ? t("advanced.online.connect.list", { host })
                    : t("advanced.online.connect.list.noHost")}
            </span>
          </button>
          <span className="sr-only" role="status">
            {looked.name === "looking" ? t("advanced.online.connect.list.looking", { host }) : ""}
          </span>
          {ready ? null : (
            <p id={`${id}-needs`} className={styles.from}>
              {t("advanced.online.connect.list.needs")}
            </p>
          )}
          {looked.name === "failed" ? (
            <div className={styles.listProblem} role="alert">
              <p>{t(listProblem(looked.problem)[0], { host: named, used: form.model.trim() })}</p>
              <p>{t(listProblem(looked.problem)[1])}</p>
            </div>
          ) : looked.name === "none" ? (
            <div className={styles.listProblem} data-calm="" role="status">
              <p>{t("advanced.online.connect.list.none", { host: named })}</p>
              <p>{t("advanced.online.connect.list.none.detail")}</p>
            </div>
          ) : looked.name === "error" ? (
            <p className={styles.listProblem} role="alert">
              {t("advanced.online.connect.list.failed")}
            </p>
          ) : null}
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
              disabled={!filled || looked.name === "looking"}
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
      {listOpen && names ? (
        <ModelDialog
          host={host}
          names={names}
          chosen={form.model.trim()}
          onChoose={choose}
          onClose={() => {
            setListOpen(false);
            nameField.current?.focus();
          }}
        />
      ) : null}
    </Frame>
  );
}
