import { t } from "../../copy/t";
import styles from "./Ask.module.css";

interface AnswerTextProps {
  text: string;
  /** The nth bold span stands for the nth memory. */
  memoryIds: string[];
  /** Which memory the pointer or the keyboard is on, or null. */
  onActive: (memoryId: string | null) => void;
  onJump: (memoryId: string) => void;
}

/**
 * The answer as a paragraph. Each bold span is a control that shows where it came from: hovering
 * or focusing it outlines its receipt, pressing it moves to that receipt.
 */
export function AnswerText({ text, memoryIds, onActive, onJump }: AnswerTextProps) {
  const only = memoryIds.length === 1 ? memoryIds[0] : undefined;
  return (
    <p className={styles.answer}>
      {text.split("**").map((part, index) => {
        const key = `${index}-${part}`;
        if (index % 2 === 0) return part;
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
