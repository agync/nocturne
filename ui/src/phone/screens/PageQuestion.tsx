import { useCallback, useEffect, useRef, useState } from "react";
import { chipText, choiceText, opening } from "../../components/PageSheet/questionWords";
import { t } from "../../copy/t";
import type { PhoneError, Question, QuestionChoice } from "../../engine/bindings";
import {
  ConnectionError,
  type PendingKeep,
  type PhoneEngine,
  PhoneRequestError,
  type Queue,
} from "../engine";
import { withKeepLock } from "../queue";
import { clientId } from "./Keep";

const CHOICES = ["fine", "notGreat", "notYet"] as const;

/** Answers the computer cannot take now but will: the answer waits on the phone, as a keep does. */
const WAIT_CODES: ReadonlySet<PhoneError> = new Set([
  "busy",
  "tooMany",
  "notReady",
  "paused",
  "sessionExpired",
]);

/** What this phone sent, or holds while the computer is away. */
interface Sent {
  clientId: string;
  choice: QuestionChoice;
  line: string | null;
}

type Note = "elsewhere" | "replaced" | null;

interface PageQuestionProps {
  engine: PhoneEngine;
  queue: Queue;
  connected: boolean;
  pageDate: string;
  question: Question;
  /** The page is the latest, so a line may still be added. */
  latest: boolean;
  now(): number;
  onQueued(): void;
  /** Opens the journal day the quoted block was kept on. */
  onOpenDay(day: string): void;
  onOpenJournal(): void;
}

/**
 * The page's question from the journal, on the phone: ask, answered, then added, from whichever
 * device answered first. An answer pressed while the computer is away waits on the phone.
 */
export function PageQuestion(p: PageQuestionProps) {
  const { engine, queue, connected, pageDate, onQueued } = p;
  const [question, setQuestion] = useState(p.question);
  const [pending, setPending] = useState<Sent | null>(null);
  const [note, setNote] = useState<Note>(null);
  const [problem, setProblem] = useState<"choice" | "line" | null>(null);
  const [line, setLine] = useState("");
  const busy = useRef(false);
  const field = useRef<HTMLInputElement>(null);
  const kept = useRef<HTMLParagraphElement>(null);
  const focusNext = useRef<"field" | "kept" | null>(null);

  useEffect(() => setQuestion(p.question), [p.question]);

  const keep = useCallback(
    (sent: Sent): PendingKeep => ({
      clientId: sent.clientId,
      kind: "answer",
      answer: { pageDate, choice: sent.choice, line: sent.line },
      size: sent.line?.length ?? 0,
      createdAt: p.now(),
    }),
    [pageDate, p.now],
  );

  const send = useCallback(
    async (sent: Sent) => {
      if (busy.current) return;
      busy.current = true;
      setProblem(null);
      const drop = async () => {
        await queue.remove(sent.clientId);
        onQueued();
        setPending(null);
      };
      try {
        const page = await withKeepLock(() =>
          engine.answerJournalQuestion(
            pageDate,
            sent.choice,
            sent.clientId,
            sent.line ?? undefined,
          ),
        );
        await drop();
        const now = page.question;
        if (!now) return;
        setQuestion(now);
        const stored = now.journal?.line ?? null;
        if (now.answered !== sent.choice) setNote("elsewhere");
        else if (sent.line && stored !== sent.line) setNote(stored ? "elsewhere" : "replaced");
        else setNote(null);
      } catch (e) {
        if (e instanceof ConnectionError) return;
        if (e instanceof PhoneRequestError && WAIT_CODES.has(e.code)) return;
        await drop();
        if (sent.line) setLine(sent.line);
        setProblem(sent.line ? "line" : "choice");
      } finally {
        busy.current = false;
      }
    },
    [engine, queue, pageDate, onQueued],
  );

  // An answer that waited on this phone, from this visit or an earlier one, goes across as soon
  // as the computer can be reached. The computer keeps it once however often it arrives.
  useEffect(() => {
    let gone = false;
    queue.list().then(
      (keeps) => {
        if (gone) return;
        const waiting = keeps.find((k) => k.kind === "answer" && k.answer?.pageDate === pageDate);
        if (waiting?.answer)
          setPending({
            clientId: waiting.clientId,
            choice: waiting.answer.choice,
            line: waiting.answer.line,
          });
      },
      () => undefined,
    );
    return () => {
      gone = true;
    };
  }, [queue, pageDate]);

  useEffect(() => {
    if (connected && pending) void send(pending);
  }, [connected, pending, send]);

  const answered = question.answered ?? pending?.choice ?? null;
  const waiting = pending !== null && question.answered === null;
  const added = question.journal?.line ?? null;
  const lineWaiting = !added && pending?.line ? pending.line : null;
  const canAdd = p.latest && answered !== null && !added && !lineWaiting && note !== "replaced";

  useEffect(() => {
    if (focusNext.current === "field" && canAdd) field.current?.focus();
    else if (focusNext.current) kept.current?.focus();
    focusNext.current = null;
  });

  const ask = question.journal;
  if (!ask) return null;

  const choose = async (choice: QuestionChoice) => {
    if (busy.current || pending) return;
    const sent: Sent = { clientId: clientId(), choice, line: null };
    await queue.add(keep(sent));
    onQueued();
    setNote(null);
    focusNext.current = p.latest ? "field" : "kept";
    setPending(sent);
  };

  const addLine = async () => {
    const clean = line.trim();
    if (clean === "" || answered === null || busy.current) return;
    // A line added while the answer waits goes across with it, under the same id.
    const sent: Sent =
      waiting && pending
        ? { ...pending, line: clean }
        : { clientId: clientId(), choice: answered, line: clean };
    await queue.add(keep(sent));
    onQueued();
    setLine("");
    focusNext.current = "kept";
    setPending(sent);
  };

  const id = `pq-${pageDate}`;
  return (
    <section className="ph-jq" aria-labelledby={`${id}-q`}>
      <h3 className="ph-lbl">{t("journal.question.label")}</h3>
      <p id={`${id}-q`} className="ph-jq-text" data-quiet={answered !== null ? true : undefined}>
        {opening(ask)} <em>{t("journal.question.quote", { sentence: question.text })}</em>{" "}
        {t("journal.question.howDidItGo")}
      </p>
      {question.receipt ? (
        <button type="button" className="ph-src" onClick={() => p.onOpenDay(ask.writtenOn)}>
          {chipText(question.receipt, ask)}
        </button>
      ) : null}
      {answered === null ? (
        <div className="ph-choices">
          {CHOICES.map((choice) => (
            <button
              key={choice}
              type="button"
              className="ph-choice"
              onClick={() => void choose(choice)}
            >
              {choiceText(choice)}
            </button>
          ))}
        </div>
      ) : (
        <div className="ph-jq-kept" role="status">
          <p ref={kept} tabIndex={-1} className="ph-jq-done">
            {waiting
              ? t("phone.page.question.waiting", { choice: choiceText(answered) })
              : t("journal.question.answered", { choice: choiceText(answered) })}
          </p>
          {note === "elsewhere" ? (
            <p className="ph-jq-note">{t("phone.page.question.elsewhere")}</p>
          ) : null}
          {note === "replaced" ? (
            <p className="ph-jq-note">{t("phone.page.question.replaced")}</p>
          ) : null}
          {waiting || lineWaiting ? (
            <p className="ph-jq-note">{t("phone.keep.waiting.body")}</p>
          ) : null}
          {added ? (
            <p className="ph-jq-note">{t("journal.question.added", { line: added })}</p>
          ) : null}
          {lineWaiting ? (
            <p className="ph-jq-note">
              {t("phone.page.question.lineWaiting", { line: lineWaiting })}
            </p>
          ) : null}
          {added ? (
            <button type="button" className="ph-link" onClick={p.onOpenJournal}>
              {t("journal.question.open")}
            </button>
          ) : null}
        </div>
      )}
      {problem ? (
        <p className="ph-error" role="alert">
          <strong className="ph-error-title">{t("phone.keep.failed.title")}</strong>{" "}
          {t("phone.keep.failed.body")}
        </p>
      ) : null}
      {canAdd ? (
        <form
          className="ph-jq-add"
          onSubmit={(event) => {
            event.preventDefault();
            void addLine();
          }}
        >
          <label htmlFor={`${id}-more`} className="ph-label">
            {t("journal.question.add.label")}
          </label>
          <div className="ph-jq-row">
            <input
              ref={field}
              id={`${id}-more`}
              className="ph-input ph-input--line"
              type="text"
              value={line}
              placeholder={t("journal.question.add.placeholder")}
              onChange={(event) => setLine(event.target.value)}
            />
            <button
              type="submit"
              className="ph-btn ph-btn--add"
              aria-disabled={line.trim() === "" ? true : undefined}
            >
              {t("journal.question.add.button")}
            </button>
          </div>
        </form>
      ) : null}
    </section>
  );
}
