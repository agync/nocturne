import { useEffect, useRef, useState } from "react";
import { formatTimeOfDay } from "../../copy/format";
import { type CopyId, t } from "../../copy/t";
import type { SameNameAnswer, SameNameAnswered, SameNameQuestion } from "../../engine/bindings";
import { weekdayDay } from "../../screens/lookback/text";
import type { PhoneEngine } from "../engine";
import { BackBar, Button, Page } from "./ui";

type Load =
  | { name: "loading" }
  | { name: "ready"; question: SameNameQuestion }
  | { name: "gone" }
  | { name: "failed" };

interface WhichProps {
  engine: PhoneEngine;
  /** The question as the day card held it; read again from the day when the page was reloaded. */
  question: SameNameQuestion | null;
  questionId: string;
  day: string;
  today: boolean;
  onBack(): void;
  /** After an answer, back to the journal. */
  onDone(): void;
}

function whenText(question: SameNameQuestion, today: boolean): string {
  const time = formatTimeOfDay(question.receipt.keptAt);
  return today
    ? t("phone.journal.which.when.today", { time })
    : t("phone.journal.which.when.day", { day: weekdayDay(question.receipt.keptAt), time });
}

/** "Which {name} is this?": the one change the phone makes beyond keeping. */
export function Which({ engine, question, questionId, day, today, onBack, onDone }: WhichProps) {
  const [load, setLoad] = useState<Load>(
    question ? { name: "ready", question } : { name: "loading" },
  );
  const [choice, setChoice] = useState<string | null>(null);
  const [attempt, setAttempt] = useState(0);
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState(false);
  const [done, setDone] = useState<SameNameAnswered | null>(null);
  const heading = useRef<HTMLHeadingElement>(null);

  useEffect(() => {
    if (question) return;
    let gone = false;
    setLoad({ name: "loading" });
    engine.getJournalDay(day).then(
      (data) => {
        if (gone) return;
        const found = data.sameName.find((q) => q.id === questionId);
        setLoad(found ? { name: "ready", question: found } : { name: "gone" });
      },
      () => !gone && setLoad({ name: "failed" }),
    );
    return () => {
      gone = true;
    };
  }, [engine, question, questionId, day, attempt]);

  const stage = done ? "done" : load.name;
  useEffect(() => {
    void stage;
    heading.current?.focus({ preventScroll: true });
  }, [stage]);

  const answer = async (reply: SameNameAnswer) => {
    if (busy) return;
    setBusy(true);
    setProblem(false);
    try {
      const filed = await engine.answerSameName(questionId, reply);
      if (reply.type === "leave") return onBack();
      setDone(filed);
    } catch {
      setProblem(true);
    } finally {
      setBusy(false);
    }
  };

  const bar = <BackBar title={t("phone.journal.needsYou")} onBack={onBack} />;

  if (done) {
    const role = done.role ?? t("journal.people.someoneNew").toLowerCase();
    return (
      <Page bar={bar}>
        <main className="ph-main" style={{ paddingTop: 40, gap: 12 }}>
          <h1 className="ph-h1 ph-h1--small" tabIndex={-1} ref={heading}>
            {t("journal.people.filed", { name: done.name, role })}
          </h1>
          <p className="ph-body">{t("phone.journal.which.doneBody")}</p>
        </main>
        <footer className="ph-foot">
          <Button kind="primary" onClick={onDone}>
            {t("phone.journal.which.back")}
          </Button>
        </footer>
      </Page>
    );
  }

  if (load.name === "failed") {
    return (
      <Page bar={bar}>
        <main className="ph-main">
          <h1 className="ph-h1 ph-h1--small" role="alert">
            {t("journal.timeline.error")}
          </h1>
          <div className="ph-spacer">
            <Button kind="primary" onClick={() => setAttempt((n) => n + 1)}>
              {t("phone.tryAgain")}
            </Button>
          </div>
        </main>
      </Page>
    );
  }

  if (load.name === "loading") {
    return (
      <Page bar={bar}>
        <main className="ph-main">
          <p className="ph-status" role="status" style={{ fontSize: "1rem" }}>
            {t("phone.journal.loadingQuestion")}
          </p>
        </main>
      </Page>
    );
  }

  if (load.name !== "ready") {
    return (
      <Page bar={bar}>
        <main className="ph-main" style={{ paddingTop: 40, gap: 12 }}>
          <h1 className="ph-h1 ph-h1--small" tabIndex={-1} ref={heading}>
            {t("phone.journal.which.gone.title")}
          </h1>
          <p className="ph-body">{t("phone.journal.which.gone.body")}</p>
        </main>
        <footer className="ph-foot">
          <Button kind="primary" onClick={onDone}>
            {t("phone.journal.which.back")}
          </Button>
        </footer>
      </Page>
    );
  }

  const { question: q } = load;
  const reply: SameNameAnswer | null =
    choice === null
      ? null
      : choice === "new"
        ? { type: "new" }
        : { type: "entity", entityId: choice };
  const kind = t(`kind.${q.receipt.kind}.spoken` as CopyId);
  return (
    <Page bar={bar}>
      <main className="ph-main ph-main--tight" style={{ gap: 14 }}>
        <span className="ph-badge">{t("journal.people.badge")}</span>
        <h1 className="ph-h1 ph-h1--small" tabIndex={-1} ref={heading}>
          {t("journal.people.which", { name: q.name })}
        </h1>
        <p className="ph-serif ph-quote-line">{`“${q.quote}”`}</p>
        <p className="ph-rc">{t("phone.journal.which.from", { kind, when: whenText(q, today) })}</p>
        <fieldset className="ph-options" aria-label={t("journal.people.which", { name: q.name })}>
          {q.options.map((o) => (
            <label key={o.entityId} className="ph-option">
              <input
                type="radio"
                name="which"
                checked={choice === o.entityId}
                onChange={() => setChoice(o.entityId)}
              />
              <span className="ph-radio" aria-hidden="true" />
              <span>
                {o.name}
                {o.role ? <span className="ph-option-role">{o.role}</span> : null}
              </span>
            </label>
          ))}
          <label className="ph-option">
            <input
              type="radio"
              name="which"
              checked={choice === "new"}
              onChange={() => setChoice("new")}
            />
            <span className="ph-radio" aria-hidden="true" />
            <span>{t("journal.people.someoneNew")}</span>
          </label>
        </fieldset>
        <p className="ph-hint">{t("phone.journal.which.note")}</p>
        {problem ? (
          <p className="ph-error" role="alert">
            {t("phone.keep.failed.title")}
          </p>
        ) : null}
      </main>
      <footer className="ph-foot">
        <Button
          kind="primary"
          disabled={reply === null || busy}
          onClick={() => reply && void answer(reply)}
        >
          {t("journal.people.thisOne")}
        </Button>
        <Button kind="quiet" disabled={busy} onClick={() => void answer({ type: "leave" })}>
          {t("journal.people.leave")}
        </Button>
      </footer>
    </Page>
  );
}
