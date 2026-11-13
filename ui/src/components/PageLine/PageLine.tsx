import { formatMonth, formatWeekday } from "../../copy/format";
import { t } from "../../copy/t";
import type { Choice, PageLine as Line, Receipt } from "../../engine/bindings";
import { Button } from "../Button/Button";
import { ReceiptChip, receiptText } from "../ReceiptChip/ReceiptChip";
import { Thumb } from "../Thumb/Thumb";
import styles from "./PageLine.module.css";
import { richText } from "./richText";

export type LineAction =
  | { type: "done"; line: Line }
  | { type: "which"; line: Line; choice: Choice }
  | { type: "confirm"; line: Line; confirmed: boolean }
  | { type: "bringBack"; line: Line }
  | { type: "open"; receipt: Receipt };

interface PageLineProps {
  line: Line;
  onAction: (action: LineAction) => void;
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

/** The unit of the morning page: the claim, where it came from, and what to do about it. */
export function PageLine({ line, onAction }: PageLineProps) {
  if (line.forgotten) {
    return (
      <div className={styles.line} data-kind={line.kind} data-forgotten="true">
        <p className={styles.forgotten}>{t("page.line.forgotten")}</p>
      </div>
    );
  }

  if (line.kind === "letGo" || line.kind === "merged") {
    const restorable = line.kind === "letGo";
    const why = line.done
      ? t("page.letGo.restored")
      : line.kind === "merged"
        ? t("page.merged.why")
        : line.lastUsed
          ? t("page.letGo.why", { month: formatMonth(line.lastUsed) })
          : t("page.letGo.whyNever");
    return (
      <div className={styles.letGo} data-kind={line.kind} data-restored={line.done}>
        <div className={styles.letGoText}>
          <span className={styles.letGoTitle}>{richText(line.text)}</span>
          <span className={styles.why} role={line.done ? "status" : undefined}>
            {why}
          </span>
        </div>
        {restorable && !line.done ? (
          <Button variant="secondary" onClick={() => onAction({ type: "bringBack", line })}>
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
    return (
      <div className={styles.line} data-kind="which" data-done={line.done}>
        <p className={styles.text}>{richText(line.text)}</p>
        {!line.done ? (
          <fieldset className={styles.choices}>
            <legend className="sr-only">{line.text}</legend>
            {line.choices.map((choice) => (
              <button
                key={choice.memoryId}
                type="button"
                className={styles.choice}
                onClick={() => onAction({ type: "which", line, choice })}
              >
                <span className={styles.choiceText}>{choice.text}</span>
                <span className={styles.choiceSource}>
                  <Thumb
                    kind={choice.receipt.kind}
                    size={choice.receipt.kind === "image" ? 16 : 12}
                  />
                  {receiptText(choice.receipt)}
                </span>
              </button>
            ))}
          </fieldset>
        ) : null}
        <div className={styles.result} role="status" aria-live="polite">
          {chosen ? (
            <span>
              <strong>{t("page.which.resolved", { choice: chosen.text })}</strong>
            </span>
          ) : null}
        </div>
      </div>
    );
  }

  const actionable = line.kind === "due" || line.kind === "promise";
  return (
    <div className={styles.line} data-kind={line.kind} data-done={line.done}>
      <p className={styles.text} data-done={line.done}>
        {richText(line.text)}
      </p>
      <div className={styles.footer}>
        <Chips receipts={line.receipts} onAction={onAction} />
        {actionable && !line.done ? (
          <Button variant="quiet" onClick={() => onAction({ type: "done", line })}>
            {t("button.markDone")}
          </Button>
        ) : null}
        {line.kind === "unconfirmed" && !line.done ? (
          <>
            <Button
              variant="secondary"
              onClick={() => onAction({ type: "confirm", line, confirmed: true })}
            >
              {t("page.unconfirmed.confirm")}
            </Button>
            <Button
              variant="quiet"
              onClick={() => onAction({ type: "confirm", line, confirmed: false })}
            >
              {t("page.unconfirmed.deny")}
            </Button>
          </>
        ) : null}
        {actionable ? (
          <span className={styles.status} role="status" aria-live="polite">
            {line.done ? t("page.line.done") : ""}
          </span>
        ) : null}
      </div>
    </div>
  );
}
