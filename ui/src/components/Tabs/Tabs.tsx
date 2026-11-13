import { type KeyboardEvent, type ReactNode, useId, useRef } from "react";
import styles from "./Tabs.module.css";

export interface TabSpec {
  id: string;
  label: string;
  panel: ReactNode;
}

interface TabsProps {
  /** Names the set of tabs for screen readers. */
  label: string;
  tabs: TabSpec[];
  value: string;
  onChange: (id: string) => void;
}

/**
 * The ARIA tabs pattern: arrow keys move focus and choose, Home and End jump, and only the
 * chosen tab is in the Tab order. The chosen panel is the one that is shown.
 */
export function Tabs({ label, tabs, value, onChange }: TabsProps) {
  const base = useId();
  const refs = useRef<Record<string, HTMLButtonElement | null>>({});

  const move = (index: number) => {
    const next = tabs[(index + tabs.length) % tabs.length];
    if (!next) return;
    onChange(next.id);
    refs.current[next.id]?.focus();
  };

  const onKeyDown = (event: KeyboardEvent, index: number) => {
    if (event.key === "ArrowRight") move(index + 1);
    else if (event.key === "ArrowLeft") move(index - 1);
    else if (event.key === "Home") move(0);
    else if (event.key === "End") move(tabs.length - 1);
    else return;
    event.preventDefault();
  };

  const current = tabs.find((tab) => tab.id === value) ?? tabs[0];

  return (
    <div className={styles.tabs}>
      <div role="tablist" aria-label={label} className={styles.list}>
        {tabs.map((tab, index) => (
          <button
            key={tab.id}
            ref={(el) => {
              refs.current[tab.id] = el;
            }}
            type="button"
            role="tab"
            id={`${base}-tab-${tab.id}`}
            aria-selected={tab.id === current?.id}
            aria-controls={tab.id === current?.id ? `${base}-panel` : undefined}
            tabIndex={tab.id === current?.id ? 0 : -1}
            className={styles.tab}
            onClick={() => onChange(tab.id)}
            onKeyDown={(event) => onKeyDown(event, index)}
          >
            {tab.label}
          </button>
        ))}
      </div>
      {current ? (
        <div
          role="tabpanel"
          id={`${base}-panel`}
          aria-labelledby={`${base}-tab-${current.id}`}
          className={styles.panel}
        >
          {current.panel}
        </div>
      ) : null}
    </div>
  );
}
