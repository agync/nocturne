import { useState } from "react";
import { Button } from "../../components/Button/Button";
import { t } from "../../copy/t";
import type { FeelingPart, FeelingPoint } from "../../engine/bindings";
import { FeelingMark } from "../../journal/FeelingMark";
import {
  type FeelingWord,
  MAIN_WORDS,
  nearestWords,
  PART_ORDER,
  PARTS,
  WORDS,
  wordsFor,
} from "../../journal/feelings";
import styles from "./Checkin.module.css";
import { MAX_WORDS } from "./selection";

/** The word the pointer or the keyboard is on, so its meaning can be shown as plain text. */
export function useMeaning() {
  const [hovered, setHovered] = useState<FeelingWord | null>(null);
  const [focused, setFocused] = useState<FeelingWord | null>(null);
  const bind = (word: FeelingWord) => ({
    onMouseEnter: () => setHovered(word),
    onMouseLeave: () => setHovered(null),
    onFocus: () => setFocused(word),
    onBlur: () => setFocused((now) => (now === word ? null : now)),
  });
  return { shown: hovered ?? focused, bind };
}

export function Meaning({ word }: { word: FeelingWord | null }) {
  return (
    <p className={styles.meaning} aria-live="polite">
      {word ? (
        <>
          <strong>{t(word.wordId)}</strong>
          {t("journal.checkin.meaning", { meaning: t(word.meaningId) })}
        </>
      ) : null}
    </p>
  );
}

interface PickerProps {
  words: readonly string[];
  onToggle: (word: string) => void;
  bind: ReturnType<typeof useMeaning>["bind"];
}

interface FieldWordsProps extends PickerProps {
  part: FeelingPart;
  point: FeelingPoint;
}

/** The part's words as toggles, the ones nearest the point first. */
export function FieldWords({ part, point, words, onToggle, bind }: FieldWordsProps) {
  const [more, setMore] = useState(false);
  const name = t(PARTS[part].name);
  const nearest = nearestWords(part, point);
  const shown = more ? [...nearest, ...WORDS[part].slice(MAIN_WORDS)] : nearest;
  const full = words.length >= MAX_WORDS;
  return (
    <>
      <div className={styles.sectionHead}>
        <h2 className="t-section">{t("journal.checkin.words", { part: name })}</h2>
        <span className={styles.small}>{t("journal.checkin.hint")}</span>
      </div>
      <fieldset className={styles.fieldset}>
        <legend className="sr-only">{t("journal.checkin.words", { part: name })}</legend>
        <ul className={styles.wordGrid}>
          {shown.map((word) => {
            const on = words.includes(word.word);
            const blocked = full && !on;
            return (
              <li key={word.id}>
                <button
                  type="button"
                  className={styles.chip}
                  data-part={part}
                  aria-pressed={on}
                  aria-disabled={blocked || undefined}
                  onClick={() => onToggle(word.word)}
                  {...bind(word)}
                >
                  {t(word.wordId)}
                </button>
              </li>
            );
          })}
        </ul>
      </fieldset>
      <Button variant="quiet" flush onClick={() => setMore(!more)} aria-expanded={more}>
        {t(more ? "journal.checkin.fewer" : "journal.checkin.more")}
      </Button>
    </>
  );
}

/** Every word, grouped by part, for anyone who finds the field hard to use. */
export function ListWords({ words, onToggle, bind }: PickerProps) {
  const [open, setOpen] = useState<ReadonlySet<FeelingPart>>(new Set());
  const full = words.length >= MAX_WORDS;
  const flip = (part: FeelingPart) =>
    setOpen((now) => {
      const next = new Set(now);
      if (!next.delete(part)) next.add(part);
      return next;
    });
  return (
    <div className={styles.lists}>
      {PART_ORDER.map((part) => (
        <fieldset key={part} className={styles.listPart} data-part={part}>
          <legend className={styles.listLegend}>
            <FeelingMark mark={part} size={16} />
            {t(PARTS[part].name)}
          </legend>
          {wordsFor(part, open.has(part)).map((word) => {
            const on = words.includes(word.word);
            return (
              <label key={word.id} className={styles.listWord} {...bind(word)}>
                <input
                  type="checkbox"
                  className={styles.listInput}
                  checked={on}
                  aria-disabled={(full && !on) || undefined}
                  onChange={() => onToggle(word.word)}
                />
                <span className={styles.box} aria-hidden="true" />
                {t(word.wordId)}
              </label>
            );
          })}
          <Button variant="quiet" flush onClick={() => flip(part)} aria-expanded={open.has(part)}>
            {t(open.has(part) ? "journal.checkin.fewer" : "journal.checkin.more")}
          </Button>
        </fieldset>
      ))}
    </div>
  );
}
