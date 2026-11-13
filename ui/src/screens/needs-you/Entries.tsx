import { useId } from "react";
import { Button } from "../../components/Button/Button";
import { ReceiptChip } from "../../components/ReceiptChip/ReceiptChip";
import { Thumb } from "../../components/Thumb/Thumb";
import { formatDay, formatDuration } from "../../copy/format";
import { t } from "../../copy/t";
import type { MemoryView, Receipt, WhichOption } from "../../engine/bindings";
import styles from "./NeedsYou.module.css";

const shortDate = (iso: string) =>
  new Intl.DateTimeFormat("en-GB", { day: "numeric", month: "short" }).format(new Date(iso));

const weekdayLong = (iso: string) =>
  new Intl.DateTimeFormat("en-GB", { weekday: "long" }).format(new Date(`${iso}T12:00:00`));

/** "Appointment card · photo · 3 Nov", or for a voice note "Voice note · 0:08 · 10 Nov". */
export function optionMeta(receipt: Receipt): string {
  if (receipt.kind === "audio") {
    return [
      t("kind.audio"),
      receipt.durationMs === null ? null : formatDuration(receipt.durationMs),
      shortDate(receipt.keptAt),
    ]
      .filter(Boolean)
      .join(" · ");
  }
  return t("needs.source", {
    label: receipt.label,
    kind: t(`kind.${receipt.kind}.spoken`),
    date: shortDate(receipt.keptAt),
  });
}

/** "Letter · photo · Tue 11 Nov" */
export function sourceLine(receipt: Receipt): string {
  return t("needs.source", {
    label: receipt.label,
    kind: t(`kind.${receipt.kind}.spoken`),
    date: formatDay(receipt.keptAt),
  });
}

/** "Due Friday" within the week, "Due Fri 21 Nov" after that. */
export function dueBadge(due: string | null, today: string | undefined): string {
  if (!due) return t("needs.due.badge.unknown");
  const days = today
    ? (Date.parse(`${due}T12:00:00Z`) - Date.parse(`${today}T12:00:00Z`)) / 864e5
    : 99;
  return t("needs.due.badge", { day: days >= 0 && days < 7 ? weekdayLong(due) : formatDay(due) });
}

const WAVE = [10, 24, 16, 30, 12, 20];

function Preview({ receipt }: { receipt: Receipt }) {
  return (
    <span className={styles.preview} data-kind={receipt.kind} aria-hidden="true">
      {receipt.kind === "audio" ? (
        WAVE.map((h, i) => <span key={`${i}-${h}`} className={styles.bar} style={{ height: h }} />)
      ) : receipt.kind === "image" ? null : (
        <Thumb kind={receipt.kind} size={18} />
      )}
    </span>
  );
}

interface EntryProps {
  index: number;
  busy: boolean;
}

interface WhichProps extends EntryProps {
  title: string;
  options: WhichOption[];
  onPick: (option: WhichOption) => void;
  onOpen: (receipt: Receipt) => void;
}

/** Two sources disagree: each is a card with where it came from and a button to choose it. */
export function WhichEntry({ title, options, onPick, busy, onOpen }: WhichProps) {
  const heading = useId();
  return (
    <section className={styles.entry} aria-labelledby={heading}>
      <div className={styles.entryHead}>
        <span className={styles.badge} data-tone="ember">
          {t("needs.which.badge")}
        </span>
        <h2 id={heading} className={styles.entryTitle}>
          {title}
        </h2>
      </div>
      <fieldset className={styles.options}>
        <legend className="sr-only">{t("needs.which.group")}</legend>
        {options.map((option, i) => (
          <div key={option.memoryId} className={styles.option}>
            <Preview receipt={option.receipt} />
            <div className={styles.optionBody}>
              <span className={styles.optionText}>{option.text}</span>
              <button
                type="button"
                className={styles.optionMeta}
                onClick={() => onOpen(option.receipt)}
              >
                {optionMeta(option.receipt)}
              </button>
              <button
                type="button"
                className={styles.pick}
                data-choice
                disabled={busy}
                aria-keyshortcuts={String(i + 1)}
                aria-label={t("needs.which.choose", { text: option.text })}
                onClick={() => onPick(option)}
              >
                {t("needs.which.pick")}
                <span className={styles.key} data-key={i + 1} aria-hidden="true" />
              </button>
            </div>
          </div>
        ))}
      </fieldset>
    </section>
  );
}

interface UnconfirmedProps extends EntryProps {
  memory: MemoryView;
  agreeingSources: number;
  used: boolean;
  onTrue: () => void;
  onFalse: () => void;
  onLeave: () => void;
  onOpen: (receipt: Receipt) => void;
}

/** Heard once, from a source Nocturne does not trust: the person decides. */
export function UnconfirmedEntry({
  memory,
  agreeingSources,
  used,
  onTrue,
  onFalse,
  onLeave,
  onOpen,
  busy,
}: UnconfirmedProps) {
  const heading = useId();
  const receipt = memory.receipts[0];
  return (
    <section className={styles.entry} aria-labelledby={heading}>
      <div className={styles.entryHead}>
        <span className={styles.badge} data-tone="marker">
          {t("needs.unconfirmed.badge")}
        </span>
        <h2 id={heading} className={styles.entryTitle}>
          {t("needs.unconfirmed.quote", { sentence: memory.sentence.replace(/\.$/, "") })}
        </h2>
      </div>
      <div className={styles.evidence}>
        {receipt ? <ReceiptChip receipt={receipt} onOpen={onOpen} /> : null}
        <p className={styles.evidenceText}>
          {agreeingSources > 0
            ? t("needs.unconfirmed.agree", { count: agreeingSources })
            : t("needs.unconfirmed.alone")}{" "}
          {t(used ? "needs.unconfirmed.used" : "needs.unconfirmed.unused")}
        </p>
      </div>
      <div className={styles.buttons}>
        <Button
          variant="secondary"
          data-choice
          disabled={busy}
          aria-keyshortcuts="1"
          onClick={onTrue}
        >
          {t("needs.unconfirmed.true")}
        </Button>
        <Button
          variant="primary"
          data-choice
          disabled={busy}
          aria-keyshortcuts="2"
          onClick={onFalse}
        >
          {t("needs.unconfirmed.false")}
        </Button>
        <button
          type="button"
          className={styles.leave}
          data-choice
          disabled={busy}
          aria-keyshortcuts="3"
          onClick={onLeave}
        >
          {t("needs.unconfirmed.leave")}
        </button>
      </div>
    </section>
  );
}

interface DueProps extends EntryProps {
  memory: MemoryView;
  badge: string;
  onDone: () => void;
  onOpen: (receipt: Receipt) => void;
}

/** A form to sign, a promise to keep: it waits until the person says it is done. */
export function DueEntry({ memory, badge, onDone, onOpen, busy }: DueProps) {
  const heading = useId();
  const receipt = memory.receipts[0];
  return (
    <section className={styles.entry} data-row="true" aria-labelledby={heading}>
      <div className={styles.dueText}>
        <div className={styles.entryHead}>
          <span className={styles.badge} data-tone="ember">
            {badge}
          </span>
          <h2 id={heading} className={styles.entryTitle}>
            {memory.sentence}
          </h2>
        </div>
        {receipt ? (
          <button type="button" className={styles.source} onClick={() => onOpen(receipt)}>
            {sourceLine(receipt)}
          </button>
        ) : null}
      </div>
      <Button
        variant="secondary"
        data-choice
        disabled={busy}
        aria-keyshortcuts="1"
        aria-label={`${t("needs.done")}: ${memory.sentence}`}
        onClick={onDone}
      >
        {t("needs.done")}
      </Button>
    </section>
  );
}
