import { useEffect, useRef, useState } from "react";
import { formatMonth, formatWeekday } from "../../copy/format";
import { t } from "../../copy/t";
import type { Choice, PageLine as Line, Receipt, WhichResolved } from "../../engine/bindings";
import { Button } from "../Button/Button";
import { ReceiptChip } from "../ReceiptChip/ReceiptChip";
import styles from "./PageLine.module.css";
import { plainText, richText } from "./richText";

export type LineAction =
  | { type: "done"; line: Line }
  | { type: "which"; line: Line; choice: Choice }
  | { type: "confirm"; line: Line }
  | { type: "forget"; line: Line }
  | { type: "bringBack"; line: Line }
  | { type: "open"; receipt: Receipt };

interface PageLineProps {
  line: Line;
  onAction: (action: LineAction) => void;
  /** What the engine said it settled, for the spoken confirmation of a which-is-right choice. */
  resolved?: WhichResolved;
  /** Days a brought-back line stays clear of fading. */
  fadeDays?: number;
}

function Chips({
  receipts,
  onAction,
}: {
  receipts: Receipt[];
  onAction: PageLineProps["onAction"];
}) {
  if (receipts.length === 0) return null;
  return (
    <span className={styles.chips}>
      {receipts.map((receipt) => (
        <ReceiptChip
          key={`${receipt.itemId}-${JSON.stringify(receipt.span)}`}
          receipt={receipt}
          onOpen={(r) => onAction({ type: "open", receipt: r })}
        />
      ))}
    </span>
  );
}

/**
 * The control a person just used disappears when it is done, which would drop focus to the page
 * body. This hands focus to the result instead, once, after the person's own action.
 */
function useFocusResult(done: boolean) {
  const result = useRef<HTMLElement>(null);
  const acted = useRef(false);
  useEffect(() => {
    if (done && acted.current) {
      acted.current = false;
      result.current?.focus();
    }
  }, [done]);
  return {
    result,
    act: () => {
      acted.current = true;
    },
  };
}

type Outcome = "confirmed" | null;

/** The unit of the morning page: the claim, where it came from, and what to do about it. */
export function PageLine({ line, onAction, resolved, fadeDays = 30 }: PageLineProps) {
  const { result, act } = useFocusResult(line.done);
  const [outcome, setOutcome] = useState<Outcome>(null);
  // A line with nothing behind it cannot be acted on, so it offers nothing to pretend with.
  const canAct = line.memoryIds.length > 0;

  if (line.forgotten) {
    return (
      <div className={styles.line} data-kind={line.kind} data-forgotten="true">
        <p className={styles.forgotten} data-forgotten-line={line.id} tabIndex={-1}>
          {t("page.line.forgotten")}
        </p>
      </div>
    );
  }

  if (line.kind === "letGo" || line.kind === "merged") {
    const restorable = line.kind === "letGo";
    const why = line.done
      ? t("page.letGo.restored", { days: fadeDays })
      : line.kind === "merged"
        ? t("page.merged.why")
        : line.lastUsed
          ? t("page.letGo.why", { month: formatMonth(line.lastUsed) })
          : t("page.letGo.whyNever");
    return (
      <div className={styles.letGo} data-kind={line.kind} data-restored={line.done}>
        <div className={styles.letGoText}>
          <span className={styles.letGoTitle}>{richText(line.text)}</span>
          <span
            ref={result}
            className={styles.why}
            role={line.done ? "status" : undefined}
            aria-live={line.done ? "polite" : undefined}
            tabIndex={-1}
          >
            {why}
          </span>
          <Chips receipts={line.receipts} onAction={onAction} />
        </div>
        {restorable && !line.done ? (
          <Button
            variant="secondary"
            aria-label={t("fading.bringBack.named", { sentence: plainText(line.text) })}
            disabled={!canAct}
            onClick={() => {
              act();
              onAction({ type: "bringBack", line });
            }}
          >
            {t("button.bringBack")}
          </Button>
        ) : null}
      </div>
    );
  }

  if (line.kind === "upcoming") {
    return (
      <div className={styles.upcoming} data-kind="upcoming">
        <span className={styles.weekday}>{line.on ? formatWeekday(line.on) : ""}</span>
        <div className={styles.upcomingBody}>
          <span className={styles.upcomingText}>{richText(line.text)}</span>
          <Chips receipts={line.receipts} onAction={onAction} />
        </div>
      </div>
    );
  }

  if (line.kind === "which") {
    const chosen = line.done
      ? line.choices.find((c) => c.memoryId === line.memoryIds[0])
      : undefined;
    const spoken = resolved
      ? t("page.which.announce", { subject: resolved.subject, chosen: resolved.chosen })
      : "";
    return (
      <div className={styles.line} data-kind="which" data-done={line.done}>
        {!line.done ? (
          <fieldset className={styles.choices}>
            <legend className={styles.text}>{richText(line.text)}</legend>
            {line.choices.map((choice) => (
              <div key={choice.memoryId} className={styles.choiceCard}>
                <button
                  type="button"
                  className={styles.choice}
                  disabled={!line.conflictId}
                  onClick={() => {
                    act();
                    onAction({ type: "which", line, choice });
                  }}
                >
                  {choice.text}
                </button>
                <ReceiptChip
                  receipt={choice.receipt}
                  onOpen={(r) => onAction({ type: "open", receipt: r })}
                />
              </div>
            ))}
          </fieldset>
        ) : (
          <p className={styles.text}>{richText(line.text)}</p>
        )}
        <p
          ref={result as React.RefObject<HTMLParagraphElement>}
          className={styles.result}
          tabIndex={-1}
        >
          {chosen ? richText(t("page.which.resolved", { chosen: `**${chosen.text}**` })) : null}
        </p>
        <span className="sr-only" role="status" aria-live="polite">
          {spoken}
        </span>
      </div>
    );
  }

  const actionable = line.kind === "due" || line.kind === "promise";
  const unconfirmed = line.kind === "unconfirmed";
  const message = line.done
    ? unconfirmed
      ? outcome === "confirmed"
        ? t("page.unconfirmed.confirmed")
        : t("page.unconfirmed.settled")
      : t("page.line.done")
    : "";
  return (
    <div className={styles.line} data-kind={line.kind} data-done={line.done}>
      <p className={styles.text} data-done={line.done}>
        {richText(line.text)}
      </p>
      <div className={styles.footer}>
        <Chips receipts={line.receipts} onAction={onAction} />
        {actionable && !line.done ? (
          <Button
            variant="quiet"
            disabled={!canAct}
            onClick={() => {
              act();
              onAction({ type: "done", line });
            }}
          >
            {t("button.markDone")}
          </Button>
        ) : null}
        {unconfirmed && !line.done ? (
          <>
            <Button
              variant="secondary"
              disabled={!canAct}
              onClick={() => {
                act();
                setOutcome("confirmed");
                onAction({ type: "confirm", line });
              }}
            >
              {t("page.unconfirmed.confirm")}
            </Button>
            <Button
              variant="quiet"
              disabled={!canAct}
              onClick={() => {
                onAction({ type: "forget", line });
              }}
            >
              {t("page.unconfirmed.deny")}
            </Button>
          </>
        ) : null}
        {actionable || unconfirmed ? (
          <span
            ref={result}
            className={styles.status}
            role="status"
            aria-live="polite"
            tabIndex={-1}
          >
            {message}
          </span>
        ) : null}
      </div>
    </div>
  );
}
