import { type KeyboardEvent, useEffect, useId, useRef, useState } from "react";
import { Dialog } from "../../components/Dialog/Dialog";
import { t } from "../../copy/t";
import { marked, matching, SEARCH_ABOVE } from "./modelList";
import styles from "./Online.module.css";

/** How long typing pauses before the match count is read out. */
const SETTLE_MS = 600;

/**
 * "{host}'s list" over Connect: the service's names, A to Z, exactly as listed. A click or Enter
 * chooses and closes; the arrow keys move without closing; closing changes nothing.
 */
export function ModelDialog({
  host,
  names,
  chosen,
  onChoose,
  onClose,
}: {
  host: string;
  /** In order already. */
  names: string[];
  chosen: string;
  onChoose(name: string): void;
  onClose(): void;
}) {
  const id = useId();
  const searching = names.length > SEARCH_ABOVE;
  const [query, setQuery] = useState("");
  const [settled, setSettled] = useState("");
  const shown = matching(names, query);
  const typed = query.trim();
  const start = Math.max(0, shown.indexOf(chosen));
  const [at, setAt] = useState(start);
  const search = useRef<HTMLInputElement>(null);
  const rows = useRef<(HTMLButtonElement | null)[]>([]);
  // Focus opens on the search when there is one, otherwise on the name in the field or the first.
  const initial = useRef<HTMLElement | null>(null);

  useEffect(() => {
    const timer = setTimeout(() => setSettled(query), SETTLE_MS);
    return () => clearTimeout(timer);
  }, [query]);

  const move = (event: KeyboardEvent, i: number) => {
    const to =
      event.key === "ArrowDown"
        ? Math.min(shown.length - 1, i + 1)
        : event.key === "ArrowUp"
          ? Math.max(0, i - 1)
          : null;
    if (to === null) return;
    event.preventDefault();
    setAt(to);
    rows.current[to]?.focus();
  };

  return (
    <Dialog
      title={t("advanced.online.connect.list.title", { host })}
      onClose={onClose}
      initialFocus={initial}
    >
      <p className={styles.sub}>
        {t("advanced.online.connect.list.count", { count: names.length })}
      </p>
      {searching ? (
        <label className={styles.listSearch}>
          <svg
            width="18"
            height="18"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.8"
            strokeLinecap="round"
            strokeLinejoin="round"
            aria-hidden="true"
          >
            <circle cx="11" cy="11" r="7" />
            <path d="m20 20-4-4" />
          </svg>
          <span className="sr-only">{t("advanced.online.connect.list.search")}</span>
          <input
            ref={(el) => {
              search.current = el;
              initial.current = el;
            }}
            type="search"
            placeholder={t("advanced.online.connect.list.search")}
            autoComplete="off"
            spellCheck={false}
            value={query}
            onChange={(event) => {
              setQuery(event.target.value);
              setAt(0);
            }}
            onKeyDown={(event) => {
              if (event.key === "ArrowDown" && shown.length > 0) {
                event.preventDefault();
                rows.current[0]?.focus();
              }
            }}
          />
        </label>
      ) : null}
      <p className={styles.sub} aria-live="polite">
        {typed !== "" && shown.length > 0 && settled === query
          ? t("advanced.online.connect.list.matches", {
              shown: shown.length,
              count: names.length,
              query: typed,
            })
          : null}
      </p>
      {shown.length === 0 ? (
        <div className={styles.listNone} role="status">
          <p>{t("advanced.online.connect.list.noMatch", { query: typed })}</p>
          <button type="button" className={styles.listUse} onClick={() => onChoose(typed)}>
            {t("advanced.online.connect.list.useTyped", { query: typed })}
          </button>
        </div>
      ) : (
        <div
          className={styles.listRows}
          role="radiogroup"
          aria-label={t("advanced.online.connect.list.title", { host })}
          id={`${id}-rows`}
        >
          {shown.map((name, i) => (
            // biome-ignore lint/a11y/useSemanticElements: a native radio chooses as the arrows move; here they only move
            <button
              key={name}
              ref={(el) => {
                rows.current[i] = el;
                if (!searching && i === start) initial.current = el;
              }}
              type="button"
              role="radio"
              aria-label={name}
              aria-checked={name === chosen}
              tabIndex={i === Math.min(at, shown.length - 1) ? 0 : -1}
              className={styles.listRow}
              onClick={() => onChoose(name)}
              onKeyDown={(event) => move(event, i)}
            >
              {marked(name, query).map((part, n) =>
                part.match ? <mark key={n}>{part.text}</mark> : <span key={n}>{part.text}</span>,
              )}
            </button>
          ))}
        </div>
      )}
      {shown.length > 0 ? (
        <p className={styles.sub}>{t("advanced.online.connect.list.note")}</p>
      ) : null}
    </Dialog>
  );
}
