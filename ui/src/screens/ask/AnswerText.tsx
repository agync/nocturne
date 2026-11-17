import type { ReactNode } from "react";
import { ControlChip } from "../../components/ControlChip/ControlChip";
import type { ChipId } from "../../components/ControlChip/registry";
import { t } from "../../copy/t";
import styles from "./Ask.module.css";

interface AnswerTextProps {
  text: string;
  /** The nth bold span stands for the nth memory. */
  memoryIds: string[];
  /** The person said it was wrong and gave the right sentence: the old answer stays, struck through. */
  corrected?: boolean;
  /** Which memory the pointer or the keyboard is on, or null. */
  onActive: (memoryId: string | null) => void;
  onJump: (memoryId: string) => void;
  /** Controls the answer may name, drawn as their chips where its plain words name them. */
  chips?: ReadonlyArray<{ words: string; id: ChipId }>;
}

const literal = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

/** Plain answer words, with each control they name drawn as its chip. */
function withChips(
  text: string,
  chips: ReadonlyArray<{ words: string; id: ChipId }>,
  at: number,
): ReactNode {
  if (chips.length === 0 || text === "") return text;
  const longest = [...chips].sort((a, b) => b.words.length - a.words.length);
  const pattern = new RegExp(`(${longest.map((c) => literal(c.words)).join("|")})`);
  return text.split(pattern).map((part, i) => {
    const chip = longest.find((c) => c.words === part);
    return chip ? <ControlChip key={`${at}-${i}`} id={chip.id} /> : part;
  });
}

/**
 * The answer as a paragraph. Each bold span is a control that shows where it came from: hovering
 * or focusing it outlines its receipt, pressing it moves to that receipt.
 */
export function AnswerText({
  text,
  memoryIds,
  corrected = false,
  onActive,
  onJump,
  chips = [],
}: AnswerTextProps) {
  const only = memoryIds.length === 1 ? memoryIds[0] : undefined;
  return (
    <p className={styles.answer} data-corrected={corrected}>
      {text.split("**").map((part, index) => {
        const key = `${index}-${part}`;
        if (index % 2 === 0) return withChips(part, chips, index);
        const memoryId = memoryIds[(index - 1) / 2] ?? only;
        if (!memoryId || part === "") return <strong key={key}>{part}</strong>;
        return (
          <button
            key={key}
            type="button"
            className={styles.span}
            aria-label={t("ask.span", { text: part })}
            onMouseEnter={() => onActive(memoryId)}
            onMouseLeave={() => onActive(null)}
            onFocus={() => onActive(memoryId)}
            onBlur={() => onActive(null)}
            onClick={() => onJump(memoryId)}
          >
            {part}
          </button>
        );
      })}
    </p>
  );
}
