import { type KeyboardEvent, useRef } from "react";
import { t } from "../../copy/t";
import type { Edition } from "../../engine/bindings";
import styles from "./EditionTabs.module.css";

const EDITIONS: Edition[] = ["screen", "audio", "print"];

const LABEL = {
  screen: "edition.read",
  audio: "edition.listen",
  print: "edition.print",
} as const;

interface EditionTabsProps {
  current: Edition;
  onSelect: (edition: Edition) => void;
  /** The id of the panel these tabs control. */
  panelId: string;
  tabIdFor: (edition: Edition) => string;
}

/** Read, Listen and Print: the same page in three forms. Arrow keys move, Enter chooses. */
export function EditionTabs({ current, onSelect, panelId, tabIdFor }: EditionTabsProps) {
  const refs = useRef<Partial<Record<Edition, HTMLButtonElement | null>>>({});

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const index = EDITIONS.indexOf(
      (document.activeElement as HTMLElement | null)?.dataset.edition as Edition,
    );
    if (index === -1) return;
    let next = index;
    if (event.key === "ArrowRight") next = (index + 1) % EDITIONS.length;
    else if (event.key === "ArrowLeft") next = (index - 1 + EDITIONS.length) % EDITIONS.length;
    else if (event.key === "Home") next = 0;
    else if (event.key === "End") next = EDITIONS.length - 1;
    else return;
    event.preventDefault();
    refs.current[EDITIONS[next] as Edition]?.focus();
  };

  return (
    <div
      role="tablist"
      aria-label={t("edition.label")}
      className={styles.tabs}
      onKeyDown={onKeyDown}
    >
      {EDITIONS.map((edition) => (
        <button
          key={edition}
          ref={(el) => {
            refs.current[edition] = el;
          }}
          type="button"
          role="tab"
          id={tabIdFor(edition)}
          data-edition={edition}
          aria-selected={edition === current}
          aria-controls={edition === current ? panelId : undefined}
          tabIndex={edition === current ? 0 : -1}
          className={styles.tab}
          onClick={() => onSelect(edition)}
        >
          {t(LABEL[edition])}
        </button>
      ))}
    </div>
  );
}
