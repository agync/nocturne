import { useId, useState } from "react";
import { ActionBar } from "../../components/ActionBar/ActionBar";
import { Button } from "../../components/Button/Button";
import { tc } from "../../components/ControlChip/copy";
import { t } from "../../copy/t";
import type { OnlineLimit } from "../../engine/bindings";
import { type StepProps, withLimit } from "./draft";
import { DEFAULT_LIMIT, parseQuestions, parseUsd } from "./limits";
import styles from "./Online.module.css";
import { Frame } from "./parts";
import { StepAside, StepLabel } from "./SetupBits";

type Mode = OnlineLimit["kind"];

/** O7: how many questions, or how much money, may go online in a month. */
export function LimitStep({ draft, provider, costReported, setDraft, onBack, onNext }: StepProps) {
  const id = useId();
  const startMode: Mode =
    draft.limit.kind === "usd" && !costReported ? "questions" : draft.limit.kind;
  const [mode, setMode] = useState<Mode>(startMode);
  const [questions, setQuestions] = useState(String(draft.limit.questions || 200));
  const [dollars, setDollars] = useState(draft.limit.usd ? draft.limit.usd.toFixed(2) : "5.00");

  const parsedQuestions = parseQuestions(questions);
  const parsedUsd = parseUsd(dollars);
  const invalid =
    mode === "questions" ? parsedQuestions === null : mode === "usd" && parsedUsd === null;

  const limit = (): OnlineLimit | null => {
    if (mode === "none") return { kind: "none", questions: draft.limit.questions, usd: null };
    if (mode === "questions" && parsedQuestions !== null) {
      return { kind: "questions", questions: parsedQuestions, usd: null };
    }
    if (mode === "usd" && parsedUsd !== null) {
      return { kind: "usd", questions: draft.limit.questions, usd: parsedUsd };
    }
    return null;
  };

  const next = () => {
    const chosen = limit();
    if (!chosen) return;
    setDraft(withLimit(draft, chosen));
    onNext();
  };

  const edited =
    mode === "questions" && parsedQuestions !== null && parsedQuestions !== DEFAULT_LIMIT.questions;

  return (
    <Frame
      back={{ to: "/settings/online", label: t("screen.online.title") }}
      aside={
        <StepAside
          heading={t("online.limit.aside.heading")}
          paragraphs={[t("online.limit.aside.answers"), t("online.limit.aside.first")]}
        />
      }
    >
      <StepLabel step={4} />
      <h1 className={styles.stepTitle} tabIndex={-1}>
        {t("online.limit.title")}
      </h1>
      {mode === "none" ? (
        <div className={styles.limitBox}>
          <p className={styles.body}>{t("online.limit.noneNow")}</p>
          <div>
            <Button
              variant="secondary"
              onClick={() => setMode(startMode === "none" ? "questions" : startMode)}
            >
              {t("online.limit.setOne")}
            </Button>
          </div>
        </div>
      ) : (
        <>
          <p className={styles.body}>
            {t(costReported ? "online.limit.money" : "online.limit.count", { host: provider.host })}
          </p>
          {costReported ? (
            <fieldset className={styles.limitBox}>
              <legend className={styles.legend}>{t("online.limit.legend")}</legend>
              <label className={styles.option}>
                <input
                  type="radio"
                  name={`${id}-kind`}
                  checked={mode === "usd"}
                  onChange={() => setMode("usd")}
                />
                <span>{t("online.limit.amount")}</span>
                <span className={styles.limitLine}>
                  $
                  <input
                    className={styles.number}
                    value={dollars}
                    inputMode="decimal"
                    aria-label={t("online.limit.amount.label")}
                    aria-invalid={mode === "usd" && parsedUsd === null}
                    aria-describedby={
                      mode === "usd" && parsedUsd === null ? `${id}-err` : undefined
                    }
                    onFocus={() => setMode("usd")}
                    onChange={(event) => setDollars(event.target.value)}
                  />
                </span>
                <span>{t("online.limit.amount.after")}</span>
              </label>
              <label className={styles.option}>
                <input
                  type="radio"
                  name={`${id}-kind`}
                  checked={mode === "questions"}
                  onChange={() => setMode("questions")}
                />
                <span>{t("online.limit.number")}</span>
                {mode === "questions" ? (
                  <input
                    className={styles.number}
                    value={questions}
                    inputMode="numeric"
                    aria-label={t("online.limit.number")}
                    aria-invalid={parsedQuestions === null}
                    aria-describedby={parsedQuestions === null ? `${id}-err` : undefined}
                    onChange={(event) => setQuestions(event.target.value)}
                  />
                ) : null}
              </label>
              <p className={styles.sub}>{t("online.limit.usdNote")}</p>
              {invalid ? (
                <p id={`${id}-err`} role="alert" className={styles.error}>
                  {t(mode === "usd" ? "online.limit.invalidUsd" : "online.limit.invalid")}
                </p>
              ) : null}
            </fieldset>
          ) : (
            <div className={styles.limitBox}>
              <label className={styles.limitLine} htmlFor={`${id}-q`}>
                {t("online.limit.before")}
                <input
                  id={`${id}-q`}
                  className={styles.number}
                  value={questions}
                  inputMode="numeric"
                  aria-invalid={parsedQuestions === null}
                  aria-describedby={`${id}-hint`}
                  onChange={(event) => setQuestions(event.target.value)}
                />
                {t("online.limit.after")}
              </label>
              {parsedQuestions === null ? (
                <p id={`${id}-hint`} role="alert" className={styles.error}>
                  {t("online.limit.invalid")}
                </p>
              ) : (
                <p id={`${id}-hint`} className={styles.sub}>
                  {tc(edited ? "online.limit.changed" : "online.limit.hint")}
                </p>
              )}
            </div>
          )}
          <button
            type="button"
            className={`${styles.linkButton} hit`}
            onClick={() => setMode("none")}
            style={{ alignSelf: "flex-start" }}
          >
            {t("online.limit.useNone")}
          </button>
        </>
      )}
      <ActionBar
        lineId={`${id}-why`}
        line={
          invalid
            ? t(mode === "usd" ? "online.limit.needs.amount" : "online.limit.needs.number")
            : t("online.bar.line")
        }
      >
        <Button
          variant="primary"
          disabled={invalid}
          aria-describedby={invalid ? `${id}-why` : undefined}
          onClick={next}
        >
          {t("online.continue")}
        </Button>
        <Button variant="quiet" onClick={onBack}>
          {t("online.back")}
        </Button>
      </ActionBar>
    </Frame>
  );
}
