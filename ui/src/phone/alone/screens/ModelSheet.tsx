import { useEffect, useId, useRef, useState } from "react";
import { t } from "../../../copy/t";
import { marked, matching, SEARCH_ABOVE } from "../../../screens/online/modelList";
import { BottomSheet } from "../../screens/Dialog";
import { Icon } from "../../screens/ui";

/** How far each arrow key moves along the names. */
const ARROWS: Record<string, number> = { ArrowDown: 1, ArrowRight: 1, ArrowUp: -1, ArrowLeft: -1 };

/** How long typing pauses before the match count is read out. */
const SETTLE_MS = 600;

/**
 * "{host}'s list": the service's names, A to Z, exactly as listed, to choose one from. One tap
 * chooses and closes; closing changes nothing. A long list opens with its search.
 */
export function ModelSheet({
  host,
  names,
  chosen,
  onChoose,
  onClose,
}: {
  host: string;
  /** In order already. */
  names: string[];
  /** The name in the field, marked when it is listed. */
  chosen: string;
  onChoose(name: string): void;
  onClose(): void;
}) {
  const id = useId();
  const searching = names.length > SEARCH_ABOVE;
  const [query, setQuery] = useState("");
  const [settled, setSettled] = useState("");
  const search = useRef<HTMLInputElement>(null);
  const first = useRef<HTMLInputElement>(null);
  const on = useRef<HTMLInputElement>(null);
  const shown = matching(names, query);
  const typed = query.trim();

  useEffect(() => {
    (searching ? search.current : (on.current ?? first.current))?.focus();
  }, [searching]);
  useEffect(() => {
    const timer = setTimeout(() => setSettled(query), SETTLE_MS);
    return () => clearTimeout(timer);
  }, [query]);

  const title = t("phone.mind.list.title", { host });
  return (
    <>
      <div className="ph-scrim" aria-hidden="true" onClick={onClose} />
      <BottomSheet
        label={title}
        onClose={onClose}
        className={`ph-names-sheet${searching ? " ph-names-sheet--tall" : ""}`}
      >
        <div className="ph-names-head">
          <h2 id={`${id}-title`} className="ph-names-title">
            {title}
            <span className="ph-names-count">
              {t("phone.mind.list.count", { count: names.length })}
            </span>
          </h2>
          <button
            type="button"
            className="ph-ib ph-names-close"
            aria-label={t("phone.mind.list.close")}
            onClick={onClose}
          >
            <Icon name="close" size={22} />
          </button>
        </div>
        {searching ? (
          <label className="ph-names-search">
            <Icon name="search" size={20} />
            <span className="sr-only">{t("phone.mind.list.search")}</span>
            <input
              ref={search}
              type="search"
              placeholder={t("phone.mind.list.search")}
              autoComplete="off"
              autoCapitalize="none"
              autoCorrect="off"
              spellCheck={false}
              value={query}
              onChange={(e) => setQuery(e.target.value)}
            />
          </label>
        ) : null}
        <p className="ph-names-note" aria-live="polite">
          {typed !== "" && shown.length > 0 && settled === query
            ? t("phone.mind.list.matches", {
                shown: shown.length,
                count: names.length,
                query: typed,
              })
            : null}
        </p>
        {shown.length === 0 ? (
          <div className="ph-names-none" role="status">
            <p>{t("phone.mind.list.noMatch", { query: typed })}</p>
            <button type="button" className="ph-btn ph-btn--tonal" onClick={() => onChoose(typed)}>
              {t("phone.mind.list.useTyped", { query: typed })}
            </button>
          </div>
        ) : (
          <fieldset className="ph-names" aria-labelledby={`${id}-title`}>
            {shown.map((name, i) => (
              <label key={name} className="ph-names-row">
                <input
                  ref={name === chosen ? on : i === 0 ? first : undefined}
                  type="radio"
                  aria-label={name}
                  name={`${id}-name`}
                  checked={name === chosen}
                  readOnly
                  // The arrow keys only move; a tap, a screen reader's activation, Space or
                  // Enter chooses.
                  onClick={() => onChoose(name)}
                  onKeyDown={(e) => {
                    const by = ARROWS[e.key];
                    if (by !== undefined) {
                      e.preventDefault();
                      const group = e.currentTarget.closest("fieldset");
                      const rows = group
                        ? [...group.querySelectorAll<HTMLInputElement>("input[type=radio]")]
                        : [];
                      const at = rows.indexOf(e.currentTarget);
                      rows[Math.max(0, Math.min(rows.length - 1, at + by))]?.focus();
                    } else if (e.key === "Enter") {
                      e.preventDefault();
                      onChoose(name);
                    }
                  }}
                />
                <span className="ph-radio" aria-hidden="true" />
                <span className="ph-names-name">
                  {marked(name, query).map((part, n) =>
                    part.match ? (
                      <mark key={n}>{part.text}</mark>
                    ) : (
                      <span key={n}>{part.text}</span>
                    ),
                  )}
                </span>
              </label>
            ))}
          </fieldset>
        )}
        {shown.length > 0 ? <p className="ph-names-note">{t("phone.mind.list.note")}</p> : null}
      </BottomSheet>
    </>
  );
}
