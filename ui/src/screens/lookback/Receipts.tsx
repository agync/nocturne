import { Fragment, type ReactNode, useCallback, useRef, useState } from "react";
import { receiptPath } from "../../components/ReceiptChip/receiptPath";
import { formatDayMonth, formatDuration } from "../../copy/format";
import type { CopyId } from "../../copy/t";
import { t, tParts } from "../../copy/t";
import type { Receipt } from "../../engine/bindings";
import styles from "./Lookback.module.css";
import { clock, receiptKey, weekdayDay } from "./text";

/** A sentence whose placeholders are drawn as elements, each given a key. */
export function rich(id: CopyId, values: Record<string, unknown>): ReactNode[] {
  return tParts(id, values).map((part, index) => {
    const key = `${id}-${index}`;
    return typeof part === "string" ? part : <Fragment key={key}>{part as ReactNode}</Fragment>;
  });
}

/** Puts a separator between nodes. */
export function joined(nodes: ReactNode[], separator: ReactNode = ", "): ReactNode[] {
  return nodes.flatMap((node, index) => {
    const key = `n-${index}`;
    return index === 0
      ? [<Fragment key={key}>{node}</Fragment>]
      : [
          <Fragment key={`s-${index}`}>{separator}</Fragment>,
          <Fragment key={key}>{node}</Fragment>,
        ];
  });
}

/** What the chip of a receipt says: "7:52 am · note", "Voice note · 0:12", "Tue 10 · entry". */
export type DayStyle = false | "weekday" | "date";

export function chipText(receipt: Receipt, withDay: DayStyle): string {
  if (receipt.kind === "audio") {
    const at = receipt.span?.type === "time" ? receipt.span.startMs : (receipt.durationMs ?? 0);
    return t("journal.lookback.chip.voice", { duration: formatDuration(at) });
  }
  const isEntry = /entry$/i.test(receipt.label);
  const kind = isEntry ? "entry" : t(`kind.${receipt.kind}.spoken` as CopyId);
  if (isEntry && !withDay) return receipt.label;
  return withDay
    ? t("journal.lookback.chip.day", {
        day: withDay === "date" ? formatDayMonth(receipt.keptAt) : weekdayDay(receipt.keptAt),
        kind,
      })
    : t("journal.lookback.chip.time", { time: clock(receipt.keptAt), kind });
}

export interface ActiveReceipt {
  active: string | null;
  setActive: (key: string | null) => void;
  /** Moves focus to the chip of a receipt. */
  jump: (key: string) => void;
  register: (key: string) => (element: HTMLAnchorElement | null) => void;
}

/** Which receipt the pointer or keyboard is on, and where each chip is. */
export function useActiveReceipt(): ActiveReceipt {
  const [active, setActive] = useState<string | null>(null);
  const chips = useRef(new Map<string, HTMLAnchorElement>());
  const register = useCallback(
    (key: string) => (element: HTMLAnchorElement | null) => {
      if (element) chips.current.set(key, element);
      else chips.current.delete(key);
    },
    [],
  );
  const jump = useCallback((key: string) => chips.current.get(key)?.focus(), []);
  return { active, setActive, jump, register };
}

interface SpanTextProps {
  /** Text with `**spans**`; the nth span stands for the nth receipt. */
  text: string;
  receipts: readonly Receipt[];
  receipt: ActiveReceipt;
  /** Marks the one place a span is plain emphasis. */
  className?: string;
}

/** Bold spans that outline their receipt on hover and focus, as an answer's spans do. */
export function SpanText({ text, receipts, receipt, className }: SpanTextProps) {
  const only = receipts.length === 1 ? receipts[0] : undefined;
  return (
    <span className={className}>
      {text.split("**").map((part, index) => {
        const key = `${index}-${part}`;
        if (index % 2 === 0) return part;
        const source = receipts[(index - 1) / 2] ?? only;
        if (!source || part === "") return <strong key={key}>{part}</strong>;
        const id = receiptKey(source);
        return (
          <button
            key={key}
            type="button"
            className={styles.span}
            data-on={receipt.active === id}
            onMouseEnter={() => receipt.setActive(id)}
            onMouseLeave={() => receipt.setActive(null)}
            onFocus={() => receipt.setActive(id)}
            onBlur={() => receipt.setActive(null)}
            onClick={() => receipt.jump(id)}
          >
            {part}
          </button>
        );
      })}
    </span>
  );
}

/** The receipts of a paragraph, once each, as chips that open the original. */
export function Chips({
  receipts,
  receipt,
  withDay = false,
}: {
  receipts: readonly Receipt[];
  receipt: ActiveReceipt;
  withDay?: DayStyle;
}) {
  const seen = new Set<string>();
  const unique = receipts.filter((r) => {
    const key = receiptKey(r);
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
  if (unique.length === 0) return null;
  return (
    <ul className={styles.chipRow} aria-label={t("journal.lookback.chips")}>
      {unique.map((r) => {
        const key = receiptKey(r);
        return (
          <li key={key}>
            <a
              ref={receipt.register(key)}
              className={styles.receiptChip}
              href={`#${receiptPath(r)}`}
              data-on={receipt.active === key}
              data-kind={r.kind}
              onMouseEnter={() => receipt.setActive(key)}
              onMouseLeave={() => receipt.setActive(null)}
              onFocus={() => receipt.setActive(key)}
              onBlur={() => receipt.setActive(null)}
            >
              {chipText(r, withDay)}
            </a>
          </li>
        );
      })}
    </ul>
  );
}
