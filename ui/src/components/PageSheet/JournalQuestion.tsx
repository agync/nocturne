import { useEffect, useRef, useState } from "react";
import { t } from "../../copy/t";
import type { Question, QuestionChoice, Receipt } from "../../engine/bindings";
import { Button } from "../Button/Button";
import { ReceiptChip } from "../ReceiptChip/ReceiptChip";
import { chipText, choiceText, opening } from "./journalQuestion";
import styles from "./PageSheet.module.css";

interface JournalQuestionProps {
  id: string;
  question: Question;
  onChoose: (choice: QuestionChoice) => void;
  onAddLine: (line: string) => void;
  onOpenReceipt: (receipt: Receipt) => void;
}

const CHOICES = ["fine", "notGreat", "notYet"] as const;

/** The page's one question when it comes from the journal: ask, answered, then added. */
export function JournalQuestion({
  id,
  question,
  onChoose,
  onAddLine,
  onOpenReceipt,
}: JournalQuestionProps) {
  const ask = question.journal;
  const [line, setLine] = useState("");
  const field = useRef<HTMLInputElement>(null);
  const kept = useRef<HTMLParagraphElement>(null);
  const pressed = useRef(false);
  const submitted = useRef(false);
  const answered = question.answered;
  const added = ask?.line ? ask.line : null;

  useEffect(() => {
    // The choices are gone once one is pressed, so focus moves to what replaced them.
    if (answered && pressed.current && !added) {
      pressed.current = false;
      field.current?.focus();
    }
    // Add removes the form, so focus moves to the line that replaced it.
    if (added && submitted.current) {
      submitted.current = false;
      kept.current?.focus();
    }
  }, [answered, added]);

  if (!ask) return null;

  const submit = () => {
    const clean = line.trim();
    if (clean === "") return;
    submitted.current = true;
    onAddLine(clean);
  };

  return (
    <section className={styles.question} aria-labelledby={`${id}-q`}>
      <h2 className={`t-section ${styles.sectionTitle}`}>{t("journal.question.label")}</h2>
      <p id={`${id}-q`} className={styles.questionText} data-quiet={answered !== null}>
        {opening(ask)}{" "}
        <em className={styles.quote}>{t("journal.question.quote", { sentence: question.text })}</em>{" "}
        {t("journal.question.howDidItGo")}
      </p>
      {question.receipt ? (
        <span className={styles.questionSource}>
          <ReceiptChip
            receipt={question.receipt}
            text={chipText(question.receipt, ask)}
            onOpen={onOpenReceipt}
          />
        </span>
      ) : null}
      {answered === null ? (
        <div className={styles.answers}>
          {CHOICES.map((choice) => (
            <Button
              key={choice}
              variant="secondary"
              touch
              onClick={() => {
                pressed.current = true;
                onChoose(choice);
              }}
            >
              {choiceText(choice)}
            </Button>
          ))}
        </div>
      ) : (
        <div className={styles.kept} role="status" aria-live="polite">
          <p className={styles.keptTitle}>
            {t("journal.question.answered", { choice: choiceText(answered) })}
          </p>
          {added ? (
            <>
              <p ref={kept} tabIndex={-1} className={styles.keptLine}>
                {t("journal.question.added", { line: added })}
              </p>
              <a className={`${styles.openJournal} hit`} href="#/journal">
                {t("journal.question.open")}
              </a>
            </>
          ) : null}
        </div>
      )}
      {answered !== null && !added ? (
        <form
          className={styles.addLine}
          onSubmit={(event) => {
            event.preventDefault();
            submit();
          }}
        >
          <label htmlFor={`${id}-more`} className={styles.addLabel}>
            {t("journal.question.add.label")}
          </label>
          <div className={styles.addRow}>
            <input
              ref={field}
              id={`${id}-more`}
              className={styles.addInput}
              type="text"
              value={line}
              placeholder={t("journal.question.add.placeholder")}
              onChange={(event) => setLine(event.target.value)}
            />
            <Button type="submit" variant="secondary" touch disabled={line.trim() === ""}>
              {t("journal.question.add.button")}
            </Button>
          </div>
        </form>
      ) : null}
    </section>
  );
}
