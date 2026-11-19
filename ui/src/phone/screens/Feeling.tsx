import { middleOf } from "../../components/FeelingField/FeelingField";
import { t } from "../../copy/t";
import type { Feeling, FeelingPart } from "../../engine/bindings";
import { FeelingMark } from "../../journal/FeelingMark";
import { PART_ORDER, PARTS, WORDS } from "../../journal/feelings";

/** What a keep may say about how it feels: a part and, optionally, one word of it. */
export interface Pick {
  part: FeelingPart;
  word: string | null;
}

/** The feeling a keep carries: the middle of the part, and the chosen word. */
export function feelingOfPick(pick: Pick): Feeling {
  return {
    part: pick.part,
    point: middleOf(pick.part),
    words: pick.word ? [pick.word] : [],
  };
}

interface PickProps {
  pick: Pick | null;
  onPick(next: Pick | null): void;
  /** "More words…" opens the whole check-in at this part; without it the link is not offered. */
  onMore?(part: FeelingPart): void;
}

/**
 * "How does it feel? (optional)": the four parts as buttons, and for the chosen part the four
 * words nearest its middle with "More words…".
 */
export function FeelingPick({ pick, onPick, onMore }: PickProps) {
  const words = pick ? WORDS[pick.part].slice(0, 4) : [];
  return (
    <>
      <fieldset className="ph-fieldset">
        <legend className="ph-lbl">{t("journal.quick.label")}</legend>
        <div className="ph-feel">
          {PART_ORDER.map((part) => {
            const on = pick?.part === part;
            return (
              <button
                key={part}
                type="button"
                className="ph-feel-btn"
                data-part={part}
                aria-pressed={on}
                onClick={() => onPick(on ? null : { part, word: null })}
              >
                <FeelingMark mark={part} size={20} />
                {t(PARTS[part].name)}
              </button>
            );
          })}
        </div>
      </fieldset>
      {pick ? (
        <div className="ph-stack" style={{ gap: 8 }}>
          <fieldset className="ph-fieldset">
            <legend className="ph-lbl">{t("journal.quick.words")}</legend>
            <div className="ph-fwords">
              {words.map((entry) => {
                const on = pick.word === entry.word;
                return (
                  <button
                    key={entry.id}
                    type="button"
                    className="ph-fword"
                    data-part={pick.part}
                    aria-pressed={on}
                    onClick={() => onPick({ part: pick.part, word: on ? null : entry.word })}
                  >
                    {t(entry.wordId)}
                  </button>
                );
              })}
            </div>
          </fieldset>
          {onMore ? (
            <button type="button" className="ph-link" onClick={() => onMore(pick.part)}>
              {t("journal.quick.more")}
            </button>
          ) : null}
        </div>
      ) : null}
    </>
  );
}

/** "This is a good thing", a switch with the sun. */
export function GoodSwitch({ on, onChange }: { on: boolean; onChange(next: boolean): void }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={on}
      className="ph-switch-row"
      onClick={() => onChange(!on)}
    >
      <FeelingMark mark="sun" size={20} tone={false} />
      <span className="ph-switch-label">{t("phone.journal.good")}</span>
      <span className="ph-switch" aria-hidden="true" />
    </button>
  );
}
