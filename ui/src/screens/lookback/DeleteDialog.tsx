import { useRef } from "react";
import { Button } from "../../components/Button/Button";
import { Dialog } from "../../components/Dialog/Dialog";
import { t } from "../../copy/t";
import type { Period } from "../../engine/bindings";
import styles from "./Lookback.module.css";
import { dayParts, monthName } from "./text";

/** "from 9 to 15 November", "from 30 November to 6 December", "in November", "in 2026". */
export function recapPeriodText(period: Period): string {
  if (period.kind === "week") {
    const start = dayParts(period.start);
    const end = dayParts(period.end);
    return t("journal.lookback.delete.recap.period.week", {
      start: start.month === end.month ? String(start.day) : `${start.day} ${start.month}`,
      end: `${end.day} ${end.month}`,
    });
  }
  if (period.kind === "month") {
    return t("journal.lookback.delete.recap.period.month", { month: monthName(period.start) });
  }
  return t("journal.lookback.delete.recap.period.year", { year: period.start.slice(0, 4) });
}

interface DeleteDialogProps {
  period: Period;
  onConfirm: () => void;
  onClose: () => void;
}

/** Deleting the writing for a day, week, month or year: small, and focus starts on Keep it. */
export function DeleteDialog({ period, onConfirm, onClose }: DeleteDialogProps) {
  const keep = useRef<HTMLElement | null>(null);
  const isDay = period.kind === "day";
  const kind = isDay ? "week" : period.kind;
  const day = dayParts(period.start);
  return (
    <Dialog
      title={t(
        isDay ? "journal.lookback.delete.reflection.title" : "journal.lookback.delete.recap.title",
      )}
      onClose={onClose}
      initialFocus={keep}
    >
      <div className={styles.dialogBody}>
        <p>
          {isDay
            ? t("journal.lookback.delete.reflection.body", {
                day: `${day.weekday} ${day.day} ${day.month}`,
              })
            : t("journal.lookback.delete.recap.body", { period: recapPeriodText(period) })}
        </p>
        <p className={styles.dialogNote}>
          {isDay
            ? t("journal.lookback.delete.reflection.note")
            : t("journal.lookback.delete.recap.note", { kind })}
        </p>
        <div className={styles.dialogActions}>
          <span
            ref={(el) => {
              keep.current = el?.querySelector("button") ?? null;
            }}
          >
            <Button variant="secondary" onClick={onClose}>
              {t("journal.lookback.delete.keep")}
            </Button>
          </span>
          <Button variant="forget" confirming onClick={onConfirm}>
            {t(
              isDay
                ? "journal.lookback.delete.reflection.confirm"
                : "journal.lookback.delete.recap.confirm",
            )}
          </Button>
        </div>
      </div>
    </Dialog>
  );
}
