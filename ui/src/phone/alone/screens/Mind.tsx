import {
  Children,
  type InputHTMLAttributes,
  type ReactNode,
  useEffect,
  useId,
  useRef,
  useState,
} from "react";
import { type CopyId, type CopyValues, t, tParts } from "../../../copy/t";
import type { CheckOutcome, ModelList } from "../../../engine/bindings";
import { parseQuestions, parseUsd, usdText } from "../../../screens/online/limits";
import {
  afterLook,
  against,
  type ListProblem,
  type ListState,
  typingHelps,
} from "../../../screens/online/modelList";
import { useHeading } from "../../native/screens/parts";
import { BackBar, Button, Icon, Page } from "../../screens/ui";
import {
  hostOf,
  type MindLimit,
  type MindReading,
  type ProviderForm,
  type ReadKind,
} from "../mind";
import { type ReadAction, readBlock } from "../readNow";
import { ModelSheet } from "./ModelSheet";
import { Chip } from "./parts";

/** What the person has entered so far, held above the steps so Back keeps it. */
export interface MindDraft {
  form: ProviderForm;
  /** The host and model that answered the check for `form` as it stands. */
  works: { host: string; model: string } | null;
  limit: MindLimit;
}

export const NEW_DRAFT: MindDraft = {
  form: { url: "", key: "", model: "" },
  works: null,
  limit: { kind: "none" },
};

function BackOnly({ onBack }: { onBack(): void }) {
  return (
    <header className="ph-bar ph-bar--back">
      <button
        type="button"
        className="ph-ib ph-ib--ink"
        onClick={onBack}
        aria-label={t("phone.mind.back")}
      >
        <Icon name="back" />
      </button>
    </header>
  );
}

function StepLabel({ step }: { step: number }) {
  return <p className="ph-mind-step">{t("phone.mind.step", { step })}</p>;
}

/** A sentence with `{thisPhone}` and `{seeWhatWasSent}` drawn as the controls they name. */
function named(id: CopyId, values?: CopyValues): ReactNode {
  return Children.toArray(
    tParts(id, {
      ...values,
      thisPhone: <Chip>{t("phone.sheet.title")}</Chip>,
      seeWhatWasSent: <Chip>{t("phone.mind.row.sent")}</Chip>,
    }) as ReactNode[],
  );
}

/** "At most 200 things a month…", or no limit. */
export function limitLine(limit: MindLimit): string {
  if (limit.kind === "things") return t("phone.mind.agree.limit.things", { count: limit.count });
  if (limit.kind === "amount")
    return t("phone.mind.agree.limit.amount", { amount: usdText(limit.usd) });
  return t("phone.mind.agree.limit.none");
}

/**
 * Step 4's sections. Before a service is chosen (behind step 1's expander) it is "the service",
 * and the changing-your-mind lines wait for the limit.
 */
function WhatHappens({
  host,
  used,
  limit,
}: {
  host: string | null;
  used: string | null;
  limit: MindLimit | null;
}) {
  const name = host ?? t("phone.mind.theService");
  return (
    <>
      <h2 className="ph-mind-sech">{t("phone.mind.agree.sent.heading")}</h2>
      <p className="ph-mind-small">{t("phone.mind.agree.sent")}</p>
      <p className="ph-mind-example">{t("phone.mind.agree.example")}</p>
      <h2 className="ph-mind-sech">{t("phone.mind.agree.goes.heading")}</h2>
      <p className="ph-mind-small">{t("phone.mind.agree.goes", { host: name })}</p>
      <h2 className="ph-mind-sech">{t("phone.mind.agree.sees.heading")}</h2>
      <p className="ph-mind-small">
        {used
          ? t("phone.mind.agree.sees", { host: name, used })
          : t("phone.mind.agree.seesAny", { host: name })}
      </p>
      <p className="ph-mind-small">{t("phone.mind.agree.keeps")}</p>
      <h2 className="ph-mind-sech">{t("phone.mind.agree.private.heading")}</h2>
      <p className="ph-mind-small">
        {t("phone.mind.private")} {t("phone.mind.agree.letGo")}
      </p>
      <h2 className="ph-mind-sech">{t("phone.mind.agree.changes.heading")}</h2>
      <p className="ph-mind-small">{t("phone.mind.agree.changes", { host: name })}</p>
      {limit ? (
        <>
          <h2 className="ph-mind-sech">{t("phone.mind.agree.mind.heading")}</h2>
          <p className="ph-mind-small">{limitLine(limit)}</p>
          <p className="ph-mind-small">{named("phone.mind.agree.off")}</p>
          <p className="ph-mind-small">{named("phone.mind.agree.where")}</p>
        </>
      ) : null}
    </>
  );
}

export type MindWay = "online" | "computer";

/** "Where should Nocturne's mind be?": in the first release, an online service or a computer. */
export function ChooseMind({ onBack, onChoose }: { onBack(): void; onChoose(way: MindWay): void }) {
  const [way, setWay] = useState<MindWay>("online");
  const heading = useHeading("choose");
  const name = useId();
  const choices: [MindWay, CopyId, CopyId][] = [
    ["online", "phone.mind.choose.online", "phone.mind.choose.online.hint"],
    ["computer", "phone.mind.choose.computer", "phone.mind.choose.computer.hint"],
  ];
  return (
    <Page bar={<BackBar title={t("phone.mind.bar")} onBack={onBack} />}>
      <main className="ph-main" style={{ gap: 14 }}>
        <h2 className="ph-steph" tabIndex={-1} ref={heading}>
          {t("phone.mind.choose.title")}
        </h2>
        <p className="ph-body">{t("phone.mind.choose.lead")}</p>
        <fieldset className="ph-options ph-mind-ways">
          <legend className="sr-only">{t("phone.mind.choose.legend")}</legend>
          {choices.map(([id, title, hint]) => (
            <label key={id} className="ph-option ph-mind-way">
              <input type="radio" name={name} checked={way === id} onChange={() => setWay(id)} />
              <span className="ph-radio" aria-hidden="true" />
              <span>
                <strong>{t(title)}</strong>
                <span className="ph-mind-way-hint">{t(hint)}</span>
              </span>
            </label>
          ))}
        </fieldset>
      </main>
      <footer className="ph-foot">
        <Button kind="primary" onClick={() => onChoose(way)}>
          {t("phone.mind.continue")}
        </Button>
        <button type="button" className="ph-textbtn" onClick={onBack}>
          {t("phone.mind.notNow")}
        </button>
      </footer>
    </Page>
  );
}

/** Step 1: what goes. */
export function MindWhat({
  onBack,
  onNext,
  onNotNow,
}: {
  onBack(): void;
  onNext(): void;
  onNotNow(): void;
}) {
  const [open, setOpen] = useState(false);
  const heading = useHeading("what");
  const more = useId();
  return (
    <Page bar={<BackOnly onBack={onBack} />}>
      <main className="ph-main" style={{ gap: 14 }}>
        <StepLabel step={1} />
        <h1 className="ph-steph" tabIndex={-1} ref={heading}>
          {t("phone.mind.what.title")}
        </h1>
        <dl className="ph-mind-lines">
          <div>
            <dt>{t("phone.mind.what.sent.label")}</dt>
            <dd>{t("phone.mind.what.sent")}</dd>
          </div>
          <div>
            <dt>{t("phone.mind.what.private.label")}</dt>
            <dd>{t("phone.mind.private")}</dd>
          </div>
          <div>
            <dt>{t("phone.mind.what.need.label")}</dt>
            <dd>{t("phone.mind.what.need")}</dd>
          </div>
        </dl>
        <div className="ph-mind-more">
          <button
            type="button"
            className="ph-mind-expander"
            aria-expanded={open}
            aria-controls={more}
            onClick={() => setOpen((o) => !o)}
          >
            <span>{t("phone.mind.what.more")}</span>
            <span className={open ? "ph-mind-chevron ph-mind-chevron--open" : "ph-mind-chevron"}>
              <Icon name="forward" size={18} />
            </span>
          </button>
          {open ? (
            <div id={more} className="ph-stack ph-mind-what" style={{ gap: 8 }}>
              <WhatHappens host={null} used={null} limit={null} />
            </div>
          ) : null}
        </div>
      </main>
      <footer className="ph-foot">
        <Button kind="primary" onClick={onNext}>
          {t("phone.mind.continue")}
        </Button>
        <button type="button" className="ph-textbtn" onClick={onNotNow}>
          {t("phone.mind.notNow")}
        </button>
        <p className="ph-mind-barline">{t("phone.mind.barLine")}</p>
      </footer>
    </Page>
  );
}

const CHECK_WORDS: Record<
  Exclude<CheckOutcome["type"], "works" | "cancelled">,
  [CopyId, CopyId]
> = {
  unreachable: ["phone.mind.check.unreachable", "phone.mind.check.unreachable.detail"],
  keyRefused: ["phone.mind.check.keyRefused", "phone.mind.check.keyRefused.detail"],
  noCredit: ["phone.mind.check.noCredit", "phone.mind.check.noCredit.detail"],
  modelMissing: ["phone.mind.check.modelMissing", "phone.mind.check.modelMissing.detail"],
  noInternet: ["phone.mind.check.noInternet", "phone.mind.check.noInternet.detail"],
  notHttps: ["phone.mind.check.notHttps", "phone.mind.check.notHttps.detail"],
  addressNotAllowed: [
    "phone.mind.check.addressNotAllowed",
    "phone.mind.check.addressNotAllowed.detail",
  ],
  busy: ["phone.mind.check.busy", "phone.mind.check.busy.detail"],
  serverError: ["phone.mind.check.serverError", "phone.mind.check.serverError.detail"],
  emptyReply: ["phone.mind.check.emptyReply", "phone.mind.check.emptyReply.detail"],
};

type Checking =
  | { name: "idle" }
  | { name: "checking"; host: string }
  | { name: "done"; outcome: CheckOutcome; host: string; model: string }
  | { name: "failed" };

/** The words under the name field for a look that failed: the check's title, and the list's help. */
function listProblem(problem: ListProblem): [CopyId, CopyId] {
  const [title, detail] = CHECK_WORDS[problem];
  if (typingHelps(problem)) return [title, "phone.mind.details.list.later.detail"];
  if (problem === "keyRefused") return [title, "phone.mind.details.list.keyRefused.detail"];
  if (problem === "noInternet") return [title, "phone.mind.details.list.noInternet.detail"];
  return [title, detail];
}

/** Step 2: the service's three details, the service's list to choose the name from, and the check. */
export function MindDetails({
  draft,
  check,
  list,
  onDraft,
  onBack,
  onNext,
}: {
  draft: MindDraft;
  check(form: ProviderForm): Promise<CheckOutcome>;
  /** One look at the service's list; none on a screen that offers no list. */
  list?(form: ProviderForm): Promise<ModelList>;
  onDraft(next: MindDraft): void;
  onBack(): void;
  onNext(): void;
}) {
  const [form, setForm] = useState(draft.form);
  const [checking, setChecking] = useState<Checking>(
    draft.works
      ? {
          name: "done",
          outcome: { type: "works", ...draft.works },
          host: draft.works.host,
          model: draft.works.model,
        }
      : { name: "idle" },
  );
  // The list is the step's alone: leaving it forgets the list, and a new address or key drops it.
  const [looked, setLooked] = useState<ListState | { name: "error" }>({ name: "idle" });
  const [open, setOpen] = useState(false);
  const heading = useHeading("details");
  const id = useId();
  const nameField = useRef<HTMLInputElement>(null);
  const pick = useRef<HTMLButtonElement>(null);
  // Each edit or check starts a new run; a check that answers for an older one is dropped.
  const runs = useRef(0);
  const looks = useRef(0);
  const filled = form.url.trim() !== "" && form.key.trim() !== "" && form.model.trim() !== "";
  const ready = form.url.trim() !== "" && form.key.trim() !== "";
  const works = checking.name === "done" && checking.outcome.type === "works";
  const host = hostOf(form.url);
  const names = looked.name === "listed" ? looked.names : null;
  const stands = against(form.model, names);

  const edit = (part: Partial<ProviderForm>) => {
    runs.current += 1;
    const next = { ...form, ...part };
    setForm(next);
    setChecking({ name: "idle" });
    if (part.url !== undefined || part.key !== undefined) {
      looks.current += 1;
      setLooked({ name: "idle" });
    }
    onDraft({ ...draft, form: next, works: null });
  };

  const look = async () => {
    if (!list || !ready || looked.name === "looking") return;
    if (names) return setOpen(true);
    const mine = ++looks.current;
    setLooked({ name: "looking" });
    try {
      const found = afterLook(await list(form));
      if (mine !== looks.current) return;
      setLooked(found);
      if (found.name === "listed") setOpen(true);
      else if (found.name === "none") nameField.current?.focus();
    } catch {
      if (mine === looks.current) setLooked({ name: "error" });
    }
  };

  const choose = (name: string) => {
    setOpen(false);
    edit({ model: name });
    nameField.current?.focus();
  };

  const run = async () => {
    if (!filled || checking.name === "checking" || looked.name === "looking") return;
    const mine = ++runs.current;
    setChecking({ name: "checking", host });
    try {
      const outcome = await check(form);
      if (mine !== runs.current) return;
      if (outcome.type === "cancelled") return setChecking({ name: "idle" });
      setChecking({ name: "done", outcome, host, model: form.model.trim() });
      onDraft({
        ...draft,
        form,
        works: outcome.type === "works" ? { host: outcome.host, model: outcome.model } : null,
      });
    } catch {
      if (mine === runs.current) setChecking({ name: "failed" });
    }
  };

  let result: ReactNode = null;
  if (checking.name === "checking") {
    result = (
      <p className="ph-status" role="status">
        <span className="ph-dot ph-dot--breathe" aria-hidden="true" />
        {t("phone.mind.checking", { host: checking.host })}
      </p>
    );
  } else if (checking.name === "failed") {
    result = (
      <p className="ph-error" role="alert">
        {t("phone.mind.check.failed")}
      </p>
    );
  } else if (checking.name === "done") {
    const o = checking.outcome;
    if (o.type === "works") {
      result = (
        <p className="ph-mind-works" role="status">
          <Icon name="check" size={18} />
          {t("phone.mind.works", { host: o.host })}
        </p>
      );
    } else if (o.type !== "cancelled") {
      const [title, detail] = CHECK_WORDS[o.type];
      result = (
        <div className="ph-mind-problem" role="alert">
          <p className="ph-error-title">
            {t(title, { host: checking.host, used: checking.model })}
          </p>
          <p className="ph-hint ph-hint--ink2">{t(detail)}</p>
        </div>
      );
    }
  }

  const field = (
    part: "url" | "key",
    label: CopyId,
    extra: InputHTMLAttributes<HTMLInputElement>,
  ) => (
    <div className="ph-field">
      <label htmlFor={`${id}-${part}`}>{t(label)}</label>
      <input
        id={`${id}-${part}`}
        className="ph-input ph-input--note"
        autoComplete="off"
        autoCapitalize="none"
        autoCorrect="off"
        spellCheck={false}
        value={form[part]}
        onChange={(e) => edit({ [part]: e.target.value })}
        {...extra}
      />
    </div>
  );

  // The line under the name field, joined to it.
  let under: ReactNode = null;
  if (stands.kind === "on") {
    under = <p className="ph-mind-from">{t("phone.mind.details.list.on", { host })}</p>;
  } else if (stands.kind === "notOn") {
    under = (
      <p className="ph-mind-from ph-mind-from--ink">
        {t("phone.mind.details.list.notOn", { name: form.model.trim(), host })}
      </p>
    );
  } else if (stands.kind === "capitals") {
    under = (
      <div className="ph-stack" style={{ gap: 6 }}>
        <p className="ph-mind-from ph-mind-from--ink">
          {t("phone.mind.details.list.capitals", {
            name: form.model.trim(),
            host,
            listed: stands.listed,
          })}
        </p>
        <button type="button" className="ph-link" onClick={() => choose(stands.listed)}>
          {t("phone.mind.details.list.useListed", { listed: stands.listed })}
        </button>
      </div>
    );
  }

  let problem: ReactNode = null;
  if (looked.name === "failed") {
    const [title, detail] = listProblem(looked.problem);
    problem = (
      <div className="ph-mind-prob" role="alert">
        <p>{t(title, { host, used: form.model.trim() })}</p>
        <p>{t(detail)}</p>
      </div>
    );
  } else if (looked.name === "none") {
    problem = (
      <div className="ph-mind-prob ph-mind-prob--calm" role="status">
        <p>{t("phone.mind.details.list.none", { host })}</p>
        <p>{t("phone.mind.details.list.none.detail")}</p>
      </div>
    );
  } else if (looked.name === "error") {
    problem = (
      <p className="ph-error" role="alert">
        {t("phone.mind.details.list.failed")}
      </p>
    );
  }

  const again = looked.name === "failed" || looked.name === "none" || looked.name === "error";
  let control: ReactNode = null;
  if (list) {
    control =
      looked.name === "looking" ? (
        <p className="ph-mind-pick" data-busy="" role="status">
          <span className="ph-dot ph-dot--breathe" aria-hidden="true" />
          <span>{t("phone.mind.details.list.looking", { host })}</span>
        </p>
      ) : (
        <>
          <button
            ref={pick}
            type="button"
            className="ph-mind-pick"
            aria-disabled={ready ? undefined : true}
            aria-describedby={ready ? undefined : `${id}-needs`}
            onClick={() => void look()}
          >
            <Icon name={again ? "refresh" : "list"} size={20} />
            <span>
              {again
                ? t("phone.mind.details.list.again")
                : ready && host
                  ? t("phone.mind.details.list", { host })
                  : t("phone.mind.details.list.noHost")}
            </span>
          </button>
          {ready ? null : (
            <p id={`${id}-needs`} className="ph-mind-from">
              {t("phone.mind.details.list.needs")}
            </p>
          )}
        </>
      );
  }

  return (
    <Page bar={<BackOnly onBack={onBack} />}>
      <main className="ph-main" style={{ gap: 14 }}>
        <StepLabel step={2} />
        <h1 className="ph-steph" tabIndex={-1} ref={heading}>
          {t("phone.mind.details.title")}
        </h1>
        <p className="ph-body">{t("phone.mind.details.lead")}</p>
        {field("url", "phone.mind.details.url", { type: "url", inputMode: "url" })}
        {field("key", "phone.mind.details.key", {
          type: "password",
          "aria-describedby": `${id}-kept`,
        })}
        <p id={`${id}-kept`} className="ph-hint ph-hint--ink2" style={{ marginTop: -6 }}>
          {t("phone.mind.details.kept")}
        </p>
        <div className="ph-field">
          <label htmlFor={`${id}-model`}>{t("phone.mind.details.model")}</label>
          <input
            ref={nameField}
            id={`${id}-model`}
            className="ph-input ph-input--note ph-mind-name"
            type="text"
            autoComplete="off"
            autoCapitalize="none"
            autoCorrect="off"
            spellCheck={false}
            placeholder={t("phone.mind.details.model.placeholder")}
            value={form.model}
            aria-describedby={under ? `${id}-under` : undefined}
            onChange={(e) => edit({ model: e.target.value })}
          />
          {under ? <div id={`${id}-under`}>{under}</div> : null}
        </div>
        {control}
        {problem}
        <div className="ph-stack" style={{ gap: 10 }}>
          <Button
            disabled={!filled || checking.name === "checking" || looked.name === "looking"}
            onClick={() => void run()}
          >
            {checking.name === "done" && !works
              ? t("phone.mind.checkAgain")
              : t("phone.mind.check")}
          </Button>
          <div aria-live="polite">{result}</div>
        </div>
      </main>
      <footer className="ph-foot">
        <Button kind="primary" disabled={!works} onClick={works ? onNext : undefined}>
          {t("phone.mind.continue")}
        </Button>
        <button type="button" className="ph-textbtn" onClick={onBack}>
          {t("phone.mind.back")}
        </button>
        {works ? null : (
          <p id={`${id}-why`} className="ph-mind-barline">
            {t("phone.mind.needsCheck")}
          </p>
        )}
      </footer>
      {open && names ? (
        <ModelSheet
          host={host}
          names={names}
          chosen={form.model.trim()}
          onChoose={choose}
          onClose={() => {
            setOpen(false);
            pick.current?.focus();
          }}
        />
      ) : null}
    </Page>
  );
}

/**
 * Step 3: a monthly limit, none unless the person sets one.
 */
export function MindLimitStep({
  draft,
  onDraft,
  onBack,
  onNext,
}: {
  draft: MindDraft;
  onDraft(next: MindDraft): void;
  onBack(): void;
  onNext(): void;
}) {
  const [mode, setMode] = useState<MindLimit["kind"]>(draft.limit.kind);
  const [count, setCount] = useState(
    draft.limit.kind === "things" ? String(draft.limit.count) : "200",
  );
  const [usd, setUsd] = useState(
    draft.limit.kind === "amount" ? draft.limit.usd.toFixed(2) : "5.00",
  );
  const heading = useHeading("limit");
  const id = useId();
  const things = parseQuestions(count);
  const amount = parseUsd(usd);
  const invalid = (mode === "things" && things === null) || (mode === "amount" && amount === null);

  const next = () => {
    let limit: MindLimit = { kind: "none" };
    if (mode === "things" && things !== null) limit = { kind: "things", count: things };
    else if (mode === "amount" && amount !== null) limit = { kind: "amount", usd: amount };
    else if (mode !== "none") return;
    onDraft({ ...draft, limit });
    onNext();
  };

  const option = (kind: MindLimit["kind"], label: CopyId, input?: ReactNode) => (
    <div className="ph-mind-limit">
      <label className="ph-option">
        <input
          type="radio"
          name={`${id}-limit`}
          checked={mode === kind}
          onChange={() => setMode(kind)}
        />
        <span className="ph-radio" aria-hidden="true" />
        <span>{t(label)}</span>
      </label>
      {mode === kind ? input : null}
    </div>
  );

  return (
    <Page bar={<BackOnly onBack={onBack} />}>
      <main className="ph-main" style={{ gap: 14 }}>
        <StepLabel step={3} />
        <h1 className="ph-steph" tabIndex={-1} ref={heading}>
          {t("phone.mind.limit.title")}
        </h1>
        <fieldset className="ph-options">
          <legend className="sr-only">{t("phone.mind.limit.title")}</legend>
          {option("none", "phone.mind.limit.none")}
          {option(
            "things",
            "phone.mind.limit.things",
            <div className="ph-field ph-mind-limit-field">
              <label htmlFor={`${id}-things`}>{t("phone.mind.limit.thingsLabel")}</label>
              <input
                id={`${id}-things`}
                className="ph-input ph-input--note"
                inputMode="numeric"
                value={count}
                aria-invalid={things === null}
                aria-describedby={things === null ? `${id}-err` : undefined}
                onChange={(e) => setCount(e.target.value)}
              />
            </div>,
          )}
          {option(
            "amount",
            "phone.mind.limit.amount",
            <div className="ph-field ph-mind-limit-field">
              <label htmlFor={`${id}-usd`}>{t("phone.mind.limit.amountLabel")}</label>
              <input
                id={`${id}-usd`}
                className="ph-input ph-input--note"
                inputMode="decimal"
                value={usd}
                aria-invalid={amount === null}
                aria-describedby={amount === null ? `${id}-err` : undefined}
                onChange={(e) => setUsd(e.target.value)}
              />
            </div>,
          )}
        </fieldset>
        {invalid ? (
          <p id={`${id}-err`} className="ph-error" role="alert">
            {t(mode === "amount" ? "phone.mind.limit.invalidUsd" : "phone.mind.limit.invalid")}
          </p>
        ) : null}
      </main>
      <footer className="ph-foot">
        <Button kind="primary" disabled={invalid} onClick={next}>
          {t("phone.mind.continue")}
        </Button>
        <button type="button" className="ph-textbtn" onClick={onBack}>
          {t("phone.mind.back")}
        </button>
        <p className="ph-mind-barline">
          {invalid
            ? t(
                mode === "amount"
                  ? "phone.mind.limit.needs.amount"
                  : "phone.mind.limit.needs.number",
              )
            : t("phone.mind.barLine")}
        </p>
      </footer>
    </Page>
  );
}

/** Step 4: every fact on screen, then "Turn on". */
export function MindAgree({
  draft,
  onBack,
  onNotNow,
  onTurnOn,
}: {
  draft: MindDraft;
  onBack(): void;
  onNotNow(): void;
  onTurnOn(): Promise<void>;
}) {
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);
  const heading = useHeading("agree");
  const host = draft.works?.host ?? hostOf(draft.form.url);
  const used = draft.works?.model ?? draft.form.model.trim();

  const go = async () => {
    if (busy) return;
    setBusy(true);
    setFailed(false);
    try {
      await onTurnOn();
    } catch {
      setFailed(true);
      setBusy(false);
    }
  };

  return (
    <Page bar={<BackOnly onBack={onBack} />}>
      <main className="ph-main" style={{ gap: 10 }}>
        <StepLabel step={4} />
        <h1 className="ph-steph ph-mind-agree-title" tabIndex={-1} ref={heading}>
          {t("phone.mind.agree.title")}
        </h1>
        <p className="ph-body">{t("phone.mind.agree.lead", { host })}</p>
        <WhatHappens host={host} used={used} limit={draft.limit} />
        {failed ? (
          <p className="ph-error" role="alert">
            {t("phone.mind.agree.failed")}
          </p>
        ) : null}
      </main>
      <footer className="ph-foot">
        <Button kind="primary" disabled={busy} onClick={() => void go()}>
          {busy ? t("phone.mind.turningOn") : t("phone.mind.turnOn")}
        </Button>
        <button type="button" className="ph-textbtn" onClick={onNotNow}>
          {t("phone.mind.notNow")}
        </button>
      </footer>
    </Page>
  );
}

/**
 * After turning on, once: what was kept being read, and when the first page comes. On the charger
 * it reads by itself; on battery it says it waits to charge and offers Read now, the person's own
 * choice, with Stop while it reads.
 */
export function MindReadingScreen({
  first,
  reading,
  readNow,
  readStop,
  ios = false,
  onDone,
}: {
  first: MindReading;
  reading(): Promise<MindReading>;
  readNow?(kind: ReadKind): Promise<MindReading>;
  readStop?(): Promise<MindReading | null>;
  ios?: boolean;
  onDone(): void;
}) {
  const [now, setNow] = useState(first);
  const [busy, setBusy] = useState(false);
  const heading = useHeading("reading");
  useEffect(() => {
    let gone = false;
    const timer = setInterval(() => {
      reading().then(
        (r) => !gone && setNow(r),
        () => undefined,
      );
    }, 5000);
    return () => {
      gone = true;
      clearInterval(timer);
    };
  }, [reading]);

  const act = async (action: ReadAction) => {
    if (busy || !readNow || !readStop) return;
    setBusy(true);
    try {
      if (action === "stop") {
        const after = await readStop();
        if (after) setNow(after);
      } else {
        setNow(await readNow(action === "anyway" ? "anyway" : "now"));
      }
    } catch {
      reading().then(setNow, () => undefined);
    } finally {
      setBusy(false);
    }
  };

  const share = now.count > 0 ? Math.min(1, now.done / now.count) : 0;
  // A phone that never says whether it charges is taken as on battery, as the core takes it.
  const onBattery = now.charging === false && now.count > now.done;
  const block = onBattery ? readBlock({ state: "fine", host: "" }, now, ios) : null;
  const waiting = onBattery && !now.now;

  const progress = (
    <div
      className="ph-progress"
      role="progressbar"
      aria-label={t("phone.mind.reading.title")}
      aria-valuemin={0}
      aria-valuemax={now.count}
      aria-valuenow={now.done}
    >
      <span style={{ width: `${Math.round(share * 100)}%` }} />
    </div>
  );

  let status: ReactNode;
  let note: ReactNode;
  if (waiting) {
    status = (
      <>
        {block?.said ? <p className="ph-body">{block.said}</p> : null}
        <p className="ph-status" role="status">
          <Icon name="moon" size={16} />
          {t("phone.mind.reading.waiting", { count: now.count - now.done })}
        </p>
      </>
    );
    note = (
      <p className="ph-small">
        {block?.said
          ? block.lines.join(" ")
          : t(
              ios
                ? "phone.mind.reading.noteWaiting.iphone"
                : "phone.mind.reading.noteWaiting.android",
            )}
      </p>
    );
  } else if (now.now) {
    status = (
      <>
        {progress}
        <p className="ph-status" role="status">
          <span className="ph-dot ph-dot--breathe" aria-hidden="true" />
          {t("phone.mind.row.readingNow", { done: now.done, count: now.count })}
        </p>
      </>
    );
    note = <p className="ph-small">{block?.lines.join(" ")}</p>;
  } else {
    status = (
      <>
        {progress}
        <p className="ph-status">
          <span className="ph-dot ph-dot--breathe" aria-hidden="true" />
          {t("phone.mind.reading.progress", { done: now.done, count: now.count })}
        </p>
      </>
    );
    note = <p className="ph-small">{t("phone.mind.reading.note")}</p>;
  }

  const button = block?.button;
  return (
    <Page bar={null}>
      <main className="ph-main" style={{ gap: 14 }}>
        <div className="ph-hero" style={{ paddingTop: 72 }}>
          <span className="ph-mark ph-mind-mark" aria-hidden="true" />
          <h1 className="ph-steph" tabIndex={-1} ref={heading}>
            {t("phone.mind.reading.title")}
          </h1>
          <p className="ph-body">
            {waiting
              ? t("phone.mind.reading.bodyWaiting", { count: first.count })
              : t("phone.mind.reading.body", { count: first.count })}
          </p>
        </div>
        <div className="ph-stack" style={{ gap: 8 }}>
          {status}
        </div>
        {note}
      </main>
      <footer className="ph-foot ph-foot--plain">
        <Button kind="primary" onClick={onDone}>
          {t("phone.mind.reading.ok")}
        </Button>
        {button && readNow ? (
          <Button
            kind={button.action === "stop" ? "quiet" : undefined}
            disabled={busy || Boolean(button.needs)}
            onClick={() => void act(button.action)}
          >
            {button.label}
          </Button>
        ) : null}
      </footer>
    </Page>
  );
}
