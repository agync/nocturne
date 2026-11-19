import type { KeyboardEvent, Ref } from "react";
import styles from "./Entry.module.css";

export interface Mark {
  start: number;
  end: number;
}

interface GrowTextProps {
  value: string;
  label: string;
  placeholder?: string;
  /** Ranges of the text to outline, in UTF-16 code units. Only drawn for text that is unchanged. */
  marks?: Mark[];
  textRef?: Ref<HTMLTextAreaElement>;
  onChange: (value: string) => void;
  onKeyDown?: (event: KeyboardEvent<HTMLTextAreaElement>) => void;
  onKeyUp?: (event: KeyboardEvent<HTMLTextAreaElement>) => void;
  onBlur?: () => void;
}

/** The text as plain pieces and outlined pieces, so the outline can sit behind the text. */
export function pieces(text: string, marks: Mark[]): Array<{ text: string; marked: boolean }> {
  const out: Array<{ text: string; marked: boolean }> = [];
  let at = 0;
  const sorted = [...marks].sort((a, b) => a.start - b.start);
  for (const mark of sorted) {
    if (mark.start < at || mark.end > text.length || mark.end <= mark.start) continue;
    if (mark.start > at) out.push({ text: text.slice(at, mark.start), marked: false });
    out.push({ text: text.slice(mark.start, mark.end), marked: true });
    at = mark.end;
  }
  if (at < text.length) out.push({ text: text.slice(at), marked: false });
  return out;
}

/**
 * A paragraph that grows with what is written. A copy of the words sits behind the field, which
 * gives it its height and draws the outline round a name while its chip is hovered or focused.
 */
export function GrowText({
  value,
  label,
  placeholder,
  marks = [],
  textRef,
  onChange,
  onKeyDown,
  onKeyUp,
  onBlur,
}: GrowTextProps) {
  return (
    <div className={styles.grow}>
      <div className={styles.mirror} aria-hidden="true">
        {pieces(value, marks).map((piece, i) =>
          piece.marked ? (
            <mark key={`${i}-${piece.text}`} className={styles.mark}>
              {piece.text}
            </mark>
          ) : (
            <span key={`${i}-${piece.text}`}>{piece.text}</span>
          ),
        )}
        {"\n"}
      </div>
      <textarea
        ref={textRef}
        className={styles.field}
        rows={1}
        value={value}
        aria-label={label}
        placeholder={placeholder}
        onChange={(event) => onChange(event.target.value)}
        onKeyDown={onKeyDown}
        onKeyUp={onKeyUp}
        onBlur={onBlur}
      />
    </div>
  );
}
